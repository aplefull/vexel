use super::reader::ByteReader;
use super::types::LercDataType;
use crate::utils::error::VexelResult;

pub struct ValueBuffer {
    values: Vec<f64>,
    dt: LercDataType,
}

impl ValueBuffer {
    pub fn new(len: usize, dt: LercDataType) -> Self {
        ValueBuffer { values: vec![0.0; len], dt }
    }

    pub fn into_values(self) -> Vec<f64> {
        self.values
    }

    pub fn get(&self, index: usize) -> f64 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, index: usize, value: f64) {
        if let Some(slot) = self.values.get_mut(index) {
            *slot = self.dt.clamp(value);
        }
    }

    pub fn set_raw(&mut self, index: usize, value: f64) {
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
        }
    }
}

pub fn read_value(reader: &mut ByteReader, dt: LercDataType) -> VexelResult<f64> {
    let value = match dt {
        LercDataType::Char => reader.read_u8()? as i8 as f64,
        LercDataType::Byte => reader.read_u8()? as f64,
        LercDataType::Short => {
            let bytes = reader.take(2)?;
            i16::from_le_bytes([bytes[0], bytes[1]]) as f64
        }
        LercDataType::UShort => {
            let bytes = reader.take(2)?;
            u16::from_le_bytes([bytes[0], bytes[1]]) as f64
        }
        LercDataType::Int => reader.read_i32()? as f64,
        LercDataType::UInt => reader.read_u32()? as f64,
        LercDataType::Float => {
            let bytes = reader.take(4)?;
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
        }
        LercDataType::Double => reader.read_f64()?,
    };

    Ok(value)
}
