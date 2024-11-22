use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::io::{Read, Seek};
use crate::bitreader::BitReader;
use crate::{log_warn, Image, ImageFrame, PixelData, PixelFormat};

pub struct FrameInfo {
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
    frames: Vec<FrameInfo>,
    comments: Vec<String>,
    app_extensions: Vec<ApplicationExtension>,
    plain_text_extensions: Vec<PlainTextExtension>,
    reader: BitReader<R>,
}

#[derive(Debug)]
pub struct ApplicationExtension {
    pub loop_count: Option<u16>,
    pub buffer_size: Option<u8>,
    pub identifier: String,
    pub auth_code: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct GraphicsControlExtension {
    disposal_method: DisposalMethod,
    user_input: bool,
    transparency: bool,
    delay: u16,
    transparent_color_index: u8,
}

#[derive(Debug)]
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

#[derive(Debug, Copy, Clone)]
pub enum DisposalMethod {
    None,
    Background,
    Previous,
}

impl Debug for FrameInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameInfo")
            .field("left", &self.left)
            .field("top", &self.top)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("local_color_table_flag", &self.local_color_table_flag)
            .field("interlace_flag", &self.interlace_flag)
            .field("sort_flag", &self.sort_flag)
            .field("size_of_local_color_table", &self.size_of_local_color_table)
            .field("local_color_table", &self.local_color_table)
            .field("lzw_minimum_code_size", &self.lzw_minimum_code_size)
            .field("transparent_index", &self.transparent_index)
            .field("disposal_method", &self.disposal_method)
            .field("delay", &self.delay)
            .field("data", &self.data.iter().len())
            .finish()
    }
}

impl<R: Read + Seek> Debug for GifDecoder<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JpegLsDecoder")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("canvas_width", &self.canvas_width)
            .field("canvas_height", &self.canvas_height)
            .field("version", &self.version)
            .field("global_color_table_flag", &self.global_color_table_flag)
            .field("color_resolution", &self.color_resolution)
            .field("sort_flag", &self.sort_flag)
            .field("size_of_global_color_table", &self.size_of_global_color_table)
            .field("background_color_index", &self.background_color_index)
            .field("pixel_aspect_ratio", &self.pixel_aspect_ratio)
            .field("global_color_table", &self.global_color_table)
            .field("frames", &self.frames)
            .field("comments", &self.comments)
            .field("app_extensions", &self.app_extensions)
            .finish()
    }
}

impl<R: Read + Seek> GifDecoder<R> {
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

    fn read_header(&mut self) -> Result<(), std::io::Error> {
        // Skip the magic number
        self.reader.read_bits(24)?;

        // Read the version
        let version = self.reader.read_bits(24)?;
        self.version = String::from_utf8(version.to_be_bytes().to_vec()).unwrap();

        // Read canvas width and height
        self.canvas_width = (self.reader.read_u16()?).swap_bytes() as u32;
        self.canvas_height = (self.reader.read_u16()?).swap_bytes() as u32;

        // Read the packed fields
        let packed_fields = self.reader.read_u8()?;

        self.global_color_table_flag = (packed_fields & 0b10000000) != 0;
        self.color_resolution = ((packed_fields & 0b01110000) >> 4);
        self.sort_flag = (packed_fields & 0b00001000) != 0;
        self.size_of_global_color_table = (packed_fields & 0b00000111);

        // Read the background color index
        self.background_color_index = self.reader.read_u8()?;

        // Read the pixel aspect ratio
        self.pixel_aspect_ratio = self.reader.read_u8()?;

        Ok(())
    }

    fn read_global_color_table(&mut self) -> Result<(), std::io::Error> {
        if !self.global_color_table_flag {
            return Ok(());
        }

        let num_entries = 1 << (self.size_of_global_color_table + 1);
        let table_size = num_entries * 3;

        for _ in 0..table_size {
            self.global_color_table.push(self.reader.read_u8()?);
        }

        Ok(())
    }

