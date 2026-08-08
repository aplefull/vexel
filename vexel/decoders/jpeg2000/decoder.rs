use std::io::{Read, Seek, SeekFrom};

use crate::decoders::jpeg2000::codestream::{self, Codestream};
use crate::decoders::jpeg2000::simd;
use crate::decoders::jpeg2000::tile::{self, TileDecoder};
use crate::decoders::jpeg2000::types::*;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, PixelData};
use crate::utils::info::{Jpeg2000ComponentInfo, Jpeg2000Info};
use crate::utils::limits::Limits;
use crate::{log_debug, log_warn};

pub struct Jpeg2000Decoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    info: Jpeg2000Info,
}

impl<R: Read + Seek> Jpeg2000Decoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, limits: Limits::default(), info: Jpeg2000Info::default() }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> Jpeg2000Info {
        self.info.clone()
    }

    pub fn precision(&self) -> u8 {
        self.info.components.first().map(|c| c.precision.min(255) as u8).unwrap_or(0)
    }

    pub fn is_signed(&self) -> bool {
        self.info.components.first().map(|c| c.signed).unwrap_or(false)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.reader.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        self.reader.read_to_end(&mut data)?;

        let (codestream_data, container) = if data.starts_with(&JP2_SIGNATURE) {
            let container = codestream::parse_jp2(&data)?;
            (container.codestream.clone(), Some(container))
        } else {
            (data, None)
        };

        let parsed = codestream::parse(&codestream_data)?;

        if parsed.uses_ht && !parsed.defaults.iter().any(|c| c.coding.cblk_style.high_throughput()) {
            log_warn!("JPEG 2000: CAP marker declares HT but no component signals HT block coding");
        }

        self.info = build_info(&parsed);

        let width = parsed.siz.width();
        let height = parsed.siz.height();

        self.limits.check_dimensions(width, height)?;

        if width == 0 || height == 0 {
            return Err(VexelError::InvalidDimensions { width, height });
        }

        let planes = self.decode_all_tiles(&parsed, &codestream_data)?;
        let pixels = self.assemble(&parsed, planes, container.as_ref())?;

        Ok(Image::from_pixels(width, height, pixels))
    }

    fn decode_all_tiles(&mut self, parsed: &Codestream, data: &[u8]) -> VexelResult<Vec<Vec<i64>>> {
        decode_codestream_planes(parsed, data, &mut self.limits)
    }
}

pub fn decode_codestream_planes(parsed: &Codestream, data: &[u8], limits: &mut Limits) -> VexelResult<Vec<Vec<i64>>> {
    let siz = &parsed.siz;
    let num_components = siz.components.len();

    let mut planes: Vec<Vec<i64>> = Vec::with_capacity(num_components);
    for component in &siz.components {
        let (cw, ch) = component_dimensions(siz, component);
        limits.reserve_usize(cw.saturating_mul(ch).saturating_mul(8))?;
        planes.push(vec![0i64; cw * ch]);
    }

    let mut tile_data: Vec<Vec<u8>> = vec![Vec::new(); (siz.num_tiles_x() * siz.num_tiles_y()).max(1) as usize];

    for part in &parsed.tile_parts {
        let start = part.data_start.min(data.len());
        let end = (start + part.data_len).min(data.len());

        if let Some(slot) = tile_data.get_mut(part.tile_index as usize) {
            slot.extend_from_slice(&data[start..end]);
        }
    }

    let mut decoded_any = false;
    let mut ppm_cursor = 0usize;

    let occupied = tile_data.iter().filter(|bytes| !bytes.is_empty()).count();
    let sequential_headers = !parsed.ppm_data.is_empty();

    if can_decode_tiles_in_parallel(occupied, sequential_headers) {
        return decode_tiles_parallel(parsed, &tile_data, planes, limits);
    }

    for (tile_index, bytes) in tile_data.iter().enumerate() {
        if bytes.is_empty() {
            continue;
        }

        let mut decoder = TileDecoder::new(parsed, limits, &mut ppm_cursor);

        match decoder.decode_tile(tile_index as u32, bytes) {
            Ok(decoded) => {
                tile::log_tile_summary(tile_index as u32, &decoded);
                compose_tile(&mut planes, &decoded, siz);
                decoded_any = true;
            }
            Err(error) => {
                log_warn!("JPEG 2000: failed to decode tile {}: {}", tile_index, error);
            }
        }
    }

    if !decoded_any {
        return Err(VexelError::Custom("JPEG 2000: no tile could be decoded".to_string()));
    }

    Ok(planes)
}

