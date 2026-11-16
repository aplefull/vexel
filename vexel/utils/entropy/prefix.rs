use crate::utils::bits::MsbBitReader;

const MAX_EXPLICIT_LENGTH: u32 = 64;

#[derive(Clone, Copy, Default)]
struct Slot<T> {
    len: u8,
    symbol: T,
}

#[derive(Clone)]
struct LongCodes<T> {
    len: u32,
    codes: Vec<(u64, T)>,
}

#[derive(Clone)]
enum Overflow<T> {
    Explicit(Vec<LongCodes<T>>),
    Canonical { counts: Vec<u64>, symbols: Vec<T> },
}

#[derive(Clone)]
pub struct PrefixCodeTable<T> {
    lut: Box<[Slot<T>]>,
    lut_bits: u32,
    overflow: Overflow<T>,
}

impl<T: Copy + Default> PrefixCodeTable<T> {
    pub fn new(codes: impl IntoIterator<Item = (u32, u64, T)>, max_lut_bits: u32) -> Self {
        let codes: Vec<(u32, u64, T)> = codes
            .into_iter()
            .filter(|&(len, code, _)| (1..=MAX_EXPLICIT_LENGTH).contains(&len) && (len == 64 || code >> len == 0))
            .collect();

        let max_len = codes.iter().map(|&(len, _, _)| len).max().unwrap_or(0);
        let lut_bits = max_len.min(max_lut_bits);
        let mut lut = vec![Slot::default(); 1 << lut_bits].into_boxed_slice();
        let mut long: Vec<LongCodes<T>> = Vec::new();

        for &(len, code, symbol) in &codes {
            if len <= lut_bits {
                fill_slots(&mut lut, lut_bits, len, code, symbol);
            } else {
                match long.iter_mut().find(|group| group.len == len) {
                    Some(group) => group.codes.push((code, symbol)),
                    None => long.push(LongCodes { len, codes: vec![(code, symbol)] }),
                }
            }
        }

        long.sort_by_key(|group| group.len);
        for group in &mut long {
            group.codes.sort_by_key(|&(code, _)| code);
        }

        PrefixCodeTable { lut, lut_bits, overflow: Overflow::Explicit(long) }
    }

    pub fn canonical(lengths: &[u32], symbols: &[T], max_lut_bits: u32) -> Self {
        let max_len = lengths.iter().copied().max().unwrap_or(0) as usize;
        let mut counts = vec![0u64; max_len + 1];
        for &len in lengths.iter().filter(|&&len| len > 0) {
            counts[len as usize] += 1;
        }

        let lut_bits = (max_len as u32).min(max_lut_bits);
        let mut lut = vec![Slot::default(); 1 << lut_bits].into_boxed_slice();
        let mut next_code = vec![0u64; lut_bits as usize + 1];
        let mut code = 0u64;
        for len in 1..=lut_bits as usize {
            code = (code + counts[len - 1]) << 1;
            next_code[len] = code;
        }

        let mut order: Vec<usize> = (0..lengths.len()).filter(|&index| lengths[index] > 0).collect();
        order.sort_by_key(|&index| lengths[index]);

        for &index in &order {
            let len = lengths[index];
            if len <= lut_bits {
                let code = next_code[len as usize];
                next_code[len as usize] += 1;
                fill_slots(&mut lut, lut_bits, len, code, symbols[index]);
            }
        }

        let symbols = order.iter().map(|&index| symbols[index]).collect();
        PrefixCodeTable { lut, lut_bits, overflow: Overflow::Canonical { counts, symbols } }
    }

    #[inline(always)]
    pub fn decode(&self, bits: &mut MsbBitReader) -> Option<T> {
        let slot = self.lut[bits.peek(self.lut_bits) as usize];
        if slot.len != 0 {
            bits.skip(u64::from(slot.len));
            return Some(slot.symbol);
        }
        self.decode_long(bits)
    }

    #[cold]
    fn decode_long(&self, bits: &mut MsbBitReader) -> Option<T> {
        match &self.overflow {
            Overflow::Explicit(long) => {
                for group in long {
                    let value = bits.peek_u64(group.len);
                    if let Ok(index) = group.codes.binary_search_by_key(&value, |&(code, _)| code) {
                        bits.skip(u64::from(group.len));
                        return Some(group.codes[index].1);
                    }
                }
                None
            }
            Overflow::Canonical { counts, symbols } => {
                let start = bits.position();
                let mut distance = 0u64;
                let mut preceding = 0usize;
                for &count in counts.iter().skip(1) {
                    distance = (distance << 1) | u64::from(bits.read_bit());
                    if distance < count {
                        return symbols.get(preceding + distance as usize).copied();
                    }
                    preceding += count as usize;
                    distance -= count;
                    if distance > symbols.len() as u64 {
                        break;
                    }
                }
                bits.seek_bits(start);
                None
            }
        }
    }
}

fn fill_slots<T: Copy>(lut: &mut [Slot<T>], lut_bits: u32, len: u32, code: u64, symbol: T) {
    if code >> len != 0 {
        return;
    }
    let spread = lut_bits - len;
    let start = (code << spread) as usize;
    lut[start..start + (1 << spread)].fill(Slot { len: len as u8, symbol });
}
