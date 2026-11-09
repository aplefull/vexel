use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};

use serde::Serialize;
use tsify::Tsify;

use crate::bitreader::BitReader;
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::exif::{parse_value, tag_name as exif_tag_name, ExifValue, IfdType};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter, Section};
use crate::utils::limits::Limits;
use crate::utils::tiff::ifd::{type_size, Ifd, IfdCycleGuard, IfdEntry};
use crate::utils::tiff::types::TiffTags;
use crate::utils::types::ByteOrder;

const MAX_VALUE_BYTES: usize = 4096;
const MAX_PREVIEW_VALUES: usize = 16;
const MAX_TOTAL_ENTRIES: usize = 1 << 18;
const MAX_STORED_VALUE_COST: usize = 16 << 20;
const SMALL_VALUE_BYTES: u64 = 128;
const SMALL_VALUE_RESERVE: usize = 4 << 20;
const ASCII_STRING_COST: usize = 64;

const TIFF_MAGIC: u16 = 42;
const JPEG_XR_MAGIC: u16 = 0x01BC;
const JPEG_XR_PIXEL_FORMAT: u16 = 48129;
const JPEG_XR_IMAGE_OFFSET: u16 = 48320;
const SUB_IFDS: u16 = 330;
const EXIF_IFD: u16 = 34665;
const GPS_IFD: u16 = 34853;
const INTEROP_IFD: u16 = 40965;

