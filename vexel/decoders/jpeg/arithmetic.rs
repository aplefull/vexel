use std::io::{Read, Seek};

use super::decoder::JpegDecoder;
use super::planes::ComponentPlane;
use crate::decoders::jpeg::types::{JpegMode, ZIGZAG_MAP};
use crate::log_warn;
use crate::utils::error::VexelResult;

#[rustfmt::skip]
pub(super) const ARITH_TABLE: [(u32, u8, u8, u8); 114] = [
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

pub(super) struct ArithmeticDecoder<'a> {
    pub(super) data: &'a [u8],
    pub(super) pos: usize,
    pub(super) c: u32,
    pub(super) a: u32,
    pub(super) ct: i32,
    pub(super) fixed_bin: u8,
    pub(super) error: bool,
    pub(super) overrun: u32,
}

pub(super) const ARITH_EXHAUSTED_PADDING_BYTES: u32 = 4;

impl<'a> ArithmeticDecoder<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, c: 0, a: 0, ct: -16, fixed_bin: 113, error: false, overrun: 0 }
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.overrun > ARITH_EXHAUSTED_PADDING_BYTES
    }

    pub(super) fn reset(&mut self) {
        self.c = 0;
        self.a = 0;
        self.ct = -16;
        self.error = false;
    }

    pub(super) fn read_byte(&mut self) -> u32 {
        if self.pos >= self.data.len() {
            self.overrun = self.overrun.saturating_add(1);
            return 0;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b as u32
    }

    pub(super) fn arith_decode(&mut self, st: &mut u8) -> u8 {
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

    pub(super) fn decode_dc_coeff(
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

    pub(super) fn decode_dc_first(
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

    pub(super) fn decode_dc_refine(&mut self, block: &mut [i16], successive_low: u8) {
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

    pub(super) fn decode_ac_first(
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

    pub(super) fn decode_ac_refine(
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

    pub(super) fn decode_mcu_sequential(
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

    pub(super) fn decode_lossless_difference(
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

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn decode_arithmetic_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
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

        let mut arith = ArithmeticDecoder::new(&self.entropy[scan.data.clone()]);

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

        let mut restart_counter = scan.restart_interval as u32;

        'mcus: for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                if arith.is_exhausted() {
                    log_warn!("Arithmetic scan data exhausted at MCU ({}, {})", mcu_x, mcu_y);
                    break 'mcus;
                }

                if scan.restart_interval > 0 {
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
                        restart_counter = scan.restart_interval as u32;
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

    pub(super) fn decode_progressive_scans_arithmetic(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
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

            let mut arith = ArithmeticDecoder::new(&self.entropy[scan.data.clone()]);

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

            let restart_interval = scan.restart_interval;
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
}
