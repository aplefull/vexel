#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdLevel {
    Scalar,
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    Simd128,
}

impl SimdLevel {
    #[allow(unreachable_code)]
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            return SimdLevel::Avx2;
        }
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        return SimdLevel::Simd128;
        SimdLevel::Scalar
    }
}

const FIX_0_298631336: i32 = 2446;

const FIX_0_390180644: i32 = 3196;

const FIX_0_541196100: i32 = 4433;

const FIX_0_765366865: i32 = 6270;

const FIX_0_899976223: i32 = 7373;

const FIX_1_175875602: i32 = 9633;

const FIX_1_501321110: i32 = 12299;

const FIX_1_847759065: i32 = 15137;

const FIX_1_961570560: i32 = 16069;

const FIX_2_053119869: i32 = 16819;

const FIX_2_562915447: i32 = 20995;

const FIX_3_072711026: i32 = 25172;

const CONST_BITS: i32 = 13;

const PASS1_BITS: i32 = 2;

const PASS1_SHIFT: i32 = CONST_BITS - PASS1_BITS;

const PASS2_SHIFT: i32 = CONST_BITS + PASS1_BITS + 3;

const WIDE_PASS1_BITS: i32 = 1;

const WIDE_PASS1_SHIFT: i32 = CONST_BITS - WIDE_PASS1_BITS;

const WIDE_PASS2_SHIFT: i32 = CONST_BITS + WIDE_PASS1_BITS + 3;

#[inline(always)]
pub fn idct_islow_u8(
    level: SimdLevel,
    coef: &[i16; 64],
    quant: &[i32; 64],
    out: &mut [u8],
    stride: usize,
    dc_only: bool,
) {
    let out = &mut out[..7 * stride + 8];
    if dc_only {
        let value = (((coef[0] as i32).wrapping_mul(quant[0]).wrapping_add(4) >> 3) + 128).clamp(0, 255) as u8;
        for row in 0..8 {
            out[row * stride..row * stride + 8].fill(value);
        }
        return;
    }
    match level {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => unsafe { idct_islow_u8_avx2(coef, quant, out, stride) },
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        SimdLevel::Simd128 => idct_islow_u8_simd128(coef, quant, out, stride),
        SimdLevel::Scalar => idct_islow_u8_scalar(coef, quant, out, stride),
    }
}

#[inline(always)]
#[allow(unreachable_code)]
pub fn nonzero_mask(block: &[i16; 64]) -> u64 {
    #[cfg(target_arch = "x86_64")]
    return nonzero_mask_sse2(block);

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return nonzero_mask_wasm(block);

    nonzero_mask_scalar(block)
}

// ─── Scalar ───────────────────────────────────────────────────

#[inline(always)]
fn islow_1d(v: [i32; 8]) -> [i32; 8] {
    let z2 = v[2];
    let z3 = v[6];
    let z1 = (z2 + z3).wrapping_mul(FIX_0_541196100);
    let tmp2 = z1.wrapping_add(z3.wrapping_mul(-FIX_1_847759065));
    let tmp3 = z1.wrapping_add(z2.wrapping_mul(FIX_0_765366865));
    let tmp0 = v[0].wrapping_add(v[4]).wrapping_shl(CONST_BITS as u32);
    let tmp1 = v[0].wrapping_sub(v[4]).wrapping_shl(CONST_BITS as u32);
    let tmp10 = tmp0.wrapping_add(tmp3);
    let tmp13 = tmp0.wrapping_sub(tmp3);
    let tmp11 = tmp1.wrapping_add(tmp2);
    let tmp12 = tmp1.wrapping_sub(tmp2);

    let (t0, t1, t2, t3) = (v[7], v[5], v[3], v[1]);
    let z1 = t0.wrapping_add(t3);
    let z2 = t1.wrapping_add(t2);
    let z3 = t0.wrapping_add(t2);
    let z4 = t1.wrapping_add(t3);
    let z5 = z3.wrapping_add(z4).wrapping_mul(FIX_1_175875602);
    let t0 = t0.wrapping_mul(FIX_0_298631336);
    let t1 = t1.wrapping_mul(FIX_2_053119869);
    let t2 = t2.wrapping_mul(FIX_3_072711026);
    let t3 = t3.wrapping_mul(FIX_1_501321110);
    let z1 = z1.wrapping_mul(-FIX_0_899976223);
    let z2 = z2.wrapping_mul(-FIX_2_562915447);
    let z3 = z3.wrapping_mul(-FIX_1_961570560).wrapping_add(z5);
    let z4 = z4.wrapping_mul(-FIX_0_390180644).wrapping_add(z5);
    let t0 = t0.wrapping_add(z1).wrapping_add(z3);
    let t1 = t1.wrapping_add(z2).wrapping_add(z4);
    let t2 = t2.wrapping_add(z2).wrapping_add(z3);
    let t3 = t3.wrapping_add(z1).wrapping_add(z4);

    [
        tmp10.wrapping_add(t3),
        tmp11.wrapping_add(t2),
        tmp12.wrapping_add(t1),
        tmp13.wrapping_add(t0),
        tmp13.wrapping_sub(t0),
        tmp12.wrapping_sub(t1),
        tmp11.wrapping_sub(t2),
        tmp10.wrapping_sub(t3),
    ]
}

