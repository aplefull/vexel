use crate::decoders::exr::huffman::huf_decompress;
use crate::decoders::exr::types::ChunkChannel;
use crate::utils::error::{VexelError, VexelResult};

const USHORT_RANGE: usize = 1 << 16;
const BITMAP_SIZE: usize = USHORT_RANGE >> 3;

const NBITS: i32 = 16;
const A_OFFSET: i32 = 1 << (NBITS - 1);
const MOD_MASK: i32 = (1 << NBITS) - 1;

fn reverse_lut_from_bitmap(bitmap: &[u8; BITMAP_SIZE]) -> (Vec<u16>, u16) {
    let mut lut = vec![0u16; USHORT_RANGE];
    let mut k = 0usize;

    for i in 0..USHORT_RANGE {
        if i == 0 || (bitmap[i >> 3] & (1 << (i & 7))) != 0 {
            lut[k] = i as u16;
            k += 1;
        }
    }

    let max_value = (k - 1) as u16;
    (lut, max_value)
}

fn apply_lut(lut: &[u16], data: &mut [u16]) {
    for v in data.iter_mut() {
        *v = lut[*v as usize];
    }
}

#[inline(always)]
fn wdec14(l: u16, h: u16) -> (u16, u16) {
    let ls = l as i16;
    let hs = h as i16;

    let hi = hs as i32;
    let ai = ls as i32 + (hi & 1) + (hi >> 1);

    let a = ai as i16;
    let b = (ai - hi) as i16;

    (a as u16, b as u16)
}

#[inline(always)]
fn wdec16(l: u16, h: u16) -> (u16, u16) {
    let m = l as i32;
    let d = h as i32;
    let b = (m - (d >> 1)) & MOD_MASK;
    let a = (d + b - A_OFFSET) & MOD_MASK;
    (a as u16, b as u16)
}

fn wav_2d_decode(buf: &mut [u16], base: usize, nx: usize, ox: usize, ny: usize, oy: usize, mx: u16) {
    let w14 = mx < (1 << 14);
    let n = nx.min(ny);

    let mut p = 1usize;
    while p <= n {
        p <<= 1;
    }
    p >>= 1;
    let mut p2 = p;
    p >>= 1;

    while p >= 1 {
        let oy1 = oy * p;
        let oy2 = oy * p2;
        let ox1 = ox * p;
        let ox2 = ox * p2;

        let ey = base + oy * (ny - p2);
        let mut py = base;

        while py <= ey {
            let ex = py + ox * (nx - p2);
            let mut px = py;

            while px <= ex {
                let p01 = px + ox1;
                let p10 = px + oy1;
                let p11 = p10 + ox1;

                if w14 {
                    let a = buf[px] as i16;
                    let b = buf[p10] as i16;
                    let c = buf[p01] as i16;
                    let d = buf[p11] as i16;

                    let ai = a as i32;
                    let bi = b as i32;
                    let ci = c as i32;
                    let di = d as i32;

                    let i00 = ai + (bi & 1) + (bi >> 1);
                    let i10 = i00 - bi;
                    let i01 = ci + (di & 1) + (di >> 1);
                    let i11 = i01 - di;

                    let ai = i00 + (i01 & 1) + (i01 >> 1);
                    let bi = ai - i01;
                    let ci = i10 + (i11 & 1) + (i11 >> 1);
                    let di = ci - i11;

                    buf[px] = ai as u16;
                    buf[p01] = bi as u16;
                    buf[p10] = ci as u16;
                    buf[p11] = di as u16;
                } else {
                    let (i00, i10) = wdec16(buf[px], buf[p10]);
                    let (i01, i11) = wdec16(buf[p01], buf[p11]);
                    let (a, b) = wdec16(i00, i01);
                    buf[px] = a;
                    buf[p01] = b;
                    let (a, b) = wdec16(i10, i11);
                    buf[p10] = a;
                    buf[p11] = b;
                }

                px += ox2;
            }

            if nx & p != 0 {
                let p10 = px + oy1;
                let (a, b) = if w14 { wdec14(buf[px], buf[p10]) } else { wdec16(buf[px], buf[p10]) };
                buf[px] = a;
                buf[p10] = b;
            }

            py += oy2;
        }

        if ny & p != 0 {
            let ex = py + ox * (nx - p2);
            let mut px = py;

            while px <= ex {
                let p01 = px + ox1;
                let (a, b) = if w14 { wdec14(buf[px], buf[p01]) } else { wdec16(buf[px], buf[p01]) };
                buf[px] = a;
                buf[p01] = b;
                px += ox2;
            }
        }

        p2 = p;
        p >>= 1;
    }
}

pub fn piz_decompress(data: &[u8], channels: &[ChunkChannel]) -> VexelResult<Vec<u8>> {
    let total_u16: usize = channels.iter().map(|c| c.width * c.rows * (c.bpe / 2)).sum();

    if data.len() < 4 {
        return Err(VexelError::Custom("EXR: PIZ chunk too short".to_string()));
    }

    let min_non_zero = u16::from_le_bytes([data[0], data[1]]) as usize;
    let max_non_zero = u16::from_le_bytes([data[2], data[3]]) as usize;
    let mut pos = 4usize;

    if max_non_zero >= BITMAP_SIZE {
        return Err(VexelError::Custom("EXR: PIZ bitmap range out of bounds".to_string()));
    }

    let mut bitmap = [0u8; BITMAP_SIZE];
    if min_non_zero <= max_non_zero {
        let count = max_non_zero - min_non_zero + 1;
        if pos + count > data.len() {
            return Err(VexelError::Custom("EXR: PIZ bitmap truncated".to_string()));
        }
        bitmap[min_non_zero..=max_non_zero].copy_from_slice(&data[pos..pos + count]);
        pos += count;
    }

    let (lut, max_value) = reverse_lut_from_bitmap(&bitmap);

    if pos + 4 > data.len() {
        return Err(VexelError::Custom("EXR: PIZ chunk truncated".to_string()));
    }
    let huf_bytes = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    if pos + huf_bytes > data.len() {
        return Err(VexelError::Custom("EXR: PIZ huffman data truncated".to_string()));
    }

    let mut values = huf_decompress(&data[pos..pos + huf_bytes], total_u16)?;

    let mut offset = 0usize;
    for channel in channels {
        let nx = channel.width;
        let ny = channel.rows;
        let wcount = channel.bpe / 2;
        let size = nx * ny * wcount;

        if nx > 0 && ny > 0 {
            for j in 0..wcount {
                wav_2d_decode(&mut values, offset + j, nx, wcount, ny, wcount * nx, max_value);
            }
        }

        offset += size;
    }

    apply_lut(&lut, &mut values);

    let mut out = Vec::with_capacity(total_u16 * 2);
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }

    Ok(out)
}
