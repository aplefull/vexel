use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::BmpInfo;
use crate::utils::traits::SafeAccess;
use crate::{log_error, log_warn, Image, PixelData};
use serde::Serialize;
use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum BitmapCompression {
    BiRgb = 0,
    BiRle8 = 1,
    BiRle4 = 2,
    BiBitfields = 3,
    BiJpeg = 4,
    BiPng = 5,
    BiAlphaBitfields = 6,
    BiCmyk = 11,
    BiCMYKRle8 = 12,
    BiCMYKRle4 = 13,
}

impl BitmapCompression {
    fn from_u32(value: u32) -> Self {
        match value {
            0 => BitmapCompression::BiRgb,
            1 => BitmapCompression::BiRle8,
            2 => BitmapCompression::BiRle4,
            3 => BitmapCompression::BiBitfields,
            4 => BitmapCompression::BiJpeg,
            5 => BitmapCompression::BiPng,
            6 => BitmapCompression::BiAlphaBitfields,
            11 => BitmapCompression::BiCmyk,
            12 => BitmapCompression::BiCMYKRle8,
            13 => BitmapCompression::BiCMYKRle4,
            _ => {
                log_warn!("Invalid compression type: {}", value);
                BitmapCompression::BiRgb
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapFileHeader {
    pub file_size: u32,
    pub reserved1: u16,
    pub reserved2: u16,
    pub pixel_offset: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum DibHeader {
    Core(BitmapCoreHeader),
    OS2V2(OS22XBitmapHeader),
    Info(BitmapInfoHeader),
    V2(BitmapV2InfoHeader),
    V3(BitmapV3InfoHeader),
    V4(BitmapV4Header),
    V5(BitmapV5Header),
}

impl DibHeader {
    fn bits_per_pixel(&self) -> u16 {
        match self {
            DibHeader::Core(h) => h.bits_per_pixel,
            DibHeader::OS2V2(h) => h.bits_per_pixel,
            DibHeader::Info(h) => h.bits_per_pixel,
            DibHeader::V2(h) => h.info.bits_per_pixel,
            DibHeader::V3(h) => h.v2.info.bits_per_pixel,
            DibHeader::V4(h) => h.v3.v2.info.bits_per_pixel,
            DibHeader::V5(h) => h.v4.v3.v2.info.bits_per_pixel,
        }
    }

    fn colors_used(&self) -> u32 {
        match self {
            DibHeader::Core(_) => 0,
            DibHeader::OS2V2(h) => h.colors_used,
            DibHeader::Info(h) => h.colors_used,
            DibHeader::V2(h) => h.info.colors_used,
            DibHeader::V3(h) => h.v2.info.colors_used,
            DibHeader::V4(h) => h.v3.v2.info.colors_used,
            DibHeader::V5(h) => h.v4.v3.v2.info.colors_used,
        }
    }

    fn height(&self) -> i32 {
        match self {
            DibHeader::Core(h) => h.height as i32,
            DibHeader::OS2V2(h) => h.height,
            DibHeader::Info(h) => h.height,
            DibHeader::V2(h) => h.info.height,
            DibHeader::V3(h) => h.v2.info.height,
            DibHeader::V4(h) => h.v3.v2.info.height,
            DibHeader::V5(h) => h.v4.v3.v2.info.height,
        }
    }

    fn compression(&self) -> BitmapCompression {
        match self {
            DibHeader::Core(_) => BitmapCompression::BiRgb,
            DibHeader::OS2V2(h) => h.compression,
            DibHeader::Info(h) => h.compression,
            DibHeader::V2(h) => h.info.compression,
            DibHeader::V3(h) => h.v2.info.compression,
            DibHeader::V4(h) => h.v3.v2.info.compression,
            DibHeader::V5(h) => h.v4.v3.v2.info.compression,
        }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapCoreHeader {
    pub width: u16,
    pub height: u16,
    pub planes: u16,
    pub bits_per_pixel: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct OS22XBitmapHeader {
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits_per_pixel: u16,
    pub compression: BitmapCompression,
    pub image_size: u32,
    pub x_pixels_per_meter: i32,
    pub y_pixels_per_meter: i32,
    pub colors_used: u32,
    pub important_colors: u32,
    pub units: u16,
    pub reserved: u16,
    pub recording: u16,
    pub rendering: u16,
    pub size1: u32,
    pub size2: u32,
    pub color_encoding: u32,
    pub identifier: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapInfoHeader {
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits_per_pixel: u16,
    pub compression: BitmapCompression,
    pub image_size: u32,
    pub x_pixels_per_meter: i32,
    pub y_pixels_per_meter: i32,
    pub colors_used: u32,
    pub important_colors: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV2InfoHeader {
    pub info: BitmapInfoHeader,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV3InfoHeader {
    pub v2: BitmapV2InfoHeader,
    pub alpha_mask: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV4Header {
    pub v3: BitmapV3InfoHeader,
    pub cs_type: u32,
    pub endpoints: ColorSpace,
    pub gamma_red: u32,
    pub gamma_green: u32,
    pub gamma_blue: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV5Header {
    pub v4: BitmapV4Header,
    pub intent: u32,
    pub profile_data: u32,
    pub profile_size: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorSpace {
    pub ciexyz_red: CIEXYZ,
    pub ciexyz_green: CIEXYZ,
    pub ciexyz_blue: CIEXYZ,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CIEXYZ {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorEntry {
    pub blue: u8,
    pub green: u8,
    pub red: u8,
    pub reserved: u8,
}

pub struct BmpDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    file_header: BitmapFileHeader,
    dib_header: DibHeader,
    color_table: Vec<ColorEntry>,
    data: Vec<u8>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> BmpDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            file_header: BitmapFileHeader {
                file_size: 0,
                reserved1: 0,
                reserved2: 0,
                pixel_offset: 0,
            },
            dib_header: DibHeader::Info(BitmapInfoHeader {
                width: 0,
                height: 0,
                planes: 0,
                bits_per_pixel: 0,
                compression: BitmapCompression::BiRgb,
                image_size: 0,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                important_colors: 0,
            }),
            color_table: Vec::new(),
            data: Vec::new(),
            reader: BitReader::with_le(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> BmpInfo {
        BmpInfo {
            width: self.width,
            height: self.height,
            file_header: self.file_header.clone(),
            dib_header: self.dib_header.clone(),
            color_table: self.color_table.clone(),
        }
    }

    fn read_file_header(&mut self) -> VexelResult<()> {
        let signature = self.reader.read_u16()?;

        match signature {
            0x4D42 => (), // "BM" - Windows bitmap
            0x4142 => (), // "BA" - OS/2 bitmap array
            0x4943 => (), // "CI" - OS/2 color icon
            0x5043 => (), // "CP" - OS/2 color pointer
            0x4349 => (), // "IC" - OS/2 icon
            0x5450 => (), // "PT" - OS/2 pointer
            _ => {
                log_warn!("Invalid BMP signature: 0x{:X}", signature);
            }
        }

        let file_header = BitmapFileHeader {
            file_size: self.reader.read_u32()?,
            reserved1: self.reader.read_u16()?,
            reserved2: self.reader.read_u16()?,
            pixel_offset: self.reader.read_u32()?,
        };

        self.file_header = file_header;

        Ok(())
    }

    fn read_info_header(&mut self) -> VexelResult<()> {
        let header_size = self.reader.read_u32()?;

        self.dib_header = match header_size {
            12 => DibHeader::Core(self.read_bitmap_core_header()?),
            64 => DibHeader::OS2V2(self.read_os2_v2_header()?),
            40 => DibHeader::Info(self.read_bitmap_info_header()?),
            52 => DibHeader::V2(self.read_v2_header()?),
            56 => DibHeader::V3(self.read_v3_header()?),
            108 => DibHeader::V4(self.read_v4_header()?),
            124 => DibHeader::V5(self.read_v5_header()?),
            _ => {
                log_warn!(
                    "Invalid DIB header size: {}, assuming 40 bytes. This may cause issues.",
                    header_size
                );
                DibHeader::Info(self.read_bitmap_info_header()?)
            }
        };

        match &self.dib_header {
            DibHeader::Core(h) => {
                self.width = h.width as u32;
                self.height = h.height as u32;
            }
            DibHeader::OS2V2(h) => {
                self.width = h.width.abs() as u32;
                self.height = h.height.abs() as u32;
            }
            DibHeader::Info(h) => {
                self.width = h.width.abs() as u32;
                self.height = h.height.abs() as u32;
            }
            DibHeader::V2(h) => {
                self.width = h.info.width.abs() as u32;
                self.height = h.info.height.abs() as u32;
            }
            DibHeader::V3(h) => {
                self.width = h.v2.info.width.abs() as u32;
                self.height = h.v2.info.height.abs() as u32;
            }
            DibHeader::V4(h) => {
                self.width = h.v3.v2.info.width.abs() as u32;
                self.height = h.v3.v2.info.height.abs() as u32;
            }
            DibHeader::V5(h) => {
                self.width = h.v4.v3.v2.info.width.abs() as u32;
                self.height = h.v4.v3.v2.info.height.abs() as u32;
            }
        }

        if self.width <= 0 || self.height <= 0 {
            return Err(VexelError::InvalidDimensions {
                width: self.width,
                height: self.height,
            });
        }

        Ok(())
    }

    fn read_bitmap_core_header(&mut self) -> VexelResult<BitmapCoreHeader> {
        Ok(BitmapCoreHeader {
            width: self.reader.read_u16()?,
            height: self.reader.read_u16()?,
            planes: self.reader.read_u16()?,
            bits_per_pixel: self.reader.read_u16()?,
        })
    }

    fn read_os2_v2_header(&mut self) -> VexelResult<OS22XBitmapHeader> {
        Ok(OS22XBitmapHeader {
            width: self.reader.read_u32()? as i32,
            height: self.reader.read_u32()? as i32,
            planes: self.reader.read_u16()?,
            bits_per_pixel: self.reader.read_u16()?,
            compression: BitmapCompression::from_u32(self.reader.read_u32()?),
            image_size: self.reader.read_u32()?,
            x_pixels_per_meter: self.reader.read_u32()? as i32,
            y_pixels_per_meter: self.reader.read_u32()? as i32,
            colors_used: self.reader.read_u32()?,
            important_colors: self.reader.read_u32()?,
            units: self.reader.read_u16()?,
            reserved: self.reader.read_u16()?,
            recording: self.reader.read_u16()?,
            rendering: self.reader.read_u16()?,
            size1: self.reader.read_u32()?,
            size2: self.reader.read_u32()?,
            color_encoding: self.reader.read_u32()?,
            identifier: self.reader.read_u32()?,
        })
    }

    fn read_bitmap_info_header(&mut self) -> VexelResult<BitmapInfoHeader> {
        Ok(BitmapInfoHeader {
            width: self.reader.read_u32()? as i32,
            height: self.reader.read_u32()? as i32,
            planes: self.reader.read_u16()?,
            bits_per_pixel: self.reader.read_u16()?,
            compression: BitmapCompression::from_u32(self.reader.read_u32()?),
            image_size: self.reader.read_u32()?,
            x_pixels_per_meter: self.reader.read_u32()? as i32,
            y_pixels_per_meter: self.reader.read_u32()? as i32,
            colors_used: self.reader.read_u32()?,
            important_colors: self.reader.read_u32()?,
        })
    }

    fn read_v2_header(&mut self) -> VexelResult<BitmapV2InfoHeader> {
        Ok(BitmapV2InfoHeader {
            info: self.read_bitmap_info_header()?,
            red_mask: self.reader.read_u32()?,
            green_mask: self.reader.read_u32()?,
            blue_mask: self.reader.read_u32()?,
        })
    }

    fn read_v3_header(&mut self) -> VexelResult<BitmapV3InfoHeader> {
        Ok(BitmapV3InfoHeader {
            v2: self.read_v2_header()?,
            alpha_mask: self.reader.read_u32()?,
        })
    }

    fn read_v4_header(&mut self) -> VexelResult<BitmapV4Header> {
        Ok(BitmapV4Header {
            v3: self.read_v3_header()?,
            cs_type: self.reader.read_u32()?,
            endpoints: self.read_color_space()?,
            gamma_red: self.reader.read_u32()?,
            gamma_green: self.reader.read_u32()?,
            gamma_blue: self.reader.read_u32()?,
        })
    }

    fn read_v5_header(&mut self) -> VexelResult<BitmapV5Header> {
        Ok(BitmapV5Header {
            v4: self.read_v4_header()?,
            intent: self.reader.read_u32()?,
            profile_data: self.reader.read_u32()?,
            profile_size: self.reader.read_u32()?,
            reserved: self.reader.read_u32()?,
        })
    }

    fn read_color_space(&mut self) -> VexelResult<ColorSpace> {
        Ok(ColorSpace {
            ciexyz_red: self.read_ciexyz()?,
            ciexyz_green: self.read_ciexyz()?,
            ciexyz_blue: self.read_ciexyz()?,
        })
    }

    fn read_ciexyz(&mut self) -> VexelResult<CIEXYZ> {
        Ok(CIEXYZ {
            x: self.reader.read_u32()? as i32,
            y: self.reader.read_u32()? as i32,
            z: self.reader.read_u32()? as i32,
        })
    }

    fn read_color_table(&mut self) -> VexelResult<()> {
        if self.dib_header.bits_per_pixel() <= 8 {
            let num_colors = if self.dib_header.colors_used() > 0 {
                self.dib_header.colors_used()
            } else {
                1u32 << self.dib_header.bits_per_pixel()
            };

            for _ in 0..num_colors {
                let blue = self.reader.read_u8()?;
                let green = self.reader.read_u8()?;
                let red = self.reader.read_u8()?;
                let reserved = self.reader.read_u8()?;

                self.color_table.push(ColorEntry {
                    blue,
                    green,
                    red,
                    reserved,
                });
            }
        }

        Ok(())
    }

    fn read_pixel_data(&mut self) -> VexelResult<()> {
        self.reader
            .seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;

        let row_size = ((self.dib_header.bits_per_pixel() as u32 * self.width + 31) / 32) * 4;
        let data_size = row_size * self.height;

        let bytes_until_eof = self.reader.bytes_left()?;
        let bytes_to_read = std::cmp::min(data_size as u64, bytes_until_eof);
        let data = self.reader.read_bytes(bytes_to_read as usize)?;

        self.data = data;

        Ok(())
    }

    fn flip_v(data: &mut Vec<u8>, width: u32, height: u32, channels: u32) {
        let row_size = (width * channels) as usize;
        let mut temp_row = vec![0u8; row_size];

        for y in 0..(height as usize / 2) {
            let top_row_start = y * row_size;
            let bottom_row_start = (height as usize - 1 - y) * row_size;

            if data.check_range(top_row_start..top_row_start + row_size).is_err() {
                log_warn!("Invalid top row range: {}..{}", top_row_start, top_row_start + row_size);
                continue;
            }

            if data.check_range(bottom_row_start..bottom_row_start + row_size).is_err() {
                log_warn!(
                    "Invalid bottom row range: {}..{}",
                    bottom_row_start,
                    bottom_row_start + row_size
                );
                continue;
            }

            temp_row.copy_from_slice(&data[top_row_start..top_row_start + row_size]);

            data.copy_within(bottom_row_start..bottom_row_start + row_size, top_row_start);

            data[bottom_row_start..bottom_row_start + row_size].copy_from_slice(&temp_row);
        }
    }

    fn decode_rle8(&mut self) -> VexelResult<()> {
        let mut decoded = vec![0u8; (self.width * self.height) as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));
        let mut x = 0;
        let mut y = 0;

        while y < self.height {
            let count = reader.read_u8()?;
            let value = reader.read_u8()?;

            if count == 0 {
                // Escape sequence
                match value {
                    0 => {
                        // End of line
                        x = 0;
                        y += 1;
                    }
                    1 => {
                        // End of bitmap
                        break;
                    }
                    2 => {
                        // Delta - move current position
                        let dx = reader.read_u8()?;
                        let dy = reader.read_u8()?;
                        x += dx as u32;
                        y += dy as u32;
                    }
                    n => {
                        // Absolute mode - next n bytes are literal values
                        for _ in 0..n {
                            if x < self.width {
                                let pos = (y * self.width + x) as usize;
                                if pos < decoded.len() {
                                    if pos >= decoded.len() {
                                        log_warn!("Invalid pixel position: {}", pos);
                                        break;
                                    }

                                    decoded[pos] = reader.read_u8()?;
                                }
                                x += 1;
                            }
                        }
                        // Pad to word boundary
                        if (n % 2) == 1 {
                            reader.read_u8()?; // Skip padding byte
                        }
                    }
                }
            } else {
                // Encoded mode - repeat value count times
                for _ in 0..count {
                    if x < self.width {
                        let pos = (y * self.width + x) as usize;
                        if pos < decoded.len() {
                            if pos >= decoded.len() {
                                log_warn!("Invalid pixel position: {}", pos);
                                break;
                            }

                            decoded[pos] = value;
                        }
                        x += 1;
                    }
                }
            }

            // Handle line wrapping
            if x >= self.width {
                x = 0;
                y += 1;
            }
        }

        self.data = decoded;

        Ok(())
    }

    fn decode_rle4(&mut self) -> VexelResult<()> {
        let mut decoded = vec![0u8; (self.width * self.height) as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));
        let mut x = 0;
        let mut y = 0;

        while y < self.height {
            let count = reader.read_u8()?;
            let value = reader.read_u8()?;

            if count == 0 {
                // Escape sequence
                match value {
                    0 => {
                        // End of line
                        x = 0;
                        y += 1;
                    }
                    1 => {
                        // End of bitmap
                        break;
                    }
                    2 => {
                        // Delta - move current position
                        let dx = reader.read_u8()?;
                        let dy = reader.read_u8()?;
                        x += dx as u32;
                        y += dy as u32;
                    }
                    n => {
                        // Absolute mode - next n pixels are literal values
                        let mut pixels_remaining = n;
                        while pixels_remaining > 0 {
                            let byte = reader.read_u8()?;
                            // Each byte contains two 4-bit pixels
                            for i in 0..2 {
                                if pixels_remaining > 0 && x < self.width {
                                    let pixel = if i == 0 { (byte >> 4) & 0x0F } else { byte & 0x0F };

                                    let pos = (y * self.width + x) as usize;
                                    if pos < decoded.len() {
                                        if pos >= decoded.len() {
                                            log_warn!("Invalid pixel position: {}", pos);
                                            break;
                                        }

                                        decoded[pos] = pixel;
                                    }
                                    x += 1;
                                    pixels_remaining -= 1;
                                }
                            }
                        }
                        // Pad to word boundary
                        if ((n + 1) / 2) % 2 == 1 {
                            reader.read_u8()?; // Skip padding byte
                        }
                    }
                }
            } else {
                // Encoded mode - repeat value count times, alternating high/low nibbles
                let high = (value >> 4) & 0x0F;
                let low = value & 0x0F;
                for i in 0..count {
                    if x < self.width {
                        let pos = (y * self.width + x) as usize;
                        if pos < decoded.len() {
                            if pos >= decoded.len() {
                                log_warn!("Invalid pixel position: {}", pos);
                                break;
                            }

                            decoded[pos] = if i % 2 == 0 { high } else { low };
                        }
                        x += 1;
                    }
                }
            }

            // Handle line wrapping
            if x >= self.width {
                x = 0;
                y += 1;
            }
        }

        self.data = decoded;

        Ok(())
    }

    fn decode_jpeg(&self) -> VexelResult<Image> {
        // TODO: Implement JPEG decompression
        unimplemented!("JPEG compression not yet implemented");
    }

    fn decode_png(&self) -> VexelResult<Image> {
        // TODO: Implement PNG decompression
        unimplemented!("PNG compression not yet implemented");
    }

    fn decode_1bit_image(&self) -> Image {
        if self.color_table.len() < 2 {
            log_warn!("Invalid color table for 1-bit image");
        }

        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width + 7) / 8) as usize;
        let row_padding = (4 - (bytes_per_row % 4)) % 4;
        let total_row_size = bytes_per_row + row_padding;

        // Process each row
        for y in 0..self.height as usize {
            let row_start = y * total_row_size;

            // Process each pixel in the row
            for x in 0..self.width as usize {
                let byte_index = row_start + (x / 8);
                let bit_offset = 7 - (x % 8);

                if byte_index < self.data.len() {
                    let byte = self.data.get_safe(byte_index).unwrap_or_else(|e| {
                        log_warn!("Error reading pixel data: {}", e);
                        &0
                    });

                    let pixel_value = (byte >> bit_offset) & 1;
                    let color = self.color_table.get_safe(pixel_value as usize).unwrap_or_else(|e| {
                        log_warn!("Error reading color table: {}", e);

                        &ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        }
                    });

                    image_data.push(color.red);
                    image_data.push(color.green);
                    image_data.push(color.blue);
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 3);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_4bit_image(&self) -> Image {
        if self.color_table.len() < 16 {
            log_warn!("Invalid color table for 4-bit image");
        }

        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width + 1) / 2) as usize;
        let row_padding = (4 - (bytes_per_row % 4)) % 4;
        let total_row_size = bytes_per_row + row_padding;

        // Process each row
        for y in 0..self.height as usize {
            let row_start = y * total_row_size;

            // Process each pixel in the row
            for x in 0..self.width as usize {
                let byte_index = row_start + (x / 2);
                let is_high_nibble = x % 2 == 0;

                if byte_index < self.data.len() {
                    let byte = *self.data.get_safe(byte_index).unwrap_or_else(|e| {
                        log_warn!("Error reading pixel data: {}", e);
                        &0
                    });

                    let pixel_value = if is_high_nibble {
                        (byte >> 4) & 0x0F
                    } else {
                        byte & 0x0F
                    };

                    let color = self.color_table.get_safe(pixel_value as usize).unwrap_or_else(|e| {
                        log_warn!("Error reading color table: {}", e);

                        &ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        }
                    });

                    image_data.push(color.red);
                    image_data.push(color.green);
                    image_data.push(color.blue);
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 3);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_8bit_image(&self) -> Image {
        if self.color_table.len() < 256 {
            log_warn!("Invalid color table for 8-bit image");
        }

        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = self.width as usize;
        let row_padding = (4 - (bytes_per_row % 4)) % 4;
        let total_row_size = bytes_per_row + row_padding;

        // Process each row
        for y in 0..self.height as usize {
            let row_start = y * total_row_size;

            // Process each pixel in the row
            for x in 0..self.width as usize {
                let byte_index = row_start + x;

                if byte_index < self.data.len() {
                    let pixel_value = *self.data.get_safe(byte_index).unwrap_or_else(|e| {
                        log_warn!("Error reading pixel data: {}", e);
                        &0
                    });

                    let color = self.color_table.get_safe(pixel_value as usize).unwrap_or_else(|e| {
                        log_warn!("Error reading color table: {}", e);

                        &ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        }
                    });

                    image_data.push(color.red);
                    image_data.push(color.green);
                    image_data.push(color.blue);
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 3);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_16bit_image(&self) -> Image {
        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width * 16 + 31) / 32) * 4;
        let row_padding = bytes_per_row - (self.width * 2);

        // Create a BitReader for the pixel data
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        // Process each row
        for _ in 0..self.height {
            // Process each pixel in the row
            for _ in 0..self.width {
                let pixel = reader.read_u16().unwrap_or_else(|e| {
                    log_error!("Error reading pixel data: {}", e);
                    0
                });

                // Extract color components (5-5-5 format)
                // Red: bits 10-14 (5 bits)
                // Green: bits 5-9 (5 bits)
                // Blue: bits 0-4 (5 bits)
                let r = ((pixel >> 10) & 0x1F) as u8;
                let g = ((pixel >> 5) & 0x1F) as u8;
                let b = (pixel & 0x1F) as u8;

                // Scale from 5 bits (0-31) to 8 bits (0-255)
                image_data.push((r << 3) | (r >> 2)); // Replicate top bits to get better color distribution
                image_data.push((g << 3) | (g >> 2));
                image_data.push((b << 3) | (b >> 2));
            }

            // Skip row padding
            if row_padding > 0 {
                match reader.seek(SeekFrom::Current(row_padding as i64)) {
                    Err(e) => {
                        log_error!("Error skipping row padding: {}", e);
                    }
                    Ok(_) => {
                        continue;
                    }
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 3);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_24bit_image(&self) -> Image {
        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width * 24 + 31) / 32) * 4;
        let row_padding = bytes_per_row - (self.width * 3);

        // Create a BitReader for the pixel data
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        // Process each row
        for _ in 0..self.height {
            // Process each pixel in the row
            for _ in 0..self.width {
                // Read BGR values (BMP stores in BGR order)
                let b = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                let g = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                let r = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                // Store in RGB order
                image_data.push(r);
                image_data.push(g);
                image_data.push(b);
            }

            // Skip row padding
            if row_padding > 0 {
                match reader.seek(SeekFrom::Current(row_padding as i64)) {
                    Err(e) => {
                        log_error!("Error skipping row padding: {}", e);
                    }
                    Ok(_) => {
                        continue;
                    }
                };
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 3);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_32bit_image(&self) -> Image {
        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width * 32 + 31) / 32) * 4;
        let row_padding = bytes_per_row - (self.width * 4);

        // Create a BitReader for the pixel data
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        // Process each row
        for _ in 0..self.height {
            // Process each pixel in the row
            for _ in 0..self.width {
                // Read BGRA values (BMP stores in BGRA order)
                let b = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                let g = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                let r = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                let a = reader.read_u8().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                });

                // Store in RGBA order
                image_data.push(r);
                image_data.push(g);
                image_data.push(b);
                image_data.push(a);
            }

            // Skip row padding
            if row_padding > 0 {
                match reader.seek(SeekFrom::Current(row_padding as i64)) {
                    Err(e) => {
                        log_error!("Error skipping row padding: {}", e);
                    }
                    Ok(_) => {
                        continue;
                    }
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 4);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGBA8(image_data))
    }

    fn decode_64bit_image(&self) -> Image {
        let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
        let bytes_per_row = ((self.width * 64 + 31) / 32) * 4;
        let row_padding = bytes_per_row - (self.width * 8);

        // Create a BitReader for the pixel data
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        // Process each row
        for _ in 0..self.height {
            // Process each pixel in the row
            for _ in 0..self.width {
                // Read BGRA values (each channel is 16 bits)
                let b = reader.read_u16().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                }) >> 8;

                let g = reader.read_u16().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                }) >> 8;

                let r = reader.read_u16().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                }) >> 8;

                let a = reader.read_u16().unwrap_or_else(|e| {
                    log_warn!("Error reading pixel data: {}", e);
                    0
                }) >> 8;

                // Store in RGBA order
                image_data.push(r as u8);
                image_data.push(g as u8);
                image_data.push(b as u8);
                image_data.push(a as u8);
            }

            // Skip row padding
            if row_padding > 0 {
                match reader.seek(SeekFrom::Current(row_padding as i64)) {
                    Err(e) => {
                        log_error!("Error skipping row padding: {}", e);
                    }
                    Ok(_) => {
                        continue;
                    }
                }
            }
        }

        if self.dib_header.height() > 0 {
            Self::flip_v(&mut image_data, self.width, self.height, 4);
        }

        Image::from_pixels(self.width, self.height, PixelData::RGBA8(image_data))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.read_file_header() {
            Err(e) => {
                log_error!("Error reading file header. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_info_header() {
            Err(e) => {
                log_error!("Error reading info header. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_color_table() {
            Err(e) => {
                log_error!("Error reading color table. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_pixel_data() {
            Err(e) => {
                log_error!("Error reading pixel data. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.dib_header.compression() {
            BitmapCompression::BiRgb => (),
            BitmapCompression::BiRle8 => {
                if self.dib_header.bits_per_pixel() != 8 {
                    log_warn!(
                        "Invalid bit depth for RLE8 compression: {}",
                        self.dib_header.bits_per_pixel()
                    );
                }

                self.decode_rle8()?;
            }
            BitmapCompression::BiRle4 => {
                if self.dib_header.bits_per_pixel() != 4 {
                    log_warn!(
                        "Invalid bit depth for RLE4 compression: {}",
                        self.dib_header.bits_per_pixel()
                    );
                }

                self.decode_rle4()?;
            }
            BitmapCompression::BiJpeg => {
                return self.decode_jpeg();
            }
            BitmapCompression::BiPng => {
                return self.decode_png();
            }
            _ => {
                // TODO: Implement other compression types
                log_warn!("Unsupported compression type: {:?}", self.dib_header.compression());
            }
        }

        let image = match self.dib_header.bits_per_pixel() {
            1 => self.decode_1bit_image(),
            4 => self.decode_4bit_image(),
            8 => self.decode_8bit_image(),
            16 => self.decode_16bit_image(),
            24 => self.decode_24bit_image(),
            32 => self.decode_32bit_image(),
            64 => self.decode_64bit_image(),
            _ => {
                log_warn!(
                    "Invalid bit depth: {}. Attempting to decode as 24bit.",
                    self.dib_header.bits_per_pixel()
                );
                self.decode_24bit_image()
            }
        };

        Ok(image)
    }
}
