use std::sync::OnceLock;

use crate::log_warn;
use crate::utils::bits::MsbBitReader;
use crate::utils::entropy::prefix::PrefixCodeTable;

const EOL_RUN: i16 = -1;
const INVALID_RUN: i16 = -2;

const EOL_CODE: u32 = 0x001;
const EOFB_CODE: u32 = 0x001001;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaxMode {
    ModifiedHuffman { word_aligned: bool },
    Group3 { two_dimensional: bool },
    Group4,
}

pub struct FaxOutput {
    pub pixels: Vec<u8>,
    pub clean_rows: usize,
    pub bytes_used: usize,
}

const WHITE_CODES: &[(u8, u16, i16)] = &[
    (8, 0x35, 0),
    (6, 0x7, 1),
    (4, 0x7, 2),
    (4, 0x8, 3),
    (4, 0xB, 4),
    (4, 0xC, 5),
    (4, 0xE, 6),
    (4, 0xF, 7),
    (5, 0x13, 8),
    (5, 0x14, 9),
    (5, 0x7, 10),
    (5, 0x8, 11),
    (6, 0x8, 12),
    (6, 0x3, 13),
    (6, 0x34, 14),
    (6, 0x35, 15),
    (6, 0x2A, 16),
    (6, 0x2B, 17),
    (7, 0x27, 18),
    (7, 0xC, 19),
    (7, 0x8, 20),
    (7, 0x17, 21),
    (7, 0x3, 22),
    (7, 0x4, 23),
    (7, 0x28, 24),
    (7, 0x2B, 25),
    (7, 0x13, 26),
    (7, 0x24, 27),
    (7, 0x18, 28),
    (8, 0x2, 29),
    (8, 0x3, 30),
    (8, 0x1A, 31),
    (8, 0x1B, 32),
    (8, 0x12, 33),
    (8, 0x13, 34),
    (8, 0x14, 35),
    (8, 0x15, 36),
    (8, 0x16, 37),
    (8, 0x17, 38),
    (8, 0x28, 39),
    (8, 0x29, 40),
    (8, 0x2A, 41),
    (8, 0x2B, 42),
    (8, 0x2C, 43),
    (8, 0x2D, 44),
    (8, 0x4, 45),
    (8, 0x5, 46),
    (8, 0xA, 47),
    (8, 0xB, 48),
    (8, 0x52, 49),
    (8, 0x53, 50),
    (8, 0x54, 51),
    (8, 0x55, 52),
    (8, 0x24, 53),
    (8, 0x25, 54),
    (8, 0x58, 55),
    (8, 0x59, 56),
    (8, 0x5A, 57),
    (8, 0x5B, 58),
    (8, 0x4A, 59),
    (8, 0x4B, 60),
    (8, 0x32, 61),
    (8, 0x33, 62),
    (8, 0x34, 63),
    (5, 0x1B, 64),
    (5, 0x12, 128),
    (6, 0x17, 192),
    (7, 0x37, 256),
    (8, 0x36, 320),
    (8, 0x37, 384),
    (8, 0x64, 448),
    (8, 0x65, 512),
    (8, 0x68, 576),
    (8, 0x67, 640),
    (9, 0xCC, 704),
    (9, 0xCD, 768),
    (9, 0xD2, 832),
    (9, 0xD3, 896),
    (9, 0xD4, 960),
    (9, 0xD5, 1024),
    (9, 0xD6, 1088),
    (9, 0xD7, 1152),
    (9, 0xD8, 1216),
    (9, 0xD9, 1280),
    (9, 0xDA, 1344),
    (9, 0xDB, 1408),
    (9, 0x98, 1472),
    (9, 0x99, 1536),
    (9, 0x9A, 1600),
    (6, 0x18, 1664),
    (9, 0x9B, 1728),
    (11, 0x8, 1792),
    (11, 0xC, 1856),
    (11, 0xD, 1920),
    (12, 0x12, 1984),
    (12, 0x13, 2048),
    (12, 0x14, 2112),
    (12, 0x15, 2176),
    (12, 0x16, 2240),
    (12, 0x17, 2304),
    (12, 0x1C, 2368),
    (12, 0x1D, 2432),
    (12, 0x1E, 2496),
    (12, 0x1F, 2560),
    (12, 0x1, EOL_RUN),
    (12, 0x0, INVALID_RUN),
];

