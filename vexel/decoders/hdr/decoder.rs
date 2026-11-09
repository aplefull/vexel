use std::io::{Read, Seek, SeekFrom};

use super::info::{HdrFormat, HdrHeader, HdrInfo, HdrPixelData, HdrResolution, HdrScanlineEncoding, HdrSection};
use super::pixels::PixelDecoder;
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, ImageSummary, SampleFormat};
use crate::{log_warn, Image, Limits};

pub struct HdrDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    format: HdrFormat,
    reader: BitReader<R>,
    info: Option<HdrInfo>,
}

impl<R: Read + Seek> HdrDecoder<R> {
    pub fn new(reader: R) -> Self {
        HdrDecoder {
            width: 0,
            height: 0,
            limits: Limits::default(),
            format: HdrFormat::RGBE,
            reader: BitReader::new(reader),
            info: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
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

    fn read_header_lines(&mut self, header: &mut HdrHeader) -> VexelResult<()> {
        loop {
            let line = String::from_utf8_lossy(self.read_until_newline()?.as_slice()).to_string();

            if let Some(program) = line.strip_prefix("#?") {
                header.program = Some(program.trim().to_string());
                continue;
            }

            if line.starts_with('#') {
                header.comments.push(line.trim_start_matches('#').trim().to_string());
                continue;
            }

            if line.starts_with("FORMAT") {
                let lowered = line.to_lowercase();
                let rest = line.trim_start_matches("FORMAT");
                let format = rest.split_once('=').map_or(rest, |(_, value)| value).trim().to_string();

                if lowered.contains("32-bit_rle_rgbe") {
                    self.format = HdrFormat::RGBE;
                } else if lowered.contains("32-bit_rle_xyze") {
                    self.format = HdrFormat::XYZE;
                } else {
                    log_warn!("Invalid HDR format: {}", format);
                    self.format = HdrFormat::RGBE;
                }

                header.format = Some(format);
                continue;
            }

            if line.starts_with("GAMMA") {
                header.gamma = Some(Self::parse_f32(Self::get_value(line).as_str()));
                continue;
            }

            if line.starts_with("EXPOSURE") {
                let value = Self::get_value(line);
                match value.trim().parse::<f32>() {
                    Ok(value) => header.exposure = Some(header.exposure.unwrap_or(1.0) * value),
                    Err(_) => log_warn!("Failed to parse exposure: {}", value),
                }
                continue;
            }

            if line.starts_with("PIXASPECT") {
                header.pixel_aspect_ratio = Some(Self::parse_f32(Self::get_value(line).as_str()));
                continue;
            }

            if line.starts_with("COLORCORR") {
                let value = Self::get_value(line);
                let parts: Vec<&str> = value.split_whitespace().collect();

                let r = Self::parse_f32(parts.first().unwrap_or(&"1"));
                let g = Self::parse_f32(parts.get(1).unwrap_or(&"1"));
                let b = Self::parse_f32(parts.get(2).unwrap_or(&"1"));

                let [pr, pg, pb] = header.color_correction.unwrap_or([1.0, 1.0, 1.0]);
                header.color_correction = Some([pr * r, pg * g, pb * b]);
                continue;
            }

            if line.starts_with("PRIMARIES") {
                let value = Self::get_value(line);
                let mut p: Vec<f32> = value.split_whitespace().map(Self::parse_f32).collect();

                if p.len() != 8 {
                    log_warn!("Invalid number of primaries: {}", p.len());
                    p.resize(8, 0.0);
                }

                header.primaries = Some([p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]]);
                continue;
            }

            if line.starts_with("SOFTWARE") {
                header.software = Some(Self::get_value(line).trim().to_string());
                continue;
            }

            if line.starts_with("VIEW") {
                header.view = Some(Self::get_value(line).trim().to_string());
                continue;
            }

            if line.trim().is_empty() {
                return Ok(());
            }

            header.other_lines.push(line.trim().to_string());
        }
    }

    fn read_resolution(&mut self) -> VexelResult<HdrResolution> {
        loop {
            let line = String::from_utf8_lossy(self.read_until_newline()?.as_slice()).to_string();

            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() < 4 {
                return Err(VexelError::Custom(format!("Invalid header line: {}", line)));
            }

            let (dim1, dim2) = match parts[0].chars().nth(1) {
                Some('Y') => (parts[3], parts[1]),
                Some('X') => (parts[1], parts[3]),
                _ => {
                    return Err(VexelError::Custom(format!(
                        "Invalid header line: {}, cant parse image dimensions",
                        line
                    )));
                }
            };

            let width = dim1.parse::<u32>().map_err(|_| "Failed to parse width")?;
            let height = dim2.parse::<u32>().map_err(|_| "Failed to parse height")?;

            return Ok(HdrResolution { line: line.trim().to_string(), width, height });
        }
    }

    fn scan(&mut self) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.format = HdrFormat::RGBE;
        let mut info = HdrInfo::default();

        let mut header = HdrHeader::default();
        let header_result = self.read_header_lines(&mut header);
        let header_end = self.reader.stream_position().unwrap_or(0);
        info.push(0, Some(header_end), HdrSection::Header(header));

        let result = header_result.and_then(|_| self.read_resolution());
        let resolution = match result {
            Ok(resolution) => resolution,
            Err(e) => {
                self.info = Some(info);
                return Err(e);
            }
        };

        let resolution_end = self.reader.stream_position().unwrap_or(header_end);
        self.width = resolution.width;
        self.height = resolution.height;
        info.push(header_end, Some(resolution_end - header_end), HdrSection::Resolution(resolution));

        let available = self.reader.bytes_left().unwrap_or(0);
        let first_scanline = self.reader.peek_bytes(4.min(available as usize)).unwrap_or_default();
        let encoding = match first_scanline.as_slice() {
            [] => None,
            [2, 2, hi, lo] if u32::from(u16::from_be_bytes([*hi, *lo])) == self.width => Some(HdrScanlineEncoding::Rle),
            _ => Some(HdrScanlineEncoding::Flat),
        };
        let decoded_length = (u64::from(self.width) * u64::from(self.height)).saturating_mul(4);
        info.push(resolution_end, Some(available), HdrSection::PixelData(HdrPixelData { encoding, decoded_length }));

        let color_model = if self.format == HdrFormat::XYZE { ColorModel::Xyz } else { ColorModel::Rgb };
        let mut summary = ImageSummary::new(self.width, self.height, color_model, 3, 32);
        summary.sample_format = SampleFormat::Float;
        summary.compression = encoding.map(|encoding| match encoding {
            HdrScanlineEncoding::Rle => "Adaptive RLE".to_string(),
            HdrScanlineEncoding::Flat => "None".to_string(),
        });
        info.summary = Some(summary);
        self.info = Some(info);

        Ok(())
    }

    fn read_header(&mut self) -> VexelResult<()> {
        self.scan()?;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        self.limits.reserve_buffer(self.width, self.height, 4)?;

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

        let mut channel_buf = [vec![0u8; width], vec![0u8; width], vec![0u8; width], vec![0u8; width]];

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
                for dst in channel_buf.iter_mut() {
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

    pub fn get_info(&mut self) -> HdrInfo {
        if self.info.is_none() {
            if let Err(e) = self.scan() {
                log_warn!("Failed to read HDR header: {}", e);
            }
        }

        self.info.clone().unwrap_or_default()
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
