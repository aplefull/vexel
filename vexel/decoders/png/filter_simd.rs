pub fn decode_average(src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { decode_average_avx2(src, dst, prior, bytes_per_pixel) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return decode_average_wasm(src, dst, prior, bytes_per_pixel);

    decode_average_scalar(src, dst, prior, bytes_per_pixel);
}

// ─── Scalar ───────────────────────────────────────────────────

fn decode_average_scalar(src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
    let len = src.len();
    for i in 0..bytes_per_pixel {
        dst[i] = src[i].wrapping_add(prior[i] >> 1);
    }
    for i in bytes_per_pixel..len {
        let left = dst[i - bytes_per_pixel] as u16;
        let above = prior[i] as u16;
        dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn decode_average_avx2(src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
    use std::arch::x86_64::*;

    let len = src.len();

    const CHUNK: usize = 512;
    let mut above_half = [0u8; CHUNK];

    let mask = _mm256_set1_epi16(0x00FF_u16 as i16);

    let mut pos = 0;
    while pos < len {
        let end = (pos + CHUNK).min(len);
        let chunk_len = end - pos;
        let mut j = 0;

        while j + 32 <= chunk_len {
            let p = _mm256_loadu_si256(prior.as_ptr().add(pos + j) as *const __m256i);
            let lo = _mm256_srli_epi16(_mm256_and_si256(p, mask), 1);
            let hi = _mm256_srli_epi16(_mm256_srli_epi16(p, 8), 1);
            let half = _mm256_or_si256(lo, _mm256_slli_epi16(hi, 8));
            _mm256_storeu_si256(above_half.as_mut_ptr().add(j) as *mut __m256i, half);
            j += 32;
        }
        while j < chunk_len {
            above_half[j] = prior[pos + j] >> 1;
            j += 1;
        }

        let chunk_start = pos;
        let chunk_end = end;

        if chunk_start < bytes_per_pixel {
            let init_end = bytes_per_pixel.min(chunk_end);
            for i in chunk_start..init_end {
                dst[i] = src[i].wrapping_add(above_half[i - chunk_start]);
            }
            for i in init_end..chunk_end {
                let left = dst[i - bytes_per_pixel] as u16;
                let above = prior[i] as u16;
                dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
            }
        } else {
            for i in chunk_start..chunk_end {
                let left = dst[i - bytes_per_pixel] as u16;
                let above = prior[i] as u16;
                dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
            }
        }

        pos = end;
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn decode_average_wasm(src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
    use std::arch::wasm32::*;

    let len = src.len();

    const CHUNK: usize = 512;
    let mut above_half = [0u8; CHUNK];
    let mask = u16x8_splat(0x00FF);

    let mut pos = 0;
    while pos < len {
        let end = (pos + CHUNK).min(len);
        let chunk_len = end - pos;
        let mut j = 0;

        while j + 16 <= chunk_len {
            let p = unsafe { v128_load(prior.as_ptr().add(pos + j) as *const v128) };
            let lo = u16x8_shr(v128_and(p, mask), 1);
            let hi = u16x8_shr(u16x8_shr(p, 8), 1);
            let half = v128_or(lo, u16x8_shl(hi, 8));
            unsafe { v128_store(above_half.as_mut_ptr().add(j) as *mut v128, half) };
            j += 16;
        }
        while j < chunk_len {
            above_half[j] = prior[pos + j] >> 1;
            j += 1;
        }

        let chunk_start = pos;
        let chunk_end = end;

        if chunk_start < bytes_per_pixel {
            let init_end = bytes_per_pixel.min(chunk_end);
            for i in chunk_start..init_end {
                dst[i] = src[i].wrapping_add(above_half[i - chunk_start]);
            }
            for i in init_end..chunk_end {
                let left = dst[i - bytes_per_pixel] as u16;
                let above = prior[i] as u16;
                dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
            }
        } else {
            for i in chunk_start..chunk_end {
                let left = dst[i - bytes_per_pixel] as u16;
                let above = prior[i] as u16;
                dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
            }
        }

        pos = end;
    }
}
