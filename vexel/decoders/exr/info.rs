use std::borrow::Cow;
use std::fmt;

use serde::Serialize;
use tsify::Tsify;

use super::decoder::select_channels;
use super::parser::{self, fixed_attr_size, scan_chlist_size, ByteCursor, ExrFile};
use super::types::{
    ExrCompression, ExrPixelType, PartHeader, PartKind, EXR_MAGIC, VERSION_FLAG_DEEP, VERSION_FLAG_MULTIPART,
    VERSION_FLAG_TILED,
};
use crate::decoders::jpeg::info::subsampling_name;
use crate::utils::info::{ColorModel, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat};
use crate::utils::tiff::color::half_to_f32;

const VERSION_FLAG_LONG_NAMES: u32 = 0x400;
const KNOWN_VERSION_FLAGS: u32 =
    VERSION_FLAG_TILED as u32 | VERSION_FLAG_LONG_NAMES | VERSION_FLAG_DEEP as u32 | VERSION_FLAG_MULTIPART as u32;
const MAX_NAME_LENGTH: usize = 256;
const MAX_ATTRIBUTES: usize = 4096;
const MAX_STRING_VECTOR: usize = 256;
const MAX_FLOAT_VECTOR: usize = 256;
const MAX_PARTS: usize = 512;

