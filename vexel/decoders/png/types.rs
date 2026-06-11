use serde::Serialize;
use tsify::Tsify;
use crate::utils::icc::ICCProfile;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PngChunk {
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

pub fn get_chunk(chunk_type: &[u8; 4]) -> Option<PngChunk> {
    match chunk_type {
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
pub enum RenderingIntent {
    Perceptual = 0,
    RelativeColorimetric = 1,
    Saturation = 2,
    AbsoluteColorimetric = 3,
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

pub struct CrcCalculator {
    table: [[u32; 256]; 4],
}

impl CrcCalculator {
    pub fn new() -> Self {
        let mut t0 = [0u32; 256];
        for n in 0..256 {
            let mut c = n as u32;
            for _ in 0..8 {
                if c & 1 == 1 {
                    c = 0xedb88320u32 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            t0[n] = c;
        }
        let mut t1 = [0u32; 256];
        let mut t2 = [0u32; 256];
        let mut t3 = [0u32; 256];
        for n in 0..256 {
            t1[n] = (t0[n] >> 8) ^ t0[(t0[n] & 0xff) as usize];
            t2[n] = (t1[n] >> 8) ^ t0[(t1[n] & 0xff) as usize];
            t3[n] = (t2[n] >> 8) ^ t0[(t2[n] & 0xff) as usize];
        }
        Self { table: [t0, t1, t2, t3] }
    }

    fn update_crc(&self, crc: u32, buf: &[u8]) -> u32 {
        let [t0, t1, t2, t3] = &self.table;
        let mut c = crc;
        let mut chunks = buf.chunks_exact(4);
        for chunk in chunks.by_ref() {
            let w = c ^ u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            c = t3[(w & 0xff) as usize]
                ^ t2[((w >> 8) & 0xff) as usize]
                ^ t1[((w >> 16) & 0xff) as usize]
                ^ t0[(w >> 24) as usize];
        }
        for &b in chunks.remainder() {
            c = t0[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
        }
        c
    }

    pub fn calculate_crc(&self, data: &[u8]) -> u32 {
        self.update_crc(0xffffffff, data) ^ 0xffffffff
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
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
pub struct PhysicalDimensions {
    pub pixels_per_unit_x: u32,
    pub pixels_per_unit_y: u32,
    pub unit: PhysicalUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum PhysicalUnit {
    Unknown,
    Meter,
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

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IhdrChunkData {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: ColorType,
    pub compression_method: u8,
    pub filter_method: u8,
    pub interlace_method: u8,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PlteChunkData {
    pub entries: Vec<[u8; 3]>,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IdatChunkData {
    pub data_length: u32,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GamaChunkData {
    pub gamma: f32,
    pub gamma_raw: u32,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ChrmChunkData {
    pub chromaticities: Chromaticities,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TrnsChunkData {
    pub transparency: TransparencyData,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BkgdChunkData {
    pub background: BackgroundData,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PhysChunkData {
    pub physical_dimensions: PhysicalDimensions,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SbitChunkData {
    pub significant_bits: SignificantBits,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TimeChunkData {
    pub time: ImageTime,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TextChunkData {
    pub text: PngText,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SrgbChunkData {
    pub rendering_intent: RenderingIntent,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IccpChunkData {
    pub profile_name: String,
    pub profile: ICCProfile,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SpltChunkData {
    pub palette: SuggestedPalette,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HistChunkData {
    pub frequencies: Vec<u16>,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ActlChunkData {
    pub actl: ActlChunk,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FctlChunkData {
    pub fctl: FctlChunk,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FdatChunkData {
    pub sequence_number: u32,
    pub data_length: u32,
    pub crc: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum PngChunkData {
    IHDR(IhdrChunkData),
    PLTE(PlteChunkData),
    IDAT(IdatChunkData),
    GAMA(GamaChunkData),
    CHRM(ChrmChunkData),
    TRNS(TrnsChunkData),
    BKGD(BkgdChunkData),
    PHYS(PhysChunkData),
    SBIT(SbitChunkData),
    TIME(TimeChunkData),
    TEXT(TextChunkData),
    ZTXT(TextChunkData),
    ITXT(TextChunkData),
    SRGB(SrgbChunkData),
    ICCP(IccpChunkData),
    SPLT(SpltChunkData),
    HIST(HistChunkData),
    ACTL(ActlChunkData),
    FCTL(FctlChunkData),
    FDAT(FdatChunkData),
    IEND { crc: u32 },
    Unknown { chunk_type: String, length: u32, crc: u32 },
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PngChunkInfo {
    pub start_offset: u64,
    pub chunk_type: String,
    pub length: u32,
    pub raw_bytes: Vec<u8>,
    pub data: PngChunkData,
}
