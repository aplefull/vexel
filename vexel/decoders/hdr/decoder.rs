use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::HdrInfo;
use crate::{log_warn, Image};
use std::io::{Read, Seek};

use super::pixels::PixelDecoder;
use super::types::HdrFormat;

pub struct HdrDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    gamma: Option<f32>,
    exposure: Option<f32>,
    pixel_aspect_ratio: Option<f32>,
    color_correction: Option<[f32; 3]>,
    primaries: Option<[f32; 8]>,
    format: HdrFormat,
    comments: Vec<String>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> HdrDecoder<R> {
    pub fn new(reader: R) -> Self {
        HdrDecoder {
            width: 0,
            height: 0,
            gamma: None,
            exposure: None,
            pixel_aspect_ratio: None,
            color_correction: None,
            primaries: None,
            format: HdrFormat::RGBE,
            comments: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn read_until_newline(&mut self) -> VexelResult<Vec<u8>> {
        let mut buffer = Vec::new();

        loop {
            let byte = self.reader.read_u8()?;

            if byte == b'\n' {
                break;
            }

            buffer.push(byte);
        }

        Ok(buffer)
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let mut line;

        loop {
            line = String::from_utf8_lossy(self.read_until_newline()?.as_slice()).to_string();
            if line.starts_with("#?RADIANCE") {
                continue;
            }

            if line.starts_with("#") {
                self.comments.push(line);
                continue;
            }

            if line.starts_with("FORMAT") {
                let format = line.to_lowercase();

                if format.contains("32-bit_rle_rgbe") {
                    self.format = HdrFormat::RGBE;
                } else if format.contains("32-bit_rle_xyze") {
                    self.format = HdrFormat::XYZE;
                } else {
                    log_warn!("Invalid HDR format: {}", format);
                    self.format = HdrFormat::RGBE;
                }

                continue;
            }

            if line.starts_with("GAMMA") {
                self.gamma = Some(Self::parse_f32(Self::get_value(line).as_str()));

                continue;
            }

            if line.starts_with("EXPOSURE") {
                self.exposure = Some(Self::parse_f32(Self::get_value(line).as_str()));

                continue;
            }

            if line.starts_with("PIXASPECT") {
                self.pixel_aspect_ratio = Some(Self::parse_f32(Self::get_value(line).as_str()));

                continue;
            }

            if line.starts_with("COLORCORR") {
                let value = Self::get_value(line);
                let parts: Vec<&str> = value.split_whitespace().collect();

                let r = Self::parse_f32(parts.get(1).unwrap_or(&"0"));
                let g = Self::parse_f32(parts.get(2).unwrap_or(&"0"));
                let b = Self::parse_f32(parts.get(3).unwrap_or(&"0"));

                self.color_correction = Some([r, g, b]);

                continue;
            }

            if line.starts_with("PRIMARIES") {
                let value = Self::get_value(line);
                let parts: Vec<&str> = value.split_whitespace().collect();

                let mut primaries = Vec::new();

                for part in parts.iter() {
                    primaries.push(Self::parse_f32(part));
                }

                if primaries.len() != 8 {
                    log_warn!("Invalid number of primaries: {}", primaries.len());

                    if primaries.len() > 8 {
                        primaries.truncate(8);
                    } else {
                        while primaries.len() < 8 {
                            primaries.push(0.0);
                        }
                    }
                }
                self.primaries = Some([
                    primaries[0],
                    primaries[1],
                    primaries[2],
                    primaries[3],
                    primaries[4],
                    primaries[5],
                    primaries[6],
                    primaries[7],
                ]);

                continue;
            }

            if line.trim().is_empty() {
                break;
            }
        }

        loop {
            line = String::from_utf8_lossy(self.read_until_newline()?.as_slice()).to_string();

            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() < 4 {
                return Err(VexelError::Custom(format!("Invalid header line: {}", line)));
            }

            let (dim1, dim2) = match parts[0].chars().nth(1) {
                Some('Y') => {
                    let height_str = parts[1];
                    let width_str = parts[3];

                    (width_str, height_str)
                }
                Some('X') => {
                    let width_str = parts[1];
                    let height_str = parts[3];

                    (width_str, height_str)
                }
                _ => {
                    return Err(VexelError::Custom(format!(
                        "Invalid header line: {}, cant parse image dimensions",
                        line
                    )));
                }
            };

            self.width = dim1.parse::<u32>().map_err(|_| "Failed to parse width")?;
            self.height = dim2.parse::<u32>().map_err(|_| "Failed to parse height")?;

            if self.width == 0 || self.height == 0 {
                return Err(VexelError::InvalidDimensions {
                    width: self.width,
                    height: self.height,
                });
            }

            break;
        }

        Ok(())
    }

    fn get_value(str: String) -> String {
        let parts: Vec<&str> = str.split('=').collect();

        parts.get(1).unwrap_or(&"").to_string()
    }

    fn parse_f32(str: &str) -> f32 {
        match str.parse::<f32>() {
            Ok(value) => value,
            Err(_) => {
                log_warn!("Failed to parse float: {}", str);

                0.0
            }
        }
    }

    fn read_scanlines(&mut self) -> VexelResult<Vec<u8>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let num_pixels = width * height;
        let mut rgbe_data = vec![0u8; num_pixels * 4];

        let compressed = self.reader.read_to_end()?;
        let mut cursor = 0usize;

        let read_byte = |cursor: &mut usize| -> Option<u8> {
            if *cursor < compressed.len() {
                let b = compressed[*cursor];
                *cursor += 1;
                Some(b)
            } else {
                None
            }
        };

        let mut channel_buf = [
            vec![0u8; width],
            vec![0u8; width],
            vec![0u8; width],
            vec![0u8; width],
        ];

        for y in 0..height {
            let b0 = match read_byte(&mut cursor) {
                Some(b) => b,
                None => break,
            };
            let b1 = match read_byte(&mut cursor) {
                Some(b) => b,
                None => break,
            };
            let b2 = match read_byte(&mut cursor) {
                Some(b) => b,
                None => break,
            };
            let b3 = match read_byte(&mut cursor) {
                Some(b) => b,
                None => break,
            };

            let rle_header = (b0 as u16) << 8 | b1 as u16;
            let rle_width = (b2 as u16) << 8 | b3 as u16;

            if rle_header == 0x0202 && rle_width == self.width as u16 {
                for component in 0..4 {
                    let dst = &mut channel_buf[component];
                    let mut pos = 0;

                    while pos < width {
                        let count = match read_byte(&mut cursor) {
                            Some(b) => b as usize,
                            None => break,
                        };

                        if count > 128 {
                            let run_length = count - 128;
                            let value = match read_byte(&mut cursor) {
                                Some(b) => b,
                                None => break,
                            };
                            let end = (pos + run_length).min(width);
                            dst[pos..end].fill(value);
                            pos += run_length;
                        } else {
                            let end = (pos + count).min(width);
                            let src_end = cursor + (end - pos);
                            if src_end <= compressed.len() {
                                dst[pos..end].copy_from_slice(&compressed[cursor..src_end]);
                                cursor = src_end;
                            } else {
                                let available = compressed.len() - cursor;
                                dst[pos..pos + available].copy_from_slice(&compressed[cursor..]);
                                cursor = compressed.len();
                            }
                            pos += count;
                        }
                    }
                }

                let scanline_start = y * width * 4;
                let scanline = &mut rgbe_data[scanline_start..scanline_start + width * 4];

                for x in 0..width {
                    scanline[x * 4] = channel_buf[0][x];
                    scanline[x * 4 + 1] = channel_buf[1][x];
                    scanline[x * 4 + 2] = channel_buf[2][x];
                    scanline[x * 4 + 3] = channel_buf[3][x];
                }
            } else {
                cursor -= 4;

                let scanline_start = y * width * 4;
                let scanline_bytes = width * 4;

                if scanline_start + scanline_bytes > rgbe_data.len() {
                    log_warn!(
                        "Scanline index out of bounds: {} >= {}",
                        scanline_start + scanline_bytes,
                        rgbe_data.len()
                    );
                    break;
                }

                let src_end = cursor + scanline_bytes;
                if src_end > compressed.len() {
                    log_warn!(
                        "Compressed data too short for scanline {}: need {} bytes, have {}",
                        y,
                        scanline_bytes,
                        compressed.len() - cursor
                    );
                    break;
                }

                rgbe_data[scanline_start..scanline_start + scanline_bytes]
                    .copy_from_slice(&compressed[cursor..src_end]);
                cursor = src_end;
            }
        }

        Ok(rgbe_data)
    }

    pub fn get_info(&self) -> HdrInfo {
        HdrInfo {
            width: self.width,
            height: self.height,
            gamma: self.gamma,
            exposure: self.exposure,
            pixel_aspect_ratio: self.pixel_aspect_ratio,
            color_correction: self.color_correction,
            primaries: self.primaries,
            format: self.format,
            comments: self.comments.clone(),
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let rgbe_data = self.read_scanlines()?;
        let pixel_decoder = PixelDecoder::new(self.width, self.height, self.format);
        let mut pixel_data = pixel_decoder.decode(&rgbe_data)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
