#[derive(Clone, Copy)]
pub struct JpegBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    bits: u32,
    padding: u32,
    overrun: bool,
}

impl<'a> JpegBitReader<'a> {
    #[inline(always)]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, buf: 0, bits: 0, padding: 0, overrun: false }
    }

    #[inline(always)]
    pub fn refill(&mut self) {
        if self.pos + 8 <= self.data.len() {
            let chunk = u64::from_be_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
            self.buf |= chunk >> self.bits;
            self.pos += ((63 - self.bits) >> 3) as usize;
            self.bits |= 56;
        } else {
            *self = (*self).refill_tail();
        }
    }

    #[cold]
    #[inline(never)]
    fn refill_tail(mut self) -> Self {
        while self.bits <= 56 && self.pos < self.data.len() {
            self.buf |= (self.data[self.pos] as u64) << (56 - self.bits);
            self.pos += 1;
            self.bits += 8;
        }
        if self.bits < 56 {
            if self.bits < self.padding {
                self.overrun = true;
            }
            self.padding = self.padding.min(self.bits) + (56 - self.bits);
            self.bits = 56;
        }
        self
    }

    #[cfg(feature = "rayon")]
    pub fn bit_position(&self) -> u64 {
        (self.pos as u64 * 8 + self.padding as u64).saturating_sub(self.bits as u64)
    }

    #[cfg(feature = "rayon")]
    pub fn seek_bits(&mut self, bit: u64) {
        self.pos = ((bit / 8) as usize).min(self.data.len());
        self.buf = 0;
        self.bits = 0;
        self.padding = 0;
        self.overrun = false;
        self.refill();
        let skip = (bit % 8) as u32;
        if skip > 0 {
            self.consume(skip);
        }
    }

    #[inline(always)]
    pub fn fill_to(&mut self, n: u32) {
        if self.bits < n {
            self.refill();
        }
    }

    #[inline(always)]
    pub fn peek_unchecked(&self, n: u32) -> u32 {
        (self.buf >> (64 - n)) as u32
    }

    #[inline(always)]
    pub fn read_bits_unchecked(&mut self, n: u32) -> u32 {
        let v = (self.buf >> (63 - n) >> 1) as u32;
        self.buf <<= n;
        self.bits -= n;
        v
    }

    #[inline(always)]
    pub fn peek_fast(&mut self) -> Option<u32> {
        const N: u32 = crate::decoders::jpeg::types::HUFF_PEEK_BITS as u32;
        if self.bits < N {
            self.refill();
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
        }
        self.read_bits_unchecked(n)
    }

    #[inline(always)]
    pub fn is_exhausted(&self) -> bool {
        self.overrun || self.bits < self.padding
    }

    #[inline(always)]
    pub fn clear_buffer(&mut self) {
        let real_bits = self.bits.saturating_sub(self.padding);
        self.pos = self.pos.saturating_sub((real_bits / 8) as usize);
        self.bits = 0;
        self.buf = 0;
        self.padding = 0;

        while self.pos + 1 < self.data.len() {
            if self.data[self.pos] == 0xFF && self.data[self.pos + 1] >= 0xD0 && self.data[self.pos + 1] <= 0xD7 {
                self.pos += 2;
                self.overrun = false;
                return;
            }
            self.pos += 1;
        }
        self.pos = self.data.len();
    }
}
