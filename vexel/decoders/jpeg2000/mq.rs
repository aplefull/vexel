use crate::decoders::jpeg2000::types::{T1_CTXNO_AGG, T1_CTXNO_UNI, T1_CTXNO_ZC, T1_NUMCTXS};

#[derive(Clone, Copy)]
struct MqState {
    qeval: u32,
    mps: u32,
    nmps: usize,
    nlps: usize,
}

#[rustfmt::skip]
static MQ_STATES: [MqState; 94] = [
    MqState { qeval: 0x5601, mps: 0, nmps: 2, nlps: 3 },
    MqState { qeval: 0x5601, mps: 1, nmps: 3, nlps: 2 },
    MqState { qeval: 0x3401, mps: 0, nmps: 4, nlps: 12 },
    MqState { qeval: 0x3401, mps: 1, nmps: 5, nlps: 13 },
    MqState { qeval: 0x1801, mps: 0, nmps: 6, nlps: 18 },
    MqState { qeval: 0x1801, mps: 1, nmps: 7, nlps: 19 },
    MqState { qeval: 0x0ac1, mps: 0, nmps: 8, nlps: 24 },
    MqState { qeval: 0x0ac1, mps: 1, nmps: 9, nlps: 25 },
    MqState { qeval: 0x0521, mps: 0, nmps: 10, nlps: 58 },
    MqState { qeval: 0x0521, mps: 1, nmps: 11, nlps: 59 },
    MqState { qeval: 0x0221, mps: 0, nmps: 76, nlps: 66 },
    MqState { qeval: 0x0221, mps: 1, nmps: 77, nlps: 67 },
    MqState { qeval: 0x5601, mps: 0, nmps: 14, nlps: 13 },
    MqState { qeval: 0x5601, mps: 1, nmps: 15, nlps: 12 },
    MqState { qeval: 0x5401, mps: 0, nmps: 16, nlps: 28 },
    MqState { qeval: 0x5401, mps: 1, nmps: 17, nlps: 29 },
    MqState { qeval: 0x4801, mps: 0, nmps: 18, nlps: 28 },
    MqState { qeval: 0x4801, mps: 1, nmps: 19, nlps: 29 },
    MqState { qeval: 0x3801, mps: 0, nmps: 20, nlps: 28 },
    MqState { qeval: 0x3801, mps: 1, nmps: 21, nlps: 29 },
    MqState { qeval: 0x3001, mps: 0, nmps: 22, nlps: 34 },
    MqState { qeval: 0x3001, mps: 1, nmps: 23, nlps: 35 },
    MqState { qeval: 0x2401, mps: 0, nmps: 24, nlps: 36 },
    MqState { qeval: 0x2401, mps: 1, nmps: 25, nlps: 37 },
    MqState { qeval: 0x1c01, mps: 0, nmps: 26, nlps: 40 },
    MqState { qeval: 0x1c01, mps: 1, nmps: 27, nlps: 41 },
    MqState { qeval: 0x1601, mps: 0, nmps: 58, nlps: 42 },
    MqState { qeval: 0x1601, mps: 1, nmps: 59, nlps: 43 },
    MqState { qeval: 0x5601, mps: 0, nmps: 30, nlps: 29 },
    MqState { qeval: 0x5601, mps: 1, nmps: 31, nlps: 28 },
    MqState { qeval: 0x5401, mps: 0, nmps: 32, nlps: 28 },
    MqState { qeval: 0x5401, mps: 1, nmps: 33, nlps: 29 },
    MqState { qeval: 0x5101, mps: 0, nmps: 34, nlps: 30 },
    MqState { qeval: 0x5101, mps: 1, nmps: 35, nlps: 31 },
    MqState { qeval: 0x4801, mps: 0, nmps: 36, nlps: 32 },
    MqState { qeval: 0x4801, mps: 1, nmps: 37, nlps: 33 },
    MqState { qeval: 0x3801, mps: 0, nmps: 38, nlps: 34 },
    MqState { qeval: 0x3801, mps: 1, nmps: 39, nlps: 35 },
    MqState { qeval: 0x3401, mps: 0, nmps: 40, nlps: 36 },
    MqState { qeval: 0x3401, mps: 1, nmps: 41, nlps: 37 },
    MqState { qeval: 0x3001, mps: 0, nmps: 42, nlps: 38 },
    MqState { qeval: 0x3001, mps: 1, nmps: 43, nlps: 39 },
    MqState { qeval: 0x2801, mps: 0, nmps: 44, nlps: 38 },
    MqState { qeval: 0x2801, mps: 1, nmps: 45, nlps: 39 },
    MqState { qeval: 0x2401, mps: 0, nmps: 46, nlps: 40 },
    MqState { qeval: 0x2401, mps: 1, nmps: 47, nlps: 41 },
    MqState { qeval: 0x2201, mps: 0, nmps: 48, nlps: 42 },
    MqState { qeval: 0x2201, mps: 1, nmps: 49, nlps: 43 },
    MqState { qeval: 0x1c01, mps: 0, nmps: 50, nlps: 44 },
    MqState { qeval: 0x1c01, mps: 1, nmps: 51, nlps: 45 },
    MqState { qeval: 0x1801, mps: 0, nmps: 52, nlps: 46 },
    MqState { qeval: 0x1801, mps: 1, nmps: 53, nlps: 47 },
    MqState { qeval: 0x1601, mps: 0, nmps: 54, nlps: 48 },
    MqState { qeval: 0x1601, mps: 1, nmps: 55, nlps: 49 },
    MqState { qeval: 0x1401, mps: 0, nmps: 56, nlps: 50 },
    MqState { qeval: 0x1401, mps: 1, nmps: 57, nlps: 51 },
    MqState { qeval: 0x1201, mps: 0, nmps: 58, nlps: 52 },
    MqState { qeval: 0x1201, mps: 1, nmps: 59, nlps: 53 },
    MqState { qeval: 0x1101, mps: 0, nmps: 60, nlps: 54 },
    MqState { qeval: 0x1101, mps: 1, nmps: 61, nlps: 55 },
    MqState { qeval: 0x0ac1, mps: 0, nmps: 62, nlps: 56 },
    MqState { qeval: 0x0ac1, mps: 1, nmps: 63, nlps: 57 },
    MqState { qeval: 0x09c1, mps: 0, nmps: 64, nlps: 58 },
    MqState { qeval: 0x09c1, mps: 1, nmps: 65, nlps: 59 },
    MqState { qeval: 0x08a1, mps: 0, nmps: 66, nlps: 60 },
    MqState { qeval: 0x08a1, mps: 1, nmps: 67, nlps: 61 },
    MqState { qeval: 0x0521, mps: 0, nmps: 68, nlps: 62 },
    MqState { qeval: 0x0521, mps: 1, nmps: 69, nlps: 63 },
    MqState { qeval: 0x0441, mps: 0, nmps: 70, nlps: 64 },
    MqState { qeval: 0x0441, mps: 1, nmps: 71, nlps: 65 },
    MqState { qeval: 0x02a1, mps: 0, nmps: 72, nlps: 66 },
    MqState { qeval: 0x02a1, mps: 1, nmps: 73, nlps: 67 },
    MqState { qeval: 0x0221, mps: 0, nmps: 74, nlps: 68 },
    MqState { qeval: 0x0221, mps: 1, nmps: 75, nlps: 69 },
    MqState { qeval: 0x0141, mps: 0, nmps: 76, nlps: 70 },
    MqState { qeval: 0x0141, mps: 1, nmps: 77, nlps: 71 },
    MqState { qeval: 0x0111, mps: 0, nmps: 78, nlps: 72 },
    MqState { qeval: 0x0111, mps: 1, nmps: 79, nlps: 73 },
    MqState { qeval: 0x0085, mps: 0, nmps: 80, nlps: 74 },
    MqState { qeval: 0x0085, mps: 1, nmps: 81, nlps: 75 },
    MqState { qeval: 0x0049, mps: 0, nmps: 82, nlps: 76 },
    MqState { qeval: 0x0049, mps: 1, nmps: 83, nlps: 77 },
    MqState { qeval: 0x0025, mps: 0, nmps: 84, nlps: 78 },
    MqState { qeval: 0x0025, mps: 1, nmps: 85, nlps: 79 },
    MqState { qeval: 0x0015, mps: 0, nmps: 86, nlps: 80 },
    MqState { qeval: 0x0015, mps: 1, nmps: 87, nlps: 81 },
    MqState { qeval: 0x0009, mps: 0, nmps: 88, nlps: 82 },
    MqState { qeval: 0x0009, mps: 1, nmps: 89, nlps: 83 },
    MqState { qeval: 0x0005, mps: 0, nmps: 90, nlps: 84 },
    MqState { qeval: 0x0005, mps: 1, nmps: 91, nlps: 85 },
    MqState { qeval: 0x0001, mps: 0, nmps: 90, nlps: 86 },
    MqState { qeval: 0x0001, mps: 1, nmps: 91, nlps: 87 },
    MqState { qeval: 0x5601, mps: 0, nmps: 92, nlps: 92 },
    MqState { qeval: 0x5601, mps: 1, nmps: 93, nlps: 93 },
];

