use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::{TgaExtensionAreaData, TgaFooterData, TgaHeader, IMAGE_TYPE_FLAG_RLE};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type TgaInfo = FormatInfo<TgaSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaImageId {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaColorMap {
    pub first_entry_index: u16,
    pub entry_count: u16,
    pub entry_size: u8,
    pub used: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct TgaPixelData {
    pub rle: bool,
    pub expected_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaPostageStamp {
    pub width: u8,
    pub height: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaScanLineTable {
    pub entries: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaDeveloperTag {
    pub tag: u16,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaDeveloperDirectory {
    pub declared_count: u16,
    pub entries: Vec<TgaDeveloperTag>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum TgaSection {
    Header(TgaHeader),
    ImageId(TgaImageId),
    ColorMap(TgaColorMap),
    PixelData(TgaPixelData),
    ExtensionArea(TgaExtensionAreaData),
    PostageStamp(TgaPostageStamp),
    ColorCorrectionTable,
    ScanLineTable(TgaScanLineTable),
    DeveloperDirectory(TgaDeveloperDirectory),
    Footer(TgaFooterData),
}

fn image_type_name(raw: u8) -> String {
    let base = match raw & !IMAGE_TYPE_FLAG_RLE {
        0 => "No image data",
        1 => "Color-mapped",
        2 => "True-color",
        3 => "Grayscale",
        _ => "Unknown",
    };
    let rle = if raw & IMAGE_TYPE_FLAG_RLE != 0 { ", RLE" } else { "" };
    format!("{}{} ({})", base, rle, raw)
}

fn attributes_name(raw: u8) -> String {
    let name = match raw {
        0 => "No alpha",
        1 => "Undefined alpha, ignore",
        2 => "Undefined alpha, retain",
        3 => "Alpha",
        4 => "Premultiplied alpha",
        _ => "Reserved",
    };
    format!("{} ({})", name, raw)
}

fn describe_header(header: &TgaHeader, out: &mut InfoWriter) {
    out.field("Image type", image_type_name(header.image_type_raw));
    out.field("Color map type", header.palette_type);
    if header.palette_length > 0 {
        out.field("Color map first entry", header.palette_first);
        out.field("Color map length", header.palette_length);
        out.field("Color map entry size", format!("{} bits", header.palette_bpp));
    }
    out.field("Origin", format!("({}, {})", header.x_origin, header.y_origin));
    out.field("Width", header.width);
    out.field("Height", header.height);
    out.field("Pixel depth", format!("{} bits", header.bpp));
    out.field("Descriptor", format!("0x{:02X}", header.flags));
    out.field("Alpha bits", header.flags & 0x0F);
    let vertical = if header.flags & 0x20 != 0 { "top" } else { "bottom" };
    let horizontal = if header.flags & 0x10 != 0 { "right" } else { "left" };
    out.field("First pixel", format!("{}-{}", vertical, horizontal));
    let interleave = (header.flags >> 6) & 0x03;
    if interleave != 0 {
        out.field("Interleave (obsolete)", interleave);
    }
}

fn describe_extension(ext: &TgaExtensionAreaData, out: &mut InfoWriter) {
    out.field("Extension size", format!("{} bytes", ext.extension_size));
    if !ext.author_name.trim().is_empty() {
        out.text("Author name", ext.author_name.trim());
    }
    let comments = ext.author_comments.trim();
    if !comments.is_empty() {
        out.text("Author comments", comments);
    }
    if ext.date_year != 0 {
        out.field(
            "Date",
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                ext.date_year, ext.date_month, ext.date_day, ext.time_hour, ext.time_minute, ext.time_second
            ),
        );
    }
    if !ext.job_name.trim().is_empty() {
        out.text("Job name", ext.job_name.trim());
        out.field("Job time", format!("{:02}:{:02}:{:02}", ext.job_hours, ext.job_minutes, ext.job_seconds));
    }
    if !ext.software_id.trim().is_empty() {
        out.text("Software ID", ext.software_id.trim());
    }
    if ext.software_version_number != 0 {
        let letter = match ext.software_version_letter {
            0 | b' ' => String::new(),
            letter => (letter as char).to_string(),
        };
        out.field(
            "Software version",
            format!("{}.{:02}{}", ext.software_version_number / 100, ext.software_version_number % 100, letter),
        );
    }
    out.field(
        "Key color",
        format!("A={} R={} G={} B={}", ext.key_color_a, ext.key_color_r, ext.key_color_g, ext.key_color_b),
    );
    if ext.pixel_aspect_ratio_denominator != 0 {
        out.field(
            "Pixel aspect ratio",
            format!("{}/{}", ext.pixel_aspect_ratio_numerator, ext.pixel_aspect_ratio_denominator),
        );
    }
    if ext.gamma_value_denominator != 0 {
        out.field("Gamma", format!("{}/{}", ext.gamma_value_numerator, ext.gamma_value_denominator));
    }
    let offset = |value: u32| if value == 0 { "none".to_string() } else { format!("0x{:08X}", value) };
    out.field("Color correction offset", offset(ext.color_correction_offset));
    out.field("Postage stamp offset", offset(ext.postage_stamp_offset));
    out.field("Scan line table offset", offset(ext.scan_line_offset));
    out.field("Attributes", attributes_name(ext.attributes_type));
}

impl InfoSection for TgaSection {
    fn title(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            TgaSection::Header(_) => "Header",
            TgaSection::ImageId(_) => "Image ID",
            TgaSection::ColorMap(_) => "Color Map",
            TgaSection::PixelData(_) => "Pixel Data",
            TgaSection::ExtensionArea(_) => "Extension Area",
            TgaSection::PostageStamp(_) => "Postage Stamp",
            TgaSection::ColorCorrectionTable => "Color Correction Table",
            TgaSection::ScanLineTable(_) => "Scan Line Table",
            TgaSection::DeveloperDirectory(_) => "Developer Directory",
            TgaSection::Footer(_) => "Footer",
        })
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            TgaSection::Header(header) => describe_header(header, out),
            TgaSection::ImageId(id) => {
                out.text("Text", &id.text);
            }
            TgaSection::ColorMap(map) => {
                out.field("First entry index", map.first_entry_index);
                out.field("Entries", map.entry_count);
                out.field("Entry size", format!("{} bits", map.entry_size));
                if !map.used {
                    out.field("Used", "no (image is not color-mapped)");
                }
            }
            TgaSection::PixelData(pixel_data) => {
                out.field("Encoding", if pixel_data.rle { "RLE" } else { "Uncompressed" });
                out.opt_field("Expected length", pixel_data.expected_length.map(|n| format!("{} bytes", n)));
            }
            TgaSection::ExtensionArea(ext) => describe_extension(ext, out),
            TgaSection::PostageStamp(stamp) => {
                out.field("Dimensions", format!("{}x{}", stamp.width, stamp.height));
            }
            TgaSection::ColorCorrectionTable => {
                out.field("Entries", 256);
            }
            TgaSection::ScanLineTable(table) => {
                out.field("Entries", table.entries);
            }
            TgaSection::DeveloperDirectory(dir) => {
                out.field("Declared entries", dir.declared_count);
                for entry in &dir.entries {
                    out.field(
                        &format!("Tag {}", entry.tag),
                        format!("offset 0x{:08X}, {} bytes", entry.offset, entry.size),
                    );
                }
            }
            TgaSection::Footer(footer) => {
                let offset = |value: u32| if value == 0 { "none".to_string() } else { format!("0x{:08X}", value) };
                out.field("Extension area offset", offset(footer.extension_area_offset));
                out.field("Developer directory offset", offset(footer.developer_dir_offset));
            }
        }
    }
}
