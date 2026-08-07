use crate::decoders::jpeg_xr::segdec::CbpModel;
use crate::decoders::jpeg_xr::tables::BLK_OFFSET_UV_422;
use crate::decoders::jpeg_xr::types::*;

const ORIENT_WEIGHT: i32 = 4;

#[derive(Debug, Clone, Copy, Default)]
pub struct PredInfo {
    pub qp_index: i32,
    pub cbp: i32,
    pub dc: i32,
    pub ad: [i32; 6],
}

#[derive(Debug, Clone)]
pub struct MacroblockInfo {
    pub block_dc: [[i32; MB_BLOCKS]; MAX_CHANNELS],
    pub orientation: i32,
    pub cbp: [i32; MAX_CHANNELS],
    pub diff_cbp: [i32; MAX_CHANNELS],
    pub qindex_lp: u8,
    pub qindex_hp: u8,
}

impl Default for MacroblockInfo {
    fn default() -> Self {
        MacroblockInfo {
            block_dc: [[0; MB_BLOCKS]; MAX_CHANNELS],
            orientation: 0,
            cbp: [0; MAX_CHANNELS],
            diff_cbp: [0; MAX_CHANNELS],
            qindex_lp: 0,
            qindex_hp: 0,
        }
    }
}

pub struct PredContext {
    pub current: Vec<Vec<PredInfo>>,
    pub previous: Vec<Vec<PredInfo>>,
}

impl PredContext {
    pub fn new(channels: usize, mb_width: usize) -> Self {
        let row = vec![PredInfo::default(); mb_width.max(1) + 1];
        PredContext { current: vec![row.clone(); channels.max(1)], previous: vec![row; channels.max(1)] }
    }

    pub fn advance_row(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
        for row in self.current.iter_mut() {
            for info in row.iter_mut() {
                *info = PredInfo::default();
            }
        }
    }
}

fn abs_sum(values: &[i32]) -> i32 {
    values.iter().fold(0i32, |acc, &v| acc.wrapping_add(v.wrapping_abs()))
}

fn orient(str_h: i32, str_v: i32) -> i32 {
    if str_h.wrapping_mul(ORIENT_WEIGHT) < str_v {
        1
    } else if str_v.wrapping_mul(ORIENT_WEIGHT) < str_h {
        0
    } else {
        2
    }
}

pub fn ac_pred_mode(mb: &MacroblockInfo, color_format: ColorFormat) -> i32 {
    let y = &mb.block_dc[0];
    let mut str_h = abs_sum(&[y[1], y[2], y[3]]);
    let mut str_v = abs_sum(&[y[4], y[8], y[12]]);

    if !matches!(color_format, ColorFormat::YOnly | ColorFormat::NComponent) {
        let u = &mb.block_dc[1];
        let v = &mb.block_dc[2];

        str_h = str_h.wrapping_add(abs_sum(&[u[1], v[1]]));

        match color_format {
            ColorFormat::Yuv420 => {
                str_v = str_v.wrapping_add(abs_sum(&[u[2], v[2]]));
            }
            ColorFormat::Yuv422 => {
                str_v = str_v.wrapping_add(abs_sum(&[u[2], v[2], u[6], v[6]]));
                str_h = str_h.wrapping_add(abs_sum(&[u[5], v[5]]));
            }
            _ => {
                str_v = str_v.wrapping_add(abs_sum(&[u[4], v[4]]));
            }
        }
    }

    orient(str_h, str_v)
}

pub fn dc_ac_pred_mode(
    pred: &PredContext,
    mb: &MacroblockInfo,
    mb_x: usize,
    at_left: bool,
    at_top: bool,
    color_format: ColorFormat,
) -> i32 {
    let dc_mode = if at_left && at_top {
        3
    } else if at_left {
        1
    } else if at_top {
        0
    } else {
        let left = pred.current[0][mb_x - 1].dc;
        let top = pred.previous[0][mb_x].dc;
        let top_left = pred.previous[0][mb_x - 1].dc;

        let diff_h = top_left.wrapping_sub(left).wrapping_abs();
        let diff_v = top_left.wrapping_sub(top).wrapping_abs();

        let (str_h, str_v) = if matches!(color_format, ColorFormat::YOnly | ColorFormat::NComponent) {
            (diff_h, diff_v)
        } else {
            let scale = match color_format {
                ColorFormat::Yuv420 => 8,
                ColorFormat::Yuv422 => 4,
                _ => 2,
            };

            let tu = pred.previous[1][mb_x].dc;
            let lu = pred.current[1][mb_x - 1].dc;
            let tlu = pred.previous[1][mb_x - 1].dc;
            let tv = pred.previous[2][mb_x].dc;
            let lv = pred.current[2][mb_x - 1].dc;
            let tlv = pred.previous[2][mb_x - 1].dc;

            (
                diff_h
                    .wrapping_mul(scale)
                    .wrapping_add(tlu.wrapping_sub(lu).wrapping_abs())
                    .wrapping_add(tlv.wrapping_sub(lv).wrapping_abs()),
                diff_v
                    .wrapping_mul(scale)
                    .wrapping_add(tlu.wrapping_sub(tu).wrapping_abs())
                    .wrapping_add(tlv.wrapping_sub(tv).wrapping_abs()),
            )
        };

        orient(str_h, str_v)
    };

    let mut ad_mode = 2;

    if dc_mode == 1 && i32::from(mb.qindex_lp) == pred.previous[0][mb_x].qp_index {
        ad_mode = 1;
    }
    if dc_mode == 0 && i32::from(mb.qindex_lp) == pred.current[0][mb_x - 1].qp_index {
        ad_mode = 0;
    }

    dc_mode + (ad_mode << 2)
}

