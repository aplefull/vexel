use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type GifInfo = FormatInfo<GifSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifHeader {
    pub signature_offset: u32,
    pub signature: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub global_color_table_flag: bool,
    pub color_resolution: u8,
    pub sort_flag: bool,
    pub global_color_table_size: u8,
    pub background_color_index: u8,
    pub pixel_aspect_ratio: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifColorTable {
    pub frame: Option<u32>,
    pub declared_entries: u32,
    pub entries: Vec<[u8; 3]>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct GifGraphicsControl {
    pub block_size: u8,
    pub disposal_method: u8,
    pub user_input: bool,
    pub transparent_color_index: Option<u8>,
    pub delay_cs: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifImageDescriptor {
    pub frame: u32,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub local_color_table_flag: bool,
    pub interlace_flag: bool,
    pub sort_flag: bool,
    pub local_color_table_size: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifImageData {
    pub frame: u32,
    pub lzw_minimum_code_size: u8,
    pub sub_blocks: u32,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifComment {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum GifApplicationKind {
    Netscape,
    AnimExts,
    Xmp,
    Icc,
    Other,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct GifApplicationExtension {
    pub identifier: String,
    pub auth_code: String,
    pub kind: GifApplicationKind,
    pub loop_count: Option<u16>,
    pub buffer_size: Option<u32>,
    pub sub_blocks: u32,
    pub data_length: u64,
    pub icc_parsed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifPlainText {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cell_width: u8,
    pub cell_height: u8,
    pub foreground_color: u8,
    pub background_color: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifUnknownExtension {
    pub label: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum GifSection {
    Header(GifHeader),
    GlobalColorTable(GifColorTable),
    GraphicsControlExtension(GifGraphicsControl),
    ImageDescriptor(GifImageDescriptor),
    LocalColorTable(GifColorTable),
    ImageData(GifImageData),
    CommentExtension(GifComment),
    ApplicationExtension(GifApplicationExtension),
    PlainTextExtension(GifPlainText),
    UnknownExtension(GifUnknownExtension),
    Trailer,
    TrailingData,
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn describe_color_table(table: &GifColorTable, out: &mut InfoWriter) {
    if table.entries.len() as u32 == table.declared_entries {
        out.field("Entries", table.declared_entries);
    } else {
        out.field("Entries", format!("{} of {} declared (truncated)", table.entries.len(), table.declared_entries));
    }
    let colors =
        table.entries.iter().map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b)).collect::<Vec<_>>().join(", ");
    out.text("Colors", &colors);
}

fn disposal_name(raw: u8) -> String {
    match raw {
        0 => "Unspecified (0)".to_string(),
        1 => "Do not dispose (1)".to_string(),
        2 => "Restore to background (2)".to_string(),
        3 => "Restore to previous (3)".to_string(),
        other => format!("Reserved ({})", other),
    }
}

impl InfoSection for GifSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            GifSection::Header(_) => Cow::Borrowed("Header"),
            GifSection::GlobalColorTable(_) => Cow::Borrowed("Global Color Table"),
            GifSection::GraphicsControlExtension(_) => Cow::Borrowed("Graphics Control Extension"),
            GifSection::ImageDescriptor(d) => Cow::Owned(format!("Image Descriptor (frame {})", d.frame + 1)),
            GifSection::LocalColorTable(t) => {
                Cow::Owned(format!("Local Color Table (frame {})", t.frame.unwrap_or(0) + 1))
            }
            GifSection::ImageData(d) => Cow::Owned(format!("Image Data (frame {})", d.frame + 1)),
            GifSection::CommentExtension(_) => Cow::Borrowed("Comment Extension"),
            GifSection::ApplicationExtension(_) => Cow::Borrowed("Application Extension"),
            GifSection::PlainTextExtension(_) => Cow::Borrowed("Plain Text Extension"),
            GifSection::UnknownExtension(_) => Cow::Borrowed("Unknown Extension"),
            GifSection::Trailer => Cow::Borrowed("Trailer"),
            GifSection::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            GifSection::Header(h) => {
                if h.signature_offset != 0 {
                    out.field(
                        "Signature",
                        format!("{} (at offset {}, corrupted prefix)", h.signature, h.signature_offset),
                    );
                } else {
                    out.field("Signature", &h.signature);
                }
                out.field("Canvas", format!("{}x{}", h.canvas_width, h.canvas_height));
                out.field("Global color table", yes_no(h.global_color_table_flag));
                if h.global_color_table_flag {
                    out.field(
                        "Global color table size",
                        format!(
                            "{} entries (raw {})",
                            1u32 << (h.global_color_table_size + 1),
                            h.global_color_table_size
                        ),
                    );
                    out.field("Sorted", yes_no(h.sort_flag));
                    out.field("Background color index", h.background_color_index);
                }
                out.field(
                    "Color resolution",
                    format!("{} bits per channel (raw {})", h.color_resolution + 1, h.color_resolution),
                );
                if h.pixel_aspect_ratio == 0 {
                    out.field("Pixel aspect ratio", "not specified (0)");
                } else {
                    out.field(
                        "Pixel aspect ratio",
                        format!(
                            "{:.4} (raw {})",
                            (f64::from(h.pixel_aspect_ratio) + 15.0) / 64.0,
                            h.pixel_aspect_ratio
                        ),
                    );
                }
            }
            GifSection::GlobalColorTable(table) | GifSection::LocalColorTable(table) => {
                describe_color_table(table, out)
            }
            GifSection::GraphicsControlExtension(gce) => {
                if gce.block_size != 4 {
                    out.field("Block size", format!("{} (expected 4)", gce.block_size));
                }
                out.field("Disposal method", disposal_name(gce.disposal_method));
                out.field("User input", yes_no(gce.user_input));
                match gce.transparent_color_index {
                    Some(index) => out.field("Transparent color index", index),
                    None => out.field("Transparent color index", "none"),
                };
                out.field("Delay", format!("{} cs ({} ms)", gce.delay_cs, u32::from(gce.delay_cs) * 10));
            }
            GifSection::ImageDescriptor(d) => {
                out.field("Position", format!("({}, {})", d.left, d.top));
                out.field("Dimensions", format!("{}x{}", d.width, d.height));
                out.field("Interlaced", yes_no(d.interlace_flag));
                out.field("Local color table", yes_no(d.local_color_table_flag));
                if d.local_color_table_flag {
                    out.field(
                        "Local color table size",
                        format!(
                            "{} entries (raw {})",
                            1u32 << (d.local_color_table_size + 1),
                            d.local_color_table_size
                        ),
                    );
                    out.field("Sorted", yes_no(d.sort_flag));
                }
            }
            GifSection::ImageData(d) => {
                out.field("LZW minimum code size", d.lzw_minimum_code_size);
                out.field("Sub-blocks", d.sub_blocks);
                out.field("Compressed data", format!("{} bytes", d.data_length));
            }
            GifSection::CommentExtension(c) => {
                out.text("Text", &c.text);
            }
            GifSection::ApplicationExtension(ext) => {
                out.text("Identifier", &format!("{}{}", ext.identifier, ext.auth_code));
                let kind = match ext.kind {
                    GifApplicationKind::Netscape => "Netscape looping",
                    GifApplicationKind::AnimExts => "AnimExts looping",
                    GifApplicationKind::Xmp => "XMP metadata",
                    GifApplicationKind::Icc => "ICC profile",
                    GifApplicationKind::Other => "Unknown",
                };
                out.field("Kind", kind);
                match ext.loop_count {
                    Some(0) => {
                        out.field("Loop count", "infinite (0)");
                    }
                    Some(count) => {
                        out.field("Loop count", count);
                    }
                    None => {}
                }
                out.opt_field("Buffer size", ext.buffer_size.map(|n| format!("{} bytes", n)));
                out.field("Sub-blocks", ext.sub_blocks);
                out.field("Data", format!("{} bytes", ext.data_length));
                if let Some(parsed) = ext.icc_parsed {
                    out.field("Profile parsed", yes_no(parsed));
                }
            }
            GifSection::PlainTextExtension(pt) => {
                out.field("Position", format!("({}, {})", pt.left, pt.top));
                out.field("Dimensions", format!("{}x{}", pt.width, pt.height));
                out.field("Cell", format!("{}x{}", pt.cell_width, pt.cell_height));
                out.field("Foreground color index", pt.foreground_color);
                out.field("Background color index", pt.background_color);
                out.text("Text", &pt.text);
            }
            GifSection::UnknownExtension(ext) => {
                out.field("Label", format!("0x{:02X}", ext.label));
            }
            GifSection::Trailer | GifSection::TrailingData => {}
        }
    }
}
