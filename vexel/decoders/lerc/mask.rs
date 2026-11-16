use crate::log_warn;
use crate::utils::codecs::rle::{self, Packet};

#[derive(Debug, Clone)]
pub struct BitMask {
    bits: Vec<u8>,
    n_cols: usize,
    n_rows: usize,
}

impl BitMask {
    pub fn new(n_cols: usize, n_rows: usize) -> Self {
        let size = (n_cols * n_rows).div_ceil(8);
        BitMask { bits: vec![0; size], n_cols, n_rows }
    }

    pub fn all_valid(n_cols: usize, n_rows: usize) -> Self {
        let size = (n_cols * n_rows).div_ceil(8);
        BitMask { bits: vec![0xff; size], n_cols, n_rows }
    }

    pub fn bits_mut(&mut self) -> &mut [u8] {
        &mut self.bits
    }

    pub fn is_valid(&self, k: usize) -> bool {
        let byte = k >> 3;
        let bit = (1u8 << 7) >> (k & 7);
        self.bits.get(byte).map(|b| b & bit != 0).unwrap_or(false)
    }

    pub fn count_valid(&self) -> usize {
        let total = self.n_cols * self.n_rows;
        (0..total).filter(|&k| self.is_valid(k)).count()
    }

    pub fn is_all_valid(&self) -> bool {
        self.count_valid() == self.n_cols * self.n_rows
    }
}

pub fn rle_decompress(src: &[u8], out: &mut [u8]) -> bool {
    let mut decoded = Vec::with_capacity(out.len());
    let result = rle::decode(src, &mut decoded, out.len(), 1, |input| {
        let header = input.get(..2)?;
        let count = i16::from_le_bytes([header[0], header[1]]);
        *input = &input[2..];
        Some(match count {
            i16::MIN => Packet::End,
            1.. => Packet::Literal(count as usize),
            _ => Packet::Run(count.unsigned_abs() as usize),
        })
    });

    if !result.ended && decoded.len() < out.len() {
        log_warn!("LERC mask RLE stream ended without a terminator, using {} decoded bytes", decoded.len());
    }

    out[..decoded.len()].copy_from_slice(&decoded);
    !decoded.is_empty()
}
