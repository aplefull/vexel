use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::log_warn;
use crate::utils::exif::{ExifData, ExifReader};
use crate::utils::icc::ICCProfile;
use crate::utils::info::InfoWriter;
use crate::utils::types::ByteOrder;

const XMP_IDENTIFIER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const EXTENDED_XMP_IDENTIFIER: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const ICC_IDENTIFIER: &[u8] = b"ICC_PROFILE\0";
const PHOTOSHOP_IDENTIFIER: &[u8] = b"Photoshop 3.0\0";
const MAX_IDENTIFIER_LENGTH: usize = 32;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JfifSegment {
    pub version_major: u8,
    pub version_minor: u8,
    pub density_units: u8,
    pub x_density: u16,
    pub y_density: u16,
    pub thumbnail_width: u8,
    pub thumbnail_height: u8,
    pub thumbnail_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JfxxSegment {
    pub extension_code: u8,
    pub thumbnail_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ExifSegment {
    pub tiff_length: u32,
    pub exif: Option<ExifData>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct XmpSegment {
    pub packet: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExtendedXmpSegment {
    pub guid: String,
    pub full_length: u32,
    pub chunk_offset: u32,
    pub chunk_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IccChunk {
    pub sequence: u8,
    pub total: u8,
    pub data_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct MpfEntry {
    pub attribute: u32,
    pub size: u32,
    pub offset: u32,
    pub absolute_offset: Option<u64>,
    pub dependent_images: [u16; 2],
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct MpfSegment {
    pub byte_order: Option<ByteOrder>,
    pub version: Option<String>,
    pub image_count: Option<u32>,
    pub entries: Vec<MpfEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AdobeSegment {
    pub version: u16,
    pub flags0: u16,
    pub flags1: u16,
    pub color_transform: u8,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PhotoshopResource {
    pub signature: String,
    pub id: u16,
    pub name: String,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PhotoshopSegment {
    pub resources: Vec<PhotoshopResource>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SpiffSegment {
    pub version_major: u8,
    pub version_minor: u8,
    pub profile_id: u8,
    pub component_count: u8,
    pub height: u32,
    pub width: u32,
    pub color_space: u8,
    pub bits_per_sample: u8,
    pub compression_type: u8,
    pub resolution_units: u8,
    pub vertical_resolution: u32,
    pub horizontal_resolution: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HpTransformSegment {
    pub transform: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JpegXtSegment {
    pub instance: u16,
    pub sequence: u32,
    pub box_length: u32,
    pub box_type: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct GenericAppSegment {
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum AppSegment {
    Jfif(JfifSegment),
    Jfxx(JfxxSegment),
    Exif(ExifSegment),
    Xmp(XmpSegment),
    ExtendedXmp(ExtendedXmpSegment),
    Icc(IccChunk),
    Mpf(MpfSegment),
    Adobe(AdobeSegment),
    Photoshop(PhotoshopSegment),
    Spiff(SpiffSegment),
    HpTransform(HpTransformSegment),
    JpegXt(JpegXtSegment),
    Generic(GenericAppSegment),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AppSection {
    pub index: u8,
    pub segment: AppSegment,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CommentSegment {
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct IccAssembler {
    chunks: Vec<(u8, Vec<u8>)>,
}

impl IccAssembler {
    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn assemble(&self) -> Option<ICCProfile> {
        if self.chunks.is_empty() {
            return None;
        }

        let mut chunks: Vec<&(u8, Vec<u8>)> = self.chunks.iter().collect();
        chunks.sort_by_key(|(sequence, _)| *sequence);

        let profile_data: Vec<u8> = chunks.iter().flat_map(|(_, data)| data.iter().copied()).collect();

        match ICCProfile::new(&profile_data) {
            Ok(profile) => Some(profile),
            Err(e) => {
                log_warn!("Failed to parse ICC profile from APP2 segments: {}", e);
                None
            }
        }
    }
}

struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    fn u16_be(&self, at: usize) -> u16 {
        u16::from_be_bytes([self.u8(at), self.u8(at.saturating_add(1))])
    }

    fn u32_be(&self, at: usize) -> u32 {
        u32::from_be_bytes([
            self.u8(at),
            self.u8(at.saturating_add(1)),
            self.u8(at.saturating_add(2)),
            self.u8(at.saturating_add(3)),
        ])
    }

    fn u16(&self, at: usize, order: ByteOrder) -> u16 {
        match order {
            ByteOrder::BigEndian => self.u16_be(at),
            ByteOrder::LittleEndian => u16::from_le_bytes([self.u8(at), self.u8(at.saturating_add(1))]),
        }
    }

    fn u32(&self, at: usize, order: ByteOrder) -> u32 {
        match order {
            ByteOrder::BigEndian => self.u32_be(at),
            ByteOrder::LittleEndian => u32::from_le_bytes([
                self.u8(at),
                self.u8(at.saturating_add(1)),
                self.u8(at.saturating_add(2)),
                self.u8(at.saturating_add(3)),
            ]),
        }
    }
}

fn printable_ascii(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }).collect()
}

fn generic_identifier(payload: &[u8]) -> Option<String> {
    let window = &payload[..payload.len().min(MAX_IDENTIFIER_LENGTH)];
    let end = window.iter().position(|&b| b == 0).unwrap_or(window.len());
    let identifier = &window[..end];
    if identifier.is_empty() || !identifier.iter().all(|&b| b.is_ascii_graphic() || b == b' ') {
        return None;
    }
    Some(String::from_utf8_lossy(identifier).into_owned())
}

pub fn parse_comment(payload: &[u8]) -> CommentSegment {
    CommentSegment { text: String::from_utf8_lossy(payload).trim_end_matches('\0').to_string() }
}

pub fn parse_app(index: u8, payload: &[u8], payload_offset: u64, icc: &mut IccAssembler) -> AppSegment {
    match index {
        0 if payload.starts_with(b"JFIF\0") => AppSegment::Jfif(parse_jfif(&payload[5..])),
        0 if payload.starts_with(b"JFXX\0") => AppSegment::Jfxx(JfxxSegment {
            extension_code: Bytes(payload).u8(5),
            thumbnail_length: payload.len().saturating_sub(6) as u32,
        }),
        1 if payload.starts_with(b"Exif\0") && payload.len() >= 6 => {
            let tiff = &payload[6..];
            AppSegment::Exif(ExifSegment { tiff_length: tiff.len() as u32, exif: ExifReader::parse(tiff) })
        }
        1 if payload.starts_with(XMP_IDENTIFIER) => AppSegment::Xmp(XmpSegment {
            packet: String::from_utf8_lossy(&payload[XMP_IDENTIFIER.len()..]).trim_end_matches('\0').to_string(),
        }),
        1 if payload.starts_with(EXTENDED_XMP_IDENTIFIER) => {
            let rest = &payload[EXTENDED_XMP_IDENTIFIER.len()..];
            let bytes = Bytes(rest);
            AppSegment::ExtendedXmp(ExtendedXmpSegment {
                guid: printable_ascii(&rest[..rest.len().min(32)]),
                full_length: bytes.u32_be(32),
                chunk_offset: bytes.u32_be(36),
                chunk_length: rest.len().saturating_sub(40) as u32,
            })
        }
        2 if payload.starts_with(ICC_IDENTIFIER) && payload.len() >= ICC_IDENTIFIER.len() + 2 => {
            let header = ICC_IDENTIFIER.len();
            let sequence = payload[header];
            let total = payload[header + 1];
            let data = &payload[header + 2..];
            icc.chunks.push((sequence, data.to_vec()));
            AppSegment::Icc(IccChunk { sequence, total, data_length: data.len() as u32 })
        }
        2 if payload.starts_with(b"MPF\0") => AppSegment::Mpf(parse_mpf(&payload[4..], payload_offset + 4)),
        8 if payload.starts_with(b"SPIFF\0") => AppSegment::Spiff(parse_spiff(&payload[6..])),
        8 if payload.starts_with(b"mrfx") && payload.len() >= 5 => {
            AppSegment::HpTransform(HpTransformSegment { transform: payload[4] })
        }
        11 if payload.starts_with(b"JP") && payload.len() >= 16 => {
            let bytes = Bytes(payload);
            AppSegment::JpegXt(JpegXtSegment {
                instance: bytes.u16_be(2),
                sequence: bytes.u32_be(4),
                box_length: bytes.u32_be(8),
                box_type: printable_ascii(&payload[12..16]),
            })
        }
        13 if payload.starts_with(PHOTOSHOP_IDENTIFIER) => {
            AppSegment::Photoshop(parse_photoshop(&payload[PHOTOSHOP_IDENTIFIER.len()..]))
        }
        14 if payload.starts_with(b"Adobe") && payload.len() >= 10 => {
            let bytes = Bytes(payload);
            AppSegment::Adobe(AdobeSegment {
                version: bytes.u16_be(5),
                flags0: bytes.u16_be(7),
                flags1: bytes.u16_be(9),
                color_transform: bytes.u8(11),
                truncated: payload.len() < 12,
            })
        }
        _ => AppSegment::Generic(GenericAppSegment { identifier: generic_identifier(payload) }),
    }
}

fn parse_jfif(body: &[u8]) -> JfifSegment {
    let bytes = Bytes(body);
    let thumbnail_width = bytes.u8(7);
    let thumbnail_height = bytes.u8(8);
    let declared_thumbnail = usize::from(thumbnail_width) * usize::from(thumbnail_height) * 3;
    let available_thumbnail = body.len().saturating_sub(9);

    if body.len() != 9 + declared_thumbnail {
        log_warn!("Invalid JFIF segment length, expected {}, got {}", 16 + declared_thumbnail, body.len() + 7);
    }

    JfifSegment {
        version_major: bytes.u8(0),
        version_minor: bytes.u8(1),
        density_units: bytes.u8(2),
        x_density: bytes.u16_be(3),
        y_density: bytes.u16_be(5),
        thumbnail_width,
        thumbnail_height,
        thumbnail_length: declared_thumbnail.min(available_thumbnail) as u32,
    }
}

fn parse_spiff(body: &[u8]) -> SpiffSegment {
    let bytes = Bytes(body);
    SpiffSegment {
        version_major: bytes.u8(0),
        version_minor: bytes.u8(1),
        profile_id: bytes.u8(2),
        component_count: bytes.u8(3),
        height: bytes.u32_be(4),
        width: bytes.u32_be(8),
        color_space: bytes.u8(12),
        bits_per_sample: bytes.u8(13),
        compression_type: bytes.u8(14),
        resolution_units: bytes.u8(15),
        vertical_resolution: bytes.u32_be(16),
        horizontal_resolution: bytes.u32_be(20),
        truncated: body.len() < 24,
    }
}

fn parse_photoshop(body: &[u8]) -> PhotoshopSegment {
    let bytes = Bytes(body);
    let mut resources = Vec::new();
    let mut pos = 0usize;

    while body.len().saturating_sub(pos) >= 4 {
        let signature = &body[pos..pos + 4];
        if !matches!(signature, b"8BIM" | b"PHUT" | b"AgHg" | b"DCSR" | b"MeSa") {
            break;
        }
        let id = bytes.u16_be(pos + 4);
        let name_length = usize::from(bytes.u8(pos + 6));
        let name_start = (pos + 7).min(body.len());
        let name_end = (name_start + name_length).min(body.len());
        let name = String::from_utf8_lossy(&body[name_start..name_end]).into_owned();
        let size_at = pos + 6 + ((1 + name_length + 1) & !1);
        let size = bytes.u32_be(size_at);
        resources.push(PhotoshopResource { signature: printable_ascii(signature), id, name, size });

        let data_start = size_at + 4;
        let padded = (size as usize).saturating_add(size as usize & 1);
        pos = data_start.saturating_add(padded);
    }

    PhotoshopSegment { resources }
}

fn parse_mpf(tiff: &[u8], tiff_offset: u64) -> MpfSegment {
    let mut segment = MpfSegment { byte_order: None, version: None, image_count: None, entries: Vec::new() };

    let order = match tiff.get(..2) {
        Some(b"II") => ByteOrder::LittleEndian,
        Some(b"MM") => ByteOrder::BigEndian,
        _ => return segment,
    };
    segment.byte_order = Some(order);

    let bytes = Bytes(tiff);
    let ifd = bytes.u32(4, order) as usize;
    let entry_count = usize::from(bytes.u16(ifd, order));
    let mut entries_at = None;
    let mut entries_length = 0usize;

    for index in 0..entry_count {
        let at = ifd.saturating_add(2 + index * 12);
        if at.saturating_add(12) > tiff.len() {
            break;
        }
        let tag = bytes.u16(at, order);
        let count = bytes.u32(at + 4, order) as usize;
        match tag {
            0xB000 => segment.version = Some(printable_ascii(&tiff[at + 8..at + 12])),
            0xB001 => segment.image_count = Some(bytes.u32(at + 8, order)),
            0xB002 => {
                entries_length = count;
                entries_at = Some(if count <= 4 { at + 8 } else { bytes.u32(at + 8, order) as usize });
            }
            _ => {}
        }
    }

    if let Some(start) = entries_at {
        let end = start.saturating_add(entries_length).min(tiff.len());
        let mut at = start;
        while at.saturating_add(16) <= end {
            let offset = bytes.u32(at + 8, order);
            segment.entries.push(MpfEntry {
                attribute: bytes.u32(at, order),
                size: bytes.u32(at + 4, order),
                offset,
                absolute_offset: (offset != 0).then(|| tiff_offset + u64::from(offset)),
                dependent_images: [bytes.u16(at + 12, order), bytes.u16(at + 14, order)],
            });
            at += 16;
        }
    }

    segment
}

fn density_units_name(units: u8) -> String {
    match units {
        0 => "Aspect ratio only (0)".to_string(),
        1 => "Dots per inch (1)".to_string(),
        2 => "Dots per centimeter (2)".to_string(),
        other => format!("Unknown ({})", other),
    }
}

fn jfxx_extension_name(code: u8) -> String {
    match code {
        0x10 => "JPEG thumbnail (0x10)".to_string(),
        0x11 => "1-byte palette thumbnail (0x11)".to_string(),
        0x13 => "3-byte RGB thumbnail (0x13)".to_string(),
        other => format!("Unknown (0x{:02X})", other),
    }
}

fn adobe_transform_name(transform: u8) -> String {
    match transform {
        0 => "None, RGB or CMYK (0)".to_string(),
        1 => "YCbCr (1)".to_string(),
        2 => "YCCK (2)".to_string(),
        other => format!("Unknown ({})", other),
    }
}

fn hp_transform_name(transform: u8) -> String {
    match transform {
        0 => "None (0)".to_string(),
        1 => "HP1 (1)".to_string(),
        2 => "HP2 (2)".to_string(),
        3 => "HP3 (3)".to_string(),
        other => format!("Unknown ({})", other),
    }
}

fn mpf_type_name(attribute: u32) -> String {
    let code = attribute & 0x00FF_FFFF;
    let name = match code {
        0x030000 => "Baseline MP primary image",
        0x010001 => "Large thumbnail (VGA)",
        0x010002 => "Large thumbnail (full HD)",
        0x020001 => "Multi-frame panorama",
        0x020002 => "Multi-frame disparity",
        0x020003 => "Multi-frame multi-angle",
        0x000000 => "Undefined",
        _ => "Unknown",
    };
    format!("{} (0x{:06X})", name, code)
}

fn photoshop_resource_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x03E9 => "Print info",
        0x03ED => "Resolution info",
        0x03F3 => "Print flags",
        0x03F5 => "Color halftoning",
        0x03F8 => "Color transfer functions",
        0x0400 => "Layer state",
        0x0402 => "Layer groups",
        0x0404 => "IPTC-NAA record",
        0x0406 => "JPEG quality",
        0x0408 => "Grid and guides",
        0x0409 | 0x040C => "Thumbnail",
        0x040A => "Copyright flag",
        0x040B => "URL",
        0x040D => "Global angle",
        0x040F => "ICC profile",
        0x0411 => "ICC untagged",
        0x0414 => "Document IDs seed",
        0x0419 => "Global altitude",
        0x041A => "Slices",
        0x041E => "URL list",
        0x0421 => "Version info",
        0x0422 => "Exif data",
        0x0423 => "Exif data 3",
        0x0424 => "XMP metadata",
        0x0425 => "IPTC digest",
        0x0426 => "Print scale",
        0x0428 => "Pixel aspect ratio",
        0x0429 => "Layer comps",
        0x2710 => "Print flags info",
        _ => return None,
    })
}

fn spiff_profile_name(profile: u8) -> String {
    match profile {
        0 => "Not specified (0)".to_string(),
        1 => "Continuous-tone base (1)".to_string(),
        2 => "Continuous-tone progressive (2)".to_string(),
        3 => "Bi-level facsimile (3)".to_string(),
        4 => "Continuous-tone facsimile (4)".to_string(),
        other => format!("Reserved ({})", other),
    }
}

fn spiff_color_space_name(color_space: u8) -> String {
    let name = match color_space {
        0 => "Bi-level, black zero",
        1 => "YCbCr (ITU-R BT.709 video)",
        2 => "No color space specified",
        3 => "YCbCr (ITU-R BT.601-1 RGB)",
        4 => "YCbCr (ITU-R BT.601-1 video)",
        8 => "Grayscale",
        9 => "PhotoYCC",
        10 => "RGB",
        11 => "CMY",
        12 => "CMYK",
        13 => "YCCK",
        14 => "CIELab",
        15 => "Bi-level, white zero",
        _ => "Reserved",
    };
    format!("{} ({})", name, color_space)
}

fn spiff_compression_name(compression: u8) -> String {
    let name = match compression {
        0 => "Uncompressed",
        1 => "Modified Huffman",
        2 => "Modified READ",
        3 => "Modified Modified READ",
        4 => "JBIG",
        5 => "JPEG",
        6 => "JPEG-LS",
        _ => "Reserved",
    };
    format!("{} ({})", name, compression)
}

fn spiff_resolution_units_name(units: u8) -> String {
    match units {
        0 => "Aspect ratio (0)".to_string(),
        1 => "Dots per inch (1)".to_string(),
        2 => "Dots per centimeter (2)".to_string(),
        other => format!("Reserved ({})", other),
    }
}

impl AppSection {
    pub fn title(&self) -> String {
        format!("APP{} ({})", self.index, self.segment.label())
    }
}

impl AppSegment {
    pub fn label(&self) -> Cow<'_, str> {
        match self {
            AppSegment::Jfif(_) => Cow::Borrowed("JFIF"),
            AppSegment::Jfxx(_) => Cow::Borrowed("JFXX"),
            AppSegment::Exif(_) => Cow::Borrowed("Exif"),
            AppSegment::Xmp(_) => Cow::Borrowed("XMP"),
            AppSegment::ExtendedXmp(_) => Cow::Borrowed("Extended XMP"),
            AppSegment::Icc(chunk) => Cow::Owned(format!("ICC Profile {}/{}", chunk.sequence, chunk.total)),
            AppSegment::Mpf(_) => Cow::Borrowed("Multi-Picture Format"),
            AppSegment::Adobe(_) => Cow::Borrowed("Adobe"),
            AppSegment::Photoshop(_) => Cow::Borrowed("Photoshop"),
            AppSegment::Spiff(_) => Cow::Borrowed("SPIFF"),
            AppSegment::HpTransform(_) => Cow::Borrowed("HP Color Transform"),
            AppSegment::JpegXt(_) => Cow::Borrowed("JPEG XT"),
            AppSegment::Generic(generic) => match &generic.identifier {
                Some(identifier) => Cow::Owned(identifier.clone()),
                None => Cow::Borrowed("Application Data"),
            },
        }
    }

    pub fn describe(&self, out: &mut InfoWriter) {
        match self {
            AppSegment::Jfif(jfif) => {
                out.field("Version", format!("{}.{:02}", jfif.version_major, jfif.version_minor));
                out.field("Density units", density_units_name(jfif.density_units));
                out.field("Density", format!("{}x{}", jfif.x_density, jfif.y_density));
                if jfif.thumbnail_width > 0 || jfif.thumbnail_height > 0 {
                    out.field("Thumbnail", format!("{}x{}", jfif.thumbnail_width, jfif.thumbnail_height));
                    let declared = u32::from(jfif.thumbnail_width) * u32::from(jfif.thumbnail_height) * 3;
                    if jfif.thumbnail_length < declared {
                        out.field(
                            "Thumbnail data",
                            format!("{} of {} bytes (truncated)", jfif.thumbnail_length, declared),
                        );
                    } else {
                        out.field("Thumbnail data", format!("{} bytes", jfif.thumbnail_length));
                    }
                }
            }
            AppSegment::Jfxx(jfxx) => {
                out.field("Extension", jfxx_extension_name(jfxx.extension_code));
                out.field("Thumbnail data", format!("{} bytes", jfxx.thumbnail_length));
            }
            AppSegment::Exif(segment) => {
                out.field("TIFF data", format!("{} bytes", segment.tiff_length));
                match &segment.exif {
                    Some(exif) => exif.describe(out),
                    None => {
                        out.field("Parsed", "no");
                    }
                }
            }
            AppSegment::Xmp(xmp) => {
                out.field("Packet", format!("{} bytes", xmp.packet.len()));
                out.text("Content", &xmp.packet);
            }
            AppSegment::ExtendedXmp(xmp) => {
                out.field("GUID", &xmp.guid);
                out.field("Full length", format!("{} bytes", xmp.full_length));
                out.field("Chunk offset", xmp.chunk_offset);
                out.field("Chunk length", format!("{} bytes", xmp.chunk_length));
            }
            AppSegment::Icc(chunk) => {
                out.field("Chunk", format!("{} of {}", chunk.sequence, chunk.total));
                out.field("Profile data", format!("{} bytes", chunk.data_length));
            }
            AppSegment::Mpf(mpf) => {
                match mpf.byte_order {
                    Some(ByteOrder::LittleEndian) => out.field("Byte order", "Little-endian (II)"),
                    Some(ByteOrder::BigEndian) => out.field("Byte order", "Big-endian (MM)"),
                    None => out.field("Byte order", "invalid"),
                };
                out.opt_field("Version", mpf.version.as_deref());
                out.opt_field("Images", mpf.image_count);
                for (index, entry) in mpf.entries.iter().enumerate() {
                    out.group(format!("Image {}", index + 1), |out| {
                        out.field("Type", mpf_type_name(entry.attribute));
                        let format = (entry.attribute >> 24) & 0x7;
                        out.field(
                            "Format",
                            if format == 0 { "JPEG".to_string() } else { format!("Unknown ({})", format) },
                        );
                        let mut flags = Vec::new();
                        if entry.attribute & 0x8000_0000 != 0 {
                            flags.push("dependent parent");
                        }
                        if entry.attribute & 0x4000_0000 != 0 {
                            flags.push("dependent child");
                        }
                        if entry.attribute & 0x2000_0000 != 0 {
                            flags.push("representative");
                        }
                        if !flags.is_empty() {
                            out.field("Flags", flags.join(", "));
                        }
                        out.field("Size", format!("{} bytes", entry.size));
                        match entry.absolute_offset {
                            Some(offset) => out.field("Offset", format!("0x{:08X}", offset)),
                            None => out.field("Offset", "primary image"),
                        };
                        if entry.dependent_images != [0, 0] {
                            out.field(
                                "Dependent images",
                                format!("{}, {}", entry.dependent_images[0], entry.dependent_images[1]),
                            );
                        }
                    });
                }
            }
            AppSegment::Adobe(adobe) => {
                out.field("Version", adobe.version);
                out.field("Flags0", format!("0x{:04X}", adobe.flags0));
                out.field("Flags1", format!("0x{:04X}", adobe.flags1));
                if adobe.truncated {
                    out.field("Color transform", format!("{} (truncated segment)", adobe_transform_name(0)));
                } else {
                    out.field("Color transform", adobe_transform_name(adobe.color_transform));
                }
            }
            AppSegment::Photoshop(photoshop) => {
                out.field("Resources", photoshop.resources.len());
                for resource in &photoshop.resources {
                    let name = match photoshop_resource_name(resource.id) {
                        Some(name) => format!("{} (0x{:04X})", name, resource.id),
                        None => format!("0x{:04X}", resource.id),
                    };
                    let mut value = format!("{} bytes", resource.size);
                    if !resource.name.is_empty() {
                        value = format!("{}, \"{}\"", value, resource.name);
                    }
                    if resource.signature != "8BIM" {
                        value = format!("{}, signature {}", value, resource.signature);
                    }
                    out.field(&name, value);
                }
            }
            AppSegment::Spiff(spiff) => {
                out.field("Version", format!("{}.{:02}", spiff.version_major, spiff.version_minor));
                out.field("Profile", spiff_profile_name(spiff.profile_id));
                out.field("Components", spiff.component_count);
                out.field("Dimensions", format!("{}x{}", spiff.width, spiff.height));
                out.field("Color space", spiff_color_space_name(spiff.color_space));
                out.field("Bits per sample", spiff.bits_per_sample);
                out.field("Compression", spiff_compression_name(spiff.compression_type));
                out.field("Resolution units", spiff_resolution_units_name(spiff.resolution_units));
                out.field("Resolution", format!("{}x{}", spiff.horizontal_resolution, spiff.vertical_resolution));
                if spiff.truncated {
                    out.field("Truncated", "yes");
                }
            }
            AppSegment::HpTransform(hp) => {
                out.field("Transform", hp_transform_name(hp.transform));
            }
            AppSegment::JpegXt(xt) => {
                out.field("Box instance", xt.instance);
                out.field("Packet sequence", xt.sequence);
                out.field("Box type", &xt.box_type);
                out.field("Box length", format!("{} bytes", xt.box_length));
            }
            AppSegment::Generic(generic) => {
                out.opt_field("Identifier", generic.identifier.as_deref());
            }
        }
    }
}

impl CommentSegment {
    pub fn describe(&self, out: &mut InfoWriter) {
        out.text("Text", &self.text);
    }
}