#[tsify::declare]
pub type ExrInfo = FormatInfo<ExrSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrVersion {
    pub version: u8,
    pub flags: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrChannelEntry {
    pub name: String,
    pub pixel_type: i32,
    pub p_linear: u8,
    pub reserved: [u8; 3],
    pub x_sampling: i32,
    pub y_sampling: i32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum ExrAttributeValue {
    Int(i32),
    Float(f32),
    Half(f32),
    Double(f64),
    Text(String),
    StringVector(Vec<String>),
    FloatVector(Vec<f32>),
    IntVector(Vec<i32>),
    FloatVectorD(Vec<f64>),
    Box2i([i32; 4]),
    Box2f([f32; 4]),
    Channels(Vec<ExrChannelEntry>),
    Compression(u8),
    LineOrder(u8),
    Envmap(u8),
    DeepImageState(u8),
    TileDesc { x_size: u32, y_size: u32, mode: u8 },
    Chromaticities([f32; 8]),
    Rational { numerator: i32, denominator: u32 },
    TimeCode { time_and_flags: u32, user_data: u32 },
    KeyCode([i32; 7]),
    Matrix { size: u8, values: Vec<f64> },
    Preview { width: u32, height: u32 },
    Opaque,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ExrAttribute {
    pub offset: u64,
    pub name: String,
    pub type_name: String,
    pub declared_size: i32,
    pub recovered_size: Option<u32>,
    pub value: ExrAttributeValue,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrPartHeader {
    pub part: u32,
    pub attributes: Vec<ExrAttribute>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ExrOffsetTable {
    pub part: u32,
    pub entries: u32,
    pub declared_chunks: Option<i32>,
    pub computed_chunks: u64,
    pub invalid_entries: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrChunkData {
    pub part: u32,
    pub chunks: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum ExrSection {
    Version(ExrVersion),
    PartHeader(Box<ExrPartHeader>),
    EndOfHeaders,
    OffsetTable(ExrOffsetTable),
    ChunkData(ExrChunkData),
}

fn compression_name(code: u8) -> &'static str {
    match ExrCompression::from_u8(code) {
        ExrCompression::None => "None",
        ExrCompression::Rle => "RLE",
        ExrCompression::Zips => "ZIPS",
        ExrCompression::Zip => "ZIP",
        ExrCompression::Piz => "PIZ",
        ExrCompression::Pxr24 => "PXR24",
        ExrCompression::B44 => "B44",
        ExrCompression::B44a => "B44A",
        ExrCompression::Dwaa => "DWAA",
        ExrCompression::Dwab => "DWAB",
        ExrCompression::Htj2k256 => "HTJ2K (256 lines)",
        ExrCompression::Htj2k32 => "HTJ2K (32 lines)",
        ExrCompression::Unknown(_) => "Unknown",
    }
}

fn pixel_type_name(code: i32) -> &'static str {
    ExrPixelType::from_i32(code).map_or("unknown", |pixel_type| pixel_type.name())
}

fn join<T: fmt::Display>(values: &[T]) -> String {
    values.iter().map(|value| value.to_string()).collect::<Vec<_>>().join(", ")
}

impl fmt::Display for ExrAttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExrAttributeValue::Int(value) => write!(f, "{}", value),
            ExrAttributeValue::Float(value) | ExrAttributeValue::Half(value) => write!(f, "{}", value),
            ExrAttributeValue::Double(value) => write!(f, "{}", value),
            ExrAttributeValue::Text(value) => write!(f, "{}", value),
            ExrAttributeValue::StringVector(values) => write!(f, "[{}]", values.join(", ")),
            ExrAttributeValue::FloatVector(values) => write!(f, "({})", join(values)),
            ExrAttributeValue::IntVector(values) => write!(f, "({})", join(values)),
            ExrAttributeValue::FloatVectorD(values) => write!(f, "({})", join(values)),
            ExrAttributeValue::Box2i([x_min, y_min, x_max, y_max]) => {
                write!(f, "({}, {}) - ({}, {})", x_min, y_min, x_max, y_max)
            }
            ExrAttributeValue::Box2f([x_min, y_min, x_max, y_max]) => {
                write!(f, "({}, {}) - ({}, {})", x_min, y_min, x_max, y_max)
            }
            ExrAttributeValue::Channels(channels) => write!(f, "{} channels", channels.len()),
            ExrAttributeValue::Compression(code) => write!(f, "{} ({})", compression_name(*code), code),
            ExrAttributeValue::LineOrder(code) => {
                let name = match code {
                    0 => "increasing Y",
                    1 => "decreasing Y",
                    2 => "random Y",
                    _ => "unknown",
                };
                write!(f, "{} ({})", name, code)
            }
            ExrAttributeValue::Envmap(code) => {
                let name = match code {
                    0 => "latitude-longitude",
                    1 => "cube",
                    _ => "unknown",
                };
                write!(f, "{} ({})", name, code)
            }
            ExrAttributeValue::DeepImageState(code) => {
                let name = match code {
                    0 => "messy",
                    1 => "sorted",
                    2 => "non-overlapping",
                    3 => "tidy",
                    _ => "unknown",
                };
                write!(f, "{} ({})", name, code)
            }
            ExrAttributeValue::TileDesc { x_size, y_size, mode } => {
                let level = match mode & 0x0F {
                    0 => "one level",
                    1 => "mipmap",
                    2 => "ripmap",
                    _ => "unknown level mode",
                };
                let rounding = match mode >> 4 {
                    0 => "round down",
                    1 => "round up",
                    _ => "unknown rounding",
                };
                write!(f, "{}x{}, {}, {} (0x{:02X})", x_size, y_size, level, rounding, mode)
            }
            ExrAttributeValue::Chromaticities(values) => write!(
                f,
                "R ({}, {}), G ({}, {}), B ({}, {}), white ({}, {})",
                values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7]
            ),
            ExrAttributeValue::Rational { numerator, denominator } => write!(f, "{}/{}", numerator, denominator),
            ExrAttributeValue::TimeCode { time_and_flags, user_data } => {
                let bcd = |shift: u32, mask: u32| {
                    let value = (time_and_flags >> shift) & mask;
                    (value >> 4) * 10 + (value & 0x0F)
                };
                write!(
                    f,
                    "{:02}:{:02}:{:02}:{:02} (flags 0x{:08X}, user data 0x{:08X})",
                    bcd(24, 0x3F),
                    bcd(16, 0x7F),
                    bcd(8, 0x7F),
                    bcd(0, 0x3F),
                    time_and_flags,
                    user_data
                )
            }
            ExrAttributeValue::KeyCode(values) => write!(
                f,
                "manufacturer {}, type {}, prefix {}, count {}, perf offset {}, perfs/frame {}, perfs/count {}",
                values[0], values[1], values[2], values[3], values[4], values[5], values[6]
            ),
            ExrAttributeValue::Matrix { values, .. } => write!(f, "[{}]", join(values)),
            ExrAttributeValue::Preview { width, height } => write!(f, "{}x{} RGBA", width, height),
            ExrAttributeValue::Opaque => write!(f, "opaque"),
            ExrAttributeValue::Invalid => write!(f, "invalid"),
        }
    }
}

fn describe_attribute(attribute: &ExrAttribute, out: &mut InfoWriter) {
    let size = match attribute.recovered_size {
        Some(recovered) => format!("declared {} bytes, recovered {}", attribute.declared_size, recovered),
        None => format!("{} bytes", attribute.declared_size),
    };
    let title = format!("{} ({}, {})", attribute.name, attribute.type_name, size);
    out.section(title, attribute.offset, None, |out| match &attribute.value {
        ExrAttributeValue::Channels(channels) => {
            for channel in channels {
                let mut text = format!("{} ({})", pixel_type_name(channel.pixel_type), channel.pixel_type);
                if channel.x_sampling != 1 || channel.y_sampling != 1 {
                    text.push_str(&format!(", sampling {}x{}", channel.x_sampling, channel.y_sampling));
                }
                if channel.p_linear != 0 {
                    text.push_str(", perceptually linear");
                }
                if channel.reserved != [0; 3] {
                    text.push_str(&format!(", reserved {:02X?}", channel.reserved));
                }
                out.field(&channel.name, text);
            }
        }
        ExrAttributeValue::Text(text) => {
            out.text("Value", text);
        }
        ExrAttributeValue::Opaque => {}
        value => {
            out.field("Value", value);
        }
    });
}

impl InfoSection for ExrSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            ExrSection::Version(_) => Cow::Borrowed("Magic and Version"),
            ExrSection::PartHeader(header) => Cow::Owned(format!("Part {} Header", header.part)),
            ExrSection::EndOfHeaders => Cow::Borrowed("End of Headers"),
            ExrSection::OffsetTable(table) => Cow::Owned(format!("Part {} Offset Table", table.part)),
            ExrSection::ChunkData(chunks) => Cow::Owned(format!("Part {} Chunk Data", chunks.part)),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            ExrSection::Version(version) => {
                out.field("Version", version.version);
                let mut flags = Vec::new();
                if version.flags & VERSION_FLAG_TILED as u32 != 0 {
                    flags.push("single-part tiled".to_string());
                }
                if version.flags & VERSION_FLAG_LONG_NAMES != 0 {
                    flags.push("long names".to_string());
                }
                if version.flags & VERSION_FLAG_DEEP as u32 != 0 {
                    flags.push("non-image (deep)".to_string());
                }
                if version.flags & VERSION_FLAG_MULTIPART as u32 != 0 {
                    flags.push("multipart".to_string());
                }
                let reserved = version.flags & !KNOWN_VERSION_FLAGS;
                if reserved != 0 {
                    flags.push(format!("reserved bits 0x{:06X}", reserved));
                }
                let names = if flags.is_empty() { "none".to_string() } else { flags.join(", ") };
                out.field("Flags", format!("{} (0x{:06X})", names, version.flags));
            }
            ExrSection::PartHeader(header) => {
                out.field("Attributes", header.attributes.len());
                if !header.complete {
                    out.field("Complete", "no, header parse stopped early");
                }
                for attribute in &header.attributes {
                    describe_attribute(attribute, out);
                }
            }
            ExrSection::EndOfHeaders => {}
            ExrSection::OffsetTable(table) => {
                out.field("Entries", table.entries);
                out.opt_field("Declared chunk count", table.declared_chunks);
                out.field("Computed chunk count", table.computed_chunks);
                if table.invalid_entries > 0 {
                    out.field("Invalid offsets", table.invalid_entries);
                }
            }
            ExrSection::ChunkData(chunks) => {
                out.field("Chunks", chunks.chunks);
                out.field("Data", format!("{} bytes", chunks.bytes));
            }
        }
    }
}

