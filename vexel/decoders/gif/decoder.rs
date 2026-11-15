use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use super::compose::{compose_first, compose_layer, Layer, Palette, Placement, Update};
use super::info::{
    GifApplicationExtension, GifApplicationKind, GifColorTable, GifComment, GifGraphicsControl, GifHeader,
    GifImageData, GifImageDescriptor, GifInfo, GifPlainText, GifSection, GifUnknownExtension,
};
use super::lzw::{decode_frame_indices, gather_sub_blocks, FrameIndices};
use super::source::GifSource;
use super::types::{DisposalMethod, GifDecoder, GifFrameInfo, GraphicsControlExtension};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::frames::{reserve_canvas, FrameExtent, FramePrefetch, FrameSource, Frames, OutputBudget};
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, ImageSummary};
use crate::{log_debug, log_warn, Image, ImageFrame, Limits, PixelData};

impl<R: Read + Seek + Sync> GifDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            canvas_width: 0,
            canvas_height: 0,
            version: String::new(),
            global_color_table_flag: false,
            color_resolution: 0,
            sort_flag: false,
            size_of_global_color_table: 0,
            background_color_index: 0,
            pixel_aspect_ratio: 0,
            global_color_table: Vec::new(),
            frames: Vec::new(),
            collected: GifInfo::default(),
            info: None,
            icc_profile: None,
            loop_count: None,
            collect_frame_data: true,
            frame_index: 0,
            reader: GifSource::new(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> GifInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            self.collect_frame_data = false;
            self.reader.set_retain(false);
            if let Err(e) = self.read_structure(FrameExtent::All) {
                log_warn!("Failed to read GIF structure: {}", e);
                self.finish_info();
            }
            self.collect_frame_data = true;
            self.reader.set_retain(true);
            self.frames.clear();
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    fn push_section(&mut self, start: u64, section: GifSection) {
        let end = self.reader.stream_position();
        self.collected.push(start, Some(end.saturating_sub(start)), section);
    }

    fn finish_info(&mut self) {
        let mut info = std::mem::take(&mut self.collected);

        let header = info.sections.iter().find_map(|section| match &section.data {
            GifSection::Header(header) => Some(header.clone()),
            _ => None,
        });

        if let Some(header) = header {
            let frame_count = info.sections.iter().filter(|s| matches!(s.data, GifSection::ImageDescriptor(_))).count();
            let has_transparency = info.sections.iter().any(|s| {
                matches!(&s.data, GifSection::GraphicsControlExtension(gce) if gce.transparent_color_index.is_some())
            });
            let delays: Vec<u64> = info
                .sections
                .iter()
                .filter_map(|s| match &s.data {
                    GifSection::GraphicsControlExtension(gce) => Some(u64::from(gce.delay_cs)),
                    _ => None,
                })
                .collect();

            let mut summary = ImageSummary::new(header.canvas_width, header.canvas_height, ColorModel::Indexed, 1, 8);
            summary.has_alpha = has_transparency;
            summary.compression = Some("LZW".to_string());
            summary.icc_profile = self.icc_profile.clone();
            if frame_count > 1 {
                summary.frames = Some(frame_count as u32);
                summary.loop_count = self.loop_count.map(u32::from);
                summary.duration_ms = (!delays.is_empty()).then(|| delays.iter().sum::<u64>() * 10);
            }
            info.summary = Some(summary);
        }

        self.info = Some(info);
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let start_offset = self.reader.stream_position();

        let mut buf = [0u8; 10];
        let mut n = 0;
        while n < buf.len() {
            match self.reader.read_u8() {
                Ok(b) => {
                    buf[n] = b;
                    n += 1;
                }
                Err(_) => break,
            }
        }
        let buf = &buf[..n];
        let sig_offset = (0..4).find(|&i| {
            let tail = buf.get(i..).unwrap_or(&[]);
            tail.starts_with(b"GIF87a")
                || tail.starts_with(b"GIF89a")
                || tail.starts_with(b"IF87a")
                || tail.starts_with(b"IF89a")
        });
        let offset = sig_offset.unwrap_or(0);
        if offset > 0 || (!buf.starts_with(b"GIF87a") && !buf.starts_with(b"GIF89a")) {
            log_warn!("GIF signature at offset {}, possibly corrupted header", offset);
        }
        let sig_len = if buf[offset..].starts_with(b"GIF") { 6 } else { 5 };
        let signature = String::from_utf8_lossy(buf.get(offset..offset + sig_len).unwrap_or(b"")).to_string();
        self.version =
            String::from_utf8_lossy(buf.get(offset + (sig_len - 3)..offset + sig_len).unwrap_or(b"87a")).to_string();
        self.reader.seek(std::io::SeekFrom::Start((offset + sig_len) as u64))?;

        self.canvas_width = self.reader.read_u16()? as u32;
        self.canvas_height = self.reader.read_u16()? as u32;

        let packed_fields = self.reader.read_u8()?;

        self.global_color_table_flag = (packed_fields & 0b10000000) != 0;
        self.color_resolution = (packed_fields & 0b01110000) >> 4;
        self.sort_flag = (packed_fields & 0b00001000) != 0;
        self.size_of_global_color_table = packed_fields & 0b00000111;

        self.background_color_index = self.reader.read_u8()?;
        self.pixel_aspect_ratio = self.reader.read_u8()?;

        self.push_section(
            start_offset,
            GifSection::Header(GifHeader {
                signature_offset: offset as u32,
                signature,
                canvas_width: self.canvas_width,
                canvas_height: self.canvas_height,
                global_color_table_flag: self.global_color_table_flag,
                color_resolution: self.color_resolution,
                sort_flag: self.sort_flag,
                global_color_table_size: self.size_of_global_color_table,
                background_color_index: self.background_color_index,
                pixel_aspect_ratio: self.pixel_aspect_ratio,
            }),
        );

        if self.canvas_width == 0 || self.canvas_height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.canvas_width, height: self.canvas_height });
        }

        if self.collect_frame_data {
            self.limits.reserve_buffer(self.canvas_width, self.canvas_height, 4)?;
        }

        Ok(())
    }

    fn read_color_table(&mut self, size_field: u8) -> VexelResult<(Vec<u8>, u32)> {
        let num_entries = 1u32 << (size_field + 1);
        let table_size = num_entries * 3;
        self.limits.reserve(table_size as u64)?;

        let mut table = Vec::with_capacity(table_size as usize);
        for _ in 0..table_size {
            match self.reader.read_u8() {
                Ok(byte) => table.push(byte),
                Err(e) => {
                    log_warn!("Color table truncated after {} of {} bytes: {:?}", table.len(), table_size, e);
                    break;
                }
            }
        }

        Ok((table, num_entries))
    }

    fn read_global_color_table(&mut self) {
        if !self.global_color_table_flag {
            return;
        }

        let start_offset = self.reader.stream_position();
        let (table, declared_entries) = match self.read_color_table(self.size_of_global_color_table) {
            Ok(result) => result,
            Err(e) => {
                log_warn!("Global color table too large, skipping: {:?}", e);
                return;
            }
        };

        let entries = table.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        self.global_color_table = table;
        self.push_section(
            start_offset,
            GifSection::GlobalColorTable(GifColorTable { frame: None, declared_entries, entries }),
        );
    }

    fn read_application_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let block_size = self.reader.read_u8()?;
        if block_size != 11 {
            log_warn!("Invalid application extension block size: {}", block_size);
        }

        let mut identifier = [0u8; 8];
        let mut auth_code = [0u8; 3];
        self.reader.read_exact(&mut identifier)?;
        self.reader.read_exact(&mut auth_code)?;

        let kind = match (&identifier, &auth_code) {
            (b"NETSCAPE", b"2.0") => GifApplicationKind::Netscape,
            (b"ANIMEXTS", b"1.0") => GifApplicationKind::AnimExts,
            (b"XMP Data", b"XMP") => GifApplicationKind::Xmp,
            (b"ICCRGBG1", b"012") => GifApplicationKind::Icc,
            _ => GifApplicationKind::Other,
        };
        let looping = matches!(kind, GifApplicationKind::Netscape | GifApplicationKind::AnimExts);

        let mut extension = GifApplicationExtension {
            identifier: String::from_utf8_lossy(&identifier).to_string(),
            auth_code: String::from_utf8_lossy(&auth_code).to_string(),
            kind,
            loop_count: None,
            buffer_size: None,
            sub_blocks: 0,
            data_length: 0,
            icc_parsed: None,
        };

        let mut icc_data = Vec::new();
        let mut truncated = false;
        loop {
            let sub_block_size = match self.reader.read_u8() {
                Ok(0) => break,
                Ok(size) => size,
                Err(_) => {
                    truncated = true;
                    break;
                }
            };
            extension.sub_blocks += 1;
            extension.data_length += u64::from(sub_block_size);

            let mut payload = vec![0u8; sub_block_size as usize];
            if self.reader.read_exact(&mut payload).is_err() {
                truncated = true;
                break;
            }

            if looping {
                let value = |bytes: &[u8], count: usize| {
                    bytes.iter().take(count).rev().fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte))
                };
                match payload.first() {
                    Some(1) => extension.loop_count = Some(value(&payload[1..], 2) as u16),
                    Some(2) => extension.buffer_size = Some(value(&payload[1..], 4)),
                    Some(other) => log_debug!("Skipping unknown {} sub-block: {:#04x}", extension.identifier, other),
                    None => {}
                }
            } else if kind == GifApplicationKind::Icc {
                match self.limits.reserve_usize(payload.len()) {
                    Ok(()) => icc_data.extend_from_slice(&payload),
                    Err(e) => log_warn!("ICC application extension too large, ignoring: {:?}", e),
                }
            }
        }

        if truncated {
            log_warn!("Truncated application extension {}", extension.identifier);
        }

        if kind == GifApplicationKind::Icc {
            let profile = ICCProfile::new(&icc_data).ok();
            extension.icc_parsed = Some(profile.is_some());
            if profile.is_some() {
                self.icc_profile = profile;
            }
        }

        if looping && self.loop_count.is_none() {
            self.loop_count = extension.loop_count;
        }

        self.push_section(start_offset, GifSection::ApplicationExtension(extension));

        Ok(())
    }

    fn skip_sub_blocks(reader: &mut GifSource<R>) -> VexelResult<()> {
        loop {
            let block_size = match reader.read_u8() {
                Ok(size) => size as usize,
                Err(_) => return Ok(()),
            };

            if block_size == 0 || reader.skip(block_size).is_err() {
                return Ok(());
            }
        }
    }

    fn read_text_sub_blocks(&mut self) -> VexelResult<String> {
        let mut text = String::new();
        loop {
            let sub_block_size = self.reader.read_u8()? as usize;
            if sub_block_size == 0 {
                break;
            }

            if let Err(e) = self.limits.reserve_usize(sub_block_size) {
                log_warn!("Text extension too large, truncating: {:?}", e);
                for _ in 0..sub_block_size {
                    self.reader.read_u8()?;
                }
                break;
            }

            let mut block = vec![0u8; sub_block_size];
            self.reader.read_exact(&mut block)?;
            text.push_str(&String::from_utf8_lossy(&block));
        }

        Ok(text)
    }

    fn read_plain_text_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let block_size = self.reader.read_u8()? as usize;

        let mut header = [0u8; 12];
        let header_len = block_size.min(header.len());
        if block_size != header.len() {
            log_warn!("Invalid plain text extension block size: {}", block_size);
        }

        for byte in header.iter_mut().take(header_len) {
            *byte = self.reader.read_u8()?;
        }
        for _ in header_len..block_size {
            self.reader.read_u8()?;
        }

        let read_u16 = |bytes: &[u8]| u16::from_le_bytes([bytes[0], bytes[1]]);
        let text = self.read_text_sub_blocks()?;

        self.push_section(
            start_offset,
            GifSection::PlainTextExtension(GifPlainText {
                left: read_u16(&header[0..2]),
                top: read_u16(&header[2..4]),
                width: read_u16(&header[4..6]),
                height: read_u16(&header[6..8]),
                cell_width: header[8],
                cell_height: header[9],
                foreground_color: header[10],
                background_color: header[11],
                text,
            }),
        );

        Ok(())
    }

    fn read_comment_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let text = self.read_text_sub_blocks()?;
        self.push_section(start_offset, GifSection::CommentExtension(GifComment { text }));
        Ok(())
    }

    fn read_frames(&mut self, extent: FrameExtent) -> VexelResult<bool> {
        let mut current_gce: Option<(u64, GraphicsControlExtension)> = None;

        loop {
            let block_start = self.reader.stream_position();
            let block_type = self.reader.read_u8()?;

            match block_type {
                // Image Separator (0x2C)
                0x2C => {
                    self.read_frame(block_start, current_gce.take().map(|(_, gce)| gce))?;
                    if extent == FrameExtent::First && !self.frames.is_empty() {
                        return Ok(false);
                    }
                }
                // Extension Introducer (0x21)
                0x21 => {
                    let label = self.reader.read_u8()?;
                    match label {
                        0xF9 => {
                            let gce = self.read_graphics_control_extension(block_start)?;
                            current_gce = Some((block_start, gce));
                        }
                        0xFE => {
                            self.read_comment_extension(block_start)?;
                        }
                        0xFF => {
                            self.read_application_extension(block_start)?;
                        }
                        0x01 => {
                            let resume_offset = self.reader.stream_position();
                            if let Err(e) = self.read_plain_text_extension(block_start) {
                                log_warn!("Failed to read plain text extension, skipping: {:?}", e);
                                self.reader.seek(SeekFrom::Start(resume_offset))?;
                                Self::skip_sub_blocks(&mut self.reader)?;
                            }
                        }
                        _ => {
                            log_warn!("Skipping unknown extension: {:#04x}", label);
                            Self::skip_sub_blocks(&mut self.reader)?;
                            self.push_section(block_start, GifSection::UnknownExtension(GifUnknownExtension { label }));
                        }
                    }
                }
                // End of image (0x3B)
                0x3B => {
                    self.push_section(block_start, GifSection::Trailer);
                    let trailing_start = self.reader.stream_position();
                    let trailing = self.reader.bytes_left().unwrap_or(0);
                    if trailing > 0 {
                        self.collected.push(trailing_start, Some(trailing), GifSection::TrailingData);
                    }
                    break;
                }
                _ => {}
            }
        }

        Ok(true)
    }

    fn read_frame(&mut self, descriptor_offset: u64, gce: Option<GraphicsControlExtension>) -> VexelResult<()> {
        let frame_index = self.frame_index;
        self.frame_index += 1;

        let left = self.reader.read_u16()? as u32;
        let top = self.reader.read_u16()? as u32;
        let width = self.reader.read_u16()? as u32;
        let height = self.reader.read_u16()? as u32;

        let packed_fields = self.reader.read_u8()?;
        let local_color_table_flag = (packed_fields & 0b10000000) != 0;
        let interlace_flag = (packed_fields & 0b01000000) != 0;
        let sort_flag = (packed_fields & 0b00100000) != 0;
        let size_of_local_color_table = packed_fields & 0b00000111;

        self.push_section(
            descriptor_offset,
            GifSection::ImageDescriptor(GifImageDescriptor {
                frame: frame_index,
                left,
                top,
                width,
                height,
                local_color_table_flag,
                interlace_flag,
                sort_flag,
                local_color_table_size: size_of_local_color_table,
            }),
        );

        let mut local_color_table = Vec::new();
        if local_color_table_flag {
            let lct_offset = self.reader.stream_position();
            let (table, declared_entries) = match self.read_color_table(size_of_local_color_table) {
                Ok(result) => result,
                Err(e) => {
                    log_warn!("Local color table too large, skipping frame: {:?}", e);
                    return Ok(());
                }
            };
            let entries = table.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            local_color_table = table;
            self.push_section(
                lct_offset,
                GifSection::LocalColorTable(GifColorTable { frame: Some(frame_index), declared_entries, entries }),
            );
        }

        let image_data_offset = self.reader.stream_position();
        let lzw_minimum_code_size = self.reader.read_u8()?;

        let chain_start = self.reader.offset();
        let mut chain_end = chain_start;
        let mut payload = 0usize;
        let mut data_length: u64 = 0;
        let mut sub_blocks: u32 = 0;
        loop {
            let block_size = match self.reader.read_u8() {
                Ok(0) => break,
                Ok(size) => size as usize,
                Err(e) => {
                    log_warn!("Error reading image sub-block size: {:?}", e);
                    break;
                }
            };

            sub_blocks += 1;
            data_length += block_size as u64;

            if !self.collect_frame_data {
                if self.reader.skip(block_size).is_err() {
                    break;
                }
                continue;
            }

            if let Err(e) = self.limits.reserve_usize(block_size) {
                log_warn!("Frame image data too large, truncating: {:?}", e);
                self.reader.skip(block_size).ok();
                break;
            }

            if let Err(e) = self.reader.skip(block_size) {
                log_warn!("Error reading image sub-block data: {:?}", e);
                break;
            }
            chain_end = self.reader.offset();
            payload += block_size;
        }

        self.push_section(
            image_data_offset,
            GifSection::ImageData(GifImageData { frame: frame_index, lzw_minimum_code_size, sub_blocks, data_length }),
        );

        let frame = GifFrameInfo {
            left,
            top,
            width,
            height,
            local_color_table_flag,
            interlace_flag,
            local_color_table,
            lzw_minimum_code_size,
            transparent_index: gce.as_ref().filter(|gce| gce.transparency).map(|gce| gce.transparent_color_index),
            disposal_method: gce.as_ref().map(|gce| gce.disposal_method).unwrap_or(DisposalMethod::None),
            delay: gce.map(|gce| gce.delay).unwrap_or(100),
            data: chain_start..chain_end,
            payload,
        };

        if self.frames.is_empty() {
            if self.canvas_width == 0 || self.canvas_height == 0 {
                log_warn!("GIF canvas is {}x{}, using the first frame's extent", self.canvas_width, self.canvas_height);
                if self.canvas_width == 0 {
                    self.canvas_width = left + width;
                }
                if self.canvas_height == 0 {
                    self.canvas_height = top + height;
                }
            }
            self.width = self.canvas_width;
            self.height = self.canvas_height;
        }

        self.frames.push(frame);

        Ok(())
    }

    fn read_graphics_control_extension(&mut self, start_offset: u64) -> VexelResult<GraphicsControlExtension> {
        let block_size = self.reader.read_u8()?;
        if block_size != 4 {
            log_warn!("Invalid graphics control extension block size: {}", block_size);
        }

        let packed = self.reader.read_u8()?;
        let disposal_raw = (packed >> 2) & 0x07;
        let disposal_method = match disposal_raw {
            2 => DisposalMethod::Background,
            3 => DisposalMethod::Previous,
            _ => DisposalMethod::None,
        };

        let user_input = (packed & 0x02) != 0;
        let transparency = (packed & 0x01) != 0;

        let delay_cs = self.reader.read_u16()?;
        let delay = u32::from(delay_cs) * 10;

        let transparent_color_index = self.reader.read_u8()?;

        let terminator = self.reader.read_u8()?;
        if terminator != 0 {
            log_warn!("Invalid graphics control extension block terminator: {}", terminator);
        }

        self.push_section(
            start_offset,
            GifSection::GraphicsControlExtension(GifGraphicsControl {
                block_size,
                disposal_method: disposal_raw,
                user_input,
                transparent_color_index: transparency.then_some(transparent_color_index),
                delay_cs,
            }),
        );

        Ok(GraphicsControlExtension { disposal_method, transparency, delay, transparent_color_index })
    }

    fn read_structure(&mut self, extent: FrameExtent) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.collected = GifInfo::default();
        self.info = None;
        self.frames.clear();
        self.global_color_table.clear();
        self.icc_profile = None;
        self.loop_count = None;
        self.frame_index = 0;

        match self.read_header() {
            Ok(_) => {}
            Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
            Err(e) => {
                log_warn!("Error reading header, this might be critical! Error: {:?}", e);
            }
        };

        self.read_global_color_table();

        let complete = match self.read_frames(extent) {
            Ok(complete) => complete,
            Err(e) => {
                log_warn!("Error reading frames, this might be critical! Error: {:?}", e);
                true
            }
        };

        if complete {
            self.finish_info();
        } else {
            self.collected = GifInfo::default();
        }

        Ok(())
    }

    fn resolve_dimensions(&mut self) -> VexelResult<()> {
        if self.width == 0 {
            self.width = self.canvas_width;
        }

        if self.height == 0 {
            self.height = self.canvas_height;
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        Ok(())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.collect_frame_data = true;
        self.read_structure(FrameExtent::First)?;
        self.resolve_dimensions()?;

        self.limits.check_dimensions(self.width, self.height)?;
        let size = (self.width as usize).saturating_mul(self.height as usize).saturating_mul(4);
        self.limits.reserve_usize(size)?;

        let Some(frame) = self.frames.first() else {
            return Ok(Image::from_pixels(self.width, self.height, PixelData::RGBA8(vec![0u8; size])));
        };

        let indices = frame_indices(frame, self.reader.data(), self.width, self.height, Vec::new());
        let palette = Palette::new(color_table(frame, &self.global_color_table), frame.transparent_index);
        let layer = Layer {
            placement: placement(frame, self.width, self.height),
            indices: &indices,
            palette: &palette,
            update: Update::Keep,
        };

        let mut pixels = Vec::with_capacity(size);
        compose_first(&layer, &mut pixels, self.width as usize, self.height as usize);

        Ok(Image::from_pixels(self.width, self.height, PixelData::RGBA8(pixels)))
    }

    pub fn decode_frames(&mut self) -> VexelResult<Frames> {
        let max_alloc = self.limits.max_alloc;

        self.collect_frame_data = true;
        self.read_structure(FrameExtent::All)?;
        self.resolve_dimensions()?;

        let canvas = reserve_canvas(&mut self.limits, self.width, self.height)?;
        let shared = Shared {
            frames: std::mem::take(&mut self.frames),
            source: self.reader.take_data(),
            width: self.width,
            height: self.height,
            buffers: Mutex::new(Vec::new()),
        };

        Ok(Frames::new(GifFrames::new(shared, self.global_color_table.clone(), canvas, max_alloc)))
    }
}

