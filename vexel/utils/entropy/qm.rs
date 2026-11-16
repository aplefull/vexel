pub const FIXED_STATE: u8 = 113;

const EXHAUSTED_PADDING_BYTES: u32 = 4;

#[rustfmt::skip]
const QM_STATES: [(u32, u8, u8, u8); 114] = [
    (0x5a1d,  1,  1, 1), (0x2586, 14,  2, 0), (0x1114, 16,  3, 0), (0x080b, 18,  4, 0),
    (0x03d8, 20,  5, 0), (0x01da, 23,  6, 0), (0x00e5, 25,  7, 0), (0x006f, 28,  8, 0),
    (0x0036, 30,  9, 0), (0x001a, 33, 10, 0), (0x000d, 35, 11, 0), (0x0006,  9, 12, 0),
    (0x0003, 10, 13, 0), (0x0001, 12, 13, 0), (0x5a7f, 15, 15, 1), (0x3f25, 36, 16, 0),
    (0x2cf2, 38, 17, 0), (0x207c, 39, 18, 0), (0x17b9, 40, 19, 0), (0x1182, 42, 20, 0),
    (0x0cef, 43, 21, 0), (0x09a1, 45, 22, 0), (0x072f, 46, 23, 0), (0x055c, 48, 24, 0),
    (0x0406, 49, 25, 0), (0x0303, 51, 26, 0), (0x0240, 52, 27, 0), (0x01b1, 54, 28, 0),
    (0x0144, 56, 29, 0), (0x00f5, 57, 30, 0), (0x00b7, 59, 31, 0), (0x008a, 60, 32, 0),
    (0x0068, 62, 33, 0), (0x004e, 63, 34, 0), (0x003b, 32, 35, 0), (0x002c, 33,  9, 0),
    (0x5ae1, 37, 37, 1), (0x484c, 64, 38, 0), (0x3a0d, 65, 39, 0), (0x2ef1, 67, 40, 0),
    (0x261f, 68, 41, 0), (0x1f33, 69, 42, 0), (0x19a8, 70, 43, 0), (0x1518, 72, 44, 0),
    (0x1177, 73, 45, 0), (0x0e74, 74, 46, 0), (0x0bfb, 75, 47, 0), (0x09f8, 77, 48, 0),
    (0x0861, 78, 49, 0), (0x0706, 79, 50, 0), (0x05cd, 48, 51, 0), (0x04de, 50, 52, 0),
    (0x040f, 50, 53, 0), (0x0363, 51, 54, 0), (0x02d4, 52, 55, 0), (0x025c, 53, 56, 0),
    (0x01f8, 54, 57, 0), (0x01a4, 55, 58, 0), (0x0160, 56, 59, 0), (0x0125, 57, 60, 0),
    (0x00f6, 58, 61, 0), (0x00cb, 59, 62, 0), (0x00ab, 61, 63, 0), (0x008f, 61, 32, 0),
    (0x5b12, 65, 65, 1), (0x4d04, 80, 66, 0), (0x412c, 81, 67, 0), (0x37d8, 82, 68, 0),
    (0x2fe8, 83, 69, 0), (0x293c, 84, 70, 0), (0x2379, 86, 71, 0), (0x1edf, 87, 72, 0),
    (0x1aa9, 87, 73, 0), (0x174e, 72, 74, 0), (0x1424, 72, 75, 0), (0x119c, 74, 76, 0),
    (0x0f6b, 74, 77, 0), (0x0d51, 75, 78, 0), (0x0bb6, 77, 79, 0), (0x0a40, 77, 48, 0),
    (0x5832, 80, 81, 1), (0x4d1c, 88, 82, 0), (0x438e, 89, 83, 0), (0x3bdd, 90, 84, 0),
    (0x34ee, 91, 85, 0), (0x2eae, 92, 86, 0), (0x299a, 93, 87, 0), (0x2516, 86, 71, 0),
    (0x5570, 88, 89, 1), (0x4ca9, 95, 90, 0), (0x44d9, 96, 91, 0), (0x3e22, 97, 92, 0),
    (0x3824, 99, 93, 0), (0x32b4, 99, 94, 0), (0x2e17, 93, 86, 0), (0x56a8, 95, 96, 1),
    (0x4f46,101, 97, 0), (0x47e5,102, 98, 0), (0x41cf,103, 99, 0), (0x3c3d,104,100, 0),
    (0x375e, 99, 93, 0), (0x5231,105,102, 0), (0x4c0f,106,103, 0), (0x4639,107,104, 0),
    (0x415e,103, 99, 0), (0x5627,105,106, 1), (0x50e7,108,107, 0), (0x4b85,109,103, 0),
    (0x5597,110,109, 0), (0x504f,111,107, 0), (0x5a10,110,111, 1), (0x5522,112,109, 0),
    (0x59eb,112,111, 1), (0x5a1d,113,113, 0),
];

pub struct QmDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    c: u32,
    a: u32,
    ct: i32,
    overrun: u32,
}

impl<'a> QmDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, c: 0, a: 0, ct: -16, overrun: 0 }
    }

    pub fn is_exhausted(&self) -> bool {
        self.overrun > EXHAUSTED_PADDING_BYTES
    }

    pub fn reset(&mut self) {
        self.c = 0;
        self.a = 0;
        self.ct = -16;
    }

    #[inline]
    fn next_byte(&mut self) -> u32 {
        match self.data.get(self.pos) {
            Some(&byte) => {
                self.pos += 1;
                u32::from(byte)
            }
            None => {
                self.overrun = self.overrun.saturating_add(1);
                0
            }
        }
    }

    #[inline]
    fn read_byte(&mut self) -> u32 {
        match self.next_byte() {
            0xFF if self.next_byte() == 0 => 0xFF,
            0xFF => 0,
            byte => byte,
        }
    }

    #[inline]
    pub fn decode(&mut self, st: &mut u8) -> u8 {
        while self.a < 0x8000 {
            self.ct -= 1;
            if self.ct < 0 {
                let data = self.read_byte();
                self.c = (self.c << 8) | data;
                self.ct += 8;
                if self.ct < 0 {
                    self.ct += 1;
                    if self.ct == 0 {
                        self.a = 0x8000;
                    }
                }
            }
            self.a <<= 1;
        }

        let sv = *st;
        let (qe, nl, nm, switch_mps) = QM_STATES[(sv & 0x7F) as usize];
        let mps = sv >> 7;

        self.a = self.a.wrapping_sub(qe);
        let shifted = self.a << (self.ct as u32);

        if self.c >= shifted {
            self.c -= shifted;
            if self.a < qe {
                self.a = qe;
                *st = (sv & 0x80) ^ nm;
                mps
            } else {
                self.a = qe;
                *st = (sv & 0x80) ^ nl ^ (switch_mps << 7);
                1 - mps
            }
        } else if self.a < 0x8000 {
            if self.a < qe {
                *st = (sv & 0x80) ^ nl ^ (switch_mps << 7);
                1 - mps
            } else {
                *st = (sv & 0x80) ^ nm;
                mps
            }
        } else {
            mps
        }
    }
}
