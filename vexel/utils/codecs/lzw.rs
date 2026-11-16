use crate::log_warn;

const MAX_CODES: usize = 4096;
const LITERALS: usize = 256;
const SLACK: usize = MAX_CODES + 32;
const TAIL: usize = 16;
const MAX_CAPACITY: usize = u32::MAX as usize - SLACK - LITERALS - TAIL;

pub const INPUT_PADDING: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    LsbFirst,
    MsbFirst,
}

#[derive(Clone, Copy)]
pub struct LzwOptions {
    pub literal_bits: u8,
    pub bit_order: BitOrder,
    pub early_change: bool,
}

impl LzwOptions {
    pub fn gif(literal_bits: u8) -> LzwOptions {
        LzwOptions { literal_bits, bit_order: BitOrder::LsbFirst, early_change: false }
    }

    pub fn tiff() -> LzwOptions {
        LzwOptions { literal_bits: 8, bit_order: BitOrder::MsbFirst, early_change: true }
    }

    pub fn tiff_old_style() -> LzwOptions {
        LzwOptions { literal_bits: 8, bit_order: BitOrder::LsbFirst, early_change: false }
    }
}

fn clamp_capacity(capacity: usize) -> usize {
    capacity.min(MAX_CAPACITY)
}

pub fn decompress(data: &[u8], options: LzwOptions, max_output: usize) -> Vec<u8> {
    let mut padded = Vec::with_capacity(data.len() + INPUT_PADDING);
    padded.extend_from_slice(data);
    let mut out = Vec::new();
    let written = decode_padded(options, padded, &mut out, max_output);
    out.truncate(written);
    out.shrink_to_fit();
    out
}

pub fn decode_padded(options: LzwOptions, mut padded: Vec<u8>, out: &mut Vec<u8>, capacity: usize) -> usize {
    let data_len = padded.len();
    padded.resize(data_len + INPUT_PADDING, 0);

    let capacity = clamp_capacity(capacity);
    let literals = capacity + SLACK;
    let needed = literals + LITERALS + TAIL;
    if out.is_empty() {
        *out = vec![0u8; needed];
    } else if out.len() < needed {
        out.resize(needed, 0);
    }
    for (value, byte) in out[literals..literals + LITERALS].iter_mut().enumerate() {
        *byte = value as u8;
    }

    match options.bit_order {
        BitOrder::LsbFirst => decode::<false>(options, &padded, data_len, out, capacity),
        BitOrder::MsbFirst => decode::<true>(options, &padded, data_len, out, capacity),
    }
}

#[inline(always)]
fn read_code<const MSB: bool>(src: *const u8, bitpos: u64, width: u32, mask: usize) -> usize {
    let bytes = unsafe { (src.add((bitpos >> 3) as usize) as *const [u8; 8]).read_unaligned() };
    if MSB {
        ((u64::from_be_bytes(bytes) << (bitpos & 7)) >> (64 - width)) as usize
    } else {
        (u64::from_le_bytes(bytes) >> (bitpos & 7)) as usize & mask
    }
}

fn decode<const MSB: bool>(options: LzwOptions, padded: &[u8], data_len: usize, out: &mut [u8], cap: usize) -> usize {
    let literal_bits = u32::from(options.literal_bits.clamp(2, 8));
    let clear = 1usize << literal_bits;
    let end = clear + 1;
    let min_width = literal_bits + 1;
    let early = usize::from(options.early_change);
    let literals = cap + SLACK;
    let total_bits = data_len as u64 * 8;
    assert!(padded.len() >= data_len + INPUT_PADDING);
    assert!(out.len() >= literals + LITERALS + TAIL);

    let mut table = [0u64; MAX_CODES + 1];
    for (value, entry) in table.iter_mut().enumerate().take(clear) {
        *entry = (literals + value) as u64 | 1 << 32;
    }

    let bump_for = |width: u32| if width < 12 { (1usize << width) - early } else { usize::MAX };
    let safe_bits = total_bits.saturating_sub(12);
    let mut o = 0usize;
    let mut bitpos = 0u64;
    let mut width = min_width;
    let mut mask = (1usize << width) - 1;
    let mut bump_at = bump_for(width);
    let mut next = clear + 2;
    let mut has_prev = false;
    let src = padded.as_ptr();

    'outer: loop {
        if has_prev {
            let p = out.as_mut_ptr();
            loop {
                if bitpos > safe_bits {
                    break;
                }
                let value = read_code::<MSB>(src, bitpos, width, mask);
                if value.wrapping_sub(clear) < 2 || value > next {
                    break;
                }

                bitpos += u64::from(width);
                let entry = unsafe { *table.get_unchecked(value) };
                let s = entry as u32 as usize;
                let l = (entry >> 32) as usize;

                unsafe {
                    *p.add(o) = *p.add(s);
                    let v = (p.add(s) as *const u128).read_unaligned();
                    (p.add(o) as *mut u128).write_unaligned(v);
                    if l > 16 {
                        let mut k = 16;
                        while k < l {
                            let v = (p.add(s + k) as *const u128).read_unaligned();
                            (p.add(o + k) as *mut u128).write_unaligned(v);
                            k += 16;
                        }
                    }
                }

                if next < MAX_CODES {
                    next += 1;
                    if next == bump_at {
                        width += 1;
                        mask = (1 << width) - 1;
                        bump_at = bump_for(width);
                    }
                }
                unsafe { *table.get_unchecked_mut(next) = o as u64 | ((l as u64 + 1) << 32) };
                o += l;
                if o >= cap {
                    break 'outer;
                }
            }
        }

        if bitpos + u64::from(width) > total_bits {
            break;
        }
        let value = read_code::<MSB>(src, bitpos, width, mask);
        bitpos += u64::from(width);

        if value == clear {
            width = min_width;
            mask = (1 << width) - 1;
            bump_at = bump_for(width);
            next = clear + 2;
            has_prev = false;
            continue;
        }
        if value == end {
            break;
        }
        if value > next || (value == next && !has_prev) {
            log_warn!("Invalid LZW code: {}", value);
            has_prev = false;
            continue;
        }

        let entry = table[value];
        let s = entry as u32 as usize;
        let l = (entry >> 32) as usize;
        for k in 0..l {
            out[o + k] = out[s + k];
        }
        if has_prev && next < MAX_CODES {
            next += 1;
            if next == bump_at {
                width += 1;
                mask = (1 << width) - 1;
                bump_at = bump_for(width);
            }
        }
        table[next] = o as u64 | ((l as u64 + 1) << 32);
        o += l;
        has_prev = true;
        if o >= cap {
            break;
        }
    }

    o.min(cap)
}
