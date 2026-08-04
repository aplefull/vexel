pub fn decode_be_u16(src: &[u8], dst: &mut [u16]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { decode_be_u16_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return decode_be_u16_wasm(src, dst);

    #[allow(unreachable_code)]
    decode_be_u16_scalar(src, dst);
}

pub fn drop_alpha_u16(src: &[u16], dst: &mut [u16]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { drop_alpha_u16_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return drop_alpha_u16_wasm(src, dst);

    #[allow(unreachable_code)]
    drop_alpha_u16_scalar(src, dst);
}

pub fn is_opaque_u16(samples: &[u16]) -> bool {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { is_opaque_u16_avx2(samples) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return is_opaque_u16_wasm(samples);

    #[allow(unreachable_code)]
    is_opaque_u16_scalar(samples)
}

fn decode_be_u16_scalar(src: &[u8], dst: &mut [u16]) {
    let n = (src.len() / 2).min(dst.len());

    for (d, chunk) in dst[..n].iter_mut().zip(src.chunks_exact(2)) {
        *d = u16::from_be_bytes([chunk[0], chunk[1]]);
    }
}

fn drop_alpha_u16_scalar(src: &[u16], dst: &mut [u16]) {
    let n = (src.len() / 4).min(dst.len() / 3);

    for (d, s) in dst[..n * 3].chunks_exact_mut(3).zip(src.chunks_exact(4)) {
        d[0] = s[0];
        d[1] = s[1];
        d[2] = s[2];
    }
}

fn is_opaque_u16_scalar(samples: &[u16]) -> bool {
    samples.chunks_exact(4).all(|px| px[3] == u16::MAX)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn decode_be_u16_avx2(src: &[u8], dst: &mut [u16]) {
    use std::arch::x86_64::*;

    let n = (src.len() / 2).min(dst.len());
    let mut i = 0;

    let bswap = _mm256_set_epi8(
        14, 15, 12, 13, 10, 11, 8, 9, 6, 7, 4, 5, 2, 3, 0, 1, 14, 15, 12, 13, 10, 11, 8, 9, 6, 7, 4, 5, 2, 3, 0, 1,
    );

    while i + 32 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i * 2) as *const __m256i);
        let b = _mm256_loadu_si256(src.as_ptr().add(i * 2 + 32) as *const __m256i);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, _mm256_shuffle_epi8(a, bswap));
        _mm256_storeu_si256(dst.as_mut_ptr().add(i + 16) as *mut __m256i, _mm256_shuffle_epi8(b, bswap));

        i += 32;
    }

    while i + 16 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i * 2) as *const __m256i);
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, _mm256_shuffle_epi8(a, bswap));
        i += 16;
    }

    if i < n {
        decode_be_u16_scalar(&src[i * 2..], &mut dst[i..n]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn drop_alpha_u16_avx2(src: &[u16], dst: &mut [u16]) {
    use std::arch::x86_64::*;

    let pixels = (src.len() / 4).min(dst.len() / 3);
    let mut i = 0;

    let gather_rgb = _mm_set_epi8(-1, -1, -1, -1, 13, 12, 11, 10, 9, 8, 5, 4, 3, 2, 1, 0);

    let simd_pixels = pixels.saturating_sub(1);

    while i + 8 <= simd_pixels {
        let a = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
        let b = _mm_loadu_si128(src.as_ptr().add(i * 4 + 8) as *const __m128i);
        let c = _mm_loadu_si128(src.as_ptr().add(i * 4 + 16) as *const __m128i);
        let d = _mm_loadu_si128(src.as_ptr().add(i * 4 + 24) as *const __m128i);

        let pa = _mm_shuffle_epi8(a, gather_rgb);
        let pb = _mm_shuffle_epi8(b, gather_rgb);
        let pc = _mm_shuffle_epi8(c, gather_rgb);
        let pd = _mm_shuffle_epi8(d, gather_rgb);

        _mm_storeu_si128(dst.as_mut_ptr().add(i * 3) as *mut __m128i, pa);
        _mm_storeu_si128(dst.as_mut_ptr().add(i * 3 + 6) as *mut __m128i, pb);
        _mm_storeu_si128(dst.as_mut_ptr().add(i * 3 + 12) as *mut __m128i, pc);
        _mm_storeu_si128(dst.as_mut_ptr().add(i * 3 + 18) as *mut __m128i, pd);

        i += 8;
    }

    while i + 2 <= simd_pixels {
        let a = _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i);
        _mm_storeu_si128(dst.as_mut_ptr().add(i * 3) as *mut __m128i, _mm_shuffle_epi8(a, gather_rgb));
        i += 2;
    }

    if i < pixels {
        drop_alpha_u16_scalar(&src[i * 4..pixels * 4], &mut dst[i * 3..pixels * 3]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn is_opaque_u16_avx2(samples: &[u16]) -> bool {
    use std::arch::x86_64::*;

    const ALPHA_BITS: i32 = 0xC0C0C0C0u32 as i32;

    let pixels = samples.len() / 4;
    let mut i = 0;

    let ones = _mm256_set1_epi16(-1);

    while i + 16 <= pixels {
        let a = _mm256_cmpeq_epi16(_mm256_loadu_si256(samples.as_ptr().add(i * 4) as *const __m256i), ones);
        let b = _mm256_cmpeq_epi16(_mm256_loadu_si256(samples.as_ptr().add(i * 4 + 16) as *const __m256i), ones);
        let c = _mm256_cmpeq_epi16(_mm256_loadu_si256(samples.as_ptr().add(i * 4 + 32) as *const __m256i), ones);
        let d = _mm256_cmpeq_epi16(_mm256_loadu_si256(samples.as_ptr().add(i * 4 + 48) as *const __m256i), ones);

        let ok = (_mm256_movemask_epi8(a) & ALPHA_BITS == ALPHA_BITS)
            & (_mm256_movemask_epi8(b) & ALPHA_BITS == ALPHA_BITS)
            & (_mm256_movemask_epi8(c) & ALPHA_BITS == ALPHA_BITS)
            & (_mm256_movemask_epi8(d) & ALPHA_BITS == ALPHA_BITS);

        if !ok {
            return false;
        }

        i += 16;
    }

    while i + 4 <= pixels {
        let v = _mm256_loadu_si256(samples.as_ptr().add(i * 4) as *const __m256i);

        if _mm256_movemask_epi8(_mm256_cmpeq_epi16(v, ones)) & ALPHA_BITS != ALPHA_BITS {
            return false;
        }

        i += 4;
    }

    is_opaque_u16_scalar(&samples[i * 4..])
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn decode_be_u16_wasm(src: &[u8], dst: &mut [u16]) {
    use std::arch::wasm32::*;

    let n = (src.len() / 2).min(dst.len());
    let mut i = 0;

    while i + 8 <= n {
        let v = v128_load(src.as_ptr().add(i * 2) as *const v128);
        let swapped = u8x16_shuffle::<1, 0, 3, 2, 5, 4, 7, 6, 9, 8, 11, 10, 13, 12, 15, 14>(v, v);
        v128_store(dst.as_mut_ptr().add(i) as *mut v128, swapped);
        i += 8;
    }

    if i < n {
        decode_be_u16_scalar(&src[i * 2..], &mut dst[i..n]);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn drop_alpha_u16_wasm(src: &[u16], dst: &mut [u16]) {
    use std::arch::wasm32::*;

    let pixels = (src.len() / 4).min(dst.len() / 3);
    let simd_pixels = pixels.saturating_sub(1);
    let mut i = 0;

    while i + 2 <= simd_pixels {
        let v = v128_load(src.as_ptr().add(i * 4) as *const v128);
        let packed = u16x8_shuffle::<0, 1, 2, 4, 5, 6, 0, 0>(v, v);
        v128_store(dst.as_mut_ptr().add(i * 3) as *mut v128, packed);
        i += 2;
    }

    if i < pixels {
        drop_alpha_u16_scalar(&src[i * 4..pixels * 4], &mut dst[i * 3..pixels * 3]);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn is_opaque_u16_wasm(samples: &[u16]) -> bool {
    use std::arch::wasm32::*;

    let pixels = samples.len() / 4;
    let mut i = 0;

    let ones = u16x8_splat(u16::MAX);

    while i + 2 <= pixels {
        let v = v128_load(samples.as_ptr().add(i * 4) as *const v128);
        let eq = u16x8_eq(v, ones);
        let alphas = u16x8_shuffle::<3, 7, 3, 7, 3, 7, 3, 7>(eq, eq);

        if !u16x8_all_true(alphas) {
            return false;
        }

        i += 2;
    }

    is_opaque_u16_scalar(&samples[i * 4..])
}
