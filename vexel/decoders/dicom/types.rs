#![allow(dead_code)]

use serde::Serialize;
use tsify::Tsify;

pub const fn tag(group: u16, element: u16) -> u32 {
    ((group as u32) << 16) | element as u32
}

pub const fn tag_group(tag: u32) -> u16 {
    (tag >> 16) as u16
}

pub const fn tag_element(tag: u32) -> u16 {
    (tag & 0xFFFF) as u16
}

pub mod tags {
    use super::tag;

    pub const FILE_META_GROUP_LENGTH: u32 = tag(0x0002, 0x0000);
    pub const TRANSFER_SYNTAX_UID: u32 = tag(0x0002, 0x0010);

    pub const SOP_CLASS_UID: u32 = tag(0x0008, 0x0016);
    pub const MODALITY: u32 = tag(0x0008, 0x0060);
    pub const RECOMMENDED_DISPLAY_FRAME_RATE: u32 = tag(0x0008, 0x2144);

    pub const FRAME_TIME: u32 = tag(0x0018, 0x1063);
    pub const FRAME_TIME_VECTOR: u32 = tag(0x0018, 0x1065);

    pub const SAMPLES_PER_PIXEL: u32 = tag(0x0028, 0x0002);
    pub const PHOTOMETRIC_INTERPRETATION: u32 = tag(0x0028, 0x0004);
    pub const PLANAR_CONFIGURATION: u32 = tag(0x0028, 0x0006);
    pub const NUMBER_OF_FRAMES: u32 = tag(0x0028, 0x0008);
    pub const ROWS: u32 = tag(0x0028, 0x0010);
    pub const COLUMNS: u32 = tag(0x0028, 0x0011);
    pub const BITS_ALLOCATED: u32 = tag(0x0028, 0x0100);
    pub const BITS_STORED: u32 = tag(0x0028, 0x0101);
    pub const HIGH_BIT: u32 = tag(0x0028, 0x0102);
    pub const PIXEL_REPRESENTATION: u32 = tag(0x0028, 0x0103);
    pub const SMALLEST_IMAGE_PIXEL_VALUE: u32 = tag(0x0028, 0x0106);
    pub const LARGEST_IMAGE_PIXEL_VALUE: u32 = tag(0x0028, 0x0107);

    pub const RED_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1101);
    pub const GREEN_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1102);
    pub const BLUE_PALETTE_DESCRIPTOR: u32 = tag(0x0028, 0x1103);
    pub const RED_PALETTE_DATA: u32 = tag(0x0028, 0x1201);
    pub const GREEN_PALETTE_DATA: u32 = tag(0x0028, 0x1202);
    pub const BLUE_PALETTE_DATA: u32 = tag(0x0028, 0x1203);
    pub const SEGMENTED_RED_PALETTE_DATA: u32 = tag(0x0028, 0x1221);
    pub const SEGMENTED_GREEN_PALETTE_DATA: u32 = tag(0x0028, 0x1222);
    pub const SEGMENTED_BLUE_PALETTE_DATA: u32 = tag(0x0028, 0x1223);

    pub const WINDOW_CENTER: u32 = tag(0x0028, 0x1050);
    pub const WINDOW_WIDTH: u32 = tag(0x0028, 0x1051);
    pub const RESCALE_INTERCEPT: u32 = tag(0x0028, 0x1052);
    pub const RESCALE_SLOPE: u32 = tag(0x0028, 0x1053);
    pub const VOI_LUT_FUNCTION: u32 = tag(0x0028, 0x1056);

    pub const MODALITY_LUT_SEQUENCE: u32 = tag(0x0028, 0x3000);
    pub const LUT_DESCRIPTOR: u32 = tag(0x0028, 0x3002);
    pub const LUT_DATA: u32 = tag(0x0028, 0x3006);
    pub const VOI_LUT_SEQUENCE: u32 = tag(0x0028, 0x3010);

    pub const FRAME_VOI_LUT_SEQUENCE: u32 = tag(0x0028, 0x9132);
    pub const PIXEL_VALUE_TRANSFORMATION_SEQUENCE: u32 = tag(0x0028, 0x9145);
    pub const SHARED_FUNCTIONAL_GROUPS_SEQUENCE: u32 = tag(0x5200, 0x9229);
    pub const PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE: u32 = tag(0x5200, 0x9230);

    pub const ICON_IMAGE_SEQUENCE: u32 = tag(0x0088, 0x0200);

    pub const FLOAT_PIXEL_DATA: u32 = tag(0x7FE0, 0x0008);
    pub const DOUBLE_FLOAT_PIXEL_DATA: u32 = tag(0x7FE0, 0x0009);
    pub const PIXEL_DATA: u32 = tag(0x7FE0, 0x0010);

    pub const ITEM: u32 = tag(0xFFFE, 0xE000);
    pub const ITEM_DELIMITER: u32 = tag(0xFFFE, 0xE00D);
    pub const SEQUENCE_DELIMITER: u32 = tag(0xFFFE, 0xE0DD);
}

