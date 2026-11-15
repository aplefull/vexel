use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::limits::Limits;
use crate::utils::tiff::types::TiffTags;
use crate::utils::types::ByteOrder;

pub const TIFF_MAGIC: u16 = 42;
pub const BIGTIFF_MAGIC: u16 = 43;

const BIGTIFF_OFFSET_SIZE: u16 = 8;
const MAX_INLINE_BYTES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IfdFormat {
    pub byte_order: ByteOrder,
    pub big: bool,
}

impl IfdFormat {
    pub fn classic(byte_order: ByteOrder) -> Self {
        Self { byte_order, big: false }
    }

    pub fn inline_capacity(self) -> usize {
        if self.big {
            8
        } else {
            4
        }
    }

    pub fn header_length(self) -> u64 {
        if self.big {
            16
        } else {
            8
        }
    }

    fn count_size(self) -> u64 {
        if self.big {
            8
        } else {
            2
        }
    }

    fn entry_size(self) -> u64 {
        if self.big {
            20
        } else {
            12
        }
    }

    fn offset_size(self) -> u64 {
        if self.big {
            8
        } else {
            4
        }
    }

    pub fn ifd_length(self, entries: usize) -> u64 {
        self.count_size() + self.entry_size() * entries as u64 + self.offset_size()
    }

    fn read_offset<R: Read + Seek>(self, reader: &mut BitReader<R>) -> std::io::Result<u64> {
        if self.big {
            reader.read_u64()
        } else {
            reader.read_u32().map(u64::from)
        }
    }

    pub fn decode_offset(self, bytes: &[u8]) -> u64 {
        let mut buf = [0u8; 8];
        let len = bytes.len().min(self.offset_size() as usize);
        match self.byte_order {
            ByteOrder::LittleEndian => {
                buf[..len].copy_from_slice(&bytes[..len]);
                u64::from_le_bytes(buf)
            }
            ByteOrder::BigEndian => {
                buf[8 - len..].copy_from_slice(&bytes[..len]);
                u64::from_be_bytes(buf)
            }
        }
    }
}

pub struct TiffFileHeader {
    pub byte_order: ByteOrder,
    pub format: IfdFormat,
    pub first_ifd_offset: u64,
}

pub fn read_file_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<TiffFileHeader> {
    let mut byte_order_marker = [0u8; 2];
    reader.read_exact(&mut byte_order_marker)?;

    let byte_order = match &byte_order_marker {
        b"II" => ByteOrder::LittleEndian,
        b"MM" => ByteOrder::BigEndian,
        _ => return Err(VexelError::Custom("Invalid byte order marker".to_string())),
    };

    reader.set_endianness(byte_order);

    let magic = reader.read_u16()?;
    let format = match magic {
        TIFF_MAGIC => IfdFormat::classic(byte_order),
        BIGTIFF_MAGIC => {
            let offset_size = reader.read_u16()?;
            if offset_size != BIGTIFF_OFFSET_SIZE {
                log_warn!("BigTIFF offset size is {} instead of 8, reading 8-byte offsets", offset_size);
            }
            let reserved = reader.read_u16()?;
            if reserved != 0 {
                log_warn!("BigTIFF reserved header field is {} instead of 0", reserved);
            }
            IfdFormat { byte_order, big: true }
        }
        _ => return Err(VexelError::Custom("Not a TIFF file".to_string())),
    };

    let first_ifd_offset = format.read_offset(reader)?;

    Ok(TiffFileHeader { byte_order, format, first_ifd_offset })
}

#[derive(Debug, Clone)]
pub struct IfdCycleGuard {
    visited: HashSet<u64>,
    remaining: usize,
}

impl Default for IfdCycleGuard {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl IfdCycleGuard {
    pub fn new(max_ifds: usize) -> Self {
        Self { visited: HashSet::new(), remaining: max_ifds }
    }

