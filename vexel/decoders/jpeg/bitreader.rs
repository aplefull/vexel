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
        if self.bits >= 56 {
            return;
        }

        if self.pos + 8 <= self.data.len() {
            let chunk = u64::from_be_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
            self.buf |= chunk >> self.bits;
            let bytes = (63 - self.bits) >> 3;
            self.pos += bytes as usize;
            self.bits |= 56;
        } else {
            while self.bits <= 56 && self.pos < self.data.len() {
                self.buf |= (self.data[self.pos] as u64) << (56 - self.bits);
                self.pos += 1;
                self.bits += 8;
            }
        }
    }

    #[inline(always)]
    pub fn ensure(&mut self, n: u32) -> bool {
        if self.bits < n {
            self.refill();
        }
        self.bits >= n
    }

    #[inline(always)]
    pub fn peek_unchecked(&self, n: u32) -> u32 {
        (self.buf >> (64 - n)) as u32
    }

    #[inline(always)]
    pub fn read_bits_unchecked(&mut self, n: u32) -> u32 {
        let v = (self.buf >> (64 - n)) as u32;
        self.buf <<= n;
        self.bits -= n;
        v
    }

    #[inline(always)]
    pub fn peek_fast(&mut self) -> Option<u32> {
        const N: u32 = crate::decoders::jpeg::types::HUFF_PEEK_BITS as u32;
        if self.bits < N {
            self.refill();
            if self.bits < N {
                return None;
            }
        }
        Some((self.buf >> (64 - N)) as u32)
    }

    #[inline(always)]
    pub fn consume(&mut self, n: u32) {
        self.buf <<= n;
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
        let v = (self.buf >> (64 - n)) as u32;
        self.buf <<= n;
        self.bits -= n;
        v
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
