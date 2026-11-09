use serde::Serialize;
use tsify::Tsify;

use crate::log_warn;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum BitmapCompression {
    BiRgb = 0,
    BiRle8 = 1,
    BiRle4 = 2,
    BiBitfields = 3,
    BiJpeg = 4,
    BiPng = 5,
    BiAlphaBitfields = 6,
    BiCmyk = 11,
    BiCMYKRle8 = 12,
    BiCMYKRle4 = 13,
    Os2Huffman1D = 100,
    Os2Rle24 = 101,
}

impl BitmapCompression {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => BitmapCompression::BiRgb,
            1 => BitmapCompression::BiRle8,
            2 => BitmapCompression::BiRle4,
            3 => BitmapCompression::BiBitfields,
            4 => BitmapCompression::BiJpeg,
            5 => BitmapCompression::BiPng,
            6 => BitmapCompression::BiAlphaBitfields,
            11 => BitmapCompression::BiCmyk,
            12 => BitmapCompression::BiCMYKRle8,
            13 => BitmapCompression::BiCMYKRle4,
            _ => {
                log_warn!("Invalid compression type: {}", value);
                BitmapCompression::BiRgb
            }
        }
    }

    pub fn from_u32_os2(value: u32) -> Self {
        match value {
            0 => BitmapCompression::BiRgb,
            1 => BitmapCompression::BiRle8,
            2 => BitmapCompression::BiRle4,
            3 => BitmapCompression::Os2Huffman1D,
            4 => BitmapCompression::Os2Rle24,
            _ => {
                log_warn!("Invalid OS/2 compression type: {}", value);
                BitmapCompression::BiRgb
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapArrayHeader {
    pub file_size: u32,
    pub next_offset: u32,
    pub screen_width: u16,
    pub screen_height: u16,
}

#[derive(Debug, Clone, Default, Serialize, Tsify)]
pub struct BitmapFileHeader {
    pub signature: u16,
    pub file_size: u32,
    pub reserved1: u16,
    pub reserved2: u16,
    pub pixel_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum DibKind {
    Core,
    Os2V2Short,
    Os2V2,
    Info,
    V2,
    V3,
    V4,
    V5,
}

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ColorMasks {
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub alpha: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DibColorSpace {
    pub cs_type: u32,
    pub endpoints: ColorSpace,
    pub gamma_red: u32,
    pub gamma_green: u32,
    pub gamma_blue: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DibV5Fields {
    pub intent: u32,
    pub profile_data: u32,
    pub profile_size: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Os2Fields {
    pub units: u16,
    pub reserved: u16,
    pub recording: u16,
    pub rendering: u16,
    pub size1: u32,
    pub size2: u32,
    pub color_encoding: u32,
    pub identifier: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct DibHeader {
    pub kind: DibKind,
    pub header_size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits_per_pixel: u16,
    pub compression: BitmapCompression,
    pub compression_raw: u32,
    pub image_size: u32,
    pub x_pixels_per_meter: i32,
    pub y_pixels_per_meter: i32,
    pub colors_used: u32,
    pub important_colors: u32,
    pub masks: Option<ColorMasks>,
    pub color_space: Option<DibColorSpace>,
    pub v5: Option<DibV5Fields>,
    pub os2: Option<Os2Fields>,
}

impl DibHeader {
    pub fn empty() -> Self {
        DibHeader {
            kind: DibKind::Info,
            header_size: 0,
            width: 0,
            height: 0,
            planes: 0,
            bits_per_pixel: 0,
            compression: BitmapCompression::BiRgb,
            compression_raw: 0,
            image_size: 0,
            x_pixels_per_meter: 0,
            y_pixels_per_meter: 0,
            colors_used: 0,
            important_colors: 0,
            masks: None,
            color_space: None,
            v5: None,
            os2: None,
        }
    }

    pub fn color_masks(&self) -> Option<(u32, u32, u32, u32)> {
        self.masks.map(|masks| (masks.red, masks.green, masks.blue, masks.alpha.unwrap_or(0)))
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorSpace {
    pub ciexyz_red: CIEXYZ,
    pub ciexyz_green: CIEXYZ,
    pub ciexyz_blue: CIEXYZ,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CIEXYZ {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorEntry {
    pub blue: u8,
    pub green: u8,
    pub red: u8,
    pub reserved: u8,
}
