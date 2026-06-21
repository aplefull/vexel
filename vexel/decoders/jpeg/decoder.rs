use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::exif::ExifReader;
use crate::utils::info::JpegInfo;
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use crate::decoders::jpeg::idct::dequantize_and_idct;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use std::fmt::Debug;
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::decoders::jpeg::types::{ArithmeticCodingTable, ArithmeticCodingValue, ColorComponentInfo, DACData, DHTData, DQTData, HuffmanTable, JFIFData, JFIFHeader, JpegCodingMethod, JpegMode, JpegSegmentData, JpegSegmentInfo, Predictor, QuantizationTable, SOFData, SOSData, ScanComponent, ScanData, DEFAULT_QUANTIZATION_TABLE, ZIGZAG_MAP};

#[derive(Debug, Clone)]
struct ComponentPlane {
    data: Vec<i32>,
    blocks_per_line: u32,
}

impl ComponentPlane {
    fn new(width: u32, height: u32) -> Self {
        let blocks_per_line = (width + 7) / 8;
        let block_lines = (height + 7) / 8;

        Self {
            blocks_per_line,
            data: vec![0; (blocks_per_line * block_lines * 64) as usize],
        }
    }

    fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i32; 64]> {
        let block_idx = block_y * self.blocks_per_line + block_x;
        let start = (block_idx * 64) as usize;
        if start + 64 <= self.data.len() {
            (&mut self.data[start..start + 64]).try_into().ok()
        } else {
            None
        }
    }

    fn deinterleave(&self, sw: u32, sh: u32) -> Vec<i32> {
        use crate::decoders::jpeg::upsample as up;
        let mut out = vec![0i32; (sw * sh) as usize];
        up::deinterleave_blocks(&self.data, self.blocks_per_line, sw, sh, &mut out);
        out
    }

}

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
}

