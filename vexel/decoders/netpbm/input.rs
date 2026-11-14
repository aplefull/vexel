use std::io::{ErrorKind, Read, Seek, SeekFrom};

const BUFFER_SIZE: usize = 1 << 16;

pub struct Input<R: Read + Seek> {
    reader: R,
    buf: Box<[u8]>,
    pos: usize,
    end: usize,
    offset: u64,
    eof: bool,
}

impl<R: Read + Seek> Input<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, buf: vec![0u8; BUFFER_SIZE].into_boxed_slice(), pos: 0, end: 0, offset: 0, eof: false }
    }

    pub fn rewind(&mut self) -> std::io::Result<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.pos = 0;
        self.end = 0;
        self.offset = 0;
        self.eof = false;
        Ok(())
    }

    pub fn position(&self) -> u64 {
        self.offset + self.pos as u64
    }

    pub fn buffered(&self) -> &[u8] {
        &self.buf[self.pos..self.end]
    }

    pub fn consume(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.end);
    }

    pub fn fill(&mut self) -> bool {
        if self.pos < self.end {
            return true;
        }
        if self.eof {
            return false;
        }
        self.offset += self.end as u64;
        self.pos = 0;
        self.end = 0;
        while self.end == 0 {
            match self.reader.read(&mut self.buf) {
                Ok(0) | Err(_) => {
                    self.eof = true;
                    return false;
                }
                Ok(n) => self.end = n,
            }
        }
        true
    }

    pub fn peek(&mut self) -> Option<u8> {
        if self.fill() {
            Some(self.buf[self.pos])
        } else {
            None
        }
    }

    pub fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    pub fn read_into(&mut self, dst: &mut [u8]) -> usize {
        let buffered = (self.end - self.pos).min(dst.len());
        dst[..buffered].copy_from_slice(&self.buf[self.pos..self.pos + buffered]);
        self.pos += buffered;
        let mut filled = buffered;

        while filled < dst.len() && !self.eof {
            match self.reader.read(&mut dst[filled..]) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    filled += n;
                    self.offset += n as u64;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => self.eof = true,
            }
        }

        filled
    }

    pub fn skip(&mut self, n: u64) {
        let buffered = (self.end - self.pos) as u64;
        if n <= buffered {
            self.pos += n as usize;
            return;
        }

        let rest = n - buffered;
        self.offset += self.end as u64;
        self.pos = 0;
        self.end = 0;

        let available = self.stream_remaining().unwrap_or(0);
        let step = rest.min(available);
        if self.reader.seek(SeekFrom::Current(step as i64)).is_ok() {
            self.offset += step;
        }
        if step < rest {
            self.eof = true;
        }
    }

    pub fn remaining(&mut self) -> Option<u64> {
        Some((self.end - self.pos) as u64 + self.stream_remaining()?)
    }

    fn stream_remaining(&mut self) -> Option<u64> {
        let current = self.reader.stream_position().ok()?;
        let end = self.reader.seek(SeekFrom::End(0)).ok()?;
        self.reader.seek(SeekFrom::Start(current)).ok()?;
        Some(end.saturating_sub(current))
    }
}
