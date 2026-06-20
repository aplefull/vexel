pub fn scale_u16_to_u8(src: &[u16], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u16_to_u8_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_u16_to_u8_wasm(src, dst);

    scale_u16_to_u8_scalar(src, dst);
}

pub fn scale_f32_to_u8(src: &[f32], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_f32_to_u8_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return scale_f32_to_u8_wasm(src, dst);

    scale_f32_to_u8_scalar(src, dst);
}

pub fn rgb_to_rgba(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len() % 3, 0);
    debug_assert_eq!(dst.len(), src.len() / 3 * 4);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { rgb_to_rgba_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return rgb_to_rgba_wasm(src, dst);

    rgb_to_rgba_scalar(src, dst);
}

pub fn rgba_to_rgb(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len() % 4, 0);
    debug_assert_eq!(dst.len(), src.len() / 4 * 3);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { rgba_to_rgb_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return rgba_to_rgb_wasm(src, dst);

    rgba_to_rgb_scalar(src, dst);
}

// ─── Scalar ───────────────────────────────────────────────────

fn rgb_to_rgba_scalar(src: &[u8], dst: &mut [u8]) {
    let mut out = 0;
    for chunk in src.chunks_exact(3) {
        dst[out] = chunk[0];
        dst[out + 1] = chunk[1];
        dst[out + 2] = chunk[2];
        dst[out + 3] = 255;
        out += 4;
    }
}

fn rgba_to_rgb_scalar(src: &[u8], dst: &mut [u8]) {
    let mut out = 0;
    for chunk in src.chunks_exact(4) {
        dst[out] = chunk[0];
        dst[out + 1] = chunk[1];
        dst[out + 2] = chunk[2];
        out += 3;
    }
}

fn scale_u16_to_u8_scalar(src: &[u16], dst: &mut [u8]) {
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (*s as f32 * 255.0 / 65535.0).round() as u8;
    }
}

