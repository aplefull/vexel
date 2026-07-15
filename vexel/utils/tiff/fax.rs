use std::sync::OnceLock;

use crate::log_warn;
use crate::utils::tiff::types::Compression;

const EOL_RUN: i16 = -1;
const INVALID_RUN: i16 = -2;

const WHITE_LUT_BITS: u32 = 12;
const BLACK_LUT_BITS: u32 = 13;

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

#[derive(Clone, Copy)]
struct LutEntry {
    run: i16,
    len: u8,
}

fn build_lut(codes: &[(u8, u16, i16)], lut_bits: u32) -> Vec<LutEntry> {
    let mut lut = vec![LutEntry { run: INVALID_RUN, len: 0 }; 1 << lut_bits];
    for &(len, code, run) in codes {
        let shift = lut_bits - len as u32;
        let base = (code as usize) << shift;
        for slot in &mut lut[base..base + (1 << shift)] {
            *slot = LutEntry { run, len };
        }
    }
    lut
}

fn white_lut() -> &'static [LutEntry] {
    static LUT: OnceLock<Vec<LutEntry>> = OnceLock::new();
    LUT.get_or_init(|| build_lut(WHITE_CODES, WHITE_LUT_BITS))
}

fn black_lut() -> &'static [LutEntry] {
    static LUT: OnceLock<Vec<LutEntry>> = OnceLock::new();
    LUT.get_or_init(|| build_lut(BLACK_CODES, BLACK_LUT_BITS))
}

struct FaxBits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> FaxBits<'a> {
    fn new(data: &'a [u8]) -> Self {
        FaxBits { data, pos: 0 }
    }

    fn bits_left(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.pos)
    }

    fn peek(&self, nbits: u32) -> u32 {
        let mut value = 0u32;
        for i in 0..nbits as usize {
            let bit_idx = self.pos + i;
            let byte = self.data.get(bit_idx / 8).copied().unwrap_or(0);
            let bit = (byte >> (7 - (bit_idx % 8))) & 1;
            value = (value << 1) | bit as u32;
        }
        value
    }

    fn skip(&mut self, nbits: u32) {
        self.pos += nbits as usize;
    }

    fn align_to(&mut self, bits: usize) {
        self.pos = self.pos.div_ceil(bits) * bits;
    }

    fn sync_eol(&mut self) -> bool {
        while self.bits_left() >= 12 {
            if self.peek(12) == 1 {
                self.skip(12);
                return true;
            }
            self.skip(1);
        }
        false
    }
}

fn decode_run(bits: &mut FaxBits, is_white: bool) -> i16 {
    let (lut, lut_bits) = if is_white { (white_lut(), WHITE_LUT_BITS) } else { (black_lut(), BLACK_LUT_BITS) };

    let mut total: i32 = 0;
    loop {
        if bits.bits_left() == 0 {
            return INVALID_RUN;
        }
        let entry = lut[bits.peek(lut_bits) as usize];
        if entry.len == 0 || entry.run == INVALID_RUN {
            return INVALID_RUN;
        }
        if entry.run == EOL_RUN {
            return if total == 0 { EOL_RUN } else { INVALID_RUN };
        }
        bits.skip(entry.len as u32);
        total += entry.run as i32;
        if entry.run < 64 {
            return total.min(i16::MAX as i32) as i16;
        }
    }
}

fn decode_row_1d(bits: &mut FaxBits, width: u32) -> Option<Vec<u32>> {
    let mut transitions = Vec::new();
    let mut pos = 0u32;
    let mut is_white = true;

    while pos < width {
        let run = decode_run(bits, is_white);
        if run < 0 {
            return None;
        }
        pos = (pos + run as u32).min(width);
        transitions.push(pos);
        is_white = !is_white;
    }

    Some(transitions)
}

enum Mode2D {
    Pass,
    Horizontal,
    Vertical(i32),
    Eol,
    Invalid,
}

fn decode_mode(bits: &mut FaxBits) -> Mode2D {
    if bits.bits_left() == 0 {
        return Mode2D::Invalid;
    }
    if bits.peek(1) == 1 {
        bits.skip(1);
        return Mode2D::Vertical(0);
    }
    match bits.peek(3) {
        0b011 => {
            bits.skip(3);
            return Mode2D::Vertical(1);
        }
        0b010 => {
            bits.skip(3);
            return Mode2D::Vertical(-1);
        }
        0b001 => {
            bits.skip(3);
            return Mode2D::Horizontal;
        }
        _ => {}
    }
    if bits.peek(4) == 0b0001 {
        bits.skip(4);
        return Mode2D::Pass;
    }
    match bits.peek(6) {
        0b000011 => {
            bits.skip(6);
            return Mode2D::Vertical(2);
        }
        0b000010 => {
            bits.skip(6);
            return Mode2D::Vertical(-2);
        }
        _ => {}
    }
    match bits.peek(7) {
        0b0000011 => {
            bits.skip(7);
            return Mode2D::Vertical(3);
        }
        0b0000010 => {
            bits.skip(7);
            return Mode2D::Vertical(-3);
        }
        _ => {}
    }
    if bits.bits_left() >= 12 && bits.peek(12) == 1 {
        return Mode2D::Eol;
    }
    Mode2D::Invalid
}

fn find_b1(reference: &[u32], a0: i64, a0_color_white: bool) -> usize {
    let mut idx = 0;
    while idx < reference.len() && (reference[idx] as i64) <= a0 {
        idx += 1;
    }
    let want_even = a0_color_white;
    if idx < reference.len() && (idx % 2 == 0) != want_even {
        idx += 1;
    }
    idx
}

