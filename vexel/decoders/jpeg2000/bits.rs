pub struct StuffedBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    byte: u8,
    bits: u32,
    fill: u8,
    stopped: bool,
}

impl<'a> StuffedBitReader<'a> {
    pub fn new(data: &'a [u8], fill: u8) -> Self {
        Self { data, pos: 0, byte: 0, bits: 0, fill, stopped: false }
    }

    fn next_byte(&mut self) {
        let after_ff = self.byte == 0xFF;
        match self.data.get(self.pos) {
            Some(&byte) if !self.stopped && !(after_ff && byte > 0x8F) => {
                self.pos += 1;
                self.byte = byte;
                self.bits = if after_ff { 7 } else { 8 };
            }
            _ => {
                self.stopped = true;
                self.byte = self.fill;
                self.bits = 8;
            }
        }
    }

    #[inline]
    pub fn read_bit(&mut self) -> u32 {
        if self.bits == 0 {
            self.next_byte();
        }
        self.bits -= 1;
        u32::from(self.byte >> self.bits) & 1
    }

    pub fn read(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |value, _| (value << 1) | self.read_bit())
    }

    pub fn align(&mut self) {
        if self.byte == 0xFF && !self.stopped {
            self.next_byte();
        }
        self.bits = 0;
    }

    pub fn exhausted(&self) -> bool {
        self.stopped
    }

    pub fn bytes_consumed(&self) -> usize {
        self.pos
    }
}
