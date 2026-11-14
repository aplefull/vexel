use std::mem::MaybeUninit;

use super::lzw::FrameIndices;

const KEEP: u32 = 1;
const BAND_PIXELS: usize = 1 << 15;
const STREAMING_MIN_BYTES: usize = 8 << 20;
#[cfg(feature = "rayon")]
const PARALLEL_MIN_BYTES: usize = 1 << 20;

pub struct Palette {
    colors: [u32; 256],
    keep_index: Option<u8>,
}

impl Palette {
    pub fn new(color_table: &[u8], transparent_index: Option<u8>) -> Palette {
        let mut colors = [0u32; 256];
        let valid = (color_table.len() / 3).min(256);
        for (color, rgb) in colors.iter_mut().zip(color_table.chunks_exact(3)) {
            *color = u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 0xFF]);
        }

        let keep_index = match transparent_index {
            Some(index) => Some(index),
            None if valid < 256 => Some(valid as u8),
            None => None,
        };
        if let Some(index) = keep_index {
            colors[index as usize] = KEEP;
        }

        Palette { colors, keep_index }
    }
}

impl Palette {
    fn background_colors(&self) -> [u32; 256] {
        let mut colors = self.colors;
        if let Some(index) = self.keep_index {
            colors[index as usize] = 0;
        }
        colors
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Keep,
    Clear,
    Leave,
}

#[derive(Clone, Copy)]
pub struct Placement {
    pub left: usize,
    pub top: usize,
    pub width: usize,
    pub height: usize,
}

pub struct Layer<'a> {
    pub placement: Placement,
    pub indices: &'a FrameIndices,
    pub palette: &'a Palette,
    pub update: Update,
}

#[cfg(feature = "rayon")]
fn parallel(size: usize) -> bool {
    size >= PARALLEL_MIN_BYTES && rayon::current_num_threads() > 1
}

pub fn compose_first(layer: &Layer, output: &mut Vec<u8>, canvas_width: usize, canvas_height: usize) {
    let stride = canvas_width * 4;
    let size = stride * canvas_height;
    output.clear();
    output.reserve_exact(size);
    if stride == 0 {
        return;
    }

    let Placement { left, top, width, height } = layer.placement;
    let rows = top..top + height.min(layer.indices.rows());
    let colors = layer.palette.background_colors();
    let rows_per_band = (BAND_PIXELS / canvas_width).max(1);
    let band_bytes = rows_per_band * stride;

    let run = |(index, band): (usize, &mut [MaybeUninit<u8>])| {
        for (offset, row) in band.chunks_mut(stride).enumerate() {
            let y = index * rows_per_band + offset;
            let row = row.as_mut_ptr().cast::<u8>();
            unsafe {
                if !rows.contains(&y) {
                    std::ptr::write_bytes(row, 0, stride);
                    continue;
                }
                std::ptr::write_bytes(row, 0, left * 4);
                lookup_span(layer.indices.row(y - top, width), &colors, row.add(left * 4));
                std::ptr::write_bytes(row.add((left + width) * 4), 0, stride - (left + width) * 4);
            }
        }
    };

    let spare = &mut output.spare_capacity_mut()[..size];

    #[cfg(feature = "rayon")]
    if parallel(size) {
        use rayon::prelude::*;
        spare.par_chunks_mut(band_bytes).enumerate().for_each(run);
        unsafe { output.set_len(size) };
        return;
    }

    spare.chunks_mut(band_bytes).enumerate().for_each(run);
    unsafe { output.set_len(size) };
}

pub fn compose_layer(layer: &Layer, canvas: &mut [u8], output: &mut Vec<u8>, canvas_width: usize) {
    let stride = canvas_width * 4;
    let size = canvas.len();
    output.clear();
    output.reserve_exact(size);
    if stride == 0 {
        return;
    }

    let rows_per_band = (BAND_PIXELS / canvas_width).max(1);
    let band_bytes = rows_per_band * stride;
    let streaming = size >= STREAMING_MIN_BYTES;
    let spare = &mut output.spare_capacity_mut()[..size];

    let run = |(index, (canvas_band, output_band)): (usize, (&mut [u8], &mut [MaybeUninit<u8>]))| {
        let mut scratch = Vec::new();
        let first_row = index * rows_per_band;
        for (row_offset, (canvas_row, output_row)) in
            canvas_band.chunks_mut(stride).zip(output_band.chunks_mut(stride)).enumerate()
        {
            compose_row(layer, first_row + row_offset, canvas_row, output_row, &mut scratch, streaming);
        }
        if streaming {
            store_fence();
        }
    };

    #[cfg(feature = "rayon")]
    if parallel(size) {
        use rayon::prelude::*;
        canvas.par_chunks_mut(band_bytes).zip(spare.par_chunks_mut(band_bytes)).enumerate().for_each(run);
        unsafe { output.set_len(size) };
        return;
    }

    canvas.chunks_mut(band_bytes).zip(spare.chunks_mut(band_bytes)).enumerate().for_each(run);
    unsafe { output.set_len(size) };
}

