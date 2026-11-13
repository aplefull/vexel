use crate::decoders::jpeg::idct::SimdLevel;

pub trait Pixel: Copy + Default {
    fn value(self) -> u32;
    fn from_value(value: u32) -> Self;
}

impl Pixel for u8 {
    #[inline(always)]
    fn value(self) -> u32 {
        self as u32
    }

    #[inline(always)]
    fn from_value(value: u32) -> Self {
        value as u8
    }
}

impl Pixel for u16 {
    #[inline(always)]
    fn value(self) -> u32 {
        self as u32
    }

    #[inline(always)]
    fn from_value(value: u32) -> Self {
        value as u16
    }
}

pub fn fancy_h2v1_u8(level: SimdLevel, src: &[u8], dst: &mut [u8]) {
    let start = match level {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => unsafe { fancy_h2v1_u8_avx2(src, dst) },
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        SimdLevel::Simd128 => fancy_h2v1_u8_simd128(src, dst),
        SimdLevel::Scalar => 0,
    };
    fancy_h2v1(src, dst, start);
}

pub fn fancy_h2v2_u8(level: SimdLevel, near: &[u8], far: &[u8], dst: &mut [u8]) {
    let n = near.len().min(far.len());
    let (near, far) = (&near[..n], &far[..n]);
    let start = match level {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => unsafe { fancy_h2v2_u8_avx2(near, far, dst) },
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        SimdLevel::Simd128 => fancy_h2v2_u8_simd128(near, far, dst),
        SimdLevel::Scalar => 0,
    };
    fancy_h2v2(near, far, dst, start);
}

// ─── Scalar ───────────────────────────────────────────────────

pub fn fancy_h2v1<T: Pixel>(src: &[T], dst: &mut [T], start: usize) {
    let n = src.len();
    if n == 0 {
        return;
    }
    let pairs = n.min(dst.len().div_ceil(2));
    for i in (0..pairs).filter(|&i| i == 0 || i >= start) {
        let c = src[i].value() * 3;
        let left = src[i.saturating_sub(1)].value();
        let right = src[(i + 1).min(n - 1)].value();
        dst[2 * i] = T::from_value((c + left + 1) >> 2);
        if let Some(odd) = dst.get_mut(2 * i + 1) {
            *odd = T::from_value((c + right + 2) >> 2);
        }
    }
}

pub fn fancy_h1v2<T: Pixel>(near: &[T], far: &[T], bias: u32, dst: &mut [T]) {
    for ((d, &a), &b) in dst.iter_mut().zip(near).zip(far) {
        *d = T::from_value((a.value() * 3 + b.value() + bias) >> 2);
    }
}

pub fn fancy_h2v2<T: Pixel>(near: &[T], far: &[T], dst: &mut [T], start: usize) {
    let n = near.len().min(far.len());
    if n == 0 {
        return;
    }
    let pairs = n.min(dst.len().div_ceil(2));
    let colsum = |i: usize| near[i].value() * 3 + far[i].value();
    for i in (0..pairs).filter(|&i| i == 0 || i >= start) {
        let c = colsum(i) * 3;
        let left = colsum(i.saturating_sub(1));
        let right = colsum((i + 1).min(n - 1));
        dst[2 * i] = T::from_value((c + left + 8) >> 4);
        if let Some(odd) = dst.get_mut(2 * i + 1) {
            *odd = T::from_value((c + right + 7) >> 4);
        }
    }
}

