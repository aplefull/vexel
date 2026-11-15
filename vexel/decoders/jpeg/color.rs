use crate::decoders::jpeg::idct::SimdLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JpegColorspace {
    Grayscale,
    YCbCr,
    Rgb,
    Cmyk,
    Ycck,
    Rgba,
    Unknown,
}

impl JpegColorspace {
    pub fn channels(self) -> usize {
        match self {
            JpegColorspace::Grayscale => 1,
            JpegColorspace::Rgba => 4,
            _ => 3,
        }
    }
}

const NEUTRAL: [u8; 1] = [128];

pub fn convert_row(level: SimdLevel, kind: JpegColorspace, rows: &[&[u8]; 4], out: &mut [u8], width: usize) {
    let channel = |i: usize| -> &[u8] {
        let row = rows[i];
        if row.len() >= width {
            &row[..width]
        } else {
            &[]
        }
    };
    match kind {
        JpegColorspace::Grayscale => {
            let src = &rows[0][..width.min(rows[0].len())];
            out[..src.len()].copy_from_slice(src);
            out[src.len()..width].fill(0);
        }
        JpegColorspace::YCbCr => {
            let (y, cb, cr) = (channel(0), channel(1), channel(2));
            if cb.is_empty() || cr.is_empty() {
                interleave3(y, cb, cr, out, width);
            } else {
                ycbcr_to_rgb(level, y, cb, cr, &mut out[..width * 3]);
            }
        }
        JpegColorspace::Rgb | JpegColorspace::Unknown => interleave3(channel(0), channel(1), channel(2), out, width),
        JpegColorspace::Cmyk => cmyk_to_rgb(channel(0), channel(1), channel(2), channel(3), out, width),
        JpegColorspace::Ycck => ycck_to_rgb(channel(0), channel(1), channel(2), channel(3), out, width),
        JpegColorspace::Rgba => {
            for x in 0..width {
                for c in 0..4 {
                    out[x * 4 + c] = channel(c).get(x).copied().unwrap_or(NEUTRAL[0]);
                }
            }
        }
    }
}

fn interleave3(a: &[u8], b: &[u8], c: &[u8], out: &mut [u8], width: usize) {
    for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
        px[0] = a.get(x).copied().unwrap_or(NEUTRAL[0]);
        px[1] = b.get(x).copied().unwrap_or(NEUTRAL[0]);
        px[2] = c.get(x).copied().unwrap_or(NEUTRAL[0]);
    }
}

#[inline(always)]
fn ycc_pixel(y: u8, cb: u8, cr: u8) -> (i32, i32, i32) {
    let y = (y as i32) << 16;
    let cb = cb as i32 - 128;
    let cr = cr as i32 - 128;
    ((y + 91881 * cr + 32768) >> 16, (y - 22554 * cb - 46802 * cr + 32768) >> 16, (y + 116130 * cb + 32768) >> 16)
}

fn ycbcr_to_rgb(level: SimdLevel, y: &[u8], cb: &[u8], cr: &[u8], out: &mut [u8]) {
    match level {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => unsafe { ycbcr_to_rgb_avx2(y, cb, cr, out) },
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        SimdLevel::Simd128 => ycbcr_to_rgb_simd128(y, cb, cr, out),
        SimdLevel::Scalar => ycbcr_to_rgb_scalar(y, cb, cr, out),
    }
}

fn ycbcr_to_rgb_scalar(y: &[u8], cb: &[u8], cr: &[u8], out: &mut [u8]) {
    for (((px, &y), &cb), &cr) in out.chunks_exact_mut(3).zip(y).zip(cb).zip(cr) {
        let (r, g, b) = ycc_pixel(y, cb, cr);
        px[0] = r.clamp(0, 255) as u8;
        px[1] = g.clamp(0, 255) as u8;
        px[2] = b.clamp(0, 255) as u8;
    }
}

