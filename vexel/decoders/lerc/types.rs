use crate::utils::error::{VexelError, VexelResult};

pub const LERC2_MAGIC: &[u8; 6] = b"Lerc2 ";
pub const LERC1_MAGIC: &[u8; 10] = b"CntZImage ";

pub const CURRENT_VERSION: i32 = 6;
pub const MAX_MICRO_BLOCK_SIZE: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LercDataType {
    Char,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Float,
    Double,
}

impl LercDataType {
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Char),
            1 => Some(Self::Byte),
            2 => Some(Self::Short),
            3 => Some(Self::UShort),
            4 => Some(Self::Int),
            5 => Some(Self::UInt),
            6 => Some(Self::Float),
            7 => Some(Self::Double),
            _ => None,
        }
    }

    pub fn code(self) -> i32 {
        match self {
            Self::Char => 0,
            Self::Byte => 1,
            Self::Short => 2,
            Self::UShort => 3,
            Self::Int => 4,
            Self::UInt => 5,
            Self::Float => 6,
            Self::Double => 7,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::Float | Self::Double)
    }

    pub fn clamp(self, z: f64) -> f64 {
        match self {
            Self::Char => z.clamp(i8::MIN as f64, i8::MAX as f64),
            Self::Byte => z.clamp(0.0, u8::MAX as f64),
            Self::Short => z.clamp(i16::MIN as f64, i16::MAX as f64),
            Self::UShort => z.clamp(0.0, u16::MAX as f64),
            Self::Int => z.clamp(i32::MIN as f64, i32::MAX as f64),
            Self::UInt => z.clamp(0.0, u32::MAX as f64),
            Self::Float => {
                let f = z as f32;
                if f.is_finite() {
                    f as f64
                } else {
                    z
                }
            }
            Self::Double => z,
        }
    }

    pub fn reduced(self, type_code: i32) -> Option<Self> {
        let validate = |code: i32| Self::from_code(code);

        match self {
            Self::Short | Self::Int => validate(self.code() - type_code),
            Self::UShort | Self::UInt => validate(self.code() - 2 * type_code),
            Self::Float => match type_code {
                0 => Some(self),
                1 => Some(Self::Short),
                _ => Some(Self::Byte),
            },
            Self::Double => {
                if type_code == 0 {
                    Some(self)
                } else {
                    validate(self.code() - 2 * type_code + 1)
                }
            }
            _ => Some(self),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEncodeMode {
    Tiling,
    DeltaHuffman,
    Huffman,
    DeltaDeltaHuffman,
}

impl ImageEncodeMode {
    pub fn from_flag(flag: u8) -> Option<Self> {
        match flag {
            0 => Some(Self::Tiling),
            1 => Some(Self::DeltaHuffman),
            2 => Some(Self::Huffman),
            3 => Some(Self::DeltaDeltaHuffman),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Lerc2Header {
    pub version: i32,
    pub checksum: u32,
    pub n_rows: i32,
    pub n_cols: i32,
    pub n_depth: i32,
    pub num_valid_pixel: i32,
    pub micro_block_size: i32,
    pub blob_size: i32,
    pub n_blobs_more: i32,
    pub pass_no_data_values: bool,
    pub is_int: bool,
    pub dt: Option<LercDataType>,
    pub max_z_error: f64,
    pub z_min: f64,
    pub z_max: f64,
    pub no_data_val: f64,
    pub no_data_val_orig: f64,
    pub header_size: usize,
}

impl Lerc2Header {
    pub fn data_type(&self) -> VexelResult<LercDataType> {
        self.dt.ok_or_else(|| VexelError::Custom("LERC blob has no valid data type".to_string()))
    }

    pub fn try_huffman_int(&self) -> bool {
        self.version >= 2
            && matches!(self.dt, Some(LercDataType::Byte) | Some(LercDataType::Char))
            && self.max_z_error == 0.5
    }

    pub fn try_huffman_flt(&self) -> bool {
        self.version >= 6
            && matches!(self.dt, Some(LercDataType::Float) | Some(LercDataType::Double))
            && self.max_z_error == 0.0
    }

    pub fn num_pixels(&self) -> usize {
        self.n_rows as usize * self.n_cols as usize
    }

    pub fn num_values(&self) -> usize {
        self.num_pixels() * self.n_depth as usize
    }
}

#[derive(Debug, Clone)]
pub struct Lerc1Header {
    pub width: i32,
    pub height: i32,
    pub max_z_error: f64,
}