pub fn pred_dc_ac(
    mb: &mut MacroblockInfo,
    pred: &PredContext,
    mb_x: usize,
    at_left: bool,
    at_top: bool,
    color_format: ColorFormat,
    channels: usize,
) {
    let mode = dc_ac_pred_mode(pred, mb, mb_x, at_left, at_top, color_format);
    let dc_mode = mode & 0x3;
    let ad_mode = mode & 0xc;

    let planes = if color_format.is_chroma_subsampled() { 1 } else { channels };

    for ch in 0..planes.min(MAX_CHANNELS) {
        let org = &mut mb.block_dc[ch];

        if dc_mode == 1 {
            org[0] = org[0].wrapping_add(pred.previous[ch][mb_x].dc);
        } else if dc_mode == 0 {
            org[0] = org[0].wrapping_add(pred.current[ch][mb_x - 1].dc);
        } else if dc_mode == 2 {
            org[0] = org[0].wrapping_add((pred.current[ch][mb_x - 1].dc.wrapping_add(pred.previous[ch][mb_x].dc)) >> 1);
        }

        if ad_mode == 4 {
            let reference = pred.previous[ch][mb_x].ad;
            org[4] = org[4].wrapping_add(reference[3]);
            org[8] = org[8].wrapping_add(reference[4]);
            org[12] = org[12].wrapping_add(reference[5]);
        } else if ad_mode == 0 {
            let reference = pred.current[ch][mb_x - 1].ad;
            org[1] = org[1].wrapping_add(reference[0]);
            org[2] = org[2].wrapping_add(reference[1]);
            org[3] = org[3].wrapping_add(reference[2]);
        }
    }

    if color_format == ColorFormat::Yuv420 {
        for ch in 1..3 {
            let org = &mut mb.block_dc[ch];

            if dc_mode == 1 {
                org[0] = org[0].wrapping_add(pred.previous[ch][mb_x].dc);
            } else if dc_mode == 0 {
                org[0] = org[0].wrapping_add(pred.current[ch][mb_x - 1].dc);
            } else if dc_mode == 2 {
                org[0] = org[0].wrapping_add(
                    (pred.current[ch][mb_x - 1].dc.wrapping_add(pred.previous[ch][mb_x].dc).wrapping_add(1)) >> 1,
                );
            }

            if ad_mode == 4 {
                org[2] = org[2].wrapping_add(pred.previous[ch][mb_x].ad[1]);
            } else if ad_mode == 0 {
                org[1] = org[1].wrapping_add(pred.current[ch][mb_x - 1].ad[0]);
            }
        }
    } else if color_format == ColorFormat::Yuv422 {
        for ch in 1..3 {
            let org = &mut mb.block_dc[ch];

            if dc_mode == 1 {
                org[0] = org[0].wrapping_add(pred.previous[ch][mb_x].dc);
            } else if dc_mode == 0 {
                org[0] = org[0].wrapping_add(pred.current[ch][mb_x - 1].dc);
            } else if dc_mode == 2 {
                org[0] = org[0].wrapping_add(
                    (pred.current[ch][mb_x - 1].dc.wrapping_add(pred.previous[ch][mb_x].dc).wrapping_add(1)) >> 1,
                );
            }

            if ad_mode == 4 {
                org[4] = org[4].wrapping_add(pred.previous[ch][mb_x].ad[4]);
                org[2] = org[2].wrapping_add(pred.previous[ch][mb_x].ad[3]);
                org[6] = org[6].wrapping_add(org[2]);
            } else if ad_mode == 0 {
                org[4] = org[4].wrapping_add(pred.current[ch][mb_x - 1].ad[4]);
                org[1] = org[1].wrapping_add(pred.current[ch][mb_x - 1].ad[0]);
                org[5] = org[5].wrapping_add(pred.current[ch][mb_x - 1].ad[2]);
            } else if dc_mode == 1 {
                org[6] = org[6].wrapping_add(org[2]);
            }
        }
    }

    mb.orientation = 2 - ac_pred_mode(mb, color_format);
}

