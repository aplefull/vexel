use crate::utils::lzma::dictionary::Dictionary;
use crate::utils::lzma::range_decoder::{RangeDecoder, PROB_INIT};

pub const LCLP_MAX: u32 = 4;
pub const PB_MAX: u32 = 4;

const STATES: usize = 12;
const LIT_STATES: u32 = 7;
const POS_STATES_MAX: usize = 1 << PB_MAX;
const LITERAL_CODER_SIZE: usize = 0x300;
const LITERAL_CODERS_MAX: usize = 1 << LCLP_MAX;

const MATCH_LEN_MIN: u32 = 2;
const LEN_LOW_BITS: u32 = 3;
const LEN_LOW_SYMBOLS: usize = 1 << LEN_LOW_BITS;
const LEN_MID_BITS: u32 = 3;
const LEN_MID_SYMBOLS: usize = 1 << LEN_MID_BITS;
const LEN_HIGH_BITS: u32 = 8;
const LEN_HIGH_SYMBOLS: usize = 1 << LEN_HIGH_BITS;

const LEN_TO_POS_STATES: usize = 4;
const POS_SLOT_BITS: u32 = 6;
const POS_SLOTS: usize = 1 << POS_SLOT_BITS;
const START_POS_MODEL_INDEX: u32 = 4;
const END_POS_MODEL_INDEX: u32 = 14;
const FULL_DISTANCES: usize = 1 << (END_POS_MODEL_INDEX / 2);
const ALIGN_BITS: u32 = 4;
const ALIGN_TABLE_SIZE: usize = 1 << ALIGN_BITS;

const LITERAL_NEXT_STATE: [u32; STATES] = [0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 4, 5];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Finished,
    EndMarker,
    NeedsInput,
    OutputFull,
    Corrupt,
}

struct LengthDecoder {
    choice: u16,
    choice2: u16,
    low: [[u16; LEN_LOW_SYMBOLS]; POS_STATES_MAX],
    mid: [[u16; LEN_MID_SYMBOLS]; POS_STATES_MAX],
    high: [u16; LEN_HIGH_SYMBOLS],
}

impl LengthDecoder {
    fn new() -> Self {
        LengthDecoder {
            choice: PROB_INIT,
            choice2: PROB_INIT,
            low: [[PROB_INIT; LEN_LOW_SYMBOLS]; POS_STATES_MAX],
            mid: [[PROB_INIT; LEN_MID_SYMBOLS]; POS_STATES_MAX],
            high: [PROB_INIT; LEN_HIGH_SYMBOLS],
        }
    }

    fn reset(&mut self) {
        self.choice = PROB_INIT;
        self.choice2 = PROB_INIT;
        self.low = [[PROB_INIT; LEN_LOW_SYMBOLS]; POS_STATES_MAX];
        self.mid = [[PROB_INIT; LEN_MID_SYMBOLS]; POS_STATES_MAX];
        self.high = [PROB_INIT; LEN_HIGH_SYMBOLS];
    }

    fn decode(&mut self, rc: &mut RangeDecoder, pos_state: usize) -> u32 {
        if rc.decode_bit(&mut self.choice) == 0 {
            return MATCH_LEN_MIN + rc.decode_bittree(&mut self.low[pos_state]);
        }

        if rc.decode_bit(&mut self.choice2) == 0 {
            return MATCH_LEN_MIN + LEN_LOW_SYMBOLS as u32 + rc.decode_bittree(&mut self.mid[pos_state]);
        }

        MATCH_LEN_MIN + (LEN_LOW_SYMBOLS + LEN_MID_SYMBOLS) as u32 + rc.decode_bittree(&mut self.high)
    }
}

#[derive(Clone, Copy)]
pub struct Properties {
    pub lc: u32,
    pub lp: u32,
    pub pb: u32,
}

impl Properties {
    pub fn from_byte(byte: u8) -> Option<Self> {
        if byte > (4 * 5 + 4) * 9 + 8 {
            return None;
        }

        let mut value = u32::from(byte);
        let pb = value / (9 * 5);
        value -= pb * 9 * 5;
        let lp = value / 9;
        let lc = value - lp * 9;

        if lc + lp > LCLP_MAX {
            return None;
        }

        Some(Properties { lc, lp, pb })
    }
}

