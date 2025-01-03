use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::NetpbmInfo;
use crate::{log_error, log_warn, Image, PixelData};
use serde::Serialize;
use std::cmp::PartialEq;
use std::fmt::Debug;
use std::io::{Cursor, Read, Seek, SeekFrom};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum NetpbmFormat {
    P1, // ASCII bitmap
    P2, // ASCII graymap
    P3, // ASCII pixmap
    P4, // Binary bitmap
    P5, // Binary graymap
    P6, // Binary pixmap
    P7, // PAM
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize)]
pub enum TupleType {
    BlackAndWhite,
    Grayscale,
    RGB,
    BlackAndWhiteAlpha,
    GrayscaleAlpha,
    RGBAlpha,
}

pub struct NetPbmDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    max_value: u32,
    depth: u8,
    format: Option<NetpbmFormat>,
    tuple_type: Option<TupleType>,
    data: Vec<u8>,
    reader: BitReader<R>,
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
            data: Vec::new(),
            reader: BitReader::new(reader),
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

    fn skip_whitespace_and_comments<T: Read + Seek>(reader: &mut BitReader<T>) -> VexelResult<()> {
        loop {
            let byte = reader.read_u8()?;
            match byte {
                b'#' => loop {
                    let b = reader.read_u8()?;
                    if b == b'\n' {
                        break;
                    }
                },
                b' ' | b'\t' | b'\n' | b'\r' => continue,
                _ => {
                    reader.seek(SeekFrom::Current(-1))?;
                    break;
                }
            }
        }

        Ok(())
    }

    fn read_decimal<T: Read + Seek>(reader: &mut BitReader<T>) -> VexelResult<u32> {
        let mut number = 0u32;
        let mut has_digits = false;

        loop {
            let byte = reader.read_u8()?;
            match byte {
                b'0'..=b'9' => {
                    has_digits = true;
                    number = match number.checked_mul(10).and_then(|n| n.checked_add((byte - b'0') as u32)) {
                        Some(n) => n,
                        None => {
                            log_warn!("Number is too large: {} + {}", number, (byte - b'0') as u32);

                            number
                        }
                    };
                }
                _ => {
                    reader.seek(SeekFrom::Current(-1))?;
                    break;
                }
            }
        }

        if !has_digits {
            log_warn!("No digits found in decimal number");

            return Ok(0);
        }

        Ok(number)
    }

    fn read_pam_tuple(&mut self) -> VexelResult<(String, String)> {
        let mut key = String::new();
        let mut value = String::new();
        let mut reading_key = true;

        loop {
            let byte = self.reader.read_u8()?;

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
        }

        Ok((key.trim().to_string(), value.trim().to_string()))
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let magick = self.reader.read_bits(16)? as u16;

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
        Self::skip_whitespace_and_comments(&mut self.reader)?;

        self.width = Self::read_decimal(&mut self.reader)?;

        Self::skip_whitespace_and_comments(&mut self.reader)?;

        self.height = Self::read_decimal(&mut self.reader)?;

        match format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => {
                self.max_value = 1;
            }
            _ => {
                Self::skip_whitespace_and_comments(&mut self.reader)?;

                self.max_value = Self::read_decimal(&mut self.reader)?;

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

        self.reader.read_u8()?;

        Ok(())
    }

    fn read_pam_header(&mut self) -> VexelResult<()> {
        loop {
            Self::skip_whitespace_and_comments(&mut self.reader)?;

            let (key, value) = self.read_pam_tuple()?;

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
                    self.tuple_type = Some(match value.as_str() {
                        "BLACKANDWHITE" => TupleType::BlackAndWhite,
                        "GRAYSCALE" => TupleType::Grayscale,
                        "RGB" => TupleType::RGB,
                        "BLACKANDWHITE_ALPHA" => TupleType::BlackAndWhiteAlpha,
                        "GRAYSCALE_ALPHA" => TupleType::GrayscaleAlpha,
                        "RGB_ALPHA" => TupleType::RGBAlpha,
                        _ => {
                            log_warn!("Invalid TUPLTYPE value: {}", value);
                            TupleType::RGB
                        }
                    });
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

        Ok(())
    }

    fn read_data(&mut self) -> VexelResult<()> {
        self.data = self.reader.read_to_end()?;

        Ok(())
    }

    fn read_ascii_number<T: Read + Seek>(reader: &mut BitReader<T>) -> VexelResult<u32> {
        Self::skip_whitespace_and_comments(reader)?;

        Ok(Self::read_decimal(reader)?)
    }

    fn decode_ascii_bitmap(&mut self) -> VexelResult<PixelData> {
        let mut image_data: Vec<u8> = Vec::new();
        let mut reader = BitReader::new(Cursor::new(&self.data));

        for _ in 0..self.height {
            for _ in 0..self.width {
                let value = match Self::read_ascii_number(&mut reader) {
                    Ok(v) => v.clamp(0, self.max_value),
                    Err(e) => {
                        log_warn!("Error reading ASCII number: {:?}", e);
                        0
                    }
                };

                image_data.push(!(value as u8) & 1);
            }
        }

        Ok(PixelData::L1(image_data))
    }

    fn decode_ascii_graymap(&mut self) -> VexelResult<PixelData> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };
        let mut reader = BitReader::new(Cursor::new(&self.data));

        let pixel_count = (self.width * self.height) as usize;
        let values: Vec<_> = (0..pixel_count)
            .map(|_| {
                Self::read_ascii_number(&mut reader)
                    .map(|v| v.clamp(0, self.max_value))
                    .unwrap_or_else(|e| {
                        log_warn!("Error reading ASCII number: {:?}", e);
                        0
                    })
            })
            .collect();

        if bits_per_sample == 8 {
            Ok(PixelData::L8(
                values.iter().map(|&v| Self::scale_to_8bit(v, self.max_value)).collect(),
            ))
        } else {
            Ok(PixelData::L16(
                values
                    .iter()
                    .map(|&v| Self::scale_to_16bit(v, self.max_value))
                    .collect(),
            ))
        }
    }

    fn decode_ascii_pixmap(&mut self) -> VexelResult<PixelData> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };
        let mut reader = BitReader::new(Cursor::new(&self.data));

        let pixel_count = (self.width * self.height) as usize;
        let values: Vec<_> = (0..pixel_count * 3)
            .map(|_| {
                Self::read_ascii_number(&mut reader)
                    .map(|v| v.clamp(0, self.max_value))
                    .unwrap_or_else(|e| {
                        log_warn!("Error reading ASCII number: {:?}", e);
                        0
                    })
            })
            .collect();

        if bits_per_sample == 8 {
            Ok(PixelData::RGB8(
                values.iter().map(|&v| Self::scale_to_8bit(v, self.max_value)).collect(),
            ))
        } else {
            Ok(PixelData::RGB16(
                values
                    .iter()
                    .map(|&v| Self::scale_to_16bit(v, self.max_value))
                    .collect(),
            ))
        }
    }

    fn decode_binary_bitmap(&mut self) -> VexelResult<PixelData> {
        let mut image_data: Vec<u8> = Vec::new();

        let mut reader = BitReader::new(Cursor::new(&self.data));

        for _ in 0..self.height {
            for _ in 0..self.width {
                image_data.push(!reader.read_bit()? as u8);
            }
        }

        Ok(PixelData::L1(image_data))
    }

    fn decode_binary_graymap(&mut self) -> VexelResult<PixelData> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        let mut reader = BitReader::new(Cursor::new(&self.data));

        if bits_per_sample == 8 {
            let mut image_data = Vec::new();

            for _ in 0..self.height {
                for _ in 0..self.width {
                    let value = reader.read_u8()?.clamp(0, self.max_value as u8);
                    image_data.push(Self::scale_to_8bit(value as u32, self.max_value));
                }
            }

            return Ok(PixelData::L8(image_data));
        }

        let mut image_data = Vec::new();

        for _ in 0..self.height {
            for _ in 0..self.width {
                let value = reader.read_u16()?.clamp(0, self.max_value as u16);
                image_data.push(Self::scale_to_16bit(value as u32, self.max_value));
            }
        }

        Ok(PixelData::L16(image_data))
    }

    fn decode_binary_pixmap(&mut self) -> VexelResult<PixelData> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        let mut reader = BitReader::new(Cursor::new(&self.data));

        if bits_per_sample == 8 {
            let mut image_data = Vec::new();

            for _ in 0..self.height {
                for _ in 0..self.width {
                    let r = reader.read_u8().unwrap_or(0).clamp(0, self.max_value as u8);
                    let g = reader.read_u8().unwrap_or(0).clamp(0, self.max_value as u8);
                    let b = reader.read_u8().unwrap_or(0).clamp(0, self.max_value as u8);

                    image_data.push(Self::scale_to_8bit(r as u32, self.max_value));
                    image_data.push(Self::scale_to_8bit(g as u32, self.max_value));
                    image_data.push(Self::scale_to_8bit(b as u32, self.max_value));
                }
            }

            return Ok(PixelData::RGB8(image_data));
        }

        let mut image_data = Vec::new();

        for _ in 0..self.height {
            for _ in 0..self.width {
                let r = reader.read_u16()?.clamp(0, self.max_value as u16);
                let g = reader.read_u16()?.clamp(0, self.max_value as u16);
                let b = reader.read_u16()?.clamp(0, self.max_value as u16);

                image_data.push(Self::scale_to_16bit(r as u32, self.max_value));
                image_data.push(Self::scale_to_16bit(g as u32, self.max_value));
                image_data.push(Self::scale_to_16bit(b as u32, self.max_value));
            }
        }

        Ok(PixelData::RGB16(image_data))
    }

    fn decode_pam(&mut self) -> VexelResult<PixelData> {
        let mut reader = BitReader::new(Cursor::new(&self.data));

        if self.depth == 0 {
            self.depth = 3;
        }

        if self.max_value == 0 {
            self.max_value = 255;
        }

        if self.max_value > 65535 {
            self.max_value = 65535;
        }

        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        match (&self.tuple_type, self.depth) {
            // BLACKANDWHITE format (1 channel, maxval must be 1)
            (Some(TupleType::BlackAndWhite), 1) => {
                if self.max_value != 1 {
                    log_warn!(
                        "BLACKANDWHITE tuple type requires maxval of 1 for color channel, found: {}",
                        self.max_value
                    );
                    self.max_value = 1;
                }

                let mut image_data = Vec::with_capacity((self.width * self.height) as usize);
                for _ in 0..self.height {
                    for _ in 0..self.width {
                        let value = reader.read_u8()?;
                        image_data.push(value & 1);
                    }
                }
                Ok(PixelData::L1(image_data))
            }

            // GRAYSCALE format (1 channel)
            (Some(TupleType::Grayscale), 1) => {
                if bits_per_sample == 8 {
                    let mut image_data = Vec::with_capacity((self.width * self.height) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let value = reader.read_u8()?;
                            image_data.push(Self::scale_to_8bit(value as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::L8(image_data))
                } else {
                    let mut image_data = Vec::with_capacity((self.width * self.height) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let value = reader.read_u16()?;
                            image_data.push(Self::scale_to_16bit(value as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::L16(image_data))
                }
            }

            // RGB format (3 channels)
            (Some(TupleType::RGB), 3) => {
                if bits_per_sample == 8 {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let r = reader.read_u8()?;
                            let g = reader.read_u8()?;
                            let b = reader.read_u8()?;

                            image_data.push(Self::scale_to_8bit(r as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(g as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(b as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::RGB8(image_data))
                } else {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 3) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let r = reader.read_u16()?;
                            let g = reader.read_u16()?;
                            let b = reader.read_u16()?;

                            image_data.push(Self::scale_to_16bit(r as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(g as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(b as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::RGB16(image_data))
                }
            }

            // BLACKANDWHITE_ALPHA format (2 channels)
            (Some(TupleType::BlackAndWhiteAlpha), 2) => {
                if self.max_value != 1 {
                    log_warn!(
                        "BLACKANDWHITE_ALPHA tuple type requires maxval of 1 for color channel, found: {}",
                        self.max_value
                    );
                    self.max_value = 1;
                }

                let mut image_data = Vec::with_capacity((self.width * self.height * 2) as usize);
                for _ in 0..self.height {
                    for _ in 0..self.width {
                        let value = reader.read_u8()?;
                        let alpha = reader.read_u8()?;
                        image_data.push(value & 1);
                        image_data.push(Self::scale_to_8bit(alpha as u32, self.max_value));
                    }
                }

                Ok(PixelData::LA8(image_data))
            }

            // GRAYSCALE_ALPHA format (2 channels)
            (Some(TupleType::GrayscaleAlpha), 2) => {
                if bits_per_sample == 8 {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 2) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let gray = reader.read_u8()?;
                            let alpha = reader.read_u8()?;

                            image_data.push(Self::scale_to_8bit(gray as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(alpha as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::LA8(image_data))
                } else {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 2) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let gray = reader.read_u16()?;
                            let alpha = reader.read_u16()?;

                            image_data.push(Self::scale_to_16bit(gray as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(alpha as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::LA16(image_data))
                }
            }

            // RGB_ALPHA format (4 channels)
            (Some(TupleType::RGBAlpha), 4) => {
                if bits_per_sample == 8 {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 4) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let r = reader.read_u8()?;
                            let g = reader.read_u8()?;
                            let b = reader.read_u8()?;
                            let a = reader.read_u8()?;

                            image_data.push(Self::scale_to_8bit(r as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(g as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(b as u32, self.max_value));
                            image_data.push(Self::scale_to_8bit(a as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::RGBA8(image_data))
                } else {
                    let mut image_data = Vec::with_capacity((self.width * self.height * 4) as usize);
                    for _ in 0..self.height {
                        for _ in 0..self.width {
                            let r = reader.read_u16()?;
                            let g = reader.read_u16()?;
                            let b = reader.read_u16()?;
                            let a = reader.read_u16()?;

                            image_data.push(Self::scale_to_16bit(r as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(g as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(b as u32, self.max_value));
                            image_data.push(Self::scale_to_16bit(a as u32, self.max_value));
                        }
                    }

                    Ok(PixelData::RGBA16(image_data))
                }
            }

            _ => {
                // TODO
                log_error!(
                    "Invalid combination of tuple type and depth: {:?}, {}",
                    self.tuple_type,
                    self.depth
                );
                panic!("Invalid combination of tuple type and depth");
            }
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.read_header() {
            Ok(_) => {}
            Err(e) => {
                return Err(VexelError::Custom("Error reading header".to_string()));
            }
        };

        match self.read_data() {
            Ok(_) => {}
            Err(e) => {
                return Err(VexelError::Custom("Error reading data".to_string()));
            }
        };

        let mut result = match self.format {
            Some(NetpbmFormat::P1) => self.decode_ascii_bitmap(),
            Some(NetpbmFormat::P2) => self.decode_ascii_graymap(),
            Some(NetpbmFormat::P3) => self.decode_ascii_pixmap(),
            Some(NetpbmFormat::P4) => self.decode_binary_bitmap(),
            Some(NetpbmFormat::P5) => self.decode_binary_graymap(),
            Some(NetpbmFormat::P6) => self.decode_binary_pixmap(),
            Some(NetpbmFormat::P7) => self.decode_pam(),
            None => {
                log_warn!("Format not set before decoding, assuming binary pixmap (P6)");
                self.decode_binary_pixmap()
            }
        };

        let mut pixel_data = match result {
            Ok(data) => data,
            Err(e) => {
                return Err(VexelError::Custom("Error decoding image".to_string()));
            }
        };

        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
