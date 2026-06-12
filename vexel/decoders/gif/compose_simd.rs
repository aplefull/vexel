pub fn compose_frame(
    indices: &[u8],
    color_table: &[u8],
    transparent_index: Option<u8>,
    frame_left: usize,
    frame_top: usize,
    frame_width: usize,
    frame_height: usize,
    canvas_width: usize,
    canvas_height: usize,
    canvas: &mut [u8],
) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe {
            compose_frame_avx2(
                indices,
                color_table,
                transparent_index,
                frame_left,
                frame_top,
                frame_width,
                frame_height,
                canvas_width,
                canvas_height,
                canvas,
            )
        };
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return compose_frame_wasm(
        indices,
        color_table,
        transparent_index,
        frame_left,
        frame_top,
        frame_width,
        frame_height,
        canvas_width,
        canvas_height,
        canvas,
    );

    compose_frame_scalar(
        indices,
        color_table,
        transparent_index,
        frame_left,
        frame_top,
        frame_width,
        frame_height,
        canvas_width,
        canvas_height,
        canvas,
    );
}

fn build_palette(color_table: &[u8]) -> [u32; 256] {
    let mut palette = [0u32; 256];
    let valid = (color_table.len() / 3).min(256);
    for i in 0..valid {
        let r = color_table[i * 3] as u32;
        let g = color_table[i * 3 + 1] as u32;
        let b = color_table[i * 3 + 2] as u32;
        palette[i] = r | (g << 8) | (b << 16) | (0xFF << 24);
    }
    palette
}

// ─── Scalar ───────────────────────────────────────────────────

