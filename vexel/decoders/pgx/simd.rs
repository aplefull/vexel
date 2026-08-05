pub fn unpack_u8(src: &[u8], dst: &mut [u8], signed: bool, shift: u8) {
    if signed {
        level_shift_u8(src, dst, shift);
    } else {
        scale_u8(src, dst, shift);
    }
}

pub fn unpack_u16(src: &[u8], dst: &mut [u16], signed: bool, big_endian: bool, shift: u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { unpack_u16_avx2(src, dst, signed, big_endian, shift) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return unpack_u16_wasm(src, dst, signed, big_endian, shift);

    #[allow(unreachable_code)]
    unpack_u16_scalar(src, dst, signed, big_endian, shift);
}

fn scale_u8(src: &[u8], dst: &mut [u8], shift: u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { scale_u8_avx2(src, dst, shift) };
    }

    #[allow(unreachable_code)]
    scale_u8_scalar(src, dst, shift);
}

fn level_shift_u8(src: &[u8], dst: &mut [u8], shift: u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { level_shift_u8_avx2(src, dst, shift) };
    }

    #[allow(unreachable_code)]
    level_shift_u8_scalar(src, dst, shift);
}

fn scale_u8_scalar(src: &[u8], dst: &mut [u8], shift: u8) {
    let n = src.len().min(dst.len());

    for (d, &s) in dst[..n].iter_mut().zip(&src[..n]) {
        *d = replicate_u8(s, shift);
    }
}

fn level_shift_u8_scalar(src: &[u8], dst: &mut [u8], shift: u8) {
    let n = src.len().min(dst.len());
    let bias = 0x80u8 >> shift;

    for (d, &s) in dst[..n].iter_mut().zip(&src[..n]) {
        *d = replicate_u8(s.wrapping_add(bias), shift);
    }
}

fn replicate_u8(v: u8, shift: u8) -> u8 {
    if shift == 0 {
        return v;
    }

    let masked = v & (0xFFu8 >> shift);
    let up = masked << shift;

    up | (up >> (8 - shift))
}

fn unpack_u16_scalar(src: &[u8], dst: &mut [u16], signed: bool, big_endian: bool, shift: u8) {
    let n = (src.len() / 2).min(dst.len());
    let bias = if signed { 0x8000u16 >> shift } else { 0 };

    for (d, chunk) in dst[..n].iter_mut().zip(src.chunks_exact(2)) {
        let raw = if big_endian {
            u16::from_be_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_le_bytes([chunk[0], chunk[1]])
        };

        *d = replicate_u16(raw.wrapping_add(bias), shift);
    }
}

