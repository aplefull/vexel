use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::lerc2::compute_checksum_fletcher32;
use super::reader::ByteReader;
use super::types::{ImageEncodeMode, LercDataType, LERC1_MAGIC, LERC2_MAGIC};
use super::values::read_value;
use crate::utils::error::VexelResult;
use crate::utils::info::{ColorModel, DataExtent, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat};

const LERC1_HEADER_SIZE: usize = 34;
const LERC1_PART_HEADER_SIZE: usize = 16;
const MAX_DEPTH_RANGES: usize = 64;

#[tsify::declare]
pub type LercInfo = FormatInfo<LercSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct LercDepthRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum Lerc2Encoding {
    NoValidPixels,
    Constant,
    ConstantPerDepth,
    OneSweep,
    Tiling,
    DeltaHuffman,
    Huffman,
    DeltaDeltaHuffman,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct Lerc2Blob {
    pub band: u32,
    pub version: i32,
    pub checksum: Option<u32>,
    pub computed_checksum: Option<u32>,
    pub rows: i32,
    pub columns: i32,
    pub depth: Option<i32>,
    pub valid_pixels: i32,
    pub micro_block_size: i32,
    pub blob_size: i32,
    pub data_type: i32,
    pub blobs_more: Option<i32>,
    pub pass_no_data_values: Option<bool>,
    pub is_int: Option<bool>,
    pub max_z_error: f64,
    pub z_min: f64,
    pub z_max: f64,
    pub no_data_value: Option<f64>,
    pub no_data_value_original: Option<f64>,
    pub header_size: u32,
    pub mask_offset: Option<u64>,
    pub mask_bytes: Option<i32>,
    pub depth_ranges: Vec<LercDepthRange>,
    pub encoding: Option<Lerc2Encoding>,
    pub encoding_flag: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Lerc1Header {
    pub band: u32,
    pub version: i32,
    pub image_type: i32,
    pub height: i32,
    pub width: i32,
    pub max_z_error: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum Lerc1PartKind {
    Count,
    Z,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Lerc1Part {
    pub band: u32,
    pub kind: Lerc1PartKind,
    pub tiles_vertical: i32,
    pub tiles_horizontal: i32,
    pub data_bytes: i32,
    pub max_value: f32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum LercSection {
    Lerc2Blob(Box<Lerc2Blob>),
    Lerc1Header(Lerc1Header),
    Lerc1Part(Lerc1Part),
    TrailingData,
}

fn data_type_name(code: i32) -> Option<&'static str> {
    let name = match LercDataType::from_code(code)? {
        LercDataType::Char => "Char, signed 8-bit",
        LercDataType::Byte => "Byte, unsigned 8-bit",
        LercDataType::Short => "Short, signed 16-bit",
        LercDataType::UShort => "UShort, unsigned 16-bit",
        LercDataType::Int => "Int, signed 32-bit",
        LercDataType::UInt => "UInt, unsigned 32-bit",
        LercDataType::Float => "Float, 32-bit",
        LercDataType::Double => "Double, 64-bit",
    };
    Some(name)
}

fn named(name: Option<&str>, raw: impl std::fmt::Display) -> String {
    match name {
        Some(name) => format!("{} ({})", name, raw),
        None => format!("Unknown ({})", raw),
    }
}

impl Lerc2Encoding {
    fn name(self) -> &'static str {
        match self {
            Lerc2Encoding::NoValidPixels => "None, no valid pixels",
            Lerc2Encoding::Constant => "Constant image",
            Lerc2Encoding::ConstantPerDepth => "Constant per depth",
            Lerc2Encoding::OneSweep => "One sweep, raw values",
            Lerc2Encoding::Tiling => "Tiling",
            Lerc2Encoding::DeltaHuffman => "Delta Huffman",
            Lerc2Encoding::Huffman => "Huffman",
            Lerc2Encoding::DeltaDeltaHuffman => "Delta-delta Huffman (lossless float)",
        }
    }
}

impl InfoSection for LercSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            LercSection::Lerc2Blob(blob) => Cow::Owned(format!("Lerc2 Blob (band {})", blob.band)),
            LercSection::Lerc1Header(header) => Cow::Owned(format!("CntZImage Header (band {})", header.band)),
            LercSection::Lerc1Part(part) => match part.kind {
                Lerc1PartKind::Count => Cow::Owned(format!("Count Part (band {})", part.band)),
                Lerc1PartKind::Z => Cow::Owned(format!("Z Part (band {})", part.band)),
            },
            LercSection::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            LercSection::Lerc2Blob(blob) => describe_lerc2(blob, out),
            LercSection::Lerc1Header(header) => {
                out.field("Version", header.version);
                out.field("Image type", header.image_type);
                out.field("Width", header.width);
                out.field("Height", header.height);
                out.field("Max Z error", header.max_z_error);
            }
            LercSection::Lerc1Part(part) => {
                if part.tiles_vertical == 0 && part.tiles_horizontal == 0 {
                    out.field("Tiles", "none");
                } else {
                    out.field("Tiles", format!("{}x{}", part.tiles_horizontal, part.tiles_vertical));
                }
                out.field("Data", format!("{} bytes", part.data_bytes));
                out.field("Max value", part.max_value);
            }
            LercSection::TrailingData => {}
        }
    }
}