pub fn nearest<T: Pixel>(src: &[T], h: usize, hmax: usize, dst: &mut [T]) {
    if src.is_empty() {
        dst.fill(T::default());
        return;
    }
    let last = src.len() - 1;
    if hmax.is_multiple_of(h) {
        let factor = hmax / h;
        for (chunk, &s) in dst.chunks_mut(factor).zip(src) {
            chunk.fill(s);
        }
        let covered = src.len() * factor;
        if covered < dst.len() {
            dst[covered..].fill(src[last]);
        }
        return;
    }
    for (x, d) in dst.iter_mut().enumerate() {
        *d = src[(x * h / hmax).min(last)];
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fancy_h2v1_u8_avx2(src: &[u8], dst: &mut [u8]) -> usize {
    use std::arch::x86_64::*;
    let n = src.len();
    let one = _mm256_set1_epi16(1);
    let two = _mm256_set1_epi16(2);
    let three = _mm256_set1_epi16(3);
    let load = |i: usize| _mm256_cvtepu8_epi16(_mm_loadu_si128(src.as_ptr().add(i) as *const __m128i));
    let mut i = 1;
    while i + 17 <= n && 2 * i + 32 <= dst.len() {
        let c = _mm256_mullo_epi16(load(i), three);
        let even = _mm256_srli_epi16::<2>(_mm256_add_epi16(_mm256_add_epi16(c, load(i - 1)), one));
        let odd = _mm256_srli_epi16::<2>(_mm256_add_epi16(_mm256_add_epi16(c, load(i + 1)), two));
        let packed = _mm256_or_si256(even, _mm256_slli_epi16::<8>(odd));
        _mm256_storeu_si256(dst.as_mut_ptr().add(2 * i) as *mut __m256i, packed);
        i += 16;
    }
    i
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fancy_h2v2_u8_avx2(near: &[u8], far: &[u8], dst: &mut [u8]) -> usize {
    use std::arch::x86_64::*;
    let n = near.len();
    let seven = _mm256_set1_epi16(7);
    let eight = _mm256_set1_epi16(8);
    let three = _mm256_set1_epi16(3);
    let colsum = |i: usize| {
        let a = _mm256_cvtepu8_epi16(_mm_loadu_si128(near.as_ptr().add(i) as *const __m128i));
        let b = _mm256_cvtepu8_epi16(_mm_loadu_si128(far.as_ptr().add(i) as *const __m128i));
        _mm256_add_epi16(_mm256_mullo_epi16(a, three), b)
    };
    let mut i = 1;
    while i + 17 <= n && 2 * i + 32 <= dst.len() {
        let c = _mm256_mullo_epi16(colsum(i), three);
        let even = _mm256_srli_epi16::<4>(_mm256_add_epi16(_mm256_add_epi16(c, colsum(i - 1)), eight));
        let odd = _mm256_srli_epi16::<4>(_mm256_add_epi16(_mm256_add_epi16(c, colsum(i + 1)), seven));
        let packed = _mm256_or_si256(even, _mm256_slli_epi16::<8>(odd));
        _mm256_storeu_si256(dst.as_mut_ptr().add(2 * i) as *mut __m256i, packed);
        i += 16;
    }
    i
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn fancy_h2v1_u8_simd128(src: &[u8], dst: &mut [u8]) -> usize {
    use core::arch::wasm32::*;
    let n = src.len();
    let three = u16x8_splat(3);
    let load = |i: usize| unsafe { u16x8_load_extend_u8x8(src.as_ptr().add(i)) };
    let mut i = 1;
    while i + 9 <= n && 2 * i + 16 <= dst.len() {
        let c = i16x8_mul(load(i), three);
        let even = u16x8_shr(i16x8_add(i16x8_add(c, load(i - 1)), u16x8_splat(1)), 2);
        let odd = u16x8_shr(i16x8_add(i16x8_add(c, load(i + 1)), u16x8_splat(2)), 2);
        let packed = v128_or(even, i16x8_shl(odd, 8));
        unsafe { v128_store(dst.as_mut_ptr().add(2 * i) as *mut v128, packed) };
        i += 8;
    }
    i
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn fancy_h2v2_u8_simd128(near: &[u8], far: &[u8], dst: &mut [u8]) -> usize {
    use core::arch::wasm32::*;
    let n = near.len();
    let three = u16x8_splat(3);
    let colsum = |i: usize| unsafe {
        let a = u16x8_load_extend_u8x8(near.as_ptr().add(i));
        let b = u16x8_load_extend_u8x8(far.as_ptr().add(i));
        i16x8_add(i16x8_mul(a, three), b)
    };
    let mut i = 1;
    while i + 9 <= n && 2 * i + 16 <= dst.len() {
        let c = i16x8_mul(colsum(i), three);
        let even = u16x8_shr(i16x8_add(i16x8_add(c, colsum(i - 1)), u16x8_splat(8)), 4);
        let odd = u16x8_shr(i16x8_add(i16x8_add(c, colsum(i + 1)), u16x8_splat(7)), 4);
        let packed = v128_or(even, i16x8_shl(odd, 8));
        unsafe { v128_store(dst.as_mut_ptr().add(2 * i) as *mut v128, packed) };
        i += 8;
    }
    i
}
