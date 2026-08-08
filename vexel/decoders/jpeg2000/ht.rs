use crate::decoders::jpeg2000::ht_luts::{VLC_TABLE_0, VLC_TABLE_1};

struct MelDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    tmp: u64,
    bits: i32,
    size: i32,
    unstuff: bool,
    k: usize,
    num_runs: i32,
    runs: u64,
}

const MEL_EXP: [i32; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5];

impl<'a> MelDecoder<'a> {
    fn new(data: &'a [u8], lcup: usize, scup: usize) -> Option<Self> {
        let mut mel = Self {
            data,
            pos: lcup - scup,
            tmp: 0,
            bits: 0,
            size: (scup - 1) as i32,
            unstuff: false,
            k: 0,
            num_runs: 0,
            runs: 0,
        };

        for _ in 0..4 {
            if mel.unstuff && mel.byte_at(mel.pos) > 0x8F {
                return None;
            }

            let mut d = if mel.size > 0 { mel.byte_at(mel.pos) as u64 } else { 0xFF };
            if mel.size == 1 {
                d |= 0xF;
            }

            if mel.size > 0 {
                mel.pos += 1;
            }
            mel.size -= 1;

            let d_bits = 8 - mel.unstuff as i32;
            mel.tmp = (mel.tmp << d_bits) | d;
            mel.bits += d_bits;
            mel.unstuff = (d & 0xFF) == 0xFF;
        }

        mel.tmp <<= 64 - mel.bits;
        Some(mel)
    }

    fn byte_at(&self, index: usize) -> u8 {
        self.data.get(index).copied().unwrap_or(0xFF)
    }

    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }

        let mut val = 0xFFFF_FFFFu32;

        if self.size > 4 {
            val = u32::from_le_bytes([
                self.byte_at(self.pos),
                self.byte_at(self.pos + 1),
                self.byte_at(self.pos + 2),
                self.byte_at(self.pos + 3),
            ]);
            self.pos += 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut shift = 0;
            while self.size > 1 {
                let v = self.byte_at(self.pos) as u32;
                self.pos += 1;
                let mask = !(0xFFu32 << shift);
                val = (val & mask) | (v << shift);
                self.size -= 1;
                shift += 8;
            }

            let v = self.byte_at(self.pos) as u32 | 0xF;
            self.pos += 1;
            let mask = !(0xFFu32 << shift);
            val = (val & mask) | (v << shift);
            self.size -= 1;
        }

        let mut bits = 32 - self.unstuff as i32;
        let mut t = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 8) & 0xFF;
        unstuff = ((val >> 8) & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 16) & 0xFF;
        unstuff = ((val >> 16) & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 24) & 0xFF;
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        self.tmp |= (t as u64) << (64 - bits - self.bits);
        self.bits += bits;
    }

    fn decode(&mut self) {
        if self.bits < 6 {
            self.read();
        }

        while self.bits >= 6 && self.num_runs < 8 {
            let eval = MEL_EXP[self.k];
            let run;

            if self.tmp & (1u64 << 63) != 0 {
                run = ((1 << eval) - 1) << 1;
                self.k = (self.k + 1).min(12);
                self.tmp <<= 1;
                self.bits -= 1;
            } else {
                let value = ((self.tmp >> (63 - eval)) as i32) & ((1 << eval) - 1);
                self.k = self.k.saturating_sub(1);
                self.tmp <<= eval + 1;
                self.bits -= eval + 1;
                run = (value << 1) + 1;
            }

            let shift = self.num_runs * 7;
            self.runs &= !(0x3Fu64 << shift);
            self.runs |= (run as u64) << shift;
            self.num_runs += 1;
        }
    }

    fn get_run(&mut self) -> i32 {
        if self.num_runs == 0 {
            self.decode();
        }

        let t = (self.runs & 0x7F) as i32;
        self.runs >>= 7;
        self.num_runs -= 1;
        t
    }
}

struct RevReader<'a> {
    data: &'a [u8],
    pos: i64,
    tmp: u64,
    bits: u32,
    size: i32,
    unstuff: bool,
    zero_fill: bool,
}

impl<'a> RevReader<'a> {
    fn new_vlc(data: &'a [u8], lcup: usize, scup: usize) -> Self {
        let mut reader = Self {
            data,
            pos: lcup as i64 - 2,
            tmp: 0,
            bits: 0,
            size: scup as i32 - 2,
            unstuff: false,
            zero_fill: false,
        };

        let d = reader.byte_at(reader.pos) as u64;
        reader.pos -= 1;
        reader.tmp = d >> 4;
        reader.bits = 4 - ((reader.tmp & 7) == 7) as u32;
        reader.unstuff = d > 0x8F;

        let num = 4.min(reader.size);
        for _ in 0..num {
            let d = reader.byte_at(reader.pos) as u64;
            reader.pos -= 1;
            let d_bits = 8 - (reader.unstuff && (d & 0x7F) == 0x7F) as u32;
            reader.tmp |= d << reader.bits;
            reader.bits += d_bits;
            reader.unstuff = d > 0x8F;
        }
        reader.size -= num;

        reader.read();
        reader
    }