fn read_value(type_name: &str, value: &[u8]) -> ExrAttributeValue {
    read_known_value(type_name, value).unwrap_or(ExrAttributeValue::Invalid)
}

fn read_known_value(type_name: &str, value: &[u8]) -> Option<ExrAttributeValue> {
    let mut cursor = ByteCursor::new(value);
    let ints = |cursor: &mut ByteCursor, count: usize| -> Option<Vec<i32>> {
        (0..count).map(|_| cursor.read_i32().ok()).collect()
    };
    let floats = |cursor: &mut ByteCursor, count: usize| -> Option<Vec<f32>> {
        (0..count).map(|_| cursor.read_f32().ok()).collect()
    };
    let doubles = |cursor: &mut ByteCursor, count: usize| -> Option<Vec<f64>> {
        (0..count).map(|_| cursor.read_u64().ok().map(f64::from_bits)).collect()
    };

    let parsed = match type_name {
        "int" => ExrAttributeValue::Int(cursor.read_i32().ok()?),
        "float" => ExrAttributeValue::Float(cursor.read_f32().ok()?),
        "half" => {
            let bytes = cursor.read_bytes(2).ok()?;
            ExrAttributeValue::Half(half_to_f32(u16::from_le_bytes([bytes[0], bytes[1]])))
        }
        "double" => ExrAttributeValue::Double(f64::from_bits(cursor.read_u64().ok()?)),
        "string" => ExrAttributeValue::Text(String::from_utf8_lossy(value).into_owned()),
        "stringvector" => {
            let mut strings = Vec::new();
            while cursor.remaining() >= 4 && strings.len() < MAX_STRING_VECTOR {
                let length = cursor.read_i32().ok()?.max(0) as usize;
                let bytes = cursor.read_bytes(length.min(cursor.remaining())).ok()?;
                strings.push(String::from_utf8_lossy(bytes).into_owned());
            }
            ExrAttributeValue::StringVector(strings)
        }
        "floatvector" => {
            let count = (value.len() / 4).min(MAX_FLOAT_VECTOR);
            ExrAttributeValue::FloatVector(floats(&mut cursor, count)?)
        }
        "v2i" => ExrAttributeValue::IntVector(ints(&mut cursor, 2)?),
        "v3i" => ExrAttributeValue::IntVector(ints(&mut cursor, 3)?),
        "v2f" => ExrAttributeValue::FloatVector(floats(&mut cursor, 2)?),
        "v3f" => ExrAttributeValue::FloatVector(floats(&mut cursor, 3)?),
        "v2d" => ExrAttributeValue::FloatVectorD(doubles(&mut cursor, 2)?),
        "v3d" => ExrAttributeValue::FloatVectorD(doubles(&mut cursor, 3)?),
        "box2i" => {
            let values = ints(&mut cursor, 4)?;
            ExrAttributeValue::Box2i([values[0], values[1], values[2], values[3]])
        }
        "box2f" => {
            let values = floats(&mut cursor, 4)?;
            ExrAttributeValue::Box2f([values[0], values[1], values[2], values[3]])
        }
        "chlist" => ExrAttributeValue::Channels(read_channels(&mut cursor)),
        "compression" => ExrAttributeValue::Compression(cursor.read_u8().ok()?),
        "lineOrder" => ExrAttributeValue::LineOrder(cursor.read_u8().ok()?),
        "envmap" => ExrAttributeValue::Envmap(cursor.read_u8().ok()?),
        "deepImageState" => ExrAttributeValue::DeepImageState(cursor.read_u8().ok()?),
        "tiledesc" => ExrAttributeValue::TileDesc {
            x_size: cursor.read_u32().ok()?,
            y_size: cursor.read_u32().ok()?,
            mode: cursor.read_u8().ok()?,
        },
        "chromaticities" => {
            let values = floats(&mut cursor, 8)?;
            let mut array = [0f32; 8];
            array.copy_from_slice(&values);
            ExrAttributeValue::Chromaticities(array)
        }
        "rational" => {
            ExrAttributeValue::Rational { numerator: cursor.read_i32().ok()?, denominator: cursor.read_u32().ok()? }
        }
        "timecode" => {
            ExrAttributeValue::TimeCode { time_and_flags: cursor.read_u32().ok()?, user_data: cursor.read_u32().ok()? }
        }
        "keycode" => {
            let values = ints(&mut cursor, 7)?;
            let mut array = [0i32; 7];
            array.copy_from_slice(&values);
            ExrAttributeValue::KeyCode(array)
        }
        "m33f" => {
            ExrAttributeValue::Matrix { size: 3, values: floats(&mut cursor, 9)?.into_iter().map(f64::from).collect() }
        }
        "m44f" => {
            ExrAttributeValue::Matrix { size: 4, values: floats(&mut cursor, 16)?.into_iter().map(f64::from).collect() }
        }
        "m33d" => ExrAttributeValue::Matrix { size: 3, values: doubles(&mut cursor, 9)? },
        "m44d" => ExrAttributeValue::Matrix { size: 4, values: doubles(&mut cursor, 16)? },
        "preview" => ExrAttributeValue::Preview { width: cursor.read_u32().ok()?, height: cursor.read_u32().ok()? },
        _ => ExrAttributeValue::Opaque,
    };
    Some(parsed)
}