impl<'a> ArithmeticDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            c: 0,
            a: 0,
            ct: -16,
            fixed_bin: 113,
            error: false,
        }
    }

    fn reset(&mut self) {
        self.c = 0;
        self.a = 0;
        self.ct = -16;
        self.error = false;
    }

    fn read_byte(&mut self) -> u32 {
        if self.pos >= self.data.len() {
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
        let qe = qe as u32;

        let temp = self.a.wrapping_sub(qe);
        self.a = temp;
        let shifted = temp << (self.ct as u32);

        if self.c >= shifted {
            self.c -= shifted;
            if self.a < qe {
                self.a = qe;
                *st = ((*st as u8) & 0x80) ^ nm;
                sv as u8 >> 7
            } else {
                self.a = qe;
                *st = ((*st as u8) & 0x80) ^ nl;
                if switch_mps != 0 {
                    *st ^= 0x80;
                }
                1 - (sv as u8 >> 7)
            }
        } else if self.a < 0x8000 {
            if self.a < qe {
                *st = ((*st as u8) & 0x80) ^ nl;
                if switch_mps != 0 {
                    *st ^= 0x80;
                }
                1 - (sv as u8 >> 7)
            } else {
                *st = ((*st as u8) & 0x80) ^ nm;
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
        if raw >= 0x8000 { raw - 0x10000 } else { raw }
    }

    fn decode_dc_first(
        &mut self,
        block: &mut [i32],
        comp_idx: usize,
        dc_tbl: usize,
        dc_l: u8,
        dc_u: u8,
        successive_low: u8,
        dc_context: &mut Vec<usize>,
        last_dc_val: &mut Vec<i32>,
        dc_stats: &mut Vec<Vec<u8>>,
        is_differential: bool,
    ) {
        let val = self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats, is_differential);
        block[0] = val << (successive_low as i32);
    }

    fn decode_dc_refine(
        &mut self,
        block: &mut [i32],
        successive_low: u8,
    ) {
        if self.error { return; }
        let p1 = 1i32 << successive_low;
        let mut st = self.fixed_bin;
        if self.arith_decode(&mut st) != 0 {
            block[0] |= p1;
        }
        self.fixed_bin = st;
    }

    fn decode_ac_first(
        &mut self,
        block: &mut [i32],
        ac_tbl: usize,
        ac_k: u8,
        ss: usize,
        se: usize,
        successive_low: u8,
        ac_stats: &mut Vec<Vec<u8>>,
    ) {
        if self.error || ac_tbl >= ac_stats.len() { return; }

        let mut k = ss;
        while k <= se {
            let st_base = 3 * (k.saturating_sub(1));
            if st_base + 2 >= ac_stats[ac_tbl].len() { break; }

            if self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 { break; }

            loop {
                let cur_base = 3 * (k.saturating_sub(1));
                if cur_base + 1 >= ac_stats[ac_tbl].len() { break; }
                if self.arith_decode(&mut ac_stats[ac_tbl][cur_base + 1]) != 0 { break; }
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
            if cur_base + 2 >= ac_stats[ac_tbl].len() { break; }

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
                        if self.arith_decode(&mut ac_stats[ac_tbl][idx]) == 0 { break; }
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
            if sign != 0 { v = -v; }

            if k < 64 {
                block[ZIGZAG_MAP[k] as usize] = v << (successive_low as i32);
            }
            k += 1;
        }
    }

    fn decode_ac_refine(
        &mut self,
        block: &mut [i32],
        ac_tbl: usize,
        ss: usize,
        se: usize,
        successive_low: u8,
        ac_stats: &mut Vec<Vec<u8>>,
    ) {
        if self.error || ac_tbl >= ac_stats.len() { return; }

        let p1 = 1i32 << successive_low;
        let m1 = (-1i32) << successive_low;
        let tbl_len = ac_stats[ac_tbl].len();

        let mut kex = se;
        while kex > 0 && kex < 64 {
            if block[ZIGZAG_MAP[kex] as usize] != 0 { break; }
            kex -= 1;
        }

        let mut k = ss;
        'outer: while k <= se {
            let st_base = 3 * (k.saturating_sub(1));
            if st_base + 2 >= tbl_len { break; }

            if k > kex {
                if self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 { break; }
            }

            loop {
                if k >= 64 { break 'outer; }
                let coef_idx = ZIGZAG_MAP[k] as usize;
                let coef = block[coef_idx];
                let st_base = 3 * (k.saturating_sub(1));
                if st_base + 2 >= tbl_len { break 'outer; }

                if coef != 0 {
                    if self.arith_decode(&mut ac_stats[ac_tbl][st_base + 2]) != 0 {
                        if coef < 0 {
                            block[coef_idx] += m1;
                        } else {
                            block[coef_idx] += p1;
                        }
                    }
                    break;
                } else {
                    if self.arith_decode(&mut ac_stats[ac_tbl][st_base + 1]) != 0 {
                        let mut fixed_st = self.fixed_bin;
                        let sign = self.arith_decode(&mut fixed_st);
                        self.fixed_bin = fixed_st;
                        block[coef_idx] = if sign != 0 { m1 } else { p1 };
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
        block: &mut [i32],
        comp_idx: usize,
        dc_tbl: usize,
        ac_tbl: usize,
        dc_l: u8,
        dc_u: u8,
        ac_k: u8,
        dc_context: &mut Vec<usize>,
        last_dc_val: &mut Vec<i32>,
        dc_stats: &mut Vec<Vec<u8>>,
        ac_stats: &mut Vec<Vec<u8>>,
        is_differential: bool,
    ) {
        if self.error { return; }

        let val = self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats, is_differential);
        block[0] = val;

        if self.error { return; }

        if ac_tbl >= ac_stats.len() { return; }

        let mut k = 1usize;
        while k <= 63 {
            let st_base = 3 * (k - 1);
            if st_base + 2 >= ac_stats[ac_tbl].len() { break; }

            if self.arith_decode(&mut ac_stats[ac_tbl][st_base]) != 0 { break; }

            loop {
                let cur_base = 3 * (k - 1);
                if cur_base + 1 >= ac_stats[ac_tbl].len() { break; }
                if self.arith_decode(&mut ac_stats[ac_tbl][cur_base + 1]) != 0 { break; }
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
            if cur_base + 2 >= ac_stats[ac_tbl].len() { break; }

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
                        if self.arith_decode(&mut ac_stats[ac_tbl][idx]) == 0 { break; }
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
            if sign != 0 { v = -v; }

            if k < 64 {
                block[ZIGZAG_MAP[k] as usize] = v;
            }
            k += 1;
        }
    }

    fn decode_lossless_difference(
        &mut self,
        contexts: &mut [u8; 160],
        da: i32,
        db: i32,
        l: u8,
        u: u8,
    ) -> i32 {
        fn classify(diff: i32, l: u8, u: u8) -> usize {
            let abs = diff.unsigned_abs() as i64;
            let threshold_l = if l == 0 { 0i64 } else { (1i64 << l) >> 1 };
            let threshold_u = 1i64 << u;
            if abs <= threshold_l {
                2
            } else if abs <= threshold_u {
                if diff < 0 { 1 } else { 3 }
            } else {
                if diff < 0 { 0 } else { 4 }
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

        if sign { -(sz + 1) } else { sz + 1 }
    }
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

pub struct JpegDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    jfif_header: Option<JFIFHeader>,
    comments: Vec<String>,
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
    segments: Vec<JpegSegmentInfo>,
    reader: BitReader<R>,
    adobe_color_transform: Option<u8>,
    is_hierarchical: bool,
    dhp_width: u32,
    dhp_height: u32,
    pending_expand_h: bool,
    pending_expand_v: bool,
    hierarchical_frames: Vec<HierarchicalFrame>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    // TODO remove redundant fields, that are duplicated in scans
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            comments: Vec::new(),
            jfif_header: None,
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
            segments: Vec::new(),
            reader: BitReader::new(reader),
            adobe_color_transform: None,
            is_hierarchical: false,
            dhp_width: 0,
            dhp_height: 0,
            pending_expand_h: false,
            pending_expand_v: false,
            hierarchical_frames: Vec::new(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> JpegInfo {
        JpegInfo {
            segments: self.segments.clone(),
        }
    }

    fn skip_unknown_marker_segment(&mut self, marker: &str, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        for _ in 0..(length - 2) {
            self.reader.read_u8()?;
        }

        self.record_segment(segment_start, marker, JpegSegmentData::Unknown {
            marker: marker.to_string(),
            length,
        });

        Ok(())
    }

    fn read_com(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let mut comment_bytes = Vec::new();
        for _ in 0..length - 2 {
            comment_bytes.push(self.reader.read_u8()?);
        }

        let text = String::from_utf8_lossy(&comment_bytes).to_string();
        self.comments.push(text.clone());

        self.record_segment(segment_start, "COM", JpegSegmentData::COM { text });

        Ok(())
    }

    fn read_app0_jfif(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        if length < 7 {
            for _ in 0..(length.saturating_sub(2)) {
                self.reader.read_u8()?;
            }
            self.record_segment(segment_start, "APP0", JpegSegmentData::Unknown {
                marker: "APP0".to_string(),
                length,
            });
            return Ok(());
        }

        let mut identifier = Vec::new();
        for _ in 0..5 {
            identifier.push(self.reader.read_u8()?);
        }

        let identifier = String::from_utf8_lossy(&identifier).to_string();

        if identifier != "JFIF\0" {
            log_warn!(
                "Invalid JFIF identifier in APP0, might not be a JFIF header: {}",
                identifier
            );
            let remaining = (length as i32) - 7;
            for _ in 0..remaining.max(0) {
                self.reader.read_u8()?;
            }
            self.record_segment(segment_start, "APP0", JpegSegmentData::Unknown {
                marker: "APP0".to_string(),
                length,
            });
            return Ok(());
        }

        let payload_len = (length as usize).saturating_sub(7);
        let mut payload = vec![0u8; payload_len];
        for i in 0..payload_len {
            payload[i] = self.reader.read_u8().unwrap_or(0);
        }

        let get = |i: usize| -> u8 { payload.get(i).copied().unwrap_or(0) };
        let get_u16 = |i: usize| -> u16 { u16::from_be_bytes([get(i), get(i + 1)]) };

        let version_major = get(0);
        let version_minor = get(1);
        let density_units = get(2);
        let x_density = get_u16(3);
        let y_density = get_u16(5);
        let thumbnail_width = get(7);
        let thumbnail_height = get(8);

        let max_thumbnail_bytes = payload_len.saturating_sub(9);
        let thumbnail_size = (thumbnail_width as usize * thumbnail_height as usize * 3).min(max_thumbnail_bytes);
        let thumbnail_data = payload.get(9..9 + thumbnail_size).unwrap_or(&[]).to_vec();

        self.jfif_header = Some(JFIFHeader {
            identifier: identifier.clone(),
            version_major,
            version_minor,
            density_units,
            x_density,
            y_density,
            thumbnail_width,
            thumbnail_height,
            thumbnail_data: thumbnail_data.clone(),
        });

        let expected_len = 16 + thumbnail_width as u16 * thumbnail_height as u16 * 3;
        if length != expected_len {
            log_warn!(
                "Invalid JFIF segment length, expected {}, got {}",
                expected_len,
                length
            );
        }

        self.record_segment(segment_start, "APP0", JpegSegmentData::APP0(JFIFData {
            length,
            identifier,
            version_major,
            version_minor,
            density_units,
            x_density,
            y_density,
            thumbnail_width,
            thumbnail_height,
            thumbnail_data,
        }));

        Ok(())
    }

    fn read_app1_exif(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;
        let payload = self.reader.read_bytes((length - 2) as usize)?;

        let exif = if payload.starts_with(b"Exif\0\0") {
            ExifReader::parse(&payload[6..])
        } else {
            None
        };

        self.record_segment(segment_start, "APP1", JpegSegmentData::APP1 { length, exif });

        Ok(())
    }

    fn read_app14_adobe(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        if length < 12 {
            for _ in 0..(length.saturating_sub(2)) {
                self.reader.read_u8()?;
            }
            self.record_segment(segment_start, "APP14", JpegSegmentData::APP {
                marker: "APP14".to_string(),
                length,
            });
            return Ok(());
        }

        let mut id = [0u8; 5];
        for b in &mut id {
            *b = self.reader.read_u8()?;
        }

        if &id != b"Adobe" {
            let remaining = (length as usize).saturating_sub(7);
            for _ in 0..remaining {
                self.reader.read_u8()?;
            }
            self.record_segment(segment_start, "APP14", JpegSegmentData::APP {
                marker: "APP14".to_string(),
                length,
            });
            return Ok(());
        }

        let _version = self.reader.read_u16()?;
        let _flags0 = self.reader.read_u16()?;
        let _flags1 = self.reader.read_u16()?;
        let color_transform = self.reader.read_u8()?;

        self.adobe_color_transform = Some(color_transform);
        log_debug!("Adobe APP14: color_transform={}", color_transform);

        self.record_segment(segment_start, "APP14", JpegSegmentData::APP {
            marker: "APP14".to_string(),
            length,
        });

        Ok(())
    }

    fn should_convert_ycbcr(&self) -> bool {
        let nc = self.components.len();
        if nc == 1 {
            return false;
        }

        if let Some(ct) = self.adobe_color_transform {
            return ct != 0;
        }

        if nc == 3 {
            let ids: Vec<u8> = self.components.iter().map(|c| c.id).collect();
            if ids == [82, 71, 66] {
                return false;
            }
        }

        true
    }

    fn read_start_of_frame(&mut self, sof_marker: &str, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.precision = self.reader.read_u8()?;

        if self.mode == JpegMode::Lossless {
            if self.precision < 2 || self.precision > 16 {
                log_warn!("Invalid precision for lossless jpeg mode: {}, clamping", self.precision);
                self.precision = self.precision.clamp(2, 16);
            }
        }

        self.height = self.reader.read_u16()? as u32;
        self.width = self.reader.read_u16()? as u32;

        if self.height == 0 || self.width == 0 {
            return Err(VexelError::from(Error::new(
                ErrorKind::InvalidData,
                "Invalid image dimensions",
            )));
        }

        // TODO rename them, they are not MCU dimensions, but dimensions of the image in MCUs
        self.mcu_width = (self.width + 7) / 8;
        self.mcu_height = (self.height + 7) / 8;

        self.component_count = self.reader.read_u8()?;

        if self.component_count > 4 || self.component_count == 0 {
            log_warn!(
                "Invalid number of components in SOF marker: {}, assuming 3",
                self.component_count
            );
            self.component_count = 3;
        }

        self.components.clear();

        for _ in 0..self.component_count {
            let id = self.reader.read_u8()?;
            let sampling_factors = self.reader.read_u8()?;
            let horizontal_sampling_factor = (sampling_factors >> 4) & 0xF;
            let vertical_sampling_factor = sampling_factors & 0xF;
            let quantization_table_id = self.reader.read_u8()?;

            if id == 1 {
                self.horizontal_sampling_factor = horizontal_sampling_factor;
                self.vertical_sampling_factor = vertical_sampling_factor;
            }

            self.components.push(ColorComponentInfo {
                id,
                horizontal_sampling_factor,
                vertical_sampling_factor,
                quantization_table_id,
                dc_table_selector: 0,
                ac_table_selector: 0,
            });
        }

        if length != 8 + 3 * self.component_count as u16 {
            log_warn!(
                "Invalid SOF marker length, expected {}, got {}",
                8 + 3 * self.component_count,
                length
            );
        }

        self.record_segment(segment_start, sof_marker, JpegSegmentData::SOF(SOFData {
            length,
            marker: sof_marker.to_string(),
            precision: self.precision,
            width: self.width,
            height: self.height,
            component_count: self.component_count,
            components: self.components.clone(),
        }));

        Ok(())
    }

    fn read_restart_interval(&mut self, segment_start: u64) -> VexelResult<()> {
        self.reader.read_u16()?;

        self.restart_interval = self.reader.read_u16()?;

        self.record_segment(segment_start, "DRI", JpegSegmentData::DRI { restart_interval: self.restart_interval });

        Ok(())
    }

    fn read_quantization_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut table_length = (segment_length as i16).saturating_sub(2);

        let mut new_tables = Vec::new();

        while table_length > 0 {
            let mut table = Vec::new();
            let table_spec = self.reader.read_u8()?;
            let id = table_spec & 0x0F;
            let precision = (table_spec >> 4) & 0x0F;

            table_length = table_length.saturating_sub(1);

            if precision == 0 {
                for _ in 0..64 {
                    table.push(self.reader.read_u8()? as u16);
                }
                table_length = table_length.saturating_sub(64);
            } else {
                for _ in 0..64 {
                    table.push(self.reader.read_u16()?);
                }
                table_length = table_length.saturating_sub(128);
            }

            let qt = QuantizationTable {
                id,
                precision,
                length: 0,
                table: Self::unzigzag_block(&table.as_slice()).to_vec(),
            };

            new_tables.push(qt.clone());
            self.quantization_tables.push(qt);
        }

        self.record_segment(segment_start, "DQT", JpegSegmentData::DQT(DQTData {
            length: segment_length,
            tables: new_tables,
        }));

        Ok(())
    }

    fn unzigzag_block(block: &[u16]) -> [u16; 64] {
        let mut unzigzagged = [0u16; 64];

        for i in 0..64 {
            unzigzagged[ZIGZAG_MAP[i] as usize] = block.get(i).copied().unwrap_or(0);
        }

        unzigzagged
    }

    fn read_huffman_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let total_length = self.reader.read_bits(16)? as u16;
        let mut segment_length = (total_length as i16).saturating_sub(2);

        let mut new_tables = Vec::new();

        while segment_length > 0 {
            let table_spec = self.reader.read_bits(8)?;
            let id = (table_spec & 0x0F) as u8;
            let class = ((table_spec >> 4) & 0x0F) as u8;

            let mut offsets = Vec::with_capacity(17);
            let mut total_symbols: u32 = 0;

            offsets.push(0);
            for _ in 1..17 {
                let offset = self.reader.read_bits(8)?;
                total_symbols += offset;
                offsets.push(total_symbols);
            }

            if total_symbols > 256 {
                log_warn!("Too many symbols in Huffman table: {}", total_symbols);
                total_symbols = 256;
            }

            let mut table = Vec::with_capacity(total_symbols as usize);
            for _ in 0..total_symbols {
                table.push(self.reader.read_bits(8)? as u8);
            }

            segment_length -= 1 + 16 + total_symbols as i16;

            let mut huffman_table = HuffmanTable {
                id,
                class,
                offsets,
                symbols: table,
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
            new_tables.push(huffman_table.clone());

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

        self.record_segment(segment_start, "DHT", JpegSegmentData::DHT(DHTData {
            length: total_length,
            tables: new_tables,
        }));

        Ok(())
    }

    fn read_dac(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut data_length = segment_length - 2;

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();

        while data_length > 0 {
            let table_info = self.reader.read_u8()?;
            let table_class = (table_info >> 4) & 0x0F;
            let identifier = table_info & 0x0F;

            let value = self.reader.read_u8()?;

            let (value, length) = if table_class == 0 {
                ((value >> 4) & 0x0F, value & 0x0F)
            } else {
                (value, 0)
            };

            let ac_value = ArithmeticCodingValue { value, length };

            let table = ArithmeticCodingTable {
                table_class,
                identifier,
                values: Vec::from([ac_value]),
            };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => {
                    log_warn!(
                        "Invalid arithmetic coding table class: {}, ignoring the table",
                        table_class
                    );
                }
            }

            data_length -= 2;
        }

        self.ac_arithmetic_tables = ac_tables.clone();
        self.dc_arithmetic_tables = dc_tables.clone();

        self.record_segment(segment_start, "DAC", JpegSegmentData::DAC(DACData {
            length: segment_length,
            ac_tables,
            dc_tables,
        }));

        Ok(())
    }

    fn read_start_of_scan(&mut self, segment_start: u64) -> VexelResult<()> {
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

        let start_spectral = self.reader.read_u8()?;
        let end_spectral = self.reader.read_u8()?;
        let successive_approx = self.reader.read_u8()?;
        let successive_high = (successive_approx >> 4) & 0x0F;
        let successive_low = successive_approx & 0x0F;

        if length != 6 + (2 * scan_component_count as u16) {
            log_warn!(
                "Invalid SOS marker length, expected {}, got {}",
                6 + (2 * scan_component_count as u16),
                length
            );
        }

        let mut current_byte = self.reader.read_u8().unwrap_or_else(|_| {
            log_warn!("Unexpected EOF while reading first byte of scan data");
            0
        });

        let mut scan_data = Vec::with_capacity((self.width as usize * self.height as usize) / 4);
        let is_arithmetic = self.coding_method == JpegCodingMethod::Arithmetic;

        loop {
            if current_byte != 0xFF {
                scan_data.push(current_byte);
                current_byte = match self.reader.read_u8() {
                    Ok(byte) => byte,
                    Err(_) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                };

                continue;
            }

            // We have 0xFF byte, read the next one
            let next_byte = match self.reader.read_u8() {
                Ok(byte) => byte,
                Err(_) => {
                    log_warn!("Unexpected EOF while reading scan data, breaking");
                    break;
                }
            };

            match next_byte {
                0x00 => {
                    // Stuffed byte: for Huffman, de-stuff (emit 0xFF, discard 0x00)
                    // For arithmetic, pass both bytes raw so the arithmetic decoder handles them
                    scan_data.push(current_byte);
                    if is_arithmetic {
                        scan_data.push(0x00);
                    }
                    current_byte = match self.reader.read_u8() {
                        Ok(byte) => byte,
                        Err(_) => {
                            log_warn!("Unexpected EOF while reading scan data, breaking");
                            break;
                        }
                    };
                }
                0xFF => {
                    // Another FF, reprocess it
                    current_byte = next_byte;
                }
                b if b >= (JpegMarker::RST0.to_u16() & 0xFF) as u8
                    && b <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 =>
                    {
                        // Restart marker: for arithmetic, include marker bytes in stream
                        if is_arithmetic {
                            scan_data.push(0xFF);
                            scan_data.push(b);
                        }
                        current_byte = match self.reader.read_u8() {
                            Ok(byte) => byte,
                            Err(_) => {
                                log_warn!("Unexpected EOF while reading scan data, breaking");
                                break;
                            }
                        };
                    }
                b if b == (JpegMarker::EOI.to_u16() & 0xFF) as u8 => {
                    self.reader.seek(SeekFrom::Current(-2))?;
                    break;
                }
                _ => {
                    // Any other marker - end of scan
                    self.reader.seek(SeekFrom::Current(-2))?;
                    break;
                }
            }
        }

        let data_length = scan_data.len() as u64;
        let scan = ScanData {
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            components: scan_components.clone(),
            dc_tables: self.dc_huffman_tables.clone(),
            ac_tables: self.ac_huffman_tables.clone(),
            arith_dc_tables: self.dc_arithmetic_tables.clone(),
            arith_ac_tables: self.ac_arithmetic_tables.clone(),
            data: scan_data,
        };

        self.scans.push(scan);

        self.record_segment(segment_start, "SOS", JpegSegmentData::SOS(SOSData {
            length,
            component_count: scan_component_count,
            components: scan_components,
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            dc_tables: self.dc_huffman_tables.clone(),
            ac_tables: self.ac_huffman_tables.clone(),
            data_length,
        }));

        Ok(())
    }

    fn default_lossless_dc_table() -> HuffmanTable {
        let mut t = HuffmanTable {
            class: 0,
            id: 0,
            offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            codes: vec![0b000, 0b010, 0b011, 0b100, 0b101, 0b110, 0b1110, 0b11110, 0b111110, 0b1111110, 0b11111110, 0b111111110],
            first_code: vec![u32::MAX, u32::MAX, 0b000, 0b011, u32::MAX, 0b100, 0b101, 0b110, 0b1110, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX],
            fast_lookup: Vec::new(),
        };
        t.build_fast_lookup();
        t
    }

    #[inline(always)]
    fn get_next_symbol(reader: &mut JpegBitReader<'_>, table: &HuffmanTable) -> u8 {
        if let Some(peek) = reader.peek9() {
            let entry = unsafe { *table.fast_lookup.get_unchecked(peek as usize) };
            if entry >> 16 != 0 {
                reader.consume(entry & 0xFF);
                return ((entry >> 8) & 0xFF) as u8;
            }
            reader.consume(9);
            let mut code = peek;
            for i in 9..16 {
                code = (code << 1) | reader.read_bits(1);
                let first = unsafe { *table.first_code.get_unchecked(i) };
                if first != u32::MAX {
                    let count = unsafe {
                        table.offsets.get_unchecked(i + 1).wrapping_sub(*table.offsets.get_unchecked(i))
                    } as usize;
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

    fn decode_mcu(
        &self,
        reader: &mut JpegBitReader<'_>,
        mcu_component: &mut [i32; 64],
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        previous_dc: &mut i32,
        is_differential: bool,
    ) -> VexelResult<()> {
        let length = Self::get_next_symbol(reader, dc_table);

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
            let v = dc_coeff + *previous_dc;
            *previous_dc = v;
            v
        };
        unsafe { *mcu_component.get_unchecked_mut(0) = dc_value; }

        let mut i = 1usize;
        while i < 64 {
            let symbol = Self::get_next_symbol(reader, ac_table);

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
                coefficient_length = 0;
            }

            if coefficient_length != 0 {
                let mut coefficient = reader.read_bits(coefficient_length as u32) as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                unsafe { *mcu_component.get_unchecked_mut(*ZIGZAG_MAP.get_unchecked(i) as usize) = coefficient; }
                i += 1;
            }
        }

        Ok(())
    }

    fn decode_progressive(&mut self) -> VexelResult<Image> {
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                let comp_width = mcu_width * 8 * comp.horizontal_sampling_factor as u32;
                let comp_height = mcu_height * 8 * comp.vertical_sampling_factor as u32;

                ComponentPlane::new(comp_width, comp_height)
            })
            .collect();

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut component_planes)?,
        }
        self.dequantize_and_idct_planes(&mut component_planes)?;

        let mut pixel_data = self.upsample_and_convert(&component_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut previous_dc = vec![0i32; planes.len()];

        for scan in &self.scans {
            let mut reader = JpegBitReader::new(scan.data.as_slice());
            let mut skips = 0;
            let restart_interval = self.restart_interval;

            let mut max_h_samp = self
                .components
                .iter()
                .map(|c| c.horizontal_sampling_factor)
                .max()
                .unwrap_or(1);
            let mut max_v_samp = self
                .components
                .iter()
                .map(|c| c.vertical_sampling_factor)
                .max()
                .unwrap_or(1);

            let is_non_interleaved = scan.components.len() == 1;

            if is_non_interleaved {
                max_h_samp = 1;
                max_v_samp = 1;
            }

            let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
            let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

            struct ScanCompInfo {
                plane_index: usize,
                h_blocks: u8,
                v_blocks: u8,
                dc_table_index: Option<usize>,
                ac_table_index: Option<usize>,
            }

            let scan_comp_infos: Vec<ScanCompInfo> = scan.components.iter().filter_map(|scan_comp| {
                let (plane_index, comp) = match self
                    .components
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.id == scan_comp.component_id)
                {
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

                let dc_table_index = scan.dc_tables.iter().position(|t| t.id == scan_comp.dc_table_selector);
                let ac_table_index = scan.ac_tables.iter().position(|t| t.id == scan_comp.ac_table_selector);

                if scan.start_spectral == 0 && scan.successive_high == 0 && dc_table_index.is_none() {
                    log_warn!("DC table not found: {}", scan_comp.dc_table_selector);
                }
                if scan.end_spectral > 0 && ac_table_index.is_none() {
                    log_warn!("AC table not found: {}", scan_comp.ac_table_selector);
                }

                Some(ScanCompInfo { plane_index, h_blocks, v_blocks, dc_table_index, ac_table_index })
            }).collect();

            let mut restart_counter = restart_interval;

            for mcu_y in 0..mcu_height {
                for mcu_x in 0..mcu_width {
                    if restart_interval > 0 {
                        if restart_counter == 0 {
                            previous_dc.fill(0);
                            reader.clear_buffer();
                            restart_counter = restart_interval;
                        }
                        restart_counter = restart_counter.saturating_sub(1);
                    }

                    for info in &scan_comp_infos {
                        let plane_index = info.plane_index;
                        let h_blocks = info.h_blocks;
                        let v_blocks = info.v_blocks;
                        let plane_blocks_per_line = planes[plane_index].blocks_per_line;

                        for v in 0..v_blocks {
                            for h in 0..h_blocks {
                                let block_x = mcu_x * h_blocks as u32 + h as u32;
                                let block_y = mcu_y * v_blocks as u32 + v as u32;

                                if block_x >= plane_blocks_per_line {
                                    continue;
                                }

                                if let Some(component_data) = planes[plane_index].get_block_mut(block_x, block_y) {
                                    assert_eq!(component_data.len(), 64);
                                    if scan.start_spectral == 0 {
                                        if scan.successive_high == 0 {
                                            // First DC scan
                                            let dc_table = match info.dc_table_index.and_then(|i| scan.dc_tables.get(i)) {
                                                Some(table) => table,
                                                None => {
                                                    log_warn!("DC table missing for block, skipping");
                                                    continue;
                                                }
                                            };

                                            let length = Self::get_next_symbol(&mut reader, dc_table);

                                            if length > 15 {
                                                log_warn!("Invalid DC coefficient length (>15): {}", length);
                                                continue;
                                            }

                                            let bits = reader.read_bits(length as u32);

                                            let mut value = bits as i32;

                                            if length != 0 && value < (1 << (length - 1)) {
                                                value -= (1 << length) - 1;
                                            }

                                            let is_differential = matches!(self.mode, JpegMode::DifferentialProgressive | JpegMode::DifferentialSequential);
                                            if is_differential {
                                                previous_dc[plane_index] = value;
                                            } else {
                                                value += previous_dc[plane_index];
                                                previous_dc[plane_index] = value;
                                            }
                                            component_data[0] = value << scan.successive_low;
                                        } else {
                                            // Refining DC scan
                                            let bit = reader.read_bits(1);

                                            component_data[0] |= (bit as i32) << scan.successive_low;
                                        }
                                    }

                                    if scan.end_spectral > 0 {
                                        if scan.successive_high == 0 {
                                            // First AC scan
                                            if skips > 0 {
                                                skips -= 1;
                                                continue;
                                            }

                                            let ac_table = match info.ac_table_index.and_then(|i| scan.ac_tables.get(i)) {
                                                Some(table) => table,
                                                None => {
                                                    log_warn!("AC table missing for block, skipping");
                                                    continue;
                                                }
                                            };

                                            let mut k = scan.start_spectral as usize;
                                            while k <= scan.end_spectral as usize {
                                                let symbol = Self::get_next_symbol(&mut reader, ac_table);

                                                let num_zeros = symbol >> 4;
                                                let length = symbol & 0xF;

                                                if length != 0 {
                                                    if k + num_zeros as usize > 63 {
                                                        log_warn!(
                                                            "Zero run-length exceeded spectral selection: {}",
                                                            k + num_zeros as usize
                                                        );
                                                        break;
                                                    }

                                                    for _ in 0..num_zeros {
                                                        component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                        k += 1;
                                                    }

                                                    let max_ac_len = if self.precision > 8 { 15 } else { 10 };
                                                    if length > max_ac_len {
                                                        log_warn!("Invalid AC coefficient length (>{}): {}", max_ac_len, length);
                                                        break;
                                                    }

                                                    let bits = reader.read_bits(length as u32);
                                                    let mut value = bits as i32;

                                                    if value < (1 << (length - 1)) {
                                                        value -= (1 << length) - 1;
                                                    }

                                                    let zigzag_idx = ZIGZAG_MAP[k] as usize;
                                                    component_data[zigzag_idx] = value << scan.successive_low;
                                                    k += 1;
                                                } else {
                                                    if num_zeros == 15 {
                                                        if k + num_zeros as usize > scan.end_spectral as usize {
                                                            log_warn!(
                                                                "Zero run-length exceeded spectral selection: {}",
                                                                k + num_zeros as usize
                                                            );
                                                            break;
                                                        }

                                                        for _ in 0..num_zeros {
                                                            component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                            k += 1;
                                                        }
                                                    } else {
                                                        skips = (1 << num_zeros) - 1;
                                                        skips += reader.read_bits(num_zeros as u32);
                                                        break;
                                                    }

                                                    k += 1;
                                                }
                                            }
                                        } else {
                                            // Refining AC scan
                                            let positive = 1 << scan.successive_low;
                                            let negative = -1 << scan.successive_low;
                                            let mut k = scan.start_spectral as usize;

                                            if skips == 0 {
                                                let ac_table = match info.ac_table_index.and_then(|i| scan.ac_tables.get(i)) {
                                                    Some(table) => table,
                                                    None => {
                                                        log_warn!("AC table missing for block (refining), skipping");
                                                        continue;
                                                    }
                                                };

                                                while k <= scan.end_spectral as usize {
                                                    let symbol = Self::get_next_symbol(&mut reader, ac_table);

                                                    let mut num_zeros = symbol >> 4;
                                                    let length = symbol & 0xF;
                                                    let mut coefficient = 0;

                                                    if length != 0 {
                                                        if length != 1 {
                                                            log_warn!(
                                                                "Invalid AC coefficient length (refining): {}",
                                                                length
                                                            );
                                                            break;
                                                        }

                                                        coefficient = if reader.read_bits(1) != 0 { positive } else { negative };
                                                    } else {
                                                        if num_zeros != 15 {
                                                            skips = 1 << num_zeros;
                                                            skips += reader.read_bits(num_zeros as u32);
                                                            break;
                                                        }
                                                    }

                                                    loop {
                                                        let val = unsafe { component_data.get_unchecked_mut(ZIGZAG_MAP[k] as usize) };
                                                        if *val != 0 {
                                                            if reader.read_bits(1) == 1 {
                                                                if *val & positive == 0 {
                                                                    if *val >= 0 {
                                                                        *val += positive;
                                                                    } else {
                                                                        *val += negative;
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            if num_zeros == 0 {
                                                                break;
                                                            }
                                                            num_zeros -= 1;
                                                        }

                                                        k += 1;

                                                        if k > scan.end_spectral as usize {
                                                            break;
                                                        }
                                                    }

                                                    if coefficient != 0 && k <= scan.end_spectral as usize {
                                                        unsafe { *component_data.get_unchecked_mut(ZIGZAG_MAP[k] as usize) = coefficient; }
                                                    }

                                                    k += 1;
                                                }
                                            }

                                            if skips > 0 {
                                                while k <= scan.end_spectral as usize {
                                                    let val = unsafe { component_data.get_unchecked_mut(ZIGZAG_MAP[k] as usize) };
                                                    if *val != 0 {
                                                        if reader.read_bits(1) == 1 {
                                                            if *val & positive == 0 {
                                                                if *val >= 0 {
                                                                    *val += positive;
                                                                } else {
                                                                    *val += negative;
                                                                }
                                                            }
                                                        }
                                                    }

                                                    k += 1;
                                                }

                                                skips -= 1;
                                            }
                                        }
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

    fn decode_differences(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let mut reader = JpegBitReader::new(scan.data.as_slice());

        let mut differences: Vec<Vec<i32>> = vec![vec![]; scan.components.len()];

        let width = self.width as usize;
        let height = self.height as usize;

        for diffs in &mut differences {
            diffs.reserve(width * height);
        }

        // TODO handle restarts
        for _ in 0..height {
            for _ in 0..width {
                for (i, scan_component) in scan.components.iter().enumerate() {
                    let dc_table = match scan.dc_tables.iter().find(|t| t.id == scan_component.dc_table_selector) {
                        Some(table) => table,
                        None => {
                            log_warn!("No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.", i);
                            &Self::default_lossless_dc_table()
                        }
                    };

                    let bits_to_read = Self::get_next_symbol(&mut reader, dc_table);

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
                        16 => 32768,
                        _ => {
                            log_warn!("Invalid difference: {}", bits_to_read);
                            0
                        }
                    };

                    differences[i].push(diff);
                }
            }
        }

        Ok(differences)
    }

    fn decode_differences_arithmetic(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let num_components = scan.components.len();
        let width = self.width as usize;
        let height = self.height as usize;

        let mut differences: Vec<Vec<i32>> = vec![vec![0i32; width * height]; num_components];

        let l_values: Vec<u8> = scan.components.iter().map(|sc| {
            scan.arith_dc_tables
                .iter()
                .find(|t| t.identifier == sc.dc_table_selector)
                .and_then(|t| t.values.first())
                .map(|v| v.length)
                .unwrap_or(0)
        }).collect();

        let u_values: Vec<u8> = scan.components.iter().map(|sc| {
            scan.arith_dc_tables
                .iter()
                .find(|t| t.identifier == sc.dc_table_selector)
                .and_then(|t| t.values.first())
                .map(|v| v.value)
                .unwrap_or(1)
        }).collect();

        let ctx_table_indices: Vec<usize> = scan.components.iter().map(|sc| {
            (sc.dc_table_selector & 0x3) as usize
        }).collect();

        let mut all_contexts = vec![[0u8; 160]; 4];
        let mut arith = ArithmeticDecoder::new(&scan.data);

        let mut da: Vec<i32> = vec![0i32; num_components];
        let mut db: Vec<Vec<i32>> = vec![vec![0i32; width]; num_components];

        for y in 0..height {
            for c in 0..num_components {
                da[c] = 0;
            }

            for x in 0..width {
                for c in 0..num_components {
                    let l = l_values[c];
                    let u = u_values[c];
                    let ctx_idx = ctx_table_indices[c];

                    let v = arith.decode_lossless_difference(
                        &mut all_contexts[ctx_idx],
                        da[c],
                        db[c][x],
                        l,
                        u,
                    );

                    differences[c][y * width + x] = v;
                    da[c] = v;
                    db[c][x] = v;
                }
            }
        }

        Ok(differences)
    }

    fn predict(
        ra: i32,
        rb: i32,
        rc: i32,
        predictor: Predictor,
        point_transform: u8,
        input_precision: u8,
        x: usize,
        y: usize,
    ) -> i32 {
        // TODO handle restarts as well
        if x == 0 && y == 0 {
            if input_precision > point_transform + 1 {
                1 << (input_precision - point_transform - 1)
            } else {
                0
            }
        } else if y == 0 {
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
        &self,
        differences: Vec<Vec<i32>>,
        predictor: Predictor,
        point_transform: u8,
    ) -> VexelResult<Vec<Vec<u16>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = differences.len();

        let mut samples = vec![vec![0u16; width * height]; components_count];

        if predictor == Predictor::Ra {
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
                    for component_index in 0..components_count {
                        let index = y * width + x;
                        let diff = differences[component_index][index];

                        let ra = if x > 0 {
                            samples[component_index][index - 1] as i32
                        } else {
                            0
                        };
                        let rb = if y > 0 {
                            samples[component_index][(y - 1) * width + x] as i32
                        } else {
                            0
                        };
                        let rc = if x > 0 && y > 0 {
                            samples[component_index][(y - 1) * width + (x - 1)] as i32
                        } else {
                            0
                        };

                        let prediction =
                            Self::predict(ra, rb, rc, predictor.clone(), point_transform, self.precision, x, y);

                        samples[component_index][index] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        }

        Ok(samples)
    }

    fn samples_to_image(&self, samples: Vec<Vec<u16>>) -> VexelResult<Image> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = samples.len();

        let mut output: Vec<u16> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let pixel_pos = y * width + x;

                for component_index in 0..components_count {
                    let sample = samples[component_index][pixel_pos];
                    output.push(sample);
                }
            }
        }

        if components_count == 1 {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L16(pixels), 0)])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::L8
                } else {
                    PixelFormat::L16
                },
                frames,
            ))
        } else if components_count == 4 {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::RGBA8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(
                    width as u32,
                    height as u32,
                    PixelData::RGBA16(pixels),
                    0,
                )])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::RGBA8
                } else {
                    PixelFormat::RGBA16
                },
                frames,
            ))
        } else {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::RGB8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(
                    width as u32,
                    height as u32,
                    PixelData::RGB16(pixels),
                    0,
                )])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::RGB8
                } else {
                    PixelFormat::RGB16
                },
                frames,
            ))
        }
    }

    fn decode_lossless(&mut self) -> VexelResult<Image> {
        // TODO there can be multiple scans in lossless mode somehow
        let scan = match self.scans.first() {
            Some(s) => s.clone(),
            None => {
                return Err(VexelError::from(Error::new(
                    ErrorKind::InvalidData,
                    "No scan data found",
                )))
            }
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
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        let samples = self.reconstruct_samples(differences, predictor, point_transform)?;

        self.samples_to_image(samples)
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
            scan.arith_dc_tables.iter().find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first()).map(|v| v.length).unwrap_or(0)
        };
        let get_dc_u = |tbl: usize| -> u8 {
            scan.arith_dc_tables.iter().find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first()).map(|v| v.value).unwrap_or(1)
        };
        let get_ac_k = |tbl: usize| -> u8 {
            scan.arith_ac_tables.iter().find(|t| t.identifier as usize == tbl)
                .and_then(|t| t.values.first()).map(|v| v.value).unwrap_or(5)
        };

        let mut arith = ArithmeticDecoder::new(&scan.data);

        let num_components = planes.len();
        let mut dc_context = vec![0usize; num_components];
        let mut last_dc_val = vec![0i32; num_components];
        let mut dc_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 64]).collect();
        let mut ac_stats: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 256]).collect();

        let is_non_interleaved = components.len() == 1;
        let max_h_samp = if is_non_interleaved { 1 } else { self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1) };
        let max_v_samp = if is_non_interleaved { 1 } else { self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1) };

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut restart_counter = self.restart_interval as u32;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        dc_context.fill(0);
                        last_dc_val.fill(0);
                        for s in dc_stats.iter_mut() { s.fill(0); }
                        for s in ac_stats.iter_mut() { s.fill(0); }
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
                                    matches!(self.mode, JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive),
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

        for scan in &self.scans {
            let is_dc_scan = scan.start_spectral == 0;
            let is_first_scan = scan.successive_high == 0;
            let is_first_dc_scan = is_dc_scan && is_first_scan;

            for scan_comp in &scan.components {
                let (plane_idx, _comp) = match self
                    .components
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.id == scan_comp.component_id)
                {
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
                if !is_dc_scan {
                    if ac_sel < per_scan_ac_stats.len() {
                        per_scan_ac_stats[ac_sel].fill(0);
                    }
                }
            }

            let get_dc_l = |tbl: usize| -> u8 {
                scan.arith_dc_tables.iter().find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first()).map(|v| v.length).unwrap_or(0)
            };
            let get_dc_u = |tbl: usize| -> u8 {
                scan.arith_dc_tables.iter().find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first()).map(|v| v.value).unwrap_or(1)
            };
            let get_ac_k = |tbl: usize| -> u8 {
                scan.arith_ac_tables.iter().find(|t| t.identifier as usize == tbl)
                    .and_then(|t| t.values.first()).map(|v| v.value).unwrap_or(5)
            };

            let mut arith = ArithmeticDecoder::new(&scan.data);

            let is_non_interleaved = scan.components.len() == 1;
            let max_h_samp = if is_non_interleaved { 1 } else {
                self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1)
            };
            let max_v_samp = if is_non_interleaved { 1 } else {
                self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1)
            };

            let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
            let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

            let restart_interval = self.restart_interval;
            let mut restart_counter = restart_interval as u32;

            for mcu_y in 0..mcu_height {
                for mcu_x in 0..mcu_width {
                    if restart_interval > 0 {
                        if restart_counter == 0 {
                            per_scan_dc_context.fill(0);
                            per_scan_last_dc_val.fill(0);
                            for s in per_scan_dc_stats.iter_mut() { s.fill(0); }
                            for s in per_scan_ac_stats.iter_mut() { s.fill(0); }
                            arith.reset();
                            restart_counter = restart_interval as u32;
                        }
                        restart_counter = restart_counter.saturating_sub(1);
                    }

                    for scan_comp in scan.components.iter() {
                        let (plane_index, comp) = match self
                            .components
                            .iter()
                            .enumerate()
                            .find(|(_, c)| c.id == scan_comp.component_id)
                        {
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
                                                matches!(self.mode, JpegMode::DifferentialProgressive | JpegMode::DifferentialSequential),
                                            );
                                        } else {
                                            arith.decode_dc_refine(
                                                block,
                                                scan.successive_low,
                                            );
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

    fn decode_huffman_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.len() < 1 {
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
            self.components
                .iter()
                .map(|c| c.horizontal_sampling_factor)
                .max()
                .unwrap_or(1)
        };
        let mut max_v_samp = if is_non_interleaved {
            1
        } else {
            self.components
                .iter()
                .map(|c| c.vertical_sampling_factor)
                .max()
                .unwrap_or(1)
        };

        if max_h_samp == 0 || max_v_samp == 0 {
            log_warn!("Invalid sampling factors: ({}, {})", max_h_samp, max_v_samp);
            max_h_samp = 1;
            max_v_samp = 1;
        }

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let default_dc_table = Self::default_lossless_dc_table();
        let default_ac_table = Self::default_lossless_dc_table();

        struct BaselineCompInfo {
            h_samp: u8,
            v_samp: u8,
            dc_table: HuffmanTable,
            ac_table: HuffmanTable,
        }

        let comp_infos: Vec<BaselineCompInfo> = self.components.iter().enumerate().filter_map(|(comp_idx, comp)| {
            if self.scans[0].components.len() <= comp_idx {
                log_warn!("Component index out of bounds: {} {}", self.scans[0].components.len(), comp_idx);
                return None;
            }

            let dc_selector = self.scans[0].components[comp_idx].dc_table_selector;
            let ac_selector = self.scans[0].components[comp_idx].ac_table_selector;

            let dc_table = self.scans[0].dc_tables.iter().find(|t| t.id == dc_selector).cloned().unwrap_or_else(|| {
                log_warn!("DC table {} not found in baseline mode, substituting default, image will be corrupted.", dc_selector);
                default_dc_table.clone()
            });

            let ac_table = self.scans[0].ac_tables.iter().find(|t| t.id == ac_selector).cloned().unwrap_or_else(|| {
                log_warn!("AC table {} not found in baseline mode, substituting default, image will be corrupted.", ac_selector);
                default_ac_table.clone()
            });

            let h_samp = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
            let v_samp = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

            Some(BaselineCompInfo { h_samp, v_samp, dc_table, ac_table })
        }).collect();

        let mut restart_counter = self.restart_interval as u32;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
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
                                let is_differential = matches!(self.mode, JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive);
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

    fn dequantize_and_idct_planes(&self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let level_shift = if self.precision <= 8 { 128i32 } else { 2048i32 };

        let default_table = QuantizationTable {
            id: 0,
            precision: 8,
            length: 64,
            table: DEFAULT_QUANTIZATION_TABLE.to_vec(),
        };

        for (comp_idx, plane) in planes.iter_mut().enumerate() {
            let quant_data: &[u16] = self
                .components
                .get(comp_idx)
                .and_then(|comp| {
                    self.quantization_tables
                        .iter()
                        .find(|q| q.id == comp.quantization_table_id)
                })
                .map(|t| t.table.as_slice())
                .unwrap_or_else(|| {
                    log_warn!("Quantization table not found for component, substituting default one.");
                    default_table.table.as_slice()
                });

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                plane.data.par_chunks_mut(64 * 8).for_each(|chunk| {
                    dequantize_and_idct(chunk, quant_data, level_shift);
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                dequantize_and_idct(&mut plane.data, quant_data, level_shift);
            }
        }

        Ok(())
    }

    fn upsample_and_convert(&self, planes: &[ComponentPlane]) -> VexelResult<PixelData> {
        use crate::decoders::jpeg::upsample as up;

        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

        let tw = self.width as usize;
        let th = self.height as usize;
        let npixels = tw * th;

        let source_dims: Vec<(usize, usize)> = self
            .components
            .iter()
            .map(|comp| {
                let sw = ((self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1)
                    / max_h_samp as u32) as usize;
                let sh = ((self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1)
                    / max_v_samp as u32) as usize;
                (sw, sh)
            })
            .collect();

        #[cfg(feature = "rayon")]
        let deinterleaved: Vec<Vec<i32>> = {
            use rayon::prelude::*;
            planes
                .par_iter()
                .zip(source_dims.par_iter())
                .map(|(plane, &(sw, sh))| plane.deinterleave(sw as u32, sh as u32))
                .collect()
        };

        #[cfg(not(feature = "rayon"))]
        let deinterleaved: Vec<Vec<i32>> = planes
            .iter()
            .zip(source_dims.iter())
            .map(|(plane, &(sw, sh))| plane.deinterleave(sw as u32, sh as u32))
            .collect();

        if deinterleaved.len() == 1 {
            let src = &deinterleaved[0];
            let (sw, sh) = source_dims[0];
            return if self.precision <= 8 {
                let mut pixels = Vec::with_capacity(npixels);
                if sw == tw && sh == th {
                    for i in 0..npixels.min(src.len()) {
                        pixels.push((src[i] + 128).clamp(0, 255) as u8);
                    }
                } else {
                    for i in 0..npixels.min(src.len()) {
                        pixels.push((src[i] + 128).clamp(0, 255) as u8);
                    }
                }
                pixels.resize(npixels, 0);
                Ok(PixelData::L8(pixels))
            } else {
                let mut pixels16 = Vec::with_capacity(npixels);
                for i in 0..npixels.min(src.len()) {
                    pixels16.push(((src[i] + 2048).clamp(0, 4095) as u16) << 4);
                }
                pixels16.resize(npixels, 0);
                Ok(PixelData::L16(pixels16))
            };
        }

        if deinterleaved.len() < 3 {
            log_warn!("Invalid number of planes for RGB conversion: {}.", deinterleaved.len());
        }

        let (y_sw, y_sh) = source_dims[0];
        let (c_sw, c_sh) = if deinterleaved.len() >= 2 { source_dims[1] } else { (tw, th) };

        let h_2x = tw == c_sw * 2 || tw == c_sw * 2 - 1;
        let v_2x = th == c_sh * 2 || th == c_sh * 2 - 1;
        let is_h2v1 = h_2x && th == c_sh;
        let is_h1v2 = !h_2x && v_2x && tw == c_sw;
        let is_h2v2 = h_2x && v_2x;
        let is_identity = c_sw == tw && c_sh == th;

        let y_plane = &deinterleaved[0];
        let cb_plane = if deinterleaved.len() > 1 { deinterleaved[1].as_slice() } else { &[] };
        let cr_plane = if deinterleaved.len() > 2 { deinterleaved[2].as_slice() } else { &[] };

        let _upsample_y_row = |dy: usize, tmp: &mut Vec<i32>| {
            if y_sw == tw && y_sh == th {
                let base = dy * tw;
                tmp.clear();
                if tmp.capacity() < tw {
                    tmp.reserve(tw);
                }
                tmp.extend_from_slice(&y_plane[base..base + tw.min(y_plane.len().saturating_sub(base))]);
                tmp.resize(tw, 0);
            } else {
                tmp.resize(tw, 0);
                let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                let sy_above = sy.saturating_sub(1);
                let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                let v = dy % 2;
                let sy_near = if v == 0 { sy_above } else { sy_below };
                let mut col_sums = vec![0i32; y_sw];
                let mut prev_col_sums = vec![0i32; y_sw];
                let src_row = &y_plane[sy * y_sw..(sy * y_sw + y_sw).min(y_plane.len())];
                let near_row = &y_plane[sy_near * y_sw..(sy_near * y_sw + y_sw).min(y_plane.len())];
                let prev_near = if v == 0 { sy_above } else { sy };
                let prev_near_row = &y_plane[prev_near * y_sw..(prev_near * y_sw + y_sw).min(y_plane.len())];
                up::compute_col_sums(src_row, near_row, &mut col_sums, y_sw.min(src_row.len()).min(near_row.len()));
                up::compute_col_sums(src_row, prev_near_row, &mut prev_col_sums, y_sw.min(src_row.len()).min(prev_near_row.len()));
                up::upsample_h2v2_row(&col_sums, &prev_col_sums, tmp, y_sw, tw);
            }
        };

        macro_rules! upsample_chroma_row {
            ($plane:expr, $dy:expr, $tmp:expr) => {{
                $tmp.resize(tw, 0);
                if is_identity {
                    let base = $dy * tw;
                    let avail = $plane.len().saturating_sub(base);
                    let copy_len = tw.min(avail);
                    $tmp[..copy_len].copy_from_slice(&$plane[base..base + copy_len]);
                    $tmp[copy_len..].fill(0);
                } else if is_h2v2 {
                    let sy = ($dy / 2).min(c_sh.saturating_sub(1));
                    let v = $dy % 2;
                    let sy_above = sy.saturating_sub(1);
                    let sy_below = (sy + 1).min(c_sh.saturating_sub(1));
                    let sy_near = if v == 0 { sy_above } else { sy_below };
                    let prev_near = if v == 0 { sy_above } else { sy };
                    let src_row = &$plane[sy * c_sw..(sy * c_sw + c_sw).min($plane.len())];
                    let near_row = &$plane[sy_near * c_sw..(sy_near * c_sw + c_sw).min($plane.len())];
                    let prev_near_row = &$plane[prev_near * c_sw..(prev_near * c_sw + c_sw).min($plane.len())];
                    let sw_eff = c_sw.min(src_row.len()).min(near_row.len());
                    let mut col_sums = vec![0i32; c_sw];
                    let mut prev_col_sums = vec![0i32; c_sw];
                    up::compute_col_sums(src_row, near_row, &mut col_sums, sw_eff);
                    up::compute_col_sums(src_row, prev_near_row, &mut prev_col_sums, sw_eff.min(prev_near_row.len()));
                    up::upsample_h2v2_row(&col_sums, &prev_col_sums, &mut $tmp, c_sw, tw);
                } else if is_h2v1 {
                    let sy = $dy.min(c_sh.saturating_sub(1));
                    let src_row = &$plane[sy * c_sw..(sy * c_sw + c_sw).min($plane.len())];
                    up::upsample_h2v1_row(src_row, &mut $tmp, c_sw.min(src_row.len()), tw);
                } else if is_h1v2 {
                    let sy = ($dy / 2).min(c_sh.saturating_sub(1));
                    let v = $dy % 2;
                    let sy_near = if v == 0 { sy.saturating_sub(1) } else { (sy + 1).min(c_sh.saturating_sub(1)) };
                    let bias = if v == 0 { 1i32 } else { 2i32 };
                    let src_row = &$plane[sy * c_sw..(sy * c_sw + c_sw).min($plane.len())];
                    let neighbor_row = &$plane[sy_near * c_sw..(sy_near * c_sw + c_sw).min($plane.len())];
                    let sw_eff = c_sw.min(src_row.len()).min(neighbor_row.len()).min($tmp.len());
                    up::upsample_h1v2_row(src_row, neighbor_row, bias, &mut $tmp[..sw_eff.min(tw)], sw_eff);
                } else {
                    for x in 0..tw {
                        let fx = (x as f32 + 0.5) * c_sw as f32 / tw as f32 - 0.5;
                        let fy = ($dy as f32 + 0.5) * c_sh as f32 / th as f32 - 0.5;
                        let x0 = (fx.floor() as i64).clamp(0, c_sw as i64 - 1) as usize;
                        let y0 = (fy.floor() as i64).clamp(0, c_sh as i64 - 1) as usize;
                        let x1 = (x0 + 1).min(c_sw.saturating_sub(1));
                        let y1 = (y0 + 1).min(c_sh.saturating_sub(1));
                        let wx = fx - fx.floor();
                        let wy = fy - fy.floor();
                        let get = |px: usize, py: usize| -> i32 {
                            let idx = py * c_sw + px;
                            if idx < $plane.len() { $plane[idx] } else { 0 }
                        };
                        let v = (1.0 - wy) * ((1.0 - wx) * get(x0, y0) as f32 + wx * get(x1, y0) as f32)
                            + wy * ((1.0 - wx) * get(x0, y1) as f32 + wx * get(x1, y1) as f32);
                        $tmp[x] = v.round() as i32;
                    }
                }
            }};
        }

        let convert_ycbcr = self.should_convert_ycbcr();

        macro_rules! fill_row_8 {
            ($dy:expr, $y_row:expr, $cb_row:expr, $cr_row:expr, $dst:expr) => {{
                upsample_chroma_row!(cb_plane, $dy, $cb_row);
                upsample_chroma_row!(cr_plane, $dy, $cr_row);
                for col in 0..tw {
                    let c0 = $y_row.get(col).copied().unwrap_or(0);
                    let c1 = $cb_row.get(col).copied().unwrap_or(0);
                    let c2 = $cr_row.get(col).copied().unwrap_or(0);
                    if convert_ycbcr {
                        let y128 = (c0 + 128) << 16;
                        $dst[col * 3]     = ((y128 + 91881 * c2 + 32768) >> 16).clamp(0, 255) as u8;
                        $dst[col * 3 + 1] = ((y128 - 22554 * c1 - 46802 * c2 + 32768) >> 16).clamp(0, 255) as u8;
                        $dst[col * 3 + 2] = ((y128 + 116130 * c1 + 32768) >> 16).clamp(0, 255) as u8;
                    } else {
                        $dst[col * 3]     = (c0 + 128).clamp(0, 255) as u8;
                        $dst[col * 3 + 1] = (c1 + 128).clamp(0, 255) as u8;
                        $dst[col * 3 + 2] = (c2 + 128).clamp(0, 255) as u8;
                    }
                }
            }};
        }

        macro_rules! fill_row_16 {
            ($dy:expr, $y_row:expr, $cb_row:expr, $cr_row:expr, $dst:expr) => {{
                upsample_chroma_row!(cb_plane, $dy, $cb_row);
                upsample_chroma_row!(cr_plane, $dy, $cr_row);
                for col in 0..tw {
                    let c0 = $y_row.get(col).copied().unwrap_or(0);
                    let c1 = $cb_row.get(col).copied().unwrap_or(0);
                    let c2 = $cr_row.get(col).copied().unwrap_or(0);
                    if convert_ycbcr {
                        let y2048 = (c0 + 2048) << 16;
                        $dst[col * 3]     = (((y2048 + 91881 * c2 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        $dst[col * 3 + 1] = (((y2048 - 22554 * c1 - 46802 * c2 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        $dst[col * 3 + 2] = (((y2048 + 116130 * c1 + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                    } else {
                        $dst[col * 3]     = ((c0 + 2048).clamp(0, 65535) as u16) << 4;
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
                pixels.par_chunks_mut(tw * 3).enumerate().for_each(|(dy, dst)| {
                    let mut y_row = vec![0i32; tw];
                    let mut cb_row = vec![0i32; tw];
                    let mut cr_row = vec![0i32; tw];

                    if y_sw == tw && y_sh == th {
                        let base = dy * tw;
                        let copy_len = tw.min(y_plane.len().saturating_sub(base));
                        y_row[..copy_len].copy_from_slice(&y_plane[base..base + copy_len]);
                    } else {
                        let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                        let v = dy % 2;
                        let sy_above = sy.saturating_sub(1);
                        let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                        let sy_near = if v == 0 { sy_above } else { sy_below };
                        let prev_near = if v == 0 { sy_above } else { sy };
                        let src_row = &y_plane[sy * y_sw..(sy * y_sw + y_sw).min(y_plane.len())];
                        let near_row = &y_plane[sy_near * y_sw..(sy_near * y_sw + y_sw).min(y_plane.len())];
                        let prev_near_row = &y_plane[prev_near * y_sw..(prev_near * y_sw + y_sw).min(y_plane.len())];
                        let sw_eff = y_sw.min(src_row.len()).min(near_row.len());
                        let mut col_sums = vec![0i32; y_sw];
                        let mut prev_col_sums = vec![0i32; y_sw];
                        up::compute_col_sums(src_row, near_row, &mut col_sums, sw_eff);
                        up::compute_col_sums(src_row, prev_near_row, &mut prev_col_sums, sw_eff.min(prev_near_row.len()));
                        up::upsample_h2v2_row(&col_sums, &prev_col_sums, &mut y_row, y_sw, tw);
                    }

                    fill_row_8!(dy, y_row, cb_row, cr_row, dst);
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                let mut y_row = vec![0i32; tw];
                let mut cb_row = vec![0i32; tw];
                let mut cr_row = vec![0i32; tw];
                for dy in 0..th {
                    _upsample_y_row(dy, &mut y_row);
                    let base = dy * tw * 3;
                    fill_row_8!(dy, y_row, cb_row, cr_row, pixels[base..base + tw * 3]);
                }
            }

            Ok(PixelData::RGB8(pixels))
        } else {
            let mut pixels16 = vec![0u16; npixels * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                pixels16.par_chunks_mut(tw * 3).enumerate().for_each(|(dy, dst)| {
                    let mut y_row = vec![0i32; tw];
                    let mut cb_row = vec![0i32; tw];
                    let mut cr_row = vec![0i32; tw];

                    if y_sw == tw && y_sh == th {
                        let base = dy * tw;
                        let copy_len = tw.min(y_plane.len().saturating_sub(base));
                        y_row[..copy_len].copy_from_slice(&y_plane[base..base + copy_len]);
                    } else {
                        let sy = (dy * y_sh / th).min(y_sh.saturating_sub(1));
                        let v = dy % 2;
                        let sy_above = sy.saturating_sub(1);
                        let sy_below = (sy + 1).min(y_sh.saturating_sub(1));
                        let sy_near = if v == 0 { sy_above } else { sy_below };
                        let prev_near = if v == 0 { sy_above } else { sy };
                        let src_row = &y_plane[sy * y_sw..(sy * y_sw + y_sw).min(y_plane.len())];
                        let near_row = &y_plane[sy_near * y_sw..(sy_near * y_sw + y_sw).min(y_plane.len())];
                        let prev_near_row = &y_plane[prev_near * y_sw..(prev_near * y_sw + y_sw).min(y_plane.len())];
                        let sw_eff = y_sw.min(src_row.len()).min(near_row.len());
                        let mut col_sums = vec![0i32; y_sw];
                        let mut prev_col_sums = vec![0i32; y_sw];
                        up::compute_col_sums(src_row, near_row, &mut col_sums, sw_eff);
                        up::compute_col_sums(src_row, prev_near_row, &mut prev_col_sums, sw_eff.min(prev_near_row.len()));
                        up::upsample_h2v2_row(&col_sums, &prev_col_sums, &mut y_row, y_sw, tw);
                    }

                    fill_row_16!(dy, y_row, cb_row, cr_row, dst);
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                let mut y_row = vec![0i32; tw];
                let mut cb_row = vec![0i32; tw];
                let mut cr_row = vec![0i32; tw];
                for dy in 0..th {
                    _upsample_y_row(dy, &mut y_row);
                    let base = dy * tw * 3;
                    fill_row_16!(dy, y_row, cb_row, cr_row, pixels16[base..base + tw * 3]);
                }
            }

            Ok(PixelData::RGB16(pixels16))
        }
    }

    fn decode_baseline(&mut self) -> VexelResult<Image> {
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                let comp_width = mcu_width * 8 * comp.horizontal_sampling_factor as u32;
                let comp_height = mcu_height * 8 * comp.vertical_sampling_factor as u32;

                ComponentPlane::new(comp_width, comp_height)
            })
            .collect();

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut component_planes)?,
        }

        self.dequantize_and_idct_planes(&mut component_planes)?;

        let mut pixel_data = self.upsample_and_convert(&component_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_frame_to_pixels(
        &mut self,
        frame: &HierarchicalFrame,
    ) -> VexelResult<Vec<Vec<i32>>> {
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
                let pixels: Vec<Vec<i32>> = samples.into_iter().map(|c| c.into_iter().map(|v| v as i32).collect()).collect();

                Ok(pixels)
            }

            JpegMode::Progressive | JpegMode::DifferentialProgressive => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
                let mcu_h = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

                let mut planes: Vec<ComponentPlane> = self.components.iter().map(|comp| {
                    ComponentPlane::new(
                        mcu_w * 8 * comp.horizontal_sampling_factor as u32,
                        mcu_h * 8 * comp.vertical_sampling_factor as u32,
                    )
                }).collect();

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut planes)?,
                }

                self.dequantize_and_idct_planes(&mut planes)?;
                
                Ok(self.planes_to_component_pixels(&planes))
            }

            _ => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
                let mcu_h = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

                let mut planes: Vec<ComponentPlane> = self.components.iter().map(|comp| {
                    ComponentPlane::new(
                        mcu_w * 8 * comp.horizontal_sampling_factor as u32,
                        mcu_h * 8 * comp.vertical_sampling_factor as u32,
                    )
                }).collect();

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut planes)?,
                }

                self.dequantize_and_idct_planes(&mut planes)?;
                let result = self.planes_to_component_pixels(&planes);

                Ok(result)
            }
        }
    }

    fn planes_to_component_pixels(&self, planes: &[ComponentPlane]) -> Vec<Vec<i32>> {
        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
        let tw = self.width as usize;
        let th = self.height as usize;

        planes.iter().zip(self.components.iter()).map(|(plane, comp)| {
            let sw = ((self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1) / max_h_samp as u32) as usize;
            let sh = ((self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1) / max_v_samp as u32) as usize;
            let raw = plane.deinterleave(sw as u32, sh as u32);

            if sw == tw && sh == th {
                raw
            } else {
                let mut out = vec![0i32; tw * th];
                for dy in 0..th {
                    for dx in 0..tw {
                        let sx = (dx * sw / tw).min(sw.saturating_sub(1));
                        let sy = (dy * sh / th).min(sh.saturating_sub(1));
                        out[dy * tw + dx] = raw[sy * sw + sx];
                    }
                }
                out
            }
        }).collect()
    }

    fn expand_2x(src: &[i32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<i32> {
        if src_w == dst_w && src_h == dst_h {
            return src.to_vec();
        }
        let expand_h = dst_w > src_w;
        let expand_v = dst_h > src_h;
        let h_w = if expand_h { dst_w } else { src_w };

        let mut h_buf = vec![0i32; h_w * src_h];
        if expand_h {
            for y in 0..src_h {
                let row = &src[y * src_w..(y + 1) * src_w];
                let out = &mut h_buf[y * h_w..(y + 1) * h_w];
                for x in 0..src_w {
                    let xi2 = x * 2;
                    let next = if x + 1 < src_w { row[x + 1] } else { row[x] };
                    if xi2 < h_w { out[xi2] = row[x]; }
                    if xi2 + 1 < h_w { out[xi2 + 1] = (row[x] + next) >> 1; }
                }
            }
        } else {
            h_buf.copy_from_slice(src);
        }

        let mut out = vec![0i32; dst_w * dst_h];
        if expand_v {
            for y in 0..dst_h {
                let sy = (y / 2).min(src_h - 1);
                let ny = (sy + 1).min(src_h - 1);
                if y % 2 == 0 {
                    out[y * dst_w..(y + 1) * dst_w].copy_from_slice(&h_buf[sy * h_w..(sy + 1) * h_w]);
                } else {
                    let ra = &h_buf[sy * h_w..(sy + 1) * h_w];
                    let rb = &h_buf[ny * h_w..(ny + 1) * h_w];
                    for x in 0..dst_w {
                        out[y * dst_w + x] = (ra[x] + rb[x]) >> 1;
                    }
                }
            }
        } else {
            out.copy_from_slice(&h_buf);
        }
        out
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

        for frame in frames.iter().skip(1) {
            let dst_w = frame.width as usize;
            let dst_h = frame.height as usize;

            let mut upsampled: Vec<Vec<i32>> = current_pixels.iter().map(|comp| {
                Self::expand_2x(comp, current_w, current_h, dst_w, dst_h)
            }).collect();

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

            current_pixels = upsampled;
            current_w = dst_w;
            current_h = dst_h;
        }

        self.hierarchical_frames = frames;

        let out_w = final_width as usize;
        let out_h = final_height as usize;

        if current_w != out_w || current_h != out_h {
            current_pixels = current_pixels.into_iter().map(|comp| {
                Self::expand_2x(&comp, current_w, current_h, out_w, out_h)
            }).collect();
        }

        let max_val = if precision <= 8 { 255i32 } else { 65535i32 };
        let npixels = out_w * out_h;
        let convert_ycbcr = self.should_convert_ycbcr();

        if nc == 1 {
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels];
                for i in 0..npixels {
                    pixels[i] = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u8;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels];
                for i in 0..npixels {
                    pixels[i] = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u16;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L16(pixels)))
            }
        } else if nc == 3 {
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels * 3];
                for i in 0..npixels {
                    let c0 = current_pixels[0].get(i).copied().unwrap_or(128);
                    let c1 = current_pixels[1].get(i).copied().unwrap_or(128);
                    let c2 = current_pixels[2].get(i).copied().unwrap_or(128);
                    if convert_ycbcr {
                        let cb = c1 - 128;
                        let cr = c2 - 128;
                        let y128 = c0 << 16;
                        pixels[i * 3]     = ((y128 + 91881 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 1] = ((y128 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 2] = ((y128 + 116130 * cb + 32768) >> 16).clamp(0, 255) as u8;
                    } else {
                        pixels[i * 3]     = c0.clamp(0, 255) as u8;
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
                        let cb = c1 - 2048;
                        let cr = c2 - 2048;
                        let y2048 = c0 << 16;
                        pixels[i * 3]     = (((y2048 + 91881 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 1] = (((y2048 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 2] = (((y2048 + 116130 * cb + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                    } else {
                        pixels[i * 3]     = (c0.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 1] = (c1.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 2] = (c2.clamp(0, 65535) as u16) << 4;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB16(pixels)))
            }
        } else {
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels * nc];
                for i in 0..npixels {
                    for c in 0..nc {
                        pixels[i * nc + c] = current_pixels[c].get(i).copied().unwrap_or(0).clamp(0, max_val) as u8;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels * nc];
                for i in 0..npixels {
                    for c in 0..nc {
                        pixels[i * nc + c] = current_pixels[c].get(i).copied().unwrap_or(0).clamp(0, max_val) as u16;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB16(pixels)))
            }
        }
    }

    fn read_dhp(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let precision = self.reader.read_u8()?;
        let height = self.reader.read_u16()? as u32;
        let width = self.reader.read_u16()? as u32;
        let component_count = self.reader.read_u8()?;

        for _ in 0..component_count {
            self.reader.read_u8()?;
            self.reader.read_u8()?;
            self.reader.read_u8()?;
        }

        self.is_hierarchical = true;
        self.dhp_width = width;
        self.dhp_height = height;

        self.record_segment(segment_start, "DHP", JpegSegmentData::SOF(crate::decoders::jpeg::types::SOFData {
            length,
            marker: "DHP".to_string(),
            precision,
            width,
            height,
            component_count,
            components: Vec::new(),
        }));

        Ok(())
    }

    fn read_exp(&mut self, _segment_start: u64) -> VexelResult<()> {
        let _length = self.reader.read_u16()?;
        let expand = self.reader.read_u8()?;
        self.pending_expand_h = (expand >> 4) & 1 != 0;
        self.pending_expand_v = expand & 1 != 0;
        Ok(())
    }

    fn finalize_current_frame(&mut self) {
        if self.scans.is_empty() {
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

    fn record_segment(&mut self, start_offset: u64, marker: &str, data: JpegSegmentData) {
        self.segments.push(JpegSegmentInfo {
            start_offset,
            marker: marker.to_string(),
            data,
        });
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            match marker {
                Some(marker) => {
                    log_debug!("Found marker: {:?}", marker);

                    let segment_start = self.reader.stream_position().unwrap_or(0).saturating_sub(2);

                    let result = match marker {
                        JpegMarker::SOI => {
                            self.record_segment(segment_start, "SOI", JpegSegmentData::SOI);
                            Ok(())
                        }
                        JpegMarker::EOI => {
                            self.record_segment(segment_start, "EOI", JpegSegmentData::EOI);
                            break;
                        }
                        JpegMarker::COM => self.read_com(segment_start),
                        JpegMarker::APP0 => self.read_app0_jfif(segment_start),
                        JpegMarker::APP1 => self.read_app1_exif(segment_start),
                        JpegMarker::APP14 => self.read_app14_adobe(segment_start),
                        JpegMarker::SOF0 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.read_start_of_frame("SOF0", segment_start)
                        }
                        JpegMarker::SOF1 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::ExtendedSequential;
                            self.read_start_of_frame("SOF1", segment_start)
                        }
                        JpegMarker::SOF2 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::Progressive;
                            self.read_start_of_frame("SOF2", segment_start)
                        }
                        JpegMarker::SOF3 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::Lossless;
                            self.read_start_of_frame("SOF3", segment_start)
                        }
                        JpegMarker::SOF5 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialSequential;
                            self.read_start_of_frame("SOF5", segment_start)
                        }
                        JpegMarker::SOF6 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialProgressive;
                            self.read_start_of_frame("SOF6", segment_start)
                        }
                        JpegMarker::SOF7 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialLossless;
                            self.read_start_of_frame("SOF7", segment_start)
                        }
                        JpegMarker::SOF9 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::ExtendedSequential;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF9", segment_start)
                        }
                        JpegMarker::SOF10 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::Progressive;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF10", segment_start)
                        }
                        JpegMarker::SOF11 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::Lossless;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF11", segment_start)
                        }
                        JpegMarker::SOF13 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialSequential;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF13", segment_start)
                        }
                        JpegMarker::SOF14 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialProgressive;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF14", segment_start)
                        }
                        JpegMarker::SOF15 => {
                            if self.is_hierarchical { self.finalize_current_frame(); }
                            self.mode = JpegMode::DifferentialLossless;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF15", segment_start)
                        }
                        JpegMarker::DHP => self.read_dhp(segment_start),
                        JpegMarker::EXP => self.read_exp(segment_start),
                        JpegMarker::DRI => self.read_restart_interval(segment_start),
                        JpegMarker::DQT => self.read_quantization_table(segment_start),
                        JpegMarker::DHT => self.read_huffman_table(segment_start),
                        JpegMarker::DAC => self.read_dac(segment_start),
                        JpegMarker::SOS => self.read_start_of_scan(segment_start),
                        _ => {
                            log_warn!("Unhandled marker found: {:?}", marker);
                            self.skip_unknown_marker_segment(&format!("{:?}", marker), segment_start)
                        }
                    };

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            log_warn!("Failed to process {:?} marker segment: {}", marker, e);
                        }
                    }
                }
                None => {
                    log_debug!("End of file reached");
                    break;
                }
            }
        }

        log_debug!(
            "Dimensions: {}x{}. Number of pixels: {}",
            self.width,
            self.height,
            self.width * self.height
        );
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

        if self.width == 0 || self.height == 0 || self.components.is_empty() {
            return Err(VexelError::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Invalid JPEG: dimensions={}x{}, components={}",
                    self.width,
                    self.height,
                    self.components.len()
                ),
            )));
        }

        if self.is_hierarchical {
            return self.decode_hierarchical();
        }

        match &self.mode {
            JpegMode::Baseline => {
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::ExtendedSequential => {
                // TODO general decoding process is same as baseline, so this method can be used,
                // but it would be nice to have a different name for this mode
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::Progressive => {
                let image = self.decode_progressive()?;
                Ok(image)
            }
            JpegMode::Lossless => {
                let image = self.decode_lossless()?;
                Ok(image)
            }
            JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive | JpegMode::DifferentialLossless => {
                log_warn!("Differential JPEG mode outside hierarchical context, treating as baseline");
                let image = self.decode_baseline()?;
                Ok(image)
            }
        }
    }
}