const BLACK_CODES: &[(u8, u16, i16)] = &[
    (10, 0x37, 0),
    (3, 0x2, 1),
    (2, 0x3, 2),
    (2, 0x2, 3),
    (3, 0x3, 4),
    (4, 0x3, 5),
    (4, 0x2, 6),
    (5, 0x3, 7),
    (6, 0x5, 8),
    (6, 0x4, 9),
    (7, 0x4, 10),
    (7, 0x5, 11),
    (7, 0x7, 12),
    (8, 0x4, 13),
    (8, 0x7, 14),
    (9, 0x18, 15),
    (10, 0x17, 16),
    (10, 0x18, 17),
    (10, 0x8, 18),
    (11, 0x67, 19),
    (11, 0x68, 20),
    (11, 0x6C, 21),
    (11, 0x37, 22),
    (11, 0x28, 23),
    (11, 0x17, 24),
    (11, 0x18, 25),
    (12, 0xCA, 26),
    (12, 0xCB, 27),
    (12, 0xCC, 28),
    (12, 0xCD, 29),
    (12, 0x68, 30),
    (12, 0x69, 31),
    (12, 0x6A, 32),
    (12, 0x6B, 33),
    (12, 0xD2, 34),
    (12, 0xD3, 35),
    (12, 0xD4, 36),
    (12, 0xD5, 37),
    (12, 0xD6, 38),
    (12, 0xD7, 39),
    (12, 0x6C, 40),
    (12, 0x6D, 41),
    (12, 0xDA, 42),
    (12, 0xDB, 43),
    (12, 0x54, 44),
    (12, 0x55, 45),
    (12, 0x56, 46),
    (12, 0x57, 47),
    (12, 0x64, 48),
    (12, 0x65, 49),
    (12, 0x52, 50),
    (12, 0x53, 51),
    (12, 0x24, 52),
    (12, 0x37, 53),
    (12, 0x38, 54),
    (12, 0x27, 55),
    (12, 0x28, 56),
    (12, 0x58, 57),
    (12, 0x59, 58),
    (12, 0x2B, 59),
    (12, 0x2C, 60),
    (12, 0x5A, 61),
    (12, 0x66, 62),
    (12, 0x67, 63),
    (10, 0xF, 64),
    (12, 0xC8, 128),
    (12, 0xC9, 192),
    (12, 0x5B, 256),
    (12, 0x33, 320),
    (12, 0x34, 384),
    (12, 0x35, 448),
    (13, 0x6C, 512),
    (13, 0x6D, 576),
    (13, 0x4A, 640),
    (13, 0x4B, 704),
    (13, 0x4C, 768),
    (13, 0x4D, 832),
    (13, 0x72, 896),
    (13, 0x73, 960),
    (13, 0x74, 1024),
    (13, 0x75, 1088),
    (13, 0x76, 1152),
    (13, 0x77, 1216),
    (13, 0x52, 1280),
    (13, 0x53, 1344),
    (13, 0x54, 1408),
    (13, 0x55, 1472),
    (13, 0x5A, 1536),
    (13, 0x5B, 1600),
    (13, 0x64, 1664),
    (13, 0x65, 1728),
    (11, 0x8, 1792),
    (11, 0xC, 1856),
    (11, 0xD, 1920),
    (12, 0x12, 1984),
    (12, 0x13, 2048),
    (12, 0x14, 2112),
    (12, 0x15, 2176),
    (12, 0x16, 2240),
    (12, 0x17, 2304),
    (12, 0x1C, 2368),
    (12, 0x1D, 2432),
    (12, 0x1E, 2496),
    (12, 0x1F, 2560),
    (12, 0x1, EOL_RUN),
    (12, 0x0, INVALID_RUN),
];

fn run_table(codes: &[(u8, u16, i16)]) -> PrefixCodeTable<i16> {
    PrefixCodeTable::new(codes.iter().map(|&(len, code, run)| (u32::from(len), u64::from(code), run)), 16)
}

