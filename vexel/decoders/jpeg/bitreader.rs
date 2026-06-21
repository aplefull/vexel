pub struct JpegBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    bits: u32,
}

impl<'a> JpegBitReader<'a> {
    #[inline(always)]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, buf: 0, bits: 0 }
    }

    #[inline(always)]
    fn refill(&mut self) {
        while self.bits <= 56 && self.pos < self.data.len() {
            self.buf = (self.buf << 8) | self.data[self.pos] as u64;
            self.pos += 1;
            self.bits += 8;
        }
    }

    #[inline(always)]
    pub fn peek9(&mut self) -> Option<u32> {
        if self.bits < 9 {
            self.refill();
            if self.bits < 9 {
                return None;
            }
        }
        Some(((self.buf >> (self.bits - 9)) & 0x1FF) as u32)
    }

    #[inline(always)]
    pub fn consume(&mut self, n: u32) {
        self.bits -= n;
    }

    #[inline(always)]
    pub fn read_bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        if self.bits < n {
            self.refill();
            if self.bits < n {
                self.bits = n;
            }
        }
        self.bits -= n;
        ((self.buf >> self.bits) & ((1u64 << n) - 1)) as u32
    }

    #[inline(always)]
    pub fn clear_buffer(&mut self) {
        let whole_bytes = (self.bits / 8) as usize;
        self.pos = self.pos.saturating_sub(whole_bytes);
        self.bits = 0;
        self.buf = 0;

        while self.pos + 1 < self.data.len() {
            if self.data[self.pos] == 0xFF && self.data[self.pos + 1] >= 0xD0 && self.data[self.pos + 1] <= 0xD7 {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }
}
