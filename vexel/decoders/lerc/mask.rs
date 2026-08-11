use crate::log_warn;

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
    let mut pos = 0usize;
    let mut idx = 0usize;
    let mut complete = false;

    while pos + 2 <= src.len() {
        let cnt = i16::from_le_bytes([src[pos], src[pos + 1]]);
        pos += 2;

        if cnt == -32768 {
            complete = true;
            break;
        }

        if cnt > 0 {
            let count = cnt as usize;
            let available = src.len().saturating_sub(pos).min(count);
            let writable = out.len().saturating_sub(idx).min(available);

            if writable > 0 {
                out[idx..idx + writable].copy_from_slice(&src[pos..pos + writable]);
                idx += writable;
            }

            pos += available;

            if available < count || writable < available {
                break;
            }
        } else {
            let count = (-(cnt as i32)) as usize;

            if pos >= src.len() {
                break;
            }

            let value = src[pos];
            pos += 1;

            let writable = out.len().saturating_sub(idx).min(count);
            if writable > 0 {
                out[idx..idx + writable].fill(value);
                idx += writable;
            }

            if writable < count {
                break;
            }
        }
    }

    if !complete {
        log_warn!("LERC mask RLE stream ended without a terminator, using {} decoded bytes", idx);
    }

    idx > 0
}
