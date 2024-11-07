use std::f32::consts::PI;
use std::fmt::{Debug, Display, Formatter};
use std::io::{Cursor, Error, ErrorKind, Read, Seek};
use crate::bitreader::BitReader;
use crate::utils::info_display::JpegInfo;
use crate::utils::marker::Marker;
use crate::utils::types::ByteOrder;

#[derive(Clone, Debug, PartialEq)]
pub enum JpegMarker {
    // Start Of Frame markers, non-differential, Huffman coding
    SOF0,  // Baseline DCT
    SOF1,  // Extended sequential DCT
    SOF2,  // Progressive DCT
    SOF3,  // Lossless (sequential)

    // Start Of Frame markers, differential, Huffman coding
    SOF5,  // Differential sequential DCT
    SOF6,  // Differential progressive DCT
    SOF7,  // Differential lossless (sequential)

    // Start Of Frame markers, non-differential, arithmetic coding
    JPG,   // Reserved for JPEG extensions
    SOF9,  // Extended sequential DCT
    SOF10, // Progressive DCT
    SOF11, // Lossless (sequential)

    // Start Of Frame markers, differential, arithmetic coding
    SOF13, // Differential sequential DCT
    SOF14, // Differential progressive DCT
    SOF15, // Differential lossless (sequential)

    // Huffman table specification
    DHT,   // Define Huffman table(s)

    // Arithmetic coding conditioning specification
    DAC,   // Define arithmetic coding conditioning(s)

    // Restart interval termination
    RST0,
    RST1,
    RST2,
    RST3,
    RST4,
    RST5,
    RST6,
    RST7,

    // Other markers
    SOI,   // Start of image
    EOI,   // End of image
    SOS,   // Start of scan
    DQT,   // Define quantization table(s)
    DNL,   // Define number of lines
    DRI,   // Define restart interval
    DHP,   // Define hierarchical progression
    EXP,   // Expand reference component(s)

    // Application segments
    APP0,
    APP1,
    APP2,
    APP3,
    APP4,
    APP5,
    APP6,
    APP7,
    APP8,
    APP9,
    APP10,
    APP11,
    APP12,
    APP13,
    APP14,
    APP15,

    // JPEG extensions
    JPG0,
    JPG1,
    JPG2,
    JPG3,
    JPG4,
    JPG5,
    JPG6,
    JPG7,
    JPG8,
    JPG9,
    JPG10,
    JPG11,
    JPG12,
    JPG13,

    COM,   // Comment

    // Special markers
    TEM,   // For temporary private use in arithmetic coding

    // Reserved marker
    RES(u8),
}

impl Marker for JpegMarker {
    fn from_u16(value: u16) -> Option<JpegMarker> {
        match value {
            0xFFC0 => Some(JpegMarker::SOF0),
            0xFFC1 => Some(JpegMarker::SOF1),
            0xFFC2 => Some(JpegMarker::SOF2),
            0xFFC3 => Some(JpegMarker::SOF3),
            0xFFC5 => Some(JpegMarker::SOF5),
            0xFFC6 => Some(JpegMarker::SOF6),
            0xFFC7 => Some(JpegMarker::SOF7),
            0xFFC8 => Some(JpegMarker::JPG),
            0xFFC9 => Some(JpegMarker::SOF9),
            0xFFCA => Some(JpegMarker::SOF10),
            0xFFCB => Some(JpegMarker::SOF11),
            0xFFCD => Some(JpegMarker::SOF13),
            0xFFCE => Some(JpegMarker::SOF14),
            0xFFCF => Some(JpegMarker::SOF15),
            0xFFC4 => Some(JpegMarker::DHT),
            0xFFCC => Some(JpegMarker::DAC),
            0xFFD0 => Some(JpegMarker::RST0),
            0xFFD1 => Some(JpegMarker::RST1),
            0xFFD2 => Some(JpegMarker::RST2),
            0xFFD3 => Some(JpegMarker::RST3),
            0xFFD4 => Some(JpegMarker::RST4),
            0xFFD5 => Some(JpegMarker::RST5),
            0xFFD6 => Some(JpegMarker::RST6),
            0xFFD7 => Some(JpegMarker::RST7),
            0xFFD8 => Some(JpegMarker::SOI),
            0xFFD9 => Some(JpegMarker::EOI),
            0xFFDA => Some(JpegMarker::SOS),
            0xFFDB => Some(JpegMarker::DQT),
            0xFFDC => Some(JpegMarker::DNL),
            0xFFDD => Some(JpegMarker::DRI),
            0xFFDE => Some(JpegMarker::DHP),
            0xFFDF => Some(JpegMarker::EXP),
            0xFFE0 => Some(JpegMarker::APP0),
            0xFFE1 => Some(JpegMarker::APP1),
            0xFFE2 => Some(JpegMarker::APP2),
            0xFFE3 => Some(JpegMarker::APP3),
            0xFFE4 => Some(JpegMarker::APP4),
            0xFFE5 => Some(JpegMarker::APP5),
            0xFFE6 => Some(JpegMarker::APP6),
            0xFFE7 => Some(JpegMarker::APP7),
            0xFFE8 => Some(JpegMarker::APP8),
            0xFFE9 => Some(JpegMarker::APP9),
            0xFFEA => Some(JpegMarker::APP10),
            0xFFEB => Some(JpegMarker::APP11),
            0xFFEC => Some(JpegMarker::APP12),
            0xFFED => Some(JpegMarker::APP13),
            0xFFEE => Some(JpegMarker::APP14),
            0xFFEF => Some(JpegMarker::APP15),
            0xFFF0 => Some(JpegMarker::JPG0),
            0xFFF1 => Some(JpegMarker::JPG1),
            0xFFF2 => Some(JpegMarker::JPG2),
            0xFFF3 => Some(JpegMarker::JPG3),
            0xFFF4 => Some(JpegMarker::JPG4),
            0xFFF5 => Some(JpegMarker::JPG5),
            0xFFF6 => Some(JpegMarker::JPG6),
            0xFFF7 => Some(JpegMarker::JPG7),
            0xFFF8 => Some(JpegMarker::JPG8),
            0xFFF9 => Some(JpegMarker::JPG9),
            0xFFFA => Some(JpegMarker::JPG10),
            0xFFFB => Some(JpegMarker::JPG11),
            0xFFFC => Some(JpegMarker::JPG12),
            0xFFFD => Some(JpegMarker::JPG13),
            0xFFFE => Some(JpegMarker::COM),
            0xFF01 => Some(JpegMarker::TEM),
            0xFF02..=0xFFBF => Some(JpegMarker::RES((value & 0xFF) as u8)),
            _ => None,
        }
    }