fn compose_row(
    layer: &Layer,
    y: usize,
    canvas_row: &mut [u8],
    output_row: &mut [MaybeUninit<u8>],
    scratch: &mut Vec<u8>,
    streaming: bool,
) {
    let Placement { left, top, width, height } = layer.placement;
    let row = y.wrapping_sub(top);
    if y < top || row >= height {
        copy_row(canvas_row, output_row, streaming);
        return;
    }

    let span = left * 4..(left + width) * 4;
    if row >= layer.indices.rows() {
        copy_row(canvas_row, output_row, streaming);
        if layer.update == Update::Clear {
            canvas_row[span].fill(0);
        }
        return;
    }

    let indices = layer.indices.row(row, width);
    if layer.update == Update::Keep {
        unsafe { compose_span(indices, layer.palette, canvas_row[span].as_mut_ptr()) };
        copy_row(canvas_row, output_row, streaming);
        return;
    }

    scratch.clear();
    scratch.extend_from_slice(canvas_row);
    unsafe { compose_span(indices, layer.palette, scratch[span.clone()].as_mut_ptr()) };
    copy_row(scratch, output_row, streaming);
    if layer.update == Update::Clear {
        canvas_row[span].fill(0);
    }
}

fn copy_row(src: &[u8], dst: &mut [MaybeUninit<u8>], streaming: bool) {
    let len = src.len().min(dst.len());
    let dst = dst.as_mut_ptr().cast::<u8>();

    #[cfg(target_arch = "x86_64")]
    if streaming {
        return unsafe { stream_copy(src.as_ptr(), dst, len) };
    }

    let _ = streaming;
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), dst, len) };
}

#[cfg(target_arch = "x86_64")]
unsafe fn stream_copy(src: *const u8, dst: *mut u8, len: usize) {
    use std::arch::x86_64::*;

    let head = (dst.align_offset(16)).min(len);
    std::ptr::copy_nonoverlapping(src, dst, head);
    let mut offset = head;
    while offset + 64 <= len {
        let a = _mm_loadu_si128(src.add(offset) as *const __m128i);
        let b = _mm_loadu_si128(src.add(offset + 16) as *const __m128i);
        let c = _mm_loadu_si128(src.add(offset + 32) as *const __m128i);
        let d = _mm_loadu_si128(src.add(offset + 48) as *const __m128i);
        _mm_stream_si128(dst.add(offset) as *mut __m128i, a);
        _mm_stream_si128(dst.add(offset + 16) as *mut __m128i, b);
        _mm_stream_si128(dst.add(offset + 32) as *mut __m128i, c);
        _mm_stream_si128(dst.add(offset + 48) as *mut __m128i, d);
        offset += 64;
    }
    while offset + 16 <= len {
        _mm_stream_si128(dst.add(offset) as *mut __m128i, _mm_loadu_si128(src.add(offset) as *const __m128i));
        offset += 16;
    }
    std::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), len - offset);
}

fn store_fence() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_mm_sfence()
    };
}

unsafe fn lookup_span(indices: &[u8], colors: &[u32; 256], dst: *mut u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return lookup_span_avx2(indices, colors, dst);
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return lookup_span_wasm(indices, colors, dst);

    #[allow(unreachable_code)]
    lookup_span_scalar(indices, colors, dst)
}

