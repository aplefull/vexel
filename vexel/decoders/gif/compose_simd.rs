pub struct Palette {
    colors: [u32; 256],
    valid: usize,
}

impl Palette {
    pub fn new(color_table: &[u8]) -> Palette {
        let mut colors = [0u32; 256];
        let valid = (color_table.len() / 3).min(256);

        for i in 0..valid {
            let r = color_table[i * 3] as u32;
            let g = color_table[i * 3 + 1] as u32;
            let b = color_table[i * 3 + 2] as u32;
            colors[i] = r | (g << 8) | (b << 16) | (0xFF << 24);
        }

        Palette { colors, valid }
    }

    fn effective_transparent(&self, transparent_index: Option<u8>) -> Option<u8> {
        match transparent_index {
            Some(t) => Some(t),
            None if self.valid < 256 => Some(self.valid as u8),
            None => None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn compose_frame(
    indices: &[u8],
    palette: &Palette,
    transparent_index: Option<u8>,
    frame_left: usize,
    frame_top: usize,
    frame_width: usize,
    frame_height: usize,
    canvas_width: usize,
    canvas_height: usize,
    canvas: &mut [u8],
) {
    if !(canvas.as_ptr() as usize).is_multiple_of(4) || !canvas.len().is_multiple_of(4) {
        return;
    }

    let clamped_h = frame_height.min(canvas_height.saturating_sub(frame_top));
    let clamped_w = frame_width.min(canvas_width.saturating_sub(frame_left));

    if clamped_w == 0 || clamped_h == 0 {
        return;
    }

    let canvas_u32 = unsafe { std::slice::from_raw_parts_mut(canvas.as_mut_ptr() as *mut u32, canvas.len() / 4) };

    for y in 0..clamped_h {
        let index_row_start = y * frame_width;
        let canvas_row_start = (frame_top + y) * canvas_width + frame_left;

        if index_row_start + clamped_w > indices.len() || canvas_row_start + clamped_w > canvas_u32.len() {
            continue;
        }

        let src = &indices[index_row_start..index_row_start + clamped_w];
        let dst = &mut canvas_u32[canvas_row_start..canvas_row_start + clamped_w];

        compose_row(src, dst, palette, transparent_index);
    }
}

fn compose_row(src: &[u8], dst: &mut [u32], palette: &Palette, transparent_index: Option<u8>) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { compose_row_avx2(src, dst, palette, transparent_index) };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return compose_row_wasm(src, dst, palette, transparent_index);

    #[allow(unreachable_code)]
    compose_row_scalar(src, dst, palette, transparent_index)
}

// ─── Scalar ───────────────────────────────────────────────────

fn compose_row_scalar(src: &[u8], dst: &mut [u32], palette: &Palette, transparent_index: Option<u8>) {
    let colors = &palette.colors;

    match palette.effective_transparent(transparent_index) {
        None => {
            for (idx, px) in src.iter().zip(dst.iter_mut()) {
                *px = colors[*idx as usize];
            }
        }
        Some(transparent) => {
            let valid = palette.valid;
            for (idx, px) in src.iter().zip(dst.iter_mut()) {
                if *idx != transparent && (*idx as usize) < valid {
                    *px = colors[*idx as usize];
                }
            }
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn lookup8(colors: *const u32, src: *const u8) -> std::arch::x86_64::__m256i {
    use std::arch::x86_64::*;

    let c0 = *colors.add(*src as usize) as i32;
    let c1 = *colors.add(*src.add(1) as usize) as i32;
    let c2 = *colors.add(*src.add(2) as usize) as i32;
    let c3 = *colors.add(*src.add(3) as usize) as i32;
    let c4 = *colors.add(*src.add(4) as usize) as i32;
    let c5 = *colors.add(*src.add(5) as usize) as i32;
    let c6 = *colors.add(*src.add(6) as usize) as i32;
    let c7 = *colors.add(*src.add(7) as usize) as i32;

    _mm256_set_epi32(c7, c6, c5, c4, c3, c2, c1, c0)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compose_row_avx2(src: &[u8], dst: &mut [u32], palette: &Palette, transparent_index: Option<u8>) {
    use std::arch::x86_64::*;

    let len = src.len().min(dst.len());
    let colors = palette.colors.as_ptr();
    let src_ptr = src.as_ptr();
    let dst_ptr = dst.as_mut_ptr();

    let Some(transparent) = palette.effective_transparent(transparent_index) else {
        let mut x = 0;
        while x + 32 <= len {
            let c0 = lookup8(colors, src_ptr.add(x));
            let c1 = lookup8(colors, src_ptr.add(x + 8));
            let c2 = lookup8(colors, src_ptr.add(x + 16));
            let c3 = lookup8(colors, src_ptr.add(x + 24));

            _mm256_storeu_si256(dst_ptr.add(x) as *mut __m256i, c0);
            _mm256_storeu_si256(dst_ptr.add(x + 8) as *mut __m256i, c1);
            _mm256_storeu_si256(dst_ptr.add(x + 16) as *mut __m256i, c2);
            _mm256_storeu_si256(dst_ptr.add(x + 24) as *mut __m256i, c3);
            x += 32;
        }

        while x + 8 <= len {
            _mm256_storeu_si256(dst_ptr.add(x) as *mut __m256i, lookup8(colors, src_ptr.add(x)));
            x += 8;
        }

        while x < len {
            *dst_ptr.add(x) = *colors.add(*src_ptr.add(x) as usize);
            x += 1;
        }

        return;
    };

    let v_transparent = _mm256_set1_epi8(transparent as i8);
    let mut x = 0;

    while x + 32 <= len {
        let raw = _mm256_loadu_si256(src_ptr.add(x) as *const __m256i);
        let eq = _mm256_cmpeq_epi8(raw, v_transparent);
        let mask = _mm256_movemask_epi8(eq) as u32;

        if mask == u32::MAX {
            x += 32;
            continue;
        }

        let c0 = lookup8(colors, src_ptr.add(x));
        let c1 = lookup8(colors, src_ptr.add(x + 8));
        let c2 = lookup8(colors, src_ptr.add(x + 16));
        let c3 = lookup8(colors, src_ptr.add(x + 24));

        if mask == 0 {
            _mm256_storeu_si256(dst_ptr.add(x) as *mut __m256i, c0);
            _mm256_storeu_si256(dst_ptr.add(x + 8) as *mut __m256i, c1);
            _mm256_storeu_si256(dst_ptr.add(x + 16) as *mut __m256i, c2);
            _mm256_storeu_si256(dst_ptr.add(x + 24) as *mut __m256i, c3);
            x += 32;
            continue;
        }

        let keep0 = _mm256_cvtepi8_epi32(_mm256_castsi256_si128(eq));
        let keep1 = _mm256_cvtepi8_epi32(_mm_srli_si128(_mm256_castsi256_si128(eq), 8));
        let hi = _mm256_extracti128_si256(eq, 1);
        let keep2 = _mm256_cvtepi8_epi32(hi);
        let keep3 = _mm256_cvtepi8_epi32(_mm_srli_si128(hi, 8));

        blend_store(dst_ptr.add(x), c0, keep0);
        blend_store(dst_ptr.add(x + 8), c1, keep1);
        blend_store(dst_ptr.add(x + 16), c2, keep2);
        blend_store(dst_ptr.add(x + 24), c3, keep3);
        x += 32;
    }

    while x + 8 <= len {
        let raw = _mm_loadl_epi64(src_ptr.add(x) as *const __m128i);
        let eq = _mm_cmpeq_epi8(raw, _mm256_castsi256_si128(v_transparent));
        let mask = _mm_movemask_epi8(eq) as u32 & 0xFF;

        if mask != 0xFF {
            let c = lookup8(colors, src_ptr.add(x));
            if mask == 0 {
                _mm256_storeu_si256(dst_ptr.add(x) as *mut __m256i, c);
            } else {
                blend_store(dst_ptr.add(x), c, _mm256_cvtepi8_epi32(eq));
            }
        }

        x += 8;
    }

    while x < len {
        let idx = *src_ptr.add(x);
        if idx != transparent {
            *dst_ptr.add(x) = *colors.add(idx as usize);
        }
        x += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn blend_store(dst: *mut u32, colors: std::arch::x86_64::__m256i, keep: std::arch::x86_64::__m256i) {
    use std::arch::x86_64::*;

    let existing = _mm256_loadu_si256(dst as *const __m256i);
    _mm256_storeu_si256(dst as *mut __m256i, _mm256_blendv_epi8(colors, existing, keep));
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn compose_row_wasm(src: &[u8], dst: &mut [u32], palette: &Palette, transparent_index: Option<u8>) {
    use std::arch::wasm32::*;

    let len = src.len().min(dst.len());
    let colors = &palette.colors;

    let Some(transparent) = palette.effective_transparent(transparent_index) else {
        let mut x = 0;
        while x + 4 <= len {
            let c = u32x4(
                colors[src[x] as usize],
                colors[src[x + 1] as usize],
                colors[src[x + 2] as usize],
                colors[src[x + 3] as usize],
            );
            unsafe { v128_store(dst.as_mut_ptr().add(x) as *mut v128, c) };
            x += 4;
        }

        while x < len {
            dst[x] = colors[src[x] as usize];
            x += 1;
        }

        return;
    };

    let v_transparent = u8x16_splat(transparent);
    let mut x = 0;

    while x + 16 <= len {
        let raw = unsafe { v128_load(src.as_ptr().add(x) as *const v128) };
        let eq = u8x16_eq(raw, v_transparent);

        if !v128_any_true(eq) {
            for g in 0..4 {
                let b = x + g * 4;
                let c = u32x4(
                    colors[src[b] as usize],
                    colors[src[b + 1] as usize],
                    colors[src[b + 2] as usize],
                    colors[src[b + 3] as usize],
                );
                unsafe { v128_store(dst.as_mut_ptr().add(b) as *mut v128, c) };
            }
            x += 16;
            continue;
        }

        if u8x16_all_true(eq) {
            x += 16;
            continue;
        }

        for i in x..x + 16 {
            let idx = src[i];
            if idx != transparent {
                dst[i] = colors[idx as usize];
            }
        }
        x += 16;
    }

    while x < len {
        let idx = src[x];
        if idx != transparent {
            dst[x] = colors[idx as usize];
        }
        x += 1;
    }
}
