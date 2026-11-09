use std::fmt::Debug;
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::jpeg::app::{parse_app, parse_comment, AppSection, AppSegment, IccAssembler};
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::idct::{dequantize_and_idct, nonzero_mask};
use crate::decoders::jpeg::info::{
    coding_name, process_name, subsampling_name, JpegArithmeticConditioning, JpegExpand, JpegFrame, JpegFrameComponent,
    JpegHuffmanTable, JpegHuffmanTables, JpegInfo, JpegQuantizationTable, JpegQuantizationTables, JpegScan,
    JpegScanComponent, JpegSection,
};
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::decoders::jpeg::types::{
    ArithmeticCodingTable, ArithmeticCodingValue, ColorComponentInfo, HuffmanTable, JpegCodingMethod, JpegMode,
    Predictor, QuantizationTable, ScanComponent, ScanData, DEFAULT_QUANTIZATION_TABLE, HUFF_PEEK_BITS, ZIGZAG_MAP,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, ImageSummary};
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn, Image, Limits, PixelData};

#[derive(Debug, Clone)]
struct ComponentPlane {
    data: Vec<i16>,
    blocks_per_line: u32,
}

impl ComponentPlane {
    fn try_new(width: u32, height: u32, limits: &mut Limits) -> VexelResult<Self> {
        let blocks_per_line = width.div_ceil(8);
        let block_lines = height.div_ceil(8);
        let elem_count = (blocks_per_line as usize).saturating_mul(block_lines as usize).saturating_mul(64);
        let byte_count = elem_count.saturating_mul(2);
        limits.reserve_usize(byte_count)?;
        Ok(Self { blocks_per_line, data: vec![0; elem_count] })
    }

    fn free(&self, limits: &mut Limits) {
        let byte_count = self.data.len().saturating_mul(2);
        limits.free_usize(byte_count);
    }

    fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i16; 64]> {
        let block_idx = block_y as usize * self.blocks_per_line as usize + block_x as usize;
        let start = block_idx * 64;
        if start + 64 <= self.data.len() {
            (&mut self.data[start..start + 64]).try_into().ok()
        } else {
            None
        }
    }
}

struct RasterPlane {
    data: Vec<i16>,
    width: usize,
    height: usize,
}

#[inline(always)]
fn get_next_symbol(reader: &mut JpegBitReader<'_>, table: &HuffmanTable) -> u8 {
    if let Some(peek) = reader.peek_fast() {
        let entry = unsafe { *table.fast_lookup.get_unchecked(peek as usize) };
        if entry >> 16 != 0 {
            reader.consume(entry & 0xFF);
            return ((entry >> 8) & 0xFF) as u8;
        }
        reader.consume(HUFF_PEEK_BITS as u32);
        let mut code = peek;
        for i in HUFF_PEEK_BITS..16 {
            code = (code << 1) | reader.read_bits(1);
            let first = unsafe { *table.first_code.get_unchecked(i) };
            if first != u32::MAX {
                let count = unsafe { table.offsets.get_unchecked(i + 1).wrapping_sub(*table.offsets.get_unchecked(i)) }
                    as usize;
                if code >= first && (code - first) < count as u32 {
                    let idx = unsafe { *table.offsets.get_unchecked(i) } as usize + (code - first) as usize;
                    return unsafe { *table.symbols.get_unchecked(idx) };
                }
            }
        }
        log_warn!("Invalid Huffman code: {}, replacing with 0", code);
        return 0;
    }

    let mut code = 0u32;
    for i in 0..16 {
        code = (code << 1) | reader.read_bits(1);
        let first = table.first_code[i];
        if first != u32::MAX {
            let count = (table.offsets[i + 1] - table.offsets[i]) as usize;
            if code >= first && (code - first) < count as u32 {
                let idx = table.offsets[i] as usize + (code - first) as usize;
                return table.symbols[idx];
            }
        }
    }

    log_warn!("Invalid Huffman code: {}, replacing with 0", code);
    0
}

struct ProgressiveScanComp<'a> {
    plane_index: usize,
    h_blocks: u8,
    v_blocks: u8,
    dc_table: Option<&'a HuffmanTable>,
    ac_table: Option<&'a HuffmanTable>,
}

fn run_progressive_blocks<'a, 'b, F>(
    planes: &mut [ComponentPlane],
    infos: &[ProgressiveScanComp<'b>],
    mcu_width: u32,
    mcu_height: u32,
    restart_interval: u16,
    reader: &mut JpegBitReader<'a>,
    previous_dc: &mut [i32],
    mut body: F,
) -> u64
where
    F: FnMut(&mut JpegBitReader<'a>, &mut [i16; 64], &ProgressiveScanComp<'b>, &mut [i32]),
{
    let mut restart_counter = restart_interval;
    let mut processed_mcus = 0u64;

    for mcu_y in 0..mcu_height {
        for mcu_x in 0..mcu_width {
            if reader.is_exhausted() {
                log_warn!("Scan data exhausted at MCU ({}, {}), leaving the rest of the scan empty", mcu_x, mcu_y);
                return processed_mcus;
            }
            processed_mcus += 1;

            if restart_interval > 0 {
                if restart_counter == 0 {
                    previous_dc.fill(0);
                    reader.clear_buffer();
                    restart_counter = restart_interval;
                }
                restart_counter = restart_counter.saturating_sub(1);
            }

            for info in infos {
                let h_blocks = info.h_blocks as u32;
                let v_blocks = info.v_blocks as u32;
                let plane = &mut planes[info.plane_index];
                let blocks_per_line = plane.blocks_per_line;

                for v in 0..v_blocks {
                    for h in 0..h_blocks {
                        let block_x = mcu_x * h_blocks + h;
                        let block_y = mcu_y * v_blocks + v;

                        if block_x >= blocks_per_line {
                            continue;
                        }

                        if let Some(block) = plane.get_block_mut(block_x, block_y) {
                            body(reader, block, info, previous_dc);
                        }
                    }
                }
            }
        }
    }

    processed_mcus
}

fn dezigzag_blocks(blocks: &mut [i16]) {
    let mut natural = [0i16; 64];
    for block in blocks.chunks_exact_mut(64) {
        for zz in 0..64 {
            unsafe {
                *natural.get_unchecked_mut(*ZIGZAG_MAP.get_unchecked(zz) as usize) = *block.get_unchecked(zz);
            }
        }
        block.copy_from_slice(&natural);
    }
}

#[inline(always)]
fn prog_dc_first(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    dc_table: Option<&HuffmanTable>,
    prev_dc: &mut i32,
    successive_low: u8,
    is_differential: bool,
) -> bool {
    let dc_table = match dc_table {
        Some(table) => table,
        None => {
            log_warn!("DC table missing for block, skipping");
            return false;
        }
    };

    let length = get_next_symbol(reader, dc_table);

    if length > 15 {
        log_warn!("Invalid DC coefficient length (>15): {}", length);
        return false;
    }

    let bits = reader.read_bits(length as u32);
    let mut value = bits as i32;

    if length != 0 && value < (1 << (length - 1)) {
        value -= (1 << length) - 1;
    }

    if is_differential {
        *prev_dc = value;
    } else {
        value = value.wrapping_add(*prev_dc);
        *prev_dc = value;
    }

    block[0] = (value << successive_low) as i16;
    true
}

#[inline(always)]
fn prog_dc_refine(reader: &mut JpegBitReader<'_>, block: &mut [i16; 64], successive_low: u8) {
    let bit = reader.read_bits(1);
    block[0] = (block[0] as i32 | ((bit as i32) << successive_low)) as i16;
}

#[inline(always)]
fn prog_ac_first(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    ac_table: Option<&HuffmanTable>,
    skips: &mut u32,
    start_spectral: usize,
    end_spectral: usize,
    successive_low: u8,
    max_ac_len: u8,
) {
    if *skips > 0 {
        *skips -= 1;
        return;
    }

    let ac_table = match ac_table {
        Some(table) => table,
        None => {
            log_warn!("AC table missing for block, skipping");
            return;
        }
    };

    let mut k = start_spectral;
    while k <= end_spectral {
        let symbol = get_next_symbol(reader, ac_table);

        let num_zeros = (symbol >> 4) as usize;
        let length = symbol & 0xF;

        if length != 0 {
            if k + num_zeros > 63 {
                log_warn!("Zero run-length exceeded spectral selection: {}", k + num_zeros);
                break;
            }

            block[k..k + num_zeros].fill(0);
            k += num_zeros;

            if length > max_ac_len {
                log_warn!("Invalid AC coefficient length (>{}): {}", max_ac_len, length);
                break;
            }

            let bits = reader.read_bits(length as u32);
            let mut value = bits as i32;

            if value < (1 << (length - 1)) {
                value -= (1 << length) - 1;
            }

            block[k] = (value << successive_low) as i16;
            k += 1;
        } else if num_zeros == 15 {
            if k + num_zeros > end_spectral {
                log_warn!("Zero run-length exceeded spectral selection: {}", k + num_zeros);
                break;
            }

            block[k..k + num_zeros].fill(0);
            k += num_zeros + 1;
        } else {
            *skips = (1u32 << num_zeros) - 1;
            *skips += reader.read_bits(num_zeros as u32);
            break;
        }
    }
}

#[inline(always)]
fn prog_ac_refine(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    ac_table: Option<&HuffmanTable>,
    skips: &mut u32,
    start_spectral: usize,
    end_spectral: usize,
    positive: i32,
    negative: i32,
) {
    let mut k = start_spectral;

    if start_spectral > end_spectral {
        if *skips > 0 {
            *skips -= 1;
        }
        return;
    }

    let mask = nonzero_mask(block) & (!0u64 >> (63 - end_spectral)) & (!0u64 << start_spectral);

    if *skips == 0 {
        let ac_table = match ac_table {
            Some(table) => table,
            None => {
                log_warn!("AC table missing for block (refining), skipping");
                return;
            }
        };

        while k <= end_spectral {
            let symbol = get_next_symbol(reader, ac_table);

            let num_zeros = (symbol >> 4) as u32;
            let length = symbol & 0xF;
            let mut coefficient = 0;

            if length != 0 {
                if length != 1 {
                    log_warn!("Invalid AC coefficient length (refining): {}", length);
                    break;
                }

                coefficient = if reader.read_bits(1) != 0 { positive } else { negative };
            } else if num_zeros != 15 {
                *skips = 1 << num_zeros;
                *skips += reader.read_bits(num_zeros);
                break;
            }

            let mut zeros_left = num_zeros as usize;
            loop {
                let m = mask >> k;
                let ones = m.trailing_ones() as usize;
                if ones > 0 {
                    for pos in k..k + ones {
                        let val = unsafe { block.get_unchecked_mut(pos) };
                        let v = *val as i32;
                        if reader.read_bits(1) == 1 && v & positive == 0 {
                            if v >= 0 {
                                *val = (v + positive) as i16;
                            } else {
                                *val = (v + negative) as i16;
                            }
                        }
                    }
                    k += ones;
                    if k > end_spectral {
                        break;
                    }
                    continue;
                }

                if zeros_left == 0 {
                    break;
                }

                let zero_run = m.trailing_zeros() as usize;
                let skip = zeros_left.min(zero_run).min(end_spectral + 1 - k);
                zeros_left -= skip;
                k += skip;

                if k > end_spectral {
                    break;
                }
            }

            if coefficient != 0 && k <= end_spectral {
                unsafe {
                    *block.get_unchecked_mut(k) = coefficient as i16;
                }
            }

            k += 1;
        }
    }

    if *skips > 0 {
        if k <= end_spectral {
            let mut m = mask >> k;
            let mut pos = k;
            while m != 0 {
                let step = m.trailing_zeros() as usize;
                pos += step;
                m >>= step;

                let val = unsafe { block.get_unchecked_mut(pos) };
                let v = *val as i32;
                if reader.read_bits(1) == 1 && v & positive == 0 {
                    if v >= 0 {
                        *val = (v + positive) as i16;
                    } else {
                        *val = (v + negative) as i16;
                    }
                }

                pos += 1;
                m >>= 1;
            }
        }

        *skips -= 1;
    }
}

#[rustfmt::skip]
const SCAN_BLOCK_SIZE: usize = 64 * 1024;

const MAX_SCANS: usize = 500;
const MAX_SCAN_COMPONENTS: usize = 4;
const MAX_HIERARCHICAL_FRAMES: usize = 32;
const PROGRESSIVE_PASS_BUDGET: u64 = 128;
const LOSSLESS_PASS_BUDGET: u64 = 4;
const HIERARCHICAL_PIXEL_BUDGET: u64 = 4;

#[cfg(feature = "rayon")]
const PARALLEL_LOSSLESS_MIN_SAMPLES: usize = 256 * 256;

#[rustfmt::skip]
const ARITH_TABLE: [(u32, u8, u8, u8); 114] = [
    (0x5a1d,  1,  1, 1), (0x2586, 14,  2, 0), (0x1114, 16,  3, 0), (0x080b, 18,  4, 0),
    (0x03d8, 20,  5, 0), (0x01da, 23,  6, 0), (0x00e5, 25,  7, 0), (0x006f, 28,  8, 0),
    (0x0036, 30,  9, 0), (0x001a, 33, 10, 0), (0x000d, 35, 11, 0), (0x0006,  9, 12, 0),
    (0x0003, 10, 13, 0), (0x0001, 12, 13, 0), (0x5a7f, 15, 15, 1), (0x3f25, 36, 16, 0),
    (0x2cf2, 38, 17, 0), (0x207c, 39, 18, 0), (0x17b9, 40, 19, 0), (0x1182, 42, 20, 0),
    (0x0cef, 43, 21, 0), (0x09a1, 45, 22, 0), (0x072f, 46, 23, 0), (0x055c, 48, 24, 0),
    (0x0406, 49, 25, 0), (0x0303, 51, 26, 0), (0x0240, 52, 27, 0), (0x01b1, 54, 28, 0),
    (0x0144, 56, 29, 0), (0x00f5, 57, 30, 0), (0x00b7, 59, 31, 0), (0x008a, 60, 32, 0),
    (0x0068, 62, 33, 0), (0x004e, 63, 34, 0), (0x003b, 32, 35, 0), (0x002c, 33,  9, 0),
    (0x5ae1, 37, 37, 1), (0x484c, 64, 38, 0), (0x3a0d, 65, 39, 0), (0x2ef1, 67, 40, 0),
    (0x261f, 68, 41, 0), (0x1f33, 69, 42, 0), (0x19a8, 70, 43, 0), (0x1518, 72, 44, 0),
    (0x1177, 73, 45, 0), (0x0e74, 74, 46, 0), (0x0bfb, 75, 47, 0), (0x09f8, 77, 48, 0),
    (0x0861, 78, 49, 0), (0x0706, 79, 50, 0), (0x05cd, 48, 51, 0), (0x04de, 50, 52, 0),
    (0x040f, 50, 53, 0), (0x0363, 51, 54, 0), (0x02d4, 52, 55, 0), (0x025c, 53, 56, 0),
    (0x01f8, 54, 57, 0), (0x01a4, 55, 58, 0), (0x0160, 56, 59, 0), (0x0125, 57, 60, 0),
    (0x00f6, 58, 61, 0), (0x00cb, 59, 62, 0), (0x00ab, 61, 63, 0), (0x008f, 61, 32, 0),
    (0x5b12, 65, 65, 1), (0x4d04, 80, 66, 0), (0x412c, 81, 67, 0), (0x37d8, 82, 68, 0),
    (0x2fe8, 83, 69, 0), (0x293c, 84, 70, 0), (0x2379, 86, 71, 0), (0x1edf, 87, 72, 0),
    (0x1aa9, 87, 73, 0), (0x174e, 72, 74, 0), (0x1424, 72, 75, 0), (0x119c, 74, 76, 0),
    (0x0f6b, 74, 77, 0), (0x0d51, 75, 78, 0), (0x0bb6, 77, 79, 0), (0x0a40, 77, 48, 0),
    (0x5832, 80, 81, 1), (0x4d1c, 88, 82, 0), (0x438e, 89, 83, 0), (0x3bdd, 90, 84, 0),
    (0x34ee, 91, 85, 0), (0x2eae, 92, 86, 0), (0x299a, 93, 87, 0), (0x2516, 86, 71, 0),
    (0x5570, 88, 89, 1), (0x4ca9, 95, 90, 0), (0x44d9, 96, 91, 0), (0x3e22, 97, 92, 0),
    (0x3824, 99, 93, 0), (0x32b4, 99, 94, 0), (0x2e17, 93, 86, 0), (0x56a8, 95, 96, 1),
    (0x4f46,101, 97, 0), (0x47e5,102, 98, 0), (0x41cf,103, 99, 0), (0x3c3d,104,100, 0),
    (0x375e, 99, 93, 0), (0x5231,105,102, 0), (0x4c0f,106,103, 0), (0x4639,107,104, 0),
    (0x415e,103, 99, 0), (0x5627,105,106, 1), (0x50e7,108,107, 0), (0x4b85,109,103, 0),
    (0x5597,110,109, 0), (0x504f,111,107, 0), (0x5a10,110,111, 1), (0x5522,112,109, 0),
    (0x59eb,112,111, 1), (0x5a1d,113,113, 0),
];

struct ArithmeticDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    c: u32,
    a: u32,
    ct: i32,
    fixed_bin: u8,
    error: bool,
    overrun: u32,
}

const ARITH_EXHAUSTED_PADDING_BYTES: u32 = 4;

