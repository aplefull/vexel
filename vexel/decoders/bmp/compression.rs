use crate::bitreader::BitReader;
use crate::decoders::bmp::simd;
use crate::decoders::bmp::types::ColorEntry;
use crate::utils::error::VexelResult;
use crate::utils::tiff::fax;
use crate::utils::tiff::types::Compression;
use crate::{log_warn, Image, PixelData};

pub struct RleDecoder;

impl RleDecoder {
    pub fn decode_rle8(data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let mut decoded = vec![0u8; width as usize * height as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(data));
        let mut x = 0;
        let mut y = 0;

        while y < height {
            let count = match reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };
            let value = match reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };

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
                            let byte = match reader.read_u8() {
                                Ok(v) => v,
                                Err(_) => break,
                            };
                            let pos = (y * width + x) as usize;
                            if x < width && pos < decoded.len() {
                                decoded[pos] = byte;
                            }
                            x += 1;
                        }
                        // Pad to word boundary
                        if (n % 2) == 1 {
                            let _ = reader.read_u8();
                        }
                    }
                }
            } else {
                // Encoded mode - repeat value count times
                let start = (y * width + x) as usize;
                let available = (width - x) as usize;
                let fill_count = (count as usize).min(available);
                if start + fill_count <= decoded.len() {
                    simd::fill_bytes(&mut decoded[start..start + fill_count], value);
                    x += fill_count as u32;
                } else if start < decoded.len() {
                    simd::fill_bytes(&mut decoded[start..], value);
                    x = width;
                }
            }
        }

        Ok(decoded)
    }

    pub fn decode_rle4(data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let mut decoded = vec![0u8; width as usize * height as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(data));
        let mut x = 0;
        let mut y = 0;

        while y < height {
            let count = match reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };
            let value = match reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };

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
                            let byte = match reader.read_u8() {
                                Ok(v) => v,
                                Err(_) => break,
                            };
                            for shift in [4u8, 0u8] {
                                if pixels_remaining > 0 {
                                    let pixel = (byte >> shift) & 0x0F;
                                    let pos = (y * width + x) as usize;
                                    if x < width && pos < decoded.len() {
                                        decoded[pos] = pixel;
                                    }
                                    x += 1;
                                    pixels_remaining -= 1;
                                }
                            }
                        }
                        // Pad to word boundary
                        if n.div_ceil(2) % 2 == 1 {
                            let _ = reader.read_u8();
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
                            decoded[pos] = if i % 2 == 0 { high } else { low };
                        }
                        x += 1;
                    }
                }
            }
        }

        Ok(decoded)
    }

    pub fn decode_rle24(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let stride = (width as usize * 3 + 3) & !3;
        let mut decoded = vec![0u8; stride * height as usize];
        let mut reader = BitReader::new(std::io::Cursor::new(data));
        let mut x = 0u32;
        let mut y = 0u32;

        while y < height {
            let count = match reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };

            if count == 0 {
                let escape = match reader.read_u8() {
                    Ok(v) => v,
                    Err(_) => break,
                };

                match escape {
                    0 => {
                        x = 0;
                        y += 1;
                    }
                    1 => {
                        break;
                    }
                    2 => {
                        let dx = match reader.read_u8() {
                            Ok(v) => v,
                            Err(_) => break,
                        };
                        let dy = match reader.read_u8() {
                            Ok(v) => v,
                            Err(_) => break,
                        };
                        x += dx as u32;
                        y += dy as u32;
                    }
                    n => {
                        let mut truncated = false;
                        for _ in 0..n {
                            let pixel = match reader.read_bytes(3) {
                                Ok(p) => p,
                                Err(_) => {
                                    truncated = true;
                                    break;
                                }
                            };
                            if x < width && y < height {
                                let pos = y as usize * stride + x as usize * 3;
                                decoded[pos..pos + 3].copy_from_slice(&pixel);
                            }
                            x += 1;
                        }
                        if truncated {
                            break;
                        }
                        if (n as usize * 3) % 2 == 1 {
                            let _ = reader.read_u8();
                        }
                    }
                }
            } else {
                let pixel = match reader.read_bytes(3) {
                    Ok(p) => p,
                    Err(_) => break,
                };
                for _ in 0..count {
                    if x < width && y < height {
                        let pos = y as usize * stride + x as usize * 3;
                        decoded[pos..pos + 3].copy_from_slice(&pixel);
                    }
                    x += 1;
                }
            }
        }

        decoded
    }

    pub fn decode_huffman1d(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let rows = height as usize;
        let (mut packed, clean_rows) = fax::decompress_ccitt_counted(data, width, rows, Compression::CCITTFax3, 0);

        if clean_rows < rows {
            let reversed: Vec<u8> = data.iter().map(|byte| byte.reverse_bits()).collect();
            let (packed_reversed, clean_rows_reversed) =
                fax::decompress_ccitt_counted(&reversed, width, rows, Compression::CCITTFax3, 0);

            if clean_rows_reversed > clean_rows {
                log_warn!(
                    "Huffman 1D bitstream appears to be packed LSB-first, decoded {}/{} rows after bit reversal (MSB-first gave {})",
                    clean_rows_reversed,
                    rows,
                    clean_rows
                );
                packed = packed_reversed;
            }
        }

        let src_stride = (width as usize).div_ceil(8);
        let dst_stride = (src_stride + 3) & !3;
        let mut padded = vec![0u8; dst_stride * height as usize];

        for row in 0..height as usize {
            let src_offset = row * src_stride;
            let dst_offset = row * dst_stride;
            padded[dst_offset..dst_offset + src_stride].copy_from_slice(&packed[src_offset..src_offset + src_stride]);
        }

        padded
    }

    pub fn decode_rle4_image(
        data: &[u8],
        width: u32,
        height: u32,
        bottom_up: bool,
        color_table: &[ColorEntry],
    ) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        for dst_row in 0..height_usize {
            let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
            let src_offset = src_row * width_usize;
            let dst_offset = dst_row * width_usize * 3;
            let src_end = (src_offset + width_usize).min(data.len());
            let indices = if src_offset < data.len() { &data[src_offset..src_end] } else { &[] };
            simd::apply_palette_row(
                indices,
                color_table,
                &mut image_data[dst_offset..dst_offset + width_usize * 3],
                width_usize,
            );
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_rle8_image(
        data: &[u8],
        width: u32,
        height: u32,
        bottom_up: bool,
        color_table: &[ColorEntry],
    ) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        for dst_row in 0..height_usize {
            let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
            let src_offset = src_row * width_usize;
            let dst_offset = dst_row * width_usize * 3;
            let src_end = (src_offset + width_usize).min(data.len());
            let indices = if src_offset < data.len() { &data[src_offset..src_end] } else { &[] };
            simd::apply_palette_row(
                indices,
                color_table,
                &mut image_data[dst_offset..dst_offset + width_usize * 3],
                width_usize,
            );
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }
}
