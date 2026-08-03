use crate::decoders::jpeg2000::mq::MqDecoder;
use crate::decoders::jpeg2000::t1_luts::{LUT_CTXNO_SC, LUT_CTXNO_ZC, LUT_SPB};
use crate::decoders::jpeg2000::types::*;

pub struct Segment {
    pub data_start: usize,
    pub data_len: usize,
    pub num_passes: u32,
}

pub struct CodeBlock<'a> {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub num_bps: u32,
    pub segments: &'a [Segment],
    pub data: &'a [u8],
}

impl CodeBlock<'_> {
    pub fn width(&self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentCoder {
    Mq,
    Raw,
}

impl SegmentCoder {
    fn is_raw(&self) -> bool {
        *self == SegmentCoder::Raw
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassType {
    Significance,
    Refinement,
    Cleanup,
}

impl PassType {
    fn next(&self) -> Self {
        match self {
            PassType::Significance => PassType::Refinement,
            PassType::Refinement => PassType::Cleanup,
            PassType::Cleanup => PassType::Significance,
        }
    }
}

pub struct T1Decoder {
    mqc: MqDecoder,
    data: Vec<i32>,
    flags: Vec<u32>,
    w: u32,
    h: u32,
    flags_stride: u32,
}

impl T1Decoder {
    pub fn new() -> Self {
        Self { mqc: MqDecoder::new(), data: Vec::new(), flags: Vec::new(), w: 0, h: 0, flags_stride: 0 }
    }

    pub fn data(&self) -> &[i32] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [i32] {
        &mut self.data
    }

    fn allocate(&mut self, w: u32, h: u32) {
        self.w = w;
        self.h = h;
        self.flags_stride = w + 2;

        let data_size = (w as usize) * (h as usize);
        self.data.clear();
        self.data.resize(data_size, 0);

        let flags_height = h.div_ceil(4);
        let flags_rows = flags_height + 2;
        let stride = self.flags_stride as usize;
        let flags_size = stride * (flags_rows as usize);

        self.flags.clear();
        self.flags.resize(flags_size, 0);

        let sentinel = T1_PI_0 | (T1_PI_0 << 3) | (T1_PI_0 << 6) | (T1_PI_0 << 9);

        for slot in self.flags[..stride].iter_mut() {
            *slot = sentinel;
        }

        let bottom = (flags_height as usize + 1) * stride;
        for slot in self.flags[bottom..bottom + stride].iter_mut() {
            *slot = sentinel;
        }

        if !h.is_multiple_of(4) {
            let partial = match h % 4 {
                1 => (T1_PI_0 << 3) | (T1_PI_0 << 6) | (T1_PI_0 << 9),
                2 => (T1_PI_0 << 6) | (T1_PI_0 << 9),
                _ => T1_PI_0 << 9,
            };

            let start = flags_height as usize * stride;
            for slot in self.flags[start..start + stride].iter_mut() {
                *slot = partial;
            }
        }
    }

    fn flag_index(&self, col: u32, row_group: u32) -> usize {
        (row_group as usize + 1) * self.flags_stride as usize + col as usize + 1
    }

    fn getctxno_zc(&self, orient: BandOrient, f: u32) -> usize {
        let base = orient.lut_index() << 9;
        LUT_CTXNO_ZC[base + (f & T1_SIGMA_NEIGHBOURS) as usize] as usize
    }

    fn getctxno_sc_or_spb_index(fx: u32, pfx: u32, nfx: u32, ci: u32) -> usize {
        let mut lu = (fx >> (ci * 3)) & (T1_SIGMA_1 | T1_SIGMA_3 | T1_SIGMA_5 | T1_SIGMA_7);

        lu |= (pfx >> (T1_CHI_1_I + ci * 3)) & (1 << 0);
        lu |= (nfx >> (T1_CHI_1_I - 2 + ci * 3)) & (1 << 2);

        if ci == 0 {
            lu |= (fx >> (T1_CHI_0_I - 4)) & (1 << 4);
        } else {
            lu |= (fx >> (T1_CHI_1_I - 4 + (ci - 1) * 3)) & (1 << 4);
        }

        lu |= (fx >> (T1_CHI_2_I - 6 + ci * 3)) & (1 << 6);
        lu as usize
    }

    fn getctxno_mag(f: u32) -> usize {
        if f & T1_MU_0 != 0 {
            T1_CTXNO_MAG + 2
        } else if f & T1_SIGMA_NEIGHBOURS != 0 {
            T1_CTXNO_MAG + 1
        } else {
            T1_CTXNO_MAG
        }
    }

    fn update_flags(&mut self, index: usize, ci: u32, s: u32, vsc: bool) {
        let stride = self.flags_stride as usize;

        self.flags[index - 1] |= T1_SIGMA_5 << (3 * ci);
        self.flags[index] |= ((s << T1_CHI_1_I) | T1_SIGMA_4) << (3 * ci);
        self.flags[index + 1] |= T1_SIGMA_3 << (3 * ci);

        if ci == 0 && !vsc {
            let north = index - stride;
            self.flags[north] |= (s << T1_CHI_5_I) | T1_SIGMA_16;
            self.flags[north - 1] |= T1_SIGMA_17;
            self.flags[north + 1] |= T1_SIGMA_15;
        }

        if ci == 3 {
            let south = index + stride;
            self.flags[south] |= (s << T1_CHI_0_I) | T1_SIGMA_1;
            self.flags[south - 1] |= T1_SIGMA_2;
            self.flags[south + 1] |= T1_SIGMA_0;
        }
    }

    fn sigpass_step(
        &mut self,
        index: usize,
        data_index: usize,
        oneplushalf: i32,
        ci: u32,
        vsc: bool,
        orient: BandOrient,
        raw: bool,
    ) {
        let flags = self.flags[index];

        if flags & ((T1_SIGMA_THIS | T1_PI_THIS) << (ci * 3)) != 0 {
            return;
        }
        if flags & (T1_SIGMA_NEIGHBOURS << (ci * 3)) == 0 {
            return;
        }

        let significant = if raw {
            self.mqc.raw_decode()
        } else {
            let ctx = self.getctxno_zc(orient, flags >> (ci * 3));
            self.mqc.set_curctx(ctx);
            self.mqc.decode()
        };

        if significant != 0 {
            let sign = if raw {
                self.mqc.raw_decode()
            } else {
                let lu = Self::getctxno_sc_or_spb_index(flags, self.flags[index - 1], self.flags[index + 1], ci);
                self.mqc.set_curctx(LUT_CTXNO_SC[lu] as usize);
                let bit = self.mqc.decode();
                bit ^ LUT_SPB[lu] as u32
            };

            self.data[data_index] = if sign != 0 { -oneplushalf } else { oneplushalf };
            self.update_flags(index, ci, sign, vsc);
        }

        self.flags[index] |= T1_PI_THIS << (ci * 3);
    }

    fn sigpass(&mut self, bpno: i32, cblksty: CodeBlockStyle, orient: BandOrient, raw: bool) {
        let one = 1i32 << bpno;
        let half = one >> 1;
        let oneplushalf = one | half;
        let vsc_mode = cblksty.vertically_causal();

        for k in (0..self.h).step_by(4) {
            let rows = (self.h - k).min(4);
            for i in 0..self.w {
                let index = self.flag_index(i, k / 4);

                if self.flags[index] == 0 {
                    continue;
                }

                for j in 0..rows {
                    let vsc = vsc_mode && j == 0;
                    let data_index = ((k + j) * self.w + i) as usize;
                    self.sigpass_step(index, data_index, oneplushalf, j, vsc, orient, raw);
                }
            }
        }
    }

    fn refpass_step(&mut self, index: usize, data_index: usize, poshalf: i32, ci: u32, raw: bool) {
        let flags = self.flags[index];

        if flags & ((T1_SIGMA_THIS | T1_PI_THIS) << (ci * 3)) != T1_SIGMA_THIS << (ci * 3) {
            return;
        }

        let v = if raw {
            self.mqc.raw_decode()
        } else {
            let ctx = Self::getctxno_mag(flags >> (ci * 3));
            self.mqc.set_curctx(ctx);
            self.mqc.decode()
        };

        let value = self.data[data_index];
        let negative = (value < 0) as u32;
        self.data[data_index] = if v ^ negative != 0 { value + poshalf } else { value - poshalf };

        self.flags[index] |= T1_MU_THIS << (ci * 3);
    }

    fn refpass(&mut self, bpno: i32, raw: bool) {
        let one = 1i32 << bpno;
        let poshalf = one >> 1;

        for k in (0..self.h).step_by(4) {
            let rows = (self.h - k).min(4);
            for i in 0..self.w {
                let index = self.flag_index(i, k / 4);
                for j in 0..rows {
                    let data_index = ((k + j) * self.w + i) as usize;
                    self.refpass_step(index, data_index, poshalf, j, raw);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn clnpass_step(
        &mut self,
        index: usize,
        data_index: usize,
        oneplushalf: i32,
        ci: u32,
        vsc: bool,
        orient: BandOrient,
        check_flags: bool,
        partial: bool,
    ) {
        let flags = self.flags[index];

        if check_flags && flags & ((T1_SIGMA_THIS | T1_PI_THIS) << (ci * 3)) != 0 {
            return;
        }

        if !partial {
            let ctx = self.getctxno_zc(orient, flags >> (ci * 3));
            self.mqc.set_curctx(ctx);
            if self.mqc.decode() == 0 {
                return;
            }
        }

        let lu = Self::getctxno_sc_or_spb_index(self.flags[index], self.flags[index - 1], self.flags[index + 1], ci);
        self.mqc.set_curctx(LUT_CTXNO_SC[lu] as usize);
        let sign = self.mqc.decode() ^ LUT_SPB[lu] as u32;

        self.data[data_index] = if sign != 0 { -oneplushalf } else { oneplushalf };
        self.update_flags(index, ci, sign, vsc);
    }

    fn clnpass(&mut self, bpno: i32, cblksty: CodeBlockStyle, orient: BandOrient) {
        let one = 1i32 << bpno;
        let half = one >> 1;
        let oneplushalf = one | half;
        let vsc_mode = cblksty.vertically_causal();

        for k in (0..self.h).step_by(4) {
            let rows = (self.h - k).min(4);

            for i in 0..self.w {
                let index = self.flag_index(i, k / 4);

                let mut runlen = 0;
                let mut partial = false;

                if rows == 4 && self.flags[index] == 0 {
                    self.mqc.set_curctx(T1_CTXNO_AGG);
                    if self.mqc.decode() == 0 {
                        continue;
                    }

                    self.mqc.set_curctx(T1_CTXNO_UNI);
                    let high = self.mqc.decode();
                    let low = self.mqc.decode();
                    runlen = (high << 1) | low;
                    partial = true;
                }

                for j in runlen..rows {
                    let vsc = vsc_mode && j == 0;
                    let data_index = ((k + j) * self.w + i) as usize;
                    let is_first = j == runlen;

                    self.clnpass_step(
                        index,
                        data_index,
                        oneplushalf,
                        j,
                        vsc,
                        orient,
                        !(partial && is_first),
                        partial && is_first,
                    );
                }

                for j in 0..rows {
                    self.flags[index] &= !(T1_PI_THIS << (3 * j));
                }
            }
        }

        if cblksty.segmentation_symbols() {
            self.mqc.set_curctx(T1_CTXNO_UNI);
            let mut symbol = self.mqc.decode();
            symbol = (symbol << 1) | self.mqc.decode();
            symbol = (symbol << 1) | self.mqc.decode();
            symbol = (symbol << 1) | self.mqc.decode();
            let _ = symbol;
        }
    }

    pub fn decode_cblk(
        &mut self,
        cblk: &CodeBlock,
        orient: BandOrient,
        roi_shift: u32,
        cblksty: CodeBlockStyle,
    ) -> bool {
        let w = cblk.width();
        let h = cblk.height();

        if w == 0 || h == 0 {
            self.allocate(w, h);
            return true;
        }

        self.allocate(w, h);

        let mut bpno_plus_one = (roi_shift + cblk.num_bps) as i32;
        if bpno_plus_one >= 31 {
            return false;
        }

        let mut passtype = PassType::Cleanup;
        self.mqc.reset_dec_states();

        if !cblk.segments.iter().any(|s| s.num_passes > 0) {
            return true;
        }

        for segment in cblk.segments.iter().filter(|s| s.num_passes > 0) {
            let use_raw = bpno_plus_one <= cblk.num_bps as i32 - 4 && passtype != PassType::Cleanup && cblksty.lazy();
            let coder = if use_raw { SegmentCoder::Raw } else { SegmentCoder::Mq };

            let start = segment.data_start.min(cblk.data.len());
            let end = start.saturating_add(segment.data_len).min(cblk.data.len());
            let seg_data = &cblk.data[start..end];

            match coder {
                SegmentCoder::Raw => self.mqc.raw_init_dec(seg_data),
                SegmentCoder::Mq => self.mqc.init_dec(seg_data),
            }

            for _ in 0..segment.num_passes {
                if bpno_plus_one < 1 {
                    break;
                }

                match passtype {
                    PassType::Significance => self.sigpass(bpno_plus_one, cblksty, orient, coder.is_raw()),
                    PassType::Refinement => self.refpass(bpno_plus_one, coder.is_raw()),
                    PassType::Cleanup => self.clnpass(bpno_plus_one, cblksty, orient),
                }

                if cblksty.reset() && coder == SegmentCoder::Mq {
                    self.mqc.reset_dec_states();
                }

                passtype = passtype.next();
                if passtype == PassType::Significance {
                    bpno_plus_one -= 1;
                }
            }
        }

        true
    }
}

impl Default for T1Decoder {
    fn default() -> Self {
        Self::new()
    }
}