    fn new_vlc8(data: &'a [u8], lcup: usize, scup: usize) -> Self {
        let mut reader = Self {
            data,
            pos: lcup as i64 - 2,
            tmp: 0,
            bits: 0,
            size: scup as i32 - 2,
            unstuff: false,
            zero_fill: false,
        };

        let val = reader.byte_at(reader.pos) >> 4;
        reader.pos -= 1;

        let stuffed = ((val & 0x7) == 0x7) as u32;
        let val = val & (0xFu8 >> stuffed);
        reader.tmp = val as u64;
        reader.bits = 4 - stuffed;
        reader.unstuff = val > 0x8;

        reader
    }

    fn new_mrp(data: &'a [u8], lcup: usize, len2: usize) -> Self {
        let mut reader = Self {
            data,
            pos: (lcup + len2) as i64 - 1,
            tmp: 0,
            bits: 0,
            size: len2 as i32,
            unstuff: true,
            zero_fill: true,
        };

        for _ in 0..4 {
            let d = if reader.size > 0 {
                let d = reader.byte_at(reader.pos) as u64;
                reader.pos -= 1;
                d
            } else {
                0
            };
            reader.size -= 1;

            let d_bits = 8 - (reader.unstuff && (d & 0x7F) == 0x7F) as u32;
            reader.tmp |= d << reader.bits;
            reader.bits += d_bits;
            reader.unstuff = d > 0x8F;
        }

        reader.read();
        reader
    }

    fn byte_at(&self, index: i64) -> u8 {
        if index < 0 {
            return 0;
        }
        self.data.get(index as usize).copied().unwrap_or(0)
    }

    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }

        let mut val = 0u32;

        if self.size > 3 {
            val = u32::from_le_bytes([
                self.byte_at(self.pos - 3),
                self.byte_at(self.pos - 2),
                self.byte_at(self.pos - 1),
                self.byte_at(self.pos),
            ]);
            self.pos -= 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut shift = 24;
            while self.size > 0 {
                let v = self.byte_at(self.pos) as u32;
                self.pos -= 1;
                val |= v << shift;
                self.size -= 1;
                shift -= 8;
            }
        } else if self.zero_fill {
            val = 0;
        }

        let mut tmp = val >> 24;
        let mut bits = 8 - (self.unstuff && ((val >> 24) & 0x7F) == 0x7F) as u32;
        let mut unstuff = (val >> 24) > 0x8F;

        tmp |= ((val >> 16) & 0xFF) << bits;
        bits += 8 - (unstuff && ((val >> 16) & 0x7F) == 0x7F) as u32;
        unstuff = ((val >> 16) & 0xFF) > 0x8F;

        tmp |= ((val >> 8) & 0xFF) << bits;
        bits += 8 - (unstuff && ((val >> 8) & 0x7F) == 0x7F) as u32;
        unstuff = ((val >> 8) & 0xFF) > 0x8F;

        tmp |= (val & 0xFF) << bits;
        bits += 8 - (unstuff && (val & 0x7F) == 0x7F) as u32;
        unstuff = (val & 0xFF) > 0x8F;

        self.tmp |= (tmp as u64) << self.bits;
        self.bits += bits;
        self.unstuff = unstuff;
    }

    fn fetch(&mut self) -> u32 {
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        self.tmp as u32
    }

    fn advance(&mut self, num_bits: u32) -> u32 {
        let num_bits = num_bits.min(self.bits);
        self.tmp >>= num_bits;
        self.bits -= num_bits;
        self.tmp as u32
    }

    fn read8(&mut self) {
        let mut val = 0u8;
        if self.size > 0 {
            val = self.byte_at(self.pos);
            self.pos -= 1;
            self.size -= 1;
        }

        let stuffed = (self.unstuff && (val & 0x7F) == 0x7F) as u32;
        let val = val & (0xFFu8 >> stuffed);
        self.tmp |= (val as u64) << self.bits;
        self.bits += 8 - stuffed;
        self.unstuff = val > 0x8F;
    }

    fn fetch64(&mut self) -> u64 {
        while self.bits <= 56 {
            self.read8();
        }
        self.tmp
    }

    fn advance64(&mut self, num_bits: u32) -> u64 {
        let num_bits = num_bits.min(self.bits);
        self.tmp >>= num_bits;
        self.bits -= num_bits;
        self.tmp
    }

    fn fetch_bits(&mut self, wide: bool) -> u64 {
        if wide {
            self.fetch64()
        } else {
            self.fetch() as u64
        }
    }

    fn advance_bits(&mut self, num_bits: u32, wide: bool) -> u64 {
        if wide {
            self.advance64(num_bits)
        } else {
            self.advance(num_bits) as u64
        }
    }
}

struct FrwdReader<'a> {
    data: &'a [u8],
    pos: usize,
    tmp: u64,
    bits: u32,
    unstuff: bool,
    size: i32,
    fill: u8,
}