fn describe_lerc2(blob: &Lerc2Blob, out: &mut InfoWriter) {
    out.field("Version", blob.version);
    if let Some(checksum) = blob.checksum {
        let status = match blob.computed_checksum {
            Some(computed) if computed == checksum => "valid".to_string(),
            Some(computed) => format!("mismatch, computed 0x{:08X}", computed),
            None => "not checked".to_string(),
        };
        out.field("Checksum", format!("0x{:08X} ({})", checksum, status));
    }
    out.field("Columns", blob.columns);
    out.field("Rows", blob.rows);
    out.opt_field("Depth", blob.depth);
    out.field("Valid pixels", blob.valid_pixels);
    out.field("Micro block size", blob.micro_block_size);
    out.field("Blob size", format!("{} bytes", blob.blob_size));
    out.field("Data type", named(data_type_name(blob.data_type), blob.data_type));
    out.opt_field("Blobs following", blob.blobs_more);
    out.opt_field("Pass no-data values", blob.pass_no_data_values);
    out.opt_field("Integer values", blob.is_int);
    out.field("Max Z error", blob.max_z_error);
    out.field("Z min", blob.z_min);
    out.field("Z max", blob.z_max);
    out.opt_field("No-data value", blob.no_data_value);
    out.opt_field("No-data value (original)", blob.no_data_value_original);
    out.field("Header size", format!("{} bytes", blob.header_size));
    if let (Some(offset), Some(bytes)) = (blob.mask_offset, blob.mask_bytes) {
        out.field("Mask", format!("{} bytes at 0x{:08X}", bytes, offset));
    }
    if !blob.depth_ranges.is_empty() {
        out.group("Depth ranges", |out| {
            for (index, range) in blob.depth_ranges.iter().enumerate() {
                out.field(&format!("Depth {}", index), format!("{} .. {}", range.min, range.max));
            }
        });
    }
    match (blob.encoding, blob.encoding_flag) {
        (Some(encoding), Some(flag)) => {
            out.field("Encoding", format!("{} ({})", encoding.name(), flag));
        }
        (Some(encoding), None) => {
            out.field("Encoding", encoding.name());
        }
        (None, Some(flag)) => {
            out.field("Encoding", format!("Unknown ({})", flag));
        }
        (None, None) => {}
    }
}

fn try_huffman(version: i32, dt: LercDataType, max_z_error: f64) -> bool {
    let int = version >= 2 && matches!(dt, LercDataType::Byte | LercDataType::Char) && max_z_error == 0.5;
    let float = version >= 6 && dt.is_float() && max_z_error == 0.0;
    int || float
}

