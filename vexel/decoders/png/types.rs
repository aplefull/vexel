use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChunkKind {
    IHDR,
    PLTE,
    IDAT,
    IEND,
    TRNS,
    CHRM,
    GAMA,
    ICCP,
    SBIT,
    SRGB,
    TEXT,
    ZTXT,
    ITXT,
    BKGD,
    PHYS,
    TIME,
    SPLT,
    HIST,
    ACTL,
    FCTL,
    FDAT,
}

pub fn chunk_kind(chunk_type: &[u8; 4]) -> Option<ChunkKind> {
    match chunk_type {
        b"IHDR" => Some(ChunkKind::IHDR),
        b"PLTE" => Some(ChunkKind::PLTE),
        b"IDAT" => Some(ChunkKind::IDAT),
        b"IEND" => Some(ChunkKind::IEND),
        b"tRNS" => Some(ChunkKind::TRNS),
        b"cHRM" => Some(ChunkKind::CHRM),
        b"gAMA" => Some(ChunkKind::GAMA),
        b"iCCP" => Some(ChunkKind::ICCP),
        b"sBIT" => Some(ChunkKind::SBIT),
        b"sRGB" => Some(ChunkKind::SRGB),
        b"tEXt" => Some(ChunkKind::TEXT),
        b"zTXt" => Some(ChunkKind::ZTXT),
        b"iTXt" => Some(ChunkKind::ITXT),
        b"bKGD" => Some(ChunkKind::BKGD),
        b"pHYs" => Some(ChunkKind::PHYS),
        b"tIME" => Some(ChunkKind::TIME),
        b"sPLT" => Some(ChunkKind::SPLT),
        b"hIST" => Some(ChunkKind::HIST),
        b"acTL" => Some(ChunkKind::ACTL),
        b"fcTL" => Some(ChunkKind::FCTL),
        b"fdAT" => Some(ChunkKind::FDAT),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum ColorType {
    Grayscale = 0,
    RGB = 2,
    Indexed = 3,
    GrayscaleAlpha = 4,
    RGBA = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum CompressionMethod {
    Deflate = 0,
    None = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum FilterType {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum TransparencyData {
    Grayscale(u16),
    RGB(u16, u16, u16),
    Palette(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum BackgroundData {
    Grayscale(u16),
    RGB(u16, u16, u16),
    PaletteIndex(u8),
}

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
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

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ActlChunk {
    pub num_frames: u32,
    pub num_plays: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
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

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PngFrame {
    pub fctl_info: FctlChunk,
    pub fdat: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SuggestedPaletteSample {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub alpha: u16,
    pub frequency: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SuggestedPalette {
    pub name: String,
    pub sample_depth: u8,
    pub samples: Vec<SuggestedPaletteSample>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum SignificantBits {
    Grayscale { gray: u8 },
    RGB { red: u8, green: u8, blue: u8 },
    Indexed { red: u8, green: u8, blue: u8 },
    GrayscaleAlpha { gray: u8, alpha: u8 },
    RGBA { red: u8, green: u8, blue: u8, alpha: u8 },
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ImageTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
