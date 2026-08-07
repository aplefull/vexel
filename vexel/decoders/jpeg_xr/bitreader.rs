pub const TRAILING_SLACK_BITS: u64 = 128;

pub struct XrBitReader<'a> {
    data: &'a [u8],
    bit_pos: u64,
    limit_bits: u64,
    overrun: bool,
}

impl<'a> XrBitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let limit_bits = (data.len() as u64).saturating_mul(8);
        XrBitReader { data, bit_pos: 0, limit_bits, overrun: false }
    }

    pub fn set_limit_bytes(&mut self, end: usize) {
        let end = end.min(self.data.len());
        self.limit_bits = (end as u64).saturating_mul(8);
    }

    pub fn total_bits(&self) -> u64 {
        self.limit_bits
    }

    pub fn byte_pos(&self) -> usize {
        ((self.bit_pos + 7) >> 3) as usize
    }

    pub fn overrun(&self) -> bool {
        self.overrun
    }

    pub fn exhausted(&self) -> bool {
        self.bit_pos >= self.total_bits()
    }

    pub fn seek_bits(&mut self, pos: u64) {
        self.bit_pos = pos.min(self.total_bits());
    }

    pub fn seek_bytes(&mut self, pos: usize) {
        self.seek_bits((pos as u64).saturating_mul(8));
    }

    #[inline]
    fn bits_at(&self, pos: u64, nbits: u32) -> u32 {
        if nbits == 0 {
            return 0;
        }

        let byte = (pos >> 3) as usize;
        let shift = (pos & 7) as u32;
        let end = (self.limit_bits >> 3) as usize;

        let word = if byte + 8 <= end {
            u64::from_be_bytes(self.data[byte..byte + 8].try_into().unwrap())
        } else {
            let mut word = 0u64;
            for i in 0..8usize {
                let index = byte + i;
                let b = if index < end { self.data[index] } else { 0 };
                word = (word << 8) | u64::from(b);
            }
            word
        };

        ((word << shift) >> (64 - nbits)) as u32
    }

    #[inline]
    pub fn peek(&self, nbits: u32) -> u32 {
        self.bits_at(self.bit_pos, nbits.min(32))
    }

    #[inline]
    pub fn read(&mut self, nbits: u32) -> u32 {
        if nbits == 0 {
            return 0;
        }

        let nbits = nbits.min(32);
        let value = self.bits_at(self.bit_pos, nbits);
        self.skip(u64::from(nbits));
        value
    }

    #[inline]
    pub fn read_bool(&mut self) -> bool {
        self.read(1) != 0
    }

    pub fn read_i32(&mut self, nbits: u32) -> i32 {
        self.read(nbits) as i32
    }

    #[inline]
    pub fn skip(&mut self, nbits: u64) {
        let target = self.bit_pos.saturating_add(nbits);
        let slack = self.total_bits().saturating_add(TRAILING_SLACK_BITS);

        if target > slack {
            self.overrun = true;
        }

        self.bit_pos = target.min(slack);
    }

    pub fn align_byte(&mut self) {
        let rem = self.bit_pos & 7;
        if rem != 0 {
            self.skip(8 - rem);
        }
    }

    pub fn read_sign(&mut self) -> i32 {
        if self.read_bool() {
            -1
        } else {
            0
        }
    }

    pub fn read_zero_or_signed(&mut self, nbits: u32) -> i32 {
        let raw = self.peek(nbits + 1) as i32;
        let value = ((raw >> 1) ^ -(raw & 1)) + (raw & 1);
        self.skip(u64::from(nbits) + u64::from(value != 0));
        value
    }

    pub fn read_vl_word_esc(&mut self) -> (u64, Option<u32>) {
        let first = self.read(8);

        if first >= 0xfd {
            return (0, Some(first));
        }

        if first < 0xfb {
            let low = self.read(8);
            return ((u64::from(first) << 8) | u64::from(low), None);
        }

        let wide = first - 0xfb;
        let mut value = 0u64;

        if wide != 0 {
            value = u64::from(self.read(16)) << 16;
            value = (value | u64::from(self.read(16))) << 16;
            value <<= 16;
        }

        value |= u64::from(self.read(16)) << 16;
        value |= u64::from(self.read(16));

        (value, None)
    }
}