    fn to_u16(&self) -> u16 {
        match self {
            JpegMarker::SOF0 => 0xFFC0,
            JpegMarker::SOF1 => 0xFFC1,
            JpegMarker::SOF2 => 0xFFC2,
            JpegMarker::SOF3 => 0xFFC3,
            JpegMarker::SOF5 => 0xFFC5,
            JpegMarker::SOF6 => 0xFFC6,
            JpegMarker::SOF7 => 0xFFC7,
            JpegMarker::JPG => 0xFFC8,
            JpegMarker::SOF9 => 0xFFC9,
            JpegMarker::SOF10 => 0xFFCA,
            JpegMarker::SOF11 => 0xFFCB,
            JpegMarker::SOF13 => 0xFFCD,
            JpegMarker::SOF14 => 0xFFCE,
            JpegMarker::SOF15 => 0xFFCF,
            JpegMarker::DHT => 0xFFC4,
            JpegMarker::DAC => 0xFFCC,
            JpegMarker::RST0 => 0xFFD0,
            JpegMarker::RST1 => 0xFFD1,
            JpegMarker::RST2 => 0xFFD2,
            JpegMarker::RST3 => 0xFFD3,
            JpegMarker::RST4 => 0xFFD4,
            JpegMarker::RST5 => 0xFFD5,
            JpegMarker::RST6 => 0xFFD6,
            JpegMarker::RST7 => 0xFFD7,
            JpegMarker::SOI => 0xFFD8,
            JpegMarker::EOI => 0xFFD9,
            JpegMarker::SOS => 0xFFDA,
            JpegMarker::DQT => 0xFFDB,
            JpegMarker::DNL => 0xFFDC,
            JpegMarker::DRI => 0xFFDD,
            JpegMarker::DHP => 0xFFDE,
            JpegMarker::EXP => 0xFFDF,
            JpegMarker::APP0 => 0xFFE0,
            JpegMarker::APP1 => 0xFFE1,
            JpegMarker::APP2 => 0xFFE2,
            JpegMarker::APP3 => 0xFFE3,
            JpegMarker::APP4 => 0xFFE4,
            JpegMarker::APP5 => 0xFFE5,
            JpegMarker::APP6 => 0xFFE6,
            JpegMarker::APP7 => 0xFFE7,
            JpegMarker::APP8 => 0xFFE8,
            JpegMarker::APP9 => 0xFFE9,
            JpegMarker::APP10 => 0xFFEA,
            JpegMarker::APP11 => 0xFFEB,
            JpegMarker::APP12 => 0xFFEC,
            JpegMarker::APP13 => 0xFFED,
            JpegMarker::APP14 => 0xFFEE,
            JpegMarker::APP15 => 0xFFEF,
            JpegMarker::JPG0 => 0xFFF0,
            JpegMarker::JPG1 => 0xFFF1,
            JpegMarker::JPG2 => 0xFFF2,
            JpegMarker::JPG3 => 0xFFF3,
            JpegMarker::JPG4 => 0xFFF4,
            JpegMarker::JPG5 => 0xFFF5,
            JpegMarker::JPG6 => 0xFFF6,
            JpegMarker::JPG7 => 0xFFF7,
            JpegMarker::JPG8 => 0xFFF8,
            JpegMarker::JPG9 => 0xFFF9,
            JpegMarker::JPG10 => 0xFFFA,
            JpegMarker::JPG11 => 0xFFFB,
            JpegMarker::JPG12 => 0xFFFC,
            JpegMarker::JPG13 => 0xFFFD,
            JpegMarker::COM => 0xFFFE,
            JpegMarker::TEM => 0xFF01,
            JpegMarker::RES(value) => 0xFF00 | (*value as u16),
        }
    }
}

static JPEG_MARKERS: [JpegMarker; 64] =
    [
        JpegMarker::SOF0,
        JpegMarker::SOF1,
        JpegMarker::SOF2,
        JpegMarker::SOF3,
        JpegMarker::SOF5,
        JpegMarker::SOF6,
        JpegMarker::SOF7,
        JpegMarker::JPG,
        JpegMarker::SOF9,
        JpegMarker::SOF10,
        JpegMarker::SOF11,
        JpegMarker::SOF13,
        JpegMarker::SOF14,
        JpegMarker::SOF15,
        JpegMarker::DHT,
        JpegMarker::DAC,
        JpegMarker::RST0,
        JpegMarker::RST1,
        JpegMarker::RST2,
        JpegMarker::RST3,
        JpegMarker::RST4,
        JpegMarker::RST5,
        JpegMarker::RST6,
        JpegMarker::RST7,
        JpegMarker::SOI,
        JpegMarker::EOI,
        JpegMarker::SOS,
        JpegMarker::DQT,
        JpegMarker::DNL,
        JpegMarker::DRI,
        JpegMarker::DHP,
        JpegMarker::EXP,
        JpegMarker::APP0,
        JpegMarker::APP1,
        JpegMarker::APP2,
        JpegMarker::APP3,
        JpegMarker::APP4,
        JpegMarker::APP5,
        JpegMarker::APP6,
        JpegMarker::APP7,
        JpegMarker::APP8,
        JpegMarker::APP9,
        JpegMarker::APP10,
        JpegMarker::APP11,
        JpegMarker::APP12,
        JpegMarker::APP13,
        JpegMarker::APP14,
        JpegMarker::APP15,
        JpegMarker::JPG0,
        JpegMarker::JPG1,
        JpegMarker::JPG2,
        JpegMarker::JPG3,
        JpegMarker::JPG4,
        JpegMarker::JPG5,
        JpegMarker::JPG6,
        JpegMarker::JPG7,
        JpegMarker::JPG8,
        JpegMarker::JPG9,
        JpegMarker::JPG10,
        JpegMarker::JPG11,
        JpegMarker::JPG12,
        JpegMarker::JPG13,
        JpegMarker::COM,
        JpegMarker::TEM,
    ];

