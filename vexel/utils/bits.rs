pub struct MsbBitReader<'a> {
    data: &'a [u8],
    pos: u64,
    end: u64,
    slack: u64,
    overrun: bool,
}

impl<'a> MsbBitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let end = (data.len() as u64).saturating_mul(8);
        MsbBitReader { data, pos: 0, end, slack: 0, overrun: false }
    }

    pub fn with_slack(mut self, bits: u64) -> Self {
        self.slack = bits;
        self
    }

    pub fn set_limit_bytes(&mut self, end: usize) {
        self.data = &self.data[..end.min(self.data.len())];
        self.end = (self.data.len() as u64).saturating_mul(8);
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    pub fn total_bits(&self) -> u64 {
        self.end
    }

    pub fn bits_left(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    pub fn byte_position(&self) -> usize {
        self.pos.div_ceil(8) as usize
    }

    pub fn exhausted(&self) -> bool {
        self.pos >= self.end
    }

    pub fn overrun(&self) -> bool {
        self.overrun
    }

    pub fn set_overrun(&mut self) {
        self.overrun = true;
    }

    pub fn seek_bits(&mut self, pos: u64) {
        self.pos = pos.min(self.end);
    }

    pub fn seek_bytes(&mut self, pos: usize) {
        self.seek_bits((pos as u64).saturating_mul(8));
    }

    #[inline(always)]
    fn word_at(&self, byte: usize) -> u64 {
        match self.data.get(byte..byte + 8) {
            Some(bytes) => u64::from_be_bytes(bytes.try_into().unwrap()),
            None => {
                let mut bytes = [0u8; 8];
                let tail = self.data.get(byte..).unwrap_or(&[]);
                bytes[..tail.len()].copy_from_slice(tail);
                u64::from_be_bytes(bytes)
            }
        }
    }

    #[inline(always)]
    pub fn peek(&self, n: u32) -> u32 {
        let n = n.min(32);
        if n == 0 {
            return 0;
        }
        let word = self.word_at((self.pos >> 3) as usize) << (self.pos & 7);
        (word >> (64 - n)) as u32
    }

    #[inline]
    pub fn peek_u64(&self, n: u32) -> u64 {
        let n = n.min(64);
        if n == 0 {
            return 0;
        }
        let byte = (self.pos >> 3) as usize;
        let shift = (self.pos & 7) as u32;
        let mut word = self.word_at(byte) << shift;
        if n > 64 - shift {
            word |= u64::from(self.data.get(byte + 8).copied().unwrap_or(0)) >> (8 - shift);
        }
        word >> (64 - n)
    }

    #[inline(always)]
    pub fn read(&mut self, n: u32) -> u32 {
        let n = n.min(32);
        let value = self.peek(n);
        self.skip(u64::from(n));
        value
    }

    #[inline]
    pub fn read_u64(&mut self, n: u32) -> u64 {
        let n = n.min(64);
        let value = self.peek_u64(n);
        self.skip(u64::from(n));
        value
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> bool {
        self.read(1) != 0
    }

    pub fn try_read(&mut self, n: u32) -> Option<u32> {
        if self.bits_left() < u64::from(n) {
            self.overrun = true;
            self.pos = self.end;
            return None;
        }
        Some(self.read(n))
    }

    #[inline(always)]
    pub fn skip(&mut self, n: u64) {
        let target = self.pos.saturating_add(n);
        let limit = self.end.saturating_add(self.slack);
        if target > limit {
            self.overrun = true;
            self.pos = limit;
        } else {
            self.pos = target;
        }
    }

    pub fn align(&mut self, n: u32) {
        let n = u64::from(n.max(1));
        let rem = self.pos % n;
        if rem != 0 {
            self.skip(n - rem);
        }
    }
}
