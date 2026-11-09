#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
use std::arch::wasm32::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
const I32_MAX_REPRESENTABLE: f32 = 2_147_483_520.0;

pub fn round_half_to_even_row(src: &[f32], dst: &mut [i64]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { round_half_to_even_row_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unsafe { round_half_to_even_row_wasm(src, dst) };

    #[allow(unreachable_code)]
    round_half_to_even_row_scalar(src, dst);
}

pub fn dequantize_row(src: &[i64], dst: &mut [f32], step_size: f32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { dequantize_row_avx2(src, dst, step_size) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unsafe { dequantize_row_wasm(src, dst, step_size) };

    #[allow(unreachable_code)]
    dequantize_row_scalar(src, dst, step_size);
}

pub fn inverse_ict_row(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { inverse_ict_row_avx2(y, u, v) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unsafe { inverse_ict_row_wasm(y, u, v) };

    #[allow(unreachable_code)]
    inverse_ict_row_scalar(y, u, v);
}

pub fn inverse_rct_row(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { inverse_rct_row_avx2(y, u, v) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unsafe { inverse_rct_row_wasm(y, u, v) };

    #[allow(unreachable_code)]
    inverse_rct_row_scalar(y, u, v);
}

pub fn shift_and_clamp(samples: &mut [i64], shift: i64, min: i64, max: i64) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { shift_and_clamp_avx2(samples, shift, min, max) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unsafe { shift_and_clamp_wasm(samples, shift, min, max) };

    #[allow(unreachable_code)]
    shift_and_clamp_scalar(samples, shift, min, max);
}

pub fn gather_column_f32(src: &[f32], dst: &mut [f32], start: usize, stride: usize, count: usize) {
    gather_column_f32_scalar(src, dst, start, stride, count);
}

pub fn scatter_column_f32(src: &[f32], dst: &mut [f32], start: usize, stride: usize, count: usize) {
    scatter_column_f32_scalar(src, dst, start, stride, count);
}

pub fn scale_stride2(a: &mut [f32], offset: usize, count: usize, c: f32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_stride2_avx2(a, offset, count, c) };
    }

    scale_stride2_scalar(a, offset, count, c);
}

pub fn lift_stride2(a: &mut [f32], write: usize, count: usize, c: f32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { lift_stride2_avx2(a, write, count, c) };
    }

    lift_stride2_scalar(a, write, count, c);
}

pub fn replicate_row(src: &[i64], dst: &mut [i64], factor: usize) {
    #[cfg(target_arch = "x86_64")]
    if factor == 2 && is_x86_feature_detected!("avx2") {
        return unsafe { replicate_row_2x_avx2(src, dst) };
    }

    replicate_row_scalar(src, dst, factor);
}

// ─── Scalar ───────────────────────────────────────────────────

fn round_half_to_even_row_scalar(src: &[f32], dst: &mut [i64]) {
    let n = src.len().min(dst.len());
    for i in 0..n {
        let value = src[i];
        dst[i] =
            if value.is_finite() { value.round_ties_even().clamp(i32::MIN as f32, i32::MAX as f32) as i64 } else { 0 };
    }
}

fn dequantize_row_scalar(src: &[i64], dst: &mut [f32], step_size: f32) {
    let n = src.len().min(dst.len());
    for i in 0..n {
        dst[i] = (src[i] as f32 / 2.0) * step_size;
    }
}

fn inverse_ict_row_scalar(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    for i in 0..n {
        let cy = y[i];
        let cu = u[i];
        let cv = v[i];

        y[i] = cy + 1.402 * cv;
        u[i] = cy - 0.344_13 * cu - 0.714_14 * cv;
        v[i] = cy + 1.772 * cu;
    }
}

fn inverse_rct_row_scalar(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    for i in 0..n {
        let cy = y[i] as i32;
        let cu = u[i] as i32;
        let cv = v[i] as i32;

        let g = cy.wrapping_sub(cu.wrapping_add(cv) >> 2);
        let r = cv.wrapping_add(g);
        let b = cu.wrapping_add(g);

        y[i] = r as f32;
        u[i] = g as f32;
        v[i] = b as f32;
    }
}

fn shift_and_clamp_scalar(samples: &mut [i64], shift: i64, min: i64, max: i64) {
    for sample in samples.iter_mut() {
        *sample = sample.saturating_add(shift).clamp(min, max);
    }
}

fn scale_stride2_scalar(a: &mut [f32], offset: usize, count: usize, c: f32) {
    for i in 0..count {
        a[offset + i * 2] *= c;
    }
}

fn lift_stride2_scalar(a: &mut [f32], write: usize, count: usize, c: f32) {
    for i in 0..count {
        let w = write + i * 2;
        a[w - 1] += (a[w - 2] + a[w]) * c;
    }
}

fn replicate_row_scalar(src: &[i64], dst: &mut [i64], factor: usize) {
    if src.is_empty() {
        dst.fill(0);
        return;
    }

    let factor = factor.max(1);
    let last = src.len() - 1;

    for (x, slot) in dst.iter_mut().enumerate() {
        *slot = src[(x / factor).min(last)];
    }
}

fn gather_column_f32_scalar(src: &[f32], dst: &mut [f32], start: usize, stride: usize, count: usize) {
    let count = count.min(dst.len());

    if count == 0 {
        return;
    }

    if stride > 0 && start + (count - 1) * stride < src.len() {
        for (i, slot) in dst[..count].iter_mut().enumerate() {
            *slot = unsafe { *src.get_unchecked(start + i * stride) };
        }
        return;
    }

    for (i, slot) in dst[..count].iter_mut().enumerate() {
        *slot = src.get(start + i * stride).copied().unwrap_or(0.0);
    }
}

fn scatter_column_f32_scalar(src: &[f32], dst: &mut [f32], start: usize, stride: usize, count: usize) {
    let count = count.min(src.len());

    if count == 0 {
        return;
    }

    if stride > 0 && start + (count - 1) * stride < dst.len() {
        for (i, &value) in src[..count].iter().enumerate() {
            unsafe { *dst.get_unchecked_mut(start + i * stride) = value };
        }
        return;
    }

    for (i, &value) in src[..count].iter().enumerate() {
        if let Some(slot) = dst.get_mut(start + i * stride) {
            *slot = value;
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn round_half_to_even_row_avx2(src: &[f32], dst: &mut [i64]) {
    let n = src.len().min(dst.len());
    let mut i = 0;

    let saturation_point = _mm256_set1_ps(I32_MAX_REPRESENTABLE);
    let i32_max = _mm256_set1_epi32(i32::MAX);

    while i + 8 <= n {
        let value = _mm256_loadu_ps(src.as_ptr().add(i));

        let finite = _mm256_cmp_ps(_mm256_sub_ps(value, value), _mm256_setzero_ps(), _CMP_EQ_OQ);

        let rounded = _mm256_round_ps(value, _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC);
        let safe = _mm256_and_ps(rounded, finite);

        let converted = _mm256_cvttps_epi32(safe);

        let overflows = _mm256_cmp_ps(safe, saturation_point, _CMP_GT_OQ);
        let result = _mm256_blendv_epi8(converted, i32_max, _mm256_castps_si256(overflows));

        let low = _mm256_cvtepi32_epi64(_mm256_castsi256_si128(result));
        let high = _mm256_cvtepi32_epi64(_mm256_extracti128_si256(result, 1));

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, low);
        _mm256_storeu_si256(dst.as_mut_ptr().add(i + 4) as *mut __m256i, high);
        i += 8;
    }

    while i < n {
        let value = src[i];
        dst[i] =
            if value.is_finite() { value.round_ties_even().clamp(i32::MIN as f32, i32::MAX as f32) as i64 } else { 0 };
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dequantize_row_avx2(src: &[i64], dst: &mut [f32], step_size: f32) {
    let n = src.len().min(dst.len());
    let scale = _mm256_set1_ps(step_size * 0.5);
    let mut i = 0;

    while i + 8 <= n {
        let low = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let high = _mm256_loadu_si256(src.as_ptr().add(i + 4) as *const __m256i);

        let packed = _mm256_set_epi32(
            _mm256_extract_epi64(high, 3) as i32,
            _mm256_extract_epi64(high, 2) as i32,
            _mm256_extract_epi64(high, 1) as i32,
            _mm256_extract_epi64(high, 0) as i32,
            _mm256_extract_epi64(low, 3) as i32,
            _mm256_extract_epi64(low, 2) as i32,
            _mm256_extract_epi64(low, 1) as i32,
            _mm256_extract_epi64(low, 0) as i32,
        );

        _mm256_storeu_ps(dst.as_mut_ptr().add(i), _mm256_mul_ps(_mm256_cvtepi32_ps(packed), scale));
        i += 8;
    }

    while i < n {
        dst[i] = (src[i] as f32 / 2.0) * step_size;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn inverse_ict_row_avx2(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    let mut i = 0;

    let cr_r = _mm256_set1_ps(1.402);
    let cb_g = _mm256_set1_ps(-0.344_13);
    let cr_g = _mm256_set1_ps(-0.714_14);
    let cb_b = _mm256_set1_ps(1.772);

    while i + 8 <= n {
        let cy = _mm256_loadu_ps(y.as_ptr().add(i));
        let cu = _mm256_loadu_ps(u.as_ptr().add(i));
        let cv = _mm256_loadu_ps(v.as_ptr().add(i));

        let r = _mm256_add_ps(cy, _mm256_mul_ps(cr_r, cv));
        let g = _mm256_add_ps(_mm256_add_ps(cy, _mm256_mul_ps(cb_g, cu)), _mm256_mul_ps(cr_g, cv));
        let b = _mm256_add_ps(cy, _mm256_mul_ps(cb_b, cu));

        _mm256_storeu_ps(y.as_mut_ptr().add(i), r);
        _mm256_storeu_ps(u.as_mut_ptr().add(i), g);
        _mm256_storeu_ps(v.as_mut_ptr().add(i), b);
        i += 8;
    }

    while i < n {
        let cy = y[i];
        let cu = u[i];
        let cv = v[i];

        y[i] = cy + 1.402 * cv;
        u[i] = cy - 0.344_13 * cu - 0.714_14 * cv;
        v[i] = cy + 1.772 * cu;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn inverse_rct_row_avx2(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    let mut i = 0;

    while i + 8 <= n {
        let cy = _mm256_cvttps_epi32(_mm256_loadu_ps(y.as_ptr().add(i)));
        let cu = _mm256_cvttps_epi32(_mm256_loadu_ps(u.as_ptr().add(i)));
        let cv = _mm256_cvttps_epi32(_mm256_loadu_ps(v.as_ptr().add(i)));

        let g = _mm256_sub_epi32(cy, _mm256_srai_epi32(_mm256_add_epi32(cu, cv), 2));
        let r = _mm256_add_epi32(cv, g);
        let b = _mm256_add_epi32(cu, g);

        _mm256_storeu_ps(y.as_mut_ptr().add(i), _mm256_cvtepi32_ps(r));
        _mm256_storeu_ps(u.as_mut_ptr().add(i), _mm256_cvtepi32_ps(g));
        _mm256_storeu_ps(v.as_mut_ptr().add(i), _mm256_cvtepi32_ps(b));
        i += 8;
    }

    while i < n {
        let cy = y[i] as i32;
        let cu = u[i] as i32;
        let cv = v[i] as i32;

        let g = cy.wrapping_sub(cu.wrapping_add(cv) >> 2);

        y[i] = cv.wrapping_add(g) as f32;
        u[i] = g as f32;
        v[i] = cu.wrapping_add(g) as f32;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn shift_and_clamp_avx2(samples: &mut [i64], shift: i64, min: i64, max: i64) {
    let n = samples.len();
    let mut i = 0;

    let offset = _mm256_set1_epi64x(shift);
    let lower = _mm256_set1_epi64x(min);
    let upper = _mm256_set1_epi64x(max);

    while i + 4 <= n {
        let value = _mm256_loadu_si256(samples.as_ptr().add(i) as *const __m256i);
        let shifted = _mm256_add_epi64(value, offset);

        let below = _mm256_cmpgt_epi64(lower, shifted);
        let raised = _mm256_blendv_epi8(shifted, lower, below);

        let above = _mm256_cmpgt_epi64(raised, upper);
        let clamped = _mm256_blendv_epi8(raised, upper, above);

        _mm256_storeu_si256(samples.as_mut_ptr().add(i) as *mut __m256i, clamped);
        i += 4;
    }

    while i < n {
        samples[i] = samples[i].saturating_add(shift).clamp(min, max);
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn deinterleave_ps(lo: __m256, hi: __m256) -> (__m256, __m256) {
    let packed_even = _mm256_shuffle_ps(lo, hi, 0b10_00_10_00);
    let packed_odd = _mm256_shuffle_ps(lo, hi, 0b11_01_11_01);

    let even = _mm256_permute4x64_pd(_mm256_castps_pd(packed_even), 0b11_01_10_00);
    let odd = _mm256_permute4x64_pd(_mm256_castps_pd(packed_odd), 0b11_01_10_00);

    (_mm256_castpd_ps(even), _mm256_castpd_ps(odd))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_stride2_avx2(a: &mut [f32], offset: usize, count: usize, c: f32) {
    let mut i = 0;
    let factor = _mm256_set1_ps(c);

    while i + 8 <= count && offset + i * 2 + 16 <= a.len() {
        let base = a.as_ptr().add(offset + i * 2);

        let lo = _mm256_loadu_ps(base);
        let hi = _mm256_loadu_ps(base.add(8));

        let (even, odd) = deinterleave_ps(lo, hi);
        let scaled = _mm256_mul_ps(even, factor);

        let out_lo = _mm256_unpacklo_ps(scaled, odd);
        let out_hi = _mm256_unpackhi_ps(scaled, odd);

        let dst = a.as_mut_ptr().add(offset + i * 2);
        _mm256_storeu_ps(dst, _mm256_permute2f128_ps(out_lo, out_hi, 0x20));
        _mm256_storeu_ps(dst.add(8), _mm256_permute2f128_ps(out_lo, out_hi, 0x31));

        i += 8;
    }

    while i < count {
        a[offset + i * 2] *= c;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn lift_stride2_avx2(a: &mut [f32], write: usize, count: usize, c: f32) {
    let mut i = 0;
    let factor = _mm256_set1_ps(c);

    while i + 8 <= count && write + i * 2 + 16 <= a.len() {
        let read = a.as_ptr().add(write + i * 2 - 2);

        let lo = _mm256_loadu_ps(read);
        let hi = _mm256_loadu_ps(read.add(8));
        let (left, target) = deinterleave_ps(lo, hi);

        let lo_next = _mm256_loadu_ps(read.add(2));
        let hi_next = _mm256_loadu_ps(read.add(10));
        let (right, _) = deinterleave_ps(lo_next, hi_next);

        let updated = _mm256_add_ps(target, _mm256_mul_ps(_mm256_add_ps(left, right), factor));

        let out_lo = _mm256_unpacklo_ps(left, updated);
        let out_hi = _mm256_unpackhi_ps(left, updated);

        let dst = a.as_mut_ptr().add(write + i * 2 - 2);
        _mm256_storeu_ps(dst, _mm256_permute2f128_ps(out_lo, out_hi, 0x20));
        _mm256_storeu_ps(dst.add(8), _mm256_permute2f128_ps(out_lo, out_hi, 0x31));

        i += 8;
    }

    while i < count {
        let w = write + i * 2;
        a[w - 1] += (a[w - 2] + a[w]) * c;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn replicate_row_2x_avx2(src: &[i64], dst: &mut [i64]) {
    if src.is_empty() {
        dst.fill(0);
        return;
    }

    let pairs = (dst.len() / 2).min(src.len());
    let mut i = 0;

    while i + 4 <= pairs {
        let values = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);

        let lo = _mm256_unpacklo_epi64(values, values);
        let hi = _mm256_unpackhi_epi64(values, values);

        let first = _mm256_permute2x128_si256(lo, hi, 0x20);
        let second = _mm256_permute2x128_si256(lo, hi, 0x31);

        _mm256_storeu_si256(dst.as_mut_ptr().add(2 * i) as *mut __m256i, first);
        _mm256_storeu_si256(dst.as_mut_ptr().add(2 * i + 4) as *mut __m256i, second);
        i += 4;
    }

    while i < pairs {
        dst[2 * i] = src[i];
        dst[2 * i + 1] = src[i];
        i += 1;
    }

    let last = src.len() - 1;
    for (x, slot) in dst.iter_mut().enumerate().skip(2 * pairs) {
        *slot = src[(x / 2).min(last)];
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn round_half_to_even_row_wasm(src: &[f32], dst: &mut [i64]) {
    let n = src.len().min(dst.len());
    let mut i = 0;

    while i + 4 <= n {
        let value = v128_load(src.as_ptr().add(i) as *const v128);
        let finite = f32x4_eq(f32x4_sub(value, value), f32x4_splat(0.0));
        let rounded = f32x4_nearest(value);

        let safe = v128_and(rounded, finite);
        let narrow = i32x4_trunc_sat_f32x4(safe);

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, i64x2_extend_low_i32x4(narrow));
        v128_store(dst.as_mut_ptr().add(i + 2) as *mut v128, i64x2_extend_high_i32x4(narrow));
        i += 4;
    }

    while i < n {
        let value = src[i];
        dst[i] =
            if value.is_finite() { value.round_ties_even().clamp(i32::MIN as f32, i32::MAX as f32) as i64 } else { 0 };
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn dequantize_row_wasm(src: &[i64], dst: &mut [f32], step_size: f32) {
    let n = src.len().min(dst.len());
    let scale = f32x4_splat(step_size * 0.5);
    let mut i = 0;

    while i + 4 <= n {
        let packed = i32x4(src[i] as i32, src[i + 1] as i32, src[i + 2] as i32, src[i + 3] as i32);

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, f32x4_mul(f32x4_convert_i32x4(packed), scale));
        i += 4;
    }

    while i < n {
        dst[i] = (src[i] as f32 / 2.0) * step_size;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn inverse_ict_row_wasm(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    let mut i = 0;

    let cr_r = f32x4_splat(1.402);
    let cb_g = f32x4_splat(-0.344_13);
    let cr_g = f32x4_splat(-0.714_14);
    let cb_b = f32x4_splat(1.772);

    while i + 4 <= n {
        let cy = v128_load(y.as_ptr().add(i) as *const v128);
        let cu = v128_load(u.as_ptr().add(i) as *const v128);
        let cv = v128_load(v.as_ptr().add(i) as *const v128);

        let r = f32x4_add(cy, f32x4_mul(cr_r, cv));
        let g = f32x4_add(cy, f32x4_add(f32x4_mul(cb_g, cu), f32x4_mul(cr_g, cv)));
        let b = f32x4_add(cy, f32x4_mul(cb_b, cu));

        v128_store(y.as_mut_ptr().add(i) as *mut v128, r);
        v128_store(u.as_mut_ptr().add(i) as *mut v128, g);
        v128_store(v.as_mut_ptr().add(i) as *mut v128, b);
        i += 4;
    }

    while i < n {
        let cy = y[i];
        let cu = u[i];
        let cv = v[i];

        y[i] = cy + 1.402 * cv;
        u[i] = cy - 0.344_13 * cu - 0.714_14 * cv;
        v[i] = cy + 1.772 * cu;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn inverse_rct_row_wasm(y: &mut [f32], u: &mut [f32], v: &mut [f32]) {
    let n = y.len().min(u.len()).min(v.len());
    let mut i = 0;

    while i + 4 <= n {
        let cy = i32x4_trunc_sat_f32x4(v128_load(y.as_ptr().add(i) as *const v128));
        let cu = i32x4_trunc_sat_f32x4(v128_load(u.as_ptr().add(i) as *const v128));
        let cv = i32x4_trunc_sat_f32x4(v128_load(v.as_ptr().add(i) as *const v128));

        let g = i32x4_sub(cy, i32x4_shr(i32x4_add(cu, cv), 2));
        let r = i32x4_add(cv, g);
        let b = i32x4_add(cu, g);

        v128_store(y.as_mut_ptr().add(i) as *mut v128, f32x4_convert_i32x4(r));
        v128_store(u.as_mut_ptr().add(i) as *mut v128, f32x4_convert_i32x4(g));
        v128_store(v.as_mut_ptr().add(i) as *mut v128, f32x4_convert_i32x4(b));
        i += 4;
    }

    while i < n {
        let cy = y[i] as i32;
        let cu = u[i] as i32;
        let cv = v[i] as i32;

        let g = cy.wrapping_sub(cu.wrapping_add(cv) >> 2);

        y[i] = cv.wrapping_add(g) as f32;
        u[i] = g as f32;
        v[i] = cu.wrapping_add(g) as f32;
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn shift_and_clamp_wasm(samples: &mut [i64], shift: i64, min: i64, max: i64) {
    let n = samples.len();
    let mut i = 0;

    let offset = i64x2_splat(shift);
    let lower = i64x2_splat(min);
    let upper = i64x2_splat(max);

    while i + 2 <= n {
        let value = v128_load(samples.as_ptr().add(i) as *const v128);
        let shifted = i64x2_add(value, offset);

        let raised = v128_bitselect(lower, shifted, i64x2_gt(lower, shifted));
        let clamped = v128_bitselect(upper, raised, i64x2_gt(raised, upper));

        v128_store(samples.as_mut_ptr().add(i) as *mut v128, clamped);
        i += 2;
    }

    while i < n {
        samples[i] = samples[i].saturating_add(shift).clamp(min, max);
        i += 1;
    }
}