fn visible_size(frame: &GifFrameInfo, canvas_width: u32, canvas_height: u32) -> (usize, usize) {
    let width = frame.width.min(canvas_width.saturating_sub(frame.left));
    let height = frame.height.min(canvas_height.saturating_sub(frame.top));
    (width as usize, height as usize)
}

fn placement(frame: &GifFrameInfo, canvas_width: u32, canvas_height: u32) -> Placement {
    let (width, height) = visible_size(frame, canvas_width, canvas_height);
    if width == 0 || height == 0 {
        return Placement { left: 0, top: 0, width: 0, height: 0 };
    }
    Placement { left: frame.left as usize, top: frame.top as usize, width, height }
}

fn color_table<'a>(frame: &'a GifFrameInfo, global_color_table: &'a [u8]) -> &'a [u8] {
    if frame.local_color_table_flag {
        frame.local_color_table.as_slice()
    } else {
        global_color_table
    }
}

fn frame_indices(
    frame: &GifFrameInfo,
    source: &[u8],
    canvas_width: u32,
    canvas_height: u32,
    buffer: Vec<u8>,
) -> FrameIndices {
    let (visible_width, visible_height) = visible_size(frame, canvas_width, canvas_height);
    if visible_width == 0 || visible_height == 0 {
        return FrameIndices::empty();
    }

    let data = gather_sub_blocks(source, frame.data.clone(), frame.payload);
    decode_frame_indices(
        frame.lzw_minimum_code_size,
        data,
        frame.width as usize,
        frame.height as usize,
        visible_width,
        visible_height,
        frame.interlace_flag,
        buffer,
    )
}

