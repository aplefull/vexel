pub const BIT_MODEL_TOTAL_BITS: u32 = 11;
pub const BIT_MODEL_TOTAL: u16 = 1 << BIT_MODEL_TOTAL_BITS;
pub const PROB_INIT: u16 = BIT_MODEL_TOTAL / 2;

const MOVE_BITS: u32 = 5;
const TOP_VALUE: u32 = 1 << 24;
const SHIFT_BITS: u32 = 8;

pub struct RangeDecoder<'a> {
    input: &'a [u8],
    pos: usize,
    range: u32,
    code: u32,
    truncated: bool,
}

impl<'a> RangeDecoder<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        let mut rc = RangeDecoder { input, pos: 0, range: u32::MAX, code: 0, truncated: false };
        rc.reset();
        rc
    }

    pub fn reset(&mut self) {
        self.range = u32::MAX;
        self.code = 0;
        for _ in 0..5 {
            let byte = self.next_byte();
            self.code = (self.code << 8) | u32::from(byte);
        }
    }

    fn next_byte(&mut self) -> u8 {
        match self.input.get(self.pos) {
            Some(&byte) => {
                self.pos += 1;
                byte
            }
            None => {
                self.truncated = true;
                0
            }
        }
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    fn normalize(&mut self) {
        if self.range < TOP_VALUE {
            let byte = self.next_byte();
            self.range <<= SHIFT_BITS;
            self.code = (self.code << SHIFT_BITS) | u32::from(byte);
        }
    }

    pub fn decode_bit(&mut self, prob: &mut u16) -> u32 {
        self.normalize();

        let bound = (self.range >> BIT_MODEL_TOTAL_BITS) * u32::from(*prob);

        if self.code < bound {
            self.range = bound;
            *prob += (BIT_MODEL_TOTAL - *prob) >> MOVE_BITS;
            0
        } else {
            self.range -= bound;
            self.code -= bound;
            *prob -= *prob >> MOVE_BITS;
            1
        }
    }

    pub fn decode_direct_bits(&mut self, count: u32) -> u32 {
        let mut result = 0u32;

        for _ in 0..count {
            self.normalize();
            self.range >>= 1;
            self.code = self.code.wrapping_sub(self.range);
            let mask = 0u32.wrapping_sub(self.code >> 31);
            self.code = self.code.wrapping_add(self.range & mask);
            result = (result << 1).wrapping_add(mask.wrapping_add(1));
        }

        result
    }

    pub fn decode_bittree(&mut self, probs: &mut [u16]) -> u32 {
        let mut symbol = 1usize;

        while symbol < probs.len() {
            let bit = self.decode_bit(&mut probs[symbol]);
            symbol = (symbol << 1) | bit as usize;
        }

        symbol as u32 - probs.len() as u32
    }

    pub fn decode_bittree_reverse(&mut self, probs: &mut [u16], count: u32) -> u32 {
        let mut symbol = 1usize;
        let mut result = 0u32;

        for offset in 0..count {
            let bit = self.decode_bit(&mut probs[symbol]);
            symbol = (symbol << 1) | bit as usize;
            result |= bit << offset;
        }

        result
    }
}