#[rustfmt::skip]
const ZIGZAG_MAP: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Debug, Clone)]
pub struct QuantizationTable {
    pub id: u8,
    pub precision: u8,
    pub length: u16,
    pub table: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct HuffmanTable {
    pub id: u8,
    pub class: u8,
    pub offsets: Vec<u32>,
    pub symbols: Vec<u8>,
    pub codes: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ArithmeticCodingValue {
    pub value: u8,
    pub length: u8,
}

#[derive(Debug, Clone)]
pub struct ArithmeticCodingTable {
    pub table_class: u8,
    pub identifier: u8,
    pub values: Vec<ArithmeticCodingValue>,
}

#[derive(Debug)]
pub struct ColorComponentInfo {
    pub id: u8,
    pub horizontal_sampling_factor: u8,
    pub vertical_sampling_factor: u8,
    pub quantization_table_id: u8,
    pub dc_table_selector: u8,
    pub ac_table_selector: u8,
}

#[derive(Debug, Clone)]
pub struct MCU {
    y: Vec<i32>,
    cb: Vec<i32>,
    cr: Vec<i32>,
}

#[derive(Debug, Clone)]
pub struct JFIFHeader {
    pub identifier: String,     // "JFIF\0" (5 bytes)
    pub version_major: u8,      // JFIF format version major
    pub version_minor: u8,      // JFIF format version minor
    pub density_units: u8,      // Units for pixel density (0 = no units, 1 = pixels per inch, 2 = pixels per cm)
    pub x_density: u16,        // Horizontal pixel density
    pub y_density: u16,        // Vertical pixel density
    pub thumbnail_width: u8,   // Width of embedded thumbnail (0 if no thumbnail)
    pub thumbnail_height: u8,  // Height of embedded thumbnail (0 if no thumbnail)
    pub thumbnail_data: Vec<u8>, // RGB thumbnail data if present
}

#[derive(Debug, Clone)]
pub struct ExifHeader {
    pub identifier: String,     // "Exif\0\0" (6 bytes)
    pub byte_order: ByteOrder,  // Either "II" (little-endian) or "MM" (big-endian)
    pub first_ifd_offset: u32, // Offset to first IFD
    pub ifd_entries: Vec<IFDEntry>,
}

#[derive(Debug, Clone)]
pub struct IFDEntry {
    pub tag: u16,
    pub format: u16,
    pub components: u32,
    pub value_offset: u32,
}

pub struct JpegDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    jfif_header: Option<JFIFHeader>,
    exif_header: Option<ExifHeader>,
    comments: Vec<String>,
    quantization_tables: Vec<QuantizationTable>,
    ac_huffman_tables: Vec<HuffmanTable>,
    dc_huffman_tables: Vec<HuffmanTable>,
    ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    start_of_spectral_selection: u8,
    end_of_spectral_selection: u8,
    successive_approximation_high: u8,
    successive_approximation_low: u8,
    horizontal_sampling_factor: u8,
    vertical_sampling_factor: u8,
    restart_interval: u16,
    // Internal state
    mcu_width: u32,
    mcu_height: u32,
    mcu_r_width: u32,
    mcu_r_height: u32,
    precision: u8,
    component_count: u8,
    components_in_scan: u8,
    components: Vec<ColorComponentInfo>,
    data: Vec<u8>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            comments: Vec::new(),
            jfif_header: None,
            exif_header: None,
            mcu_width: 0,
            mcu_height: 0,
            mcu_r_width: 0,
            mcu_r_height: 0,
            precision: 0,
            component_count: 0,
            start_of_spectral_selection: 0,
            end_of_spectral_selection: 0,
            successive_approximation_high: 0,
            successive_approximation_low: 0,
            components_in_scan: 0,
            components: Vec::new(),
            quantization_tables: Vec::new(),
            ac_huffman_tables: Vec::new(),
            dc_huffman_tables: Vec::new(),
            ac_arithmetic_tables: Vec::new(),
            dc_arithmetic_tables: Vec::new(),
            horizontal_sampling_factor: 1,
            vertical_sampling_factor: 1,
            restart_interval: 0,
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

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub fn component_count(&self) -> u8 {
        self.component_count
    }

    pub fn components(&self) -> &Vec<ColorComponentInfo> {
        &self.components
    }

    pub fn quantization_tables(&self) -> &Vec<QuantizationTable> {
        &self.quantization_tables
    }

    pub fn huffman_tables(&self) -> [&Vec<HuffmanTable>; 2] {
        [&self.ac_huffman_tables, &self.dc_huffman_tables]
    }

    pub fn get_data(&self) -> JpegInfo {
        JpegInfo {
            width: self.width,
            height: self.height,
            color_depth: self.precision,
            number_of_components: self.component_count,
            jfif_header: self.jfif_header.clone(),
            exif_header: self.exif_header.clone(),
            quantization_tables: self.quantization_tables.clone(),
            ac_huffman_tables: self.ac_huffman_tables.clone(),
            dc_huffman_tables: self.dc_huffman_tables.clone(),
            ac_arithmetic_tables: self.ac_arithmetic_tables.clone(),
            dc_arithmetic_tables: self.dc_arithmetic_tables.clone(),
            spectral_selection: (self.start_of_spectral_selection, self.end_of_spectral_selection),
            successive_approximation: (self.successive_approximation_high, self.successive_approximation_low),
            horizontal_sampling_factor: self.horizontal_sampling_factor,
            vertical_sampling_factor: self.vertical_sampling_factor,
            restart_interval: self.restart_interval,
            comments: self.comments.clone(),
        }
    }

    fn read_com(&mut self) -> Result<(), Error> {
        // Read marker length (2 bytes)
        let length = self.reader.read_bits(16)? as usize - 2; // Subtract length field size

        // Read comment text
        let mut comment_bytes = Vec::with_capacity(length);
        for _ in 0..length {
            comment_bytes.push(self.reader.read_bits(8)? as u8);
        }

        // Try to convert to UTF-8, fallback to lossy conversion if invalid
        let text = String::from_utf8_lossy(&comment_bytes).to_string();

        self.comments.push(text);

        Ok(())
    }

    fn read_app0_jfif(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        let length = self.reader.read_u16()?;

        // Read JFIF identifier (5 bytes)
        let mut identifier = Vec::new();
        for _ in 0..5 {
            identifier.push(self.reader.read_u8()?);
        }

        let identifier = String::from_utf8_lossy(&identifier).to_string();

        if identifier != "JFIF\0" {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid JFIF signature"));
        }

        // Read version
        let version_major = self.reader.read_bits(8)? as u8;
        let version_minor = self.reader.read_bits(8)? as u8;

        // Read density information
        let density_units = self.reader.read_bits(8)? as u8;
        let x_density = self.reader.read_bits(16)? as u16;
        let y_density = self.reader.read_bits(16)? as u16;

        // Read thumbnail dimensions
        let thumbnail_width = self.reader.read_bits(8)? as u8;
        let thumbnail_height = self.reader.read_bits(8)? as u8;

        // Read thumbnail data if present
        let thumbnail_size = thumbnail_width * thumbnail_height * 3; // RGB data
        let mut thumbnail_data = Vec::new();

        if thumbnail_size > 0 {
            for _ in 0..thumbnail_size {
                thumbnail_data.push(self.reader.read_bits(8)? as u8);
            }
        }

        self.jfif_header = Some(JFIFHeader {
            identifier,
            version_major,
            version_minor,
            density_units,
            x_density,
            y_density,
            thumbnail_width,
            thumbnail_height,
            thumbnail_data,
        });

        Ok(())
    }

    fn read_app1_exif(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        self.reader.read_bits(16)? as usize;

        // Read Exif identifier (6 bytes)
        let mut identifier = Vec::new();
        for _ in 0..6 {
            identifier.push(self.reader.read_bits(8)? as u8);
        }

        let identifier = String::from_utf8_lossy(&identifier).to_string();

        if identifier != "Exif\0\0" {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid Exif signature"));
        }

        // Read byte order marker
        let mut byte_order_marker = Vec::new();
        for _ in 0..2 {
            byte_order_marker.push(self.reader.read_bits(8)? as u8);
        }

        let byte_order_str = String::from_utf8_lossy(&byte_order_marker).to_string();

        let byte_order = match byte_order_str.as_str() {
            "II" => Some(ByteOrder::LittleEndian),
            "MM" => Some(ByteOrder::BigEndian),
            // Try to figure out byte order from the 42 constant later
            _ => None
        };

        // Read 42 constant
        let byte_order = match byte_order {
            Some(_) => {
                self.reader.read_bits(16)?;

                byte_order.unwrap()
            }
            None => {
                let forty_two = self.reader.read_bits(16)?;

                if forty_two == 42 {
                    ByteOrder::LittleEndian
                } else if forty_two.swap_bytes() == 42 {
                    ByteOrder::BigEndian
                } else {
                    // Something is very wrong, let's warn and assume little-endian
                    eprintln!("Invalid 42 constant in Exif header, assuming little-endian byte order");
                    ByteOrder::LittleEndian
                }
            }
        };

        // Read first IFD offset
        let first_ifd_offset = match byte_order {
            ByteOrder::LittleEndian => self.reader.read_bits(32)?,
            ByteOrder::BigEndian => self.reader.read_bits(32)?.swap_bytes(),
        };

        // Read IFD entries
        let mut ifd_entries = Vec::new();

        // Seek to first IFD
        self.reader.seek(std::io::SeekFrom::Start(first_ifd_offset as u64))?;

        // Read number of IFD entries
        let num_entries = match byte_order {
            ByteOrder::LittleEndian => self.reader.read_bits(16)?,
            ByteOrder::BigEndian => self.reader.read_bits(16)?.swap_bytes(),
        };

        // Read each IFD entry
        for _ in 0..num_entries {
            let tag = match byte_order {
                ByteOrder::LittleEndian => self.reader.read_u16()?,
                ByteOrder::BigEndian => self.reader.read_u16()?.swap_bytes(),
            };

            let format = match byte_order {
                ByteOrder::LittleEndian => self.reader.read_u16()?,
                ByteOrder::BigEndian => self.reader.read_u16()?.swap_bytes(),
            };

            let components = match byte_order {
                ByteOrder::LittleEndian => self.reader.read_bits(32)?,
                ByteOrder::BigEndian => self.reader.read_bits(32)?.swap_bytes(),
            };

            let value_offset = match byte_order {
                ByteOrder::LittleEndian => self.reader.read_bits(32)?,
                ByteOrder::BigEndian => self.reader.read_bits(32)?.swap_bytes(),
            };

            ifd_entries.push(IFDEntry {
                tag,
                format,
                components,
                value_offset,
            });
        }

        self.exif_header = Some(ExifHeader {
            identifier,
            byte_order,
            first_ifd_offset,
            ifd_entries,
        });

        Ok(())
    }

    fn read_sof_info(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        self.reader.read_bits(16)?;

        // Read precision (1 byte)
        self.precision = self.reader.read_bits(8)? as u8;

        // Read height and width (2 bytes each)
        self.height = self.reader.read_bits(16)?;
        self.width = self.reader.read_bits(16)?;

        if self.height == 0 || self.width == 0 {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid image dimensions"));
        }

        self.mcu_width = (self.width + 7) / 8;
        self.mcu_height = (self.height + 7) / 8;
        self.mcu_r_width = self.mcu_width;
        self.mcu_r_height = self.mcu_height;

        // Read number of components (1 byte)
        self.component_count = self.reader.read_bits(8)? as u8;

        // Read component info
        self.components.clear();
        for _ in 0..self.component_count {
            let id = self.reader.read_bits(8)? as u8;
            let sampling_factors = self.reader.read_bits(8)? as u8;
            let horizontal_sampling_factor = (sampling_factors >> 4) & 0xF;
            let vertical_sampling_factor = sampling_factors & 0xF;
            let quantization_table_id = self.reader.read_bits(8)? as u8;

            if id == 1 {
                if horizontal_sampling_factor == 2 && self.mcu_width % 2 == 1 {
                    self.mcu_r_width += 1;
                }

                if vertical_sampling_factor == 2 && self.mcu_height % 2 == 1 {
                    self.mcu_r_height += 1;
                }

                self.horizontal_sampling_factor = horizontal_sampling_factor;
                self.vertical_sampling_factor = vertical_sampling_factor;
            }

            self.components.push(ColorComponentInfo {
                id,
                horizontal_sampling_factor,
                vertical_sampling_factor,
                quantization_table_id,
                dc_table_selector: 0,
                ac_table_selector: 0,
            });
        }

        Ok(())
    }

    fn read_restart_interval(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        self.reader.read_bits(16)?;

        // Read restart interval (2 bytes)
        self.restart_interval = self.reader.read_bits(16)? as u16;

        Ok(())
    }

    fn read_quantization_table(&mut self) -> Result<(), Error> {
        let mut table_length = self.reader.read_bits(16)? as i16;
        table_length -= 2; // Subtract 2 bytes for the length field itself

        while table_length > 0 {
            let mut table = Vec::new();
            let table_spec = self.reader.read_bits(8)?;
            let id = (table_spec & 0x0F) as u8;
            let precision = ((table_spec >> 4) & 0x0F) as u8;

            table_length -= 1;

            if precision == 0 {
                for _ in 0..64 {
                    table.push(self.reader.read_bits(8)? as u16);
                }
                table_length -= 64;
            } else {
                for _ in 0..64 {
                    table.push(self.reader.read_bits(16)? as u16);
                }
                table_length -= 128;
            }

            self.quantization_tables.push(QuantizationTable {
                id,
                precision,
                length: 0,
                table: Self::unzigzag_block(&table.as_slice()).to_vec(),
            });
        }

        Ok(())
    }

    fn unzigzag_block(block: &[u16]) -> [u16; 64] {
        let mut unzigzagged = [0u16; 64];

        for i in 0..64 {
            unzigzagged[ZIGZAG_MAP[i] as usize] = block[i];
        }

        unzigzagged
    }

    fn read_huffman_table(&mut self) -> Result<(), Error> {
        let mut table_length = self.reader.read_bits(16)? as i16;

        while table_length > 0 {
            let table_spec = self.reader.read_bits(8)?;
            let id = (table_spec & 0x0F) as u8;
            let class = ((table_spec >> 4) & 0x0F) as u8;

            let mut offsets = Vec::with_capacity(17);
            let mut total_symbols: u32 = 0;

            offsets.push(0);
            for _ in 1..17 {
                let offset = self.reader.read_bits(8)?;
                total_symbols += offset;
                offsets.push(total_symbols);
            }

            if total_symbols > 162 {
                return Err(Error::new(ErrorKind::InvalidData, "Too many symbols in Huffman table"));
            }

            let mut table = Vec::with_capacity(total_symbols as usize);
            for _ in 0..total_symbols {
                table.push(self.reader.read_bits(8)? as u8);
            }

            table_length -= 2 + 1 + 1 + 16 + total_symbols as i16;

            let huffman_table = HuffmanTable {
                id,
                class,
                offsets,
                symbols: table,
                codes: vec![0; 162],
            };

            match class {
                0 => self.dc_huffman_tables.push(huffman_table),
                1 => self.ac_huffman_tables.push(huffman_table),
                _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid Huffman table class")),
            }
        }

        Ok(())
    }

    fn read_dac(&mut self) -> Result<(), Error> {
        // Read length of DAC segment (includes the length bytes)
        let mut data_length = self.reader.read_bits(16)? as usize - 2;

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();

        // Read tables while we have data
        while data_length > 0 {
            // Read table class and identifier
            let table_info = self.reader.read_bits(8)? as u8;
            let table_class = (table_info >> 4) & 0x0F;  // Upper 4 bits
            let identifier = table_info & 0x0F;         // Lower 4 bits

            // Read conditioning values
            let value = self.reader.read_bits(8)? as u8;

            // For DC tables (class 0), the value represents:
            // - Lower 4 bits: Conditioning length (Li)
            // - Upper 4 bits: Conditioning value (Vi)
            // For AC tables (class 1), all 8 bits are the value
            let (value, length) = if table_class == 0 {
                ((value >> 4) & 0x0F, value & 0x0F)
            } else {
                (value, 0) // AC tables don't use length
            };

            let ac_value = ArithmeticCodingValue {
                value,
                length,
            };

            let table = ArithmeticCodingTable {
                table_class,
                identifier,
                values: Vec::from([ac_value]),
            };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid DAC table class")),
            }

            data_length -= 2;
        }

        self.ac_arithmetic_tables = ac_tables;
        self.dc_arithmetic_tables = dc_tables;

        Ok(())
    }

