use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::decoders::jpeg::app::{AppSection, CommentSegment};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type JpegLsInfo = FormatInfo<JpegLsSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsFrameComponent {
    pub id: u8,
    pub horizontal_sampling: u8,
    pub vertical_sampling: u8,
    pub reserved: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsFrame {
    pub precision: u8,
    pub height: u16,
    pub width: u16,
    pub component_count: u8,
    pub components: Vec<JpegLsFrameComponent>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsScanComponent {
    pub id: u8,
    pub mapping_table: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsScan {
    pub header_length: u16,
    pub components: Vec<JpegLsScanComponent>,
    pub near: u8,
    pub interleave_mode: u8,
    pub point_transform: u8,
    pub maxval: i32,
    pub thresholds: [i32; 3],
    pub reset: i32,
    pub restart_interval: u16,
    pub entropy_coded_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsPresetParameters {
    pub maxval: u16,
    pub t1: u16,
    pub t2: u16,
    pub t3: u16,
    pub reset: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsMappingTable {
    pub table_id: u8,
    pub entry_count: u16,
    pub entries: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegLsExtension {
    pub id: u8,
    pub data_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JpegLsSection {
    StartOfImage,
    Frame(JpegLsFrame),
    Scan(JpegLsScan),
    PresetParameters(JpegLsPresetParameters),
    MappingTable(JpegLsMappingTable),
    Extension(JpegLsExtension),
    RestartInterval(u16),
    NumberOfLines(u16),
    App(AppSection),
    Comment(CommentSegment),
    EndOfImage,
    TrailingData,
}

pub fn interleave_name(mode: u8) -> String {
    match mode {
        0 => "None (0)".to_string(),
        1 => "Line (1)".to_string(),
        2 => "Sample (2)".to_string(),
        other => format!("Invalid ({}), treated as line", other),
    }
}

fn extension_name(id: u8) -> &'static str {
    match id {
        3 => "Mapping table continuation",
        4 => "Oversize image dimensions",
        _ => "Unknown",
    }
}

impl InfoSection for JpegLsSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            JpegLsSection::StartOfImage => Cow::Borrowed("SOI (Start of Image)"),
            JpegLsSection::Frame(_) => Cow::Borrowed("SOF55 (JPEG-LS Frame)"),
            JpegLsSection::Scan(_) => Cow::Borrowed("SOS (Start of Scan)"),
            JpegLsSection::PresetParameters(_) => Cow::Borrowed("LSE (Preset Coding Parameters)"),
            JpegLsSection::MappingTable(_) => Cow::Borrowed("LSE (Mapping Table)"),
            JpegLsSection::Extension(ext) => Cow::Owned(format!("LSE ({})", extension_name(ext.id))),
            JpegLsSection::RestartInterval(_) => Cow::Borrowed("DRI (Restart Interval)"),
            JpegLsSection::NumberOfLines(_) => Cow::Borrowed("DNL (Number of Lines)"),
            JpegLsSection::App(app) => Cow::Owned(app.title()),
            JpegLsSection::Comment(_) => Cow::Borrowed("COM (Comment)"),
            JpegLsSection::EndOfImage => Cow::Borrowed("EOI (End of Image)"),
            JpegLsSection::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            JpegLsSection::Frame(frame) => {
                out.field("Precision", format!("{} bits", frame.precision));
                out.field("Dimensions", format!("{}x{}", frame.width, frame.height));
                out.field("Components", frame.component_count);
                for component in &frame.components {
                    let mut value =
                        format!("sampling {}x{}", component.horizontal_sampling, component.vertical_sampling);
                    if component.reserved != 0 {
                        value = format!("{}, reserved byte {}", value, component.reserved);
                    }
                    out.field(&format!("Component {}", component.id), value);
                }
            }
            JpegLsSection::Scan(scan) => {
                out.field("Components", scan.components.len());
                for component in &scan.components {
                    let value = if component.mapping_table == 0 {
                        "no mapping table".to_string()
                    } else {
                        format!("mapping table {}", component.mapping_table)
                    };
                    out.field(&format!("Component {}", component.id), value);
                }
                if scan.near == 0 {
                    out.field("NEAR", "0 (lossless)");
                } else {
                    out.field("NEAR", format!("{} (near-lossless)", scan.near));
                }
                out.field("Interleave mode", interleave_name(scan.interleave_mode));
                if scan.point_transform & 0xF0 != 0 {
                    out.field(
                        "Point transform",
                        format!("{} (raw byte 0x{:02X})", scan.point_transform & 0x0F, scan.point_transform),
                    );
                } else {
                    out.field("Point transform", scan.point_transform);
                }
                out.field("MAXVAL (effective)", scan.maxval);
                out.field(
                    "Thresholds (effective)",
                    format!("T1={}, T2={}, T3={}", scan.thresholds[0], scan.thresholds[1], scan.thresholds[2]),
                );
                out.field("RESET (effective)", scan.reset);
                if scan.restart_interval > 0 {
                    out.field("Restart interval", scan.restart_interval);
                }
                out.field("Entropy-coded data", format!("{} bytes", scan.entropy_coded_length));
            }
            JpegLsSection::PresetParameters(params) => {
                out.field("MAXVAL", params.maxval);
                out.field("T1", params.t1);
                out.field("T2", params.t2);
                out.field("T3", params.t3);
                out.field("RESET", params.reset);
            }
            JpegLsSection::MappingTable(table) => {
                out.field("Table ID", table.table_id);
                out.field("Entries", table.entries.len());
                let values = table.entries.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                out.text("Values", &values);
            }
            JpegLsSection::Extension(ext) => {
                out.field("ID", ext.id);
                out.field("Data", format!("{} bytes", ext.data_length));
            }
            JpegLsSection::RestartInterval(interval) => {
                out.field("Restart interval", interval);
            }
            JpegLsSection::NumberOfLines(lines) => {
                out.field("Number of lines", lines);
            }
            JpegLsSection::App(app) => app.segment.describe(out),
            JpegLsSection::Comment(comment) => comment.describe(out),
            JpegLsSection::StartOfImage | JpegLsSection::EndOfImage | JpegLsSection::TrailingData => {}
        }
    }
}