fn decode_row_2d(bits: &mut FaxBits, reference: &[u32], width: u32) -> Option<Vec<u32>> {
    let mut transitions: Vec<u32> = Vec::new();
    let mut a0: i64 = -1;
    let mut is_white = true;

    while a0 < width as i64 {
        let b1_idx = find_b1(reference, a0, is_white);
        let b1 = reference.get(b1_idx).copied().unwrap_or(width).min(width);
        let b2 = reference.get(b1_idx + 1).copied().unwrap_or(width).min(width);

        match decode_mode(bits) {
            Mode2D::Pass => {
                a0 = b2 as i64;
            }
            Mode2D::Horizontal => {
                let run1 = decode_run(bits, is_white);
                let run2 = decode_run(bits, !is_white);
                if run1 < 0 || run2 < 0 {
                    return None;
                }
                let start = a0.max(0) as u32;
                let a1 = (start + run1 as u32).min(width);
                let a2 = (a1 + run2 as u32).min(width);
                transitions.push(a1);
                transitions.push(a2);
                a0 = a2 as i64;
                if a2 >= width {
                    break;
                }
            }
            Mode2D::Vertical(d) => {
                let a1 = ((b1 as i64 + d as i64).max(0) as u32).min(width);
                transitions.push(a1);
                is_white = !is_white;
                a0 = a1 as i64;
                if a1 >= width {
                    break;
                }
            }
            Mode2D::Eol => {
                return if transitions.is_empty() { None } else { Some(transitions) };
            }
            Mode2D::Invalid => {
                return None;
            }
        }
        if a0 >= width as i64 {
            break;
        }
    }

    Some(transitions)
}

fn render_row(transitions: &[u32], width: u32, out: &mut [u8]) {
    let mut is_white = true;
    let mut pos = 0u32;
    for &t in transitions {
        let t = t.min(width);
        if !is_white {
            for px in pos..t {
                out[(px / 8) as usize] |= 0x80 >> (px % 8);
            }
        }
        pos = t;
        is_white = !is_white;
    }
    if !is_white {
        for px in pos..width {
            out[(px / 8) as usize] |= 0x80 >> (px % 8);
        }
    }
}

pub fn decompress_ccitt(data: &[u8], width: u32, rows: usize, compression: Compression, t4_options: u32) -> Vec<u8> {
    decompress_ccitt_counted(data, width, rows, compression, t4_options).0
}

pub fn decompress_ccitt_counted(
    data: &[u8],
    width: u32,
    rows: usize,
    compression: Compression,
    t4_options: u32,
) -> (Vec<u8>, usize) {
    let row_bytes = (width as usize).div_ceil(8);
    let mut out = vec![0u8; row_bytes * rows];
    let mut clean_rows = 0usize;
    if width == 0 || rows == 0 {
        return (out, clean_rows);
    }

    let mut bits = FaxBits::new(data);
    let is_g4 = compression == Compression::CCITTFax4;
    let is_g3 = compression == Compression::CCITTFax3;
    let g3_2d = is_g3 && (t4_options & 1) != 0;

    let mut reference: Vec<u32> = Vec::new();

    for row in 0..rows {
        let out_row = &mut out[row * row_bytes..(row + 1) * row_bytes];

        match compression {
            Compression::CCITTRLE | Compression::CCITTRLEWord => {
                let transitions = match decode_row_1d(&mut bits, width) {
                    Some(t) => t,
                    None => {
                        log_warn!("CCITT RLE decode error at row {}, remaining rows left white", row);
                        break;
                    }
                };
                render_row(&transitions, width, out_row);
                clean_rows += 1;
                bits.align_to(if compression == Compression::CCITTRLEWord { 16 } else { 8 });
            }
            _ if is_g4 => {
                if bits.bits_left() >= 24 && bits.peek(24) == 0x001001 {
                    break;
                }
                let transitions = match decode_row_2d(&mut bits, &reference, width) {
                    Some(t) => t,
                    None => {
                        log_warn!("CCITT G4 decode error at row {}, remaining rows left white", row);
                        break;
                    }
                };
                render_row(&transitions, width, out_row);
                clean_rows += 1;
                reference = transitions;
            }
            _ => {
                let saw_eol = if bits.peek(12) == 1 {
                    bits.skip(12);
                    true
                } else if bits.peek(11) == 0 {
                    if !bits.sync_eol() {
                        break;
                    }
                    true
                } else {
                    if row == 0 {
                        log_warn!("CCITT G3 data does not start with EOL, decoding as plain 1D");
                    }
                    false
                };

                let is_1d = if g3_2d && saw_eol {
                    if bits.bits_left() == 0 {
                        break;
                    }
                    let flag = bits.peek(1) == 1;
                    bits.skip(1);
                    flag
                } else {
                    true
                };

                let transitions =
                    if is_1d { decode_row_1d(&mut bits, width) } else { decode_row_2d(&mut bits, &reference, width) };

                let transitions = match transitions {
                    Some(t) => t,
                    None => {
                        if bits.sync_eol() {
                            reference = Vec::new();
                            continue;
                        }
                        log_warn!("CCITT G3 decode error at row {}, remaining rows left white", row);
                        break;
                    }
                };
                render_row(&transitions, width, out_row);
                clean_rows += 1;
                reference = transitions;
            }
        }
    }

    (out, clean_rows)
}
