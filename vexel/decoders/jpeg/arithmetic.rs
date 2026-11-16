use std::io::{Read, Seek};

use super::decoder::JpegDecoder;
use super::planes::ComponentPlane;
use crate::decoders::jpeg::types::{JpegMode, ZIGZAG_MAP};
use crate::log_warn;
use crate::utils::entropy::qm::{QmDecoder, FIXED_STATE};
use crate::utils::error::VexelResult;

pub(super) struct ArithmeticDecoder<'a> {
    qm: QmDecoder<'a>,
    fixed_bin: u8,
    error: bool,
}

impl<'a> ArithmeticDecoder<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { qm: QmDecoder::new(data), fixed_bin: FIXED_STATE, error: false }
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.qm.is_exhausted()
    }

    pub(super) fn reset(&mut self) {
        self.qm.reset();
        self.error = false;
    }

    #[inline]
    pub(super) fn arith_decode(&mut self, st: &mut u8) -> u8 {
        if self.error {
            return 0;
        }
        self.qm.decode(st)
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