    pub fn enter(&mut self, offset: u64) -> bool {
        if offset == 0 || self.remaining == 0 || !self.visited.insert(offset) {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

pub fn type_size(field_type: u16) -> Option<usize> {
    match field_type {
        1 | 2 | 6 | 7 | 129 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 | 16 | 17 | 18 => Some(8),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct IfdEntry {
    pub tag: u16,
    pub field_type: u16,
    pub count: u64,
    pub value_offset: u64,
    pub inline: [u8; MAX_INLINE_BYTES],
}

#[derive(Debug, Clone)]
pub struct Ifd {
    pub offset: u64,
    pub byte_order: ByteOrder,
    pub format: IfdFormat,
    pub declared_entries: u64,
    pub entries: Vec<IfdEntry>,
    pub next_ifd_offset: u64,
}

impl Ifd {
    pub fn read<R: Read + Seek>(
        reader: &mut BitReader<R>,
        offset: u64,
        format: IfdFormat,
        limits: &mut Limits,
    ) -> VexelResult<Ifd> {
        reader.seek(SeekFrom::Start(offset))?;
        let declared_entries = if format.big { reader.read_u64()? } else { u64::from(reader.read_u16()?) };

        let table_start = offset.saturating_add(format.count_size());
        let stream_len = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(table_start))?;
        let available_entries = stream_len.saturating_sub(table_start) / format.entry_size();
        if declared_entries > available_entries {
            log_warn!(
                "TIFF IFD at {} declares {} entries but only {} fit in the file",
                offset,
                declared_entries,
                available_entries
            );
        }
        let num_entries = declared_entries.min(available_entries) as usize;

        let table_bytes = num_entries.saturating_mul(std::mem::size_of::<IfdEntry>());
        limits.reserve_usize(table_bytes)?;

        let capacity = format.inline_capacity();
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let entry = (|| {
                let tag = reader.read_u16()?;
                let field_type = reader.read_u16()?;
                let count = if format.big { reader.read_u64()? } else { u64::from(reader.read_u32()?) };
                let mut inline = [0u8; MAX_INLINE_BYTES];
                reader.read_exact(&mut inline[..capacity])?;
                let value_offset = format.decode_offset(&inline[..capacity]);
                Ok::<IfdEntry, VexelError>(IfdEntry { tag, field_type, count, value_offset, inline })
            })();

            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => break,
            }
        }
        limits.free_usize(table_bytes);

        let next_ifd_offset = format.read_offset(reader).unwrap_or(0);

        Ok(Ifd { offset, byte_order: format.byte_order, format, declared_entries, entries, next_ifd_offset })
    }

    pub fn entry(&self, tag: TiffTags) -> Option<&IfdEntry> {
        let tag = tag as u16;
        self.entries.iter().find(|e| e.tag == tag)
    }

    pub fn has(&self, tag: TiffTags) -> bool {
        self.entry(tag).is_some()
    }

    pub fn value<T, R>(&self, tag: TiffTags, reader: &mut BitReader<R>) -> Option<T>
    where
        T: TryFrom<u64>,
        R: Read + Seek,
    {
        let entry = self.entry(tag)?;
        read_single_value(entry, self.format, reader).ok()
    }

    pub fn values<T, R>(&self, tag: TiffTags, reader: &mut BitReader<R>, limits: &mut Limits) -> Option<Vec<T>>
    where
        T: TryFrom<u64>,
        R: Read + Seek,
    {
        let entry = self.entry(tag)?;
        read_multiple_values(entry, self.format, reader, limits).ok()
    }

    pub fn rational<R: Read + Seek>(&self, tag: TiffTags, reader: &mut BitReader<R>) -> Option<f32> {
        let entry = self.entry(tag)?;
        let (numerator, denominator) = if 8 <= self.format.inline_capacity() {
            let mut inline = inline_reader(entry, self.format);
            (inline.read_u32().ok()?, inline.read_u32().ok()?)
        } else {
            reader.seek(SeekFrom::Start(entry.value_offset)).ok()?;
            (reader.read_u32().ok()?, reader.read_u32().ok()?)
        };
        (denominator != 0).then(|| numerator as f32 / denominator as f32)
    }

    pub fn rationals<R: Read + Seek>(
        &self,
        tag: TiffTags,
        reader: &mut BitReader<R>,
        limits: &mut Limits,
    ) -> Option<Vec<f32>> {
        let entry = self.entry(tag)?;
        read_rationals(entry, self.format, reader, limits, false).ok()
    }

    pub fn signed_rationals<R: Read + Seek>(
        &self,
        tag: TiffTags,
        reader: &mut BitReader<R>,
        limits: &mut Limits,
    ) -> Option<Vec<f32>> {
        let entry = self.entry(tag)?;
        read_rationals(entry, self.format, reader, limits, true).ok()
    }

    pub fn string<R: Read + Seek>(&self, tag: TiffTags, reader: &mut BitReader<R>) -> Option<String> {
        let entry = self.entry(tag)?;
        let count = (entry.count as usize).min(256);

        let bytes = if count <= self.format.inline_capacity() {
            self.inline_bytes(entry)[..count].to_vec()
        } else {
            let mut buf = vec![0u8; count];
            if reader.seek(SeekFrom::Start(entry.value_offset)).is_err() || reader.read_exact(&mut buf).is_err() {
                return None;
            }
            buf
        };

        Some(String::from_utf8_lossy(&bytes).trim_end_matches('\0').trim().to_string())
    }

    pub fn byte_data<R: Read + Seek>(
        &self,
        tag: TiffTags,
        reader: &mut BitReader<R>,
        limits: &mut Limits,
    ) -> Option<Vec<u8>> {
        let entry = self.entry(tag)?;
        let declared = usize::try_from(entry.count).unwrap_or(usize::MAX);

        if declared <= self.format.inline_capacity() {
            return Some(self.inline_bytes(entry)[..declared].to_vec());
        }

        reader.seek(SeekFrom::Start(entry.value_offset)).ok()?;
        let len = declared.min(usize::try_from(reader.bytes_left().ok()?).unwrap_or(usize::MAX));
        limits.reserve_usize(len).ok()?;
        let data = reader.read_bytes(len);
        limits.free_usize(len);

        data.ok().filter(|data| data.len() == declared)
    }

    pub(crate) fn inline_bytes<'a>(&self, entry: &'a IfdEntry) -> &'a [u8] {
        &entry.inline[..self.format.inline_capacity()]
    }
}

fn conversion_error() -> VexelError {
    VexelError::Custom("Value conversion error".to_string())
}

fn read_scalar<R: Read + Seek>(type_: u16, reader: &mut BitReader<R>) -> VexelResult<u64> {
    Ok(match type_ {
        1 | 2 | 6 | 7 => u64::from(reader.read_u8()?),
        3 | 8 => u64::from(reader.read_u16()?),
        4 | 9 | 13 => u64::from(reader.read_u32()?),
        16..=18 => reader.read_u64()?,
        _ => return Err(VexelError::Custom("Unsupported type".to_string())),
    })
}

fn inline_reader(entry: &IfdEntry, format: IfdFormat) -> BitReader<std::io::Cursor<[u8; MAX_INLINE_BYTES]>> {
    let mut inline = BitReader::new(std::io::Cursor::new(entry.inline));
    inline.set_endianness(format.byte_order);
    inline
}

fn read_single_value<T, R: Read + Seek>(
    entry: &IfdEntry,
    format: IfdFormat,
    reader: &mut BitReader<R>,
) -> VexelResult<T>
where
    T: TryFrom<u64>,
{
    let size = type_size(entry.field_type).unwrap_or(4);
    let value = match entry.field_type {
        5 | 10 | 11 | 12 if size > format.inline_capacity() => {
            reader.seek(SeekFrom::Start(entry.value_offset))?;
            u64::from(reader.read_u32()?)
        }
        5 | 10 | 11 | 12 => u64::from(inline_reader(entry, format).read_u32()?),
        _ if size <= format.inline_capacity() => read_scalar(entry.field_type, &mut inline_reader(entry, format))
            .unwrap_or_else(|_| u64::from(entry.inline[0])),
        _ => {
            reader.seek(SeekFrom::Start(entry.value_offset))?;
            read_scalar(entry.field_type, reader)?
        }
    };

    T::try_from(value).map_err(|_| conversion_error())
}

fn read_multiple_values<T, R: Read + Seek>(
    entry: &IfdEntry,
    format: IfdFormat,
    reader: &mut BitReader<R>,
    limits: &mut Limits,
) -> VexelResult<Vec<T>>
where
    T: TryFrom<u64>,
{
    if entry.count == 1 {
        return Ok(vec![read_single_value(entry, format, reader)?]);
    }

    let type_ = entry.field_type;
    let bytes_per_value = type_size(type_).unwrap_or(4) as u64;
    let total_bytes = bytes_per_value.saturating_mul(entry.count);

    if total_bytes <= format.inline_capacity() as u64 {
        let mut inline = inline_reader(entry, format);
        let mut values = Vec::with_capacity(entry.count as usize);
        for _ in 0..entry.count {
            let value = read_scalar(type_, &mut inline).or_else(|_| inline.read_u8().map(u64::from))?;
            values.push(T::try_from(value).map_err(|_| conversion_error())?);
        }
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(entry.value_offset))?;
    let capacity = entry.count.min(reader.bytes_left()? / bytes_per_value) as usize;
    let vec_bytes = capacity.saturating_mul(std::mem::size_of::<T>());
    limits.reserve_usize(vec_bytes)?;

    let result = (|| {
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..entry.count {
            let value = read_scalar(type_, reader)?;
            values.push(T::try_from(value).map_err(|_| conversion_error())?);
        }
        Ok(values)
    })();

    limits.free_usize(vec_bytes);
    result
}

fn read_rationals<R: Read + Seek>(
    entry: &IfdEntry,
    format: IfdFormat,
    reader: &mut BitReader<R>,
    limits: &mut Limits,
    signed: bool,
) -> VexelResult<Vec<f32>> {
    let to_float = |numerator: u32, denominator: u32| {
        if denominator == 0 {
            0.0
        } else if signed {
            numerator as i32 as f32 / denominator as i32 as f32
        } else {
            numerator as f32 / denominator as f32
        }
    };

    if entry.count.saturating_mul(8) <= format.inline_capacity() as u64 {
        let mut inline = inline_reader(entry, format);
        let mut values = Vec::new();
        for _ in 0..entry.count {
            let numerator = inline.read_u32()?;
            let denominator = inline.read_u32()?;
            values.push(to_float(numerator, denominator));
        }
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(entry.value_offset))?;
    let capacity = entry.count.min(reader.bytes_left()? / 8) as usize;
    let vec_bytes = capacity.saturating_mul(std::mem::size_of::<f32>());
    limits.reserve_usize(vec_bytes)?;
    let result = (|| {
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..entry.count {
            let numerator = reader.read_u32()?;
            let denominator = reader.read_u32()?;
            values.push(to_float(numerator, denominator));
        }
        Ok::<_, VexelError>(values)
    })();
    limits.free_usize(vec_bytes);
    result
}