pub fn tag_name(t: u32) -> Option<&'static str> {
    let name = match t {
        0x0002_0000 => "File Meta Information Group Length",
        0x0002_0001 => "File Meta Information Version",
        0x0002_0002 => "Media Storage SOP Class UID",
        0x0002_0003 => "Media Storage SOP Instance UID",
        0x0002_0010 => "Transfer Syntax UID",
        0x0002_0012 => "Implementation Class UID",
        0x0002_0013 => "Implementation Version Name",
        0x0008_0005 => "Specific Character Set",
        0x0008_0008 => "Image Type",
        0x0008_0016 => "SOP Class UID",
        0x0008_0018 => "SOP Instance UID",
        0x0008_0020 => "Study Date",
        0x0008_0030 => "Study Time",
        0x0008_0060 => "Modality",
        0x0008_0070 => "Manufacturer",
        0x0008_0080 => "Institution Name",
        0x0008_1030 => "Study Description",
        0x0008_103E => "Series Description",
        0x0008_2144 => "Recommended Display Frame Rate",
        0x0010_0010 => "Patient's Name",
        0x0010_0020 => "Patient ID",
        0x0010_0030 => "Patient's Birth Date",
        0x0010_0040 => "Patient's Sex",
        0x0018_0050 => "Slice Thickness",
        0x0018_0060 => "KVP",
        0x0018_1063 => "Frame Time",
        0x0018_1065 => "Frame Time Vector",
        0x0018_5100 => "Patient Position",
        0x0020_000D => "Study Instance UID",
        0x0020_000E => "Series Instance UID",
        0x0020_0011 => "Series Number",
        0x0020_0013 => "Instance Number",
        0x0020_0032 => "Image Position (Patient)",
        0x0020_0037 => "Image Orientation (Patient)",
        0x0028_0002 => "Samples per Pixel",
        0x0028_0004 => "Photometric Interpretation",
        0x0028_0006 => "Planar Configuration",
        0x0028_0008 => "Number of Frames",
        0x0028_0010 => "Rows",
        0x0028_0011 => "Columns",
        0x0028_0030 => "Pixel Spacing",
        0x0028_0034 => "Pixel Aspect Ratio",
        0x0028_0100 => "Bits Allocated",
        0x0028_0101 => "Bits Stored",
        0x0028_0102 => "High Bit",
        0x0028_0103 => "Pixel Representation",
        0x0028_0106 => "Smallest Image Pixel Value",
        0x0028_0107 => "Largest Image Pixel Value",
        0x0028_1050 => "Window Center",
        0x0028_1051 => "Window Width",
        0x0028_1052 => "Rescale Intercept",
        0x0028_1053 => "Rescale Slope",
        0x0028_1054 => "Rescale Type",
        0x0028_1056 => "VOI LUT Function",
        0x0028_1101 => "Red Palette Color LUT Descriptor",
        0x0028_1102 => "Green Palette Color LUT Descriptor",
        0x0028_1103 => "Blue Palette Color LUT Descriptor",
        0x0028_1201 => "Red Palette Color LUT Data",
        0x0028_1202 => "Green Palette Color LUT Data",
        0x0028_1203 => "Blue Palette Color LUT Data",
        0x0028_1221 => "Segmented Red Palette Color LUT Data",
        0x0028_1222 => "Segmented Green Palette Color LUT Data",
        0x0028_1223 => "Segmented Blue Palette Color LUT Data",
        0x0028_2000 => "ICC Profile",
        0x0028_3000 => "Modality LUT Sequence",
        0x0028_3002 => "LUT Descriptor",
        0x0028_3006 => "LUT Data",
        0x0028_3010 => "VOI LUT Sequence",
        0x0088_0200 => "Icon Image Sequence",
        0x7FE0_0008 => "Float Pixel Data",
        0x7FE0_0009 => "Double Float Pixel Data",
        0x7FE0_0010 => "Pixel Data",
        0xFFFE_E000 => "Item",
        0xFFFE_E00D => "Item Delimitation",
        0xFFFE_E0DD => "Sequence Delimitation",
        _ => return None,
    };
    Some(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vr {
    Ae,
    As,
    At,
    Cs,
    Da,
    Ds,
    Dt,
    Fl,
    Fd,
    Is,
    Lo,
    Lt,
    Ob,
    Od,
    Of,
    Ol,
    Ov,
    Ow,
    Pn,
    Sh,
    Sl,
    Sq,
    Ss,
    St,
    Sv,
    Tm,
    Uc,
    Ui,
    Ul,
    Un,
    Ur,
    Us,
    Ut,
    Uv,
    Implicit,
}

impl Vr {
    pub fn from_bytes(b: [u8; 2]) -> Option<Vr> {
        let vr = match &b {
            b"AE" => Vr::Ae,
            b"AS" => Vr::As,
            b"AT" => Vr::At,
            b"CS" => Vr::Cs,
            b"DA" => Vr::Da,
            b"DS" => Vr::Ds,
            b"DT" => Vr::Dt,
            b"FL" => Vr::Fl,
            b"FD" => Vr::Fd,
            b"IS" => Vr::Is,
            b"LO" => Vr::Lo,
            b"LT" => Vr::Lt,
            b"OB" => Vr::Ob,
            b"OD" => Vr::Od,
            b"OF" => Vr::Of,
            b"OL" => Vr::Ol,
            b"OV" => Vr::Ov,
            b"OW" => Vr::Ow,
            b"PN" => Vr::Pn,
            b"SH" => Vr::Sh,
            b"SL" => Vr::Sl,
            b"SQ" => Vr::Sq,
            b"SS" => Vr::Ss,
            b"ST" => Vr::St,
            b"SV" => Vr::Sv,
            b"TM" => Vr::Tm,
            b"UC" => Vr::Uc,
            b"UI" => Vr::Ui,
            b"UL" => Vr::Ul,
            b"UN" => Vr::Un,
            b"UR" => Vr::Ur,
            b"US" => Vr::Us,
            b"UT" => Vr::Ut,
            b"UV" => Vr::Uv,
            _ => return None,
        };
        Some(vr)
    }

    pub fn has_long_length(&self) -> bool {
        matches!(
            self,
            Vr::Ob
                | Vr::Od
                | Vr::Of
                | Vr::Ol
                | Vr::Ov
                | Vr::Ow
                | Vr::Sq
                | Vr::Sv
                | Vr::Uc
                | Vr::Un
                | Vr::Ur
                | Vr::Ut
                | Vr::Uv
        )
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Vr::Ae
                | Vr::As
                | Vr::Cs
                | Vr::Da
                | Vr::Ds
                | Vr::Dt
                | Vr::Is
                | Vr::Lo
                | Vr::Lt
                | Vr::Pn
                | Vr::Sh
                | Vr::St
                | Vr::Tm
                | Vr::Uc
                | Vr::Ui
                | Vr::Ur
                | Vr::Ut
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Vr::Ae => "AE",
            Vr::As => "AS",
            Vr::At => "AT",
            Vr::Cs => "CS",
            Vr::Da => "DA",
            Vr::Ds => "DS",
            Vr::Dt => "DT",
            Vr::Fl => "FL",
            Vr::Fd => "FD",
            Vr::Is => "IS",
            Vr::Lo => "LO",
            Vr::Lt => "LT",
            Vr::Ob => "OB",
            Vr::Od => "OD",
            Vr::Of => "OF",
            Vr::Ol => "OL",
            Vr::Ov => "OV",
            Vr::Ow => "OW",
            Vr::Pn => "PN",
            Vr::Sh => "SH",
            Vr::Sl => "SL",
            Vr::Sq => "SQ",
            Vr::Ss => "SS",
            Vr::St => "ST",
            Vr::Sv => "SV",
            Vr::Tm => "TM",
            Vr::Uc => "UC",
            Vr::Ui => "UI",
            Vr::Ul => "UL",
            Vr::Un => "UN",
            Vr::Ur => "UR",
            Vr::Us => "US",
            Vr::Ut => "UT",
            Vr::Uv => "UV",
            Vr::Implicit => "??",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferSyntax {
    ImplicitLittleEndian,
    ExplicitLittleEndian,
    ExplicitBigEndian,
    PrivateGeBigEndian,
    DeflatedExplicitLittleEndian,
    EncapsulatedUncompressed,
    DeflatedImageFrameCompression,
    JpegDct,
    JpegLossless,
    JpegLs,
    Rle,
    Jpeg2000,
    JpegXl,
    Video,
    Unknown,
}

impl TransferSyntax {
    pub fn from_uid(uid: &str) -> TransferSyntax {
        match uid {
            "1.2.840.10008.1.2" => TransferSyntax::ImplicitLittleEndian,
            "1.2.840.10008.1.2.1" => TransferSyntax::ExplicitLittleEndian,
            "1.2.840.10008.1.2.2" => TransferSyntax::ExplicitBigEndian,
            "1.2.840.10008.1.2.1.98" => TransferSyntax::EncapsulatedUncompressed,
            "1.2.840.10008.1.2.1.99" => TransferSyntax::DeflatedExplicitLittleEndian,
            "1.2.840.10008.1.2.8.1" => TransferSyntax::DeflatedImageFrameCompression,
            "1.2.840.10008.1.2.4.57"
            | "1.2.840.10008.1.2.4.58"
            | "1.2.840.10008.1.2.4.65"
            | "1.2.840.10008.1.2.4.66"
            | "1.2.840.10008.1.2.4.70" => TransferSyntax::JpegLossless,
            "1.2.840.10008.1.2.4.80" | "1.2.840.10008.1.2.4.81" => TransferSyntax::JpegLs,
            "1.2.840.10008.1.2.4.90"
            | "1.2.840.10008.1.2.4.91"
            | "1.2.840.10008.1.2.4.92"
            | "1.2.840.10008.1.2.4.93"
            | "1.2.840.10008.1.2.4.201"
            | "1.2.840.10008.1.2.4.202"
            | "1.2.840.10008.1.2.4.203" => TransferSyntax::Jpeg2000,
            "1.2.840.10008.1.2.4.110" | "1.2.840.10008.1.2.4.111" | "1.2.840.10008.1.2.4.112" => TransferSyntax::JpegXl,
            "1.2.840.10008.1.2.5" => TransferSyntax::Rle,
            "1.2.840.113619.5.2" => TransferSyntax::PrivateGeBigEndian,
            _ => {
                if let Some(rest) = uid.strip_prefix("1.2.840.10008.1.2.4.") {
                    if let Ok(n) = rest.split('.').next().unwrap_or("").parse::<u32>() {
                        return match n {
                            50..=66 => TransferSyntax::JpegDct,
                            100..=108 => TransferSyntax::Video,
                            _ => TransferSyntax::Unknown,
                        };
                    }
                }
                TransferSyntax::Unknown
            }
        }
    }

    pub fn is_explicit_vr(&self) -> bool {
        !matches!(self, TransferSyntax::ImplicitLittleEndian | TransferSyntax::PrivateGeBigEndian)
    }

    pub fn is_big_endian(&self) -> bool {
        matches!(self, TransferSyntax::ExplicitBigEndian)
    }

    pub fn pixel_data_big_endian(&self) -> bool {
        matches!(self, TransferSyntax::PrivateGeBigEndian)
    }

    pub fn is_encapsulated(&self) -> bool {
        matches!(
            self,
            TransferSyntax::EncapsulatedUncompressed
                | TransferSyntax::DeflatedImageFrameCompression
                | TransferSyntax::JpegDct
                | TransferSyntax::JpegLossless
                | TransferSyntax::JpegLs
                | TransferSyntax::Rle
                | TransferSyntax::Jpeg2000
                | TransferSyntax::JpegXl
                | TransferSyntax::Video
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            TransferSyntax::ImplicitLittleEndian => "Implicit VR Little Endian",
            TransferSyntax::ExplicitLittleEndian => "Explicit VR Little Endian",
            TransferSyntax::ExplicitBigEndian => "Explicit VR Big Endian",
            TransferSyntax::PrivateGeBigEndian => "Private GE Implicit VR Little Endian with Big Endian Pixel Data",
            TransferSyntax::DeflatedExplicitLittleEndian => "Deflated Explicit VR Little Endian",
            TransferSyntax::EncapsulatedUncompressed => "Encapsulated Uncompressed",
            TransferSyntax::DeflatedImageFrameCompression => "Deflated Image Frame Compression",
            TransferSyntax::JpegDct => "JPEG (DCT)",
            TransferSyntax::JpegLossless => "JPEG Lossless",
            TransferSyntax::JpegLs => "JPEG-LS",
            TransferSyntax::Rle => "RLE Lossless",
            TransferSyntax::Jpeg2000 => "JPEG 2000",
            TransferSyntax::JpegXl => "JPEG XL",
            TransferSyntax::Video => "Video (MPEG/HEVC)",
            TransferSyntax::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Photometric {
    Monochrome1,
    Monochrome2,
    PaletteColor,
    Rgb,
    Hsv,
    Argb,
    Cmyk,
    YbrFull,
    YbrFull422,
    YbrPartial422,
    YbrPartial420,
    YbrIct,
    YbrRct,
    Unknown(String),
}

impl Photometric {
    pub fn from_str(s: &str) -> Photometric {
        match s.trim() {
            "MONOCHROME1" => Photometric::Monochrome1,
            "MONOCHROME2" => Photometric::Monochrome2,
            "PALETTE COLOR" => Photometric::PaletteColor,
            "RGB" => Photometric::Rgb,
            "HSV" => Photometric::Hsv,
            "ARGB" => Photometric::Argb,
            "CMYK" => Photometric::Cmyk,
            "YBR_FULL" => Photometric::YbrFull,
            "YBR_FULL_422" => Photometric::YbrFull422,
            "YBR_PARTIAL_422" => Photometric::YbrPartial422,
            "YBR_PARTIAL_420" => Photometric::YbrPartial420,
            "YBR_ICT" => Photometric::YbrIct,
            "YBR_RCT" => Photometric::YbrRct,
            other => Photometric::Unknown(other.to_string()),
        }
    }

    pub fn is_monochrome(&self) -> bool {
        matches!(self, Photometric::Monochrome1 | Photometric::Monochrome2)
    }

    pub fn is_ybr(&self) -> bool {
        matches!(
            self,
            Photometric::YbrFull | Photometric::YbrFull422 | Photometric::YbrPartial422 | Photometric::YbrPartial420
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiFunction {
    Linear,
    LinearExact,
    Sigmoid,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DicomElementInfo {
    pub group: u16,
    pub element: u16,
    pub vr: String,
    pub length: u32,
    pub name: String,
    pub value: String,
    pub depth: u8,
}