fn read_channels(cursor: &mut ByteCursor) -> Vec<ExrChannelEntry> {
    let mut channels = Vec::new();
    while cursor.peek_u8().is_some_and(|byte| byte != 0) {
        let Ok(name) = cursor.read_cstring(MAX_NAME_LENGTH) else {
            break;
        };
        let (Ok(pixel_type), Ok(p_linear), Ok(reserved), Ok(x_sampling), Ok(y_sampling)) =
            (cursor.read_i32(), cursor.read_u8(), cursor.read_bytes(3), cursor.read_i32(), cursor.read_i32())
        else {
            break;
        };
        channels.push(ExrChannelEntry {
            name,
            pixel_type,
            p_linear,
            reserved: [reserved[0], reserved[1], reserved[2]],
            x_sampling,
            y_sampling,
        });
    }
    channels
}

struct HeaderWalk {
    header: ExrPartHeader,
    start: usize,
    end: usize,
}

fn walk_header(data: &[u8], start: usize, part: u32) -> HeaderWalk {
    let mut header = ExrPartHeader { part, attributes: Vec::new(), complete: false };
    let mut pos = start;

    while header.attributes.len() < MAX_ATTRIBUTES {
        let mut cursor = ByteCursor::new(&data[pos..]);
        let Ok(name) = cursor.read_cstring(MAX_NAME_LENGTH) else {
            break;
        };
        if name.is_empty() {
            header.complete = true;
            pos += 1;
            break;
        }
        let (Ok(type_name), Ok(declared_size)) = (cursor.read_cstring(MAX_NAME_LENGTH), cursor.read_i32()) else {
            break;
        };
        let value_start = pos + (data.len() - pos - cursor.remaining());
        let remaining = data.len() - value_start;

        let (size, recovered_size) = if declared_size < 0 || declared_size as usize > remaining {
            let recovered = if type_name == "chlist" {
                scan_chlist_size(&data[value_start..])
            } else {
                fixed_attr_size(&type_name).filter(|&size| size <= remaining)
            };
            match recovered {
                Some(size) => (size, Some(size as u32)),
                None => {
                    header.attributes.push(ExrAttribute {
                        offset: pos as u64,
                        name,
                        type_name,
                        declared_size,
                        recovered_size: None,
                        value: ExrAttributeValue::Invalid,
                    });
                    pos = value_start;
                    break;
                }
            }
        } else {
            (declared_size as usize, None)
        };

        let value = read_value(&type_name, &data[value_start..value_start + size]);
        header.attributes.push(ExrAttribute {
            offset: pos as u64,
            name,
            type_name,
            declared_size,
            recovered_size,
            value,
        });
        pos = value_start + size;
    }

    HeaderWalk { header, start, end: pos }
}

