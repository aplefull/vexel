pub struct ForwardBits<'a> {
    data: &'a [u8],
    bit_pos: usize,
    truncated: bool,
}

impl<'a> ForwardBits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        ForwardBits { data, bit_pos: 0, truncated: false }
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub fn read(&mut self, count: u32) -> u64 {
        if count == 0 {
            return 0;
        }

        let count = count.min(64);
        let mut result = 0u64;

        for index in 0..count as usize {
            let bit_pos = self.bit_pos + index;
            let byte = match self.data.get(bit_pos / 8) {
                Some(&byte) => byte,
                None => {
                    self.truncated = true;
                    0
                }
            };

            result |= u64::from((byte >> (bit_pos % 8)) & 1) << index;
        }

        self.bit_pos += count as usize;
        result
    }

    pub fn skip_bytes(&mut self, count: usize) {
        self.bit_pos += count * 8;
    }

    pub fn rewind(&mut self, count: usize) {
        self.bit_pos = self.bit_pos.saturating_sub(count);
    }

    pub fn align(&mut self) {
        self.bit_pos = self.bit_pos.div_ceil(8) * 8;
    }

    pub fn bytes_consumed(&self) -> usize {
        self.bit_pos.div_ceil(8)
    }
}

pub struct BackwardBits<'a> {
    data: &'a [u8],
    bit_offset: i64,
}

impl<'a> BackwardBits<'a> {
    pub fn new(data: &'a [u8]) -> Option<Self> {
        let last = *data.last()?;
        if last == 0 {
            return None;
        }

        let padding = 8 - (7 - last.leading_zeros() as i64);
        let bit_offset = data.len() as i64 * 8 - padding;

        Some(BackwardBits { data, bit_offset })
    }

    pub fn offset(&self) -> i64 {
        self.bit_offset
    }

    pub fn is_exhausted(&self) -> bool {
        self.bit_offset < 0
    }

    pub fn read(&mut self, count: u32) -> u64 {
        self.bit_offset -= i64::from(count);

        let (start, bits) = if self.bit_offset < 0 {
            (0i64, i64::from(count) + self.bit_offset)
        } else {
            (self.bit_offset, i64::from(count))
        };

        if bits <= 0 {
            return 0;
        }

        let mut result = 0u64;

        for index in 0..bits {
            let bit_pos = (start + index) as usize;
            let byte = self.data.get(bit_pos / 8).copied().unwrap_or(0);
            result |= u64::from((byte >> (bit_pos % 8)) & 1) << index;
        }

        if self.bit_offset < 0 {
            let shift = -self.bit_offset;
            result = if shift >= 64 { 0 } else { result << shift };
        }

        result
    }
}
