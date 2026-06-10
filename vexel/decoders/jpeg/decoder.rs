use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::exif::ExifReader;
use crate::utils::info::JpegInfo;
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use std::f32::consts::PI;
use std::fmt::Debug;
use std::io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom};
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::decoders::jpeg::types::{ArithmeticCodingTable, ArithmeticCodingValue, ColorComponentInfo, DACData, DHTData, DQTData, HuffmanTable, JFIFData, JFIFHeader, JpegCodingMethod, JpegMode, JpegSegmentData, JpegSegmentInfo, Predictor, QuantizationTable, SOFData, SOSData, ScanComponent, ScanData, DEFAULT_QUANTIZATION_TABLE, ZIGZAG_MAP};

#[derive(Debug, Clone)]
struct UpsampledPlane {
    data: Vec<i32>,
    width: u32,
    height: u32,
}

impl UpsampledPlane {
    fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![0; (width * height) as usize],
            width,
            height,
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, value: i32) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = value;
        }
    }
}

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

    fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i32]> {
        let block_idx = block_y * self.blocks_per_line + block_x;
        let start = (block_idx * 64) as usize;
        if start + 64 <= self.data.len() {
            Some(&mut self.data[start..start + 64])
        } else {
            None
        }
    }

    fn upsample(&self, source_width: u32, source_height: u32, target_width: u32, target_height: u32, scratch: &mut Vec<i32>) -> UpsampledPlane {
        use crate::decoders::jpeg::upsample as up;

        let sw = source_width as usize;
        let sh = source_height as usize;
        let tw = target_width as usize;
        let th = target_height as usize;

        scratch.clear();
        if scratch.capacity() < sw * sh {
            scratch.reserve(sw * sh - scratch.capacity());
        }
        scratch.resize(sw * sh, 0);
        let source_pixels = scratch;
        up::deinterleave_blocks(&self.data, self.blocks_per_line, source_width, source_height, source_pixels);

        if source_width == target_width && source_height == target_height {
            return UpsampledPlane { data: source_pixels.to_vec(), width: target_width, height: target_height };
        }

        let mut upsampled = UpsampledPlane::new(target_width, target_height);

        let h_2x = tw == sw * 2 || tw == sw * 2 - 1;
        let v_2x = th == sh * 2 || th == sh * 2 - 1;
        let is_h2v1 = h_2x && th == sh;
        let is_h1v2 = !h_2x && v_2x && tw == sw;
        let is_h2v2 = h_2x && v_2x;

        if is_h2v1 {
            for sy in 0..sh {
                let src_row = &source_pixels[sy * sw..(sy * sw + sw)];
                let dst_row = &mut upsampled.data[sy * tw..sy * tw + tw];
                up::upsample_h2v1_row(src_row, dst_row, sw, tw);
            }
        } else if is_h1v2 {
            for sy in 0..sh {
                let sy_above = sy.saturating_sub(1);
                let sy_below = (sy + 1).min(sh - 1);

                for v in 0..2usize {
                    let dy = sy * 2 + v;
                    if dy >= th {
                        continue;
                    }
                    let (sy_near, bias) = if v == 0 { (sy_above, 1i32) } else { (sy_below, 2i32) };
                    let src_row = &source_pixels[sy * sw..sy * sw + sw];
                    let neighbor_row = &source_pixels[sy_near * sw..sy_near * sw + sw];
                    let dst_row = &mut upsampled.data[dy * tw..dy * tw + sw.min(tw)];
                    up::upsample_h1v2_row(src_row, neighbor_row, bias, dst_row, sw.min(dst_row.len()));
                }
            }
        } else if is_h2v2 {
            let mut col_sums = vec![0i32; sw];
            let mut prev_col_sums = vec![0i32; sw];

            for sy in 0..sh {
                let sy_above = sy.saturating_sub(1);
                let sy_below = (sy + 1).min(sh - 1);

                for v in 0..2usize {
                    let dy = sy * 2 + v;
                    if dy >= th {
                        continue;
                    }
                    let sy_near = if v == 0 { sy_above } else { sy_below };
                    let src_row = &source_pixels[sy * sw..sy * sw + sw];
                    let near_row = &source_pixels[sy_near * sw..sy_near * sw + sw];

                    up::compute_col_sums(src_row, near_row, &mut col_sums, sw);

                    let prev_near = if v == 0 { sy_above } else { sy };
                    let prev_src_row = &source_pixels[sy * sw..sy * sw + sw];
                    let prev_near_row = &source_pixels[prev_near * sw..prev_near * sw + sw];
                    up::compute_col_sums(prev_src_row, prev_near_row, &mut prev_col_sums, sw);

                    let dst_row = &mut upsampled.data[dy * tw..dy * tw + tw.min((sw * 2).min(tw))];
                    up::upsample_h2v2_row(&col_sums, &prev_col_sums, dst_row, sw, dst_row.len());
                }
            }
        } else {
            let get_src = |sx: usize, sy: usize| -> i32 {
                let idx = sy * sw + sx;
                if idx < source_pixels.len() { source_pixels[idx] } else { 0 }
            };

            for y in 0..target_height {
                for x in 0..target_width {
                    let fx = (x as f32 + 0.5) * source_width as f32 / target_width as f32 - 0.5;
                    let fy = (y as f32 + 0.5) * source_height as f32 / target_height as f32 - 0.5;

                    let x0 = (fx.floor() as i64).clamp(0, source_width as i64 - 1) as usize;
                    let y0 = (fy.floor() as i64).clamp(0, source_height as i64 - 1) as usize;
                    let x1 = (x0 + 1).min(source_width as usize - 1);
                    let y1 = (y0 + 1).min(source_height as usize - 1);

                    let wx = fx - fx.floor();
                    let wy = fy - fy.floor();

                    let v = (1.0 - wy) * ((1.0 - wx) * get_src(x0, y0) as f32 + wx * get_src(x1, y0) as f32)
                        + wy * ((1.0 - wx) * get_src(x0, y1) as f32 + wx * get_src(x1, y1) as f32);

                    upsampled.set_pixel(x, y, v.round() as i32);
                }
            }
        }

        upsampled
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

            let threshold_l = if dc_l == 0 { 0 } else { (1i32 << dc_l) >> 1 };
            let threshold_u = if dc_u == 0 { 0 } else { (1i32 << dc_u) >> 1 };
            dc_context[comp_idx] = if m < threshold_l {
                0
            } else if m > threshold_u {
                12 + sign as usize * 4
            } else {
                4 + sign as usize * 4
            };

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
            last_dc_val[comp_idx] = (last_dc_val[comp_idx] + v) & 0xFFFF;
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
    ) {
        let val = self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats);
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
    ) {
        if self.error { return; }

        let val = self.decode_dc_coeff(comp_idx, dc_tbl, dc_l, dc_u, dc_context, last_dc_val, dc_stats);
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

    #[inline(always)]
    fn get_next_symbol<S: Read + Seek>(&self, reader: &mut BitReader<S>, table: &HuffmanTable) -> VexelResult<u8> {
        if table.first_code.len() < 16 || table.offsets.len() < 17 {
            log_warn!("Huffman table is malformed, first_code.len()={}, offsets.len()={}", table.first_code.len(), table.offsets.len());
            return Ok(0);
        }

        let mut code = 0u32;

        for i in 0..16 {
            code = (code << 1) | reader.read_bits_unchecked(1);

            let first = table.first_code[i];
            if first != u32::MAX {
                let count = (table.offsets[i + 1] - table.offsets[i]) as usize;
                if code >= first && (code - first) < count as u32 {
                    let idx = table.offsets[i] as usize + (code - first) as usize;
                    return Ok(table.symbols[idx]);
                }
            }
        }

        log_warn!("Invalid Huffman code: {}, replacing with 0", code);

        Ok(0)
    }

    fn decode_mcu<S: Read + Seek>(
        &self,
        reader: &mut BitReader<S>,
        mcu_component: &mut [i32],
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        previous_dc: &mut i32,
    ) -> VexelResult<()> {
        let length = self.get_next_symbol(reader, dc_table)?;

        let max_length = if self.precision > 8 { 12 } else { 11 };
        if length > max_length {
            log_warn!("Invalid DC coefficient length (>{}): {}", max_length, length);
            return Ok(());
        }

        let mut coefficient = reader.read_bits(length)? as i32;

        if length != 0 && coefficient < (1 << (length - 1)) {
            coefficient -= (1 << length) - 1;
        }

        mcu_component[0] = coefficient + *previous_dc;
        *previous_dc = mcu_component[0];

        let mut i = 1;
        while i < 64 {
            let symbol = self.get_next_symbol(reader, ac_table).unwrap_or_else(|_| {
                log_warn!("Failed to get next AC symbol during decoding, replacing with 0");
                0
            });

            if symbol == 0 {
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }

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
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }
                return Ok(());
            }

            for _ in 0..zero_count {
                mcu_component[ZIGZAG_MAP[i] as usize] = 0;
                i += 1;
            }

            let max_coefficient_length = if self.precision > 8 { 16 } else { 10 };
            if coefficient_length > max_coefficient_length {
                log_warn!("Invalid coefficient length: {}, replacing with 0", coefficient_length);
                coefficient_length = 0;
            }

            if coefficient_length != 0 {
                coefficient = reader.read_bits(coefficient_length)? as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                if mcu_component.len() <= ZIGZAG_MAP[i] as usize {
                    log_warn!("Invalid zigzag index: {}, skipping", ZIGZAG_MAP[i]);
                    i += 1;
                    continue;
                }

                mcu_component[ZIGZAG_MAP[i] as usize] = coefficient;
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

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut previous_dc = vec![0i32; planes.len()];

        for scan in &self.scans {
            let mut reader = BitReader::new(Cursor::new(scan.data.as_slice()));
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

                        for v in 0..v_blocks {
                            for h in 0..h_blocks {
                                let plane_blocks_per_line = planes[plane_index].blocks_per_line;
                                let block_x = mcu_x * h_blocks as u32 + h as u32;
                                let block_y = mcu_y * v_blocks as u32 + v as u32;

                                if block_x >= plane_blocks_per_line {
                                    continue;
                                }

                                if let Some(component_data) = planes[plane_index].get_block_mut(block_x, block_y) {
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

                                            let length = self.get_next_symbol(&mut reader, dc_table)?;

                                            if length > 11 {
                                                log_warn!("Invalid DC coefficient length (>11): {}", length);
                                                continue;
                                            }

                                            let bits = reader.read_bits_unchecked(length);

                                            let mut value = bits as i32;

                                            if length != 0 && value < (1 << (length - 1)) {
                                                value -= (1 << length) - 1;
                                            }

                                            value += previous_dc[plane_index];
                                            previous_dc[plane_index] = value;
                                            component_data[0] = value << scan.successive_low;
                                        } else {
                                            // Refining DC scan
                                            let bit = reader.read_bits_unchecked(1);

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
                                                let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                    Ok(symbol) => symbol,
                                                    Err(e) => {
                                                        log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                        break;
                                                    }
                                                };

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
                                                        if k > ZIGZAG_MAP.len() {
                                                            log_warn!("k value exceeded zigzag map: {}", k);
                                                            break;
                                                        }

                                                        component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                        k += 1;
                                                    }

                                                    if length > 10 {
                                                        log_warn!("Invalid AC coefficient length (>10): {}", length);
                                                        break;
                                                    }

                                                    let bits = reader.read_bits_unchecked(length);
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
                                                            if k > ZIGZAG_MAP.len() {
                                                                log_warn!("k value exceeded zigzag map: {}", k);
                                                                break;
                                                            }

                                                            component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                            k += 1;
                                                        }
                                                    } else {
                                                        skips = (1 << num_zeros) - 1;
                                                        skips += reader.read_bits_unchecked(num_zeros);
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
                                                    let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                        Ok(symbol) => symbol,
                                                        Err(e) => {
                                                            log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                            break;
                                                        }
                                                    };

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

                                                        coefficient = if reader.read_bits_unchecked(1) != 0 { positive } else { negative };
                                                    } else {
                                                        if num_zeros != 15 {
                                                            skips = 1 << num_zeros;
                                                            skips += reader.read_bits_unchecked(num_zeros);
                                                            break;
                                                        }
                                                    }

                                                    if component_data.len() <= ZIGZAG_MAP[k] as usize {
                                                        log_warn!(
                                                            "Value from a zigzag map exceeds component data length: {}",
                                                            ZIGZAG_MAP[k]
                                                        );
                                                        break;
                                                    }

                                                    loop {
                                                        if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                            if reader.read_bits_unchecked(1) == 1 {
                                                                if component_data[ZIGZAG_MAP[k] as usize] & positive == 0 {
                                                                    if component_data[ZIGZAG_MAP[k] as usize] >= 0 {
                                                                        component_data[ZIGZAG_MAP[k] as usize] += positive;
                                                                    } else {
                                                                        component_data[ZIGZAG_MAP[k] as usize] += negative;
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
                                                        component_data[ZIGZAG_MAP[k] as usize] = coefficient;
                                                    }

                                                    k += 1;
                                                }
                                            }

                                            if skips > 0 {
                                                while k <= scan.end_spectral as usize {
                                                    if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                        if reader.read_bits_unchecked(1) == 1 {
                                                            if component_data[ZIGZAG_MAP[k] as usize] & positive == 0 {
                                                                if component_data[ZIGZAG_MAP[k] as usize] >= 0 {
                                                                    component_data[ZIGZAG_MAP[k] as usize] += positive;
                                                                } else {
                                                                    component_data[ZIGZAG_MAP[k] as usize] += negative;
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
        let mut reader = BitReader::new(Cursor::new(scan.data.as_slice()));

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
                            &HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![0b000, 0b010, 0b011, 0b100, 0b101, 0b110, 0b1110, 0b11110, 0b111110, 0b1111110, 0b11111110, 0b111111110],
                                first_code: vec![u32::MAX, u32::MAX, 0b000, 0b011, u32::MAX, 0b100, 0b101, 0b110, 0b1110, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX],
                            }
                        }
                    };

                    let bits_to_read = self.get_next_symbol(&mut reader, dc_table)?;

                    let diff = match bits_to_read {
                        0 => 0,
                        1..=15 => {
                            let additional_bits = reader.read_bits(bits_to_read)? as i32;

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

        let differences = self.decode_differences(&scan)?;

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
        let mut reader = BitReader::new(Cursor::new(scan.data.as_slice()));
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

        let default_dc_table = HuffmanTable {
            class: 0,
            id: 0,
            offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            codes: vec![0b000, 0b010, 0b011, 0b100, 0b101, 0b110, 0b1110, 0b11110, 0b111110, 0b1111110, 0b11111110, 0b111111110],
            first_code: vec![u32::MAX, u32::MAX, 0b000, 0b011, u32::MAX, 0b100, 0b101, 0b110, 0b1110, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX],
        };
        let default_ac_table = HuffmanTable {
            class: 0,
            id: 0,
            offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            codes: vec![0b000, 0b010, 0b011, 0b100, 0b101, 0b110, 0b1110, 0b11110, 0b111110, 0b1111110, 0b11111110, 0b111111110],
            first_code: vec![u32::MAX, u32::MAX, 0b000, 0b011, u32::MAX, 0b100, 0b101, 0b110, 0b1110, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX],
        };

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
                                match self.decode_mcu(
                                    &mut reader,
                                    block,
                                    &info.dc_table,
                                    &info.ac_table,
                                    &mut previous_dc[comp_idx],
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
        let m_0 = 2.0 * (1.0 / 16.0 * 2.0 * PI).cos();
        let m_1 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_3 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_5 = 2.0 * (3.0 / 16.0 * 2.0 * PI).cos();
        let m_2 = m_0 - m_5;
        let m_4 = m_0 + m_5;

        let s_0 = (0.0 / 16.0 * PI).cos() / 8.0_f32.sqrt();
        let s_1 = (1.0 / 16.0 * PI).cos() / 2.0;
        let s_2 = (2.0 / 16.0 * PI).cos() / 2.0;
        let s_3 = (3.0 / 16.0 * PI).cos() / 2.0;
        let s_4 = (4.0 / 16.0 * PI).cos() / 2.0;
        let s_5 = (5.0 / 16.0 * PI).cos() / 2.0;
        let s_6 = (6.0 / 16.0 * PI).cos() / 2.0;
        let s_7 = (7.0 / 16.0 * PI).cos() / 2.0;

        let level_shift = if self.precision <= 8 { 128i32 } else { 2048i32 };

        #[allow(clippy::too_many_arguments)]
        fn idct_block(
            block: &mut [i32],
            quant: &[u16],
            m_1: f32, m_2: f32, m_3: f32, m_4: f32, m_5: f32,
            s_0: f32, s_1: f32, s_2: f32, s_3: f32, s_4: f32, s_5: f32, s_6: f32, s_7: f32,
            level_shift: i32,
        ) {
            let mut temp = [0.0f32; 64];

            for col in 0..8 {
                let g_0 = block[0 * 8 + col] as f32 * quant[0 * 8 + col] as f32 * s_0;
                let g_1 = block[4 * 8 + col] as f32 * quant[4 * 8 + col] as f32 * s_4;
                let g_2 = block[2 * 8 + col] as f32 * quant[2 * 8 + col] as f32 * s_2;
                let g_3 = block[6 * 8 + col] as f32 * quant[6 * 8 + col] as f32 * s_6;
                let g_4 = block[5 * 8 + col] as f32 * quant[5 * 8 + col] as f32 * s_5;
                let g_5 = block[1 * 8 + col] as f32 * quant[1 * 8 + col] as f32 * s_1;
                let g_6 = block[7 * 8 + col] as f32 * quant[7 * 8 + col] as f32 * s_7;
                let g_7 = block[3 * 8 + col] as f32 * quant[3 * 8 + col] as f32 * s_3;

                let f_4 = g_4 - g_7;
                let f_5 = g_5 + g_6;
                let f_6 = g_5 - g_6;
                let f_7 = g_4 + g_7;

                let e_2 = g_2 - g_3;
                let e_3 = g_2 + g_3;
                let e_5 = f_5 - f_7;
                let e_7 = f_5 + f_7;
                let e_8 = f_4 + f_6;

                let d_2 = e_2 * m_1;
                let d_4 = f_4 * m_2;
                let d_5 = e_5 * m_3;
                let d_6 = f_6 * m_4;
                let d_8 = e_8 * m_5;

                let c_0 = g_0 + g_1;
                let c_1 = g_0 - g_1;
                let c_2 = d_2 - e_3;
                let c_4 = d_4 + d_8;
                let c_5 = d_5 + e_7;
                let c_6 = d_6 - d_8;
                let c_8 = c_5 - c_6;

                let b_0 = c_0 + e_3;
                let b_1 = c_1 + c_2;
                let b_2 = c_1 - c_2;
                let b_3 = c_0 - e_3;
                let b_4 = c_4 - c_8;
                let b_6 = c_6 - e_7;

                temp[0 * 8 + col] = b_0 + e_7;
                temp[1 * 8 + col] = b_1 + b_6;
                temp[2 * 8 + col] = b_2 + c_8;
                temp[3 * 8 + col] = b_3 + b_4;
                temp[4 * 8 + col] = b_3 - b_4;
                temp[5 * 8 + col] = b_2 - c_8;
                temp[6 * 8 + col] = b_1 - b_6;
                temp[7 * 8 + col] = b_0 - e_7;
            }

            for row in 0..8 {
                let g_0 = temp[row * 8 + 0] * s_0;
                let g_1 = temp[row * 8 + 4] * s_4;
                let g_2 = temp[row * 8 + 2] * s_2;
                let g_3 = temp[row * 8 + 6] * s_6;
                let g_4 = temp[row * 8 + 5] * s_5;
                let g_5 = temp[row * 8 + 1] * s_1;
                let g_6 = temp[row * 8 + 7] * s_7;
                let g_7 = temp[row * 8 + 3] * s_3;

                let f_4 = g_4 - g_7;
                let f_5 = g_5 + g_6;
                let f_6 = g_5 - g_6;
                let f_7 = g_4 + g_7;

                let e_2 = g_2 - g_3;
                let e_3 = g_2 + g_3;
                let e_5 = f_5 - f_7;
                let e_7 = f_5 + f_7;
                let e_8 = f_4 + f_6;

                let d_2 = e_2 * m_1;
                let d_4 = f_4 * m_2;
                let d_5 = e_5 * m_3;
                let d_6 = f_6 * m_4;
                let d_8 = e_8 * m_5;

                let c_0 = g_0 + g_1;
                let c_1 = g_0 - g_1;
                let c_2 = d_2 - e_3;
                let c_4 = d_4 + d_8;
                let c_5 = d_5 + e_7;
                let c_6 = d_6 - d_8;
                let c_8 = c_5 - c_6;

                let b_0 = c_0 + e_3;
                let b_1 = c_1 + c_2;
                let b_2 = c_1 - c_2;
                let b_3 = c_0 - e_3;
                let b_4 = c_4 - c_8;
                let b_6 = c_6 - e_7;

                block[row * 8 + 0] = ((b_0 + e_7).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 1] = ((b_1 + b_6).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 2] = ((b_2 + c_8).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 3] = ((b_3 + b_4).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 4] = ((b_3 - b_4).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 5] = ((b_2 - c_8).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 6] = ((b_1 - b_6).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
                block[row * 8 + 7] = ((b_0 - e_7).round() as i32).clamp(-level_shift, level_shift * 2 - 1);
            }
        }

        let default_table = QuantizationTable {
            id: 0,
            precision: 8,
            length: 64,
            table: DEFAULT_QUANTIZATION_TABLE.to_vec(),
        };

        for (comp_idx, plane) in planes.iter_mut().enumerate() {
            let quant_data: Vec<u16> = self
                .components
                .get(comp_idx)
                .and_then(|comp| {
                    self.quantization_tables
                        .iter()
                        .find(|q| q.id == comp.quantization_table_id)
                })
                .map(|t| t.table.clone())
                .unwrap_or_else(|| {
                    log_warn!("Quantization table not found for component, substituting default one.");
                    default_table.table.clone()
                });

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                plane.data.par_chunks_mut(64).for_each(|block| {
                    if block.len() == 64 {
                        idct_block(block, &quant_data, m_1, m_2, m_3, m_4, m_5, s_0, s_1, s_2, s_3, s_4, s_5, s_6, s_7, level_shift);
                    }
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                for block in plane.data.chunks_mut(64) {
                    if block.len() == 64 {
                        idct_block(block, &quant_data, m_1, m_2, m_3, m_4, m_5, s_0, s_1, s_2, s_3, s_4, s_5, s_6, s_7, level_shift);
                    }
                }
            }
        }

        Ok(())
    }

    fn upsample_planes(&self, planes: &[ComponentPlane]) -> Vec<UpsampledPlane> {
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

        let target_width = self.width;
        let target_height = self.height;

        let pairs: Vec<(&ComponentPlane, (u32, u32))> = planes
            .iter()
            .zip(self.components.iter())
            .map(|(plane, comp)| {
                let source_width = (target_width * comp.horizontal_sampling_factor as u32
                    + max_h_samp as u32 - 1) / max_h_samp as u32;
                let source_height = (target_height * comp.vertical_sampling_factor as u32
                    + max_v_samp as u32 - 1) / max_v_samp as u32;
                (plane, (source_width, source_height))
            })
            .collect();

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            pairs
                .into_par_iter()
                .map(|(plane, (sw, sh))| plane.upsample(sw, sh, target_width, target_height, &mut Vec::new()))
                .collect()
        }

        #[cfg(not(feature = "rayon"))]
        {
            let mut scratch = Vec::new();
            pairs
                .into_iter()
                .map(|(plane, (sw, sh))| plane.upsample(sw, sh, target_width, target_height, &mut scratch))
                .collect()
        }
    }

    fn convert_colorspace(&self, planes: &[UpsampledPlane]) -> VexelResult<PixelData> {
        let width = self.width as usize;
        let height = self.height as usize;
        let npixels = width * height;

        if planes.len() == 1 {
            return if self.precision <= 8 {
                let plane = &planes[0].data;
                let mut pixels = Vec::with_capacity(npixels);
                for i in 0..npixels.min(plane.len()) {
                    pixels.push((plane[i] + 128).clamp(0, 255) as u8);
                }
                if pixels.len() < npixels {
                    pixels.resize(npixels, 0);
                }
                Ok(PixelData::L8(pixels))
            } else {
                let plane = &planes[0].data;
                let mut pixels16 = Vec::with_capacity(npixels);
                for i in 0..npixels.min(plane.len()) {
                    pixels16.push((plane[i] + 2048).clamp(0, 4095) as u16);
                }
                if pixels16.len() < npixels {
                    pixels16.resize(npixels, 0);
                }
                Ok(PixelData::L16(pixels16))
            };
        }

        if planes.len() < 3 {
            log_warn!("Invalid number of planes for RGB conversion: {}.", planes.len());
        }

        let plane0 = &planes[0].data;
        let plane1 = planes.get(1).map(|p| p.data.as_slice()).unwrap_or(&[]);
        let plane2 = planes.get(2).map(|p| p.data.as_slice()).unwrap_or(&[]);

        if self.precision <= 8 {
            let mut pixels = vec![0u8; npixels * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                pixels.par_chunks_mut(width * 3).enumerate().for_each(|(row, dst)| {
                    let row_start = row * width;
                    for col in 0..width {
                        let i = row_start + col;
                        let y = plane0.get(i).copied().unwrap_or(0);
                        let cb = plane1.get(i).copied().unwrap_or(0);
                        let cr = plane2.get(i).copied().unwrap_or(0);
                        let y128 = (y + 128) << 16;
                        dst[col * 3] = ((y128 + 91881 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        dst[col * 3 + 1] = ((y128 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        dst[col * 3 + 2] = ((y128 + 116130 * cb + 32768) >> 16).clamp(0, 255) as u8;
                    }
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                for i in 0..npixels {
                    let y = plane0.get(i).copied().unwrap_or(0);
                    let cb = plane1.get(i).copied().unwrap_or(0);
                    let cr = plane2.get(i).copied().unwrap_or(0);
                    let y128 = (y + 128) << 16;
                    let base = i * 3;
                    pixels[base] = ((y128 + 91881 * cr + 32768) >> 16).clamp(0, 255) as u8;
                    pixels[base + 1] = ((y128 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 255) as u8;
                    pixels[base + 2] = ((y128 + 116130 * cb + 32768) >> 16).clamp(0, 255) as u8;
                }
            }

            Ok(PixelData::RGB8(pixels))
        } else {
            let mut pixels16 = vec![0u16; npixels * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                pixels16.par_chunks_mut(width * 3).enumerate().for_each(|(row, dst)| {
                    let row_start = row * width;
                    for col in 0..width {
                        let i = row_start + col;
                        let y = plane0.get(i).copied().unwrap_or(0);
                        let cb = plane1.get(i).copied().unwrap_or(0);
                        let cr = plane2.get(i).copied().unwrap_or(0);
                        let y2048 = (y + 2048) << 16;
                        dst[col * 3] = ((y2048 + 91881 * cr + 32768) >> 16).clamp(0, 4095) as u16;
                        dst[col * 3 + 1] = ((y2048 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 4095) as u16;
                        dst[col * 3 + 2] = ((y2048 + 116130 * cb + 32768) >> 16).clamp(0, 4095) as u16;
                    }
                });
            }

            #[cfg(not(feature = "rayon"))]
            {
                for i in 0..npixels {
                    let y = plane0.get(i).copied().unwrap_or(0);
                    let cb = plane1.get(i).copied().unwrap_or(0);
                    let cr = plane2.get(i).copied().unwrap_or(0);
                    let y2048 = (y + 2048) << 16;
                    let base = i * 3;
                    pixels16[base] = ((y2048 + 91881 * cr + 32768) >> 16).clamp(0, 4095) as u16;
                    pixels16[base + 1] = ((y2048 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 4095) as u16;
                    pixels16[base + 2] = ((y2048 + 116130 * cb + 32768) >> 16).clamp(0, 4095) as u16;
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

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
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
                        JpegMarker::SOF0 => self.read_start_of_frame("SOF0", segment_start),
                        JpegMarker::SOF1 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.read_start_of_frame("SOF1", segment_start)
                        }
                        JpegMarker::SOF2 => {
                            self.mode = JpegMode::Progressive;
                            self.read_start_of_frame("SOF2", segment_start)
                        }
                        JpegMarker::SOF3 => {
                            self.mode = JpegMode::Lossless;
                            self.read_start_of_frame("SOF3", segment_start)
                        }
                        JpegMarker::SOF9 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF9", segment_start)
                        }
                        JpegMarker::SOF10 => {
                            self.mode = JpegMode::Progressive;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF10", segment_start)
                        }
                        JpegMarker::SOF11 => {
                            self.mode = JpegMode::Lossless;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF11", segment_start)
                        }
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
        }
    }
}
