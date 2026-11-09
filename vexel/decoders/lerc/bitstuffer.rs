use super::reader::ByteReader;
use crate::utils::error::{VexelError, VexelResult};

fn num_tail_bytes_not_needed(num_elem: u32, num_bits: u32) -> usize {
    let num_bits_tail = ((num_elem as u64 * num_bits as u64) & 31) as u32;
    let num_bytes_tail = (num_bits_tail + 7) >> 3;
    if num_bytes_tail > 0 {
        4 - num_bytes_tail as usize
    } else {
        0
    }
}

fn load_u32_buffer(src: &[u8], num_uints: usize) -> Vec<u32> {
    let mut words = vec![0u32; num_uints];

    for (i, word) in words.iter_mut().enumerate() {
        let start = i * 4;
        let mut bytes = [0u8; 4];
        let available = src.len().saturating_sub(start).min(4);
        if available > 0 {
            bytes[..available].copy_from_slice(&src[start..start + available]);
        }
        *word = u32::from_le_bytes(bytes);
    }

    words
}

fn unstuff_words(words: &[u32], num_elements: u32, num_bits: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(num_elements as usize);
    let mut bit_pos = 0u32;
    let mut idx = 0usize;
    let nb = 32 - num_bits;

    for _ in 0..num_elements {
        let current = words.get(idx).copied().unwrap_or(0);

        if nb >= bit_pos {
            out.push((current << (nb - bit_pos)) >> nb);
            bit_pos += num_bits;
            if bit_pos == 32 {
                bit_pos = 0;
                idx += 1;
            }
        } else {
            let next = words.get(idx + 1).copied().unwrap_or(0);
            idx += 1;
            let low = current >> bit_pos;
            let shift = 64 - num_bits - bit_pos;
            bit_pos -= nb;
            out.push(low | ((next << shift) >> nb));
        }
    }

    out
}

fn unstuff_words_before_v3(words: &[u32], num_elements: u32, num_bits: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(num_elements as usize);
    let mut bit_pos = 0u32;
    let mut idx = 0usize;

    for _ in 0..num_elements {
        let current = words.get(idx).copied().unwrap_or(0);

        if 32 - bit_pos >= num_bits {
            out.push((current << bit_pos) >> (32 - num_bits));
            bit_pos += num_bits;
            if bit_pos == 32 {
                bit_pos = 0;
                idx += 1;
            }
        } else {
            idx += 1;
            let next = words.get(idx).copied().unwrap_or(0);
            let high = (current << bit_pos) >> (32 - num_bits);
            bit_pos -= 32 - num_bits;
            out.push(high | (next >> (32 - bit_pos)));
        }
    }

    out
}

fn bit_unstuff(reader: &mut ByteReader, num_elements: u32, num_bits: u32) -> VexelResult<Vec<u32>> {
    if num_elements == 0 || num_bits == 0 || num_bits >= 32 {
        return Err(VexelError::Custom(format!(
            "Invalid LERC bit stuffing: {} elements, {} bits",
            num_elements, num_bits
        )));
    }

    let num_uints = (num_elements as u64 * num_bits as u64).div_ceil(32) as usize;
    let num_bytes = num_uints * 4;
    let num_bytes_used = num_bytes - num_tail_bytes_not_needed(num_elements, num_bits);

    let available = reader.remaining().min(num_bytes_used);
    let src = reader.peek(available);
    let words = load_u32_buffer(src, num_uints);
    reader.skip(available);

    Ok(unstuff_words(&words, num_elements, num_bits))
}

fn bit_unstuff_before_v3(reader: &mut ByteReader, num_elements: u32, num_bits: u32) -> VexelResult<Vec<u32>> {
    if num_elements == 0 || num_bits == 0 || num_bits >= 32 {
        return Err(VexelError::Custom(format!(
            "Invalid LERC bit stuffing: {} elements, {} bits",
            num_elements, num_bits
        )));
    }

    let num_uints = (num_elements as u64 * num_bits as u64).div_ceil(32) as usize;
    let num_bytes_to_copy = (num_elements as u64 * num_bits as u64).div_ceil(8) as usize;

    let available = reader.remaining().min(num_bytes_to_copy);
    let src = reader.peek(available);
    let mut words = load_u32_buffer(src, num_uints);
    reader.skip(available);

    let tail = num_tail_bytes_not_needed(num_elements, num_bits);
    if let Some(last) = words.last_mut() {
        for _ in 0..tail {
            *last <<= 8;
        }
    }

    Ok(unstuff_words_before_v3(&words, num_elements, num_bits))
}