pub fn pred_ac(coeffs: &mut [Vec<i32>], mb: &MacroblockInfo, color_format: ColorFormat, channels: usize) {
    let mode = 2 - mb.orientation;
    let planes = if color_format.is_chroma_subsampled() { 1 } else { channels };

    for ch in 0..planes.min(coeffs.len()) {
        let plane = &mut coeffs[ch];

        match mode {
            1 => {
                const BLOCKS: [usize; 12] = [1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15];
                for &block in BLOCKS.iter() {
                    let org = 16 * block;
                    let reference = org - 16;
                    for &k in &[2usize, 10, 9] {
                        plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                    }
                }
            }
            0 => {
                let mut org = 64;
                while org < 256 {
                    let reference = org - 64;
                    for &k in &[1usize, 5, 6] {
                        plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                    }
                    org += 16;
                }
            }
            _ => {}
        }
    }

    if color_format == ColorFormat::Yuv420 {
        for ch in 1..3.min(coeffs.len()) {
            let plane = &mut coeffs[ch];
            match mode {
                1 => {
                    for j in [1usize, 3] {
                        let org = 16 * j;
                        let reference = org - 16;
                        for &k in &[2usize, 10, 9] {
                            plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                        }
                    }
                }
                0 => {
                    for j in 2..4usize {
                        let org = 16 * j;
                        let reference = org - 32;
                        for &k in &[1usize, 5, 6] {
                            plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                        }
                    }
                }
                _ => {}
            }
        }
    } else if color_format == ColorFormat::Yuv422 {
        for ch in 1..3.min(coeffs.len()) {
            let plane = &mut coeffs[ch];
            match mode {
                1 => {
                    for &org in BLK_OFFSET_UV_422.iter().skip(2) {
                        let reference = org - 16;
                        for &k in &[10usize, 2, 9] {
                            plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                        }
                    }
                }
                0 => {
                    let mut j = 1usize;
                    while j < 8 {
                        let org = BLK_OFFSET_UV_422[j];
                        let reference = org - 64;
                        for &k in &[1usize, 5, 6] {
                            plane[org + k] = plane[org + k].wrapping_add(plane[reference + k]);
                        }
                        j += 2;
                    }
                }
                _ => {}
            }
        }
    }
}

fn num_ones(value: i32) -> i32 {
    (value & 0xffff).count_ones() as i32
}

fn saturate32(value: &mut i32) {
    if (*value + 16) as u32 >= 32 {
        *value = if *value < 0 { -16 } else { 15 };
    }
}

fn update_cbp_model(model: &mut CbpModel, slot: usize, ones: i32) {
    model.count0[slot] += ones - AVG_NDIFF;
    saturate32(&mut model.count0[slot]);

    model.count1[slot] += 16 - ones - AVG_NDIFF;
    saturate32(&mut model.count1[slot]);

    model.state[slot] = if model.count0[slot] < 0 {
        if model.count0[slot] < model.count1[slot] {
            1
        } else {
            2
        }
    } else if model.count1[slot] < 0 {
        2
    } else {
        0
    };
}

pub fn pred_cbp_luma(
    mut cbp: i32,
    pred: &PredContext,
    mb_x: usize,
    channel: usize,
    at_left: bool,
    at_top: bool,
    model: &mut CbpModel,
) -> i32 {
    let slot = usize::from(channel != 0);

    if model.state[slot] == 0 {
        if at_left {
            if at_top {
                cbp ^= 1;
            } else {
                cbp ^= (pred.previous[channel][mb_x].cbp >> 10) & 1;
            }
        } else {
            cbp ^= (pred.current[channel][mb_x - 1].cbp >> 5) & 1;
        }

        cbp ^= 0x02 & (cbp << 1);
        cbp ^= 0x10 & (cbp << 3);
        cbp ^= 0x20 & (cbp << 1);

        cbp ^= (cbp & 0x33) << 2;
        cbp ^= (cbp & 0xcc) << 6;
        cbp ^= (cbp & 0x3300) << 2;
    } else if model.state[slot] == 2 {
        cbp ^= 0xffff;
    }

    update_cbp_model(model, slot, num_ones(cbp));
    cbp
}