pub struct Lzma1Decoder {
    literal: Vec<[u16; LITERAL_CODER_SIZE]>,
    is_match: [[u16; POS_STATES_MAX]; STATES],
    is_rep: [u16; STATES],
    is_rep0: [u16; STATES],
    is_rep1: [u16; STATES],
    is_rep2: [u16; STATES],
    is_rep0_long: [[u16; POS_STATES_MAX]; STATES],
    pos_slot: [[u16; POS_SLOTS]; LEN_TO_POS_STATES],
    pos_special: [u16; FULL_DISTANCES - END_POS_MODEL_INDEX as usize],
    pos_align: [u16; ALIGN_TABLE_SIZE],
    match_len: LengthDecoder,
    rep_len: LengthDecoder,

    state: u32,
    rep0: u32,
    rep1: u32,
    rep2: u32,
    rep3: u32,

    pos_mask: u32,
    literal_context_bits: u32,
    literal_pos_mask: u32,
}

impl Lzma1Decoder {
    pub fn new(props: Properties) -> Self {
        let mut decoder = Lzma1Decoder {
            literal: vec![[PROB_INIT; LITERAL_CODER_SIZE]; LITERAL_CODERS_MAX],
            is_match: [[PROB_INIT; POS_STATES_MAX]; STATES],
            is_rep: [PROB_INIT; STATES],
            is_rep0: [PROB_INIT; STATES],
            is_rep1: [PROB_INIT; STATES],
            is_rep2: [PROB_INIT; STATES],
            is_rep0_long: [[PROB_INIT; POS_STATES_MAX]; STATES],
            pos_slot: [[PROB_INIT; POS_SLOTS]; LEN_TO_POS_STATES],
            pos_special: [PROB_INIT; FULL_DISTANCES - END_POS_MODEL_INDEX as usize],
            pos_align: [PROB_INIT; ALIGN_TABLE_SIZE],
            match_len: LengthDecoder::new(),
            rep_len: LengthDecoder::new(),
            state: 0,
            rep0: 0,
            rep1: 0,
            rep2: 0,
            rep3: 0,
            pos_mask: 0,
            literal_context_bits: 0,
            literal_pos_mask: 0,
        };
        decoder.reset(props);
        decoder
    }

    pub fn reset(&mut self, props: Properties) {
        self.pos_mask = (1 << props.pb) - 1;
        self.literal_context_bits = props.lc;
        self.literal_pos_mask = (1 << props.lp) - 1;

        for coder in self.literal.iter_mut() {
            coder.fill(PROB_INIT);
        }

        self.is_match = [[PROB_INIT; POS_STATES_MAX]; STATES];
        self.is_rep = [PROB_INIT; STATES];
        self.is_rep0 = [PROB_INIT; STATES];
        self.is_rep1 = [PROB_INIT; STATES];
        self.is_rep2 = [PROB_INIT; STATES];
        self.is_rep0_long = [[PROB_INIT; POS_STATES_MAX]; STATES];
        self.pos_slot = [[PROB_INIT; POS_SLOTS]; LEN_TO_POS_STATES];
        self.pos_special = [PROB_INIT; FULL_DISTANCES - END_POS_MODEL_INDEX as usize];
        self.pos_align = [PROB_INIT; ALIGN_TABLE_SIZE];
        self.match_len.reset();
        self.rep_len.reset();

        self.state = 0;
        self.rep0 = 0;
        self.rep1 = 0;
        self.rep2 = 0;
        self.rep3 = 0;
    }

    fn decode_literal(&mut self, rc: &mut RangeDecoder, dict: &Dictionary) -> u8 {
        let prev_byte = if dict.available() > 0 { dict.byte_at_distance(0) } else { 0 };

        let index = (((dict.position() as u32 & self.literal_pos_mask) << self.literal_context_bits)
            + (u32::from(prev_byte) >> (8 - self.literal_context_bits))) as usize;

        let index = index.min(self.literal.len() - 1);
        let probs = &mut self.literal[index];
        let mut symbol = 1usize;

        if self.state < LIT_STATES {
            while symbol < 0x100 {
                let bit = rc.decode_bit(&mut probs[symbol]);
                symbol = (symbol << 1) | bit as usize;
            }
        } else {
            let mut match_byte = u32::from(dict.byte_at_distance(self.rep0)) << 1;
            let mut offset = 0x100usize;

            while symbol < 0x100 {
                let match_bit = match_byte as usize & offset;
                let bit = rc.decode_bit(&mut probs[offset + match_bit + symbol]);
                symbol = (symbol << 1) | bit as usize;

                if bit == 0 {
                    offset &= !match_bit;
                } else {
                    offset &= match_bit;
                }

                match_byte <<= 1;
            }
        }

        self.state = LITERAL_NEXT_STATE[self.state as usize];

        symbol as u8
    }

