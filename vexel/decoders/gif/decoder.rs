use crate::bitreader::BitReader;
use crate::utils::error::VexelResult;
use crate::utils::info::GifInfo;
use crate::{log_debug, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use rayon::prelude::*;
use std::io::{Read, Seek};

use super::compose_simd::compose_frame;
use super::lzw::decompress_lzw;
use super::types::{ApplicationExtension, DisposalMethod, GifDecoder, GifFrameInfo, GraphicsControlExtension, PlainTextExtension};

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

        let decoded_indices: Vec<Vec<u8>> = self
            .frames
            .par_iter()
            .map(|frame| {
                let mut indices = match decompress_lzw(frame) {
                    Ok(i) => i,
                    Err(e) => {
                        log_warn!("Error decoding frame: {:?}", e);
                        return Err(e);
                    }
                };

                if frame.interlace_flag {
                    indices = Self::deinterlace_indices(frame.width, frame.height, &indices);
                }

                Ok(indices)
            })
            .collect::<VexelResult<Vec<_>>>()?;

        let canvas_size = (self.width * self.height * 4) as usize;
        let mut canvas = vec![0u8; canvas_size];
        let mut image_frames = Vec::new();

        for (frame, indices) in self.frames.iter().zip(decoded_indices) {
            let saved_canvas = match frame.disposal_method {
                DisposalMethod::Previous | DisposalMethod::Background => Some(canvas.clone()),
                DisposalMethod::None => None,
            };

            let color_table = if frame.local_color_table_flag {
                &frame.local_color_table
            } else {
                &self.global_color_table
            };

            compose_frame(
                &indices,
                color_table,
                frame.transparent_index,
                frame.left as usize,
                frame.top as usize,
                frame.width as usize,
                frame.height as usize,
                self.width as usize,
                self.height as usize,
                &mut canvas,
            );

            match frame.disposal_method {
                DisposalMethod::None => {
                    let next_canvas = canvas.clone();
                    image_frames.push(ImageFrame {
                        width: self.width,
                        height: self.height,
                        pixels: PixelData::RGBA8(canvas),
                        delay: frame.delay as u32,
                    });
                    canvas = next_canvas;
                }
                DisposalMethod::Background => {
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
                    image_frames.push(ImageFrame {
                        width: self.width,
                        height: self.height,
                        pixels: PixelData::RGBA8(canvas),
                        delay: frame.delay as u32,
                    });
                    canvas = next_canvas;
                }
                DisposalMethod::Previous => {
                    image_frames.push(ImageFrame {
                        width: self.width,
                        height: self.height,
                        pixels: PixelData::RGBA8(canvas),
                        delay: frame.delay as u32,
                    });
                    canvas = saved_canvas.unwrap_or_else(|| vec![0u8; canvas_size]);
                }
            }
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
