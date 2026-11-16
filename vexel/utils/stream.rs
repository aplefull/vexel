use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use crate::utils::marker::Marker;
use crate::utils::read::{read_vec, read_vec_up_to};
use crate::utils::types::ByteOrder;

pub struct StreamReader<R: Read + Seek> {
    reader: R,
    little_endian: bool,
}

impl<R: Read + Seek> StreamReader<R> {
    pub fn new(reader: R) -> Self {
        StreamReader { reader, little_endian: false }
    }

    pub fn with_le(reader: R) -> Self {
        StreamReader { reader, little_endian: true }
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    pub fn set_endianness(&mut self, byte_order: ByteOrder) {
        self.little_endian = byte_order == ByteOrder::LittleEndian;
    }

    pub fn stream_position(&mut self) -> Result<u64, std::io::Error> {
        self.reader.stream_position()
    }

    pub fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        let mut byte = [0u8; 1];
        self.reader.read_exact(&mut byte)?;
        Ok(byte[0])
    }

    pub fn read_u16(&mut self) -> Result<u16, std::io::Error> {
        let mut bytes = [0u8; 2];
        self.reader.read_exact(&mut bytes)?;
        if self.little_endian {
            Ok(u16::from_le_bytes(bytes))
        } else {
            Ok(u16::from_be_bytes(bytes))
        }
    }

    pub fn read_u32(&mut self) -> Result<u32, std::io::Error> {
        let mut bytes = [0u8; 4];
        self.reader.read_exact(&mut bytes)?;
        if self.little_endian {
            Ok(u32::from_le_bytes(bytes))
        } else {
            Ok(u32::from_be_bytes(bytes))
        }
    }

    pub fn read_u64(&mut self) -> Result<u64, std::io::Error> {
        let mut bytes = [0u8; 8];
        self.reader.read_exact(&mut bytes)?;
        if self.little_endian {
            Ok(u64::from_le_bytes(bytes))
        } else {
            Ok(u64::from_be_bytes(bytes))
        }
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), std::io::Error> {
        self.reader.read_exact(buf)
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.reader.read(buf)
    }

    pub fn next_marker<M: Marker>(&mut self, known_markers: &[M]) -> Result<Option<M>, std::io::Error> {
        let marker_set: HashSet<u16> = known_markers.iter().map(|m| m.to_u16()).collect();
        let mut buffer = [0u8; 2];
        let mut sliding_window = [0u8; 2];

        match self.reader.read_exact(&mut buffer[..1]) {
            Ok(_) => sliding_window[0] = buffer[0],
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        loop {
            match self.reader.read_exact(&mut buffer[..1]) {
                Ok(_) => {
                    sliding_window[1] = buffer[0];

                    let potential_marker = u16::from_be_bytes(sliding_window);
                    if marker_set.contains(&potential_marker) {
                        return Ok(M::from_u16(potential_marker));
                    }

                    sliding_window[0] = sliding_window[1];
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64, std::io::Error> {
        self.reader.seek(pos)
    }

    // TODO: use this in places where we read bytes and go back
    #[allow(dead_code)]
    pub fn peek_bytes(&mut self, n: usize) -> Result<Vec<u8>, std::io::Error> {
        let bytes = read_vec_up_to(&mut self.reader, n)?;
        self.reader.seek(SeekFrom::Current(-(bytes.len() as i64)))?;
        if bytes.len() < n {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }

        Ok(bytes)
    }

    pub fn bytes_left(&mut self) -> Result<u64, std::io::Error> {
        let current_pos = self.reader.stream_position()?;
        let end_pos = self.reader.seek(SeekFrom::End(0))?;
        self.reader.seek(SeekFrom::Start(current_pos))?;

        Ok(end_pos.saturating_sub(current_pos))
    }

    pub fn read_to_end(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let capacity = self.bytes_left().unwrap_or(0) as usize;
        let mut bytes = Vec::with_capacity(capacity);

        self.reader.read_to_end(&mut bytes)?;

        Ok(bytes)
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, std::io::Error> {
        read_vec(&mut self.reader, n)
    }
}