const POOLED_BUFFERS: usize = 128;

struct Shared {
    frames: Vec<GifFrameInfo>,
    source: Vec<u8>,
    width: u32,
    height: u32,
    buffers: Mutex<Vec<Vec<u8>>>,
}

impl Shared {
    fn decode(&self, index: usize) -> FrameIndices {
        let buffer = self.buffers.lock().ok().and_then(|mut buffers| buffers.pop()).unwrap_or_default();
        frame_indices(&self.frames[index], &self.source, self.width, self.height, buffer)
    }

    fn recycle(&self, indices: FrameIndices) {
        if let Ok(mut buffers) = self.buffers.lock() {
            if buffers.len() < POOLED_BUFFERS {
                buffers.push(indices.into_buffer());
            }
        }
    }
}

pub(crate) struct GifFrames {
    shared: Arc<Shared>,
    global_color_table: Vec<u8>,
    canvas: Vec<u8>,
    prefetch: FramePrefetch<FrameIndices>,
    next_index: usize,
    exhausted: bool,
    budget: OutputBudget,
}

impl GifFrames {
    fn new(shared: Shared, global_color_table: Vec<u8>, canvas: Vec<u8>, max_alloc: Option<u64>) -> GifFrames {
        let compressed_bytes = shared.frames.iter().map(|frame| frame.payload as u64).fold(0u64, u64::saturating_add);
        let budget = OutputBudget::new(max_alloc, compressed_bytes);

        let costs = shared
            .frames
            .iter()
            .map(|frame| {
                let (visible_width, visible_height) = visible_size(frame, shared.width, shared.height);
                visible_width.saturating_mul(visible_height)
            })
            .collect();

        let shared = Arc::new(shared);
        let worker = shared.clone();
        let prefetch = FramePrefetch::new(costs, true, move |index| worker.decode(index));

        GifFrames { shared, global_color_table, canvas, prefetch, next_index: 0, exhausted: false, budget }
    }
}

