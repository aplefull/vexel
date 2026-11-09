use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::{
    BitmapArrayHeader, BitmapCompression, BitmapFileHeader, ColorEntry, ColorMasks, DibHeader, DibKind,
};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type BmpInfo = FormatInfo<BmpSection>;

pub const CS_TYPE_CALIBRATED_RGB: u32 = 0;
pub const CS_TYPE_SRGB: u32 = 0x7352_4742;
pub const CS_TYPE_WINDOWS: u32 = 0x5769_6E20;
pub const CS_TYPE_LINKED: u32 = 0x4C49_4E4B;
pub const CS_TYPE_EMBEDDED: u32 = 0x4D42_4544;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BmpIccProfile {
    pub declared_size: u32,
    pub parsed: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct BmpLinkedProfile {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct BmpPixelData {
    pub compression: BitmapCompression,
    pub expected_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum BmpSection {
    BitmapArrayHeader(BitmapArrayHeader),
    FileHeader(BitmapFileHeader),
    DibHeader(DibHeader),
    ExtraMasks(ColorMasks),
    ColorTable(Vec<ColorEntry>),
    IccProfile(BmpIccProfile),
    LinkedProfile(BmpLinkedProfile),
    PixelData(BmpPixelData),
}

pub fn compression_name(compression: BitmapCompression) -> &'static str {
    match compression {
        BitmapCompression::BiRgb => "BI_RGB (none)",
        BitmapCompression::BiRle8 => "BI_RLE8",
        BitmapCompression::BiRle4 => "BI_RLE4",
        BitmapCompression::BiBitfields => "BI_BITFIELDS",
        BitmapCompression::BiJpeg => "BI_JPEG",
        BitmapCompression::BiPng => "BI_PNG",
        BitmapCompression::BiAlphaBitfields => "BI_ALPHABITFIELDS",
        BitmapCompression::BiCmyk => "BI_CMYK",
        BitmapCompression::BiCMYKRle8 => "BI_CMYKRLE8",
        BitmapCompression::BiCMYKRle4 => "BI_CMYKRLE4",
        BitmapCompression::Os2Huffman1D => "Huffman 1D (OS/2)",
        BitmapCompression::Os2Rle24 => "RLE24 (OS/2)",
    }
}

fn kind_name(header: &DibHeader) -> String {
    let name = match header.kind {
        DibKind::Core => "BITMAPCOREHEADER",
        DibKind::Os2V2Short => "OS22XBITMAPHEADER (short)",
        DibKind::Os2V2 => "OS22XBITMAPHEADER",
        DibKind::Info => "BITMAPINFOHEADER",
        DibKind::V2 => "BITMAPV2INFOHEADER",
        DibKind::V3 => "BITMAPV3INFOHEADER",
        DibKind::V4 => "BITMAPV4HEADER",
        DibKind::V5 => "BITMAPV5HEADER",
    };
    let expected = match header.kind {
        DibKind::Core => 12,
        DibKind::Os2V2Short => 16,
        DibKind::Os2V2 => 64,
        DibKind::Info => 40,
        DibKind::V2 => 52,
        DibKind::V3 => 56,
        DibKind::V4 => 108,
        DibKind::V5 => 124,
    };
    if header.header_size == expected {
        format!("{} ({} bytes)", name, header.header_size)
    } else {
        format!("Unknown size {}, parsed as {}", header.header_size, name)
    }
}

fn signature_name(signature: u16) -> String {
    let bytes = signature.to_le_bytes();
    let text: String = bytes.iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '.' }).collect();
    let kind = match &bytes {
        b"BM" => "Windows bitmap",
        b"BA" => "OS/2 bitmap array",
        b"CI" => "OS/2 color icon",
        b"CP" => "OS/2 color pointer",
        b"IC" => "OS/2 icon",
        b"PT" => "OS/2 pointer",
        _ => "invalid",
    };
    format!("{} ({})", text, kind)
}

pub fn cs_type_name(cs_type: u32) -> String {
    match cs_type {
        CS_TYPE_CALIBRATED_RGB => "LCS_CALIBRATED_RGB (0)".to_string(),
        CS_TYPE_SRGB => "LCS_sRGB ('sRGB')".to_string(),
        CS_TYPE_WINDOWS => "LCS_WINDOWS_COLOR_SPACE ('Win ')".to_string(),
        CS_TYPE_LINKED => "PROFILE_LINKED ('LINK')".to_string(),
        CS_TYPE_EMBEDDED => "PROFILE_EMBEDDED ('MBED')".to_string(),
        other => format!("Unknown (0x{:08X})", other),
    }
}

fn intent_name(intent: u32) -> String {
    match intent {
        1 => "LCS_GM_BUSINESS (saturation)".to_string(),
        2 => "LCS_GM_GRAPHICS (relative colorimetric)".to_string(),
        4 => "LCS_GM_IMAGES (perceptual)".to_string(),
        8 => "LCS_GM_ABS_COLORIMETRIC (absolute colorimetric)".to_string(),
        other => format!("Unknown ({})", other),
    }
}

fn fixed_16_16(value: u32) -> f64 {
    f64::from(value) / 65536.0
}

fn fixed_2_30(value: i32) -> f64 {
    f64::from(value) / f64::from(1u32 << 30)
}

fn describe_masks(masks: &ColorMasks, out: &mut InfoWriter) {
    out.field("Red mask", format!("0x{:08X}", masks.red));
    out.field("Green mask", format!("0x{:08X}", masks.green));
    out.field("Blue mask", format!("0x{:08X}", masks.blue));
    if let Some(alpha) = masks.alpha {
        out.field("Alpha mask", format!("0x{:08X}", alpha));
    }
}

