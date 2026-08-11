use crate::log_warn;
use crate::utils::zstd::bitstream::{BackwardBits, ForwardBits};
use crate::utils::zstd::fse::FseTable;

pub const LITERAL_LENGTH_DEFAULT_DIST: [i16; 36] =
    [4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1];
pub const OFFSET_DEFAULT_DIST: [i16; 29] =
    [1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1];
pub const MATCH_LENGTH_DEFAULT_DIST: [i16; 53] = [
    1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
];

const LITERAL_LENGTH_BASELINES: [u32; 36] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32, 40, 48, 64, 128, 256, 512, 1024,
    2048, 4096, 8192, 16384, 32768, 65536,
];
const LITERAL_LENGTH_EXTRA_BITS: [u8; 36] =
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

const MATCH_LENGTH_BASELINES: [u32; 53] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 37, 39, 41, 43, 47, 51, 59, 67, 83, 99, 131, 259, 515, 1027, 2051, 4099, 8195, 16387, 32771, 65539,
];
const MATCH_LENGTH_EXTRA_BITS: [u8; 53] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2,
    3, 3, 4, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
];

const MAX_LITERAL_LENGTH_CODE: u8 = 35;
const MAX_MATCH_LENGTH_CODE: u8 = 52;

#[derive(Clone, Copy)]
pub struct Sequence {
    pub literal_length: u32,
    pub match_length: u32,
    pub offset: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SeqPart {
    LiteralLength,
    Offset,
    MatchLength,
}

impl SeqPart {
    fn default_distribution(self) -> &'static [i16] {
        match self {
            SeqPart::LiteralLength => &LITERAL_LENGTH_DEFAULT_DIST,
            SeqPart::Offset => &OFFSET_DEFAULT_DIST,
            SeqPart::MatchLength => &MATCH_LENGTH_DEFAULT_DIST,
        }
    }

    fn default_accuracy_log(self) -> u32 {
        match self {
            SeqPart::Offset => 5,
            _ => 6,
        }
    }

    fn max_accuracy_log(self) -> u32 {
        match self {
            SeqPart::Offset => 8,
            _ => 9,
        }
    }
}

pub fn decode_table(table: &mut FseTable, input: &mut ForwardBits, data: &[u8], part: SeqPart, mode: u8) -> Option<()> {
    match mode {
        0 => {
            *table = FseTable::from_distribution(part.default_distribution(), part.default_accuracy_log())?;
        }
        1 => {
            let start = input.bytes_consumed();
            let Some(&symbol) = data.get(start) else {
                log_warn!("zstd RLE sequence table symbol truncated");
                return None;
            };

            input.read(8);
            *table = FseTable::rle(symbol);
        }
        2 => {
            let start = input.bytes_consumed();
            if start > data.len() {
                log_warn!("zstd FSE sequence table starts past end of block");
                return None;
            }

            let mut sub = ForwardBits::new(&data[start..]);
            *table = FseTable::parse_header(&mut sub, part.max_accuracy_log())?;
            input.skip_bytes(sub.bytes_consumed());
        }
        _ => {
            if !table.is_initialized() {
                log_warn!("zstd repeat sequence table mode without a previous table");
                return None;
            }
        }
    }

    Some(())
}

pub fn decode_sequences(
    data: &[u8],
    count: usize,
    ll_table: &FseTable,
    of_table: &FseTable,
    ml_table: &FseTable,
) -> Vec<Sequence> {
    let Some(mut stream) = BackwardBits::new(data) else {
        log_warn!("zstd sequence bitstream has no final bit marker");
        return Vec::new();
    };

    let mut ll_state = ll_table.init_state(&mut stream);
    let mut of_state = of_table.init_state(&mut stream);
    let mut ml_state = ml_table.init_state(&mut stream);

    let mut sequences = Vec::with_capacity(count.min(1 << 16));

    for index in 0..count {
        let of_code = of_table.peek(of_state);
        let ll_code = ll_table.peek(ll_state);
        let ml_code = ml_table.peek(ml_state);

        if ll_code > MAX_LITERAL_LENGTH_CODE || ml_code > MAX_MATCH_LENGTH_CODE || of_code >= 32 {
            log_warn!("zstd sequence code out of range, keeping {} sequences", sequences.len());
            break;
        }

        let offset = (1u32 << of_code) + stream.read(u32::from(of_code)) as u32;
        let match_length = MATCH_LENGTH_BASELINES[ml_code as usize]
            + stream.read(u32::from(MATCH_LENGTH_EXTRA_BITS[ml_code as usize])) as u32;
        let literal_length = LITERAL_LENGTH_BASELINES[ll_code as usize]
            + stream.read(u32::from(LITERAL_LENGTH_EXTRA_BITS[ll_code as usize])) as u32;

        sequences.push(Sequence { literal_length, match_length, offset });

        if index + 1 < count {
            ll_table.update_state(&mut ll_state, &mut stream);
            ml_table.update_state(&mut ml_state, &mut stream);
            of_table.update_state(&mut of_state, &mut stream);
        }
    }

    if sequences.len() == count && stream.offset() != 0 {
        log_warn!("zstd sequence bitstream not fully consumed");
    }

    sequences
}