fn white_table() -> &'static PrefixCodeTable<i16> {
    static TABLE: OnceLock<PrefixCodeTable<i16>> = OnceLock::new();
    TABLE.get_or_init(|| run_table(WHITE_CODES))
}

fn black_table() -> &'static PrefixCodeTable<i16> {
    static TABLE: OnceLock<PrefixCodeTable<i16>> = OnceLock::new();
    TABLE.get_or_init(|| run_table(BLACK_CODES))
}

fn at_code(bits: &MsbBitReader, len: u32, code: u32) -> bool {
    bits.bits_left() >= u64::from(len) && bits.peek(len) == code
}

fn sync_eol(bits: &mut MsbBitReader) -> bool {
    while bits.bits_left() >= 12 {
        if bits.peek(12) == EOL_CODE {
            bits.skip(12);
            return true;
        }
        bits.skip(1);
    }
    false
}

fn decode_run(bits: &mut MsbBitReader, is_white: bool) -> Option<u32> {
    let table = if is_white { white_table() } else { black_table() };

    let mut total = 0u32;
    loop {
        if bits.exhausted() {
            return None;
        }
        let start = bits.position();
        match table.decode(bits) {
            Some(run) if run >= 0 => {
                total = total.saturating_add(run as u32);
                if run < 64 {
                    return Some(total);
                }
            }
            _ => {
                bits.seek_bits(start);
                return None;
            }
        }
    }
}

fn decode_row_1d(bits: &mut MsbBitReader, width: u32, transitions: &mut Vec<u32>) -> bool {
    transitions.clear();
    let mut pos = 0u32;
    let mut is_white = true;

    while pos < width {
        let Some(run) = decode_run(bits, is_white) else {
            return false;
        };
        pos = pos.saturating_add(run).min(width);
        transitions.push(pos);
        is_white = !is_white;
    }

    true
}

#[derive(Clone, Copy)]
enum Mode2D {
    Pass,
    Horizontal,
    Vertical(i32),
    Eol,
    Invalid,
}

const MODE_CODES: [(u32, u32, Mode2D); 9] = [
    (1, 0b1, Mode2D::Vertical(0)),
    (3, 0b011, Mode2D::Vertical(1)),
    (3, 0b010, Mode2D::Vertical(-1)),
    (3, 0b001, Mode2D::Horizontal),
    (4, 0b0001, Mode2D::Pass),
    (6, 0b000011, Mode2D::Vertical(2)),
    (6, 0b000010, Mode2D::Vertical(-2)),
    (7, 0b0000011, Mode2D::Vertical(3)),
    (7, 0b0000010, Mode2D::Vertical(-3)),
];

fn decode_mode(bits: &mut MsbBitReader) -> Mode2D {
    if bits.exhausted() {
        return Mode2D::Invalid;
    }
    let window = bits.peek(7);
    for (len, code, mode) in &MODE_CODES {
        if window >> (7 - len) == *code {
            bits.skip(u64::from(*len));
            return *mode;
        }
    }
    if at_code(bits, 12, EOL_CODE) {
        return Mode2D::Eol;
    }
    Mode2D::Invalid
}

fn find_b1(reference: &[u32], a0: i64, a0_color_white: bool) -> usize {
    let mut idx = reference.partition_point(|&t| (t as i64) <= a0);
    if idx < reference.len() && (idx % 2 == 0) != a0_color_white {
        idx += 1;
    }
    idx
}

fn decode_row_2d(bits: &mut MsbBitReader, reference: &[u32], width: u32, transitions: &mut Vec<u32>) -> bool {
    transitions.clear();
    let mut a0: i64 = -1;
    let mut is_white = true;

    while a0 < width as i64 {
        let b1_idx = find_b1(reference, a0, is_white);
        let b1 = reference.get(b1_idx).copied().unwrap_or(width).min(width);
        let b2 = reference.get(b1_idx + 1).copied().unwrap_or(width).min(width);

        match decode_mode(bits) {
            Mode2D::Pass => a0 = b2 as i64,
            Mode2D::Horizontal => {
                let (Some(run1), Some(run2)) = (decode_run(bits, is_white), decode_run(bits, !is_white)) else {
                    return false;
                };
                let a1 = (a0.max(0) as u32).saturating_add(run1).min(width);
                let a2 = a1.saturating_add(run2).min(width);
                transitions.push(a1);
                transitions.push(a2);
                a0 = a2 as i64;
            }
            Mode2D::Vertical(d) => {
                let last = transitions.last().copied().unwrap_or(0);
                let a1 = ((b1 as i64 + d as i64).max(0) as u32).min(width).max(last);
                transitions.push(a1);
                is_white = !is_white;
                a0 = a1 as i64;
            }
            Mode2D::Eol => return !transitions.is_empty(),
            Mode2D::Invalid => return false,
        }
    }

    true
}