fn replicate_u16(v: u16, shift: u8) -> u16 {
    if shift == 0 {
        return v;
    }

    let masked = v & (0xFFFFu16 >> shift);
    let up = masked << shift;

    up | (up >> (16 - shift))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_u8_avx2(src: &[u8], dst: &mut [u8], shift: u8) {
    use std::arch::x86_64::*;

    if shift == 0 {
        let n = src.len().min(dst.len());
        dst[..n].copy_from_slice(&src[..n]);
        return;
    }

    let n = src.len().min(dst.len());
    let mut i = 0;

    let mask = _mm256_set1_epi8((0xFFu8 >> shift) as i8);
    let down = 8 - shift;
    let up_count = _mm_cvtsi32_si128(shift as i32);
    let down_count = _mm_cvtsi32_si128(down as i32);
    let up_mask = _mm256_set1_epi8((0xFFu8 << shift) as i8);
    let lo_mask = _mm256_set1_epi8(((0xFFu16 >> down) as u8) as i8);

    while i + 32 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let m = _mm256_and_si256(v, mask);

        let up = _mm256_and_si256(_mm256_sll_epi16(m, up_count), up_mask);
        let lo = _mm256_and_si256(_mm256_srl_epi16(up, down_count), lo_mask);

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, _mm256_or_si256(up, lo));

        i += 32;
    }

    if i < n {
        scale_u8_scalar(&src[i..n], &mut dst[i..n], shift);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn level_shift_u8_avx2(src: &[u8], dst: &mut [u8], shift: u8) {
    use std::arch::x86_64::*;

    let n = src.len().min(dst.len());
    let mut i = 0;

    let bias = _mm256_set1_epi8((0x80u8 >> shift) as i8);
    let keep = _mm256_set1_epi8((0xFFu8 >> shift) as i8);
    let up_mask = _mm256_set1_epi8((0xFFu8 << shift) as i8);
    let down = 8 - shift;
    let lo_mask = _mm256_set1_epi8(((0xFFu16 >> down) as u8) as i8);
    let up_count = _mm_cvtsi32_si128(shift as i32);
    let down_count = _mm_cvtsi32_si128(down as i32);

    while i + 32 <= n {
        let v = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let shifted = _mm256_add_epi8(v, bias);

        let out = if shift == 0 {
            shifted
        } else {
            let up = _mm256_and_si256(_mm256_sll_epi16(_mm256_and_si256(shifted, keep), up_count), up_mask);
            _mm256_or_si256(up, _mm256_and_si256(_mm256_srl_epi16(up, down_count), lo_mask))
        };

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, out);

        i += 32;
    }

    if i < n {
        level_shift_u8_scalar(&src[i..n], &mut dst[i..n], shift);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn unpack_u16_avx2(src: &[u8], dst: &mut [u16], signed: bool, big_endian: bool, shift: u8) {
    use std::arch::x86_64::*;

    let n = (src.len() / 2).min(dst.len());
    let mut i = 0;

    let bswap = _mm256_set_epi8(
        14, 15, 12, 13, 10, 11, 8, 9, 6, 7, 4, 5, 2, 3, 0, 1, 14, 15, 12, 13, 10, 11, 8, 9, 6, 7, 4, 5, 2, 3, 0, 1,
    );

    let bias = _mm256_set1_epi16(if signed { (0x8000u16 >> shift) as i16 } else { 0 });
    let mask = _mm256_set1_epi16((0xFFFFu16 >> shift) as i16);
    let down = 16 - shift;
    let up_count = _mm_cvtsi32_si128(shift as i32);
    let down_count = _mm_cvtsi32_si128(down as i32);

    while i + 16 <= n {
        let raw = _mm256_loadu_si256(src.as_ptr().add(i * 2) as *const __m256i);
        let v = if big_endian { _mm256_shuffle_epi8(raw, bswap) } else { raw };
        let shifted = _mm256_add_epi16(v, bias);

        let out = if shift == 0 {
            shifted
        } else {
            let up = _mm256_sll_epi16(_mm256_and_si256(shifted, mask), up_count);
            _mm256_or_si256(up, _mm256_srl_epi16(up, down_count))
        };

        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, out);

        i += 16;
    }

    if i < n {
        unpack_u16_scalar(&src[i * 2..], &mut dst[i..n], signed, big_endian, shift);
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unpack_u16_wasm(src: &[u8], dst: &mut [u16], signed: bool, big_endian: bool, shift: u8) {
    use std::arch::wasm32::*;

    let n = (src.len() / 2).min(dst.len());
    let mut i = 0;

    let bias = u16x8_splat(if signed { 0x8000u16 >> shift } else { 0 });
    let mask = u16x8_splat(0xFFFFu16 >> shift);
    let down = (16 - shift) as u32;

    while i + 8 <= n {
        let raw = v128_load(src.as_ptr().add(i * 2) as *const v128);
        let v = if big_endian {
            u8x16_shuffle::<1, 0, 3, 2, 5, 4, 7, 6, 9, 8, 11, 10, 13, 12, 15, 14>(raw, raw)
        } else {
            raw
        };
        let shifted = u16x8_add(v, bias);

        let out = if shift == 0 {
            shifted
        } else {
            let up = u16x8_shl(v128_and(shifted, mask), shift as u32);
            v128_or(up, u16x8_shr(up, down))
        };

        v128_store(dst.as_mut_ptr().add(i) as *mut v128, out);

        i += 8;
    }

    if i < n {
        unpack_u16_scalar(&src[i * 2..], &mut dst[i..n], signed, big_endian, shift);
    }
}
