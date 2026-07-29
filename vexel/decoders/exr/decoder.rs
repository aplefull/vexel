use std::io::{Read, Seek, SeekFrom};

use crate::decoders::exr::compression::{b44_decompress, pxr24_reconstruct, rle_decompress, unpredict_and_reorder};
use crate::decoders::exr::deep::{self, DeepChunkPixels};
use crate::decoders::exr::parser::{self, ByteCursor, ExrFile, ExrPart};
use crate::decoders::exr::piz::piz_decompress;
use crate::decoders::exr::types::{
    Box2i, ChunkChannel, ExrChannelInfo, ExrCompression, ExrPartInfo, ExrPixelType, PartHeader,
};
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, PixelData};
use crate::utils::info::ExrInfo;
use crate::utils::tiff::color::half_to_f32;
use crate::{log_debug, log_warn, Limits};

const DEFAULT_LUMINANCE_WEIGHTS: [f32; 3] = [0.212639, 0.715169, 0.072192];

pub struct ExrDecoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    data: Vec<u8>,
    file: Option<ExrFile>,
    info: ExrInfo,
}

struct Plane {
    width: usize,
    height: usize,
    samples: Vec<f32>,
}

struct ChunkLayout {
    channels: Vec<ChunkChannel>,
    rows: Vec<(i64, Vec<bool>)>,
    unpacked_size: usize,
}

fn num_samples(sampling: i64, min: i64, max: i64) -> i64 {
    if sampling <= 1 {
        return (max - min + 1).max(0);
    }
    let mut first = min.div_euclid(sampling) * sampling;
    if first < min {
        first += sampling;
    }
    if first > max {
        0
    } else {
        (max - first) / sampling + 1
    }
}

fn sample_index(v: i64, min: i64, sampling: i64) -> i64 {
    if sampling <= 1 {
        return v - min;
    }
    let mut first = min.div_euclid(sampling) * sampling;
    if first < min {
        first += sampling;
    }
    v.div_euclid(sampling) - first.div_euclid(sampling)
}

fn is_sampled(v: i64, sampling: i64) -> bool {
    sampling <= 1 || v.rem_euclid(sampling) == 0
}

