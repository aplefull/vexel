use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom};
use flate2::read::ZlibDecoder;
use crate::bitreader::BitReader;
use crate::{log_warn, Image, ImageFrame, PixelData, PixelFormat};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::PngInfo;
use crate::utils::traits::SafeAccess;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PngChunk {
    // Critical chunks
    IHDR,  // Image header
    PLTE,  // Palette
    IDAT,  // Image data
    IEND,  // End of image

    // Ancillary chunks
    TRNS,  // Transparency
    CHRM,  // Chromaticity
    GAMA,  // Gamma
    ICCP,  // ICC Profile
    SBIT,  // Significant bits
    SRGB,  // Standard RGB
    TEXT,  // Text
    ZTXT,  // Compressed text
    ITXT,  // International text
    BKGD,  // Background color
    PHYS,  // Physical dimensions
    TIME,  // Last modification time
    SPLT,  // Suggested palette
    HIST,  // Palette histogram

    // Animation chunks
    ACTL,  // Animation control
    FCTL,  // Frame control
    FDAT,  // Frame data
}

fn get_chunk(chunk_type: &[u8; 4]) -> Option<PngChunk> {
    let chunk = match chunk_type {
        b"IHDR" => Some(PngChunk::IHDR),
        b"PLTE" => Some(PngChunk::PLTE),
        b"IDAT" => Some(PngChunk::IDAT),
        b"IEND" => Some(PngChunk::IEND),
        b"tRNS" => Some(PngChunk::TRNS),
        b"cHRM" => Some(PngChunk::CHRM),
        b"gAMA" => Some(PngChunk::GAMA),
        b"iCCP" => Some(PngChunk::ICCP),
        b"sBIT" => Some(PngChunk::SBIT),
        b"sRGB" => Some(PngChunk::SRGB),
        b"tEXt" => Some(PngChunk::TEXT),
        b"zTXt" => Some(PngChunk::ZTXT),
        b"iTXt" => Some(PngChunk::ITXT),
        b"bKGD" => Some(PngChunk::BKGD),
        b"pHYs" => Some(PngChunk::PHYS),
        b"tIME" => Some(PngChunk::TIME),
        b"sPLT" => Some(PngChunk::SPLT),
        b"hIST" => Some(PngChunk::HIST),
        b"acTL" => Some(PngChunk::ACTL),
        b"fcTL" => Some(PngChunk::FCTL),
        b"fdAT" => Some(PngChunk::FDAT),
        _ => None,
    };

    chunk
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorType {
    Grayscale = 0,
    RGB = 2,
    Indexed = 3,
    GrayscaleAlpha = 4,
    RGBA = 6,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompressionMethod {
    Deflate = 0,
    None = 1,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

#[derive(Debug, Clone)]
pub enum TransparencyData {
    Grayscale(u16),
    RGB(u16, u16, u16),
    Palette(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum BackgroundData {
    Grayscale(u16),
    RGB(u16, u16, u16),
    PaletteIndex(u8),
}

#[derive(Debug, Clone, Copy)]
pub enum RenderingIntent {
    Perceptual = 0,
    RelativeColorimetric = 1,
    Saturation = 2,
    AbsoluteColorimetric = 3,
}

#[derive(Debug, Clone, Copy)]
pub struct Chromaticities {
    pub white_point_x: f32,
    pub white_point_y: f32,
    pub red_x: f32,
    pub red_y: f32,
    pub green_x: f32,
    pub green_y: f32,
    pub blue_x: f32,
    pub blue_y: f32,
}

#[derive(Debug, Clone)]
pub struct ActlChunk {
    pub num_frames: u32,
    pub num_plays: u32,
}

#[derive(Debug, Clone)]
pub struct FctlChunk {
    pub sequence_number: u32,
    pub width: u32,
    pub height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub delay_num: u16,
    pub delay_den: u16,
    pub dispose_op: u8,
    pub blend_op: u8,
}

#[derive(Debug, Clone)]
pub struct PngFrame {
    pub fctl_info: FctlChunk,
    pub fdat: Vec<u8>,
}

struct CrcCalculator {
    table: [u32; 256],
}

impl CrcCalculator {
    fn new() -> Self {
        let mut table = [0u32; 256];
        for n in 0..256 {
            let mut c = n as u32;
            for _ in 0..8 {
                if c & 1 == 1 {
                    c = 0xedb88320u32 ^ (c >> 1);
                } else {
                    c = c >> 1;
                }
            }
            table[n] = c;
        }
        Self { table }
    }

    fn update_crc(&self, crc: u32, buf: &[u8]) -> u32 {
        let mut c = crc;
        for &b in buf {
            c = self.table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
        }
        c
    }

    fn calculate_crc(&self, data: &[u8]) -> u32 {
        self.update_crc(0xffffffff, data) ^ 0xffffffff
    }
}

#[derive(Debug, Clone)]
pub enum PngText {
    Basic {
        keyword: String,
        text: String,
    },
    Compressed {
        keyword: String,
        text: String,
    },
    International {
        keyword: String,
        language_tag: String,
        translated_keyword: String,
        text: String,
    },
}

#[derive(Debug, Clone)]
pub struct SuggestedPaletteSample {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub alpha: u16,
    pub frequency: u16,
}

#[derive(Debug, Clone)]
pub struct SuggestedPalette {
    pub name: String,
    pub sample_depth: u8,
    pub samples: Vec<SuggestedPaletteSample>,
}

#[derive(Debug, Clone)]
pub struct PhysicalDimensions {
    pub pixels_per_unit_x: u32,
    pub pixels_per_unit_y: u32,
    pub unit: PhysicalUnit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhysicalUnit {
    Unknown,
    Meter,
}

#[derive(Debug, Clone)]
pub enum SignificantBits {
    Grayscale { gray: u8 },
    RGB { red: u8, green: u8, blue: u8 },
    Indexed { red: u8, green: u8, blue: u8 },
    GrayscaleAlpha { gray: u8, alpha: u8 },
    RGBA { red: u8, green: u8, blue: u8, alpha: u8 },
}

#[derive(Debug, Clone)]
pub struct ImageTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub struct PngDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: ColorType,
    compression_method: CompressionMethod,
    has_filters: bool,
    interlace: bool,
    palette: Option<Vec<[u8; 3]>>,
    idat_data: Vec<u8>,
    gamma: Option<f32>,
    transparency: Option<TransparencyData>,
    background: Option<BackgroundData>,
    rendering_intent: Option<RenderingIntent>,
    chromaticities: Option<Chromaticities>,
    suggested_palettes: Vec<SuggestedPalette>,
    physical_dimensions: Option<PhysicalDimensions>,
    significant_bits: Option<SignificantBits>,
    histogram: Option<Vec<u16>>,
    modification_time: Option<ImageTime>,
    text_chunks: Vec<PngText>,
    frames: Vec<PngFrame>,
    actl_info: Option<ActlChunk>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> PngDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            bit_depth: 0,
            color_type: ColorType::RGB,
            compression_method: CompressionMethod::None,
            has_filters: true,
            interlace: false,
            palette: None,
            idat_data: Vec::new(),
            gamma: None,
            transparency: None,
            background: None,
            rendering_intent: None,
            chromaticities: None,
            suggested_palettes: Vec::new(),
            physical_dimensions: None,
            significant_bits: None,
            histogram: None,
            modification_time: None,
            text_chunks: Vec::new(),
            frames: Vec::new(),
            actl_info: None,
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> PngInfo {
        PngInfo {
            width: self.width,
            height: self.height,
            bit_depth: self.bit_depth,
            color_type: self.color_type,
            compression_method: self.compression_method,
            has_filters: self.has_filters,
            interlace: self.interlace,
            palette: self.palette.clone(),
            gamma: self.gamma,
            transparency: self.transparency.clone(),
            background: self.background.clone(),
            rendering_intent: self.rendering_intent,
            chromaticities: self.chromaticities,
            suggested_palettes: self.suggested_palettes.clone(),
            physical_dimensions: self.physical_dimensions.clone(),
            significant_bits: self.significant_bits.clone(),
            histogram: self.histogram.clone(),
            modification_time: self.modification_time.clone(),
            text_chunks: self.text_chunks.clone(),
            frames: self.frames.clone(),
            actl_info: self.actl_info.clone(),
        }
    }

    fn read_ihdr(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let width = self.reader.read_u32()?;
        let height = self.reader.read_u32()?;
        let bit_depth = self.reader.read_u8()?;
        let color_type = self.reader.read_u8()?;
        let compression_method = self.reader.read_u8()?;
        let filter_method = self.reader.read_u8()?;
        let interlace_method = self.reader.read_u8()?;

        self.width = width;
        self.height = height;

        self.bit_depth = match bit_depth {
            1 | 2 | 4 | 8 | 16 => bit_depth,
            _ => {
                log_warn!("Invalid bit depth: {}", bit_depth);
                8
            }
        };

        self.color_type = match color_type {
            0 => ColorType::Grayscale,
            2 => ColorType::RGB,
            3 => ColorType::Indexed,
            4 => ColorType::GrayscaleAlpha,
            6 => ColorType::RGBA,
            _ => {
                log_warn!("Invalid color type: {}", color_type);
                ColorType::RGB
            }
        };

        self.compression_method = match compression_method {
            0 => CompressionMethod::Deflate,
            1 => CompressionMethod::None,
            _ => {
                log_warn!("Invalid compression method: {}", compression_method);
                CompressionMethod::None
            }
        };

        self.has_filters = match filter_method {
            0 => true,
            1 => false,
            _ => {
                log_warn!("Invalid filter method: {}", filter_method);
                true
            }
        };

        self.interlace = match interlace_method {
            0 => false,
            1 => true,
            _ => {
                log_warn!("Invalid interlace method: {}", interlace_method);
                false
            }
        };

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        Ok(())
    }

    fn read_plte(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;

        if length % 3 != 0 {
            log_warn!("PLTE chunk length is not a multiple of 3");
        }

        let entries = length / 3;
        let mut palette = Vec::with_capacity(entries as usize);

        for _ in 0..entries {
            let r = self.reader.read_u8()?;
            let g = self.reader.read_u8()?;
            let b = self.reader.read_u8()?;

            palette.push([r, g, b]);
        }

        self.palette = Some(palette);

        Ok(())
    }

    fn read_idat(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;
        let mut chunk_data = vec![0; length as usize];

        self.reader.read_exact(&mut chunk_data)?;

        // If we have at least one fcTL chunk, it's APNG
        // and frame needs data from IDAT chunks
        if !self.frames.is_empty() {
            let fctl_info = self.frames.last_mut().unwrap();
            fctl_info.fdat.extend(chunk_data.clone());
        }

        self.idat_data.extend(chunk_data);

        Ok(())
    }

    fn read_splt(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;

        let mut name = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            if byte == 0 {
                break;
            }

            if !((byte >= 32 && byte <= 126) || byte >= 161) {
                log_warn!("Invalid character in sPLT name: {}, replacing with space", byte);
                name.push(32);
            }

            name.push(byte);

            if name.len() >= 79 {
                log_warn!("sPLT name too long");
                break;
            }
        }

        let mut sample_depth = self.reader.read_u8()?;

        if sample_depth != 8 && sample_depth != 16 {
            log_warn!("Invalid sPLT sample depth: {}, assuming 8", sample_depth);
            sample_depth = 8;
        }

        let entry_size = if sample_depth == 8 { 6 } else { 10 };
        let remaining_bytes = length - (name.len() as u32 + 2);

        if remaining_bytes % entry_size as u32 != 0 {
            log_warn!("Invalid sPLT chunk length");
        }

        let num_entries = remaining_bytes / entry_size as u32;

        let mut entries = Vec::new();
        for _ in 0..num_entries {
            let (red, green, blue, alpha) = if sample_depth == 8 {
                (
                    self.reader.read_u8()? as u16,
                    self.reader.read_u8()? as u16,
                    self.reader.read_u8()? as u16,
                    self.reader.read_u8()? as u16
                )
            } else {
                (
                    self.reader.read_u16()?,
                    self.reader.read_u16()?,
                    self.reader.read_u16()?,
                    self.reader.read_u16()?
                )
            };
            let frequency = self.reader.read_u16()?;

            entries.push(SuggestedPaletteSample {
                red,
                green,
                blue,
                alpha,
                frequency,
            });
        }

        let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
        self.suggested_palettes.push(SuggestedPalette {
            name: name_str,
            sample_depth,
            samples: entries,
        });

        Ok(())
    }

    fn read_srgb(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let intent = match self.reader.read_u8()? {
            0 => RenderingIntent::Perceptual,
            1 => RenderingIntent::RelativeColorimetric,
            2 => RenderingIntent::Saturation,
            3 => RenderingIntent::AbsoluteColorimetric,
            n => {
                log_warn!("Invalid sRGB rendering intent: {}", n);
                RenderingIntent::Perceptual
            }
        };

        self.rendering_intent = Some(intent);

        Ok(())
    }

    fn read_gama(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let gamma_int = self.reader.read_u32()?;
        let gamma = gamma_int as f32 / 100000.0;

        self.gamma = Some(gamma);

        Ok(())
    }

    fn read_chrm(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let white_x = self.reader.read_u32()?;
        let white_y = self.reader.read_u32()?;
        let red_x = self.reader.read_u32()?;
        let red_y = self.reader.read_u32()?;
        let green_x = self.reader.read_u32()?;
        let green_y = self.reader.read_u32()?;
        let blue_x = self.reader.read_u32()?;
        let blue_y = self.reader.read_u32()?;

        let chromaticities = Chromaticities {
            white_point_x: white_x as f32 / 100000.0,
            white_point_y: white_y as f32 / 100000.0,
            red_x: red_x as f32 / 100000.0,
            red_y: red_y as f32 / 100000.0,
            green_x: green_x as f32 / 100000.0,
            green_y: green_y as f32 / 100000.0,
            blue_x: blue_x as f32 / 100000.0,
            blue_y: blue_y as f32 / 100000.0,
        };

        self.chromaticities = Some(chromaticities);

        Ok(())
    }

    fn read_trns(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;

        let trns_data = match self.color_type {
            ColorType::Grayscale => {
                if length != 2 {
                    log_warn!("Invalid tRNS length for grayscale");
                }

                let value = self.reader.read_u16()?;

                TransparencyData::Grayscale(value)
            }
            ColorType::RGB => {
                if length != 6 {
                    log_warn!("Invalid tRNS length for RGB");
                }

                let r = self.reader.read_u16()?;
                let g = self.reader.read_u16()?;
                let b = self.reader.read_u16()?;

                TransparencyData::RGB(r, g, b)
            }
            ColorType::Indexed => {
                if self.palette.is_none() {
                    log_warn!("tRNS chunk before PLTE chunk");
                }

                let mut value = vec![0; length as usize];
                self.reader.read_exact(&mut value)?;

                TransparencyData::Palette(value)
            }
            _ => {
                log_warn!("tRNS chunk not allowed for color type {:?}", self.color_type);
                return Ok(());
            }
        };

        self.transparency = Some(trns_data);

        Ok(())
    }

    fn read_bkgd(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;

        let background = match self.color_type {
            ColorType::Grayscale | ColorType::GrayscaleAlpha => {
                if length != 2 {
                    log_warn!("Invalid bKGD length for grayscale");
                }

                let value = self.reader.read_u16()?;

                BackgroundData::Grayscale(value)
            }
            ColorType::RGB | ColorType::RGBA => {
                if length != 6 {
                    log_warn!("Invalid bKGD length for RGB");
                }

                let r = self.reader.read_u16()?;
                let g = self.reader.read_u16()?;
                let b = self.reader.read_u16()?;

                BackgroundData::RGB(r, g, b)
            }
            ColorType::Indexed => {
                if length != 1 {
                    log_warn!("Invalid bKGD length for indexed color");
                }

                if self.palette.is_none() {
                    log_warn!("bKGD chunk before PLTE chunk");
                }

                let value = self.reader.read_u8()?;

                BackgroundData::PaletteIndex(value)
            }
        };

        self.background = Some(background);

        Ok(())
    }

    fn read_phys(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let pixels_per_unit_x = self.reader.read_u32()?;
        let pixels_per_unit_y = self.reader.read_u32()?;
        let unit_specifier = self.reader.read_u8()?;

        let unit = match unit_specifier {
            0 => PhysicalUnit::Unknown,
            1 => PhysicalUnit::Meter,
            _ => {
                PhysicalUnit::Unknown
            }
        };

        self.physical_dimensions = Some(PhysicalDimensions {
            pixels_per_unit_x,
            pixels_per_unit_y,
            unit,
        });

        Ok(())
    }

    fn read_sbit(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;

        let mut chunk_data = vec![0; length as usize];
        self.reader.read_exact(&mut chunk_data)?;

        let sbit_data = match self.color_type {
            ColorType::Grayscale => {
                if length != 1 {
                    log_warn!("Invalid sBIT length for grayscale: {}", length);
                }

                SignificantBits::Grayscale { gray: self.reader.read_u8()? }
            }
            ColorType::RGB => {
                if length != 3 {
                    log_warn!("Invalid sBIT length for RGB: {}", length);
                }

                SignificantBits::RGB {
                    red: self.reader.read_u8()?,
                    green: self.reader.read_u8()?,
                    blue: self.reader.read_u8()?,
                }
            }
            ColorType::Indexed => {
                if length != 3 {
                    log_warn!("Invalid sBIT length for indexed color: {}", length);
                }

                SignificantBits::Indexed {
                    red: self.reader.read_u8()?,
                    green: self.reader.read_u8()?,
                    blue: self.reader.read_u8()?,
                }
            }
            ColorType::GrayscaleAlpha => {
                if length != 2 {
                    log_warn!("Invalid sBIT length for grayscale alpha: {}", length);
                }

                SignificantBits::GrayscaleAlpha {
                    gray: self.reader.read_u8()?,
                    alpha: self.reader.read_u8()?,
                }
            }
            ColorType::RGBA => {
                if length != 4 {
                    log_warn!("Invalid sBIT length for RGBA: {}", length);
                }

                SignificantBits::RGBA {
                    red: self.reader.read_u8()?,
                    green: self.reader.read_u8()?,
                    blue: self.reader.read_u8()?,
                    alpha: self.reader.read_u8()?,
                }
            }
        };

        self.significant_bits = Some(sbit_data);

        Ok(())
    }

    fn read_hist(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        if self.palette.is_none() {
            log_warn!("Encountered hIST chunk before PLTE chunk");
            return Ok(());
        }

        let palette_len = self.palette.as_ref().unwrap().len();
        let length = self.get_chunk_length()?;

        if length as usize != palette_len * 2 {
            log_warn!("Invalid hIST length: {}, expected {}", length, palette_len * 2);
        }

        let mut frequencies = Vec::new();

        for _ in 0..palette_len {
            frequencies.push(self.reader.read_u16()?);
        }

        self.histogram = Some(frequencies);

        Ok(())
    }

    fn read_time(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let year = self.reader.read_u16()?;
        let month = self.reader.read_u8()?;
        let day = self.reader.read_u8()?;
        let hour = self.reader.read_u8()?;
        let minute = self.reader.read_u8()?;
        let second = self.reader.read_u8()?;

        if month < 1 || month > 12 {
            log_warn!("Invalid month in tIME chunk: {}", month);
        }

        if day < 1 || day > 31 {
            log_warn!("Invalid day in tIME chunk: {}", day);
        }

        if hour > 23 {
            log_warn!("Invalid hour in tIME chunk: {}", hour);
        }

        if minute > 59 {
            log_warn!("Invalid minute in tIME chunk: {}", minute);
        }

        if second > 60 {
            log_warn!("Invalid second in tIME chunk: {}", second);
        }

        self.modification_time = Some(ImageTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        });

        Ok(())
    }

    fn read_text(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;
        let mut num_read = 0;

        let mut keyword_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            keyword_bytes.push(byte);
        }

        let keyword = String::from_utf8_lossy(&keyword_bytes).to_string();

        let mut text_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if num_read >= length {
                break;
            }

            text_bytes.push(byte);
        }

        let text = String::from_utf8_lossy(&text_bytes).to_string();

        self.text_chunks.push(PngText::Basic { keyword, text });

        Ok(())
    }

    fn read_ztxt(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;
        let mut num_read = 0;

        let mut keyword_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            keyword_bytes.push(byte);
        }

        let keyword = String::from_utf8_lossy(&keyword_bytes).to_string();

        let compression_method = self.reader.read_u8()?;

        if compression_method != 0 {
            log_warn!("Unknown compression method in zTXt chunk: {}", compression_method);
            return Ok(());
        }

        let mut compressed_text = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if num_read >= length {
                break;
            }

            compressed_text.push(byte);
        }

        let mut decoder = ZlibDecoder::new(&compressed_text[..]);
        let mut text_bytes = Vec::new();
        decoder.read_to_end(&mut text_bytes)?;

        let text = String::from_utf8_lossy(&text_bytes).to_string();

        self.text_chunks.push(PngText::Compressed { keyword, text });

        Ok(())
    }

    fn read_itxt(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()?;
        let mut num_read = 0;

        let mut keyword_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            keyword_bytes.push(byte);
        }

        let keyword = String::from_utf8_lossy(&keyword_bytes).to_string();

        let compression_flag = self.reader.read_u8()?;
        let compression_method = self.reader.read_u8()?;

        let mut lang_tag_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            lang_tag_bytes.push(byte);
        }

        let language_tag = String::from_utf8_lossy(&lang_tag_bytes).to_string();

        let mut trans_keyword_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            trans_keyword_bytes.push(byte);
        }

        let translated_keyword = String::from_utf8_lossy(&trans_keyword_bytes).to_string();

        let mut text_bytes = Vec::new();
        loop {
            let byte = self.reader.read_u8()?;
            num_read += 1;

            if num_read >= length {
                break;
            }

            text_bytes.push(byte);
        }

        let text = if compression_flag == 1 {
            if compression_method != 0 {
                log_warn!("Invalid compression method in iTXt chunk: {}", compression_method);
            }

            let mut decoder = ZlibDecoder::new(&text_bytes[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;

            String::from_utf8_lossy(&decompressed).to_string()
        } else {
            String::from_utf8_lossy(&text_bytes).to_string()
        };

        self.text_chunks.push(PngText::International {
            keyword,
            language_tag,
            translated_keyword,
            text,
        });

        Ok(())
    }

    fn read_actl(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let num_frames = self.reader.read_u32()?;
        let num_plays = self.reader.read_u32()?;

        if num_frames == 0 {
            log_warn!("acTL chunk with zero frames");
        }

        self.actl_info = Some(ActlChunk {
            num_frames,
            num_plays,
        });

        Ok(())
    }

    fn read_fctl(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let sequence_number = self.reader.read_u32()?;
        let mut width = self.reader.read_u32()?;
        let mut height = self.reader.read_u32()?;
        let x_offset = self.reader.read_u32()?;
        let y_offset = self.reader.read_u32()?;
        let delay_num = self.reader.read_u16()?;
        let delay_den = self.reader.read_u16()?;
        let mut dispose_op = self.reader.read_u8()?;
        let mut blend_op = self.reader.read_u8()?;

        if x_offset + width > self.width {
            log_warn!(format!("fcTL width would overflow actual image width, clamping: x_offset={}, width={}, image_width={}", x_offset, width, self.width));
            width = self.width.saturating_sub(x_offset);
        }

        if y_offset + height > self.height {
            log_warn!(format!("fcTL height would overflow actual image height, clamping: y_offset={}, height={}, image_height={}", y_offset, height, self.height));
            height = self.height.saturating_sub(y_offset);
        }

        if width == 0 || height == 0 {
            log_warn!(format!("Invalid fcTL parameters: width={}, height={}", width, height));

            return Ok(());
        }

        if dispose_op > 2 {
            log_warn!(format!("Invalid fcTL dispose_op: {}", dispose_op));
            dispose_op = 0;
        }

        if blend_op > 1 {
            log_warn!(format!("Invalid fcTL blend_op: {}", blend_op));
            blend_op = 0;
        }

        let fctl_info = FctlChunk {
            sequence_number,
            width,
            height,
            x_offset,
            y_offset,
            delay_num,
            delay_den,
            dispose_op,
            blend_op,
        };

        self.frames.push(PngFrame {
            fctl_info,
            fdat: Vec::new(),
        });

        Ok(())
    }

    fn read_fdat(&mut self) -> VexelResult<()> {
        self.validate_chunk_crc()?;

        let length = self.get_chunk_length()? - 4;

        self.reader.read_u32()?;

        if self.frames.is_empty() {
            log_warn!("fdAT chunk without preceding fcTL chunk");
            return Ok(());
        }

        let mut frame_data = vec![0; length as usize];
        self.reader.read_exact(&mut frame_data)?;

        if let Some(frame) = self.frames.last_mut() {
            frame.fdat.extend(frame_data);
        }

        Ok(())
    }

    fn get_chunk_length(&mut self) -> VexelResult<u32> {
        self.reader.seek(SeekFrom::Current(-8))?;
        let length = self.reader.read_u32()?;
        self.reader.seek(SeekFrom::Current(4))?;

        Ok(length)
    }

    fn validate_chunk_crc(&mut self) -> VexelResult<()> {
        let length = self.get_chunk_length()? as usize;

        self.reader.seek(SeekFrom::Current(-4))?;

        let mut chunk_type = vec![0; 4];
        self.reader.read_exact(&mut chunk_type)?;

        let mut chunk_data = vec![0; length];
        self.reader.read_exact(&mut chunk_data)?;

        let expected_crc = self.reader.read_u32()?;

        self.reader.seek(SeekFrom::Current(-(length as i64) - 4))?;

        let mut crc_data = Vec::with_capacity(4 + length);
        crc_data.extend_from_slice(chunk_type.as_slice());
        crc_data.extend_from_slice(chunk_data.as_slice());

        let calculator = CrcCalculator::new();
        let calculated_crc = calculator.calculate_crc(&crc_data);

        if calculated_crc != expected_crc {
            log_warn!(
                "CRC mismatch for chunk {:?}: expected 0x{:08x}, calculated 0x{:08x}",
                String::from_utf8_lossy(chunk_type.as_slice()),
                expected_crc,
                calculated_crc
            );
        }

        Ok(())
    }

    fn get_bits_per_pixel(&self) -> u32 {
        match self.color_type {
            ColorType::Grayscale => self.bit_depth as u32,
            ColorType::RGB => self.bit_depth as u32 * 3,
            ColorType::Indexed => self.bit_depth as u32,
            ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
            ColorType::RGBA => self.bit_depth as u32 * 4,
        }
    }

    fn decode_sub_filter(&self, src: &[u8], dst: &mut [u8], bytes_per_pixel: usize) {
        if dst.check_range(0..bytes_per_pixel).is_err() || src.check_range(0..bytes_per_pixel).is_err() {
            log_warn!("Invalid range for sub filter: {}", bytes_per_pixel);
            return;
        }

        dst[..bytes_per_pixel].copy_from_slice(&src[..bytes_per_pixel]);

        for i in bytes_per_pixel..src.len() {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || dst.get_safe(i - bytes_per_pixel).is_err() {
                log_warn!("Invalid range for sub filter: {}", i);
                break;
            }

            dst[i] = src[i].wrapping_add(dst[i - bytes_per_pixel]);
        }
    }

    fn decode_up_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8]) {
        for i in 0..src.len() {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || prior.get_safe(i).is_err() {
                log_warn!("Invalid range for up filter: {}", i);
                break;
            }

            dst[i] = src[i].wrapping_add(prior[i]);
        }
    }

    fn decode_average_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
        for i in 0..bytes_per_pixel {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || prior.get_safe(i).is_err() {
                log_warn!("Invalid range for average filter: {}", i);
                break;
            }

            dst[i] = src[i].wrapping_add(prior[i] >> 1);
        }

        for i in bytes_per_pixel..src.len() {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || prior.get_safe(i).is_err() || dst.get_safe(i - bytes_per_pixel).is_err() {
                log_warn!("Invalid range for average filter: {}", i);
                break;
            }

            let left = dst[i - bytes_per_pixel] as u16;
            let above = prior[i] as u16;
            let avg = ((left + above) >> 1) as u8;
            dst[i] = src[i].wrapping_add(avg);
        }
    }

    fn decode_paeth_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
        for i in 0..bytes_per_pixel {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || prior.get_safe(i).is_err() {
                log_warn!("Invalid range for paeth filter: {}", i);
                break;
            }

            dst[i] = src[i].wrapping_add(prior[i]);
        }

        for i in bytes_per_pixel..src.len() {
            if dst.get_safe(i).is_err() || src.get_safe(i).is_err() || prior.get_safe(i).is_err() || dst.get_safe(i - bytes_per_pixel).is_err() {
                log_warn!("Invalid range for paeth filter: {}", i);
                break;
            }

            let left = dst[i - bytes_per_pixel];
            let above = prior[i];
            let upper_left = prior[i - bytes_per_pixel];

            dst[i] = src[i].wrapping_add(self.paeth_predictor(left, above, upper_left));
        }
    }

    fn paeth_predictor(&self, a: u8, b: u8, c: u8) -> u8 {
        // a = left, b = above, c = upper left
        let a = a as i16;
        let b = b as i16;
        let c = c as i16;

        let p = a + b - c;        // Initial estimate
        let pa = (p - a).abs();   // Distance to a
        let pb = (p - b).abs();   // Distance to b
        let pc = (p - c).abs();   // Distance to c

        if pa <= pb && pa <= pc {
            a as u8
        } else if pb <= pc {
            b as u8
        } else {
            c as u8
        }
    }

    fn unfilter_scanlines(&self, data: &[u8], pass_width: u32) -> VexelResult<Vec<u8>> {
        let bits_per_pixel = self.get_bits_per_pixel();

        let bytes_per_pixel = (bits_per_pixel as usize + 7) / 8;
        let bytes_per_row = (pass_width as usize * bits_per_pixel as usize + 7) / 8;

        let scanline_bytes = 1 + bytes_per_row;
        let mut unfiltered = Vec::new();
        let mut prior_scanline = vec![0u8; scanline_bytes - 1];

        for (_, scanline) in data.chunks(scanline_bytes).enumerate() {
            if scanline.len() < scanline_bytes {
                log_warn!("Invalid scanline length: {}, expected: {}", scanline.len(), scanline_bytes);
                break;
            }

            let filter_type = match scanline[0] {
                0 => FilterType::None,
                1 => FilterType::Sub,
                2 => FilterType::Up,
                3 => FilterType::Average,
                4 => FilterType::Paeth,
                _ => {
                    log_warn!("Invalid filter type: {}", scanline[0]);
                    FilterType::None
                }
            };

            if scanline.len() < 1 {
                log_warn!("Invalid scanline length: {}", scanline.len());
                continue;
            }

            let filtered = &scanline[1..];
            let mut decoded = vec![0u8; filtered.len()];

            match filter_type {
                FilterType::None => {
                    if decoded.len() != filtered.len() {
                        log_warn!("Length mismatch for unfiltered scanline: {} != {}", decoded.len(), filtered.len());
                        continue;
                    }

                    decoded.copy_from_slice(filtered);
                }
                FilterType::Sub => {
                    self.decode_sub_filter(filtered, &mut decoded, bytes_per_pixel);
                }
                FilterType::Up => {
                    self.decode_up_filter(filtered, &mut decoded, &prior_scanline);
                }
                FilterType::Average => {
                    self.decode_average_filter(filtered, &mut decoded, &prior_scanline, bytes_per_pixel);
                }
                FilterType::Paeth => {
                    self.decode_paeth_filter(filtered, &mut decoded, &prior_scanline, bytes_per_pixel);
                }
            }

            prior_scanline.copy_from_slice(&decoded);
            unfiltered.extend_from_slice(&decoded);
        }

        Ok(unfiltered)
    }

    fn decode_indexed(&self, input: &[u8]) -> VexelResult<PixelData> {
        let palette = match &self.palette {
            Some(palette) => palette,
            None => {
                log_warn!("No palette found for indexed color");
                return Ok(PixelData::RGB8(Vec::new()));
            }
        };

        let trans = if let Some(TransparencyData::Palette(alpha)) = &self.transparency {
            Some(alpha)
        } else {
            None
        };

        let mut output = Vec::new();
        let has_trans = trans.is_some();

        match self.bit_depth {
            8 => {
                for &index in input {
                    let color = palette.get(index as usize).unwrap_or(&[0, 0, 0]);
                    if has_trans {
                        let alpha = trans.as_ref().unwrap().get(index as usize).unwrap_or(&255);
                        output.extend_from_slice(&[color[0], color[1], color[2], *alpha]);
                    } else {
                        output.extend_from_slice(&[color[0], color[1], color[2]]);
                    }
                }
            }
            1 | 2 | 4 => {
                let bits_per_pixel = self.bit_depth as usize;
                let pixels_per_byte = 8 / bits_per_pixel;
                let mask = (1 << bits_per_pixel) - 1;
                let width = self.width as usize;
                let mut pixel_count = 0;

                for &byte in input {
                    for shift in (0..pixels_per_byte).rev() {
                        if pixel_count >= width {
                            break;
                        }

                        let index = (byte >> (shift * bits_per_pixel)) & mask;
                        let color = palette.get(index as usize).unwrap_or(&[0, 0, 0]);

                        if has_trans {
                            let alpha = trans.as_ref().unwrap().get(index as usize).unwrap_or(&255);
                            output.extend_from_slice(&[color[0], color[1], color[2], *alpha]);
                        } else {
                            output.extend_from_slice(&[color[0], color[1], color[2]]);
                        }

                        pixel_count += 1;
                    }
                    if pixel_count >= width {
                        pixel_count = 0;
                    }
                }
            }
            _ => unreachable!()
        };

        if has_trans {
            Ok(PixelData::RGBA8(output))
        } else {
            Ok(PixelData::RGB8(output))
        }
    }

    // TODO remove after I confirm above version is actually correct
    /* fn decode_indexed(&self, input: &[u8]) -> VexelResult<PixelData> {
         let palette = match &self.palette {
             Some(palette) => palette,
             None => {
                 log_warn!("No palette found for indexed color");
                 &Vec::new()
             }
         };

         let mut output = Vec::new();

         match self.bit_depth {
             8 => {
                 for &index in input {
                     let color = palette.get(index as usize).unwrap_or_else(|| &RGB { r: 0, g: 0, b: 0 });

                     output.extend_from_slice(&[color.r, color.g, color.b]);
                 }
             }
             1 | 2 | 4 => {
                 let bits_per_pixel = self.bit_depth as usize;
                 let pixels_per_byte = 8 / bits_per_pixel;
                 let mask = (1 << bits_per_pixel) - 1;
                 let width = self.width as usize;
                 let mut pixel_count = 0;

                 for (_, &byte) in input.iter().enumerate() {
                     for shift in 0..pixels_per_byte {
                         if pixel_count >= width {
                             break;
                         }

                         let mask_shift = (pixels_per_byte - 1 - shift) * bits_per_pixel;
                         let positioned_mask = mask << mask_shift;
                         let extracted = (byte & positioned_mask) >> mask_shift;

                         let color = palette.get(extracted as usize)
                             .unwrap_or_else(|| &RGB { r: 0, g: 0, b: 0 });

                         output.extend_from_slice(&[color.r, color.g, color.b]);
                         pixel_count += 1;
                     }

                     if pixel_count >= width {
                         pixel_count = 0;
                     }
                 }
             }
             _ => unreachable!()
         }

         Ok(PixelData::RGB8(output.to_vec()))
     }*/

    fn decode_grayscale(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => {
                Ok(PixelData::L8(input.to_vec()))
            }
            16 => {
                let mut output = Vec::with_capacity(input.len() / 2);

                for gray in input.chunks_exact(2) {
                    output.push(u16::from_be_bytes([*gray.get_safe(0).unwrap_or_else(|_| &0), *gray.get_safe(1).unwrap_or_else(|_| &0)]));
                }

                Ok(PixelData::L16(output))
            }
            1 | 2 | 4 => {
                let bits_per_pixel = self.bit_depth as usize;
                let pixels_per_byte = 8 / bits_per_pixel;
                let mask = (1 << bits_per_pixel) - 1;
                let max_value = mask;
                let width = self.width as usize;
                let mut pixel_count = 0;
                let mut output = Vec::new();

                for &byte in input {
                    for shift in (0..pixels_per_byte).rev() {
                        if pixel_count >= width {
                            break;
                        }
                        let value = (byte >> (shift * bits_per_pixel)) & mask;
                        let scaled = (value as u16 * 255 / max_value as u16) as u8;
                        output.push(scaled);
                        pixel_count += 1;
                    }
                    if pixel_count >= width {
                        pixel_count = 0;
                    }
                }

                Ok(PixelData::L8(output))
            }
            _ => unreachable!()
        }
    }

    fn decode_grayscale_alpha(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => {
                Ok(PixelData::LA8(input.to_vec()))
            }
            16 => {
                let mut output = Vec::with_capacity((input.len() / 4) * 2);

                for ga in input.chunks_exact(4) {
                    output.push(u16::from_be_bytes([ga[0], ga[1]]));
                    output.push(u16::from_be_bytes([ga[2], ga[3]]));
                }

                Ok(PixelData::LA16(output))
            }
            _ => {
                log_warn!("Invalid bit depth for grayscale alpha: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::LA8(input.to_vec()))
            }
        }
    }

    fn decode_rgb(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => {
                Ok(PixelData::RGB8(input.to_vec()))
            }
            16 => {
                let mut output = Vec::with_capacity((input.len() / 6) * 3);

                for rgb in input.chunks_exact(6) {
                    output.push(u16::from_be_bytes([rgb[0], rgb[1]]));
                    output.push(u16::from_be_bytes([rgb[2], rgb[3]]));
                    output.push(u16::from_be_bytes([rgb[4], rgb[5]]));
                }

                Ok(PixelData::RGB16(output))
            }
            _ => {
                log_warn!("Invalid bit depth for RGB color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGB8(input.to_vec()))
            }
        }
    }

    fn decode_rgba(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => {
                Ok(PixelData::RGBA8(input.to_vec()))
            }
            16 => {
                let mut output = Vec::with_capacity((input.len() / 8) * 4);
                for rgba in input.chunks_exact(8) {
                    output.push(u16::from_be_bytes([rgba[0], rgba[1]]));
                    output.push(u16::from_be_bytes([rgba[2], rgba[3]]));
                    output.push(u16::from_be_bytes([rgba[4], rgba[5]]));
                    output.push(u16::from_be_bytes([rgba[6], rgba[7]]));
                }

                Ok(PixelData::RGBA16(output))
            }
            _ => {
                log_warn!("Invalid bit depth for RGBA color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGBA8(input.to_vec()))
            }
        }
    }

    fn deinterlace_scan_lines(&self, data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        if !self.interlace {
            return self.unfilter_scanlines(data, width);
        }

        const ADAM7_COL_START: [usize; 7] = [0, 4, 0, 2, 0, 1, 0];
        const ADAM7_ROW_START: [usize; 7] = [0, 0, 4, 0, 2, 0, 1];
        const ADAM7_COL_DELTA: [usize; 7] = [8, 8, 4, 4, 2, 2, 1];
        const ADAM7_ROW_DELTA: [usize; 7] = [8, 8, 8, 4, 4, 2, 2];

        let bits_per_pixel = self.get_bits_per_pixel();

        let out_bytes = (bits_per_pixel as usize * width as usize + 7) / 8;
        let mut output = vec![0u8; out_bytes * height as usize];
        let mut data_offset = 0;

        for pass in 0..7 {
            let pass_width = (width as usize + ADAM7_COL_DELTA[pass] - 1 - ADAM7_COL_START[pass]) / ADAM7_COL_DELTA[pass];
            let pass_height = (height as usize + ADAM7_ROW_DELTA[pass] - 1 - ADAM7_ROW_START[pass]) / ADAM7_ROW_DELTA[pass];

            if pass_width == 0 || pass_height == 0 {
                continue;
            }

            let pass_bits_per_row = bits_per_pixel as usize * pass_width;
            let pass_bytes_per_row = (pass_bits_per_row + 7) / 8;
            let pass_size = (pass_bytes_per_row + 1) * pass_height;

            if data_offset + pass_size > data.len() {
                log_warn!("Insufficient data for interlaced image");
                break;
            }

            let pass_data = &data[data_offset..data_offset + pass_size];
            let unfiltered = self.unfilter_scanlines(pass_data, pass_width as u32)?;
            let mut unfiltered_idx = 0;

            for row in 0..pass_height {
                let out_y = row * ADAM7_ROW_DELTA[pass] + ADAM7_ROW_START[pass];
                if out_y >= height as usize {
                    break;
                }

                if bits_per_pixel < 8 {
                    let pixels_per_byte = 8 / bits_per_pixel as usize;
                    let bit_mask = (1 << bits_per_pixel) - 1;

                    for col in 0..pass_width {
                        let out_x = col * ADAM7_COL_DELTA[pass] + ADAM7_COL_START[pass];
                        if out_x >= width as usize {
                            break;
                        }

                        let in_byte_idx = unfiltered_idx + (col / pixels_per_byte);
                        let in_bit_shift = (pixels_per_byte - 1 - (col % pixels_per_byte)) * bits_per_pixel as usize;

                        if in_byte_idx > unfiltered.len() {
                            log_warn!("Invalid byte index: {} > {}", in_byte_idx, unfiltered.len());
                            continue;
                        }

                        let in_pixel = (unfiltered[in_byte_idx] >> in_bit_shift) & bit_mask;

                        let out_byte_idx = (out_y * out_bytes) + (out_x / pixels_per_byte);
                        let out_bit_shift = (pixels_per_byte - 1 - (out_x % pixels_per_byte)) * bits_per_pixel as usize;

                        if out_byte_idx < output.len() {
                            output[out_byte_idx] &= !(bit_mask << out_bit_shift);
                            output[out_byte_idx] |= in_pixel << out_bit_shift;
                        }
                    }
                } else {
                    let bytes_per_pixel = (bits_per_pixel as usize) / 8;

                    for col in 0..pass_width {
                        let out_x = col * ADAM7_COL_DELTA[pass] + ADAM7_COL_START[pass];
                        if out_x >= width as usize {
                            break;
                        }

                        let out_pos = (out_y * out_bytes) + (out_x * bytes_per_pixel);
                        let in_pos = unfiltered_idx + (col * bytes_per_pixel);

                        if out_pos + bytes_per_pixel <= output.len() &&
                            in_pos + bytes_per_pixel <= unfiltered.len() {
                            output[out_pos..out_pos + bytes_per_pixel]
                                .copy_from_slice(&unfiltered[in_pos..in_pos + bytes_per_pixel]);
                        }
                    }
                }

                unfiltered_idx += pass_bytes_per_row;
            }

            data_offset += pass_size;
        }

        Ok(output)
    }

    fn compose_frame(&self, pixels: &PixelData, fctl: &FctlChunk, prev_frame: Option<PixelData>, dispose_op: u8, prev_fctl: Option<&FctlChunk>) -> VexelResult<PixelData> {
        // TODO maybe try to not convert to RGBA if possible
        let frame_pixels = pixels.clone().into_rgba8();

        let mut output = match (dispose_op, prev_frame) {
            (_, None) => {
                PixelData::RGBA8(vec![0; (self.width * self.height * 4) as usize])
            }

            // DISPOSE_OP_NONE - keep previous frame as is
            (0, Some(prev)) => {
                prev.clone().into_rgba8()
            }

            // DISPOSE_OP_BACKGROUND - clear previous frame's region to transparent
            (1, Some(prev)) => {
                let mut output = prev.clone().into_rgba8();
                let output_data = output.as_bytes_mut();

                if let Some(prev_fctl) = prev_fctl {
                    for y in 0..prev_fctl.height {
                        let row_start = ((y + prev_fctl.y_offset) * self.width + prev_fctl.x_offset) as usize * 4;
                        for x in 0..prev_fctl.width {
                            let pixel_start = row_start + (x as usize * 4);
                            if pixel_start + 3 < output_data.len() {
                                output_data[pixel_start] = 0;     
                                output_data[pixel_start + 1] = 0; 
                                output_data[pixel_start + 2] = 0; 
                                output_data[pixel_start + 3] = 0; 
                            }
                        }
                    }
                }
                output
            }

            // DISPOSE_OP_PREVIOUS - revert to frame before previous
            // For now, treat same as DISPOSE_OP_BACKGROUND
            (2, Some(prev)) => {
                let mut output = prev.clone().into_rgba8();
                let output_data = output.as_bytes_mut();

                if let Some(prev_fctl) = prev_fctl {
                    for y in 0..prev_fctl.height {
                        let row_start = ((y + prev_fctl.y_offset) * self.width + prev_fctl.x_offset) as usize * 4;
                        for x in 0..prev_fctl.width {
                            let pixel_start = row_start + (x as usize * 4);
                            if pixel_start + 3 < output_data.len() {
                                output_data[pixel_start] = 0;
                                output_data[pixel_start + 1] = 0;
                                output_data[pixel_start + 2] = 0;
                                output_data[pixel_start + 3] = 0;
                            }
                        }
                    }
                }
                output
            }

            _ => PixelData::RGBA8(vec![0; (self.width * self.height * 4) as usize]),
        };

        let frame_data = frame_pixels.as_bytes();
        let output_data = output.as_bytes_mut();

        for y in 0..fctl.height {
            let frame_row_start = (y * fctl.width) as usize * 4;
            let output_row_start = ((y + fctl.y_offset) * self.width + fctl.x_offset) as usize * 4;

            for x in 0..fctl.width {
                let frame_pixel_start = frame_row_start + (x as usize * 4);
                let output_pixel_start = output_row_start + (x as usize * 4);

                if frame_pixel_start + 4 > frame_data.len() ||
                    output_pixel_start + 4 > output_data.len() {
                    continue;
                }

                if fctl.blend_op == 0 {
                    // Source blend - direct copy
                    output_data[output_pixel_start..output_pixel_start + 4]
                        .copy_from_slice(&frame_data[frame_pixel_start..frame_pixel_start + 4]);
                } else {
                    // Alpha blend (Over)
                    let src_a = frame_data[frame_pixel_start + 3] as f32 / 255.0;
                    if src_a > 0.0 {  // Only blend if source has some opacity
                        let dst_a = output_data[output_pixel_start + 3] as f32 / 255.0;
                        let out_a = src_a + dst_a * (1.0 - src_a);

                        if out_a > 0.0 {
                            for i in 0..3 {
                                let src = frame_data[frame_pixel_start + i] as f32;
                                let dst = output_data[output_pixel_start + i] as f32;
                                let blended = ((src * src_a + dst * dst_a * (1.0 - src_a)) / out_a) as u8;
                                output_data[output_pixel_start + i] = blended;
                            }
                            output_data[output_pixel_start + 3] = (out_a * 255.0) as u8;
                        }
                    }
                }
            }
        }

        Ok(output)
    }

    fn decode_apng_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        let mut decoded_frames: Vec<ImageFrame> = Vec::new();
        let mut previous_frame = None;
        let mut prev_dispose_op = 0;
        let mut prev_fctl = None;

        if self.frames.iter().filter(|f| f.fctl_info.width > 0 && f.fctl_info.height > 0 && !f.fdat.is_empty()).count() == 0 {
            return Err(VexelError::Custom("No valid frames found".into()));
        }

        for frame in self.frames.iter() {
            let fctl = &frame.fctl_info;

            if fctl.width == 0 || fctl.height == 0 ||
                fctl.x_offset + fctl.width > self.width ||
                fctl.y_offset + fctl.height > self.height {
                return Err(VexelError::Custom("Invalid frame dimensions".into()));
            }

            let mut decoder = ZlibDecoder::new(&frame.fdat[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;

            let frame_pixels = self.deinterlace_scan_lines(&decompressed, fctl.width, fctl.height)?;

            let mut pixels = match self.color_type {
                ColorType::Indexed => self.decode_indexed(&frame_pixels)?,
                ColorType::RGB => self.decode_rgb(&frame_pixels)?,
                ColorType::RGBA => self.decode_rgba(&frame_pixels)?,
                ColorType::Grayscale => self.decode_grayscale(&frame_pixels)?,
                ColorType::GrayscaleAlpha => self.decode_grayscale_alpha(&frame_pixels)?,
            };

            pixels.correct_pixels(fctl.width, fctl.height);

            let out = self.compose_frame(&pixels, fctl, previous_frame, prev_dispose_op, prev_fctl)?;

            previous_frame = Some(out.clone());
            prev_dispose_op = fctl.dispose_op;
            prev_fctl = Some(fctl);

            decoded_frames.push(ImageFrame {
                width: self.width,
                height: self.height,
                delay: if fctl.delay_den == 0 {
                    (fctl.delay_num as f32 / 100.0) as u32
                } else {
                    (fctl.delay_num as f32 / fctl.delay_den as f32) as u32
                },
                pixels: out,
            });
        }

        Ok(decoded_frames)
    }

    fn decode_pixels(&mut self) -> VexelResult<PixelData> {
        if self.compression_method == CompressionMethod::Deflate {
            // TODO replace with custom implementation
            let mut decoder = ZlibDecoder::new(self.idat_data.as_slice());
            let mut decompressed = Vec::new();

            decoder.read_to_end(&mut decompressed)?;

            self.idat_data = decompressed;
        }

        let data = self.deinterlace_scan_lines(&self.idat_data, self.width, self.height)?;

        let mut pixels = match self.color_type {
            ColorType::Indexed => self.decode_indexed(&data)?,
            ColorType::RGB => self.decode_rgb(&data)?,
            ColorType::RGBA => self.decode_rgba(&data)?,
            ColorType::Grayscale => self.decode_grayscale(&data)?,
            ColorType::GrayscaleAlpha => self.decode_grayscale_alpha(&data)?,
        };

        pixels.correct_pixels(self.width, self.height);

        Ok(pixels)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut window = [0u8; 4];

        for i in 1..4 {
            window[i] = self.reader.read_u8()?;
        }

        while let Ok(byte) = self.reader.read_u8() {
            window[0] = window[1];
            window[1] = window[2];
            window[2] = window[3];
            window[3] = byte;

            match get_chunk(&window) {
                Some(chunk) => {
                    let result = match chunk {
                        PngChunk::IHDR => self.read_ihdr(),
                        PngChunk::PLTE => self.read_plte(),
                        PngChunk::IDAT => self.read_idat(),
                        PngChunk::GAMA => self.read_gama(),
                        PngChunk::CHRM => self.read_chrm(),
                        PngChunk::TEXT => self.read_text(),
                        PngChunk::ZTXT => self.read_ztxt(),
                        PngChunk::ITXT => self.read_itxt(),
                        PngChunk::SRGB => self.read_srgb(),
                        PngChunk::TRNS => self.read_trns(),
                        PngChunk::BKGD => self.read_bkgd(),
                        PngChunk::PHYS => self.read_phys(),
                        PngChunk::SBIT => self.read_sbit(),
                        PngChunk::HIST => self.read_hist(),
                        PngChunk::TIME => self.read_time(),
                        PngChunk::SPLT => self.read_splt(),
                        PngChunk::ACTL => self.read_actl(),
                        PngChunk::FCTL => self.read_fctl(),
                        PngChunk::FDAT => self.read_fdat(),
                        PngChunk::ICCP => {
                            log_warn!("Ignoring ICCP chunk");
                            Ok(())
                        }
                        PngChunk::IEND => break,
                    };

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            log_warn!("Error reading chunk {:?}: {:?}", chunk, e);
                        }
                    }
                }
                None => {}
            }
        }

        // We have APNG
        if self.actl_info.is_some() {
            let result = self.decode_apng_frames();

            if let Ok(image_frames) = result {
                return Ok(Image::new(self.width, self.height, PixelFormat::RGBA8, image_frames));
            } else {
                log_warn!("Error decoding APNG frames: {:?}", result);
            }
        }

        // Regular PNG
        let pixel_data = self.decode_pixels()?;

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