fn cmyk_to_rgb(c: &[u8], m: &[u8], y: &[u8], k: &[u8], out: &mut [u8], width: usize) {
    for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
        let kv = k.get(x).copied().unwrap_or(NEUTRAL[0]) as u32;
        let scale = |v: u8| ((v as u32 * kv + 127) / 255) as u8;
        px[0] = scale(c.get(x).copied().unwrap_or(NEUTRAL[0]));
        px[1] = scale(m.get(x).copied().unwrap_or(NEUTRAL[0]));
        px[2] = scale(y.get(x).copied().unwrap_or(NEUTRAL[0]));
    }
}

fn ycck_to_rgb(yy: &[u8], cb: &[u8], cr: &[u8], k: &[u8], out: &mut [u8], width: usize) {
    for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
        let kv = k.get(x).copied().unwrap_or(NEUTRAL[0]) as i32;
        let (r, g, b) = ycc_pixel(
            yy.get(x).copied().unwrap_or(NEUTRAL[0]),
            cb.get(x).copied().unwrap_or(NEUTRAL[0]),
            cr.get(x).copied().unwrap_or(NEUTRAL[0]),
        );
        let scale = |v: i32| (((255 - v).clamp(0, 255) * kv + 127) / 255) as u8;
        px[0] = scale(r);
        px[1] = scale(g);
        px[2] = scale(b);
    }
}

#[cfg(target_arch = "x86_64")]
const fn rgb_interleave_masks() -> [[[i8; 16]; 3]; 3] {
    let mut masks = [[[-128i8; 16]; 3]; 3];
    let mut chunk = 0;
    while chunk < 3 {
        let mut k = 0;
        while k < 16 {
            let byte = chunk * 16 + k;
            masks[chunk][byte % 3][k] = (byte / 3) as i8;
            k += 1;
        }
        chunk += 1;
    }
    masks
}

#[cfg(target_arch = "x86_64")]
const RGB_MASKS: [[[i8; 16]; 3]; 3] = rgb_interleave_masks();

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn store_rgb16(
    r: std::arch::x86_64::__m128i,
    g: std::arch::x86_64::__m128i,
    b: std::arch::x86_64::__m128i,
    dst: *mut u8,
) {
    use std::arch::x86_64::*;
    for (chunk, masks) in RGB_MASKS.iter().enumerate() {
        let m0 = _mm_loadu_si128(masks[0].as_ptr() as *const __m128i);
        let m1 = _mm_loadu_si128(masks[1].as_ptr() as *const __m128i);
        let m2 = _mm_loadu_si128(masks[2].as_ptr() as *const __m128i);
        let v = _mm_or_si128(_mm_or_si128(_mm_shuffle_epi8(r, m0), _mm_shuffle_epi8(g, m1)), _mm_shuffle_epi8(b, m2));
        _mm_storeu_si128(dst.add(chunk * 16) as *mut __m128i, v);
    }
}