impl<R: Read + Seek> ExrDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            limits: Limits::default(),
            data: Vec::new(),
            file: None,
            info: ExrInfo { version: 0, multipart: false, parts: Vec::new() },
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> ExrInfo {
        self.info.clone()
    }

    fn load(&mut self) -> VexelResult<()> {
        if self.file.is_some() {
            return Ok(());
        }

        self.reader.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        self.reader.read_to_end(&mut data)?;
        self.data = data;

        let file = parser::parse(&self.data)?;

        self.info = ExrInfo {
            version: file.version,
            multipart: file.multipart,
            parts: file.parts.iter().map(build_part_info).collect(),
        };

        self.file = Some(file);
        Ok(())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.load()?;

        let file = self.file.as_ref().ok_or_else(|| VexelError::Custom("EXR: file not loaded".to_string()))?;

        if file.parts.is_empty() {
            return Err(VexelError::Custom("EXR: file has no parts".to_string()));
        }

        let mut deep_seen = false;
        let mut candidate = None;
        for (i, part) in file.parts.iter().enumerate() {
            if part.header.kind.is_deep() {
                deep_seen = true;
                continue;
            }
            candidate = Some(i);
            break;
        }

        match candidate {
            Some(index) => {
                if deep_seen {
                    log_debug!("EXR: file contains both deep and flat parts, decoding the first flat image part");
                }
                self.decode_part(index)
            }
            None => self.decode_part(0),
        }
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        self.load()?;

        let part_count = self.file.as_ref().map(|f| f.parts.len()).unwrap_or(0);
        let mut pages = Vec::new();
        let mut last_error = None;

        for i in 0..part_count {
            match self.decode_part(i) {
                Ok(image) => pages.push(image),
                Err(e) => {
                    log_warn!("EXR: failed to decode part {}: {}", i, e);
                    last_error = Some(e);
                }
            }
        }

        if pages.is_empty() {
            return Err(last_error
                .unwrap_or_else(|| VexelError::UnsupportedFormat("EXR: no decodable image parts found".to_string())));
        }

        Ok(pages)
    }

    fn decode_part(&mut self, part_index: usize) -> VexelResult<Image> {
        let file = self.file.as_ref().ok_or_else(|| VexelError::Custom("EXR: file not loaded".to_string()))?;
        let part = &file.parts[part_index];
        let header = part.header.clone();
        let multipart = file.multipart;
        let chunk_offsets = part.chunk_offsets.clone();

        if !header.compression.is_supported() {
            return Err(VexelError::UnsupportedFormat(format!(
                "EXR: {} compression is not supported",
                header.compression.name()
            )));
        }

        if header.channels.is_empty() {
            return Err(VexelError::Custom("EXR: part has no channels".to_string()));
        }

        let dw = header.data_window;
        let width = dw.width();
        let height = dw.height();

        if width <= 0 || height <= 0 || width > i32::MAX as i64 || height > i32::MAX as i64 {
            return Err(VexelError::InvalidDimensions {
                width: width.clamp(0, u32::MAX as i64) as u32,
                height: height.clamp(0, u32::MAX as i64) as u32,
            });
        }

        self.limits.check_dimensions(width as u32, height as u32)?;

        let mut header = header;
        if (header.kind.is_tiled() || header.kind.is_deep())
            && header.channels.iter().any(|c| c.x_sampling != 1 || c.y_sampling != 1)
        {
            log_warn!(
                "EXR: {} part declares subsampled channels, treating all channels as unsampled",
                header.kind.name()
            );
            for channel in header.channels.iter_mut() {
                channel.x_sampling = 1;
                channel.y_sampling = 1;
            }
        }

        let dwx = header.display_window;
        if dwx.x_min != dw.x_min || dwx.y_min != dw.y_min || dwx.x_max != dw.x_max || dwx.y_max != dw.y_max {
            log_debug!("EXR: display window differs from data window, decoding the data window");
        }

        let mut planes = Vec::with_capacity(header.channels.len());
        let mut reserved_bytes = 0usize;
        for channel in &header.channels {
            let pw = num_samples(channel.x_sampling as i64, dw.x_min as i64, dw.x_max as i64) as usize;
            let ph = num_samples(channel.y_sampling as i64, dw.y_min as i64, dw.y_max as i64) as usize;
            let bytes = pw
                .checked_mul(ph)
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| VexelError::Custom("EXR: channel plane size overflow".to_string()))?;
            self.limits.reserve_usize(bytes)?;
            reserved_bytes += bytes;
            planes.push(Plane { width: pw, height: ph, samples: vec![0f32; pw * ph] });
        }

        let result = self.decode_chunks(&header, part_index, multipart, &chunk_offsets, &mut planes);

        if let Err(e) = result {
            log_warn!("EXR: chunk decoding stopped early: {}", e);
        }

        let image = build_image(&header, planes, width as u32, height as u32);
        self.limits.free_usize(reserved_bytes);
        image
    }

    fn decode_chunks(
        &mut self,
        header: &PartHeader,
        part_index: usize,
        multipart: bool,
        chunk_offsets: &[u64],
        planes: &mut [Plane],
    ) -> VexelResult<()> {
        let data = &self.data;
        let limits = &mut self.limits;

        let dw = header.data_window;
        let tiled = header.kind.is_tiled();
        let deep = header.kind.is_deep();
        let lines_per_block = header.compression.lines_per_block() as i64;
        let (alpha_index, z_index) = if deep { deep_composite_channels(header) } else { (None, None) };

        for (chunk_index, &offset) in chunk_offsets.iter().enumerate() {
            if offset < 8 || offset as usize >= data.len() {
                log_warn!("EXR: chunk {} has invalid offset {}, skipping", chunk_index, offset);
                continue;
            }

            let mut cursor = ByteCursor::new(&data[offset as usize..]);

            if multipart {
                let part_number = match cursor.read_i32() {
                    Ok(v) => v,
                    Err(_) => {
                        log_warn!("EXR: chunk {} header truncated, skipping", chunk_index);
                        continue;
                    }
                };
                if part_number != part_index as i32 {
                    log_warn!(
                        "EXR: chunk {} belongs to part {} but was referenced by part {}, skipping",
                        chunk_index,
                        part_number,
                        part_index
                    );
                    continue;
                }
            }

            if deep {
                match deep::decode_deep_chunk(limits, header, &mut cursor, alpha_index, z_index) {
                    Ok(Some(pixels)) => scatter_deep_chunk(&pixels, planes, dw),
                    Ok(None) => {}
                    Err(e) => log_warn!("EXR: failed to decode deep chunk {}: {}", chunk_index, e),
                }
                continue;
            }

            let chunk_result = if tiled {
                read_tile_chunk(header, &mut cursor)
            } else {
                read_scanline_chunk(header, &mut cursor, lines_per_block)
            };

            let (layout, x0, chunk_data) = match chunk_result {
                Ok(Some(v)) => v,
                Ok(None) => continue,
                Err(e) => {
                    log_warn!("EXR: failed to read chunk {}: {}", chunk_index, e);
                    continue;
                }
            };

            match decompress_chunk(limits, header.compression, &layout, chunk_data) {
                Ok(unpacked) => {
                    scatter_chunk(header, &layout, x0, &unpacked, planes, dw);
                }
                Err(e) => {
                    log_warn!("EXR: failed to decompress chunk {}: {}", chunk_index, e);
                }
            }
        }

        Ok(())
    }
}