impl<'a> ArithmeticDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, c: 0, a: 0, ct: -16, fixed_bin: 113, error: false, overrun: 0 }
    }

    fn is_exhausted(&self) -> bool {
        self.overrun > ARITH_EXHAUSTED_PADDING_BYTES
    }

    fn reset(&mut self) {
        self.c = 0;
        self.a = 0;
        self.ct = -16;
        self.error = false;
    }

    fn read_byte(&mut self) -> u32 {
        if self.pos >= self.data.len() {
            self.overrun = self.overrun.saturating_add(1);
            return 0;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b as u32
    }

    fn arith_decode(&mut self, st: &mut u8) -> u8 {
        if self.error {
            return 0;
        }

        while self.a < 0x8000 {
            self.ct -= 1;
            if self.ct < 0 {
                let data = self.read_byte();
                let data = if data == 0xFF {
                    let next = self.read_byte();
                    if next == 0 {
                        0xFF
                    } else {
                        0
                    }
                } else {
                    data
                };
                self.c = (self.c << 8) | data;
                self.ct += 8;
                if self.ct < 0 {
                    self.ct += 1;
                    if self.ct == 0 {
                        self.a = 0x8000;
                    }
                }
            }
            self.a <<= 1;
        }

        let sv = *st as usize;
        let (qe, nl, nm, switch_mps) = ARITH_TABLE[sv & 0x7F];

        let temp = self.a.wrapping_sub(qe);
        self.a = temp;
        let shifted = temp << (self.ct as u32);

        if self.c >= shifted {
            self.c -= shifted;
            if self.a < qe {
                self.a = qe;
                *st = (*st & 0x80) ^ nm;
                sv as u8 >> 7
            } else {
                self.a = qe;
                *st = (*st & 0x80) ^ nl;
                if switch_mps != 0 {
                    *st ^= 0x80;
                }
                1 - (sv as u8 >> 7)
            }
        } else if self.a < 0x8000 {
            if self.a < qe {
                *st = (*st & 0x80) ^ nl;
                if switch_mps != 0 {
                    *st ^= 0x80;
                }
                1 - (sv as u8 >> 7)
            } else {
                *st = (*st & 0x80) ^ nm;
                sv as u8 >> 7
            }
        } else {
            sv as u8 >> 7
        }
    }

    fn decode_dc_coeff(
        &mut self,
        comp_idx: usize,
        dc_tbl: usize,
        dc_l: u8,
        dc_u: u8,
        dc_context: &mut [usize],
        last_dc_val: &mut [i32],
        dc_stats: &mut [Vec<u8>],
        is_differential: bool,
    ) -> i32 {
        if self.error || dc_tbl >= dc_stats.len() || comp_idx >= dc_context.len() {
            return last_dc_val.get(comp_idx).copied().unwrap_or(0);
        }

        let tbl_len = dc_stats[dc_tbl].len();
        let ctx = dc_context[comp_idx].min(tbl_len.saturating_sub(4));

        if self.arith_decode(&mut dc_stats[dc_tbl][ctx]) == 0 {
            dc_context[comp_idx] = 0;
        } else {
            let sign = self.arith_decode(&mut dc_stats[dc_tbl][ctx + 1]) as i32;
            let mut st = (ctx + 2 + sign as usize).min(tbl_len - 1);

            let mut m: i32 = 0;
            if self.arith_decode(&mut dc_stats[dc_tbl][st]) != 0 {
                m = 1;
                st = 20;
                loop {
                    let idx = st.min(tbl_len - 1);
                    if self.arith_decode(&mut dc_stats[dc_tbl][idx]) == 0 {
                        break;
                    }
                    m <<= 1;
                    if m == 0x8000 {
                        log_warn!("Arithmetic DC magnitude overflow");
                        self.error = true;
                        return last_dc_val[comp_idx];
                    }
                    st += 1;
                }
            }

            st = (st + 14).min(tbl_len - 1);
            let mut v = m;
            let mut bit_m = m;
            while bit_m > 1 {
                bit_m >>= 1;
                if self.arith_decode(&mut dc_stats[dc_tbl][st]) != 0 {
                    v |= bit_m;
                }
            }
            v += 1;
            if sign != 0 {
                v = -v;
            }

            let abs_v = v.unsigned_abs() as i32;
            let threshold_l = (1i32 << dc_l) >> 1;
            let threshold_u = 1i32 << dc_u;
            dc_context[comp_idx] = if abs_v <= threshold_l {
                0
            } else if abs_v > threshold_u {
                12 + sign as usize * 4
            } else {
                4 + sign as usize * 4
            };

            if is_differential {
                last_dc_val[comp_idx] = v & 0xFFFF;
            } else {
                last_dc_val[comp_idx] = (last_dc_val[comp_idx] + v) & 0xFFFF;
            }
        }

        let raw = last_dc_val[comp_idx];
        if raw >= 0x8000 {
            raw - 0x10000
        } else {
            raw
        }
    }

    fn decode_dc_first(
        &mut self,
        block: &mut [i16],
        comp_idx: usize,
        dc_tbl: usize,
        dc_l: u8,
        dc_u: u8,
        successive_low: u8,
        dc_context: &mut [usize],
        last_dc_val: &mut [i32],
        dc_stats: &mut [Vec<u8>],
        is_differential: bool,
    ) {
        let val =
            self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats, is_differential);
        block[0] = (val << (successive_low as i32)) as i16;
    }

    fn decode_dc_refine(&mut self, block: &mut [i16], successive_low: u8) {
        if self.error {
            return;
        }
        let p1 = 1i32 << successive_low;
        let mut st = self.fixed_bin;
        if self.arith_decode(&mut st) != 0 {
            block[0] = (block[0] as i32 | p1) as i16;
        }
        self.fixed_bin = st;
    }

    fn decode_ac_first(
        &mut self,
        block: &mut [i16],
        ac_tbl: usize,
        ac_k: u8,
        ss: usize,
        se: usize,
        successive_low: u8,
        ac_stats: &mut [Vec<u8>],
    ) {
        if self.error || ac_tbl >= ac_stats.len() {
            return;
        }

        let mut k = ss;
        while k <= se {
            let st_base = 3 * (k.saturating_sub(1));
            if st_base + 2 >= ac_stats[ac_tbl].len() {
                break;
            }

            if self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 {
                break;
            }

            loop {
                let cur_base = 3 * (k.saturating_sub(1));
                if cur_base + 1 >= ac_stats[ac_tbl].len() {
                    break;
                }
                if self.arith_decode(&mut ac_stats[ac_tbl][cur_base + 1]) != 0 {
                    break;
                }
                k += 1;
                if k > se {
                    log_warn!("Arithmetic AC spectral overflow");
                    self.error = true;
                    return;
                }
            }

            let mut fixed_st = self.fixed_bin;
            let sign = self.arith_decode(&mut fixed_st) as i32;
            self.fixed_bin = fixed_st;

            let cur_base = 3 * (k.saturating_sub(1));
            if cur_base + 2 >= ac_stats[ac_tbl].len() {
                break;
            }

            let tbl_len = ac_stats[ac_tbl].len();

            let mut st = cur_base + 2;
            let mut m: i32 = 0;
            if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                m = 1;
                if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                    m <<= 1;
                    st = if k <= ac_k as usize { 189 } else { 217 };
                    loop {
                        let idx = st.min(tbl_len - 1);
                        if self.arith_decode(&mut ac_stats[ac_tbl][idx]) == 0 {
                            break;
                        }
                        m <<= 1;
                        if m == 0x8000 {
                            log_warn!("Arithmetic AC magnitude overflow");
                            self.error = true;
                            return;
                        }
                        st += 1;
                    }
                }
            }

            st = (st + 14).min(tbl_len - 1);
            let mut v = m;
            let mut bit_m = m;
            while bit_m > 1 {
                bit_m >>= 1;
                if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                    v |= bit_m;
                }
            }
            v += 1;
            if sign != 0 {
                v = -v;
            }

            if k < 64 {
                block[k] = (v << (successive_low as i32)) as i16;
            }
            k += 1;
        }
    }

    fn decode_ac_refine(
        &mut self,
        block: &mut [i16],
        ac_tbl: usize,
        ss: usize,
        se: usize,
        successive_low: u8,
        ac_stats: &mut [Vec<u8>],
    ) {
        if self.error || ac_tbl >= ac_stats.len() {
            return;
        }

        let p1 = 1i32 << successive_low;
        let m1 = (-1i32) << successive_low;
        let tbl_len = ac_stats[ac_tbl].len();

        let mut kex = se;
        while kex > 0 && kex < 64 {
            if block[kex] != 0 {
                break;
            }
            kex -= 1;
        }

        let mut k = ss;
        'outer: while k <= se {
            let st_base = 3 * (k.saturating_sub(1));
            if st_base + 2 >= tbl_len {
                break;
            }

            if k > kex && self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 {
                break;
            }

            loop {
                if k >= 64 {
                    break 'outer;
                }
                let coef_idx = k;
                let coef = block[coef_idx] as i32;
                let st_base = 3 * (k.saturating_sub(1));
                if st_base + 2 >= tbl_len {
                    break 'outer;
                }

                if coef != 0 {
                    if self.arith_decode(&mut ac_stats[ac_tbl][st_base + 2]) != 0 {
                        if coef < 0 {
                            block[coef_idx] = (coef + m1) as i16;
                        } else {
                            block[coef_idx] = (coef + p1) as i16;
                        }
                    }
                    break;
                } else {
                    if self.arith_decode(&mut ac_stats[ac_tbl][st_base + 1]) != 0 {
                        let mut fixed_st = self.fixed_bin;
                        let sign = self.arith_decode(&mut fixed_st);
                        self.fixed_bin = fixed_st;
                        block[coef_idx] = if sign != 0 { m1 as i16 } else { p1 as i16 };
                        break;
                    }
                    k += 1;
                    if k > se {
                        break 'outer;
                    }
                }
            }

            k += 1;
        }
    }

    fn decode_mcu_sequential(
        &mut self,
        block: &mut [i16],
        comp_idx: usize,
        dc_tbl: usize,
        ac_tbl: usize,
        dc_l: u8,
        dc_u: u8,
        ac_k: u8,
        dc_context: &mut [usize],
        last_dc_val: &mut [i32],
        dc_stats: &mut [Vec<u8>],
        ac_stats: &mut [Vec<u8>],
        is_differential: bool,
    ) {
        if self.error {
            return;
        }

        let val =
            self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats, is_differential);
        block[0] = val as i16;

        if self.error {
            return;
        }

        if ac_tbl >= ac_stats.len() {
            return;
        }

        let mut k = 1usize;
        while k <= 63 {
            let st_base = 3 * (k - 1);
            if st_base + 2 >= ac_stats[ac_tbl].len() {
                break;
            }

            if self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 {
                break;
            }

            loop {
                let cur_base = 3 * (k - 1);
                if cur_base + 1 >= ac_stats[ac_tbl].len() {
                    break;
                }
                if self.arith_decode(&mut ac_stats[ac_tbl][cur_base + 1]) != 0 {
                    break;
                }
                k += 1;
                if k > 63 {
                    log_warn!("Arithmetic AC sequential spectral overflow");
                    self.error = true;
                    return;
                }
            }

            let mut fixed_st = self.fixed_bin;
            let sign = self.arith_decode(&mut fixed_st) as i32;
            self.fixed_bin = fixed_st;

            let cur_base = 3 * (k - 1);
            if cur_base + 2 >= ac_stats[ac_tbl].len() {
                break;
            }

            let tbl_len = ac_stats[ac_tbl].len();

            let mut st = cur_base + 2;
            let mut m: i32 = 0;
            if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                m = 1;
                if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                    m <<= 1;
                    st = if k <= ac_k as usize { 189 } else { 217 };
                    loop {
                        let idx = st.min(tbl_len - 1);
                        if self.arith_decode(&mut ac_stats[ac_tbl][idx]) == 0 {
                            break;
                        }
                        m <<= 1;
                        if m == 0x8000 {
                            log_warn!("Arithmetic AC magnitude overflow (sequential)");
                            self.error = true;
                            return;
                        }
                        st += 1;
                    }
                }
            }

            st = (st + 14).min(tbl_len - 1);
            let mut v = m;
            let mut bit_m = m;
            while bit_m > 1 {
                bit_m >>= 1;
                if self.arith_decode(&mut ac_stats[ac_tbl][st]) != 0 {
                    v |= bit_m;
                }
            }
            v += 1;
            if sign != 0 {
                v = -v;
            }

            if k < 64 {
                block[ZIGZAG_MAP[k] as usize] = v as i16;
            }
            k += 1;
        }
    }

    fn decode_lossless_difference(&mut self, contexts: &mut [u8; 160], da: i32, db: i32, l: u8, u: u8) -> i32 {
        fn classify(diff: i32, l: u8, u: u8) -> usize {
            let abs = diff.unsigned_abs() as i64;
            let threshold_l = if l == 0 { 0i64 } else { (1i64 << l) >> 1 };
            let threshold_u = 1i64 << u;
            if abs <= threshold_l {
                2
            } else if abs <= threshold_u {
                if diff < 0 {
                    1
                } else {
                    3
                }
            } else if diff < 0 {
                0
            } else {
                4
            }
        }

        let da_cls = classify(da, l, u);
        let db_cls = classify(db, l, u);
        let base = da_cls * 5 + db_cls;

        let s0_idx = base;
        let ss_idx = 25 + base;
        let sp_idx = 50 + base;
        let sn_idx = 75 + base;

        if self.arith_decode(&mut contexts[s0_idx]) == 0 {
            return 0;
        }

        let sign = self.arith_decode(&mut contexts[ss_idx]) != 0;

        let magnitude_base = if db.unsigned_abs() > (1u32 << u) { 130usize } else { 100usize };

        let sz = if self.arith_decode(&mut contexts[if sign { sn_idx } else { sp_idx }]) != 0 {
            let mut i = 0usize;
            let mut m = 2i32;
            while self.arith_decode(&mut contexts[magnitude_base + i]) != 0 {
                m <<= 1;
                i += 1;
                if i >= 15 {
                    log_warn!("Arithmetic lossless magnitude overflow");
                    self.error = true;
                    return 0;
                }
            }
            m >>= 1;
            let mut sz = m;
            let refinement_base = magnitude_base + 15;
            while m > 1 {
                m >>= 1;
                if self.arith_decode(&mut contexts[refinement_base + i]) != 0 {
                    sz |= m;
                }
            }
            sz
        } else {
            0
        };

        if sign {
            -(sz + 1)
        } else {
            sz + 1
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum JpegColorspace {
    Grayscale,
    RGB,
    YCbCr,
    CMYK,
    YCCK,
    RGBA,
    Unknown,
}

struct LosslessComponent<'t> {
    pos: usize,
    stride: usize,
    row_stride: usize,
    table: &'t HuffmanTable,
    ra: i32,
    rb: i32,
}

struct HierarchicalFrame {
    width: u32,
    height: u32,
    mode: JpegMode,
    coding_method: JpegCodingMethod,
    precision: u8,
    components: Vec<ColorComponentInfo>,
    quantization_tables: Vec<QuantizationTable>,
    scans: Vec<ScanData>,
}

struct RawHuffmanTable {
    id: u8,
    class: u8,
    counts: Vec<u8>,
    offsets: Vec<u32>,
    symbols: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WalkMode {
    InfoOnly,
    Decode,
}

pub struct JpegDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    has_jfif: bool,
    mode: JpegMode,
    coding_method: JpegCodingMethod,
    quantization_tables: Vec<QuantizationTable>,
    ac_huffman_tables: Vec<HuffmanTable>,
    dc_huffman_tables: Vec<HuffmanTable>,
    ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    successive_approximation_low: u8,
    horizontal_sampling_factor: u8,
    vertical_sampling_factor: u8,
    restart_interval: u16,
    mcu_width: u32,
    mcu_height: u32,
    precision: u8,
    component_count: u8,
    components: Vec<ColorComponentInfo>,
    scans: Vec<ScanData>,
    info: Option<JpegInfo>,
    walk_mode: WalkMode,
    reader: BitReader<R>,
    adobe_color_transform: Option<u8>,
    is_hierarchical: bool,
    dhp_width: u32,
    dhp_height: u32,
    pending_expand_h: bool,
    pending_expand_v: bool,
    hierarchical_frames: Vec<HierarchicalFrame>,
    lossless_ssss16_is_min: bool,
    lossless_skip_color_transform: bool,
    icc: IccAssembler,
    collected_scans: usize,
    frame_has_scan: bool,
}

impl<R: Read + Seek> JpegDecoder<R> {
    // TODO remove redundant fields, that are duplicated in scans
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            has_jfif: false,
            mode: JpegMode::Baseline,
            coding_method: JpegCodingMethod::Huffman,
            mcu_width: 0,
            mcu_height: 0,
            precision: 0,
            component_count: 0,
            successive_approximation_low: 0,
            components: Vec::new(),
            quantization_tables: Vec::new(),
            ac_huffman_tables: Vec::new(),
            dc_huffman_tables: Vec::new(),
            ac_arithmetic_tables: Vec::new(),
            dc_arithmetic_tables: Vec::new(),
            horizontal_sampling_factor: 1,
            vertical_sampling_factor: 1,
            restart_interval: 0,
            scans: Vec::new(),
            info: None,
            walk_mode: WalkMode::Decode,
            reader: BitReader::new(reader),
            adobe_color_transform: None,
            is_hierarchical: false,
            dhp_width: 0,
            dhp_height: 0,
            pending_expand_h: false,
            pending_expand_v: false,
            hierarchical_frames: Vec::new(),
            lossless_ssss16_is_min: true,
            lossless_skip_color_transform: false,
            icc: IccAssembler::default(),
            collected_scans: 0,
            frame_has_scan: false,
        }
    }

    pub fn set_lossless_ssss16_is_min(&mut self, is_min: bool) {
        self.lossless_ssss16_is_min = is_min;
    }

    pub fn set_lossless_skip_color_transform(&mut self, skip: bool) {
        self.lossless_skip_color_transform = skip;
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> JpegInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk(WalkMode::InfoOnly) {
                log_warn!("Failed to read JPEG structure: {}", e);
            }
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    fn reset_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.has_jfif = false;
        self.mode = JpegMode::Baseline;
        self.coding_method = JpegCodingMethod::Huffman;
        self.mcu_width = 0;
        self.mcu_height = 0;
        self.precision = 0;
        self.component_count = 0;
        self.successive_approximation_low = 0;
        self.components.clear();
        self.quantization_tables.clear();
        self.ac_huffman_tables.clear();
        self.dc_huffman_tables.clear();
        self.ac_arithmetic_tables.clear();
        self.dc_arithmetic_tables.clear();
        self.horizontal_sampling_factor = 1;
        self.vertical_sampling_factor = 1;
        self.restart_interval = 0;
        self.scans.clear();
        self.adobe_color_transform = None;
        self.is_hierarchical = false;
        self.dhp_width = 0;
        self.dhp_height = 0;
        self.pending_expand_h = false;
        self.pending_expand_v = false;
        self.hierarchical_frames.clear();
        self.icc.clear();
        self.collected_scans = 0;
        self.frame_has_scan = false;
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    fn record(&mut self, offset: u64, length: Option<u64>, section: JpegSection) {
        if let Some(info) = &mut self.info {
            info.push(offset, length, section);
        }
    }

    fn read_segment_payload(&mut self) -> VexelResult<(u16, Vec<u8>)> {
        let length = self.reader.read_u16()?;
        let payload_len = usize::from(length.saturating_sub(2));
        let mut payload = vec![0u8; payload_len];
        let mut filled = 0;
        while filled < payload_len {
            match self.reader.read(&mut payload[filled..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        payload.truncate(filled);
        Ok((length, payload))
    }

    fn skip_unknown_marker_segment(&mut self, marker: u16, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::Unknown(marker));

        for _ in 0..length.saturating_sub(2) {
            self.reader.read_u8()?;
        }

        Ok(())
    }

    fn read_app(&mut self, index: u8, segment_start: u64) -> VexelResult<()> {
        let (length, payload) = self.read_segment_payload()?;
        let segment = parse_app(index, &payload, segment_start + 4, &mut self.icc);

        match &segment {
            AppSegment::Jfif(_) => self.has_jfif = true,
            AppSegment::Adobe(adobe) => {
                self.adobe_color_transform = Some(adobe.color_transform);
                log_debug!("Adobe APP14: color_transform={}", adobe.color_transform);
            }
            AppSegment::Generic(_) if index == 0 => {
                log_warn!("APP0 segment is neither JFIF nor JFXX");
            }
            _ => {}
        }

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::App(AppSection { index, segment }));

        Ok(())
    }

    fn read_com(&mut self, segment_start: u64) -> VexelResult<()> {
        let (length, payload) = self.read_segment_payload()?;
        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::Comment(parse_comment(&payload)));
        Ok(())
    }

    fn detect_colorspace(&self) -> JpegColorspace {
        let nc = self.components.len();
        let ids: Vec<u8> = self.components.iter().map(|c| c.id).collect();

        match nc {
            1 => JpegColorspace::Grayscale,
            3 => {
                if ids == [0x52, 0x47, 0x42] {
                    return JpegColorspace::RGB;
                }
                if self.has_jfif {
                    return JpegColorspace::YCbCr;
                }
                if let Some(ct) = self.adobe_color_transform {
                    return match ct {
                        0 => JpegColorspace::RGB,
                        _ => JpegColorspace::YCbCr,
                    };
                }
                JpegColorspace::YCbCr
            }
            4 => {
                if ids == [0x43, 0x4D, 0x59, 0x4B] {
                    return JpegColorspace::CMYK;
                }
                if let Some(ct) = self.adobe_color_transform {
                    return match ct {
                        2 => JpegColorspace::YCCK,
                        _ => JpegColorspace::CMYK,
                    };
                }
                if ids == [0x00, 0x01, 0x02, 0x03] {
                    return JpegColorspace::RGBA;
                }
                JpegColorspace::CMYK
            }
            _ => JpegColorspace::Unknown,
        }
    }

    fn read_start_of_frame(&mut self, marker: u8, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let raw_precision = self.reader.read_u8()?;
        self.precision = raw_precision;

        if self.mode == JpegMode::Lossless {
            if self.precision < 2 || self.precision > 16 {
                log_warn!("Invalid precision for lossless jpeg mode: {}, clamping", self.precision);
                self.precision = self.precision.clamp(2, 16);
            }
        } else if self.precision < 8 || self.precision > 16 {
            log_warn!("Invalid precision: {}, clamping to 8", self.precision);
            self.precision = 8;
        }

        let raw_height = self.reader.read_u16()?;
        let raw_width = self.reader.read_u16()?;
        self.height = u32::from(raw_height);
        self.width = u32::from(raw_width);

        let raw_component_count = self.reader.read_u8()?;
        self.component_count = raw_component_count;

        if self.component_count > 4 || self.component_count == 0 {
            log_warn!("Invalid number of components in SOF marker: {}, assuming 3", self.component_count);
            self.component_count = 3;
        }

        self.components.clear();
        let mut frame_components = Vec::with_capacity(usize::from(self.component_count));

        for _ in 0..self.component_count {
            let id = match self.reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };
            let sampling_factors = self.reader.read_u8().unwrap_or(0x11);
            let horizontal_sampling_factor = ((sampling_factors >> 4) & 0xF).max(1);
            let vertical_sampling_factor = (sampling_factors & 0xF).max(1);
            let quantization_table_id = self.reader.read_u8().unwrap_or(0);

            if id == 1 {
                self.horizontal_sampling_factor = horizontal_sampling_factor;
                self.vertical_sampling_factor = vertical_sampling_factor;
            }

            frame_components.push(JpegFrameComponent {
                id,
                horizontal_sampling: sampling_factors >> 4,
                vertical_sampling: sampling_factors & 0xF,
                quantization_table: quantization_table_id,
            });

            self.components.push(ColorComponentInfo {
                id,
                horizontal_sampling_factor,
                vertical_sampling_factor,
                quantization_table_id,
                dc_table_selector: 0,
                ac_table_selector: 0,
            });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::Frame(JpegFrame {
                marker,
                precision: raw_precision,
                height: raw_height,
                width: raw_width,
                component_count: raw_component_count,
                components: frame_components,
            }),
        );

        if length != 8 + 3 * self.component_count as u16 {
            log_warn!("Invalid SOF marker length, expected {}, got {}", 8 + 3 * self.component_count, length);
        }

        if self.width == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if self.height == 0 {
            log_warn!("SOF height is 0, expecting DNL marker to define number of lines");
        }

        if self.walk_mode == WalkMode::Decode {
            self.limits.reserve_buffer(self.width, self.height, 4)?;
            self.limits.free(u64::from(self.width).saturating_mul(u64::from(self.height)).saturating_mul(4));
        }

        // TODO rename them, they are not MCU dimensions, but dimensions of the image in MCUs
        self.mcu_width = self.width.div_ceil(8);
        self.mcu_height = self.height.div_ceil(8);

        Ok(())
    }

    fn read_restart_interval(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.restart_interval = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::RestartInterval(self.restart_interval));

        Ok(())
    }

    fn read_dnl(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        if length != 4 {
            log_warn!("Invalid DNL segment length: {}, expected 4", length);
        }

        let number_of_lines = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::NumberOfLines(number_of_lines));

        if number_of_lines == 0 {
            log_warn!("DNL specifies 0 lines, ignoring");
        } else {
            if self.walk_mode == WalkMode::Decode {
                self.limits.reserve_buffer(self.width, u32::from(number_of_lines), 4)?;
                self.limits.free(u64::from(self.width).saturating_mul(u64::from(number_of_lines)).saturating_mul(4));
            }
            self.height = number_of_lines as u32;
            self.mcu_height = self.height.div_ceil(8);
            log_debug!("DNL: updated height to {}", self.height);
        }

        Ok(())
    }

    fn read_quantization_entry(&mut self) -> std::io::Result<(u8, u8, Vec<u16>)> {
        let table_spec = self.reader.read_u8()?;
        let id = table_spec & 0x0F;
        let precision = (table_spec >> 4) & 0x0F;

        let mut table = Vec::with_capacity(64);
        if precision == 0 {
            for _ in 0..64 {
                table.push(self.reader.read_u8()? as u16);
            }
        } else {
            for _ in 0..64 {
                table.push(self.reader.read_u16()?);
            }
        }

        Ok((id, precision, table))
    }

    fn read_quantization_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut table_length = (segment_length as i16).saturating_sub(2);

        let mut info_tables = Vec::new();
        let mut failure = None;

        while table_length > 0 {
            let (id, precision, table) = match self.read_quantization_entry() {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };

            table_length = table_length.saturating_sub(1);
            table_length = table_length.saturating_sub(if precision == 0 { 64 } else { 128 });

            let table_natural = Self::unzigzag_block(&table).to_vec();
            info_tables.push(JpegQuantizationTable { id, precision, values: table_natural.clone() });
            self.quantization_tables.push(QuantizationTable { id, table_natural });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(segment_length)),
            JpegSection::QuantizationTables(JpegQuantizationTables {
                tables: info_tables,
                truncated: failure.is_some(),
            }),
        );

        match failure {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    fn unzigzag_block(block: &[u16]) -> [u16; 64] {
        let mut unzigzagged = [0u16; 64];

        for i in 0..64 {
            unzigzagged[ZIGZAG_MAP[i] as usize] = block.get(i).copied().unwrap_or(0);
        }

        unzigzagged
    }

    fn read_huffman_entry(&mut self) -> std::io::Result<RawHuffmanTable> {
        let table_spec = self.reader.read_bits(8)?;
        let id = (table_spec & 0x0F) as u8;
        let class = ((table_spec >> 4) & 0x0F) as u8;

        let mut counts = Vec::with_capacity(16);
        let mut offsets = Vec::with_capacity(17);
        let mut total_symbols: u32 = 0;

        offsets.push(0);
        for _ in 0..16 {
            let count = self.reader.read_bits(8)? as u8;
            counts.push(count);
            total_symbols += count as u32;
            offsets.push(total_symbols);
        }

        if total_symbols > 256 {
            log_warn!("Too many symbols in Huffman table: {}", total_symbols);
            total_symbols = 256;
            for offset in offsets.iter_mut() {
                *offset = (*offset).min(total_symbols);
            }
        }

        let mut symbols = Vec::with_capacity(total_symbols as usize);
        for _ in 0..total_symbols {
            symbols.push(self.reader.read_bits(8)? as u8);
        }

        Ok(RawHuffmanTable { id, class, counts, offsets, symbols })
    }

    fn read_huffman_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let total_length = self.reader.read_bits(16)? as u16;
        let mut segment_length = (total_length as i16).saturating_sub(2);

        let mut info_tables = Vec::new();
        let mut failure = None;

        while segment_length > 0 {
            let RawHuffmanTable { id, class, counts, offsets, symbols } = match self.read_huffman_entry() {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };

            let total_symbols = symbols.len() as u32;
            segment_length -= 1 + 16 + total_symbols as i16;

            let mut huffman_table = HuffmanTable {
                id,
                symbols: symbols.clone(),
                offsets,
                codes: vec![0; total_symbols as usize],
                first_code: vec![u32::MAX; 16],
                fast_lookup: Vec::new(),
            };

            let mut code = 0;
            for i in 0..16 {
                if huffman_table.offsets.len() <= i + 1 {
                    log_warn!("Offset index {} is out of bounds in Huffman table", i);
                    break;
                }

                for k in huffman_table.offsets[i]..huffman_table.offsets[i + 1] {
                    if huffman_table.codes.len() <= k as usize {
                        log_warn!("Code index {} is out of bounds in Huffman table", k);
                        break;
                    }

                    huffman_table.codes[k as usize] = code;
                    code += 1;
                }

                code <<= 1;
            }

            for i in 0..16 {
                if huffman_table.offsets.len() > i + 1 {
                    let start = huffman_table.offsets[i] as usize;
                    let end = huffman_table.offsets[i + 1] as usize;
                    if start < end && start < huffman_table.codes.len() {
                        huffman_table.first_code[i] = huffman_table.codes[start];
                    }
                }
            }

            huffman_table.build_fast_lookup();
            info_tables.push(JpegHuffmanTable { class, id, counts, symbols });

            match class {
                0 => {
                    if let Some(existing_table) = self.dc_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.dc_huffman_tables.push(huffman_table);
                    }
                }
                1 => {
                    if let Some(existing_table) = self.ac_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.ac_huffman_tables.push(huffman_table);
                    }
                }
                _ => {
                    log_warn!("Invalid Huffman table class: {}, ignoring the table", class);
                }
            }
        }

        self.record(
            segment_start,
            Some(2 + u64::from(total_length)),
            JpegSection::HuffmanTables(JpegHuffmanTables { tables: info_tables, truncated: failure.is_some() }),
        );

        match failure {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    fn read_dac(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut data_length = segment_length.saturating_sub(2);

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();
        let mut info_tables = Vec::new();
        let mut failure = None;

        while data_length >= 2 {
            let entry = self.reader.read_u8().and_then(|info| Ok((info, self.reader.read_u8()?)));
            let (table_info, raw_value) = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };
            let table_class = (table_info >> 4) & 0x0F;
            let identifier = table_info & 0x0F;

            info_tables.push(JpegArithmeticConditioning { class: table_class, id: identifier, value: raw_value });

            let (value, length) =
                if table_class == 0 { ((raw_value >> 4) & 0x0F, raw_value & 0x0F) } else { (raw_value, 0) };

            let ac_value = ArithmeticCodingValue { value, length };

            let table = ArithmeticCodingTable { identifier, values: Vec::from([ac_value]) };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => {
                    log_warn!("Invalid arithmetic coding table class: {}, ignoring the table", table_class);
                }
            }

            data_length -= 2;
        }

        self.record(
            segment_start,
            Some(2 + u64::from(segment_length)),
            JpegSection::ArithmeticConditioning(info_tables),
        );

        if let Some(e) = failure {
            return Err(e.into());
        }

        self.ac_arithmetic_tables = ac_tables;
        self.dc_arithmetic_tables = dc_tables;

        Ok(())
    }

    fn read_start_of_scan(&mut self, segment_start: u64) -> VexelResult<()> {
        if self.coding_method == JpegCodingMethod::Huffman {
            self.ensure_default_huffman_tables();
        }

        let collect = self.walk_mode == WalkMode::Decode && self.collected_scans < MAX_SCANS;
        if self.walk_mode == WalkMode::Decode && !collect {
            log_warn!("More than {} scans in the image, ignoring the rest", MAX_SCANS);
        }
        if self.width > 0 && !self.components.is_empty() {
            self.frame_has_scan = true;
        }

        let length = self.reader.read_u16()?;

        let scan_component_count = self.reader.read_u8()?;

        let mut scan_components = Vec::new();
        for _ in 0..scan_component_count {
            let component_selector = self.reader.read_u8()?;
            let table_selectors = self.reader.read_u8()?;

            scan_components.push(ScanComponent {
                component_id: component_selector,
                dc_table_selector: (table_selectors >> 4) & 0x0F,
                ac_table_selector: table_selectors & 0x0F,
            });

            if let Some(color_component) = self.components.iter_mut().find(|c| c.id == component_selector) {
                color_component.dc_table_selector = (table_selectors >> 4) & 0x0F;
                color_component.ac_table_selector = table_selectors & 0x0F;
            }
        }

        let raw_start_spectral = self.reader.read_u8()?;
        let raw_end_spectral = self.reader.read_u8()?;
        let start_spectral = raw_start_spectral.min(63);
        let end_spectral = raw_end_spectral.min(63);
        let successive_approx = self.reader.read_u8()?;
        let successive_high = (successive_approx >> 4) & 0x0F;
        let successive_low = successive_approx & 0x0F;

        if length != 6 + (2 * scan_component_count as u16) {
            log_warn!("Invalid SOS marker length, expected {}, got {}", 6 + (2 * scan_component_count as u16), length);
        }

        let scan_start = self.reader.stream_position()?;
        let scan_data_limit = if collect {
            (self.width as usize).saturating_mul(self.height as usize).saturating_mul(4).max(65536)
        } else {
            usize::MAX
        };
        if collect {
            self.limits.reserve_usize(scan_data_limit)?;
        }
        let remaining = if collect { self.reader.bytes_left().unwrap_or(0) as usize } else { 0 };
        let mut scan_data = Vec::with_capacity(remaining.min(scan_data_limit));
        let is_arithmetic = self.coding_method == JpegCodingMethod::Arithmetic;

        let mut restart_offsets: Vec<usize> = vec![0];
        let mut restart_markers = 0u32;
        let mut block = vec![0u8; SCAN_BLOCK_SIZE];
        let mut filled = 0usize;
        let mut cursor = 0usize;
        let mut block_origin = 0u64;
        let mut truncated = false;
        let mut end_marker: Option<u64> = None;

        loop {
            if filled - cursor < 2 {
                block.copy_within(cursor..filled, 0);
                block_origin += cursor as u64;
                filled -= cursor;
                cursor = 0;

                match self.reader.read(&mut block[filled..]) {
                    Ok(0) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                    Ok(n) => filled += n,
                    Err(_) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                }
                continue;
            }

            let chunk = &block[cursor..filled];
            let run = chunk.iter().position(|&b| b == 0xFF).unwrap_or(chunk.len());
            if run > 0 {
                if collect {
                    let space = scan_data_limit.saturating_sub(scan_data.len());
                    if run >= space {
                        scan_data.extend_from_slice(&chunk[..space]);
                        cursor += space;
                        truncated = true;
                        break;
                    }
                    scan_data.extend_from_slice(&chunk[..run]);
                }
                cursor += run;
                continue;
            }

            if collect && scan_data.len() >= scan_data_limit {
                truncated = true;
                break;
            }

            match block[cursor + 1] {
                0x00 => {
                    if collect {
                        scan_data.push(0xFF);
                        if is_arithmetic {
                            scan_data.push(0x00);
                        }
                    }
                    cursor += 2;
                }
                0xFF => {
                    cursor += 1;
                }
                b if b >= (JpegMarker::RST0.to_u16() & 0xFF) as u8 && b <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 => {
                    restart_markers += 1;
                    if collect {
                        scan_data.push(0xFF);
                        scan_data.push(b);
                        restart_offsets.push(scan_data.len());
                    }
                    cursor += 2;
                }
                b if matches!(JpegMarker::from_u16(0xFF00 | b as u16), None | Some(JpegMarker::RES(_))) => {
                    log_warn!("Skipping invalid marker 0xFF{:02X} inside scan data", b);
                    cursor += 2;
                }
                _ => {
                    end_marker = Some(block_origin + cursor as u64);
                    break;
                }
            }
        }

        if truncated {
            log_warn!("Scan data exceeded limit of {} bytes, truncating", scan_data_limit);
        }

        let stop = end_marker.unwrap_or(block_origin + cursor as u64);
        self.reader.seek(SeekFrom::Start(scan_start + stop))?;

        let lossless = matches!(self.mode, JpegMode::Lossless | JpegMode::DifferentialLossless);
        let header_span = scan_start.saturating_sub(segment_start);
        self.record(
            segment_start,
            Some(header_span + stop),
            JpegSection::Scan(JpegScan {
                header_length: length,
                components: scan_components
                    .iter()
                    .map(|c| JpegScanComponent {
                        id: c.component_id,
                        dc_table: c.dc_table_selector,
                        ac_table: c.ac_table_selector,
                    })
                    .collect(),
                spectral_start: raw_start_spectral,
                spectral_end: raw_end_spectral,
                approximation_high: successive_high,
                approximation_low: successive_low,
                lossless,
                restart_interval: self.restart_interval,
                entropy_coded_length: stop,
                restart_markers,
            }),
        );

        if !collect {
            return Ok(());
        }

        let actual_scan_size = scan_data.len();
        self.limits.free_usize(scan_data_limit.saturating_sub(actual_scan_size));
        scan_data.shrink_to_fit();

        if scan_components.len() > MAX_SCAN_COMPONENTS {
            log_warn!("Scan lists {} components, using the first {}", scan_components.len(), MAX_SCAN_COMPONENTS);
            scan_components.truncate(MAX_SCAN_COMPONENTS);
        }

        let dc_tables = self
            .dc_huffman_tables
            .iter()
            .filter(|t| scan_components.iter().any(|c| c.dc_table_selector == t.id))
            .cloned()
            .collect();
        let ac_tables = self
            .ac_huffman_tables
            .iter()
            .filter(|t| scan_components.iter().any(|c| c.ac_table_selector == t.id))
            .cloned()
            .collect();

        let scan = ScanData {
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            components: scan_components,
            dc_tables,
            ac_tables,
            arith_dc_tables: self.dc_arithmetic_tables.clone(),
            arith_ac_tables: self.ac_arithmetic_tables.clone(),
            data: scan_data,
            restart_offsets,
        };

        self.scans.push(scan);
        self.collected_scans += 1;

        Ok(())
    }

    fn build_huffman_table_from_bits(id: u8, bits: &[u8; 16], vals: &[u8]) -> HuffmanTable {
        let mut offsets = Vec::with_capacity(17);
        offsets.push(0u32);
        let mut total: u32 = 0;
        for &b in bits.iter() {
            total += b as u32;
            offsets.push(total);
        }

        let symbols: Vec<u8> = vals[..total as usize].to_vec();
        let mut codes = vec![0u32; total as usize];
        let mut first_code = vec![u32::MAX; 16];

        let mut code: u32 = 0;
        for i in 0..16 {
            for k in offsets[i]..offsets[i + 1] {
                codes[k as usize] = code;
                code += 1;
            }
            code <<= 1;
        }

        for i in 0..16 {
            let start = offsets[i] as usize;
            let end = offsets[i + 1] as usize;
            if start < end && start < codes.len() {
                first_code[i] = codes[start];
            }
        }

        let mut t = HuffmanTable { id, symbols, offsets, codes, first_code, fast_lookup: Vec::new() };
        t.build_fast_lookup();
        t
    }

    fn ensure_default_huffman_tables(&mut self) {
        #[rustfmt::skip]
        const BITS_DC_LUMA: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
        #[rustfmt::skip]
        const VALS_DC_LUMA: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

        #[rustfmt::skip]
        const BITS_DC_CHROMA: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
        #[rustfmt::skip]
        const VALS_DC_CHROMA: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

        #[rustfmt::skip]
        const BITS_AC_LUMA: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
        #[rustfmt::skip]
        const VALS_AC_LUMA: [u8; 162] = [
            0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12,
            0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
            0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08,
            0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
            0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16,
            0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
            0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
            0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
            0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
            0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98,
            0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
            0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
            0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
            0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4,
            0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
            0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea,
            0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
            0xf9, 0xfa,
        ];

        #[rustfmt::skip]
        const BITS_AC_CHROMA: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
        #[rustfmt::skip]
        const VALS_AC_CHROMA: [u8; 162] = [
            0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21,
            0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
            0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91,
            0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
            0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34,
            0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
            0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
            0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
            0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
            0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
            0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
            0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96,
            0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
            0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
            0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
            0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2,
            0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
            0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9,
            0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
            0xf9, 0xfa,
        ];

        if !self.dc_huffman_tables.iter().any(|t| t.id == 0) {
            self.dc_huffman_tables.push(Self::build_huffman_table_from_bits(0, &BITS_DC_LUMA, &VALS_DC_LUMA));
        }
        if !self.dc_huffman_tables.iter().any(|t| t.id == 1) {
            self.dc_huffman_tables.push(Self::build_huffman_table_from_bits(1, &BITS_DC_CHROMA, &VALS_DC_CHROMA));
        }
        if !self.ac_huffman_tables.iter().any(|t| t.id == 0) {
            self.ac_huffman_tables.push(Self::build_huffman_table_from_bits(0, &BITS_AC_LUMA, &VALS_AC_LUMA));
        }
        if !self.ac_huffman_tables.iter().any(|t| t.id == 1) {
            self.ac_huffman_tables.push(Self::build_huffman_table_from_bits(1, &BITS_AC_CHROMA, &VALS_AC_CHROMA));
        }
    }

    fn default_lossless_dc_table() -> HuffmanTable {
        let mut t = HuffmanTable {
            id: 0,
            symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            codes: vec![
                0b000,
                0b010,
                0b011,
                0b100,
                0b101,
                0b110,
                0b1110,
                0b11110,
                0b111110,
                0b1111110,
                0b11111110,
                0b111111110,
            ],
            first_code: vec![
                u32::MAX,
                u32::MAX,
                0b000,
                0b011,
                u32::MAX,
                0b100,
                0b101,
                0b110,
                0b1110,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
            ],
            fast_lookup: Vec::new(),
        };
        t.build_fast_lookup();
        t
    }

    #[inline(always)]
    fn decode_mcu(
        &self,
        reader: &mut JpegBitReader<'_>,
        mcu_component: &mut [i16; 64],
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        previous_dc: &mut i32,
        is_differential: bool,
    ) -> VexelResult<()> {
        let length = get_next_symbol(reader, dc_table);

        if length > 15 {
            log_warn!("Invalid DC coefficient length (>15): {}", length);
            return Ok(());
        }

        let mut dc_coeff = reader.read_bits(length as u32) as i32;

        if length != 0 && dc_coeff < (1 << (length - 1)) {
            dc_coeff -= (1 << length) - 1;
        }

        let dc_value = if is_differential {
            *previous_dc = dc_coeff;
            dc_coeff
        } else {
            let v = dc_coeff.wrapping_add(*previous_dc);
            *previous_dc = v;
            v
        };
        unsafe {
            *mcu_component.get_unchecked_mut(0) = dc_value as i16;
        }

        let mut i = 1usize;
        while i < 64 {
            let symbol = get_next_symbol(reader, ac_table);

            if symbol == 0 {
                return Ok(());
            }

            let mut zero_count = symbol >> 4;
            let mut coefficient_length = symbol & 0xF;

            if symbol == 0xF0 {
                zero_count = 16;
                coefficient_length = 0;
            }

            if i + zero_count as usize >= 64 {
                log_warn!("Sum of zero count and current index of mcu value exceeds 64");
                return Ok(());
            }

            i += zero_count as usize;

            let max_coefficient_length = if self.precision > 8 { 16 } else { 10 };
            if coefficient_length > max_coefficient_length {
                log_warn!("Invalid coefficient length: {}, replacing with 0", coefficient_length);
                i += 1;
                continue;
            }

            if coefficient_length != 0 {
                let mut coefficient = reader.read_bits(coefficient_length as u32) as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                unsafe {
                    *mcu_component.get_unchecked_mut(*ZIGZAG_MAP.get_unchecked(i) as usize) = coefficient as i16;
                }
                i += 1;
            }
        }

        Ok(())
    }

    fn decode_progressive(&mut self) -> VexelResult<Image> {
        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

        let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
        let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

        let components_snapshot: Vec<_> =
            self.components.iter().map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor)).collect();
        let mut component_planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
        for (h, v) in &components_snapshot {
            let comp_width = mcu_width * 8 * *h as u32;
            let comp_height = mcu_height * 8 * *v as u32;
            component_planes.push(ComponentPlane::try_new(comp_width, comp_height, &mut self.limits)?);
        }

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut component_planes)?,
        }
        let raster_planes = self.dequantize_and_idct_planes(&component_planes, true);
        for plane in &component_planes {
            plane.free(&mut self.limits);
        }
        drop(component_planes);

        let mut pixel_data = self.upsample_and_convert(&raster_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut previous_dc = vec![0i32; planes.len()];
        let is_differential = matches!(self.mode, JpegMode::DifferentialProgressive | JpegMode::DifferentialSequential);
        let max_ac_len: u8 = if self.precision > 8 { 15 } else { 10 };
        let block_budget = Self::progressive_block_budget(planes);
        let mut blocks_visited = 0u64;

        for scan in &self.scans {
            if blocks_visited > block_budget {
                log_warn!("Progressive scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }

            let mut reader = JpegBitReader::new(scan.data.as_slice());
            let mut skips: u32 = 0;
            let restart_interval = self.restart_interval;

            let mut max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
            let mut max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

            let is_non_interleaved = scan.components.len() == 1;

            if is_non_interleaved {
                max_h_samp = 1;
                max_v_samp = 1;
            }

            let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
            let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

            let scan_comp_infos: Vec<ProgressiveScanComp<'_>> = scan
                .components
                .iter()
                .filter_map(|scan_comp| {
                    let (plane_index, comp) =
                        match self.components.iter().enumerate().find(|(_, c)| c.id == scan_comp.component_id) {
                            Some((i, c)) => (i, c),
                            None => {
                                log_warn!("Component not found: {}", scan_comp.component_id);
                                return None;
                            }
                        };

                    if plane_index >= planes.len() {
                        log_warn!("Invalid plane index: {}", plane_index);
                        return None;
                    }

                    let h_blocks = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
                    let v_blocks = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

                    let dc_table = scan.dc_tables.iter().find(|t| t.id == scan_comp.dc_table_selector);
                    let ac_table = scan.ac_tables.iter().find(|t| t.id == scan_comp.ac_table_selector);

                    if scan.start_spectral == 0 && scan.successive_high == 0 && dc_table.is_none() {
                        log_warn!("DC table not found: {}", scan_comp.dc_table_selector);
                    }
                    if scan.end_spectral > 0 && ac_table.is_none() {
                        log_warn!("AC table not found: {}", scan_comp.ac_table_selector);
                    }

                    Some(ProgressiveScanComp { plane_index, h_blocks, v_blocks, dc_table, ac_table })
                })
                .collect();

            let start_spectral = scan.start_spectral as usize;
            let end_spectral = (scan.end_spectral as usize).min(63);
            let successive_low = scan.successive_low;
            let is_first_pass = scan.successive_high == 0;
            let has_dc = scan.start_spectral == 0;
            let has_ac = scan.end_spectral > 0;
            let positive = 1 << successive_low;
            let negative = -1 << successive_low;

            macro_rules! run_scan {
                ($body:expr) => {
                    run_progressive_blocks(
                        planes,
                        &scan_comp_infos,
                        mcu_width,
                        mcu_height,
                        restart_interval,
                        &mut reader,
                        &mut previous_dc,
                        $body,
                    )
                };
            }

            let blocks_per_mcu: u64 =
                scan_comp_infos.iter().map(|info| u64::from(info.h_blocks) * u64::from(info.v_blocks)).sum();

            let processed_mcus = match (has_dc, has_ac, is_first_pass) {
                (true, false, true) => run_scan!(|reader, block, info, previous_dc| {
                    prog_dc_first(
                        reader,
                        block,
                        info.dc_table,
                        &mut previous_dc[info.plane_index],
                        successive_low,
                        is_differential,
                    );
                }),
                (true, false, false) => run_scan!(|reader, block, _info, _previous_dc| {
                    prog_dc_refine(reader, block, successive_low);
                }),
                (false, true, true) => run_scan!(|reader, block, info, _previous_dc| {
                    prog_ac_first(
                        reader,
                        block,
                        info.ac_table,
                        &mut skips,
                        start_spectral,
                        end_spectral,
                        successive_low,
                        max_ac_len,
                    );
                }),
                (false, true, false) => run_scan!(|reader, block, info, _previous_dc| {
                    prog_ac_refine(
                        reader,
                        block,
                        info.ac_table,
                        &mut skips,
                        start_spectral,
                        end_spectral,
                        positive,
                        negative,
                    );
                }),
                (true, true, true) => run_scan!(|reader, block, info, previous_dc| {
                    if prog_dc_first(
                        reader,
                        block,
                        info.dc_table,
                        &mut previous_dc[info.plane_index],
                        successive_low,
                        is_differential,
                    ) {
                        prog_ac_first(
                            reader,
                            block,
                            info.ac_table,
                            &mut skips,
                            start_spectral,
                            end_spectral,
                            successive_low,
                            max_ac_len,
                        );
                    }
                }),
                (true, true, false) => run_scan!(|reader, block, info, _previous_dc| {
                    prog_dc_refine(reader, block, successive_low);
                    prog_ac_refine(
                        reader,
                        block,
                        info.ac_table,
                        &mut skips,
                        start_spectral,
                        end_spectral,
                        positive,
                        negative,
                    );
                }),
                (false, false, _) => 0,
            };
            blocks_visited = blocks_visited.saturating_add(processed_mcus.saturating_mul(blocks_per_mcu));
        }
        Ok(())
    }

    fn progressive_block_budget(planes: &[ComponentPlane]) -> u64 {
        let total_blocks: u64 = planes.iter().map(|plane| plane.data.len() as u64 / 64).sum();
        total_blocks.saturating_mul(PROGRESSIVE_PASS_BUDGET)
    }

    fn decode_differences(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let mut reader = JpegBitReader::new(scan.data.as_slice());

        let width = self.width as usize;
        let height = self.height as usize;
        let num_components = scan.components.len();
        let buf_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(4);
        self.limits.reserve_usize(buf_bytes)?;

        let mut differences: Vec<Vec<i32>> = vec![vec![]; num_components];

        for diffs in &mut differences {
            diffs.reserve(width * height);
        }

        let restart_interval = self.restart_interval as usize;
        let mut mcus_until_restart = restart_interval;

        let default_table = Self::default_lossless_dc_table();
        let tables: Vec<&HuffmanTable> = scan
            .components
            .iter()
            .enumerate()
            .map(|(i, scan_component)| {
                scan.dc_tables.iter().find(|t| t.id == scan_component.dc_table_selector).unwrap_or_else(|| {
                    log_warn!("No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.", i);
                    &default_table
                })
            })
            .collect();

        for _ in 0..height {
            if reader.is_exhausted() {
                log_warn!("Lossless scan data exhausted, filling the remaining differences with zeros");
                break;
            }

            for _ in 0..width {
                if restart_interval > 0 {
                    if mcus_until_restart == 0 {
                        reader.clear_buffer();
                        mcus_until_restart = restart_interval;
                    }
                    mcus_until_restart -= 1;
                }

                for (i, &dc_table) in tables.iter().enumerate() {
                    let bits_to_read = get_next_symbol(&mut reader, dc_table);

                    let diff = match bits_to_read {
                        0 => 0,
                        1..=15 => {
                            let additional_bits = reader.read_bits(bits_to_read as u32) as i32;

                            if additional_bits < (1 << (bits_to_read - 1)) {
                                additional_bits + (-1 << bits_to_read) + 1
                            } else {
                                additional_bits
                            }
                        }
                        16 if self.lossless_ssss16_is_min => 32768,
                        16 => {
                            let v = reader.read_bits(16) as i32;
                            if v < (1 << 15) {
                                v - 65535
                            } else {
                                v
                            }
                        }
                        _ => {
                            log_warn!("Invalid difference: {}", bits_to_read);
                            0
                        }
                    };

                    differences[i].push(diff);
                }
            }
        }

        for diffs in &mut differences {
            diffs.resize(width * height, 0);
        }

        Ok(differences)
    }

    fn decode_differences_arithmetic(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let num_components = scan.components.len();
        let width = self.width as usize;
        let height = self.height as usize;

        let diff_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(4);
        let db_bytes = width.saturating_mul(num_components).saturating_mul(4);
        self.limits.reserve_usize(diff_bytes.saturating_add(db_bytes))?;

        let mut differences: Vec<Vec<i32>> = vec![vec![0i32; width * height]; num_components];

        let l_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.length)
                    .unwrap_or(0)
            })
            .collect();

        let u_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(1)
            })
            .collect();

        let ctx_table_indices: Vec<usize> =
            scan.components.iter().map(|sc| (sc.dc_table_selector & 0x3) as usize).collect();

        let mut all_contexts = vec![[0u8; 160]; 4];
        let mut arith = ArithmeticDecoder::new(&scan.data);

        let mut da: Vec<i32> = vec![0i32; num_components];
        let mut db: Vec<Vec<i32>> = vec![vec![0i32; width]; num_components];

        for y in 0..height {
            if arith.is_exhausted() {
                log_warn!("Lossless arithmetic scan data exhausted, filling the remaining differences with zeros");
                break;
            }

            da.iter_mut().for_each(|d| *d = 0);

            for x in 0..width {
                #[allow(clippy::needless_range_loop)]
                for c in 0..num_components {
                    let l = l_values[c];
                    let u = u_values[c];
                    let ctx_idx = ctx_table_indices[c];

                    let v = arith.decode_lossless_difference(&mut all_contexts[ctx_idx], da[c], db[c][x], l, u);

                    differences[c][y * width + x] = v;
                    da[c] = v;
                    db[c][x] = v;
                }
            }
        }

        self.limits.free_usize(db_bytes);

        Ok(differences)
    }

    fn predict(
        ra: i32,
        rb: i32,
        rc: i32,
        predictor: Predictor,
        point_transform: u8,
        input_precision: u8,
        at_interval_start: bool,
        on_interval_first_line: bool,
        x: usize,
    ) -> i32 {
        if at_interval_start {
            if input_precision > point_transform + 1 {
                1 << (input_precision - point_transform - 1)
            } else {
                0
            }
        } else if on_interval_first_line {
            ra
        } else if x == 0 {
            rb
        } else {
            match predictor {
                Predictor::NoPrediction => 0,
                Predictor::Ra => ra,
                Predictor::Rb => rb,
                Predictor::Rc => rc,
                Predictor::RaRbRc1 => ra + rb - rc,
                Predictor::RaRbRc2 => ra + ((rb - rc) >> 1),
                Predictor::RaRbRc3 => rb + ((ra - rc) >> 1),
                Predictor::RaRb => (ra + rb) / 2,
            }
        }
    }

    fn reconstruct_samples(
        &mut self,
        differences: Vec<Vec<i32>>,
        predictor: Predictor,
        point_transform: u8,
    ) -> VexelResult<Vec<Vec<u16>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = differences.len();

        let diff_bytes = width.saturating_mul(height).saturating_mul(components_count).saturating_mul(4);
        let samples_bytes = width.saturating_mul(height).saturating_mul(components_count).saturating_mul(2);
        self.limits.reserve_usize(samples_bytes)?;

        let mut samples = vec![vec![0u16; width * height]; components_count];

        let restart_interval = self.restart_interval as usize;

        if predictor == Predictor::Ra && restart_interval == 0 {
            for component_index in 0..components_count {
                let default_prediction = 1 << (self.precision - point_transform - 1);

                let first_diff = differences[component_index][0];
                samples[component_index][0] = (((default_prediction + first_diff) & 0xFFFF) as u16) << point_transform;

                for y in 1..height {
                    let diff = differences[component_index][y * width];
                    let rb = samples[component_index][(y - 1) * width] as i32;
                    samples[component_index][y * width] = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                }

                for y in 0..height {
                    for x in 1..width {
                        let index = y * width + x;
                        let diff = differences[component_index][index];
                        let ra = samples[component_index][index - 1] as i32;

                        samples[component_index][index] = (((ra + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;

                    let (at_interval_start, on_interval_first_line) = if restart_interval > 0 {
                        let rel = index % restart_interval;
                        (rel == 0, y == (index - rel) / width)
                    } else {
                        (index == 0, y == 0)
                    };

                    for component_index in 0..components_count {
                        let diff = differences[component_index][index];

                        let ra = if x > 0 { samples[component_index][index - 1] as i32 } else { 0 };
                        let rb = if y > 0 { samples[component_index][(y - 1) * width + x] as i32 } else { 0 };
                        let rc =
                            if x > 0 && y > 0 { samples[component_index][(y - 1) * width + (x - 1)] as i32 } else { 0 };

                        let prediction = Self::predict(
                            ra,
                            rb,
                            rc,
                            predictor,
                            point_transform,
                            self.precision,
                            at_interval_start,
                            on_interval_first_line,
                            x,
                        );

                        samples[component_index][index] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        }

        self.limits.free_usize(diff_bytes);

        Ok(samples)
    }

    fn interleaved_samples_to_image(&mut self, mut samples: Vec<u16>, components_count: usize) -> VexelResult<Image> {
        let sample_bytes = samples.len().saturating_mul(2);

        let ids: Vec<u8> = self.components.iter().map(|c| c.id).collect();
        let explicit_ycbcr = self.has_jfif || self.adobe_color_transform.is_some_and(|ct| ct != 0);
        if components_count == 3 && ids != [0x52, 0x47, 0x42] && explicit_ycbcr && !self.lossless_skip_color_transform {
            Self::lossless_ycbcr_to_rgb(&mut samples, self.precision);
        }

        let pixel_data = if self.precision <= 8 {
            let precision_correction = 8 - self.precision;
            self.limits.reserve_usize(samples.len())?;
            let pixels: Vec<u8> = samples.iter().map(|&s| (s as u8) << precision_correction).collect();
            drop(samples);
            self.limits.free_usize(sample_bytes);

            match components_count {
                1 => PixelData::L8(pixels),
                2 => PixelData::LA8(pixels),
                4 => PixelData::RGBA8(pixels),
                _ => PixelData::RGB8(pixels),
            }
        } else {
            let precision_correction = 16 - self.precision;
            let mut pixels = samples;
            if precision_correction > 0 {
                let shift = |chunk: &mut [u16]| {
                    for v in chunk {
                        *v <<= precision_correction;
                    }
                };

                #[cfg(feature = "rayon")]
                {
                    use rayon::prelude::*;
                    pixels.par_chunks_mut(1 << 16).for_each(shift);
                }
                #[cfg(not(feature = "rayon"))]
                shift(&mut pixels);
            }

            match components_count {
                1 => PixelData::L16(pixels),
                2 => PixelData::LA16(pixels),
                4 => PixelData::RGBA16(pixels),
                _ => PixelData::RGB16(pixels),
            }
        };

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn lossless_ycbcr_to_rgb(samples: &mut [u16], precision: u8) {
        let half = 1i32 << (precision.saturating_sub(1));
        let maxval = (1i32 << precision) - 1;

        for pixel in samples.chunks_exact_mut(3) {
            let y = pixel[0] as i32;
            let cb = pixel[1] as i32 - half;
            let cr = pixel[2] as i32 - half;

            let r = y as f32 + 1.402 * cr as f32;
            let g = y as f32 - 0.3441363 * cb as f32 - 0.7141363 * cr as f32;
            let b = y as f32 + 1.772 * cb as f32;

            pixel[0] = r.round().clamp(0.0, maxval as f32) as u16;
            pixel[1] = g.round().clamp(0.0, maxval as f32) as u16;
            pixel[2] = b.round().clamp(0.0, maxval as f32) as u16;
        }
    }

    fn decode_lossless(&mut self) -> VexelResult<Image> {
        if self.scans.is_empty() {
            return Err(VexelError::from(Error::new(ErrorKind::InvalidData, "No scan data found")));
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let num_components = self.components.len().max(1);

        let output_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(2);
        self.limits.reserve_usize(output_bytes)?;
        let mut output = vec![0u16; width * height * num_components];

        let h_max = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1).max(1) as usize;
        let v_max = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1).max(1) as usize;

        let sample_budget = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(num_components as u64)
            .saturating_mul(LOSSLESS_PASS_BUDGET);
        let mut samples_scheduled = 0u64;

        let scans = std::mem::take(&mut self.scans);
        for scan in &scans {
            if scan.components.is_empty() {
                continue;
            }

            samples_scheduled = samples_scheduled.saturating_add(
                (width as u64).saturating_mul(height as u64).saturating_mul(scan.components.len() as u64),
            );
            if samples_scheduled > sample_budget {
                log_warn!("Lossless scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }

            let component_indices: Vec<usize> = scan
                .components
                .iter()
                .enumerate()
                .map(|(i, sc)| {
                    self.components.iter().position(|c| c.id == sc.component_id).unwrap_or_else(|| {
                        log_warn!(
                            "Scan component id {} not found among frame components, using scan order",
                            sc.component_id
                        );
                        i.min(num_components - 1)
                    })
                })
                .collect();

            if scan.components.len() == 1 {
                let ci = component_indices[0];
                let (h_i, v_i) = self
                    .components
                    .get(ci)
                    .map(|c| (c.horizontal_sampling_factor.max(1) as usize, c.vertical_sampling_factor.max(1) as usize))
                    .unwrap_or((1, 1));
                let comp_width = width.saturating_mul(h_i).div_ceil(h_max);
                let comp_height = height.saturating_mul(v_i).div_ceil(v_max);

                if comp_width == width && comp_height == height {
                    self.decode_lossless_scan(scan, width, height, &[(ci, num_components)], &mut output);
                } else {
                    let plane_bytes = comp_width.saturating_mul(comp_height).saturating_mul(2);
                    if self.limits.reserve_usize(plane_bytes).is_err() {
                        log_warn!("Subsampled lossless component plane exceeds limits, skipping");
                        continue;
                    }
                    let mut plane = vec![0u16; comp_width * comp_height];
                    self.decode_lossless_scan(scan, comp_width, comp_height, &[(0, 1)], &mut plane);
                    for y in 0..height {
                        let sy = (y * comp_height / height.max(1)).min(comp_height.saturating_sub(1));
                        for x in 0..width {
                            let sx = (x * comp_width / width.max(1)).min(comp_width.saturating_sub(1));
                            output[(y * width + x) * num_components + ci] = plane[sy * comp_width + sx];
                        }
                    }
                    drop(plane);
                    self.limits.free_usize(plane_bytes);
                }
            } else {
                let sampling: Vec<(usize, usize)> = component_indices
                    .iter()
                    .map(|&ci| {
                        self.components
                            .get(ci)
                            .map(|c| {
                                (
                                    c.horizontal_sampling_factor.max(1) as usize,
                                    c.vertical_sampling_factor.max(1) as usize,
                                )
                            })
                            .unwrap_or((1, 1))
                    })
                    .collect();
                let mixed_sampling = sampling.iter().any(|&(h, v)| (h, v) != (h_max, v_max));

                if mixed_sampling {
                    self.decode_lossless_scan_subsampled(
                        scan,
                        width,
                        height,
                        h_max,
                        v_max,
                        &component_indices,
                        &sampling,
                        num_components,
                        &mut output,
                    );
                } else {
                    let views: Vec<(usize, usize)> = component_indices.iter().map(|&ci| (ci, num_components)).collect();
                    self.decode_lossless_scan(scan, width, height, &views, &mut output);
                }
            }
        }
        self.scans = scans;

        self.interleaved_samples_to_image(output, num_components)
    }

    fn decode_lossless_scan_subsampled(
        &mut self,
        scan: &ScanData,
        width: usize,
        height: usize,
        h_max: usize,
        v_max: usize,
        component_indices: &[usize],
        sampling: &[(usize, usize)],
        num_components: usize,
        output: &mut [u16],
    ) {
        let mcu_cols = width.div_ceil(h_max);
        let mcu_rows = height.div_ceil(v_max);

        let plane_dims: Vec<(usize, usize)> = sampling.iter().map(|&(h, v)| (mcu_cols * h, mcu_rows * v)).collect();

        let total_plane_samples =
            plane_dims.iter().fold(0usize, |total, &(w, h)| total.saturating_add(w.saturating_mul(h)));
        if self.limits.reserve_usize(total_plane_samples.saturating_mul(2)).is_err() {
            log_warn!("Subsampled interleaved lossless scan exceeds limits, skipping");
            return;
        }

        let mut planes: Vec<Vec<u16>> = plane_dims.iter().map(|&(w, h)| vec![0u16; w * h]).collect();

        let point_transform = scan.successive_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        let tables: Vec<HuffmanTable> = (0..sampling.len())
            .map(|i| {
                scan.components
                    .get(i)
                    .and_then(|sc| scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).cloned())
                    .unwrap_or_else(|| {
                        log_warn!(
                            "No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.",
                            i
                        );
                        Self::default_lossless_dc_table()
                    })
            })
            .collect();

        let default_prediction =
            if self.precision > point_transform + 1 { 1i32 << (self.precision - point_transform - 1) } else { 0 };
        let ssss16_is_min = self.lossless_ssss16_is_min;
        let restart_interval = self.restart_interval as usize;

        let mut reader = JpegBitReader::new(scan.data.as_slice());

        let total_mcus = mcu_cols * mcu_rows;
        let interval = if restart_interval > 0 { restart_interval } else { total_mcus };
        let mut mcu = 0usize;
        let mut restart_row_start = 0usize;

        'mcus: while mcu < total_mcus {
            if mcu > 0 {
                reader.clear_buffer();
                restart_row_start = mcu / mcu_cols;
            }
            let end = (mcu + interval).min(total_mcus);

            while mcu < end {
                if reader.is_exhausted() {
                    log_warn!("Lossless scan data exhausted after {} of {} MCUs", mcu, total_mcus);
                    break 'mcus;
                }
                let mcu_x = mcu % mcu_cols;
                let mcu_y = mcu / mcu_cols;

                for (ci, &(sh, sv)) in sampling.iter().enumerate() {
                    let (pw, _) = plane_dims[ci];
                    let plane = &mut planes[ci];
                    let base_x = mcu_x * sh;
                    let base_y = mcu_y * sv;

                    for dy in 0..sv {
                        let y = base_y + dy;
                        for dx in 0..sh {
                            let x = base_x + dx;
                            let diff = Self::decode_lossless_diff(&mut reader, &tables[ci], ssss16_is_min);

                            let at_restart_start = y == restart_row_start * sv && x == 0;
                            let on_first_line_of_interval = y == restart_row_start * sv;

                            let predicted = if at_restart_start {
                                default_prediction
                            } else if x == 0 {
                                plane[(y - 1) * pw] as i32
                            } else if on_first_line_of_interval {
                                plane[y * pw + x - 1] as i32
                            } else {
                                let a = plane[y * pw + x - 1] as i32;
                                let b = plane[(y - 1) * pw + x] as i32;
                                let c = plane[(y - 1) * pw + x - 1] as i32;
                                match predictor {
                                    Predictor::NoPrediction => 0,
                                    Predictor::Ra => a,
                                    Predictor::Rb => b,
                                    Predictor::Rc => c,
                                    Predictor::RaRbRc1 => a + b - c,
                                    Predictor::RaRbRc2 => a + ((b - c) >> 1),
                                    Predictor::RaRbRc3 => b + ((a - c) >> 1),
                                    Predictor::RaRb => (a + b) / 2,
                                }
                            };

                            let value = (((predicted + diff) & 0xFFFF) as u16) << point_transform;
                            plane[y * pw + x] = value;
                        }
                    }
                }

                mcu += 1;
            }
        }

        for (idx, &ci) in component_indices.iter().enumerate() {
            let (pw, ph) = plane_dims[idx];
            let plane = &planes[idx];
            for y in 0..height {
                let sy = (y * ph / height.max(1)).min(ph.saturating_sub(1));
                let row = &plane[sy * pw..sy * pw + pw];
                for x in 0..width {
                    let sx = (x * pw / width.max(1)).min(pw.saturating_sub(1));
                    output[(y * width + x) * num_components + ci] = row[sx];
                }
            }
        }

        self.limits.free_usize(total_plane_samples.saturating_mul(2));
    }

    fn decode_lossless_scan(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
    ) {
        let point_transform = scan.successive_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        if self.coding_method == JpegCodingMethod::Arithmetic {
            self.decode_lossless_scan_arithmetic(scan, sw, sh, views, buf, predictor, point_transform);
        } else {
            self.decode_lossless_scan_huffman(scan, sw, sh, views, buf, predictor, point_transform);
        }
    }

    fn decode_lossless_scan_huffman(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
    ) {
        let total = sw.saturating_mul(sh);
        if total == 0 || views.is_empty() {
            return;
        }

        let views_in_bounds = views.iter().all(|&(offset, stride)| {
            (total - 1)
                .checked_mul(stride)
                .and_then(|last| last.checked_add(offset))
                .is_some_and(|last| last < buf.len())
        });

        if !views_in_bounds {
            log_warn!("Lossless scan views exceed sample buffer bounds, skipping scan");
            return;
        }

        let tables: Vec<HuffmanTable> = (0..views.len())
            .map(|i| {
                scan.components
                    .get(i)
                    .and_then(|sc| scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).cloned())
                    .unwrap_or_else(|| {
                        log_warn!(
                            "No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.",
                            i
                        );
                        Self::default_lossless_dc_table()
                    })
            })
            .collect();

        let mut reader = JpegBitReader::new(scan.data.as_slice());
        let restart_interval = self.restart_interval as usize;
        let default_prediction =
            if self.precision > point_transform + 1 { 1i32 << (self.precision - point_transform - 1) } else { 0 };

        let make = |i: usize| LosslessComponent {
            pos: views[i].0,
            stride: views[i].1,
            row_stride: sw * views[i].1,
            table: &tables[i],
            ra: 0,
            rb: 0,
        };

        let ssss16_is_min = self.lossless_ssss16_is_min;

        #[cfg(feature = "rayon")]
        if Self::decode_lossless_scan_huffman_parallel(
            scan,
            sw,
            sh,
            views,
            buf,
            predictor,
            point_transform,
            default_prediction,
            ssss16_is_min,
            restart_interval,
            &tables,
        ) {
            return;
        }

        macro_rules! run {
            ($components:expr, $predict:expr) => {
                Self::lossless_huffman_loop(
                    &mut reader,
                    sw,
                    total,
                    buf,
                    restart_interval,
                    point_transform,
                    default_prediction,
                    ssss16_is_min,
                    $components,
                    $predict,
                )
            };
        }

        macro_rules! dispatch {
            ($predict:expr) => {
                match views.len() {
                    1 => run!([make(0)], $predict),
                    2 => run!([make(0), make(1)], $predict),
                    3 => run!([make(0), make(1), make(2)], $predict),
                    4 => run!([make(0), make(1), make(2), make(3)], $predict),
                    n => run!((0..n).map(make).collect::<Vec<_>>(), $predict),
                }
            };
        }

        match predictor {
            Predictor::NoPrediction => dispatch!(|_, _, _| 0),
            Predictor::Ra => dispatch!(|ra, _, _| ra),
            Predictor::Rb => dispatch!(|_, rb, _| rb),
            Predictor::Rc => dispatch!(|_, _, rc| rc),
            Predictor::RaRbRc1 => dispatch!(|ra, rb, rc| ra + rb - rc),
            Predictor::RaRbRc2 => dispatch!(|ra, rb, rc| ra + ((rb - rc) >> 1)),
            Predictor::RaRbRc3 => dispatch!(|ra, rb, rc| rb + ((ra - rc) >> 1)),
            Predictor::RaRb => dispatch!(|ra, rb, _| (ra + rb) / 2),
        }
    }

    #[cfg(feature = "rayon")]
    fn decode_lossless_scan_huffman_parallel(
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
        default_prediction: i32,
        ssss16_is_min: bool,
        restart_interval: usize,
        tables: &[HuffmanTable],
    ) -> bool {
        use rayon::prelude::*;

        if restart_interval == 0 || sw == 0 || !restart_interval.is_multiple_of(sw) {
            return false;
        }
        if sw.saturating_mul(sh) < PARALLEL_LOSSLESS_MIN_SAMPLES {
            return false;
        }
        let rows_per_segment = restart_interval / sw;

        let ncomp = views.len();
        if ncomp == 0 || !views.iter().enumerate().all(|(i, &(off, stride))| off == i && stride == ncomp) {
            return false;
        }
        if buf.len() < sw * sh * ncomp {
            return false;
        }

        let starts = &scan.restart_offsets;
        let expected = sh.div_ceil(rows_per_segment);
        if starts.len() < 2 || starts.len() != expected {
            return false;
        }

        let samples_per_segment = rows_per_segment * sw * ncomp;
        let data = scan.data.as_slice();

        buf[..sw * sh * ncomp].par_chunks_mut(samples_per_segment).enumerate().for_each(|(seg, out)| {
            let begin = starts[seg];
            let end = starts.get(seg + 1).copied().unwrap_or(data.len());
            let rows = out.len() / (sw * ncomp);
            if rows == 0 || begin >= end {
                return;
            }

            let mut reader = JpegBitReader::new(&data[begin..end]);
            let make = |i: usize| LosslessComponent {
                pos: i,
                stride: ncomp,
                row_stride: sw * ncomp,
                table: &tables[i],
                ra: 0,
                rb: 0,
            };

            macro_rules! run {
                ($components:expr, $predict:expr) => {
                    Self::lossless_huffman_loop(
                        &mut reader,
                        sw,
                        rows * sw,
                        out,
                        0,
                        point_transform,
                        default_prediction,
                        ssss16_is_min,
                        $components,
                        $predict,
                    )
                };
            }

            macro_rules! dispatch {
                ($predict:expr) => {
                    match ncomp {
                        1 => run!([make(0)], $predict),
                        2 => run!([make(0), make(1)], $predict),
                        3 => run!([make(0), make(1), make(2)], $predict),
                        4 => run!([make(0), make(1), make(2), make(3)], $predict),
                        n => run!((0..n).map(make).collect::<Vec<_>>(), $predict),
                    }
                };
            }

            match predictor {
                Predictor::NoPrediction => dispatch!(|_, _, _| 0),
                Predictor::Ra => dispatch!(|ra, _, _| ra),
                Predictor::Rb => dispatch!(|_, rb, _| rb),
                Predictor::Rc => dispatch!(|_, _, rc| rc),
                Predictor::RaRbRc1 => dispatch!(|ra, rb, rc| ra + rb - rc),
                Predictor::RaRbRc2 => dispatch!(|ra, rb, rc| ra + ((rb - rc) >> 1)),
                Predictor::RaRbRc3 => dispatch!(|ra, rb, rc| rb + ((ra - rc) >> 1)),
                Predictor::RaRb => dispatch!(|ra, rb, _| (ra + rb) / 2),
            }
        });

        true
    }

    fn lossless_huffman_loop<'t, C, F>(
        reader: &mut JpegBitReader<'_>,
        sw: usize,
        total: usize,
        buf: &mut [u16],
        restart_interval: usize,
        point_transform: u8,
        default_prediction: i32,
        ssss16_is_min: bool,
        mut components: C,
        predict: F,
    ) where
        C: AsMut<[LosslessComponent<'t>]>,
        F: Fn(i32, i32, i32) -> i32,
    {
        let components = components.as_mut();

        let interval = if restart_interval > 0 { restart_interval } else { total };
        let mut mcu = 0usize;

        while mcu < total {
            if reader.is_exhausted() {
                log_warn!("Lossless scan data exhausted after {} of {} samples", mcu, total);
                return;
            }
            if mcu > 0 {
                reader.clear_buffer();
            }
            let end = (mcu + interval).min(total);

            for c in components.iter_mut() {
                let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                let value = (((default_prediction + diff) & 0xFFFF) as u16) << point_transform;
                unsafe { *buf.get_unchecked_mut(c.pos) = value };
                c.pos += c.stride;
                c.ra = value as i32;
            }
            mcu += 1;

            let first_row_end = (((mcu - 1) / sw + 1) * sw).min(end);
            while mcu < first_row_end {
                for c in components.iter_mut() {
                    let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                    let value = (((c.ra + diff) & 0xFFFF) as u16) << point_transform;
                    unsafe { *buf.get_unchecked_mut(c.pos) = value };
                    c.pos += c.stride;
                    c.ra = value as i32;
                }
                mcu += 1;
            }

            while mcu < end {
                if reader.is_exhausted() {
                    log_warn!("Lossless scan data exhausted after {} of {} samples", mcu, total);
                    return;
                }
                let row_end = (mcu + sw).min(end);

                for c in components.iter_mut() {
                    let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                    let rb = unsafe { *buf.get_unchecked(c.pos - c.row_stride) } as i32;
                    let value = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                    unsafe { *buf.get_unchecked_mut(c.pos) = value };
                    c.pos += c.stride;
                    c.ra = value as i32;
                    c.rb = rb;
                }
                mcu += 1;

                while mcu < row_end {
                    for c in components.iter_mut() {
                        let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                        let rb = unsafe { *buf.get_unchecked(c.pos - c.row_stride) } as i32;
                        let value = (((predict(c.ra, rb, c.rb) + diff) & 0xFFFF) as u16) << point_transform;
                        unsafe { *buf.get_unchecked_mut(c.pos) = value };
                        c.pos += c.stride;
                        c.ra = value as i32;
                        c.rb = rb;
                    }
                    mcu += 1;
                }
            }
        }
    }

    #[inline(always)]
    fn decode_lossless_diff(reader: &mut JpegBitReader<'_>, table: &HuffmanTable, ssss16_is_min: bool) -> i32 {
        if !reader.ensure(32) {
            return Self::decode_lossless_diff_tail(reader, table, ssss16_is_min);
        }

        let window = reader.peek_unchecked(32);
        let entry = unsafe { *table.fast_lookup.get_unchecked((window >> (32 - HUFF_PEEK_BITS)) as usize) };
        let ssss = if entry >> 16 != 0 {
            let len = entry & 0xFF;
            let ssss = (entry >> 8) & 0xFF;
            if ssss.wrapping_sub(1) < 15 {
                reader.consume(len + ssss);
                let v = ((window >> (32 - len - ssss)) & ((1 << ssss) - 1)) as i32;
                return v + (((v >> (ssss - 1)) - 1) & ((-1i32 << ssss) + 1));
            }
            reader.consume(len);
            ssss
        } else {
            reader.consume(HUFF_PEEK_BITS as u32);
            let mut code = window >> (32 - HUFF_PEEK_BITS);
            let mut symbol = u32::MAX;
            for i in HUFF_PEEK_BITS..16 {
                code = (code << 1) | reader.read_bits_unchecked(1);
                let first = unsafe { *table.first_code.get_unchecked(i) };
                if first != u32::MAX {
                    let count =
                        unsafe { table.offsets.get_unchecked(i + 1).wrapping_sub(*table.offsets.get_unchecked(i)) };
                    if code >= first && code - first < count {
                        let idx = unsafe { *table.offsets.get_unchecked(i) } as usize + (code - first) as usize;
                        symbol = unsafe { *table.symbols.get_unchecked(idx) } as u32;
                        break;
                    }
                }
            }
            if symbol == u32::MAX {
                log_warn!("Invalid Huffman code: {}, replacing with 0", code);
                0
            } else {
                symbol
            }
        };

        match ssss {
            0 => 0,
            1..=15 => {
                let v = reader.read_bits(ssss) as i32;
                v + (((v >> (ssss - 1)) - 1) & ((-1i32 << ssss) + 1))
            }
            16 if ssss16_is_min => 32768,
            16 => {
                let v = reader.read_bits(16) as i32;
                v + (((v >> 15) - 1) & -65535)
            }
            _ => {
                log_warn!("Invalid difference: {}", ssss);
                0
            }
        }
    }

    #[cold]
    fn decode_lossless_diff_tail(reader: &mut JpegBitReader<'_>, table: &HuffmanTable, ssss16_is_min: bool) -> i32 {
        let ssss = get_next_symbol(reader, table);
        match ssss {
            0 => 0,
            1..=15 => {
                let v = reader.read_bits(ssss as u32) as i32;
                if v < (1 << (ssss - 1)) {
                    v + (-1i32 << ssss) + 1
                } else {
                    v
                }
            }
            16 if ssss16_is_min => 32768,
            16 => {
                let v = reader.read_bits(16) as i32;
                if v < (1 << 15) {
                    v - 65535
                } else {
                    v
                }
            }
            _ => {
                log_warn!("Invalid difference: {}", ssss);
                0
            }
        }
    }

    fn decode_lossless_scan_arithmetic(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
    ) {
        let num_components = scan.components.len();

        let l_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.length)
                    .unwrap_or(0)
            })
            .collect();

        let u_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(1)
            })
            .collect();

        let ctx_table_indices: Vec<usize> =
            scan.components.iter().map(|sc| (sc.dc_table_selector & 0x3) as usize).collect();

        let mut all_contexts = vec![[0u8; 160]; 4];
        let mut arith = ArithmeticDecoder::new(&scan.data);

        let mut da: Vec<i32> = vec![0i32; num_components];
        let mut db: Vec<Vec<i32>> = vec![vec![0i32; sw]; num_components];

        let restart_interval = self.restart_interval as usize;
        let mut mcus_until_restart = restart_interval;
        let mut interval_start_row = 0usize;

        let mut positions: Vec<usize> = views.iter().map(|&(offset, _)| offset).collect();
        let row_strides: Vec<usize> = views.iter().map(|&(_, stride)| sw * stride).collect();

        for y in 0..sh {
            if arith.is_exhausted() {
                log_warn!("Lossless arithmetic scan data exhausted at row {} of {}", y, sh);
                break;
            }

            for v in da.iter_mut() {
                *v = 0;
            }

            #[allow(clippy::needless_range_loop)]
            for x in 0..sw {
                let (at_interval_start, on_interval_first_line) = if restart_interval > 0 {
                    if mcus_until_restart == 0 {
                        mcus_until_restart = restart_interval;
                        interval_start_row = y;
                    }
                    let at_start = mcus_until_restart == restart_interval;
                    mcus_until_restart -= 1;
                    (at_start, y == interval_start_row)
                } else {
                    (x == 0 && y == 0, y == 0)
                };

                for (j, &(_, stride)) in views.iter().enumerate() {
                    let diff = arith.decode_lossless_difference(
                        &mut all_contexts[ctx_table_indices[j]],
                        da[j],
                        db[j][x],
                        l_values[j],
                        u_values[j],
                    );

                    da[j] = diff;
                    db[j][x] = diff;

                    Self::reconstruct_lossless_sample(
                        buf,
                        positions[j],
                        stride,
                        row_strides[j],
                        x,
                        y,
                        diff,
                        predictor,
                        point_transform,
                        self.precision,
                        at_interval_start,
                        on_interval_first_line,
                    );
                    positions[j] += stride;
                }
            }
        }
    }

    fn reconstruct_lossless_sample(
        buf: &mut [u16],
        pos: usize,
        stride: usize,
        row_stride: usize,
        x: usize,
        y: usize,
        diff: i32,
        predictor: Predictor,
        point_transform: u8,
        precision: u8,
        at_interval_start: bool,
        on_interval_first_line: bool,
    ) {
        let ra = if x > 0 { buf[pos - stride] as i32 } else { 0 };
        let rb = if y > 0 { buf[pos - row_stride] as i32 } else { 0 };
        let rc = if x > 0 && y > 0 { buf[pos - row_stride - stride] as i32 } else { 0 };

        let prediction = Self::predict(
            ra,
            rb,
            rc,
            predictor,
            point_transform,
            precision,
            at_interval_start,
            on_interval_first_line,
            x,
        );

        buf[pos] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
    }

    fn decode_arithmetic_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.is_empty() {
            log_warn!("No scans found in JPEG data");
            return Ok(());
        }

        let scan = &self.scans[0];
        let components = &scan.components;
        let dc_table_selectors: Vec<u8> = components.iter().map(|c| c.dc_table_selector).collect();
        let ac_table_selectors: Vec<u8> = components.iter().map(|c| c.ac_table_selector).collect();

        let get_dc_l = |tbl: usize| -> u8 {
            scan.arith_dc_tables
                .iter()
                .find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first())
                .map(|v| v.length)
                .unwrap_or(0)
        };
        let get_dc_u = |tbl: usize| -> u8 {
            scan.arith_dc_tables
                .iter()
                .find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first())
                .map(|v| v.value)
                .unwrap_or(1)
        };
        let get_ac_k = |tbl: usize| -> u8 {
            scan.arith_ac_tables
                .iter()
                .find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first())
                .map(|v| v.value)
                .unwrap_or(5)
        };

        let mut arith = ArithmeticDecoder::new(&scan.data);

        let num_components = planes.len();
        let mut dc_context = vec![0usize; num_components];
        let mut last_dc_val = vec![0i32; num_components];
        let mut dc_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 64]).collect();
        let mut ac_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 256]).collect();

        let is_non_interleaved = components.len() == 1;
        let max_h_samp = if is_non_interleaved {
            1
        } else {
            self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1)
        };
        let max_v_samp = if is_non_interleaved {
            1
        } else {
            self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1)
        };

        let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
        let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

        let mut restart_counter = self.restart_interval as u32;

        'mcus: for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                if arith.is_exhausted() {
                    log_warn!("Arithmetic scan data exhausted at MCU ({}, {})", mcu_x, mcu_y);
                    break 'mcus;
                }

                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        dc_context.fill(0);
                        last_dc_val.fill(0);
                        for s in dc_stats.iter_mut() {
                            s.fill(0);
                        }
                        for s in ac_stats.iter_mut() {
                            s.fill(0);
                        }
                        arith.reset();
                        restart_counter = self.restart_interval as u32;
                    }
                    restart_counter = restart_counter.saturating_sub(1);
                }

                for (comp_idx, comp) in self.components.iter().enumerate() {
                    if comp_idx >= components.len() {
                        continue;
                    }

                    let dc_sel = dc_table_selectors[comp_idx] as usize;
                    let ac_sel = ac_table_selectors[comp_idx] as usize;
                    let dc_l = get_dc_l(dc_sel);
                    let dc_u = get_dc_u(dc_sel);
                    let ac_k = get_ac_k(ac_sel);

                    let h_samp = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
                    let v_samp = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

                    for v in 0..v_samp {
                        for h in 0..h_samp {
                            let block_x = mcu_x * h_samp as u32 + h as u32;
                            let block_y = mcu_y * v_samp as u32 + v as u32;

                            if let Some(block) = planes[comp_idx].get_block_mut(block_x, block_y) {
                                arith.decode_mcu_sequential(
                                    block,
                                    comp_idx,
                                    dc_sel,
                                    ac_sel,
                                    dc_l,
                                    dc_u,
                                    ac_k,
                                    &mut dc_context,
                                    &mut last_dc_val,
                                    &mut dc_stats,
                                    &mut ac_stats,
                                    matches!(
                                        self.mode,
                                        JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn decode_progressive_scans_arithmetic(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut per_scan_dc_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 64]).collect();
        let mut per_scan_ac_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 256]).collect();
        let mut per_scan_dc_context = vec![0usize; planes.len()];
        let mut per_scan_last_dc_val = vec![0i32; planes.len()];
        let block_budget = Self::progressive_block_budget(planes);
        let mut blocks_visited = 0u64;

        for scan in &self.scans {
            if blocks_visited > block_budget {
                log_warn!("Progressive scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }

            let is_dc_scan = scan.start_spectral == 0;
            let is_first_scan = scan.successive_high == 0;
            let is_first_dc_scan = is_dc_scan && is_first_scan;

            for scan_comp in &scan.components {
                let (plane_idx, _comp) =
                    match self.components.iter().enumerate().find(|(_, c)| c.id == scan_comp.component_id) {
                        Some((i, c)) => (i, c.clone()),
                        None => continue,
                    };
                let dc_sel = scan_comp.dc_table_selector as usize;
                let ac_sel = scan_comp.ac_table_selector as usize;

                if is_first_dc_scan {
                    if dc_sel < per_scan_dc_stats.len() {
                        per_scan_dc_stats[dc_sel].fill(0);
                    }
                    if plane_idx < per_scan_last_dc_val.len() {
                        per_scan_last_dc_val[plane_idx] = 0;
                    }
                    if plane_idx < per_scan_dc_context.len() {
                        per_scan_dc_context[plane_idx] = 0;
                    }
                }
                if !is_dc_scan && ac_sel < per_scan_ac_stats.len() {
                    per_scan_ac_stats[ac_sel].fill(0);
                }
            }

            let get_dc_l = |tbl: usize| -> u8 {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first())
                    .map(|v| v.length)
                    .unwrap_or(0)
            };
            let get_dc_u = |tbl: usize| -> u8 {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(1)
            };
            let get_ac_k = |tbl: usize| -> u8 {
                scan.arith_ac_tables
                    .iter()
                    .find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(5)
            };

            let mut arith = ArithmeticDecoder::new(&scan.data);

            let is_non_interleaved = scan.components.len() == 1;
            let max_h_samp = if is_non_interleaved {
                1
            } else {
                self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1)
            };
            let max_v_samp = if is_non_interleaved {
                1
            } else {
                self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1)
            };

            let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
            let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

            let restart_interval = self.restart_interval;
            let mut restart_counter = restart_interval as u32;

            'mcus: for mcu_y in 0..mcu_height {
                for mcu_x in 0..mcu_width {
                    if arith.is_exhausted() {
                        log_warn!("Arithmetic scan data exhausted at MCU ({}, {})", mcu_x, mcu_y);
                        break 'mcus;
                    }

                    if restart_interval > 0 {
                        if restart_counter == 0 {
                            per_scan_dc_context.fill(0);
                            per_scan_last_dc_val.fill(0);
                            for s in per_scan_dc_stats.iter_mut() {
                                s.fill(0);
                            }
                            for s in per_scan_ac_stats.iter_mut() {
                                s.fill(0);
                            }
                            arith.reset();
                            restart_counter = restart_interval as u32;
                        }
                        restart_counter = restart_counter.saturating_sub(1);
                    }

                    for scan_comp in scan.components.iter() {
                        let (plane_index, comp) =
                            match self.components.iter().enumerate().find(|(_, c)| c.id == scan_comp.component_id) {
                                Some((i, c)) => (i, c.clone()),
                                None => continue,
                            };

                        let h_blocks = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
                        let v_blocks = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

                        if plane_index >= planes.len() {
                            continue;
                        }

                        let dc_sel = scan_comp.dc_table_selector as usize;
                        let ac_sel = scan_comp.ac_table_selector as usize;
                        let dc_l = get_dc_l(dc_sel);
                        let dc_u = get_dc_u(dc_sel);
                        let ac_k = get_ac_k(ac_sel);

                        for v in 0..v_blocks {
                            for h in 0..h_blocks {
                                let block_x = mcu_x * h_blocks as u32 + h as u32;
                                let block_y = mcu_y * v_blocks as u32 + v as u32;

                                let plane_blocks_per_line = planes[plane_index].blocks_per_line;
                                if block_x >= plane_blocks_per_line {
                                    continue;
                                }

                                let block_data_ptr = planes[plane_index].get_block_mut(block_x, block_y);
                                if let Some(block) = block_data_ptr {
                                    blocks_visited += 1;
                                    if is_dc_scan {
                                        if is_first_scan {
                                            arith.decode_dc_first(
                                                block,
                                                plane_index,
                                                dc_sel,
                                                dc_l,
                                                dc_u,
                                                scan.successive_low,
                                                &mut per_scan_dc_context,
                                                &mut per_scan_last_dc_val,
                                                &mut per_scan_dc_stats,
                                                matches!(
                                                    self.mode,
                                                    JpegMode::DifferentialProgressive
                                                        | JpegMode::DifferentialSequential
                                                ),
                                            );
                                        } else {
                                            arith.decode_dc_refine(block, scan.successive_low);
                                        }
                                    } else if is_first_scan {
                                        arith.decode_ac_first(
                                            block,
                                            ac_sel,
                                            ac_k,
                                            scan.start_spectral as usize,
                                            scan.end_spectral as usize,
                                            scan.successive_low,
                                            &mut per_scan_ac_stats,
                                        );
                                    } else {
                                        arith.decode_ac_refine(
                                            block,
                                            ac_sel,
                                            scan.start_spectral as usize,
                                            scan.end_spectral as usize,
                                            scan.successive_low,
                                            &mut per_scan_ac_stats,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // TODO: Use same optimization as progressive mode
    fn decode_huffman_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.is_empty() {
            // Well, nothing to do here, how did this even happen?
            log_warn!("No scans found in JPEG data");
            return Ok(());
        }

        let scan = &self.scans[0];
        let is_non_interleaved = scan.components.len() == 1;
        let mut reader = JpegBitReader::new(scan.data.as_slice());
        let mut previous_dc = vec![0i32; planes.len()];

        let mut max_h_samp = if is_non_interleaved {
            1
        } else {
            self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1)
        };
        let mut max_v_samp = if is_non_interleaved {
            1
        } else {
            self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1)
        };

        if max_h_samp == 0 || max_v_samp == 0 {
            log_warn!("Invalid sampling factors: ({}, {})", max_h_samp, max_v_samp);
            max_h_samp = 1;
            max_v_samp = 1;
        }

        let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
        let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

        let default_dc_table = Self::default_lossless_dc_table();
        let default_ac_table = Self::default_lossless_dc_table();

        struct BaselineCompInfo {
            h_samp: u8,
            v_samp: u8,
            dc_table: HuffmanTable,
            ac_table: HuffmanTable,
        }

        let comp_infos: Vec<BaselineCompInfo> = self
            .components
            .iter()
            .enumerate()
            .filter_map(|(comp_idx, comp)| {
                if self.scans[0].components.len() <= comp_idx {
                    log_warn!("Component index out of bounds: {} {}", self.scans[0].components.len(), comp_idx);
                    return None;
                }

                let dc_selector = self.scans[0].components[comp_idx].dc_table_selector;
                let ac_selector = self.scans[0].components[comp_idx].ac_table_selector;

                let dc_table =
                    self.scans[0].dc_tables.iter().find(|t| t.id == dc_selector).cloned().unwrap_or_else(|| {
                        log_warn!(
                            "DC table {} not found in baseline mode, substituting default, image will be corrupted.",
                            dc_selector
                        );
                        default_dc_table.clone()
                    });

                let ac_table =
                    self.scans[0].ac_tables.iter().find(|t| t.id == ac_selector).cloned().unwrap_or_else(|| {
                        log_warn!(
                            "AC table {} not found in baseline mode, substituting default, image will be corrupted.",
                            ac_selector
                        );
                        default_ac_table.clone()
                    });

                let h_samp = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
                let v_samp = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

                Some(BaselineCompInfo { h_samp, v_samp, dc_table, ac_table })
            })
            .collect();

        let mut restart_counter = self.restart_interval as u32;
        let is_differential = matches!(self.mode, JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive);
        let mut padding_blocks: Option<Vec<([i16; 64], i32, bool)>> = None;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                if padding_blocks.is_none() && reader.is_exhausted() {
                    log_warn!("Scan data exhausted at MCU ({}, {}), padding the rest of the image", mcu_x, mcu_y);
                    padding_blocks = Some(
                        comp_infos
                            .iter()
                            .map(|info| {
                                let dc_written = get_next_symbol(&mut JpegBitReader::new(&[]), &info.dc_table) <= 15;
                                let mut block = [0i16; 64];
                                let mut dc_difference = 0;
                                let _ = self.decode_mcu(
                                    &mut JpegBitReader::new(&[]),
                                    &mut block,
                                    &info.dc_table,
                                    &info.ac_table,
                                    &mut dc_difference,
                                    is_differential,
                                );
                                (block, dc_difference, dc_written)
                            })
                            .collect(),
                    );
                }

                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        previous_dc.fill(0);
                        reader.clear_buffer();
                        restart_counter = self.restart_interval as u32;
                    }

                    restart_counter = restart_counter.saturating_sub(1);
                }

                for (comp_idx, info) in comp_infos.iter().enumerate() {
                    for v in 0..info.v_samp {
                        for h in 0..info.h_samp {
                            let block_x = mcu_x * info.h_samp as u32 + h as u32;
                            let block_y = mcu_y * info.v_samp as u32 + v as u32;

                            if comp_idx >= previous_dc.len() {
                                log_warn!(
                                    "Component is larger than previous DC buffer: {} {}",
                                    comp_idx,
                                    previous_dc.len()
                                );
                                continue;
                            }

                            if let Some(block) = planes[comp_idx].get_block_mut(block_x, block_y) {
                                if let Some((padding, dc_difference, dc_written)) =
                                    padding_blocks.as_ref().and_then(|blocks| blocks.get(comp_idx))
                                {
                                    *block = *padding;
                                    if *dc_written {
                                        let dc = if is_differential {
                                            *dc_difference
                                        } else {
                                            dc_difference.wrapping_add(previous_dc[comp_idx])
                                        };
                                        block[0] = dc as i16;
                                        previous_dc[comp_idx] = dc;
                                    }
                                    continue;
                                }

                                match self.decode_mcu(
                                    &mut reader,
                                    block,
                                    &info.dc_table,
                                    &info.ac_table,
                                    &mut previous_dc[comp_idx],
                                    is_differential,
                                ) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        log_warn!("Failed to decode MCU: {}", e);
                                    }
                                };
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn dequantize_and_idct_planes(&self, planes: &[ComponentPlane], zigzag_order: bool) -> Vec<RasterPlane> {
        let level_shift = if self.precision <= 8 { 128i32 } else { 2048i32 };

        let default_table = QuantizationTable { id: 0, table_natural: DEFAULT_QUANTIZATION_TABLE.to_vec() };

        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

        planes
            .iter()
            .enumerate()
            .map(|(comp_idx, plane)| {
                let comp = self.components.get(comp_idx);
                let quant_data: &[u16] = comp
                    .and_then(|comp| self.quantization_tables.iter().find(|q| q.id == comp.quantization_table_id))
                    .map(|t| t.table_natural.as_slice())
                    .unwrap_or_else(|| {
                        log_warn!("Quantization table not found for component, substituting default one.");
                        default_table.table_natural.as_slice()
                    });

                let (sw, sh) = comp
                    .map(|c| {
                        (
                            (self.width * c.horizontal_sampling_factor as u32).div_ceil(max_h_samp as u32) as usize,
                            (self.height * c.vertical_sampling_factor as u32).div_ceil(max_v_samp as u32) as usize,
                        )
                    })
                    .unwrap_or((self.width as usize, self.height as usize));

                let mut data = vec![0i16; sw * sh];
                if sw == 0 || sh == 0 {
                    return RasterPlane { data, width: sw, height: sh };
                }

                let bpl = plane.blocks_per_line as usize;
                let nbx = sw.div_ceil(8);
                let scratch_len = nbx * 64;
                let row_len = scratch_len.min(bpl * 64);

                let process_band = |scratch: &mut [i16], by: usize, band: &mut [i16]| {
                    let row_start = by * bpl * 64;
                    let src = plane.data.get(row_start..).unwrap_or(&[]);
                    let avail = src.len().min(row_len);
                    scratch[..avail].copy_from_slice(&src[..avail]);
                    scratch[avail..].fill(0);

                    if zigzag_order {
                        dezigzag_blocks(scratch);
                    }
                    dequantize_and_idct(scratch, quant_data, level_shift);

                    let band_h = band.len() / sw;
                    for bx in 0..nbx {
                        let bw = (sw - bx * 8).min(8);
                        for py in 0..band_h {
                            let src_off = bx * 64 + py * 8;
                            let dst_off = py * sw + bx * 8;
                            band[dst_off..dst_off + bw].copy_from_slice(&scratch[src_off..src_off + bw]);
                        }
                    }
                };

                #[cfg(feature = "rayon")]
                {
                    use rayon::prelude::*;
                    data.par_chunks_mut(sw * 8).enumerate().for_each_init(
                        || vec![0i16; scratch_len],
                        |scratch, (by, band)| process_band(scratch, by, band),
                    );
                }

                #[cfg(not(feature = "rayon"))]
                {
                    let mut scratch = vec![0i16; scratch_len];
                    for (by, band) in data.chunks_mut(sw * 8).enumerate() {
                        process_band(&mut scratch, by, band);
                    }
                }

                RasterPlane { data, width: sw, height: sh }
            })
            .collect()
    }

    fn convert_4component_to_rgba(
        &self,
        planes: &[RasterPlane],
        source_dims: &[(usize, usize)],
        tw: usize,
        th: usize,
        npixels: usize,
    ) -> VexelResult<PixelData> {
        let ch: [&[i16]; 4] = [0, 1, 2, 3].map(|i| planes.get(i).map(|p| p.data.as_slice()).unwrap_or(&[]));

        let upsample = |plane: &[i16], dims: (usize, usize), dx: usize, dy: usize| -> i32 {
            let (sw, sh) = dims;
            if sw == tw && sh == th {
                return plane.get(dy * tw + dx).copied().unwrap_or(0) as i32;
            }
            let sx = (dx * sw / tw).min(sw.saturating_sub(1));
            let sy = (dy * sh / th).min(sh.saturating_sub(1));
            plane.get(sy * sw + sx).copied().unwrap_or(0) as i32
        };

        if self.precision <= 8 {
            let mut pixels = vec![0u8; npixels * 4];
            for dy in 0..th {
                for dx in 0..tw {
                    let idx = (dy * tw + dx) * 4;
                    for (c, plane) in ch.iter().enumerate() {
                        let dims = source_dims.get(c).copied().unwrap_or((tw, th));
                        pixels[idx + c] = (upsample(plane, dims, dx, dy) + 128).clamp(0, 255) as u8;
                    }
                }
            }
            Ok(PixelData::RGBA8(pixels))
        } else {
            let mut pixels = vec![0u16; npixels * 4];
            let mid = 1 << (self.precision - 1);
            let max_val = (1 << self.precision) - 1;
            let shift = 16 - self.precision;
            for dy in 0..th {
                for dx in 0..tw {
                    let idx = (dy * tw + dx) * 4;
                    for (c, plane) in ch.iter().enumerate() {
                        let dims = source_dims.get(c).copied().unwrap_or((tw, th));
                        pixels[idx + c] = ((upsample(plane, dims, dx, dy) + mid).clamp(0, max_val) as u16) << shift;
                    }
                }
            }
            Ok(PixelData::RGBA16(pixels))
        }
    }

    fn convert_4component_to_rgb(
        &self,
        planes: &[RasterPlane],
        source_dims: &[(usize, usize)],
        tw: usize,
        th: usize,
        npixels: usize,
        colorspace: JpegColorspace,
    ) -> VexelResult<PixelData> {
        let ch0: &[i16] = planes.first().map(|p| p.data.as_slice()).unwrap_or(&[]);
        let ch1: &[i16] = planes.get(1).map(|p| p.data.as_slice()).unwrap_or(&[]);
        let ch2: &[i16] = planes.get(2).map(|p| p.data.as_slice()).unwrap_or(&[]);
        let ch3: &[i16] = planes.get(3).map(|p| p.data.as_slice()).unwrap_or(&[]);

        let (y0_sw, y0_sh) = source_dims.first().copied().unwrap_or((tw, th));

        let upsample_chroma = |plane: &[i16], dims: (usize, usize), dx: usize, dy: usize| -> i32 {
            let (sw, sh) = dims;
            if sw == tw && sh == th {
                return plane.get(dy * tw + dx).copied().unwrap_or(0) as i32;
            }
            let h2x = tw == sw * 2 || tw == sw * 2 - 1;
            let v2x = th == sh * 2 || th == sh * 2 - 1;
            if h2x && v2x {
                let sy = (dy / 2).min(sh.saturating_sub(1));
                let sx = dx / 2;
                plane.get(sy * sw + sx.min(sw.saturating_sub(1))).copied().unwrap_or(0) as i32
            } else if h2x {
                let sx = dx / 2;
                plane.get(dy * sw + sx.min(sw.saturating_sub(1))).copied().unwrap_or(0) as i32
            } else if v2x {
                let sy = (dy / 2).min(sh.saturating_sub(1));
                plane.get(sy * sw + dx.min(sw.saturating_sub(1))).copied().unwrap_or(0) as i32
            } else {
                let sx = (dx * sw / tw).min(sw.saturating_sub(1));
                let sy = (dy * sh / th).min(sh.saturating_sub(1));
                plane.get(sy * sw + sx).copied().unwrap_or(0) as i32
            }
        };

        let mut pixels = vec![0u8; npixels * 3];

        for dy in 0..th {
            for dx in 0..tw {
                let y0_val = if y0_sw == tw && y0_sh == th {
                    ch0.get(dy * tw + dx).copied().unwrap_or(0) as i32
                } else {
                    let sx = (dx * y0_sw / tw).min(y0_sw.saturating_sub(1));
                    let sy = (dy * y0_sh / th).min(y0_sh.saturating_sub(1));
                    ch0.get(sy * y0_sw + sx).copied().unwrap_or(0) as i32
                };
                let c1_val = upsample_chroma(ch1, source_dims.get(1).copied().unwrap_or((tw, th)), dx, dy);
                let c2_val = upsample_chroma(ch2, source_dims.get(2).copied().unwrap_or((tw, th)), dx, dy);
                let k_val = upsample_chroma(ch3, source_dims.get(3).copied().unwrap_or((tw, th)), dx, dy);

                let k_inv = (k_val + 128).clamp(0, 255);

                let (c_inv, m_inv, y_inv) = if colorspace == JpegColorspace::YCCK {
                    let cb = c1_val;
                    let cr = c2_val;
                    let y_luma = (y0_val + 128) << 16;
                    let r_inv = (255 << 16) - y_luma - 91881 * cr;
                    let g_inv = (255 << 16) - y_luma + 22554 * cb + 46802 * cr;
                    let b_inv = (255 << 16) - y_luma - 116130 * cb;
                    (
                        ((r_inv + 32768) >> 16).clamp(0, 255),
                        ((g_inv + 32768) >> 16).clamp(0, 255),
                        ((b_inv + 32768) >> 16).clamp(0, 255),
                    )
                } else {
                    ((y0_val + 128).clamp(0, 255), (c1_val + 128).clamp(0, 255), (c2_val + 128).clamp(0, 255))
                };

                let idx = dy * tw + dx;
                let r_out = ((c_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                let g_out = ((m_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                let b_out = ((y_inv * k_inv + 127) / 255).clamp(0, 255) as u8;

                pixels[idx * 3] = r_out;
                pixels[idx * 3 + 1] = g_out;
                pixels[idx * 3 + 2] = b_out;
            }
        }

        Ok(PixelData::RGB8(pixels))
    }

    fn upsample_and_convert(&self, planes: &[RasterPlane]) -> VexelResult<PixelData> {
        use crate::decoders::jpeg::upsample as up;

        let tw = self.width as usize;
        let th = self.height as usize;
        let npixels = tw * th;

        let source_dims: Vec<(usize, usize)> = planes.iter().map(|p| (p.width, p.height)).collect();

        if planes.len() == 1 {
            let src = &planes[0].data;
            return if self.precision <= 8 {
                let mut pixels = Vec::with_capacity(npixels);
                for &s in src.iter().take(npixels) {
                    pixels.push((s as i32 + 128).clamp(0, 255) as u8);
                }
                pixels.resize(npixels, 0);
                Ok(PixelData::L8(pixels))
            } else {
                let mut pixels16 = Vec::with_capacity(npixels);
                for &s in src.iter().take(npixels) {
                    pixels16.push(((s as i32 + 2048).clamp(0, 4095) as u16) << 4);
                }
                pixels16.resize(npixels, 0);
                Ok(PixelData::L16(pixels16))
            };
        }

        let colorspace = self.detect_colorspace();

        if colorspace == JpegColorspace::CMYK || colorspace == JpegColorspace::YCCK {
            return self.convert_4component_to_rgb(planes, &source_dims, tw, th, npixels, colorspace);
        }

        if colorspace == JpegColorspace::RGBA {
            return self.convert_4component_to_rgba(planes, &source_dims, tw, th, npixels);
        }

        if planes.len() < 3 {
            log_warn!("Invalid number of planes for RGB conversion: {}.", planes.len());
        }

        let (y_sw, y_sh) = source_dims[0];
        let (cb_sw, cb_sh) = if planes.len() >= 2 { source_dims[1] } else { (tw, th) };
        let (cr_sw, cr_sh) = if planes.len() >= 3 { source_dims[2] } else { (cb_sw, cb_sh) };

        let y_plane: &[i16] = &planes[0].data;
        let cb_plane: &[i16] = if planes.len() > 1 { &planes[1].data } else { &[] };
        let cr_plane: &[i16] = if planes.len() > 2 { &planes[2].data } else { &[] };

        let max_sw = y_sw.max(cb_sw).max(cr_sw).max(1);

        #[cfg(not(feature = "rayon"))]
        let upsample_y_row = |dy: usize, tmp: &mut Vec<i32>, col_sums: &mut Vec<i32>, prev_col_sums: &mut Vec<i32>| {
            tmp.resize(tw, 0);
            if y_sw == tw && y_sh == th {
                let base = dy * tw;
                let copy_len = tw.min(y_plane.len().saturating_sub(base));
                up::widen_i16_row(&y_plane[base..base + copy_len], &mut tmp[..copy_len]);
                tmp[copy_len..].fill(0);
            } else {
                let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                let sy_above = sy.saturating_sub(1);
                let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                let v = dy % 2;
                let sy_near = if v == 0 { sy_above } else { sy_below };
                let prev_near = if v == 0 { sy_above } else { sy };
                let ylen = y_plane.len();
                let src_start = (sy * y_sw).min(ylen);
                let near_start = (sy_near * y_sw).min(ylen);
                let prev_start = (prev_near * y_sw).min(ylen);
                let src_row = &y_plane[src_start..(src_start + y_sw).min(ylen)];
                let near_row = &y_plane[near_start..(near_start + y_sw).min(ylen)];
                let prev_near_row = &y_plane[prev_start..(prev_start + y_sw).min(ylen)];
                col_sums[..y_sw].fill(0);
                prev_col_sums[..y_sw].fill(0);
                up::compute_col_sums(src_row, near_row, &mut col_sums[..], y_sw.min(src_row.len()).min(near_row.len()));
                up::compute_col_sums(
                    src_row,
                    prev_near_row,
                    &mut prev_col_sums[..],
                    y_sw.min(src_row.len()).min(prev_near_row.len()),
                );
                up::upsample_h2v2_row(&col_sums[..], &prev_col_sums[..], tmp, y_sw, tw);
            }
        };

        macro_rules! upsample_chroma_row {
            ($plane:expr, $p_sw:expr, $p_sh:expr, $dy:expr, $tmp:expr, $col_sums:expr, $prev_col_sums:expr) => {{
                let p_sw = $p_sw;
                let p_sh = $p_sh;
                let p_h_2x = tw == p_sw * 2 || tw == (p_sw * 2).saturating_sub(1);
                let p_v_2x = th == p_sh * 2 || th == (p_sh * 2).saturating_sub(1);
                let p_is_h2v1 = p_h_2x && th == p_sh;
                let p_is_h1v2 = !p_h_2x && p_v_2x && tw == p_sw;
                let p_is_h2v2 = p_h_2x && p_v_2x;
                let p_is_identity = p_sw == tw && p_sh == th;
                $tmp.resize(tw, 0);
                if $plane.is_empty() {
                } else if p_is_identity {
                    let base = $dy * tw;
                    let avail = $plane.len().saturating_sub(base);
                    let copy_len = tw.min(avail);
                    up::widen_i16_row(&$plane[base..base + copy_len], &mut $tmp[..copy_len]);
                    $tmp[copy_len..].fill(0);
                } else if p_is_h2v2 {
                    let sy = ($dy / 2).min(p_sh.saturating_sub(1));
                    let v = $dy % 2;
                    let sy_above = sy.saturating_sub(1);
                    let sy_below = (sy + 1).min(p_sh.saturating_sub(1));
                    let sy_near = if v == 0 { sy_above } else { sy_below };
                    let prev_near = if v == 0 { sy_above } else { sy };
                    let plen = $plane.len();
                    let src_start = (sy * p_sw).min(plen);
                    let near_start = (sy_near * p_sw).min(plen);
                    let prev_start = (prev_near * p_sw).min(plen);
                    let src_row = &$plane[src_start..(src_start + p_sw).min(plen)];
                    let near_row = &$plane[near_start..(near_start + p_sw).min(plen)];
                    let prev_near_row = &$plane[prev_start..(prev_start + p_sw).min(plen)];
                    let sw_eff = p_sw.min(src_row.len()).min(near_row.len());
                    $col_sums[..p_sw].fill(0);
                    $prev_col_sums[..p_sw].fill(0);
                    up::compute_col_sums(src_row, near_row, &mut $col_sums[..], sw_eff);
                    up::compute_col_sums(
                        src_row,
                        prev_near_row,
                        &mut $prev_col_sums[..],
                        sw_eff.min(prev_near_row.len()),
                    );
                    up::upsample_h2v2_row(&$col_sums[..], &$prev_col_sums[..], &mut $tmp[..], p_sw, tw);
                } else if p_is_h2v1 {
                    let sy = $dy.min(p_sh.saturating_sub(1));
                    let plen = $plane.len();
                    let src_start = (sy * p_sw).min(plen);
                    let src_row = &$plane[src_start..(src_start + p_sw).min(plen)];
                    up::upsample_h2v1_row(src_row, &mut $tmp[..], p_sw.min(src_row.len()), tw);
                } else if p_is_h1v2 {
                    let sy = ($dy / 2).min(p_sh.saturating_sub(1));
                    let v = $dy % 2;
                    let sy_near = if v == 0 { sy.saturating_sub(1) } else { (sy + 1).min(p_sh.saturating_sub(1)) };
                    let bias = if v == 0 { 1i32 } else { 2i32 };
                    let plen = $plane.len();
                    let src_start = (sy * p_sw).min(plen);
                    let near_start = (sy_near * p_sw).min(plen);
                    let src_row = &$plane[src_start..(src_start + p_sw).min(plen)];
                    let neighbor_row = &$plane[near_start..(near_start + p_sw).min(plen)];
                    let sw_eff = p_sw.min(src_row.len()).min(neighbor_row.len()).min($tmp.len());
                    up::upsample_h1v2_row(src_row, neighbor_row, bias, &mut $tmp[..sw_eff.min(tw)], sw_eff);
                } else {
                    for x in 0..tw {
                        if p_sw == 0 || p_sh == 0 {
                            $tmp[x] = 0;
                            continue;
                        }
                        let fx = (x as f32 + 0.5) * p_sw as f32 / tw as f32 - 0.5;
                        let fy = ($dy as f32 + 0.5) * p_sh as f32 / th as f32 - 0.5;
                        let x0 = (fx.floor() as i64).clamp(0, p_sw as i64 - 1) as usize;
                        let y0 = (fy.floor() as i64).clamp(0, p_sh as i64 - 1) as usize;
                        let x1 = (x0 + 1).min(p_sw.saturating_sub(1));
                        let y1 = (y0 + 1).min(p_sh.saturating_sub(1));
                        let wx = fx - fx.floor();
                        let wy = fy - fy.floor();
                        let get = |px: usize, py: usize| -> i32 {
                            let idx = py * p_sw + px;
                            if idx < $plane.len() {
                                $plane[idx] as i32
                            } else {
                                0
                            }
                        };
                        let v = (1.0 - wy) * ((1.0 - wx) * get(x0, y0) as f32 + wx * get(x1, y0) as f32)
                            + wy * ((1.0 - wx) * get(x0, y1) as f32 + wx * get(x1, y1) as f32);
                        $tmp[x] = v.round() as i32;
                    }
                }
            }};
        }

        let convert_ycbcr = colorspace == JpegColorspace::YCbCr;

        macro_rules! fill_row_8 {
            ($dy:expr, $y_row:expr, $cb_row:expr, $cr_row:expr, $dst:expr, $col_sums:expr, $prev_col_sums:expr) => {{
                upsample_chroma_row!(cb_plane, cb_sw, cb_sh, $dy, $cb_row, $col_sums, $prev_col_sums);
                upsample_chroma_row!(cr_plane, cr_sw, cr_sh, $dy, $cr_row, $col_sums, $prev_col_sums);
                for col in 0..tw {
                    let c0 = $y_row.get(col).copied().unwrap_or(0);
                    let c1 = $cb_row.get(col).copied().unwrap_or(0);
                    let c2 = $cr_row.get(col).copied().unwrap_or(0);
                    if convert_ycbcr {
                        let y128 = (c0 + 128) << 16;
                        $dst[col * 3] = ((y128 + 91881 * c2 + 32768) >> 16).clamp(0, 255) as u8;
                        $dst[col * 3 + 1] = ((y128 - 22554 * c1 - 46802 * c2 + 32768) >> 16).clamp(0, 255) as u8;
                        $dst[col * 3 + 2] = ((y128 + 116130 * c1 + 32768) >> 16).clamp(0, 255) as u8;
                    } else {
                        $dst[col * 3] = (c0 + 128).clamp(0, 255) as u8;
                        $dst[col * 3 + 1] = (c1 + 128).clamp(0, 255) as u8;
                        $dst[col * 3 + 2] = (c2 + 128).clamp(0, 255) as u8;
                    }
                }
            }};
        }

        macro_rules! fill_row_16 {
            ($dy:expr, $y_row:expr, $cb_row:expr, $cr_row:expr, $dst:expr, $col_sums:expr, $prev_col_sums:expr) => {{
                upsample_chroma_row!(cb_plane, cb_sw, cb_sh, $dy, $cb_row, $col_sums, $prev_col_sums);
                upsample_chroma_row!(cr_plane, cr_sw, cr_sh, $dy, $cr_row, $col_sums, $prev_col_sums);
                for col in 0..tw {
                    let c0 = $y_row.get(col).copied().unwrap_or(0);
                    let c1 = $cb_row.get(col).copied().unwrap_or(0);
                    let c2 = $cr_row.get(col).copied().unwrap_or(0);
                    if convert_ycbcr {
                        let y2048 = (c0 + 2048) << 16;
                        $dst[col * 3] = (((y2048 + 91881 * c2 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        $dst[col * 3 + 1] =
                            (((y2048 - 22554 * c1 - 46802 * c2 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        $dst[col * 3 + 2] = (((y2048 + 116130 * c1 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                    } else {
                        $dst[col * 3] = ((c0 + 2048).clamp(0, 65535) as u16) << 4;
                        $dst[col * 3 + 1] = ((c1 + 2048).clamp(0, 65535) as u16) << 4;
                        $dst[col * 3 + 2] = ((c2 + 2048).clamp(0, 65535) as u16) << 4;
                    }
                }
            }};
        }

        if self.precision <= 8 {
            let mut pixels = vec![0u8; npixels * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                pixels.par_chunks_mut(tw * 3).enumerate().for_each_init(
                    || (vec![0i32; tw], vec![0i32; tw], vec![0i32; tw], vec![0i32; max_sw], vec![0i32; max_sw]),
                    |(y_row, cb_row, cr_row, col_sums, prev_col_sums), (dy, dst)| {
                        if y_sw == tw && y_sh == th {
                            let base = dy * tw;
                            let copy_len = tw.min(y_plane.len().saturating_sub(base));
                            up::widen_i16_row(&y_plane[base..base + copy_len], &mut y_row[..copy_len]);
                            y_row[copy_len..].fill(0);
                        } else {
                            let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                            let v = dy % 2;
                            let sy_above = sy.saturating_sub(1);
                            let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                            let sy_near = if v == 0 { sy_above } else { sy_below };
                            let prev_near = if v == 0 { sy_above } else { sy };
                            let ylen = y_plane.len();
                            let src_start = (sy * y_sw).min(ylen);
                            let near_start = (sy_near * y_sw).min(ylen);
                            let prev_start = (prev_near * y_sw).min(ylen);
                            let src_row = &y_plane[src_start..(src_start + y_sw).min(ylen)];
                            let near_row = &y_plane[near_start..(near_start + y_sw).min(ylen)];
                            let prev_near_row = &y_plane[prev_start..(prev_start + y_sw).min(ylen)];
                            let sw_eff = y_sw.min(src_row.len()).min(near_row.len());
                            col_sums[..y_sw].fill(0);
                            prev_col_sums[..y_sw].fill(0);
                            up::compute_col_sums(src_row, near_row, &mut col_sums[..], sw_eff);
                            up::compute_col_sums(
                                src_row,
                                prev_near_row,
                                &mut prev_col_sums[..],
                                sw_eff.min(prev_near_row.len()),
                            );
                            up::upsample_h2v2_row(&col_sums[..], &prev_col_sums[..], &mut y_row[..], y_sw, tw);
                        }

                        fill_row_8!(dy, y_row, cb_row, cr_row, dst, col_sums, prev_col_sums);
                    },
                );
            }

            #[cfg(not(feature = "rayon"))]
            {
                let mut y_row = vec![0i32; tw];
                let mut cb_row = vec![0i32; tw];
                let mut cr_row = vec![0i32; tw];
                let mut col_sums = vec![0i32; max_sw];
                let mut prev_col_sums = vec![0i32; max_sw];
                for dy in 0..th {
                    upsample_y_row(dy, &mut y_row, &mut col_sums, &mut prev_col_sums);
                    let base = dy * tw * 3;
                    fill_row_8!(dy, y_row, cb_row, cr_row, pixels[base..base + tw * 3], col_sums, prev_col_sums);
                }
            }

            Ok(PixelData::RGB8(pixels))
        } else {
            let mut pixels16 = vec![0u16; npixels * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                pixels16.par_chunks_mut(tw * 3).enumerate().for_each_init(
                    || (vec![0i32; tw], vec![0i32; tw], vec![0i32; tw], vec![0i32; max_sw], vec![0i32; max_sw]),
                    |(y_row, cb_row, cr_row, col_sums, prev_col_sums), (dy, dst)| {
                        if y_sw == tw && y_sh == th {
                            let base = dy * tw;
                            let copy_len = tw.min(y_plane.len().saturating_sub(base));
                            up::widen_i16_row(&y_plane[base..base + copy_len], &mut y_row[..copy_len]);
                            y_row[copy_len..].fill(0);
                        } else {
                            let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                            let v = dy % 2;
                            let sy_above = sy.saturating_sub(1);
                            let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                            let sy_near = if v == 0 { sy_above } else { sy_below };
                            let prev_near = if v == 0 { sy_above } else { sy };
                            let ylen = y_plane.len();
                            let src_start = (sy * y_sw).min(ylen);
                            let near_start = (sy_near * y_sw).min(ylen);
                            let prev_start = (prev_near * y_sw).min(ylen);
                            let src_row = &y_plane[src_start..(src_start + y_sw).min(ylen)];
                            let near_row = &y_plane[near_start..(near_start + y_sw).min(ylen)];
                            let prev_near_row = &y_plane[prev_start..(prev_start + y_sw).min(ylen)];
                            let sw_eff = y_sw.min(src_row.len()).min(near_row.len());
                            col_sums[..y_sw].fill(0);
                            prev_col_sums[..y_sw].fill(0);
                            up::compute_col_sums(src_row, near_row, &mut col_sums[..], sw_eff);
                            up::compute_col_sums(
                                src_row,
                                prev_near_row,
                                &mut prev_col_sums[..],
                                sw_eff.min(prev_near_row.len()),
                            );
                            up::upsample_h2v2_row(&col_sums[..], &prev_col_sums[..], &mut y_row[..], y_sw, tw);
                        }

                        fill_row_16!(dy, y_row, cb_row, cr_row, dst, col_sums, prev_col_sums);
                    },
                );
            }

            #[cfg(not(feature = "rayon"))]
            {
                let mut y_row = vec![0i32; tw];
                let mut cb_row = vec![0i32; tw];
                let mut cr_row = vec![0i32; tw];
                let mut col_sums = vec![0i32; max_sw];
                let mut prev_col_sums = vec![0i32; max_sw];
                for dy in 0..th {
                    upsample_y_row(dy, &mut y_row, &mut col_sums, &mut prev_col_sums);
                    let base = dy * tw * 3;
                    fill_row_16!(dy, y_row, cb_row, cr_row, pixels16[base..base + tw * 3], col_sums, prev_col_sums);
                }
            }

            Ok(PixelData::RGB16(pixels16))
        }
    }

    fn decode_baseline(&mut self) -> VexelResult<Image> {
        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

        let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
        let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

        let components_snapshot: Vec<_> =
            self.components.iter().map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor)).collect();
        let mut component_planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
        for (h, v) in &components_snapshot {
            let comp_width = mcu_width * 8 * *h as u32;
            let comp_height = mcu_height * 8 * *v as u32;
            component_planes.push(ComponentPlane::try_new(comp_width, comp_height, &mut self.limits)?);
        }

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut component_planes)?,
        }

        let raster_planes = self.dequantize_and_idct_planes(&component_planes, false);
        for plane in &component_planes {
            plane.free(&mut self.limits);
        }
        drop(component_planes);

        let mut pixel_data = self.upsample_and_convert(&raster_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_frame_to_pixels(&mut self, frame: &HierarchicalFrame) -> VexelResult<Vec<Vec<i32>>> {
        let saved_width = self.width;
        let saved_height = self.height;
        let saved_mode = self.mode.clone();
        let saved_coding = self.coding_method.clone();
        let saved_precision = self.precision;
        let saved_components = self.components.clone();
        let saved_quantization = self.quantization_tables.clone();
        let saved_scans = std::mem::take(&mut self.scans);
        let saved_restart = self.restart_interval;

        self.width = frame.width;
        self.height = frame.height;
        self.mode = frame.mode.clone();
        self.coding_method = frame.coding_method.clone();
        self.precision = frame.precision;
        self.components = frame.components.clone();
        self.quantization_tables = frame.quantization_tables.clone();
        self.scans = frame.scans.clone();

        let result = self.decode_frame_pixels_internal();

        self.width = saved_width;
        self.height = saved_height;
        self.mode = saved_mode;
        self.coding_method = saved_coding;
        self.precision = saved_precision;
        self.components = saved_components;
        self.quantization_tables = saved_quantization;
        self.scans = saved_scans;
        self.restart_interval = saved_restart;

        result
    }

    fn decode_frame_pixels_internal(&mut self) -> VexelResult<Vec<Vec<i32>>> {
        let w = self.width as usize;
        let h = self.height as usize;
        let nc = self.components.len();

        if w == 0 || h == 0 {
            log_warn!("Hierarchical frame has dimensions {}x{}, skipping it", w, h);
            return Ok(vec![Vec::new(); nc]);
        }

        match self.mode.clone() {
            JpegMode::DifferentialLossless => {
                let scan = match self.scans.first() {
                    Some(s) => s.clone(),
                    None => return Ok(vec![vec![0i32; w * h]; nc]),
                };

                let differences = if self.coding_method == JpegCodingMethod::Arithmetic {
                    self.decode_differences_arithmetic(&scan)?
                } else {
                    self.decode_differences(&scan)?
                };

                Ok(differences)
            }

            JpegMode::Lossless => {
                let scan = match self.scans.first() {
                    Some(s) => s.clone(),
                    None => return Ok(vec![vec![0i32; w * h]; nc]),
                };

                let differences = if self.coding_method == JpegCodingMethod::Arithmetic {
                    self.decode_differences_arithmetic(&scan)?
                } else {
                    self.decode_differences(&scan)?
                };

                let point_transform = self.successive_approximation_low;
                let predictor = match scan.start_spectral {
                    0 => Predictor::NoPrediction,
                    1 => Predictor::Ra,
                    2 => Predictor::Rb,
                    3 => Predictor::Rc,
                    4 => Predictor::RaRbRc1,
                    5 => Predictor::RaRbRc2,
                    6 => Predictor::RaRbRc3,
                    7 => Predictor::RaRb,
                    _ => Predictor::NoPrediction,
                };

                let samples = self.reconstruct_samples(differences, predictor, point_transform)?;
                let samples_bytes = self.width as usize * self.height as usize * samples.len() * 2;
                let pixels: Vec<Vec<i32>> =
                    samples.into_iter().map(|c| c.into_iter().map(|v| v as i32).collect()).collect();
                self.limits.free_usize(samples_bytes);

                Ok(pixels)
            }

            JpegMode::Progressive | JpegMode::DifferentialProgressive => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = self.width.div_ceil(8 * max_h_samp as u32);
                let mcu_h = self.height.div_ceil(8 * max_v_samp as u32);

                let components_snapshot: Vec<_> = self
                    .components
                    .iter()
                    .map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor))
                    .collect();
                let mut planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
                for (h, v) in &components_snapshot {
                    planes.push(ComponentPlane::try_new(
                        mcu_w * 8 * *h as u32,
                        mcu_h * 8 * *v as u32,
                        &mut self.limits,
                    )?);
                }

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut planes)?,
                }

                let raster_planes = self.dequantize_and_idct_planes(&planes, true);
                for plane in &planes {
                    plane.free(&mut self.limits);
                }
                drop(planes);
                Ok(self.planes_to_component_pixels(&raster_planes))
            }

            _ => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = self.width.div_ceil(8 * max_h_samp as u32);
                let mcu_h = self.height.div_ceil(8 * max_v_samp as u32);

                let components_snapshot: Vec<_> = self
                    .components
                    .iter()
                    .map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor))
                    .collect();
                let mut planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
                for (h, v) in &components_snapshot {
                    planes.push(ComponentPlane::try_new(
                        mcu_w * 8 * *h as u32,
                        mcu_h * 8 * *v as u32,
                        &mut self.limits,
                    )?);
                }

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut planes)?,
                }

                let raster_planes = self.dequantize_and_idct_planes(&planes, false);
                for plane in &planes {
                    plane.free(&mut self.limits);
                }
                drop(planes);
                Ok(self.planes_to_component_pixels(&raster_planes))
            }
        }
    }

    fn planes_to_component_pixels(&self, planes: &[RasterPlane]) -> Vec<Vec<i32>> {
        let tw = self.width as usize;
        let th = self.height as usize;

        planes
            .iter()
            .map(|plane| {
                let (sw, sh) = (plane.width, plane.height);

                if sw == tw && sh == th {
                    plane.data.iter().map(|&v| v as i32).collect()
                } else {
                    let mut out = vec![0i32; tw * th];
                    for dy in 0..th {
                        for dx in 0..tw {
                            let sx = (dx * sw / tw).min(sw.saturating_sub(1));
                            let sy = (dy * sh / th).min(sh.saturating_sub(1));
                            out[dy * tw + dx] = plane.data.get(sy * sw + sx).copied().unwrap_or(0) as i32;
                        }
                    }
                    out
                }
            })
            .collect()
    }

    fn expand_2x(src: &[i32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<i32> {
        if src_w == dst_w && src_h == dst_h && src.len() == dst_w * dst_h {
            return src.to_vec();
        }

        let mut out = vec![0i32; dst_w * dst_h];
        if src_w == 0 || src_h == 0 || dst_w == 0 || src.len() < src_w * src_h {
            return out;
        }

        let expand_h = dst_w > src_w;
        let expand_v = dst_h > src_h;

        let mut h_buf = vec![0i32; dst_w * src_h];
        for (row, h_row) in src.chunks_exact(src_w).zip(h_buf.chunks_exact_mut(dst_w)) {
            if expand_h {
                for x in 0..src_w {
                    let xi2 = x * 2;
                    let next = if x + 1 < src_w { row[x + 1] } else { row[x] };
                    if xi2 < dst_w {
                        h_row[xi2] = row[x];
                    }
                    if xi2 + 1 < dst_w {
                        h_row[xi2 + 1] = (row[x] + next) >> 1;
                    }
                }
            } else {
                h_row.copy_from_slice(&row[..dst_w]);
            }
        }

        if expand_v {
            for (y, out_row) in out.chunks_exact_mut(dst_w).enumerate() {
                let sy = (y / 2).min(src_h - 1);
                let ny = (sy + 1).min(src_h - 1);
                let ra = &h_buf[sy * dst_w..(sy + 1) * dst_w];
                if y % 2 == 0 {
                    out_row.copy_from_slice(ra);
                } else {
                    let rb = &h_buf[ny * dst_w..(ny + 1) * dst_w];
                    for (o, (a, b)) in out_row.iter_mut().zip(ra.iter().zip(rb.iter())) {
                        *o = (a + b) >> 1;
                    }
                }
            }
        } else {
            let len = dst_h * dst_w;
            out.copy_from_slice(&h_buf[..len]);
        }
        out
    }

    fn expansion_bytes(components: usize, dst_w: usize, dst_h: usize, src_h: usize) -> usize {
        dst_w.saturating_mul(dst_h.saturating_add(src_h)).saturating_mul(components).saturating_mul(4)
    }

    fn decode_hierarchical(&mut self) -> VexelResult<Image> {
        if self.hierarchical_frames.is_empty() {
            return Err(VexelError::from(Error::new(ErrorKind::InvalidData, "No hierarchical frames found")));
        }

        let final_width = self.dhp_width;
        let final_height = self.dhp_height;
        let nc = self.hierarchical_frames[0].components.len();
        let precision = self.hierarchical_frames[0].precision;
        let level_shift = if precision <= 8 { 128i32 } else { 2048i32 };

        let frames = std::mem::take(&mut self.hierarchical_frames);
        let base = &frames[0];
        let raw_pixels = self.decode_frame_to_pixels(base)?;
        let is_base_lossless = matches!(base.mode, JpegMode::Lossless);

        let mut current_pixels: Vec<Vec<i32>> = if is_base_lossless {
            raw_pixels
        } else {
            raw_pixels.into_iter().map(|c| c.into_iter().map(|v| v + level_shift).collect()).collect()
        };

        let mut current_w = base.width as usize;
        let mut current_h = base.height as usize;

        let frame_pixel_budget = u64::from(final_width.max(base.width))
            .saturating_mul(u64::from(final_height.max(base.height)))
            .saturating_mul(HIERARCHICAL_PIXEL_BUDGET);
        let mut frame_pixels = u64::from(base.width).saturating_mul(u64::from(base.height));

        for frame in frames.iter().skip(1) {
            let dst_w = frame.width as usize;
            let dst_h = frame.height as usize;

            frame_pixels = frame_pixels.saturating_add(u64::from(frame.width).saturating_mul(u64::from(frame.height)));
            if frame_pixels > frame_pixel_budget {
                log_warn!("Hierarchical frames exceed the decoding work budget, ignoring the remaining frames");
                break;
            }

            let expand_bytes = Self::expansion_bytes(current_pixels.len(), dst_w, dst_h, current_h);
            self.limits.reserve_usize(expand_bytes)?;
            let mut upsampled: Vec<Vec<i32>> =
                current_pixels.iter().map(|comp| Self::expand_2x(comp, current_w, current_h, dst_w, dst_h)).collect();
            self.limits.free_usize(expand_bytes);

            let diff_raw = self.decode_frame_to_pixels(frame)?;

            let max_val = (1i32 << precision) - 1;
            for (comp_idx, diff) in diff_raw.iter().enumerate() {
                if comp_idx >= upsampled.len() {
                    break;
                }
                let up = &mut upsampled[comp_idx];
                for (u, d) in up.iter_mut().zip(diff.iter()) {
                    *u = (*u + *d).clamp(0, max_val);
                }
            }

            let diff_raw_bytes = diff_raw.iter().map(|v| v.len() * 4).sum::<usize>();
            self.limits.free_usize(diff_raw_bytes);

            current_pixels = upsampled;
            current_w = dst_w;
            current_h = dst_h;
        }

        self.hierarchical_frames = frames;

        let (final_width, final_height) = if final_width == 0 || final_height == 0 {
            log_warn!("DHP dimensions are {}x{}, using the last frame size", final_width, final_height);
            (current_w as u32, current_h as u32)
        } else {
            (final_width, final_height)
        };
        let out_w = final_width as usize;
        let out_h = final_height as usize;

        if out_w == 0 || out_h == 0 {
            return Err(VexelError::InvalidDimensions { width: final_width, height: final_height });
        }

        if current_w != out_w || current_h != out_h {
            let expand_bytes = Self::expansion_bytes(current_pixels.len(), out_w, out_h, current_h);
            self.limits.reserve_usize(expand_bytes)?;
            current_pixels = current_pixels
                .into_iter()
                .map(|comp| Self::expand_2x(&comp, current_w, current_h, out_w, out_h))
                .collect();
            self.limits.free_usize(expand_bytes);
        }

        if current_pixels.len() < 4 {
            current_pixels.resize(4, Vec::new());
        }

        let max_val = if precision <= 8 { 255i32 } else { 65535i32 };
        let npixels = out_w * out_h;
        let colorspace = self.detect_colorspace();

        if nc == 1 {
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels];
                for (i, p) in pixels.iter_mut().enumerate() {
                    *p = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u8;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels];
                for (i, p) in pixels.iter_mut().enumerate() {
                    *p = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u16;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L16(pixels)))
            }
        } else if nc == 4 {
            let mut pixels = vec![0u8; npixels * 3];
            for i in 0..npixels {
                let c0 = current_pixels[0].get(i).copied().unwrap_or(128).clamp(0, 255);
                let c1 = current_pixels[1].get(i).copied().unwrap_or(128).clamp(0, 255);
                let c2 = current_pixels[2].get(i).copied().unwrap_or(128).clamp(0, 255);
                let k_inv = current_pixels[3].get(i).copied().unwrap_or(128).clamp(0, 255);
                let (c_inv, m_inv, y_inv) = if colorspace == JpegColorspace::YCCK {
                    let cb = c1 - 128;
                    let cr = c2 - 128;
                    let y_luma = c0 << 16;
                    let r_inv = (255 << 16) - y_luma - 91881 * cr;
                    let g_inv = (255 << 16) - y_luma + 22554 * cb + 46802 * cr;
                    let b_inv = (255 << 16) - y_luma - 116130 * cb;
                    (
                        ((r_inv + 32768) >> 16).clamp(0, 255),
                        ((g_inv + 32768) >> 16).clamp(0, 255),
                        ((b_inv + 32768) >> 16).clamp(0, 255),
                    )
                } else {
                    (c0.clamp(0, 255), c1.clamp(0, 255), c2.clamp(0, 255))
                };
                pixels[i * 3] = ((c_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                pixels[i * 3 + 1] = ((m_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                pixels[i * 3 + 2] = ((y_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
            }
            Ok(Image::from_pixels(final_width, final_height, PixelData::RGB8(pixels)))
        } else {
            let convert_ycbcr = colorspace == JpegColorspace::YCbCr;
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels * 3];
                for i in 0..npixels {
                    let c0 = current_pixels[0].get(i).copied().unwrap_or(128).clamp(0, 255);
                    let c1 = current_pixels[1].get(i).copied().unwrap_or(128).clamp(0, 255);
                    let c2 = current_pixels[2].get(i).copied().unwrap_or(128).clamp(0, 255);
                    if convert_ycbcr {
                        let cb = c1 - 128;
                        let cr = c2 - 128;
                        let y128 = c0 << 16;
                        pixels[i * 3] = ((y128 + 91881 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 1] = ((y128 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 2] = ((y128 + 116130 * cb + 32768) >> 16).clamp(0, 255) as u8;
                    } else {
                        pixels[i * 3] = c0.clamp(0, 255) as u8;
                        pixels[i * 3 + 1] = c1.clamp(0, 255) as u8;
                        pixels[i * 3 + 2] = c2.clamp(0, 255) as u8;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels * 3];
                for i in 0..npixels {
                    let c0 = current_pixels[0].get(i).copied().unwrap_or(2048);
                    let c1 = current_pixels[1].get(i).copied().unwrap_or(2048);
                    let c2 = current_pixels[2].get(i).copied().unwrap_or(2048);
                    if convert_ycbcr {
                        let (c0, c1, c2) = (c0.clamp(0, 4095), c1.clamp(0, 4095), c2.clamp(0, 4095));
                        let cb = c1 - 2048;
                        let cr = c2 - 2048;
                        let y2048 = c0 << 16;
                        pixels[i * 3] = (((y2048 + 91881 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 1] =
                            (((y2048 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 2] = (((y2048 + 116130 * cb + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                    } else {
                        pixels[i * 3] = (c0.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 1] = (c1.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 2] = (c2.clamp(0, 65535) as u16) << 4;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB16(pixels)))
            }
        }
    }

    fn read_dhp(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let precision = self.reader.read_u8()?;
        let height = self.reader.read_u16()?;
        let width = self.reader.read_u16()?;
        let component_count = self.reader.read_u8()?;

        let mut components = Vec::with_capacity(usize::from(component_count));
        for _ in 0..component_count {
            let id = self.reader.read_u8()?;
            let sampling = self.reader.read_u8()?;
            let quantization_table = self.reader.read_u8()?;
            components.push(JpegFrameComponent {
                id,
                horizontal_sampling: sampling >> 4,
                vertical_sampling: sampling & 0x0F,
                quantization_table,
            });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::HierarchicalProgression(JpegFrame {
                marker: 0xDE,
                precision,
                height,
                width,
                component_count,
                components,
            }),
        );

        self.is_hierarchical = true;
        self.dhp_width = u32::from(width);
        self.dhp_height = u32::from(height);

        if self.walk_mode == WalkMode::Decode {
            self.limits.reserve_buffer(self.dhp_width, self.dhp_height, 4)?;
        }

        Ok(())
    }

    fn read_exp(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;
        let expand = self.reader.read_u8()?;
        let expand_horizontal = (expand >> 4) & 1 != 0;
        let expand_vertical = expand & 1 != 0;
        self.pending_expand_h = expand_horizontal;
        self.pending_expand_v = expand_vertical;
        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::Expand(JpegExpand { horizontal: expand_horizontal, vertical: expand_vertical }),
        );
        Ok(())
    }

    fn finalize_current_frame(&mut self) {
        if self.scans.is_empty() {
            return;
        }

        if self.hierarchical_frames.len() >= MAX_HIERARCHICAL_FRAMES {
            log_warn!("More than {} hierarchical frames, ignoring the rest", MAX_HIERARCHICAL_FRAMES);
            self.scans.clear();
            return;
        }

        let frame = HierarchicalFrame {
            width: self.width,
            height: self.height,
            mode: self.mode.clone(),
            coding_method: self.coding_method.clone(),
            precision: self.precision,
            components: self.components.clone(),
            quantization_tables: self.quantization_tables.clone(),
            scans: std::mem::take(&mut self.scans),
        };

        self.hierarchical_frames.push(frame);
        self.pending_expand_h = false;
        self.pending_expand_v = false;
    }

    fn start_frame(&mut self, marker: u8, segment_start: u64) -> VexelResult<()> {
        if self.is_hierarchical {
            self.finalize_current_frame();
        }
        self.frame_has_scan = false;

        let mode = match marker & 0x07 {
            1 => Some(JpegMode::ExtendedSequential),
            2 => Some(JpegMode::Progressive),
            3 => Some(JpegMode::Lossless),
            5 => Some(JpegMode::DifferentialSequential),
            6 => Some(JpegMode::DifferentialProgressive),
            7 => Some(JpegMode::DifferentialLossless),
            _ => None,
        };
        let arithmetic = marker & 0x08 != 0;

        if let Some(mode) = mode {
            self.mode = mode;
        }
        if arithmetic {
            self.coding_method = JpegCodingMethod::Arithmetic;
        }

        self.read_start_of_frame(marker, segment_start)
    }

    fn walk(&mut self, mode: WalkMode) -> VexelResult<()> {
        self.reset_state();
        self.walk_mode = mode;
        self.info = Some(JpegInfo::default());
        self.reader.seek(SeekFrom::Start(0))?;

        let result = self.walk_markers();
        self.finish_info();
        result
    }

    fn walk_markers(&mut self) -> VexelResult<()> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            let Some(marker) = marker else {
                log_debug!("End of file reached");
                break;
            };

            log_debug!("Found marker: {:?}", marker);

            let segment_start = self.reader.stream_position().unwrap_or(0).saturating_sub(2);
            let code = marker.to_u16();

            let result = match marker {
                JpegMarker::SOI => {
                    self.record(segment_start, Some(2), JpegSection::StartOfImage);
                    Ok(())
                }
                JpegMarker::EOI => {
                    self.record(segment_start, Some(2), JpegSection::EndOfImage);
                    self.record_trailing_data(segment_start + 2);
                    break;
                }
                JpegMarker::COM => self.read_com(segment_start),
                JpegMarker::APP0
                | JpegMarker::APP1
                | JpegMarker::APP2
                | JpegMarker::APP3
                | JpegMarker::APP4
                | JpegMarker::APP5
                | JpegMarker::APP6
                | JpegMarker::APP7
                | JpegMarker::APP8
                | JpegMarker::APP9
                | JpegMarker::APP10
                | JpegMarker::APP11
                | JpegMarker::APP12
                | JpegMarker::APP13
                | JpegMarker::APP14
                | JpegMarker::APP15 => self.read_app((code & 0x0F) as u8, segment_start),
                JpegMarker::SOF0
                | JpegMarker::SOF1
                | JpegMarker::SOF2
                | JpegMarker::SOF3
                | JpegMarker::SOF5
                | JpegMarker::SOF6
                | JpegMarker::SOF7
                | JpegMarker::SOF9
                | JpegMarker::SOF10
                | JpegMarker::SOF11
                | JpegMarker::SOF13
                | JpegMarker::SOF14
                | JpegMarker::SOF15 => {
                    if !self.is_hierarchical && self.frame_has_scan {
                        log_warn!("Frame header {:?} after a completed scan, ignoring the rest of the file", marker);
                        if self.reader.seek(SeekFrom::Start(segment_start)).is_ok() {
                            self.record_trailing_data(segment_start);
                        }
                        break;
                    }
                    self.start_frame((code & 0xFF) as u8, segment_start)
                }
                JpegMarker::DHP => self.read_dhp(segment_start),
                JpegMarker::EXP => self.read_exp(segment_start),
                JpegMarker::DRI => self.read_restart_interval(segment_start),
                JpegMarker::DNL => self.read_dnl(segment_start),
                JpegMarker::DQT => self.read_quantization_table(segment_start),
                JpegMarker::DHT => self.read_huffman_table(segment_start),
                JpegMarker::DAC => self.read_dac(segment_start),
                JpegMarker::SOS => self.read_start_of_scan(segment_start),
                _ => {
                    log_warn!("Unhandled marker found: {:?}", marker);
                    self.skip_unknown_marker_segment(code, segment_start)
                }
            };

            match result {
                Ok(_) => {}
                Err(VexelError::LimitExceeded(_)) => return result,
                Err(e) => {
                    log_warn!("Failed to process {:?} marker segment: {}", marker, e);
                }
            }
        }

        Ok(())
    }

    fn record_trailing_data(&mut self, offset: u64) {
        let remaining = self.reader.bytes_left().unwrap_or(0);
        if remaining > 0 {
            self.record(offset, Some(remaining), JpegSection::TrailingData);
        }
    }

    fn finish_info(&mut self) {
        let summary = self.build_summary();
        if let Some(info) = &mut self.info {
            info.summary = summary;
        }
    }

    fn build_summary(&self) -> Option<ImageSummary> {
        let info = self.info.as_ref()?;
        let first_frame = info.sections.iter().find_map(|section| match &section.data {
            JpegSection::Frame(frame) => Some(frame.marker),
            _ => None,
        })?;

        let (width, height) =
            if self.is_hierarchical { (self.dhp_width, self.dhp_height) } else { (self.width, self.height) };

        let color_model = match self.detect_colorspace() {
            JpegColorspace::Grayscale => ColorModel::Gray,
            JpegColorspace::RGB => ColorModel::Rgb,
            JpegColorspace::YCbCr => ColorModel::YCbCr,
            JpegColorspace::CMYK => ColorModel::Cmyk,
            JpegColorspace::YCCK => ColorModel::Ycck,
            JpegColorspace::RGBA => ColorModel::Rgba,
            JpegColorspace::Unknown => ColorModel::Unknown,
        };

        let mut summary = ImageSummary::new(width, height, color_model, self.components.len() as u16, self.precision);
        summary.has_alpha = color_model == ColorModel::Rgba;
        let hierarchy = if self.is_hierarchical { "Hierarchical, " } else { "" };
        summary.compression =
            Some(format!("{}{}, {} coding", hierarchy, process_name(first_frame), coding_name(first_frame)));
        let factors: Vec<(u8, u8)> =
            self.components.iter().map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor)).collect();
        summary.subsampling = subsampling_name(&factors);
        summary.icc_profile = self.icc.assemble();
        Some(summary)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        if let Err(e) = self.walk(WalkMode::Decode) {
            self.info = None;
            return Err(e);
        }

        log_debug!("Dimensions: {}x{}. Number of pixels: {}", self.width, self.height, self.width * self.height);
        log_debug!("Number of components: {}", self.components.len());
        log_debug!("Number of scans: {}", self.scans.len());
        log_debug!("Mode: {:?}", self.mode);
        log_debug!("Coding method: {:?}", self.coding_method);
        log_debug!("Bit depth: {}", self.precision);
        log_debug!("Restart interval: {}", self.restart_interval);
        log_debug!(
            "Sampling factors: {:?}",
            self.components
                .iter()
                .map(|c| format!("{}/{}", c.horizontal_sampling_factor, c.vertical_sampling_factor))
                .collect::<Vec<String>>()
                .join(", ")
        );

        if self.is_hierarchical {
            self.finalize_current_frame();
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if self.components.is_empty() {
            return Err(VexelError::Custom("Invalid JPEG: no components".into()));
        }

        if self.is_hierarchical {
            return self.decode_hierarchical();
        }

        match &self.mode {
            JpegMode::Baseline => self.decode_baseline(),
            JpegMode::ExtendedSequential => {
                // TODO general decoding process is same as baseline, so this method can be used,
                // but it would be nice to have a different name for this mode
                self.decode_baseline()
            }
            JpegMode::Progressive => self.decode_progressive(),
            JpegMode::Lossless => self.decode_lossless(),
            JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive | JpegMode::DifferentialLossless => {
                log_warn!("Differential JPEG mode outside hierarchical context, treating as baseline");
                self.decode_baseline()
            }
        }
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        scan_jpeg_thumbnails(&mut self.reader, &self.limits)
    }
}

pub(crate) fn scan_jpeg_thumbnails<R: Read + Seek>(
    reader: &mut BitReader<R>,
    limits: &Limits,
) -> VexelResult<Vec<Image>> {
    let saved_pos = reader.stream_position().unwrap_or(0);
    let result = scan_thumbnail_segments(reader, limits);
    let _ = reader.seek(SeekFrom::Start(saved_pos));
    result
}

fn scan_thumbnail_segments<R: Read + Seek>(reader: &mut BitReader<R>, limits: &Limits) -> VexelResult<Vec<Image>> {
    reader.seek(SeekFrom::Start(0))?;

    let mut thumbnails = Vec::new();
    let mut budget = limits.clone();

    while let Ok(Some(marker)) = reader.next_marker(&JPEG_MARKERS) {
        let decoded_before = thumbnails.len();
        match marker {
            JpegMarker::SOI
            | JpegMarker::TEM
            | JpegMarker::RST0
            | JpegMarker::RST1
            | JpegMarker::RST2
            | JpegMarker::RST3
            | JpegMarker::RST4
            | JpegMarker::RST5
            | JpegMarker::RST6
            | JpegMarker::RST7 => continue,
            JpegMarker::EOI | JpegMarker::SOS => break,
            JpegMarker::APP0 | JpegMarker::APP1 => {
                let length = match reader.read_u16() {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let payload_len = (length as usize).saturating_sub(2);
                let payload = match reader.read_bytes(payload_len) {
                    Ok(p) => p,
                    Err(_) => break,
                };

                if marker == JpegMarker::APP0 {
                    extract_app0_thumbnail(&payload, &budget, &mut thumbnails);
                } else if payload.starts_with(b"Exif\0\0") {
                    if let Some(bytes) = crate::utils::exif::exif_thumbnail(&payload[6..]) {
                        if let Some(image) = decode_embedded_jpeg(bytes, &budget) {
                            thumbnails.push(image);
                        }
                    }
                }

                let new_bytes: usize = thumbnails[decoded_before..].iter().map(|t| t.pixels().as_bytes().len()).sum();
                if budget.reserve_usize(new_bytes).is_err() {
                    log_warn!("Embedded thumbnails exceed the allocation limit, ignoring the rest");
                    thumbnails.truncate(decoded_before);
                    break;
                }
            }
            _ => {
                let length = match reader.read_u16() {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let skip = (length as i64) - 2;
                if skip > 0 && reader.seek(SeekFrom::Current(skip)).is_err() {
                    break;
                }
            }
        }
    }

    Ok(thumbnails)
}

fn extract_app0_thumbnail(payload: &[u8], limits: &Limits, thumbnails: &mut Vec<Image>) {
    if payload.len() < 6 {
        return;
    }

    match &payload[..5] {
        b"JFIF\0" => {
            let width = payload.get(12).copied().unwrap_or(0) as u32;
            let height = payload.get(13).copied().unwrap_or(0) as u32;
            if width == 0 || height == 0 {
                return;
            }

            let size = (width * height * 3) as usize;
            let data = payload.get(14..).unwrap_or(&[]);
            let rgb = data.get(..size).unwrap_or(data).to_vec();

            let mut pixels = PixelData::RGB8(rgb);
            pixels.correct_pixels(width, height);
            thumbnails.push(Image::from_pixels(width, height, pixels));
        }
        b"JFXX\0" => match payload[5] {
            0x10 => {
                if let Some(image) = decode_embedded_jpeg(&payload[6..], limits) {
                    thumbnails.push(image);
                }
            }
            0x11 => {
                let width = payload.get(6).copied().unwrap_or(0) as u32;
                let height = payload.get(7).copied().unwrap_or(0) as u32;
                if width == 0 || height == 0 {
                    return;
                }

                let palette = payload.get(8..8 + 768).unwrap_or(&[]);
                let indices = payload.get(8 + 768..).unwrap_or(&[]);

                let mut rgb = Vec::with_capacity((width * height * 3) as usize);
                for &index in indices.iter().take((width * height) as usize) {
                    let base = index as usize * 3;
                    let entry = palette.get(base..base + 3).unwrap_or(&[0, 0, 0]);
                    rgb.extend_from_slice(entry);
                }

                let mut pixels = PixelData::RGB8(rgb);
                pixels.correct_pixels(width, height);
                thumbnails.push(Image::from_pixels(width, height, pixels));
            }
            0x13 => {
                let width = payload.get(6).copied().unwrap_or(0) as u32;
                let height = payload.get(7).copied().unwrap_or(0) as u32;
                if width == 0 || height == 0 {
                    return;
                }

                let size = (width * height * 3) as usize;
                let data = payload.get(8..).unwrap_or(&[]);
                let rgb = data.get(..size).unwrap_or(data).to_vec();

                let mut pixels = PixelData::RGB8(rgb);
                pixels.correct_pixels(width, height);
                thumbnails.push(Image::from_pixels(width, height, pixels));
            }
            code => {
                log_warn!("Unknown JFXX extension code: {:#04x}", code);
            }
        },
        _ => {}
    }
}

fn decode_embedded_jpeg(bytes: &[u8], limits: &Limits) -> Option<Image> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        log_warn!("Embedded thumbnail does not start with a JPEG SOI marker, skipping");
        return None;
    }

    let mut decoder = JpegDecoder::new(std::io::Cursor::new(bytes.to_vec()));
    decoder.set_limits(limits.clone());

    match decoder.decode() {
        Ok(image) => Some(image),
        Err(e) => {
            log_warn!("Failed to decode embedded thumbnail: {}", e);
            None
        }
    }
}