fn compose_frame_scalar(
    indices: &[u8],
    color_table: &[u8],
    transparent_index: Option<u8>,
    frame_left: usize,
    frame_top: usize,
    frame_width: usize,
    frame_height: usize,
    canvas_width: usize,
    canvas_height: usize,
    canvas: &mut [u8],
) {
    let palette = build_palette(color_table);
    let valid = (color_table.len() / 3).min(256);

    let clamped_h = frame_height.min(canvas_height.saturating_sub(frame_top));
    let clamped_w = frame_width.min(canvas_width.saturating_sub(frame_left));

    let canvas_u32 = unsafe {
        std::slice::from_raw_parts_mut(canvas.as_mut_ptr() as *mut u32, canvas.len() / 4)
    };

    for y in 0..clamped_h {
        let index_row_start = y * frame_width;
        let canvas_row_start = (frame_top + y) * canvas_width + frame_left;

        if index_row_start + clamped_w > indices.len() || canvas_row_start + clamped_w > canvas_u32.len() {
            continue;
        }

        let index_row = &indices[index_row_start..index_row_start + clamped_w];
        let canvas_row = &mut canvas_u32[canvas_row_start..canvas_row_start + clamped_w];

        match transparent_index {
            None => {
                for (idx, px) in index_row.iter().zip(canvas_row.iter_mut()) {
                    if (*idx as usize) < valid {
                        *px = palette[*idx as usize];
                    }
                }
            }
            Some(transparent) => {
                for (idx, px) in index_row.iter().zip(canvas_row.iter_mut()) {
                    if *idx != transparent && (*idx as usize) < valid {
                        *px = palette[*idx as usize];
                    }
                }
            }
        }
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compose_frame_avx2(
    indices: &[u8],
    color_table: &[u8],
    transparent_index: Option<u8>,
    frame_left: usize,
    frame_top: usize,
    frame_width: usize,
    frame_height: usize,
    canvas_width: usize,
    canvas_height: usize,
    canvas: &mut [u8],
) {
    use std::arch::x86_64::*;

    let palette = build_palette(color_table);
    let valid = (color_table.len() / 3).min(256);

    let clamped_h = frame_height.min(canvas_height.saturating_sub(frame_top));
    let clamped_w = frame_width.min(canvas_width.saturating_sub(frame_left));

    let canvas_u32 = std::slice::from_raw_parts_mut(canvas.as_mut_ptr() as *mut u32, canvas.len() / 4);

    let palette_ptr = palette.as_ptr() as *const i32;
    let v_scale = _mm256_set1_epi32(4);

    for y in 0..clamped_h {
        let index_row_start = y * frame_width;
        let canvas_row_start = (frame_top + y) * canvas_width + frame_left;

        if index_row_start + clamped_w > indices.len() || canvas_row_start + clamped_w > canvas_u32.len() {
            continue;
        }

        let index_row = &indices[index_row_start..index_row_start + clamped_w];
        let canvas_row = &mut canvas_u32[canvas_row_start..canvas_row_start + clamped_w];

        let mut x = 0usize;

        match transparent_index {
            None => {
                while x + 8 <= clamped_w {
                    let idx_ptr = index_row.as_ptr().add(x);

                    let idx0 = *idx_ptr as i32;
                    let idx1 = *idx_ptr.add(1) as i32;
                    let idx2 = *idx_ptr.add(2) as i32;
                    let idx3 = *idx_ptr.add(3) as i32;
                    let idx4 = *idx_ptr.add(4) as i32;
                    let idx5 = *idx_ptr.add(5) as i32;
                    let idx6 = *idx_ptr.add(6) as i32;
                    let idx7 = *idx_ptr.add(7) as i32;

                    let vindex = _mm256_set_epi32(idx7, idx6, idx5, idx4, idx3, idx2, idx1, idx0);
                    let voffsets = _mm256_mullo_epi32(vindex, v_scale);
                    let colors = _mm256_i32gather_epi32(palette_ptr, voffsets, 1);
                    _mm256_storeu_si256(canvas_row.as_mut_ptr().add(x) as *mut __m256i, colors);
                    x += 8;
                }

                while x < clamped_w {
                    let idx = index_row[x] as usize;
                    if idx < valid {
                        canvas_row[x] = palette[idx];
                    }
                    x += 1;
                }
            }
            Some(transparent) => {
                let v_transparent = _mm256_set1_epi32(transparent as i32);

                while x + 8 <= clamped_w {
                    let idx_ptr = index_row.as_ptr().add(x);

                    let idx0 = *idx_ptr as i32;
                    let idx1 = *idx_ptr.add(1) as i32;
                    let idx2 = *idx_ptr.add(2) as i32;
                    let idx3 = *idx_ptr.add(3) as i32;
                    let idx4 = *idx_ptr.add(4) as i32;
                    let idx5 = *idx_ptr.add(5) as i32;
                    let idx6 = *idx_ptr.add(6) as i32;
                    let idx7 = *idx_ptr.add(7) as i32;

                    let vindex = _mm256_set_epi32(idx7, idx6, idx5, idx4, idx3, idx2, idx1, idx0);

                    let not_transparent = _mm256_andnot_si256(
                        _mm256_cmpeq_epi32(vindex, v_transparent),
                        _mm256_set1_epi32(-1),
                    );

                    let voffsets = _mm256_mullo_epi32(vindex, v_scale);
                    let colors = _mm256_i32gather_epi32(palette_ptr, voffsets, 1);

                    let existing = _mm256_loadu_si256(canvas_row.as_ptr().add(x) as *const __m256i);
                    let blended = _mm256_blendv_epi8(existing, colors, not_transparent);
                    _mm256_storeu_si256(canvas_row.as_mut_ptr().add(x) as *mut __m256i, blended);
                    x += 8;
                }

                while x < clamped_w {
                    let idx = index_row[x];
                    if idx != transparent && (idx as usize) < valid {
                        canvas_row[x] = palette[idx as usize];
                    }
                    x += 1;
                }
            }
        }
    }
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn compose_frame_wasm(
    indices: &[u8],
    color_table: &[u8],
    transparent_index: Option<u8>,
    frame_left: usize,
    frame_top: usize,
    frame_width: usize,
    frame_height: usize,
    canvas_width: usize,
    canvas_height: usize,
    canvas: &mut [u8],
) {
    use std::arch::wasm32::*;

    let palette = build_palette(color_table);
    let valid = (color_table.len() / 3).min(256);

    let clamped_h = frame_height.min(canvas_height.saturating_sub(frame_top));
    let clamped_w = frame_width.min(canvas_width.saturating_sub(frame_left));

    let canvas_u32 = unsafe {
        std::slice::from_raw_parts_mut(canvas.as_mut_ptr() as *mut u32, canvas.len() / 4)
    };

    for y in 0..clamped_h {
        let index_row_start = y * frame_width;
        let canvas_row_start = (frame_top + y) * canvas_width + frame_left;

        if index_row_start + clamped_w > indices.len() || canvas_row_start + clamped_w > canvas_u32.len() {
            continue;
        }

        let index_row = &indices[index_row_start..index_row_start + clamped_w];
        let canvas_row = &mut canvas_u32[canvas_row_start..canvas_row_start + clamped_w];

        let mut x = 0usize;

        match transparent_index {
            None => {
                while x + 4 <= clamped_w {
                    let p0 = palette[index_row[x] as usize];
                    let p1 = palette[index_row[x + 1] as usize];
                    let p2 = palette[index_row[x + 2] as usize];
                    let p3 = palette[index_row[x + 3] as usize];
                    let colors = u32x4(p0, p1, p2, p3);
                    unsafe {
                        v128_store(canvas_row.as_mut_ptr().add(x) as *mut v128, colors);
                    }
                    x += 4;
                }

                while x < clamped_w {
                    let idx = index_row[x] as usize;
                    if idx < valid {
                        canvas_row[x] = palette[idx];
                    }
                    x += 1;
                }
            }
            Some(transparent) => {
                let v_transparent = u32x4_splat(transparent as u32);

                while x + 4 <= clamped_w {
                    let i0 = index_row[x] as u32;
                    let i1 = index_row[x + 1] as u32;
                    let i2 = index_row[x + 2] as u32;
                    let i3 = index_row[x + 3] as u32;

                    let vindex = u32x4(i0, i1, i2, i3);
                    let mask = u32x4_eq(vindex, v_transparent);

                    let p0 = palette[i0 as usize];
                    let p1 = palette[i1 as usize];
                    let p2 = palette[i2 as usize];
                    let p3 = palette[i3 as usize];
                    let colors = u32x4(p0, p1, p2, p3);

                    let existing = unsafe { v128_load(canvas_row.as_ptr().add(x) as *const v128) };
                    let blended = v128_bitselect(existing, colors, mask);
                    unsafe {
                        v128_store(canvas_row.as_mut_ptr().add(x) as *mut v128, blended);
                    }
                    x += 4;
                }

                while x < clamped_w {
                    let idx = index_row[x];
                    if idx != transparent && (idx as usize) < valid {
                        canvas_row[x] = palette[idx as usize];
                    }
                    x += 1;
                }
            }
        }
    }
}