    fn read_sos_info(&mut self) -> Result<(), Error> {
        // Read marker length (2 bytes)
        let length = self.reader.read_bits(16)?;

        // Read number of components in scan
        let scan_component_count = self.reader.read_bits(8)? as u8;

        // Read component info
        for _ in 0..scan_component_count {
            let component_selector = self.reader.read_bits(8)? as u8;

            let color_component = self.components.iter_mut()
                .find(|c| c.id == component_selector)
                .ok_or(Error::new(ErrorKind::InvalidData, "Invalid component selector"))?;

            let table_selectors = self.reader.read_bits(8)? as u8;
            let ac_table_selector = table_selectors & 0xF;
            let dc_table_selector = (table_selectors >> 4) & 0xF;

            color_component.dc_table_selector = dc_table_selector;
            color_component.ac_table_selector = ac_table_selector;
        }

        // Read spectral selection start and end
        self.start_of_spectral_selection = self.reader.read_bits(8)? as u8;
        self.end_of_spectral_selection = self.reader.read_bits(8)? as u8;

        // Read successive approximation bit positions
        let successive_approximation = self.reader.read_bits(8)? as u8;
        self.successive_approximation_high = (successive_approximation >> 4) & 0xF;
        self.successive_approximation_low = successive_approximation & 0xF;

        if length != 2 + 1 + scan_component_count as u32 * 2 + 3 {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid SOS segment length"));
        }

        Ok(())
    }

