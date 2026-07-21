use crate::utils::error::{VexelError, VexelResult};

const HUF_ENCBITS: u32 = 16;
const HUF_DECBITS: u32 = 14;
const HUF_ENCSIZE: usize = (1 << HUF_ENCBITS) + 1;
const HUF_DECSIZE: usize = 1 << HUF_DECBITS;
const HUF_DECMASK: u64 = (HUF_DECSIZE - 1) as u64;

const SHORT_ZEROCODE_RUN: u64 = 59;
const LONG_ZEROCODE_RUN: u64 = 63;
const SHORTEST_LONG_RUN: u64 = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;

#[derive(Clone, Default)]
struct HufDec {
    len: u32,
    lit: u32,
    p: Vec<u32>,
}

#[inline(always)]
fn huf_length(code: u64) -> u32 {
    (code & 63) as u32
}

#[inline(always)]
fn huf_code(code: u64) -> u64 {
    code >> 6
}

fn canonical_code_table(hcode: &mut [u64]) {
    let mut n = [0u64; 59];

    for &code in hcode.iter() {
        if code <= 58 {
            n[code as usize] += 1;
        }
    }

    let mut c = 0u64;
    for i in (1..=58usize).rev() {
        let nc = (c + n[i]) >> 1;
        n[i] = c;
        c = nc;
    }

    for code in hcode.iter_mut() {
        let l = *code;
        if l > 0 && l <= 58 {
            *code = l | (n[l as usize] << 6);
            n[l as usize] += 1;
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    c: u64,
    lc: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, c: 0, lc: 0 }
    }

    fn get_bits(&mut self, n_bits: u32) -> VexelResult<u64> {
        while self.lc < n_bits {
            if self.pos >= self.data.len() {
                return Err(VexelError::Custom("EXR: huffman table data truncated".to_string()));
            }
            self.c = (self.c << 8) | self.data[self.pos] as u64;
            self.pos += 1;
            self.lc += 8;
        }

        self.lc -= n_bits;
        Ok((self.c >> self.lc) & ((1u64 << n_bits) - 1))
    }
}

