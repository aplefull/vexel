use crate::log_warn;
use crate::utils::icc::ICCProfile;
use serde::Serialize;
use tsify::Tsify;

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
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapArrayHeader {
    pub file_size: u32,
    pub next_offset: u32,
    pub screen_width: u16,
    pub screen_height: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapFileHeader {
    pub file_size: u32,
    pub reserved1: u16,
    pub reserved2: u16,
    pub pixel_offset: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum DibHeader {
    Core(BitmapCoreHeader),
    OS2V2(OS22XBitmapHeader),
    Info(BitmapInfoHeader),
    V2(BitmapV2InfoHeader),
    V3(BitmapV3InfoHeader),
    V4(BitmapV4Header),
    V5(BitmapV5Header),
}

impl DibHeader {
    pub fn bits_per_pixel(&self) -> u16 {
        match self {
            DibHeader::Core(h) => h.bits_per_pixel,
            DibHeader::OS2V2(h) => h.bits_per_pixel,
            DibHeader::Info(h) => h.bits_per_pixel,
            DibHeader::V2(h) => h.info.bits_per_pixel,
            DibHeader::V3(h) => h.v2.info.bits_per_pixel,
            DibHeader::V4(h) => h.v3.v2.info.bits_per_pixel,
            DibHeader::V5(h) => h.v4.v3.v2.info.bits_per_pixel,
        }
    }

    pub fn colors_used(&self) -> u32 {
        match self {
            DibHeader::Core(_) => 0,
            DibHeader::OS2V2(h) => h.colors_used,
            DibHeader::Info(h) => h.colors_used,
            DibHeader::V2(h) => h.info.colors_used,
            DibHeader::V3(h) => h.v2.info.colors_used,
            DibHeader::V4(h) => h.v3.v2.info.colors_used,
            DibHeader::V5(h) => h.v4.v3.v2.info.colors_used,
        }
    }

    pub fn height(&self) -> i32 {
        match self {
            DibHeader::Core(h) => h.height as i32,
            DibHeader::OS2V2(h) => h.height,
            DibHeader::Info(h) => h.height,
            DibHeader::V2(h) => h.info.height,
            DibHeader::V3(h) => h.v2.info.height,
            DibHeader::V4(h) => h.v3.v2.info.height,
            DibHeader::V5(h) => h.v4.v3.v2.info.height,
        }
    }

    pub fn compression(&self) -> BitmapCompression {
        match self {
            DibHeader::Core(_) => BitmapCompression::BiRgb,
            DibHeader::OS2V2(h) => h.compression,
            DibHeader::Info(h) => h.compression,
            DibHeader::V2(h) => h.info.compression,
            DibHeader::V3(h) => h.v2.info.compression,
            DibHeader::V4(h) => h.v3.v2.info.compression,
            DibHeader::V5(h) => h.v4.v3.v2.info.compression,
        }
    }

    pub fn image_size(&self) -> u32 {
        match self {
            DibHeader::Core(_) => 0,
            DibHeader::OS2V2(h) => h.image_size,
            DibHeader::Info(h) => h.image_size,
            DibHeader::V2(h) => h.info.image_size,
            DibHeader::V3(h) => h.v2.info.image_size,
            DibHeader::V4(h) => h.v3.v2.info.image_size,
            DibHeader::V5(h) => h.v4.v3.v2.info.image_size,
        }
    }

    pub fn color_masks(&self) -> Option<(u32, u32, u32, u32)> {
        match self {
            DibHeader::V2(h) => Some((h.red_mask, h.green_mask, h.blue_mask, 0)),
            DibHeader::V3(h) => Some((h.v2.red_mask, h.v2.green_mask, h.v2.blue_mask, h.alpha_mask)),
            DibHeader::V4(h) => Some((h.v3.v2.red_mask, h.v3.v2.green_mask, h.v3.v2.blue_mask, h.v3.alpha_mask)),
            DibHeader::V5(h) => Some((h.v4.v3.v2.red_mask, h.v4.v3.v2.green_mask, h.v4.v3.v2.blue_mask, h.v4.v3.alpha_mask)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapCoreHeader {
    pub width: u16,
    pub height: u16,
    pub planes: u16,
    pub bits_per_pixel: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct OS22XBitmapHeader {
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits_per_pixel: u16,
    pub compression: BitmapCompression,
    pub image_size: u32,
    pub x_pixels_per_meter: i32,
    pub y_pixels_per_meter: i32,
    pub colors_used: u32,
    pub important_colors: u32,
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
pub struct BitmapInfoHeader {
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits_per_pixel: u16,
    pub compression: BitmapCompression,
    pub image_size: u32,
    pub x_pixels_per_meter: i32,
    pub y_pixels_per_meter: i32,
    pub colors_used: u32,
    pub important_colors: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV2InfoHeader {
    pub info: BitmapInfoHeader,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV3InfoHeader {
    pub v2: BitmapV2InfoHeader,
    pub alpha_mask: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV4Header {
    pub v3: BitmapV3InfoHeader,
    pub cs_type: u32,
    pub endpoints: ColorSpace,
    pub gamma_red: u32,
    pub gamma_green: u32,
    pub gamma_blue: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BitmapV5Header {
    pub v4: BitmapV4Header,
    pub intent: u32,
    pub profile_data: u32,
    pub profile_size: u32,
    pub reserved: u32,
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

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BmpExtraMasks {
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub alpha_mask: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BmpPixelDataInfo {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum BmpSectionData {
    BitmapArrayHeader(BitmapArrayHeader),
    FileHeader(BitmapFileHeader),
    DibHeader(DibHeader),
    ExtraMasks(BmpExtraMasks),
    ColorTable(Vec<ColorEntry>),
    PixelData(BmpPixelDataInfo),
    IccProfile(ICCProfile),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BmpSectionInfo {
    pub start_offset: u64,
    pub data: BmpSectionData,
}