#[cfg(feature = "rayon")]
fn can_decode_tiles_in_parallel(occupied_tiles: usize, sequential_headers: bool) -> bool {
    occupied_tiles >= PARALLEL_TILE_THRESHOLD && !sequential_headers
}

#[cfg(not(feature = "rayon"))]
fn can_decode_tiles_in_parallel(_occupied_tiles: usize, _sequential_headers: bool) -> bool {
    false
}

#[cfg(feature = "rayon")]
fn decode_tiles_parallel(
    parsed: &Codestream,
    tile_data: &[Vec<u8>],
    mut planes: Vec<Vec<i64>>,
    limits: &mut Limits,
) -> VexelResult<Vec<Vec<i64>>> {
    use rayon::prelude::*;

    let siz = &parsed.siz;
    let budget = limits.clone();

    let decoded: Vec<(usize, VexelResult<tile::DecodedTile>)> = tile_data
        .par_iter()
        .enumerate()
        .filter(|(_, bytes)| !bytes.is_empty())
        .map(|(tile_index, bytes)| {
            let mut limits = budget.clone();
            let mut ppm_cursor = 0usize;
            let mut decoder = TileDecoder::new(parsed, &mut limits, &mut ppm_cursor);

            let result = tile::with_parallel_tile(|| decoder.decode_tile(tile_index as u32, bytes));

            (tile_index, result)
        })
        .collect();

    let mut decoded_any = false;

    for (tile_index, result) in decoded {
        match result {
            Ok(tile) => {
                tile::log_tile_summary(tile_index as u32, &tile);

                let footprint: usize = tile.components.iter().map(|c| c.len() * 8).sum();
                limits.reserve_usize(footprint)?;

                compose_tile(&mut planes, &tile, siz);
                decoded_any = true;

                limits.free_usize(footprint);
            }
            Err(error) => {
                log_warn!("JPEG 2000: failed to decode tile {}: {}", tile_index, error);
            }
        }
    }

    if !decoded_any {
        return Err(VexelError::Custom("JPEG 2000: no tile could be decoded".to_string()));
    }

    Ok(planes)
}

impl<R: Read + Seek> Jpeg2000Decoder<R> {
    fn assemble(
        &mut self,
        parsed: &Codestream,
        mut planes: Vec<Vec<i64>>,
        container: Option<&codestream::Jp2Container>,
    ) -> VexelResult<PixelData> {
        let siz = &parsed.siz;
        let width = siz.width() as usize;
        let height = siz.height() as usize;

        let mut precisions = Vec::with_capacity(siz.components.len());
        for (index, component) in siz.components.iter().enumerate() {
            let precision = component.prec.clamp(1, 32);
            precisions.push(precision);

            let shift = if component.signed { 0 } else { 1i64 << (precision - 1) };
            let (min, max) = if component.signed {
                (-(1i64 << (precision - 1)), (1i64 << (precision - 1)) - 1)
            } else {
                (0, (1i64 << precision) - 1)
            };

            if let Some(plane) = planes.get_mut(index) {
                simd::shift_and_clamp(plane, shift, min, max);

                if component.signed {
                    let bias = 1i64 << (precision - 1);
                    simd::shift_and_clamp(plane, bias, i64::MIN, i64::MAX);
                }
            }
        }

        let mut upsampled = self.upsample(siz, planes, width, height)?;

        if let Some(palette) = container.and_then(|c| c.palette.as_ref()) {
            let source_precision = precisions.first().copied().unwrap_or(8);
            if let Some((expanded, expanded_precisions)) = apply_palette(palette, &upsampled, source_precision) {
                upsampled = expanded;
                precisions = expanded_precisions;
            }
        }

        let channel_kinds = apply_channel_definitions(container, &mut upsampled, &mut precisions);

        let colorspace = resolve_colorspace(container, siz, upsampled.len());
        let signed: Vec<bool> = siz.components.iter().map(|c| c.signed).collect();
        let component_precision = precisions.first().copied().unwrap_or(8).clamp(1, 32);

        match colorspace {
            Some(Colorspace::Sycc) => convert_sycc_to_rgb(&mut upsampled, component_precision),
            Some(Colorspace::Eycc) => convert_esycc_to_rgb(&mut upsampled, component_precision, &signed),
            Some(Colorspace::Cmyk) => convert_cmyk_to_rgb(&mut upsampled, &mut precisions),
            Some(Colorspace::Cielab) => {
                let range = container.and_then(|container| container.cielab_range);
                convert_cielab_to_rgb(&mut upsampled, &mut precisions, range);
            }
            _ => {}
        }

        pack_pixels(upsampled, width, height, &precisions, &channel_kinds)
    }

