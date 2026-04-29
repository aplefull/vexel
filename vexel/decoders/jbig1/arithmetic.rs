static LSZ_TAB: [u16; 113] = [
    0x5a1d, 0x2586, 0x1114, 0x080b, 0x03d8, 0x01da, 0x00e5, 0x006f,
    0x0036, 0x001a, 0x000d, 0x0006, 0x0003, 0x0001, 0x5a7f, 0x3f25,
    0x2cf2, 0x207c, 0x17b9, 0x1182, 0x0cef, 0x09a1, 0x072f, 0x055c,
    0x0406, 0x0303, 0x0240, 0x01b1, 0x0144, 0x00f5, 0x00b7, 0x008a,
    0x0068, 0x004e, 0x003b, 0x002c, 0x5ae1, 0x484c, 0x3a0d, 0x2ef1,
    0x261f, 0x1f33, 0x19a8, 0x1518, 0x1177, 0x0e74, 0x0bfb, 0x09f8,
    0x0861, 0x0706, 0x05cd, 0x04de, 0x040f, 0x0363, 0x02d4, 0x025c,
    0x01f8, 0x01a4, 0x0160, 0x0125, 0x00f6, 0x00cb, 0x00ab, 0x008f,
    0x5b12, 0x4d04, 0x412c, 0x37d8, 0x2fe8, 0x293c, 0x2379, 0x1edf,
    0x1aa9, 0x174e, 0x1424, 0x119c, 0x0f6b, 0x0d51, 0x0bb6, 0x0a40,
    0x5832, 0x4d1c, 0x438e, 0x3bdd, 0x34ee, 0x2eae, 0x299a, 0x2516,
    0x5570, 0x4ca9, 0x44d9, 0x3e22, 0x3824, 0x32b4, 0x2e17, 0x56a8,
    0x4f46, 0x47e5, 0x41cf, 0x3c3d, 0x375e, 0x5231, 0x4c0f, 0x4639,
    0x415e, 0x5627, 0x50e7, 0x4b85, 0x5597, 0x504f, 0x5a10, 0x5522,
    0x59eb,
];

static NMPS_TAB: [u8; 113] = [
     1,   2,   3,   4,   5,   6,   7,   8,
     9,  10,  11,  12,  13,  13,  15,  16,
    17,  18,  19,  20,  21,  22,  23,  24,
    25,  26,  27,  28,  29,  30,  31,  32,
    33,  34,  35,   9,  37,  38,  39,  40,
    41,  42,  43,  44,  45,  46,  47,  48,
    49,  50,  51,  52,  53,  54,  55,  56,
    57,  58,  59,  60,  61,  62,  63,  32,
    65,  66,  67,  68,  69,  70,  71,  72,
    73,  74,  75,  76,  77,  78,  79,  48,
    81,  82,  83,  84,  85,  86,  87,  71,
    89,  90,  91,  92,  93,  94,  86,  96,
    97,  98,  99, 100,  93, 102, 103, 104,
    99, 106, 107, 103, 109, 107, 111, 109,
   111,
];

static NLPS_TAB: [u8; 113] = [
    129,  14,  16,  18,  20,  23,  25,  28,
     30,  33,  35,   9,  10,  12, 143,  36,
     38,  39,  40,  42,  43,  45,  46,  48,
     49,  51,  52,  54,  56,  57,  59,  60,
     62,  63,  32,  33, 165,  64,  65,  67,
     68,  69,  70,  72,  73,  74,  75,  77,
     78,  79,  48,  50,  50,  51,  52,  53,
     54,  55,  56,  57,  58,  59,  61,  61,
    193,  80,  81,  82,  83,  84,  86,  87,
     87,  72,  72,  74,  74,  75,  77,  77,
    208,  88,  89,  90,  91,  92,  93,  86,
    216,  95,  96,  97,  99,  99,  93, 223,
    101, 102, 103, 104,  99, 105, 106, 107,
    103, 233, 108, 109, 110, 111, 238, 112,
    240,
];

pub struct ArithDecoder {
    st: [u8; 4096],
    c: u32,
    a: u32,
    ct: i32,
    startup: bool,
}

impl ArithDecoder {
    pub fn new() -> Self {
        Self {
            st: [0u8; 4096],
            c: 0,
            a: 1,
            ct: 0,
            startup: true,
        }
    }

    pub fn reset(&mut self, reuse_st: bool) {
        if !reuse_st {
            self.st = [0u8; 4096];
        }
        self.c = 0;
        self.a = 1;
        self.ct = 0;
        self.startup = true;
    }

    pub fn decode(&mut self, cx: usize, data: &[u8], pos: &mut usize) -> Option<u8> {
        while self.a < 0x8000 || self.startup {
            while self.ct <= 8 && self.ct >= 0 {
                if *pos >= data.len() {
                    self.ct = -1;
                    break;
                }
                if data[*pos] == 0xff {
                    if *pos + 1 >= data.len() {
                        self.ct = -1;
                        break;
                    }
                    if data[*pos + 1] == 0x00 {
                        self.c |= 0xff << (8 - self.ct);
                        self.ct += 8;
                        *pos += 2;
                    } else {
                        self.ct = -1;
                    }
                } else {
                    self.c |= (data[*pos] as u32) << (8 - self.ct);
                    self.ct += 8;
                    *pos += 1;
                }
            }
            self.c <<= 1;
            self.a <<= 1;
            if self.ct >= 0 {
                self.ct -= 1;
            }
            if self.a == 0x10000 {
                self.startup = false;
            }
        }

        let st = &mut self.st[cx];
        let ss = (*st & 0x7f) as usize;
        let lsz = LSZ_TAB[ss] as u32;

        self.a -= lsz;

        let pix = if (self.c >> 16) < self.a {
            if self.a & 0xffff8000 != 0 {
                *st >> 7
            } else if self.a < lsz {
                let pix = 1 - (*st >> 7);
                *st &= 0x80;
                *st ^= NLPS_TAB[ss];
                pix
            } else {
                let pix = *st >> 7;
                *st &= 0x80;
                *st |= NMPS_TAB[ss];
                pix
            }
        } else {
            self.c -= self.a << 16;
            if self.a < lsz {
                self.a = lsz;
                let pix = *st >> 7;
                *st &= 0x80;
                *st |= NMPS_TAB[ss];
                pix
            } else {
                self.a = lsz;
                let pix = 1 - (*st >> 7);
                *st &= 0x80;
                *st ^= NLPS_TAB[ss];
                pix
            }
        };

        Some(pix)
    }
}
