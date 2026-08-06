pub struct XsBitReader<'a> {
    data: &'a [u8],
    bit_pos: u64,
    overrun: bool,
}

impl<'a> XsBitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        XsBitReader { data, bit_pos: 0, overrun: false }
    }

    pub fn total_bits(&self) -> u64 {
        (self.data.len() as u64).saturating_mul(8)
    }

    pub fn consumed_bits(&self) -> u64 {
        self.bit_pos
    }

    pub fn overrun(&self) -> bool {
        self.overrun
    }

    pub fn exhausted(&self) -> bool {
        self.bit_pos >= self.total_bits()
    }

    pub fn bits_left(&self) -> u64 {
        self.total_bits().saturating_sub(self.bit_pos)
    }

    pub fn seek_bits(&mut self, pos: u64) {
        self.bit_pos = pos.min(self.total_bits());
    }

    fn bit_at(&self, pos: u64) -> u64 {
        let byte = (pos >> 3) as usize;
        match self.data.get(byte) {
            Some(&b) => ((b >> (7 - (pos & 7))) & 1) as u64,
            None => 0,
        }
    }

    #[inline]
    fn bits_at(&self, pos: u64, nbits: u32) -> u64 {
        debug_assert!(nbits <= 57);

        let byte = (pos >> 3) as usize;
        let shift = (pos & 7) as u32;

        if byte + 8 <= self.data.len() {
            let word = u64::from_be_bytes(unsafe { *(self.data.as_ptr().add(byte) as *const [u8; 8]) });
            return (word << shift) >> (64 - nbits);
        }

        let mut word = 0u64;
        for i in 0..8usize {
            let b = self.data.get(byte + i).copied().unwrap_or(0);
            word = (word << 8) | u64::from(b);
        }

        (word << shift) >> (64 - nbits)
    }

    #[inline]
    pub fn read(&mut self, nbits: u32) -> u64 {
        if nbits == 0 {
            return 0;
        }

        let nbits = nbits.min(64);
        let total = self.total_bits();

        if nbits <= 57 && self.bit_pos + u64::from(nbits) <= total {
            let value = self.bits_at(self.bit_pos, nbits);
            self.bit_pos += u64::from(nbits);
            return value;
        }

        let mut value = 0u64;

        for _ in 0..nbits {
            if self.bit_pos >= total {
                self.overrun = true;
                value <<= 1;
                continue;
            }

            value = (value << 1) | self.bit_at(self.bit_pos);
            self.bit_pos += 1;
        }

        value
    }

    pub fn peek(&self, nbits: u32) -> u64 {
        let nbits = nbits.min(64);
        let mut value = 0u64;

        for i in 0..u64::from(nbits) {
            let pos = self.bit_pos + i;
            if pos >= self.total_bits() {
                value <<= 1;
                continue;
            }
            value = (value << 1) | self.bit_at(pos);
        }

        value
    }

    pub fn can_peek(&self, nbits: u32) -> bool {
        self.bits_left() >= u64::from(nbits)
    }

    pub fn skip(&mut self, nbits: u64) {
        let target = self.bit_pos.saturating_add(nbits);
        if target > self.total_bits() {
            self.overrun = true;
        }
        self.bit_pos = target.min(self.total_bits());
    }

    pub fn align(&mut self, nbits: u32) {
        if nbits == 0 {
            return;
        }

        let rem = self.bit_pos % u64::from(nbits);
        if rem != 0 {
            self.skip(u64::from(nbits) - rem);
        }
    }

    pub fn read_unary_unsigned(&mut self) -> i32 {
        let total = self.total_bits();

        if self.bit_pos + 57 <= total {
            let word = self.bits_at(self.bit_pos, 57);
            let run = (!word << 7).leading_zeros();

            if run < 57 {
                self.bit_pos += u64::from(run) + 1;
                return run as i32;
            }
        }

        let mut val: i32 = -1;
        loop {
            if self.exhausted() {
                self.overrun = true;
                return val.max(0);
            }

            let bit = self.read(1);
            val += 1;

            if bit == 0 {
                break;
            }

            if val > MAX_UNARY_RUN {
                self.overrun = true;
                break;
            }
        }
        val
    }

    pub fn read_bounded_code(&mut self, min_allowed: i32) -> i32 {
        let trigger = min_allowed.abs();
        let tmp = self.read_unary_unsigned();

        if tmp > 2 * trigger {
            tmp - trigger
        } else {
            let val = (tmp + 1) / 2;
            if tmp % 2 != 0 {
                -val
            } else {
                val
            }
        }
    }
}

const MAX_UNARY_RUN: i32 = 4096;