fn idct_islow_u8_scalar(coef: &[i16; 64], quant: &[i32; 64], out: &mut [u8], stride: usize) {
    let mut ws = [0i32; 64];
    for col in 0..8 {
        let mut v = [0i32; 8];
        for row in 0..8 {
            v[row] = (coef[row * 8 + col] as i32).wrapping_mul(quant[row * 8 + col]);
        }
        let r = islow_1d(v);
        for row in 0..8 {
            ws[row * 8 + col] = r[row].wrapping_add(1 << (PASS1_SHIFT - 1)) >> PASS1_SHIFT;
        }
    }
    for row in 0..8 {
        let mut v = [0i32; 8];
        v.copy_from_slice(&ws[row * 8..row * 8 + 8]);
        let r = islow_1d(v);
        let dst = &mut out[row * stride..row * stride + 8];
        for col in 0..8 {
            dst[col] = ((r[col].wrapping_add(1 << (PASS2_SHIFT - 1)) >> PASS2_SHIFT) + 128).clamp(0, 255) as u8;
        }
    }
}

pub fn idct_islow_wide(
    coef: &[i16; 64],
    quant: &[i32; 64],
    out: &mut [u16],
    stride: usize,
    dc_only: bool,
    precision: u8,
) {
    let out = &mut out[..7 * stride + 8];
    let max = (1i64 << precision) - 1;
    let center = 1i64 << (precision - 1);
    if dc_only {
        let dc = (coef[0] as i64) * (quant[0] as i64);
        let value = (((dc + 4) >> 3) + center).clamp(0, max) as u16;
        for row in 0..8 {
            out[row * stride..row * stride + 8].fill(value);
        }
        return;
    }
    let mut ws = [0i32; 64];
    for col in 0..8 {
        let mut v = [0i32; 8];
        for row in 0..8 {
            v[row] = (coef[row * 8 + col] as i32).wrapping_mul(quant[row * 8 + col]);
        }
        let r = islow_1d(v);
        for row in 0..8 {
            ws[row * 8 + col] = r[row].wrapping_add(1 << (WIDE_PASS1_SHIFT - 1)) >> WIDE_PASS1_SHIFT;
        }
    }
    for row in 0..8 {
        let mut v = [0i32; 8];
        v.copy_from_slice(&ws[row * 8..row * 8 + 8]);
        let r = islow_1d(v);
        let dst = &mut out[row * stride..row * stride + 8];
        for col in 0..8 {
            let value = (r[col].wrapping_add(1 << (WIDE_PASS2_SHIFT - 1)) >> WIDE_PASS2_SHIFT) as i64 + center;
            dst[col] = value.clamp(0, max) as u16;
        }
    }
}