impl<'a> FrwdReader<'a> {
    fn new(data: &'a [u8], start: usize, size: usize, fill: u8) -> Self {
        let mut reader = Self { data, pos: start, tmp: 0, bits: 0, unstuff: false, size: size as i32, fill };

        for _ in 0..4 {
            let d = if reader.size > 0 {
                let d = reader.data.get(reader.pos).copied().unwrap_or(reader.fill);
                reader.pos += 1;
                d
            } else {
                reader.fill
            } as u64;
            reader.size -= 1;

            reader.tmp |= d << reader.bits;
            reader.bits += 8 - reader.unstuff as u32;
            reader.unstuff = (d & 0xFF) == 0xFF;
        }

        reader.read();
        reader
    }

    fn byte_at(&self, index: usize) -> u8 {
        self.data.get(index).copied().unwrap_or(self.fill)
    }

    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }

        let filled = if self.fill != 0 { 0xFFFF_FFFFu32 } else { 0 };
        let val;

        if self.size > 3 {
            val = u32::from_le_bytes([
                self.byte_at(self.pos),
                self.byte_at(self.pos + 1),
                self.byte_at(self.pos + 2),
                self.byte_at(self.pos + 3),
            ]);
            self.pos += 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut acc = filled;
            let mut shift = 0;
            while self.size > 0 {
                let v = self.byte_at(self.pos) as u32;
                self.pos += 1;
                let mask = !(0xFFu32 << shift);
                acc = (acc & mask) | (v << shift);
                self.size -= 1;
                shift += 8;
            }
            val = acc;
        } else {
            val = filled;
        }

        let mut bits = 8 - self.unstuff as u32;
        let mut t = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;

        t |= ((val >> 8) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        unstuff = ((val >> 8) & 0xFF) == 0xFF;

        t |= ((val >> 16) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        unstuff = ((val >> 16) & 0xFF) == 0xFF;

        t |= ((val >> 24) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        self.tmp |= (t as u64) << self.bits;
        self.bits += bits;
    }

    fn fetch(&mut self) -> u32 {
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        self.tmp as u32
    }

    fn advance(&mut self, num_bits: u32) {
        let num_bits = num_bits.min(self.bits);
        self.tmp >>= num_bits;
        self.bits -= num_bits;
    }

    fn new8(data: &'a [u8], start: usize, size: usize, fill: u8) -> Self {
        let mut reader = Self { data, pos: start, tmp: 0, bits: 0, unstuff: false, size: size as i32, fill };
        reader.read8();
        reader
    }

    fn read8(&mut self) {
        let mut val = self.fill;
        if self.size > 0 {
            val = self.byte_at(self.pos);
            self.pos += 1;
            self.size -= 1;
        }

        let stuffed = self.unstuff as u32;
        let val = val & (0xFFu8 >> stuffed);
        self.unstuff = val == 0xFF;
        self.tmp |= (val as u64) << self.bits;
        self.bits += 8 - stuffed;
    }

    fn fetch64(&mut self) -> u64 {
        while self.bits <= 56 {
            self.read8();
        }
        self.tmp
    }
}

const UVLC_DEC: [u8; 8] = [
    3 | (5 << 2) | (5 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
    3 | (1 << 2) | (3 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
];

struct UvlcResult {
    u: [u32; 2],
    consumed: u32,
}

const U_EXT_THRESHOLD: u32 = 32;
const U_EXT_BITS: u32 = 4;

fn apply_u_extensions(u: &mut [u32; 2], bias: &[u32; 2], vlc: &mut u64, consumed: &mut u32) {
    for quad in 0..2 {
        let encoded = u[quad] - 1;
        if encoded.saturating_sub(bias[quad]) <= U_EXT_THRESHOLD {
            continue;
        }

        let ext = (*vlc & ((1u64 << U_EXT_BITS) - 1)) as u32;
        *vlc >>= U_EXT_BITS;
        *consumed += U_EXT_BITS;
        u[quad] += ext << 2;
    }
}

fn decode_init_uvlc(mut vlc: u64, mode: u32, wide: bool) -> UvlcResult {
    let mut u = [0u32; 2];
    let mut bias = [0u32; 2];
    let mut consumed = 0u32;

    match mode {
        0 => {
            u[0] = 1;
            u[1] = 1;
        }
        1 | 2 => {
            let d = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d & 0x3;
            consumed += d & 0x3;

            let suffix_len = (d >> 2) & 0x7;
            consumed += suffix_len;

            let value = (d >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[0] = if mode == 1 { value + 1 } else { 1 };
            u[1] = if mode == 1 { 1 } else { value + 1 };
        }
        3 => {
            let mut d1 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d1 & 0x3;
            consumed += d1 & 0x3;

            if (d1 & 0x3) > 2 {
                u[1] = (vlc & 1) as u32 + 2;
                bias[1] = 1;
                consumed += 1;
                vlc >>= 1;

                let suffix_len = (d1 >> 2) & 0x7;
                consumed += suffix_len;
                d1 = (d1 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
                u[0] = d1 + 1;
            } else {
                let mut d2 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
                vlc >>= d2 & 0x3;
                consumed += d2 & 0x3;

                let suffix_len = (d1 >> 2) & 0x7;
                consumed += suffix_len;
                d1 = (d1 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
                u[0] = d1 + 1;
                vlc >>= suffix_len;

                let suffix_len = (d2 >> 2) & 0x7;
                consumed += suffix_len;
                d2 = (d2 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
                u[1] = d2 + 1;
            }
        }
        _ => {
            let mut d1 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d1 & 0x3;
            consumed += d1 & 0x3;

            let mut d2 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d2 & 0x3;
            consumed += d2 & 0x3;

            let suffix_len = (d1 >> 2) & 0x7;
            consumed += suffix_len;
            d1 = (d1 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[0] = d1 + 3;
            vlc >>= suffix_len;

            let suffix_len = (d2 >> 2) & 0x7;
            consumed += suffix_len;
            d2 = (d2 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[1] = d2 + 3;

            bias = [2, 2];
        }
    }

    if wide && mode != 0 {
        apply_u_extensions(&mut u, &bias, &mut vlc, &mut consumed);
    }

    UvlcResult { u, consumed }
}

fn decode_noninit_uvlc(mut vlc: u64, mode: u32, wide: bool) -> UvlcResult {
    let mut u = [0u32; 2];
    let mut consumed = 0u32;

    match mode {
        0 => {
            u[0] = 1;
            u[1] = 1;
        }
        1 | 2 => {
            let d = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d & 0x3;
            consumed += d & 0x3;

            let suffix_len = (d >> 2) & 0x7;
            consumed += suffix_len;

            let value = (d >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[0] = if mode == 1 { value + 1 } else { 1 };
            u[1] = if mode == 1 { 1 } else { value + 1 };
        }
        _ => {
            let mut d1 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d1 & 0x3;
            consumed += d1 & 0x3;

            let mut d2 = UVLC_DEC[(vlc & 0x7) as usize] as u32;
            vlc >>= d2 & 0x3;
            consumed += d2 & 0x3;

            let suffix_len = (d1 >> 2) & 0x7;
            consumed += suffix_len;
            d1 = (d1 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[0] = d1 + 1;
            vlc >>= suffix_len;

            let suffix_len = (d2 >> 2) & 0x7;
            consumed += suffix_len;
            d2 = (d2 >> 5) + (vlc & ((1u64 << suffix_len) - 1)) as u32;
            u[1] = d2 + 1;
        }
    }

    if wide && mode != 0 {
        apply_u_extensions(&mut u, &[0, 0], &mut vlc, &mut consumed);
    }

    UvlcResult { u, consumed }
}

const SIG_WORDS: usize = 132;

pub struct HtDecoder {
    data: Vec<u64>,
    samples: Vec<i64>,
    sigma1: Vec<u32>,
    sigma2: Vec<u32>,
    mbr1: Vec<u32>,
    mbr2: Vec<u32>,
    line_state: Vec<u8>,
    w: usize,
    h: usize,
    wide: bool,
}

pub struct HtCodeBlock<'a> {
    pub width: usize,
    pub height: usize,
    pub num_bps: u32,
    pub mb: u32,
    pub data: &'a [u8],
    pub cleanup_len: usize,
    pub refine_len: usize,
    pub num_passes: u32,
    pub stripe_causal: bool,
}

impl HtDecoder {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            samples: Vec::new(),
            sigma1: Vec::new(),
            sigma2: Vec::new(),
            mbr1: Vec::new(),
            mbr2: Vec::new(),
            line_state: Vec::new(),
            w: 0,
            h: 0,
            wide: false,
        }
    }

    pub fn data(&self) -> &[i64] {
        &self.samples
    }

    fn allocate(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;

        self.data.clear();
        self.data.resize(w * h, 0);
        self.samples.clear();
        self.samples.resize(w * h, 0);

        let words = (w.div_ceil(8) + 2).max(SIG_WORDS.min(w.div_ceil(8) + 2));

        for buffer in [&mut self.sigma1, &mut self.sigma2, &mut self.mbr1, &mut self.mbr2] {
            buffer.clear();
            buffer.resize(words, 0);
        }

        self.line_state.clear();
        self.line_state.resize(w.div_ceil(2) + 3, 0);
    }

    pub fn decode(&mut self, cblk: &HtCodeBlock) -> bool {
        let width = cblk.width;
        let height = cblk.height;

        self.allocate(width, height);

        if width == 0 || height == 0 || cblk.mb == 0 {
            return true;
        }

        self.wide = cblk.mb > 30;

        if cblk.mb > 62 {
            return false;
        }

        let zero_bplanes = (cblk.mb + 1).saturating_sub(cblk.num_bps);
        if zero_bplanes > cblk.mb {
            return false;
        }

        let mut num_passes = cblk.num_passes.min(3);

        if num_passes > 1 && cblk.refine_len == 0 {
            num_passes = 1;
        }
        if zero_bplanes == cblk.mb && num_passes > 1 {
            num_passes = 1;
        }

        let lcup = cblk.cleanup_len;
        if lcup < 2 || lcup > cblk.data.len() || lcup + cblk.refine_len > cblk.data.len() {
            return false;
        }

        let scup = ((cblk.data[lcup - 1] as usize) << 4) + (cblk.data[lcup - 2] & 0xF) as usize;
        if scup < 2 || scup > lcup || scup > 4079 {
            return false;
        }

        let mut mel = match MelDecoder::new(cblk.data, lcup, scup) {
            Some(mel) => mel,
            None => return false,
        };
        let mut vlc = if self.wide {
            RevReader::new_vlc8(cblk.data, lcup, scup)
        } else {
            RevReader::new_vlc(cblk.data, lcup, scup)
        };
        let mut magsgn = if self.wide {
            FrwdReader::new8(cblk.data, 0, lcup - scup, 0xFF)
        } else {
            FrwdReader::new(cblk.data, 0, lcup - scup, 0xFF)
        };

        let mut sigprop = FrwdReader::new(cblk.data, lcup, cblk.refine_len, 0);
        let mut magref = RevReader::new_mrp(cblk.data, lcup, cblk.refine_len);

        let p = cblk.num_bps;

        if !self.decode_cleanup(
            &mut mel,
            &mut vlc,
            &mut magsgn,
            p,
            zero_bplanes + 1,
            num_passes,
            cblk.stripe_causal,
            &mut sigprop,
            &mut magref,
        ) {
            self.finalize_samples();
            return false;
        }

        if num_passes > 1 {
            self.finish_refinement(&mut sigprop, &mut magref, p, num_passes, cblk.stripe_causal);
        }

        self.finalize_samples();
        true
    }

    fn finalize_samples(&mut self) {
        let sign_bit = if self.wide { 1u64 << 63 } else { 1u64 << 31 };
        let magnitude_mask = sign_bit - 1;

        self.samples.clear();
        self.samples.extend(self.data.iter().map(|&sample| {
            let magnitude = (sample & magnitude_mask) as i64;
            if sample & sign_bit != 0 {
                -magnitude
            } else {
                magnitude
            }
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_cleanup(
        &mut self,
        mel: &mut MelDecoder,
        vlc: &mut RevReader,
        magsgn: &mut FrwdReader,
        p: u32,
        zero_bplanes_p1: u32,
        num_passes: u32,
        stripe_causal: bool,
        sigprop: &mut FrwdReader,
        magref: &mut RevReader,
    ) -> bool {
        let width = self.w;
        let height = self.h;
        let wide = self.wide;

        let mut run = mel.get_run();
        let mut sip_index = 0usize;
        let mut sip_shift = 0u32;
        let mut sip_use_sigma1 = true;

        {
            let mut c_q = 0u32;
            let mut lsp = 0usize;
            self.line_state[0] = 0;

            let mut x = 0usize;
            while x < width {
                let mut qinf = [0u32; 2];

                let vlc_val = vlc.fetch_bits(wide);
                qinf[0] = VLC_TABLE_0[((c_q << 7) | (vlc_val as u32 & 0x7F)) as usize] as u32;

                if c_q == 0 {
                    run -= 2;
                    if run != -1 {
                        qinf[0] = 0;
                    }
                    if run < 0 {
                        run = mel.get_run();
                    }
                }

                c_q = ((qinf[0] & 0x10) >> 4) | ((qinf[0] & 0xE0) >> 5);
                let mut vlc_val = vlc.advance_bits(qinf[0] & 0x7, wide);

                self.set_sigma(sip_use_sigma1, sip_index, {
                    (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2)) << sip_shift
                });

                if x + 2 < width {
                    qinf[1] = VLC_TABLE_0[((c_q << 7) | (vlc_val as u32 & 0x7F)) as usize] as u32;

                    if c_q == 0 {
                        run -= 2;
                        if run != -1 {
                            qinf[1] = 0;
                        }
                        if run < 0 {
                            run = mel.get_run();
                        }
                    }

                    c_q = ((qinf[1] & 0x10) >> 4) | ((qinf[1] & 0xE0) >> 5);
                    vlc_val = vlc.advance_bits(qinf[1] & 0x7, wide);
                }

                self.set_sigma(sip_use_sigma1, sip_index, {
                    ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift)
                });

                if x & 0x7 != 0 {
                    sip_index += 1;
                }
                sip_shift ^= 0x10;

                let mut uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
                if uvlc_mode == 3 {
                    run -= 2;
                    if run == -1 {
                        uvlc_mode += 1;
                    }
                    if run < 0 {
                        run = mel.get_run();
                    }
                }

                let UvlcResult { u: u_q, consumed } = decode_init_uvlc(vlc_val, uvlc_mode, wide);
                if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                    return false;
                }
                vlc.advance_bits(consumed, wide);

                let mut locs = 0xFFu32;
                if x + 4 > width {
                    locs >>= (x + 4 - width) << 1;
                }
                if height <= 1 {
                    locs &= 0x55;
                }

                if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
                    return false;
                }

                self.decode_quad_pair(magsgn, &qinf, &u_q, p, locs, 0, x, lsp);
                lsp += 2;

                x += 4;
            }
        }

        let mut y = 2usize;
        while y < height {
            sip_shift ^= 0x2;
            sip_shift &= 0xFFFF_FFEF;
            sip_use_sigma1 = y & 0x4 == 0;
            sip_index = 0;

            let mut lsp = 0usize;
            let mut ls0 = self.line_state[0];
            self.line_state[0] = 0;
            let mut c_q = 0u32;

            let mut x = 0usize;
            while x < width {
                let mut qinf = [0u32; 2];

                c_q |= (ls0 >> 7) as u32;
                c_q |= ((self.line_state[lsp + 1] >> 5) & 0x4) as u32;

                let vlc_val = vlc.fetch_bits(wide);
                qinf[0] = VLC_TABLE_1[((c_q << 7) | (vlc_val as u32 & 0x7F)) as usize] as u32;

                if c_q == 0 {
                    run -= 2;
                    if run != -1 {
                        qinf[0] = 0;
                    }
                    if run < 0 {
                        run = mel.get_run();
                    }
                }

                c_q = ((qinf[0] & 0x40) >> 5) | ((qinf[0] & 0x80) >> 6);
                let mut vlc_val = vlc.advance_bits(qinf[0] & 0x7, wide);

                self.set_sigma(sip_use_sigma1, sip_index, {
                    (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2)) << sip_shift
                });

                if x + 2 < width {
                    c_q |= (self.line_state[lsp + 1] >> 7) as u32;
                    c_q |= ((self.line_state[lsp + 2] >> 5) & 0x4) as u32;

                    qinf[1] = VLC_TABLE_1[((c_q << 7) | (vlc_val as u32 & 0x7F)) as usize] as u32;

                    if c_q == 0 {
                        run -= 2;
                        if run != -1 {
                            qinf[1] = 0;
                        }
                        if run < 0 {
                            run = mel.get_run();
                        }
                    }

                    c_q = ((qinf[1] & 0x40) >> 5) | ((qinf[1] & 0x80) >> 6);
                    vlc_val = vlc.advance_bits(qinf[1] & 0x7, wide);
                }

                self.set_sigma(sip_use_sigma1, sip_index, {
                    ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift)
                });

                if x & 0x7 != 0 {
                    sip_index += 1;
                }
                sip_shift ^= 0x10;

                let uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
                let UvlcResult { u: mut u_q, consumed } = decode_noninit_uvlc(vlc_val, uvlc_mode, wide);
                vlc.advance_bits(consumed, wide);

                if (qinf[0] & 0xF0) & ((qinf[0] & 0xF0).wrapping_sub(1)) != 0 {
                    let e = ((ls0 & 0x7F) as u32).max((self.line_state[lsp + 1] & 0x7F) as u32);
                    u_q[0] += e.saturating_sub(2);
                }

                if (qinf[1] & 0xF0) & ((qinf[1] & 0xF0).wrapping_sub(1)) != 0 {
                    let e = ((self.line_state[lsp + 1] & 0x7F) as u32).max((self.line_state[lsp + 2] & 0x7F) as u32);
                    u_q[1] += e.saturating_sub(2);
                }

                if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                    return false;
                }

                ls0 = self.line_state[lsp + 2];
                self.line_state[lsp + 1] = 0;
                self.line_state[lsp + 2] = 0;

                let mut locs = 0xFFu32;
                if x + 4 > width {
                    locs >>= (x + 4 - width) << 1;
                }
                if y + 2 > height {
                    locs &= 0x55;
                }

                if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
                    return false;
                }

                self.decode_quad_pair(magsgn, &qinf, &u_q, p, locs, y, x, lsp);
                lsp += 2;

                x += 4;
            }

            y += 2;

            if num_passes > 1 && (y & 3) == 0 {
                if num_passes > 2 && y >= 4 {
                    self.magref_stripe(magref, y - 4, y & 0x4 != 0, p);
                }

                if y >= 4 {
                    self.build_mbr(y & 0x4 != 0);
                }

                if y >= 8 {
                    self.sigprop_stripe(sigprop, y - 8, y & 0x4 == 0, stripe_causal, p);
                }
            }
        }

        true
    }

    fn set_sigma(&mut self, use_sigma1: bool, index: usize, value: u32) {
        let buffer = if use_sigma1 { &mut self.sigma1 } else { &mut self.sigma2 };
        if let Some(slot) = buffer.get_mut(index) {
            *slot |= value;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_quad_pair(
        &mut self,
        magsgn: &mut FrwdReader,
        qinf: &[u32; 2],
        u_q: &[u32; 2],
        p: u32,
        locs: u32,
        y: usize,
        x: usize,
        lsp: usize,
    ) {
        for quad in 0..2 {
            for sample in 0..4 {
                let index = quad * 4 + sample;
                let col = x + index / 2;
                let row = y + index % 2;

                let slot = lsp + quad;
                if sample == 2 {
                    if let Some(state) = self.line_state.get_mut(slot + 1) {
                        *state = 0;
                    }
                }

                if qinf[quad] & (0x10 << sample) != 0 {
                    let width_bits = if self.wide { 64 } else { 32 };
                    let ms_val = if self.wide { magsgn.fetch64() } else { magsgn.fetch() as u64 };
                    let m_n = u_q[quad].saturating_sub((qinf[quad] >> (12 + sample)) & 1);
                    magsgn.advance(m_n);

                    let sign = ms_val << (width_bits - 1);
                    let mut v_n = if m_n >= width_bits { ms_val } else { ms_val & ((1u64 << m_n) - 1) };
                    v_n |= (((qinf[quad] >> (8 + sample)) & 1) as u64) << m_n;
                    v_n |= 1;

                    if col < self.w && row < self.h {
                        self.data[row * self.w + col] = sign | (v_n.wrapping_add(2) << (p - 1));
                    }

                    let exponent = 64 - v_n.leading_zeros();
                    if sample == 1 {
                        if let Some(state) = self.line_state.get_mut(slot) {
                            let previous = (*state & 0x7F) as u32;
                            *state = 0x80 | previous.max(exponent) as u8;
                        }
                    } else if sample == 3 {
                        if let Some(state) = self.line_state.get_mut(slot + 1) {
                            *state = 0x80 | exponent as u8;
                        }
                    }
                } else if locs & (1 << index) != 0 && col < self.w && row < self.h {
                    self.data[row * self.w + col] = 0;
                }
            }
        }
    }

    fn magref_stripe(&mut self, magref: &mut RevReader, y: usize, use_sigma1: bool, p: u32) {
        let width = self.w;
        let stride = self.w;
        let half = 1u64 << (p - 2);

        let mut group = 0usize;
        let mut i = 0usize;
        while i < width {
            let mut cwd = magref.fetch();
            let sig = {
                let buffer = if use_sigma1 { &self.sigma1 } else { &self.sigma2 };
                buffer.get(group).copied().unwrap_or(0)
            };

            if sig != 0 {
                for j in 0..8 {
                    let col_mask = 0xFu32 << (j * 4);
                    if sig & col_mask == 0 {
                        continue;
                    }

                    let mut sample_mask = 0x1111_1111u32 & col_mask;
                    for row in 0..4 {
                        if sig & sample_mask != 0 {
                            let x = i + j;
                            let sample_y = y + row;

                            if x < width && sample_y < self.h {
                                let index = sample_y * stride + x;
                                let sym = cwd & 1;
                                self.data[index] ^= ((1 - sym) as u64) << (p - 1);
                                self.data[index] |= half;
                            }

                            cwd >>= 1;
                        }
                        sample_mask = sample_mask.wrapping_add(sample_mask);
                    }
                }
            }

            magref.advance(sig.count_ones());

            i += 8;
            group += 1;
        }
    }

    fn build_mbr(&mut self, use_first: bool) {
        let width = self.w;
        let groups = width.div_ceil(8);

        let mut prev = 0u32;
        for group in 0..groups {
            let (sig, sig_next) = {
                let buffer = if use_first { &self.sigma1 } else { &self.sigma2 };
                (buffer.get(group).copied().unwrap_or(0), buffer.get(group + 1).copied().unwrap_or(0))
            };

            let mut m = sig;
            m |= prev >> 28;
            m |= sig << 4;
            m |= sig >> 4;
            m |= sig_next << 28;
            prev = sig;

            let t = m;
            m |= (t & 0x7777_7777) << 1;
            m |= (t & 0xEEEE_EEEE) >> 1;

            let buffer = if use_first { &mut self.mbr1 } else { &mut self.mbr2 };
            if let Some(slot) = buffer.get_mut(group) {
                *slot = m & !sig;
            }
        }
    }

    fn sigprop_stripe(&mut self, sigprop: &mut FrwdReader, y: usize, cur_is_first: bool, stripe_causal: bool, p: u32) {
        self.merge_next_stripe_candidates(cur_is_first, stripe_causal);
        self.sigprop_scan(sigprop, y, cur_is_first, 0xFFFF_FFFF, p);

        let buffer = if cur_is_first { &mut self.sigma1 } else { &mut self.sigma2 };
        for slot in buffer.iter_mut() {
            *slot = 0;
        }
    }

    fn merge_next_stripe_candidates(&mut self, cur_is_first: bool, stripe_causal: bool) {
        let groups = self.w.div_ceil(8);
        let mut prev = 0u32;

        for group in 0..groups {
            let (nxt, nxt_next) = {
                let buffer = if cur_is_first { &self.sigma2 } else { &self.sigma1 };
                (buffer.get(group).copied().unwrap_or(0), buffer.get(group + 1).copied().unwrap_or(0))
            };

            let mut t = nxt;
            t |= prev >> 28;
            t |= nxt << 4;
            t |= nxt >> 4;
            t |= nxt_next << 28;
            prev = nxt;

            let cur_sig = {
                let buffer = if cur_is_first { &self.sigma1 } else { &self.sigma2 };
                buffer.get(group).copied().unwrap_or(0)
            };

            let buffer = if cur_is_first { &mut self.mbr1 } else { &mut self.mbr2 };
            if let Some(slot) = buffer.get_mut(group) {
                if !stripe_causal {
                    *slot |= (t & 0x1111_1111) << 3;
                }
                *slot &= !cur_sig;
            }
        }
    }

    fn sigprop_scan(&mut self, sigprop: &mut FrwdReader, y: usize, cur_is_first: bool, pattern: u32, p: u32) {
        let width = self.w;
        let stride = self.w;
        let groups = width.div_ceil(8);
        let val = 3u64 << (p - 2);
        let sign_shift = if self.wide { 63 } else { 31 };

        for group in 0..groups {
            let i = group * 8;

            let cur_sig = {
                let buffer = if cur_is_first { &self.sigma1 } else { &self.sigma2 };
                buffer.get(group).copied().unwrap_or(0)
            };

            let mut mbr = {
                let buffer = if cur_is_first { &self.mbr1 } else { &self.mbr2 };
                buffer.get(group).copied().unwrap_or(0) & pattern
            };

            let mut new_sig = 0u32;

            if mbr != 0 {
                for n in [0usize, 4] {
                    let mut cwd = sigprop.fetch();
                    let mut consumed = 0u32;
                    let inv_sig = !cur_sig & pattern;
                    let end = if n + 4 + i < width { n + 4 } else { width - i };

                    for j in n..end {
                        let col_mask = 0xFu32 << (4 * j);
                        if col_mask & mbr == 0 {
                            continue;
                        }

                        let mut sample_mask = 0x1111_1111u32 & col_mask;
                        for row in 0..4 {
                            if mbr & sample_mask != 0 {
                                if cwd & 1 != 0 {
                                    new_sig |= sample_mask;
                                    let spread = match row {
                                        0 => 0x32u32,
                                        1 => 0x74,
                                        2 => 0xE8,
                                        _ => 0xC0,
                                    };
                                    mbr |= (spread << (j * 4)) & inv_sig;
                                }
                                cwd >>= 1;
                                consumed += 1;
                            }
                            sample_mask = sample_mask.wrapping_add(sample_mask);
                        }
                    }

                    if new_sig & (0xFFFFu32 << (4 * n)) != 0 {
                        for j in n..end {
                            let col_mask = 0xFu32 << (4 * j);
                            if col_mask & new_sig == 0 {
                                continue;
                            }

                            let mut sample_mask = 0x1111_1111u32 & col_mask;
                            for row in 0..4 {
                                if new_sig & sample_mask != 0 {
                                    let x = i + j;
                                    let sample_y = y + row;

                                    if x < width && sample_y < self.h {
                                        self.data[sample_y * stride + x] |= (((cwd & 1) as u64) << sign_shift) | val;
                                    }

                                    cwd >>= 1;
                                    consumed += 1;
                                }
                                sample_mask = sample_mask.wrapping_add(sample_mask);
                            }
                        }
                    }

                    sigprop.advance(consumed);

                    if n == 4 {
                        let mut t = new_sig >> 28;
                        t |= ((t & 0xE) >> 1) | ((t & 7) << 1);

                        let next_sig = {
                            let buffer = if cur_is_first { &self.sigma1 } else { &self.sigma2 };
                            buffer.get(group + 1).copied().unwrap_or(0)
                        };
                        let buffer = if cur_is_first { &mut self.mbr1 } else { &mut self.mbr2 };
                        if let Some(slot) = buffer.get_mut(group + 1) {
                            *slot |= t & !next_sig;
                        }
                    }
                }
            }

            new_sig |= cur_sig;
            let ux = (new_sig & 0x8888_8888) >> 3;
            let tx = ux | (ux << 4) | (ux >> 4);

            let (nxt_sig_prev, nxt_sig, nxt_sig_next) = {
                let buffer = if cur_is_first { &self.sigma2 } else { &self.sigma1 };
                (
                    if group > 0 { buffer.get(group - 1).copied().unwrap_or(0) } else { 0 },
                    buffer.get(group).copied().unwrap_or(0),
                    buffer.get(group + 1).copied().unwrap_or(0),
                )
            };

            let buffer = if cur_is_first { &mut self.mbr2 } else { &mut self.mbr1 };
            if group > 0 {
                if let Some(slot) = buffer.get_mut(group - 1) {
                    *slot |= (ux << 28) & !nxt_sig_prev;
                }
            }
            if let Some(slot) = buffer.get_mut(group) {
                *slot |= tx & !nxt_sig;
            }
            if let Some(slot) = buffer.get_mut(group + 1) {
                *slot |= (ux >> 28) & !nxt_sig_next;
            }
        }
    }

    fn finish_refinement(
        &mut self,
        sigprop: &mut FrwdReader,
        magref: &mut RevReader,
        p: u32,
        num_passes: u32,
        stripe_causal: bool,
    ) {
        let height = self.h;

        let trailing_partial = (height & 3) == 1 || (height & 3) == 2;

        if num_passes > 2 && trailing_partial {
            self.magref_stripe(magref, height & !3, height & 0x4 == 0, p);
        }

        if trailing_partial {
            self.build_mbr(height & 0x4 == 0);
        }

        let mut start = height as i64;
        start -= if height > 6 { (((height + 1) & 3) + 3) as i64 } else { height as i64 };

        let mut y = start.max(0) as usize;
        while y < height {
            let pattern = match height - y {
                1 => 0x1111_1111u32,
                2 => 0x3333_3333,
                3 => 0x7777_7777,
                _ => 0xFFFF_FFFF,
            };

            let cur_is_first = y & 0x4 == 0;

            if height - y > 4 {
                self.merge_next_stripe_candidates(cur_is_first, stripe_causal);
            }

            self.sigprop_scan(sigprop, y, cur_is_first, pattern, p);
            y += 4;
        }
    }
}

impl Default for HtDecoder {
    fn default() -> Self {
        Self::new()
    }
}
