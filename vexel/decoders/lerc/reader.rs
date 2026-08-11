use crate::utils::error::{VexelError, VexelResult};

pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        ByteReader { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        ByteReader { data, pos: pos.min(data.len()) }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.data.len());
    }

    pub fn skip(&mut self, count: usize) {
        self.pos = self.pos.saturating_add(count).min(self.data.len());
    }

    pub fn peek(&self, count: usize) -> &'a [u8] {
        let end = self.pos.saturating_add(count).min(self.data.len());
        &self.data[self.pos.min(self.data.len())..end]
    }

    pub fn tail(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    pub fn take(&mut self, count: usize) -> VexelResult<&'a [u8]> {
        if self.remaining() < count {
            return Err(VexelError::Custom(format!(
                "LERC blob truncated: needed {} bytes, {} left",
                count,
                self.remaining()
            )));
        }

        let start = self.pos;
        self.pos += count;
        Ok(&self.data[start..self.pos])
    }

    pub fn read_u8(&mut self) -> VexelResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn read_i32(&mut self) -> VexelResult<i32> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_u32(&mut self) -> VexelResult<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_f64(&mut self) -> VexelResult<f64> {
        let bytes = self.take(8)?;
        Ok(f64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
    }

    pub fn read_uint_sized(&mut self, num_bytes: usize) -> VexelResult<u32> {
        match num_bytes {
            1 => Ok(self.read_u8()? as u32),
            2 => {
                let bytes = self.take(2)?;
                Ok(u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
            }
            4 => self.read_u32(),
            _ => Err(VexelError::Custom(format!("Invalid LERC uint width {}", num_bytes))),
        }
    }
}