fn chunk_end(data: &[u8], offset: usize, multipart: bool, kind: PartKind) -> Option<usize> {
    let mut cursor = ByteCursor::new(data.get(offset..)?);
    if multipart {
        cursor.read_i32().ok()?;
    }
    let size = match kind {
        PartKind::Scanline => {
            cursor.read_i32().ok()?;
            cursor.read_i32().ok()?.max(0) as u64
        }
        PartKind::Tiled => {
            for _ in 0..4 {
                cursor.read_i32().ok()?;
            }
            cursor.read_i32().ok()?.max(0) as u64
        }
        PartKind::DeepScanline | PartKind::DeepTiled => {
            let coordinates = if kind == PartKind::DeepTiled { 4 } else { 1 };
            for _ in 0..coordinates {
                cursor.read_i32().ok()?;
            }
            let table = cursor.read_u64().ok()?;
            let packed = cursor.read_u64().ok()?;
            cursor.read_u64().ok()?;
            table.saturating_add(packed)
        }
    };
    let header_end = data.len() - cursor.remaining();
    Some((header_end as u64).saturating_add(size).min(data.len() as u64) as usize)
}

fn push_offsets(info: &mut ExrInfo, data: &[u8], file: &ExrFile, tables_start: usize, walks: &[HeaderWalk]) {
    let mut position = tables_start;
    let mut ranges = Vec::new();

    for (index, part) in file.parts.iter().enumerate() {
        let entries = part.chunk_offsets.len();
        let declared = walks.get(index).and_then(|walk| {
            walk.header.attributes.iter().find_map(|attribute| match (&attribute.name[..], &attribute.value) {
                ("chunkCount", ExrAttributeValue::Int(count)) => Some(*count),
                _ => None,
            })
        });

        let mut invalid = 0u32;
        let mut chunks = 0u32;
        let mut first = usize::MAX;
        let mut last = 0usize;
        let mut bytes = 0u64;
        for &offset in &part.chunk_offsets {
            let valid = offset >= tables_start as u64 && offset < data.len() as u64;
            let end = valid.then(|| chunk_end(data, offset as usize, file.multipart, part.header.kind)).flatten();
            match end {
                Some(end) => {
                    chunks += 1;
                    first = first.min(offset as usize);
                    last = last.max(end);
                    bytes += (end - offset as usize) as u64;
                }
                None => invalid += 1,
            }
        }

        info.push(
            position as u64,
            Some(entries as u64 * 8),
            ExrSection::OffsetTable(ExrOffsetTable {
                part: index as u32,
                entries: entries as u32,
                declared_chunks: declared,
                computed_chunks: computed_chunk_count(&part.header),
                invalid_entries: invalid,
            }),
        );
        position += entries * 8;

        if chunks > 0 {
            ranges.push((first, last, ExrChunkData { part: index as u32, chunks, bytes }));
        }
    }

    for (first, last, chunks) in ranges {
        info.push(first as u64, Some((last - first) as u64), ExrSection::ChunkData(chunks));
    }
}

