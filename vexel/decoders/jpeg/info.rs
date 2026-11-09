use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::app::{AppSection, CommentSegment};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type JpegInfo = FormatInfo<JpegSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegFrameComponent {
    pub id: u8,
    pub horizontal_sampling: u8,
    pub vertical_sampling: u8,
    pub quantization_table: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegFrame {
    pub marker: u8,
    pub precision: u8,
    pub height: u16,
    pub width: u16,
    pub component_count: u8,
    pub components: Vec<JpegFrameComponent>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegHuffmanTable {
    pub class: u8,
    pub id: u8,
    pub counts: Vec<u8>,
    pub symbols: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegHuffmanTables {
    pub tables: Vec<JpegHuffmanTable>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegQuantizationTable {
    pub id: u8,
    pub precision: u8,
    pub values: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegQuantizationTables {
    pub tables: Vec<JpegQuantizationTable>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegArithmeticConditioning {
    pub class: u8,
    pub id: u8,
    pub value: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegExpand {
    pub horizontal: bool,
    pub vertical: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegScanComponent {
    pub id: u8,
    pub dc_table: u8,
    pub ac_table: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegScan {
    pub header_length: u16,
    pub components: Vec<JpegScanComponent>,
    pub spectral_start: u8,
    pub spectral_end: u8,
    pub approximation_high: u8,
    pub approximation_low: u8,
    pub lossless: bool,
    pub restart_interval: u16,
    pub entropy_coded_length: u64,
    pub restart_markers: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JpegSection {
    StartOfImage,
    App(AppSection),
    Comment(CommentSegment),
    Frame(JpegFrame),
    HierarchicalProgression(JpegFrame),
    HuffmanTables(JpegHuffmanTables),
    ArithmeticConditioning(Vec<JpegArithmeticConditioning>),
    QuantizationTables(JpegQuantizationTables),
    RestartInterval(u16),
    NumberOfLines(u16),
    Expand(JpegExpand),
    Scan(JpegScan),
    Unknown(u16),
    EndOfImage,
    TrailingData,
}

pub fn process_name(marker: u8) -> &'static str {
    match marker & 0x0F {
        0x0 => "Baseline DCT",
        0x1 | 0x9 => "Extended sequential DCT",
        0x2 | 0xA => "Progressive DCT",
        0x3 | 0xB => "Lossless",
        0x5 | 0xD => "Differential sequential DCT",
        0x6 | 0xE => "Differential progressive DCT",
        0x7 | 0xF => "Differential lossless",
        _ => "Unknown",
    }
}

pub fn coding_name(marker: u8) -> &'static str {
    if marker & 0x08 != 0 {
        "arithmetic"
    } else {
        "Huffman"
    }
}

fn table_class_name(class: u8) -> String {
    match class {
        0 => "DC".to_string(),
        1 => "AC".to_string(),
        other => format!("Invalid class {}", other),
    }
}

pub fn subsampling_name(factors: &[(u8, u8)]) -> Option<String> {
    let (&first, rest) = factors.split_first()?;
    let &second = rest.first()?;
    let listing = factors.iter().map(|(h, v)| format!("{}x{}", h, v)).collect::<Vec<_>>().join(", ");

    let uniform_chroma = rest.iter().all(|&f| f == second);
    let ratio = (second.0 > 0 && second.1 > 0 && first.0 % second.0 == 0 && first.1 % second.1 == 0)
        .then(|| (first.0 / second.0, first.1 / second.1));
    let name = match (uniform_chroma, ratio) {
        (true, Some((1, 1))) => "4:4:4",
        (true, Some((2, 1))) => "4:2:2",
        (true, Some((2, 2))) => "4:2:0",
        (true, Some((1, 2))) => "4:4:0",
        (true, Some((4, 1))) => "4:1:1",
        (true, Some((4, 2))) => "4:1:0",
        _ => return Some(listing),
    };
    Some(format!("{} ({})", name, listing))
}

fn describe_frame(frame: &JpegFrame, out: &mut InfoWriter) {
    out.field("Precision", format!("{} bits", frame.precision));
    if frame.height == 0 {
        out.field("Dimensions", format!("{}x0 (height defined by DNL)", frame.width));
    } else {
        out.field("Dimensions", format!("{}x{}", frame.width, frame.height));
    }
    out.field("Components", frame.component_count);
    for component in &frame.components {
        out.field(
            &format!("Component {}", component.id),
            format!(
                "sampling {}x{}, quantization table {}",
                component.horizontal_sampling, component.vertical_sampling, component.quantization_table
            ),
        );
    }
}

impl InfoSection for JpegSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            JpegSection::StartOfImage => Cow::Borrowed("SOI (Start of Image)"),
            JpegSection::App(app) => Cow::Owned(app.title()),
            JpegSection::Comment(_) => Cow::Borrowed("COM (Comment)"),
            JpegSection::Frame(frame) => Cow::Owned(format!(
                "SOF{} ({}, {})",
                frame.marker & 0x0F,
                process_name(frame.marker),
                coding_name(frame.marker)
            )),
            JpegSection::HierarchicalProgression(_) => Cow::Borrowed("DHP (Hierarchical Progression)"),
            JpegSection::HuffmanTables(_) => Cow::Borrowed("DHT (Huffman Tables)"),
            JpegSection::ArithmeticConditioning(_) => Cow::Borrowed("DAC (Arithmetic Conditioning)"),
            JpegSection::QuantizationTables(_) => Cow::Borrowed("DQT (Quantization Tables)"),
            JpegSection::RestartInterval(_) => Cow::Borrowed("DRI (Restart Interval)"),
            JpegSection::NumberOfLines(_) => Cow::Borrowed("DNL (Number of Lines)"),
            JpegSection::Expand(_) => Cow::Borrowed("EXP (Expand Reference Components)"),
            JpegSection::Scan(_) => Cow::Borrowed("SOS (Start of Scan)"),
            JpegSection::Unknown(marker) => Cow::Owned(format!("Unknown Marker 0x{:04X}", marker)),
            JpegSection::EndOfImage => Cow::Borrowed("EOI (End of Image)"),
            JpegSection::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            JpegSection::App(app) => app.segment.describe(out),
            JpegSection::Comment(comment) => comment.describe(out),
            JpegSection::Frame(frame) | JpegSection::HierarchicalProgression(frame) => describe_frame(frame, out),
            JpegSection::HuffmanTables(dht) => {
                if dht.truncated {
                    out.field("Truncated", "yes");
                }
                for table in &dht.tables {
                    out.group(format!("{} table {}", table_class_name(table.class), table.id), |out| {
                        out.field("Symbols", table.symbols.len());
                        let lengths = table
                            .counts
                            .iter()
                            .enumerate()
                            .filter(|(_, &count)| count > 0)
                            .map(|(length, count)| format!("{}:{}", length + 1, count))
                            .collect::<Vec<_>>()
                            .join(" ");
                        out.field("Codes per length", lengths);
                        let symbols = table.symbols.iter().map(|s| format!("{:02X}", s)).collect::<Vec<_>>().join(" ");
                        out.text("Values", &symbols);
                    });
                }
            }
            JpegSection::ArithmeticConditioning(tables) => {
                for table in tables {
                    let label = format!("{} table {}", table_class_name(table.class), table.id);
                    if table.class == 0 {
                        out.field(&label, format!("L={}, U={}", table.value & 0x0F, table.value >> 4));
                    } else {
                        out.field(&label, format!("Kx={}", table.value));
                    }
                }
            }
            JpegSection::QuantizationTables(dqt) => {
                if dqt.truncated {
                    out.field("Truncated", "yes");
                }
                for table in &dqt.tables {
                    let precision = match table.precision {
                        0 => "8-bit".to_string(),
                        1 => "16-bit".to_string(),
                        other => format!("invalid precision {}", other),
                    };
                    out.group(format!("Table {} ({})", table.id, precision), |out| {
                        for (row, values) in table.values.chunks(8).enumerate() {
                            let values = values.iter().map(|v| format!("{:>3}", v)).collect::<Vec<_>>().join(" ");
                            out.field(&format!("Row {}", row + 1), values);
                        }
                    });
                }
            }
            JpegSection::RestartInterval(interval) => {
                if *interval == 0 {
                    out.field("Restart interval", "0 (disabled)");
                } else {
                    out.field("Restart interval", format!("{} MCUs", interval));
                }
            }
            JpegSection::NumberOfLines(lines) => {
                out.field("Number of lines", lines);
            }
            JpegSection::Expand(expand) => {
                out.field("Expand horizontally", if expand.horizontal { "yes" } else { "no" });
                out.field("Expand vertically", if expand.vertical { "yes" } else { "no" });
            }
            JpegSection::Scan(scan) => {
                out.field("Components", scan.components.len());
                for component in &scan.components {
                    if scan.lossless {
                        out.field(&format!("Component {}", component.id), format!("DC table {}", component.dc_table));
                    } else {
                        out.field(
                            &format!("Component {}", component.id),
                            format!("DC table {}, AC table {}", component.dc_table, component.ac_table),
                        );
                    }
                }
                if scan.lossless {
                    out.field("Predictor", scan.spectral_start);
                    out.field("Point transform", scan.approximation_low);
                } else {
                    out.field("Spectral selection", format!("{}-{}", scan.spectral_start, scan.spectral_end));
                    out.field(
                        "Successive approximation",
                        format!("high {}, low {}", scan.approximation_high, scan.approximation_low),
                    );
                }
                if scan.restart_interval > 0 {
                    out.field("Restart interval", format!("{} MCUs", scan.restart_interval));
                    out.field("Restart markers", scan.restart_markers);
                }
                out.field("Entropy-coded data", format!("{} bytes", scan.entropy_coded_length));
            }
            JpegSection::StartOfImage
            | JpegSection::Unknown(_)
            | JpegSection::EndOfImage
            | JpegSection::TrailingData => {}
        }
    }
}