    fn upsample(
        &mut self,
        siz: &SizInfo,
        planes: Vec<Vec<i64>>,
        width: usize,
        height: usize,
    ) -> VexelResult<Vec<Vec<i64>>> {
        let mut result = Vec::with_capacity(planes.len());

        for (index, plane) in planes.into_iter().enumerate() {
            let component = match siz.components.get(index) {
                Some(component) => component,
                None => continue,
            };

            if component.dx == 1 && component.dy == 1 {
                result.push(plane);
                continue;
            }

            let (cw, ch) = component_dimensions(siz, component);

            self.limits.reserve_usize(width.saturating_mul(height).saturating_mul(8))?;
            let mut expanded = vec![0i64; width * height];

            if cw == 0 || ch == 0 {
                result.push(expanded);
                continue;
            }

            let mut previous_sy = usize::MAX;

            for y in 0..height {
                let sy = (y / component.dy as usize).min(ch - 1);
                let (before, current) = expanded.split_at_mut(y * width);

                let row = &mut current[..width];

                if sy == previous_sy {
                    row.copy_from_slice(&before[(y - 1) * width..y * width]);
                    continue;
                }

                let source = &plane[sy * cw..(sy * cw + cw).min(plane.len())];
                simd::replicate_row(source, row, component.dx as usize);

                previous_sy = sy;
            }

            result.push(expanded);
        }

        Ok(result)
    }
}

fn resolve_colorspace(
    container: Option<&codestream::Jp2Container>,
    siz: &SizInfo,
    num_planes: usize,
) -> Option<Colorspace> {
    let declared = container.and_then(|c| c.enumerated_colorspace);

    let looks_like_sycc = siz.components.len() == 3
        && siz
            .components
            .first()
            .zip(siz.components.get(1))
            .is_some_and(|(luma, chroma)| luma.dx == luma.dy && chroma.dx != 1);

    let space = if declared != Some(Colorspace::Sycc) && looks_like_sycc {
        Some(Colorspace::Sycc)
    } else if num_planes <= 2 {
        Some(Colorspace::Gray)
    } else {
        declared.or(Some(Colorspace::Srgb))
    };

    if space == Some(Colorspace::Sycc) && !is_convertible_ycc_layout(siz) {
        log_warn!("JPEG 2000: sYCC image has an unsupported chroma layout; leaving samples unconverted");
        return None;
    }

    space
}

fn is_convertible_ycc_layout(siz: &SizInfo) -> bool {
    let (Some(y), Some(cb), Some(cr)) = (siz.components.first(), siz.components.get(1), siz.components.get(2)) else {
        return false;
    };

    if y.dx != 1 || y.dy != 1 {
        return false;
    }

    let chroma = (cb.dx, cb.dy, cr.dx, cr.dy);

    matches!(chroma, (1, 1, 1, 1) | (2, 1, 2, 1) | (2, 2, 2, 2))
}

fn sycc_to_rgb(offset: i64, upb: i64, y: i64, cb: i64, cr: i64) -> (i64, i64, i64) {
    let cb = cb - offset;
    let cr = cr - offset;

    let r = y + (1.402 * cr as f32) as i64;
    let g = y - (0.344 * cb as f32 + 0.714 * cr as f32) as i64;
    let b = y + (1.772 * cb as f32) as i64;

    (r.clamp(0, upb), g.clamp(0, upb), b.clamp(0, upb))
}