fn computed_chunk_count(header: &PartHeader) -> u64 {
    parser::compute_chunk_count(header)
}

fn primary_part(file: &ExrFile) -> Option<&PartHeader> {
    file.parts.iter().find(|part| !part.header.kind.is_deep()).or_else(|| file.parts.first()).map(|part| &part.header)
}

fn build_summary(file: &ExrFile) -> Option<ImageSummary> {
    let header = primary_part(file)?;
    let width = header.data_window.width().clamp(0, u32::MAX as i64) as u32;
    let height = header.data_window.height().clamp(0, u32::MAX as i64) as u32;
    let selection = select_channels(header);
    let count = header.channels.len();

    let (color_model, used): (ColorModel, Vec<usize>) = match (selection.r, selection.g, selection.b) {
        (Some(r), Some(g), Some(b)) => (ColorModel::Rgb, vec![r, g, b]),
        _ => match (selection.y, selection.ry, selection.by) {
            (Some(y), Some(ry), Some(by)) => (ColorModel::YCbCr, vec![y, ry, by]),
            _ => match selection.y {
                Some(y) => (ColorModel::Gray, vec![y]),
                None if count == 1 => (ColorModel::Gray, vec![0]),
                None if count >= 3 => (ColorModel::Rgb, vec![0, 1, 2]),
                None => match selection.a {
                    Some(a) => (ColorModel::Gray, vec![usize::from(a == 0)]),
                    None if count > 0 => (ColorModel::Gray, vec![0]),
                    None => (ColorModel::Unknown, Vec::new()),
                },
            },
        },
    };

    let mut used = used;
    let has_alpha = selection.a.is_some() && !used.is_empty();
    if let (true, Some(a)) = (has_alpha, selection.a) {
        used.push(a);
    }
    let color_model = match (color_model, has_alpha) {
        (ColorModel::Rgb, true) => ColorModel::Rgba,
        (ColorModel::Gray, true) => ColorModel::GrayAlpha,
        (model, _) => model,
    };

    let channels: Vec<_> = used.iter().filter_map(|&index| header.channels.get(index)).collect();
    let bits = channels.iter().map(|channel| channel.pixel_type.bytes_per_element() * 8).max().unwrap_or(0) as u8;
    let all_uint = !channels.is_empty() && channels.iter().all(|channel| channel.pixel_type == ExrPixelType::Uint);

    let mut summary = ImageSummary::new(width, height, color_model, channels.len() as u16, bits);
    summary.sample_format = if all_uint { SampleFormat::Unsigned } else { SampleFormat::Float };
    summary.has_alpha = has_alpha;
    summary.compression = Some(compression_name(compression_code(header.compression)).to_string());

    let max_x = channels.iter().map(|channel| channel.x_sampling).max().unwrap_or(1);
    let max_y = channels.iter().map(|channel| channel.y_sampling).max().unwrap_or(1);
    if channels.iter().any(|channel| channel.x_sampling != 1 || channel.y_sampling != 1) {
        let factors: Vec<(u8, u8)> = channels
            .iter()
            .map(|channel| {
                (
                    (max_x / channel.x_sampling.max(1)).clamp(1, 255) as u8,
                    (max_y / channel.y_sampling.max(1)).clamp(1, 255) as u8,
                )
            })
            .collect();
        summary.subsampling = subsampling_name(&factors);
    }

    if file.parts.len() > 1 {
        summary.pages = Some(file.parts.len() as u32);
    }
    Some(summary)
}

