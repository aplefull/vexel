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