#[allow(clippy::needless_range_loop)]
fn convert_sycc_to_rgb(planes: &mut [Vec<i64>], precision: u32) {
    if planes.len() < 3 {
        return;
    }

    let offset = 1i64 << (precision - 1);
    let upb = (1i64 << precision) - 1;
    let len = planes[0].len().min(planes[1].len()).min(planes[2].len());

    for i in 0..len {
        let (r, g, b) = sycc_to_rgb(offset, upb, planes[0][i], planes[1][i], planes[2][i]);
        planes[0][i] = r;
        planes[1][i] = g;
        planes[2][i] = b;
    }
}

#[allow(clippy::needless_range_loop)]
fn convert_esycc_to_rgb(planes: &mut [Vec<i64>], precision: u32, signed: &[bool]) {
    if planes.len() < 3 {
        return;
    }

    let flip = 1i64 << (precision - 1);
    let max_value = (1i64 << precision) - 1;
    let len = planes[0].len().min(planes[1].len()).min(planes[2].len());

    for i in 0..len {
        let y = planes[0][i] as f32;
        let mut cb = planes[1][i];
        let mut cr = planes[2][i];

        if !signed.get(1).copied().unwrap_or(false) {
            cb -= flip;
        }
        if !signed.get(2).copied().unwrap_or(false) {
            cr -= flip;
        }

        let (cb, cr) = (cb as f32, cr as f32);

        planes[0][i] = ((y - 0.000_036_8 * cb + 1.401_99 * cr + 0.5) as i64).clamp(0, max_value);
        planes[1][i] = ((1.0003 * y - 0.344_125 * cb - 0.714_112_8 * cr + 0.5) as i64).clamp(0, max_value);
        planes[2][i] = ((0.999_823 * y + 1.772_04 * cb - 0.000_008 * cr + 0.5) as i64).clamp(0, max_value);
    }
}

#[allow(clippy::needless_range_loop)]
fn convert_cmyk_to_rgb(planes: &mut Vec<Vec<i64>>, precisions: &mut Vec<u32>) {
    if planes.len() < 4 {
        return;
    }

    let scale = |index: usize| -> f32 {
        let precision = precisions.get(index).copied().unwrap_or(8).clamp(1, 32);
        1.0 / ((1u64 << precision) - 1) as f32
    };

    let (sc, sm, sy, sk) = (scale(0), scale(1), scale(2), scale(3));
    let len = planes.iter().take(4).map(|p| p.len()).min().unwrap_or(0);

    for i in 0..len {
        let c = 1.0 - planes[0][i] as f32 * sc;
        let m = 1.0 - planes[1][i] as f32 * sm;
        let y = 1.0 - planes[2][i] as f32 * sy;
        let k = 1.0 - planes[3][i] as f32 * sk;

        planes[0][i] = (255.0 * c * k) as i64;
        planes[1][i] = (255.0 * m * k) as i64;
        planes[2][i] = (255.0 * y * k) as i64;
    }

    planes.truncate(3);
    precisions.truncate(3);
    precisions.iter_mut().for_each(|p| *p = 8);
}

