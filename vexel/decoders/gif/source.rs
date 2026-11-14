use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};

const FIRST_CHUNK: usize = 1 << 16;
const MAX_CHUNK: usize = 1 << 24;
const DISCARD_THRESHOLD: usize = 1 << 20;

pub struct GifSource<R: Read + Seek> {
    reader: R,
    buf: Vec<u8>,
    base: u64,
    pos: usize,
    eof: bool,
    retain: bool,
}

impl<R: Read + Seek> GifSource<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, buf: Vec::new(), base: 0, pos: 0, eof: false, retain: true }
    }

    pub fn set_retain(&mut self, retain: bool) {
        self.retain = retain;
    }

    pub fn data(&self) -> &[u8] {
        &self.buf
    }

    pub fn take_data(&mut self) -> Vec<u8> {
        let data = std::mem::take(&mut self.buf);
        self.base += data.len() as u64;
        self.pos = self.pos.saturating_sub(data.len());
        data
    }

    pub fn stream_position(&self) -> u64 {
        self.base + self.pos as u64
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, target: SeekFrom) -> std::io::Result<u64> {
        let SeekFrom::Start(target) = target else {
            return Err(Error::from(ErrorKind::Unsupported));
        };
        if target >= self.base {
            self.pos = (target - self.base) as usize;
            return Ok(target);
        }
        self.reader.seek(SeekFrom::Start(target))?;
        self.buf.clear();
        self.base = target;
        self.pos = 0;
        self.eof = false;
        Ok(target)
    }

    fn fill(&mut self, need: usize) -> bool {
        while self.buf.len() < self.pos.saturating_add(need) {
            if self.eof {
                return false;
            }
            if !self.retain && self.pos > DISCARD_THRESHOLD && self.pos <= self.buf.len() {
                self.buf.drain(..self.pos);
                self.base += self.pos as u64;
                self.pos = 0;
            }
            let missing = self.pos.saturating_add(need) - self.buf.len();
            let chunk = missing.max(self.buf.len().clamp(FIRST_CHUNK, MAX_CHUNK));
            match (&mut self.reader).take(chunk as u64).read_to_end(&mut self.buf) {
                Ok(0) | Err(_) => self.eof = true,
                Ok(_) => {}
            }
        }
        true
    }

    fn exhaust(&mut self) -> Error {
        self.pos = self.pos.max(self.buf.len());
        Error::from(ErrorKind::UnexpectedEof)
    }

    pub fn read_u8(&mut self) -> std::io::Result<u8> {
        if !self.fill(1) {
            return Err(self.exhaust());
        }
        let byte = self.buf[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    pub fn read_u16(&mut self) -> std::io::Result<u16> {
        let mut bytes = [0u8; 2];
        self.read_exact(&mut bytes)?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub fn read_exact(&mut self, dst: &mut [u8]) -> std::io::Result<()> {
        if !self.fill(dst.len()) {
            return Err(self.exhaust());
        }
        dst.copy_from_slice(&self.buf[self.pos..self.pos + dst.len()]);
        self.pos += dst.len();
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> std::io::Result<()> {
        if !self.fill(n) {
            return Err(self.exhaust());
        }
        self.pos += n;
        Ok(())
    }

    pub fn bytes_left(&mut self) -> std::io::Result<u64> {
        let read_end = self.base + self.buf.len() as u64;
        let end = self.reader.seek(SeekFrom::End(0))?;
        self.reader.seek(SeekFrom::Start(read_end))?;
        Ok(end.saturating_sub(self.stream_position()))
    }
}