fn scale_f32_to_u8_scalar(src: &[f32], dst: &mut [u8]) {
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (s.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn rgb_to_rgba_avx2(src: &[u8], dst: &mut [u8]) {
    use std::arch::x86_64::*;

    let n = src.len() / 3;
    let mut i = 0;
    let mut si = 0usize;
    let mut di = 0usize;

    let shuf = _mm_set_epi8(
        0x80u8 as i8, 11, 10, 9,
        0x80u8 as i8, 8, 7, 6,
        0x80u8 as i8, 5, 4, 3,
        0x80u8 as i8, 2, 1, 0,
    );
    let alpha = _mm_set1_epi32(0xFF000000u32 as i32);

    while si + 52 <= src.len() {
        let in0 = _mm_loadu_si128(src.as_ptr().add(si) as *const __m128i);
        let in1 = _mm_loadu_si128(src.as_ptr().add(si + 12) as *const __m128i);
        let in2 = _mm_loadu_si128(src.as_ptr().add(si + 24) as *const __m128i);
        let in3 = _mm_loadu_si128(src.as_ptr().add(si + 36) as *const __m128i);

        let out0 = _mm_or_si128(_mm_shuffle_epi8(in0, shuf), alpha);
        let out1 = _mm_or_si128(_mm_shuffle_epi8(in1, shuf), alpha);
        let out2 = _mm_or_si128(_mm_shuffle_epi8(in2, shuf), alpha);
        let out3 = _mm_or_si128(_mm_shuffle_epi8(in3, shuf), alpha);

        _mm_storeu_si128(dst.as_mut_ptr().add(di) as *mut __m128i, out0);
        _mm_storeu_si128(dst.as_mut_ptr().add(di + 16) as *mut __m128i, out1);
        _mm_storeu_si128(dst.as_mut_ptr().add(di + 32) as *mut __m128i, out2);
        _mm_storeu_si128(dst.as_mut_ptr().add(di + 48) as *mut __m128i, out3);

        si += 48;
        di += 64;
        i += 16;
    }

    while i < n {
        dst[di] = src[si];
        dst[di + 1] = src[si + 1];
        dst[di + 2] = src[si + 2];
        dst[di + 3] = 255;
        si += 3;
        di += 4;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn rgba_to_rgb_avx2(src: &[u8], dst: &mut [u8]) {
    use std::arch::x86_64::*;

    let n = src.len() / 4;
    let mut i = 0;
    let mut si = 0usize;
    let mut di = 0usize;

    let shuf = _mm_set_epi8(
        0x80u8 as i8, 0x80u8 as i8, 0x80u8 as i8, 0x80u8 as i8,
        14, 13, 12,
        10, 9, 8,
        6, 5, 4,
        2, 1, 0,
    );

    while i + 16 <= n {
        let in0 = _mm_loadu_si128(src.as_ptr().add(si) as *const __m128i);
        let in1 = _mm_loadu_si128(src.as_ptr().add(si + 16) as *const __m128i);
        let in2 = _mm_loadu_si128(src.as_ptr().add(si + 32) as *const __m128i);
        let in3 = _mm_loadu_si128(src.as_ptr().add(si + 48) as *const __m128i);

        let out0 = _mm_shuffle_epi8(in0, shuf);
        let out1 = _mm_shuffle_epi8(in1, shuf);
        let out2 = _mm_shuffle_epi8(in2, shuf);
        let out3 = _mm_shuffle_epi8(in3, shuf);

        let p0 = dst.as_mut_ptr().add(di);
        let p1 = dst.as_mut_ptr().add(di + 12);
        let p2 = dst.as_mut_ptr().add(di + 24);
        let p3 = dst.as_mut_ptr().add(di + 36);

        _mm_storel_epi64(p0 as *mut __m128i, out0);
        (p0.add(8) as *mut u32).write_unaligned(_mm_extract_epi32(out0, 2) as u32);

        _mm_storel_epi64(p1 as *mut __m128i, out1);
        (p1.add(8) as *mut u32).write_unaligned(_mm_extract_epi32(out1, 2) as u32);

        _mm_storel_epi64(p2 as *mut __m128i, out2);
        (p2.add(8) as *mut u32).write_unaligned(_mm_extract_epi32(out2, 2) as u32);

        _mm_storel_epi64(p3 as *mut __m128i, out3);
        (p3.add(8) as *mut u32).write_unaligned(_mm_extract_epi32(out3, 2) as u32);

        si += 64;
        di += 48;
        i += 16;
    }

    while i < n {
        dst[di] = src[si];
        dst[di + 1] = src[si + 1];
        dst[di + 2] = src[si + 2];
        si += 4;
        di += 3;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u16_to_u8_avx2(src: &[u16], dst: &mut [u8]) {
    use std::arch::x86_64::*;

    let n = src.len();
    let scale = _mm256_set1_ps(255.0 / 65535.0);
    let zero = _mm256_setzero_si256();

    let mut i = 0;
    while i + 16 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);

        let lo = _mm256_cvtepu16_epi32(_mm256_castsi256_si128(v));
        let hi = _mm256_cvtepu16_epi32(_mm256_extracti128_si256(v, 1));

        let lo = _mm256_cvtps_epi32(_mm256_mul_ps(_mm256_cvtepi32_ps(lo), scale));
        let hi = _mm256_cvtps_epi32(_mm256_mul_ps(_mm256_cvtepi32_ps(hi), scale));

        let packed16 = _mm256_permute4x64_epi64(_mm256_packus_epi32(lo, hi), 0xD8);
        let packed8 = _mm256_permute4x64_epi64(_mm256_packus_epi16(packed16, zero), 0xD8);

        _mm_storeu_si128(
            dst.as_mut_ptr().add(i) as *mut __m128i,
            _mm256_castsi256_si128(packed8),
        );

        i += 16;
    }

    scale_u16_to_u8_scalar(&src[i..], &mut dst[i..]);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_f32_to_u8_avx2(src: &[f32], dst: &mut [u8]) {
    use std::arch::x86_64::*;

    let n = src.len();
    let zero = _mm256_setzero_ps();
    let one = _mm256_set1_ps(1.0_f32);
    let scale = _mm256_set1_ps(255.0_f32);

    let mut i = 0;
    while i + 32 <= n {
        let v0 = _mm256_loadu_ps(src.as_ptr().add(i));
        let v1 = _mm256_loadu_ps(src.as_ptr().add(i + 8));
        let v2 = _mm256_loadu_ps(src.as_ptr().add(i + 16));
        let v3 = _mm256_loadu_ps(src.as_ptr().add(i + 24));

        let clamp = |v: __m256| _mm256_min_ps(_mm256_max_ps(v, zero), one);
        let convert = |v: __m256| _mm256_cvtps_epi32(_mm256_mul_ps(clamp(v), scale));

        let g0 = convert(v0);
        let g1 = convert(v1);
        let g2 = convert(v2);
        let g3 = convert(v3);

        let p01 = _mm256_permute4x64_epi64(_mm256_packus_epi32(g0, g1), 0xD8);
        let p23 = _mm256_permute4x64_epi64(_mm256_packus_epi32(g2, g3), 0xD8);
        let result = _mm256_permute4x64_epi64(_mm256_packus_epi16(p01, p23), 0xD8);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, result);

        i += 32;
    }

    while i + 8 <= n {
        let v = _mm256_loadu_ps(src.as_ptr().add(i));
        let g = _mm256_cvtps_epi32(_mm256_mul_ps(_mm256_min_ps(_mm256_max_ps(v, zero), one), scale));
        let p16 = _mm256_permute4x64_epi64(_mm256_packus_epi32(g, g), 0xD8);
        let p8 = _mm256_permute4x64_epi64(
            _mm256_packus_epi16(p16, _mm256_setzero_si256()),
            0xD8,
        );
        _mm_storel_epi64(
            dst.as_mut_ptr().add(i) as *mut __m128i,
            _mm256_castsi256_si128(p8),
        );
        i += 8;
    }

    scale_f32_to_u8_scalar(&src[i..], &mut dst[i..]);
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn rgb_to_rgba_wasm(src: &[u8], dst: &mut [u8]) {
    use std::arch::wasm32::*;

    let n = src.len() / 3;
    let mut i = 0;
    let mut si = 0usize;
    let mut di = 0usize;

    let shuf = i8x16(0, 1, 2, -1, 3, 4, 5, -1, 6, 7, 8, -1, 9, 10, 11, -1);
    let alpha = i32x4_splat(0xFF000000u32 as i32);

    while si + 52 <= src.len() {
        let in0 = unsafe { v128_load(src.as_ptr().add(si) as *const v128) };
        let in1 = unsafe { v128_load(src.as_ptr().add(si + 12) as *const v128) };
        let in2 = unsafe { v128_load(src.as_ptr().add(si + 24) as *const v128) };
        let in3 = unsafe { v128_load(src.as_ptr().add(si + 36) as *const v128) };

        let out0 = v128_or(i8x16_swizzle(in0, shuf), alpha);
        let out1 = v128_or(i8x16_swizzle(in1, shuf), alpha);
        let out2 = v128_or(i8x16_swizzle(in2, shuf), alpha);
        let out3 = v128_or(i8x16_swizzle(in3, shuf), alpha);

        unsafe {
            v128_store(dst.as_mut_ptr().add(di) as *mut v128, out0);
            v128_store(dst.as_mut_ptr().add(di + 16) as *mut v128, out1);
            v128_store(dst.as_mut_ptr().add(di + 32) as *mut v128, out2);
            v128_store(dst.as_mut_ptr().add(di + 48) as *mut v128, out3);
        }

        si += 48;
        di += 64;
        i += 16;
    }

    while i < n {
        dst[di] = src[si];
        dst[di + 1] = src[si + 1];
        dst[di + 2] = src[si + 2];
        dst[di + 3] = 255;
        si += 3;
        di += 4;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn rgba_to_rgb_wasm(src: &[u8], dst: &mut [u8]) {
    use std::arch::wasm32::*;

    let n = src.len() / 4;
    let mut i = 0;
    let mut si = 0usize;
    let mut di = 0usize;

    let shuf = i8x16(0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14, -1, -1, -1, -1);

    while i + 16 <= n {
        let in0 = unsafe { v128_load(src.as_ptr().add(si) as *const v128) };
        let in1 = unsafe { v128_load(src.as_ptr().add(si + 16) as *const v128) };
        let in2 = unsafe { v128_load(src.as_ptr().add(si + 32) as *const v128) };
        let in3 = unsafe { v128_load(src.as_ptr().add(si + 48) as *const v128) };

        let out0 = i8x16_swizzle(in0, shuf);
        let out1 = i8x16_swizzle(in1, shuf);
        let out2 = i8x16_swizzle(in2, shuf);
        let out3 = i8x16_swizzle(in3, shuf);

        unsafe {
            let p0 = dst.as_mut_ptr().add(di);
            let p1 = dst.as_mut_ptr().add(di + 12);
            let p2 = dst.as_mut_ptr().add(di + 24);
            let p3 = dst.as_mut_ptr().add(di + 36);

            v128_store64_lane::<0>(p0 as *mut v128, out0);
            (p0.add(8) as *mut u32).write_unaligned(u32x4_extract_lane::<2>(out0));

            v128_store64_lane::<0>(p1 as *mut v128, out1);
            (p1.add(8) as *mut u32).write_unaligned(u32x4_extract_lane::<2>(out1));

            v128_store64_lane::<0>(p2 as *mut v128, out2);
            (p2.add(8) as *mut u32).write_unaligned(u32x4_extract_lane::<2>(out2));

            v128_store64_lane::<0>(p3 as *mut v128, out3);
            (p3.add(8) as *mut u32).write_unaligned(u32x4_extract_lane::<2>(out3));
        }

        si += 64;
        di += 48;
        i += 16;
    }

    while i < n {
        dst[di] = src[si];
        dst[di + 1] = src[si + 1];
        dst[di + 2] = src[si + 2];
        si += 4;
        di += 3;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_u16_to_u8_wasm(src: &[u16], dst: &mut [u8]) {
    use std::arch::wasm32::*;

    let n = src.len();
    let scale = f32x4_splat(255.0 / 65535.0);

    let mut i = 0;
    while i + 8 <= n {
        let v = unsafe { v128_load(src.as_ptr().add(i) as *const v128) };

        let lo_u32 = u32x4_extend_low_u16x8(v);
        let hi_u32 = u32x4_extend_high_u16x8(v);

        let lo = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(lo_u32), scale)));
        let hi = u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(f32x4_convert_u32x4(hi_u32), scale)));

        let packed16 = u16x8_narrow_i32x4(lo, hi);
        let packed8 = u8x16_narrow_i16x8(packed16, packed16);

        unsafe {
            (dst.as_mut_ptr().add(i) as *mut u64).write_unaligned(u64x2_extract_lane::<0>(packed8));
        }

        i += 8;
    }

    scale_u16_to_u8_scalar(&src[i..], &mut dst[i..]);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn scale_f32_to_u8_wasm(src: &[f32], dst: &mut [u8]) {
    use std::arch::wasm32::*;

    let n = src.len();
    let zero = f32x4_splat(0.0_f32);
    let one = f32x4_splat(1.0_f32);
    let scale = f32x4_splat(255.0_f32);

    let mut i = 0;
    while i + 16 <= n {
        let v0 = unsafe { v128_load(src.as_ptr().add(i) as *const v128) };
        let v1 = unsafe { v128_load(src.as_ptr().add(i + 4) as *const v128) };
        let v2 = unsafe { v128_load(src.as_ptr().add(i + 8) as *const v128) };
        let v3 = unsafe { v128_load(src.as_ptr().add(i + 12) as *const v128) };

        let clamp = |v: v128| f32x4_pmin(f32x4_pmax(v, zero), one);
        let convert = |v: v128| u32x4_trunc_sat_f32x4(f32x4_nearest(f32x4_mul(clamp(v), scale)));

        let g0 = convert(v0);
        let g1 = convert(v1);
        let g2 = convert(v2);
        let g3 = convert(v3);

        let p01 = u16x8_narrow_i32x4(g0, g1);
        let p23 = u16x8_narrow_i32x4(g2, g3);
        let result = u8x16_narrow_i16x8(p01, p23);

        unsafe {
            v128_store(dst.as_mut_ptr().add(i) as *mut v128, result);
        }

        i += 16;
    }

    scale_f32_to_u8_scalar(&src[i..], &mut dst[i..]);
}
