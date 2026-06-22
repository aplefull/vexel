use crate::utils::exif::ExifData;
use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegSegmentInfo {
    pub start_offset: u64,
    pub marker: String,
    pub data: JpegSegmentData,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JpegSegmentData {
    SOI,
    EOI,
    APP0(JFIFData),
    APP1 { length: u16, exif: Option<ExifData> },
    APP { marker: String, length: u16 },
    SOF(SOFData),
    DHT(DHTData),
    DAC(DACData),
    DQT(DQTData),
    DRI { restart_interval: u16 },
    SOS(SOSData),
    COM { text: String },
    Unknown { marker: String, length: u16 },
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JFIFData {
    pub length: u16,
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
pub struct SOFData {
    pub length: u16,
    pub marker: String,
    pub precision: u8,
    pub width: u32,
    pub height: u32,
    pub component_count: u8,
    pub components: Vec<ColorComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DHTData {
    pub length: u16,
    pub tables: Vec<HuffmanTable>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DACData {
    pub length: u16,
    pub ac_tables: Vec<ArithmeticCodingTable>,
    pub dc_tables: Vec<ArithmeticCodingTable>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DQTData {
    pub length: u16,
    pub tables: Vec<QuantizationTable>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SOSData {
    pub length: u16,
    pub component_count: u8,
    pub components: Vec<ScanComponent>,
    pub start_spectral: u8,
    pub end_spectral: u8,
    pub successive_high: u8,
    pub successive_low: u8,
    pub dc_tables: Vec<HuffmanTable>,
    pub ac_tables: Vec<HuffmanTable>,
    pub data_length: u64,
}

#[rustfmt::skip]
pub const ZIGZAG_MAP: [u8; 64] = [
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
pub const DEFAULT_QUANTIZATION_TABLE: [u16; 64] = [
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
    DifferentialSequential,
    DifferentialProgressive,
    DifferentialLossless,
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
    pub first_code: Vec<u32>,
    pub fast_lookup: Vec<u32>,
}

impl HuffmanTable {
    pub fn build_fast_lookup(&mut self) {
        const PEEK_BITS: usize = 9;
        const TABLE_SIZE: usize = 1 << PEEK_BITS;
        let mut table = vec![0u32; TABLE_SIZE];

        for i in 0..16usize {
            if i >= PEEK_BITS {
                break;
            }
            let code_len = i + 1;
            if self.offsets.len() <= i + 1 {
                continue;
            }
            let start = self.offsets[i] as usize;
            let end = self.offsets[i + 1] as usize;
            for sym_idx in start..end {
                if sym_idx >= self.symbols.len() || sym_idx >= self.codes.len() {
                    break;
                }
                let code = self.codes[sym_idx];
                let sym = self.symbols[sym_idx];
                let fill_count = 1 << (PEEK_BITS - code_len);
                let base = (code as usize) << (PEEK_BITS - code_len);
                if base >= TABLE_SIZE {
                    continue;
                }
                let clamped_count = fill_count.min(TABLE_SIZE - base);
                let entry = (1u32 << 16) | ((sym as u32) << 8) | (code_len as u32);
                table[base..base + clamped_count].fill(entry);
            }
        }

        self.fast_lookup = table;
    }
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


#[derive(Debug, Clone)]
pub struct ScanData {
    pub start_spectral: u8,
    pub end_spectral: u8,
    pub successive_high: u8,
    pub successive_low: u8,
    pub components: Vec<ScanComponent>,
    pub dc_tables: Vec<HuffmanTable>,
    pub ac_tables: Vec<HuffmanTable>,
    pub arith_dc_tables: Vec<ArithmeticCodingTable>,
    pub arith_ac_tables: Vec<ArithmeticCodingTable>,
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