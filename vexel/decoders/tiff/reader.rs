use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use std::io::{Read, Seek, SeekFrom};

pub fn read_single_value<T, R: Read + Seek>(
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
        4 => value_offset,
        _ => {
            reader.seek(SeekFrom::Start(value_offset as u64))?;
            reader.read_u32()?
        }
    };

    T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))
}

pub fn read_multiple_values<T, R: Read + Seek>(
    type_: u16,
    count: u32,
    value_offset: u32,
    byte_order: ByteOrder,
    reader: &mut BitReader<R>,
) -> VexelResult<Vec<T>>
where
    T: TryFrom<u32>,
{
    let mut values = Vec::with_capacity(count as usize);

    if count == 1 {
        values.push(read_single_value(type_, value_offset, byte_order, reader)?);
        return Ok(values);
    }

    let bytes_per_value: u32 = match type_ {
        1 | 2 => 1,
        3 => 2,
        4 => 4,
        5 | 10 => 8,
        _ => 4,
    };
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
                1 => inline_reader.read_u8()? as u32,
                3 => inline_reader.read_u16()? as u32,
                4 => inline_reader.read_u32()?,
                _ => inline_reader.read_u8()? as u32,
            };
            values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
        }
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(value_offset as u64))?;

    for _ in 0..count {
        let value = match type_ {
            1 => reader.read_u8()? as u32,
            3 => reader.read_u16()? as u32,
            4 => reader.read_u32()?,
            _ => return Err(VexelError::Custom("Unsupported type".to_string())),
        };

        values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
    }

    Ok(values)
}

pub fn read_rational<R: Read + Seek>(value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<f32> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let numerator = reader.read_u32()?;
    let denominator = reader.read_u32()?;

    if denominator == 0 {
        return Err(VexelError::Custom("Division by zero in rational".to_string()));
    }

    Ok(numerator as f32 / denominator as f32)
}

pub fn read_multiple_rationals<R: Read + Seek>(
    count: u32,
    value_offset: u32,
    reader: &mut BitReader<R>,
) -> VexelResult<Vec<f32>> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let numerator = reader.read_u32()?;
        let denominator = reader.read_u32()?;
        if denominator == 0 {
            values.push(0.0);
        } else {
            values.push(numerator as f32 / denominator as f32);
        }
    }
    Ok(values)
}