pub fn pred_cbp_420(
    mut cbp: i32,
    pred: &PredContext,
    mb_x: usize,
    channel: usize,
    at_left: bool,
    at_top: bool,
    model: &mut CbpModel,
) -> i32 {
    if model.state[1] == 0 {
        if at_left {
            if at_top {
                cbp ^= 1;
            } else {
                cbp ^= (pred.previous[channel][mb_x].cbp >> 2) & 1;
            }
        } else {
            cbp ^= (pred.current[channel][mb_x - 1].cbp >> 1) & 1;
        }

        cbp ^= 0x02 & (cbp << 1);
        cbp ^= (cbp & 0x3) << 2;
    } else if model.state[1] == 2 {
        cbp ^= 0xf;
    }

    update_cbp_model(model, 1, num_ones(cbp) * 4);
    cbp
}

pub fn pred_cbp_422(
    mut cbp: i32,
    pred: &PredContext,
    mb_x: usize,
    channel: usize,
    at_left: bool,
    at_top: bool,
    model: &mut CbpModel,
) -> i32 {
    if model.state[1] == 0 {
        if at_left {
            if at_top {
                cbp ^= 1;
            } else {
                cbp ^= (pred.previous[channel][mb_x].cbp >> 6) & 1;
            }
        } else {
            cbp ^= (pred.current[channel][mb_x - 1].cbp >> 1) & 1;
        }

        cbp ^= (cbp & 0x1) << 1;
        cbp ^= (cbp & 0x3) << 2;
        cbp ^= (cbp & 0xc) << 2;
        cbp ^= (cbp & 0x30) << 2;
    } else if model.state[1] == 2 {
        cbp ^= 0xff;
    }

    update_cbp_model(model, 1, num_ones(cbp) * 2);
    cbp
}

pub fn pred_cbp(
    mb: &mut MacroblockInfo,
    pred: &mut PredContext,
    mb_x: usize,
    at_left: bool,
    at_top: bool,
    color_format: ColorFormat,
    channels: usize,
    model: &mut CbpModel,
) {
    let planes = if color_format.is_chroma_subsampled() { 1 } else { channels };

    for ch in 0..planes.min(MAX_CHANNELS) {
        let value = pred_cbp_luma(mb.diff_cbp[ch], pred, mb_x, ch, at_left, at_top, model);
        mb.cbp[ch] = value;
        pred.current[ch][mb_x].cbp = value;
    }

    if color_format == ColorFormat::Yuv422 {
        for ch in 1..3 {
            let value = pred_cbp_422(mb.diff_cbp[ch], pred, mb_x, ch, at_left, at_top, model);
            mb.cbp[ch] = value;
            pred.current[ch][mb_x].cbp = value;
        }
    } else if color_format == ColorFormat::Yuv420 {
        for ch in 1..3 {
            let value = pred_cbp_420(mb.diff_cbp[ch], pred, mb_x, ch, at_left, at_top, model);
            mb.cbp[ch] = value;
            pred.current[ch][mb_x].cbp = value;
        }
    }
}

pub fn update_pred_info(
    mb: &MacroblockInfo,
    pred: &mut PredContext,
    mb_x: usize,
    color_format: ColorFormat,
    channels: usize,
) {
    let planes = if color_format.is_chroma_subsampled() { 1 } else { channels };

    for ch in 0..planes.min(pred.current.len()) {
        let block = &mb.block_dc[ch];
        let info = &mut pred.current[ch][mb_x];

        info.dc = block[0];
        info.qp_index = i32::from(mb.qindex_lp);
        info.ad[0] = block[1];
        info.ad[1] = block[2];
        info.ad[2] = block[3];
        info.ad[3] = block[4];
        info.ad[4] = block[8];
        info.ad[5] = block[12];
    }

    if color_format == ColorFormat::Yuv420 {
        for ch in 1..3.min(pred.current.len()) {
            let block = &mb.block_dc[ch];
            let info = &mut pred.current[ch][mb_x];
            info.dc = block[0];
            info.qp_index = i32::from(mb.qindex_lp);
            info.ad[0] = block[1];
            info.ad[1] = block[2];
        }
    } else if color_format == ColorFormat::Yuv422 {
        for ch in 1..3.min(pred.current.len()) {
            let block = &mb.block_dc[ch];
            let info = &mut pred.current[ch][mb_x];
            info.dc = block[0];
            info.qp_index = i32::from(mb.qindex_lp);
            info.ad[0] = block[1];
            info.ad[1] = block[2];
            info.ad[2] = block[5];
            info.ad[3] = block[6];
            info.ad[4] = block[4];
        }
    }
}