fn deep_composite_channels(header: &PartHeader) -> (Option<usize>, Option<usize>) {
    let selection = select_channels(header);
    let z = header.channels.iter().position(|c| channel_suffix(&c.name) == "Z");
    (selection.a, z)
}

fn scatter_deep_chunk(pixels: &DeepChunkPixels, planes: &mut [Plane], dw: Box2i) {
    for (c, values) in pixels.channels.iter().enumerate() {
        let plane = &mut planes[c];

        for r in 0..pixels.rows {
            let py = pixels.y0 - dw.y_min as i64 + r as i64;
            if py < 0 || py as usize >= plane.height {
                continue;
            }

            let px = pixels.x0 - dw.x_min as i64;
            if px < 0 {
                continue;
            }

            let count = pixels.width.min(plane.width.saturating_sub(px as usize));
            let dst_start = py as usize * plane.width + px as usize;
            let src_start = r * pixels.width;
            plane.samples[dst_start..dst_start + count].copy_from_slice(&values[src_start..src_start + count]);
        }
    }
}

fn read_scanline_chunk<'a>(
    header: &PartHeader,
    cursor: &mut ByteCursor<'a>,
    lines_per_block: i64,
) -> VexelResult<Option<(ChunkLayout, i64, &'a [u8])>> {
    let dw = header.data_window;
    let y0 = cursor.read_i32()? as i64;
    let size = cursor.read_i32()?;

    if y0 < dw.y_min as i64 || y0 > dw.y_max as i64 {
        log_warn!("EXR: scanline chunk y {} is outside the data window, skipping", y0);
        return Ok(None);
    }

    if size <= 0 || size as usize > cursor.remaining() {
        return Err(VexelError::Custom(format!("EXR: invalid chunk data size {}", size)));
    }

    let data = cursor.read_bytes(size as usize)?;
    let y1 = (y0 + lines_per_block - 1).min(dw.y_max as i64);
    let layout = build_scanline_layout(header, y0, y1);

    Ok(Some((layout, dw.x_min as i64, data)))
}

