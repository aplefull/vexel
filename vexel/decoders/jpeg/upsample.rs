pub fn widen_i16_row(src: &[i16], dst: &mut [i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { widen_i16_row_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return widen_i16_row_wasm(src, dst);

    widen_i16_row_scalar(src, dst);
}

pub fn upsample_h2v1_row(src: &[i16], dst: &mut [i32], sw: usize, tw: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { upsample_h2v1_row_avx2(src, dst, sw, tw) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return upsample_h2v1_row_wasm(src, dst, sw, tw);

    upsample_h2v1_row_scalar(src, dst, sw, tw);
}

pub fn upsample_h1v2_row(src: &[i16], neighbor: &[i16], bias: i32, dst: &mut [i32], sw: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { upsample_h1v2_row_avx2(src, neighbor, bias, dst, sw) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return upsample_h1v2_row_wasm(src, neighbor, bias, dst, sw);

    upsample_h1v2_row_scalar(src, neighbor, bias, dst, sw);
}

pub fn compute_col_sums(src: &[i16], neighbor: &[i16], col_sums: &mut [i32], sw: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { compute_col_sums_avx2(src, neighbor, col_sums, sw) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return compute_col_sums_wasm(src, neighbor, col_sums, sw);

    compute_col_sums_scalar(src, neighbor, col_sums, sw);
}

pub fn upsample_h2v2_row(col_sums: &[i32], prev_col_sums: &[i32], dst: &mut [i32], sw: usize, tw: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { upsample_h2v2_row_avx2(col_sums, prev_col_sums, dst, sw, tw) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return upsample_h2v2_row_wasm(col_sums, prev_col_sums, dst, sw, tw);

    upsample_h2v2_row_scalar(col_sums, prev_col_sums, dst, sw, tw);
}

// ─── Scalar ───────────────────────────────────────────────────

fn widen_i16_row_scalar(src: &[i16], dst: &mut [i32]) {
    let n = src.len().min(dst.len());
    for i in 0..n {
        dst[i] = src[i] as i32;
    }
}

fn upsample_h2v1_row_scalar(src: &[i16], dst: &mut [i32], sw: usize, tw: usize) {
    if sw == 0 {
        return;
    }

    if sw == 1 {
        let v = src[0] as i32;
        dst[0] = v;
        if tw > 1 {
            dst[1] = v;
        }
        return;
    }

    dst[0] = src[0] as i32;
    if tw > 1 {
        dst[1] = (src[0] as i32 * 3 + src[1] as i32 + 2) >> 2;
    }

    for sx in 1..sw - 1 {
        let cur3 = src[sx] as i32 * 3;
        let dx = sx * 2;
        if dx < tw {
            dst[dx] = (cur3 + src[sx - 1] as i32 + 1) >> 2;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (cur3 + src[sx + 1] as i32 + 2) >> 2;
        }
    }

    let last = sw - 1;
    let dx = last * 2;
    if dx < tw {
        dst[dx] = (src[last] as i32 * 3 + src[last - 1] as i32 + 1) >> 2;
    }
    if dx + 1 < tw {
        dst[dx + 1] = src[last] as i32;
    }
}

fn upsample_h1v2_row_scalar(src: &[i16], neighbor: &[i16], bias: i32, dst: &mut [i32], sw: usize) {
    for i in 0..sw {
        dst[i] = (src[i] as i32 * 3 + neighbor[i] as i32 + bias) >> 2;
    }
}

fn compute_col_sums_scalar(src: &[i16], neighbor: &[i16], col_sums: &mut [i32], sw: usize) {
    for i in 0..sw {
        col_sums[i] = src[i] as i32 * 3 + neighbor[i] as i32;
    }
}

fn upsample_h2v2_row_scalar(col_sums: &[i32], prev_col_sums: &[i32], dst: &mut [i32], sw: usize, tw: usize) {
    if sw == 0 {
        return;
    }

    let this = col_sums[0];
    dst[0] = (this * 3 + prev_col_sums[0] + 8) >> 4;
    if tw > 1 {
        dst[1] = (this * 3 + col_sums[1.min(sw - 1)] + 7) >> 4;
    }

    for sx in 1..sw.saturating_sub(1) {
        let this = col_sums[sx];
        let dx = sx * 2;
        if dx < tw {
            dst[dx] = (this * 3 + col_sums[sx - 1] + 8) >> 4;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (this * 3 + col_sums[sx + 1] + 7) >> 4;
        }
    }

    if sw > 1 {
        let this = col_sums[sw - 1];
        let dx = (sw - 1) * 2;
        if dx < tw {
            dst[dx] = (this * 3 + col_sums[sw - 2] + 8) >> 4;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (this * 4 + 7) >> 4;
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn widen_i16_row_avx2(src: &[i16], dst: &mut [i32]) {
    use std::arch::x86_64::*;

    let n = src.len().min(dst.len());
    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(i) as *const __m128i));
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, v);
        i += 8;
    }

    while i < n {
        dst[i] = src[i] as i32;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn upsample_h2v1_row_avx2(src: &[i16], dst: &mut [i32], sw: usize, tw: usize) {
    use std::arch::x86_64::*;

    if sw == 0 {
        return;
    }
    if sw == 1 {
        let v = src[0] as i32;
        dst[0] = v;
        if tw > 1 {
            dst[1] = v;
        }
        return;
    }

    dst[0] = src[0] as i32;
    if tw > 1 {
        dst[1] = (src[0] as i32 * 3 + src[1] as i32 + 2) >> 2;
    }

    let mut sx = 1usize;
    while sx + 8 < sw {
        let prev = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(sx - 1) as *const __m128i));
        let cur = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(sx) as *const __m128i));
        let next = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(sx + 1) as *const __m128i));

        let cur3 = _mm256_mullo_epi32(cur, _mm256_set1_epi32(3));
        let left = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(cur3, prev), _mm256_set1_epi32(1)), 2);
        let right = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(cur3, next), _mm256_set1_epi32(2)), 2);

        let lo_left = _mm256_extracti128_si256::<0>(left);
        let hi_left = _mm256_extracti128_si256::<1>(left);
        let lo_right = _mm256_extracti128_si256::<0>(right);
        let hi_right = _mm256_extracti128_si256::<1>(right);

        let out0 = _mm256_inserti128_si256::<1>(
            _mm256_castsi128_si256(_mm_unpacklo_epi32(lo_left, lo_right)),
            _mm_unpackhi_epi32(lo_left, lo_right),
        );
        let out1 = _mm256_inserti128_si256::<1>(
            _mm256_castsi128_si256(_mm_unpacklo_epi32(hi_left, hi_right)),
            _mm_unpackhi_epi32(hi_left, hi_right),
        );

        let dx = sx * 2;
        if dx + 16 <= tw {
            _mm256_storeu_si256(dst.as_mut_ptr().add(dx) as *mut __m256i, out0);
            _mm256_storeu_si256(dst.as_mut_ptr().add(dx + 8) as *mut __m256i, out1);
        } else {
            upsample_h2v1_scalar_range(src, dst, sx, (sx + 8).min(sw - 1), tw);
        }

        sx += 8;
    }

    upsample_h2v1_scalar_range(src, dst, sx, sw - 1, tw);

    let last = sw - 1;
    let dx = last * 2;
    if dx < tw {
        dst[dx] = (src[last] as i32 * 3 + src[last - 1] as i32 + 1) >> 2;
    }
    if dx + 1 < tw {
        dst[dx + 1] = src[last] as i32;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn upsample_h1v2_row_avx2(src: &[i16], neighbor: &[i16], bias: i32, dst: &mut [i32], sw: usize) {
    use std::arch::x86_64::*;

    let vbias = _mm256_set1_epi32(bias);
    let v3 = _mm256_set1_epi32(3);
    let mut i = 0usize;

    while i + 8 <= sw {
        let vsrc = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(i) as *const __m128i));
        let vneig = _mm256_cvtepi16_epi32(_mm_loadu_si128(neighbor.as_ptr().add(i) as *const __m128i));
        let sum = _mm256_add_epi32(_mm256_add_epi32(_mm256_mullo_epi32(vsrc, v3), vneig), vbias);
        let out = _mm256_srai_epi32(sum, 2);
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, out);
        i += 8;
    }

    while i < sw {
        dst[i] = (src[i] as i32 * 3 + neighbor[i] as i32 + bias) >> 2;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_col_sums_avx2(src: &[i16], neighbor: &[i16], col_sums: &mut [i32], sw: usize) {
    use std::arch::x86_64::*;

    let v3 = _mm256_set1_epi32(3);
    let mut i = 0usize;

    while i + 8 <= sw {
        let vsrc = _mm256_cvtepi16_epi32(_mm_loadu_si128(src.as_ptr().add(i) as *const __m128i));
        let vneig = _mm256_cvtepi16_epi32(_mm_loadu_si128(neighbor.as_ptr().add(i) as *const __m128i));
        let out = _mm256_add_epi32(_mm256_mullo_epi32(vsrc, v3), vneig);
        _mm256_storeu_si256(col_sums.as_mut_ptr().add(i) as *mut __m256i, out);
        i += 8;
    }

    while i < sw {
        col_sums[i] = src[i] as i32 * 3 + neighbor[i] as i32;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn upsample_h2v2_row_avx2(col_sums: &[i32], prev_col_sums: &[i32], dst: &mut [i32], sw: usize, tw: usize) {
    use std::arch::x86_64::*;

    if sw == 0 {
        return;
    }

    let this = col_sums[0];
    dst[0] = (this * 3 + prev_col_sums[0] + 8) >> 4;
    if tw > 1 {
        dst[1] = (this * 3 + col_sums[1.min(sw - 1)] + 7) >> 4;
    }

    let v3 = _mm256_set1_epi32(3);
    let v8 = _mm256_set1_epi32(8);
    let v7 = _mm256_set1_epi32(7);

    let mut sx = 1usize;
    while sx + 8 < sw {
        let this = _mm256_loadu_si256(col_sums.as_ptr().add(sx) as *const __m256i);
        let left = _mm256_loadu_si256(col_sums.as_ptr().add(sx - 1) as *const __m256i);
        let right = _mm256_loadu_si256(col_sums.as_ptr().add(sx + 1) as *const __m256i);

        let this3 = _mm256_mullo_epi32(this, v3);
        let out_left = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(this3, left), v8), 4);
        let out_right = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(this3, right), v7), 4);

        let lo_left = _mm256_extracti128_si256::<0>(out_left);
        let hi_left = _mm256_extracti128_si256::<1>(out_left);
        let lo_right = _mm256_extracti128_si256::<0>(out_right);
        let hi_right = _mm256_extracti128_si256::<1>(out_right);

        let out0 = _mm256_inserti128_si256::<1>(
            _mm256_castsi128_si256(_mm_unpacklo_epi32(lo_left, lo_right)),
            _mm_unpackhi_epi32(lo_left, lo_right),
        );
        let out1 = _mm256_inserti128_si256::<1>(
            _mm256_castsi128_si256(_mm_unpacklo_epi32(hi_left, hi_right)),
            _mm_unpackhi_epi32(hi_left, hi_right),
        );

        let dx = sx * 2;
        if dx + 16 <= tw {
            _mm256_storeu_si256(dst.as_mut_ptr().add(dx) as *mut __m256i, out0);
            _mm256_storeu_si256(dst.as_mut_ptr().add(dx + 8) as *mut __m256i, out1);
        } else {
            upsample_h2v2_scalar_range(col_sums, dst, sx, (sx + 8).min(sw - 1), tw);
        }

        sx += 8;
    }

    upsample_h2v2_scalar_range(col_sums, dst, sx, sw - 1, tw);

    if sw > 1 {
        let this = col_sums[sw - 1];
        let dx = (sw - 1) * 2;
        if dx < tw {
            dst[dx] = (this * 3 + col_sums[sw - 2] + 8) >> 4;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (this * 4 + 7) >> 4;
        }
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn widen_i16_row_wasm(src: &[i16], dst: &mut [i32]) {
    use std::arch::wasm32::*;

    let n = src.len().min(dst.len());
    let mut i = 0usize;

    while i + 4 <= n {
        unsafe {
            let v = i32x4_load_extend_i16x4(src.as_ptr().add(i));
            v128_store(dst.as_mut_ptr().add(i) as *mut v128, v);
        }
        i += 4;
    }

    while i < n {
        dst[i] = src[i] as i32;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn upsample_h2v1_row_wasm(src: &[i16], dst: &mut [i32], sw: usize, tw: usize) {
    use std::arch::wasm32::*;

    if sw == 0 {
        return;
    }
    if sw == 1 {
        let v = src[0] as i32;
        dst[0] = v;
        if tw > 1 {
            dst[1] = v;
        }
        return;
    }

    dst[0] = src[0] as i32;
    if tw > 1 {
        dst[1] = (src[0] as i32 * 3 + src[1] as i32 + 2) >> 2;
    }

    let mut sx = 1usize;
    while sx + 4 < sw {
        unsafe {
            let prev = i32x4_load_extend_i16x4(src.as_ptr().add(sx - 1));
            let cur = i32x4_load_extend_i16x4(src.as_ptr().add(sx));
            let next = i32x4_load_extend_i16x4(src.as_ptr().add(sx + 1));

            let cur3 = i32x4_mul(cur, i32x4_splat(3));
            let left = i32x4_shr(i32x4_add(i32x4_add(cur3, prev), i32x4_splat(1)), 2);
            let right = i32x4_shr(i32x4_add(i32x4_add(cur3, next), i32x4_splat(2)), 2);

            let out0 = i32x4(
                i32x4_extract_lane::<0>(left),
                i32x4_extract_lane::<0>(right),
                i32x4_extract_lane::<1>(left),
                i32x4_extract_lane::<1>(right),
            );
            let out1 = i32x4(
                i32x4_extract_lane::<2>(left),
                i32x4_extract_lane::<2>(right),
                i32x4_extract_lane::<3>(left),
                i32x4_extract_lane::<3>(right),
            );

            let dx = sx * 2;
            if dx + 8 <= tw {
                v128_store(dst.as_mut_ptr().add(dx) as *mut v128, out0);
                v128_store(dst.as_mut_ptr().add(dx + 4) as *mut v128, out1);
            } else {
                upsample_h2v1_scalar_range(src, dst, sx, (sx + 4).min(sw - 1), tw);
            }
        }
        sx += 4;
    }

    upsample_h2v1_scalar_range(src, dst, sx, sw - 1, tw);

    let last = sw - 1;
    let dx = last * 2;
    if dx < tw {
        dst[dx] = (src[last] as i32 * 3 + src[last - 1] as i32 + 1) >> 2;
    }
    if dx + 1 < tw {
        dst[dx + 1] = src[last] as i32;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn upsample_h1v2_row_wasm(src: &[i16], neighbor: &[i16], bias: i32, dst: &mut [i32], sw: usize) {
    use std::arch::wasm32::*;

    let vbias = i32x4_splat(bias);
    let v3 = i32x4_splat(3);
    let mut i = 0usize;

    while i + 4 <= sw {
        unsafe {
            let vsrc = i32x4_load_extend_i16x4(src.as_ptr().add(i));
            let vneig = i32x4_load_extend_i16x4(neighbor.as_ptr().add(i));
            let sum = i32x4_add(i32x4_add(i32x4_mul(vsrc, v3), vneig), vbias);
            let out = i32x4_shr(sum, 2);
            v128_store(dst.as_mut_ptr().add(i) as *mut v128, out);
        }
        i += 4;
    }

    while i < sw {
        dst[i] = (src[i] as i32 * 3 + neighbor[i] as i32 + bias) >> 2;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn compute_col_sums_wasm(src: &[i16], neighbor: &[i16], col_sums: &mut [i32], sw: usize) {
    use std::arch::wasm32::*;

    let v3 = i32x4_splat(3);
    let mut i = 0usize;

    while i + 4 <= sw {
        unsafe {
            let vsrc = i32x4_load_extend_i16x4(src.as_ptr().add(i));
            let vneig = i32x4_load_extend_i16x4(neighbor.as_ptr().add(i));
            let out = i32x4_add(i32x4_mul(vsrc, v3), vneig);
            v128_store(col_sums.as_mut_ptr().add(i) as *mut v128, out);
        }
        i += 4;
    }

    while i < sw {
        col_sums[i] = src[i] as i32 * 3 + neighbor[i] as i32;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn upsample_h2v2_row_wasm(col_sums: &[i32], prev_col_sums: &[i32], dst: &mut [i32], sw: usize, tw: usize) {
    use std::arch::wasm32::*;

    if sw == 0 {
        return;
    }

    let this = col_sums[0];
    dst[0] = (this * 3 + prev_col_sums[0] + 8) >> 4;
    if tw > 1 {
        dst[1] = (this * 3 + col_sums[1.min(sw - 1)] + 7) >> 4;
    }

    let v3 = i32x4_splat(3);
    let v8 = i32x4_splat(8);
    let v7 = i32x4_splat(7);

    let mut sx = 1usize;
    while sx + 4 < sw {
        unsafe {
            let this = v128_load(col_sums.as_ptr().add(sx) as *const v128);
            let left = v128_load(col_sums.as_ptr().add(sx - 1) as *const v128);
            let right = v128_load(col_sums.as_ptr().add(sx + 1) as *const v128);

            let this3 = i32x4_mul(this, v3);
            let out_left = i32x4_shr(i32x4_add(i32x4_add(this3, left), v8), 4);
            let out_right = i32x4_shr(i32x4_add(i32x4_add(this3, right), v7), 4);

            let out0 = i32x4(
                i32x4_extract_lane::<0>(out_left),
                i32x4_extract_lane::<0>(out_right),
                i32x4_extract_lane::<1>(out_left),
                i32x4_extract_lane::<1>(out_right),
            );
            let out1 = i32x4(
                i32x4_extract_lane::<2>(out_left),
                i32x4_extract_lane::<2>(out_right),
                i32x4_extract_lane::<3>(out_left),
                i32x4_extract_lane::<3>(out_right),
            );

            let dx = sx * 2;
            if dx + 8 <= tw {
                v128_store(dst.as_mut_ptr().add(dx) as *mut v128, out0);
                v128_store(dst.as_mut_ptr().add(dx + 4) as *mut v128, out1);
            } else {
                upsample_h2v2_scalar_range(col_sums, dst, sx, (sx + 4).min(sw - 1), tw);
            }
        }
        sx += 4;
    }

    upsample_h2v2_scalar_range(col_sums, dst, sx, sw - 1, tw);

    if sw > 1 {
        let this = col_sums[sw - 1];
        let dx = (sw - 1) * 2;
        if dx < tw {
            dst[dx] = (this * 3 + col_sums[sw - 2] + 8) >> 4;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (this * 4 + 7) >> 4;
        }
    }
}

// ─── Shared ───────────────────────────────────────────────────────────

fn upsample_h2v1_scalar_range(src: &[i16], dst: &mut [i32], from: usize, to: usize, tw: usize) {
    for sx in from..to {
        let cur3 = src[sx] as i32 * 3;
        let dx = sx * 2;
        if dx < tw {
            dst[dx] = (cur3 + src[sx - 1] as i32 + 1) >> 2;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (cur3 + src[sx + 1] as i32 + 2) >> 2;
        }
    }
}

fn upsample_h2v2_scalar_range(col_sums: &[i32], dst: &mut [i32], from: usize, to: usize, tw: usize) {
    for sx in from..to {
        let this = col_sums[sx];
        let dx = sx * 2;
        if dx < tw {
            dst[dx] = (this * 3 + col_sums[sx - 1] + 8) >> 4;
        }
        if dx + 1 < tw {
            dst[dx + 1] = (this * 3 + col_sums[sx + 1] + 7) >> 4;
        }
    }
}
