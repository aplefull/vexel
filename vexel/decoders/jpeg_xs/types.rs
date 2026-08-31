use serde::Serialize;
use tsify::Tsify;

pub const MAX_NDECOMP_H: usize = 5;
pub const MAX_NDECOMP_V: usize = 2;
pub const MAX_NCOMPS: usize = 4;
pub const MAX_NFILTER_TYPES: usize = 2 * MAX_NDECOMP_V + MAX_NDECOMP_H + 1;
pub const MAX_NBANDS: usize = MAX_NCOMPS * MAX_NFILTER_TYPES;
pub const MAX_PACKETS: usize = MAX_NBANDS + MAX_NCOMPS * 3 * ((1 << (MAX_NDECOMP_V - 1)) - 1);
pub const MAX_PREC_COLS: usize = 130;
pub const MAX_GCLI: i32 = 15;

pub const MARKER_SOC: u16 = 0xff10;
pub const MARKER_EOC: u16 = 0xff11;
pub const MARKER_PIH: u16 = 0xff12;
pub const MARKER_CDT: u16 = 0xff13;
pub const MARKER_WGT: u16 = 0xff14;
pub const MARKER_COM: u16 = 0xff15;
pub const MARKER_NLT: u16 = 0xff16;
pub const MARKER_CWD: u16 = 0xff17;
pub const MARKER_CTS: u16 = 0xff18;
pub const MARKER_CRG: u16 = 0xff19;
pub const MARKER_SLH: u16 = 0xff20;
pub const MARKER_CAP: u16 = 0xff50;

pub const GCLI_METHOD_NBITS: u32 = 2;
pub const PREC_HDR_PREC_SIZE: u32 = 24;
pub const PREC_HDR_QUANTIZATION_SIZE: u32 = 8;
pub const PREC_HDR_REFINEMENT_SIZE: u32 = 8;
pub const PREC_HDR_ALIGNMENT: u32 = 8;
pub const PKT_HDR_DATA_SIZE_SHORT: u32 = 15;
pub const PKT_HDR_DATA_SIZE_LONG: u32 = 20;
pub const PKT_HDR_GCLI_SIZE_SHORT: u32 = 13;
pub const PKT_HDR_GCLI_SIZE_LONG: u32 = 20;
pub const PKT_HDR_SIGN_SIZE_SHORT: u32 = 11;
pub const PKT_HDR_SIGN_SIZE_LONG: u32 = 15;
pub const PKT_HDR_ALIGNMENT: u32 = 8;
pub const SUBPKT_ALIGNMENT: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum ColorTransform {
    None,
    Rct,
    Tetrix,
}

impl ColorTransform {
    pub fn from_bits(value: u8) -> Self {
        match value {
            0x01 => ColorTransform::Rct,
            0x03 => ColorTransform::Tetrix,
            _ => ColorTransform::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum NltType {
    None,
    Quadratic,
    Extended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum CfaPattern {
    Rggb,
    Bggr,
    Grbg,
    Gbrg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum TetrixMode {
    Full,
    Inline,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Tsify)]
pub struct QuadraticNlt {
    pub sigma: u16,
    pub alpha: u16,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Tsify)]
pub struct ExtendedNlt {
    pub t1: u32,
    pub t2: u32,
    pub e: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
pub struct TetrixParams {
    pub cf: TetrixMode,
    pub e1: u8,
    pub e2: u8,
}

impl Default for TetrixParams {
    fn default() -> Self {
        TetrixParams { cf: TetrixMode::Full, e1: 0, e2: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bitstream_size: u64,
    pub profile: u16,
    pub level: u8,
    pub sublevel: u8,
    pub cap_bits: u16,
    pub color_transform: ColorTransform,
    pub cw: u16,
    pub slice_height: u32,
    pub n_g: u8,
    pub s_s: u8,
    pub bw: u8,
    pub fq: u8,
    pub b_r: u8,
    pub fslc: u8,
    pub ppoc: u8,
    pub nlx: u8,
    pub nly: u8,
    pub lh: u8,
    pub rl: u8,
    pub qpih: u8,
    pub fs: u8,
    pub rm: u8,
    pub sd: u8,
    pub lvl_gains: Vec<u8>,
    pub lvl_priorities: Vec<u8>,
    pub tnlt: NltType,
    pub quadratic: QuadraticNlt,
    pub extended: ExtendedNlt,
    pub tetrix: TetrixParams,
    pub cfa_pattern: CfaPattern,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bitstream_size: 0,
            profile: 0,
            level: 0,
            sublevel: 0,
            cap_bits: 0,
            color_transform: ColorTransform::None,
            cw: 0,
            slice_height: 16,
            n_g: 4,
            s_s: 8,
            bw: 20,
            fq: 0,
            b_r: 4,
            fslc: 0,
            ppoc: 0,
            nlx: 5,
            nly: 2,
            lh: 0,
            rl: 0,
            qpih: 0,
            fs: 0,
            rm: 0,
            sd: 0,
            lvl_gains: Vec::new(),
            lvl_priorities: Vec::new(),
            tnlt: NltType::None,
            quadratic: QuadraticNlt::default(),
            extended: ExtendedNlt::default(),
            tetrix: TetrixParams::default(),
            cfa_pattern: CfaPattern::Rggb,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageParams {
    pub width: u32,
    pub height: u32,
    pub ncomps: usize,
    pub depth: u8,
    pub sx: [u8; MAX_NCOMPS],
    pub sy: [u8; MAX_NCOMPS],
}

impl Default for ImageParams {
    fn default() -> Self {
        ImageParams { width: 0, height: 0, ncomps: 0, depth: 8, sx: [1; MAX_NCOMPS], sy: [1; MAX_NCOMPS] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Raw4Bits,
    UnaryUnsignedBounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prediction {
    None,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    None,
    SigFlagsZrf,
    SigFlagsZrcsf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcliMethod {
    pub alphabet: Alphabet,
    pub prediction: Prediction,
    pub run: RunMode,
}

impl GcliMethod {
    pub const RAW: GcliMethod =
        GcliMethod { alphabet: Alphabet::Raw4Bits, prediction: Prediction::None, run: RunMode::None };

    pub fn from_signaling(signaling: u8, rm: u8) -> Self {
        let uses_run = signaling & 0x2 != 0;
        let run = if !uses_run {
            RunMode::None
        } else if rm == 1 {
            RunMode::SigFlagsZrcsf
        } else {
            RunMode::SigFlagsZrf
        };

        let prediction = if signaling & 0x1 != 0 { Prediction::Vertical } else { Prediction::None };

        GcliMethod { alphabet: Alphabet::UnaryUnsignedBounded, prediction, run }
    }

    pub fn uses_sig_flags(&self) -> bool {
        matches!(self.run, RunMode::SigFlagsZrf | RunMode::SigFlagsZrcsf)
    }

    pub fn is_raw(&self) -> bool {
        self.alphabet == Alphabet::Raw4Bits
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegXsMarkerInfo {
    pub offset: u64,
    pub marker: String,
    pub length: u32,
}
