use crate::decoders::exr::types::{ChunkChannel, ExrPixelType};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::tiff::color::half_to_f32;

pub fn unpredict_and_reorder(mut data: Vec<u8>) -> Vec<u8> {
    for i in 1..data.len() {
        let d = data[i - 1] as i32 + data[i] as i32 - 128;
        data[i] = d as u8;
    }

    let half = data.len().div_ceil(2);
    let (first, second) = data.split_at(half);
    let mut out = Vec::with_capacity(data.len());

    let mut it1 = first.iter();
    let mut it2 = second.iter();
    while out.len() < data.len() {
        if let Some(&b) = it1.next() {
            out.push(b);
        }
        if out.len() >= data.len() {
            break;
        }
        if let Some(&b) = it2.next() {
            out.push(b);
        }
    }

    out
}

pub fn rle_decompress(src: &[u8], expected: usize) -> VexelResult<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    let mut pos = 0usize;

    while pos < src.len() && out.len() < expected {
        let control = src[pos] as i8;
        pos += 1;

        if control < 0 {
            let count = (-(control as i32)) as usize;
            let count = count.min(src.len() - pos).min(expected - out.len());
            out.extend_from_slice(&src[pos..pos + count]);
            pos += count;
        } else {
            if pos >= src.len() {
                break;
            }
            let count = control as usize + 1;
            let count = count.min(expected - out.len());
            let value = src[pos];
            pos += 1;
            out.resize(out.len() + count, value);
        }
    }

    if out.len() < expected {
        log_warn!("EXR: RLE data produced {} of {} expected bytes, padding with zeros", out.len(), expected);
        out.resize(expected, 0);
    }

    Ok(out)
}

pub fn pxr24_reconstruct(
    inflated: &[u8],
    channels: &[ChunkChannel],
    row_presence: &[Vec<bool>],
    expected: usize,
) -> VexelResult<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    let mut pos = 0usize;

    for row in row_presence {
        for (c, channel) in channels.iter().enumerate() {
            if !row[c] {
                continue;
            }

            let w = channel.width;
            match channel.pixel_type {
                ExrPixelType::Uint => {
                    if pos + w * 4 > inflated.len() {
                        return Err(VexelError::Custom("EXR: PXR24 data truncated".to_string()));
                    }
                    let (p0, rest) = inflated[pos..].split_at(w);
                    let (p1, rest) = rest.split_at(w);
                    let (p2, rest) = rest.split_at(w);
                    let p3 = &rest[..w];
                    pos += w * 4;

                    let mut pixel = 0u32;
                    for x in 0..w {
                        let diff =
                            ((p0[x] as u32) << 24) | ((p1[x] as u32) << 16) | ((p2[x] as u32) << 8) | p3[x] as u32;
                        pixel = pixel.wrapping_add(diff);
                        out.extend_from_slice(&pixel.to_le_bytes());
                    }
                }
                ExrPixelType::Half => {
                    if pos + w * 2 > inflated.len() {
                        return Err(VexelError::Custom("EXR: PXR24 data truncated".to_string()));
                    }
                    let (p0, rest) = inflated[pos..].split_at(w);
                    let p1 = &rest[..w];
                    pos += w * 2;

                    let mut pixel = 0u32;
                    for x in 0..w {
                        let diff = ((p0[x] as u32) << 8) | p1[x] as u32;
                        pixel = pixel.wrapping_add(diff);
                        out.extend_from_slice(&(pixel as u16).to_le_bytes());
                    }
                }
                ExrPixelType::Float => {
                    if pos + w * 3 > inflated.len() {
                        return Err(VexelError::Custom("EXR: PXR24 data truncated".to_string()));
                    }
                    let (p0, rest) = inflated[pos..].split_at(w);
                    let (p1, rest) = rest.split_at(w);
                    let p2 = &rest[..w];
                    pos += w * 3;

                    let mut pixel = 0u32;
                    for x in 0..w {
                        let diff = ((p0[x] as u32) << 24) | ((p1[x] as u32) << 16) | ((p2[x] as u32) << 8);
                        pixel = pixel.wrapping_add(diff);
                        out.extend_from_slice(&pixel.to_le_bytes());
                    }
                }
            }
        }
    }

    Ok(out)
}

fn b44_to_linear(bits: u16) -> u16 {
    if bits & 0x7C00 == 0x7C00 {
        return 0;
    }
    if bits > 0x8000 {
        return 0;
    }

    let f = half_to_f32(bits);
    f32_to_half(8.0 * f.ln())
}