fn read_lerc2_body(reader: &mut ByteReader, blob: &mut Lerc2Blob) -> VexelResult<()> {
    blob.mask_offset = Some(reader.position() as u64);
    let mask_bytes = reader.read_i32()?;
    blob.mask_bytes = Some(mask_bytes);
    reader.skip(mask_bytes.max(0) as usize);

    if blob.valid_pixels <= 0 {
        blob.encoding = Some(Lerc2Encoding::NoValidPixels);
        return Ok(());
    }
    if blob.z_min == blob.z_max {
        blob.encoding = Some(Lerc2Encoding::Constant);
        return Ok(());
    }

    let Some(dt) = LercDataType::from_code(blob.data_type) else {
        return Ok(());
    };

    if blob.version >= 4 {
        let depth = blob.depth.unwrap_or(1).max(0) as usize;
        let mut mins = Vec::with_capacity(depth.min(MAX_DEPTH_RANGES));
        let mut maxs = Vec::with_capacity(depth.min(MAX_DEPTH_RANGES));
        for _ in 0..depth {
            mins.push(read_value(reader, dt)?);
        }
        for _ in 0..depth {
            maxs.push(read_value(reader, dt)?);
        }
        let constant = mins == maxs;
        blob.depth_ranges =
            mins.into_iter().zip(maxs).take(MAX_DEPTH_RANGES).map(|(min, max)| LercDepthRange { min, max }).collect();
        if constant {
            blob.encoding = Some(Lerc2Encoding::ConstantPerDepth);
            return Ok(());
        }
    }

    let one_sweep = reader.read_u8()?;
    if one_sweep != 0 {
        blob.encoding = Some(Lerc2Encoding::OneSweep);
        blob.encoding_flag = Some(one_sweep);
        return Ok(());
    }

    if !try_huffman(blob.version, dt, blob.max_z_error) {
        blob.encoding = Some(Lerc2Encoding::Tiling);
        return Ok(());
    }

    let flag = reader.read_u8()?;
    blob.encoding_flag = Some(flag);
    blob.encoding = ImageEncodeMode::from_flag(flag).map(|mode| match mode {
        ImageEncodeMode::Tiling => Lerc2Encoding::Tiling,
        ImageEncodeMode::DeltaHuffman => Lerc2Encoding::DeltaHuffman,
        ImageEncodeMode::Huffman => Lerc2Encoding::Huffman,
        ImageEncodeMode::DeltaDeltaHuffman => Lerc2Encoding::DeltaDeltaHuffman,
    });
    Ok(())
}

fn read_lerc2_header(reader: &mut ByteReader, band: u32) -> VexelResult<Lerc2Blob> {
    let start = reader.position();
    reader.take(LERC2_MAGIC.len())?;
    let version = reader.read_i32()?;
    let checksum = if version >= 3 { Some(reader.read_u32()?) } else { None };

    let rows = reader.read_i32()?;
    let columns = reader.read_i32()?;
    let depth = if version >= 4 { Some(reader.read_i32()?) } else { None };
    let valid_pixels = reader.read_i32()?;
    let micro_block_size = reader.read_i32()?;
    let blob_size = reader.read_i32()?;
    let data_type = reader.read_i32()?;
    let blobs_more = if version >= 6 { Some(reader.read_i32()?) } else { None };

    let flags = if version >= 6 { Some(reader.take(4)?) } else { None };

    let max_z_error = reader.read_f64()?;
    let z_min = reader.read_f64()?;
    let z_max = reader.read_f64()?;
    let (no_data_value, no_data_value_original) =
        if version >= 6 { (Some(reader.read_f64()?), Some(reader.read_f64()?)) } else { (None, None) };

    Ok(Lerc2Blob {
        band,
        version,
        checksum,
        computed_checksum: None,
        rows,
        columns,
        depth,
        valid_pixels,
        micro_block_size,
        blob_size,
        data_type,
        blobs_more,
        pass_no_data_values: flags.map(|flags| flags[0] != 0),
        is_int: flags.map(|flags| flags[1] != 0),
        max_z_error,
        z_min,
        z_max,
        no_data_value,
        no_data_value_original,
        header_size: (reader.position() - start) as u32,
        mask_offset: None,
        mask_bytes: None,
        depth_ranges: Vec::new(),
        encoding: None,
        encoding_flag: None,
    })
}

struct Scanner<'a> {
    data: &'a [u8],
    info: LercInfo,
}