fn describe_dib(header: &DibHeader, out: &mut InfoWriter) {
    out.field("Type", kind_name(header));
    out.field("Width", header.width);
    let orientation = if header.height < 0 { "top-down" } else { "bottom-up" };
    out.field("Height", format!("{} ({})", header.height, orientation));
    out.field("Planes", header.planes);
    out.field("Bits per pixel", header.bits_per_pixel);

    if !matches!(header.kind, DibKind::Core | DibKind::Os2V2Short) {
        let name = compression_name(header.compression);
        if u32::from(header.compression as u16) == header.compression_raw || matches!(header.kind, DibKind::Os2V2) {
            out.field("Compression", format!("{} ({})", name, header.compression_raw));
        } else {
            out.field("Compression", format!("Invalid ({}), treated as {}", header.compression_raw, name));
        }
        out.field("Image size", format!("{} bytes", header.image_size));
        out.field("X pixels per meter", header.x_pixels_per_meter);
        out.field("Y pixels per meter", header.y_pixels_per_meter);
        out.field("Colors used", header.colors_used);
        out.field("Important colors", header.important_colors);
    }

    if let Some(masks) = &header.masks {
        describe_masks(masks, out);
    }

    if let Some(color_space) = &header.color_space {
        out.field("Color space type", cs_type_name(color_space.cs_type));
        if color_space.cs_type == CS_TYPE_CALIBRATED_RGB {
            let endpoints = &color_space.endpoints;
            for (label, xyz) in [
                ("Red endpoint", &endpoints.ciexyz_red),
                ("Green endpoint", &endpoints.ciexyz_green),
                ("Blue endpoint", &endpoints.ciexyz_blue),
            ] {
                out.field(
                    label,
                    format!("X={:.4} Y={:.4} Z={:.4}", fixed_2_30(xyz.x), fixed_2_30(xyz.y), fixed_2_30(xyz.z)),
                );
            }
            out.field(
                "Gamma",
                format!(
                    "R={:.4} G={:.4} B={:.4}",
                    fixed_16_16(color_space.gamma_red),
                    fixed_16_16(color_space.gamma_green),
                    fixed_16_16(color_space.gamma_blue)
                ),
            );
        }
    }

    if let Some(v5) = &header.v5 {
        out.field("Intent", intent_name(v5.intent));
        if v5.profile_size > 0 {
            out.field("Profile offset", format!("{} (from header start)", v5.profile_data));
            out.field("Profile size", format!("{} bytes", v5.profile_size));
        }
    }

    if let Some(os2) = &header.os2 {
        out.field("Units", os2.units);
        out.field("Recording", os2.recording);
        let rendering = match os2.rendering {
            0 => "None".to_string(),
            1 => "Error diffusion".to_string(),
            2 => "PANDA".to_string(),
            3 => "Super-circle".to_string(),
            other => format!("Unknown ({})", other),
        };
        out.field("Halftoning", rendering);
        if os2.rendering != 0 {
            out.field("Halftoning parameters", format!("{}, {}", os2.size1, os2.size2));
        }
        out.field("Color encoding", os2.color_encoding);
        out.field("Identifier", os2.identifier);
    }
}

impl InfoSection for BmpSection {
    fn title(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            BmpSection::BitmapArrayHeader(_) => "Bitmap Array Header",
            BmpSection::FileHeader(_) => "File Header",
            BmpSection::DibHeader(_) => "DIB Header",
            BmpSection::ExtraMasks(_) => "Color Masks",
            BmpSection::ColorTable(_) => "Color Table",
            BmpSection::IccProfile(_) => "ICC Profile",
            BmpSection::LinkedProfile(_) => "Linked Profile",
            BmpSection::PixelData(_) => "Pixel Data",
        })
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            BmpSection::BitmapArrayHeader(ba) => {
                out.field("File size", format!("{} bytes", ba.file_size));
                out.field("Next offset", ba.next_offset);
                out.field("Screen size", format!("{}x{}", ba.screen_width, ba.screen_height));
            }
            BmpSection::FileHeader(header) => {
                out.field("Signature", signature_name(header.signature));
                out.field("File size", format!("{} bytes", header.file_size));
                if header.reserved1 != 0 || header.reserved2 != 0 {
                    out.field("Reserved", format!("{}, {}", header.reserved1, header.reserved2));
                }
                out.field("Pixel data offset", format!("0x{:08X}", header.pixel_offset));
            }
            BmpSection::DibHeader(header) => describe_dib(header, out),
            BmpSection::ExtraMasks(masks) => describe_masks(masks, out),
            BmpSection::ColorTable(entries) => {
                out.field("Entries", entries.len());
                let colors = entries
                    .iter()
                    .map(|e| format!("#{:02X}{:02X}{:02X}", e.red, e.green, e.blue))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.text("Colors", &colors);
            }
            BmpSection::IccProfile(profile) => {
                out.field("Declared size", format!("{} bytes", profile.declared_size));
                out.field("Parsed", if profile.parsed { "yes" } else { "no" });
            }
            BmpSection::LinkedProfile(linked) => {
                out.text("Path", &linked.path);
            }
            BmpSection::PixelData(pixel_data) => {
                out.field("Compression", compression_name(pixel_data.compression));
                out.opt_field("Expected length", pixel_data.expected_length.map(|n| format!("{} bytes", n)));
            }
        }
    }
}
