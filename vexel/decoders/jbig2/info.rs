use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::decoder::{parse_segments, ParsedSegment, JBIG2_FILE_ID};
use super::segment::SegmentType;
use crate::utils::info::{ColorModel, FormatInfo, ImageSummary, InfoSection, InfoWriter};

const MAX_SEGMENT_SECTIONS: usize = 1024;
const UNKNOWN_LENGTH: u32 = 0xFFFF_FFFF;

const FILE_SEQUENTIAL: u8 = 0x01;
const FILE_UNKNOWN_PAGES: u8 = 0x02;
const FILE_TEMPLATE_12AT: u8 = 0x04;
const FILE_COLOR: u8 = 0x08;

#[tsify::declare]
pub type Jbig2Info = FormatInfo<Jbig2Section>;

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct Jbig2FileHeader {
    pub flags: u8,
    pub page_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig2Region {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub flags: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig2AtPixel {
    pub x: i8,
    pub y: i8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub enum Jbig2SegmentBody {
    PageInfo {
        width: u32,
        height: u32,
        x_resolution: u32,
        y_resolution: u32,
        flags: u8,
        striping: u16,
    },
    GenericRegion {
        region: Jbig2Region,
        flags: u8,
        at: Vec<Jbig2AtPixel>,
    },
    RefinementRegion {
        region: Jbig2Region,
        flags: u8,
        at: Vec<Jbig2AtPixel>,
    },
    TextRegion {
        region: Jbig2Region,
        flags: u16,
        huffman_flags: Option<u16>,
        refinement_at: Vec<Jbig2AtPixel>,
        instances: Option<u32>,
    },
    SymbolDictionary {
        flags: u16,
        at: Vec<Jbig2AtPixel>,
        refinement_at: Vec<Jbig2AtPixel>,
        exported_symbols: Option<u32>,
        new_symbols: Option<u32>,
    },
    PatternDictionary {
        flags: u8,
        pattern_width: u8,
        pattern_height: u8,
        gray_max: u32,
    },
    HalftoneRegion {
        region: Jbig2Region,
        flags: u8,
        grid_width: Option<u32>,
        grid_height: Option<u32>,
        grid_x: Option<i32>,
        grid_y: Option<i32>,
        vector_x: Option<u16>,
        vector_y: Option<u16>,
    },
    Tables {
        flags: u8,
        low: i32,
        high: i32,
    },
    ColorPalette {
        components: u8,
        component_length: u8,
        count: u8,
    },
    EndOfStripe {
        row: u32,
    },
    Extension {
        kind: u32,
    },
    Empty,
    Truncated,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct Jbig2Segment {
    pub number: u32,
    pub flags: u8,
    pub type_code: u8,
    pub page_association: u32,
    pub referred_to: Vec<u32>,
    pub retention: Vec<bool>,
    pub header_length: u32,
    pub data_offset: u64,
    pub declared_length: u32,
    pub data_length: u64,
    pub body: Jbig2SegmentBody,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig2MoreSegments {
    pub count: u32,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum Jbig2Section {
    FileHeader(Jbig2FileHeader),
    Segment(Box<Jbig2Segment>),
    MoreSegments(Jbig2MoreSegments),
    TrailingData,
}

fn segment_type_name(segment_type: SegmentType) -> &'static str {
    match segment_type {
        SegmentType::SymbolDictionary => "Symbol Dictionary",
        SegmentType::IntermediateTextRegion => "Intermediate Text Region",
        SegmentType::ImmediateTextRegion => "Immediate Text Region",
        SegmentType::ImmediateLosslessTextRegion => "Immediate Lossless Text Region",
        SegmentType::PatternDictionary => "Pattern Dictionary",
        SegmentType::IntermediateHalftoneRegion => "Intermediate Halftone Region",
        SegmentType::ImmediateHalftoneRegion => "Immediate Halftone Region",
        SegmentType::ImmediateLosslessHalftoneRegion => "Immediate Lossless Halftone Region",
        SegmentType::IntermediateGenericRegion => "Intermediate Generic Region",
        SegmentType::ImmediateGenericRegion => "Immediate Generic Region",
        SegmentType::ImmediateLosslessGenericRegion => "Immediate Lossless Generic Region",
        SegmentType::IntermediateRefinementRegion => "Intermediate Refinement Region",
        SegmentType::ImmediateRefinementRegion => "Immediate Refinement Region",
        SegmentType::ImmediateLosslessRefinementRegion => "Immediate Lossless Refinement Region",
        SegmentType::PageInfo => "Page Information",
        SegmentType::EndOfPage => "End of Page",
        SegmentType::EndOfStripe => "End of Stripe",
        SegmentType::EndOfFile => "End of File",
        SegmentType::Profiles => "Profiles",
        SegmentType::Tables => "Code Tables",
        SegmentType::ColorPalette => "Color Palette",
        SegmentType::Extension => "Extension",
        SegmentType::Unknown(_) => "Unknown",
    }
}

fn combination_name(op: u8) -> &'static str {
    match op {
        0 => "OR",
        1 => "AND",
        2 => "XOR",
        3 => "XNOR",
        4 => "REPLACE",
        _ => "reserved",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn at_list(pixels: &[Jbig2AtPixel]) -> String {
    pixels.iter().map(|pixel| format!("({}, {})", pixel.x, pixel.y)).collect::<Vec<_>>().join(", ")
}

fn describe_region(region: &Jbig2Region, out: &mut InfoWriter) {
    out.field("Region", format!("{}x{} at ({}, {})", region.width, region.height, region.x, region.y));
    out.field("Combination operator", format!("{} ({})", combination_name(region.flags & 7), region.flags & 7));
    if region.flags & 0x08 != 0 {
        out.field("Color extension", "yes");
    }
}

fn describe_at(label: &str, pixels: &[Jbig2AtPixel], out: &mut InfoWriter) {
    if !pixels.is_empty() {
        out.field(label, at_list(pixels));
    }
}

fn huffman_table_name(selection: u16, standard: &[&str]) -> String {
    match standard.get(selection as usize) {
        Some(name) => format!("{} ({})", name, selection),
        None => format!("reserved ({})", selection),
    }
}

fn describe_body(body: &Jbig2SegmentBody, out: &mut InfoWriter) {
    match body {
        Jbig2SegmentBody::PageInfo { width, height, x_resolution, y_resolution, flags, striping } => {
            out.field("Width", width);
            if *height == UNKNOWN_LENGTH {
                out.field("Height", format!("unknown ({}), set by end-of-stripe", height));
            } else {
                out.field("Height", height);
            }
            if *x_resolution != 0 || *y_resolution != 0 {
                out.field("Resolution", format!("{}x{} pixels/meter", x_resolution, y_resolution));
            }
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("Eventually lossless", yes_no(flags & 0x01 != 0));
            out.field("Might contain refinements", yes_no(flags & 0x02 != 0));
            out.field("Default pixel", flags >> 2 & 1);
            out.field(
                "Default combination operator",
                format!("{} ({})", combination_name(flags >> 3 & 3), flags >> 3 & 3),
            );
            out.field("Requires auxiliary buffers", yes_no(flags & 0x20 != 0));
            out.field("Combination operator override", yes_no(flags & 0x40 != 0));
            if flags & 0x80 != 0 {
                out.field("Color extension", "yes");
            }
            if striping & 0x8000 != 0 {
                out.field("Striping", format!("yes, max stripe {} rows", striping & 0x7FFF));
            } else {
                out.field("Striping", "no");
            }
        }
        Jbig2SegmentBody::GenericRegion { region, flags, at } => {
            describe_region(region, out);
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("MMR", yes_no(flags & 1 != 0));
            if flags & 1 == 0 {
                out.field("Template", flags >> 1 & 3);
                out.field("Typical prediction (TPGDON)", yes_no(flags & 0x08 != 0));
                out.field("Extended template", yes_no(flags & 0x10 != 0));
            }
            describe_at("AT pixels", at, out);
        }
        Jbig2SegmentBody::RefinementRegion { region, flags, at } => {
            describe_region(region, out);
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("Template", flags & 1);
            out.field("Typical prediction (TPGRON)", yes_no(flags & 0x02 != 0));
            describe_at("AT pixels", at, out);
        }
        Jbig2SegmentBody::TextRegion { region, flags, huffman_flags, refinement_at, instances } => {
            describe_region(region, out);
            out.field("Flags", format!("0x{:04X}", flags));
            out.field("Huffman", yes_no(flags & 1 != 0));
            out.field("Refinement", yes_no(flags & 0x02 != 0));
            out.field("Strip size", 1u32 << (flags >> 2 & 3));
            let corner = ["bottom-left", "top-left", "bottom-right", "top-right"][(flags >> 4 & 3) as usize];
            out.field("Reference corner", corner);
            out.field("Transposed", yes_no(flags & 0x40 != 0));
            let op = (flags >> 7 & 3) as u8;
            out.field("Symbol combination operator", format!("{} ({})", combination_name(op), op));
            out.field("Default pixel", flags >> 9 & 1);
            let mut ds_offset = (flags >> 10 & 0x1F) as i32;
            if ds_offset > 15 {
                ds_offset -= 32;
            }
            out.field("S offset", ds_offset);
            if flags & 0x02 != 0 {
                out.field("Refinement template", flags >> 15 & 1);
            }
            if let Some(huffman) = huffman_flags {
                out.field("Huffman tables", format!("0x{:04X}", huffman));
            }
            describe_at("Refinement AT pixels", refinement_at, out);
            out.opt_field("Symbol instances", *instances);
        }
        Jbig2SegmentBody::SymbolDictionary { flags, at, refinement_at, exported_symbols, new_symbols } => {
            out.field("Flags", format!("0x{:04X}", flags));
            let huffman = flags & 1 != 0;
            out.field("Huffman", yes_no(huffman));
            out.field("Refinement/aggregate", yes_no(flags & 0x02 != 0));
            if huffman {
                out.field(
                    "Height class table",
                    huffman_table_name(flags >> 2 & 3, &["B.4", "B.5", "reserved", "custom"]),
                );
                out.field("Width table", huffman_table_name(flags >> 4 & 3, &["B.2", "B.3", "reserved", "custom"]));
                out.field("Bitmap size table", huffman_table_name(flags >> 6 & 1, &["B.1", "custom"]));
                out.field("Aggregate instance table", huffman_table_name(flags >> 7 & 1, &["B.1", "custom"]));
            } else {
                out.field("Template", flags >> 10 & 3);
            }
            out.field("Bitmap context used", yes_no(flags & 0x0100 != 0));
            out.field("Bitmap context retained", yes_no(flags & 0x0200 != 0));
            if flags & 0x02 != 0 {
                out.field("Refinement template", flags >> 12 & 1);
            }
            describe_at("AT pixels", at, out);
            describe_at("Refinement AT pixels", refinement_at, out);
            out.opt_field("Exported symbols", *exported_symbols);
            out.opt_field("New symbols", *new_symbols);
        }
        Jbig2SegmentBody::PatternDictionary { flags, pattern_width, pattern_height, gray_max } => {
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("MMR", yes_no(flags & 1 != 0));
            if flags & 1 == 0 {
                out.field("Template", flags >> 1 & 3);
            }
            out.field("Pattern size", format!("{}x{}", pattern_width, pattern_height));
            out.field("Gray max", gray_max);
        }
        Jbig2SegmentBody::HalftoneRegion {
            region,
            flags,
            grid_width,
            grid_height,
            grid_x,
            grid_y,
            vector_x,
            vector_y,
        } => {
            describe_region(region, out);
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("MMR", yes_no(flags & 1 != 0));
            if flags & 1 == 0 {
                out.field("Template", flags >> 1 & 3);
            }
            out.field("Skip enabled", yes_no(flags & 0x08 != 0));
            let op = flags >> 4 & 7;
            out.field("Combination operator", format!("{} ({})", combination_name(op), op));
            out.field("Default pixel", flags >> 7 & 1);
            if let (Some(width), Some(height)) = (grid_width, grid_height) {
                out.field("Grid", format!("{}x{}", width, height));
            }
            if let (Some(x), Some(y)) = (grid_x, grid_y) {
                out.field("Grid origin", format!("({}, {})", x, y));
            }
            if let (Some(x), Some(y)) = (vector_x, vector_y) {
                out.field("Grid vector", format!("({}, {})", x, y));
            }
        }
        Jbig2SegmentBody::Tables { flags, low, high } => {
            out.field("Flags", format!("0x{:02X}", flags));
            out.field("Has out-of-band", yes_no(flags & 1 != 0));
            out.field("Prefix length bits", (flags >> 1 & 7) + 1);
            out.field("Range length bits", (flags >> 4 & 7) + 1);
            out.field("Range", format!("{} .. {}", low, high));
        }
        Jbig2SegmentBody::ColorPalette { components, component_length, count } => {
            out.field("Components", components);
            out.field("Component length", format!("{} bytes", component_length));
            out.field("Entries", count);
        }
        Jbig2SegmentBody::EndOfStripe { row } => {
            out.field("End row", row);
        }
        Jbig2SegmentBody::Extension { kind } => {
            out.field("Extension type", format!("0x{:08X}", kind));
        }
        Jbig2SegmentBody::Empty => {}
        Jbig2SegmentBody::Truncated => {
            out.field("Body", "truncated");
        }
    }
}

impl InfoSection for Jbig2Section {
    fn title(&self) -> Cow<'_, str> {
        match self {
            Jbig2Section::FileHeader(_) => Cow::Borrowed("File Header"),
            Jbig2Section::Segment(segment) => match SegmentType::from_flags(segment.flags) {
                SegmentType::Unknown(code) => {
                    Cow::Owned(format!("Segment {}: Unknown (type {})", segment.number, code))
                }
                known => Cow::Owned(format!("Segment {}: {}", segment.number, segment_type_name(known))),
            },
            Jbig2Section::MoreSegments(_) => Cow::Borrowed("More Segments"),
            Jbig2Section::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            Jbig2Section::FileHeader(header) => {
                out.field("Flags", format!("0x{:02X}", header.flags));
                out.field(
                    "Organization",
                    if header.flags & FILE_SEQUENTIAL != 0 { "sequential" } else { "random access" },
                );
                match header.page_count {
                    Some(count) => out.field("Pages", count),
                    None => out.field("Pages", "unknown"),
                };
                if header.flags & FILE_TEMPLATE_12AT != 0 {
                    out.field("Extended templates", "yes");
                }
                if header.flags & FILE_COLOR != 0 {
                    out.field("Color extension", "yes");
                }
            }
            Jbig2Section::Segment(segment) => {
                if segment.flags & 0x80 != 0 {
                    out.field("Deferred non-retain", "yes");
                }
                out.field("Page", segment.page_association);
                if segment.retention.first().copied().unwrap_or(false) {
                    out.field("Retained", "yes");
                }
                if !segment.referred_to.is_empty() {
                    let referred: Vec<String> = segment
                        .referred_to
                        .iter()
                        .enumerate()
                        .map(|(index, number)| {
                            let retained = segment.retention.get(index + 1).copied().unwrap_or(false);
                            if retained {
                                format!("{} (retained)", number)
                            } else {
                                number.to_string()
                            }
                        })
                        .collect();
                    out.field("Refers to", referred.join(", "));
                }
                out.field("Header length", format!("{} bytes", segment.header_length));
                if segment.declared_length == UNKNOWN_LENGTH {
                    out.field("Data length", format!("unknown, resolved to {} bytes", segment.data_length));
                } else {
                    out.field("Data length", format!("{} bytes", segment.declared_length));
                }
                out.field("Data offset", format!("0x{:08X}", segment.data_offset));
                describe_body(&segment.body, out);
            }
            Jbig2Section::MoreSegments(more) => {
                out.field("Segments", more.count);
                out.field("Data", format!("{} bytes", more.data_length));
            }
            Jbig2Section::TrailingData => {}
        }
    }
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4).map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn be_i32(data: &[u8], at: usize) -> Option<i32> {
    be_u32(data, at).map(|value| value as i32)
}

fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn at_pixels(body: &[u8], offset: &mut usize, count: usize) -> Vec<Jbig2AtPixel> {
    let mut pixels = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(bytes) = body.get(*offset..*offset + 2) else {
            break;
        };
        pixels.push(Jbig2AtPixel { x: bytes[0] as i8, y: bytes[1] as i8 });
        *offset += 2;
    }
    pixels
}

fn region(body: &[u8]) -> Option<Jbig2Region> {
    Some(Jbig2Region {
        width: be_u32(body, 0)?,
        height: be_u32(body, 4)?,
        x: be_u32(body, 8)?,
        y: be_u32(body, 12)?,
        flags: *body.get(16)?,
    })
}

fn parse_body(segment_type: SegmentType, body: &[u8]) -> Jbig2SegmentBody {
    parse_known_body(segment_type, body).unwrap_or(Jbig2SegmentBody::Truncated)
}

fn parse_known_body(segment_type: SegmentType, body: &[u8]) -> Option<Jbig2SegmentBody> {
    let parsed = match segment_type {
        SegmentType::PageInfo => Jbig2SegmentBody::PageInfo {
            width: be_u32(body, 0)?,
            height: be_u32(body, 4)?,
            x_resolution: be_u32(body, 8)?,
            y_resolution: be_u32(body, 12)?,
            flags: *body.get(16)?,
            striping: be_u16(body, 17)?,
        },
        SegmentType::IntermediateGenericRegion
        | SegmentType::ImmediateGenericRegion
        | SegmentType::ImmediateLosslessGenericRegion => {
            let region = region(body)?;
            let flags = *body.get(17)?;
            let mut offset = 18;
            let count = match (flags & 1 != 0, flags & 0x10 != 0, flags >> 1 & 3) {
                (true, _, _) => 0,
                (false, true, _) => 12,
                (false, false, 0) => 4,
                _ => 1,
            };
            let at = at_pixels(body, &mut offset, count);
            Jbig2SegmentBody::GenericRegion { region, flags, at }
        }
        SegmentType::IntermediateRefinementRegion
        | SegmentType::ImmediateRefinementRegion
        | SegmentType::ImmediateLosslessRefinementRegion => {
            let region = region(body)?;
            let flags = *body.get(17)?;
            let mut offset = 18;
            let at = at_pixels(body, &mut offset, if flags & 1 == 0 { 2 } else { 0 });
            Jbig2SegmentBody::RefinementRegion { region, flags, at }
        }
        SegmentType::IntermediateTextRegion
        | SegmentType::ImmediateTextRegion
        | SegmentType::ImmediateLosslessTextRegion => {
            let region = region(body)?;
            let flags = be_u16(body, 17)?;
            let mut offset = 19;
            let huffman_flags = if flags & 1 != 0 {
                let value = be_u16(body, offset);
                offset += 2;
                value
            } else {
                None
            };
            let refinement_count = if flags & 0x02 != 0 && flags >> 15 == 0 { 2 } else { 0 };
            let refinement_at = at_pixels(body, &mut offset, refinement_count);
            let instances = be_u32(body, offset);
            Jbig2SegmentBody::TextRegion { region, flags, huffman_flags, refinement_at, instances }
        }
        SegmentType::SymbolDictionary => {
            let flags = be_u16(body, 0)?;
            let mut offset = 2;
            let at_count = match (flags & 1 != 0, flags >> 10 & 3) {
                (true, _) => 0,
                (false, 0) => 4,
                _ => 1,
            };
            let at = at_pixels(body, &mut offset, at_count);
            let refinement_count = if flags & 0x02 != 0 && flags >> 12 & 1 == 0 { 2 } else { 0 };
            let refinement_at = at_pixels(body, &mut offset, refinement_count);
            Jbig2SegmentBody::SymbolDictionary {
                flags,
                at,
                refinement_at,
                exported_symbols: be_u32(body, offset),
                new_symbols: be_u32(body, offset + 4),
            }
        }
        SegmentType::PatternDictionary => Jbig2SegmentBody::PatternDictionary {
            flags: *body.first()?,
            pattern_width: *body.get(1)?,
            pattern_height: *body.get(2)?,
            gray_max: be_u32(body, 3)?,
        },
        SegmentType::IntermediateHalftoneRegion
        | SegmentType::ImmediateHalftoneRegion
        | SegmentType::ImmediateLosslessHalftoneRegion => Jbig2SegmentBody::HalftoneRegion {
            region: region(body)?,
            flags: *body.get(17)?,
            grid_width: be_u32(body, 18),
            grid_height: be_u32(body, 22),
            grid_x: be_i32(body, 26),
            grid_y: be_i32(body, 30),
            vector_x: be_u16(body, 34),
            vector_y: be_u16(body, 36),
        },
        SegmentType::Tables => {
            Jbig2SegmentBody::Tables { flags: *body.first()?, low: be_i32(body, 1)?, high: be_i32(body, 5)? }
        }
        SegmentType::ColorPalette => Jbig2SegmentBody::ColorPalette {
            components: *body.first()?,
            component_length: *body.get(1)?,
            count: *body.get(2)?,
        },
        SegmentType::EndOfStripe => Jbig2SegmentBody::EndOfStripe { row: be_u32(body, 0)? },
        SegmentType::Extension => Jbig2SegmentBody::Extension { kind: be_u32(body, 0)? },
        SegmentType::EndOfPage | SegmentType::EndOfFile | SegmentType::Profiles | SegmentType::Unknown(_) => {
            Jbig2SegmentBody::Empty
        }
    };
    Some(parsed)
}

fn segment_info(segment: &ParsedSegment, data: &[u8]) -> Jbig2Segment {
    let header = &segment.header;
    let body = data.get(segment.start..segment.end).unwrap_or(&[]);
    Jbig2Segment {
        number: header.number,
        flags: header.flags,
        type_code: header.flags & 0x3F,
        page_association: header.page_association,
        referred_to: header.referred_to.clone(),
        retention: header.retention.clone(),
        header_length: header.header_length as u32,
        data_offset: segment.start as u64,
        declared_length: header.data_length,
        data_length: (segment.end - segment.start) as u64,
        body: parse_body(header.segment_type, body),
    }
}

struct PageTracker {
    number: Option<u32>,
    current: Option<u32>,
    width: u32,
    height: u32,
    declared_height: u32,
    growable: bool,
    color: bool,
    pages: u32,
}

impl PageTracker {
    fn observe(&mut self, segment: &ParsedSegment, data: &[u8]) {
        let header = &segment.header;
        let body = data.get(segment.start..segment.end).unwrap_or(&[]);

        if header.segment_type == SegmentType::PageInfo {
            if body.len() < 19 {
                return;
            }
            self.pages += 1;
            self.current = Some(header.page_association);
            if self.number.is_some() {
                return;
            }
            let height = be_u32(body, 4).unwrap_or(0);
            let flags = body[16];
            let striping = be_u16(body, 17).unwrap_or(0);
            let striped = striping & 0x8000 != 0;
            let stripe_size = (striping & 0x7FFF) as u32;
            self.number = Some(header.page_association);
            self.width = be_u32(body, 0).unwrap_or(0);
            self.declared_height = height;
            self.height = match (height == UNKNOWN_LENGTH, striped && stripe_size > 0) {
                (false, _) => height,
                (true, true) => stripe_size,
                (true, false) => 0,
            };
            self.growable = striped || height == UNKNOWN_LENGTH;
            self.color = flags & 0x80 != 0;
            return;
        }

        if header.page_association != 0 {
            self.current = Some(header.page_association);
        }
        if self.number.is_none() || self.current != self.number {
            return;
        }

        match header.segment_type {
            SegmentType::EndOfStripe if self.declared_height == UNKNOWN_LENGTH => {
                if let Some(row) = be_u32(body, 0) {
                    self.height = self.height.max(row.saturating_add(1));
                }
            }
            SegmentType::ImmediateGenericRegion
            | SegmentType::ImmediateLosslessGenericRegion
            | SegmentType::ImmediateTextRegion
            | SegmentType::ImmediateLosslessTextRegion
            | SegmentType::ImmediateHalftoneRegion
            | SegmentType::ImmediateLosslessHalftoneRegion
            | SegmentType::ImmediateRefinementRegion
            | SegmentType::ImmediateLosslessRefinementRegion
                if self.growable =>
            {
                if let Some(region) = region(body) {
                    self.height = self.height.max(region.y.saturating_add(region.height));
                }
            }
            _ => {}
        }
    }

    fn summary(&self) -> Option<ImageSummary> {
        self.number?;
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let mut summary = if self.color {
            ImageSummary::new(self.width, self.height, ColorModel::Rgb, 3, 8)
        } else {
            ImageSummary::new(self.width, self.height, ColorModel::Bilevel, 1, 1)
        };
        summary.compression = Some("JBIG2 (T.88)".to_string());
        summary.pages = Some(self.pages);
        Some(summary)
    }
}

pub fn scan(data: &[u8]) -> Jbig2Info {
    let mut info = Jbig2Info::default();
    let has_file_header = data.len() >= 8 && data[..8] == JBIG2_FILE_ID;

    if has_file_header && data.len() >= 9 {
        let flags = data[8];
        let unknown_pages = flags & FILE_UNKNOWN_PAGES != 0;
        let page_count = if unknown_pages { None } else { be_u32(data, 9) };
        let length = if unknown_pages { 9 } else { 13 };
        info.push(
            0,
            Some(length.min(data.len()) as u64),
            Jbig2Section::FileHeader(Jbig2FileHeader { flags, page_count }),
        );
    }

    let segments = parse_segments(data, has_file_header);
    let mut tracker = PageTracker {
        number: None,
        current: None,
        width: 0,
        height: 0,
        declared_height: 0,
        growable: false,
        color: false,
        pages: 0,
    };
    let mut more: Option<(u64, Jbig2MoreSegments)> = None;
    let mut end = info.sections.first().and_then(|section| section.length).unwrap_or(0) as usize;

    for (index, segment) in segments.iter().enumerate() {
        tracker.observe(segment, data);
        end = end.max(segment.end).max(segment.header_offset + segment.header.header_length);

        let contiguous = segment.start == segment.header_offset + segment.header.header_length;
        let length = if contiguous {
            segment.end - segment.header_offset
        } else {
            segment.header.header_length.min(data.len() - segment.header_offset)
        };

        if index < MAX_SEGMENT_SECTIONS {
            let section = Jbig2Section::Segment(Box::new(segment_info(segment, data)));
            info.push(segment.header_offset as u64, Some(length as u64), section);
        } else {
            let (_, more) =
                more.get_or_insert((segment.header_offset as u64, Jbig2MoreSegments { count: 0, data_length: 0 }));
            more.count += 1;
            more.data_length += (segment.end - segment.start) as u64;
        }
    }

    if let Some((offset, more)) = more {
        info.push(offset, None, Jbig2Section::MoreSegments(more));
    }
    if end < data.len() {
        info.push(end as u64, Some((data.len() - end) as u64), Jbig2Section::TrailingData);
    }

    info.summary = tracker.summary();
    info
}
