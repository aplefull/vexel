use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::JpegInfo;
use crate::utils::marker::Marker;
use crate::utils::types::ByteOrder;
use crate::{log_debug, log_error, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use serde::Serialize;
use std::f32::consts::PI;
use std::fmt::Debug;
use std::io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom};
use tsify::Tsify;

#[derive(Clone, Debug, PartialEq)]
pub enum JpegMarker {
    // Start Of Frame markers, non-differential, Huffman coding
    SOF0, // Baseline DCT
    SOF1, // Extended sequential DCT
    SOF2, // Progressive DCT
    SOF3, // Lossless (sequential)

    // Start Of Frame markers, differential, Huffman coding
    SOF5, // Differential sequential DCT
    SOF6, // Differential progressive DCT
    SOF7, // Differential lossless (sequential)

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
    DHT, // Define Huffman table(s)

    // Arithmetic coding conditioning specification
    DAC, // Define arithmetic coding conditioning(s)

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
    SOI, // Start of image
    EOI, // End of image
    SOS, // Start of scan
    DQT, // Define quantization table(s)
    DNL, // Define number of lines
    DRI, // Define restart interval
    DHP, // Define hierarchical progression
    EXP, // Expand reference component(s)

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

    COM, // Comment

    // Special markers
    TEM, // For temporary private use in arithmetic coding

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

static JPEG_MARKERS: [JpegMarker; 64] = [
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

// Table K.1 from JPEG specification
#[rustfmt::skip]
const DEFAULT_QUANTIZATION_TABLE: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61,
    12, 12, 14, 19, 26, 58, 60, 55,
    14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62,
    18, 22, 37, 56, 68, 109, 103, 77,
    24, 35, 55, 64, 81, 104, 113, 92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103, 99,
];

#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
pub enum JpegMode {
    Baseline,
    ExtendedSequential,
    Progressive,
    Lossless,
}

#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
pub enum JpegCodingMethod {
    Huffman,
    Arithmetic,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct QuantizationTable {
    pub id: u8,
    pub precision: u8,
    pub length: u16,
    pub table: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HuffmanTable {
    pub id: u8,
    pub class: u8,
    pub offsets: Vec<u32>,
    pub symbols: Vec<u8>,
    pub codes: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ArithmeticCodingValue {
    pub value: u8,
    pub length: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ArithmeticCodingTable {
    pub table_class: u8,
    pub identifier: u8,
    pub values: Vec<ArithmeticCodingValue>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorComponentInfo {
    pub id: u8,
    pub horizontal_sampling_factor: u8,
    pub vertical_sampling_factor: u8,
    pub quantization_table_id: u8,
    pub dc_table_selector: u8,
    pub ac_table_selector: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JFIFHeader {
    pub identifier: String,
    pub version_major: u8,
    pub version_minor: u8,
    pub density_units: u8,
    pub x_density: u16,
    pub y_density: u16,
    pub thumbnail_width: u8,
    pub thumbnail_height: u8,
    pub thumbnail_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExifHeader {
    pub identifier: String,
    pub byte_order: ByteOrder,
    pub first_ifd_offset: u32,
    pub ifd_entries: Vec<IFDEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IFDEntry {
    pub tag: u16,
    pub format: u16,
    pub components: u32,
    pub value_offset: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ScanInfo {
    pub start_spectral: u8,
    pub end_spectral: u8,
    pub successive_high: u8,
    pub successive_low: u8,
    pub components: Vec<ScanComponent>,
    pub dc_tables: Vec<HuffmanTable>,
    pub ac_tables: Vec<HuffmanTable>,
    pub data_length: u64,
}

#[derive(Debug, Clone)]
pub struct ScanData {
    pub start_spectral: u8,
    pub end_spectral: u8,
    pub successive_high: u8,
    pub successive_low: u8,
    pub components: Vec<ScanComponent>,
    pub dc_tables: Vec<HuffmanTable>,
    pub ac_tables: Vec<HuffmanTable>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ScanComponent {
    pub component_id: u8,
    pub dc_table_selector: u8,
    pub ac_table_selector: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Predictor {
    NoPrediction = 0,
    Ra = 1,
    Rb = 2,
    Rc = 3,
    RaRbRc1 = 4,
    RaRbRc2 = 5,
    RaRbRc3 = 6,
    RaRb = 7,
}

struct ArithmeticDecoder {
    a: u32,  // Probability interval, kept in range 0x8000..=0xFFFF
    c: u32,  // Code register, contains bit stream being decoded
    ct: u32, // Count of available bits in code register
    cx: u32, // Most significant bits of code register

    state_table: Vec<u8>, // State transition table
    mps_table: Vec<u8>,   // MPS table
    max_context: usize,   // Maximum context size

    current_mps: u8,   // Current MPS symbol
    next_mps: u8,      // Next MPS value
    current_state: u8, // Current state
    next_state: u8,    // Next state
    qe: u16,           // Current probability

    reader: BitReader<Cursor<Vec<u8>>>,
}

impl ArithmeticDecoder {
    const QE_VALUE: [u16; 113] = [
        0x5a1d, 0x2586, 0x1114, 0x080b, 0x03d8, 0x01da, 0x0015, 0x006f, 0x0036, 0x001a, 0x000d, 0x0006, 0x0003, 0x0001,
        0x5a7f, 0x3f25, 0x2cf2, 0x207c, 0x17b9, 0x1182, 0x0cef, 0x09a1, 0x072f, 0x055c, 0x0406, 0x0303, 0x0240, 0x01b1,
        0x0144, 0x00f5, 0x00b7, 0x008a, 0x0068, 0x004e, 0x003b, 0x002c, 0x5ae1, 0x484c, 0x3a0d, 0x2ef1, 0x261f, 0x1f33,
        0x19a8, 0x1518, 0x1177, 0x0e74, 0x0bfb, 0x09f8, 0x0861, 0x0706, 0x05cd, 0x04de, 0x040f, 0x0363, 0x02d4, 0x025c,
        0x01f8, 0x01a4, 0x0160, 0x0125, 0x00f6, 0x00cb, 0x00ab, 0x008f, 0x5b12, 0x4d04, 0x412c, 0x37d8, 0x2fe8, 0x293c,
        0x2379, 0x1edf, 0x1aa9, 0x174e, 0x1424, 0x119c, 0x0f6b, 0x0d51, 0x0bb6, 0x0a40, 0x5832, 0x4d1c, 0x438e, 0x3bdd,
        0x34ee, 0x2eae, 0x299a, 0x2516, 0x5570, 0x4ca9, 0x44d9, 0x3e22, 0x3824, 0x32b4, 0x2e17, 0x56a8, 0x4f46, 0x47e5,
        0x41cf, 0x3c3d, 0x375e, 0x5231, 0x4c0f, 0x4639, 0x415e, 0x5627, 0x50e7, 0x4b85, 0x5597, 0x504f, 0x5a10, 0x5522,
        0x59eb,
    ];

    const QE_NEXT_LPS: [u8; 113] = [
        1, 14, 16, 18, 20, 23, 25, 28, 30, 33, 35, 9, 10, 12, 15, 36, 38, 39, 40, 42, 43, 45, 46, 48, 49, 51, 52, 54,
        56, 57, 59, 60, 62, 63, 32, 33, 37, 64, 65, 67, 68, 69, 70, 72, 73, 74, 75, 77, 78, 79, 48, 50, 50, 51, 52, 53,
        54, 55, 56, 57, 58, 59, 61, 61, 65, 80, 81, 82, 83, 84, 86, 87, 87, 72, 72, 74, 74, 75, 77, 77, 80, 88, 89, 90,
        91, 92, 93, 86, 88, 95, 96, 97, 99, 99, 93, 95, 101, 102, 103, 104, 99, 105, 106, 107, 103, 105, 108, 109, 110,
        111, 110, 112, 112,
    ];

    const QE_NEXT_MPS: [u8; 113] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 13, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
        31, 32, 33, 34, 35, 9, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58,
        59, 60, 61, 62, 63, 32, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 48, 81, 82, 83, 84, 85, 86,
        87, 71, 89, 90, 91, 92, 93, 94, 86, 96, 97, 98, 99, 100, 93, 102, 103, 104, 99, 106, 107, 103, 109, 107, 111,
        109, 111,
    ];

    const QE_SWITCH: [u8; 113] = [
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1,
        0, 1,
    ];

    fn new(reader: BitReader<Cursor<Vec<u8>>>) -> Self {
        Self {
            a: 0,
            c: 0,
            ct: 0,
            cx: 0,
            state_table: vec![0; 4096],
            mps_table: vec![0; 4096],
            max_context: 4096,
            current_mps: 0,
            next_mps: 0,
            current_state: 0,
            next_state: 0,
            qe: 0,
            reader,
        }
    }

    fn init(&mut self) {
        self.a = 0x10000;
        self.c = 0;
        self.ct = 0;

        self.byte_in();
        self.c = self.c << 8;
        self.byte_in();
        self.c = self.c << 8;

        self.ct = 0;
        self.cx = (self.c & 0xffff0000) >> 16;
    }

    fn cond_lps_exchange(&mut self) -> u8 {
        let d;

        if self.a < self.qe as u32 {
            d = self.current_mps;
            self.cx = self.cx.wrapping_sub(self.a);
            let c_low = self.c & 0x0000ffff;
            self.c = ((self.cx) << 16) + c_low;
            self.a = self.qe as u32;
            self.next_state = Self::QE_NEXT_MPS[self.current_state as usize];
        } else {
            d = 1 - self.current_mps;
            self.cx = self.cx.wrapping_sub(self.a);
            let c_low = self.c & 0x0000ffff;
            self.c = ((self.cx) << 16) + c_low;
            self.a = self.qe as u32;

            if Self::QE_SWITCH[self.current_state as usize] == 1 {
                self.next_mps = 1 - self.current_mps;
            }
            self.next_state = Self::QE_NEXT_LPS[self.current_state as usize];
        }

        d
    }

    fn cond_mps_exchange(&mut self) -> u8 {
        if self.a < self.qe as u32 {
            let d = 1 - self.current_mps;
            if Self::QE_SWITCH[self.current_state as usize] == 1 {
                self.next_mps = 1 - self.current_mps;
            }
            self.next_state = Self::QE_NEXT_LPS[self.current_state as usize];
            d
        } else {
            let d = self.current_mps;
            self.next_state = Self::QE_NEXT_MPS[self.current_state as usize];
            d
        }
    }

    fn renorm_d(&mut self) {
        while self.a < 0x8000 {
            if self.ct == 0 {
                self.byte_in();
                self.ct = 8;
            }
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
        }

        self.cx = (self.c & 0xffff0000) >> 16;
    }

    fn decode_symbol(&mut self) -> u8 {
        self.a = self.a.wrapping_sub(self.qe as u32);

        if self.cx < self.a {
            if self.a < 0x8000 {
                let d = self.cond_mps_exchange();
                self.renorm_d();
                d
            } else {
                self.current_mps
            }
        } else {
            let d = self.cond_lps_exchange();
            self.renorm_d();
            d
        }
    }

    fn decode(&mut self, s: usize) -> bool {
        if s >= self.max_context {
            let new_size = self.max_context * 2;
            let mut new_st = vec![0; new_size];
            let mut new_mps = vec![0; new_size];

            new_st[..self.max_context].copy_from_slice(&self.state_table);
            new_mps[..self.max_context].copy_from_slice(&self.mps_table);

            self.max_context = new_size;
            self.state_table = new_st;
            self.mps_table = new_mps;
        }

        self.next_state = self.state_table[s];
        self.current_state = self.state_table[s];
        self.next_mps = self.mps_table[s];
        self.current_mps = self.mps_table[s];
        self.qe = Self::QE_VALUE[self.state_table[s] as usize];

        let ret_val = self.decode_symbol();

        self.state_table[s] = self.next_state;
        self.mps_table[s] = self.next_mps;
        log_debug!(
            "State: {}, MPS: {}, QE: {}, Decision: {}",
            self.next_state,
            self.next_mps,
            self.qe,
            ret_val
        );
        ret_val != 0
    }

    fn byte_in(&mut self) {
        match self.reader.read_u8() {
            Ok(b) => {
                if b == 0xFF {
                    match self.reader.read_u8() {
                        Ok(0x00) => self.c |= 0xFF00,
                        Ok(_) => (),
                        Err(_) => (),
                    }
                } else {
                    self.c += (b as u32) << 8;
                }
            }
            Err(e) => {
                log_error!("Error reading byte: {:?}", e);
            }
        }
    }
}

// Function to test arithmetic decoder
fn run_test_sequence() {
    let test_data = vec![
        0x65, 0x5B, 0x51, 0x44, 0xF7, 0x96, 0x9D, 0x51, 0x78, 0x55, 0xBF, 0xFF, 0x00, 0xFC, 0x51, 0x84, 0xC7, 0xCE,
        0xF9, 0x39, 0x00, 0x28, 0x7D, 0x46, 0x70, 0x8E, 0xCB, 0xC0, 0xF6, 0xFF, 0xD9, 0x00,
    ];

    let expected = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1,
        0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1,
        0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1,
        1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1,
        1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0,
    ];

    //println!("Test data: {:?}", test_data);
    let reader = BitReader::new(Cursor::new(test_data));
    let mut decoder = ArithmeticDecoder::new(reader);
    decoder.init();

    for i in 0..expected.len() {
        let d = decoder.decode(0);
        //println!("{}: {}", i, d as usize);
        assert_eq!(d as usize, expected[i]);
    }

    println!("Test passed!");
}

#[derive(Debug, Clone)]
struct UpsampledPlane {
    data: Vec<i32>,
    width: u32,
    height: u32,
}

impl UpsampledPlane {
    fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![0; (width * height) as usize],
            width,
            height,
        }
    }

    fn get_pixel(&self, x: u32, y: u32) -> Option<i32> {
        if x < self.width && y < self.height {
            Some(self.data[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, value: i32) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = value;
        }
    }
}

#[derive(Debug, Clone)]
struct ComponentPlane {
    id: u8,
    h_samp: u8,
    v_samp: u8,
    data: Vec<i32>,
    width: u32,
    height: u32,
    blocks_per_line: u32,
}

impl ComponentPlane {
    fn new(width: u32, height: u32, h_samp: u8, v_samp: u8, id: u8) -> Self {
        let blocks_per_line = (width + 7) / 8;
        let block_lines = (height + 7) / 8;

        Self {
            id,
            h_samp,
            v_samp,
            width,
            height,
            blocks_per_line,
            data: vec![0; (blocks_per_line * block_lines * 64) as usize],
        }
    }

    fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i32]> {
        let block_idx = block_y * self.blocks_per_line + block_x;
        let start = (block_idx * 64) as usize;
        if start + 64 <= self.data.len() {
            Some(&mut self.data[start..start + 64])
        } else {
            None
        }
    }

    fn upsample(&self, target_width: u32, target_height: u32) -> UpsampledPlane {
        let mut upsampled = UpsampledPlane::new(target_width, target_height);

        // First, create an intermediate buffer of source pixels (not in blocks)
        let mut source_pixels = vec![0i32; (self.width * self.height) as usize];

        // Convert from block format to pixel format
        let blocks_per_line = (self.width + 7) / 8;
        for by in 0..((self.height + 7) / 8) {
            for bx in 0..blocks_per_line {
                let block_idx = (by * blocks_per_line + bx) as usize * 64;

                // Process each pixel in the block
                for py in 0..8 {
                    let y = by * 8 + py;
                    if y >= self.height {
                        continue;
                    }

                    for px in 0..8 {
                        let x = bx * 8 + px;
                        if x >= self.width {
                            continue;
                        }

                        let pixel_idx = (y * self.width + x) as usize;
                        let block_pixel_idx = block_idx + (py * 8 + px) as usize;

                        if block_pixel_idx < self.data.len() {
                            source_pixels[pixel_idx] = self.data[block_pixel_idx];
                        }
                    }
                }
            }
        }

        // Now perform the actual upsampling from the intermediate buffer
        for y in 0..target_height {
            for x in 0..target_width {
                // Calculate source coordinates
                let src_x = (x * self.width / target_width) as usize;
                let src_y = (y * self.height / target_height) as usize;

                let src_idx = src_y * self.width as usize + src_x;
                if src_idx < source_pixels.len() {
                    upsampled.set_pixel(x, y, source_pixels[src_idx]);
                }
            }
        }

        upsampled
    }
}

pub struct JpegDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    jfif_header: Option<JFIFHeader>,
    exif_header: Option<ExifHeader>,
    comments: Vec<String>,
    mode: JpegMode,
    coding_method: JpegCodingMethod,
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
    mcu_width: u32,
    mcu_height: u32,
    precision: u8,
    component_count: u8,
    components: Vec<ColorComponentInfo>,
    scans: Vec<ScanData>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    // TODO remove redundant fields, that are duplicated in scans
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            comments: Vec::new(),
            jfif_header: None,
            exif_header: None,
            mode: JpegMode::Baseline,
            coding_method: JpegCodingMethod::Huffman,
            mcu_width: 0,
            mcu_height: 0,
            precision: 0,
            component_count: 0,
            start_of_spectral_selection: 0,
            end_of_spectral_selection: 0,
            successive_approximation_high: 0,
            successive_approximation_low: 0,
            components: Vec::new(),
            quantization_tables: Vec::new(),
            ac_huffman_tables: Vec::new(),
            dc_huffman_tables: Vec::new(),
            ac_arithmetic_tables: Vec::new(),
            dc_arithmetic_tables: Vec::new(),
            horizontal_sampling_factor: 1,
            vertical_sampling_factor: 1,
            restart_interval: 0,
            scans: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> JpegInfo {
        JpegInfo {
            width: self.width,
            height: self.height,
            color_depth: self.precision,
            number_of_components: self.component_count,
            mode: self.mode.clone(),
            coding_method: self.coding_method.clone(),
            jfif_header: self.jfif_header.clone(),
            exif_header: self.exif_header.clone(),
            quantization_tables: self.quantization_tables.clone(),
            ac_arithmetic_tables: self.ac_arithmetic_tables.clone(),
            dc_arithmetic_tables: self.dc_arithmetic_tables.clone(),
            color_components: self.components.clone(),
            spectral_selection_start: self.start_of_spectral_selection,
            spectral_selection_end: self.end_of_spectral_selection,
            successive_approximation_high: self.successive_approximation_high,
            successive_approximation_low: self.successive_approximation_low,
            horizontal_sampling_factor: self.horizontal_sampling_factor,
            vertical_sampling_factor: self.vertical_sampling_factor,
            restart_interval: self.restart_interval,
            comments: self.comments.clone(),
            scans: self
                .scans
                .iter()
                .map(|scan| ScanInfo {
                    start_spectral: scan.start_spectral,
                    end_spectral: scan.end_spectral,
                    successive_high: scan.successive_high,
                    successive_low: scan.successive_low,
                    components: scan.components.clone(),
                    dc_tables: scan.dc_tables.clone(),
                    ac_tables: scan.ac_tables.clone(),
                    data_length: scan.data.len() as u64,
                })
                .collect(),
        }
    }

    fn skip_unknown_marker_segment(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()? as usize;

        for _ in 0..(length - 2) {
            self.reader.read_u8()?;
        }

        Ok(())
    }

    fn read_com(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let mut comment_bytes = Vec::new();
        for _ in 0..length - 2 {
            comment_bytes.push(self.reader.read_u8()?);
        }

        let text = String::from_utf8_lossy(&comment_bytes).to_string();

        self.comments.push(text);

        Ok(())
    }

    fn read_app0_jfif(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let mut identifier = Vec::new();
        for _ in 0..5 {
            identifier.push(self.reader.read_u8()?);
        }

        let identifier = String::from_utf8_lossy(&identifier).to_string();

        if identifier != "JFIF\0" {
            log_warn!(
                "Invalid JFIF identifier in APP0, might not be a JFIF header: {}",
                identifier
            );
        }

        let version_major = self.reader.read_bits(8)? as u8;
        let version_minor = self.reader.read_bits(8)? as u8;

        let density_units = self.reader.read_bits(8)? as u8;
        let x_density = self.reader.read_bits(16)? as u16;
        let y_density = self.reader.read_bits(16)? as u16;

        let thumbnail_width = self.reader.read_bits(8)? as u8;
        let thumbnail_height = self.reader.read_bits(8)? as u8;

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

        if length != 16 + thumbnail_size as u16 {
            log_warn!(
                "Invalid JFIF segment length, expected {}, got {}",
                16 + thumbnail_size,
                length
            );
        }

        Ok(())
    }

    fn read_app1_exif(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        for _ in 0..(length - 2) {
            self.reader.read_u8()?;
        }

        // TODO actually implement this
        return Ok(());

        // Read Exif identifier (6 bytes)
        /*let mut identifier = Vec::new();
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
                    // Something is very wrong, let's warn and assume big-endian
                    log_warn!("Invalid 42 constant in Exif header, assuming big-endian byte order");
                    ByteOrder::BigEndian
                }
            }
        };

        // Read first IFD offset
        let first_ifd_offset = match byte_order {
            ByteOrder::LittleEndian => self.reader.read_bits(32)?,
            ByteOrder::BigEndian => {
                let offset = self.reader.read_bits(32)?;
                offset.swap_bytes()
            }
        };

        // Read IFD entries
        let mut ifd_entries = Vec::new();

        // Seek to first IFD
        //self.reader.seek(std::io::SeekFrom::Current(first_ifd_offset as i64))?;

        // Read number of IFD entries
        let num_entries = match byte_order {
            ByteOrder::LittleEndian => self.reader.read_bits(16)?,
            ByteOrder::BigEndian => self.reader.read_bits(16)?.swap_bytes(),
        };

        // Read each IFD entry
        // TODO - Read all IFD entries
        for _ in 0..0 {
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

        Ok(())*/
    }

    fn read_start_of_frame(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.precision = self.reader.read_u8()?;

        if self.mode == JpegMode::Lossless {
            if self.precision < 2 || self.precision > 16 {
                log_warn!("Invalid precision for lossless jpeg mode: {}, clamping", self.precision);
                self.precision = self.precision.clamp(2, 16);
            }
        }

        self.height = self.reader.read_u16()? as u32;
        self.width = self.reader.read_u16()? as u32;

        if self.height == 0 || self.width == 0 {
            return Err(VexelError::from(Error::new(
                ErrorKind::InvalidData,
                "Invalid image dimensions",
            )));
        }

        // TODO rename them, they are not MCu dimensions, but dimensions of the image in MCUs
        self.mcu_width = (self.width + 7) / 8;
        self.mcu_height = (self.height + 7) / 8;

        self.component_count = self.reader.read_u8()?;

        if self.component_count > 4 || self.component_count == 0 {
            log_warn!(
                "Invalid number of components in SOF marker: {}, assuming 3",
                self.component_count
            );
            self.component_count = 3;
        }

        self.components.clear();

        for _ in 0..self.component_count {
            let id = self.reader.read_u8()?;
            let sampling_factors = self.reader.read_u8()?;
            let horizontal_sampling_factor = (sampling_factors >> 4) & 0xF;
            let vertical_sampling_factor = sampling_factors & 0xF;
            let quantization_table_id = self.reader.read_u8()?;

            if id == 1 {
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

        if length != 8 + 3 * self.component_count as u16 {
            log_warn!(
                "Invalid SOF marker length, expected {}, got {}",
                8 + 3 * self.component_count,
                length
            );
        }

        Ok(())
    }

    fn read_restart_interval(&mut self) -> VexelResult<()> {
        self.reader.read_u16()?;

        self.restart_interval = self.reader.read_u16()?;

        Ok(())
    }

    fn read_quantization_table(&mut self) -> VexelResult<()> {
        let mut table_length = self.reader.read_u16()? as i16;
        table_length = table_length.saturating_sub(2);

        while table_length > 0 {
            let mut table = Vec::new();
            let table_spec = self.reader.read_u8()?;
            let id = table_spec & 0x0F;
            let precision = (table_spec >> 4) & 0x0F;

            table_length = table_length.saturating_sub(1);

            if precision == 0 {
                for _ in 0..64 {
                    table.push(self.reader.read_u8()? as u16);
                }
                table_length = table_length.saturating_sub(64);
            } else {
                for _ in 0..64 {
                    table.push(self.reader.read_u16()?);
                }
                table_length = table_length.saturating_sub(128);
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
            unzigzagged[ZIGZAG_MAP[i] as usize] = block.get(i).copied().unwrap_or(0);
        }

        unzigzagged
    }

    fn read_huffman_table(&mut self) -> VexelResult<()> {
        let mut segment_length = self.reader.read_bits(16)? as i16;

        while segment_length > 0 {
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
                log_warn!("Too many symbols in Huffman table: {}", total_symbols);
                total_symbols = 162;
            }

            let mut table = Vec::with_capacity(total_symbols as usize);
            for _ in 0..total_symbols {
                table.push(self.reader.read_bits(8)? as u8);
            }

            segment_length -= 2 + 1 + 1 + 16 + total_symbols as i16;

            let mut huffman_table = HuffmanTable {
                id,
                class,
                offsets,
                symbols: table,
                codes: vec![0; 162],
            };

            // Generate codes
            let mut code = 0;
            for i in 0..16 {
                if huffman_table.offsets.len() <= i + 1 {
                    log_warn!("Offset index {} is out of bounds in Huffman table", i);
                    break;
                }

                for k in huffman_table.offsets[i]..huffman_table.offsets[i + 1] {
                    if huffman_table.codes.len() <= k as usize {
                        log_warn!("Code index {} is out of bounds in Huffman table", k);
                        break;
                    }

                    huffman_table.codes[k as usize] = code;
                    code += 1;
                }

                code <<= 1;
            }

            // We either replace the table, if we already have one with the same ID,
            // or we add a new one
            match class {
                0 => {
                    if let Some(existing_table) = self.dc_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.dc_huffman_tables.push(huffman_table);
                    }
                }
                1 => {
                    if let Some(existing_table) = self.ac_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.ac_huffman_tables.push(huffman_table);
                    }
                }
                _ => {
                    log_warn!("Invalid Huffman table class: {}, ignoring the table", class);
                }
            }
        }

        Ok(())
    }

    fn read_dac(&mut self) -> VexelResult<()> {
        let mut data_length = self.reader.read_u16()?;
        data_length -= 2;

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();

        while data_length > 0 {
            let table_info = self.reader.read_u8()?;
            let table_class = (table_info >> 4) & 0x0F;
            let identifier = table_info & 0x0F;

            let value = self.reader.read_u8()?;

            // For DC tables (class 0), the value represents:
            // - Lower 4 bits: Conditioning length (Li)
            // - Upper 4 bits: Conditioning value (Vi)
            // For AC tables (class 1), all 8 bits are the value
            let (value, length) = if table_class == 0 {
                ((value >> 4) & 0x0F, value & 0x0F)
            } else {
                (value, 0)
            };

            let ac_value = ArithmeticCodingValue { value, length };

            let table = ArithmeticCodingTable {
                table_class,
                identifier,
                values: Vec::from([ac_value]),
            };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => {
                    log_warn!(
                        "Invalid arithmetic coding table class: {}, ignoring the table",
                        table_class
                    );
                }
            }

            data_length -= 2;
        }

        self.ac_arithmetic_tables = ac_tables;
        self.dc_arithmetic_tables = dc_tables;

        Ok(())
    }

    fn read_start_of_scan(&mut self) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let scan_component_count = self.reader.read_u8()?;

        let mut scan_components = Vec::new();
        for _ in 0..scan_component_count {
            let component_selector = self.reader.read_u8()?;
            let table_selectors = self.reader.read_u8()?;

            // Create scan component with table selections
            scan_components.push(ScanComponent {
                component_id: component_selector,
                dc_table_selector: (table_selectors >> 4) & 0x0F,
                ac_table_selector: table_selectors & 0x0F,
            });

            // Update component info if it exists
            if let Some(color_component) = self.components.iter_mut().find(|c| c.id == component_selector) {
                color_component.dc_table_selector = (table_selectors >> 4) & 0x0F;
                color_component.ac_table_selector = table_selectors & 0x0F;
            }
        }

        // Read spectral selection and successive approximation
        let start_spectral = self.reader.read_u8()?;
        let end_spectral = self.reader.read_u8()?;
        let successive_approx = self.reader.read_u8()?;
        let successive_high = (successive_approx >> 4) & 0x0F;
        let successive_low = successive_approx & 0x0F;

        if length != 6 + (2 * scan_component_count as u16) {
            log_warn!(
                "Invalid SOS marker length, expected {}, got {}",
                6 + (2 * scan_component_count as u16),
                length
            );
        }

        let mut current_byte = self.reader.read_u8().unwrap_or_else(|_| {
            log_warn!("Unexpected EOF while reading first byte of scan data");
            0
        });

        let mut scan_data = Vec::new();

        // We need to preserve zero bytes in case of arithmetic coding,
        // so just push all bytes until we reach a marker
        if self.coding_method == JpegCodingMethod::Arithmetic {
            loop {
                if current_byte == 0xFF {
                    let next_byte = self.reader.read_bits(8)? as u8;

                    // This is a marker
                    if next_byte != 0x00 {
                        // End of image marker
                        if next_byte == (JpegMarker::EOI.to_u16() & 0xFF) as u8 {
                            break;
                        }

                        // Restart marker
                        if next_byte >= (JpegMarker::RST0.to_u16() & 0xFF) as u8
                            && next_byte <= (JpegMarker::RST7.to_u16() & 0xFF) as u8
                        {
                            current_byte = self.reader.read_bits(8)? as u8;
                            continue;
                        }

                        // Another FF
                        if next_byte == 0xFF {
                            current_byte = next_byte;
                            continue;
                        }

                        // Next marker is found, so it should be the end of the scan,
                        // seek back to the marker and break
                        self.reader.seek(SeekFrom::Current(-2))?;
                        break;
                    }

                    if next_byte == 0x00 {
                        scan_data.push(current_byte);
                        scan_data.push(next_byte);

                        current_byte = self.reader.read_bits(8)? as u8;
                        continue;
                    }
                } else {
                    scan_data.push(current_byte);
                    current_byte = self.reader.read_bits(8)? as u8;
                }
            }
        } else {
            loop {
                if current_byte != 0xFF {
                    // Most common case - regular data byte
                    scan_data.push(current_byte);
                    current_byte = match self.reader.read_u8() {
                        Ok(byte) => byte,
                        Err(_) => {
                            log_warn!("Unexpected EOF while reading scan data, breaking");
                            break;
                        }
                    };

                    continue;
                }

                // We have 0xFF byte, read the next one
                let next_byte = match self.reader.read_u8() {
                    Ok(byte) => byte,
                    Err(_) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                };

                match next_byte {
                    0x00 => {
                        // Stuffed byte case
                        scan_data.push(current_byte);
                        current_byte = match self.reader.read_u8() {
                            Ok(byte) => byte,
                            Err(_) => {
                                log_warn!("Unexpected EOF while reading scan data, breaking");
                                break;
                            }
                        };
                    }
                    0xFF => {
                        // Another FF, reprocess it
                        current_byte = next_byte;
                    }
                    b if b >= (JpegMarker::RST0.to_u16() & 0xFF) as u8
                        && b <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 =>
                    {
                        // Restart marker
                        current_byte = match self.reader.read_u8() {
                            Ok(byte) => byte,
                            Err(_) => {
                                log_warn!("Unexpected EOF while reading scan data, breaking");
                                break;
                            }
                        };
                    }
                    b if b == (JpegMarker::EOI.to_u16() & 0xFF) as u8 => {
                        // End of image
                        break;
                    }
                    _ => {
                        // Any other marker - end of scan
                        self.reader.seek(SeekFrom::Current(-2))?;
                        break;
                    }
                }
            }
        }

        // Create new scan with currently active tables
        let scan = ScanData {
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            components: scan_components,
            dc_tables: self.dc_huffman_tables.clone(),
            ac_tables: self.ac_huffman_tables.clone(),
            data: scan_data,
        };

        self.scans.push(scan);

        Ok(())
    }

    #[inline(always)]
    fn get_next_symbol(&self, reader: &mut BitReader<Cursor<Vec<u8>>>, table: &HuffmanTable) -> VexelResult<u8> {
        let mut code = 0;

        for i in 0..16 {
            let bit = reader.read_bit().unwrap_or_else(|_| {
                log_warn!("Failed to read bit from bit reader, replacing with 0");
                false
            }) as u32;

            code = (code << 1) | bit;

            for j in table.offsets[i] as usize..table.offsets[i + 1] as usize {
                if table.codes[j] == code {
                    return Ok(table.symbols[j]);
                }
            }
        }

        log_warn!("Invalid Huffman code: {}, replacing with 0", code);

        Ok(0)
    }

    fn decode_mcu(
        &mut self,
        reader: &mut BitReader<Cursor<Vec<u8>>>,
        mcu_component: &mut [i32],
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        previous_dc: &mut i32,
    ) -> VexelResult<()> {
        let length = self.get_next_symbol(reader, dc_table)?;

        let max_length = if self.precision > 8 { 12 } else { 11 };
        if length > max_length {
            log_warn!("Invalid DC coefficient length (>{}): {}", max_length, length);
            return Ok(());
        }

        let mut coefficient = reader.read_bits(length)? as i32;

        if length != 0 && coefficient < (1 << (length - 1)) {
            coefficient -= (1 << length) - 1;
        }

        mcu_component[0] = coefficient + *previous_dc;
        *previous_dc = mcu_component[0];

        let mut i = 1;
        while i < 64 {
            let symbol = self.get_next_symbol(reader, ac_table).unwrap_or_else(|_| {
                log_warn!("Failed to get next AC symbol during decoding, replacing with 0");
                0
            });

            if symbol == 0 {
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }

                return Ok(());
            }

            let mut zero_count = symbol >> 4;
            let mut coefficient_length = symbol & 0xF;

            if symbol == 0xF0 {
                zero_count = 16;
                coefficient_length = 0;
            }

            if i + zero_count as usize >= 64 {
                log_warn!("Sum of zero count and current index of mcu value exceeds 64");
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }
                return Ok(());
            }

            for _ in 0..zero_count {
                mcu_component[ZIGZAG_MAP[i] as usize] = 0;
                i += 1;
            }

            // For 12-bit precision, maximum AC coefficient length is 16
            let max_coefficient_length = if self.precision > 8 { 16 } else { 10 };
            if coefficient_length > max_coefficient_length {
                log_warn!("Invalid coefficient length: {}, replacing with 0", coefficient_length);
                coefficient_length = 0;
            }

            if coefficient_length != 0 {
                coefficient = reader.read_bits(coefficient_length)? as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                if mcu_component.len() <= ZIGZAG_MAP[i] as usize {
                    log_warn!("Invalid zigzag index: {}, skipping", ZIGZAG_MAP[i]);
                    i += 1;
                    continue;
                }

                mcu_component[ZIGZAG_MAP[i] as usize] = coefficient;
                i += 1;
            }
        }

        Ok(())
    }

    fn decode_progressive(&mut self) -> VexelResult<Image> {
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        // Create component planes at their native resolutions
        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                // Calculate dimensions in samples (pixels)
                let comp_width =
                    (self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1) / max_h_samp as u32;
                let comp_height =
                    (self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1) / max_v_samp as u32;

                ComponentPlane::new(
                    comp_width,
                    comp_height,
                    comp.horizontal_sampling_factor,
                    comp.vertical_sampling_factor,
                    comp.id,
                )
            })
            .collect();

        self.decode_progressive_scans(&mut component_planes)?;
        self.dequantize_planes(&mut component_planes)?;
        self.inverse_dct_planes(&mut component_planes)?;

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut previous_dc = vec![0i32; planes.len()];

        for scan in &self.scans {
            let mut reader = BitReader::new(Cursor::new(scan.data.clone()));
            let mut skips = 0;
            let restart_interval = self.restart_interval;

            let mut max_h_samp = self
                .components
                .iter()
                .map(|c| c.horizontal_sampling_factor)
                .max()
                .unwrap_or(1);
            let mut max_v_samp = self
                .components
                .iter()
                .map(|c| c.vertical_sampling_factor)
                .max()
                .unwrap_or(1);

            let is_luminance_only = scan.components.len() == 1 && scan.components[0].component_id == 1;

            if is_luminance_only {
                max_h_samp = 1;
                max_v_samp = 1;
            }

            let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
            let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

            let mut restart_counter = restart_interval;

            for mcu_y in 0..mcu_height {
                for mcu_x in 0..mcu_width {
                    if restart_interval > 0 {
                        if restart_counter == 0 {
                            previous_dc.fill(0);
                            reader.clear_buffer();
                            restart_counter = restart_interval;
                        }
                        restart_counter = restart_counter.saturating_sub(1);
                    }

                    for (comp_idx, scan_comp) in scan.components.clone().iter().enumerate() {
                        let comp = match self.components.iter().find(|c| c.id == scan_comp.component_id) {
                            Some(c) => c,
                            None => {
                                log_warn!("Component not found: {}", scan_comp.component_id);
                                continue;
                            }
                        };

                        let h_blocks = if is_luminance_only {
                            1
                        } else {
                            comp.horizontal_sampling_factor
                        };
                        let v_blocks = if is_luminance_only {
                            1
                        } else {
                            comp.vertical_sampling_factor
                        };
                        let plane_index = comp.id as usize - 1;

                        if plane_index >= planes.len() {
                            log_warn!("Invalid plane index: {}", plane_index);
                            continue;
                        }

                        for v in 0..v_blocks {
                            for h in 0..h_blocks {
                                let plane_blocks_per_line = planes[plane_index].blocks_per_line;
                                let block_x = if is_luminance_only {
                                    mcu_x + h as u32
                                } else {
                                    (mcu_x * comp.horizontal_sampling_factor as u32 + h as u32)
                                        .min(plane_blocks_per_line - 1)
                                };

                                let block_y = if is_luminance_only {
                                    mcu_y + v as u32
                                } else {
                                    mcu_y * comp.vertical_sampling_factor as u32 + v as u32
                                };

                                if block_x >= plane_blocks_per_line {
                                    continue;
                                }

                                if let Some(component_data) = planes[plane_index].get_block_mut(block_x, block_y) {
                                    let scan_component = scan_comp;

                                    if scan.start_spectral == 0 {
                                        if scan.successive_high == 0 {
                                            // First DC scan
                                            let dc_table =
                                                match scan.dc_tables.get(scan_component.dc_table_selector as usize) {
                                                    Some(table) => table,
                                                    None => {
                                                        log_warn!(
                                                            "DC table not found: {}",
                                                            scan_component.dc_table_selector
                                                        );
                                                        continue;
                                                    }
                                                };

                                            let length = self.get_next_symbol(&mut reader, dc_table)?;

                                            if length > 11 {
                                                log_warn!("Invalid DC coefficient length (>11): {}", length);
                                                continue;
                                            }

                                            let bits = match reader.read_bits(length) {
                                                Ok(bits) => bits,
                                                Err(e) => {
                                                    log_warn!("Failed to read DC coefficient bits: {}", e);
                                                    continue;
                                                }
                                            };

                                            let mut value = bits as i32;

                                            if length != 0 && value < (1 << (length - 1)) {
                                                value -= (1 << length) - 1;
                                            }

                                            value += previous_dc[comp_idx];
                                            previous_dc[comp_idx] = value;
                                            component_data[0] = value << scan.successive_low;
                                        } else {
                                            // Refining DC scan
                                            let bit = match reader.read_bits(1) {
                                                Ok(bit) => bit,
                                                Err(e) => {
                                                    log_warn!("Failed to read DC coefficient bit: {}", e);
                                                    continue;
                                                }
                                            };

                                            component_data[0] |= (bit as i32) << scan.successive_low;
                                        }
                                    }

                                    if scan.end_spectral > 0 {
                                        if scan.successive_high == 0 {
                                            // First AC scan
                                            if skips > 0 {
                                                skips -= 1;
                                                continue;
                                            }

                                            let ac_table =
                                                match scan.ac_tables.get(scan_component.ac_table_selector as usize) {
                                                    Some(table) => table,
                                                    None => {
                                                        log_warn!(
                                                            "AC table not found: {}",
                                                            scan_component.ac_table_selector
                                                        );
                                                        continue;
                                                    }
                                                };

                                            let mut k = scan.start_spectral as usize;
                                            while k <= scan.end_spectral as usize {
                                                let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                    Ok(symbol) => symbol,
                                                    Err(e) => {
                                                        log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                        break;
                                                    }
                                                };

                                                let num_zeros = symbol >> 4;
                                                let length = symbol & 0xF;

                                                if length != 0 {
                                                    if k + num_zeros as usize > 63 {
                                                        log_warn!(
                                                            "Zero run-length exceeded spectral selection: {}",
                                                            k + num_zeros as usize
                                                        );
                                                        break;
                                                    }

                                                    for _ in 0..num_zeros {
                                                        if k > ZIGZAG_MAP.len() {
                                                            log_warn!("k value exceeded zigzag map: {}", k);
                                                            break;
                                                        }

                                                        component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                        k += 1;
                                                    }

                                                    if length > 10 {
                                                        log_warn!("Invalid AC coefficient length (>10): {}", length);
                                                        break;
                                                    }

                                                    let bits = match reader.read_bits(length) {
                                                        Ok(bits) => bits,
                                                        Err(e) => {
                                                            log_warn!("Failed to read AC coefficient bits: {}", e);
                                                            break;
                                                        }
                                                    };

                                                    let mut value = bits as i32;

                                                    if value < (1 << (length - 1)) {
                                                        value -= (1 << length) - 1;
                                                    }

                                                    let zigzag_idx = ZIGZAG_MAP[k] as usize;
                                                    component_data[zigzag_idx] = value << scan.successive_low;
                                                    k += 1;
                                                } else {
                                                    if num_zeros == 15 {
                                                        if k + num_zeros as usize > scan.end_spectral as usize {
                                                            log_warn!(
                                                                "Zero run-length exceeded spectral selection: {}",
                                                                k + num_zeros as usize
                                                            );
                                                            break;
                                                        }

                                                        for _ in 0..num_zeros {
                                                            if k > ZIGZAG_MAP.len() {
                                                                log_warn!("k value exceeded zigzag map: {}", k);
                                                                break;
                                                            }

                                                            component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                            k += 1;
                                                        }
                                                    } else {
                                                        skips = (1 << num_zeros) - 1;
                                                        let extra_skips =
                                                            reader.read_bits(num_zeros).unwrap_or_else(|e| {
                                                                log_warn!("Failed to read extra skips: {}", e);
                                                                0
                                                            });

                                                        skips += extra_skips;
                                                        break;
                                                    }

                                                    k += 1;
                                                }
                                            }
                                        } else {
                                            // Refining AC scan
                                            let positive = 1 << scan.successive_low;
                                            let negative = -1 << scan.successive_low;
                                            let mut k = scan.start_spectral as usize;

                                            if skips == 0 {
                                                let ac_table =
                                                    match scan.ac_tables.get(scan_component.ac_table_selector as usize)
                                                    {
                                                        Some(table) => table,
                                                        None => {
                                                            log_warn!(
                                                                "AC table not found: {}",
                                                                scan_component.ac_table_selector
                                                            );
                                                            continue;
                                                        }
                                                    };

                                                while k <= scan.end_spectral as usize {
                                                    let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                        Ok(symbol) => symbol,
                                                        Err(e) => {
                                                            log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                            break;
                                                        }
                                                    };

                                                    let mut num_zeros = symbol >> 4;
                                                    let length = symbol & 0xF;
                                                    let mut coefficient = 0;

                                                    if length != 0 {
                                                        if length != 1 {
                                                            log_warn!(
                                                                "Invalid AC coefficient length (refining): {}",
                                                                length
                                                            );
                                                            break;
                                                        }

                                                        coefficient = match reader.read_bits(1) {
                                                            Ok(bit) => match bit {
                                                                0 => negative,
                                                                1 => positive,
                                                                _ => unreachable!(),
                                                            },
                                                            Err(e) => {
                                                                log_warn!(
                                                                    "Failed to read AC coefficient bit (refining): {}",
                                                                    e
                                                                );
                                                                break;
                                                            }
                                                        };
                                                    } else {
                                                        if num_zeros != 15 {
                                                            skips = 1 << num_zeros;
                                                            let extra_skips =
                                                                reader.read_bits(num_zeros).unwrap_or_else(|e| {
                                                                    log_warn!(
                                                                        "Failed to read extra skips (refining): {}",
                                                                        e
                                                                    );
                                                                    0
                                                                });

                                                            skips += extra_skips;
                                                            break;
                                                        }
                                                    }

                                                    if component_data.len() <= ZIGZAG_MAP[k] as usize {
                                                        log_warn!(
                                                            "Value from a zigzag map exceeds component data length: {}",
                                                            ZIGZAG_MAP[k]
                                                        );
                                                        break;
                                                    }

                                                    loop {
                                                        if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                            match reader.read_bits(1) {
                                                                Ok(bit) => {
                                                                    if bit == 1 {
                                                                        if component_data[ZIGZAG_MAP[k] as usize]
                                                                            & positive
                                                                            == 0
                                                                        {
                                                                            if component_data[ZIGZAG_MAP[k] as usize]
                                                                                >= 0
                                                                            {
                                                                                component_data
                                                                                    [ZIGZAG_MAP[k] as usize] +=
                                                                                    positive;
                                                                            } else {
                                                                                component_data
                                                                                    [ZIGZAG_MAP[k] as usize] +=
                                                                                    negative;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    log_warn!("Failed to read AC coefficient bit (refining): {}", e);
                                                                    break;
                                                                }
                                                            }
                                                        } else {
                                                            if num_zeros == 0 {
                                                                break;
                                                            }
                                                            num_zeros -= 1;
                                                        }

                                                        k += 1;

                                                        if k > scan.end_spectral as usize {
                                                            break;
                                                        }
                                                    }

                                                    if coefficient != 0 && k <= scan.end_spectral as usize {
                                                        component_data[ZIGZAG_MAP[k] as usize] = coefficient;
                                                    }

                                                    k += 1;
                                                }
                                            }

                                            if skips > 0 {
                                                while k <= scan.end_spectral as usize {
                                                    if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                        match reader.read_bits(1) {
                                                            Ok(b) => {
                                                                if b == 1 {
                                                                    if component_data[ZIGZAG_MAP[k] as usize] & positive
                                                                        == 0
                                                                    {
                                                                        if component_data[ZIGZAG_MAP[k] as usize] >= 0 {
                                                                            component_data[ZIGZAG_MAP[k] as usize] +=
                                                                                positive;
                                                                        } else {
                                                                            component_data[ZIGZAG_MAP[k] as usize] +=
                                                                                negative;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                log_warn!("Failed to read AC coefficient bit: {}", e);
                                                                break;
                                                            }
                                                        }
                                                    }

                                                    k += 1;
                                                }

                                                skips -= 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn decode_differences(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let mut reader = BitReader::new(Cursor::new(scan.data.clone()));

        let mut differences: Vec<Vec<i32>> = vec![vec![]; scan.components.len()];

        let width = self.width as usize;
        let height = self.height as usize;

        for diffs in &mut differences {
            diffs.reserve(width * height);
        }

        // TODO handle restarts
        for _ in 0..height {
            for _ in 0..width {
                for (i, scan_component) in scan.components.iter().enumerate() {
                    let dc_table = match scan.dc_tables.get(scan_component.dc_table_selector as usize) {
                        Some(table) => table,
                        None => {
                            log_warn!("No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.", i);
                            &HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    let bits_to_read = self.get_next_symbol(&mut reader, dc_table)?;

                    let diff = match bits_to_read {
                        0 => 0,
                        1..=15 => {
                            let additional_bits = reader.read_bits(bits_to_read)? as i32;

                            if additional_bits < (1 << (bits_to_read - 1)) {
                                additional_bits + (-1 << bits_to_read) + 1
                            } else {
                                additional_bits
                            }
                        }
                        16 => 32768,
                        _ => {
                            log_warn!("Invalid difference: {}", bits_to_read);
                            0
                        }
                    };

                    differences[i].push(diff);
                }
            }
        }

        Ok(differences)
    }

    fn predict(
        ra: i32,
        rb: i32,
        rc: i32,
        predictor: Predictor,
        point_transform: u8,
        input_precision: u8,
        x: usize,
        y: usize,
    ) -> i32 {
        // TODO handle restarts as well
        if x == 0 && y == 0 {
            if input_precision > point_transform + 1 {
                1 << (input_precision - point_transform - 1)
            } else {
                0
            }
        } else if y == 0 {
            ra
        } else if x == 0 {
            rb
        } else {
            match predictor {
                Predictor::NoPrediction => 0,
                Predictor::Ra => ra,
                Predictor::Rb => rb,
                Predictor::Rc => rc,
                Predictor::RaRbRc1 => ra + rb - rc,
                Predictor::RaRbRc2 => ra + ((rb - rc) >> 1),
                Predictor::RaRbRc3 => rb + ((ra - rc) >> 1),
                Predictor::RaRb => (ra + rb) / 2,
            }
        }
    }

    fn reconstruct_samples(
        &self,
        differences: Vec<Vec<i32>>,
        predictor: Predictor,
        point_transform: u8,
    ) -> VexelResult<Vec<Vec<u16>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = differences.len();

        let mut samples = vec![vec![0u16; width * height]; components_count];

        if predictor == Predictor::Ra {
            for component_index in 0..components_count {
                let default_prediction = 1 << (self.precision - point_transform - 1);

                let first_diff = differences[component_index][0];
                samples[component_index][0] = (((default_prediction + first_diff) & 0xFFFF) as u16) << point_transform;

                for y in 1..height {
                    let diff = differences[component_index][y * width];
                    let rb = samples[component_index][(y - 1) * width] as i32;
                    samples[component_index][y * width] = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                }

                for y in 0..height {
                    for x in 1..width {
                        let index = y * width + x;
                        let diff = differences[component_index][index];
                        let ra = samples[component_index][index - 1] as i32;

                        samples[component_index][index] = (((ra + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    for component_index in 0..components_count {
                        let index = y * width + x;
                        let diff = differences[component_index][index];

                        let ra = if x > 0 {
                            samples[component_index][index - 1] as i32
                        } else {
                            0
                        };
                        let rb = if y > 0 {
                            samples[component_index][(y - 1) * width + x] as i32
                        } else {
                            0
                        };
                        let rc = if x > 0 && y > 0 {
                            samples[component_index][(y - 1) * width + (x - 1)] as i32
                        } else {
                            0
                        };

                        let prediction =
                            Self::predict(ra, rb, rc, predictor.clone(), point_transform, self.precision, x, y);

                        samples[component_index][index] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        }

        Ok(samples)
    }

    fn samples_to_image(&self, samples: Vec<Vec<u16>>) -> VexelResult<Image> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = samples.len();

        let mut output: Vec<u16> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let pixel_pos = y * width + x;

                for component_index in 0..components_count {
                    let sample = samples[component_index][pixel_pos];
                    output.push(sample);
                }
            }
        }

        if components_count == 1 {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L16(pixels), 0)])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::L8
                } else {
                    PixelFormat::L16
                },
                frames,
            ))
        } else {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::RGB8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(
                    width as u32,
                    height as u32,
                    PixelData::RGB16(pixels),
                    0,
                )])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::RGB8
                } else {
                    PixelFormat::RGB16
                },
                frames,
            ))
        }
    }

    fn decode_lossless(&mut self) -> VexelResult<Image> {
        // TODO there can be multiple scans in lossless mode somehow
        let scan = match self.scans.first() {
            Some(s) => s.clone(),
            None => {
                return Err(VexelError::from(Error::new(
                    ErrorKind::InvalidData,
                    "No scan data found",
                )))
            }
        };

        let differences = self.decode_differences(&scan)?;

        let point_transform = self.successive_approximation_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        let samples = self.reconstruct_samples(differences, predictor, point_transform)?;

        self.samples_to_image(samples)
    }

    fn decode_arithmetic_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let scan = &self.scans.clone()[0];
        let mut decoder = ArithmeticDecoder::new(BitReader::new(Cursor::new(scan.data.clone())));
        decoder.init();

        let mut previous_dc = vec![0i32; planes.len()];
        let mut prev_dc_diffs = vec![0i32; planes.len()];

        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut restart_counter = self.restart_interval as u32;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                // Handle restart interval
                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        prev_dc_diffs.fill(0);
                        previous_dc.fill(0);
                        decoder = ArithmeticDecoder::new(BitReader::new(Cursor::new(scan.data.clone())));
                        decoder.init();
                        restart_counter = self.restart_interval as u32;
                    }
                    restart_counter = restart_counter.saturating_sub(1);
                }

                // Process components
                for (comp_idx, comp) in self.components.clone().iter().enumerate() {
                    // Get conditioning parameters from AC/DC tables
                    let dc_table = self.components[comp_idx].dc_table_selector;
                    let ac_table = self.components[comp_idx].ac_table_selector;

                    let small = 0; // Default L threshold
                    let large = 1; // Default U threshold
                    let kx = 5; // Default Kx value for AC coding

                    for v in 0..comp.vertical_sampling_factor {
                        for h in 0..comp.horizontal_sampling_factor {
                            let block_x = mcu_x * comp.horizontal_sampling_factor as u32 + h as u32;
                            let block_y = mcu_y * comp.vertical_sampling_factor as u32 + v as u32;

                            if let Some(block) = planes[comp_idx].get_block_mut(block_x, block_y) {
                                self.decode_arithmetic_block(
                                    &mut decoder,
                                    block,
                                    &mut previous_dc[comp_idx],
                                    &mut prev_dc_diffs[comp_idx],
                                    small,
                                    large,
                                    kx,
                                    dc_table,
                                    ac_table,
                                )?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
    fn decode_arithmetic_block(
        &mut self,
        decoder: &mut ArithmeticDecoder,
        block: &mut [i32],
        prev_dc: &mut i32,
        prev_diff: &mut i32,
        small: u8,
        large: u8,
        kx: u8,
        dc_ctx: u8,
        ac_ctx: u8,
    ) -> VexelResult<()> {
        let dc_context = if *prev_diff == 0 {
            0
        } else if *prev_diff > 0 {
            if *prev_diff <= (1 << small) {
                4
            } else {
                8
            }
        } else {
            if *prev_diff >= -(1 << small) {
                12
            } else {
                16
            }
        };

        let s0 = dc_context;

        if decoder.decode(s0) {
            // Non-zero DC
            let sign = decoder.decode(s0 + 1); // Sign in SS context

            let mut magnitude = 1;
            let mut s = s0 + if sign { 3 } else { 2 }; // SN or SP context

            while decoder.decode(s) {
                magnitude += 1;
                s += 1;
            }

            let mut value = 1 << (magnitude - 1);
            s += 14; // Switch to M contexts
            for i in (0..magnitude - 1).rev() {
                if decoder.decode(s) {
                    value |= 1 << i;
                }
            }

            if sign {
                value = -value;
            }
        }

        // AC decoding with correct contexts
        let mut k = 1;
        while k <= 63 {
            let se = 3 * (k - 1); // Base EOB context
            let s0 = se + 1; // Base zero/non-zero context

            if k > 1 && decoder.decode(se) {
                // EOB, fill with zeros
                break;
            }

            if decoder.decode(s0) {
                // Non-zero coefficient
                let sign = decoder.decode(0xFF); // Uniform context
            }
            k += 1;
        }

        Ok(())
    }
    fn decode_huffman_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.len() < 1 {
            // Well, nothing to do here, how did this even happen?
            log_warn!("No scans found in JPEG data");
            return Ok(());
        }

        let scan = &self.scans[0];
        // TODO uhh, how can we not clone this?
        let mut reader = BitReader::new(Cursor::new(scan.data.clone()));
        let mut previous_dc = vec![0i32; planes.len()];

        // Calculate MCU dimensions
        let mut max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let mut max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        if max_h_samp == 0 || max_v_samp == 0 {
            log_warn!("Invalid sampling factors: ({}, {})", max_h_samp, max_v_samp);
            max_h_samp = 1;
            max_v_samp = 1;
        }

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut restart_counter = self.restart_interval as u32;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                // Handle restart interval
                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        previous_dc.fill(0);
                        reader.clear_buffer();
                        restart_counter = self.restart_interval as u32;
                    }

                    restart_counter = restart_counter.saturating_sub(1);
                }

                // Process each component
                for (comp_idx, comp) in self.components.clone().iter().enumerate() {
                    if self.scans[0].components.len() <= comp_idx {
                        log_warn!(
                            "Component index out of bounds: {} {}",
                            self.scans[0].components.len(),
                            comp_idx
                        );
                        continue;
                    }

                    let dc_selector = self.scans[0].components[comp_idx].dc_table_selector as usize;
                    let ac_selector = self.scans[0].components[comp_idx].ac_table_selector as usize;

                    let dc_table = match self.scans[0].dc_tables.get(dc_selector) {
                        Some(table) => table.clone(),
                        None => {
                            log_warn!("DC table {} not found in baseline mode, substituting default, image will be corrupted.", dc_selector);

                            HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    let ac_table = match self.scans[0].ac_tables.get(ac_selector) {
                        Some(table) => table.clone(),
                        None => {
                            log_warn!("AC table {} not found in baseline mode, substituting default, image will be corrupted.", ac_selector);

                            HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    // Handle sampling factors
                    for v in 0..comp.vertical_sampling_factor {
                        for h in 0..comp.horizontal_sampling_factor {
                            let block_x = mcu_x * comp.horizontal_sampling_factor as u32 + h as u32;
                            let block_y = mcu_y * comp.vertical_sampling_factor as u32 + v as u32;

                            if comp_idx >= previous_dc.len() {
                                log_warn!(
                                    "Component is larger than previous DC buffer: {} {}",
                                    comp_idx,
                                    previous_dc.len()
                                );
                                continue;
                            }

                            if let Some(block) = planes[comp_idx].get_block_mut(block_x, block_y) {
                                match self.decode_mcu(
                                    &mut reader,
                                    block,
                                    &dc_table,
                                    &ac_table,
                                    &mut previous_dc[comp_idx],
                                ) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        log_warn!("Failed to decode MCU: {}", e);
                                    }
                                };
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn dequantize_planes(&self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        for (comp_idx, plane) in planes.iter_mut().enumerate() {
            let default_table = &QuantizationTable {
                id: 0,
                precision: 8,
                length: 64,
                table: DEFAULT_QUANTIZATION_TABLE.to_vec(),
            };

            let quant_table = self
                .components
                .get(comp_idx)
                .and_then(|comp| {
                    self.quantization_tables
                        .iter()
                        .find(|q| q.id == comp.quantization_table_id)
                })
                .map(|q| q)
                .unwrap_or_else(|| {
                    log_warn!("Quantization table not found for component, substituting default one.");
                    default_table
                });

            for block in plane.data.chunks_mut(64) {
                for i in 0..64 {
                    if block.len() <= i || quant_table.table.len() <= i {
                        log_warn!(
                            "Block or quantization table index out of bounds: {} {}",
                            block.len(),
                            quant_table.table.len()
                        );
                        continue;
                    }

                    block[i] *= quant_table.table[i] as i32;
                }
            }
        }

        Ok(())
    }

    fn inverse_dct_planes(&self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
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

        let level_shift = if self.precision <= 8 { 128 } else { 2048 };

        // Process each component plane
        for plane in planes {
            // Calculate number of blocks in this plane
            let block_count = (plane.data.len() / 64) as u32;

            // Process each 8x8 block
            for block_idx in 0..block_count {
                let block_start = (block_idx * 64) as usize;

                if block_start + 64 > plane.data.len() {
                    log_warn!("Block index out of bounds: {} {}", block_start, plane.data.len());
                    continue;
                }

                let block = &mut plane.data[block_start..block_start + 64];
                let mut temp = [0.0f32; 64];

                // Process columns
                for col in 0..8 {
                    let g_0 = block[0 * 8 + col] as f32 * s_0;
                    let g_1 = block[4 * 8 + col] as f32 * s_4;
                    let g_2 = block[2 * 8 + col] as f32 * s_2;
                    let g_3 = block[6 * 8 + col] as f32 * s_6;
                    let g_4 = block[5 * 8 + col] as f32 * s_5;
                    let g_5 = block[1 * 8 + col] as f32 * s_1;
                    let g_6 = block[7 * 8 + col] as f32 * s_7;
                    let g_7 = block[3 * 8 + col] as f32 * s_3;

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

                    temp[0 * 8 + col] = b_0 + b_7;
                    temp[1 * 8 + col] = b_1 + b_6;
                    temp[2 * 8 + col] = b_2 + b_5;
                    temp[3 * 8 + col] = b_3 + b_4;
                    temp[4 * 8 + col] = b_3 - b_4;
                    temp[5 * 8 + col] = b_2 - b_5;
                    temp[6 * 8 + col] = b_1 - b_6;
                    temp[7 * 8 + col] = b_0 - b_7;
                }

                // Process rows
                for row in 0..8 {
                    let g_0 = temp[row * 8 + 0] * s_0;
                    let g_1 = temp[row * 8 + 4] * s_4;
                    let g_2 = temp[row * 8 + 2] * s_2;
                    let g_3 = temp[row * 8 + 6] * s_6;
                    let g_4 = temp[row * 8 + 5] * s_5;
                    let g_5 = temp[row * 8 + 1] * s_1;
                    let g_6 = temp[row * 8 + 7] * s_7;
                    let g_7 = temp[row * 8 + 3] * s_3;

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

                    block[row * 8 + 0] = ((b_0 + b_7 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 1] = ((b_1 + b_6 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 2] = ((b_2 + b_5 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 3] = ((b_3 + b_4 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 4] = ((b_3 - b_4 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 5] = ((b_2 - b_5 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 6] = ((b_1 - b_6 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 7] = ((b_0 - b_7 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                }
            }
        }

        Ok(())
    }

    fn upsample_planes(&self, planes: &[ComponentPlane]) -> Vec<UpsampledPlane> {
        let mut upsampled_planes = Vec::new();

        for (plane) in planes.iter() {
            // For Y component (id=1), we keep original dimensions
            // For Cb and Cr (id=2,3), we upsample to full image dimensions
            // Get the final target dimensions - these should be the full image dimensions
            let target_width = self.width;
            let target_height = self.height;

            let upsampled = plane.upsample(target_width, target_height);
            upsampled_planes.push(upsampled);
        }

        upsampled_planes
    }

    fn convert_colorspace(&self, planes: &[UpsampledPlane]) -> VexelResult<PixelData> {
        let mut pixels = Vec::with_capacity((self.width * self.height * 3) as usize);

        fn get_pixel_from_planes(planes: &[UpsampledPlane], index: usize, x: u32, y: u32) -> f32 {
            match planes.get(index) {
                Some(plane) => plane.get_pixel(x, y).unwrap_or(0) as f32,
                None => 0.0,
            }
        }

        if planes.len() == 1 {
            return if self.precision <= 8 {
                for y in 0..self.height {
                    for x in 0..self.width {
                        let y_val = planes[0].get_pixel(x, y).unwrap_or(0);
                        let gray_val = y_val.clamp(0, 255) as u8;
                        pixels.push(gray_val);
                    }
                }

                Ok(PixelData::L8(pixels))
            } else {
                let mut pixels16 = Vec::with_capacity((self.width * self.height) as usize);

                for y in 0..self.height {
                    for x in 0..self.width {
                        let y_val = planes[0].get_pixel(x, y).unwrap_or(0);
                        let gray_val = y_val.clamp(0, 4095) as u16;
                        pixels16.push(gray_val);
                    }
                }

                Ok(PixelData::L16(pixels16))
            };
        }

        if planes.len() < 3 {
            log_warn!("Invalid number of planes for RGB conversion: {}.", planes.len());
        }

        if self.precision <= 8 {
            for y in 0..self.height {
                for x in 0..self.width {
                    let y_val = get_pixel_from_planes(planes, 0, x, y);
                    let cb_val = get_pixel_from_planes(planes, 1, x, y);
                    let cr_val = get_pixel_from_planes(planes, 2, x, y);

                    let r = (y_val + 1.402 * cr_val + 128.0).clamp(0.0, 255.0) as u8;
                    let g = (y_val - 0.344136 * cb_val - 0.714136 * cr_val + 128.0).clamp(0.0, 255.0) as u8;
                    let b = (y_val + 1.772 * cb_val + 128.0).clamp(0.0, 255.0) as u8;

                    pixels.extend_from_slice(&[r, g, b]);
                }
            }

            Ok(PixelData::RGB8(pixels))
        } else {
            let mut pixels16 = Vec::with_capacity((self.width * self.height * 3) as usize);

            for y in 0..self.height {
                for x in 0..self.width {
                    let y_val = get_pixel_from_planes(planes, 0, x, y);
                    let cb_val = get_pixel_from_planes(planes, 1, x, y);
                    let cr_val = get_pixel_from_planes(planes, 2, x, y);

                    let r = (y_val + 1.402 * cr_val + 2048.0).clamp(0.0, 4095.0) as u16;
                    let g = (y_val - 0.344136 * cb_val - 0.714136 * cr_val + 2048.0).clamp(0.0, 4095.0) as u16;
                    let b = (y_val + 1.772 * cb_val + 2048.0).clamp(0.0, 4095.0) as u16;

                    pixels16.extend_from_slice(&[r, g, b]);
                }
            }

            Ok(PixelData::RGB16(pixels16))
        }
    }

    fn decode_baseline(&mut self) -> VexelResult<Image> {
        // Calculate dimensions for each component based on sampling
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        // Create component planes at their native resolutions
        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                // Calculate dimensions in samples (pixels)
                let comp_width =
                    (self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1) / max_h_samp as u32;
                let comp_height =
                    (self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1) / max_v_samp as u32;

                ComponentPlane::new(
                    comp_width,
                    comp_height,
                    comp.horizontal_sampling_factor,
                    comp.vertical_sampling_factor,
                    comp.id,
                )
            })
            .collect();

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut component_planes)?,
        }

        self.dequantize_planes(&mut component_planes)?;
        self.inverse_dct_planes(&mut component_planes)?;

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            match marker {
                Some(marker) => {
                    log_debug!("Found marker: {:?}", marker);

                    let result = match marker {
                        JpegMarker::SOI => Ok(()),
                        JpegMarker::COM => self.read_com(),
                        JpegMarker::APP0 => self.read_app0_jfif(),
                        JpegMarker::APP1 => self.read_app1_exif(),
                        JpegMarker::SOF0 => self.read_start_of_frame(),
                        JpegMarker::SOF1 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.read_start_of_frame()
                        }
                        JpegMarker::SOF2 => {
                            self.mode = JpegMode::Progressive;
                            self.read_start_of_frame()
                        }
                        JpegMarker::SOF3 => {
                            self.mode = JpegMode::Lossless;
                            self.read_start_of_frame()
                        }
                        JpegMarker::SOF9 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame()
                        }
                        JpegMarker::SOF11 => {
                            self.mode = JpegMode::Lossless;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame()
                        }
                        JpegMarker::DRI => self.read_restart_interval(),
                        JpegMarker::DQT => self.read_quantization_table(),
                        JpegMarker::DHT => self.read_huffman_table(),
                        JpegMarker::DAC => self.read_dac(),
                        JpegMarker::SOS => self.read_start_of_scan(),
                        JpegMarker::EOI => {
                            break;
                        }
                        _ => {
                            log_warn!("Unhandled marker found: {:?}", marker);
                            self.skip_unknown_marker_segment()
                        }
                    };

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            log_warn!("Failed to process {:?} marker segment: {}", marker, e);
                        }
                    }
                }
                None => {
                    log_debug!("End of file reached");
                    break;
                }
            }
        }

        log_debug!(
            "Dimensions: {}x{}. Number of pixels: {}",
            self.width,
            self.height,
            self.width * self.height
        );
        log_debug!("Number of components: {}", self.components.len());
        log_debug!("Number of scans: {}", self.scans.len());
        log_debug!("Mode: {:?}", self.mode);
        log_debug!("Coding method: {:?}", self.coding_method);
        log_debug!("Bit depth: {}", self.precision);
        log_debug!("Restart interval: {}", self.restart_interval);
        log_debug!(
            "Sampling factors: {:?}",
            self.components
                .iter()
                .map(|c| format!("{}/{}", c.horizontal_sampling_factor, c.vertical_sampling_factor))
                .collect::<Vec<String>>()
                .join(", ")
        );

        match &self.mode {
            JpegMode::Baseline => {
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::ExtendedSequential => {
                // TODO general decoding process is same as baseline, so this method can be used,
                // but it would be nice to have a different name for this mode
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::Progressive => {
                let image = self.decode_progressive()?;
                Ok(image)
            }
            JpegMode::Lossless => {
                let image = self.decode_lossless()?;
                Ok(image)
            }
        }
    }
}