fn read_tile_chunk<'a>(
    header: &PartHeader,
    cursor: &mut ByteCursor<'a>,
) -> VexelResult<Option<(ChunkLayout, i64, &'a [u8])>> {
    let dw = header.data_window;
    let tile_x = cursor.read_i32()? as i64;
    let tile_y = cursor.read_i32()? as i64;
    let level_x = cursor.read_i32()?;
    let level_y = cursor.read_i32()?;
    let size = cursor.read_i32()?;

    if size <= 0 || size as usize > cursor.remaining() {
        return Err(VexelError::Custom(format!("EXR: invalid tile data size {}", size)));
    }

    let data = cursor.read_bytes(size as usize)?;

    if level_x != 0 || level_y != 0 {
        return Ok(None);
    }

    let tiles = match &header.tiles {
        Some(t) => t,
        None => return Ok(None),
    };

    let tsx = tiles.x_size as i64;
    let tsy = tiles.y_size as i64;
    let width = dw.width();
    let height = dw.height();
    let tiles_x = (width + tsx - 1) / tsx;
    let tiles_y = (height + tsy - 1) / tsy;

    if tile_x < 0 || tile_x >= tiles_x || tile_y < 0 || tile_y >= tiles_y {
        log_warn!("EXR: tile coordinates ({}, {}) are out of range, skipping", tile_x, tile_y);
        return Ok(None);
    }

    let x0 = dw.x_min as i64 + tile_x * tsx;
    let y0 = dw.y_min as i64 + tile_y * tsy;
    let x1 = (x0 + tsx - 1).min(dw.x_max as i64);
    let y1 = (y0 + tsy - 1).min(dw.y_max as i64);

    let tile_width = (x1 - x0 + 1) as usize;
    let mut channels = Vec::with_capacity(header.channels.len());
    for channel in &header.channels {
        channels.push(ChunkChannel {
            width: tile_width,
            rows: (y1 - y0 + 1) as usize,
            bpe: channel.pixel_type.bytes_per_element(),
            pixel_type: channel.pixel_type,
            p_linear: channel.p_linear,
        });
    }

    let mut rows = Vec::with_capacity((y1 - y0 + 1) as usize);
    for y in y0..=y1 {
        rows.push((y, vec![true; header.channels.len()]));
    }

    let unpacked_size = channels.iter().map(|c| c.width * c.rows * c.bpe).sum();

    Ok(Some((ChunkLayout { channels, rows, unpacked_size }, x0, data)))
}

fn decompress_chunk(
    limits: &mut Limits,
    compression: ExrCompression,
    layout: &ChunkLayout,
    data: &[u8],
) -> VexelResult<Vec<u8>> {
    let unpacked = layout.unpacked_size;

    limits.reserve_usize(unpacked)?;
    let result = decompress_chunk_inner(compression, layout, data, unpacked);
    limits.free_usize(unpacked);
    result
}

