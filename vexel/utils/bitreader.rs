use crate::utils::marker::Marker;
use crate::utils::types::ByteOrder;
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

pub struct BitReader<R: Read + Seek> {
    reader: R,
    buffer: u32,
    bits_in_buffer: u8,
    little_endian: bool,
    pub total_bits_consumed: u64,
}

impl<R: Read + Seek> BitReader<R> {
    pub fn new(reader: R) -> Self {
        BitReader {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
            little_endian: false,
            total_bits_consumed: 0,
        }
    }

    /// Creates a new BitReader with little-endian byte order.
    pub fn with_le(reader: R) -> Self {
        BitReader {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
            little_endian: true,
            total_bits_consumed: 0,
        }
    }

    /// Sets the endianness of the BitReader.
    ///
    /// # Parameters
    /// - `byte_order`: The byte order to set
    ///
    pub fn set_endianness(&mut self, byte_order: ByteOrder) {
        self.little_endian = byte_order == ByteOrder::LittleEndian;
    }

    /// Returns the current position in the bitstream.
    ///
    /// # Returns
    /// - The current position in the bitstream
    /// - `std::io::Error` if an I/O error occurs
    pub fn stream_position(&mut self) -> Result<u64, std::io::Error> {
        self.reader.stream_position()
    }

    /// Reads a single bit from the bitstream.
    ///
    /// # Returns
    /// - `true` if the bit is 1, `false` if the bit is 0
    /// - `std::io::Error` if an I/O error occurs
    #[inline(always)]
    pub fn read_bit(&mut self) -> Result<bool, std::io::Error> {
        if self.bits_in_buffer == 0 {
            let mut byte = [0u8; 1];
            self.reader.read_exact(&mut byte)?;
            self.buffer = u32::from(byte[0]);
            self.bits_in_buffer = 8;
        }

        self.bits_in_buffer -= 1;
        self.total_bits_consumed += 1;
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
    #[inline(always)]
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

    /// Refills the internal bit buffer by reading as many bytes as it can hold.
    /// Attempts a single multi-byte read, falling back to byte-by-byte on partial EOF.
    #[inline(always)]
    fn refill(&mut self) {
        let bytes_needed = (32 - self.bits_in_buffer) / 8;
        let mut buf = [0u8; 4];
        match self.reader.read_exact(&mut buf[..bytes_needed as usize]) {
            Ok(_) => {
                for i in 0..bytes_needed {
                    self.buffer = (self.buffer << 8) | buf[i as usize] as u32;
                    self.bits_in_buffer += 8;
                }
            }
            Err(_) => {
                for _ in 0..bytes_needed {
                    let mut b = [0u8; 1];
                    match self.reader.read_exact(&mut b) {
                        Ok(_) => {
                            self.buffer = (self.buffer << 8) | b[0] as u32;
                            self.bits_in_buffer += 8;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    }

    /// Reads `n` bits from the bitstream without returning an error.
    /// Big-endian only. Returns 0 on EOF or I/O error.
    ///
    /// # Parameters
    /// - `n`: The number of bits to read
    ///
    /// # Returns
    /// - The value of the bits read, or 0 on error
    #[inline(always)]
    pub fn read_bits_unchecked(&mut self, n: u8) -> u32 {
        if self.bits_in_buffer < n {
            self.refill();
        }
        if n == 0 {
            return 0;
        }
        if self.bits_in_buffer < n {
            self.bits_in_buffer = n;
        }
        self.bits_in_buffer -= n;
        self.total_bits_consumed += n as u64;
        (self.buffer >> self.bits_in_buffer) & ((1u32 << n) - 1)
    }

    /// Peeks at the next `n` bits without consuming them.
    /// Big-endian only. Returns `None` on EOF or I/O error.
    ///
    /// # Parameters
    /// - `n`: The number of bits to peek
    ///
    /// # Returns
    /// - The value of the bits, or `None` on error
    #[inline(always)]
    pub fn peek_bits_unchecked(&mut self, n: u8) -> Option<u32> {
        if self.bits_in_buffer < n {
            self.refill();
            if self.bits_in_buffer < n {
                return None;
            }
        }
        if n == 0 {
            return Some(0);
        }
        Some((self.buffer >> (self.bits_in_buffer - n)) & ((1u32 << n) - 1))
    }

    /// Discards `n` bits from the buffer without returning them.
    /// The caller must ensure `n` bits are already in the buffer.
    ///
    /// # Parameters
    /// - `n`: The number of bits to discard
    #[inline(always)]
    pub fn consume_bits(&mut self, n: u8) {
        self.bits_in_buffer -= n;
        self.total_bits_consumed += n as u64;
    }


    /// Reads a single byte from the bitstream.
    ///
    /// # Returns
    /// - The byte read
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        let mut byte = [0u8; 1];
        self.reader.read_exact(&mut byte)?;
        Ok(byte[0])
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

    /// Reads a single 24-bit value from the bitstream.
    /// Since Rust does not have a 24-bit integer type, the value is returned as a 32-bit integer.
    ///
    /// # Returns
    /// - The 24-bit value read, as a 32-bit integer
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_u24(&mut self) -> Result<u32, std::io::Error> {
        if self.little_endian {
            let b0 = self.read_u8()? as u32;
            let b1 = self.read_u8()? as u32;
            let b2 = self.read_u8()? as u32;
            Ok(b0 | (b1 << 8) | (b2 << 16))
        } else {
            let b0 = self.read_u8()? as u32;
            let b1 = self.read_u8()? as u32;
            let b2 = self.read_u8()? as u32;
            Ok((b0 << 16) | (b1 << 8) | b2)
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

    /// Reads the exact number of bytes required to fill the buffer.
    ///
    /// # Returns
    /// - `Ok(())` if the operation is successful
    /// - `std::io::Error` if an I/O error occurs
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), std::io::Error> {
        self.reader.read_exact(buf)
    }

    /// Clears the current bit buffer.
    /// Seeks back any whole bytes that were pre-fetched but not yet consumed.
    #[inline(always)]
    pub fn clear_buffer(&mut self) {
        let whole_bytes = self.bits_in_buffer / 8;
        if whole_bytes > 0 {
            self.reader.seek(SeekFrom::Current(-(whole_bytes as i64))).ok();
        }
        self.bits_in_buffer = 0;
        self.buffer = 0;
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
    // TODO: use this in places where we read bytes and go back
    #[allow(dead_code)]
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
}