fn fill_black(row: &mut [u8], start: u32, end: u32) {
    if start >= end {
        return;
    }
    let (start, end) = (start as usize, end as usize);
    let first = start / 8;
    let last = (end - 1) / 8;
    let head = 0xFFu8 >> (start % 8);
    let tail = 0xFFu8 << (7 - (end - 1) % 8);
    if first == last {
        row[first] |= head & tail;
        return;
    }
    row[first] |= head;
    row[first + 1..last].fill(0xFF);
    row[last] |= tail;
}

fn render_row(transitions: &[u32], width: u32, row: &mut [u8]) {
    let mut start = 0u32;
    for (index, &t) in transitions.iter().enumerate() {
        let t = t.min(width);
        if index % 2 == 1 {
            fill_black(row, start, t);
        }
        start = t;
    }
    if transitions.len() % 2 == 1 {
        fill_black(row, start, width);
    }
}

pub fn decode(data: &[u8], width: u32, rows: usize, mode: FaxMode) -> FaxOutput {
    let row_bytes = (width as usize).div_ceil(8);
    let Some(total) = row_bytes.checked_mul(rows) else {
        return FaxOutput { pixels: Vec::new(), clean_rows: 0, bytes_used: 0 };
    };
    let mut pixels = vec![0u8; total];
    if width == 0 || rows == 0 {
        return FaxOutput { pixels, clean_rows: 0, bytes_used: 0 };
    }

    let mut bits = MsbBitReader::new(data);
    let mut reference: Vec<u32> = Vec::new();
    let mut transitions: Vec<u32> = Vec::new();
    let mut clean_rows = 0usize;

    for (row, out_row) in pixels.chunks_exact_mut(row_bytes).enumerate() {
        let decoded = match mode {
            FaxMode::ModifiedHuffman { word_aligned } => {
                let ok = decode_row_1d(&mut bits, width, &mut transitions);
                bits.align(if word_aligned { 16 } else { 8 });
                ok
            }
            FaxMode::Group4 => {
                if at_code(&bits, 24, EOFB_CODE) {
                    break;
                }
                decode_row_2d(&mut bits, &reference, width, &mut transitions)
            }
            FaxMode::Group3 { two_dimensional } => {
                let saw_eol = if at_code(&bits, 12, EOL_CODE) {
                    bits.skip(12);
                    true
                } else if bits.peek(11) == 0 {
                    if !sync_eol(&mut bits) {
                        break;
                    }
                    true
                } else {
                    if row == 0 {
                        log_warn!("CCITT G3 data does not start with EOL, decoding as plain 1D");
                    }
                    false
                };

                let is_1d = !(two_dimensional && saw_eol) || {
                    if bits.exhausted() {
                        break;
                    }
                    bits.read_bit()
                };

                let ok = if is_1d {
                    decode_row_1d(&mut bits, width, &mut transitions)
                } else {
                    decode_row_2d(&mut bits, &reference, width, &mut transitions)
                };
                if !ok && sync_eol(&mut bits) {
                    reference.clear();
                    continue;
                }
                ok
            }
        };

        if !decoded {
            log_warn!("CCITT {:?} decode error at row {}, remaining rows left white", mode, row);
            break;
        }

        render_row(&transitions, width, out_row);
        clean_rows += 1;
        std::mem::swap(&mut reference, &mut transitions);
    }

    if mode == FaxMode::Group4 && at_code(&bits, 24, EOFB_CODE) {
        bits.skip(24);
    }

    FaxOutput { pixels, clean_rows, bytes_used: bits.byte_position() }
}