pub fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x007F_FFFF;

    if exp == 0xFF {
        if mantissa != 0 {
            return sign | 0x7C00 | ((mantissa >> 13) as u16).max(1);
        }
        return sign | 0x7C00;
    }

    let unbiased = exp - 127;
    if unbiased > 15 {
        return sign | 0x7C00;
    }

    if unbiased >= -14 {
        let half_exp = (unbiased + 15) as u32;
        let mut half_mant = mantissa >> 13;
        let round_bits = mantissa & 0x1FFF;
        if round_bits > 0x1000 || (round_bits == 0x1000 && half_mant & 1 == 1) {
            half_mant += 1;
        }
        let combined = (half_exp << 10) + half_mant;
        if combined >= 0x7C00 {
            return sign | 0x7C00;
        }
        return sign | combined as u16;
    }

    if unbiased < -25 {
        return sign;
    }

    let full_mant = mantissa | 0x0080_0000;
    let shift = (-14 - unbiased + 13) as u32;
    let mut half_mant = full_mant >> shift;
    let rem = full_mant & ((1u32 << shift) - 1);
    let halfway = 1u32 << (shift - 1);
    if rem > halfway || (rem == halfway && half_mant & 1 == 1) {
        half_mant += 1;
    }
    sign | half_mant as u16
}

fn unpack14(b: &[u8]) -> [u16; 16] {
    let mut s = [0u16; 16];
    s[0] = ((b[0] as u16) << 8) | b[1] as u16;

    let shift = (b[2] >> 2) as u32;
    let bias = 0x20u32 << shift;

    let delta =
        |acc: u16, bits: u32| -> u16 { (acc as u32).wrapping_add((bits & 0x3F) << shift).wrapping_sub(bias) as u16 };

    s[4] = delta(s[0], ((b[2] as u32) << 4) | ((b[3] as u32) >> 4));
    s[8] = delta(s[4], ((b[3] as u32) << 2) | ((b[4] as u32) >> 6));
    s[12] = delta(s[8], b[4] as u32);

    s[1] = delta(s[0], (b[5] as u32) >> 2);
    s[5] = delta(s[4], ((b[5] as u32) << 4) | ((b[6] as u32) >> 4));
    s[9] = delta(s[8], ((b[6] as u32) << 2) | ((b[7] as u32) >> 6));
    s[13] = delta(s[12], b[7] as u32);

    s[2] = delta(s[1], (b[8] as u32) >> 2);
    s[6] = delta(s[5], ((b[8] as u32) << 4) | ((b[9] as u32) >> 4));
    s[10] = delta(s[9], ((b[9] as u32) << 2) | ((b[10] as u32) >> 6));
    s[14] = delta(s[13], b[10] as u32);

    s[3] = delta(s[2], (b[11] as u32) >> 2);
    s[7] = delta(s[6], ((b[11] as u32) << 4) | ((b[12] as u32) >> 4));
    s[11] = delta(s[10], ((b[12] as u32) << 2) | ((b[13] as u32) >> 6));
    s[15] = delta(s[14], b[13] as u32);

    for v in s.iter_mut() {
        if *v & 0x8000 != 0 {
            *v &= 0x7FFF;
        } else {
            *v = !*v;
        }
    }

    s
}

fn unpack3(b: &[u8]) -> [u16; 16] {
    let mut v = ((b[0] as u16) << 8) | b[1] as u16;
    if v & 0x8000 != 0 {
        v &= 0x7FFF;
    } else {
        v = !v;
    }
    [v; 16]
}

pub fn b44_decompress(data: &[u8], channels: &[ChunkChannel]) -> VexelResult<Vec<u8>> {
    let total_bytes: usize = channels.iter().map(|c| c.width * c.rows * c.bpe).sum();
    let mut out = vec![0u8; total_bytes];

    let mut pos = 0usize;
    let mut channel_offset = 0usize;

    for channel in channels {
        let nx = channel.width;
        let ny = channel.rows;
        let n_bytes = nx * ny * channel.bpe;

        if n_bytes == 0 {
            continue;
        }

        if channel.pixel_type != ExrPixelType::Half {
            if pos + n_bytes > data.len() {
                return Err(VexelError::Custom("EXR: B44 raw channel data truncated".to_string()));
            }
            out[channel_offset..channel_offset + n_bytes].copy_from_slice(&data[pos..pos + n_bytes]);
            pos += n_bytes;
            channel_offset += n_bytes;
            continue;
        }

        let mut y = 0usize;
        while y < ny {
            let mut x = 0usize;
            while x < nx {
                if pos + 3 > data.len() {
                    return Err(VexelError::Custom("EXR: B44 block data truncated".to_string()));
                }

                let mut s = if data[pos + 2] >= (13 << 2) {
                    let block = unpack3(&data[pos..pos + 3]);
                    pos += 3;
                    block
                } else {
                    if pos + 14 > data.len() {
                        return Err(VexelError::Custom("EXR: B44 block data truncated".to_string()));
                    }
                    let block = unpack14(&data[pos..pos + 14]);
                    pos += 14;
                    block
                };

                if channel.p_linear {
                    for v in s.iter_mut() {
                        *v = b44_to_linear(*v);
                    }
                }

                let cols = (nx - x).min(4);
                let rows = (ny - y).min(4);
                for row in 0..rows {
                    let dst = channel_offset + ((y + row) * nx + x) * 2;
                    for col in 0..cols {
                        let v = s[row * 4 + col];
                        out[dst + col * 2] = (v & 0xFF) as u8;
                        out[dst + col * 2 + 1] = (v >> 8) as u8;
                    }
                }

                x += 4;
            }
            y += 4;
        }

        channel_offset += n_bytes;
    }

    Ok(out)
}