fn compression_code(compression: ExrCompression) -> u8 {
    match compression {
        ExrCompression::None => 0,
        ExrCompression::Rle => 1,
        ExrCompression::Zips => 2,
        ExrCompression::Zip => 3,
        ExrCompression::Piz => 4,
        ExrCompression::Pxr24 => 5,
        ExrCompression::B44 => 6,
        ExrCompression::B44a => 7,
        ExrCompression::Dwaa => 8,
        ExrCompression::Dwab => 9,
        ExrCompression::Htj2k256 => 10,
        ExrCompression::Htj2k32 => 11,
        ExrCompression::Unknown(code) => code,
    }
}

pub fn scan(data: &[u8]) -> ExrInfo {
    let mut info = ExrInfo::default();
    if data.len() < 8 || data[..4] != EXR_MAGIC {
        return info;
    }

    let version = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let flags = version as u32 & 0xFFFF_FF00;
    info.push(0, Some(8), ExrSection::Version(ExrVersion { version: version as u8, flags }));
    let multipart = version & VERSION_FLAG_MULTIPART != 0;

    let mut walks = Vec::new();
    let mut pos = 8;
    loop {
        let walk = walk_header(data, pos, walks.len() as u32);
        let advanced = walk.end > pos;
        pos = walk.end;
        walks.push(walk);

        if !multipart || !advanced || walks.len() >= MAX_PARTS {
            break;
        }
        match data.get(pos) {
            Some(0) => {
                info.push(pos as u64, Some(1), ExrSection::EndOfHeaders);
                pos += 1;
                break;
            }
            Some(_) => {}
            None => break,
        }
    }

    for walk in &walks {
        info.push(
            walk.start as u64,
            Some((walk.end - walk.start) as u64),
            ExrSection::PartHeader(Box::new(walk.header.clone())),
        );
    }

    if let Ok(file) = parser::parse(data) {
        push_offsets(&mut info, data, &file, pos, &walks);
        info.summary = build_summary(&file);
    }

    info.sections.sort_by_key(|section| section.offset);
    info
}