impl FrameSource for GifFrames {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        if self.exhausted {
            return Ok(None);
        }

        let indices = match self.prefetch.next() {
            Ok(Some(indices)) => indices,
            Ok(None) => {
                self.exhausted = true;
                return Ok(None);
            }
            Err(e) => {
                self.exhausted = true;
                if self.next_index == 0 {
                    return Err(e);
                }

                log_warn!("Stopping GIF frame iteration after {} frames: {:?}", self.next_index, e);
                return Ok(None);
            }
        };

        let frame = &self.shared.frames[self.next_index];

        let copies = if matches!(frame.disposal_method, DisposalMethod::Previous) { 3 } else { 1 };
        let cost = (self.canvas.len() as u64).saturating_mul(copies);
        if !self.budget.charge(cost, self.next_index) {
            log_warn!("GIF frames exceed the composition work budget, stopping after {} frames", self.next_index);
            self.exhausted = true;
            return Ok(None);
        }

        let (width, height) = (self.shared.width, self.shared.height);
        let palette = Palette::new(color_table(frame, &self.global_color_table), frame.transparent_index);
        let layer = Layer {
            placement: placement(frame, width, height),
            indices: &indices,
            palette: &palette,
            update: match frame.disposal_method {
                DisposalMethod::None => Update::Keep,
                DisposalMethod::Background => Update::Clear,
                DisposalMethod::Previous => Update::Leave,
            },
        };

        let mut output = Vec::with_capacity(self.canvas.len());
        compose_layer(&layer, &mut self.canvas, &mut output, width as usize);
        let composed = ImageFrame::new(width, height, PixelData::RGBA8(output), frame.delay);

        self.shared.recycle(indices);
        self.next_index += 1;

        Ok(Some(composed))
    }

    fn frame_count_hint(&self) -> Option<usize> {
        if self.exhausted {
            return Some(0);
        }
        Some(self.shared.frames.len().saturating_sub(self.next_index))
    }
}