fn decompress_chunk_inner(
    compression: ExrCompression,
    layout: &ChunkLayout,
    data: &[u8],
    unpacked: usize,
) -> VexelResult<Vec<u8>> {
    if data.len() == unpacked {
        return Ok(data.to_vec());
    }

    if data.len() > unpacked {
        log_warn!(
            "EXR: chunk data is larger than its uncompressed size ({} > {}), treating as uncompressed",
            data.len(),
            unpacked
        );
        return Ok(data[..unpacked].to_vec());
    }

    match compression {
        ExrCompression::None => {
            log_warn!("EXR: uncompressed chunk is truncated ({} of {} bytes), padding", data.len(), unpacked);
            let mut out = data.to_vec();
            out.resize(unpacked, 0);
            Ok(out)
        }
        ExrCompression::Rle => {
            let decoded = rle_decompress(data, unpacked)?;
            Ok(unpredict_and_reorder(decoded))
        }
        ExrCompression::Zips | ExrCompression::Zip => {
            let mut inflated = ZlibDecoder::from_slice(data).decode_with_max(unpacked);
            if inflated.len() < unpacked {
                log_warn!("EXR: zip chunk inflated to {} of {} expected bytes, padding", inflated.len(), unpacked);
                inflated.resize(unpacked, 0);
            }
            Ok(unpredict_and_reorder(inflated))
        }
        ExrCompression::Pxr24 => {
            let expected_planes: usize = layout
                .rows
                .iter()
                .map(|(_, present)| {
                    layout
                        .channels
                        .iter()
                        .enumerate()
                        .filter(|(c, _)| present[*c])
                        .map(|(_, ch)| {
                            ch.width
                                * match ch.pixel_type {
                                    ExrPixelType::Uint => 4,
                                    ExrPixelType::Half => 2,
                                    ExrPixelType::Float => 3,
                                }
                        })
                        .sum::<usize>()
                })
                .sum();

            let mut inflated = ZlibDecoder::from_slice(data).decode_with_max(expected_planes);
            if inflated.len() < expected_planes {
                log_warn!(
                    "EXR: pxr24 chunk inflated to {} of {} expected bytes, padding",
                    inflated.len(),
                    expected_planes
                );
                inflated.resize(expected_planes, 0);
            }

            let presence: Vec<Vec<bool>> = layout.rows.iter().map(|(_, p)| p.clone()).collect();
            pxr24_reconstruct(&inflated, &layout.channels, &presence, unpacked)
        }
        ExrCompression::Piz => {
            let planar = piz_decompress(data, &layout.channels)?;
            Ok(repack_planar(&planar, layout))
        }
        ExrCompression::B44 | ExrCompression::B44a => {
            let planar = b44_decompress(data, &layout.channels)?;
            Ok(repack_planar(&planar, layout))
        }
        other => Err(VexelError::UnsupportedFormat(format!("EXR: {} compression is not supported", other.name()))),
    }
}

fn build_scanline_layout(header: &PartHeader, y0: i64, y1: i64) -> ChunkLayout {
    let dw = header.data_window;

    let mut rows = Vec::with_capacity((y1 - y0 + 1).max(0) as usize);
    for y in y0..=y1 {
        let present: Vec<bool> = header.channels.iter().map(|c| is_sampled(y, c.y_sampling as i64)).collect();
        rows.push((y, present));
    }

    let channels: Vec<ChunkChannel> = header
        .channels
        .iter()
        .enumerate()
        .map(|(c, channel)| {
            let width = num_samples(channel.x_sampling as i64, dw.x_min as i64, dw.x_max as i64) as usize;
            let row_count = rows.iter().filter(|(_, present)| present[c]).count();
            ChunkChannel {
                width,
                rows: row_count,
                bpe: channel.pixel_type.bytes_per_element(),
                pixel_type: channel.pixel_type,
                p_linear: channel.p_linear,
            }
        })
        .collect();

    let unpacked_size = channels.iter().map(|c| c.width * c.rows * c.bpe).sum();

    ChunkLayout { channels, rows, unpacked_size }
}

fn repack_planar(planar: &[u8], layout: &ChunkLayout) -> Vec<u8> {
    let mut out = Vec::with_capacity(layout.unpacked_size);

    let mut channel_starts = Vec::with_capacity(layout.channels.len());
    let mut start = 0usize;
    for channel in &layout.channels {
        channel_starts.push(start);
        start += channel.width * channel.rows * channel.bpe;
    }

    let mut row_counters = vec![0usize; layout.channels.len()];

    for (_, present) in &layout.rows {
        for (c, channel) in layout.channels.iter().enumerate() {
            if !present[c] {
                continue;
            }

            let bpl = channel.width * channel.bpe;
            let src = channel_starts[c] + row_counters[c] * bpl;
            row_counters[c] += 1;

            if src + bpl <= planar.len() {
                out.extend_from_slice(&planar[src..src + bpl]);
            } else {
                out.resize(out.len() + bpl, 0);
            }
        }
    }

    out
}

