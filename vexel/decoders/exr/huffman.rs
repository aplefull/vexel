use crate::utils::bits::MsbBitReader;
use crate::utils::entropy::prefix::PrefixCodeTable;
use crate::utils::error::{VexelError, VexelResult};

const HUF_ENCBITS: u32 = 16;
const HUF_DECBITS: u32 = 14;
const HUF_ENCSIZE: usize = (1 << HUF_ENCBITS) + 1;

const SHORT_ZEROCODE_RUN: u64 = 59;
const LONG_ZEROCODE_RUN: u64 = 63;
const SHORTEST_LONG_RUN: u64 = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;

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

fn unpack_enc_table(data: &[u8], im: usize, i_m: usize) -> VexelResult<(Vec<u64>, usize)> {
    let mut hcode = vec![0u64; HUF_ENCSIZE];
    let mut reader = MsbBitReader::new(data);
    let truncated = || VexelError::Custom("EXR: huffman table data truncated".to_string());

    let mut i = im;
    while i <= i_m {
        let l = u64::from(reader.try_read(6).ok_or_else(truncated)?);
        hcode[i] = l;

        if l == LONG_ZEROCODE_RUN {
            let zerun = u64::from(reader.try_read(8).ok_or_else(truncated)?) + SHORTEST_LONG_RUN;
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
    Ok((hcode, reader.byte_position()))
}

fn build_dec_table(hcode: &[u64], im: usize, i_m: usize) -> PrefixCodeTable<u32> {
    let codes = (im..=i_m).filter_map(|symbol| {
        let code = hcode[symbol];
        let len = huf_length(code);
        (len > 0).then(|| (len, huf_code(code), symbol as u32))
    });
    PrefixCodeTable::new(codes, HUF_DECBITS)
}

fn huf_decode(
    table: &PrefixCodeTable<u32>,
    data: &[u8],
    n_bits: u64,
    rlc: u32,
    expected: usize,
) -> VexelResult<Vec<u16>> {
    let corrupt = || VexelError::Custom("EXR: corrupt huffman data".to_string());
    let byte_count = n_bits.div_ceil(8) as usize;
    if byte_count > data.len() {
        return Err(VexelError::Custom("EXR: huffman data truncated".to_string()));
    }

    let mut bits = MsbBitReader::new(&data[..byte_count]);
    let mut out: Vec<u16> = Vec::with_capacity(expected);

    while bits.position() < n_bits {
        let symbol = table.decode(&mut bits).ok_or_else(corrupt)?;

        if symbol == rlc {
            let count = bits.read(8) as usize;
            if bits.position() > n_bits {
                return Err(VexelError::Custom("EXR: huffman data truncated".to_string()));
            }
            let prev = *out
                .last()
                .ok_or_else(|| VexelError::Custom("EXR: huffman run with no previous symbol".to_string()))?;
            if out.len() + count > expected {
                return Err(VexelError::Custom("EXR: huffman run overflows output".to_string()));
            }
            out.resize(out.len() + count, prev);
        } else if bits.position() > n_bits {
            return Err(corrupt());
        } else if out.len() < expected {
            out.push(symbol as u16);
        } else {
            return Err(VexelError::Custom("EXR: huffman output overflow".to_string()));
        }
    }

    if out.len() != expected {
        return Err(VexelError::Custom(format!(
            "EXR: huffman decode produced {} of {} expected values",
            out.len(),
            expected
        )));
    }

    Ok(out)
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

    let table = build_dec_table(&hcode, im, i_m);
    huf_decode(&table, bit_data, n_bits, i_m as u32, n_raw)
}
