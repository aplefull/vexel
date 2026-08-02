#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Soc,
    Sot,
    Sod,
    Eoc,
    Siz,
    Cap,
    Cod,
    Coc,
    Rgn,
    Qcd,
    Qcc,
    Poc,
    Tlm,
    Plm,
    Plt,
    Ppm,
    Ppt,
    Sop,
    Eph,
    Crg,
    Com,
    Unknown(u16),
}

impl Marker {
    pub fn from_u16(value: u16) -> Self {
        match value {
            0xFF4F => Marker::Soc,
            0xFF90 => Marker::Sot,
            0xFF93 => Marker::Sod,
            0xFFD9 => Marker::Eoc,
            0xFF51 => Marker::Siz,
            0xFF50 => Marker::Cap,
            0xFF52 => Marker::Cod,
            0xFF53 => Marker::Coc,
            0xFF5E => Marker::Rgn,
            0xFF5C => Marker::Qcd,
            0xFF5D => Marker::Qcc,
            0xFF5F => Marker::Poc,
            0xFF55 => Marker::Tlm,
            0xFF57 => Marker::Plm,
            0xFF58 => Marker::Plt,
            0xFF60 => Marker::Ppm,
            0xFF61 => Marker::Ppt,
            0xFF91 => Marker::Sop,
            0xFF92 => Marker::Eph,
            0xFF63 => Marker::Crg,
            0xFF64 => Marker::Com,
            other => Marker::Unknown(other),
        }
    }

