use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use crate::utils::marker::Marker;

#[derive(Debug)]
pub struct BitReader<R: Read + Seek> {
    reader: R,
    buffer: u32,
    bits_in_buffer: u8,
    little_endian: bool,
}

impl<R: Read + Seek> BitReader<R> {
    pub fn new(reader: R) -> Self {
        BitReader {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
            little_endian: false,
        }
    }

    /// Creates a new BitReader with little-endian byte order.
    pub fn with_le(reader: R) -> Self {
        BitReader {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
            little_endian: true,
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
        if self.little_endian {
            Ok(((self.buffer >> (7 - self.bits_in_buffer)) & 1) != 0)
        } else {
            Ok(((self.buffer >> self.bits_in_buffer) & 1) != 0)
        }
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
        if self.little_endian {
            for i in 0..n {
                result = result | ((self.read_bit()? as u32) << i);
            }
        } else {
            for _ in 0..n {
                result = (result << 1) | (self.read_bit()? as u32);
            }
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
        if self.little_endian {
            let low = self.read_u8()? as u16;
            let high = self.read_u8()? as u16;
            Ok((high << 8) | low)
        } else {
            let high = self.read_u8()? as u16;
            let low = self.read_u8()? as u16;
            Ok((high << 8) | low)
        }
    }

    /// Reads a single 32-bit value from the bitstream.
    ///
    /// # Returns
    /// - The 32-bit value read
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_u32(&mut self) -> Result<u32, std::io::Error> {
        if self.little_endian {
            let b0 = self.read_u8()? as u32;
            let b1 = self.read_u8()? as u32;
            let b2 = self.read_u8()? as u32;
            let b3 = self.read_u8()? as u32;
            Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
        } else {
            let b0 = self.read_u8()? as u32;
            let b1 = self.read_u8()? as u32;
            let b2 = self.read_u8()? as u32;
            let b3 = self.read_u8()? as u32;
            Ok((b0 << 24) | (b1 << 16) | (b2 << 8) | b3)
        }
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
        let mut buffer = [0u8; 2];
        let mut sliding_window = [0u8; 2];

        // Read first byte for initial sliding window
        match self.reader.read_exact(&mut buffer[..1]) {
            Ok(_) => sliding_window[0] = buffer[0],
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        loop {
            // Read next byte
            match self.reader.read_exact(&mut buffer[..1]) {
                Ok(_) => {
                    // Update sliding window
                    sliding_window[1] = buffer[0];

                    // Check if sliding window matches any of the known markers
                    let potential_marker = u16::from_be_bytes(sliding_window);
                    if marker_set.contains(&potential_marker) {
                        return Ok(M::from_u16(potential_marker));
                    }

                    // Slide window forward
                    sliding_window[0] = sliding_window[1];
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(e),
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

    /// Peeks at the next `n` bytes in the bitstream without consuming them.
    /// The cursor is not moved.
    ///
    /// # Parameters
    /// - `n`: The number of bytes to peek
    ///
    /// # Returns
    /// - A vector of bytes containing the next `n` bytes in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn peek_bytes(&mut self, n: usize) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = vec![0; n];
        self.reader.read_exact(&mut bytes)?;
        self.reader.seek(SeekFrom::Current(-(n as i64)))?;

        Ok(bytes)
    }

    /// Returns number of bytes left in the bitstream.
    /// The cursor is not moved.
    /// 
    /// # Returns
    /// - The number of bytes left in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn bytes_left(&mut self) -> Result<u64, std::io::Error> {
        let current_pos = self.reader.seek(SeekFrom::Current(0))?;
        let end_pos = self.reader.seek(SeekFrom::End(0))?;
        self.reader.seek(SeekFrom::Start(current_pos))?;

        Ok(end_pos - current_pos)
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

    /// Reads specified number of bytes from the bitstream and returns them as a vector of bytes,
    /// not including the current buffer. The buffer is cleared after reading.
    ///
    /// # Parameters
    /// - `n`: The number of bytes to read
    ///
    /// # Returns
    /// - A vector of bytes containing the specified number of bytes in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, std::io::Error> {
        let mut bytes = vec![0; n];
        self.reader.read_exact(&mut bytes)?;
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
