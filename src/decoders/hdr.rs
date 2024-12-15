use std::io::{Read, Seek, SeekFrom};
use crate::bitreader::BitReader;
use crate::{log_warn, Image, PixelData};
use crate::utils::error::{VexelError, VexelResult};

#[derive(Debug, Clone, Copy)]
enum HdrFormat {
    RGBE,
    XYZE,
}

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
                    primaries[0], primaries[1], primaries[2], primaries[3],
                    primaries[4], primaries[5], primaries[6], primaries[7],
                ]);

                continue;
            }

            // We have reached the end of the header
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
                    return Err(VexelError::Custom(format!("Invalid header line: {}, cant parse image dimensions", line)));
                }
            };

            self.width = dim1.parse::<u32>()
                .map_err(|_| "Failed to parse width")?;
            self.height = dim2.parse::<u32>()
                .map_err(|_| "Failed to parse height")?;

            if self.width == 0 || self.height == 0 {
                return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
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

    fn decode_pixels(&mut self) -> VexelResult<PixelData> {
        let num_pixels = (self.width * self.height) as usize;
        let mut rgbe_data = vec![0u8; num_pixels * 4];

        for y in 0..self.height {
            let rle_header = self.reader.read_u16()?;
            let rle_width = self.reader.read_u16()?;

            if rle_header == 0x0202 && rle_width == self.width as u16 {
                // RLE encoded scanline
                let mut channel_data = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

                for component in 0..4 {
                    let mut pos = 0;

                    while pos < self.width as usize {
                        let count = self.reader.read_u8()? as usize;

                        if count > 128 {
                            let run_length = count - 128;
                            let value = self.reader.read_u8()?;

                            for _ in 0..run_length {
                                channel_data[component].push(value);
                            }

                            pos += run_length;
                        } else {
                            for _ in 0..count {
                                let value = self.reader.read_u8()?;
                                channel_data[component].push(value);
                            }

                            pos += count;
                        }
                    }
                }

                let scanline_start = (y * self.width) as usize * 4;
                for x in 0..self.width as usize {
                    for component_index in 0..4 {
                        if scanline_start + x * 4 + component_index >= rgbe_data.len() {
                            log_warn!("Scanline index out of bounds: {} >= {}", scanline_start + x * 4 + component_index, rgbe_data.len());
                            break;
                        }

                        if x >= channel_data[component_index].len() {
                            log_warn!("Scanline index out of bounds: {} >= {}", x, channel_data[component_index].len());
                            break;
                        }

                        rgbe_data[scanline_start + x * 4 + component_index] = channel_data[component_index][x];
                    }
                }
            } else {
                // Uncompressed scanline
                self.reader.seek(SeekFrom::Current(-4))?;

                let scanline_start = (y * self.width) as usize * 4;

                if scanline_start + 4 >= rgbe_data.len() {
                    log_warn!("Scanline index out of bounds: {} >= {}", scanline_start + 4, rgbe_data.len());
                    break;
                }

                self.reader.read_exact(&mut rgbe_data[scanline_start..scanline_start + 4])?;

                let bytes_to_read = (self.width as usize - 1) * 4;

                if scanline_start + bytes_to_read + 4 >= rgbe_data.len() {
                    log_warn!("Scanline index out of bounds: {} >= {}", scanline_start + bytes_to_read + 4, rgbe_data.len());
                    break;
                }

                self.reader.read_exact(&mut rgbe_data[scanline_start + 4..scanline_start + bytes_to_read + 4])?;
            }
        }

        let mut rgb_data = vec![0f32; num_pixels * 3];

        for i in 0..num_pixels {
            if i * 4 + 3 >= rgbe_data.len() {
                log_warn!("Pixel index out of bounds: {} >= {}", i * 4 + 3, rgbe_data.len());
                continue;
            }
            
            if i * 3 + 2 >= rgb_data.len() {
                log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, rgb_data.len());
                continue;
            }

            let rgbe = &rgbe_data[i * 4..(i + 1) * 4];
            let rgb = &mut rgb_data[i * 3..(i + 1) * 3];

            if rgbe[3] != 0 {
                let scale = f32::exp2(rgbe[3] as f32 - 128.0 - 8.0);

                rgb[0] = rgbe[0] as f32 * scale;
                rgb[1] = rgbe[1] as f32 * scale;
                rgb[2] = rgbe[2] as f32 * scale;
            } else {
                rgb[0] = 0.0;
                rgb[1] = 0.0;
                rgb[2] = 0.0;
            }
        }

        match self.format {
            HdrFormat::RGBE => Ok(PixelData::RGB32F(rgb_data)),
            HdrFormat::XYZE => {
                let mut final_data = vec![0f32; num_pixels * 3];

                for i in 0..num_pixels {
                    if i * 3 + 2 >= rgb_data.len() {
                        log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, rgb_data.len());
                        continue;
                    }
                    
                    if i * 3 + 2 >= final_data.len() {
                        log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, final_data.len());
                        continue;
                    }
                    
                    let xyz = &rgb_data[i * 3..(i + 1) * 3];
                    let rgb = &mut final_data[i * 3..(i + 1) * 3];

                    rgb[0] = 3.2404542 * xyz[0] - 1.5371385 * xyz[1] - 0.4985314 * xyz[2];
                    rgb[1] = -0.9692660 * xyz[0] + 1.8760108 * xyz[1] + 0.0415560 * xyz[2];
                    rgb[2] = 0.0556434 * xyz[0] - 0.2040259 * xyz[1] + 1.0572252 * xyz[2];

                    rgb[0] = rgb[0].max(0.0);
                    rgb[1] = rgb[1].max(0.0);
                    rgb[2] = rgb[2].max(0.0);
                }

                Ok(PixelData::RGB32F(final_data))
            }
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let mut pixel_data = self.decode_pixels()?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