fn scatter_chunk(header: &PartHeader, layout: &ChunkLayout, x0: i64, unpacked: &[u8], planes: &mut [Plane], dw: Box2i) {
    let mut pos = 0usize;

    for (y_abs, present) in &layout.rows {
        for (c, chunk_channel) in layout.channels.iter().enumerate() {
            if !present[c] {
                continue;
            }

            let bpl = chunk_channel.width * chunk_channel.bpe;
            if pos + bpl > unpacked.len() {
                return;
            }
            let row_bytes = &unpacked[pos..pos + bpl];
            pos += bpl;

            let channel = &header.channels[c];
            let plane = &mut planes[c];

            let row_index = sample_index(*y_abs, dw.y_min as i64, channel.y_sampling as i64);
            if row_index < 0 || row_index as usize >= plane.height {
                continue;
            }

            let col_start = sample_index(x0, dw.x_min as i64, channel.x_sampling as i64);
            if col_start < 0 {
                continue;
            }

            let dst_start = row_index as usize * plane.width + col_start as usize;
            let count = chunk_channel.width.min(plane.width.saturating_sub(col_start as usize));

            scatter_row(&mut plane.samples[dst_start..dst_start + count], row_bytes, chunk_channel.pixel_type);
        }
    }
}

fn scatter_row(dst: &mut [f32], row_bytes: &[u8], pixel_type: ExrPixelType) {
    match pixel_type {
        ExrPixelType::Half => {
            for (i, v) in dst.iter_mut().enumerate() {
                let o = i * 2;
                if o + 2 <= row_bytes.len() {
                    *v = half_to_f32(u16::from_le_bytes([row_bytes[o], row_bytes[o + 1]]));
                }
            }
        }
        ExrPixelType::Float => {
            for (i, v) in dst.iter_mut().enumerate() {
                let o = i * 4;
                if o + 4 <= row_bytes.len() {
                    let bits = u32::from_le_bytes([row_bytes[o], row_bytes[o + 1], row_bytes[o + 2], row_bytes[o + 3]]);
                    *v = f32::from_bits(bits);
                }
            }
        }
        ExrPixelType::Uint => {
            for (i, v) in dst.iter_mut().enumerate() {
                let o = i * 4;
                if o + 4 <= row_bytes.len() {
                    let value =
                        u32::from_le_bytes([row_bytes[o], row_bytes[o + 1], row_bytes[o + 2], row_bytes[o + 3]]);
                    *v = value as f32;
                }
            }
        }
    }
}

fn channel_suffix(name: &str) -> String {
    match name.rfind('.') {
        Some(pos) => name[pos + 1..].to_ascii_uppercase(),
        None => name.to_ascii_uppercase(),
    }
}

fn channel_layer(name: &str) -> &str {
    match name.rfind('.') {
        Some(pos) => &name[..pos],
        None => "",
    }
}

struct ChannelSelection {
    r: Option<usize>,
    g: Option<usize>,
    b: Option<usize>,
    a: Option<usize>,
    y: Option<usize>,
    ry: Option<usize>,
    by: Option<usize>,
}

fn select_channels(header: &PartHeader) -> ChannelSelection {
    let mut selection = ChannelSelection { r: None, g: None, b: None, a: None, y: None, ry: None, by: None };

    let layers: Vec<&str> = header.channels.iter().map(|c| channel_layer(&c.name)).collect();
    let has_base_layer = layers.iter().any(|l| l.is_empty());
    let chosen_layer: &str = if has_base_layer { "" } else { layers.first().copied().unwrap_or("") };

    if !chosen_layer.is_empty() {
        log_warn!("EXR: no base-layer channels found, using layer '{}'", chosen_layer);
    }

    for (i, channel) in header.channels.iter().enumerate() {
        if channel_layer(&channel.name) != chosen_layer {
            continue;
        }

        let slot = match channel_suffix(&channel.name).as_str() {
            "R" => &mut selection.r,
            "G" => &mut selection.g,
            "B" => &mut selection.b,
            "A" => &mut selection.a,
            "Y" => &mut selection.y,
            "RY" => &mut selection.ry,
            "BY" => &mut selection.by,
            _ => continue,
        };

        if slot.is_none() {
            *slot = Some(i);
        }
    }

    selection
}