pub struct MqDecoder {
    data: Vec<u8>,
    bp: usize,
    c: u32,
    a: u32,
    ct: u32,
    ctxs: [usize; T1_NUMCTXS],
    curctx: usize,
    end_of_stream: u32,
}

impl MqDecoder {
    pub fn new() -> Self {
        Self { data: Vec::new(), bp: 0, c: 0, a: 0, ct: 0, ctxs: [0; T1_NUMCTXS], curctx: 0, end_of_stream: 0 }
    }

    fn load(&mut self, data: &[u8]) {
        self.data.clear();
        self.data.extend_from_slice(data);
        self.data.push(0xFF);
        self.data.push(0xFF);
        self.bp = 0;
    }

    fn byte_at(&self, index: usize) -> u32 {
        self.data.get(index).copied().unwrap_or(0xFF) as u32
    }

    pub fn init_dec(&mut self, data: &[u8]) {
        let len = data.len();
        self.load(data);
        self.curctx = 0;
        self.end_of_stream = 0;

        self.c = if len == 0 { 0xFF << 16 } else { self.byte_at(0) << 16 };

        self.bytein();
        self.c <<= 7;
        self.ct = self.ct.wrapping_sub(7);
        self.a = 0x8000;
    }

    pub fn raw_init_dec(&mut self, data: &[u8]) {
        self.load(data);
        self.c = 0;
        self.ct = 0;
    }

