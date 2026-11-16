use crate::utils::entropy::mq::{MqContext, MqDecoder};

const MAX_SYMBOL_CODE_LENGTH: u32 = 22;

pub struct IntCtx {
    cx: Vec<MqContext>,
}

impl IntCtx {
    pub fn new() -> Self {
        IntCtx { cx: vec![MqContext::default(); 512] }
    }

    fn decode_bit(&mut self, arith: &mut MqDecoder, prev: &mut usize) -> u32 {
        let d = arith.decode(&mut self.cx[*prev]);
        *prev = if *prev < 256 { (*prev << 1) | d as usize } else { (((*prev << 1) | d as usize) & 511) | 256 };
        d
    }

    pub fn decode(&mut self, arith: &mut MqDecoder) -> Option<i32> {
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

        let magnitude = v as i64 + offset as i64;

        if s == 1 && magnitude == 0 {
            return None;
        }

        let value = if s == 1 { -magnitude } else { magnitude };
        Some(value.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }
}

pub struct IaidCtx {
    cx: Vec<MqContext>,
    sym_code_len: u32,
}

impl IaidCtx {
    pub fn new(sym_code_len: u32) -> Self {
        let sym_code_len = sym_code_len.min(MAX_SYMBOL_CODE_LENGTH);
        IaidCtx { cx: vec![MqContext::default(); 1usize << sym_code_len], sym_code_len }
    }

    pub fn decode(&mut self, arith: &mut MqDecoder) -> u32 {
        let mut prev = 1usize;
        for _ in 0..self.sym_code_len {
            let d = arith.decode(&mut self.cx[prev]);
            prev = (prev << 1) | d as usize;
        }
        prev as u32 - (1u32 << self.sym_code_len)
    }
}