fn unpack_enc_table(data: &[u8], im: usize, i_m: usize) -> VexelResult<(Vec<u64>, usize)> {
    let mut hcode = vec![0u64; HUF_ENCSIZE];
    let mut reader = BitReader::new(data);

    let mut i = im;
    while i <= i_m {
        let l = reader.get_bits(6)?;
        hcode[i] = l;

        if l == LONG_ZEROCODE_RUN {
            let zerun = reader.get_bits(8)? + SHORTEST_LONG_RUN;
            if i as u64 + zerun > i_m as u64 + 1 {
                return Err(VexelError::Custom("EXR: huffman table zero run too long".to_string()));
            }
            for _ in 0..zerun {
                hcode[i] = 0;
                i += 1;
            }
            continue;
        } else if l >= SHORT_ZEROCODE_RUN {
            let zerun = l - SHORT_ZEROCODE_RUN + 2;
            if i as u64 + zerun > i_m as u64 + 1 {
                return Err(VexelError::Custom("EXR: huffman table zero run too long".to_string()));
            }
            for _ in 0..zerun {
                hcode[i] = 0;
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    canonical_code_table(&mut hcode);
    Ok((hcode, reader.pos))
}

fn build_dec_table(hcode: &[u64], im: usize, i_m: usize) -> VexelResult<Vec<HufDec>> {
    let mut hdecod = vec![HufDec::default(); HUF_DECSIZE];

    for (symbol, &code) in hcode.iter().enumerate().take(i_m + 1).skip(im) {
        let c = huf_code(code);
        let l = huf_length(code);

        if l < 64 && (c >> l) != 0 {
            return Err(VexelError::Custom("EXR: invalid huffman code".to_string()));
        }

        if l > HUF_DECBITS {
            let index = (c >> (l - HUF_DECBITS)) as usize;
            if index >= HUF_DECSIZE {
                return Err(VexelError::Custom("EXR: invalid huffman code".to_string()));
            }
            let entry = &mut hdecod[index];
            if entry.len != 0 {
                return Err(VexelError::Custom("EXR: invalid huffman table".to_string()));
            }
            entry.lit += 1;
            entry.p.push(symbol as u32);
        } else if l != 0 {
            let start = (c << (HUF_DECBITS - l)) as usize;
            let count = 1usize << (HUF_DECBITS - l);
            if start + count > HUF_DECSIZE {
                return Err(VexelError::Custom("EXR: invalid huffman code".to_string()));
            }
            for entry in hdecod[start..start + count].iter_mut() {
                if entry.len != 0 || !entry.p.is_empty() {
                    return Err(VexelError::Custom("EXR: invalid huffman table".to_string()));
                }
                entry.len = l;
                entry.lit = symbol as u32;
            }
        }
    }

    Ok(hdecod)
}

struct DecodeState<'a> {
    input: &'a [u8],
    pos: usize,
    c: u64,
    lc: i32,
    out: Vec<u16>,
    expected: usize,
}

impl<'a> DecodeState<'a> {
    #[inline(always)]
    fn get_char(&mut self) {
        self.c = (self.c << 8) | self.input[self.pos] as u64;
        self.pos += 1;
        self.lc += 8;
    }

    fn emit(&mut self, symbol: u32, rlc: u32) -> VexelResult<()> {
        if symbol == rlc {
            if self.lc < 8 {
                if self.pos >= self.input.len() {
                    return Err(VexelError::Custom("EXR: huffman data truncated".to_string()));
                }
                self.get_char();
            }

            self.lc -= 8;
            let cs = (self.c >> self.lc) as u8 as usize;

            if self.out.len() + cs > self.expected {
                return Err(VexelError::Custom("EXR: huffman run overflows output".to_string()));
            }
            let prev = match self.out.last() {
                Some(&v) => v,
                None => return Err(VexelError::Custom("EXR: huffman run with no previous symbol".to_string())),
            };
            for _ in 0..cs {
                self.out.push(prev);
            }
        } else if self.out.len() < self.expected {
            self.out.push(symbol as u16);
        } else {
            return Err(VexelError::Custom("EXR: huffman output overflow".to_string()));
        }

        Ok(())
    }
}

fn huf_decode(
    hcode: &[u64],
    hdecod: &[HufDec],
    data: &[u8],
    n_bits: u64,
    rlc: u32,
    expected: usize,
) -> VexelResult<Vec<u16>> {
    let byte_count = n_bits.div_ceil(8) as usize;
    if byte_count > data.len() {
        return Err(VexelError::Custom("EXR: huffman data truncated".to_string()));
    }

    let mut state =
        DecodeState { input: &data[..byte_count], pos: 0, c: 0, lc: 0, out: Vec::with_capacity(expected), expected };

    while state.pos < state.input.len() {
        state.get_char();

        while state.lc >= HUF_DECBITS as i32 {
            let offset = ((state.c >> (state.lc - HUF_DECBITS as i32)) & HUF_DECMASK) as usize;
            let entry = &hdecod[offset];

            if entry.len != 0 {
                if entry.len as i32 > state.lc {
                    return Err(VexelError::Custom("EXR: corrupt huffman data".to_string()));
                }
                state.lc -= entry.len as i32;
                let lit = entry.lit;
                state.emit(lit, rlc)?;
            } else {
                if entry.p.is_empty() {
                    return Err(VexelError::Custom("EXR: corrupt huffman data".to_string()));
                }

                let mut found = false;
                for &candidate in entry.p.iter() {
                    let l = huf_length(hcode[candidate as usize]) as i32;

                    while state.lc < l && state.pos < state.input.len() {
                        state.get_char();
                    }

                    if state.lc >= l {
                        let mask = if l >= 64 { u64::MAX } else { (1u64 << l) - 1 };
                        if huf_code(hcode[candidate as usize]) == (state.c >> (state.lc - l)) & mask {
                            state.lc -= l;
                            state.emit(candidate, rlc)?;
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    return Err(VexelError::Custom("EXR: corrupt huffman data".to_string()));
                }
            }
        }
    }

    let trailing = (8u64.wrapping_sub(n_bits) & 7) as i32;
    state.c >>= trailing;
    state.lc -= trailing;

    while state.lc > 0 {
        let offset = ((state.c << (HUF_DECBITS as i32 - state.lc)) & HUF_DECMASK) as usize;
        let entry = &hdecod[offset];

        if entry.len != 0 {
            if entry.len as i32 > state.lc {
                return Err(VexelError::Custom("EXR: corrupt huffman data".to_string()));
            }
            state.lc -= entry.len as i32;
            let lit = entry.lit;
            state.emit(lit, rlc)?;
        } else {
            return Err(VexelError::Custom("EXR: corrupt huffman data".to_string()));
        }
    }

    if state.out.len() != expected {
        return Err(VexelError::Custom(format!(
            "EXR: huffman decode produced {} of {} expected values",
            state.out.len(),
            expected
        )));
    }

    Ok(state.out)
}

pub fn huf_decompress(compressed: &[u8], n_raw: usize) -> VexelResult<Vec<u16>> {
    if compressed.len() < 20 {
        if n_raw == 0 {
            return Ok(Vec::new());
        }
        return Err(VexelError::Custom("EXR: huffman data too short".to_string()));
    }

    let read_u32 = |offset: usize| {
        u32::from_le_bytes([compressed[offset], compressed[offset + 1], compressed[offset + 2], compressed[offset + 3]])
    };

    let im = read_u32(0) as usize;
    let i_m = read_u32(4) as usize;
    let n_bits = read_u32(12) as u64;

    if im >= HUF_ENCSIZE || i_m >= HUF_ENCSIZE || im > i_m {
        return Err(VexelError::Custom("EXR: invalid huffman table range".to_string()));
    }

    let table_data = &compressed[20..];
    let (hcode, table_bytes) = unpack_enc_table(table_data, im, i_m)?;

    let bit_data = &table_data[table_bytes..];
    if n_bits > 8 * bit_data.len() as u64 {
        return Err(VexelError::Custom("EXR: huffman bit count exceeds available data".to_string()));
    }

    let hdecod = build_dec_table(&hcode, im, i_m)?;
    huf_decode(&hcode, &hdecod, bit_data, n_bits, i_m as u32, n_raw)
}