impl Scanner<'_> {
    fn starts_with(&self, offset: usize, magic: &[u8]) -> bool {
        self.data.get(offset..).is_some_and(|rest| rest.starts_with(magic))
    }

    fn blob_end(&self, offset: usize, blob_size: i32) -> usize {
        offset.saturating_add(blob_size.max(0) as usize).min(self.data.len())
    }

    fn scan_lerc2(&mut self) -> (usize, Vec<Lerc2Blob>) {
        let mut offset = 0usize;
        let mut blobs = Vec::new();

        while self.starts_with(offset, LERC2_MAGIC) {
            let mut reader = ByteReader::at(self.data, offset);
            let Ok(mut blob) = read_lerc2_header(&mut reader, blobs.len() as u32) else {
                break;
            };

            let end = self.blob_end(offset, blob.blob_size);
            if blob.version >= 3 && offset + LERC2_MAGIC.len() + 8 < end {
                blob.computed_checksum =
                    Some(compute_checksum_fletcher32(&self.data[offset + LERC2_MAGIC.len() + 8..end]));
            }
            let _ = read_lerc2_body(&mut reader, &mut blob);

            let more = blob.version <= 5 || blob.blobs_more.is_some_and(|more| more > 0);
            let length = end.max(reader.position()) - offset;
            let next = (blob.blob_size > 0).then(|| offset.saturating_add(blob.blob_size as usize));
            self.info.push(offset as u64, Some(length as u64), LercSection::Lerc2Blob(Box::new(blob.clone())));
            blobs.push(blob);

            match next {
                Some(next) if more => offset = next,
                _ => {
                    offset += length;
                    break;
                }
            }
        }

        (offset, blobs)
    }

    fn scan_lerc1(&mut self) -> (usize, Vec<Lerc1Header>) {
        let mut offset = 0usize;
        let mut headers = Vec::new();

        while self.starts_with(offset, LERC1_MAGIC) && self.data.len() - offset >= LERC1_HEADER_SIZE {
            let band = headers.len() as u32;
            let mut reader = ByteReader::at(self.data, offset + LERC1_MAGIC.len());
            let (Ok(version), Ok(image_type), Ok(height), Ok(width), Ok(max_z_error)) =
                (reader.read_i32(), reader.read_i32(), reader.read_i32(), reader.read_i32(), reader.read_f64())
            else {
                break;
            };
            let header = Lerc1Header { band, version, image_type, height, width, max_z_error };
            self.info.push(offset as u64, Some(LERC1_HEADER_SIZE as u64), LercSection::Lerc1Header(header.clone()));
            headers.push(header);
            offset += LERC1_HEADER_SIZE;

            let kinds: &[Lerc1PartKind] =
                if band == 0 { &[Lerc1PartKind::Count, Lerc1PartKind::Z] } else { &[Lerc1PartKind::Z] };
            for &kind in kinds {
                if self.data.len() - offset < LERC1_PART_HEADER_SIZE {
                    return (offset, headers);
                }
                let mut reader = ByteReader::at(self.data, offset);
                let tiles_vertical = reader.read_i32().unwrap_or(0);
                let tiles_horizontal = reader.read_i32().unwrap_or(0);
                let data_bytes = reader.read_i32().unwrap_or(0);
                let max_value = reader.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0.0);
                let end =
                    (offset + LERC1_PART_HEADER_SIZE).saturating_add(data_bytes.max(0) as usize).min(self.data.len());
                let part = Lerc1Part { band, kind, tiles_vertical, tiles_horizontal, data_bytes, max_value };
                self.info.push(offset as u64, Some((end - offset) as u64), LercSection::Lerc1Part(part));
                offset = end;
            }
        }

        (offset, headers)
    }
}

fn channel_layout(channels: u32, masked: bool) -> (ColorModel, u16, bool) {
    match (channels, masked) {
        (1, false) => (ColorModel::Gray, 1, false),
        (1, true) => (ColorModel::GrayAlpha, 2, true),
        (2, _) => (ColorModel::GrayAlpha, 2, true),
        (3, false) => (ColorModel::Rgb, 3, false),
        (3, true) => (ColorModel::Rgba, 4, true),
        (4, _) => (ColorModel::Rgba, 4, true),
        (other, _) => (ColorModel::Multichannel, other.min(u16::MAX as u32) as u16, false),
    }
}