    fn read_sos_bitstream(&mut self) -> Result<(), Error> {
        let mut current_byte = self.reader.read_bits(8)? as u8;

        loop {
            // This can be either a marker or literal data
            if current_byte == 0xFF {
                let next_byte = self.reader.read_bits(8)? as u8;

                // This is a marker
                if next_byte != 0x00 {
                    // End of image marker
                    if next_byte == (JpegMarker::EOI.to_u16() & 0xFF) as u8 {
                        break;
                    }

                    // Restart marker
                    if next_byte >= (JpegMarker::RST0.to_u16() & 0xFF) as u8 && next_byte <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 {
                        current_byte = self.reader.read_bits(8)? as u8;
                        continue;
                    }

                    // Another FF
                    if next_byte == 0xFF {
                        current_byte = next_byte;
                        continue;
                    }

                    // Invalid marker
                    return Err(Error::new(ErrorKind::InvalidData, "Invalid marker found in bitstream"));
                }

                // This is a stuffed byte
                if next_byte == 0x00 {
                    self.data.push(current_byte);

                    current_byte = self.reader.read_bits(8)? as u8;
                    continue;
                }
            } else {
                self.data.push(current_byte);
                current_byte = self.reader.read_bits(8)? as u8;
            }
        }

        Ok(())
    }