pub fn idct_islow_i16(coef: &[i16; 64], quant: &[i32; 64], out: &mut [i16], stride: usize, precision: u8) {
    let out = &mut out[..7 * stride + 8];
    let (pass1, pass2) = if precision > 8 { (WIDE_PASS1_SHIFT, WIDE_PASS2_SHIFT) } else { (PASS1_SHIFT, PASS2_SHIFT) };
    let mut ws = [0i32; 64];
    for col in 0..8 {
        let mut v = [0i32; 8];
        for row in 0..8 {
            v[row] = (coef[row * 8 + col] as i32).wrapping_mul(quant[row * 8 + col]);
        }
        let r = islow_1d(v);
        for row in 0..8 {
            ws[row * 8 + col] = r[row].wrapping_add(1 << (pass1 - 1)) >> pass1;
        }
    }
    for row in 0..8 {
        let mut v = [0i32; 8];
        v.copy_from_slice(&ws[row * 8..row * 8 + 8]);
        let r = islow_1d(v);
        let dst = &mut out[row * stride..row * stride + 8];
        for col in 0..8 {
            dst[col] = (r[col].wrapping_add(1 << (pass2 - 1)) >> pass2).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

fn nonzero_mask_scalar(block: &[i16; 64]) -> u64 {
    let mut mask = 0u64;
    for (i, &coef) in block.iter().enumerate() {
        mask |= ((coef != 0) as u64) << i;
    }
    mask
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn islow_1d_avx2(v: [std::arch::x86_64::__m256i; 8]) -> [std::arch::x86_64::__m256i; 8] {
    use std::arch::x86_64::*;
    let k = |c: i32| _mm256_set1_epi32(c);
    let z2 = v[2];
    let z3 = v[6];
    let z1 = _mm256_mullo_epi32(_mm256_add_epi32(z2, z3), k(FIX_0_541196100));
    let tmp2 = _mm256_add_epi32(z1, _mm256_mullo_epi32(z3, k(-FIX_1_847759065)));
    let tmp3 = _mm256_add_epi32(z1, _mm256_mullo_epi32(z2, k(FIX_0_765366865)));
    let tmp0 = _mm256_slli_epi32::<CONST_BITS>(_mm256_add_epi32(v[0], v[4]));
    let tmp1 = _mm256_slli_epi32::<CONST_BITS>(_mm256_sub_epi32(v[0], v[4]));
    let tmp10 = _mm256_add_epi32(tmp0, tmp3);
    let tmp13 = _mm256_sub_epi32(tmp0, tmp3);
    let tmp11 = _mm256_add_epi32(tmp1, tmp2);
    let tmp12 = _mm256_sub_epi32(tmp1, tmp2);

    let (t0, t1, t2, t3) = (v[7], v[5], v[3], v[1]);
    let z1 = _mm256_add_epi32(t0, t3);
    let z2 = _mm256_add_epi32(t1, t2);
    let z3 = _mm256_add_epi32(t0, t2);
    let z4 = _mm256_add_epi32(t1, t3);
    let z5 = _mm256_mullo_epi32(_mm256_add_epi32(z3, z4), k(FIX_1_175875602));
    let t0 = _mm256_mullo_epi32(t0, k(FIX_0_298631336));
    let t1 = _mm256_mullo_epi32(t1, k(FIX_2_053119869));
    let t2 = _mm256_mullo_epi32(t2, k(FIX_3_072711026));
    let t3 = _mm256_mullo_epi32(t3, k(FIX_1_501321110));
    let z1 = _mm256_mullo_epi32(z1, k(-FIX_0_899976223));
    let z2 = _mm256_mullo_epi32(z2, k(-FIX_2_562915447));
    let z3 = _mm256_add_epi32(_mm256_mullo_epi32(z3, k(-FIX_1_961570560)), z5);
    let z4 = _mm256_add_epi32(_mm256_mullo_epi32(z4, k(-FIX_0_390180644)), z5);
    let t0 = _mm256_add_epi32(t0, _mm256_add_epi32(z1, z3));
    let t1 = _mm256_add_epi32(t1, _mm256_add_epi32(z2, z4));
    let t2 = _mm256_add_epi32(t2, _mm256_add_epi32(z2, z3));
    let t3 = _mm256_add_epi32(t3, _mm256_add_epi32(z1, z4));

    [
        _mm256_add_epi32(tmp10, t3),
        _mm256_add_epi32(tmp11, t2),
        _mm256_add_epi32(tmp12, t1),
        _mm256_add_epi32(tmp13, t0),
        _mm256_sub_epi32(tmp13, t0),
        _mm256_sub_epi32(tmp12, t1),
        _mm256_sub_epi32(tmp11, t2),
        _mm256_sub_epi32(tmp10, t3),
    ]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn transpose8x8_i32_avx2(r: [std::arch::x86_64::__m256i; 8]) -> [std::arch::x86_64::__m256i; 8] {
    use std::arch::x86_64::*;
    let t0 = _mm256_unpacklo_epi32(r[0], r[1]);
    let t1 = _mm256_unpackhi_epi32(r[0], r[1]);
    let t2 = _mm256_unpacklo_epi32(r[2], r[3]);
    let t3 = _mm256_unpackhi_epi32(r[2], r[3]);
    let t4 = _mm256_unpacklo_epi32(r[4], r[5]);
    let t5 = _mm256_unpackhi_epi32(r[4], r[5]);
    let t6 = _mm256_unpacklo_epi32(r[6], r[7]);
    let t7 = _mm256_unpackhi_epi32(r[6], r[7]);
    let u0 = _mm256_unpacklo_epi64(t0, t2);
    let u1 = _mm256_unpackhi_epi64(t0, t2);
    let u2 = _mm256_unpacklo_epi64(t1, t3);
    let u3 = _mm256_unpackhi_epi64(t1, t3);
    let u4 = _mm256_unpacklo_epi64(t4, t6);
    let u5 = _mm256_unpackhi_epi64(t4, t6);
    let u6 = _mm256_unpacklo_epi64(t5, t7);
    let u7 = _mm256_unpackhi_epi64(t5, t7);
    [
        _mm256_permute2x128_si256::<0x20>(u0, u4),
        _mm256_permute2x128_si256::<0x20>(u1, u5),
        _mm256_permute2x128_si256::<0x20>(u2, u6),
        _mm256_permute2x128_si256::<0x20>(u3, u7),
        _mm256_permute2x128_si256::<0x31>(u0, u4),
        _mm256_permute2x128_si256::<0x31>(u1, u5),
        _mm256_permute2x128_si256::<0x31>(u2, u6),
        _mm256_permute2x128_si256::<0x31>(u3, u7),
    ]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn idct_islow_u8_avx2(coef: &[i16; 64], quant: &[i32; 64], out: &mut [u8], stride: usize) {
    use std::arch::x86_64::*;
    let mut rows = [_mm256_setzero_si256(); 8];
    for (i, row) in rows.iter_mut().enumerate() {
        let c = _mm256_cvtepi16_epi32(_mm_loadu_si128(coef.as_ptr().add(i * 8) as *const __m128i));
        let q = _mm256_loadu_si256(quant.as_ptr().add(i * 8) as *const __m256i);
        *row = _mm256_mullo_epi32(c, q);
    }

    let pass1 = islow_1d_avx2(rows);
    let round1 = _mm256_set1_epi32(1 << (PASS1_SHIFT - 1));
    let mut ws = [_mm256_setzero_si256(); 8];
    for i in 0..8 {
        ws[i] = _mm256_srai_epi32::<PASS1_SHIFT>(_mm256_add_epi32(pass1[i], round1));
    }

    let cols = transpose8x8_i32_avx2(ws);
    let pass2 = islow_1d_avx2(cols);
    let round2 = _mm256_set1_epi32((1 << (PASS2_SHIFT - 1)) + (128 << PASS2_SHIFT));
    let mut res = [_mm256_setzero_si256(); 8];
    for i in 0..8 {
        res[i] = _mm256_srai_epi32::<PASS2_SHIFT>(_mm256_add_epi32(pass2[i], round2));
    }
    let r = transpose8x8_i32_avx2(res);

    let order = _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7);
    let p01 = _mm256_packs_epi32(r[0], r[1]);
    let p23 = _mm256_packs_epi32(r[2], r[3]);
    let p45 = _mm256_packs_epi32(r[4], r[5]);
    let p67 = _mm256_packs_epi32(r[6], r[7]);
    let b0 = _mm256_permutevar8x32_epi32(_mm256_packus_epi16(p01, p23), order);
    let b1 = _mm256_permutevar8x32_epi32(_mm256_packus_epi16(p45, p67), order);

    let dst = out.as_mut_ptr();
    let lo0 = _mm256_castsi256_si128(b0);
    let hi0 = _mm256_extracti128_si256::<1>(b0);
    let lo1 = _mm256_castsi256_si128(b1);
    let hi1 = _mm256_extracti128_si256::<1>(b1);
    _mm_storel_epi64(dst as *mut __m128i, lo0);
    _mm_storel_epi64(dst.add(stride) as *mut __m128i, _mm_unpackhi_epi64(lo0, lo0));
    _mm_storel_epi64(dst.add(2 * stride) as *mut __m128i, hi0);
    _mm_storel_epi64(dst.add(3 * stride) as *mut __m128i, _mm_unpackhi_epi64(hi0, hi0));
    _mm_storel_epi64(dst.add(4 * stride) as *mut __m128i, lo1);
    _mm_storel_epi64(dst.add(5 * stride) as *mut __m128i, _mm_unpackhi_epi64(lo1, lo1));
    _mm_storel_epi64(dst.add(6 * stride) as *mut __m128i, hi1);
    _mm_storel_epi64(dst.add(7 * stride) as *mut __m128i, _mm_unpackhi_epi64(hi1, hi1));
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn nonzero_mask_sse2(block: &[i16; 64]) -> u64 {
    use std::arch::x86_64::*;
    unsafe {
        let zero = _mm_setzero_si128();
        let ptr = block.as_ptr() as *const __m128i;
        let mut mask = 0u64;
        for pair in 0..4 {
            let a = _mm_cmpeq_epi16(_mm_loadu_si128(ptr.add(pair * 2)), zero);
            let b = _mm_cmpeq_epi16(_mm_loadu_si128(ptr.add(pair * 2 + 1)), zero);
            let zeros = _mm_movemask_epi8(_mm_packs_epi16(a, b)) as u64;
            mask |= (!zeros & 0xFFFF) << (pair * 16);
        }
        mask
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn islow_1d_simd128(v: [core::arch::wasm32::v128; 8]) -> [core::arch::wasm32::v128; 8] {
    use core::arch::wasm32::*;
    let k = i32x4_splat;
    let z2 = v[2];
    let z3 = v[6];
    let z1 = i32x4_mul(i32x4_add(z2, z3), k(FIX_0_541196100));
    let tmp2 = i32x4_add(z1, i32x4_mul(z3, k(-FIX_1_847759065)));
    let tmp3 = i32x4_add(z1, i32x4_mul(z2, k(FIX_0_765366865)));
    let tmp0 = i32x4_shl(i32x4_add(v[0], v[4]), CONST_BITS as u32);
    let tmp1 = i32x4_shl(i32x4_sub(v[0], v[4]), CONST_BITS as u32);
    let tmp10 = i32x4_add(tmp0, tmp3);
    let tmp13 = i32x4_sub(tmp0, tmp3);
    let tmp11 = i32x4_add(tmp1, tmp2);
    let tmp12 = i32x4_sub(tmp1, tmp2);

    let (t0, t1, t2, t3) = (v[7], v[5], v[3], v[1]);
    let z1 = i32x4_add(t0, t3);
    let z2 = i32x4_add(t1, t2);
    let z3 = i32x4_add(t0, t2);
    let z4 = i32x4_add(t1, t3);
    let z5 = i32x4_mul(i32x4_add(z3, z4), k(FIX_1_175875602));
    let t0 = i32x4_mul(t0, k(FIX_0_298631336));
    let t1 = i32x4_mul(t1, k(FIX_2_053119869));
    let t2 = i32x4_mul(t2, k(FIX_3_072711026));
    let t3 = i32x4_mul(t3, k(FIX_1_501321110));
    let z1 = i32x4_mul(z1, k(-FIX_0_899976223));
    let z2 = i32x4_mul(z2, k(-FIX_2_562915447));
    let z3 = i32x4_add(i32x4_mul(z3, k(-FIX_1_961570560)), z5);
    let z4 = i32x4_add(i32x4_mul(z4, k(-FIX_0_390180644)), z5);
    let t0 = i32x4_add(t0, i32x4_add(z1, z3));
    let t1 = i32x4_add(t1, i32x4_add(z2, z4));
    let t2 = i32x4_add(t2, i32x4_add(z2, z3));
    let t3 = i32x4_add(t3, i32x4_add(z1, z4));

    [
        i32x4_add(tmp10, t3),
        i32x4_add(tmp11, t2),
        i32x4_add(tmp12, t1),
        i32x4_add(tmp13, t0),
        i32x4_sub(tmp13, t0),
        i32x4_sub(tmp12, t1),
        i32x4_sub(tmp11, t2),
        i32x4_sub(tmp10, t3),
    ]
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn transpose4_i32_simd128(
    a: core::arch::wasm32::v128,
    b: core::arch::wasm32::v128,
    c: core::arch::wasm32::v128,
    d: core::arch::wasm32::v128,
) -> [core::arch::wasm32::v128; 4] {
    use core::arch::wasm32::*;
    let t0 = i32x4_shuffle::<0, 4, 1, 5>(a, b);
    let t1 = i32x4_shuffle::<2, 6, 3, 7>(a, b);
    let t2 = i32x4_shuffle::<0, 4, 1, 5>(c, d);
    let t3 = i32x4_shuffle::<2, 6, 3, 7>(c, d);
    [
        i64x2_shuffle::<0, 2>(t0, t2),
        i64x2_shuffle::<1, 3>(t0, t2),
        i64x2_shuffle::<0, 2>(t1, t3),
        i64x2_shuffle::<1, 3>(t1, t3),
    ]
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn transpose8_halves_simd128(
    lo: [core::arch::wasm32::v128; 8],
    hi: [core::arch::wasm32::v128; 8],
) -> ([core::arch::wasm32::v128; 8], [core::arch::wasm32::v128; 8]) {
    let a = transpose4_i32_simd128(lo[0], lo[1], lo[2], lo[3]);
    let b = transpose4_i32_simd128(hi[0], hi[1], hi[2], hi[3]);
    let c = transpose4_i32_simd128(lo[4], lo[5], lo[6], lo[7]);
    let d = transpose4_i32_simd128(hi[4], hi[5], hi[6], hi[7]);
    ([a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]], [c[0], c[1], c[2], c[3], d[0], d[1], d[2], d[3]])
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn idct_islow_u8_simd128(coef: &[i16; 64], quant: &[i32; 64], out: &mut [u8], stride: usize) {
    use core::arch::wasm32::*;
    let mut lo = [i32x4_splat(0); 8];
    let mut hi = [i32x4_splat(0); 8];
    for i in 0..8 {
        let row = unsafe { v128_load(coef.as_ptr().add(i * 8) as *const v128) };
        let q_lo = unsafe { v128_load(quant.as_ptr().add(i * 8) as *const v128) };
        let q_hi = unsafe { v128_load(quant.as_ptr().add(i * 8 + 4) as *const v128) };
        lo[i] = i32x4_mul(i32x4_extend_low_i16x8(row), q_lo);
        hi[i] = i32x4_mul(i32x4_extend_high_i16x8(row), q_hi);
    }

    let round1 = i32x4_splat(1 << (PASS1_SHIFT - 1));
    let p_lo = islow_1d_simd128(lo);
    let p_hi = islow_1d_simd128(hi);
    for i in 0..8 {
        lo[i] = i32x4_shr(i32x4_add(p_lo[i], round1), PASS1_SHIFT as u32);
        hi[i] = i32x4_shr(i32x4_add(p_hi[i], round1), PASS1_SHIFT as u32);
    }

    let (cols_top, cols_bottom) = transpose8_halves_simd128(lo, hi);
    let round2 = i32x4_splat((1 << (PASS2_SHIFT - 1)) + (128 << PASS2_SHIFT));
    let r_top = islow_1d_simd128(cols_top);
    let r_bottom = islow_1d_simd128(cols_bottom);
    let mut res_top = [i32x4_splat(0); 8];
    let mut res_bottom = [i32x4_splat(0); 8];
    for i in 0..8 {
        res_top[i] = i32x4_shr(i32x4_add(r_top[i], round2), PASS2_SHIFT as u32);
        res_bottom[i] = i32x4_shr(i32x4_add(r_bottom[i], round2), PASS2_SHIFT as u32);
    }

    let left_top = transpose4_i32_simd128(res_top[0], res_top[1], res_top[2], res_top[3]);
    let right_top = transpose4_i32_simd128(res_top[4], res_top[5], res_top[6], res_top[7]);
    let left_bottom = transpose4_i32_simd128(res_bottom[0], res_bottom[1], res_bottom[2], res_bottom[3]);
    let right_bottom = transpose4_i32_simd128(res_bottom[4], res_bottom[5], res_bottom[6], res_bottom[7]);
    for r in 0..8 {
        let (left, right) = if r < 4 { (left_top[r], right_top[r]) } else { (left_bottom[r - 4], right_bottom[r - 4]) };
        let words = i16x8_narrow_i32x4(left, right);
        let bytes = u8x16_narrow_i16x8(words, words);
        let dst = &mut out[r * stride..r * stride + 8];
        dst.copy_from_slice(&u64x2_extract_lane::<0>(bytes).to_le_bytes());
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn nonzero_mask_wasm(block: &[i16; 64]) -> u64 {
    use std::arch::wasm32::*;

    let ptr = block.as_ptr();
    let zero = i16x8_splat(0);
    let mut mask = 0u64;

    for group in 0..4 {
        unsafe {
            let base = ptr.add(group * 16);
            let a = v128_load(base as *const v128);
            let b = v128_load(base.add(8) as *const v128);
            let eq_a = i16x8_eq(a, zero);
            let eq_b = i16x8_eq(b, zero);
            let packed = i8x16_narrow_i16x8(eq_a, eq_b);
            let zeros = i8x16_bitmask(packed) as u64;
            mask |= (!zeros & 0xFFFF) << (group * 16);
        }
    }

    mask
}