fn upsample_to_full(header: &PartHeader, plane: &Plane, channel_index: usize, width: u32, height: u32) -> Vec<f32> {
    let channel = &header.channels[channel_index];
    let dw = header.data_window;
    let w = width as usize;
    let h = height as usize;

    if channel.x_sampling <= 1 && channel.y_sampling <= 1 && plane.width == w && plane.height == h {
        return plane.samples.clone();
    }

    let mut out = vec![0f32; w * h];
    if plane.width == 0 || plane.height == 0 {
        return out;
    }

    for row in 0..h {
        let y_abs = dw.y_min as i64 + row as i64;
        let sy =
            sample_index(y_abs, dw.y_min as i64, channel.y_sampling as i64).clamp(0, plane.height as i64 - 1) as usize;

        for col in 0..w {
            let x_abs = dw.x_min as i64 + col as i64;
            let sx = sample_index(x_abs, dw.x_min as i64, channel.x_sampling as i64).clamp(0, plane.width as i64 - 1)
                as usize;
            out[row * w + col] = plane.samples[sy * plane.width + sx];
        }
    }

    out
}

fn luminance_weights(header: &PartHeader) -> [f32; 3] {
    let c = match header.chromaticities {
        Some(c) => c,
        None => return DEFAULT_LUMINANCE_WEIGHTS,
    };

    let (rx, ry, gx, gy, bx, by, wx, wy) = (c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]);

    if wy.abs() < 1e-6 {
        return DEFAULT_LUMINANCE_WEIGHTS;
    }

    let big_x = wx / wy;
    let big_z = (1.0 - wx - wy) / wy;
    let xz = big_x + big_z;

    let d = rx * (by - gy) + bx * (gy - ry) + gx * (ry - by);
    if d.abs() < 1e-9 {
        return DEFAULT_LUMINANCE_WEIGHTS;
    }

    let sr = (big_x * (by - gy) - gx * ((by - 1.0) + by * xz) + bx * ((gy - 1.0) + gy * xz)) / d;
    let sg = (big_x * (ry - by) + rx * ((by - 1.0) + by * xz) - bx * ((ry - 1.0) + ry * xz)) / d;
    let sb = (big_x * (gy - ry) - rx * ((gy - 1.0) + gy * xz) + gx * ((ry - 1.0) + ry * xz)) / d;

    let yr = sr * ry;
    let yg = sg * gy;
    let yb = sb * by;
    let sum = yr + yg + yb;

    if !sum.is_finite() || sum.abs() < 1e-9 {
        return DEFAULT_LUMINANCE_WEIGHTS;
    }

    [yr / sum, yg / sum, yb / sum]
}

fn interleave(planes: Vec<Vec<f32>>) -> Vec<f32> {
    let n = planes.len();
    let len = planes.first().map(|p| p.len()).unwrap_or(0);
    let mut out = vec![0f32; len * n];

    for (c, plane) in planes.iter().enumerate() {
        for (i, &v) in plane.iter().enumerate() {
            out[i * n + c] = v;
        }
    }

    out
}

