use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{ImageFrame, PixelFormat};
use crate::utils::info::NetpbmInfo;
use crate::{log_warn, Image, PixelData};
use std::io::{Cursor, Read, Seek};

use super::simd;
use super::types::{NetpbmFormat, TupleType};

pub struct NetPbmDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    max_value: u32,
    depth: u8,
    format: Option<NetpbmFormat>,
    tuple_type: Option<TupleType>,
    tuple_type_raw: Option<String>,
    reader: BitReader<R>,
    lookahead: Option<u8>,
}

impl<R: Read + Seek> NetPbmDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            max_value: 0,
            depth: 0,
            format: None,
            tuple_type: None,
            tuple_type_raw: None,
            reader: BitReader::new(reader),
            lookahead: None,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> NetpbmInfo {
        NetpbmInfo {
            width: self.width,
            height: self.height,
            max_value: self.max_value,
            depth: self.depth,
            format: self.format.clone(),
            tuple_type: self.tuple_type.clone(),
        }
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
                b'#' => loop {
                    match self.next_byte() {
                        Some(b'\n') | Some(b'\r') | None => break,
                        _ => {}
                    }
                },
                b' ' | b'\t' | b'\n' | b'\r' => continue,
                _ => return Ok(byte),
            }
        }
    }

    fn read_decimal(&mut self, first: u8) -> VexelResult<u32> {
        let mut number = (first - b'0') as u32;

        loop {
            let byte = match self.next_byte() {
                Some(b) => b,
                None => break,
            };
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
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let b0 = self.next_byte().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))? as u16;
        let b1 = self.next_byte().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))? as u16;
        let magick = (b0 << 8) | b1;

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
                NetpbmFormat::P6
            }
        };

        self.format = Some(format.clone());

        match format {
            NetpbmFormat::P7 => self.read_pam_header()?,
            _ => self.read_standard_header(format)?,
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
                    self.depth = value.parse::<u8>().or_else(|_| {
                        log_warn!("Invalid DEPTH value: {}", value);
                        Ok::<u8, VexelError>(3)
                    })?
                }
                "MAXVAL" => {
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
            return Err(VexelError::InvalidDimensions {
                width: self.width,
                height: self.height,
            });
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
        let mut buf = vec![0u8; byte_count];
        match self.reader.read_exact(&mut buf) {
            Ok(()) => Ok(buf),
            Err(_) => Ok(self.reader.read_to_end().unwrap_or_default()),
        }
    }

    fn decode_ascii_bitmap(&mut self) -> VexelResult<PixelData> {
        let pixel_count = (self.width * self.height) as usize;
        let mut image_data = Vec::with_capacity(pixel_count);

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
        let mut samples = Vec::with_capacity(count);
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
        let pixel_count = (self.width * self.height) as usize;
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
        let sample_count = (self.width * self.height) as usize * 3;
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
        let mut image_data = vec![0u8; (self.width * self.height) as usize];
        simd::unpack_bits(data, &mut image_data, self.width, self.height);
        Ok(PixelData::L1(image_data))
    }

    fn decode_binary_graymap(&self, data: &[u8]) -> VexelResult<PixelData> {
        let pixel_count = (self.width * self.height) as usize;

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
        let sample_count = (self.width * self.height) as usize * 3;

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
        let pixel_count = (self.width * self.height) as usize;
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

    fn read_and_decode_frame(&mut self) -> VexelResult<PixelData> {
        let bytes_per_sample = if self.max_value > 255 { 2usize } else { 1usize };

        match &self.format {
            Some(NetpbmFormat::P1) => self.decode_ascii_bitmap(),
            Some(NetpbmFormat::P2) => self.decode_ascii_graymap(),
            Some(NetpbmFormat::P3) => self.decode_ascii_pixmap(),
            Some(NetpbmFormat::P4) => {
                let bytes_per_row = ((self.width + 7) / 8) as usize;
                let byte_count = bytes_per_row * self.height as usize;
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_bitmap(&data)
            }
            Some(NetpbmFormat::P5) => {
                let byte_count = self.width as usize * self.height as usize * bytes_per_sample;
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_graymap(&data)
            }
            Some(NetpbmFormat::P6) => {
                let byte_count = self.width as usize * self.height as usize * 3 * bytes_per_sample;
                let data = self.read_binary_frame_data(byte_count)?;
                self.decode_binary_pixmap(&data)
            }
            Some(NetpbmFormat::P7) => {
                let depth = if self.depth == 0 { 3 } else { self.depth } as usize;
                let max_value = if self.max_value == 0 { 255 } else { self.max_value };
                let bps = if max_value > 255 { 2usize } else { 1usize };
                let byte_count = self.width as usize * self.height as usize * depth * bps;
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
                let byte_count = self.width as usize * self.height as usize * 3 * bytes_per_sample;
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
        let mut frames: Vec<ImageFrame> = Vec::new();
        let mut first_width = 0u32;
        let mut first_height = 0u32;
        let mut first_pixel_format = PixelFormat::RGB8;

        loop {
            self.reset_frame_state();

            if let Err(_) = self.read_header() {
                if frames.is_empty() {
                    return Err(VexelError::Custom("Error reading header".to_string()));
                }
                break;
            }

            let width = self.width;
            let height = self.height;

            let mut pixel_data = match self.read_and_decode_frame() {
                Ok(data) => data,
                Err(_) => {
                    if frames.is_empty() {
                        return Err(VexelError::Custom("Error decoding image".to_string()));
                    }
                    break;
                }
            };

            pixel_data.correct_pixels(width, height);

            if frames.is_empty() {
                first_width = width;
                first_height = height;
                first_pixel_format = pixel_data.pixel_format();
            }

            frames.push(ImageFrame::new(width, height, pixel_data, 0));

            if !self.has_more_data() {
                break;
            }
        }

        if frames.len() == 1 {
            let frame = frames.remove(0);
            return Ok(Image::from_frame(frame));
        }

        Ok(Image::new(first_width, first_height, first_pixel_format, frames))
    }
}