unsafe fn lookup_span_scalar(indices: &[u8], colors: &[u32; 256], dst: *mut u8) {
    for (x, &index) in indices.iter().enumerate() {
        (dst.add(x * 4) as *mut u32).write_unaligned(colors[index as usize]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn lookup_span_avx2(indices: &[u8], colors: &[u32; 256], dst: *mut u8) {
    use std::arch::x86_64::*;

    let len = indices.len();
    let mut x = 0;
    while x + 8 <= len {
        _mm256_storeu_si256(dst.add(x * 4) as *mut __m256i, lookup8(colors.as_ptr(), indices.as_ptr().add(x)));
        x += 8;
    }
    lookup_span_scalar(&indices[x..], colors, dst.add(x * 4));
}

unsafe fn compose_span(indices: &[u8], palette: &Palette, dst: *mut u8) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return compose_span_avx2(indices, palette, dst);
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return compose_span_wasm(indices, palette, dst);

    #[allow(unreachable_code)]
    compose_span_scalar(indices, palette, dst)
}

unsafe fn compose_span_scalar(indices: &[u8], palette: &Palette, dst: *mut u8) {
    let colors = &palette.colors;
    for (x, &index) in indices.iter().enumerate() {
        let color = colors[index as usize];
        if color != KEEP {
            (dst.add(x * 4) as *mut u32).write_unaligned(color);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn lookup8(colors: *const u32, src: *const u8) -> std::arch::x86_64::__m256i {
    use std::arch::x86_64::*;

    let indices = _mm256_cvtepu8_epi32(_mm_loadl_epi64(src as *const __m128i));
    _mm256_i32gather_epi32::<4>(colors as *const i32, indices)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn blend8(dst: *mut u8, colors: std::arch::x86_64::__m256i, keep: std::arch::x86_64::__m256i) {
    use std::arch::x86_64::*;

    let mask = _mm256_cmpeq_epi32(colors, keep);
    let previous = _mm256_loadu_si256(dst as *const __m256i);
    _mm256_storeu_si256(dst as *mut __m256i, _mm256_blendv_epi8(colors, previous, mask));
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compose_span_avx2(indices: &[u8], palette: &Palette, dst: *mut u8) {
    use std::arch::x86_64::*;

    let len = indices.len();
    let colors = palette.colors.as_ptr();
    let src = indices.as_ptr();
    let mut x = 0;

    let Some(keep_index) = palette.keep_index else {
        while x + 8 <= len {
            _mm256_storeu_si256(dst.add(x * 4) as *mut __m256i, lookup8(colors, src.add(x)));
            x += 8;
        }
        compose_span_scalar(&indices[x..], palette, dst.add(x * 4));
        return;
    };

    let keep_bytes = _mm256_set1_epi8(keep_index as i8);
    let keep_color = _mm256_set1_epi32(KEEP as i32);
    while x + 32 <= len {
        let raw = _mm256_loadu_si256(src.add(x) as *const __m256i);
        let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(raw, keep_bytes)) as u32;

        if mask == u32::MAX {
            x += 32;
            continue;
        }

        for group in 0..4 {
            let offset = x + group * 8;
            let lanes = (mask >> (group * 8)) & 0xFF;
            if lanes == 0xFF {
                continue;
            }
            let color = lookup8(colors, src.add(offset));
            if lanes == 0 {
                _mm256_storeu_si256(dst.add(offset * 4) as *mut __m256i, color);
            } else {
                blend8(dst.add(offset * 4), color, keep_color);
            }
        }
        x += 32;
    }

    while x + 8 <= len {
        blend8(dst.add(x * 4), lookup8(colors, src.add(x)), keep_color);
        x += 8;
    }
    compose_span_scalar(&indices[x..], palette, dst.add(x * 4));
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline]
unsafe fn lookup4(colors: &[u32; 256], src: *const u8) -> std::arch::wasm32::v128 {
    std::arch::wasm32::u32x4(
        colors[*src as usize],
        colors[*src.add(1) as usize],
        colors[*src.add(2) as usize],
        colors[*src.add(3) as usize],
    )
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn lookup_span_wasm(indices: &[u8], colors: &[u32; 256], dst: *mut u8) {
    use std::arch::wasm32::*;

    let len = indices.len();
    let mut x = 0;
    while x + 4 <= len {
        v128_store(dst.add(x * 4) as *mut v128, lookup4(colors, indices.as_ptr().add(x)));
        x += 4;
    }
    lookup_span_scalar(&indices[x..], colors, dst.add(x * 4));
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn compose_span_wasm(indices: &[u8], palette: &Palette, dst: *mut u8) {
    use std::arch::wasm32::*;

    let Some(keep_index) = palette.keep_index else {
        return lookup_span_wasm(indices, &palette.colors, dst);
    };

    let len = indices.len();
    let colors = &palette.colors;
    let src = indices.as_ptr();
    let keep_bytes = u8x16_splat(keep_index);
    let keep_color = u32x4_splat(KEEP);
    let mut x = 0;

    while x + 16 <= len {
        let keep = u8x16_eq(v128_load(src.add(x) as *const v128), keep_bytes);
        if u8x16_all_true(keep) {
            x += 16;
            continue;
        }

        let mixed = v128_any_true(keep);
        for group in 0..4 {
            let offset = x + group * 4;
            let color = lookup4(colors, src.add(offset));
            let out = dst.add(offset * 4) as *mut v128;
            if mixed {
                v128_store(out, v128_bitselect(v128_load(out), color, u32x4_eq(color, keep_color)));
            } else {
                v128_store(out, color);
            }
        }
        x += 16;
    }

    compose_span_scalar(&indices[x..], palette, dst.add(x * 4));
}
