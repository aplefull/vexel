use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::limits::Limits;
use crate::utils::tiff::types::TiffTags;
use crate::utils::types::ByteOrder;

pub struct TiffFileHeader {
    pub byte_order: ByteOrder,
    pub magic: u16,
    pub first_ifd_offset: u32,
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
    if magic != 42 {
        return Err(VexelError::Custom("Not a TIFF file".to_string()));
    }

    let first_ifd_offset = reader.read_u32()?;

    Ok(TiffFileHeader { byte_order, magic, first_ifd_offset })
}

#[derive(Debug, Clone)]
pub struct IfdCycleGuard {
    visited: HashSet<u32>,
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

    pub fn enter(&mut self, offset: u32) -> bool {
        if offset == 0 || self.remaining == 0 || !self.visited.insert(offset) {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

pub fn type_size(field_type: u16) -> Option<usize> {
    match field_type {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 => Some(8),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct IfdEntry {
    pub tag: u16,
    pub field_type: u16,
    pub count: u32,
    pub value_offset: u32,
}

#[derive(Debug, Clone)]
pub struct Ifd {
    pub offset: u32,
    pub byte_order: ByteOrder,
    pub entries: Vec<IfdEntry>,
    pub next_ifd_offset: u32,
}

impl Ifd {
    pub fn read<R: Read + Seek>(
        reader: &mut BitReader<R>,
        offset: u32,
        byte_order: ByteOrder,
        limits: &mut Limits,
    ) -> VexelResult<Ifd> {
        reader.seek(SeekFrom::Start(offset as u64))?;
        let num_entries = reader.read_u16()? as u64;

        let stream_len = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(offset as u64 + 2))?;
        let available_entries = stream_len.saturating_sub(offset as u64 + 2) / 12;
        let num_entries = num_entries.min(available_entries) as usize;

        let table_bytes = num_entries.saturating_mul(std::mem::size_of::<IfdEntry>());
        limits.reserve_usize(table_bytes)?;

        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let entry = (|| {
                Ok::<IfdEntry, VexelError>(IfdEntry {
                    tag: reader.read_u16()?,
                    field_type: reader.read_u16()?,
                    count: reader.read_u32()?,
                    value_offset: reader.read_u32()?,
                })
            })();

            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => break,
            }
        }
        limits.free_usize(table_bytes);

        let next_ifd_offset = reader.read_u32().unwrap_or(0);

        Ok(Ifd { offset, byte_order, entries, next_ifd_offset })
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
        T: TryFrom<u32>,
        R: Read + Seek,
    {
        let entry = self.entry(tag)?;
        read_single_value(entry.field_type, entry.value_offset, self.byte_order, reader).ok()
    }

    pub fn values<T, R>(&self, tag: TiffTags, reader: &mut BitReader<R>, limits: &mut Limits) -> Option<Vec<T>>
    where
        T: TryFrom<u32>,
        R: Read + Seek,
    {
        let entry = self.entry(tag)?;
        read_multiple_values(entry.field_type, entry.count, entry.value_offset, self.byte_order, reader, limits).ok()
    }

    pub fn rational<R: Read + Seek>(&self, tag: TiffTags, reader: &mut BitReader<R>) -> Option<f32> {
        let entry = self.entry(tag)?;
        read_rational(entry.value_offset, reader).ok()
    }

    pub fn rationals<R: Read + Seek>(
        &self,
        tag: TiffTags,
        reader: &mut BitReader<R>,
        limits: &mut Limits,
    ) -> Option<Vec<f32>> {
        let entry = self.entry(tag)?;
        read_rationals(entry.count, entry.value_offset, reader, limits, false).ok()
    }

    pub fn signed_rationals<R: Read + Seek>(
        &self,
        tag: TiffTags,
        reader: &mut BitReader<R>,
        limits: &mut Limits,
    ) -> Option<Vec<f32>> {
        let entry = self.entry(tag)?;
        read_rationals(entry.count, entry.value_offset, reader, limits, true).ok()
    }

    pub fn string<R: Read + Seek>(&self, tag: TiffTags, reader: &mut BitReader<R>) -> Option<String> {
        let entry = self.entry(tag)?;
        let count = (entry.count as usize).min(256);

        let bytes = if count <= 4 {
            self.inline_bytes(entry)[..count].to_vec()
        } else {
            let mut buf = vec![0u8; count];
            if reader.seek(SeekFrom::Start(entry.value_offset as u64)).is_err() || reader.read_exact(&mut buf).is_err()
            {
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
        let len = entry.count as usize;

        if len <= 4 {
            return Some(self.inline_bytes(entry)[..len].to_vec());
        }

        limits.reserve_usize(len).ok()?;
        let mut data = vec![0u8; len];
        let failed =
            reader.seek(SeekFrom::Start(entry.value_offset as u64)).is_err() || reader.read_exact(&mut data).is_err();
        limits.free_usize(len);

        if failed {
            None
        } else {
            Some(data)
        }
    }

    pub(crate) fn inline_bytes(&self, entry: &IfdEntry) -> [u8; 4] {
        match self.byte_order {
            ByteOrder::LittleEndian => entry.value_offset.to_le_bytes(),
            ByteOrder::BigEndian => entry.value_offset.to_be_bytes(),
        }
    }
}

fn read_single_value<T, R: Read + Seek>(
    type_: u16,
    value_offset: u32,
    byte_order: ByteOrder,
    reader: &mut BitReader<R>,
) -> VexelResult<T>
where
    T: TryFrom<u32>,
{
    let value = match type_ {
        1 => match byte_order {
            ByteOrder::BigEndian => (value_offset >> 24) & 0xFF,
            ByteOrder::LittleEndian => value_offset & 0xFF,
        },
        3 => match byte_order {
            ByteOrder::BigEndian => (value_offset >> 16) & 0xFFFF,
            ByteOrder::LittleEndian => value_offset & 0xFFFF,
        },
        4 | 13 => value_offset,
        _ => {
            reader.seek(SeekFrom::Start(value_offset as u64))?;
            reader.read_u32()?
        }
    };

    T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))
}

fn read_multiple_values<T, R: Read + Seek>(
    type_: u16,
    count: u32,
    value_offset: u32,
    byte_order: ByteOrder,
    reader: &mut BitReader<R>,
    limits: &mut Limits,
) -> VexelResult<Vec<T>>
where
    T: TryFrom<u32>,
{
    let item_size = std::mem::size_of::<T>();
    let vec_bytes = (count as usize).saturating_mul(item_size);
    limits.reserve_usize(vec_bytes)?;
    let mut values = Vec::with_capacity(count as usize);

    if count == 1 {
        values.push(read_single_value(type_, value_offset, byte_order, reader)?);
        limits.free_usize(vec_bytes);
        return Ok(values);
    }

    let bytes_per_value = type_size(type_).unwrap_or(4) as u32;
    let total_bytes = bytes_per_value * count;

    if total_bytes <= 4 {
        let val_bytes = match byte_order {
            ByteOrder::LittleEndian => value_offset.to_le_bytes(),
            ByteOrder::BigEndian => value_offset.to_be_bytes(),
        };
        let mut cursor = std::io::Cursor::new(val_bytes);
        let mut inline_reader = BitReader::new(&mut cursor);
        inline_reader.set_endianness(byte_order);
        for _ in 0..count {
            let value = match type_ {
                1 | 2 | 6 | 7 => inline_reader.read_u8()? as u32,
                3 | 8 => inline_reader.read_u16()? as u32,
                4 | 9 | 13 => inline_reader.read_u32()?,
                _ => inline_reader.read_u8()? as u32,
            };
            values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
        }
        limits.free_usize(vec_bytes);
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(value_offset as u64))?;

    for _ in 0..count {
        let value = match type_ {
            1 | 2 | 6 | 7 => reader.read_u8()? as u32,
            3 | 8 => reader.read_u16()? as u32,
            4 | 9 | 13 => reader.read_u32()?,
            _ => return Err(VexelError::Custom("Unsupported type".to_string())),
        };

        values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
    }

    limits.free_usize(vec_bytes);
    Ok(values)
}

fn read_rational<R: Read + Seek>(value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<f32> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let numerator = reader.read_u32()?;
    let denominator = reader.read_u32()?;

    if denominator == 0 {
        return Err(VexelError::Custom("Division by zero in rational".to_string()));
    }

    Ok(numerator as f32 / denominator as f32)
}

fn read_rationals<R: Read + Seek>(
    count: u32,
    value_offset: u32,
    reader: &mut BitReader<R>,
    limits: &mut Limits,
    signed: bool,
) -> VexelResult<Vec<f32>> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let vec_bytes = (count as usize).saturating_mul(std::mem::size_of::<f32>());
    limits.reserve_usize(vec_bytes)?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let numerator = reader.read_u32()?;
        let denominator = reader.read_u32()?;
        if denominator == 0 {
            values.push(0.0);
        } else if signed {
            values.push(numerator as i32 as f32 / denominator as i32 as f32);
        } else {
            values.push(numerator as f32 / denominator as f32);
        }
    }
    limits.free_usize(vec_bytes);
    Ok(values)
}
