use std::cmp::PartialEq;
use std::fmt::Debug;
use std::io::{Read, Seek};
use crate::bitreader::BitReader;
use crate::{Image, ImageFrame, PixelData};

#[derive(Debug, Clone, PartialEq)]
enum NetpbmFormat {
    P1, // ASCII bitmap
    P2, // ASCII graymap
    P3, // ASCII pixmap
    P4, // Binary bitmap
    P5, // Binary graymap
    P6, // Binary pixmap
    P7, // PAM
}

#[derive(Debug, Clone)]
enum TupleType {
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

impl<R: Read + Seek> Debug for NetPbmDecoder<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetpbmDecoder")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("max_value", &self.max_value)
            .field("depth", &self.depth)
            .field("format", &self.format)
            .field("tuple_type", &self.tuple_type)
            .finish()
    }
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

    fn scale_to_8bit(value: u32, max_value: u32) -> u8 {
        ((value as f32 * 255.0 / max_value as f32).round() as u32).min(255) as u8
    }

    fn scale_to_16bit(value: u32, max_value: u32) -> u16 {
        ((value as f32 * 65535.0 / max_value as f32).round() as u32).min(65535) as u16
    }

    fn skip_whitespace_and_comments(reader: &mut BitReader<R>) -> Result<(), std::io::Error> {
        loop {
            let byte = reader.read_u8()?;
            match byte {
                b'#' => {
                    // Skip until newline
                    loop {
                        let b = reader.read_u8()?;
                        if b == b'\n' {
                            break;
                        }
                    }
                }
                b' ' | b'\t' | b'\n' | b'\r' => continue,
                _ => {
                    // Put back the non-whitespace byte
                    reader.seek(std::io::SeekFrom::Current(-1))?;
                    break;
                }
            }
        }

        Ok(())
    }

    fn read_decimal(reader: &mut BitReader<R>) -> Result<u32, std::io::Error> {
        let mut number = 0u32;
        let mut has_digits = false;

        loop {
            let byte = reader.read_u8()?;
            match byte {
                b'0'..=b'9' => {
                    has_digits = true;
                    number = number
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((byte - b'0') as u32))
                        .ok_or_else(|| {
                            std::io::Error::new(std::io::ErrorKind::InvalidData, "Number too large")
                        })?;
                }
                _ => {
                    // Put back the non-digit byte
                    reader.seek(std::io::SeekFrom::Current(-1))?;
                    break;
                }
            }
        }

        if !has_digits {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected decimal number",
            ));
        }

        Ok(number)
    }

    fn read_pam_tuple(&mut self) -> Result<(String, String), std::io::Error> {
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

    fn read_header(&mut self) -> Result<(), std::io::Error> {
        // Read magic number
        let magick = self.reader.read_bits(16)? as u16;

        let format = match magick {
            0x5031 => NetpbmFormat::P1,
            0x5032 => NetpbmFormat::P2,
            0x5033 => NetpbmFormat::P3,
            0x5034 => NetpbmFormat::P4,
            0x5035 => NetpbmFormat::P5,
            0x5036 => NetpbmFormat::P6,
            0x5037 => NetpbmFormat::P7,
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid magic number")),
        };

        self.format = Some(format.clone());

        match format {
            NetpbmFormat::P7 => self.read_pam_header()?,
            _ => self.read_standard_header(format)?,
        }

        Ok(())
    }

    fn read_standard_header(&mut self, format: NetpbmFormat) -> Result<(), std::io::Error> {
        // Skip whitespace and comments after magic number
        Self::skip_whitespace_and_comments(&mut self.reader)?;

        // Read width
        self.width = Self::read_decimal(&mut self.reader)?;
        Self::skip_whitespace_and_comments(&mut self.reader)?;

        // Read height
        self.height = Self::read_decimal(&mut self.reader)?;

        // Read max value for formats that have it
        match format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => {
                self.max_value = 1;
            }
            _ => {
                Self::skip_whitespace_and_comments(&mut self.reader)?;
                self.max_value = Self::read_decimal(&mut self.reader)?;

                if self.max_value == 0 || self.max_value > 65535 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid max value",
                    ));
                }
            }
        }

        // Skip single whitespace character that must come before raster
        self.reader.read_u8()?;

        Ok(())
    }

    fn read_pam_header(&mut self) -> Result<(), std::io::Error> {
        loop {
            Self::skip_whitespace_and_comments(&mut self.reader)?;

            let (key, value) = self.read_pam_tuple()?;

            match key.as_str() {
                "ENDHDR" => break,
                "WIDTH" => self.width = value.parse().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid WIDTH value")
                })?,
                "HEIGHT" => self.height = value.parse().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid HEIGHT value")
                })?,
                "DEPTH" => self.depth = value.parse().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid DEPTH value")
                })?,
                "MAXVAL" => self.max_value = value.parse().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid MAXVAL value")
                })?,
                "TUPLTYPE" => {
                    self.tuple_type = Some(match value.as_str() {
                        "BLACKANDWHITE" => TupleType::BlackAndWhite,
                        "GRAYSCALE" => TupleType::Grayscale,
                        "RGB" => TupleType::RGB,
                        "BLACKANDWHITE_ALPHA" => TupleType::BlackAndWhiteAlpha,
                        "GRAYSCALE_ALPHA" => TupleType::GrayscaleAlpha,
                        "RGB_ALPHA" => TupleType::RGBAlpha,
                        _ => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid TUPLTYPE",
                            ))
                        }
                    });
                }
                _ => {} // Ignore unknown keys
            }
        }

        Ok(())
    }

    fn read_data(&mut self) -> Result<(), std::io::Error> {
        self.data = self.reader.read_to_end()?;

        Ok(())
    }

    fn read_ascii_number(&mut self) -> Result<u32, std::io::Error> {
        Self::skip_whitespace_and_comments(&mut self.reader)?;

        Ok(Self::read_decimal(&mut self.reader)?)
    }

    fn decode_ascii_bitmap(&mut self) -> Result<PixelData, std::io::Error> {
        let mut image_data: Vec<u8> = Vec::new();

        for _ in 0..self.height {
            for _ in 0..self.width {
                let value = self.read_ascii_number()?.clamp(0, self.max_value);

                image_data.push(value as u8);
            }
        }

        Ok(PixelData::L1(image_data))
    }

    fn decode_ascii_graymap(&mut self) -> Result<PixelData, std::io::Error> {
        let mut image_data: Vec<u8> = Vec::new();

        for _ in 0..self.height {
            for _ in 0..self.width {
                let value = self.read_ascii_number()?.clamp(0, self.max_value);

                image_data.push(Self::scale_to_8bit(value, self.max_value));
            }
        }

        Ok(PixelData::RGB8(image_data))
    }

    fn decode_ascii_pixmap(&mut self) -> Result<PixelData, std::io::Error> {
        let mut image_data: Vec<u8> = Vec::new();
        for _ in 0..self.height {
            for _ in 0..self.width {
                let r = self.read_ascii_number()?.clamp(0, self.max_value);
                let g = self.read_ascii_number()?.clamp(0, self.max_value);
                let b = self.read_ascii_number()?.clamp(0, self.max_value);

                image_data.push(Self::scale_to_8bit(r, self.max_value));
                image_data.push(Self::scale_to_8bit(g, self.max_value));
                image_data.push(Self::scale_to_8bit(b, self.max_value));
            }
        }

        Ok(PixelData::RGB8(image_data))
    }

    fn decode_binary_bitmap(&mut self) -> Result<PixelData, std::io::Error> {
        let mut image_data: Vec<u8> = Vec::new();

        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        for _ in 0..self.height {
            for _ in 0..self.width {
                image_data.push(!reader.read_bit()? as u8);
            }
        }

        Ok(PixelData::L1(image_data))
    }

    fn decode_binary_graymap(&mut self) -> Result<PixelData, std::io::Error> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

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

    fn decode_binary_pixmap(&mut self) -> Result<PixelData, std::io::Error> {
        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        if bits_per_sample == 8 {
            let mut image_data = Vec::new();

            for _ in 0..self.height {
                for _ in 0..self.width {
                    let r = reader.read_u8()?.clamp(0, self.max_value as u8);
                    let g = reader.read_u8()?.clamp(0, self.max_value as u8);
                    let b = reader.read_u8()?.clamp(0, self.max_value as u8);

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

    fn decode_pam(&self) -> Result<PixelData, std::io::Error> {
        let mut reader = BitReader::new(std::io::Cursor::new(&self.data));

        // Validate required fields
        if self.depth == 0 || self.max_value == 0 || self.max_value > 65535 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid PAM parameters",
            ));
        }

        let bits_per_sample = if self.max_value > 255 { 16 } else { 8 };

        match (&self.tuple_type, self.depth) {
            // BLACKANDWHITE format (1 channel, maxval must be 1)
            (Some(TupleType::BlackAndWhite), 1) => {
                if self.max_value != 1 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "BLACKANDWHITE tuple type requires maxval of 1",
                    ));
                }

                let mut image_data = Vec::with_capacity((self.width * self.height) as usize);
                for _ in 0..self.height {
                    for _ in 0..self.width {
                        let value = reader.read_u8()?;
                        image_data.push(value & 1); // Ensure value is 0 or 1
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
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "BLACKANDWHITE_ALPHA tuple type requires maxval of 1 for color channel",
                    ));
                }

                let mut image_data = Vec::with_capacity((self.width * self.height * 2) as usize);
                for _ in 0..self.height {
                    for _ in 0..self.width {
                        let value = reader.read_u8()?;
                        let alpha = reader.read_u8()?;
                        image_data.push(value & 1); // Ensure value is 0 or 1
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

            // Invalid combination of tuple type and depth
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid combination of tuple type and depth",
            )),
        }
    }

    pub fn decode(&mut self) -> Result<Image, std::io::Error> {
        self.read_header()?;
        self.read_data()?;

        let image_data = match self.format {
            Some(NetpbmFormat::P1) => self.decode_ascii_bitmap()?,
            Some(NetpbmFormat::P2) => self.decode_ascii_graymap()?,
            Some(NetpbmFormat::P3) => self.decode_ascii_pixmap()?,
            Some(NetpbmFormat::P4) => self.decode_binary_bitmap()?,
            Some(NetpbmFormat::P5) => self.decode_binary_graymap()?,
            Some(NetpbmFormat::P6) => self.decode_binary_pixmap()?,
            Some(NetpbmFormat::P7) => self.decode_pam()?,
            None => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Format not set")),
        };

        Ok(Image::from_frame(ImageFrame::new(
            self.width,
            self.height,
            image_data,
            0,
        )))
    }
}