    fn decode_distance(&mut self, rc: &mut RangeDecoder, len: u32) -> u32 {
        let len_state = if (len as usize) < LEN_TO_POS_STATES + MATCH_LEN_MIN as usize {
            (len - MATCH_LEN_MIN) as usize
        } else {
            LEN_TO_POS_STATES - 1
        };

        let pos_slot = rc.decode_bittree(&mut self.pos_slot[len_state]);

        if pos_slot < START_POS_MODEL_INDEX {
            return pos_slot;
        }

        let direct_bits = (pos_slot >> 1) - 1;
        let mut distance = (2 | (pos_slot & 1)) << direct_bits;

        if pos_slot < END_POS_MODEL_INDEX {
            let base = (distance - pos_slot) as usize;
            distance += decode_reverse_offset(rc, &mut self.pos_special, base, direct_bits);
        } else {
            distance += rc.decode_direct_bits(direct_bits - ALIGN_BITS) << ALIGN_BITS;
            distance += rc.decode_bittree_reverse(&mut self.pos_align, ALIGN_BITS);
        }

        distance
    }

    pub fn decode(
        &mut self,
        rc: &mut RangeDecoder,
        dict: &mut Dictionary,
        uncompressed_limit: Option<usize>,
    ) -> Status {
        let start = dict.position();

        loop {
            if let Some(limit) = uncompressed_limit {
                if dict.position() - start >= limit {
                    return Status::Finished;
                }
            }

            if dict.is_full() {
                return Status::OutputFull;
            }

            if rc.is_truncated() {
                return Status::NeedsInput;
            }

            let pos_state = (dict.position() as u32 & self.pos_mask) as usize;

            if rc.decode_bit(&mut self.is_match[self.state as usize][pos_state]) == 0 {
                let byte = self.decode_literal(rc, dict);
                if !dict.push(byte) {
                    return Status::OutputFull;
                }
                continue;
            }

            let len;

            if rc.decode_bit(&mut self.is_rep[self.state as usize]) == 0 {
                self.rep3 = self.rep2;
                self.rep2 = self.rep1;
                self.rep1 = self.rep0;

                self.state = if self.state < LIT_STATES { 7 } else { 10 };

                let match_len = self.match_len.decode(rc, pos_state);
                let distance = self.decode_distance(rc, match_len);

                if distance == u32::MAX {
                    return if rc.is_truncated() { Status::NeedsInput } else { Status::EndMarker };
                }

                self.rep0 = distance;
                len = match_len;

                if !dict.is_distance_valid(self.rep0) {
                    return Status::Corrupt;
                }
            } else {
                if dict.available() == 0 {
                    return Status::Corrupt;
                }

                if rc.decode_bit(&mut self.is_rep0[self.state as usize]) == 0 {
                    if rc.decode_bit(&mut self.is_rep0_long[self.state as usize][pos_state]) == 0 {
                        self.state = if self.state < LIT_STATES { 9 } else { 11 };
                        let byte = dict.byte_at_distance(self.rep0);
                        if !dict.push(byte) {
                            return Status::OutputFull;
                        }
                        continue;
                    }
                } else if rc.decode_bit(&mut self.is_rep1[self.state as usize]) == 0 {
                    std::mem::swap(&mut self.rep1, &mut self.rep0);
                } else if rc.decode_bit(&mut self.is_rep2[self.state as usize]) == 0 {
                    let distance = self.rep2;
                    self.rep2 = self.rep1;
                    self.rep1 = self.rep0;
                    self.rep0 = distance;
                } else {
                    let distance = self.rep3;
                    self.rep3 = self.rep2;
                    self.rep2 = self.rep1;
                    self.rep1 = self.rep0;
                    self.rep0 = distance;
                }

                self.state = if self.state < LIT_STATES { 8 } else { 11 };
                len = self.rep_len.decode(rc, pos_state);
            }

            let mut wanted = len;
            if let Some(limit) = uncompressed_limit {
                let produced = dict.position() - start;
                wanted = wanted.min((limit - produced) as u32);
            }

            let copied = dict.copy_match(self.rep0, wanted);
            if copied < wanted {
                return Status::OutputFull;
            }
        }
    }
}

fn decode_reverse_offset(rc: &mut RangeDecoder, probs: &mut [u16], base: usize, count: u32) -> u32 {
    let mut symbol = 1usize;
    let mut result = 0u32;

    for offset in 0..count {
        let index = base + symbol - 1;
        if index >= probs.len() {
            return result;
        }
        let bit = rc.decode_bit(&mut probs[index]);
        symbol = (symbol << 1) | bit as usize;
        result |= bit << offset;
    }

    result
}