    fn get_next_symbol(&mut self, reader: &mut BitReader<Cursor<Vec<u8>>>, table: &HuffmanTable) -> Result<u8, Error> {
        let mut code = 0;

        for i in 0..16 {
            let bit = reader.read_bits(1)?;
            code = (code << 1) | bit;

            for j in table.offsets[i]..table.offsets[i + 1] {
                if table.codes[j as usize] == code {
                    return Ok(table.symbols[j as usize]);
                }
            }
        }


        Err(Error::new(ErrorKind::InvalidData, "Invalid Huffman code"))
    }

    fn decode_mcu(&mut self, reader: &mut BitReader<Cursor<Vec<u8>>>, mcu_component: &mut Vec<i32>, dc_table: &HuffmanTable, ac_table: &HuffmanTable, previous_dc: &mut i32) -> Result<(), Error> {
        let length = self.get_next_symbol(reader, dc_table)?;
        let mut coefficient = reader.read_bits(length)? as i32;

        if length != 0 && coefficient < (1 << (length - 1)) {
            coefficient -= (1 << length) - 1;
        }

        mcu_component[0] = coefficient as i32 + *previous_dc;
        *previous_dc = mcu_component[0];

        let mut i = 1;
        while i < 64 {
            let symbol = self.get_next_symbol(reader, ac_table)?;

            if symbol == 0 {
                for _ in i..64 {
                    mcu_component[ZIGZAG_MAP[i] as usize] = 0;
                }

                return Ok(());
            }

            let mut zero_count = symbol >> 4;
            let coefficient_length = (symbol & 0xF) as u8;
            coefficient = 0;

            if symbol == 0xF0 {
                zero_count = 16;
            }

            if i + zero_count as usize >= 64 {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid AC coefficient"));
            }

            for _ in 0..zero_count {
                mcu_component[ZIGZAG_MAP[i] as usize] = 0;
                i += 1;
            }

            if coefficient_length > 10 {
                return Err(Error::new(ErrorKind::InvalidData, "Invalid AC coefficient length"));
            }

            if coefficient_length != 0 {
                coefficient = reader.read_bits(coefficient_length)? as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                mcu_component[ZIGZAG_MAP[i] as usize] = coefficient as i32;
                i += 1;
            }
        }

        Ok(())
    }

    fn create_huffman_codes(&mut self) {
        for table in &mut self.dc_huffman_tables {
            let mut code = 0;
            for i in 0..16 {
                for k in table.offsets[i]..table.offsets[i + 1] {
                    table.codes[k as usize] = code;
                    code += 1;
                }

                code <<= 1;
            }
        }

        for table in &mut self.ac_huffman_tables {
            let mut code = 0;
            for i in 0..16 {
                for k in table.offsets[i]..table.offsets[i + 1] {
                    table.codes[k as usize] = code;
                    code += 1;
                }

                code <<= 1;
            }
        }
    }

    fn decode_huffman(&mut self) -> Result<Vec<MCU>, Error> {
        let mut mcus = Vec::new();

        mcus.resize((self.mcu_r_height * self.mcu_r_width) as usize, MCU {
            y: vec![0; 64],
            cb: vec![0; 64],
            cr: vec![0; 64],
        });

        self.create_huffman_codes();

        // TODO: cloning this is very inefficient
        let mut reader = BitReader::new(Cursor::new(self.data.clone()));

        let mut previous_dc = [0i32; 3];
        let restart_interval = self.restart_interval * self.horizontal_sampling_factor as u16 * self.vertical_sampling_factor as u16;

        for y in (0..self.mcu_height).step_by(self.vertical_sampling_factor as usize) {
            for x in (0..self.mcu_width).step_by(self.horizontal_sampling_factor as usize) {
                if restart_interval > 0 && (y * self.mcu_r_width + x) % restart_interval as u32 == 0 {
                    previous_dc = [0; 3];
                    reader.clear_buffer();
                }

                for i in 0..self.component_count {
                    for v in 0..self.components[i as usize].vertical_sampling_factor {
                        for h in 0..self.components[i as usize].horizontal_sampling_factor {
                            let dc_table = self.dc_huffman_tables[self.components[i as usize].dc_table_selector as usize].clone();
                            let ac_table = self.ac_huffman_tables[self.components[i as usize].ac_table_selector as usize].clone();

                            let mcu_index = ((y + v as u32) * self.mcu_r_width + x + h as u32) as usize;
                            let mcu_component = match i {
                                0 => &mut mcus[mcu_index].y,
                                1 => &mut mcus[mcu_index].cb,
                                2 => &mut mcus[mcu_index].cr,
                                _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid component index when decoding huffman")),
                            };

                            self.decode_mcu(&mut reader, mcu_component, &dc_table, &ac_table, &mut previous_dc[i as usize])?;
                        }
                    }
                }
            }
        }

        Ok(mcus)
    }

