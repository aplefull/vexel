use std::io::{Read, Seek, SeekFrom};

use super::info::{HdrFormat, HdrHeader, HdrInfo, HdrPixelData, HdrResolution, HdrScanlineEncoding, HdrSection};
use super::pixels::PixelDecoder;
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, ImageSummary, SampleFormat};
use crate::{log_warn, Image, Limits};

const MAX_LINE_LENGTH: usize = 64 * 1024;

pub struct HdrDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    format: HdrFormat,
    resolution: Option<HdrResolution>,
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
            resolution: None,
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

            if buffer.len() < MAX_LINE_LENGTH {
                buffer.push(byte);
            }
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

            let axis = |token: &str| match token.chars().last() {
                Some(letter @ ('X' | 'Y')) => Some((letter, token.starts_with('-'))),
                _ => None,
            };
            let (first, second) = match (axis(parts[0]), axis(parts[2])) {
                (Some(first), Some(second)) if first.0 != second.0 => (first, second),
                _ => {
                    return Err(VexelError::Custom(format!(
                        "Invalid header line: {}, cant parse image dimensions",
                        line
                    )));
                }
            };

            let first_size = parts[1].parse::<u32>().map_err(|_| "Failed to parse image dimensions")?;
            let second_size = parts[3].parse::<u32>().map_err(|_| "Failed to parse image dimensions")?;

            let y_major = first.0 == 'Y';
            let (width, height, x_decreasing, y_decreasing) = if y_major {
                (second_size, first_size, second.1, first.1)
            } else {
                (first_size, second_size, first.1, second.1)
            };

            return Ok(HdrResolution {
                line: line.trim().to_string(),
                width,
                height,
                y_major,
                x_decreasing,
                y_decreasing,
            });
        }
    }

    fn scan(&mut self) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.format = HdrFormat::RGBE;
        self.resolution = None;
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
        let scanline_length = resolution.scanline_length();
        self.resolution = Some(resolution.clone());
        info.push(header_end, Some(resolution_end - header_end), HdrSection::Resolution(resolution));

        let available = self.reader.bytes_left().unwrap_or(0);
        let first_scanline = self.reader.peek_bytes(4.min(available as usize)).unwrap_or_default();
        let encoding = match first_scanline.as_slice() {
            [] => None,
            [2, 2, hi, lo] if hi & 0x80 == 0 && u32::from(u16::from_be_bytes([*hi, *lo])) == scanline_length => {
                Some(HdrScanlineEncoding::Rle)
            }
            _ => Some(HdrScanlineEncoding::Flat),
        };
        let decoded_length = (u64::from(self.width) * u64::from(self.height)).saturating_mul(4);
        info.push(resolution_end, Some(available), HdrSection::PixelData(HdrPixelData { encoding, decoded_length }));

        let color_model = if self.format == HdrFormat::XYZE { ColorModel::Xyz } else { ColorModel::Rgb };
        let mut summary = ImageSummary::new(self.width, self.height, color_model, 3, 32);
        summary.sample_format = SampleFormat::Float;
        summary.compression = encoding.map(|encoding| match encoding {
            HdrScanlineEncoding::Rle => "Adaptive RLE".to_string(),
            HdrScanlineEncoding::Flat => "None or old-style RLE".to_string(),
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

        let rgb_copies = if self.format == HdrFormat::XYZE { 2 } else { 1 };
        let bytes_per_pixel = 4 + 12 * rgb_copies;
        let total = (u64::from(self.width) * u64::from(self.height)).saturating_mul(u64::from(bytes_per_pixel));
        if total > isize::MAX as u64 {
            return Err(VexelError::LimitExceeded(format!(
                "HDR image {}x{} is too large to allocate",
                self.width, self.height
            )));
        }
        self.limits.reserve_buffer(self.width, self.height, bytes_per_pixel)?;

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

    fn read_scanlines(&mut self, scanline_length: usize, scanline_count: usize) -> VexelResult<Vec<u8>> {
        let mut rgbe_data = vec![0u8; scanline_length * scanline_count * 4];

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

        let mut channel_buf: [Vec<u8>; 4] = std::array::from_fn(|_| vec![0u8; scanline_length]);
        let mut warned_length_mismatch = false;

        for y in 0..scanline_count {
            let Some(&[b0, b1, b2, b3]) = compressed.get(cursor..cursor.saturating_add(4)) else {
                break;
            };

            let scanline_start = y * scanline_length * 4;
            let scanline = &mut rgbe_data[scanline_start..scanline_start + scanline_length * 4];

            let new_style = b0 == 2 && b1 == 2 && b2 & 0x80 == 0;
            let rle_length = usize::from(b2) << 8 | usize::from(b3);

            if new_style && rle_length == scanline_length {
                cursor += 4;

                for dst in channel_buf.iter_mut() {
                    let mut pos = 0;

                    while pos < scanline_length {
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
                            let end = (pos + run_length).min(scanline_length);
                            dst[pos..end].fill(value);
                            pos += run_length;
                        } else {
                            let end = (pos + count).min(scanline_length);
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

                for x in 0..scanline_length {
                    scanline[x * 4] = channel_buf[0][x];
                    scanline[x * 4 + 1] = channel_buf[1][x];
                    scanline[x * 4 + 2] = channel_buf[2][x];
                    scanline[x * 4 + 3] = channel_buf[3][x];
                }
            } else {
                if new_style && !warned_length_mismatch {
                    log_warn!(
                        "HDR: scanline {} has RLE length {} but the image needs {}, reading it as old-style data",
                        y,
                        rle_length,
                        scanline_length
                    );
                    warned_length_mismatch = true;
                }

                let complete;
                (cursor, complete) = Self::read_old_style_scanline(&compressed, cursor, scanline);
                if !complete {
                    log_warn!("HDR: pixel data ends inside scanline {}", y);
                    break;
                }
            }
        }

        Ok(rgbe_data)
    }

    fn read_old_style_scanline(data: &[u8], mut cursor: usize, scanline: &mut [u8]) -> (usize, bool) {
        let pixels = scanline.len() / 4;
        let mut pos = 0;
        let mut shift = 0u32;

        while pos < pixels {
            let Some(&[r, g, b, e]) = data.get(cursor..cursor.saturating_add(4)) else {
                return (cursor, false);
            };
            cursor += 4;

            if pos > 0 && r == 1 && g == 1 && b == 1 {
                let count = if shift < 32 { (usize::from(e) << shift).min(pixels - pos) } else { pixels - pos };
                let mut previous = [0u8; 4];
                previous.copy_from_slice(&scanline[(pos - 1) * 4..pos * 4]);
                for pixel in scanline[pos * 4..(pos + count) * 4].chunks_exact_mut(4) {
                    pixel.copy_from_slice(&previous);
                }
                pos += count;
                shift += 8;
            } else {
                scanline[pos * 4..pos * 4 + 4].copy_from_slice(&[r, g, b, e]);
                pos += 1;
                shift = 0;
            }
        }

        (cursor, true)
    }

    fn reorient(rgbe_data: Vec<u8>, resolution: &HdrResolution) -> Vec<u8> {
        if resolution.is_standard() {
            return rgbe_data;
        }

        let width = resolution.width as usize;
        let height = resolution.height as usize;
        let scanline_length = resolution.scanline_length() as usize;
        let mut out = vec![0u8; rgbe_data.len()];

        for (index, pixel) in rgbe_data.chunks_exact(4).enumerate() {
            let (scanline, position) = (index / scanline_length, index % scanline_length);
            let (x, y) = if resolution.y_major { (position, scanline) } else { (scanline, position) };
            let column = if resolution.x_decreasing { width - 1 - x } else { x };
            let row = if resolution.y_decreasing { y } else { height - 1 - y };
            let target = (row * width + column) * 4;
            out[target..target + 4].copy_from_slice(pixel);
        }

        out
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

        let resolution = self.resolution.clone().ok_or_else(|| VexelError::Custom("HDR: missing resolution".into()))?;
        let rgbe_data =
            self.read_scanlines(resolution.scanline_length() as usize, resolution.scanline_count() as usize)?;
        let rgbe_data = Self::reorient(rgbe_data, &resolution);

        let pixel_decoder = PixelDecoder::new(self.width, self.height, self.format);
        let mut pixel_data = pixel_decoder.decode(&rgbe_data)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
