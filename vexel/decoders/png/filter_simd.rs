// ─── Sub ──────────────────────────────────────────────────────

pub fn unfilter_sub(row: &mut [u8], bpp: usize) {
    let len = row.len();
    if bpp == 0 || bpp > len {
        return;
    }

    #[cfg(target_arch = "x86_64")]
    if bpp == 4 && is_x86_feature_detected!("sse2") {
        return unsafe { unfilter_sub_sse2_bpp4(row) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    if bpp == 4 {
        return unfilter_sub_wasm_bpp4(row);
    }

    unfilter_sub_scalar(row, bpp);
}

fn unfilter_sub_scalar(row: &mut [u8], bpp: usize) {
    match bpp {
        3 => unfilter_sub_fixed::<3>(row),
        4 => unfilter_sub_fixed::<4>(row),
        _ => {
            for i in bpp..row.len() {
                row[i] = row[i].wrapping_add(row[i - bpp]);
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn unfilter_sub_sse2_bpp4(row: &mut [u8]) {
    use std::arch::x86_64::*;

    let len = row.len();
    let mut carry = _mm_setzero_si128();

    let mut i = 0;
    while i + 16 <= len {
        let mut v = _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i);
        v = _mm_add_epi8(v, _mm_slli_si128(v, 4));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 8));
        v = _mm_add_epi8(v, carry);
        _mm_storeu_si128(row.as_mut_ptr().add(i) as *mut __m128i, v);
        carry = _mm_shuffle_epi32(v, 0xFF);
        i += 16;
    }

    while i < len {
        if i >= 4 {
            *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(*row.get_unchecked(i - 4));
        }
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unfilter_sub_wasm_bpp4(row: &mut [u8]) {
    use std::arch::wasm32::*;

    let len = row.len();
    let mut carry = u8x16_splat(0);

    let mut i = 0;
    while i + 16 <= len {
        let mut v = unsafe { v128_load(row.as_ptr().add(i) as *const v128) };
        v = u8x16_add(v, u8x16_shuffle::<16, 16, 16, 16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11>(v, u8x16_splat(0)));
        v = u8x16_add(v, u8x16_shuffle::<16, 16, 16, 16, 16, 16, 16, 16, 0, 1, 2, 3, 4, 5, 6, 7>(v, u8x16_splat(0)));
        v = u8x16_add(v, carry);
        unsafe { v128_store(row.as_mut_ptr().add(i) as *mut v128, v) };
        carry = u8x16_shuffle::<12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15>(v, v);
        i += 16;
    }

    while i < len {
        if i >= 4 {
            row[i] = row[i].wrapping_add(row[i - 4]);
        }
        i += 1;
    }
}

fn unfilter_sub_fixed<const BPP: usize>(row: &mut [u8]) {
    let len = row.len();
    let mut prev = [0u8; BPP];
    prev.copy_from_slice(&row[..BPP]);

    let mut i = BPP;
    while i + BPP <= len {
        for k in 0..BPP {
            prev[k] = row[i + k].wrapping_add(prev[k]);
            row[i + k] = prev[k];
        }
        i += BPP;
    }
    while i < len {
        row[i] = row[i].wrapping_add(row[i - BPP]);
        i += 1;
    }
}

// ─── Up ───────────────────────────────────────────────────────

pub fn unfilter_up(row: &mut [u8], prior: &[u8]) {
    let len = row.len().min(prior.len());

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        unsafe { return unfilter_up_avx2(row, prior, len) };
    }

    for i in 0..len {
        row[i] = row[i].wrapping_add(prior[i]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn unfilter_up_avx2(row: &mut [u8], prior: &[u8], len: usize) {
    use std::arch::x86_64::*;

    let mut i = 0;
    while i + 32 <= len {
        let r = _mm256_loadu_si256(row.as_ptr().add(i) as *const __m256i);
        let p = _mm256_loadu_si256(prior.as_ptr().add(i) as *const __m256i);
        _mm256_storeu_si256(row.as_mut_ptr().add(i) as *mut __m256i, _mm256_add_epi8(r, p));
        i += 32;
    }
    while i < len {
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(*prior.get_unchecked(i));
        i += 1;
    }
}

// ─── Average ──────────────────────────────────────────────────

pub fn unfilter_average(row: &mut [u8], prior: &[u8], bpp: usize) {
    let len = row.len();
    if bpp == 0 || bpp > len || prior.len() < len {
        return;
    }

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("sse4.1") {
        match bpp {
            3 => return unsafe { unfilter_average_sse41::<3>(row, prior) },
            4 => return unsafe { unfilter_average_sse41::<4>(row, prior) },
            _ => {}
        }
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    match bpp {
        3 => return unfilter_average_wasm::<3>(row, prior),
        4 => return unfilter_average_wasm::<4>(row, prior),
        _ => {}
    }

    unfilter_average_scalar(row, prior, bpp);
}

fn unfilter_average_scalar(row: &mut [u8], prior: &[u8], bpp: usize) {
    for i in 0..bpp {
        row[i] = row[i].wrapping_add(prior[i] >> 1);
    }
    for i in bpp..row.len() {
        let left = row[i - bpp] as u16;
        let above = prior[i] as u16;
        row[i] = row[i].wrapping_add(((left + above) >> 1) as u8);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
unsafe fn unfilter_average_sse41<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    use std::arch::x86_64::*;

    let len = row.len();
    let zero = _mm_setzero_si128();
    let byte_mask = _mm_set1_epi16(0xFF);

    for i in 0..BPP {
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(prior.get_unchecked(i) >> 1);
    }

    let mut left = _mm_cvtepu8_epi16(load_pixel::<BPP>(row.as_ptr()));

    let mut i = BPP;
    while i + BPP <= len {
        let above = _mm_cvtepu8_epi16(load_pixel::<BPP>(prior.as_ptr().add(i)));
        let raw = _mm_cvtepu8_epi16(load_pixel::<BPP>(row.as_ptr().add(i)));

        let avg = _mm_srli_epi16(_mm_add_epi16(left, above), 1);
        let out = _mm_and_si128(_mm_add_epi16(raw, avg), byte_mask);

        store_pixel::<BPP>(row.as_mut_ptr().add(i), _mm_packus_epi16(out, zero));

        left = out;
        i += BPP;
    }

    while i < len {
        let l = *row.get_unchecked(i - BPP) as u16;
        let a = *prior.get_unchecked(i) as u16;
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(((l + a) >> 1) as u8);
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unfilter_average_wasm<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    use std::arch::wasm32::*;

    let len = row.len();

    for i in 0..BPP {
        row[i] = row[i].wrapping_add(prior[i] >> 1);
    }

    let mut left = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(row, 0));

    let mut i = BPP;
    while i + BPP <= len {
        let above = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(prior, i));
        let raw = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(row, i));

        let avg = u16x8_shr(u16x8_add(left, above), 1);
        let out = v128_and(u16x8_add(raw, avg), u16x8_splat(0xFF));

        store_pixel_wasm::<BPP>(row, i, u8x16_narrow_i16x8(out, out));

        left = out;
        i += BPP;
    }

    while i < len {
        let l = row[i - BPP] as u16;
        let a = prior[i] as u16;
        row[i] = row[i].wrapping_add(((l + a) >> 1) as u8);
        i += 1;
    }
}

pub fn unfilter_average_no_prior(row: &mut [u8], bpp: usize) {
    let len = row.len();
    if bpp == 0 || bpp > len {
        return;
    }
    for i in bpp..len {
        row[i] = row[i].wrapping_add(row[i - bpp] >> 1);
    }
}

// ─── Paeth ────────────────────────────────────────────────────

pub fn unfilter_paeth(row: &mut [u8], prior: &[u8], bpp: usize) {
    let len = row.len();
    if bpp == 0 || bpp > len || prior.len() < len {
        return;
    }

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("sse4.1") {
        match bpp {
            3 => return unsafe { unfilter_paeth_sse41::<3>(row, prior) },
            4 => return unsafe { unfilter_paeth_sse41::<4>(row, prior) },
            _ => {}
        }
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    match bpp {
        3 => return unfilter_paeth_wasm::<3>(row, prior),
        4 => return unfilter_paeth_wasm::<4>(row, prior),
        _ => {}
    }

    match bpp {
        3 => unfilter_paeth_scalar::<3>(row, prior),
        4 => unfilter_paeth_scalar::<4>(row, prior),
        _ => {
            for i in 0..bpp {
                row[i] = row[i].wrapping_add(prior[i]);
            }
            for i in bpp..len {
                let pred = paeth_predictor(row[i - bpp], prior[i], prior[i - bpp]);
                row[i] = row[i].wrapping_add(pred);
            }
        }
    }
}

fn unfilter_paeth_scalar<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    let len = row.len();
    for i in 0..BPP {
        row[i] = row[i].wrapping_add(prior[i]);
    }
    for i in BPP..len {
        let pred = paeth_predictor(row[i - BPP], prior[i], prior[i - BPP]);
        row[i] = row[i].wrapping_add(pred);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
unsafe fn unfilter_paeth_sse41<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    use std::arch::x86_64::*;

    let len = row.len();
    let zero = _mm_setzero_si128();

    for i in 0..BPP {
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(*prior.get_unchecked(i));
    }

    let mut a = _mm_cvtepu8_epi16(load_pixel::<BPP>(row.as_ptr()));
    let mut c = _mm_cvtepu8_epi16(load_pixel::<BPP>(prior.as_ptr()));

    let mut i = BPP;
    while i + BPP <= len {
        let b = _mm_cvtepu8_epi16(load_pixel::<BPP>(prior.as_ptr().add(i)));

        let p = _mm_sub_epi16(_mm_add_epi16(a, b), c);
        let pa = _mm_abs_epi16(_mm_sub_epi16(p, a));
        let pb = _mm_abs_epi16(_mm_sub_epi16(p, b));
        let pc = _mm_abs_epi16(_mm_sub_epi16(p, c));

        let not_a = _mm_or_si128(_mm_cmpgt_epi16(pa, pb), _mm_cmpgt_epi16(pa, pc));
        let take_b = _mm_andnot_si128(_mm_cmpgt_epi16(pb, pc), not_a);
        let pred = _mm_blendv_epi8(_mm_blendv_epi8(a, b, take_b), c, _mm_andnot_si128(take_b, not_a));

        let raw = _mm_cvtepu8_epi16(load_pixel::<BPP>(row.as_ptr().add(i)));
        let out = _mm_and_si128(_mm_add_epi16(raw, pred), _mm_set1_epi16(0xFF));

        store_pixel::<BPP>(row.as_mut_ptr().add(i), _mm_packus_epi16(out, zero));

        a = out;
        c = b;
        i += BPP;
    }

    while i < len {
        let pred = paeth_predictor(*row.get_unchecked(i - BPP), *prior.get_unchecked(i), *prior.get_unchecked(i - BPP));
        *row.get_unchecked_mut(i) = row.get_unchecked(i).wrapping_add(pred);
        i += 1;
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn unfilter_paeth_wasm<const BPP: usize>(row: &mut [u8], prior: &[u8]) {
    use std::arch::wasm32::*;

    let len = row.len();

    for i in 0..BPP {
        row[i] = row[i].wrapping_add(prior[i]);
    }

    let mut a = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(row, 0));
    let mut c = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(prior, 0));

    let mut i = BPP;
    while i + BPP <= len {
        let b = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(prior, i));

        let p = i16x8_sub(i16x8_add(a, b), c);
        let pa = i16x8_abs(i16x8_sub(p, a));
        let pb = i16x8_abs(i16x8_sub(p, b));
        let pc = i16x8_abs(i16x8_sub(p, c));

        let not_a = v128_or(i16x8_gt(pa, pb), i16x8_gt(pa, pc));
        let take_b = v128_and(v128_not(i16x8_gt(pb, pc)), not_a);
        let take_c = v128_and(v128_not(take_b), not_a);
        let pred = v128_bitselect(c, v128_bitselect(b, a, take_b), take_c);

        let raw = u16x8_extend_low_u8x16(load_pixel_wasm::<BPP>(row, i));
        let out = v128_and(u16x8_add(raw, pred), u16x8_splat(0xFF));

        store_pixel_wasm::<BPP>(row, i, u8x16_narrow_i16x8(out, out));

        a = out;
        c = b;
        i += BPP;
    }

    while i < len {
        let pred = paeth_predictor(row[i - BPP], prior[i], prior[i - BPP]);
        row[i] = row[i].wrapping_add(pred);
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn load_pixel_wasm<const BPP: usize>(src: &[u8], offset: usize) -> std::arch::wasm32::v128 {
    use std::arch::wasm32::*;
    let mut tmp = [0u8; 16];
    tmp[..BPP].copy_from_slice(&src[offset..offset + BPP]);
    unsafe { v128_load(tmp.as_ptr() as *const v128) }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn store_pixel_wasm<const BPP: usize>(dst: &mut [u8], offset: usize, v: std::arch::wasm32::v128) {
    use std::arch::wasm32::*;
    let mut tmp = [0u8; 16];
    unsafe { v128_store(tmp.as_mut_ptr() as *mut v128, v) };
    dst[offset..offset + BPP].copy_from_slice(&tmp[..BPP]);
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn load_pixel<const BPP: usize>(ptr: *const u8) -> std::arch::x86_64::__m128i {
    use std::arch::x86_64::*;
    if BPP == 4 {
        _mm_cvtsi32_si128(std::ptr::read_unaligned(ptr as *const i32))
    } else {
        let mut tmp = [0u8; 4];
        std::ptr::copy_nonoverlapping(ptr, tmp.as_mut_ptr(), BPP);
        _mm_cvtsi32_si128(i32::from_le_bytes(tmp))
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn store_pixel<const BPP: usize>(ptr: *mut u8, v: std::arch::x86_64::__m128i) {
    use std::arch::x86_64::*;
    let packed = _mm_cvtsi128_si32(v).to_le_bytes();
    std::ptr::copy_nonoverlapping(packed.as_ptr(), ptr, BPP);
}

#[inline(always)]
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i16;
    let b = b as i16;
    let c = c as i16;

    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();

    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}