fn lab_to_srgb_channel(value: f64) -> f64 {
    if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn convert_cielab_to_rgb(planes: &mut [Vec<i64>], precisions: &mut [u32], range: Option<codestream::CielabRange>) {
    if planes.len() < 3 {
        return;
    }

    let precision_of = |index: usize| -> f64 {
        let precision = precisions.get(index).copied().unwrap_or(8).clamp(1, 32);
        ((1u64 << precision) - 1) as f64
    };

    let (max_l, max_a, max_b) = (precision_of(0), precision_of(1), precision_of(2));

    let (range_l, offset_l, range_a, offset_a, range_b, offset_b) = match range {
        Some(range) => (
            range.range_l as f64,
            range.offset_l as f64,
            range.range_a as f64,
            range.offset_a as f64,
            range.range_b as f64,
            range.offset_b as f64,
        ),
        None => {
            let precision_a = precisions.get(1).copied().unwrap_or(8).clamp(1, 32);
            let precision_b = precisions.get(2).copied().unwrap_or(8).clamp(1, 32);
            (
                100.0,
                0.0,
                170.0,
                (1u64 << (precision_a - 1)) as f64,
                200.0,
                (1u64 << (precision_b - 2)) as f64 + (1u64 << (precision_b - 3)) as f64,
            )
        }
    };

    let min_l = -(range_l * offset_l) / max_l;
    let min_a = -(range_a * offset_a) / max_a;
    let min_b = -(range_b * offset_b) / max_b;

    const WHITE_X: f64 = 0.9642;
    const WHITE_Y: f64 = 1.0;
    const WHITE_Z: f64 = 0.8249;

    let len = planes.iter().take(3).map(|plane| plane.len()).min().unwrap_or(0);

    let (first, rest) = planes.split_at_mut(1);
    let (second, third) = rest.split_at_mut(1);
    let plane_l = &mut first[0][..len];
    let plane_a = &mut second[0][..len];
    let plane_b = &mut third[0][..len];

    for ((sample_l, sample_a), sample_b) in plane_l.iter_mut().zip(plane_a.iter_mut()).zip(plane_b.iter_mut()) {
        let l = min_l + *sample_l as f64 * range_l / max_l;
        let a = min_a + *sample_a as f64 * range_a / max_a;
        let b = min_b + *sample_b as f64 * range_b / max_b;

        let fy = (l + 16.0) / 116.0;
        let fx = fy + a / 500.0;
        let fz = fy - b / 200.0;

        let finv = |value: f64| -> f64 {
            if value > 6.0 / 29.0 {
                value * value * value
            } else {
                3.0 * (6.0f64 / 29.0).powi(2) * (value - 4.0 / 29.0)
            }
        };

        let x = WHITE_X * finv(fx);
        let y = WHITE_Y * finv(fy);
        let z = WHITE_Z * finv(fz);

        let red = 3.1338561 * x - 1.6168667 * y - 0.4906146 * z;
        let green = -0.9787684 * x + 1.9161415 * y + 0.0334540 * z;
        let blue = 0.0719453 * x - 0.2289914 * y + 1.4052427 * z;

        *sample_l = (lab_to_srgb_channel(red).clamp(0.0, 1.0) * 65535.0).round() as i64;
        *sample_a = (lab_to_srgb_channel(green).clamp(0.0, 1.0) * 65535.0).round() as i64;
        *sample_b = (lab_to_srgb_channel(blue).clamp(0.0, 1.0) * 65535.0).round() as i64;
    }

    precisions.iter_mut().take(3).for_each(|precision| *precision = 16);
}

pub fn component_dimensions(siz: &SizInfo, component: &ComponentInfo) -> (usize, usize) {
    let x0 = siz.x0siz.div_ceil(component.dx);
    let y0 = siz.y0siz.div_ceil(component.dy);
    let x1 = siz.xsiz.div_ceil(component.dx);
    let y1 = siz.ysiz.div_ceil(component.dy);

    (x1.saturating_sub(x0) as usize, y1.saturating_sub(y0) as usize)
}

fn compose_tile(planes: &mut [Vec<i64>], tile: &tile::DecodedTile, siz: &SizInfo) {
    for (index, samples) in tile.components.iter().enumerate() {
        let component = match siz.components.get(index) {
            Some(component) => component,
            None => continue,
        };

        let plane = match planes.get_mut(index) {
            Some(plane) => plane,
            None => continue,
        };

        let (plane_w, plane_h) = component_dimensions(siz, component);

        let dx = component.dx as i32;
        let dy = component.dy as i32;

        let tcx0 = ceil_div_i32(tile.x0, dx);
        let tcy0 = ceil_div_i32(tile.y0, dy);
        let tcx1 = ceil_div_i32(tile.x1, dx);
        let tcy1 = ceil_div_i32(tile.y1, dy);

        let tile_w = (tcx1 - tcx0).max(0) as usize;
        let tile_h = (tcy1 - tcy0).max(0) as usize;

        let origin_x = siz.x0siz.div_ceil(component.dx) as i32;
        let origin_y = siz.y0siz.div_ceil(component.dy) as i32;

        let base_x = (tcx0 - origin_x) as isize;
        let base_y = (tcy0 - origin_y) as isize;

        let x_start = (-base_x).max(0) as usize;
        let x_end = tile_w.min((plane_w as isize - base_x).max(0) as usize);

        if x_start >= x_end {
            continue;
        }

        for y in 0..tile_h {
            let dest_y = base_y + y as isize;
            if dest_y < 0 || dest_y as usize >= plane_h {
                continue;
            }

            let source_start = y * tile_w + x_start;
            let source_end = source_start + (x_end - x_start);

            let dest_start = dest_y as usize * plane_w + (base_x + x_start as isize) as usize;
            let dest_end = dest_start + (x_end - x_start);

            let Some(dest) = plane.get_mut(dest_start..dest_end) else {
                continue;
            };

            match samples {
                tile::TilePlane::Exact(samples) => {
                    let Some(source) = samples.get(source_start..source_end) else {
                        continue;
                    };

                    dest.copy_from_slice(source);
                }
                tile::TilePlane::Approximate(samples) => {
                    let Some(source) = samples.get(source_start..source_end) else {
                        continue;
                    };

                    simd::round_half_to_even_row(source, dest);
                }
            }
        }
    }
}

fn maps_every_column(mapping: &[codestream::ComponentMapping], num_columns: usize) -> bool {
    let mut used = vec![false; num_columns];

    for entry in mapping {
        if entry.kind == codestream::MappingType::Palette {
            if let Some(slot) = used.get_mut(entry.palette_column as usize) {
                *slot = true;
            }
        }
    }

    used.iter().all(|column| *column)
}

fn apply_palette(
    palette: &codestream::Palette,
    planes: &[Vec<i64>],
    source_precision: u32,
) -> Option<(Vec<Vec<i64>>, Vec<u32>)> {
    if palette.entries.is_empty() {
        return None;
    }

    let num_columns = palette.entries[0].len();
    if num_columns == 0 {
        return None;
    }

    let identity_mapping = || -> Vec<codestream::ComponentMapping> {
        (0..num_columns)
            .map(|i| codestream::ComponentMapping {
                component: 0,
                kind: codestream::MappingType::Palette,
                palette_column: i as u8,
            })
            .collect()
    };

    let mapping: Vec<codestream::ComponentMapping> = if palette.component_mapping.is_empty() {
        identity_mapping()
    } else if planes.len() == 1 && !maps_every_column(&palette.component_mapping, num_columns) {
        log_warn!("JPEG 2000: component mapping does not cover the palette; using direct column order");
        identity_mapping()
    } else {
        palette.component_mapping.clone()
    };

    if mapping.is_empty() {
        return None;
    }

    let top = palette.entries.len() as i64 - 1;

    let mut channels: Vec<(Vec<i64>, u32, bool)> = Vec::with_capacity(mapping.len());

    for entry in &mapping {
        let source = planes.get(entry.component as usize)?;

        if entry.kind == codestream::MappingType::Direct {
            channels.push((source.clone(), source_precision, false));
            continue;
        }

        let column = entry.palette_column as usize;
        if column >= num_columns {
            return None;
        }

        let depth = palette.column_depths.get(column).copied().unwrap_or(8).clamp(1, 32);
        let signed = palette.column_signed.get(column).copied().unwrap_or(false);

        let mut channel = Vec::with_capacity(source.len());
        for &index in source.iter() {
            let entry = index.clamp(0, top) as usize;
            channel.push(palette.entries[entry][column] as i64);
        }

        channels.push((channel, depth, signed));
    }

    let precisions: Vec<u32> = channels.iter().map(|(_, depth, _)| *depth).collect();

    let expanded = channels
        .into_iter()
        .map(|(mut samples, depth, signed)| {
            if signed {
                let shift = 1i64 << (depth - 1);
                for sample in samples.iter_mut() {
                    *sample += shift;
                }
            }

            let max = (1i64 << depth) - 1;
            for sample in samples.iter_mut() {
                *sample = (*sample).clamp(0, max);
            }

            samples
        })
        .collect();

    Some((expanded, precisions))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelKind {
    Color,
    Alpha,
}

#[derive(Clone, Copy)]
struct Channel<'a> {
    samples: &'a [i64],
    plane_index: usize,
}

fn apply_channel_definitions(
    container: Option<&codestream::Jp2Container>,
    planes: &mut [Vec<i64>],
    precisions: &mut [u32],
) -> Vec<ChannelKind> {
    let mut kinds = vec![ChannelKind::Color; planes.len()];

    let container = match container {
        Some(container) => container,
        None => return kinds,
    };

    let mut definitions = container.channel_definitions.clone();

    for index in 0..definitions.len() {
        let definition = definitions[index];
        let cn = definition.channel as usize;

        if cn >= planes.len() {
            log_warn!("JPEG 2000: cdef channel {} is out of range", definition.channel);
            continue;
        }

        let is_alpha = definition.kind.is_alpha();

        if definition.association == 0 || definition.association == u16::MAX {
            if is_alpha {
                kinds[cn] = ChannelKind::Alpha;
            }
            continue;
        }

        let acn = (definition.association - 1) as usize;
        if acn >= planes.len() {
            log_warn!("JPEG 2000: cdef association {} is out of range", definition.association);
            continue;
        }

        if cn != acn && definition.kind == codestream::ChannelType::Color {
            planes.swap(cn, acn);
            kinds.swap(cn, acn);
            if cn < precisions.len() && acn < precisions.len() {
                precisions.swap(cn, acn);
            }

            for later in definitions.iter_mut().skip(index + 1) {
                if later.channel as usize == cn {
                    later.channel = acn as u16;
                } else if later.channel as usize == acn {
                    later.channel = cn as u16;
                }
            }
        }

        if is_alpha {
            kinds[cn] = ChannelKind::Alpha;
        }
    }

    kinds
}

fn pack_pixels(
    planes: Vec<Vec<i64>>,
    width: usize,
    height: usize,
    precisions: &[u32],
    kinds: &[ChannelKind],
) -> VexelResult<PixelData> {
    if planes.is_empty() {
        return Err(VexelError::Custom("JPEG 2000: image has no components".to_string()));
    }

    let num_color = kinds.iter().filter(|k| **k == ChannelKind::Color).count();
    let has_alpha = kinds.contains(&ChannelKind::Alpha);

    let color_indices: Vec<usize> = (0..planes.len())
        .filter(|&i| kinds.get(i).copied().unwrap_or(ChannelKind::Color) == ChannelKind::Color)
        .collect();
    let alpha_index = (0..planes.len()).find(|&i| kinds.get(i).copied() == Some(ChannelKind::Alpha));

    let pixel_count = width * height;
    let precision = precisions.iter().copied().max().unwrap_or(8).clamp(1, 32);
    let use_16bit = precision > 8;

    let scale = |value: i64, precision: u32| -> u32 {
        let precision = precision.clamp(1, 32);
        let max_in = if precision >= 32 { u32::MAX } else { (1u32 << precision) - 1 };
        let value = value.clamp(0, max_in as i64) as u32;
        let target = if use_16bit { 16 } else { 8 };

        if precision >= target {
            value >> (precision - target)
        } else {
            let mut result = 0u32;
            let mut filled = 0u32;
            while filled < target {
                let room = target - filled;
                result = if room >= precision {
                    (result << precision) | value
                } else {
                    (result << room) | (value >> (precision - room))
                };
                filled += precision.min(room);
            }
            result
        }
        .min(if use_16bit { u16::MAX as u32 } else { u8::MAX as u32 })
    };

    let precision_of = |index: usize| -> u32 { precisions.get(index).copied().unwrap_or(precision) };

    let sample = |channel: Option<Channel<'_>>, index: usize| -> u32 {
        match channel {
            Some(channel) => {
                channel.samples.get(index).copied().map(|v| scale(v, precision_of(channel.plane_index))).unwrap_or(0)
            }
            None => 0,
        }
    };

    let full = if use_16bit { u16::MAX as u32 } else { u8::MAX as u32 };

    let channel = |slot: Option<&usize>| -> Option<Channel<'_>> {
        let &plane_index = slot?;
        planes.get(plane_index).map(|samples| Channel { samples, plane_index })
    };

    let gray = channel(color_indices.first());
    let red = gray;
    let green = channel(color_indices.get(1)).or(red);
    let blue = channel(color_indices.get(2)).or(red);
    let alpha = channel(alpha_index.as_ref());

    let alpha_or_full = |index: usize| -> u32 { alpha.map(|_| sample(alpha, index)).unwrap_or(full) };

    match (num_color, has_alpha, use_16bit) {
        (1, false, false) => {
            let out = fill_pixels(pixel_count, 1, |i, pixel| pixel[0] = sample(gray, i) as u8);
            Ok(PixelData::L8(out))
        }
        (1, false, true) => {
            let out = fill_pixels(pixel_count, 1, |i, pixel| pixel[0] = sample(gray, i) as u16);
            Ok(PixelData::L16(out))
        }
        (1, true, false) => {
            let out = fill_pixels(pixel_count, 2, |i, pixel| {
                pixel[0] = sample(gray, i) as u8;
                pixel[1] = alpha_or_full(i) as u8;
            });
            Ok(PixelData::LA8(out))
        }
        (1, true, true) => {
            let out = fill_pixels(pixel_count, 2, |i, pixel| {
                pixel[0] = sample(gray, i) as u16;
                pixel[1] = alpha_or_full(i) as u16;
            });
            Ok(PixelData::LA16(out))
        }
        (_, false, false) => {
            let out = fill_pixels(pixel_count, 3, |i, pixel| {
                pixel[0] = sample(red, i) as u8;
                pixel[1] = sample(green, i) as u8;
                pixel[2] = sample(blue, i) as u8;
            });
            Ok(PixelData::RGB8(out))
        }
        (_, false, true) => {
            let out = fill_pixels(pixel_count, 3, |i, pixel| {
                pixel[0] = sample(red, i) as u16;
                pixel[1] = sample(green, i) as u16;
                pixel[2] = sample(blue, i) as u16;
            });
            Ok(PixelData::RGB16(out))
        }
        (_, true, false) => {
            let out = fill_pixels(pixel_count, 4, |i, pixel| {
                pixel[0] = sample(red, i) as u8;
                pixel[1] = sample(green, i) as u8;
                pixel[2] = sample(blue, i) as u8;
                pixel[3] = alpha_or_full(i) as u8;
            });
            Ok(PixelData::RGBA8(out))
        }
        (_, true, true) => {
            let out = fill_pixels(pixel_count, 4, |i, pixel| {
                pixel[0] = sample(red, i) as u16;
                pixel[1] = sample(green, i) as u16;
                pixel[2] = sample(blue, i) as u16;
                pixel[3] = alpha_or_full(i) as u16;
            });
            Ok(PixelData::RGBA16(out))
        }
    }
}