    fn read_application_extension(&mut self) -> Result<(), std::io::Error> {
        let block_size = self.reader.read_u8()?;
        if block_size != 11 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid application extension block size",
            ));
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
                        
                        // Skip remaining bytes in sub-block
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
                        log_warn!("Skipping unknown Netscape extension block: {:#04x}", block_id);
                        
                        for _ in 0..(sub_block_size - 1) {
                            self.reader.read_u8()?;
                        }
                    }
                }
            }
        }

        // Read application data
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

        Ok(())
    }

    fn read_plain_text_extension(&mut self) -> Result<(), std::io::Error> {
        let block_size = self.reader.read_u8()?;
        if block_size != 12 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid plain text extension block size",
            ));
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

    fn read_comment_extension(&mut self) -> Result<(), std::io::Error> {
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

    fn read_frames(&mut self) -> Result<(), std::io::Error> {
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
                        0x01 => {
                            self.read_plain_text_extension()?;
                        }
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
                _ => {
                    log_warn!("Skipping unknown block type: {:#04x}", block_type);
                }
            }
        }

        Ok(())
    }

    fn read_frame(&mut self, gce: Option<GraphicsControlExtension>) -> Result<(), std::io::Error> {
        let mut frame = FrameInfo {
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
            transparent_index: gce.as_ref().filter(|gce| gce.transparency).map(|gce| gce.transparent_color_index),
            disposal_method: gce.as_ref().map(|gce| gce.disposal_method).unwrap_or(DisposalMethod::None),
            delay: gce.map(|gce| gce.delay).unwrap_or(0),
            data: Vec::new(),
        };

        // Read packed fields
        let packed_fields = self.reader.read_u8()?;
        frame.local_color_table_flag = (packed_fields & 0b10000000) != 0;
        frame.interlace_flag = (packed_fields & 0b01000000) != 0;
        frame.sort_flag = (packed_fields & 0b00100000) != 0;
        frame.size_of_local_color_table = packed_fields & 0b00000111;

        // Read local color table if present
        if frame.local_color_table_flag {
            let table_size = 3 * (1 << (frame.size_of_local_color_table + 1));
            for _ in 0..table_size {
                frame.local_color_table.push(self.reader.read_u8()?);
            }
        }

        // Read LZW minimum code size
        frame.lzw_minimum_code_size = self.reader.read_u8()?;

        // Read image data blocks
        loop {
            let block_size = self.reader.read_u8()? as usize;
            if block_size == 0 {
                break;
            }

            // Read block data
            for _ in 0..block_size {
                frame.data.push(self.reader.read_u8()?);
            }
        }

        // Update decoder dimensions based on first frame
        if self.frames.is_empty() {
            self.width = frame.width;
            self.height = frame.height;
        }

        self.frames.push(frame);
        Ok(())
    }

    fn read_graphics_control_extension(&mut self) -> Result<GraphicsControlExtension, std::io::Error> {
        // Read block size (should be 4)
        let block_size = self.reader.read_u8()?;
        if block_size != 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid graphics control extension block size",
            ));
        }

        // Read packed field
        let packed = self.reader.read_u8()?;
        let disposal_method = match (packed >> 2) & 0x07 {
            0 => DisposalMethod::None,
            2 => DisposalMethod::Background,
            3 => DisposalMethod::Previous,
            _ => DisposalMethod::None,
        };

        let user_input = (packed & 0x02) != 0;
        let transparency = (packed & 0x01) != 0;

        // Read delay time
        let delay = (self.reader.read_u16()?).swap_bytes();

        // Read transparent color index
        let transparent_color_index = self.reader.read_u8()?;

        // Read block terminator
        let terminator = self.reader.read_u8()?;
        if terminator != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid graphics control extension terminator",
            ));
        }

        Ok(GraphicsControlExtension {
            disposal_method,
            user_input,
            transparency,
            delay,
            transparent_color_index,
        })
    }

    // TODO maybe use bitreader here as well
    fn decompress_lzw(&self, frame: &FrameInfo) -> Result<Vec<u8>, std::io::Error> {
        let min_code_size = frame.lzw_minimum_code_size;
        let clear_code = 1 << min_code_size;
        let end_code = clear_code + 1;

        let mut code_size = min_code_size + 1;
        let mut next_code = end_code + 1;
        let mut dictionary: HashMap<u16, Vec<u8>> = HashMap::new();

        for i in 0..clear_code {
            dictionary.insert(i, Vec::from([i as u8]));
        }

        let mut result = Vec::new();
        let mut current_bits = Vec::new();
        let mut current_byte_pos = 0;

        let read_code = |pos: &mut usize, bits: &Vec<u8>, code_size: u8| -> Option<u16> {
            if *pos + code_size as usize > bits.len() * 8 {
                return None;
            }

            let mut code = 0u16;
            let mut current_bit = 0;

            while current_bit < code_size {
                let byte_pos = *pos / 8;
                if byte_pos >= bits.len() {
                    return None;
                }

                let bit_pos = *pos % 8;
                let bit = (bits[byte_pos] >> bit_pos) & 1;
                code |= (bit as u16) << current_bit;

                *pos += 1;
                current_bit += 1;
            }

            Some(code)
        };

        for &byte in &frame.data {
            current_bits.push(byte);
        }

        let mut prev_code: Option<u16> = None;

        while let Some(code) = read_code(&mut current_byte_pos, &current_bits, code_size) {
            if code == clear_code {
                code_size = min_code_size + 1;
                next_code = end_code + 1;
                dictionary.clear();
                for i in 0..clear_code {
                    dictionary.insert(i, Vec::from([i as u8]));
                }
                prev_code = None;
                continue;
            }

            if code == end_code {
                break;
            }

            if let Some(prev) = prev_code {
                if code < next_code {
                    let mut output = dictionary[&code].clone();
                    result.extend(&output);

                    if next_code < 4096 {
                        let mut new_sequence = dictionary[&prev].clone();
                        new_sequence.push(output[0]);
                        dictionary.insert(next_code, new_sequence);
                        next_code += 1;
                    }
                } else if code == next_code {
                    let mut output = dictionary[&prev].clone();
                    output.push(output[0]);
                    result.extend(&output);

                    if next_code < 4096 {
                        dictionary.insert(next_code, output);
                        next_code += 1;
                    }
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid LZW code",
                    ));
                }
            } else {
                if let Some(sequence) = dictionary.get(&code) {
                    result.extend(sequence);
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

        let mut source_pos = 0;

        // Pass 1 - every 8th row, starting at row 0
        for y in (0..height).step_by(8) {
            let row_start = (y * width * 4) as usize;
            let row_end = row_start + (width * 4) as usize;
            result[row_start..row_end].copy_from_slice(&data[source_pos..source_pos + (width * 4) as usize]);
            source_pos += (width * 4) as usize;
        }

        // Pass 2 - every 8th row, starting at row 4
        for y in (4..height).step_by(8) {
            let row_start = (y * width * 4) as usize;
            let row_end = row_start + (width * 4) as usize;
            result[row_start..row_end].copy_from_slice(&data[source_pos..source_pos + (width * 4) as usize]);
            source_pos += (width * 4) as usize;
        }

        // Pass 3 - every 4th row, starting at row 2
        for y in (2..height).step_by(4) {
            let row_start = (y * width * 4) as usize;
            let row_end = row_start + (width * 4) as usize;
            result[row_start..row_end].copy_from_slice(&data[source_pos..source_pos + (width * 4) as usize]);
            source_pos += (width * 4) as usize;
        }

        // Pass 4 - every 2nd row, starting at row 1
        for y in (1..height).step_by(2) {
            let row_start = (y * width * 4) as usize;
            let row_end = row_start + (width * 4) as usize;
            result[row_start..row_end].copy_from_slice(&data[source_pos..source_pos + (width * 4) as usize]);
            source_pos += (width * 4) as usize;
        }

        result
    }


    fn decode_frame(&self, frame: &FrameInfo) -> Result<Vec<u8>, std::io::Error> {
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

            if color_index + 2 < color_table.len() {
                image_data.push(color_table[color_index]);
                image_data.push(color_table[color_index + 1]);
                image_data.push(color_table[color_index + 2]);
                image_data.push(255);
            }
        }

        Ok(image_data)
    }

    fn compose_frame(&self, frame_index: usize, previous_canvas: Option<&Vec<u8>>) -> Result<Vec<u8>, std::io::Error> {
        let frame = &self.frames[frame_index];
        let mut frame_pixels = self.decode_frame(frame)?;

        if frame.interlace_flag {
            frame_pixels = Self::deinterlace(frame.width, frame.height, &frame_pixels);
        }

        // Create a new canvas with the full GIF dimensions
        let canvas_size = (self.width * self.height * 4) as usize;

        let mut canvas = match (frame.disposal_method, previous_canvas) {
            (DisposalMethod::Previous, Some(prev)) => prev.clone(),
            (DisposalMethod::None, Some(prev)) => prev.clone(),
            (DisposalMethod::Background, _) => {
                let mut canvas = vec![0; canvas_size];

                if !self.global_color_table.is_empty() {
                    let bg_index = self.background_color_index as usize * 3;
                    let bg_color = [
                        self.global_color_table[bg_index],
                        self.global_color_table[bg_index + 1],
                        self.global_color_table[bg_index + 2],
                        255
                    ];
                    
                    for pixel in canvas.chunks_mut(4) {
                        pixel.copy_from_slice(&bg_color);
                    }
                }
                
                canvas
            }
            _ => vec![0; canvas_size],
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
                if frame_pixels[frame_pixel_index + 3] > 0 {
                    canvas[canvas_pixel_index..canvas_pixel_index + 4]
                        .copy_from_slice(&frame_pixels[frame_pixel_index..frame_pixel_index + 4]);
                }
            }
        }

        Ok(canvas)
    }

    pub fn decode(&mut self) -> Result<Image, std::io::Error> {
        self.read_header()?;
        self.read_global_color_table()?;
        self.read_frames()?;

        let mut decoded_frames = Vec::new();
        let mut previous_canvas: Option<Vec<u8>> = None;

        for frame_index in 0..self.frames.len() {
            let frame = &self.frames[frame_index];
            let canvas = self.compose_frame(frame_index, previous_canvas.as_ref())?;

            previous_canvas = match frame.disposal_method {
                DisposalMethod::None | DisposalMethod::Previous => Some(canvas.clone()),
                DisposalMethod::Background => None,
            };

            decoded_frames.push(ImageFrame {
                width: self.width,
                height: self.height,
                pixels: PixelData::RGBA8(canvas),
                delay: frame.delay as u32,
            });
        }

        Ok(Image::new(self.width, self.height, PixelFormat::RGBA8, decoded_frames))
    }
}
