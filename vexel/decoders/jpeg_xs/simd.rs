pub fn dwt_predict_interior(evens: &mut [i32], odds: &[i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { dwt_predict_interior_avx2(evens, odds) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return dwt_predict_interior_wasm(evens, odds);

    #[allow(unreachable_code)]
    dwt_predict_interior_scalar(evens, odds);
}

pub fn dwt_update_interior(odds: &mut [i32], evens: &[i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { dwt_update_interior_avx2(odds, evens) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return dwt_update_interior_wasm(odds, evens);

    #[allow(unreachable_code)]
    dwt_update_interior_scalar(odds, evens);
}

pub fn dwt_predict_lines(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { dwt_predict_lines_avx2(cur, top, bottom) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return dwt_predict_lines_wasm(cur, top, bottom);

    #[allow(unreachable_code)]
    dwt_predict_lines_scalar(cur, top, bottom);
}

pub fn dwt_update_lines(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { dwt_update_lines_avx2(cur, top, bottom) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return dwt_update_lines_wasm(cur, top, bottom);

    #[allow(unreachable_code)]
    dwt_update_lines_scalar(cur, top, bottom);
}

pub fn gather_stride_split(src: &[i32], start: usize, inc: usize, evens: &mut [i32], odds: &mut [i32]) {
    for (i, e) in evens.iter_mut().enumerate() {
        *e = src[start + 2 * i * inc];
    }
    for (i, o) in odds.iter_mut().enumerate() {
        *o = src[start + (2 * i + 1) * inc];
    }
}

pub fn scatter_stride_split(dst: &mut [i32], start: usize, inc: usize, evens: &[i32], odds: &[i32]) {
    for (i, &e) in evens.iter().enumerate() {
        dst[start + 2 * i * inc] = e;
    }
    for (i, &o) in odds.iter().enumerate() {
        dst[start + (2 * i + 1) * inc] = o;
    }
}

pub fn deinterleave_i32(src: &[i32], evens: &mut [i32], odds: &mut [i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { deinterleave_i32_avx2(src, evens, odds) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return deinterleave_i32_wasm(src, evens, odds);

    #[allow(unreachable_code)]
    deinterleave_i32_scalar(src, evens, odds);
}

pub fn interleave_i32(evens: &[i32], odds: &[i32], dst: &mut [i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { interleave_i32_avx2(evens, odds, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return interleave_i32_wasm(evens, odds, dst);

    #[allow(unreachable_code)]
    interleave_i32_scalar(evens, odds, dst);
}

pub fn nlt_linear(values: &mut [i32], dclev_and_rounding: i64, s: u32, max_val: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { nlt_linear_avx2(values, dclev_and_rounding, s, max_val) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return nlt_linear_wasm(values, dclev_and_rounding, s, max_val);

    #[allow(unreachable_code)]
    nlt_linear_scalar(values, dclev_and_rounding, s, max_val);
}

pub fn mct_rct(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { mct_rct_avx2(c0, c1, c2) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return mct_rct_wasm(c0, c1, c2);

    #[allow(unreachable_code)]
    mct_rct_scalar(c0, c1, c2);
}

pub fn deadzone_dequant(values: &mut [u32], gtli: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { deadzone_dequant_avx2(values, gtli) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return deadzone_dequant_wasm(values, gtli);

    #[allow(unreachable_code)]
    deadzone_dequant_scalar(values, gtli);
}

pub fn sign_magnitude_row(src: &[u32], dst: &mut [i32], shift: u32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { sign_magnitude_row_avx2(src, dst, shift) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return sign_magnitude_row_wasm(src, dst, shift);

    #[allow(unreachable_code)]
    sign_magnitude_row_scalar(src, dst, shift);
}

pub fn pack_clamped_u16(src: &[i32], dst: &mut [u16], max_val: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { pack_clamped_u16_avx2(src, dst, max_val) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return pack_clamped_u16_wasm(src, dst, max_val);

    #[allow(unreachable_code)]
    pack_clamped_u16_scalar(src, dst, max_val);
}

pub fn pack_clamped_u8(src: &[i32], dst: &mut [u8], max_val: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { pack_clamped_u8_avx2(src, dst, max_val) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return pack_clamped_u8_wasm(src, dst, max_val);

    #[allow(unreachable_code)]
    pack_clamped_u8_scalar(src, dst, max_val);
}

pub fn interleave_rgb_u16(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u16], max_val: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { interleave_rgb_u16_avx2(r, g, b, dst, max_val) };
    }

    #[allow(unreachable_code)]
    interleave_rgb_u16_scalar(r, g, b, dst, max_val);
}

pub fn interleave_rgb_u8(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u8], max_val: i32) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { interleave_rgb_u8_avx2(r, g, b, dst, max_val) };
    }

    #[allow(unreachable_code)]
    interleave_rgb_u8_scalar(r, g, b, dst, max_val);
}

fn interleave_rgb_u16_scalar(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u16], max_val: i32) {
    let n = (dst.len() / 3).min(r.len()).min(g.len()).min(b.len());

    for i in 0..n {
        dst[i * 3] = r[i].clamp(0, max_val) as u16;
        dst[i * 3 + 1] = g[i].clamp(0, max_val) as u16;
        dst[i * 3 + 2] = b[i].clamp(0, max_val) as u16;
    }
}

fn interleave_rgb_u8_scalar(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u8], max_val: i32) {
    let n = (dst.len() / 3).min(r.len()).min(g.len()).min(b.len());

    for i in 0..n {
        dst[i * 3] = r[i].clamp(0, max_val) as u8;
        dst[i * 3 + 1] = g[i].clamp(0, max_val) as u8;
        dst[i * 3 + 2] = b[i].clamp(0, max_val) as u8;
    }
}

fn dwt_predict_interior_scalar(evens: &mut [i32], odds: &[i32]) {
    let n = evens.len().min(odds.len().saturating_sub(1));

    for k in 0..n {
        let s = odds[k].wrapping_add(odds[k + 1]).wrapping_add(2);
        evens[k] = evens[k].wrapping_sub(s >> 2);
    }
}

fn dwt_update_interior_scalar(odds: &mut [i32], evens: &[i32]) {
    let n = odds.len().min(evens.len().saturating_sub(1));

    for k in 0..n {
        let s = evens[k].wrapping_add(evens[k + 1]);
        odds[k] = odds[k].wrapping_add(s >> 1);
    }
}

fn dwt_predict_lines_scalar(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    for i in 0..cur.len() {
        let s = top[i].wrapping_add(bottom[i]).wrapping_add(2);
        cur[i] = cur[i].wrapping_sub(s >> 2);
    }
}

fn dwt_update_lines_scalar(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    for i in 0..cur.len() {
        let s = top[i].wrapping_add(bottom[i]);
        cur[i] = cur[i].wrapping_add(s >> 1);
    }
}

fn deinterleave_i32_scalar(src: &[i32], evens: &mut [i32], odds: &mut [i32]) {
    for (i, e) in evens.iter_mut().enumerate() {
        *e = src[i * 2];
    }
    for (i, o) in odds.iter_mut().enumerate() {
        *o = src[i * 2 + 1];
    }
}

fn interleave_i32_scalar(evens: &[i32], odds: &[i32], dst: &mut [i32]) {
    for (i, e) in evens.iter().enumerate() {
        dst[i * 2] = *e;
    }
    for (i, o) in odds.iter().enumerate() {
        dst[i * 2 + 1] = *o;
    }
}

fn nlt_linear_scalar(values: &mut [i32], dclev_and_rounding: i64, s: u32, max_val: i32) {
    for value in values.iter_mut() {
        let v = (*value as i64).wrapping_add(dclev_and_rounding) >> s;
        *value = (v.clamp(i32::MIN as i64, i32::MAX as i64) as i32).clamp(0, max_val);
    }
}

fn mct_rct_scalar(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    for i in 0..c0.len() {
        let v0 = c0[i];
        let v1 = c1[i];
        let v2 = c2[i];

        let tmp = v0.wrapping_sub((v1.wrapping_add(v2)) >> 2);

        c0[i] = tmp.wrapping_add(v2);
        c2[i] = tmp.wrapping_add(v1);
        c1[i] = tmp;
    }
}

fn deadzone_dequant_scalar(values: &mut [u32], gtli: i32) {
    let bit = 1u32 << (gtli - 1).clamp(0, 31);

    for value in values.iter_mut() {
        if *value & 0x7fff_ffff != 0 {
            *value |= bit;
        }
    }
}

fn sign_magnitude_row_scalar(src: &[u32], dst: &mut [i32], shift: u32) {
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        let magnitude = (s & 0x7fff_ffff) as i32;
        let value = if s & 0x8000_0000 != 0 { -magnitude } else { magnitude };
        *d = value.wrapping_shl(shift);
    }
}

fn pack_clamped_u16_scalar(src: &[i32], dst: &mut [u16], max_val: i32) {
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        *d = s.clamp(0, max_val) as u16;
    }
}

fn pack_clamped_u8_scalar(src: &[i32], dst: &mut [u8], max_val: i32) {
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        *d = s.clamp(0, max_val) as u8;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dwt_predict_interior_avx2(evens: &mut [i32], odds: &[i32]) {
    use std::arch::x86_64::*;

    let n = evens.len().min(odds.len().saturating_sub(1));
    let two = _mm256_set1_epi32(2);
    let mut i = 0usize;

    while i + 8 <= n {
        let l = _mm256_loadu_si256(odds.as_ptr().add(i) as *const __m256i);
        let r = _mm256_loadu_si256(odds.as_ptr().add(i + 1) as *const __m256i);
        let c = _mm256_loadu_si256(evens.as_ptr().add(i) as *const __m256i);

        let s = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(l, r), two), 2);
        _mm256_storeu_si256(evens.as_mut_ptr().add(i) as *mut __m256i, _mm256_sub_epi32(c, s));

        i += 8;
    }

    for k in i..n {
        let s = odds[k].wrapping_add(odds[k + 1]).wrapping_add(2);
        evens[k] = evens[k].wrapping_sub(s >> 2);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dwt_update_interior_avx2(odds: &mut [i32], evens: &[i32]) {
    use std::arch::x86_64::*;

    let n = odds.len().min(evens.len().saturating_sub(1));
    let mut i = 0usize;

    while i + 8 <= n {
        let l = _mm256_loadu_si256(evens.as_ptr().add(i) as *const __m256i);
        let r = _mm256_loadu_si256(evens.as_ptr().add(i + 1) as *const __m256i);
        let c = _mm256_loadu_si256(odds.as_ptr().add(i) as *const __m256i);

        let s = _mm256_srai_epi32(_mm256_add_epi32(l, r), 1);
        _mm256_storeu_si256(odds.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi32(c, s));

        i += 8;
    }

    for k in i..n {
        let s = evens[k].wrapping_add(evens[k + 1]);
        odds[k] = odds[k].wrapping_add(s >> 1);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dwt_predict_lines_avx2(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    use std::arch::x86_64::*;

    let n = cur.len().min(top.len()).min(bottom.len());
    let two = _mm256_set1_epi32(2);
    let mut i = 0usize;

    while i + 8 <= n {
        let t = _mm256_loadu_si256(top.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(bottom.as_ptr().add(i) as *const __m256i);
        let c = _mm256_loadu_si256(cur.as_ptr().add(i) as *const __m256i);

        let s = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(t, b), two), 2);
        _mm256_storeu_si256(cur.as_mut_ptr().add(i) as *mut __m256i, _mm256_sub_epi32(c, s));

        i += 8;
    }

    for k in i..n {
        let s = top[k].wrapping_add(bottom[k]).wrapping_add(2);
        cur[k] = cur[k].wrapping_sub(s >> 2);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dwt_update_lines_avx2(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    use std::arch::x86_64::*;

    let n = cur.len().min(top.len()).min(bottom.len());
    let mut i = 0usize;

    while i + 8 <= n {
        let t = _mm256_loadu_si256(top.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(bottom.as_ptr().add(i) as *const __m256i);
        let c = _mm256_loadu_si256(cur.as_ptr().add(i) as *const __m256i);

        let s = _mm256_srai_epi32(_mm256_add_epi32(t, b), 1);
        _mm256_storeu_si256(cur.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi32(c, s));

        i += 8;
    }

    for k in i..n {
        let s = top[k].wrapping_add(bottom[k]);
        cur[k] = cur[k].wrapping_add(s >> 1);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn deinterleave_i32_avx2(src: &[i32], evens: &mut [i32], odds: &mut [i32]) {
    use std::arch::x86_64::*;

    let n = evens.len().min(odds.len()).min(src.len() / 2);
    let mut i = 0usize;

    while i + 8 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i * 2) as *const __m256i);
        let b = _mm256_loadu_si256(src.as_ptr().add(i * 2 + 8) as *const __m256i);

        let av = _mm256_shuffle_epi32(a, 0b11_01_10_00);
        let bv = _mm256_shuffle_epi32(b, 0b11_01_10_00);

        let lo = _mm256_unpacklo_epi64(av, bv);
        let hi = _mm256_unpackhi_epi64(av, bv);

        let e = _mm256_permute4x64_epi64(lo, 0b11_01_10_00);
        let o = _mm256_permute4x64_epi64(hi, 0b11_01_10_00);

        _mm256_storeu_si256(evens.as_mut_ptr().add(i) as *mut __m256i, e);
        _mm256_storeu_si256(odds.as_mut_ptr().add(i) as *mut __m256i, o);

        i += 8;
    }

    for k in i..evens.len() {
        evens[k] = src[k * 2];
    }
    for k in i..odds.len() {
        odds[k] = src[k * 2 + 1];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn interleave_i32_avx2(evens: &[i32], odds: &[i32], dst: &mut [i32]) {
    use std::arch::x86_64::*;

    let n = evens.len().min(odds.len()).min(dst.len() / 2);
    let mut i = 0usize;

    while i + 8 <= n {
        let e = _mm256_loadu_si256(evens.as_ptr().add(i) as *const __m256i);
        let o = _mm256_loadu_si256(odds.as_ptr().add(i) as *const __m256i);

        let lo = _mm256_unpacklo_epi32(e, o);
        let hi = _mm256_unpackhi_epi32(e, o);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i * 2) as *mut __m256i, _mm256_permute2x128_si256(lo, hi, 0x20));
        _mm256_storeu_si256(dst.as_mut_ptr().add(i * 2 + 8) as *mut __m256i, _mm256_permute2x128_si256(lo, hi, 0x31));

        i += 8;
    }

    for k in i..evens.len() {
        dst[k * 2] = evens[k];
    }
    for k in i..odds.len() {
        dst[k * 2 + 1] = odds[k];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn nlt_linear_avx2(values: &mut [i32], dclev_and_rounding: i64, s: u32, max_val: i32) {
    use std::arch::x86_64::*;

    if s >= 32 || dclev_and_rounding > i32::MAX as i64 || dclev_and_rounding < i32::MIN as i64 {
        return nlt_linear_scalar(values, dclev_and_rounding, s, max_val);
    }

    let n = values.len();
    let bias = _mm256_set1_epi64x(dclev_and_rounding);
    let zero = _mm256_setzero_si256();
    let maxv = _mm256_set1_epi64x(max_val as i64);
    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_loadu_si256(values.as_ptr().add(i) as *const __m256i);

        let lo = _mm256_cvtepi32_epi64(_mm256_castsi256_si128(v));
        let hi = _mm256_cvtepi32_epi64(_mm256_extracti128_si256(v, 1));

        let lo = clamp_epi64(_mm256_srai_epi64x(_mm256_add_epi64(lo, bias), s), zero, maxv);
        let hi = clamp_epi64(_mm256_srai_epi64x(_mm256_add_epi64(hi, bias), s), zero, maxv);

        let lo = _mm256_shuffle_epi32(lo, 0b00_00_10_00);
        let hi = _mm256_shuffle_epi32(hi, 0b00_00_10_00);

        let lo = _mm256_permute4x64_epi64(lo, 0b00_00_10_00);
        let hi = _mm256_permute4x64_epi64(hi, 0b00_00_10_00);

        let packed = _mm256_permute2x128_si256(lo, hi, 0x20);
        _mm256_storeu_si256(values.as_mut_ptr().add(i) as *mut __m256i, packed);

        i += 8;
    }

    nlt_linear_scalar(&mut values[i..], dclev_and_rounding, s, max_val);
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn clamp_epi64(
    v: std::arch::x86_64::__m256i,
    lo: std::arch::x86_64::__m256i,
    hi: std::arch::x86_64::__m256i,
) -> std::arch::x86_64::__m256i {
    use std::arch::x86_64::*;

    let below = _mm256_cmpgt_epi64(lo, v);
    let v = _mm256_blendv_epi8(v, lo, below);

    let above = _mm256_cmpgt_epi64(v, hi);
    _mm256_blendv_epi8(v, hi, above)
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn _mm256_srai_epi64x(v: std::arch::x86_64::__m256i, s: u32) -> std::arch::x86_64::__m256i {
    use std::arch::x86_64::*;

    let count = _mm_cvtsi32_si128(s as i32);
    let logical = _mm256_srl_epi64(v, count);
    let sign_bit = _mm256_set1_epi64x(1i64 << (63 - s));
    _mm256_sub_epi64(_mm256_xor_si256(logical, sign_bit), sign_bit)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn mct_rct_avx2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    use std::arch::x86_64::*;

    let n = c0.len().min(c1.len()).min(c2.len());
    let mut i = 0usize;

    while i + 8 <= n {
        let v0 = _mm256_loadu_si256(c0.as_ptr().add(i) as *const __m256i);
        let v1 = _mm256_loadu_si256(c1.as_ptr().add(i) as *const __m256i);
        let v2 = _mm256_loadu_si256(c2.as_ptr().add(i) as *const __m256i);

        let tmp = _mm256_sub_epi32(v0, _mm256_srai_epi32(_mm256_add_epi32(v1, v2), 2));

        _mm256_storeu_si256(c0.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi32(tmp, v2));
        _mm256_storeu_si256(c2.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi32(tmp, v1));
        _mm256_storeu_si256(c1.as_mut_ptr().add(i) as *mut __m256i, tmp);

        i += 8;
    }

    for k in i..n {
        let v0 = c0[k];
        let v1 = c1[k];
        let v2 = c2[k];

        let tmp = v0.wrapping_sub((v1.wrapping_add(v2)) >> 2);

        c0[k] = tmp.wrapping_add(v2);
        c2[k] = tmp.wrapping_add(v1);
        c1[k] = tmp;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn deadzone_dequant_avx2(values: &mut [u32], gtli: i32) {
    use std::arch::x86_64::*;

    let n = values.len();
    let mask = _mm256_set1_epi32(0x7fff_ffff);
    let zero = _mm256_setzero_si256();
    let bit = _mm256_set1_epi32((1u32 << (gtli - 1).clamp(0, 31)) as i32);
    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_loadu_si256(values.as_ptr().add(i) as *const __m256i);

        let nonzero = _mm256_xor_si256(_mm256_cmpeq_epi32(_mm256_and_si256(v, mask), zero), _mm256_set1_epi32(-1));
        let updated = _mm256_or_si256(v, _mm256_and_si256(bit, nonzero));

        _mm256_storeu_si256(values.as_mut_ptr().add(i) as *mut __m256i, updated);

        i += 8;
    }

    deadzone_dequant_scalar(&mut values[i..], gtli);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn deadzone_dequant_wasm(values: &mut [u32], gtli: i32) {
    use std::arch::wasm32::*;

    let n = values.len();
    let mask = i32x4_splat(0x7fff_ffff);
    let zero = i32x4_splat(0);
    let bit = i32x4_splat((1u32 << (gtli - 1).clamp(0, 31)) as i32);
    let mut i = 0usize;

    while i + 4 <= n {
        let v = v128_load(values.as_ptr().add(i) as *const v128);

        let nonzero = v128_not(i32x4_eq(v128_and(v, mask), zero));
        let updated = v128_or(v, v128_and(bit, nonzero));

        v128_store(values.as_mut_ptr().add(i) as *mut v128, updated);

        i += 4;
    }

    deadzone_dequant_scalar(&mut values[i..], gtli);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn sign_magnitude_row_avx2(src: &[u32], dst: &mut [i32], shift: u32) {
    use std::arch::x86_64::*;

    let n = src.len().min(dst.len());
    let mask = _mm256_set1_epi32(0x7fff_ffff);
    let mut i = 0usize;

    while i + 8 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);

        let magnitude = _mm256_and_si256(v, mask);
        let sign = _mm256_srai_epi32(v, 31);
        let value = _mm256_sub_epi32(_mm256_xor_si256(magnitude, sign), sign);

        _mm256_storeu_si256(
            dst.as_mut_ptr().add(i) as *mut __m256i,
            _mm256_sll_epi32(value, _mm_cvtsi32_si128(shift as i32)),
        );

        i += 8;
    }

    sign_magnitude_row_scalar(&src[i..n], &mut dst[i..n], shift);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn pack_clamped_u16_avx2(src: &[i32], dst: &mut [u16], max_val: i32) {
    use std::arch::x86_64::*;

    let n = src.len().min(dst.len());
    let zero = _mm256_setzero_si256();
    let maxv = _mm256_set1_epi32(max_val);
    let mut i = 0usize;

    while i + 16 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(src.as_ptr().add(i + 8) as *const __m256i);

        let a = _mm256_min_epi32(_mm256_max_epi32(a, zero), maxv);
        let b = _mm256_min_epi32(_mm256_max_epi32(b, zero), maxv);

        let packed = _mm256_permute4x64_epi64(_mm256_packus_epi32(a, b), 0b11_01_10_00);
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed);

        i += 16;
    }

    pack_clamped_u16_scalar(&src[i..n], &mut dst[i..n], max_val);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn pack_clamped_u8_avx2(src: &[i32], dst: &mut [u8], max_val: i32) {
    use std::arch::x86_64::*;

    let n = src.len().min(dst.len());
    let zero = _mm256_setzero_si256();
    let maxv = _mm256_set1_epi32(max_val);
    let mut i = 0usize;

    while i + 32 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(src.as_ptr().add(i + 8) as *const __m256i);
        let c = _mm256_loadu_si256(src.as_ptr().add(i + 16) as *const __m256i);
        let d = _mm256_loadu_si256(src.as_ptr().add(i + 24) as *const __m256i);

        let a = _mm256_min_epi32(_mm256_max_epi32(a, zero), maxv);
        let b = _mm256_min_epi32(_mm256_max_epi32(b, zero), maxv);
        let c = _mm256_min_epi32(_mm256_max_epi32(c, zero), maxv);
        let d = _mm256_min_epi32(_mm256_max_epi32(d, zero), maxv);

        let ab = _mm256_packus_epi32(a, b);
        let cd = _mm256_packus_epi32(c, d);

        let packed = _mm256_permute4x64_epi64(_mm256_packus_epi16(ab, cd), 0b11_01_10_00);
        let packed = _mm256_shuffle_epi32(packed, 0b11_01_10_00);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed);

        i += 32;
    }

    pack_clamped_u8_scalar(&src[i..n], &mut dst[i..n], max_val);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn interleave_rgb_u16_avx2(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u16], max_val: i32) {
    use std::arch::x86_64::*;

    let n = (dst.len() / 3).min(r.len()).min(g.len()).min(b.len());
    let zero = _mm_setzero_si128();
    let maxv = _mm_set1_epi32(max_val);
    let mut i = 0usize;

    while i + 4 <= n {
        let rv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(r.as_ptr().add(i) as *const __m128i), zero), maxv);
        let gv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(g.as_ptr().add(i) as *const __m128i), zero), maxv);
        let bv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(b.as_ptr().add(i) as *const __m128i), zero), maxv);

        let mut lanes = [0i32; 12];
        _mm_storeu_si128(lanes.as_mut_ptr() as *mut __m128i, rv);
        _mm_storeu_si128(lanes.as_mut_ptr().add(4) as *mut __m128i, gv);
        _mm_storeu_si128(lanes.as_mut_ptr().add(8) as *mut __m128i, bv);

        for k in 0..4 {
            let o = (i + k) * 3;
            *dst.get_unchecked_mut(o) = lanes[k] as u16;
            *dst.get_unchecked_mut(o + 1) = lanes[4 + k] as u16;
            *dst.get_unchecked_mut(o + 2) = lanes[8 + k] as u16;
        }

        i += 4;
    }

    interleave_rgb_u16_scalar(&r[i..n], &g[i..n], &b[i..n], &mut dst[i * 3..n * 3], max_val);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn interleave_rgb_u8_avx2(r: &[i32], g: &[i32], b: &[i32], dst: &mut [u8], max_val: i32) {
    use std::arch::x86_64::*;

    let n = (dst.len() / 3).min(r.len()).min(g.len()).min(b.len());
    let zero = _mm_setzero_si128();
    let maxv = _mm_set1_epi32(max_val);
    let mut i = 0usize;

    while i + 4 <= n {
        let rv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(r.as_ptr().add(i) as *const __m128i), zero), maxv);
        let gv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(g.as_ptr().add(i) as *const __m128i), zero), maxv);
        let bv = _mm_min_epi32(_mm_max_epi32(_mm_loadu_si128(b.as_ptr().add(i) as *const __m128i), zero), maxv);

        let mut lanes = [0i32; 12];
        _mm_storeu_si128(lanes.as_mut_ptr() as *mut __m128i, rv);
        _mm_storeu_si128(lanes.as_mut_ptr().add(4) as *mut __m128i, gv);
        _mm_storeu_si128(lanes.as_mut_ptr().add(8) as *mut __m128i, bv);

        for k in 0..4 {
            let o = (i + k) * 3;
            *dst.get_unchecked_mut(o) = lanes[k] as u8;
            *dst.get_unchecked_mut(o + 1) = lanes[4 + k] as u8;
            *dst.get_unchecked_mut(o + 2) = lanes[8 + k] as u8;
        }

        i += 4;
    }

    interleave_rgb_u8_scalar(&r[i..n], &g[i..n], &b[i..n], &mut dst[i * 3..n * 3], max_val);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn dwt_predict_interior_wasm(evens: &mut [i32], odds: &[i32]) {
    use std::arch::wasm32::*;

    let n = evens.len().min(odds.len().saturating_sub(1));
    let two = i32x4_splat(2);
    let mut i = 0usize;

    while i + 4 <= n {
        let l = v128_load(odds.as_ptr().add(i) as *const v128);
        let r = v128_load(odds.as_ptr().add(i + 1) as *const v128);
        let c = v128_load(evens.as_ptr().add(i) as *const v128);

        let s = i32x4_shr(i32x4_add(i32x4_add(l, r), two), 2);
        v128_store(evens.as_mut_ptr().add(i) as *mut v128, i32x4_sub(c, s));

        i += 4;
    }

    for k in i..n {
        let s = odds[k].wrapping_add(odds[k + 1]).wrapping_add(2);
        evens[k] = evens[k].wrapping_sub(s >> 2);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn dwt_update_interior_wasm(odds: &mut [i32], evens: &[i32]) {
    use std::arch::wasm32::*;

    let n = odds.len().min(evens.len().saturating_sub(1));
    let mut i = 0usize;

    while i + 4 <= n {
        let l = v128_load(evens.as_ptr().add(i) as *const v128);
        let r = v128_load(evens.as_ptr().add(i + 1) as *const v128);
        let c = v128_load(odds.as_ptr().add(i) as *const v128);

        let s = i32x4_shr(i32x4_add(l, r), 1);
        v128_store(odds.as_mut_ptr().add(i) as *mut v128, i32x4_add(c, s));

        i += 4;
    }

    for k in i..n {
        let s = evens[k].wrapping_add(evens[k + 1]);
        odds[k] = odds[k].wrapping_add(s >> 1);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn dwt_predict_lines_wasm(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    use std::arch::wasm32::*;

    let n = cur.len().min(top.len()).min(bottom.len());
    let two = i32x4_splat(2);
    let mut i = 0usize;

    while i + 4 <= n {
        let t = v128_load(top.as_ptr().add(i) as *const v128);
        let b = v128_load(bottom.as_ptr().add(i) as *const v128);
        let c = v128_load(cur.as_ptr().add(i) as *const v128);

        let s = i32x4_shr(i32x4_add(i32x4_add(t, b), two), 2);
        v128_store(cur.as_mut_ptr().add(i) as *mut v128, i32x4_sub(c, s));

        i += 4;
    }

    for k in i..n {
        let s = top[k].wrapping_add(bottom[k]).wrapping_add(2);
        cur[k] = cur[k].wrapping_sub(s >> 2);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn dwt_update_lines_wasm(cur: &mut [i32], top: &[i32], bottom: &[i32]) {
    use std::arch::wasm32::*;

    let n = cur.len().min(top.len()).min(bottom.len());
    let mut i = 0usize;

    while i + 4 <= n {
        let t = v128_load(top.as_ptr().add(i) as *const v128);
        let b = v128_load(bottom.as_ptr().add(i) as *const v128);
        let c = v128_load(cur.as_ptr().add(i) as *const v128);

        let s = i32x4_shr(i32x4_add(t, b), 1);
        v128_store(cur.as_mut_ptr().add(i) as *mut v128, i32x4_add(c, s));

        i += 4;
    }

    for k in i..n {
        let s = top[k].wrapping_add(bottom[k]);
        cur[k] = cur[k].wrapping_add(s >> 1);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn deinterleave_i32_wasm(src: &[i32], evens: &mut [i32], odds: &mut [i32]) {
    use std::arch::wasm32::*;

    let n = evens.len().min(odds.len()).min(src.len() / 2);
    let mut i = 0usize;

    while i + 4 <= n {
        let a = v128_load(src.as_ptr().add(i * 2) as *const v128);
        let b = v128_load(src.as_ptr().add(i * 2 + 4) as *const v128);

        v128_store(evens.as_mut_ptr().add(i) as *mut v128, i32x4_shuffle::<0, 2, 4, 6>(a, b));
        v128_store(odds.as_mut_ptr().add(i) as *mut v128, i32x4_shuffle::<1, 3, 5, 7>(a, b));

        i += 4;
    }

    for k in i..evens.len() {
        evens[k] = src[k * 2];
    }
    for k in i..odds.len() {
        odds[k] = src[k * 2 + 1];
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn interleave_i32_wasm(evens: &[i32], odds: &[i32], dst: &mut [i32]) {
    use std::arch::wasm32::*;

    let n = evens.len().min(odds.len()).min(dst.len() / 2);
    let mut i = 0usize;

    while i + 4 <= n {
        let e = v128_load(evens.as_ptr().add(i) as *const v128);
        let o = v128_load(odds.as_ptr().add(i) as *const v128);

        v128_store(dst.as_mut_ptr().add(i * 2) as *mut v128, i32x4_shuffle::<0, 4, 1, 5>(e, o));
        v128_store(dst.as_mut_ptr().add(i * 2 + 4) as *mut v128, i32x4_shuffle::<2, 6, 3, 7>(e, o));

        i += 4;
    }

    for k in i..evens.len() {
        dst[k * 2] = evens[k];
    }
    for k in i..odds.len() {
        dst[k * 2 + 1] = odds[k];
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn nlt_linear_wasm(values: &mut [i32], dclev_and_rounding: i64, s: u32, max_val: i32) {
    nlt_linear_scalar(values, dclev_and_rounding, s, max_val);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn mct_rct_wasm(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    use std::arch::wasm32::*;

    let n = c0.len().min(c1.len()).min(c2.len());
    let mut i = 0usize;

    while i + 4 <= n {
        let v0 = v128_load(c0.as_ptr().add(i) as *const v128);
        let v1 = v128_load(c1.as_ptr().add(i) as *const v128);
        let v2 = v128_load(c2.as_ptr().add(i) as *const v128);

        let tmp = i32x4_sub(v0, i32x4_shr(i32x4_add(v1, v2), 2));

        v128_store(c0.as_mut_ptr().add(i) as *mut v128, i32x4_add(tmp, v2));
        v128_store(c2.as_mut_ptr().add(i) as *mut v128, i32x4_add(tmp, v1));
        v128_store(c1.as_mut_ptr().add(i) as *mut v128, tmp);

        i += 4;
    }

    for k in i..n {
        let v0 = c0[k];
        let v1 = c1[k];
        let v2 = c2[k];

        let tmp = v0.wrapping_sub((v1.wrapping_add(v2)) >> 2);

        c0[k] = tmp.wrapping_add(v2);
        c2[k] = tmp.wrapping_add(v1);
        c1[k] = tmp;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn sign_magnitude_row_wasm(src: &[u32], dst: &mut [i32], shift: u32) {
    use std::arch::wasm32::*;

    let n = src.len().min(dst.len());
    let mask = i32x4_splat(0x7fff_ffff);
    let mut i = 0usize;

    while i + 4 <= n {
        let v = v128_load(src.as_ptr().add(i) as *const v128);

        let magnitude = v128_and(v, mask);
        let sign = i32x4_shr(v, 31);
        let value = i32x4_sub(v128_xor(magnitude, sign), sign);

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, i32x4_shl(value, shift));

        i += 4;
    }

    sign_magnitude_row_scalar(&src[i..n], &mut dst[i..n], shift);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn pack_clamped_u16_wasm(src: &[i32], dst: &mut [u16], max_val: i32) {
    use std::arch::wasm32::*;

    let n = src.len().min(dst.len());
    let zero = i32x4_splat(0);
    let maxv = i32x4_splat(max_val);
    let mut i = 0usize;

    while i + 8 <= n {
        let a = v128_load(src.as_ptr().add(i) as *const v128);
        let b = v128_load(src.as_ptr().add(i + 4) as *const v128);

        let a = i32x4_min(i32x4_max(a, zero), maxv);
        let b = i32x4_min(i32x4_max(b, zero), maxv);

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, u16x8_narrow_i32x4(a, b));

        i += 8;
    }

    pack_clamped_u16_scalar(&src[i..n], &mut dst[i..n], max_val);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn pack_clamped_u8_wasm(src: &[i32], dst: &mut [u8], max_val: i32) {
    use std::arch::wasm32::*;

    let n = src.len().min(dst.len());
    let zero = i32x4_splat(0);
    let maxv = i32x4_splat(max_val);
    let mut i = 0usize;

    while i + 16 <= n {
        let a = v128_load(src.as_ptr().add(i) as *const v128);
        let b = v128_load(src.as_ptr().add(i + 4) as *const v128);
        let c = v128_load(src.as_ptr().add(i + 8) as *const v128);
        let d = v128_load(src.as_ptr().add(i + 12) as *const v128);

        let a = i32x4_min(i32x4_max(a, zero), maxv);
        let b = i32x4_min(i32x4_max(b, zero), maxv);
        let c = i32x4_min(i32x4_max(c, zero), maxv);
        let d = i32x4_min(i32x4_max(d, zero), maxv);

        let ab = i16x8_narrow_i32x4(a, b);
        let cd = i16x8_narrow_i32x4(c, d);

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, u8x16_narrow_i16x8(ab, cd));

        i += 16;
    }

    pack_clamped_u8_scalar(&src[i..n], &mut dst[i..n], max_val);
}
