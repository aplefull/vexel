use crate::log_warn;
use crate::utils::zstd::bitstream::{BackwardBits, ForwardBits};

pub const MAX_ACCURACY_LOG: u32 = 15;
pub const MAX_SYMBOLS: usize = 256;

#[derive(Clone, Default)]
pub struct FseTable {
    symbols: Vec<u8>,
    num_bits: Vec<u8>,
    new_state_base: Vec<u16>,
    accuracy_log: u32,
}

impl FseTable {
    pub fn is_initialized(&self) -> bool {
        !self.symbols.is_empty()
    }

    pub fn rle(symbol: u8) -> Self {
        FseTable { symbols: vec![symbol], num_bits: vec![0], new_state_base: vec![0], accuracy_log: 0 }
    }

    pub fn from_distribution(frequencies: &[i16], accuracy_log: u32) -> Option<Self> {
        if accuracy_log > MAX_ACCURACY_LOG || frequencies.len() > MAX_SYMBOLS {
            log_warn!("zstd FSE table with invalid accuracy log {}", accuracy_log);
            return None;
        }

        let size = 1usize << accuracy_log;
        let mut symbols = vec![0u8; size];
        let mut state_desc = vec![0u16; frequencies.len()];

        let mut high_threshold = size;
        for (symbol, &freq) in frequencies.iter().enumerate() {
            if freq == -1 {
                high_threshold -= 1;
                symbols[high_threshold] = symbol as u8;
                state_desc[symbol] = 1;
            }
        }

        let step = (size >> 1) + (size >> 3) + 3;
        let mask = size - 1;
        let mut pos = 0usize;

        for (symbol, &freq) in frequencies.iter().enumerate() {
            if freq <= 0 {
                continue;
            }

            state_desc[symbol] = freq as u16;

            for _ in 0..freq {
                if pos >= high_threshold {
                    log_warn!("zstd FSE distribution overflows table");
                    return None;
                }

                symbols[pos] = symbol as u8;

                loop {
                    pos = (pos + step) & mask;
                    if pos < high_threshold {
                        break;
                    }
                }
            }
        }

        if pos != 0 {
            log_warn!("zstd FSE distribution did not fill table exactly");
        }

        let mut num_bits = vec![0u8; size];
        let mut new_state_base = vec![0u16; size];

        for index in 0..size {
            let symbol = symbols[index] as usize;
            let next = state_desc[symbol];
            state_desc[symbol] = next.wrapping_add(1);

            let highest = if next == 0 { 0 } else { 15 - next.leading_zeros() };
            let bits = accuracy_log.saturating_sub(highest);

            num_bits[index] = bits as u8;
            new_state_base[index] = (u32::from(next) << bits).wrapping_sub(size as u32) as u16;
        }

        Some(FseTable { symbols, num_bits, new_state_base, accuracy_log })
    }

    pub fn parse_header(input: &mut ForwardBits, max_accuracy_log: u32) -> Option<Self> {
        let accuracy_log = 5 + input.read(4) as u32;
        if accuracy_log > max_accuracy_log {
            log_warn!("zstd FSE accuracy log {} exceeds maximum {}", accuracy_log, max_accuracy_log);
            return None;
        }

        let mut remaining = 1i32 << accuracy_log;
        let mut frequencies = Vec::with_capacity(MAX_SYMBOLS);

        while remaining > 0 && frequencies.len() < MAX_SYMBOLS {
            let bits = 32 - (remaining as u32 + 1).leading_zeros();
            let mut value = input.read(bits) as u16;

            let lower_mask = (1u16 << (bits - 1)) - 1;
            let threshold = ((1u32 << bits) - 1 - (remaining as u32 + 1)) as u16;

            if (value & lower_mask) < threshold {
                input.rewind(1);
                value &= lower_mask;
            } else if value > lower_mask {
                value -= threshold;
            }

            let probability = value as i16 - 1;
            remaining -= i32::from(probability.abs());
            frequencies.push(probability);

            if probability == 0 {
                let mut repeat = input.read(2) as usize;

                loop {
                    for _ in 0..repeat {
                        if frequencies.len() >= MAX_SYMBOLS {
                            break;
                        }
                        frequencies.push(0);
                    }

                    if repeat == 3 {
                        repeat = input.read(2) as usize;
                    } else {
                        break;
                    }
                }
            }

            if input.is_truncated() {
                log_warn!("zstd FSE header truncated");
                return None;
            }
        }

        input.align();

        if remaining != 0 {
            log_warn!("zstd FSE distribution does not sum to 1 << accuracy_log");
            return None;
        }

        Self::from_distribution(&frequencies, accuracy_log)
    }

    pub fn peek(&self, state: u16) -> u8 {
        self.symbols.get(state as usize).copied().unwrap_or(0)
    }

    pub fn init_state(&self, stream: &mut BackwardBits) -> u16 {
        stream.read(self.accuracy_log) as u16
    }

    pub fn update_state(&self, state: &mut u16, stream: &mut BackwardBits) {
        let index = *state as usize;
        let bits = self.num_bits.get(index).copied().unwrap_or(0);
        let base = self.new_state_base.get(index).copied().unwrap_or(0);
        let rest = stream.read(u32::from(bits)) as u16;

        *state = base.wrapping_add(rest);
    }

    pub fn decode_symbol(&self, state: &mut u16, stream: &mut BackwardBits) -> u8 {
        let symbol = self.peek(*state);
        self.update_state(state, stream);
        symbol
    }

    pub fn decompress_interleaved(&self, data: &[u8], output: &mut Vec<u8>, limit: usize) -> usize {
        let Some(mut stream) = BackwardBits::new(data) else {
            log_warn!("zstd FSE interleaved stream has no final bit marker");
            return 0;
        };

        let mut state1 = self.init_state(&mut stream);
        let mut state2 = self.init_state(&mut stream);
        let mut written = 0usize;

        while written < limit {
            output.push(self.decode_symbol(&mut state1, &mut stream));
            written += 1;

            if stream.is_exhausted() {
                if written < limit {
                    output.push(self.peek(state2));
                    written += 1;
                }
                break;
            }

            if written >= limit {
                break;
            }

            output.push(self.decode_symbol(&mut state2, &mut stream));
            written += 1;

            if stream.is_exhausted() {
                if written < limit {
                    output.push(self.peek(state1));
                    written += 1;
                }
                break;
            }
        }

        written
    }
}