    pub fn is_reserved_delimiter(&self) -> bool {
        matches!(self, Marker::Unknown(value) if (0xFF30..=0xFF3F).contains(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxType {
    Signature,
    FileType,
    Header,
    ImageHeader,
    Colr,
    Codestream,
    Palette,
    ComponentMapping,
    ChannelDefinition,
    BitsPerComponent,
    Unknown(u32),
}

impl BoxType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0x6A502020 => BoxType::Signature,
            0x66747970 => BoxType::FileType,
            0x6A703268 => BoxType::Header,
            0x69686472 => BoxType::ImageHeader,
            0x636F6C72 => BoxType::Colr,
            0x6A703263 => BoxType::Codestream,
            0x70636C72 => BoxType::Palette,
            0x636D6170 => BoxType::ComponentMapping,
            0x63646566 => BoxType::ChannelDefinition,
            0x62706363 => BoxType::BitsPerComponent,
            other => BoxType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colorspace {
    Cmyk,
    Srgb,
    Gray,
    Sycc,
    Eycc,
    Unknown(u32),
}

impl Colorspace {
    pub fn from_u32(value: u32) -> Self {
        match value {
            12 => Colorspace::Cmyk,
            16 => Colorspace::Srgb,
            17 => Colorspace::Gray,
            18 => Colorspace::Sycc,
            24 => Colorspace::Eycc,
            other => Colorspace::Unknown(other),
        }
    }
}

pub const JP2_SIGNATURE: [u8; 12] = [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20, 0x0D, 0x0A, 0x87, 0x0A];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeBlockStyle(u32);

impl CodeBlockStyle {
    const LAZY: u32 = 0x01;
    const RESET: u32 = 0x02;
    const TERMALL: u32 = 0x04;
    const VSC: u32 = 0x08;
    const SEGSYM: u32 = 0x20;
    const HT: u32 = 0x40;
    const HT_MIXED: u32 = 0x80;

    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn lazy(&self) -> bool {
        self.0 & Self::LAZY != 0
    }

    pub fn reset(&self) -> bool {
        self.0 & Self::RESET != 0
    }

    pub fn terminate_all(&self) -> bool {
        self.0 & Self::TERMALL != 0
    }

    pub fn vertically_causal(&self) -> bool {
        self.0 & Self::VSC != 0
    }

    pub fn segmentation_symbols(&self) -> bool {
        self.0 & Self::SEGSYM != 0
    }

    pub fn high_throughput(&self) -> bool {
        self.0 & Self::HT != 0
    }

    pub fn high_throughput_mixed(&self) -> bool {
        self.0 & Self::HT_MIXED != 0
    }
}

pub const T1_NUMCTXS_ZC: usize = 9;
pub const T1_NUMCTXS_SC: usize = 5;
pub const T1_NUMCTXS_MAG: usize = 3;
pub const T1_NUMCTXS_AGG: usize = 1;
pub const T1_NUMCTXS_UNI: usize = 1;

pub const T1_CTXNO_ZC: usize = 0;
pub const T1_CTXNO_SC: usize = T1_CTXNO_ZC + T1_NUMCTXS_ZC;
pub const T1_CTXNO_MAG: usize = T1_CTXNO_SC + T1_NUMCTXS_SC;
pub const T1_CTXNO_AGG: usize = T1_CTXNO_MAG + T1_NUMCTXS_MAG;
pub const T1_CTXNO_UNI: usize = T1_CTXNO_AGG + T1_NUMCTXS_AGG;
pub const T1_NUMCTXS: usize = T1_CTXNO_UNI + T1_NUMCTXS_UNI;

pub const T1_SIGMA_0: u32 = 1 << 0;
pub const T1_SIGMA_1: u32 = 1 << 1;
pub const T1_SIGMA_2: u32 = 1 << 2;
pub const T1_SIGMA_3: u32 = 1 << 3;
pub const T1_SIGMA_4: u32 = 1 << 4;
pub const T1_SIGMA_5: u32 = 1 << 5;
pub const T1_SIGMA_7: u32 = 1 << 7;
pub const T1_SIGMA_15: u32 = 1 << 15;
pub const T1_SIGMA_16: u32 = 1 << 16;
pub const T1_SIGMA_17: u32 = 1 << 17;

pub const T1_CHI_0_I: u32 = 18;
pub const T1_CHI_1_I: u32 = 19;
pub const T1_MU_0: u32 = 1 << 20;
pub const T1_PI_0: u32 = 1 << 21;
pub const T1_CHI_2_I: u32 = 22;
pub const T1_CHI_5_I: u32 = 31;

pub const T1_SIGMA_NW: u32 = T1_SIGMA_0;
pub const T1_SIGMA_N: u32 = T1_SIGMA_1;
pub const T1_SIGMA_NE: u32 = T1_SIGMA_2;
pub const T1_SIGMA_W: u32 = T1_SIGMA_3;
pub const T1_SIGMA_THIS: u32 = T1_SIGMA_4;
pub const T1_SIGMA_E: u32 = T1_SIGMA_5;
pub const T1_SIGMA_SW: u32 = 1 << 6;
pub const T1_SIGMA_S: u32 = T1_SIGMA_7;
pub const T1_SIGMA_SE: u32 = 1 << 8;

pub const T1_SIGMA_NEIGHBOURS: u32 =
    T1_SIGMA_NW | T1_SIGMA_N | T1_SIGMA_NE | T1_SIGMA_W | T1_SIGMA_E | T1_SIGMA_SW | T1_SIGMA_S | T1_SIGMA_SE;

pub const T1_MU_THIS: u32 = T1_MU_0;
pub const T1_PI_THIS: u32 = T1_PI_0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandOrient {
    Ll,
    Hl,
    Lh,
    Hh,
}

impl BandOrient {
    pub fn from_index(index: u32) -> Self {
        match index {
            0 => BandOrient::Ll,
            1 => BandOrient::Hl,
            2 => BandOrient::Lh,
            _ => BandOrient::Hh,
        }
    }

    pub fn lut_index(&self) -> usize {
        match self {
            BandOrient::Ll => 0,
            BandOrient::Hl => 1,
            BandOrient::Lh => 2,
            BandOrient::Hh => 3,
        }
    }

    pub fn horizontal_high_pass(&self) -> bool {
        matches!(self, BandOrient::Hl | BandOrient::Hh)
    }

    pub fn vertical_high_pass(&self) -> bool {
        matches!(self, BandOrient::Lh | BandOrient::Hh)
    }

    pub fn log2_gain(&self) -> i32 {
        match self {
            BandOrient::Ll => 0,
            BandOrient::Hl | BandOrient::Lh => 1,
            BandOrient::Hh => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionOrder {
    Lrcp,
    Rlcp,
    Rpcl,
    Pcrl,
    Cprl,
}

impl ProgressionOrder {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ProgressionOrder::Lrcp),
            1 => Some(ProgressionOrder::Rlcp),
            2 => Some(ProgressionOrder::Rpcl),
            3 => Some(ProgressionOrder::Pcrl),
            4 => Some(ProgressionOrder::Cprl),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantStyle {
    NoQuantization,
    ScalarDerived,
    ScalarExpounded,
}

impl QuantStyle {
    pub fn from_bits(value: u8) -> Option<Self> {
        match value {
            0 => Some(QuantStyle::NoQuantization),
            1 => Some(QuantStyle::ScalarDerived),
            2 => Some(QuantStyle::ScalarExpounded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StepSize {
    pub expn: i32,
    pub mant: i32,
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub dx: u32,
    pub dy: u32,
    pub prec: u32,
    pub signed: bool,
}

#[derive(Debug, Clone)]
pub struct SizInfo {
    pub xsiz: u32,
    pub ysiz: u32,
    pub x0siz: u32,
    pub y0siz: u32,
    pub xtsiz: u32,
    pub ytsiz: u32,
    pub xt0siz: u32,
    pub yt0siz: u32,
    pub components: Vec<ComponentInfo>,
}

impl SizInfo {
    pub fn width(&self) -> u32 {
        self.xsiz.saturating_sub(self.x0siz)
    }

    pub fn height(&self) -> u32 {
        self.ysiz.saturating_sub(self.y0siz)
    }

    pub fn num_tiles_x(&self) -> u32 {
        if self.xtsiz == 0 {
            return 0;
        }
        ceil_div_u32(self.xsiz.saturating_sub(self.xt0siz), self.xtsiz)
    }

    pub fn num_tiles_y(&self) -> u32 {
        if self.ytsiz == 0 {
            return 0;
        }
        ceil_div_u32(self.ysiz.saturating_sub(self.yt0siz), self.ytsiz)
    }
}

#[derive(Debug, Clone)]
pub struct CodingStyle {
    pub num_resolutions: u32,
    pub cblk_width_exp: u32,
    pub cblk_height_exp: u32,
    pub cblk_style: CodeBlockStyle,
    pub reversible: bool,
    pub precinct_sizes: Vec<u32>,
    pub uses_precincts: bool,
}

impl Default for CodingStyle {
    fn default() -> Self {
        Self {
            num_resolutions: 6,
            cblk_width_exp: 6,
            cblk_height_exp: 6,
            cblk_style: CodeBlockStyle::default(),
            reversible: true,
            precinct_sizes: Vec::new(),
            uses_precincts: false,
        }
    }
}

impl CodingStyle {
    pub fn precinct_exponents(&self, resno: usize) -> (u32, u32) {
        if !self.uses_precincts {
            return (15, 15);
        }
        match self.precinct_sizes.get(resno) {
            Some(value) => (value & 0x0F, (value >> 4) & 0x0F),
            None => (15, 15),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuantInfo {
    pub style: QuantStyle,
    pub guard_bits: u32,
    pub step_sizes: Vec<StepSize>,
}

impl Default for QuantInfo {
    fn default() -> Self {
        Self { style: QuantStyle::NoQuantization, guard_bits: 2, step_sizes: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProgressionChange {
    pub resno0: u32,
    pub compno0: u32,
    pub layno1: u32,
    pub resno1: u32,
    pub compno1: u32,
    pub order: ProgressionOrder,
}

#[derive(Debug, Clone, Default)]
pub struct ComponentCoding {
    pub coding: CodingStyle,
    pub quant: QuantInfo,
    pub roi_shift: u32,
}

pub fn ceil_div_i32(numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        return 0;
    }
    let n = numerator as i64;
    let d = denominator as i64;
    let quotient = n.div_euclid(d);
    if n.rem_euclid(d) != 0 {
        (quotient + 1) as i32
    } else {
        quotient as i32
    }
}

pub fn ceil_div_u32(numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        return 0;
    }
    numerator.div_ceil(denominator)
}

pub fn ceil_div_pow2(value: i32, exp: u32) -> i32 {
    if exp == 0 {
        return value;
    }
    let divisor = 1i64 << exp;
    let value = value as i64;
    ((value + divisor - 1) >> exp) as i32
}

pub fn int_floorlog2(mut value: u32) -> u32 {
    let mut result = 0;
    while value > 1 {
        value >>= 1;
        result += 1;
    }
    result
}
