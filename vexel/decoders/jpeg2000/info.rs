use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::codestream::{self, Jp2Container};
use super::decoder::resolve_colorspace;
use super::types::{Colorspace, JP2_SIGNATURE};
use crate::log_warn;
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat, Section};

const MAX_TILE_PART_SECTIONS: usize = 256;
const MAX_SECTIONS: usize = 4096;
const MAX_TILE_HEADER_MARKERS: usize = 256;
const MAX_BOX_DEPTH: u32 = 8;
const MAX_TEXT_BYTES: usize = 4096;

#[tsify::declare]
pub type Jpeg2000Info = FormatInfo<Jpeg2000Section>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kBox {
    pub box_type: String,
    pub superbox: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kFileType {
    pub brand: String,
    pub minor_version: u32,
    pub compatibility: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kImageHeader {
    pub height: u32,
    pub width: u32,
    pub components: u16,
    pub bits_per_component: u8,
    pub compression: u8,
    pub unknown_colorspace: u8,
    pub intellectual_property: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct J2kColorSpecification {
    pub method: u8,
    pub precedence: i8,
    pub approximation: u8,
    pub enumerated: Option<u32>,
    pub icc_length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kPaletteBox {
    pub entries: u16,
    pub columns: u8,
    pub depths: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kComponentMapping {
    pub component: u16,
    pub mapping_type: u8,
    pub palette_column: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kChannelDefinition {
    pub channel: u16,
    pub channel_type: u16,
    pub association: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kResolution {
    pub display: bool,
    pub vertical: f64,
    pub horizontal: f64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kSizComponent {
    pub ssiz: u8,
    pub dx: u8,
    pub dy: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kSiz {
    pub rsiz: u16,
    pub xsiz: u32,
    pub ysiz: u32,
    pub x0siz: u32,
    pub y0siz: u32,
    pub xtsiz: u32,
    pub ytsiz: u32,
    pub xt0siz: u32,
    pub yt0siz: u32,
    pub declared_components: u16,
    pub components: Vec<J2kSizComponent>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kCodingStyle {
    pub decomposition_levels: u8,
    pub code_block_width: u8,
    pub code_block_height: u8,
    pub code_block_style: u8,
    pub transform: u8,
    pub precincts: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kCod {
    pub scod: u8,
    pub progression: u8,
    pub layers: u16,
    pub mct: u8,
    pub style: Option<J2kCodingStyle>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kCoc {
    pub component: u16,
    pub scoc: u8,
    pub style: Option<J2kCodingStyle>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct J2kQuantization {
    pub component: Option<u16>,
    pub sqcd: u8,
    pub step_sizes: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kRegion {
    pub component: u16,
    pub style: u8,
    pub shift: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kProgressionChange {
    pub resolution_start: u8,
    pub component_start: u16,
    pub layer_end: u16,
    pub resolution_end: u8,
    pub component_end: u16,
    pub order: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct J2kComment {
    pub registration: u16,
    pub text: Option<String>,
    pub data_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kPacketMarker {
    pub index: u8,
    pub data_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum J2kMarkerBody {
    StartOfCodestream,
    ImageSize(J2kSiz),
    Capabilities(Vec<u32>),
    CodingStyle(J2kCod),
    ComponentCodingStyle(J2kCoc),
    Quantization(J2kQuantization),
    ComponentQuantization(J2kQuantization),
    RegionOfInterest(J2kRegion),
    ProgressionOrderChange(Vec<J2kProgressionChange>),
    TilePartLengths(J2kPacketMarker),
    PacketLengthsMain(J2kPacketMarker),
    PacketLengthsTile(J2kPacketMarker),
    PackedPacketHeadersMain(J2kPacketMarker),
    PackedPacketHeadersTile(J2kPacketMarker),
    ComponentRegistration(Vec<[u16; 2]>),
    Comment(J2kComment),
    EndOfCodestream,
    Unknown(u16),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kTilePart {
    pub tile: u16,
    pub part: u8,
    pub parts: u8,
    pub declared_length: u32,
    pub header_markers: Vec<Section<J2kMarkerBody>>,
    pub more_header_markers: u32,
    pub data_length: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kTilePartRemainder {
    pub tile_parts: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct J2kSectionRemainder {
    pub sections: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum Jpeg2000Section {
    Box(J2kBox),
    FileType(J2kFileType),
    ImageHeader(J2kImageHeader),
    ColorSpecification(J2kColorSpecification),
    Palette(J2kPaletteBox),
    ComponentMapping(Vec<J2kComponentMapping>),
    ChannelDefinition(Vec<J2kChannelDefinition>),
    BitsPerComponent(Vec<u8>),
    Resolution(J2kResolution),
    Xml(String),
    Uuid(String),
    Marker(J2kMarkerBody),
    TilePart(J2kTilePart),
    MoreTileParts(J2kTilePartRemainder),
    MoreSections(J2kSectionRemainder),
}

struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    fn u16(&self, at: usize) -> u16 {
        u16::from_be_bytes([self.u8(at), self.u8(at + 1)])
    }

    fn u32(&self, at: usize) -> u32 {
        u32::from_be_bytes([self.u8(at), self.u8(at + 1), self.u8(at + 2), self.u8(at + 3)])
    }
}

fn four_cc(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }).collect()
}

struct Scanner {
    sections: Vec<Section<Jpeg2000Section>>,
    icc_profile: Option<ICCProfile>,
    detailed_tile_parts: usize,
    remainder: Option<(u64, J2kTilePartRemainder)>,
    more_sections: Option<(u64, J2kSectionRemainder)>,
}

impl Scanner {
    fn push(&mut self, offset: usize, length: Option<u64>, section: Jpeg2000Section) {
        if self.sections.len() < MAX_SECTIONS {
            self.sections.push(Section::new(offset as u64, length, section));
        } else {
            self.skip(offset, length);
        }
    }

    fn skip(&mut self, offset: usize, length: Option<u64>) {
        let (_, more) =
            self.more_sections.get_or_insert((offset as u64, J2kSectionRemainder { sections: 0, bytes: 0 }));
        more.sections = more.sections.saturating_add(1);
        more.bytes = more.bytes.saturating_add(length.unwrap_or(0));
    }

    fn walk_boxes(&mut self, data: &[u8], base: usize, depth: u32) {
        if depth > MAX_BOX_DEPTH {
            return;
        }

        let mut pos = 0usize;
        while pos + 8 <= data.len() {
            let bytes = Bytes(&data[pos..]);
            let mut length = u64::from(bytes.u32(0));
            let type_bytes = &data[pos + 4..pos + 8];
            let mut header = 8usize;
            if length == 1 {
                if pos + 16 > data.len() {
                    break;
                }
                length = (u64::from(bytes.u32(8)) << 32) | u64::from(bytes.u32(12));
                header = 16;
            } else if length == 0 {
                length = (data.len() - pos) as u64;
            }
            if length < header as u64 {
                log_warn!("JPEG 2000: box at offset {} has invalid length {}", base + pos, length);
                break;
            }

            let end = (pos as u64).saturating_add(length).min(data.len() as u64) as usize;
            let body = &data[(pos + header).min(end)..end];
            let offset = base + pos;
            let span = Some(length);

            match type_bytes {
                b"jp2h" | b"res " | b"jpch" | b"jplh" | b"uinf" => {
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::Box(J2kBox { box_type: four_cc(type_bytes), superbox: true }),
                    );
                    self.walk_boxes(body, base + pos + header, depth + 1);
                }
                b"ftyp" => {
                    let b = Bytes(body);
                    let compatibility = body.get(8..).unwrap_or(&[]).chunks_exact(4).map(four_cc).collect();
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::FileType(J2kFileType {
                            brand: four_cc(&body[..body.len().min(4)]),
                            minor_version: b.u32(4),
                            compatibility,
                        }),
                    );
                }
                b"ihdr" => {
                    let b = Bytes(body);
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::ImageHeader(J2kImageHeader {
                            height: b.u32(0),
                            width: b.u32(4),
                            components: b.u16(8),
                            bits_per_component: b.u8(10),
                            compression: b.u8(11),
                            unknown_colorspace: b.u8(12),
                            intellectual_property: b.u8(13),
                        }),
                    );
                }
                b"colr" => {
                    let b = Bytes(body);
                    let method = b.u8(0);
                    let icc = body.get(3..).filter(|_| method == 2 || method == 3);
                    if let Some(icc) = icc {
                        if self.icc_profile.is_none() {
                            match ICCProfile::new(icc) {
                                Ok(profile) => self.icc_profile = Some(profile),
                                Err(e) => log_warn!("JPEG 2000: failed to parse colr ICC profile: {}", e),
                            }
                        }
                    }
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::ColorSpecification(J2kColorSpecification {
                            method,
                            precedence: b.u8(1) as i8,
                            approximation: b.u8(2),
                            enumerated: (method == 1).then(|| b.u32(3)),
                            icc_length: icc.map(|icc| icc.len() as u32),
                        }),
                    );
                }
                b"pclr" => {
                    let b = Bytes(body);
                    let columns = b.u8(2);
                    let depths = body.get(3..3 + columns as usize).unwrap_or(&[]).to_vec();
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::Palette(J2kPaletteBox { entries: b.u16(0), columns, depths }),
                    );
                }
                b"cmap" => {
                    let mappings = body
                        .chunks_exact(4)
                        .map(|c| J2kComponentMapping {
                            component: u16::from_be_bytes([c[0], c[1]]),
                            mapping_type: c[2],
                            palette_column: c[3],
                        })
                        .collect();
                    self.push(offset, span, Jpeg2000Section::ComponentMapping(mappings));
                }
                b"cdef" => {
                    let definitions = body
                        .get(2..)
                        .unwrap_or(&[])
                        .chunks_exact(6)
                        .map(|c| J2kChannelDefinition {
                            channel: u16::from_be_bytes([c[0], c[1]]),
                            channel_type: u16::from_be_bytes([c[2], c[3]]),
                            association: u16::from_be_bytes([c[4], c[5]]),
                        })
                        .collect();
                    self.push(offset, span, Jpeg2000Section::ChannelDefinition(definitions));
                }
                b"bpcc" => self.push(offset, span, Jpeg2000Section::BitsPerComponent(body.to_vec())),
                b"resc" | b"resd" => {
                    let b = Bytes(body);
                    let value = |n: u16, d: u16, e: u8| {
                        if d == 0 {
                            0.0
                        } else {
                            f64::from(n) / f64::from(d) * 10f64.powi(i32::from(e as i8))
                        }
                    };
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::Resolution(J2kResolution {
                            display: type_bytes == b"resd",
                            vertical: value(b.u16(0), b.u16(2), b.u8(8)),
                            horizontal: value(b.u16(4), b.u16(6), b.u8(9)),
                        }),
                    );
                }
                b"xml " => {
                    let text = String::from_utf8_lossy(&body[..body.len().min(MAX_TEXT_BYTES)]).into_owned();
                    self.push(offset, span, Jpeg2000Section::Xml(text));
                }
                b"uuid" => {
                    let uuid =
                        body.get(..16).map(|id| id.iter().map(|b| format!("{:02X}", b)).collect()).unwrap_or_default();
                    self.push(offset, span, Jpeg2000Section::Uuid(uuid));
                }
                b"jp2c" => {
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::Box(J2kBox { box_type: four_cc(type_bytes), superbox: false }),
                    );
                    self.walk_codestream(body, base + pos + header);
                }
                _ => {
                    self.push(
                        offset,
                        span,
                        Jpeg2000Section::Box(J2kBox { box_type: four_cc(type_bytes), superbox: false }),
                    );
                }
            }

            if end <= pos {
                break;
            }
            pos = end;
        }
    }

    fn walk_codestream(&mut self, data: &[u8], base: usize) {
        let bytes = Bytes(data);
        let mut pos = 0usize;
        let mut component_count = 0usize;

        while pos + 2 <= data.len() {
            let code = bytes.u16(pos);
            match code {
                0xFF4F => {
                    self.push(base + pos, Some(2), Jpeg2000Section::Marker(J2kMarkerBody::StartOfCodestream));
                    pos += 2;
                }
                0xFFD9 => {
                    self.push(base + pos, Some(2), Jpeg2000Section::Marker(J2kMarkerBody::EndOfCodestream));
                    return;
                }
                0xFF90 => {
                    pos = self.walk_tile_part(data, base, pos, component_count);
                }
                _ if code & 0xFF00 != 0xFF00 => {
                    log_warn!("JPEG 2000: expected a marker at codestream offset {}, stopping", pos);
                    return;
                }
                _ => {
                    let (marker, next) = read_marker(data, pos, component_count);
                    if let J2kMarkerBody::ImageSize(siz) = &marker {
                        component_count = siz.components.len();
                    }
                    self.push(base + pos, Some((next - pos) as u64), Jpeg2000Section::Marker(marker));
                    pos = next;
                }
            }
        }
    }

    fn walk_tile_part(&mut self, data: &[u8], base: usize, start: usize, component_count: usize) -> usize {
        let bytes = Bytes(data);
        let tile = bytes.u16(start + 4);
        let declared_length = bytes.u32(start + 6);
        let part = bytes.u8(start + 10);
        let parts = bytes.u8(start + 11);
        let header_end = start + 2 + usize::from(bytes.u16(start + 2));

        let declared_end = start.saturating_add(declared_length as usize);
        let truncated = declared_length != 0 && declared_end > data.len();
        let end = if declared_length == 0 {
            data.len().saturating_sub(2).max(header_end)
        } else {
            declared_end.min(data.len())
        };

        let mut header_markers = Vec::new();
        let mut more_header_markers = 0u32;
        let mut pos = header_end;
        let mut data_length = 0u64;
        while pos + 2 <= end {
            let code = bytes.u16(pos);
            if code == 0xFF93 {
                data_length = end.saturating_sub(pos + 2) as u64;
                break;
            }
            if code & 0xFF00 != 0xFF00 {
                break;
            }
            let (marker, next) = read_marker(data, pos, component_count);
            if header_markers.len() < MAX_TILE_HEADER_MARKERS {
                header_markers.push(Section::new((base + pos) as u64, Some((next - pos) as u64), marker));
            } else {
                more_header_markers = more_header_markers.saturating_add(1);
            }
            pos = next;
        }

        let span = end.saturating_sub(start) as u64;
        if self.detailed_tile_parts < MAX_TILE_PART_SECTIONS {
            self.detailed_tile_parts += 1;
            self.push(
                base + start,
                Some(span),
                Jpeg2000Section::TilePart(J2kTilePart {
                    tile,
                    part,
                    parts,
                    declared_length,
                    header_markers,
                    more_header_markers,
                    data_length,
                    truncated,
                }),
            );
        } else {
            let remainder = self
                .remainder
                .get_or_insert_with(|| ((base + start) as u64, J2kTilePartRemainder { tile_parts: 0, bytes: 0 }));
            remainder.1.tile_parts += 1;
            remainder.1.bytes += span;
        }

        end.max(start + 2)
    }
}

fn read_coding_style(body: &[u8], at: usize, precincts: bool) -> Option<J2kCodingStyle> {
    let b = Bytes(body);
    if body.len() < at + 5 {
        return None;
    }
    let decomposition_levels = b.u8(at);
    let precinct_bytes = if precincts {
        body.get(at + 5..(at + 5 + usize::from(decomposition_levels) + 1).min(body.len())).unwrap_or(&[]).to_vec()
    } else {
        Vec::new()
    };
    Some(J2kCodingStyle {
        decomposition_levels,
        code_block_width: b.u8(at + 1),
        code_block_height: b.u8(at + 2),
        code_block_style: b.u8(at + 3),
        transform: b.u8(at + 4),
        precincts: precinct_bytes,
    })
}

fn read_marker(data: &[u8], pos: usize, component_count: usize) -> (J2kMarkerBody, usize) {
    let bytes = Bytes(data);
    let code = bytes.u16(pos);
    if (0xFF30..=0xFF3F).contains(&code) || code == 0xFF92 {
        return (J2kMarkerBody::Unknown(code), pos + 2);
    }

    let length = usize::from(bytes.u16(pos + 2)).max(2);
    let next = (pos + 2 + length).min(data.len()).max(pos + 2);
    let body = &data[(pos + 4).min(next)..next];
    let b = Bytes(body);
    let wide = component_count >= 257;
    let component_size = if wide { 2 } else { 1 };
    let component_at = |at: usize| if wide { b.u16(at) } else { u16::from(b.u8(at)) };

    let marker = match code {
        0xFF51 => {
            let declared = b.u16(34);
            let components = body
                .get(36..)
                .unwrap_or(&[])
                .chunks_exact(3)
                .take(usize::from(declared))
                .map(|c| J2kSizComponent { ssiz: c[0], dx: c[1], dy: c[2] })
                .collect();
            J2kMarkerBody::ImageSize(J2kSiz {
                rsiz: b.u16(0),
                xsiz: b.u32(2),
                ysiz: b.u32(6),
                x0siz: b.u32(10),
                y0siz: b.u32(14),
                xtsiz: b.u32(18),
                ytsiz: b.u32(22),
                xt0siz: b.u32(26),
                yt0siz: b.u32(30),
                declared_components: declared,
                components,
            })
        }
        0xFF50 => J2kMarkerBody::Capabilities(
            std::iter::once(b.u32(0))
                .chain(
                    body.get(4..).unwrap_or(&[]).chunks_exact(2).map(|c| u32::from(u16::from_be_bytes([c[0], c[1]]))),
                )
                .collect(),
        ),
        0xFF52 => {
            let scod = b.u8(0);
            J2kMarkerBody::CodingStyle(J2kCod {
                scod,
                progression: b.u8(1),
                layers: b.u16(2),
                mct: b.u8(4),
                style: read_coding_style(body, 5, scod & 1 != 0),
            })
        }
        0xFF53 => {
            let scoc = b.u8(component_size);
            J2kMarkerBody::ComponentCodingStyle(J2kCoc {
                component: component_at(0),
                scoc,
                style: read_coding_style(body, component_size + 1, scoc & 1 != 0),
            })
        }
        0xFF5C | 0xFF5D => {
            let (component, at) = if code == 0xFF5D { (Some(component_at(0)), component_size) } else { (None, 0) };
            let sqcd = b.u8(at);
            let step_bytes = body.len().saturating_sub(at + 1);
            let step_sizes = if sqcd & 0x1F == 0 { step_bytes } else { step_bytes / 2 } as u32;
            let quant = J2kQuantization { component, sqcd, step_sizes };
            if code == 0xFF5C {
                J2kMarkerBody::Quantization(quant)
            } else {
                J2kMarkerBody::ComponentQuantization(quant)
            }
        }
        0xFF5E => J2kMarkerBody::RegionOfInterest(J2kRegion {
            component: component_at(0),
            style: b.u8(component_size),
            shift: b.u8(component_size + 1),
        }),
        0xFF5F => {
            let entry_size = 5 + 2 * component_size;
            let changes = body
                .chunks_exact(entry_size)
                .map(|c| {
                    let e = Bytes(c);
                    let comp = |at: usize| if wide { e.u16(at) } else { u16::from(e.u8(at)) };
                    J2kProgressionChange {
                        resolution_start: e.u8(0),
                        component_start: comp(1),
                        layer_end: e.u16(1 + component_size),
                        resolution_end: e.u8(3 + component_size),
                        component_end: comp(4 + component_size),
                        order: e.u8(4 + 2 * component_size),
                    }
                })
                .collect();
            J2kMarkerBody::ProgressionOrderChange(changes)
        }
        0xFF55 | 0xFF57 | 0xFF58 | 0xFF60 | 0xFF61 => {
            let packet = J2kPacketMarker { index: b.u8(0), data_length: body.len().saturating_sub(1) as u32 };
            match code {
                0xFF55 => J2kMarkerBody::TilePartLengths(packet),
                0xFF57 => J2kMarkerBody::PacketLengthsMain(packet),
                0xFF58 => J2kMarkerBody::PacketLengthsTile(packet),
                0xFF60 => J2kMarkerBody::PackedPacketHeadersMain(packet),
                _ => J2kMarkerBody::PackedPacketHeadersTile(packet),
            }
        }
        0xFF63 => J2kMarkerBody::ComponentRegistration(
            body.chunks_exact(4)
                .map(|c| [u16::from_be_bytes([c[0], c[1]]), u16::from_be_bytes([c[2], c[3]])])
                .collect(),
        ),
        0xFF64 => {
            let registration = b.u16(0);
            let payload = body.get(2..).unwrap_or(&[]);
            J2kMarkerBody::Comment(J2kComment {
                registration,
                text: (registration == 1).then(|| {
                    String::from_utf8_lossy(&payload[..payload.len().min(MAX_TEXT_BYTES)])
                        .trim_end_matches('\0')
                        .to_string()
                }),
                data_length: payload.len() as u32,
            })
        }
        other => J2kMarkerBody::Unknown(other),
    };

    (marker, next)
}

fn component_color_model(colorspace: Option<Colorspace>, components: usize, has_alpha: bool) -> ColorModel {
    match colorspace {
        Some(Colorspace::Gray) if has_alpha || components == 2 => ColorModel::GrayAlpha,
        Some(Colorspace::Gray) => ColorModel::Gray,
        Some(Colorspace::Srgb) if has_alpha || components >= 4 => ColorModel::Rgba,
        Some(Colorspace::Srgb) => ColorModel::Rgb,
        Some(Colorspace::Sycc) | Some(Colorspace::Eycc) => ColorModel::YCbCr,
        Some(Colorspace::Cmyk) => ColorModel::Cmyk,
        Some(Colorspace::Cielab) => ColorModel::Lab,
        _ => ColorModel::Multichannel,
    }
}

fn build_summary(codestream_data: &[u8], container: Option<&Jp2Container>) -> Option<ImageSummary> {
    let parsed = match codestream::parse(codestream_data) {
        Ok(parsed) => parsed,
        Err(e) => {
            log_warn!("JPEG 2000: failed to parse codestream for info: {}", e);
            return None;
        }
    };
    let siz = &parsed.siz;
    let components = siz.components.len();
    let has_alpha = container.is_some_and(|c| c.channel_definitions.iter().any(|d| d.kind.is_alpha()));

    let color_model = if container.is_some_and(|c| c.palette.is_some()) {
        ColorModel::Indexed
    } else {
        let declared = container.and_then(|c| c.enumerated_colorspace);
        let fallback = match components {
            1 | 2 => Some(Colorspace::Gray),
            3 | 4 => Some(Colorspace::Srgb),
            _ => None,
        };
        let colorspace = resolve_colorspace(container, siz, components).or(declared).or(fallback);
        component_color_model(colorspace, components, has_alpha)
    };

    let first = siz.components.first()?;
    let mut summary =
        ImageSummary::new(siz.width(), siz.height(), color_model, components as u16, first.prec.min(255) as u8);
    summary.has_alpha = has_alpha || matches!(color_model, ColorModel::GrayAlpha | ColorModel::Rgba);
    if first.signed {
        summary.sample_format = SampleFormat::Signed;
    }

    let coding = parsed.defaults.first().map(|c| c.coding.clone()).unwrap_or_default();
    let mut parts = vec![if parsed.uses_ht || coding.cblk_style.high_throughput() {
        "JPEG 2000 (HTJ2K)".to_string()
    } else {
        "JPEG 2000".to_string()
    }];
    parts.push(if coding.reversible {
        "5/3 reversible wavelet".to_string()
    } else {
        "9/7 irreversible wavelet".to_string()
    });
    if parsed.mct {
        parts.push(if coding.reversible { "RCT".to_string() } else { "ICT".to_string() });
    }
    parts.push(format!("{} decomposition levels", coding.num_resolutions.saturating_sub(1)));
    parts.push(format!("{:?}", parsed.progression).to_uppercase());
    parts.push(format!("{} quality layer{}", parsed.num_layers, if parsed.num_layers == 1 { "" } else { "s" }));
    summary.compression = Some(parts.join(", "));

    if components > 1 && siz.components.iter().any(|c| c.dx != first.dx || c.dy != first.dy) {
        let max_dx = siz.components.iter().map(|c| c.dx).max().unwrap_or(1);
        let max_dy = siz.components.iter().map(|c| c.dy).max().unwrap_or(1);
        let factors: Vec<(u8, u8)> =
            siz.components.iter().map(|c| ((max_dx / c.dx.max(1)) as u8, (max_dy / c.dy.max(1)) as u8)).collect();
        summary.subsampling = crate::decoders::jpeg::info::subsampling_name(&factors);
    }

    Some(summary)
}

pub fn scan(data: &[u8]) -> Jpeg2000Info {
    let mut scanner = Scanner {
        sections: Vec::new(),
        icc_profile: None,
        detailed_tile_parts: 0,
        remainder: None,
        more_sections: None,
    };

    let (codestream_data, container) = if data.starts_with(&JP2_SIGNATURE) {
        scanner.walk_boxes(data, 0, 0);
        match codestream::parse_jp2(data) {
            Ok(container) => (container.codestream.clone(), Some(container)),
            Err(e) => {
                log_warn!("JPEG 2000: {}", e);
                (Vec::new(), None)
            }
        }
    } else {
        scanner.walk_codestream(data, 0);
        (data.to_vec(), None)
    };

    if let Some((offset, remainder)) = scanner.remainder.take() {
        scanner.sections.push(Section::new(offset, Some(remainder.bytes), Jpeg2000Section::MoreTileParts(remainder)));
    }
    if let Some((offset, more)) = scanner.more_sections.take() {
        scanner.sections.push(Section::new(offset, None, Jpeg2000Section::MoreSections(more)));
    }

    let mut summary = build_summary(&codestream_data, container.as_ref());
    if let Some(summary) = &mut summary {
        summary.icc_profile = scanner.icc_profile.take();
    }

    let mut sections = scanner.sections;
    sections.sort_by_key(|section| section.offset);
    Jpeg2000Info { summary, sections }
}

fn progression_name(order: u8) -> String {
    match order {
        0 => "LRCP (0)".to_string(),
        1 => "RLCP (1)".to_string(),
        2 => "RPCL (2)".to_string(),
        3 => "PCRL (3)".to_string(),
        4 => "CPRL (4)".to_string(),
        other => format!("Unknown ({})", other),
    }
}

fn rsiz_name(rsiz: u16) -> String {
    let base = match rsiz & 0x3FFF {
        0 => "No restrictions",
        1 => "Profile 0",
        2 => "Profile 1",
        3 => "2K digital cinema",
        4 => "4K digital cinema",
        5 => "Scalable 2K digital cinema",
        6 => "Scalable 4K digital cinema",
        7 => "Long-term storage",
        _ => "Other profile",
    };
    let mut text = format!("{} (0x{:04X})", base, rsiz);
    if rsiz & 0x4000 != 0 {
        text.push_str(", capabilities in CAP marker");
    }
    text
}

fn enumerated_colorspace_name(value: u32) -> String {
    let name = match value {
        0 => "Bi-level",
        1 => "YCbCr (1)",
        3 => "YCbCr (2)",
        4 => "YCbCr (3)",
        9 => "PhotoYCC",
        11 => "CMY",
        12 => "CMYK",
        13 => "YCCK",
        14 => "CIELab",
        15 => "Bi-level (2)",
        16 => "sRGB",
        17 => "Grayscale",
        18 => "sYCC",
        19 => "CIEJab",
        20 => "e-sRGB",
        21 => "ROMM-RGB",
        22 => "YPbPr (1125/60)",
        23 => "YPbPr (1250/50)",
        24 => "e-sYCC",
        _ => "Unknown",
    };
    format!("{} ({})", name, value)
}

fn code_block_flags(style: u8) -> String {
    let mut flags = Vec::new();
    for (bit, name) in [
        (0x01, "selective bypass"),
        (0x02, "context reset"),
        (0x04, "terminate each pass"),
        (0x08, "vertically causal"),
        (0x10, "predictable termination"),
        (0x20, "segmentation symbols"),
        (0x40, "HT block coding"),
        (0x80, "mixed HT"),
    ] {
        if style & bit != 0 {
            flags.push(name);
        }
    }
    if flags.is_empty() {
        format!("none (0x{:02X})", style)
    } else {
        format!("{} (0x{:02X})", flags.join(", "), style)
    }
}

fn describe_coding_style(style: &J2kCodingStyle, out: &mut InfoWriter) {
    out.field("Decomposition levels", style.decomposition_levels);
    out.field(
        "Code-block size",
        format!(
            "{}x{}",
            1u64 << (u32::from(style.code_block_width) + 2).min(63),
            1u64 << (u32::from(style.code_block_height) + 2).min(63)
        ),
    );
    out.field("Code-block style", code_block_flags(style.code_block_style));
    let transform = match style.transform {
        0 => "9/7 irreversible (0)".to_string(),
        1 => "5/3 reversible (1)".to_string(),
        other => format!("Unknown ({})", other),
    };
    out.field("Wavelet transform", transform);
    if !style.precincts.is_empty() {
        let precincts = style
            .precincts
            .iter()
            .map(|p| format!("{}x{}", 1u64 << (p & 0x0F), 1u64 << (p >> 4)))
            .collect::<Vec<_>>()
            .join(", ");
        out.field("Precincts", precincts);
    }
}

fn marker_title(marker: &J2kMarkerBody) -> Cow<'static, str> {
    Cow::Borrowed(match marker {
        J2kMarkerBody::StartOfCodestream => "SOC (Start of Codestream)",
        J2kMarkerBody::ImageSize(_) => "SIZ (Image and Tile Size)",
        J2kMarkerBody::Capabilities(_) => "CAP (Extended Capabilities)",
        J2kMarkerBody::CodingStyle(_) => "COD (Coding Style Default)",
        J2kMarkerBody::ComponentCodingStyle(_) => "COC (Coding Style Component)",
        J2kMarkerBody::Quantization(_) => "QCD (Quantization Default)",
        J2kMarkerBody::ComponentQuantization(_) => "QCC (Quantization Component)",
        J2kMarkerBody::RegionOfInterest(_) => "RGN (Region of Interest)",
        J2kMarkerBody::ProgressionOrderChange(_) => "POC (Progression Order Change)",
        J2kMarkerBody::TilePartLengths(_) => "TLM (Tile-part Lengths)",
        J2kMarkerBody::PacketLengthsMain(_) => "PLM (Packet Lengths, Main Header)",
        J2kMarkerBody::PacketLengthsTile(_) => "PLT (Packet Lengths, Tile-part Header)",
        J2kMarkerBody::PackedPacketHeadersMain(_) => "PPM (Packed Packet Headers, Main Header)",
        J2kMarkerBody::PackedPacketHeadersTile(_) => "PPT (Packed Packet Headers, Tile-part Header)",
        J2kMarkerBody::ComponentRegistration(_) => "CRG (Component Registration)",
        J2kMarkerBody::Comment(_) => "COM (Comment)",
        J2kMarkerBody::EndOfCodestream => "EOC (End of Codestream)",
        J2kMarkerBody::Unknown(_) => "Unknown Marker",
    })
}

fn describe_marker(marker: &J2kMarkerBody, out: &mut InfoWriter) {
    match marker {
        J2kMarkerBody::ImageSize(siz) => {
            out.field("Profile", rsiz_name(siz.rsiz));
            out.field("Reference grid", format!("{}x{}", siz.xsiz, siz.ysiz));
            out.field("Image offset", format!("{}, {}", siz.x0siz, siz.y0siz));
            out.field(
                "Image size",
                format!("{}x{}", siz.xsiz.saturating_sub(siz.x0siz), siz.ysiz.saturating_sub(siz.y0siz)),
            );
            out.field("Tile size", format!("{}x{}", siz.xtsiz, siz.ytsiz));
            out.field("Tile offset", format!("{}, {}", siz.xt0siz, siz.yt0siz));
            if usize::from(siz.declared_components) != siz.components.len() {
                out.field("Components", format!("{} of {} declared", siz.components.len(), siz.declared_components));
            } else {
                out.field("Components", siz.components.len());
            }
            for (index, component) in siz.components.iter().enumerate() {
                let signed = if component.ssiz & 0x80 != 0 { "signed" } else { "unsigned" };
                out.field(
                    &format!("Component {}", index),
                    format!(
                        "{} bits {}, subsampling {}x{}",
                        (component.ssiz & 0x7F) + 1,
                        signed,
                        component.dx,
                        component.dy
                    ),
                );
            }
        }
        J2kMarkerBody::Capabilities(values) => {
            if let Some(pcap) = values.first() {
                out.field("Pcap", format!("0x{:08X}", pcap));
                if pcap & (1 << (32 - 15)) != 0 {
                    out.field("Part 15 (HTJ2K)", "yes");
                }
            }
            for (index, ccap) in values.iter().skip(1).enumerate() {
                out.field(&format!("Ccap {}", index + 1), format!("0x{:04X}", ccap));
            }
        }
        J2kMarkerBody::CodingStyle(cod) => {
            out.field("Precinct partition", if cod.scod & 1 != 0 { "user-defined" } else { "maximal" });
            out.field("SOP markers", if cod.scod & 2 != 0 { "yes" } else { "no" });
            out.field("EPH markers", if cod.scod & 4 != 0 { "yes" } else { "no" });
            out.field("Progression order", progression_name(cod.progression));
            out.field("Quality layers", cod.layers);
            let mct = match cod.mct {
                0 => "None (0)".to_string(),
                1 => "Component transform (1)".to_string(),
                other => format!("Part 2 transform ({})", other),
            };
            out.field("Multiple component transform", mct);
            if let Some(style) = &cod.style {
                describe_coding_style(style, out);
            }
        }
        J2kMarkerBody::ComponentCodingStyle(coc) => {
            out.field("Component", coc.component);
            if let Some(style) = &coc.style {
                describe_coding_style(style, out);
            }
        }
        J2kMarkerBody::Quantization(quant) | J2kMarkerBody::ComponentQuantization(quant) => {
            out.opt_field("Component", quant.component);
            let style = match quant.sqcd & 0x1F {
                0 => "No quantization (0)".to_string(),
                1 => "Scalar derived (1)".to_string(),
                2 => "Scalar expounded (2)".to_string(),
                other => format!("Unknown ({})", other),
            };
            out.field("Style", style);
            out.field("Guard bits", quant.sqcd >> 5);
            out.field("Step sizes", quant.step_sizes);
        }
        J2kMarkerBody::RegionOfInterest(region) => {
            out.field("Component", region.component);
            out.field(
                "Style",
                if region.style == 0 {
                    "Implicit (max shift)".to_string()
                } else {
                    format!("Unknown ({})", region.style)
                },
            );
            out.field("Shift", region.shift);
        }
        J2kMarkerBody::ProgressionOrderChange(changes) => {
            for (index, change) in changes.iter().enumerate() {
                out.field(
                    &format!("Change {}", index + 1),
                    format!(
                        "{}, resolutions {}..{}, components {}..{}, layers ..{}",
                        progression_name(change.order),
                        change.resolution_start,
                        change.resolution_end,
                        change.component_start,
                        change.component_end,
                        change.layer_end
                    ),
                );
            }
        }
        J2kMarkerBody::TilePartLengths(packet)
        | J2kMarkerBody::PacketLengthsMain(packet)
        | J2kMarkerBody::PacketLengthsTile(packet)
        | J2kMarkerBody::PackedPacketHeadersMain(packet)
        | J2kMarkerBody::PackedPacketHeadersTile(packet) => {
            out.field("Index", packet.index);
            out.field("Data", format!("{} bytes", packet.data_length));
        }
        J2kMarkerBody::ComponentRegistration(offsets) => {
            for (index, [x, y]) in offsets.iter().enumerate() {
                out.field(&format!("Component {}", index), format!("offset {}/65536, {}/65536", x, y));
            }
        }
        J2kMarkerBody::Comment(comment) => {
            let registration = match comment.registration {
                0 => "Binary (0)".to_string(),
                1 => "Latin-1 text (1)".to_string(),
                other => format!("Unknown ({})", other),
            };
            out.field("Registration", registration);
            match &comment.text {
                Some(text) => out.text("Text", text),
                None => out.field("Data", format!("{} bytes", comment.data_length)),
            };
        }
        J2kMarkerBody::Unknown(code) => {
            out.field("Marker", format!("0x{:04X}", code));
        }
        J2kMarkerBody::StartOfCodestream | J2kMarkerBody::EndOfCodestream => {}
    }
}

impl InfoSection for J2kMarkerBody {
    fn title(&self) -> Cow<'_, str> {
        marker_title(self)
    }

    fn describe(&self, out: &mut InfoWriter) {
        describe_marker(self, out)
    }
}

impl InfoSection for Jpeg2000Section {
    fn title(&self) -> Cow<'_, str> {
        match self {
            Jpeg2000Section::Box(b) => Cow::Owned(match b.box_type.as_str() {
                "jP  " => "Signature Box ('jP  ')".to_string(),
                "jp2h" => "JP2 Header Box ('jp2h')".to_string(),
                "jp2c" => "Contiguous Codestream Box ('jp2c')".to_string(),
                "res " => "Resolution Box ('res ')".to_string(),
                "jp2i" => "Intellectual Property Box ('jp2i')".to_string(),
                other => format!("Box '{}'", other),
            }),
            Jpeg2000Section::FileType(_) => Cow::Borrowed("File Type Box ('ftyp')"),
            Jpeg2000Section::ImageHeader(_) => Cow::Borrowed("Image Header Box ('ihdr')"),
            Jpeg2000Section::ColorSpecification(_) => Cow::Borrowed("Color Specification Box ('colr')"),
            Jpeg2000Section::Palette(_) => Cow::Borrowed("Palette Box ('pclr')"),
            Jpeg2000Section::ComponentMapping(_) => Cow::Borrowed("Component Mapping Box ('cmap')"),
            Jpeg2000Section::ChannelDefinition(_) => Cow::Borrowed("Channel Definition Box ('cdef')"),
            Jpeg2000Section::BitsPerComponent(_) => Cow::Borrowed("Bits Per Component Box ('bpcc')"),
            Jpeg2000Section::Resolution(res) => Cow::Borrowed(if res.display {
                "Display Resolution Box ('resd')"
            } else {
                "Capture Resolution Box ('resc')"
            }),
            Jpeg2000Section::Xml(_) => Cow::Borrowed("XML Box ('xml ')"),
            Jpeg2000Section::Uuid(_) => Cow::Borrowed("UUID Box ('uuid')"),
            Jpeg2000Section::Marker(marker) => marker_title(marker),
            Jpeg2000Section::TilePart(part) => Cow::Owned(format!(
                "Tile-part (tile {}, part {} of {})",
                part.tile,
                u16::from(part.part) + 1,
                part.parts
            )),
            Jpeg2000Section::MoreTileParts(_) => Cow::Borrowed("Further Tile-parts"),
            Jpeg2000Section::MoreSections(_) => Cow::Borrowed("Further Boxes and Markers"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            Jpeg2000Section::Box(b) => {
                if b.superbox {
                    out.field("Superbox", "yes");
                }
            }
            Jpeg2000Section::FileType(ftyp) => {
                out.field("Brand", &ftyp.brand);
                out.field("Minor version", ftyp.minor_version);
                out.field("Compatibility", ftyp.compatibility.join(", "));
            }
            Jpeg2000Section::ImageHeader(ihdr) => {
                out.field("Dimensions", format!("{}x{}", ihdr.width, ihdr.height));
                out.field("Components", ihdr.components);
                if ihdr.bits_per_component == 0xFF {
                    out.field("Bits per component", "varies (see 'bpcc')");
                } else {
                    let signed = if ihdr.bits_per_component & 0x80 != 0 { "signed" } else { "unsigned" };
                    out.field("Bits per component", format!("{} {}", (ihdr.bits_per_component & 0x7F) + 1, signed));
                }
                out.field(
                    "Compression type",
                    if ihdr.compression == 7 {
                        "Wavelet (7)".to_string()
                    } else {
                        format!("Unknown ({})", ihdr.compression)
                    },
                );
                out.field("Colorspace unknown", if ihdr.unknown_colorspace != 0 { "yes" } else { "no" });
                out.field("Intellectual property", if ihdr.intellectual_property != 0 { "yes" } else { "no" });
            }
            Jpeg2000Section::ColorSpecification(colr) => {
                let method = match colr.method {
                    1 => "Enumerated (1)".to_string(),
                    2 => "Restricted ICC (2)".to_string(),
                    3 => "Any ICC (3)".to_string(),
                    4 => "Vendor (4)".to_string(),
                    other => format!("Unknown ({})", other),
                };
                out.field("Method", method);
                out.field("Precedence", colr.precedence);
                out.field("Approximation", colr.approximation);
                out.opt_field("Colorspace", colr.enumerated.map(enumerated_colorspace_name));
                out.opt_field("ICC profile", colr.icc_length.map(|n| format!("{} bytes", n)));
            }
            Jpeg2000Section::Palette(palette) => {
                out.field("Entries", palette.entries);
                out.field("Columns", palette.columns);
                let depths = palette
                    .depths
                    .iter()
                    .map(|d| format!("{}{}", (d & 0x7F) + 1, if d & 0x80 != 0 { "s" } else { "" }))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.field("Column depths", depths);
            }
            Jpeg2000Section::ComponentMapping(mappings) => {
                for (index, mapping) in mappings.iter().enumerate() {
                    let value = match mapping.mapping_type {
                        0 => format!("component {} direct", mapping.component),
                        1 => format!("component {} via palette column {}", mapping.component, mapping.palette_column),
                        other => format!("component {}, unknown mapping {}", mapping.component, other),
                    };
                    out.field(&format!("Channel {}", index), value);
                }
            }
            Jpeg2000Section::ChannelDefinition(definitions) => {
                for definition in definitions {
                    let kind = match definition.channel_type {
                        0 => "color".to_string(),
                        1 => "opacity".to_string(),
                        2 => "premultiplied opacity".to_string(),
                        0xFFFF => "unspecified".to_string(),
                        other => format!("type {}", other),
                    };
                    let association = match definition.association {
                        0 => "whole image".to_string(),
                        0xFFFF => "none".to_string(),
                        n => format!("color {}", n),
                    };
                    out.field(&format!("Channel {}", definition.channel), format!("{}, {}", kind, association));
                }
            }
            Jpeg2000Section::BitsPerComponent(depths) => {
                let depths = depths
                    .iter()
                    .map(|d| format!("{}{}", (d & 0x7F) + 1, if d & 0x80 != 0 { "s" } else { "" }))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.field("Bits", depths);
            }
            Jpeg2000Section::Resolution(res) => {
                out.field("Vertical", format!("{:.2} per meter ({:.2} DPI)", res.vertical, res.vertical * 0.0254));
                out.field(
                    "Horizontal",
                    format!("{:.2} per meter ({:.2} DPI)", res.horizontal, res.horizontal * 0.0254),
                );
            }
            Jpeg2000Section::Xml(text) => {
                out.text("Content", text);
            }
            Jpeg2000Section::Uuid(uuid) => {
                out.field("UUID", uuid);
            }
            Jpeg2000Section::Marker(marker) => describe_marker(marker, out),
            Jpeg2000Section::TilePart(part) => {
                if part.declared_length == 0 {
                    out.field("Declared length", "0 (until end of codestream)");
                } else if part.truncated {
                    out.field("Declared length", format!("{} bytes (truncated)", part.declared_length));
                }
                out.field("Tile data", format!("{} bytes", part.data_length));
                for marker in &part.header_markers {
                    out.section(marker.data.title(), marker.offset, marker.length, |out| marker.data.describe(out));
                }
                if part.more_header_markers > 0 {
                    out.field("Further header markers", part.more_header_markers);
                }
            }
            Jpeg2000Section::MoreTileParts(remainder) => {
                out.field("Tile-parts", remainder.tile_parts);
                out.field("Data", format!("{} bytes", remainder.bytes));
            }
            Jpeg2000Section::MoreSections(more) => {
                out.field("Sections", more.sections);
                out.field("Data", format!("{} bytes", more.bytes));
            }
        }
    }
}
