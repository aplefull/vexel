use crate::log_warn;
use crate::utils::zstd::bitstream::{BackwardBits, ForwardBits};
use crate::utils::zstd::fse::FseTable;

const MAX_BITS: u32 = 16;
const MAX_SYMBOLS: usize = 256;
const WEIGHTS_MAX_ACCURACY_LOG: u32 = 6;

#[derive(Clone, Default)]
pub struct HuffmanTable {
    symbols: Vec<u8>,
    num_bits: Vec<u8>,
    max_bits: u32,
}

impl HuffmanTable {
    pub fn is_initialized(&self) -> bool {
        !self.symbols.is_empty()
    }

    fn from_bit_counts(bits: &[u8]) -> Option<Self> {
        let mut max_bits = 0u32;
        let mut rank_count = [0u32; MAX_BITS as usize + 1];

        for &count in bits {
            if u32::from(count) > MAX_BITS {
                log_warn!("zstd Huffman code length {} too large", count);
                return None;
            }

            max_bits = max_bits.max(u32::from(count));
            rank_count[count as usize] += 1;
        }

        if max_bits == 0 {
            log_warn!("zstd Huffman table has no codes");
            return None;
        }

        let table_size = 1usize << max_bits;
        let mut symbols = vec![0u8; table_size];
        let mut num_bits = vec![0u8; table_size];
        let mut rank_idx = [0u32; MAX_BITS as usize + 2];

        rank_idx[max_bits as usize] = 0;
        for rank in (1..=max_bits as usize).rev() {
            rank_idx[rank - 1] = rank_idx[rank] + rank_count[rank] * (1 << (max_bits as usize - rank));

            let start = rank_idx[rank] as usize;
            let end = (rank_idx[rank - 1] as usize).min(table_size);
            if start < end {
                num_bits[start..end].fill(rank as u8);
            }
        }

        if rank_idx[0] as usize != table_size {
            log_warn!("zstd Huffman code lengths do not form a complete tree");
            return None;
        }

        for (symbol, &length) in bits.iter().enumerate() {
            if length == 0 {
                continue;
            }

            let code = rank_idx[length as usize] as usize;
            let span = 1usize << (max_bits - u32::from(length));
            let end = (code + span).min(table_size);

            if code < end {
                symbols[code..end].fill(symbol as u8);
            }

            rank_idx[length as usize] += span as u32;
        }

        Some(HuffmanTable { symbols, num_bits, max_bits })
    }

    fn from_weights(weights: &[u8]) -> Option<Self> {
        if weights.len() + 1 > MAX_SYMBOLS {
            log_warn!("zstd Huffman weight count {} too large", weights.len());
            return None;
        }

        let mut weight_sum = 0u64;
        for &weight in weights {
            if u32::from(weight) > MAX_BITS {
                log_warn!("zstd Huffman weight {} out of range", weight);
                return None;
            }

            if weight > 0 {
                weight_sum += 1u64 << (weight - 1);
            }
        }

        if weight_sum == 0 {
            log_warn!("zstd Huffman weights sum to zero");
            return None;
        }

        let max_bits = 64 - weight_sum.leading_zeros();
        let left_over = (1u64 << max_bits) - weight_sum;

        if left_over == 0 || left_over & (left_over - 1) != 0 {
            log_warn!("zstd Huffman weights do not resolve to a power of two remainder");
            return None;
        }

        let last_weight = 64 - left_over.leading_zeros();

        let mut bits = Vec::with_capacity(weights.len() + 1);
        for &weight in weights {
            bits.push(if weight > 0 { (max_bits + 1 - u32::from(weight)) as u8 } else { 0 });
        }
        bits.push((max_bits + 1 - last_weight) as u8);

        Self::from_bit_counts(&bits)
    }

    pub fn parse_description(input: &mut ForwardBits, data: &[u8]) -> Option<Self> {
        let header = input.read(8) as u8;

        if header >= 128 {
            let num_symbols = usize::from(header) - 127;
            let bytes = num_symbols.div_ceil(2);
            let start = input.bytes_consumed();
            let end = start + bytes;

            if end > data.len() {
                log_warn!("zstd Huffman direct weights truncated");
                return None;
            }

            let mut weights = Vec::with_capacity(num_symbols);
            for index in 0..num_symbols {
                let byte = data[start + index / 2];
                weights.push(if index % 2 == 0 { byte >> 4 } else { byte & 0xf });
            }

            input.skip_bytes(bytes);

            Self::from_weights(&weights)
        } else {
            let start = input.bytes_consumed();
            let end = start + usize::from(header);

            if end > data.len() {
                log_warn!("zstd Huffman FSE weight stream truncated");
                return None;
            }

            let fse_data = &data[start..end];
            let mut fse_input = ForwardBits::new(fse_data);
            let table = FseTable::parse_header(&mut fse_input, WEIGHTS_MAX_ACCURACY_LOG)?;

            let consumed = fse_input.bytes_consumed();
            if consumed > fse_data.len() {
                log_warn!("zstd Huffman FSE header overran its stream");
                return None;
            }

            let mut weights = Vec::with_capacity(MAX_SYMBOLS);
            table.decompress_interleaved(&fse_data[consumed..], &mut weights, MAX_SYMBOLS - 1);

            input.skip_bytes(usize::from(header));

            Self::from_weights(&weights)
        }
    }

    fn init_state(&self, stream: &mut BackwardBits) -> u16 {
        stream.read(self.max_bits) as u16
    }

    fn decode_symbol(&self, state: &mut u16, stream: &mut BackwardBits) -> u8 {
        let index = *state as usize;
        let symbol = self.symbols.get(index).copied().unwrap_or(0);
        let bits = self.num_bits.get(index).copied().unwrap_or(0);
        let rest = stream.read(u32::from(bits)) as u32;
        let mask = (1u32 << self.max_bits) - 1;

        *state = (((u32::from(*state) << bits).wrapping_add(rest)) & mask) as u16;

        symbol
    }

    fn decompress_stream(&self, data: &[u8], output: &mut Vec<u8>, limit: usize) -> usize {
        let Some(mut stream) = BackwardBits::new(data) else {
            log_warn!("zstd Huffman stream has no final bit marker");
            return 0;
        };

        let mut state = self.init_state(&mut stream);
        let mut written = 0usize;

        while stream.offset() > -(self.max_bits as i64) && written < limit {
            output.push(self.decode_symbol(&mut state, &mut stream));
            written += 1;
        }

        written
    }

    pub fn decompress_single(&self, data: &[u8], output: &mut Vec<u8>, limit: usize) -> usize {
        self.decompress_stream(data, output, limit)
    }

    pub fn decompress_four(&self, data: &[u8], output: &mut Vec<u8>, limit: usize) -> usize {
        if data.len() < 6 {
            log_warn!("zstd Huffman 4-stream jump table truncated");
            return 0;
        }

        let size1 = u16::from_le_bytes([data[0], data[1]]) as usize;
        let size2 = u16::from_le_bytes([data[2], data[3]]) as usize;
        let size3 = u16::from_le_bytes([data[4], data[5]]) as usize;

        let body = &data[6..];
        let Some(size4) = body.len().checked_sub(size1 + size2 + size3) else {
            log_warn!("zstd Huffman 4-stream sizes exceed available data");
            return 0;
        };

        let mut written = 0usize;
        let mut start = 0usize;

        for size in [size1, size2, size3, size4] {
            let end = start + size;
            written += self.decompress_stream(&body[start..end], output, limit.saturating_sub(written));
            start = end;
        }

        written
    }
}
