use crate::bitreader::BitReader;
use crate::utils::error::VexelResult;
use crate::utils::info::GifInfo;
use crate::utils::traits::SafeAccess;
use crate::{log_debug, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use rayon::prelude::*;
use serde::Serialize;
use std::fmt::Debug;
use std::io::{Read, Seek};
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifFrameInfo {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub local_color_table_flag: bool,
    pub interlace_flag: bool,
    pub sort_flag: bool,
    pub size_of_local_color_table: u8,
    pub local_color_table: Vec<u8>,
    pub lzw_minimum_code_size: u8,
    pub transparent_index: Option<u8>,
    pub disposal_method: DisposalMethod,
    pub delay: u16,
    pub user_input: bool,
    pub data: Vec<u8>,
}

pub struct GifDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    canvas_width: u32,
    canvas_height: u32,
    version: String,
    global_color_table_flag: bool,
    color_resolution: u8,
    sort_flag: bool,
    size_of_global_color_table: u8,
    background_color_index: u8,
    pixel_aspect_ratio: u8,
    global_color_table: Vec<u8>,
    frames: Vec<GifFrameInfo>,
    comments: Vec<String>,
    app_extensions: Vec<ApplicationExtension>,
    plain_text_extensions: Vec<PlainTextExtension>,
    reader: BitReader<R>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ApplicationExtension {
    pub loop_count: Option<u16>,
    pub buffer_size: Option<u8>,
    pub identifier: String,
    pub auth_code: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct GraphicsControlExtension {
    pub disposal_method: DisposalMethod,
    pub user_input: bool,
    pub transparency: bool,
    pub delay: u16,
    pub transparent_color_index: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PlainTextExtension {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cell_width: u8,
    pub cell_height: u8,
    pub foreground_color: u8,
    pub background_color: u8,
    pub text: String,
}

#[derive(Debug, Copy, Clone, Serialize, Tsify)]
pub enum DisposalMethod {
    None,
    Background,
    Previous,
}

impl<R: Read + Seek + Sync> GifDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
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
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> GifInfo {
        GifInfo {
            width: self.width,
            height: self.height,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            version: self.version.clone(),
            global_color_table_flag: self.global_color_table_flag,
            color_resolution: self.color_resolution,
            sort_flag: self.sort_flag,
            size_of_global_color_table: self.size_of_global_color_table,
            background_color_index: self.background_color_index,
            pixel_aspect_ratio: self.pixel_aspect_ratio,
            global_color_table: self.global_color_table.clone(),
            frames: self.frames.clone(),
            comments: self.comments.clone(),
            app_extensions: self.app_extensions.clone(),
            plain_text_extensions: self.plain_text_extensions.clone(),
        }
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let mut buf = [0u8; 10];
        let mut n = 0;
        while n < buf.len() {
            match self.reader.read_u8() {
                Ok(b) => { buf[n] = b; n += 1; }
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
        self.version = String::from_utf8_lossy(buf.get(offset + (sig_len - 3)..offset + sig_len).unwrap_or(b"87a")).to_string();
        self.reader.seek(std::io::SeekFrom::Start((offset + sig_len) as u64))?;

        self.canvas_width = self.reader.read_u16()?.swap_bytes() as u32;
        self.canvas_height = self.reader.read_u16()?.swap_bytes() as u32;

        let packed_fields = self.reader.read_u8()?;

        self.global_color_table_flag = (packed_fields & 0b10000000) != 0;
        self.color_resolution = (packed_fields & 0b01110000) >> 4;
        self.sort_flag = (packed_fields & 0b00001000) != 0;
        self.size_of_global_color_table = packed_fields & 0b00000111;

        self.background_color_index = self.reader.read_u8()?;

        self.pixel_aspect_ratio = self.reader.read_u8()?;

        Ok(())
    }

    fn read_global_color_table(&mut self) {
        if !self.global_color_table_flag {
            return;
        }

        let num_entries = 1 << (self.size_of_global_color_table + 1);
        let table_size = num_entries * 3;

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
    }

    fn read_application_extension(&mut self) -> VexelResult<()> {
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

        if identifier == b"NETSCAPE" && auth_code == b"2.0" {
            loop {
                let sub_block_size = self.reader.read_u8()?;
                if sub_block_size == 0 {
                    break;
                }

                let mut app_extension = ApplicationExtension {
                    loop_count: None,
                    buffer_size: None,
                    identifier: String::from_utf8_lossy(identifier.as_slice()).to_string(),
                    auth_code: String::from_utf8_lossy(auth_code.as_slice()).to_string(),
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

                self.app_extensions.push(app_extension);
            }
        } else {
            let mut data = Vec::new();
            loop {
                let sub_block_size = self.reader.read_u8()? as usize;

                if sub_block_size == 0 {
                    break;
                }

                for _ in 0..sub_block_size {
                    data.push(self.reader.read_u8()?);
                }
            }

            self.app_extensions.push(ApplicationExtension {
                loop_count: None,
                buffer_size: None,
                identifier: String::from_utf8_lossy(identifier.as_slice()).to_string(),
                auth_code: String::from_utf8_lossy(auth_code.as_slice()).to_string(),
                data,
            });
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn read_plain_text_extension(&mut self) -> VexelResult<()> {
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

            let mut block = Vec::with_capacity(sub_block_size);
            for _ in 0..sub_block_size {
                block.push(self.reader.read_u8()?);
            }

            text.push_str(&String::from_utf8_lossy(&block));
        }

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

    fn read_comment_extension(&mut self) -> VexelResult<()> {
        loop {
            let block_size = self.reader.read_u8()?;
            if block_size == 0 {
                break;
            }

            let mut block = Vec::with_capacity(block_size as usize);
            for _ in 0..block_size {
                let byte = self.reader.read_u8()?;
                block.push(byte);
            }

            self.comments.push(String::from_utf8(block).unwrap());
        }

        Ok(())
    }

    fn read_frames(&mut self) -> VexelResult<()> {
        let mut current_gce: Option<GraphicsControlExtension> = None;

        loop {
            let block_type = self.reader.read_u8()?;

            match block_type {
                // Image Separator (0x2C)
                0x2C => {
                    self.read_frame(current_gce.take())?;
                }
                // Extension Introducer (0x21)
                0x21 => {
                    let label = self.reader.read_u8()?;
                    match label {
                        0xF9 => {
                            current_gce = Some(self.read_graphics_control_extension()?);
                        }
                        0xFE => {
                            self.read_comment_extension()?;
                        }
                        0xFF => {
                            self.read_application_extension()?;
                        }
                        /*0x01 => {
                            self.read_plain_text_extension()?;
                        }*/
                        _ => {
                            log_warn!("Skipping unknown extension: {:#04x}", label);

                            loop {
                                let block_size = self.reader.read_u8()? as usize;
                                if block_size == 0 {
                                    break;
                                }
                                // Skip block data
                                for _ in 0..block_size {
                                    self.reader.read_u8()?;
                                }
                            }
                        }
                    }
                }
                // End of image (0x3B)
                0x3B => {
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn read_frame(&mut self, gce: Option<GraphicsControlExtension>) -> VexelResult<()> {
        let mut frame = GifFrameInfo {
            left: self.reader.read_u16()?.swap_bytes() as u32,
            top: self.reader.read_u16()?.swap_bytes() as u32,
            width: self.reader.read_u16()?.swap_bytes() as u32,
            height: self.reader.read_u16()?.swap_bytes() as u32,
            local_color_table_flag: false,
            interlace_flag: false,
            sort_flag: false,
            size_of_local_color_table: 0,
            local_color_table: Vec::new(),
            lzw_minimum_code_size: 0,
            user_input: gce.as_ref().map(|gce| gce.user_input).unwrap_or(false),
            transparent_index: gce
                .as_ref()
                .filter(|gce| gce.transparency)
                .map(|gce| gce.transparent_color_index),
            disposal_method: gce
                .as_ref()
                .map(|gce| gce.disposal_method)
                .unwrap_or(DisposalMethod::None),
            delay: gce.map(|gce| gce.delay).unwrap_or(100),
            data: Vec::new(),
        };

        let packed_fields = self.reader.read_u8()?;
        frame.local_color_table_flag = (packed_fields & 0b10000000) != 0;
        frame.interlace_flag = (packed_fields & 0b01000000) != 0;
        frame.sort_flag = (packed_fields & 0b00100000) != 0;
        frame.size_of_local_color_table = packed_fields & 0b00000111;

        if frame.local_color_table_flag {
            let table_size = 3 * (1 << (frame.size_of_local_color_table + 1));
            for _ in 0..table_size {
                frame.local_color_table.push(self.reader.read_u8()?);
            }
        }

        frame.lzw_minimum_code_size = self.reader.read_u8()?;

        loop {
            let block_size = match self.reader.read_u8() {
                Ok(0) => break,
                Ok(size) => size as usize,
                Err(e) => {
                    log_warn!("Error reading image sub-block size: {:?}", e);
                    break;
                }
            };

            let mut buffer = vec![0u8; block_size];
            match self.reader.read_exact(&mut buffer) {
                Ok(_) => {}
                Err(e) => {
                    log_warn!("Error reading image sub-block data: {:?}", e);
                    frame.data.extend_from_slice(&buffer);
                    break;
                }
            }

            frame.data.extend(buffer);
        }

        if self.frames.is_empty() {
            self.width = self.canvas_width;
            self.height = self.canvas_height;
        }

        self.frames.push(frame);

        Ok(())
    }

    fn read_graphics_control_extension(&mut self) -> VexelResult<GraphicsControlExtension> {
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

        Ok(GraphicsControlExtension {
            disposal_method,
            user_input,
            transparency,
            delay,
            transparent_color_index,
        })
    }

    fn decompress_lzw(&self, frame: &GifFrameInfo) -> VexelResult<Vec<u8>> {
        let min_code_size = frame.lzw_minimum_code_size.clamp(2, 8) as u32;
        let clear_code = 1u32 << min_code_size;
        let end_code = clear_code + 1;

        let mut table: Vec<Vec<u8>> = Vec::with_capacity(4096);
        for i in 0..clear_code {
            table.push(vec![i as u8]);
        }
        table.push(vec![]);
        table.push(vec![]);

        let data = &frame.data;
        let data_len = data.len();

        let mut bit_buf: u64 = 0;
        let mut n_bits: u32 = 0;
        let mut byte_pos: usize = 0;

        let read_code = |bit_buf: &mut u64, n_bits: &mut u32, byte_pos: &mut usize, width: u32| -> Option<u32> {
            while *n_bits < width {
                if *byte_pos >= data_len {
                    return None;
                }
                let space = (64 - *n_bits) / 8;
                let available = (data_len - *byte_pos).min(space as usize).min(4);
                for _ in 0..available {
                    *bit_buf |= (data[*byte_pos] as u64) << *n_bits;
                    *byte_pos += 1;
                    *n_bits += 8;
                }
            }
            let code = (*bit_buf & ((1u64 << width) - 1)) as u32;
            *bit_buf >>= width;
            *n_bits -= width;
            Some(code)
        };

        let mut result: Vec<u8> = Vec::with_capacity(frame.width as usize * frame.height as usize);

        let mut code_size = min_code_size + 1;
        let mut next_code = end_code + 1;
        let mut prev_code: Option<u32> = None;

        loop {
            let code = match read_code(&mut bit_buf, &mut n_bits, &mut byte_pos, code_size) {
                Some(c) => c,
                None => break,
            };

            if code == clear_code {
                code_size = min_code_size + 1;
                next_code = end_code + 1;
                table.truncate(next_code as usize);
                prev_code = None;
                continue;
            }

            if code == end_code {
                break;
            }

            if let Some(prev) = prev_code {
                if code < next_code {
                    let (first, seq_clone) = match table.get(code as usize) {
                        Some(seq) if !seq.is_empty() => (seq[0], seq.to_vec()),
                        _ => {
                            log_warn!("Invalid LZW code: {}", code);
                            prev_code = Some(code);
                            if next_code >= (1 << code_size) && code_size < 12 {
                                code_size += 1;
                            }
                            continue;
                        }
                    };
                    result.extend_from_slice(&seq_clone);
                    if next_code < 4096 {
                        if let Some(prev_seq) = table.get(prev as usize) {
                            let mut new_entry = prev_seq.to_vec();
                            new_entry.push(first);
                            table.push(new_entry);
                            next_code += 1;
                        }
                    }
                } else if code == next_code {
                    if let Some(prev_seq) = table.get(prev as usize) {
                        let first = prev_seq.first().copied().unwrap_or(0);
                        let mut seq = prev_seq.to_vec();
                        seq.push(first);
                        result.extend_from_slice(&seq);
                        if next_code < 4096 {
                            table.push(seq);
                            next_code += 1;
                        }
                    }
                } else {
                    log_warn!("Invalid LZW code: {}", code);
                }
            } else {
                if let Some(seq) = table.get(code as usize) {
                    result.extend_from_slice(seq);
                }
            }

            prev_code = Some(code);

            if next_code >= (1 << code_size) && code_size < 12 {
                code_size += 1;
            }
        }

        Ok(result)
    }

    fn deinterlace(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut result = vec![0; data.len()];

        // GIF interlacing passes:
        // Pass 1: Starting at row 0, every 8th row
        // Pass 2: Starting at row 4, every 8th row
        // Pass 3: Starting at row 2, every 4th row
        // Pass 4: Starting at row 1, every 2nd row

        let passes = [(0, 8), (4, 8), (2, 4), (1, 2)];

        let mut source_pos = 0;

        for pass in passes.iter() {
            for y in (pass.0..height).step_by(pass.1) {
                let row_start = (y * width * 4) as usize;
                let row_end = row_start + (width * 4) as usize;
                let source_pos_to = source_pos + (width * 4) as usize;

                if let Ok(source_slice) = data.get_range_safe(source_pos..source_pos_to) {
                    if result.check_range(row_start..row_end).is_err() {
                        log_warn!("Row end out of bounds: {} (len {})", row_end, result.len());
                        continue;
                    }

                    result[row_start..row_end].copy_from_slice(source_slice);
                } else {
                    log_warn!("Failed to get source slice for row {}", y);
                }

                source_pos += (width * 4) as usize;
            }
        }

        result
    }

    fn decode_frame(&self, frame: &GifFrameInfo) -> VexelResult<Vec<u8>> {
        let indices = self.decompress_lzw(frame)?;
        let mut image_data = Vec::with_capacity(frame.width as usize * frame.height as usize * 4);

        let color_table = if frame.local_color_table_flag {
            &frame.local_color_table
        } else {
            &self.global_color_table
        };

        for &index in &indices {
            let color_index = index as usize * 3;

            if let Some(transparent_idx) = frame.transparent_index {
                if index == transparent_idx {
                    image_data.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
            }

            if color_index + 2 <= color_table.len() {
                image_data.push(*color_table.get(color_index).unwrap_or(&0));
                image_data.push(*color_table.get(color_index + 1).unwrap_or(&0));
                image_data.push(*color_table.get(color_index + 2).unwrap_or(&0));
                image_data.push(255);
            }
        }

        Ok(image_data)
    }

    fn compose_frame(&self, frame: &GifFrameInfo, previous_canvas: Option<Vec<u8>>) -> VexelResult<Vec<u8>> {
        let canvas_size = (self.width * self.height * 4) as usize;

        let mut canvas = match previous_canvas {
            Some(prev) => prev,
            None => vec![0u8; canvas_size],
        };

        // Calculate frame boundaries
        let frame_width = frame.width;
        let frame_height = frame.height;
        let left = frame.left;
        let top = frame.top;

        // Compose frame onto canvas
        for y in 0..frame_height {
            let canvas_y = top + y;
            if canvas_y >= self.height {
                continue;
            }

            for x in 0..frame_width {
                let canvas_x = left + x;
                if canvas_x >= self.width {
                    continue;
                }

                let frame_pixel_index = ((y * frame_width + x) * 4) as usize;
                let canvas_pixel_index = ((canvas_y * self.width + canvas_x) * 4) as usize;

                // Only copy non-transparent pixels
                let alpha = match frame.data.get_safe(frame_pixel_index + 3) {
                    Ok(a) => *a,
                    Err(_) => continue,
                };

                if alpha > 0 {
                    if canvas.check_range(canvas_pixel_index..canvas_pixel_index + 4).is_err() {
                        log_warn!(
                            "Canvas pixel index out of bounds: {} (len {})",
                            canvas_pixel_index,
                            canvas.len()
                        );
                        continue;
                    }

                    let src = match frame.data.get_range_safe(frame_pixel_index..frame_pixel_index + 4) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    canvas[canvas_pixel_index..canvas_pixel_index + 4].copy_from_slice(src);
                }
            }
        }

        Ok(canvas)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.read_header() {
            Ok(_) => {}
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

        let mut previous_canvas: Option<Vec<u8>> = None;

        let decoded_frames: Vec<GifFrameInfo> = self
            .frames
            .par_iter()
            .map(|frame| {
                let mut frame_pixels = match self.decode_frame(frame) {
                    Ok(pixels) => pixels,
                    Err(e) => {
                        log_warn!("Error decoding frame: {:?}", e);
                        return Err(e);
                    }
                };

                if frame.interlace_flag {
                    frame_pixels = Self::deinterlace(frame.width, frame.height, &frame_pixels);
                }

                Ok(GifFrameInfo {
                    left: frame.left,
                    top: frame.top,
                    width: frame.width,
                    height: frame.height,
                    local_color_table_flag: frame.local_color_table_flag,
                    interlace_flag: frame.interlace_flag,
                    sort_flag: frame.sort_flag,
                    size_of_local_color_table: frame.size_of_local_color_table,
                    local_color_table: frame.local_color_table.clone(),
                    lzw_minimum_code_size: frame.lzw_minimum_code_size,
                    transparent_index: frame.transparent_index,
                    disposal_method: frame.disposal_method,
                    delay: frame.delay,
                    user_input: frame.user_input,
                    data: frame_pixels,
                })
            })
            .collect::<VexelResult<Vec<_>>>()?;

        let mut image_frames = Vec::new();

        for (frame_index, frame) in decoded_frames.iter().enumerate() {
            let saved_canvas = match frame.disposal_method {
                DisposalMethod::Previous | DisposalMethod::Background => previous_canvas.clone(),
                DisposalMethod::None => None,
            };

            let canvas = match self.compose_frame(frame, previous_canvas) {
                Ok(canvas) => canvas,
                Err(e) => {
                    log_warn!("Error composing frame. Skipping frame {}. Error: {:?}", frame_index, e);
                    previous_canvas = saved_canvas;
                    continue;
                }
            };

            previous_canvas = match frame.disposal_method {
                DisposalMethod::None => {
                    Some(canvas.clone())
                }
                DisposalMethod::Background => {
                    let canvas_size = (self.width * self.height * 4) as usize;
                    let mut next_canvas = saved_canvas.unwrap_or_else(|| vec![0u8; canvas_size]);
                    let clamped_h = frame.height.min(self.height.saturating_sub(frame.top));
                    let clamped_w = frame.width.min(self.width.saturating_sub(frame.left));
                    for y in 0..clamped_h {
                        let canvas_y = frame.top + y;
                        let row_base = (canvas_y * self.width + frame.left) as usize * 4;
                        let row_end = row_base + clamped_w as usize * 4;
                        if row_end <= next_canvas.len() {
                            next_canvas[row_base..row_end].fill(0);
                        }
                    }
                    Some(next_canvas)
                }
                DisposalMethod::Previous => saved_canvas,
            };

            image_frames.push(ImageFrame {
                width: self.width,
                height: self.height,
                pixels: PixelData::RGBA8(canvas),
                delay: frame.delay as u32,
            });
        }

        if self.width == 0 {
            self.width = self.canvas_width;
        }

        if self.height == 0 {
            self.height = self.canvas_height;
        }

        if self.width == 0 || self.height == 0 {
            return Err(crate::utils::error::VexelError::InvalidDimensions {
                width: self.width,
                height: self.height,
            });
        }

        Ok(Image::new(self.width, self.height, PixelFormat::RGBA8, image_frames))
    }
}
