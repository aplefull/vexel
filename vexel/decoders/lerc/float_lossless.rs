use super::huffman::{Huffman, HuffmanBitStream};
use super::reader::ByteReader;
use crate::log_warn;
use crate::utils::error::VexelResult;

const MAX_DELTA: u8 = 5;

const HUFFMAN_NORMAL: u8 = 0;
const HUFFMAN_RLE: u8 = 1;
const HUFFMAN_NO_ENCODING: u8 = 2;
const HUFFMAN_PACKBITS: u8 = 3;

const FLT_MANT_MASK: u32 = 0x007F_FFFF;
const FLT_9BIT_MASK: u32 = 0xFF80_0000;
const DBL_MANT_MASK: u64 = 0x000F_FFFF_FFFF_FFFF;
const DBL_12BIT_MASK: u64 = 0xFFF0_0000_0000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PredictorType {
    None,
    Delta1,
    RowsCols,
}

impl PredictorType {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::None),
            1 => Some(Self::Delta1),
            2 => Some(Self::RowsCols),
            _ => None,
        }
    }

    fn int_delta(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Delta1 => 1,
            Self::RowsCols => 2,
        }
    }
}

fn add32_bit_flt(a: u32, b: u32) -> u32 {
    let mantissa = (a.wrapping_add(b)) & FLT_MANT_MASK;
    let ae = (a & FLT_9BIT_MASK) >> 23;
    let be = (b & FLT_9BIT_MASK) >> 23;
    mantissa | ((ae.wrapping_add(be) & 0x1FF) << 23)
}

fn add64_bit_dbl(a: u64, b: u64) -> u64 {
    let am = a & DBL_MANT_MASK;
    let bm = b & DBL_MANT_MASK;
    let mantissa = (am.wrapping_add(bm)) & DBL_MANT_MASK;
    let ae = (a & DBL_12BIT_MASK) >> 52;
    let be = (b & DBL_12BIT_MASK) >> 52;
    mantissa | ((ae.wrapping_add(be) & 0xFFF) << 52)
}

fn undo_move_bits_to_front(a: u32) -> u32 {
    let mut ret = a & FLT_MANT_MASK;
    let ae = ((a & FLT_9BIT_MASK) >> 24) & 0xFF;
    let sign = (a >> 23) & 0x01;
    ret |= ae << 23;
    ret |= sign << 31;
    ret
}

fn decode_packbits(src: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0usize;

    while i < src.len() && out.len() < expected {
        let b = src[i] as usize;
        i += 1;

        if b <= 127 {
            let count = b + 1;
            let available = src.len().saturating_sub(i).min(count);
            let writable = expected.saturating_sub(out.len()).min(available);
            out.extend_from_slice(&src[i..i + writable]);
            i += available;

            if available < count {
                break;
            }
        } else {
            if i >= src.len() {
                break;
            }

            let count = b - 127 + 1;
            let value = src[i];
            i += 1;

            let writable = expected.saturating_sub(out.len()).min(count);
            out.extend(std::iter::repeat_n(value, writable));
        }
    }

    if out.len() < expected {
        log_warn!("LERC packbits plane truncated, padding {} bytes", expected - out.len());
        out.resize(expected, 0);
    }

    out
}

fn decode_esri_huffman(src: &[u8], expected: usize) -> Vec<u8> {
    if src.is_empty() {
        log_warn!("Empty LERC float byte plane, filling with zeros");
        return vec![0; expected];
    }

    match src[0] {
        HUFFMAN_RLE => {
            let value = src.get(1).copied().unwrap_or(0);
            vec![value; expected]
        }
        HUFFMAN_NO_ENCODING => {
            let mut out = vec![0u8; expected];
            let available = src.len().saturating_sub(1).min(expected);
            out[..available].copy_from_slice(&src[1..1 + available]);

            if available < expected {
                log_warn!("LERC uncompressed byte plane truncated, padding {} bytes", expected - available);
            }

            out
        }
        HUFFMAN_PACKBITS => decode_packbits(&src[1..], expected),
        flag => {
            if flag != HUFFMAN_NORMAL {
                log_warn!("Unknown LERC float plane encoding {}, trying Huffman", flag);
            }

            let mut reader = ByteReader::at(src, 1);

            let huffman = match Huffman::read_code_table(&mut reader, 5) {
                Ok(huffman) => huffman,
                Err(e) => {
                    log_warn!("LERC float byte plane code table failed: {}, filling with zeros", e);
                    return vec![0; expected];
                }
            };

            let mut stream = HuffmanBitStream::new(reader.tail());
            let mut out = Vec::with_capacity(expected);

            for _ in 0..expected {
                match stream.decode_one(&huffman) {
                    Ok(value) => out.push(value as u8),
                    Err(e) => {
                        log_warn!("LERC float byte plane decode stopped early: {}", e);
                        break;
                    }
                }
            }

            if out.len() < expected {
                out.resize(expected, 0);
            }

            out
        }
    }
}

fn restore_sequence(data: &mut [u8], level: u8) {
    for l in (1..=level as usize).rev() {
        if l >= data.len() {
            continue;
        }

        for i in l..data.len() {
            data[i] = data[i].wrapping_add(data[i - 1]);
        }
    }
}

fn restore_block_sequence_f32(data: &mut [u32], n_cols: usize, n_rows: usize, delta: u32) {
    if delta == 0 {
        return;
    }

    if delta == 2 {
        for row in 0..n_rows {
            let base = row * n_cols;
            for i in 2..n_cols {
                data[base + i] = add32_bit_flt(data[base + i], data[base + i - 1]);
            }
        }
    }

    for row in 0..n_rows {
        let base = row * n_cols;
        for i in 1..n_cols {
            data[base + i] = add32_bit_flt(data[base + i], data[base + i - 1]);
        }
    }
}

