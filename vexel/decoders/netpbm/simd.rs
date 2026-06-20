pub fn scale_u32_to_u8(src: &[u32], dst: &mut [u8], max_value: u32) {
    debug_assert_eq!(src.len(), dst.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u32_to_u8_avx2(src, dst, max_value) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_u32_to_u8_wasm(src, dst, max_value);

    scale_u32_to_u8_scalar(src, dst, max_value);
}

pub fn scale_u32_to_u16(src: &[u32], dst: &mut [u16], max_value: u32) {
    debug_assert_eq!(src.len(), dst.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u32_to_u16_avx2(src, dst, max_value) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_u32_to_u16_wasm(src, dst, max_value);

    scale_u32_to_u16_scalar(src, dst, max_value);
}

pub fn scale_u8(src: &[u8], dst: &mut [u8], max_value: u8) {
    debug_assert_eq!(src.len(), dst.len());

    if max_value == 255 {
        dst.copy_from_slice(src);
        return;
    }

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u8_avx2(src, dst, max_value) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_u8_wasm(src, dst, max_value);

    scale_u8_scalar(src, dst, max_value);
}

pub fn scale_u16_be(src: &[u8], dst: &mut [u16], max_value: u16) {
    debug_assert_eq!(src.len(), dst.len() * 2);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u16_be_avx2(src, dst, max_value) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_u16_be_wasm(src, dst, max_value);

    scale_u16_be_scalar(src, dst, max_value);
}

pub fn unpack_bits(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    debug_assert_eq!(dst.len(), (width * height) as usize);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("ssse3") {
        return unsafe { unpack_bits_avx2(src, dst, width, height) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unpack_bits_wasm(src, dst, width, height);

    unpack_bits_scalar(src, dst, width, height);
}

// ─── Scalar ───────────────────────────────────────────────────

fn scale_u32_to_u8_scalar(src: &[u32], dst: &mut [u8], max_value: u32) {
    let scale = 255.0 / max_value as f32;
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (*s as f32 * scale).round() as u8;
    }
}

fn scale_u32_to_u16_scalar(src: &[u32], dst: &mut [u16], max_value: u32) {
    let scale = 65535.0 / max_value as f32;
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (*s as f32 * scale).round().min(65535.0) as u16;
    }
}

fn scale_u8_scalar(src: &[u8], dst: &mut [u8], max_value: u8) {
    let max = max_value as f32;
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (*s as f32 * 255.0 / max).round() as u8;
    }
}

fn scale_u16_be_scalar(src: &[u8], dst: &mut [u16], max_value: u16) {
    let max = max_value as f32;
    for (chunk, d) in src.chunks_exact(2).zip(dst.iter_mut()) {
        let raw = u16::from_be_bytes([chunk[0], chunk[1]]);
        *d = if max_value == 65535 {
            raw
        } else {
            (raw as f32 * 65535.0 / max).round().min(65535.0) as u16
        };
    }
}

fn unpack_bits_scalar(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    let bytes_per_row = ((width + 7) / 8) as usize;
    let width = width as usize;

    for row in 0..height as usize {
        let src_row = &src[row * bytes_per_row..];
        let dst_row = &mut dst[row * width..][..width];
        for x in 0..width {
            let byte = src_row[x / 8];
            let bit = (byte >> (7 - (x & 7))) & 1;
            dst_row[x] = !bit & 1;
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u8_avx2(src: &[u8], dst: &mut [u8], max_value: u8) {
    use std::arch::x86_64::*;

    let n = src.len();
    let scale = _mm256_set1_ps(255.0 / max_value as f32);

    let mut i = 0usize;

    while i + 32 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);

        let b0 = _mm256_cvtepu8_epi32(_mm256_castsi256_si128(v));
        let b1 = _mm256_cvtepu8_epi32(_mm_srli_si128(_mm256_castsi256_si128(v), 8));
        let b2 = _mm256_cvtepu8_epi32(_mm256_extracti128_si256(v, 1));
        let b3 = _mm256_cvtepu8_epi32(_mm_srli_si128(_mm256_extracti128_si256(v, 1), 8));

        let s0 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(b0), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
        let s1 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(b1), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
        let s2 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(b2), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
        let s3 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(b3), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));

        let p01 = _mm256_permute4x64_epi64(_mm256_packus_epi32(s0, s1), 0xD8);
        let p23 = _mm256_permute4x64_epi64(_mm256_packus_epi32(s2, s3), 0xD8);
        let result = _mm256_permute4x64_epi64(_mm256_packus_epi16(p01, p23), 0xD8);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, result);
        i += 32;
    }

    scale_u8_scalar(&src[i..], &mut dst[i..], max_value);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u16_be_avx2(src: &[u8], dst: &mut [u16], max_value: u16) {
    use std::arch::x86_64::*;

    let n = dst.len();
    let scale = _mm256_set1_ps(65535.0 / max_value as f32);

    let bswap16 = _mm256_set_epi8(
        14, 15, 12, 13, 10, 11, 8, 9,
        6, 7, 4, 5, 2, 3, 0, 1,
        14, 15, 12, 13, 10, 11, 8, 9,
        6, 7, 4, 5, 2, 3, 0, 1,
    );

    let mut i = 0usize;

    while i + 16 <= n {
        let raw = _mm256_loadu_si256(src.as_ptr().add(i * 2) as *const __m256i);
        let swapped = _mm256_shuffle_epi8(raw, bswap16);

        let lo = _mm256_cvtepu16_epi32(_mm256_castsi256_si128(swapped));
        let hi = _mm256_cvtepu16_epi32(_mm256_extracti128_si256(swapped, 1));

        if max_value == 65535 {
            _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, swapped);
        } else {
            let s0 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(lo), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
            let s1 = _mm256_cvtps_epi32(_mm256_round_ps(_mm256_mul_ps(_mm256_cvtepi32_ps(hi), scale), _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
            let packed = _mm256_permute4x64_epi64(_mm256_packus_epi32(s0, s1), 0xD8);
            _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed);
        }

        i += 16;
    }

    scale_u16_be_scalar(&src[i * 2..], &mut dst[i..], max_value);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn unpack_bits_avx2(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    use std::arch::x86_64::*;

    let bytes_per_row = ((width + 7) / 8) as usize;
    let width = width as usize;

    let bit_mask = _mm_set_epi8(1, 2, 4, 8, 16, 32, 64, -128i8, 1, 2, 4, 8, 16, 32, 64, -128i8);
    let one = _mm_set1_epi8(1i8);
    let zero = _mm_setzero_si128();

    for row in 0..height as usize {
        let src_row = &src[row * bytes_per_row..];
        let dst_row = &mut dst[row * width..][..width];

        let mut x = 0usize;

        while x + 16 <= width {
            let byte_idx = x / 8;

            let b0 = _mm_set1_epi8(src_row[byte_idx] as i8);
            let b1 = _mm_set1_epi8(src_row[byte_idx + 1] as i8);

            let bits0 = _mm_cmpeq_epi8(_mm_and_si128(b0, bit_mask), zero);
            let bits1 = _mm_cmpeq_epi8(_mm_and_si128(b1, bit_mask), zero);

            let out0 = _mm_and_si128(bits0, one);
            let out1 = _mm_and_si128(bits1, one);

            _mm_storel_epi64(dst_row.as_mut_ptr().add(x) as *mut __m128i, out0);
            _mm_storel_epi64(dst_row.as_mut_ptr().add(x + 8) as *mut __m128i, out1);

            x += 16;
        }

        while x < width {
            let byte = src_row[x / 8];
            let bit = (byte >> (7 - (x & 7))) & 1;
            dst_row[x] = !bit & 1;
            x += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u32_to_u8_avx2(src: &[u32], dst: &mut [u8], max_value: u32) {
    use std::arch::x86_64::*;

    let n = src.len();
    let scale = _mm256_set1_ps(255.0 / max_value as f32);

    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let f = _mm256_mul_ps(_mm256_cvtepi32_ps(v), scale);
        let s = _mm256_cvtps_epi32(_mm256_round_ps(f, _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));

        let zero = _mm256_setzero_si256();
        let packed16 = _mm256_permute4x64_epi64(_mm256_packus_epi32(s, zero), 0xD8);
        let packed8 = _mm256_permute4x64_epi64(_mm256_packus_epi16(packed16, zero), 0xD8);

        _mm_storel_epi64(
            dst.as_mut_ptr().add(i) as *mut __m128i,
            _mm256_castsi256_si128(packed8),
        );

        i += 8;
    }

    scale_u32_to_u8_scalar(&src[i..], &mut dst[i..], max_value);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u32_to_u16_avx2(src: &[u32], dst: &mut [u16], max_value: u32) {
    use std::arch::x86_64::*;

    let n = src.len();
    let scale = _mm256_set1_ps(65535.0 / max_value as f32);

    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let f = _mm256_mul_ps(_mm256_cvtepi32_ps(v), scale);
        let s = _mm256_cvtps_epi32(_mm256_round_ps(f, _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));

        let zero = _mm256_setzero_si256();
        let packed16 = _mm256_permute4x64_epi64(_mm256_packus_epi32(s, zero), 0xD8);

        _mm_storeu_si128(
            dst.as_mut_ptr().add(i) as *mut __m128i,
            _mm256_castsi256_si128(packed16),
        );

        i += 8;
    }

    scale_u32_to_u16_scalar(&src[i..], &mut dst[i..], max_value);
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_u32_to_u8_wasm(src: &[u32], dst: &mut [u8], max_value: u32) {
    use std::arch::wasm32::*;

    let n = src.len();
    let scale = f32x4_splat(255.0 / max_value as f32);

    let mut i = 0usize;

    while i + 4 <= n {
        let v = unsafe { v128_load(src.as_ptr().add(i) as *const v128) };
        let s = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(v), scale)));
        let p16 = u16x8_narrow_i32x4(s, s);
        let p8 = u8x16_narrow_i16x8(p16, p16);

        unsafe {
            (dst.as_mut_ptr().add(i) as *mut u32).write_unaligned(u32x4_extract_lane::<0>(p8));
        }

        i += 4;
    }

    scale_u32_to_u8_scalar(&src[i..], &mut dst[i..], max_value);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_u32_to_u16_wasm(src: &[u32], dst: &mut [u16], max_value: u32) {
    use std::arch::wasm32::*;

    let n = src.len();
    let scale = f32x4_splat(65535.0 / max_value as f32);

    let mut i = 0usize;

    while i + 4 <= n {
        let v = unsafe { v128_load(src.as_ptr().add(i) as *const v128) };
        let s = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(v), scale)));
        let packed = u16x8_narrow_i32x4(s, s);

        unsafe {
            (dst.as_mut_ptr().add(i) as *mut u64).write_unaligned(u64x2_extract_lane::<0>(packed));
        }

        i += 4;
    }

    scale_u32_to_u16_scalar(&src[i..], &mut dst[i..], max_value);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_u8_wasm(src: &[u8], dst: &mut [u8], max_value: u8) {
    use std::arch::wasm32::*;

    let n = src.len();
    let scale = f32x4_splat(255.0 / max_value as f32);

    let mut i = 0usize;

    while i + 16 <= n {
        let v = unsafe { v128_load(src.as_ptr().add(i) as *const v128) };

        let b0 = u32x4_extend_low_u16x8(u16x8_extend_low_u8x16(v));
        let b1 = u32x4_extend_high_u16x8(u16x8_extend_low_u8x16(v));
        let b2 = u32x4_extend_low_u16x8(u16x8_extend_high_u8x16(v));
        let b3 = u32x4_extend_high_u16x8(u16x8_extend_high_u8x16(v));

        let s0 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(b0), scale)));
        let s1 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(b1), scale)));
        let s2 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(b2), scale)));
        let s3 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(b3), scale)));

        let p01 = u16x8_narrow_i32x4(s0, s1);
        let p23 = u16x8_narrow_i32x4(s2, s3);
        let result = u8x16_narrow_i16x8(p01, p23);

        unsafe { v128_store(dst.as_mut_ptr().add(i) as *mut v128, result) };
        i += 16;
    }

    scale_u8_scalar(&src[i..], &mut dst[i..], max_value);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_u16_be_wasm(src: &[u8], dst: &mut [u16], max_value: u16) {
    use std::arch::wasm32::*;

    let n = dst.len();
    let scale = f32x4_splat(65535.0 / max_value as f32);

    let bswap16 = i8x16(1, 0, 3, 2, 5, 4, 7, 6, 9, 8, 11, 10, 13, 12, 15, 14);

    let mut i = 0usize;

    while i + 8 <= n {
        let raw = unsafe { v128_load(src.as_ptr().add(i * 2) as *const v128) };
        let swapped = i8x16_swizzle(raw, bswap16);

        if max_value == 65535 {
            unsafe { v128_store(dst.as_mut_ptr().add(i) as *mut v128, swapped) };
        } else {
            let lo = u32x4_extend_low_u16x8(swapped);
            let hi = u32x4_extend_high_u16x8(swapped);

            let s0 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(lo), scale)));
            let s1 = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(hi), scale)));

            let packed = u16x8_narrow_i32x4(s0, s1);
            unsafe { v128_store(dst.as_mut_ptr().add(i) as *mut v128, packed) };
        }

        i += 8;
    }

    scale_u16_be_scalar(&src[i * 2..], &mut dst[i..], max_value);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unpack_bits_wasm(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    use std::arch::wasm32::*;

    let bytes_per_row = ((width + 7) / 8) as usize;
    let width = width as usize;

    let bit_mask = i8x16(
        -128i8, 64, 32, 16, 8, 4, 2, 1,
        -128i8, 64, 32, 16, 8, 4, 2, 1,
    );
    let one = i8x16_splat(1i8);
    let zero = i8x16_splat(0i8);

    for row in 0..height as usize {
        let src_row = &src[row * bytes_per_row..];
        let dst_row = &mut dst[row * width..][..width];

        let mut x = 0usize;

        while x + 16 <= width {
            let byte_idx = x / 8;

            let b0 = i8x16_splat(src_row[byte_idx] as i8);
            let b1 = i8x16_splat(src_row[byte_idx + 1] as i8);

            let eq0 = i8x16_eq(v128_and(b0, bit_mask), zero);
            let eq1 = i8x16_eq(v128_and(b1, bit_mask), zero);

            let out0 = v128_and(eq0, one);
            let out1 = v128_and(eq1, one);

            let combined = i8x16_shuffle::<0, 1, 2, 3, 4, 5, 6, 7, 16, 17, 18, 19, 20, 21, 22, 23>(out0, out1);

            unsafe { v128_store(dst_row.as_mut_ptr().add(x) as *mut v128, combined) };
            x += 16;
        }

        while x < width {
            let byte = src_row[x / 8];
            let bit = (byte >> (7 - (x & 7))) & 1;
            dst_row[x] = !bit & 1;
            x += 1;
        }
    }
}
