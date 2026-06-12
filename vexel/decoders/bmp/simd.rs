use crate::decoders::bmp::types::ColorEntry;

pub fn bgr_to_rgb_row(src: &[u8], dst: &mut [u8], width: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { bgr_to_rgb_row_avx2(src, dst, width) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return bgr_to_rgb_row_wasm(src, dst, width);

    bgr_to_rgb_row_scalar(src, dst, width);
}

pub fn apply_palette_row(indices: &[u8], palette: &[ColorEntry], dst: &mut [u8], width: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { apply_palette_row_avx2(indices, palette, dst, width) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return apply_palette_row_wasm(indices, palette, dst, width);

    apply_palette_row_scalar(indices, palette, dst, width);
}

pub fn expand_rgb555_row(src: &[u8], dst: &mut [u8], width: usize) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { expand_rgb555_row_avx2(src, dst, width) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return expand_rgb555_row_wasm(src, dst, width);

    expand_rgb555_row_scalar(src, dst, width);
}

pub fn extract_channels_rgb_row(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    r_bits: u32,
    g_shift: u32,
    g_bits: u32,
    b_shift: u32,
    b_bits: u32,
) {
    let byte_aligned = r_bits == 8 && g_bits == 8 && b_bits == 8;

    #[cfg(target_arch = "x86_64")]
    if byte_aligned && is_x86_feature_detected!("avx2") {
        return unsafe { extract_channels_rgb_row_avx2(src, dst, width, r_shift, g_shift, b_shift) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    if byte_aligned {
        return extract_channels_rgb_row_wasm(src, dst, width, r_shift, g_shift, b_shift);
    }

    extract_channels_rgb_row_scalar(src, dst, width, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits);
}

pub fn extract_channels_rgba_row(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    r_bits: u32,
    g_shift: u32,
    g_bits: u32,
    b_shift: u32,
    b_bits: u32,
    a_shift: u32,
    a_bits: u32,
) {
    let byte_aligned = r_bits == 8 && g_bits == 8 && b_bits == 8 && a_bits == 8;

    #[cfg(target_arch = "x86_64")]
    if byte_aligned && is_x86_feature_detected!("avx2") {
        return unsafe {
            extract_channels_rgba_row_avx2(src, dst, width, r_shift, g_shift, b_shift, a_shift)
        };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    if byte_aligned {
        return extract_channels_rgba_row_wasm(src, dst, width, r_shift, g_shift, b_shift, a_shift);
    }

    extract_channels_rgba_row_scalar(
        src, dst, width, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits, a_shift, a_bits,
    );
}

pub fn fill_bytes(dst: &mut [u8], value: u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { fill_bytes_avx2(dst, value) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return fill_bytes_wasm(dst, value);

    dst.fill(value);
}

// ─── Scalar ───────────────────────────────────────────────────

fn bgr_to_rgb_row_scalar(src: &[u8], dst: &mut [u8], width: usize) {
    for x in 0..width {
        let s = x * 3;
        if s + 2 < src.len() {
            dst[s] = src[s + 2];
            dst[s + 1] = src[s + 1];
            dst[s + 2] = src[s];
        }
    }
}

fn apply_palette_row_scalar(indices: &[u8], palette: &[ColorEntry], dst: &mut [u8], width: usize) {
    let fallback = ColorEntry { red: 0, green: 0, blue: 0, reserved: 0 };
    for x in 0..width {
        let idx = if x < indices.len() { indices[x] as usize } else { 0 };
        let color = palette.get(idx).unwrap_or(&fallback);
        dst[x * 3] = color.red;
        dst[x * 3 + 1] = color.green;
        dst[x * 3 + 2] = color.blue;
    }
}

fn expand_rgb555_row_scalar(src: &[u8], dst: &mut [u8], width: usize) {
    for x in 0..width {
        let s = x * 2;
        let pixel = if s + 1 < src.len() {
            u16::from_le_bytes([src[s], src[s + 1]])
        } else {
            0
        };
        let r = ((pixel >> 10) & 0x1F) as u8;
        let g = ((pixel >> 5) & 0x1F) as u8;
        let b = (pixel & 0x1F) as u8;
        dst[x * 3] = (r << 3) | (r >> 2);
        dst[x * 3 + 1] = (g << 3) | (g >> 2);
        dst[x * 3 + 2] = (b << 3) | (b >> 2);
    }
}

#[inline(always)]
fn extract_channel_scalar(pixel: u32, shift: u32, bits: u32) -> u8 {
    if bits == 0 {
        return 0;
    }
    let mask = ((1u32 << bits) - 1) << shift;
    let raw = (pixel & mask) >> shift;
    if bits >= 8 {
        (raw >> (bits - 8)) as u8
    } else {
        ((raw * 255) / ((1u32 << bits) - 1)) as u8
    }
}

fn extract_channels_rgb_row_scalar(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    r_bits: u32,
    g_shift: u32,
    g_bits: u32,
    b_shift: u32,
    b_bits: u32,
) {
    for x in 0..width {
        let s = x * 4;
        if s + 3 < src.len() {
            let pixel = u32::from_le_bytes([src[s], src[s + 1], src[s + 2], src[s + 3]]);
            dst[x * 3] = extract_channel_scalar(pixel, r_shift, r_bits);
            dst[x * 3 + 1] = extract_channel_scalar(pixel, g_shift, g_bits);
            dst[x * 3 + 2] = extract_channel_scalar(pixel, b_shift, b_bits);
        }
    }
}

fn extract_channels_rgba_row_scalar(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    r_bits: u32,
    g_shift: u32,
    g_bits: u32,
    b_shift: u32,
    b_bits: u32,
    a_shift: u32,
    a_bits: u32,
) {
    for x in 0..width {
        let s = x * 4;
        if s + 3 < src.len() {
            let pixel = u32::from_le_bytes([src[s], src[s + 1], src[s + 2], src[s + 3]]);
            dst[x * 4] = extract_channel_scalar(pixel, r_shift, r_bits);
            dst[x * 4 + 1] = extract_channel_scalar(pixel, g_shift, g_bits);
            dst[x * 4 + 2] = extract_channel_scalar(pixel, b_shift, b_bits);
            dst[x * 4 + 3] = extract_channel_scalar(pixel, a_shift, a_bits);
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn bgr_to_rgb_row_avx2(src: &[u8], dst: &mut [u8], width: usize) {
    use std::arch::x86_64::*;

    let shuf = _mm_set_epi8(
        -1, 12, 13, 14, 9, 10, 11, 6, 7, 8, 3, 4, 5, 0, 1, 2,
    );

    let mut x = 0usize;
    while x + 5 <= width {
        let s = x * 3;
        if s + 16 > src.len() || s + 16 > dst.len() {
            break;
        }
        let v = _mm_loadu_si128(src.as_ptr().add(s) as *const __m128i);
        let out = _mm_shuffle_epi8(v, shuf);
        _mm_storeu_si128(dst.as_mut_ptr().add(s) as *mut __m128i, out);
        x += 5;
    }

    bgr_to_rgb_row_scalar(&src[x * 3..], &mut dst[x * 3..], width - x);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn apply_palette_row_avx2(indices: &[u8], palette: &[ColorEntry], dst: &mut [u8], width: usize) {
    use std::arch::x86_64::*;

    let mut flat = [0u8; 256 * 3];
    let max = palette.len().min(256);
    for i in 0..max {
        flat[i * 3] = palette[i].red;
        flat[i * 3 + 1] = palette[i].green;
        flat[i * 3 + 2] = palette[i].blue;
    }

    let mut x = 0usize;
    while x + 8 <= width {
        if x + 8 > indices.len() {
            break;
        }
        let idx = _mm256_cvtepu8_epi32(_mm_loadl_epi64(indices.as_ptr().add(x) as *const __m128i));

        let r_base = flat.as_ptr() as i64;

        let r_offsets = _mm256_mullo_epi32(idx, _mm256_set1_epi32(3));
        let g_offsets = _mm256_add_epi32(r_offsets, _mm256_set1_epi32(1));
        let b_offsets = _mm256_add_epi32(r_offsets, _mm256_set1_epi32(2));

        let vr = _mm256_i32gather_epi32(r_base as *const i32, r_offsets, 1);
        let vg = _mm256_i32gather_epi32(r_base as *const i32, g_offsets, 1);
        let vb = _mm256_i32gather_epi32(r_base as *const i32, b_offsets, 1);

        let r_bytes = _mm256_and_si256(vr, _mm256_set1_epi32(0xFF));
        let g_bytes = _mm256_and_si256(vg, _mm256_set1_epi32(0xFF));
        let b_bytes = _mm256_and_si256(vb, _mm256_set1_epi32(0xFF));

        let d = dst.as_mut_ptr().add(x * 3);
        macro_rules! extract_lane {
            ($v:expr, $i:expr) => {
                match $i {
                    0 => _mm256_extract_epi32($v, 0) as u8,
                    1 => _mm256_extract_epi32($v, 1) as u8,
                    2 => _mm256_extract_epi32($v, 2) as u8,
                    3 => _mm256_extract_epi32($v, 3) as u8,
                    4 => _mm256_extract_epi32($v, 4) as u8,
                    5 => _mm256_extract_epi32($v, 5) as u8,
                    6 => _mm256_extract_epi32($v, 6) as u8,
                    _ => _mm256_extract_epi32($v, 7) as u8,
                }
            };
        }
        for i in 0..8usize {
            *d.add(i * 3) = extract_lane!(r_bytes, i);
            *d.add(i * 3 + 1) = extract_lane!(g_bytes, i);
            *d.add(i * 3 + 2) = extract_lane!(b_bytes, i);
        }

        x += 8;
    }

    let remaining_indices = if x < indices.len() { &indices[x..] } else { &[] };
    apply_palette_row_scalar(remaining_indices, palette, &mut dst[x * 3..], width - x);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn expand_rgb555_row_avx2(src: &[u8], dst: &mut [u8], width: usize) {
    use std::arch::x86_64::*;

    let mask5 = _mm256_set1_epi16(0x1F);
    let mut x = 0usize;

    while x + 16 <= width {
        let s = x * 2;
        if s + 32 > src.len() {
            break;
        }

        let pixels = _mm256_loadu_si256(src.as_ptr().add(s) as *const __m256i);

        let r_raw = _mm256_and_si256(_mm256_srli_epi16(pixels, 10), mask5);
        let g_raw = _mm256_and_si256(_mm256_srli_epi16(pixels, 5), mask5);
        let b_raw = _mm256_and_si256(pixels, mask5);

        let r8 = _mm256_or_si256(_mm256_slli_epi16(r_raw, 3), _mm256_srli_epi16(r_raw, 2));
        let g8 = _mm256_or_si256(_mm256_slli_epi16(g_raw, 3), _mm256_srli_epi16(g_raw, 2));
        let b8 = _mm256_or_si256(_mm256_slli_epi16(b_raw, 3), _mm256_srli_epi16(b_raw, 2));

        let r_lo = _mm256_extracti128_si256::<0>(r8);
        let r_hi = _mm256_extracti128_si256::<1>(r8);
        let g_lo = _mm256_extracti128_si256::<0>(g8);
        let g_hi = _mm256_extracti128_si256::<1>(g8);
        let b_lo = _mm256_extracti128_si256::<0>(b8);
        let b_hi = _mm256_extracti128_si256::<1>(b8);

        let d = dst.as_mut_ptr().add(x * 3);
        macro_rules! extract_epi16 {
            ($v:expr, $i:expr) => {
                match $i {
                    0 => _mm_extract_epi16($v, 0) as u8,
                    1 => _mm_extract_epi16($v, 1) as u8,
                    2 => _mm_extract_epi16($v, 2) as u8,
                    3 => _mm_extract_epi16($v, 3) as u8,
                    4 => _mm_extract_epi16($v, 4) as u8,
                    5 => _mm_extract_epi16($v, 5) as u8,
                    6 => _mm_extract_epi16($v, 6) as u8,
                    _ => _mm_extract_epi16($v, 7) as u8,
                }
            };
        }
        for i in 0..8usize {
            *d.add(i * 3) = extract_epi16!(r_lo, i);
            *d.add(i * 3 + 1) = extract_epi16!(g_lo, i);
            *d.add(i * 3 + 2) = extract_epi16!(b_lo, i);
        }

        let d2 = dst.as_mut_ptr().add((x + 8) * 3);
        for i in 0..8usize {
            *d2.add(i * 3) = extract_epi16!(r_hi, i);
            *d2.add(i * 3 + 1) = extract_epi16!(g_hi, i);
            *d2.add(i * 3 + 2) = extract_epi16!(b_hi, i);
        }

        x += 16;
    }

    expand_rgb555_row_scalar(&src[x * 2..], &mut dst[x * 3..], width - x);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn extract_channels_rgb_row_avx2(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
) {
    use std::arch::x86_64::*;

    let r_sh = _mm256_set1_epi32(r_shift as i32);
    let g_sh = _mm256_set1_epi32(g_shift as i32);
    let b_sh = _mm256_set1_epi32(b_shift as i32);
    let mask8 = _mm256_set1_epi32(0xFF);

    let mut x = 0usize;
    while x + 8 <= width {
        let s = x * 4;
        if s + 32 > src.len() {
            break;
        }

        let pixels = _mm256_loadu_si256(src.as_ptr().add(s) as *const __m256i);

        let r = _mm256_and_si256(_mm256_srlv_epi32(pixels, r_sh), mask8);
        let g = _mm256_and_si256(_mm256_srlv_epi32(pixels, g_sh), mask8);
        let b = _mm256_and_si256(_mm256_srlv_epi32(pixels, b_sh), mask8);

        let d = dst.as_mut_ptr().add(x * 3);
        macro_rules! extract_epi32 {
            ($v:expr, $i:expr) => {
                match $i {
                    0 => _mm256_extract_epi32($v, 0) as u8,
                    1 => _mm256_extract_epi32($v, 1) as u8,
                    2 => _mm256_extract_epi32($v, 2) as u8,
                    3 => _mm256_extract_epi32($v, 3) as u8,
                    4 => _mm256_extract_epi32($v, 4) as u8,
                    5 => _mm256_extract_epi32($v, 5) as u8,
                    6 => _mm256_extract_epi32($v, 6) as u8,
                    _ => _mm256_extract_epi32($v, 7) as u8,
                }
            };
        }
        for i in 0..8usize {
            *d.add(i * 3) = extract_epi32!(r, i);
            *d.add(i * 3 + 1) = extract_epi32!(g, i);
            *d.add(i * 3 + 2) = extract_epi32!(b, i);
        }

        x += 8;
    }

    extract_channels_rgb_row_scalar(&src[x * 4..], &mut dst[x * 3..], width - x, r_shift, 8, g_shift, 8, b_shift, 8);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn extract_channels_rgba_row_avx2(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
    a_shift: u32,
) {
    use std::arch::x86_64::*;

    let r_sh = _mm256_set1_epi32(r_shift as i32);
    let g_sh = _mm256_set1_epi32(g_shift as i32);
    let b_sh = _mm256_set1_epi32(b_shift as i32);
    let a_sh = _mm256_set1_epi32(a_shift as i32);
    let mask8 = _mm256_set1_epi32(0xFF);

    let mut x = 0usize;
    while x + 8 <= width {
        let s = x * 4;
        if s + 32 > src.len() {
            break;
        }

        let pixels = _mm256_loadu_si256(src.as_ptr().add(s) as *const __m256i);

        let r = _mm256_and_si256(_mm256_srlv_epi32(pixels, r_sh), mask8);
        let g = _mm256_and_si256(_mm256_srlv_epi32(pixels, g_sh), mask8);
        let b = _mm256_and_si256(_mm256_srlv_epi32(pixels, b_sh), mask8);
        let a = _mm256_and_si256(_mm256_srlv_epi32(pixels, a_sh), mask8);

        let d = dst.as_mut_ptr().add(x * 4);
        macro_rules! extract_epi32 {
            ($v:expr, $i:expr) => {
                match $i {
                    0 => _mm256_extract_epi32($v, 0) as u8,
                    1 => _mm256_extract_epi32($v, 1) as u8,
                    2 => _mm256_extract_epi32($v, 2) as u8,
                    3 => _mm256_extract_epi32($v, 3) as u8,
                    4 => _mm256_extract_epi32($v, 4) as u8,
                    5 => _mm256_extract_epi32($v, 5) as u8,
                    6 => _mm256_extract_epi32($v, 6) as u8,
                    _ => _mm256_extract_epi32($v, 7) as u8,
                }
            };
        }
        for i in 0..8usize {
            *d.add(i * 4) = extract_epi32!(r, i);
            *d.add(i * 4 + 1) = extract_epi32!(g, i);
            *d.add(i * 4 + 2) = extract_epi32!(b, i);
            *d.add(i * 4 + 3) = extract_epi32!(a, i);
        }

        x += 8;
    }

    extract_channels_rgba_row_scalar(
        &src[x * 4..],
        &mut dst[x * 4..],
        width - x,
        r_shift, 8, g_shift, 8, b_shift, 8, a_shift, 8,
    );
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fill_bytes_avx2(dst: &mut [u8], value: u8) {
    use std::arch::x86_64::*;

    let v = _mm256_set1_epi8(value as i8);
    let mut i = 0usize;
    while i + 32 <= dst.len() {
        _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, v);
        i += 32;
    }
    while i < dst.len() {
        dst[i] = value;
        i += 1;
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn bgr_to_rgb_row_wasm(src: &[u8], dst: &mut [u8], width: usize) {
    use std::arch::wasm32::*;

    let mut x = 0usize;
    while x + 5 <= width {
        let s = x * 3;
        if s + 16 > src.len() || s + 16 > dst.len() {
            break;
        }
        let v = unsafe { v128_load(src.as_ptr().add(s) as *const v128) };
        let out = i8x16_shuffle::<2, 1, 0, 5, 4, 3, 8, 7, 6, 11, 10, 9, 14, 13, 12, 15>(v, v);
        unsafe { v128_store(dst.as_mut_ptr().add(s) as *mut v128, out) };
        x += 5;
    }
    bgr_to_rgb_row_scalar(&src[x * 3..], &mut dst[x * 3..], width - x);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn apply_palette_row_wasm(indices: &[u8], palette: &[ColorEntry], dst: &mut [u8], width: usize) {
    use std::arch::wasm32::*;

    let mut flat = [0u8; 256 * 3];
    let max = palette.len().min(256);
    for i in 0..max {
        flat[i * 3] = palette[i].red;
        flat[i * 3 + 1] = palette[i].green;
        flat[i * 3 + 2] = palette[i].blue;
    }

    let mut x = 0usize;
    while x + 4 <= width {
        if x + 4 > indices.len() {
            break;
        }
        for i in 0..4usize {
            let idx = indices[x + i] as usize;
            let fp = idx * 3;
            if fp + 2 < flat.len() {
                dst[(x + i) * 3] = flat[fp];
                dst[(x + i) * 3 + 1] = flat[fp + 1];
                dst[(x + i) * 3 + 2] = flat[fp + 2];
            }
        }
        x += 4;
    }

    let remaining_indices = if x < indices.len() { &indices[x..] } else { &[] };
    apply_palette_row_scalar(remaining_indices, palette, &mut dst[x * 3..], width - x);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn expand_rgb555_row_wasm(src: &[u8], dst: &mut [u8], width: usize) {
    use std::arch::wasm32::*;

    let mask5 = u16x8_splat(0x1F);
    let mut x = 0usize;

    while x + 8 <= width {
        let s = x * 2;
        if s + 16 > src.len() {
            break;
        }

        let pixels = unsafe { v128_load(src.as_ptr().add(s) as *const v128) };

        let r_raw = v128_and(u16x8_shr(pixels, 10), mask5);
        let g_raw = v128_and(u16x8_shr(pixels, 5), mask5);
        let b_raw = v128_and(pixels, mask5);

        let r8 = v128_or(u16x8_shl(r_raw, 3), u16x8_shr(r_raw, 2));
        let g8 = v128_or(u16x8_shl(g_raw, 3), u16x8_shr(g_raw, 2));
        let b8 = v128_or(u16x8_shl(b_raw, 3), u16x8_shr(b_raw, 2));

        let d = &mut dst[x * 3..];
        macro_rules! extract_u16 {
            ($v:expr, $i:expr) => {
                match $i {
                    0 => u16x8_extract_lane::<0>($v) as u8,
                    1 => u16x8_extract_lane::<1>($v) as u8,
                    2 => u16x8_extract_lane::<2>($v) as u8,
                    3 => u16x8_extract_lane::<3>($v) as u8,
                    4 => u16x8_extract_lane::<4>($v) as u8,
                    5 => u16x8_extract_lane::<5>($v) as u8,
                    6 => u16x8_extract_lane::<6>($v) as u8,
                    _ => u16x8_extract_lane::<7>($v) as u8,
                }
            };
        }
        for i in 0..8usize {
            d[i * 3] = extract_u16!(r8, i);
            d[i * 3 + 1] = extract_u16!(g8, i);
            d[i * 3 + 2] = extract_u16!(b8, i);
        }

        x += 8;
    }

    expand_rgb555_row_scalar(&src[x * 2..], &mut dst[x * 3..], width - x);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn extract_channels_rgb_row_wasm(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
) {
    use std::arch::wasm32::*;

    let mask8 = u32x4_splat(0xFF);
    let mut x = 0usize;

    while x + 4 <= width {
        let s = x * 4;
        if s + 16 > src.len() {
            break;
        }

        let pixels = unsafe { v128_load(src.as_ptr().add(s) as *const v128) };

        let r = v128_and(u32x4_shr(pixels, r_shift), mask8);
        let g = v128_and(u32x4_shr(pixels, g_shift), mask8);
        let b = v128_and(u32x4_shr(pixels, b_shift), mask8);

        let d = &mut dst[x * 3..];
        for i in 0..4usize {
            let (rv, gv, bv) = match i {
                0 => (u32x4_extract_lane::<0>(r) as u8, u32x4_extract_lane::<0>(g) as u8, u32x4_extract_lane::<0>(b) as u8),
                1 => (u32x4_extract_lane::<1>(r) as u8, u32x4_extract_lane::<1>(g) as u8, u32x4_extract_lane::<1>(b) as u8),
                2 => (u32x4_extract_lane::<2>(r) as u8, u32x4_extract_lane::<2>(g) as u8, u32x4_extract_lane::<2>(b) as u8),
                _ => (u32x4_extract_lane::<3>(r) as u8, u32x4_extract_lane::<3>(g) as u8, u32x4_extract_lane::<3>(b) as u8),
            };
            d[i * 3] = rv;
            d[i * 3 + 1] = gv;
            d[i * 3 + 2] = bv;
        }

        x += 4;
    }

    extract_channels_rgb_row_scalar(&src[x * 4..], &mut dst[x * 3..], width - x, r_shift, 8, g_shift, 8, b_shift, 8);
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn extract_channels_rgba_row_wasm(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
    a_shift: u32,
) {
    use std::arch::wasm32::*;

    let mask8 = u32x4_splat(0xFF);
    let mut x = 0usize;

    while x + 4 <= width {
        let s = x * 4;
        if s + 16 > src.len() {
            break;
        }

        let pixels = unsafe { v128_load(src.as_ptr().add(s) as *const v128) };

        let r = v128_and(u32x4_shr(pixels, r_shift), mask8);
        let g = v128_and(u32x4_shr(pixels, g_shift), mask8);
        let b = v128_and(u32x4_shr(pixels, b_shift), mask8);
        let a = v128_and(u32x4_shr(pixels, a_shift), mask8);

        let d = &mut dst[x * 4..];
        for i in 0..4usize {
            let (rv, gv, bv, av) = match i {
                0 => (
                    u32x4_extract_lane::<0>(r) as u8,
                    u32x4_extract_lane::<0>(g) as u8,
                    u32x4_extract_lane::<0>(b) as u8,
                    u32x4_extract_lane::<0>(a) as u8,
                ),
                1 => (
                    u32x4_extract_lane::<1>(r) as u8,
                    u32x4_extract_lane::<1>(g) as u8,
                    u32x4_extract_lane::<1>(b) as u8,
                    u32x4_extract_lane::<1>(a) as u8,
                ),
                2 => (
                    u32x4_extract_lane::<2>(r) as u8,
                    u32x4_extract_lane::<2>(g) as u8,
                    u32x4_extract_lane::<2>(b) as u8,
                    u32x4_extract_lane::<2>(a) as u8,
                ),
                _ => (
                    u32x4_extract_lane::<3>(r) as u8,
                    u32x4_extract_lane::<3>(g) as u8,
                    u32x4_extract_lane::<3>(b) as u8,
                    u32x4_extract_lane::<3>(a) as u8,
                ),
            };
            d[i * 4] = rv;
            d[i * 4 + 1] = gv;
            d[i * 4 + 2] = bv;
            d[i * 4 + 3] = av;
        }

        x += 4;
    }

    extract_channels_rgba_row_scalar(
        &src[x * 4..],
        &mut dst[x * 4..],
        width - x,
        r_shift, 8, g_shift, 8, b_shift, 8, a_shift, 8,
    );
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn fill_bytes_wasm(dst: &mut [u8], value: u8) {
    use std::arch::wasm32::*;

    let v = i8x16_splat(value as i8);
    let mut i = 0usize;
    while i + 16 <= dst.len() {
        unsafe { v128_store(dst.as_mut_ptr().add(i) as *mut v128, v) };
        i += 16;
    }
    while i < dst.len() {
        dst[i] = value;
        i += 1;
    }
}