pub fn decode_bit_stuffed(
    reader: &mut ByteReader,
    max_element_count: usize,
    lerc2_version: i32,
) -> VexelResult<Vec<u32>> {
    let num_bits_byte = reader.read_u8()?;

    let bits67 = num_bits_byte >> 6;
    let nb = if bits67 == 0 { 4 } else { 3 - bits67 as usize };
    let do_lut = (num_bits_byte & (1 << 5)) != 0;
    let num_bits = (num_bits_byte & 31) as u32;

    let num_elements = reader.read_uint_sized(nb)?;

    if num_elements as usize > max_element_count {
        return Err(VexelError::Custom(format!(
            "LERC bit stuffed block claims {} elements, expected at most {}",
            num_elements, max_element_count
        )));
    }

    if !do_lut {
        if num_bits == 0 {
            return Ok(vec![0; num_elements as usize]);
        }

        return if lerc2_version >= 3 {
            bit_unstuff(reader, num_elements, num_bits)
        } else {
            bit_unstuff_before_v3(reader, num_elements, num_bits)
        };
    }

    if num_bits == 0 {
        return Err(VexelError::Custom("LERC LUT block with zero bits".to_string()));
    }

    let n_lut_byte = reader.read_u8()?;
    let n_lut = n_lut_byte.saturating_sub(1) as u32;

    let mut lut = if lerc2_version >= 3 {
        bit_unstuff(reader, n_lut, num_bits)?
    } else {
        bit_unstuff_before_v3(reader, n_lut, num_bits)?
    };

    let mut n_bits_lut = 0u32;
    while (n_lut >> n_bits_lut) != 0 {
        n_bits_lut += 1;
    }

    if n_bits_lut == 0 {
        return Err(VexelError::Custom("LERC LUT block with empty lookup table".to_string()));
    }

    let mut indexes = if lerc2_version >= 3 {
        bit_unstuff(reader, num_elements, n_bits_lut)?
    } else {
        bit_unstuff_before_v3(reader, num_elements, n_bits_lut)?
    };

    lut.insert(0, 0);

    for value in indexes.iter_mut() {
        *value = lut.get(*value as usize).copied().unwrap_or(0);
    }

    Ok(indexes)
}

pub fn decode_bit_stuffed_lerc1(reader: &mut ByteReader, max_element_count: usize) -> VexelResult<Vec<u32>> {
    let num_bits_byte = reader.read_u8()?;

    let bits67 = num_bits_byte >> 6;
    let nb = if bits67 == 0 { 4 } else { 3 - bits67 as usize };
    let num_bits = (num_bits_byte & 63) as u32;

    if num_bits >= 32 {
        return Err(VexelError::Custom(format!("Invalid LERC1 bit width {}", num_bits)));
    }

    let num_elements = reader.read_uint_sized(nb)?;

    if num_elements as usize > max_element_count {
        return Err(VexelError::Custom(format!(
            "LERC1 bit stuffed block claims {} elements, expected at most {}",
            num_elements, max_element_count
        )));
    }

    if num_bits == 0 {
        return Ok(vec![0; num_elements as usize]);
    }

    let num_uints = (num_elements as u64 * num_bits as u64).div_ceil(32) as usize;
    let num_bytes_to_copy = (num_elements as u64 * num_bits as u64).div_ceil(8) as usize;

    let available = reader.remaining().min(num_bytes_to_copy);
    let src = reader.peek(available);
    reader.skip(available);

    let mut words = vec![0u32; num_uints];
    for (i, word) in words.iter_mut().enumerate() {
        let start = i * 4;
        let mut bytes = [0u8; 4];
        let got = src.len().saturating_sub(start).min(4);
        if got > 0 {
            bytes[..got].copy_from_slice(&src[start..start + got]);
        }
        *word = u32::from_le_bytes(bytes);
    }

    let tail = num_tail_bytes_not_needed(num_elements, num_bits);
    if let Some(last) = words.last_mut() {
        for _ in 0..tail {
            *last <<= 8;
        }
    }

    Ok(unstuff_words_before_v3(&words, num_elements, num_bits))
}
