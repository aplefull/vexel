use serde::Serialize;
use tsify::Tsify;

pub const MB_SIZE: usize = 16;
pub const MB_BLOCKS: usize = 16;
pub const MAX_CHANNELS: usize = 16;
pub const LOG_MAX_TILES: u32 = 12;

pub const CODEC_VERSION: u32 = 1;
pub const CODEC_SUBVERSION: u32 = 0;
pub const CODEC_SUBVERSION_NEWSCALING_SOFT_TILES: u32 = 1;
pub const CODEC_SUBVERSION_NEWSCALING_HARD_TILES: u32 = 9;

pub const CONTEXTX: usize = 8;
pub const CTDC: usize = 5;
pub const NUM_VLC_TABLES: usize = CONTEXTX * 2 + CTDC;
pub const AVG_NDIFF: i32 = 3;

pub const SHIFTZERO: i32 = 1;
pub const QPFRACBITS: i32 = 2;

pub const CONTAINER_MAGIC: [u8; 4] = [0x49, 0x49, 0xbc, 0x01];
pub const CODESTREAM_MAGIC: &[u8; 8] = b"WMPHOTO\0";

pub const TAG_PIXEL_FORMAT: u16 = 0xbc01;
pub const TAG_TRANSFORMATION: u16 = 0xbc02;
pub const TAG_IMAGE_WIDTH: u16 = 0xbc80;
pub const TAG_IMAGE_HEIGHT: u16 = 0xbc81;
pub const TAG_IMAGE_OFFSET: u16 = 0xbcc0;
pub const TAG_IMAGE_BYTE_COUNT: u16 = 0xbcc1;
pub const TAG_ALPHA_OFFSET: u16 = 0xbcc2;
pub const TAG_ALPHA_BYTE_COUNT: u16 = 0xbcc3;
pub const TAG_ICC_PROFILE: u16 = 0x8773;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum ColorFormat {
    YOnly,
    Yuv420,
    Yuv422,
    Yuv444,
    Cmyk,
    NComponent,
    Rgb,
    Rgbe,
}

impl ColorFormat {
    pub fn from_bits(value: u32) -> Option<Self> {
        match value {
            0 => Some(ColorFormat::YOnly),
            1 => Some(ColorFormat::Yuv420),
            2 => Some(ColorFormat::Yuv422),
            3 => Some(ColorFormat::Yuv444),
            4 => Some(ColorFormat::Cmyk),
            6 => Some(ColorFormat::NComponent),
            7 => Some(ColorFormat::Rgb),
            8 => Some(ColorFormat::Rgbe),
            _ => None,
        }
    }

    pub fn is_chroma_subsampled(self) -> bool {
        matches!(self, ColorFormat::Yuv420 | ColorFormat::Yuv422)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum BitDepth {
    Bd1White,
    Bd8,
    Bd16,
    Bd16S,
    Bd16F,
    Bd32,
    Bd32S,
    Bd32F,
    Bd5,
    Bd10,
    Bd565,
    Bd1Black,
}

impl BitDepth {
    pub fn from_bits(value: u32) -> Option<Self> {
        match value {
            0 => Some(BitDepth::Bd1White),
            1 => Some(BitDepth::Bd8),
            2 => Some(BitDepth::Bd16),
            3 => Some(BitDepth::Bd16S),
            4 => Some(BitDepth::Bd16F),
            5 => Some(BitDepth::Bd32),
            6 => Some(BitDepth::Bd32S),
            7 => Some(BitDepth::Bd32F),
            8 => Some(BitDepth::Bd5),
            9 => Some(BitDepth::Bd10),
            10 => Some(BitDepth::Bd565),
            0xf => Some(BitDepth::Bd1Black),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitstreamFormat {
    Spatial,
    Frequency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    None,
    One,
    Two,
}

impl Overlap {
    pub fn from_bits(value: u32) -> Option<Self> {
        match value {
            0 => Some(Overlap::None),
            1 => Some(Overlap::One),
            2 => Some(Overlap::Two),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subband {
    All,
    NoFlexbits,
    NoHighpass,
    DcOnly,
    Isolated,
}

impl Subband {
    pub fn from_bits(value: u32) -> Self {
        match value {
            1 => Subband::NoFlexbits,
            2 => Subband::NoHighpass,
            3 => Subband::DcOnly,
            4 => Subband::Isolated,
            _ => Subband::All,
        }
    }

    pub fn has_highpass(self) -> bool {
        self < Subband::NoHighpass
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    None,
    FlipV,
    FlipH,
    FlipVH,
    Rcw,
    RcwFlipV,
    RcwFlipH,
    RcwFlipVH,
}

impl Orientation {
    pub fn from_bits(value: u32) -> Self {
        match value & 7 {
            1 => Orientation::FlipV,
            2 => Orientation::FlipH,
            3 => Orientation::FlipVH,
            4 => Orientation::Rcw,
            5 => Orientation::RcwFlipV,
            6 => Orientation::RcwFlipH,
            7 => Orientation::RcwFlipVH,
            _ => Orientation::None,
        }
    }

    pub fn rotates(self) -> bool {
        matches!(self, Orientation::Rcw | Orientation::RcwFlipV | Orientation::RcwFlipH | Orientation::RcwFlipVH)
    }

    pub fn flips_h(self) -> bool {
        matches!(self, Orientation::FlipH | Orientation::FlipVH | Orientation::RcwFlipH | Orientation::RcwFlipVH)
    }

    pub fn flips_v(self) -> bool {
        matches!(self, Orientation::FlipV | Orientation::FlipVH | Orientation::RcwFlipV | Orientation::RcwFlipVH)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Dc,
    Lp,
    Ac,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Quantizer {
    pub index: u8,
    pub qp: i32,
    pub offset: i32,
}

impl Quantizer {
    pub fn from_index(index: u8, shift: i32, scaled_arith: bool) -> Self {
        let mut q = Quantizer { index, ..Default::default() };
        q.remap(shift, scaled_arith);
        q
    }

    fn remap(&mut self, shift: i32, scaled_arith: bool) {
        let index = i32::from(self.index);

        if index == 0 {
            self.qp = 1;
            self.offset = 0;
            return;
        }

        let (man, exp) = if scaled_arith {
            if index < 16 {
                (index, shift)
            } else {
                (16 + (index & 0xf), ((index >> 4) - 1) + shift)
            }
        } else {
            let base = -QPFRACBITS;
            if index < 32 {
                ((index + 3) >> 2, base + 2)
            } else if index < 48 {
                ((16 + (index & 0xf) + 1) >> 1, ((index >> 4) - 1) + 1 + base)
            } else {
                (16 + (index & 0xf), ((index >> 4) - 1) + base)
            }
        };

        self.qp = if exp >= 0 { man << exp } else { man >> (-exp) };
        self.offset = (self.qp * 3 + 1) >> 3;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum PixelFormatKind {
    BlackWhite1,
    Gray8,
    Gray16,
    Gray16Fixed,
    Gray16Half,
    Gray32Fixed,
    Gray32Float,
    Rgb24,
    Bgr24,
    Bgr32,
    Rgb48,
    Rgb48Fixed,
    Rgb48Half,
    Rgb96Fixed,
    Rgb128Float,
    Rgbe32,
    Bgra32,
    Rgba64,
    Rgba64Fixed,
    Rgba64Half,
    Rgba128Fixed,
    Rgba128Float,
    Bgr555,
    Bgr565,
    Bgr101010,
    Cmyk32,
    Cmyk64,
    Cmyka40,
    Cmyka80,
    NChannel,
    Unknown,
}