const PARALLEL_PIXEL_THRESHOLD: usize = 1 << 16;
const PARALLEL_TILE_THRESHOLD: usize = 4;

fn fill_pixels<T, F>(pixel_count: usize, channels: usize, write: F) -> Vec<T>
where
    T: Default + Clone + Send,
    F: Fn(usize, &mut [T]) + Sync + Send,
{
    let mut out = vec![T::default(); pixel_count * channels];

    #[cfg(feature = "rayon")]
    if pixel_count >= PARALLEL_PIXEL_THRESHOLD {
        use rayon::prelude::*;

        out.par_chunks_mut(channels).enumerate().for_each(|(i, pixel)| write(i, pixel));

        return out;
    }

    for (i, pixel) in out.chunks_mut(channels).enumerate() {
        write(i, pixel);
    }

    out
}

fn build_info(parsed: &Codestream) -> Jpeg2000Info {
    let siz = &parsed.siz;
    let default = parsed.defaults.first().cloned().unwrap_or_default();

    log_debug!(
        "JPEG 2000: {}x{}, {} components, {} tiles",
        siz.width(),
        siz.height(),
        siz.components.len(),
        siz.num_tiles_x() * siz.num_tiles_y()
    );

    Jpeg2000Info {
        width: siz.width(),
        height: siz.height(),
        num_components: siz.components.len() as u32,
        tile_width: siz.xtsiz,
        tile_height: siz.ytsiz,
        num_tiles: siz.num_tiles_x() * siz.num_tiles_y(),
        num_layers: parsed.num_layers,
        num_resolutions: default.coding.num_resolutions,
        progression_order: format!("{:?}", parsed.progression),
        reversible: default.coding.reversible,
        multiple_component_transform: parsed.mct,
        components: siz
            .components
            .iter()
            .map(|component| Jpeg2000ComponentInfo {
                precision: component.prec,
                signed: component.signed,
                dx: component.dx,
                dy: component.dy,
            })
            .collect(),
    }
}
