use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use crate::utils::marker::Marker;

#[derive(Debug)]
pub struct BitReader<R: Read + Seek> {
    reader: R,
    buffer: u32,
    bits_in_buffer: u8,
}

impl<R: Read + Seek> BitReader<R> {
    pub fn new(reader: R) -> Self {
        BitReader {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
        }
    }

    /// Reads a single bit from the bitstream.
    /// 
    /// # Returns
    /// - `true` if the bit is 1, `false` if the bit is 0
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_bit(&mut self) -> Result<bool, std::io::Error> {
        if self.bits_in_buffer == 0 {
            let mut byte = [0u8; 1];
            self.reader.read_exact(&mut byte)?;
            self.buffer = u32::from(byte[0]);
            self.bits_in_buffer = 8;
        }

        self.bits_in_buffer -= 1;
        Ok(((self.buffer >> self.bits_in_buffer) & 1) != 0)
    }

    /// Reads `n` bits from the bitstream.
    /// 
    /// # Parameters
    /// - `n`: The number of bits to read
    /// 
    /// # Returns
    /// - The value of the bits read
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_bits(&mut self, n: u8) -> Result<u32, std::io::Error> {
        let mut result = 0;
        for _ in 0..n {
            result = (result << 1) | (self.read_bit()? as u32);
        }
        Ok(result)
    }
    
    /// Reads a single byte from the bitstream.
    /// 
    /// # Returns
    /// - The byte read
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        self.read_bits(8).map(|b| b as u8)
    }
    
    /// Reads a single 16-bit value from the bitstream.
    /// 
    /// # Returns
    /// - The 16-bit value read
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_u16(&mut self) -> Result<u16, std::io::Error> {
        self.read_bits(16).map(|b| b as u16)
    }
    
    /// Clears the current bit buffer.
    pub fn clear_buffer(&mut self) {
        self.bits_in_buffer = 0;
        self.buffer = 0;
    }

    /// Searches for a marker in the bitstream.
    /// If marker is found, cursor is positioned right after the marker.
    /// If marker is not found, cursor returns to the start of the bitstream.
    ///
    /// # Parameters
    /// - `marker`: The marker to search for
    ///
    /// # Returns
    /// - `true` if marker is found, `false` otherwise
    /// - `std::io::Error` if an I/O error occurs
    pub fn find_marker<M: Marker>(&mut self, marker: M) -> Result<bool, std::io::Error> {
        let marker = marker.to_u16();
        let mut byte_buffer = [0u8; 2];

        while let Ok(_) = self.reader.read_exact(&mut byte_buffer) {
            let current_value = u16::from_be_bytes(byte_buffer);

            if current_value == marker {
                return Ok(true);
            }

            self.reader.seek(SeekFrom::Current(-1))?;
        }

        self.reader.seek(SeekFrom::Start(0))?;

        Ok(false)
    }

    /// Searches for the next known marker in the bitstream.
    /// If marker is found, cursor is positioned right after the marker.
    /// 
    /// # Parameters
    /// - `known_markers`: A list of known markers to search for
    /// 
    /// # Returns
    /// - `Some(marker)` if a marker is found, `None` otherwise
    /// - `None` if the end of the bitstream is reached
    /// - `std::io::Error` if an I/O error occurs
    pub fn next_marker<M: Marker>(&mut self, known_markers: &[M]) -> Result<Option<M>, std::io::Error> {
        let marker_set: HashSet<u16> = known_markers.iter().map(|m| m.to_u16()).collect();
        let mut buffer = [0u8; 1];

        // TODO - patterns like 0xFFFF11 are probably handled incorrectly and 11 is being skipped
        loop {
            match self.reader.read_exact(&mut buffer) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(e),
            }

            if buffer[0] == 0xFF {
                // Read the next byte
                match self.reader.read_exact(&mut buffer) {
                    Ok(_) => {},
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                    Err(e) => return Err(e),
                }
                
                let marker = u16::from_be_bytes([0xFF, buffer[0]]);
                if marker_set.contains(&marker) {
                    return Ok(M::from_u16(marker));
                }
            }
        }
    }
    
    /// Seeks to a specific position in the bitstream.
    /// 
    /// # Parameters
    /// - `pos`: The position to seek to
    /// 
    /// # Returns
    /// - The new position in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64, std::io::Error> {
        self.reader.seek(pos)
    }
    
    /// Reads the remaining bits in the bitstream and returns them as a vector of bytes,
    /// not including the current buffer. The buffer is cleared after reading.
    /// 
    /// # Returns
    /// - A vector of bytes containing the remaining bits in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_to_end(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = Vec::new();
        
        self.reader.read_to_end(&mut bytes)?;
        self.clear_buffer();
        
        Ok(bytes)
    }
    
    /// Resets the bitreader to the start of the bitstream and clears the buffer.
    /// 
    /// # Returns
    /// - `std::io::Error` if an I/O error occurs
    /// - `Ok(())` if the operation is successful
    pub fn reset(&mut self) -> Result<(), std::io::Error> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.buffer = 0;
        self.bits_in_buffer = 0;
        Ok(())
    }
}