    fn dequantize(&self, mcus: &mut Vec<MCU>) -> Result<(), Error> {
        for y in (0..self.mcu_height).step_by(self.vertical_sampling_factor as usize) {
            for x in (0..self.mcu_width).step_by(self.horizontal_sampling_factor as usize) {
                for i in 0..self.component_count {
                    for v in 0..self.components[i as usize].vertical_sampling_factor {
                        for h in 0..self.components[i as usize].horizontal_sampling_factor {
                            let mcu_index = ((y + v as u32) * self.mcu_r_width + x + h as u32) as usize;
                            let mcu = &mut mcus[mcu_index];

                            let quantization_table = self.quantization_tables.iter()
                                .find(|q| q.id == self.components[i as usize].quantization_table_id)
                                .ok_or(Error::new(ErrorKind::InvalidData, "Invalid quantization table"))?;

                            let mcu_component = match i {
                                0 => &mut mcu.y,
                                1 => &mut mcu.cb,
                                2 => &mut mcu.cr,
                                _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid component index when dequantizing")),
                            };

                            for j in 0..64 {
                                mcu_component[j] *= quantization_table.table[j] as i32;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn inverse_dct(&self, mcus: &mut Vec<MCU>) -> Result<(), Error> {
        let m_0 = 2.0 * (1.0 / 16.0 * 2.0 * PI).cos();
        let m_1 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_3 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_5 = 2.0 * (3.0 / 16.0 * 2.0 * PI).cos();
        let m_2 = m_0 - m_5;
        let m_4 = m_0 + m_5;

        let s_0 = (0.0 / 16.0 * PI).cos() / 8.0_f32.sqrt();
        let s_1 = (1.0 / 16.0 * PI).cos() / 2.0;
        let s_2 = (2.0 / 16.0 * PI).cos() / 2.0;
        let s_3 = (3.0 / 16.0 * PI).cos() / 2.0;
        let s_4 = (4.0 / 16.0 * PI).cos() / 2.0;
        let s_5 = (5.0 / 16.0 * PI).cos() / 2.0;
        let s_6 = (6.0 / 16.0 * PI).cos() / 2.0;
        let s_7 = (7.0 / 16.0 * PI).cos() / 2.0;

        for y in (0..self.mcu_height).step_by(self.vertical_sampling_factor as usize) {
            for x in (0..self.mcu_width).step_by(self.horizontal_sampling_factor as usize) {
                for i in 0..self.component_count {
                    for v in 0..self.components[i as usize].vertical_sampling_factor {
                        for h in 0..self.components[i as usize].horizontal_sampling_factor {
                            let mcu_index = ((y + v as u32) * self.mcu_r_width + x + h as u32) as usize;
                            let mcu = &mut mcus[mcu_index];

                            let mcu_component = match i {
                                0 => &mut mcu.y,
                                1 => &mut mcu.cb,
                                2 => &mut mcu.cr,
                                _ => return Err(Error::new(ErrorKind::InvalidData, "Invalid component index when performing inverse DCT")),
                            };

                            let mut temp_components = [0.0; 64];

                            // Process columns
                            for col in 0..8 {
                                let g_0 = mcu_component[0 * 8 + col] as f32 * s_0;
                                let g_1 = mcu_component[4 * 8 + col] as f32 * s_4;
                                let g_2 = mcu_component[2 * 8 + col] as f32 * s_2;
                                let g_3 = mcu_component[6 * 8 + col] as f32 * s_6;
                                let g_4 = mcu_component[5 * 8 + col] as f32 * s_5;
                                let g_5 = mcu_component[1 * 8 + col] as f32 * s_1;
                                let g_6 = mcu_component[7 * 8 + col] as f32 * s_7;
                                let g_7 = mcu_component[3 * 8 + col] as f32 * s_3;

                                let f_0 = g_0;
                                let f_1 = g_1;
                                let f_2 = g_2;
                                let f_3 = g_3;
                                let f_4 = g_4 - g_7;
                                let f_5 = g_5 + g_6;
                                let f_6 = g_5 - g_6;
                                let f_7 = g_4 + g_7;

                                let e_0 = f_0;
                                let e_1 = f_1;
                                let e_2 = f_2 - f_3;
                                let e_3 = f_2 + f_3;
                                let e_4 = f_4;
                                let e_5 = f_5 - f_7;
                                let e_6 = f_6;
                                let e_7 = f_5 + f_7;
                                let e_8 = f_4 + f_6;

                                let d_0 = e_0;
                                let d_1 = e_1;
                                let d_2 = e_2 * m_1;
                                let d_3 = e_3;
                                let d_4 = e_4 * m_2;
                                let d_5 = e_5 * m_3;
                                let d_6 = e_6 * m_4;
                                let d_7 = e_7;
                                let d_8 = e_8 * m_5;

                                let c_0 = d_0 + d_1;
                                let c_1 = d_0 - d_1;
                                let c_2 = d_2 - d_3;
                                let c_3 = d_3;
                                let c_4 = d_4 + d_8;
                                let c_5 = d_5 + d_7;
                                let c_6 = d_6 - d_8;
                                let c_7 = d_7;
                                let c_8 = c_5 - c_6;

                                let b_0 = c_0 + c_3;
                                let b_1 = c_1 + c_2;
                                let b_2 = c_1 - c_2;
                                let b_3 = c_0 - c_3;
                                let b_4 = c_4 - c_8;
                                let b_5 = c_8;
                                let b_6 = c_6 - c_7;
                                let b_7 = c_7;

                                temp_components[0 * 8 + col] = b_0 + b_7;
                                temp_components[1 * 8 + col] = b_1 + b_6;
                                temp_components[2 * 8 + col] = b_2 + b_5;
                                temp_components[3 * 8 + col] = b_3 + b_4;
                                temp_components[4 * 8 + col] = b_3 - b_4;
                                temp_components[5 * 8 + col] = b_2 - b_5;
                                temp_components[6 * 8 + col] = b_1 - b_6;
                                temp_components[7 * 8 + col] = b_0 - b_7;
                            }

                            // Process rows
                            for row in 0..8 {
                                let g_0 = temp_components[row * 8 + 0] * s_0;
                                let g_1 = temp_components[row * 8 + 4] * s_4;
                                let g_2 = temp_components[row * 8 + 2] * s_2;
                                let g_3 = temp_components[row * 8 + 6] * s_6;
                                let g_4 = temp_components[row * 8 + 5] * s_5;
                                let g_5 = temp_components[row * 8 + 1] * s_1;
                                let g_6 = temp_components[row * 8 + 7] * s_7;
                                let g_7 = temp_components[row * 8 + 3] * s_3;

                                let f_0 = g_0;
                                let f_1 = g_1;
                                let f_2 = g_2;
                                let f_3 = g_3;
                                let f_4 = g_4 - g_7;
                                let f_5 = g_5 + g_6;
                                let f_6 = g_5 - g_6;
                                let f_7 = g_4 + g_7;

                                let e_0 = f_0;
                                let e_1 = f_1;
                                let e_2 = f_2 - f_3;
                                let e_3 = f_2 + f_3;
                                let e_4 = f_4;
                                let e_5 = f_5 - f_7;
                                let e_6 = f_6;
                                let e_7 = f_5 + f_7;
                                let e_8 = f_4 + f_6;

                                let d_0 = e_0;
                                let d_1 = e_1;
                                let d_2 = e_2 * m_1;
                                let d_3 = e_3;
                                let d_4 = e_4 * m_2;
                                let d_5 = e_5 * m_3;
                                let d_6 = e_6 * m_4;
                                let d_7 = e_7;
                                let d_8 = e_8 * m_5;

                                let c_0 = d_0 + d_1;
                                let c_1 = d_0 - d_1;
                                let c_2 = d_2 - d_3;
                                let c_3 = d_3;
                                let c_4 = d_4 + d_8;
                                let c_5 = d_5 + d_7;
                                let c_6 = d_6 - d_8;
                                let c_7 = d_7;
                                let c_8 = c_5 - c_6;

                                let b_0 = c_0 + c_3;
                                let b_1 = c_1 + c_2;
                                let b_2 = c_1 - c_2;
                                let b_3 = c_0 - c_3;
                                let b_4 = c_4 - c_8;
                                let b_5 = c_8;
                                let b_6 = c_6 - c_7;
                                let b_7 = c_7;

                                mcu_component[row * 8 + 0] = (b_0 + b_7 + 0.5) as i32;
                                mcu_component[row * 8 + 1] = (b_1 + b_6 + 0.5) as i32;
                                mcu_component[row * 8 + 2] = (b_2 + b_5 + 0.5) as i32;
                                mcu_component[row * 8 + 3] = (b_3 + b_4 + 0.5) as i32;
                                mcu_component[row * 8 + 4] = (b_3 - b_4 + 0.5) as i32;
                                mcu_component[row * 8 + 5] = (b_2 - b_5 + 0.5) as i32;
                                mcu_component[row * 8 + 6] = (b_1 - b_6 + 0.5) as i32;
                                mcu_component[row * 8 + 7] = (b_0 - b_7 + 0.5) as i32;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn ycbcr_to_rgb(&self, mcus: &mut Vec<MCU>) -> Result<(), Error> {
        for y in (0..self.mcu_height).step_by(self.vertical_sampling_factor as usize) {
            for x in (0..self.mcu_width).step_by(self.horizontal_sampling_factor as usize) {
                let cbcr_mcu_index = (y * self.mcu_r_width + x) as usize;
                let cbcr_mcu = mcus[cbcr_mcu_index].clone();

                for v in (0..self.vertical_sampling_factor).rev() {
                    for h in (0..self.horizontal_sampling_factor).rev() {
                        let y_mcu_index = ((y + v as u32) * self.mcu_r_width + x + h as u32) as usize;
                        let mcu = &mut mcus[y_mcu_index];

                        for y in (0..8).rev() {
                            for x in (0..8).rev() {
                                let y_index = (y * 8 + x) as usize;
                                let cbcr_row = y / self.vertical_sampling_factor + 4 * v;
                                let cbcr_col = x / self.horizontal_sampling_factor + 4 * h;
                                let cbcr_index = (cbcr_row * 8 + cbcr_col) as usize;

                                let y = mcu.y[y_index] as f32;
                                let cb = cbcr_mcu.cb[cbcr_index] as f32;
                                let cr = cbcr_mcu.cr[cbcr_index] as f32;

                                let mut r = y + 1.402 * cr + 128.0;
                                let mut g = y - 0.344136 * cb - 0.714136 * cr + 128.0;
                                let mut b = y + 1.772 * cb + 128.0;

                                r = r.max(0.0).min(255.0);
                                g = g.max(0.0).min(255.0);
                                b = b.max(0.0).min(255.0);

                                mcu.y[y_index] = r as i32;
                                mcu.cb[y_index] = g as i32;
                                mcu.cr[y_index] = b as i32;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn mcu_to_pixels(&self, mcus: &Vec<MCU>) -> Vec<u8> {
        let mut pixels = Vec::new();
        let mcu_width = (self.width + 7) / 8;

        for y in 0..self.height {
            let mcu_row = y / 8;
            let y_in_mcu = y % 8;

            for x in 0..self.width {
                let mcu_col = x / 8;
                let x_in_mcu = x % 8;

                let mcu_index = (mcu_row * mcu_width + mcu_col) as usize;
                let pixel_index = (y_in_mcu * 8 + x_in_mcu) as usize;

                let r = mcus[mcu_index].y[pixel_index] as u8;
                let g = mcus[mcu_index].cb[pixel_index] as u8;
                let b = mcus[mcu_index].cr[pixel_index] as u8;

                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
            }
        }

        pixels
    }

    pub fn decode(&mut self) -> Result<Vec<u8>, Error> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            match marker {
                Some(marker) => {
                    //log_info!("Found marker: {:?} {}");
                    //println!("Found marker: {:?}", marker);
                    match marker {
                        JpegMarker::SOI => {}
                        JpegMarker::COM => self.read_com()?,
                        JpegMarker::APP0 => self.read_app0_jfif()?,
                        JpegMarker::APP1 => self.read_app1_exif()?,
                        JpegMarker::SOF0 => self.read_sof_info()?,
                        JpegMarker::DRI => self.read_restart_interval()?,
                        JpegMarker::DQT => self.read_quantization_table()?,
                        JpegMarker::DHT => self.read_huffman_table()?,
                        JpegMarker::DAC => self.read_dac()?,
                        JpegMarker::SOS => {
                            self.read_sos_info()?;
                            self.read_sos_bitstream()?;
                        }
                        JpegMarker::EOI => {
                            break;
                        }
                        _ => {
                            println!("Unhandled marker found: {:?}", marker);
                        }
                    }
                }
                None => {
                    println!("No more markers found");
                    break;
                }
            }
        }

        let mut mcus = self.decode_huffman()?;
        self.dequantize(&mut mcus)?;
        self.inverse_dct(&mut mcus)?;
        self.ycbcr_to_rgb(&mut mcus)?;

        let pixels = self.mcu_to_pixels(&mcus);

        Ok(pixels)
    }
}
