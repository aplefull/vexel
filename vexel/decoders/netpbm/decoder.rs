use std::io::{Cursor, Read, Seek, SeekFrom};

use super::info::{NetpbmComment, NetpbmEncoding, NetpbmHeader, NetpbmInfo, NetpbmPixelData, NetpbmSection};
use super::simd;
use super::types::{NetpbmFormat, TupleType};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::ImageFrame;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary};
use crate::{log_warn, Image, Limits, PixelData};

#[derive(Clone, Copy, PartialEq)]
enum WalkMode {
    InfoOnly,
    DecodeFirst,
    DecodeAll,
}

pub struct NetPbmDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    max_value: u32,
    depth: u8,
    format: Option<NetpbmFormat>,
    tuple_type: Option<TupleType>,
    tuple_type_raw: Option<String>,
    raw_max_value: Option<u32>,
    raw_depth: Option<u32>,
    reader: BitReader<R>,
    lookahead: Option<u8>,
    image_index: u32,
    collected: NetpbmInfo,
    info: Option<NetpbmInfo>,
}

impl<R: Read + Seek> NetPbmDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            max_value: 0,
            depth: 0,
            format: None,
            tuple_type: None,
            tuple_type_raw: None,
            raw_max_value: None,
            raw_depth: None,
            reader: BitReader::new(reader),
            lookahead: None,
            image_index: 0,
            collected: NetpbmInfo::default(),
            info: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> NetpbmInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk(WalkMode::InfoOnly) {
                log_warn!("Failed to read Netpbm structure: {}", e);
            }
            self.limits = saved_limits;
            self.finish_info();
        }

        self.info.clone().unwrap_or_default()
    }

    fn position(&mut self) -> u64 {
        let position = self.reader.stream_position().unwrap_or(0);
        if self.lookahead.is_some() {
            position.saturating_sub(1)
        } else {
            position
        }
    }

    fn finish_info(&mut self) {
        let mut info = std::mem::take(&mut self.collected);
        info.sections.sort_by_key(|section| section.offset);

        let image_count = info.sections.iter().filter(|s| matches!(s.data, NetpbmSection::Header(_))).count();

        let first_header = info.sections.iter().find_map(|section| match &section.data {
            NetpbmSection::Header(header) => Some(header.clone()),
            _ => None,
        });
        let first_pixels = info.sections.iter().find_map(|section| match &section.data {
            NetpbmSection::PixelData(pixel_data) if pixel_data.image == 0 => {
                Some((pixel_data.clone(), section.length.unwrap_or(0)))
            }
            _ => None,
        });

        if let Some(header) = first_header {
            info.summary = Some(Self::build_summary(&header, first_pixels, image_count as u32));
        }

        self.info = Some(info);
    }

    fn build_summary(header: &NetpbmHeader, pixels: Option<(NetpbmPixelData, u64)>, image_count: u32) -> ImageSummary {
        let max_value = match header.max_value {
            None => 1,
            Some(0) => 255,
            Some(value) => value.min(65535),
        };
        let bits = (32 - max_value.leading_zeros()) as u8;

        let (color_model, channels) = match header.format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => (ColorModel::Bilevel, 1),
            NetpbmFormat::P2 | NetpbmFormat::P5 => (ColorModel::Gray, 1),
            NetpbmFormat::P3 | NetpbmFormat::P6 => (ColorModel::Rgb, 3),
            NetpbmFormat::P7 => {
                let depth = header.depth.filter(|&d| d > 0).unwrap_or(3);
                let model = match (&header.tuple_type, depth) {
                    (Some(TupleType::BlackAndWhite), _) => ColorModel::Bilevel,
                    (Some(TupleType::Grayscale), _) => ColorModel::Gray,
                    (Some(TupleType::RGB), _) => ColorModel::Rgb,
                    (Some(TupleType::BlackAndWhiteAlpha | TupleType::GrayscaleAlpha), _) => ColorModel::GrayAlpha,
                    (Some(TupleType::RGBAlpha), _) => ColorModel::Rgba,
                    (Some(TupleType::CMYK), _) => ColorModel::Cmyk,
                    (Some(TupleType::CMYKAlpha), _) => ColorModel::Cmyka,
                    (None, 1) => ColorModel::Gray,
                    (None, 2) => ColorModel::GrayAlpha,
                    (None, 3) => ColorModel::Rgb,
                    (None, 4) => ColorModel::Rgba,
                    (None, _) => ColorModel::Multichannel,
                };
                (model, depth.min(u32::from(u16::MAX)) as u16)
            }
        };

        let mut summary = ImageSummary::new(header.width, header.height, color_model, channels, bits);
        summary.has_alpha = matches!(color_model, ColorModel::GrayAlpha | ColorModel::Rgba | ColorModel::Cmyka);
        if image_count > 1 {
            summary.pages = Some(image_count);
        }
        if let Some((pixel_data, actual)) = pixels {
            if let Some(expected) = pixel_data.expected_length {
                summary.data = Some(DataExtent { expected, actual });
            }
        }

        summary
    }

    fn scale_to_8bit(value: u32, max_value: u32) -> u8 {
        ((value as f32 * 255.0 / max_value as f32).round() as u32).min(255) as u8
    }

    fn scale_to_16bit(value: u32, max_value: u32) -> u16 {
        ((value as f32 * 65535.0 / max_value as f32).round() as u32).min(65535) as u16
    }

    fn next_byte(&mut self) -> Option<u8> {
        if let Some(b) = self.lookahead.take() {
            return Some(b);
        }
        self.reader.read_u8().ok()
    }

    fn skip_whitespace_and_comments(&mut self) -> VexelResult<u8> {
        loop {
            let byte = match self.next_byte() {
                Some(b) => b,
                None => return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()),
            };
            match byte {
                b'#' => {
                    let comment_offset = self.position().saturating_sub(1);
                    let mut comment = String::new();
                    loop {
                        match self.next_byte() {
                            Some(b'\n') | Some(b'\r') | None => break,
                            Some(b) => comment.push(b as char),
                        }
                    }
                    let comment_end = self.position();
                    let image = self.image_index;
                    self.collected.push(
                        comment_offset,
                        Some(comment_end.saturating_sub(comment_offset)),
                        NetpbmSection::Comment(NetpbmComment { image, text: comment }),
                    );
                }
                b' ' | b'\t' | b'\n' | b'\r' => continue,
                _ => return Ok(byte),
            }
        }
    }

    fn read_decimal(&mut self, first: u8) -> VexelResult<u32> {
        let mut number = match first {
            b'0'..=b'9' => (first - b'0') as u32,
            _ => {
                log_warn!("Expected a digit, got byte: {}", first);
                0
            }
        };

        while let Some(byte) = self.next_byte() {
            match byte {
                b'0'..=b'9' => {
                    number = match number.checked_mul(10).and_then(|n| n.checked_add((byte - b'0') as u32)) {
                        Some(n) => n,
                        None => {
                            log_warn!("Number is too large: {} + {}", number, (byte - b'0') as u32);
                            number
                        }
                    };
                }
                _ => {
                    self.lookahead = Some(byte);
                    break;
                }
            }
        }

        Ok(number)
    }

    fn read_pam_tuple(&mut self, first: u8) -> VexelResult<(String, String)> {
        let mut key = String::new();
        let mut value = String::new();
        let mut reading_key = true;

        let mut byte = first;
        loop {
            match byte {
                b'\n' => break,
                b' ' | b'\t' if reading_key => {
                    reading_key = false;
                }
                _ => {
                    if reading_key {
                        key.push(byte as char);
                    } else {
                        value.push(byte as char);
                    }
                }
            }
            byte = match self.next_byte() {
                Some(b) => b,
                None => break,
            };
        }

        Ok((key.trim().to_string(), value.trim().to_string()))
    }

    fn reset_frame_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.max_value = 0;
        self.depth = 0;
        self.format = None;
        self.tuple_type = None;
        self.tuple_type_raw = None;
        self.raw_max_value = None;
        self.raw_depth = None;
    }

    fn read_header(&mut self, mode: WalkMode) -> VexelResult<()> {
        let header_start = self.position();

        let b0 = self.next_byte().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))? as u16;
        let b1 = self.next_byte().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))? as u16;
        let magick = (b0 << 8) | b1;

        let mut magic_valid = true;
        let format = match magick {
            0x5031 => NetpbmFormat::P1,
            0x5032 => NetpbmFormat::P2,
            0x5033 => NetpbmFormat::P3,
            0x5034 => NetpbmFormat::P4,
            0x5035 => NetpbmFormat::P5,
            0x5036 => NetpbmFormat::P6,
            0x5037 => NetpbmFormat::P7,
            _ => {
                log_warn!("Invalid magic number: {}", magick);
                magic_valid = false;
                NetpbmFormat::P6
            }
        };

        self.format = Some(format.clone());

        let parsed = match format {
            NetpbmFormat::P7 => self.read_pam_header(),
            _ => self.read_standard_header(format.clone()),
        };

        let header_end = self.position();
        self.collected.push(
            header_start,
            Some(header_end.saturating_sub(header_start)),
            NetpbmSection::Header(NetpbmHeader {
                image: self.image_index,
                format,
                magic_valid,
                width: self.width,
                height: self.height,
                max_value: self.raw_max_value,
                depth: self.raw_depth,
                tuple_type: self.tuple_type.clone(),
                tuple_type_raw: self.tuple_type_raw.clone(),
            }),
        );

        parsed?;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if mode != WalkMode::InfoOnly {
            self.limits.reserve_buffer(self.width, self.height, 4)?;
        }

        Ok(())
    }

    fn read_standard_header(&mut self, format: NetpbmFormat) -> VexelResult<()> {
        let first = self.skip_whitespace_and_comments()?;
        self.width = self.read_decimal(first)?;

        let first = self.skip_whitespace_and_comments()?;
        self.height = self.read_decimal(first)?;

        match format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => {
                self.max_value = 1;
            }
            _ => {
                let first = self.skip_whitespace_and_comments()?;
                self.max_value = self.read_decimal(first)?;
                self.raw_max_value = Some(self.max_value);

                if self.max_value == 0 {
                    log_warn!("Invalid MAXVAL value: {}", self.max_value);
                    self.max_value = 255;
                }

                if self.max_value > 65535 {
                    log_warn!("Invalid MAXVAL value: {}", self.max_value);
                    self.max_value = 65535;
                }
            }
        }

        self.lookahead = None;

        Ok(())
    }

    fn read_pam_header(&mut self) -> VexelResult<()> {
        loop {
            let first = self.skip_whitespace_and_comments()?;

            let (key, value) = self.read_pam_tuple(first)?;

            match key.as_str() {
                "ENDHDR" => break,
                "WIDTH" => {
                    self.width = value.parse::<u32>().or_else(|_| {
                        log_warn!("Invalid WIDTH value: {}", value);
                        Ok::<u32, VexelError>(0)
                    })?
                }
                "HEIGHT" => {
                    self.height = value.parse::<u32>().or_else(|_| {
                        log_warn!("Invalid HEIGHT value: {}", value);
                        Ok::<u32, VexelError>(0)
                    })?
                }
                "DEPTH" => {
                    self.raw_depth = value.parse::<u32>().ok();
                    self.depth = value.parse::<u8>().or_else(|_| {
                        log_warn!("Invalid DEPTH value: {}", value);
                        Ok::<u8, VexelError>(3)
                    })?
                }
                "MAXVAL" => {
                    self.raw_max_value = value.parse::<u32>().ok();
                    self.max_value = value.parse().or_else(|_| {
                        log_warn!("Invalid MAXVAL value: {}", value);
                        Ok::<u32, VexelError>(255)
                    })?
                }
                "TUPLTYPE" => {
                    let concatenated = match &self.tuple_type_raw {
                        Some(existing) => format!("{} {}", existing, value),
                        None => value.clone(),
                    };
                    self.tuple_type_raw = Some(concatenated);
                }
                _ => {}
            }
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        };

        self.tuple_type = self.tuple_type_raw.as_deref().and_then(|s| match s {
            "BLACKANDWHITE" => Some(TupleType::BlackAndWhite),
            "GRAYSCALE" => Some(TupleType::Grayscale),
            "RGB" => Some(TupleType::RGB),
            "BLACKANDWHITE_ALPHA" => Some(TupleType::BlackAndWhiteAlpha),
            "GRAYSCALE_ALPHA" => Some(TupleType::GrayscaleAlpha),
            "RGB_ALPHA" => Some(TupleType::RGBAlpha),
            "CMYK" => Some(TupleType::CMYK),
            "CMYK_ALPHA" => Some(TupleType::CMYKAlpha),
            _ => {
                log_warn!("Unrecognized TUPLTYPE value: {}", s);
                None
            }
        });

        Ok(())
    }

    fn read_ascii_number(&mut self) -> VexelResult<u32> {
        let first = self.skip_whitespace_and_comments()?;
        self.read_decimal(first)
    }

    fn read_binary_frame_data(&mut self, byte_count: usize) -> VexelResult<Vec<u8>> {
        let mut buf = Vec::new();
        buf.try_reserve_exact(byte_count).map_err(|_| {
            VexelError::LimitExceeded(format!("Netpbm frame of {} bytes cannot be allocated", byte_count))
        })?;
        buf.resize(byte_count, 0);
        if self.reader.read_exact(&mut buf).is_err() {
            let available = self.reader.read_to_end().unwrap_or_default();
            let n = available.len().min(buf.len());
            buf[..n].copy_from_slice(&available[..n]);
        }
        Ok(buf)
    }

    fn decode_ascii_bitmap(&mut self) -> VexelResult<PixelData> {
        let pixel_count = (self.width as usize).saturating_mul(self.height as usize);
        let mut image_data = Vec::with_capacity(pixel_count.min(1 << 20));

        for _ in 0..pixel_count {
            let value = match self.read_ascii_number() {
                Ok(v) => v.clamp(0, self.max_value),
                Err(e) => {
                    log_warn!("Error reading ASCII number: {:?}", e);
                    0
                }
            };
            image_data.push(!(value as u8) & 1);
        }

        Ok(PixelData::L1(image_data))
    }

    fn read_ascii_samples(&mut self, count: usize) -> Vec<u32> {
        let max_value = self.max_value;
        let mut samples = Vec::with_capacity(count.min(1 << 20));
        for _ in 0..count {
            let v = self.read_ascii_number().map(|v| v.clamp(0, max_value)).unwrap_or_else(|e| {
                log_warn!("Error reading ASCII number: {:?}", e);
                0
            });
            samples.push(v);
        }
        samples
    }

    fn decode_ascii_graymap(&mut self) -> VexelResult<PixelData> {
        let pixel_count = (self.width as usize).saturating_mul(self.height as usize);
        let samples = self.read_ascii_samples(pixel_count);

        if self.max_value > 255 {
            let mut image_data = vec![0u16; pixel_count];
            simd::scale_u32_to_u16(&samples, &mut image_data, self.max_value);
            Ok(PixelData::L16(image_data))
        } else {
            let mut image_data = vec![0u8; pixel_count];
            simd::scale_u32_to_u8(&samples, &mut image_data, self.max_value);
            Ok(PixelData::L8(image_data))
        }
    }

    fn decode_ascii_pixmap(&mut self) -> VexelResult<PixelData> {
        let sample_count = (self.width as usize).saturating_mul(self.height as usize).saturating_mul(3);
        let samples = self.read_ascii_samples(sample_count);

        if self.max_value > 255 {
            let mut image_data = vec![0u16; sample_count];
            simd::scale_u32_to_u16(&samples, &mut image_data, self.max_value);
            Ok(PixelData::RGB16(image_data))
        } else {
            let mut image_data = vec![0u8; sample_count];
            simd::scale_u32_to_u8(&samples, &mut image_data, self.max_value);
            Ok(PixelData::RGB8(image_data))
        }
    }

    fn decode_binary_bitmap(&self, data: &[u8]) -> VexelResult<PixelData> {
        let mut image_data = vec![0u8; (self.width as usize).saturating_mul(self.height as usize)];
        simd::unpack_bits(data, &mut image_data, self.width, self.height);
        Ok(PixelData::L1(image_data))
    }

    fn decode_binary_graymap(&self, data: &[u8]) -> VexelResult<PixelData> {
        let pixel_count = (self.width as usize).saturating_mul(self.height as usize);

        if self.max_value <= 255 {
            let mut image_data = vec![0u8; pixel_count];
            simd::scale_u8(data, &mut image_data, self.max_value as u8);
            return Ok(PixelData::L8(image_data));
        }

        let mut image_data = vec![0u16; pixel_count];
        simd::scale_u16_be(data, &mut image_data, self.max_value as u16);
        Ok(PixelData::L16(image_data))
    }

    fn decode_binary_pixmap(&self, data: &[u8]) -> VexelResult<PixelData> {
        let sample_count = (self.width as usize).saturating_mul(self.height as usize).saturating_mul(3);

        if self.max_value <= 255 {
            let mut image_data = vec![0u8; sample_count];
            simd::scale_u8(data, &mut image_data, self.max_value as u8);
            return Ok(PixelData::RGB8(image_data));
        }

        let mut image_data = vec![0u16; sample_count];
        simd::scale_u16_be(data, &mut image_data, self.max_value as u16);
        Ok(PixelData::RGB16(image_data))
    }

    fn decode_pam(&self, data: &[u8]) -> VexelResult<PixelData> {
        let depth = self.depth;
        let max_value = self.max_value;
        let pixel_count = (self.width as usize).saturating_mul(self.height as usize);
        let is_16bit = max_value > 255;

        match (&self.tuple_type, depth) {
            (Some(TupleType::BlackAndWhite), 1) => {
                let mut image_data = vec![0u8; pixel_count];
                for (d, s) in image_data.iter_mut().zip(data.iter()) {
                    *d = s & 1;
                }
                Ok(PixelData::L1(image_data))
            }

            (Some(TupleType::Grayscale), 1) => {
                if !is_16bit {
                    let mut image_data = vec![0u8; pixel_count];
                    simd::scale_u8(data, &mut image_data, max_value as u8);
                    Ok(PixelData::L8(image_data))
                } else {
                    let mut image_data = vec![0u16; pixel_count];
                    simd::scale_u16_be(data, &mut image_data, max_value as u16);
                    Ok(PixelData::L16(image_data))
                }
            }

            (Some(TupleType::RGB), 3) => {
                let sample_count = pixel_count * 3;
                if !is_16bit {
                    let mut image_data = vec![0u8; sample_count];
                    simd::scale_u8(data, &mut image_data, max_value as u8);
                    Ok(PixelData::RGB8(image_data))
                } else {
                    let mut image_data = vec![0u16; sample_count];
                    simd::scale_u16_be(data, &mut image_data, max_value as u16);
                    Ok(PixelData::RGB16(image_data))
                }
            }

            (Some(TupleType::BlackAndWhiteAlpha), 2) => {
                let mut image_data = vec![0u8; pixel_count * 2];
                let mut reader = BitReader::new(Cursor::new(data));
                for chunk in image_data.chunks_exact_mut(2) {
                    let value = reader.read_u8().unwrap_or(0);
                    let alpha = reader.read_u8().unwrap_or(0);
                    chunk[0] = Self::scale_to_8bit((value & 1) as u32, 1);
                    chunk[1] = Self::scale_to_8bit(alpha as u32, max_value);
                }
                Ok(PixelData::LA8(image_data))
            }

            (Some(TupleType::GrayscaleAlpha), 2) => {
                let sample_count = pixel_count * 2;
                if !is_16bit {
                    let mut image_data = vec![0u8; sample_count];
                    simd::scale_u8(data, &mut image_data, max_value as u8);
                    Ok(PixelData::LA8(image_data))
                } else {
                    let mut image_data = vec![0u16; sample_count];
                    simd::scale_u16_be(data, &mut image_data, max_value as u16);
                    Ok(PixelData::LA16(image_data))
                }
            }

            (Some(TupleType::RGBAlpha), 4) => {
                let sample_count = pixel_count * 4;
                if !is_16bit {
                    let mut image_data = vec![0u8; sample_count];
                    simd::scale_u8(data, &mut image_data, max_value as u8);
                    Ok(PixelData::RGBA8(image_data))
                } else {
                    let mut image_data = vec![0u16; sample_count];
                    simd::scale_u16_be(data, &mut image_data, max_value as u16);
                    Ok(PixelData::RGBA16(image_data))
                }
            }

            (Some(TupleType::CMYK), 4) => {
                let mv = max_value as f32;
                if !is_16bit {
                    let mut image_data = vec![0u8; pixel_count * 3];
                    for (i, chunk) in data.chunks_exact(4).enumerate() {
                        let c = chunk[0] as f32 / mv;
                        let m = chunk[1] as f32 / mv;
                        let y = chunk[2] as f32 / mv;
                        let k = chunk[3] as f32 / mv;
                        image_data[i * 3] = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
                        image_data[i * 3 + 1] = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
                        image_data[i * 3 + 2] = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;
                    }
                    Ok(PixelData::RGB8(image_data))
                } else {
                    let mut image_data = vec![0u16; pixel_count * 3];
                    let mut reader = BitReader::new(Cursor::new(data));
                    for i in 0..pixel_count {
                        let c = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let m = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let y = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let k = reader.read_u16().unwrap_or(0) as f32 / mv;
                        image_data[i * 3] = ((1.0 - c) * (1.0 - k) * 65535.0).round() as u16;
                        image_data[i * 3 + 1] = ((1.0 - m) * (1.0 - k) * 65535.0).round() as u16;
                        image_data[i * 3 + 2] = ((1.0 - y) * (1.0 - k) * 65535.0).round() as u16;
                    }
                    Ok(PixelData::RGB16(image_data))
                }
            }

            (Some(TupleType::CMYKAlpha), 5) => {
                let mv = max_value as f32;
                if !is_16bit {
                    let mut image_data = vec![0u8; pixel_count * 4];
                    for (i, chunk) in data.chunks_exact(5).enumerate() {
                        let c = chunk[0] as f32 / mv;
                        let m = chunk[1] as f32 / mv;
                        let y = chunk[2] as f32 / mv;
                        let k = chunk[3] as f32 / mv;
                        let a = chunk[4];
                        image_data[i * 4] = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
                        image_data[i * 4 + 1] = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
                        image_data[i * 4 + 2] = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;
                        image_data[i * 4 + 3] = Self::scale_to_8bit(a as u32, max_value);
                    }
                    Ok(PixelData::RGBA8(image_data))
                } else {
                    let mut image_data = vec![0u16; pixel_count * 4];
                    let mut reader = BitReader::new(Cursor::new(data));
                    for i in 0..pixel_count {
                        let c = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let m = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let y = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let k = reader.read_u16().unwrap_or(0) as f32 / mv;
                        let a = reader.read_u16().unwrap_or(0);
                        image_data[i * 4] = ((1.0 - c) * (1.0 - k) * 65535.0).round() as u16;
                        image_data[i * 4 + 1] = ((1.0 - m) * (1.0 - k) * 65535.0).round() as u16;
                        image_data[i * 4 + 2] = ((1.0 - y) * (1.0 - k) * 65535.0).round() as u16;
                        image_data[i * 4 + 3] = Self::scale_to_16bit(a as u32, max_value);
                    }
                    Ok(PixelData::RGBA16(image_data))
                }
            }

            _ => {
                log_warn!(
                    "Incorrect tuple type / depth combination: {:?}, {}. Inferring layout from depth.",
                    self.tuple_type,
                    self.depth
                );

                let depth = depth as usize;

                match (depth, is_16bit) {
                    (1, false) => {
                        let mut image_data = vec![0u8; pixel_count];
                        simd::scale_u8(data, &mut image_data, max_value as u8);
                        Ok(PixelData::L8(image_data))
                    }
                    (1, true) => {
                        let mut image_data = vec![0u16; pixel_count];
                        simd::scale_u16_be(data, &mut image_data, max_value as u16);
                        Ok(PixelData::L16(image_data))
                    }
                    (2, false) => {
                        let mut image_data = vec![0u8; pixel_count * 2];
                        simd::scale_u8(data, &mut image_data, max_value as u8);
                        Ok(PixelData::LA8(image_data))
                    }
                    (2, true) => {
                        let mut image_data = vec![0u16; pixel_count * 2];
                        simd::scale_u16_be(data, &mut image_data, max_value as u16);
                        Ok(PixelData::LA16(image_data))
                    }
                    (3, false) => {
                        let mut image_data = vec![0u8; pixel_count * 3];
                        simd::scale_u8(data, &mut image_data, max_value as u8);
                        Ok(PixelData::RGB8(image_data))
                    }
                    (3, true) => {
                        let mut image_data = vec![0u16; pixel_count * 3];
                        simd::scale_u16_be(data, &mut image_data, max_value as u16);
                        Ok(PixelData::RGB16(image_data))
                    }
                    (4, false) => {
                        let mut image_data = vec![0u8; pixel_count * 4];
                        simd::scale_u8(data, &mut image_data, max_value as u8);
                        Ok(PixelData::RGBA8(image_data))
                    }
                    (4, true) => {
                        let mut image_data = vec![0u16; pixel_count * 4];
                        simd::scale_u16_be(data, &mut image_data, max_value as u16);
                        Ok(PixelData::RGBA16(image_data))
                    }
                    (_, false) => {
                        let mut image_data = vec![0u8; pixel_count * 3];
                        let src = &data[..pixel_count * depth];
                        for (px, chunk) in src.chunks_exact(depth).zip(image_data.chunks_exact_mut(3)) {
                            chunk[0] = Self::scale_to_8bit(px[0] as u32, max_value);
                            chunk[1] = Self::scale_to_8bit(px[1] as u32, max_value);
                            chunk[2] = Self::scale_to_8bit(px[2] as u32, max_value);
                        }
                        Ok(PixelData::RGB8(image_data))
                    }
                    (_, true) => {
                        let mut image_data = vec![0u16; pixel_count * 3];
                        let mut reader = BitReader::new(Cursor::new(data));
                        for chunk in image_data.chunks_exact_mut(3) {
                            #[allow(clippy::needless_range_loop)]
                            for i in 0..depth {
                                let v = reader.read_u16().unwrap_or(0);
                                if i < 3 {
                                    chunk[i] = Self::scale_to_16bit(v as u32, max_value);
                                }
                            }
                        }
                        Ok(PixelData::RGB16(image_data))
                    }
                }
            }
        }
    }

    fn pixel_layout(&self) -> NetpbmPixelData {
        let pixels = u64::from(self.width) * u64::from(self.height);
        let bytes_per_sample = if self.max_value > 255 { 2u8 } else { 1u8 };
        let (encoding, samples, bytes_per_sample, expected_length) = match &self.format {
            Some(NetpbmFormat::P1) => (NetpbmEncoding::Plain, pixels, None, None),
            Some(NetpbmFormat::P2) => (NetpbmEncoding::Plain, pixels, None, None),
            Some(NetpbmFormat::P3) => (NetpbmEncoding::Plain, pixels.saturating_mul(3), None, None),
            Some(NetpbmFormat::P4) => {
                let expected = u64::from(self.width.div_ceil(8)) * u64::from(self.height);
                (NetpbmEncoding::Raw, pixels, None, Some(expected))
            }
            Some(NetpbmFormat::P5) => (
                NetpbmEncoding::Raw,
                pixels,
                Some(bytes_per_sample),
                Some(pixels.saturating_mul(u64::from(bytes_per_sample))),
            ),
            Some(NetpbmFormat::P7) => {
                let depth = if self.depth == 0 { 3 } else { self.depth };
                let max_value = if self.max_value == 0 { 255 } else { self.max_value };
                let bytes_per_sample = if max_value > 255 { 2u8 } else { 1u8 };
                let samples = pixels.saturating_mul(u64::from(depth));
                (
                    NetpbmEncoding::Raw,
                    samples,
                    Some(bytes_per_sample),
                    Some(samples.saturating_mul(u64::from(bytes_per_sample))),
                )
            }
            Some(NetpbmFormat::P6) | None => {
                let samples = pixels.saturating_mul(3);
                (
                    NetpbmEncoding::Raw,
                    samples,
                    Some(bytes_per_sample),
                    Some(samples.saturating_mul(u64::from(bytes_per_sample))),
                )
            }
        };

        NetpbmPixelData { image: self.image_index, encoding, samples, bytes_per_sample, expected_length }
    }

    fn skip_frame(&mut self, layout: &NetpbmPixelData) -> VexelResult<()> {
        match layout.expected_length {
            Some(expected) => {
                let available = self.reader.bytes_left().unwrap_or(0);
                let position = self.reader.stream_position()?;
                self.reader.seek(SeekFrom::Start(position + expected.min(available)))?;
            }
            None => {
                for _ in 0..layout.samples {
                    if self.read_ascii_number().is_err() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn read_and_decode_frame(&mut self) -> VexelResult<PixelData> {
        let bytes_per_sample = if self.max_value > 255 { 2usize } else { 1usize };

        let samples_per_pixel = match &self.format {
            Some(NetpbmFormat::P7) => u64::from(self.depth.max(3)),
            Some(NetpbmFormat::P3) | Some(NetpbmFormat::P6) | None => 3,
            _ => 1,
        };
        let frame_bytes = (u64::from(self.width) * u64::from(self.height)).checked_mul(samples_per_pixel * 2);
        if frame_bytes.is_none_or(|bytes| bytes > isize::MAX as u64) {
            return Err(VexelError::LimitExceeded(format!(
                "Netpbm image {}x{} is too large to allocate",
                self.width, self.height
            )));
        }

        match &self.format {
            Some(NetpbmFormat::P1) => self.decode_ascii_bitmap(),
            Some(NetpbmFormat::P2) => self.decode_ascii_graymap(),
            Some(NetpbmFormat::P3) => self.decode_ascii_pixmap(),
            Some(NetpbmFormat::P4) => {
                let bytes_per_row = self.width.div_ceil(8) as usize;
                let byte_count = bytes_per_row.saturating_mul(self.height as usize);
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_bitmap(&data)
            }
            Some(NetpbmFormat::P5) => {
                let byte_count =
                    (self.width as usize).saturating_mul(self.height as usize).saturating_mul(bytes_per_sample);
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_graymap(&data)
            }
            Some(NetpbmFormat::P6) => {
                let byte_count = (self.width as usize)
                    .saturating_mul(self.height as usize)
                    .saturating_mul(3)
                    .saturating_mul(bytes_per_sample);
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_pixmap(&data)
            }
            Some(NetpbmFormat::P7) => {
                let depth = if self.depth == 0 { 3 } else { self.depth } as usize;
                let max_value = if self.max_value == 0 { 255 } else { self.max_value };
                let bps = if max_value > 255 { 2usize } else { 1usize };
                let byte_count = (self.width as usize)
                    .saturating_mul(self.height as usize)
                    .saturating_mul(depth)
                    .saturating_mul(bps);
                let data = self.read_binary_frame_data(byte_count)?;

                if self.depth == 0 {
                    self.depth = 3;
                }
                if self.max_value == 0 {
                    self.max_value = 255;
                }
                if self.max_value > 65535 {
                    self.max_value = 65535;
                }

                self.decode_pam(&data)
            }
            None => {
                log_warn!("Format not set before decoding, assuming binary pixmap (P6)");
                let byte_count = (self.width as usize)
                    .saturating_mul(self.height as usize)
                    .saturating_mul(3)
                    .saturating_mul(bytes_per_sample);
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_pixmap(&data)
            }
        }
    }

    fn has_more_data(&mut self) -> bool {
        match self.skip_whitespace_and_comments() {
            Ok(byte) => {
                self.lookahead = Some(byte);
                byte == b'P'
            }
            Err(_) => false,
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut pages = self.walk(WalkMode::DecodeFirst)?;
        if pages.is_empty() {
            return Err(VexelError::Custom("No Netpbm image decoded".to_string()));
        }
        Ok(pages.remove(0))
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        let pages = self.walk(WalkMode::DecodeAll)?;
        self.finish_info();
        Ok(pages)
    }

    fn walk(&mut self, mode: WalkMode) -> VexelResult<Vec<Image>> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.lookahead = None;
        self.collected = NetpbmInfo::default();
        self.image_index = 0;

        let mut pages: Vec<Image> = Vec::new();

        loop {
            self.reset_frame_state();

            match self.read_header(mode) {
                Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
                Err(e) => {
                    if self.image_index == 0 {
                        return Err(e);
                    }
                    log_warn!("Stopping at image {}: failed to read header: {}", self.image_index + 1, e);
                    break;
                }
                Ok(_) => {}
            }

            let layout = self.pixel_layout();
            let start = self.position();
            let result = match mode {
                WalkMode::InfoOnly => self.skip_frame(&layout).map(|_| None),
                _ => self.read_and_decode_frame().map(Some),
            };
            let end = self.position();
            self.collected.push(start, Some(end.saturating_sub(start)), NetpbmSection::PixelData(layout));

            match result {
                Ok(Some(mut pixel_data)) => {
                    pixel_data.correct_pixels(self.width, self.height);
                    pages.push(Image::from_frame(ImageFrame::new(self.width, self.height, pixel_data, 0)));
                }
                Ok(None) => {}
                Err(e) => {
                    if self.image_index == 0 {
                        return Err(e);
                    }
                    log_warn!("Stopping at image {}: failed to decode: {}", self.image_index + 1, e);
                    break;
                }
            }

            self.image_index += 1;

            if mode == WalkMode::DecodeFirst || !self.has_more_data() {
                break;
            }
        }

        Ok(pages)
    }
}