    pub fn reset_states(&mut self) {
        self.ctxs = [0; T1_NUMCTXS];
    }

    pub fn set_state(&mut self, ctxno: usize, msb: u32, prob: usize) {
        self.ctxs[ctxno] = prob * 2 + msb as usize;
    }

    pub fn reset_dec_states(&mut self) {
        self.reset_states();
        self.set_state(T1_CTXNO_UNI, 0, 46);
        self.set_state(T1_CTXNO_AGG, 0, 3);
        self.set_state(T1_CTXNO_ZC, 0, 4);
    }

    pub fn set_curctx(&mut self, ctxno: usize) {
        self.curctx = ctxno;
    }

    fn bytein(&mut self) {
        let cur = self.byte_at(self.bp);
        let next = self.byte_at(self.bp + 1);

        if cur == 0xFF {
            if next > 0x8F {
                self.c = self.c.wrapping_add(0xFF00);
                self.ct = 8;
                self.end_of_stream = self.end_of_stream.saturating_add(1);
            } else {
                self.bp += 1;
                self.c = self.c.wrapping_add(next << 9);
                self.ct = 7;
            }
        } else {
            self.bp += 1;
            self.c = self.c.wrapping_add(next << 8);
            self.ct = 8;
        }
    }

    fn renormd(&mut self) {
        loop {
            if self.ct == 0 {
                self.bytein();
            }
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.a & 0x8000 != 0 {
                break;
            }
        }
    }

    fn mpsexchange(&mut self) -> u32 {
        let state = MQ_STATES[self.ctxs[self.curctx]];
        if self.a < state.qeval {
            self.ctxs[self.curctx] = state.nlps;
            1 - state.mps
        } else {
            self.ctxs[self.curctx] = state.nmps;
            state.mps
        }
    }

    fn lpsexchange(&mut self) -> u32 {
        let state = MQ_STATES[self.ctxs[self.curctx]];
        let below = self.a < state.qeval;
        self.a = state.qeval;

        if below {
            self.ctxs[self.curctx] = state.nmps;
            state.mps
        } else {
            self.ctxs[self.curctx] = state.nlps;
            1 - state.mps
        }
    }

    pub fn decode(&mut self) -> u32 {
        let qeval = MQ_STATES[self.ctxs[self.curctx]].qeval;
        self.a = self.a.wrapping_sub(qeval);

        if (self.c >> 16) < qeval {
            let d = self.lpsexchange();
            self.renormd();
            d
        } else {
            self.c = self.c.wrapping_sub(qeval << 16);
            if self.a & 0x8000 == 0 {
                let d = self.mpsexchange();
                self.renormd();
                d
            } else {
                MQ_STATES[self.ctxs[self.curctx]].mps
            }
        }
    }

    pub fn raw_decode(&mut self) -> u32 {
        if self.ct == 0 {
            if self.c == 0xFF {
                if self.byte_at(self.bp) > 0x8F {
                    self.c = 0xFF;
                    self.ct = 8;
                } else {
                    self.c = self.byte_at(self.bp);
                    self.bp += 1;
                    self.ct = 7;
                }
            } else {
                self.c = self.byte_at(self.bp);
                self.bp += 1;
                self.ct = 8;
            }
        }
        self.ct -= 1;
        (self.c >> self.ct) & 0x01
    }
}

impl Default for MqDecoder {
    fn default() -> Self {
        Self::new()
    }
}