#[tsify::declare]
pub type TiffInfo = FormatInfo<TiffSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TiffFileHeaderSection {
    pub byte_order: ByteOrder,
    pub magic: u16,
    pub first_ifd_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum IfdKind {
    Page,
    SubIfd,
    Exif,
    Gps,
    Interop,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct TiffEntry {
    pub tag: u16,
    pub field_type: u16,
    pub count: u32,
    pub value_offset: Option<u32>,
    pub value: Option<ExifValue>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct TiffIfdSection {
    pub kind: IfdKind,
    pub index: u32,
    pub parent: Option<u32>,
    pub declared_entries: u16,
    pub next_ifd_offset: u32,
    pub entries: Vec<TiffEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TiffImageData {
    pub ifd: u32,
    pub tiled: bool,
    pub chunk_count: u32,
    pub declared_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum TiffSection {
    FileHeader(TiffFileHeaderSection),
    Ifd(TiffIfdSection),
    ImageData(TiffImageData),
}

pub struct TiffImageIfd {
    pub kind: IfdKind,
    pub index: u32,
    pub ifd: Ifd,
    pub image_data: Option<TiffImageData>,
}

pub struct TiffStructure {
    pub sections: Vec<Section<TiffSection>>,
    pub image_ifds: Vec<TiffImageIfd>,
}

impl TiffStructure {
    pub fn ifd_section(&self, index: u32) -> Option<&TiffIfdSection> {
        self.sections.iter().find_map(|section| match &section.data {
            TiffSection::Ifd(ifd) if ifd.index == index => Some(ifd),
            _ => None,
        })
    }
}

impl TiffIfdSection {
    pub fn entry(&self, tag: u16) -> Option<&TiffEntry> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }
}

impl TiffEntry {
    pub fn numbers(&self) -> Vec<f64> {
        match &self.value {
            Some(ExifValue::Byte(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::Short(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::Long(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::SByte(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::SShort(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::SLong(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::Float(v)) => v.iter().map(|&x| f64::from(x)).collect(),
            Some(ExifValue::Double(v)) => v.clone(),
            Some(ExifValue::Rational(v)) => {
                v.iter().map(|&(n, d)| if d == 0 { 0.0 } else { f64::from(n) / f64::from(d) }).collect()
            }
            Some(ExifValue::SRational(v)) => {
                v.iter().map(|&(n, d)| if d == 0 { 0.0 } else { f64::from(n) / f64::from(d) }).collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn text(&self) -> Option<String> {
        match &self.value {
            Some(ExifValue::Ascii(strings)) => Some(strings.join(" / ")),
            Some(ExifValue::Byte(bytes)) | Some(ExifValue::Undefined(bytes)) => {
                Some(String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string())
            }
            _ => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.value {
            Some(ExifValue::Byte(bytes)) | Some(ExifValue::Undefined(bytes)) => Some(bytes),
            _ => None,
        }
    }
}

struct Walker<'a, R: Read + Seek> {
    reader: &'a mut BitReader<R>,
    limits: &'a mut Limits,
    byte_order: ByteOrder,
    stream_len: u64,
    guard: IfdCycleGuard,
    next_index: u32,
    total_entries: usize,
    value_budget: usize,
    value_budget_warned: bool,
    structure: TiffStructure,
}

enum WalkTask {
    Visit { offset: u32, kind: IfdKind, parent: Option<u32> },
    SubIfds { pending: Vec<u32>, parent: u32 },
    Finish(Box<TiffImageIfd>),
}

pub fn walk_structure<R: Read + Seek>(reader: &mut BitReader<R>, limits: &mut Limits) -> VexelResult<TiffStructure> {
    reader.seek(SeekFrom::Start(0))?;
    let mut marker = [0u8; 2];
    reader.read_exact(&mut marker)?;
    let byte_order = match &marker {
        b"II" => ByteOrder::LittleEndian,
        b"MM" => ByteOrder::BigEndian,
        _ => return Err(VexelError::Custom("Invalid byte order marker".to_string())),
    };
    reader.set_endianness(byte_order);
    let magic = reader.read_u16()?;
    let first_ifd_offset = reader.read_u32().unwrap_or(0);
    let stream_len = reader.seek(SeekFrom::End(0))?;

    let mut walker = Walker {
        reader,
        limits,
        byte_order,
        stream_len,
        guard: IfdCycleGuard::default(),
        next_index: 0,
        total_entries: 0,
        value_budget: MAX_STORED_VALUE_COST,
        value_budget_warned: false,
        structure: TiffStructure { sections: Vec::new(), image_ifds: Vec::new() },
    };

    walker.structure.sections.push(Section::new(
        0,
        Some(8),
        TiffSection::FileHeader(TiffFileHeaderSection { byte_order, magic, first_ifd_offset }),
    ));

    if magic != TIFF_MAGIC && magic != JPEG_XR_MAGIC {
        log_warn!("TIFF magic is {} instead of 42, not walking IFDs", magic);
    } else {
        walker.walk(first_ifd_offset);
    }

    let mut structure = walker.structure;
    structure.sections.sort_by_key(|section| section.offset);
    Ok(structure)
}

impl<R: Read + Seek> Walker<'_, R> {
    fn walk(&mut self, first: u32) {
        let mut tasks = vec![WalkTask::Visit { offset: first, kind: IfdKind::Page, parent: None }];
        while let Some(task) = tasks.pop() {
            match task {
                WalkTask::Visit { offset, kind, parent } => self.visit_ifd(offset, kind, parent, &mut tasks),
                WalkTask::SubIfds { mut pending, parent } => {
                    if let Some(offset) = pending.pop() {
                        if !pending.is_empty() {
                            tasks.push(WalkTask::SubIfds { pending, parent });
                        }
                        tasks.push(WalkTask::Visit { offset, kind: IfdKind::SubIfd, parent: Some(parent) });
                    }
                }
                WalkTask::Finish(image) => self.structure.image_ifds.push(*image),
            }
        }
    }

    fn visit_ifd(&mut self, offset: u32, kind: IfdKind, parent: Option<u32>, tasks: &mut Vec<WalkTask>) {
        if offset == 0 {
            return;
        }
        if self.total_entries >= MAX_TOTAL_ENTRIES {
            log_warn!("Too many TIFF entries, stopping structure walk");
            return;
        }
        if !self.guard.enter(offset) {
            return;
        }

        let ifd = match Ifd::read(self.reader, offset, self.byte_order, self.limits) {
            Ok(ifd) => ifd,
            Err(e) => {
                log_warn!("Failed to read TIFF IFD at 0x{:08X}: {}", offset, e);
                return;
            }
        };
        self.total_entries += ifd.entries.len();

        let index = self.next_index;
        self.next_index += 1;

        let entries = ifd.entries.iter().map(|entry| self.read_entry(&ifd, entry, true)).collect();
        let length = 2 + 12 * ifd.entries.len() as u64 + 4;
        self.structure.sections.push(Section::new(
            u64::from(offset),
            Some(length),
            TiffSection::Ifd(TiffIfdSection {
                kind,
                index,
                parent,
                declared_entries: ifd.declared_entries,
                next_ifd_offset: ifd.next_ifd_offset,
                entries,
            }),
        ));

        let is_image = matches!(kind, IfdKind::Page | IfdKind::SubIfd);
        let image_data = if is_image { self.image_data(&ifd, index) } else { None };
        let image_data = image_data.map(|(data, start)| {
            self.structure.sections.push(Section::new(
                start,
                Some(data.declared_bytes),
                TiffSection::ImageData(data.clone()),
            ));
            data
        });

        let mut children = Vec::new();
        for (tag, child_kind) in [(EXIF_IFD, IfdKind::Exif), (GPS_IFD, IfdKind::Gps), (INTEROP_IFD, IfdKind::Interop)] {
            if let Some(child) = self.pointer(&ifd, tag) {
                children.push((child, child_kind));
            }
        }
        let mut sub_ifds =
            if kind != IfdKind::Exif && kind != IfdKind::Gps { self.pointers(&ifd, SUB_IFDS) } else { Vec::new() };

        if is_image {
            let next = ifd.next_ifd_offset;
            if next != 0 {
                tasks.push(WalkTask::Visit { offset: next, kind, parent });
            }
            tasks.push(WalkTask::Finish(Box::new(TiffImageIfd { kind, index, ifd, image_data })));
        }

        if !sub_ifds.is_empty() {
            sub_ifds.reverse();
            tasks.push(WalkTask::SubIfds { pending: sub_ifds, parent: index });
        }
        for (child, child_kind) in children.into_iter().rev() {
            tasks.push(WalkTask::Visit { offset: child, kind: child_kind, parent: Some(index) });
        }
    }

    fn read_entry(&mut self, ifd: &Ifd, entry: &IfdEntry, budgeted: bool) -> TiffEntry {
        let mut result = TiffEntry {
            tag: entry.tag,
            field_type: entry.field_type,
            count: entry.count,
            value_offset: None,
            value: None,
            truncated: false,
        };

        let Some(size) = type_size(entry.field_type) else {
            return result;
        };

        let total = u64::from(entry.count).saturating_mul(size as u64);
        if total <= 4 {
            let bytes = &ifd.inline_bytes(entry)[..total as usize];
            result.value = Some(parse_value(entry.field_type, bytes.len() / size, bytes, ifd.byte_order));
            return result;
        }

        result.value_offset = Some(entry.value_offset);
        let wanted = (total.min(MAX_VALUE_BYTES as u64) as usize) / size * size;
        result.truncated = (wanted as u64) < total;

        let available = match (budgeted, total <= SMALL_VALUE_BYTES) {
            (false, _) => usize::MAX,
            (true, true) => self.value_budget,
            (true, false) => self.value_budget.saturating_sub(SMALL_VALUE_RESERVE),
        };
        if wanted > available {
            return self.budget_exhausted(result);
        }

        let mut buffer = vec![0u8; wanted];
        let read = self
            .reader
            .seek(SeekFrom::Start(u64::from(entry.value_offset)))
            .and_then(|_| self.reader.read_exact(&mut buffer));
        if read.is_err() {
            result.truncated = false;
            return result;
        }

        let value = parse_value(entry.field_type, wanted / size, &buffer, ifd.byte_order);
        if budgeted {
            let cost = match &value {
                ExifValue::Ascii(strings) => wanted.saturating_add(strings.len().saturating_mul(ASCII_STRING_COST)),
                _ => wanted,
            };
            if cost > available {
                return self.budget_exhausted(result);
            }
            self.value_budget -= cost;
        }
        result.value = Some(value);
        result
    }

    fn budget_exhausted(&mut self, mut result: TiffEntry) -> TiffEntry {
        if !self.value_budget_warned {
            log_warn!(
                "TIFF info value budget of {} bytes exhausted, remaining values are not read",
                MAX_STORED_VALUE_COST
            );
            self.value_budget_warned = true;
        }
        result.truncated = true;
        result
    }

    fn pointer(&mut self, ifd: &Ifd, tag: u16) -> Option<u32> {
        self.pointers(ifd, tag).into_iter().next()
    }

    fn pointers(&mut self, ifd: &Ifd, tag: u16) -> Vec<u32> {
        let Some(entry) = ifd.entries.iter().find(|e| e.tag == tag) else {
            return Vec::new();
        };
        let entry = self.read_entry(ifd, entry, false);
        let values: Vec<u32> = match entry.value {
            Some(ExifValue::Long(v)) => v,
            Some(ExifValue::Short(v)) => v.into_iter().map(u32::from).collect(),
            Some(ExifValue::Undefined(v)) | Some(ExifValue::Byte(v)) if v.len() >= 4 => v
                .chunks_exact(4)
                .map(|b| match ifd.byte_order {
                    ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                })
                .collect(),
            _ => Vec::new(),
        };
        values.into_iter().filter(|&o| o != 0).collect()
    }

    fn chunk_arrays(&mut self, ifd: &Ifd) -> Option<(bool, Vec<u32>, Vec<u32>)> {
        let tiled = ifd.has(TiffTags::TileOffsets);
        let (offsets_tag, counts_tag) = if tiled {
            (TiffTags::TileOffsets, TiffTags::TileByteCounts)
        } else if !ifd.has(TiffTags::StripOffsets) && ifd.has(TiffTags::ImageOffset) {
            (TiffTags::ImageOffset, TiffTags::ImageByteCount)
        } else {
            (TiffTags::StripOffsets, TiffTags::StripByteCounts)
        };
        let offsets = ifd.values::<u32, _>(offsets_tag, self.reader, self.limits)?;
        let counts = ifd.values::<u32, _>(counts_tag, self.reader, self.limits).unwrap_or_default();
        Some((tiled, offsets, counts))
    }

    fn image_data(&mut self, ifd: &Ifd, index: u32) -> Option<(TiffImageData, u64)> {
        let (tiled, offsets, counts) = self.chunk_arrays(ifd)?;
        let start = offsets.iter().filter(|&&o| o != 0).min().map(|&o| u64::from(o)).unwrap_or(0);
        let mut declared_bytes = 0u64;
        let mut available_bytes = 0u64;
        for (i, &offset) in offsets.iter().enumerate() {
            let count = u64::from(counts.get(i).copied().unwrap_or(0));
            declared_bytes += count;
            let end = (u64::from(offset) + count).min(self.stream_len);
            available_bytes += end.saturating_sub(u64::from(offset));
        }
        Some((
            TiffImageData { ifd: index, tiled, chunk_count: offsets.len() as u32, declared_bytes, available_bytes },
            start,
        ))
    }
}

pub fn first_u32(entry: &TiffEntry) -> Option<u32> {
    match entry.value.as_ref()? {
        ExifValue::Short(v) => v.first().map(|&x| u32::from(x)),
        ExifValue::Long(v) => v.first().copied(),
        ExifValue::Byte(v) => v.first().map(|&x| u32::from(x)),
        _ => None,
    }
}

pub fn compression_name(code: u32) -> &'static str {
    match code {
        1 => "None",
        2 => "CCITT modified Huffman RLE",
        3 => "CCITT Group 3 fax",
        4 => "CCITT Group 4 fax",
        5 => "LZW",
        6 => "Old-style JPEG",
        7 => "JPEG",
        8 => "Deflate (Adobe)",
        9 => "JBIG (T.85)",
        10 => "JBIG (T.43)",
        32766 => "NeXT 2-bit RLE",
        32771 => "CCITT RLE (word-aligned)",
        32773 => "PackBits",
        32809 => "ThunderScan",
        32895 => "IT8 CT padding",
        32896 => "IT8 linework RLE",
        32897 => "IT8 monochrome",
        32898 => "IT8 binary line art",
        32908 => "Pixar film",
        32909 => "Pixar log",
        32946 => "Deflate",
        32947 => "Kodak DCS",
        34661 => "JBIG",
        34676 => "SGI LogL",
        34677 => "SGI LogLuv 24-bit",
        34712 => "JPEG 2000",
        34887 => "LERC",
        34892 => "Lossy JPEG (DNG)",
        34925 => "LZMA",
        34933 => "PNG",
        34934 => "JPEG XR",
        50000 => "Zstandard",
        50001 => "WebP",
        50002 => "JPEG XL",
        _ => "Unknown",
    }
}

pub fn photometric_name(code: u32) -> &'static str {
    match code {
        0 => "WhiteIsZero",
        1 => "BlackIsZero",
        2 => "RGB",
        3 => "Palette",
        4 => "Transparency mask",
        5 => "Separated (CMYK)",
        6 => "YCbCr",
        8 => "CIELab",
        9 => "ICCLab",
        10 => "ITULab",
        32803 => "Color filter array",
        32844 => "LogL",
        32845 => "LogLuv",
        34892 => "Linear raw",
        51177 => "Depth",
        _ => "Unknown",
    }
}

fn enumerated(tag: u16, value: u32) -> Option<String> {
    let name = match tag {
        259 => compression_name(value),
        262 => photometric_name(value),
        266 => match value {
            1 => "MSB first",
            2 => "LSB first",
            _ => "Unknown",
        },
        274 => match value {
            1 => "Top-left",
            2 => "Top-right",
            3 => "Bottom-right",
            4 => "Bottom-left",
            5 => "Left-top",
            6 => "Right-top",
            7 => "Right-bottom",
            8 => "Left-bottom",
            _ => "Unknown",
        },
        284 => match value {
            1 => "Chunky",
            2 => "Planar",
            _ => "Unknown",
        },
        296 => match value {
            1 => "No unit",
            2 => "Inch",
            3 => "Centimeter",
            _ => "Unknown",
        },
        317 => match value {
            1 => "None",
            2 => "Horizontal differencing",
            3 => "Floating point",
            34892 => "Horizontal differencing X2",
            34893 => "Horizontal differencing X4",
            34894 => "Floating point X2",
            34895 => "Floating point X4",
            _ => "Unknown",
        },
        338 => match value {
            0 => "Unspecified",
            1 => "Associated alpha",
            2 => "Unassociated alpha",
            _ => "Unknown",
        },
        339 => match value {
            1 => "Unsigned integer",
            2 => "Signed integer",
            3 => "IEEE floating point",
            4 => "Undefined",
            5 => "Complex integer",
            6 => "Complex floating point",
            _ => "Unknown",
        },
        531 => match value {
            1 => "Centered",
            2 => "Co-sited",
            _ => "Unknown",
        },
        _ => return None,
    };
    Some(format!("{} ({})", name, value))
}

fn subfile_flags(value: u32) -> String {
    let mut flags = Vec::new();
    if value & 1 != 0 {
        flags.push("reduced resolution");
    }
    if value & 2 != 0 {
        flags.push("page of multi-page image");
    }
    if value & 4 != 0 {
        flags.push("transparency mask");
    }
    if value & 0x10 != 0 {
        flags.push("depth map");
    }
    if flags.is_empty() {
        format!("Full-resolution image ({})", value)
    } else {
        format!("{} ({})", flags.join(", "), value)
    }
}

fn is_offset_tag(tag: u16) -> bool {
    matches!(tag, 273 | 288 | 324 | 330 | 513 | 34665 | 34853 | 40965 | JPEG_XR_IMAGE_OFFSET | 48322)
}

fn is_blob_tag(tag: u16) -> bool {
    matches!(tag, 347 | 33723 | 34377 | 34675 | 37500 | 50740 | 50831 | 50833)
}

fn preview<T>(values: &[T], format: impl Fn(&T) -> String) -> String {
    values.iter().take(MAX_PREVIEW_VALUES).map(format).collect::<Vec<_>>().join(", ")
}

fn rational_text(numerator: f64, denominator: f64, n: impl std::fmt::Display, d: impl std::fmt::Display) -> String {
    if denominator == 0.0 {
        format!("{}/{}", n, d)
    } else {
        format!("{}/{} ({})", n, d, numerator / denominator)
    }
}

fn entry_value(entry: &TiffEntry) -> String {
    let Some(value) = &entry.value else {
        return match type_size(entry.field_type) {
            None => format!("unsupported field type {}", entry.field_type),
            Some(_) if entry.truncated => format!("{} values (not read, info size budget exhausted)", entry.count),
            Some(_) => "unreadable".to_string(),
        };
    };

    if entry.tag == 254 {
        if let Some(v) = first_u32(entry) {
            return subfile_flags(v);
        }
    }

    if is_blob_tag(entry.tag) {
        return format!("{} bytes", entry.count);
    }

    if entry.tag == JPEG_XR_PIXEL_FORMAT {
        if let ExifValue::Byte(bytes) | ExifValue::Undefined(bytes) = value {
            return bytes.iter().map(|b| format!("{:02X}", b)).collect();
        }
    }

    if entry.tag == 700 {
        if let ExifValue::Byte(bytes) | ExifValue::Undefined(bytes) = value {
            return String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string();
        }
    }

    let numbers: Option<Vec<u32>> = match value {
        ExifValue::Short(v) => Some(v.iter().map(|&x| u32::from(x)).collect()),
        ExifValue::Long(v) => Some(v.clone()),
        _ => None,
    };

    let text = match (&numbers, value) {
        (Some(v), _) if v.len() == 1 && enumerated(entry.tag, v[0]).is_some() => {
            return enumerated(entry.tag, v[0]).unwrap_or_default();
        }
        (Some(v), _) if entry.tag == 338 || entry.tag == 339 => {
            preview(v, |&x| enumerated(entry.tag, x).unwrap_or_else(|| x.to_string()))
        }
        (Some(v), _) if is_offset_tag(entry.tag) => preview(v, |x| format!("0x{:08X}", x)),
        (Some(v), _) => preview(v, |x| x.to_string()),
        (_, ExifValue::Ascii(strings)) => return strings.join(" / "),
        (_, ExifValue::Byte(_)) | (_, ExifValue::Undefined(_)) => return format!("{} bytes", entry.count),
        (_, ExifValue::Rational(v)) => preview(v, |&(n, d)| rational_text(f64::from(n), f64::from(d), n, d)),
        (_, ExifValue::SRational(v)) => preview(v, |&(n, d)| rational_text(f64::from(n), f64::from(d), n, d)),
        (_, ExifValue::SByte(v)) => preview(v, |x| x.to_string()),
        (_, ExifValue::SShort(v)) => preview(v, |x| x.to_string()),
        (_, ExifValue::SLong(v)) => preview(v, |x| x.to_string()),
        (_, ExifValue::Float(v)) => preview(v, |x| x.to_string()),
        (_, ExifValue::Double(v)) => preview(v, |x| x.to_string()),
        _ => value.to_string(),
    };

    if entry.count as usize > MAX_PREVIEW_VALUES {
        format!("{}, ... ({} values)", text, entry.count)
    } else {
        text
    }
}

fn entry_label(entry: &TiffEntry, kind: IfdKind) -> String {
    let name = match kind {
        IfdKind::Exif => exif_tag_name(entry.tag, IfdType::ExifIfd).or_else(|| TiffTags::name(entry.tag)),
        IfdKind::Gps => exif_tag_name(entry.tag, IfdType::GpsIfd),
        IfdKind::Interop => exif_tag_name(entry.tag, IfdType::InteropIfd),
        IfdKind::Page | IfdKind::SubIfd => {
            TiffTags::name(entry.tag).or_else(|| exif_tag_name(entry.tag, IfdType::Ifd0))
        }
    };
    let name = name.or(match entry.tag {
        48256 => Some("ImageWidth"),
        _ => None,
    });
    match name {
        Some(name) => name.to_string(),
        None => format!("Tag 0x{:04X} ({})", entry.tag, entry.tag),
    }
}

impl InfoSection for TiffSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            TiffSection::FileHeader(_) => Cow::Borrowed("File Header"),
            TiffSection::Ifd(ifd) => {
                let parent = ifd.parent.unwrap_or(0);
                Cow::Owned(match ifd.kind {
                    IfdKind::Page => format!("IFD {}", ifd.index),
                    IfdKind::SubIfd => format!("IFD {} (SubIFD of IFD {})", ifd.index, parent),
                    IfdKind::Exif => format!("IFD {} (Exif IFD of IFD {})", ifd.index, parent),
                    IfdKind::Gps => format!("IFD {} (GPS IFD of IFD {})", ifd.index, parent),
                    IfdKind::Interop => format!("IFD {} (Interoperability IFD of IFD {})", ifd.index, parent),
                })
            }
            TiffSection::ImageData(data) => Cow::Owned(format!("Image Data (IFD {})", data.ifd)),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            TiffSection::FileHeader(header) => {
                let order = match header.byte_order {
                    ByteOrder::LittleEndian => "Little-endian (II)",
                    ByteOrder::BigEndian => "Big-endian (MM)",
                };
                out.field("Byte order", order);
                match header.magic {
                    TIFF_MAGIC => out.field("Magic", "42 (TIFF)"),
                    JPEG_XR_MAGIC => out.field("Magic", "0x01BC (JPEG XR)"),
                    43 => out.field("Magic", "43 (BigTIFF, unsupported)"),
                    other => out.field("Magic", format!("{} (invalid)", other)),
                };
                out.field("First IFD offset", format!("0x{:08X}", header.first_ifd_offset));
            }
            TiffSection::Ifd(ifd) => {
                if ifd.declared_entries as usize != ifd.entries.len() {
                    out.field("Entries", format!("{} of {} declared", ifd.entries.len(), ifd.declared_entries));
                } else {
                    out.field("Entries", ifd.entries.len());
                }
                for entry in &ifd.entries {
                    out.text(&entry_label(entry, ifd.kind), &entry_value(entry));
                }
                if ifd.next_ifd_offset != 0 {
                    out.field("Next IFD offset", format!("0x{:08X}", ifd.next_ifd_offset));
                }
            }
            TiffSection::ImageData(data) => {
                out.field(if data.tiled { "Tiles" } else { "Strips" }, data.chunk_count);
                if data.available_bytes < data.declared_bytes {
                    out.field("Data", format!("{} of {} bytes (truncated)", data.available_bytes, data.declared_bytes));
                } else {
                    out.field("Data", format!("{} bytes", data.declared_bytes));
                }
            }
        }
    }
}
