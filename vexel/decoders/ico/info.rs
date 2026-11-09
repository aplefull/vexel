use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::{IcoImageFormat, IcoType};
use crate::decoders::bmp::types::DibHeader;
use crate::decoders::png::PngInfo;
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type IcoInfo = FormatInfo<IcoSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct IcoDirectory {
    pub reserved: u16,
    pub type_raw: u16,
    pub ico_type: Option<IcoType>,
    pub image_count: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct IcoDirectoryEntry {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub color_count: u8,
    pub reserved: u8,
    pub planes: Option<u16>,
    pub bit_count: Option<u16>,
    pub hotspot: Option<[u16; 2]>,
    pub bytes_in_res: u32,
    pub image_offset: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct EmbeddedBmp {
    pub dib: Option<DibHeader>,
    pub color_table_entries: u32,
    pub xor_length: u64,
    pub and_mask_length: u64,
    pub and_mask_present: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum EmbeddedImage {
    Bmp(EmbeddedBmp),
    Png(PngInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct IcoImageData {
    pub index: u32,
    pub image_format: IcoImageFormat,
    pub declared_length: u32,
    pub directory_bit_count: Option<u16>,
    pub embedded: Option<EmbeddedImage>,
    pub same_data_as: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum IcoSection {
    Directory(IcoDirectory),
    DirectoryEntry(IcoDirectoryEntry),
    ImageData(IcoImageData),
}

fn describe_embedded_bmp(bmp: &EmbeddedBmp, directory_bit_count: Option<u16>, out: &mut InfoWriter) {
    let Some(dib) = &bmp.dib else {
        out.field("DIB header", "unreadable");
        return;
    };

    out.field("DIB header size", format!("{} bytes", dib.header_size));
    out.field("Width", dib.width);
    out.field("Height", format!("{} (XOR + AND masks)", dib.height));
    match directory_bit_count {
        Some(bits) if bits != 0 && bits != dib.bits_per_pixel => {
            out.field("Bits per pixel", format!("{} (directory says {})", dib.bits_per_pixel, bits));
        }
        _ => {
            out.field("Bits per pixel", dib.bits_per_pixel);
        }
    }
    out.field(
        "Compression",
        format!("{} ({})", crate::decoders::bmp::compression_name(dib.compression), dib.compression_raw),
    );
    if bmp.color_table_entries > 0 {
        out.field("Color table entries", bmp.color_table_entries);
    }
    out.field("XOR bitmap", format!("{} bytes", bmp.xor_length));
    if bmp.and_mask_present {
        out.field("AND mask", format!("{} bytes", bmp.and_mask_length));
    } else {
        out.field("AND mask", "missing");
    }
}

impl InfoSection for IcoSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            IcoSection::Directory(_) => Cow::Borrowed("Icon Directory"),
            IcoSection::DirectoryEntry(entry) => Cow::Owned(format!("Directory Entry {}", entry.index + 1)),
            IcoSection::ImageData(data) => Cow::Owned(format!("Image {}", data.index + 1)),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            IcoSection::Directory(dir) => {
                let kind = match dir.ico_type {
                    Some(IcoType::Ico) => "Icon (1)".to_string(),
                    Some(IcoType::Cur) => "Cursor (2)".to_string(),
                    None => format!("Unknown ({}), treated as icon", dir.type_raw),
                };
                out.field("Type", kind);
                out.field("Images", dir.image_count);
                if dir.reserved != 0 {
                    out.field("Reserved", dir.reserved);
                }
            }
            IcoSection::DirectoryEntry(entry) => {
                out.field("Dimensions", format!("{}x{}", entry.width, entry.height));
                out.field("Color count", entry.color_count);
                if entry.reserved != 0 {
                    out.field("Reserved", entry.reserved);
                }
                out.opt_field("Planes", entry.planes);
                out.opt_field("Bit count", entry.bit_count);
                if let Some([x, y]) = entry.hotspot {
                    out.field("Hotspot", format!("({}, {})", x, y));
                }
                out.field("Bytes in resource", entry.bytes_in_res);
                out.field("Image offset", format!("0x{:08X}", entry.image_offset));
            }
            IcoSection::ImageData(data) => {
                let format = match data.image_format {
                    IcoImageFormat::Bmp => "BMP (DIB)",
                    IcoImageFormat::Png => "PNG",
                };
                out.field("Format", format);
                out.field("Declared length", format!("{} bytes", data.declared_length));
                match &data.embedded {
                    Some(EmbeddedImage::Bmp(bmp)) => describe_embedded_bmp(bmp, data.directory_bit_count, out),
                    Some(EmbeddedImage::Png(png)) => {
                        if let Some(summary) = &png.summary {
                            out.group("PNG summary", |out| summary.describe(out));
                        }
                        png.describe_sections(out);
                    }
                    None => {}
                }
                out.opt_field("Same data as image", data.same_data_as.map(|index| index + 1));
            }
        }
    }
}
