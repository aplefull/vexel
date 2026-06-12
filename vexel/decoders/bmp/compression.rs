use crate::decoders::bmp::types::ColorEntry;
use crate::utils::error::VexelResult;
use crate::{log_warn, Image, PixelData};
use crate::bitreader::BitReader;

pub struct RleDecoder;

impl RleDecoder {
    pub fn decode_rle8(data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let mut decoded = vec![0u8; (width * height) as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(data));
        let mut x = 0;
        let mut y = 0;

        while y < height {
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
                            if x < width {
                                let pos = (y * width + x) as usize;
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
                    if x < width {
                        let pos = (y * width + x) as usize;
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
        }

        Ok(decoded)
    }

    pub fn decode_rle4(data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let mut decoded = vec![0u8; (width * height) as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(data));
        let mut x = 0;
        let mut y = 0;

        while y < height {
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
                                if pixels_remaining > 0 && x < width {
                                    let pixel = if i == 0 { (byte >> 4) & 0x0F } else { byte & 0x0F };

                                    let pos = (y * width + x) as usize;
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
                    if x < width {
                        let pos = (y * width + x) as usize;
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
        }

        Ok(decoded)
    }

    pub fn decode_rle4_image(data: &[u8], width: u32, height: u32, color_table: &[ColorEntry]) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        for dst_row in 0..height_usize {
            let src_row = height_usize - 1 - dst_row;
            let src_offset = src_row * width_usize;
            let dst_offset = dst_row * width_usize * 3;

            for x in 0..width_usize {
                let idx = src_offset + x;
                let pixel_value = if idx < data.len() { data[idx] as usize } else { 0 };
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

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_rle8_image(data: &[u8], width: u32, height: u32, color_table: &[ColorEntry]) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        for dst_row in 0..height_usize {
            let src_row = height_usize - 1 - dst_row;
            let src_offset = src_row * width_usize;
            let dst_offset = dst_row * width_usize * 3;

            for x in 0..width_usize {
                let idx = src_offset + x;
                let pixel_value = if idx < data.len() { data[idx] as usize } else { 0 };
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

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }
}