fn sample_layout(dt: LercDataType) -> (u8, SampleFormat) {
    match dt {
        LercDataType::Char => (8, SampleFormat::Signed),
        LercDataType::Byte => (8, SampleFormat::Unsigned),
        LercDataType::Short => (16, SampleFormat::Signed),
        LercDataType::UShort => (16, SampleFormat::Unsigned),
        LercDataType::Int => (32, SampleFormat::Signed),
        LercDataType::UInt => (32, SampleFormat::Unsigned),
        LercDataType::Float => (32, SampleFormat::Float),
        LercDataType::Double => (64, SampleFormat::Float),
    }
}

fn lerc2_summary(blobs: &[Lerc2Blob], data_len: usize) -> Option<ImageSummary> {
    let first = blobs.first()?;
    let dt = LercDataType::from_code(first.data_type)?;
    if first.rows <= 0 || first.columns <= 0 {
        return None;
    }

    let bands: Vec<&Lerc2Blob> = blobs
        .iter()
        .take_while(|blob| blob.rows == first.rows && blob.columns == first.columns && blob.depth == first.depth)
        .collect();
    let depth = first.depth.unwrap_or(1).max(1) as u32;
    let pixels = first.rows as i64 * first.columns as i64;
    let masked = (first.valid_pixels as i64) < pixels && first.mask_bytes.is_some_and(|bytes| bytes > 0);

    let (color_model, channels, has_alpha) = channel_layout(bands.len() as u32 * depth, masked);
    let (bits, sample_format) = sample_layout(dt);
    let mut summary = ImageSummary::new(first.columns as u32, first.rows as u32, color_model, channels, bits);
    summary.sample_format = sample_format;
    summary.has_alpha = has_alpha;
    summary.compression = Some(format!("Lerc2 v{}", first.version));

    let expected: u64 = bands.iter().map(|blob| blob.blob_size.max(0) as u64).sum();
    summary.data = Some(DataExtent { expected, actual: expected.min(data_len as u64) });
    Some(summary)
}

fn lerc1_summary(headers: &[Lerc1Header], count_part: Option<&Lerc1Part>) -> Option<ImageSummary> {
    let first = headers.first()?;
    if first.width <= 0 || first.height <= 0 {
        return None;
    }
    let bands = headers.iter().take_while(|header| header.width == first.width && header.height == first.height);
    let all_valid = count_part.is_some_and(|part| {
        part.tiles_vertical == 0 && part.tiles_horizontal == 0 && part.data_bytes == 0 && part.max_value > 0.0
    });
    let (color_model, channels, has_alpha) = channel_layout(bands.count() as u32, !all_valid);
    let mut summary = ImageSummary::new(first.width as u32, first.height as u32, color_model, channels, 32);
    summary.sample_format = SampleFormat::Float;
    summary.has_alpha = has_alpha;
    summary.compression = Some(format!("Lerc1 (CntZImage v{})", first.version));
    Some(summary)
}

pub fn scan(data: &[u8]) -> LercInfo {
    let mut scanner = Scanner { data, info: LercInfo::default() };

    let (end, summary) = if scanner.starts_with(0, LERC2_MAGIC) {
        let (end, blobs) = scanner.scan_lerc2();
        (end, lerc2_summary(&blobs, data.len()))
    } else if scanner.starts_with(0, LERC1_MAGIC) {
        let (end, headers) = scanner.scan_lerc1();
        let count_part = scanner.info.sections.iter().find_map(|section| match &section.data {
            LercSection::Lerc1Part(part) if part.kind == Lerc1PartKind::Count => Some(part),
            _ => None,
        });
        let summary = lerc1_summary(&headers, count_part);
        (end, summary)
    } else {
        (0, None)
    };

    if end < data.len() {
        scanner.info.push(end as u64, Some((data.len() - end) as u64), LercSection::TrailingData);
    }

    scanner.info.summary = summary;
    scanner.info
}