fn restore_block_sequence_f64(data: &mut [u64], n_cols: usize, n_rows: usize, delta: u32) {
    if delta == 0 {
        return;
    }

    if delta == 2 {
        for row in 0..n_rows {
            let base = row * n_cols;
            for i in 2..n_cols {
                data[base + i] = add64_bit_dbl(data[base + i], data[base + i - 1]);
            }
        }
    }

    for row in 0..n_rows {
        let base = row * n_cols;
        for i in 1..n_cols {
            data[base + i] = add64_bit_dbl(data[base + i], data[base + i - 1]);
        }
    }
}

fn restore_cross_bytes_f32(data: &mut [u32], n_cols: usize, n_rows: usize, delta: u32) {
    if delta == 0 {
        return;
    }

    if delta == 2 {
        for col in 0..n_cols {
            let mut shift = n_cols;
            for _ in 1..n_rows {
                data[col + shift] = add32_bit_flt(data[col + shift], data[col + shift - n_cols]);
                shift += n_cols;
            }
        }
    }

    for row in 0..n_rows {
        let base = row * n_cols;
        for i in 1..n_cols {
            data[base + i] = add32_bit_flt(data[base + i], data[base + i - 1]);
        }
    }
}

fn restore_cross_bytes_f64(data: &mut [u64], n_cols: usize, n_rows: usize, delta: u32) {
    if delta == 0 {
        return;
    }

    if delta == 2 {
        for col in 0..n_cols {
            let mut shift = n_cols;
            for _ in 1..n_rows {
                data[col + shift] = add64_bit_dbl(data[col + shift], data[col + shift - n_cols]);
                shift += n_cols;
            }
        }
    }

    for row in 0..n_rows {
        let base = row * n_cols;
        for i in 1..n_cols {
            data[base + i] = add64_bit_dbl(data[base + i], data[base + i - 1]);
        }
    }
}

fn decode_slice(reader: &mut ByteReader, is_double: bool, n_cols: usize, n_rows: usize) -> VexelResult<Vec<f64>> {
    let unit_size = if is_double { 8usize } else { 4usize };
    let expected_size = n_cols * n_rows;

    let pred_code = reader.read_u8().unwrap_or(0);

    let predictor = PredictorType::from_code(pred_code).unwrap_or_else(|| {
        log_warn!("Unknown LERC float predictor code {}, decoding without a predictor", pred_code);
        PredictorType::None
    });

    let mut planes: Vec<Option<Vec<u8>>> = vec![None; unit_size];

    for plane_number in 0..unit_size {
        let Ok(byte_index) = reader.read_u8().map(usize::from) else {
            log_warn!("LERC float slice ended after {} of {} byte planes", plane_number, unit_size);
            break;
        };

        let best_level = reader.read_u8().unwrap_or(0);
        let compressed_size = reader.read_u32().unwrap_or(0) as usize;

        let byte_index = if byte_index >= unit_size {
            let fallback = planes.iter().position(|p| p.is_none()).unwrap_or(0);
            log_warn!("LERC float byte plane index {} out of range, using plane {}", byte_index, fallback);
            fallback
        } else {
            byte_index
        };

        let level = if best_level > MAX_DELTA {
            log_warn!("LERC float byte plane delta level {} exceeds max, clamping", best_level);
            MAX_DELTA
        } else {
            best_level
        };

        let available = reader.remaining().min(compressed_size);
        if available < compressed_size {
            log_warn!("LERC float byte plane claims {} bytes, only {} available", compressed_size, available);
        }

        let compressed = reader.peek(available);
        let mut plane = decode_esri_huffman(compressed, expected_size);
        reader.skip(available);
        restore_sequence(&mut plane, level);

        planes[byte_index] = Some(plane);
    }

    let mut raw = vec![0u8; expected_size * unit_size];

    for (byte_index, plane) in planes.iter().enumerate() {
        let Some(plane) = plane else {
            log_warn!("LERC float byte plane {} missing, filling with zeros", byte_index);
            continue;
        };

        for i in 0..expected_size {
            raw[i * unit_size + byte_index] = plane[i];
        }
    }

    let delta = predictor.int_delta();
    let use_cross = predictor == PredictorType::RowsCols;

    let values = if is_double {
        let mut words: Vec<u64> =
            raw.chunks_exact(8).map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])).collect();

        if use_cross {
            restore_cross_bytes_f64(&mut words, n_cols, n_rows, delta);
        } else {
            restore_block_sequence_f64(&mut words, n_cols, n_rows, delta);
        }

        words.into_iter().map(f64::from_bits).collect()
    } else {
        let mut words: Vec<u32> = raw.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();

        if use_cross {
            restore_cross_bytes_f32(&mut words, n_cols, n_rows, delta);
        } else {
            restore_block_sequence_f32(&mut words, n_cols, n_rows, delta);
        }

        words.into_iter().map(|w| f32::from_bits(undo_move_bits_to_front(w)) as f64).collect()
    };

    Ok(values)
}

pub fn decode_huffman_float(
    reader: &mut ByteReader,
    is_double: bool,
    n_cols: usize,
    n_rows: usize,
    n_depth: usize,
) -> VexelResult<Vec<f64>> {
    if n_depth == 1 {
        decode_slice(reader, is_double, n_cols, n_rows)
    } else {
        decode_slice(reader, is_double, n_depth, n_cols * n_rows)
    }
}
