use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image, PixelData};
use std::fmt::Debug;
use std::io::{Read, Seek};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TgaImageType {
    #[default]
    NoImageData = 0,
    UncompressedColorMapped = 1,
    UncompressedRGB = 2,
    UncompressedBW = 3,
    RLEColorMapped = 9,
    RLERGB = 10,
    RLEBlackWhite = 11,
    HuffmanColorMapped = 32,
    HuffmanQuadTree = 33,
}

impl TryFrom<u8> for TgaImageType {
    type Error = VexelError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TgaImageType::NoImageData),
            1 => Ok(TgaImageType::UncompressedColorMapped),
            2 => Ok(TgaImageType::UncompressedRGB),
            3 => Ok(TgaImageType::UncompressedBW),
            9 => Ok(TgaImageType::RLEColorMapped),
            10 => Ok(TgaImageType::RLERGB),
            11 => Ok(TgaImageType::RLEBlackWhite),
            32 => Ok(TgaImageType::HuffmanColorMapped),
            33 => Ok(TgaImageType::HuffmanQuadTree),
            _ => Err(VexelError::Custom(format!("Invalid image type: {}", value))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TgaColorMapSpec {
    pub origin: u16,
    pub length: u16,
    pub entry_size: u8,
}

#[derive(Debug, Clone, Default)]
pub struct TgaImageSpec {
    pub x_origin: u16,
    pub y_origin: u16,
    pub width: u16,
    pub height: u16,
    pub pixel_depth: u8,
    pub descriptor: u8,
}

impl TgaImageSpec {
    pub fn is_top_to_bottom(&self) -> bool {
        // Bit 5 of descriptor determines image origin
        // 0 = bottom left, 1 = top left
        (self.descriptor & 0x20) != 0
    }

    pub fn is_right_to_left(&self) -> bool {
        // Bits 4-5 of descriptor determine horizontal orientation
        // 0 = left to right, 1 = right to left
        (self.descriptor & 0x10) != 0
    }

    pub fn alpha_channel_bits(&self) -> u8 {
        // Bits 0-3 specify number of alpha channel bits
        self.descriptor & 0x0F
    }
}

#[derive(Debug, Clone, Default)]
pub struct TgaHeader {
    pub id_length: u8,
    pub color_map_type: u8,
    pub image_type: TgaImageType,
    pub color_map_spec: TgaColorMapSpec,
    pub image_spec: TgaImageSpec,
}

pub struct TgaDecoder<R: Read + Seek> {
    width: u16,
    height: u16,
    header: TgaHeader,
    color_map: Vec<[u8; 4]>,
    image_id: String,
    reader: BitReader<R>,
}

impl<R: Read + Seek> TgaDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            header: TgaHeader::default(),
            color_map: Vec::new(),
            image_id: String::new(),
            reader: BitReader::with_le(reader),
        }
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let id_length = self.reader.read_u8()?;
        let color_map_type = self.reader.read_u8()?;
        let image_type = TgaImageType::try_from(self.reader.read_u8()?)?;

        let color_map_spec = TgaColorMapSpec {
            origin: self.reader.read_u16()?,
            length: self.reader.read_u16()?,
            entry_size: self.reader.read_u8()?,
        };

        let image_spec = TgaImageSpec {
            x_origin: self.reader.read_u16()?,
            y_origin: self.reader.read_u16()?,
            width: self.reader.read_u16()?,
            height: self.reader.read_u16()?,
            pixel_depth: self.reader.read_u8()?,
            descriptor: self.reader.read_u8()?,
        };

        self.width = image_spec.width;
        self.height = image_spec.height;

        self.header = TgaHeader {
            id_length,
            color_map_type,
            image_type,
            color_map_spec,
            image_spec,
        };

        Ok(())
    }

    fn read_image_id(&mut self) -> VexelResult<()> {
        if self.header.id_length > 0 {
            let mut id_data = vec![0u8; self.header.id_length as usize];
            self.reader.read_exact(&mut id_data)?;
            self.image_id = String::from_utf8_lossy(&id_data).to_string();
        }

        Ok(())
    }

    fn read_color_map(&mut self) -> VexelResult<()> {
        let header = &self.header;

        if header.color_map_type == 1 {
            let entry_size = header.color_map_spec.entry_size;
            let map_length = header.color_map_spec.length as usize;
            let mut color_map = Vec::with_capacity(map_length);

            for _ in 0..map_length {
                let entry = match entry_size {
                    15 | 16 => {
                        let pixel = self.reader.read_u16()?;
                        let r = (((pixel >> 10) & 0x1F) as f32 * 255.0 / 31.0) as u8;
                        let g = (((pixel >> 5) & 0x1F) as f32 * 255.0 / 31.0) as u8;
                        let b = ((pixel & 0x1F) as f32 * 255.0 / 31.0) as u8;
                        let a = if pixel & 0x8000 != 0 { 255 } else { 0 };
                        [r, g, b, a]
                    }
                    24 => {
                        let b = self.reader.read_u8()?;
                        let g = self.reader.read_u8()?;
                        let r = self.reader.read_u8()?;
                        [r, g, b, 255]
                    }
                    32 => {
                        let b = self.reader.read_u8()?;
                        let g = self.reader.read_u8()?;
                        let r = self.reader.read_u8()?;
                        let a = self.reader.read_u8()?;
                        [r, g, b, a]
                    }
                    _ => {
                        return Err(VexelError::Custom(format!(
                            "Unsupported color map entry size: {}",
                            entry_size
                        )))
                    }
                };

                color_map.push(entry);
            }

            self.color_map = color_map;
        }

        Ok(())
    }

    fn flip_pixels_vertical(pixels: &mut [u8], width: usize, height: usize, bytes_per_pixel: usize) {
        let stride = width * bytes_per_pixel;
        let rows = height / 2;

        for row in 0..rows {
            let top_start = row * stride;
            let bottom_start = (height - 1 - row) * stride;

            for i in 0..stride {
                pixels.swap(top_start + i, bottom_start + i);
            }
        }
    }

    fn decode_pixel(&mut self, pixel_depth: u8) -> VexelResult<[u8; 4]> {
        match pixel_depth {
            8 => {
                let v = self.reader.read_u8()?;
                Ok([v, v, v, 255])
            }
            15 | 16 => {
                let pixel = self.reader.read_u16()?;
                let r = (((pixel >> 10) & 0x1F) as f32 * 255.0 / 31.0) as u8;
                let g = (((pixel >> 5) & 0x1F) as f32 * 255.0 / 31.0) as u8;
                let b = ((pixel & 0x1F) as f32 * 255.0 / 31.0) as u8;
                let a = if pixel & 0x8000 != 0 { 255 } else { 0 };
                Ok([r, g, b, a])
            }
            24 => {
                let b = self.reader.read_u8()?;
                let g = self.reader.read_u8()?;
                let r = self.reader.read_u8()?;
                Ok([r, g, b, 255])
            }
            32 => {
                let b = self.reader.read_u8()?;
                let g = self.reader.read_u8()?;
                let r = self.reader.read_u8()?;
                let a = self.reader.read_u8()?;
                Ok([r, g, b, a])
            }
            _ => {
                log_warn!("Invalid pixel depth: {}", pixel_depth);
                
                let v = self.reader.read_u8()?;
                Ok([v, v, v, 255])
            }
        }
    }

    fn decode_uncompressed(&mut self) -> VexelResult<PixelData> {
        let header = &self.header;
        let width = self.width as usize;
        let height = self.height as usize;
        let pixel_depth = header.image_spec.pixel_depth;
        let is_top_to_bottom = header.image_spec.is_top_to_bottom();
        let is_right_to_left = header.image_spec.is_right_to_left();

        // Create a 2D vector to help with orientation handling
        let mut pixel_rows = Vec::with_capacity(height);
        for _ in 0..height {
            let mut row = Vec::with_capacity(width * 4);
            for _ in 0..width {
                let pixel = self.decode_pixel(pixel_depth)?;
                row.extend_from_slice(&pixel);
            }
            
            if is_right_to_left {
                // Reverse pixels in the row
                let mut flipped_row = Vec::with_capacity(width * 4);
                for pixel_idx in (0..row.len()).step_by(4).rev() {
                    flipped_row.extend_from_slice(&row[pixel_idx..pixel_idx + 4]);
                }
                row = flipped_row;
            }
            
            pixel_rows.push(row);
        }

        // Flatten the 2D vector into the final pixel buffer
        let mut pixels = Vec::with_capacity(width * height * 4);
        if is_top_to_bottom {
            // Top-to-bottom: use rows in order
            for row in pixel_rows {
                pixels.extend(row);
            }
        } else {
            // Bottom-to-top: reverse row order
            for row in pixel_rows.into_iter().rev() {
                pixels.extend(row);
            }
        }

        Ok(PixelData::RGBA8(pixels))
    }

    fn decode_rle(&mut self) -> VexelResult<PixelData> {
        let width = self.width as usize;
        let height = self.height as usize;
        let pixel_depth = self.header.image_spec.pixel_depth;
        let is_top_to_bottom = self.header.image_spec.is_top_to_bottom();

        let mut pixels = Vec::with_capacity(width * height * 4);
        let mut pixel_count = 0;
        let total_pixels = width * height;

        while pixel_count < total_pixels {
            let packet_header = self.reader.read_u8()?;
            let run_length = (packet_header & 0x7F) as usize + 1;

            if packet_header & 0x80 != 0 {
                // RLE packet
                let pixel = self.decode_pixel(pixel_depth)?;
                for _ in 0..run_length {
                    if pixel_count < total_pixels {
                        pixels.extend_from_slice(&pixel);
                        pixel_count += 1;
                    }
                }
            } else {
                // Raw packet
                for _ in 0..run_length {
                    if pixel_count < total_pixels {
                        let pixel = self.decode_pixel(pixel_depth)?;
                        pixels.extend_from_slice(&pixel);
                        pixel_count += 1;
                    }
                }
            }
        }

        if !is_top_to_bottom {
            Self::flip_pixels_vertical(&mut pixels, width, height, 4);
        }

        Ok(PixelData::RGBA8(pixels))
    }

    fn decode_color_mapped(&mut self, is_rle: bool) -> VexelResult<PixelData> {
        let width = self.width as usize;
        let height = self.height as usize;
        let total_pixels = width * height;

        let mut pixels = Vec::with_capacity(width * height * 4);
        let mut pixel_count = 0;

        if !is_rle {
            // Uncompressed color mapped
            while pixel_count < total_pixels {
                let index = self.reader.read_u8()? as usize;
                if let Some(color) = self.color_map.get(index) {
                    pixels.extend_from_slice(color);
                    pixel_count += 1;
                }
            }
        } else {
            // RLE color mapped
            while pixel_count < total_pixels {
                let packet_header = self.reader.read_u8()?;
                let run_length = (packet_header & 0x7F) as usize + 1;

                if packet_header & 0x80 != 0 {
                    // RLE packet
                    let index = self.reader.read_u8()? as usize;
                    if let Some(color) = self.color_map.get(index) {
                        for _ in 0..run_length {
                            if pixel_count < total_pixels {
                                pixels.extend_from_slice(color);
                                pixel_count += 1;
                            }
                        }
                    }
                } else {
                    // Raw packet
                    for _ in 0..run_length {
                        if pixel_count < total_pixels {
                            let index = self.reader.read_u8()? as usize;
                            if let Some(color) = self.color_map.get(index) {
                                pixels.extend_from_slice(color);
                                pixel_count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(PixelData::RGBA8(pixels))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        // Read header
        self.read_header()?;

        // Read optional image ID
        self.read_image_id()?;

        // Read color map if present
        self.read_color_map()?;

        // Validate image dimensions
        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions {
                width: self.width as u32,
                height: self.height as u32,
            });
        }

        let mut pixel_data = match self.header.image_type {
            TgaImageType::UncompressedRGB | TgaImageType::UncompressedBW => self.decode_uncompressed()?,
            TgaImageType::RLERGB | TgaImageType::RLEBlackWhite => self.decode_rle()?,
            TgaImageType::UncompressedColorMapped => self.decode_color_mapped(false)?,
            TgaImageType::RLEColorMapped => self.decode_color_mapped(true)?,
            TgaImageType::NoImageData => {
                log_warn!("Header specifies no image data in TGA image, will attempt to decode as uncompressed");
                self.decode_uncompressed()?
            }
            TgaImageType::HuffmanColorMapped | TgaImageType::HuffmanQuadTree => {
                log_warn!("Header specifies Huffman coding in TGA image, which doesn't actually exist, treating as uncompressed");
                self.decode_uncompressed()?
            }
        };

        pixel_data.correct_pixels(self.width as u32, self.height as u32);

        Ok(Image::from_pixels(self.width as u32, self.height as u32, pixel_data))
    }
}
