
pub struct JlsBitReader {
    data: Vec<u8>,
    pos: usize,
    pub reg: u32,
    bits: i32,
    pub zero_lut: [u8; 256],
}

impl JlsBitReader {
    pub fn new(data: Vec<u8>) -> Self {
        let mut reader = Self {
            data,
            pos: 0,
            reg: 0,
            bits: 24,
            zero_lut: [0u8; 256],
        };
        reader.build_zero_lut();
        reader.init();
        reader
    }

    fn build_zero_lut(&mut self) {
        for i in 0u32..256u32 {
            let mut count = 0u8;
            let mut val = i;
            while count < 8 && (val & 0x80) == 0 {
                count += 1;
                val <<= 1;
            }
            self.zero_lut[i as usize] = count;
        }
    }

    fn get_byte(&mut self) -> u8 {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            b
        } else {
            0xFF
        }
    }

    fn unget_byte(&mut self, b: u8) {
        if self.pos > 0 {
            self.pos -= 1;
            self.data[self.pos] = b;
        }
    }

    fn init(&mut self) {
        self.reg = 0;
        self.bits = 24;
        self.fill_buffer(0);
    }

    pub fn fill_buffer(&mut self, no: i32) {
        self.reg <<= no;
        self.bits += no;

        while self.bits >= 0 {
            let x = self.get_byte();
            if x == 0xFF {
                if self.bits < 8 {
                    self.unget_byte(0xFF);
                    break;
                } else {
                    let x2 = self.get_byte();
                    if (x2 & 0x80) == 0 {
                        self.reg |= (0xFF_u32 << self.bits) | ((x2 as u32 & 0x7F) << (self.bits - 7));
                        self.bits -= 15;
                    } else {
                        self.reg |= (0xFF_u32 << self.bits) | ((x2 as u32) << (self.bits - 8));
                        self.bits -= 16;
                    }
                    continue;
                }
            }
            self.reg |= (x as u32) << self.bits;
            self.bits -= 8;
        }
    }

    pub fn get_bits(&mut self, n: i32) -> u32 {
        let val = self.reg >> (32 - n);
        self.fill_buffer(n);
        val
    }

    pub fn count_leading_zeros(&mut self) -> i32 {
        let mut count = 0i32;
        loop {
            let temp = self.zero_lut[(self.reg >> 24) as usize] as i32;
            count += temp;
            if temp != 8 {
                self.fill_buffer(temp + 1);
                break;
            }
            self.fill_buffer(8);
        }
        count
    }

    pub fn consume_restart_marker(&mut self) {
        let look_back = ((24 - self.bits + 15) / 8 + 4).min(self.pos as i32) as usize;
        let search_start = self.pos.saturating_sub(look_back);

        let mut found = self.pos;
        for i in search_start..self.pos.saturating_sub(1) {
            if self.data[i] == 0xFF && (0xD0..=0xD7).contains(&self.data[i + 1]) {
                found = i + 2;
                break;
            }
        }

        if found == self.pos {
            while found < self.data.len() {
                if self.data[found] == 0xFF && found + 1 < self.data.len() && (0xD0..=0xD7).contains(&self.data[found + 1]) {
                    found += 2;
                    break;
                }
                found += 1;
            }
        }

        self.pos = found;
        self.reg = 0;
        self.bits = 24;
        self.fill_buffer(0);
    }
}
