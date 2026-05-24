use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::BmpInfo;
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

        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = (((width + 7) / 8) + 3) & !3;
        let bottom_up = self.dib_header.height() > 0;
        let color_table = &self.color_table;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_index = row_offset + x / 8;
                        let bit_offset = 7 - (x % 8);
                        let pixel_value = if byte_index < src.len() {
                            ((src[byte_index] >> bit_offset) & 1) as usize
                        } else {
                            0
                        };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 3;

                for x in 0..width {
                    let byte_index = row_offset + x / 8;
                    let bit_offset = 7 - (x % 8);
                    let pixel_value = if byte_index < self.data.len() {
                        ((self.data[byte_index] >> bit_offset) & 1) as usize
                    } else {
                        0
                    };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_4bit_image(&self) -> Image {
        if self.color_table.len() < 16 {
            log_warn!("Invalid color table for 4-bit image");
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = (((width + 1) / 2) + 3) & !3;
        let bottom_up = self.dib_header.height() > 0;
        let color_table = &self.color_table;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_index = row_offset + x / 2;
                        let pixel_value = if byte_index < src.len() {
                            let byte = src[byte_index];
                            if x % 2 == 0 { ((byte >> 4) & 0x0F) as usize } else { (byte & 0x0F) as usize }
                        } else {
                            0
                        };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 3;

                for x in 0..width {
                    let byte_index = row_offset + x / 2;
                    let pixel_value = if byte_index < self.data.len() {
                        let byte = self.data[byte_index];
                        if x % 2 == 0 { ((byte >> 4) & 0x0F) as usize } else { (byte & 0x0F) as usize }
                    } else {
                        0
                    };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_8bit_image(&self) -> Image {
        if self.color_table.len() < 256 {
            log_warn!("Invalid color table for 8-bit image");
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = (width + 3) & !3;
        let bottom_up = self.dib_header.height() > 0;
        let color_table = &self.color_table;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_index = row_offset + x;
                        let pixel_value = if byte_index < src.len() { src[byte_index] as usize } else { 0 };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 3;

                for x in 0..width {
                    let byte_index = row_offset + x;
                    let pixel_value = if byte_index < self.data.len() { self.data[byte_index] as usize } else { 0 };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_16bit_image(&self) -> Image {
        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = (((width * 2) + 3) & !3) as usize;
        let bottom_up = self.dib_header.height() > 0;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_offset = row_offset + x * 2;
                        let pixel = if byte_offset + 1 < src.len() {
                            u16::from_le_bytes([src[byte_offset], src[byte_offset + 1]])
                        } else {
                            0
                        };

                        let r = ((pixel >> 10) & 0x1F) as u8;
                        let g = ((pixel >> 5) & 0x1F) as u8;
                        let b = (pixel & 0x1F) as u8;

                        dst[x * 3] = (r << 3) | (r >> 2);
                        dst[x * 3 + 1] = (g << 3) | (g >> 2);
                        dst[x * 3 + 2] = (b << 3) | (b >> 2);
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 3;

                for x in 0..width {
                    let byte_offset = row_offset + x * 2;
                    let pixel = if byte_offset + 1 < self.data.len() {
                        u16::from_le_bytes([self.data[byte_offset], self.data[byte_offset + 1]])
                    } else {
                        0
                    };

                    let r = ((pixel >> 10) & 0x1F) as u8;
                    let g = ((pixel >> 5) & 0x1F) as u8;
                    let b = (pixel & 0x1F) as u8;

                    image_data[dst_offset + x * 3] = (r << 3) | (r >> 2);
                    image_data[dst_offset + x * 3 + 1] = (g << 3) | (g >> 2);
                    image_data[dst_offset + x * 3 + 2] = (b << 3) | (b >> 2);
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_24bit_image(&self) -> Image {
        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = (((width * 3) + 3) & !3) as usize;
        let bottom_up = self.dib_header.height() > 0;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_offset = row_offset + x * 3;
                        if byte_offset + 2 < src.len() {
                            dst[x * 3] = src[byte_offset + 2];
                            dst[x * 3 + 1] = src[byte_offset + 1];
                            dst[x * 3 + 2] = src[byte_offset];
                        }
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 3;

                for x in 0..width {
                    let byte_offset = row_offset + x * 3;
                    if byte_offset + 2 < self.data.len() {
                        image_data[dst_offset + x * 3] = self.data[byte_offset + 2];
                        image_data[dst_offset + x * 3 + 1] = self.data[byte_offset + 1];
                        image_data[dst_offset + x * 3 + 2] = self.data[byte_offset];
                    }
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGB8(image_data))
    }

    fn decode_32bit_image(&self) -> Image {
        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = width * 4;
        let bottom_up = self.dib_header.height() > 0;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 4];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 4)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_offset = row_offset + x * 4;
                        if byte_offset + 3 < src.len() {
                            dst[x * 4] = src[byte_offset + 2];
                            dst[x * 4 + 1] = src[byte_offset + 1];
                            dst[x * 4 + 2] = src[byte_offset];
                            dst[x * 4 + 3] = src[byte_offset + 3];
                        }
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 4;

                for x in 0..width {
                    let byte_offset = row_offset + x * 4;
                    if byte_offset + 3 < self.data.len() {
                        image_data[dst_offset + x * 4] = self.data[byte_offset + 2];
                        image_data[dst_offset + x * 4 + 1] = self.data[byte_offset + 1];
                        image_data[dst_offset + x * 4 + 2] = self.data[byte_offset];
                        image_data[dst_offset + x * 4 + 3] = self.data[byte_offset + 3];
                    }
                }
            }
        }

        Image::from_pixels(self.width, self.height, PixelData::RGBA8(image_data))
    }

    fn decode_64bit_image(&self) -> Image {
        let width = self.width as usize;
        let height = self.height as usize;
        let src_stride = width * 8;
        let bottom_up = self.dib_header.height() > 0;
        let src = &self.data;

        let mut image_data = vec![0u8; width * height * 4];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width * 4)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width {
                        let byte_offset = row_offset + x * 8;
                        if byte_offset + 7 < src.len() {
                            dst[x * 4] = src[byte_offset + 5];
                            dst[x * 4 + 1] = src[byte_offset + 3];
                            dst[x * 4 + 2] = src[byte_offset + 1];
                            dst[x * 4 + 3] = src[byte_offset + 7];
                        }
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height {
                let src_row = if bottom_up { height - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width * 4;

                for x in 0..width {
                    let byte_offset = row_offset + x * 8;
                    if byte_offset + 7 < self.data.len() {
                        image_data[dst_offset + x * 4] = self.data[byte_offset + 5];
                        image_data[dst_offset + x * 4 + 1] = self.data[byte_offset + 3];
                        image_data[dst_offset + x * 4 + 2] = self.data[byte_offset + 1];
                        image_data[dst_offset + x * 4 + 3] = self.data[byte_offset + 7];
                    }
                }
            }
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