fn build_image(header: &PartHeader, planes: Vec<Plane>, width: u32, height: u32) -> VexelResult<Image> {
    let selection = select_channels(header);
    let full = |index: usize| upsample_to_full(header, &planes[index], index, width, height);

    if header.channels.iter().any(|c| c.pixel_type == ExrPixelType::Uint) {
        log_warn!("EXR: uint channels are converted to float, values above 2^24 lose precision");
    }

    let pixels = match (selection.r, selection.g, selection.b) {
        (Some(r), Some(g), Some(b)) => {
            let channels = vec![full(r), full(g), full(b)];
            match selection.a {
                Some(a) => {
                    let mut channels = channels;
                    channels.push(full(a));
                    PixelData::RGBA32F(interleave(channels))
                }
                None => PixelData::RGB32F(interleave(channels)),
            }
        }
        _ => match (selection.y, selection.ry, selection.by) {
            (Some(y), Some(ry), Some(by)) => {
                let yw = luminance_weights(header);
                let y_plane = full(y);
                let ry_plane = full(ry);
                let by_plane = full(by);

                let mut rgb = vec![0f32; y_plane.len() * 3];
                for i in 0..y_plane.len() {
                    let yv = y_plane[i];
                    let r = (ry_plane[i] + 1.0) * yv;
                    let b = (by_plane[i] + 1.0) * yv;
                    let g = (yv - r * yw[0] - b * yw[2]) / yw[1];
                    rgb[i * 3] = r;
                    rgb[i * 3 + 1] = g;
                    rgb[i * 3 + 2] = b;
                }

                match selection.a {
                    Some(a) => {
                        let a_plane = full(a);
                        let mut rgba = vec![0f32; y_plane.len() * 4];
                        for i in 0..y_plane.len() {
                            rgba[i * 4] = rgb[i * 3];
                            rgba[i * 4 + 1] = rgb[i * 3 + 1];
                            rgba[i * 4 + 2] = rgb[i * 3 + 2];
                            rgba[i * 4 + 3] = a_plane[i];
                        }
                        PixelData::RGBA32F(rgba)
                    }
                    None => PixelData::RGB32F(rgb),
                }
            }
            _ => match selection.y {
                Some(y) => match selection.a {
                    Some(a) => PixelData::LA32F(interleave(vec![full(y), full(a)])),
                    None => PixelData::L32F(full(y)),
                },
                None => {
                    let n = header.channels.len();
                    if n == 1 {
                        log_warn!("EXR: mapping channel '{}' to grayscale", header.channels[0].name);
                        PixelData::L32F(full(0))
                    } else if n >= 3 {
                        log_warn!(
                            "EXR: no recognized color channels, mapping '{}', '{}', '{}' to RGB",
                            header.channels[0].name,
                            header.channels[1].name,
                            header.channels[2].name
                        );
                        match selection.a {
                            Some(a) => PixelData::RGBA32F(interleave(vec![full(0), full(1), full(2), full(a)])),
                            None => PixelData::RGB32F(interleave(vec![full(0), full(1), full(2)])),
                        }
                    } else {
                        match selection.a {
                            Some(a) => {
                                let other = if a == 0 { 1 } else { 0 };
                                log_warn!(
                                    "EXR: mapping channel '{}' to grayscale with alpha",
                                    header.channels[other].name
                                );
                                PixelData::LA32F(interleave(vec![full(other), full(a)]))
                            }
                            None => {
                                log_warn!(
                                    "EXR: no recognized color channels, mapping '{}' to grayscale",
                                    header.channels[0].name
                                );
                                PixelData::L32F(full(0))
                            }
                        }
                    }
                }
            },
        },
    };

    Ok(Image::from_pixels(width, height, pixels))
}

fn build_part_info(part: &ExrPart) -> ExrPartInfo {
    let header = &part.header;
    let dw = header.data_window;

    ExrPartInfo {
        name: header.name.clone(),
        part_type: header.kind.name().to_string(),
        compression: header.compression.name(),
        width: dw.width().clamp(0, u32::MAX as i64) as u32,
        height: dw.height().clamp(0, u32::MAX as i64) as u32,
        data_window: [dw.x_min, dw.y_min, dw.x_max, dw.y_max],
        display_window: [
            header.display_window.x_min,
            header.display_window.y_min,
            header.display_window.x_max,
            header.display_window.y_max,
        ],
        line_order: header.line_order.name().to_string(),
        tile_size: header.tiles.map(|t| [t.x_size, t.y_size]),
        chunk_count: part.chunk_count,
        channels: header
            .channels
            .iter()
            .map(|c| ExrChannelInfo {
                name: c.name.clone(),
                pixel_type: c.pixel_type.name().to_string(),
                x_sampling: c.x_sampling,
                y_sampling: c.y_sampling,
                p_linear: c.p_linear,
            })
            .collect(),
        attributes: header.attributes.clone(),
    }
}
