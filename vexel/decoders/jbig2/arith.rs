const QE: [(u16, u8, u8, u8); 47] = [
    (0x5601, 1, 1, 1),
    (0x3401, 2, 6, 0),
    (0x1801, 3, 9, 0),
    (0x0AC1, 4, 12, 0),
    (0x0521, 5, 29, 0),
    (0x0221, 38, 33, 0),
    (0x5601, 7, 6, 1),
    (0x5401, 8, 14, 0),
    (0x4801, 9, 14, 0),
    (0x3801, 10, 14, 0),
    (0x3001, 11, 17, 0),
    (0x2401, 12, 18, 0),
    (0x1C01, 13, 20, 0),
    (0x1601, 29, 21, 0),
    (0x5601, 15, 14, 1),
    (0x5401, 16, 14, 0),
    (0x5101, 17, 15, 0),
    (0x4801, 18, 16, 0),
    (0x3801, 19, 17, 0),
    (0x3401, 20, 18, 0),
    (0x3001, 21, 19, 0),
    (0x2801, 22, 19, 0),
    (0x2401, 23, 20, 0),
    (0x2201, 24, 21, 0),
    (0x1C01, 25, 22, 0),
    (0x1801, 26, 23, 0),
    (0x1601, 27, 24, 0),
    (0x1401, 28, 25, 0),
    (0x1201, 29, 26, 0),
    (0x1101, 30, 27, 0),
    (0x0AC1, 31, 28, 0),
    (0x09C1, 32, 29, 0),
    (0x08A1, 33, 30, 0),
    (0x0521, 34, 31, 0),
    (0x0441, 35, 32, 0),
    (0x02A1, 36, 33, 0),
    (0x0221, 37, 34, 0),
    (0x0141, 38, 35, 0),
    (0x0111, 39, 36, 0),
    (0x0085, 40, 37, 0),
    (0x0049, 41, 38, 0),
    (0x0025, 42, 39, 0),
    (0x0015, 43, 40, 0),
    (0x0009, 44, 41, 0),
    (0x0005, 45, 42, 0),
    (0x0001, 45, 43, 0),
    (0x5601, 46, 46, 0),
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Cx(u8);

impl Cx {
    fn index(self) -> usize {
        (self.0 & 0x7f) as usize
    }

    fn mps(self) -> u32 {
        (self.0 >> 7) as u32
    }

    fn switch_mps(&mut self, nmps: u8) {
        self.0 = (self.0 & 0x80) | nmps;
    }

    fn switch_lps(&mut self, nlps: u8, switch: bool) {
        let mps = if switch { (self.0 ^ 0x80) & 0x80 } else { self.0 & 0x80 };
        self.0 = mps | nlps;
    }
}

pub struct ArithDecoder<'a> {
    data: &'a [u8],
    bp: usize,
    c: u32,
    a: u32,
    ct: i32,
}

impl<'a> ArithDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut dec = ArithDecoder { data, bp: 0, c: 0, a: 0, ct: 0 };
        dec.init();
        dec
    }

    fn byte_at(&self, index: usize) -> u8 {
        self.data.get(index).copied().unwrap_or(0xff)
    }

    fn init(&mut self) {
        self.bp = 0;
        self.c = (self.byte_at(0) as u32) << 16;
        self.byte_in();
        self.c <<= 7;
        self.ct -= 7;
        self.a = 0x8000;
    }

    fn byte_in(&mut self) {
        if self.byte_at(self.bp) == 0xff {
            if self.byte_at(self.bp + 1) > 0x8f {
                self.c += 0xff00;
                self.ct = 8;
            } else {
                self.bp += 1;
                self.c += (self.byte_at(self.bp) as u32) << 9;
                self.ct = 7;
            }
        } else {
            self.bp += 1;
            self.c += (self.byte_at(self.bp) as u32) << 8;
            self.ct = 8;
        }
    }

    fn renorm(&mut self) {
        loop {
            if self.ct == 0 {
                self.byte_in();
            }
            self.a <<= 1;
            self.c = self.c.wrapping_shl(1);
            self.ct -= 1;
            if self.a & 0x8000 != 0 {
                break;
            }
        }
    }

    pub fn decode(&mut self, cx: &mut Cx) -> u32 {
        let (qe, nmps, nlps, switch) = QE[cx.index()];
        let qe = qe as u32;

        self.a = self.a.wrapping_sub(qe);

        if (self.c >> 16) < qe {
            let d = if self.a < qe {
                let d = cx.mps();
                cx.switch_mps(nmps);
                d
            } else {
                let d = 1 - cx.mps();
                cx.switch_lps(nlps, switch != 0);
                d
            };
            self.a = qe;
            self.renorm();
            d
        } else {
            self.c -= qe << 16;
            if self.a & 0x8000 != 0 {
                return cx.mps();
            }
            let d = if self.a < qe {
                let d = 1 - cx.mps();
                cx.switch_lps(nlps, switch != 0);
                d
            } else {
                let d = cx.mps();
                cx.switch_mps(nmps);
                d
            };
            self.renorm();
            d
        }
    }
}

pub struct IntCtx {
    cx: Vec<Cx>,
}

impl IntCtx {
    pub fn new() -> Self {
        IntCtx { cx: vec![Cx::default(); 512] }
    }

    fn decode_bit(&mut self, arith: &mut ArithDecoder, prev: &mut usize) -> u32 {
        let d = arith.decode(&mut self.cx[*prev]);
        *prev = if *prev < 256 { (*prev << 1) | d as usize } else { (((*prev << 1) | d as usize) & 511) | 256 };
        d
    }

    pub fn decode(&mut self, arith: &mut ArithDecoder) -> Option<i32> {
        let mut prev = 1usize;

        let s = self.decode_bit(arith, &mut prev);

        let (n_tail, offset) = if self.decode_bit(arith, &mut prev) == 0 {
            (2, 0)
        } else if self.decode_bit(arith, &mut prev) == 0 {
            (4, 4)
        } else if self.decode_bit(arith, &mut prev) == 0 {
            (6, 20)
        } else if self.decode_bit(arith, &mut prev) == 0 {
            (8, 84)
        } else if self.decode_bit(arith, &mut prev) == 0 {
            (12, 340)
        } else {
            (32, 4436)
        };

        let mut v: u32 = 0;
        for _ in 0..n_tail {
            let d = self.decode_bit(arith, &mut prev);
            v = (v << 1) | d;
        }

        let v = v.saturating_add(offset).min(i32::MAX as u32) as i32;

        if s == 1 && v == 0 {
            return None;
        }

        Some(if s == 1 { -v } else { v })
    }
}

pub struct IaidCtx {
    cx: Vec<Cx>,
    sym_code_len: u32,
}

impl IaidCtx {
    pub fn new(sym_code_len: u32) -> Self {
        let sym_code_len = sym_code_len.min(31);
        IaidCtx { cx: vec![Cx::default(); 1usize << sym_code_len], sym_code_len }
    }

    pub fn decode(&mut self, arith: &mut ArithDecoder) -> u32 {
        let mut prev = 1usize;
        for _ in 0..self.sym_code_len {
            let d = arith.decode(&mut self.cx[prev]);
            prev = (prev << 1) | d as usize;
        }
        prev as u32 - (1u32 << self.sym_code_len)
    }
}
