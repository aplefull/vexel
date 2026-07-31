use std::collections::VecDeque;
use std::io::{Read, Seek};

use super::compose_simd::{compose_frame, Palette};
use super::lzw::decompress_lzw;
use super::types::{
    ApplicationExtension, DisposalMethod, GifApplicationExtensionData, GifColorTableInfo, GifCommentData, GifDecoder,
    GifFrameInfo, GifGraphicsControlData, GifHeaderData, GifImageDataInfo, GifImageDescriptorData, GifPlainTextData,
    GifSectionData, GifSectionInfo, GraphicsControlExtension, PlainTextExtension,
};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::frames::FrameSource;
use crate::utils::info::GifInfo;
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
            comments: Vec::new(),
            app_extensions: Vec::new(),
            plain_text_extensions: Vec::new(),
            sections: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> GifInfo {
        GifInfo { sections: self.sections.clone() }
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let start_offset = self.reader.stream_position().unwrap_or(0);

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
            buf[i..].starts_with(b"GIF87a")
                || buf[i..].starts_with(b"GIF89a")
                || buf[i..].starts_with(b"IF87a")
                || buf[i..].starts_with(b"IF89a")
        });
        let offset = sig_offset.unwrap_or(0);
        if offset > 0 || (!buf.starts_with(b"GIF87a") && !buf.starts_with(b"GIF89a")) {
            log_warn!("GIF signature at offset {}, possibly corrupted header", offset);
        }
        let sig_len = if buf[offset..].starts_with(b"GIF") { 6 } else { 5 };
        self.version =
            String::from_utf8_lossy(buf.get(offset + (sig_len - 3)..offset + sig_len).unwrap_or(b"87a")).to_string();
        self.reader.seek(std::io::SeekFrom::Start((offset + sig_len) as u64))?;

        self.canvas_width = self.reader.read_u16()?.swap_bytes() as u32;
        self.canvas_height = self.reader.read_u16()?.swap_bytes() as u32;

        if self.canvas_width == 0 || self.canvas_height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.canvas_width, height: self.canvas_height });
        }

        self.limits.reserve_buffer(self.canvas_width, self.canvas_height, 4)?;

        let packed_fields = self.reader.read_u8()?;

        self.global_color_table_flag = (packed_fields & 0b10000000) != 0;
        self.color_resolution = (packed_fields & 0b01110000) >> 4;
        self.sort_flag = (packed_fields & 0b00001000) != 0;
        self.size_of_global_color_table = packed_fields & 0b00000111;

        self.background_color_index = self.reader.read_u8()?;
        self.pixel_aspect_ratio = self.reader.read_u8()?;

        self.sections.push(GifSectionInfo {
            start_offset,
            data: GifSectionData::Header(GifHeaderData {
                version: self.version.clone(),
                canvas_width: self.canvas_width,
                canvas_height: self.canvas_height,
                global_color_table_flag: self.global_color_table_flag,
                color_resolution: self.color_resolution,
                sort_flag: self.sort_flag,
                size_of_global_color_table: self.size_of_global_color_table,
                background_color_index: self.background_color_index,
                pixel_aspect_ratio: self.pixel_aspect_ratio,
            }),
        });

        Ok(())
    }

    fn read_global_color_table(&mut self) {
        if !self.global_color_table_flag {
            return;
        }

        let start_offset = self.reader.stream_position().unwrap_or(0);
        let num_entries = 1u32 << (self.size_of_global_color_table + 1);
        let table_size = num_entries * 3;

        if let Err(e) = self.limits.reserve(table_size as u64) {
            log_warn!("Global color table too large, skipping: {:?}", e);
            return;
        }

        for _ in 0..table_size {
            let bit = match self.reader.read_u8() {
                Ok(bit) => bit,
                Err(e) => {
                    log_warn!("Error reading global color table: {:?}", e);
                    continue;
                }
            };

            self.global_color_table.push(bit);
        }

        let entries = self.global_color_table.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();

        self.sections.push(GifSectionInfo {
            start_offset,
            data: GifSectionData::GlobalColorTable(GifColorTableInfo {
                entry_count: num_entries,
                length: table_size,
                entries,
            }),
        });
    }

    fn read_application_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let block_size = self.reader.read_u8()?;
        if block_size != 11 {
            log_warn!("Invalid application extension block size: {}", block_size);
        }

        let mut identifier = Vec::with_capacity(8);
        let mut auth_code = Vec::with_capacity(3);

        for _ in 0..8 {
            identifier.push(self.reader.read_u8()?);
        }
        for _ in 0..3 {
            auth_code.push(self.reader.read_u8()?);
        }

        let identifier_str = String::from_utf8_lossy(identifier.as_slice()).to_string();
        let auth_code_str = String::from_utf8_lossy(auth_code.as_slice()).to_string();

        if identifier == b"NETSCAPE" && auth_code == b"2.0" {
            loop {
                let sub_block_size = self.reader.read_u8()?;
                if sub_block_size == 0 {
                    break;
                }

                let mut app_extension = ApplicationExtension {
                    loop_count: None,
                    buffer_size: None,
                    identifier: identifier_str.clone(),
                    auth_code: auth_code_str.clone(),
                    data: Vec::new(),
                };

                let block_id = self.reader.read_u8()?;
                match block_id {
                    1 => {
                        let count = self.reader.read_u16()?;
                        app_extension.loop_count = Some(count);

                        for _ in 0..(sub_block_size - 3) {
                            self.reader.read_u8()?;
                        }
                    }
                    2 => {
                        for _ in 0..(sub_block_size - 1) {
                            let buffer_size = self.reader.read_u8()?;
                            app_extension.buffer_size = Some(buffer_size);
                        }
                    }
                    _ => {
                        log_debug!("Skipping unknown Netscape extension block: {:#04x}", block_id);

                        for _ in 0..(sub_block_size - 1) {
                            self.reader.read_u8()?;
                        }
                    }
                }

                self.sections.push(GifSectionInfo {
                    start_offset,
                    data: GifSectionData::ApplicationExtension(GifApplicationExtensionData {
                        identifier: app_extension.identifier.clone(),
                        auth_code: app_extension.auth_code.clone(),
                        loop_count: app_extension.loop_count,
                        buffer_size: app_extension.buffer_size,
                        data_length: sub_block_size as u64,
                    }),
                });
                self.app_extensions.push(app_extension);
            }
        } else {
            let mut data = Vec::new();
            loop {
                let sub_block_size = self.reader.read_u8()? as usize;

                if sub_block_size == 0 {
                    break;
                }

                if let Err(e) = self.limits.reserve_usize(sub_block_size) {
                    log_warn!("Application extension data too large, truncating: {:?}", e);
                    for _ in 0..sub_block_size {
                        self.reader.read_u8()?;
                    }
                    break;
                }

                for _ in 0..sub_block_size {
                    data.push(self.reader.read_u8()?);
                }
            }

            let data_length = data.len() as u64;
            self.sections.push(GifSectionInfo {
                start_offset,
                data: GifSectionData::ApplicationExtension(GifApplicationExtensionData {
                    identifier: identifier_str.clone(),
                    auth_code: auth_code_str.clone(),
                    loop_count: None,
                    buffer_size: None,
                    data_length,
                }),
            });
            self.app_extensions.push(ApplicationExtension {
                loop_count: None,
                buffer_size: None,
                identifier: identifier_str,
                auth_code: auth_code_str,
                data,
            });
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn read_plain_text_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let block_size = self.reader.read_u8()?;
        if block_size != 12 {
            log_warn!("Invalid plain text extension block size: {}", block_size);
        }

        let left = self.reader.read_u16()?.swap_bytes();
        let top = self.reader.read_u16()?.swap_bytes();
        let width = self.reader.read_u16()?.swap_bytes();
        let height = self.reader.read_u16()?.swap_bytes();
        let cell_width = self.reader.read_u8()?;
        let cell_height = self.reader.read_u8()?;
        let foreground_color = self.reader.read_u8()?;
        let background_color = self.reader.read_u8()?;

        let mut text = String::new();
        loop {
            let sub_block_size = self.reader.read_u8()? as usize;
            if sub_block_size == 0 {
                break;
            }

            if let Err(e) = self.limits.reserve_usize(sub_block_size) {
                log_warn!("Plain text extension too large, truncating: {:?}", e);
                for _ in 0..sub_block_size {
                    self.reader.read_u8()?;
                }
                break;
            }

            let mut block = Vec::with_capacity(sub_block_size);
            for _ in 0..sub_block_size {
                block.push(self.reader.read_u8()?);
            }

            text.push_str(&String::from_utf8_lossy(&block));
        }

        self.sections.push(GifSectionInfo {
            start_offset,
            data: GifSectionData::PlainTextExtension(GifPlainTextData {
                left,
                top,
                width,
                height,
                cell_width,
                cell_height,
                foreground_color,
                background_color,
                text: text.clone(),
            }),
        });
        self.plain_text_extensions.push(PlainTextExtension {
            left,
            top,
            width,
            height,
            cell_width,
            cell_height,
            foreground_color,
            background_color,
            text,
        });

        Ok(())
    }

    fn read_comment_extension(&mut self, start_offset: u64) -> VexelResult<()> {
        let mut full_text = String::new();

        loop {
            let block_size = self.reader.read_u8()?;
            if block_size == 0 {
                break;
            }

            if let Err(e) = self.limits.reserve(block_size as u64) {
                log_warn!("Comment extension too large, truncating: {:?}", e);
                for _ in 0..block_size {
                    self.reader.read_u8()?;
                }
                break;
            }

            let mut block = Vec::with_capacity(block_size as usize);
            for _ in 0..block_size {
                let byte = self.reader.read_u8()?;
                block.push(byte);
            }

            let text = String::from_utf8_lossy(&block).into_owned();
            full_text.push_str(&text);
            self.comments.push(text);
        }

        self.sections.push(GifSectionInfo {
            start_offset,
            data: GifSectionData::CommentExtension(GifCommentData { text: full_text }),
        });

        Ok(())
    }

    fn read_frames(&mut self) -> VexelResult<()> {
        let mut current_gce: Option<(u64, GraphicsControlExtension)> = None;

        loop {
            let block_start = self.reader.stream_position().unwrap_or(0);
            let block_type = self.reader.read_u8()?;

            match block_type {
                // Image Separator (0x2C)
                0x2C => {
                    self.read_frame(block_start, current_gce.take().map(|(_, gce)| gce))?;
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
                        /*0x01 => {
                            self.read_plain_text_extension(block_start)?;
                        }*/
                        _ => {
                            log_warn!("Skipping unknown extension: {:#04x}", label);

                            loop {
                                let block_size = self.reader.read_u8()? as usize;
                                if block_size == 0 {
                                    break;
                                }
                                for _ in 0..block_size {
                                    self.reader.read_u8()?;
                                }
                            }
                        }
                    }
                }
                // End of image (0x3B)
                0x3B => {
                    self.sections.push(GifSectionInfo { start_offset: block_start, data: GifSectionData::Trailer });
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn read_frame(&mut self, descriptor_offset: u64, gce: Option<GraphicsControlExtension>) -> VexelResult<()> {
        let left = self.reader.read_u16()?.swap_bytes() as u32;
        let top = self.reader.read_u16()?.swap_bytes() as u32;
        let width = self.reader.read_u16()?.swap_bytes() as u32;
        let height = self.reader.read_u16()?.swap_bytes() as u32;

        let packed_fields = self.reader.read_u8()?;
        let local_color_table_flag = (packed_fields & 0b10000000) != 0;
        let interlace_flag = (packed_fields & 0b01000000) != 0;
        let sort_flag = (packed_fields & 0b00100000) != 0;
        let size_of_local_color_table = packed_fields & 0b00000111;

        self.sections.push(GifSectionInfo {
            start_offset: descriptor_offset,
            data: GifSectionData::ImageDescriptor(GifImageDescriptorData {
                left,
                top,
                width,
                height,
                local_color_table_flag,
                interlace_flag,
                sort_flag,
                size_of_local_color_table,
            }),
        });

        let mut local_color_table = Vec::new();
        if local_color_table_flag {
            let lct_offset = self.reader.stream_position().unwrap_or(0);
            let num_entries = 1u32 << (size_of_local_color_table + 1);
            let table_size = num_entries * 3;
            if let Err(e) = self.limits.reserve(table_size as u64) {
                log_warn!("Local color table too large, skipping frame: {:?}", e);
                return Ok(());
            }
            for _ in 0..table_size {
                local_color_table.push(self.reader.read_u8()?);
            }
            let entries = local_color_table.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            self.sections.push(GifSectionInfo {
                start_offset: lct_offset,
                data: GifSectionData::LocalColorTable(GifColorTableInfo {
                    entry_count: num_entries,
                    length: table_size,
                    entries,
                }),
            });
        }

        let image_data_offset = self.reader.stream_position().unwrap_or(0);
        let lzw_minimum_code_size = self.reader.read_u8()?;

        let mut data = Vec::new();
        let mut data_length: u64 = 1;
        loop {
            let block_size = match self.reader.read_u8() {
                Ok(0) => break,
                Ok(size) => size as usize,
                Err(e) => {
                    log_warn!("Error reading image sub-block size: {:?}", e);
                    break;
                }
            };

            data_length += 1 + block_size as u64;

            if let Err(e) = self.limits.reserve_usize(block_size) {
                log_warn!("Frame image data too large, truncating: {:?}", e);
                for _ in 0..block_size {
                    self.reader.read_u8().ok();
                }
                break;
            }

            let start = data.len();
            data.resize(start + block_size, 0);
            if let Err(e) = self.reader.read_exact(&mut data[start..]) {
                log_warn!("Error reading image sub-block data: {:?}", e);
                break;
            }
        }

        self.sections.push(GifSectionInfo {
            start_offset: image_data_offset,
            data: GifSectionData::ImageData(GifImageDataInfo { lzw_minimum_code_size, data_length }),
        });

        let frame = GifFrameInfo {
            left,
            top,
            width,
            height,
            local_color_table_flag,
            interlace_flag,
            sort_flag,
            size_of_local_color_table,
            local_color_table,
            lzw_minimum_code_size,
            user_input: gce.as_ref().map(|gce| gce.user_input).unwrap_or(false),
            transparent_index: gce.as_ref().filter(|gce| gce.transparency).map(|gce| gce.transparent_color_index),
            disposal_method: gce.as_ref().map(|gce| gce.disposal_method).unwrap_or(DisposalMethod::None),
            delay: gce.map(|gce| gce.delay).unwrap_or(100),
            data,
        };

        if self.frames.is_empty() {
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
        let disposal_method = match (packed >> 2) & 0x07 {
            0 => DisposalMethod::None,
            2 => DisposalMethod::Background,
            3 => DisposalMethod::Previous,
            _ => DisposalMethod::None,
        };

        let user_input = (packed & 0x02) != 0;
        let transparency = (packed & 0x01) != 0;

        let delay = (self.reader.read_u16()?).swap_bytes() * 10;

        let transparent_color_index = self.reader.read_u8()?;

        let terminator = self.reader.read_u8()?;
        if terminator != 0 {
            log_warn!("Invalid graphics control extension block terminator: {}", terminator);
        }

        self.sections.push(GifSectionInfo {
            start_offset,
            data: GifSectionData::GraphicsControlExtension(GifGraphicsControlData {
                disposal_method,
                user_input,
                transparent_color_index: transparency.then_some(transparent_color_index),
                delay,
            }),
        });

        Ok(GraphicsControlExtension { disposal_method, user_input, transparency, delay, transparent_color_index })
    }

    fn deinterlace_indices(width: u32, height: u32, indices: &[u8]) -> Vec<u8> {
        let mut result = vec![0u8; indices.len()];
        let passes = [(0usize, 8usize), (4, 8), (2, 4), (1, 2)];
        let row_stride = width as usize;
        let mut source_pos = 0;

        for (start, step) in passes {
            let mut y = start;
            while y < height as usize {
                let dst_start = y * row_stride;
                let dst_end = dst_start + row_stride;
                let src_end = source_pos + row_stride;

                if src_end <= indices.len() && dst_end <= result.len() {
                    result[dst_start..dst_end].copy_from_slice(&indices[source_pos..src_end]);
                } else {
                    log_warn!("Interlace index out of bounds at row {}", y);
                }

                source_pos += row_stride;
                y += step;
            }
        }

        result
    }

    fn read_structure(&mut self) -> VexelResult<()> {
        match self.read_header() {
            Ok(_) => {}
            Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
            Err(e) => {
                log_warn!("Error reading header, this might be critical! Error: {:?}", e);
            }
        };

        self.read_global_color_table();

        match self.read_frames() {
            Ok(_) => {}
            Err(e) => {
                log_warn!("Error reading frames, this might be critical! Error: {:?}", e);
            }
        };

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

    fn decode_frame_indices(frame: &GifFrameInfo) -> VexelResult<Vec<u8>> {
        let max_output = (frame.width as usize).saturating_mul(frame.height as usize).max(1);
        let mut indices = decompress_lzw(frame, max_output).inspect_err(|e| {
            log_warn!("Error decoding frame: {:?}", e);
        })?;

        if frame.interlace_flag {
            indices = Self::deinterlace_indices(frame.width, frame.height, &indices);
        }

        Ok(indices)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_structure()?;
        self.resolve_dimensions()?;

        let indices = match self.frames.first() {
            Some(frame) => Self::decode_frame_indices(frame)?,
            None => {
                let canvas_size = (self.width as usize).saturating_mul(self.height as usize).saturating_mul(4);
                self.limits.reserve_usize(canvas_size)?;
                return Ok(Image::from_pixels(self.width, self.height, PixelData::RGBA8(vec![0u8; canvas_size])));
            }
        };

        let mut composer = GifComposer::new(self.width, self.height, false, &mut self.limits)?;
        let frame = composer.compose(&self.frames[0], &indices, &self.global_color_table);

        Ok(Image::from_frame(frame))
    }

    pub(crate) fn into_frame_source(mut self) -> VexelResult<GifFrames<R>> {
        self.read_structure()?;
        self.resolve_dimensions()?;

        let needs_scratch = self.frames.iter().any(|f| matches!(f.disposal_method, DisposalMethod::Previous));
        let composer = GifComposer::new(self.width, self.height, needs_scratch, &mut self.limits)?;
        let frames = std::sync::Arc::new(std::mem::take(&mut self.frames));

        Ok(GifFrames {
            decoder: self,
            frames,
            composer,
            next_index: 0,
            batch: VecDeque::new(),
            decoded_upto: 0,
            pending: None,
            exhausted: false,
        })
    }
}

struct GifComposer {
    width: u32,
    height: u32,
    canvas: Vec<u8>,
    scratch: Vec<u8>,
    palette: Palette,
    palette_source: Vec<u8>,
}

impl GifComposer {
    fn new(width: u32, height: u32, needs_scratch: bool, limits: &mut Limits) -> VexelResult<GifComposer> {
        let canvas_size = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        limits.reserve_usize(canvas_size)?;
        let canvas = vec![0u8; canvas_size];

        let scratch = if needs_scratch {
            limits.reserve_usize(canvas_size)?;
            vec![0u8; canvas_size]
        } else {
            Vec::new()
        };

        Ok(GifComposer { width, height, canvas, scratch, palette: Palette::new(&[]), palette_source: Vec::new() })
    }

    fn sync_palette(&mut self, color_table: &[u8]) {
        if self.palette_source != color_table {
            self.palette = Palette::new(color_table);
            self.palette_source.clear();
            self.palette_source.extend_from_slice(color_table);
        }
    }

    fn compose(&mut self, frame: &GifFrameInfo, indices: &[u8], global_color_table: &[u8]) -> ImageFrame {
        if matches!(frame.disposal_method, DisposalMethod::Previous) && !self.scratch.is_empty() {
            self.scratch.copy_from_slice(&self.canvas);
        }

        let color_table =
            if frame.local_color_table_flag { frame.local_color_table.as_slice() } else { global_color_table };
        self.sync_palette(color_table);

        let width = self.width as usize;
        let height = self.height as usize;

        compose_frame(
            indices,
            &self.palette,
            frame.transparent_index,
            frame.left as usize,
            frame.top as usize,
            frame.width as usize,
            frame.height as usize,
            width,
            height,
            &mut self.canvas,
        );

        let composed =
            ImageFrame::new(self.width, self.height, PixelData::RGBA8(self.canvas.clone()), frame.delay as u32);

        self.dispose(frame);

        composed
    }

    fn dispose(&mut self, frame: &GifFrameInfo) {
        match frame.disposal_method {
            DisposalMethod::None => {}
            DisposalMethod::Background => {
                let clamped_h = frame.height.min(self.height.saturating_sub(frame.top));
                let clamped_w = frame.width.min(self.width.saturating_sub(frame.left));
                for y in 0..clamped_h {
                    let canvas_y = frame.top + y;
                    let row_base = (canvas_y * self.width + frame.left) as usize * 4;
                    let row_end = row_base + clamped_w as usize * 4;
                    if row_end <= self.canvas.len() {
                        self.canvas[row_base..row_end].fill(0);
                    }
                }
            }
            DisposalMethod::Previous => {
                if !self.scratch.is_empty() {
                    self.canvas.copy_from_slice(&self.scratch);
                }
            }
        }
    }
}

const LOOKAHEAD_BATCH: usize = 16;

pub(crate) struct GifFrames<R: Read + Seek + Sync> {
    decoder: GifDecoder<R>,
    frames: std::sync::Arc<Vec<GifFrameInfo>>,
    composer: GifComposer,
    next_index: usize,
    batch: VecDeque<Vec<u8>>,
    decoded_upto: usize,
    pending: Option<std::sync::mpsc::Receiver<VexelResult<Vec<Vec<u8>>>>>,
    exhausted: bool,
}

impl<R: Read + Seek + Sync> GifFrames<R> {
    fn decode_range(frames: &[GifFrameInfo]) -> VexelResult<Vec<Vec<u8>>> {
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            frames.par_iter().map(GifDecoder::<R>::decode_frame_indices).collect()
        }

        #[cfg(not(feature = "rayon"))]
        frames.iter().map(GifDecoder::<R>::decode_frame_indices).collect()
    }

    #[cfg(feature = "rayon")]
    fn start_prefetch(&mut self) {
        if self.pending.is_some() || self.decoded_upto >= self.frames.len() {
            return;
        }

        let start = self.decoded_upto;
        let end = (start + LOOKAHEAD_BATCH).min(self.frames.len());
        self.decoded_upto = end;

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.pending = Some(rx);

        let frames = self.frames.clone();
        rayon::spawn(move || {
            let _ = tx.send(Self::decode_range(&frames[start..end]));
        });
    }

    fn fill_batch(&mut self) -> VexelResult<()> {
        #[cfg(feature = "rayon")]
        if let Some(rx) = self.pending.take() {
            let decoded = rx.recv().map_err(|_| VexelError::Custom("GIF prefetch worker disappeared".into()))??;
            self.batch = decoded.into();
            self.start_prefetch();
            return Ok(());
        }

        let start = self.next_index;
        let end = (start + LOOKAHEAD_BATCH).min(self.frames.len());
        self.decoded_upto = end;
        self.batch = Self::decode_range(&self.frames[start..end])?.into();

        #[cfg(feature = "rayon")]
        self.start_prefetch();

        Ok(())
    }
}

impl<R: Read + Seek + Sync> FrameSource for GifFrames<R> {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        if self.exhausted || self.next_index >= self.frames.len() {
            return Ok(None);
        }

        if self.batch.is_empty() {
            if let Err(e) = self.fill_batch() {
                self.exhausted = true;
                if self.next_index == 0 {
                    return Err(e);
                }

                log_warn!("Stopping GIF frame iteration after {} frames: {:?}", self.next_index, e);
                return Ok(None);
            }
        }

        let Some(indices) = self.batch.pop_front() else {
            self.exhausted = true;
            return Ok(None);
        };

        let frame = &self.frames[self.next_index];
        let composed = self.composer.compose(frame, &indices, &self.decoder.global_color_table);
        self.next_index += 1;

        Ok(Some(composed))
    }

    fn frame_count_hint(&self) -> Option<usize> {
        Some(self.frames.len().saturating_sub(self.next_index))
    }
}