// TODO maybe move all SIMD to separate files project-wide
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn narrow_u8(v: std::arch::x86_64::__m256i) -> std::arch::x86_64::__m128i {
    use std::arch::x86_64::*;
    _mm256_castsi256_si128(_mm256_permute4x64_epi64::<0b1000>(_mm256_packus_epi16(v, v)))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn ycbcr_to_rgb_avx2(y: &[u8], cb: &[u8], cr: &[u8], out: &mut [u8]) {
    use std::arch::x86_64::*;
    let n = y.len().min(cb.len()).min(cr.len()).min(out.len() / 3);
    let center = _mm256_set1_epi16(128);
    let one = _mm256_set1_epi16(1);
    let f_0_402 = _mm256_set1_epi16(26345);
    let f_m0_228 = _mm256_set1_epi16(-14942);
    let g_coef = _mm256_set1_epi32(((18734i32) << 16) | (-22554i32 & 0xFFFF));
    let half = _mm256_set1_epi32(32768);
    let mut x = 0;
    while x + 16 <= n {
        let yv = _mm256_cvtepu8_epi16(_mm_loadu_si128(y.as_ptr().add(x) as *const __m128i));
        let cbv = _mm256_sub_epi16(_mm256_cvtepu8_epi16(_mm_loadu_si128(cb.as_ptr().add(x) as *const __m128i)), center);
        let crv = _mm256_sub_epi16(_mm256_cvtepu8_epi16(_mm_loadu_si128(cr.as_ptr().add(x) as *const __m128i)), center);

        let r_hi = _mm256_mulhi_epi16(_mm256_add_epi16(crv, crv), f_0_402);
        let r_off = _mm256_add_epi16(crv, _mm256_srai_epi16::<1>(_mm256_add_epi16(r_hi, one)));
        let b_hi = _mm256_mulhi_epi16(_mm256_add_epi16(cbv, cbv), f_m0_228);
        let b_off = _mm256_add_epi16(_mm256_add_epi16(cbv, cbv), _mm256_srai_epi16::<1>(_mm256_add_epi16(b_hi, one)));
        let lo = _mm256_madd_epi16(_mm256_unpacklo_epi16(cbv, crv), g_coef);
        let hi = _mm256_madd_epi16(_mm256_unpackhi_epi16(cbv, crv), g_coef);
        let lo = _mm256_srai_epi32::<16>(_mm256_add_epi32(lo, half));
        let hi = _mm256_srai_epi32::<16>(_mm256_add_epi32(hi, half));
        let g_off = _mm256_sub_epi16(_mm256_packs_epi32(lo, hi), crv);

        let r = narrow_u8(_mm256_add_epi16(yv, r_off));
        let g = narrow_u8(_mm256_add_epi16(yv, g_off));
        let b = narrow_u8(_mm256_add_epi16(yv, b_off));
        store_rgb16(r, g, b, out.as_mut_ptr().add(x * 3));
        x += 16;
    }
    ycbcr_to_rgb_scalar(&y[x..n], &cb[x..n], &cr[x..n], &mut out[x * 3..n * 3]);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn ycbcr_to_rgb_simd128(y: &[u8], cb: &[u8], cr: &[u8], out: &mut [u8]) {
    use core::arch::wasm32::*;
    let n = y.len().min(cb.len()).min(cr.len()).min(out.len() / 3);
    let center = i32x4_splat(128);
    let half = i32x4_splat(32768);
    let channel = |yv: v128, cbv: v128, crv: v128| -> (v128, v128, v128) {
        let yv = i32x4_shl(yv, 16);
        let cbv = i32x4_sub(cbv, center);
        let crv = i32x4_sub(crv, center);
        let r = i32x4_shr(i32x4_add(i32x4_add(yv, i32x4_mul(crv, i32x4_splat(91881))), half), 16);
        let g = i32x4_add(i32x4_mul(cbv, i32x4_splat(-22554)), i32x4_mul(crv, i32x4_splat(-46802)));
        let g = i32x4_shr(i32x4_add(i32x4_add(yv, g), half), 16);
        let b = i32x4_shr(i32x4_add(i32x4_add(yv, i32x4_mul(cbv, i32x4_splat(116130))), half), 16);
        (r, g, b)
    };
    let quarters = |v: v128| -> [v128; 4] {
        let lo = u16x8_extend_low_u8x16(v);
        let hi = u16x8_extend_high_u8x16(v);
        [
            u32x4_extend_low_u16x8(lo),
            u32x4_extend_high_u16x8(lo),
            u32x4_extend_low_u16x8(hi),
            u32x4_extend_high_u16x8(hi),
        ]
    };
    let pack =
        |q: [v128; 4]| -> v128 { u8x16_narrow_i16x8(i16x8_narrow_i32x4(q[0], q[1]), i16x8_narrow_i32x4(q[2], q[3])) };
    let mut x = 0;
    while x + 16 <= n {
        let (yq, cbq, crq) = unsafe {
            (
                quarters(v128_load(y.as_ptr().add(x) as *const v128)),
                quarters(v128_load(cb.as_ptr().add(x) as *const v128)),
                quarters(v128_load(cr.as_ptr().add(x) as *const v128)),
            )
        };
        let mut rq = [i32x4_splat(0); 4];
        let mut gq = [i32x4_splat(0); 4];
        let mut bq = [i32x4_splat(0); 4];
        for i in 0..4 {
            let (r, g, b) = channel(yq[i], cbq[i], crq[i]);
            rq[i] = r;
            gq[i] = g;
            bq[i] = b;
        }
        let r = pack(rq);
        let g = pack(gq);
        let b = pack(bq);
        let rg0 = i8x16_shuffle::<0, 16, 0, 1, 17, 1, 2, 18, 2, 3, 19, 3, 4, 20, 4, 5>(r, g);
        let out0 = i8x16_shuffle::<0, 1, 16, 3, 4, 17, 6, 7, 18, 9, 10, 19, 12, 13, 20, 15>(rg0, b);
        let rg1 = i8x16_shuffle::<21, 5, 6, 22, 6, 7, 23, 7, 8, 24, 8, 9, 25, 9, 10, 26>(r, g);
        let out1 = i8x16_shuffle::<0, 21, 2, 3, 22, 5, 6, 23, 8, 9, 24, 11, 12, 25, 14, 15>(rg1, b);
        let rg2 = i8x16_shuffle::<10, 11, 27, 11, 12, 28, 12, 13, 29, 13, 14, 30, 14, 15, 31, 15>(r, g);
        let out2 = i8x16_shuffle::<26, 1, 2, 27, 4, 5, 28, 7, 8, 29, 10, 11, 30, 13, 14, 31>(rg2, b);
        unsafe {
            let dst = out.as_mut_ptr().add(x * 3);
            v128_store(dst as *mut v128, out0);
            v128_store(dst.add(16) as *mut v128, out1);
            v128_store(dst.add(32) as *mut v128, out2);
        }
        x += 16;
    }
    ycbcr_to_rgb_scalar(&y[x..n], &cb[x..n], &cr[x..n], &mut out[x * 3..n * 3]);
}

pub fn convert_row_wide(kind: JpegColorspace, rows: &[&[u16]; 4], out: &mut [u16], width: usize, precision: u8) {
    let max = (1i32 << precision) - 1;
    let center = 1i32 << (precision - 1);
    let sample = |c: usize, x: usize| -> i32 { rows[c].get(x).map(|&v| v as i32).unwrap_or(center) };
    let ycc = |x: usize| -> (i32, i32, i32) {
        let y = (sample(0, x) as i64) << 16;
        let cb = (sample(1, x) - center) as i64;
        let cr = (sample(2, x) - center) as i64;
        (
            ((y + 91881 * cr + 32768) >> 16) as i32,
            ((y - 22554 * cb - 46802 * cr + 32768) >> 16) as i32,
            ((y + 116130 * cb + 32768) >> 16) as i32,
        )
    };
    match kind {
        JpegColorspace::Grayscale => {
            for (x, o) in out[..width].iter_mut().enumerate() {
                *o = sample(0, x).clamp(0, max) as u16;
            }
        }
        JpegColorspace::YCbCr => {
            for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
                let (r, g, b) = ycc(x);
                px[0] = r.clamp(0, max) as u16;
                px[1] = g.clamp(0, max) as u16;
                px[2] = b.clamp(0, max) as u16;
            }
        }
        JpegColorspace::Rgb | JpegColorspace::Unknown => {
            for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
                for (c, o) in px.iter_mut().enumerate() {
                    *o = sample(c, x).clamp(0, max) as u16;
                }
            }
        }
        JpegColorspace::Cmyk => {
            for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
                let k = sample(3, x).clamp(0, max) as i64;
                for (c, o) in px.iter_mut().enumerate() {
                    let v = sample(c, x).clamp(0, max) as i64;
                    *o = ((v * k + max as i64 / 2) / max as i64) as u16;
                }
            }
        }
        JpegColorspace::Ycck => {
            for (x, px) in out[..width * 3].chunks_exact_mut(3).enumerate() {
                let k = sample(3, x).clamp(0, max) as i64;
                let (r, g, b) = ycc(x);
                for (o, v) in px.iter_mut().zip([r, g, b]) {
                    let inverted = (max - v).clamp(0, max) as i64;
                    *o = ((inverted * k + max as i64 / 2) / max as i64) as u16;
                }
            }
        }
        JpegColorspace::Rgba => {
            for (x, px) in out[..width * 4].chunks_exact_mut(4).enumerate() {
                for (c, o) in px.iter_mut().enumerate() {
                    *o = sample(c, x).clamp(0, max) as u16;
                }
            }
        }
    }
}
