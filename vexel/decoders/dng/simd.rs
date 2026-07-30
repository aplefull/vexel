pub fn shift_u16_to_u8(src: &[u16], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), dst.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { shift_u16_to_u8_avx2(src, dst) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return shift_u16_to_u8_wasm(src, dst);

    #[allow(unreachable_code)]
    shift_u16_to_u8_scalar(src, dst);
}

fn shift_u16_to_u8_scalar(src: &[u16], dst: &mut [u8]) {
    for (s, d) in src.iter().zip(dst.iter_mut()) {
        *d = (*s >> 8) as u8;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn shift_u16_to_u8_avx2(src: &[u16], dst: &mut [u8]) {
    use std::arch::x86_64::*;

    let n = src.len();
    let mut i = 0;

    while i + 32 <= n {
        let a = _mm256_loadu_si256(src.as_ptr().add(i) as *const __m256i);
        let b = _mm256_loadu_si256(src.as_ptr().add(i + 16) as *const __m256i);

        let a = _mm256_srli_epi16(a, 8);
        let b = _mm256_srli_epi16(b, 8);

        let packed = _mm256_permute4x64_epi64(_mm256_packus_epi16(a, b), 0xD8);
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed);

        i += 32;
    }

    shift_u16_to_u8_scalar(&src[i..], &mut dst[i..]);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn shift_u16_to_u8_wasm(src: &[u16], dst: &mut [u8]) {
    use std::arch::wasm32::*;

    let n = src.len();
    let mut i = 0;

    while i + 16 <= n {
        unsafe {
            let a = v128_load(src.as_ptr().add(i) as *const v128);
            let b = v128_load(src.as_ptr().add(i + 8) as *const v128);

            let a = u16x8_shr(a, 8);
            let b = u16x8_shr(b, 8);

            v128_store(dst.as_mut_ptr().add(i) as *mut v128, u8x16_narrow_i16x8(a, b));
        }

        i += 16;
    }

    shift_u16_to_u8_scalar(&src[i..], &mut dst[i..]);
}
