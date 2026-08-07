use crate::decoders::jpeg_xr::adapthuff::{get_huff, get_huff_short, AdaptiveHuffman};
use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::predict::MacroblockInfo;
use crate::decoders::jpeg_xr::tables::*;
use crate::decoders::jpeg_xr::types::*;
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

const MAXTOTAL: u32 = 32767;
const MODELWEIGHT: i32 = 70;

#[derive(Debug, Clone, Copy)]
pub struct AdaptiveScan {
    pub total: u32,
    pub scan: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct AdaptiveModel {
    pub flc_state: [i32; 2],
    pub flc_bits: [i32; 2],
    pub band: Band,
}

impl AdaptiveModel {
    fn new(band: Band) -> Self {
        let flc_bits = match band {
            Band::Lp => [4, 4],
            Band::Dc => [8, 8],
            _ => [0, 0],
        };
        AdaptiveModel { flc_state: [0, 0], flc_bits, band }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CbpModel {
    pub count0: [i32; 2],
    pub count1: [i32; 2],
    pub state: [i32; 2],
}

impl Default for CbpModel {
    fn default() -> Self {
        CbpModel { count0: [-4, -4], count1: [4, 4], state: [0, 0] }
    }
}

pub struct CodingContext {
    pub cbpcy: AdaptiveHuffman,
    pub cbpcy1: AdaptiveHuffman,
    pub expt: Vec<AdaptiveHuffman>,
    pub scan_lowpass: [AdaptiveScan; 16],
    pub scan_horiz: [AdaptiveScan; 16],
    pub scan_vert: [AdaptiveScan; 16],
    pub model_ac: AdaptiveModel,
    pub model_lp: AdaptiveModel,
    pub model_dc: AdaptiveModel,
    pub cbp_count_zero: i32,
    pub cbp_count_max: i32,
    pub cbp_model: CbpModel,
    pub trim_flexbits: i32,
}

fn scan_totals() -> [u32; 16] {
    let mut totals = [0u32; 16];
    totals[0] = MAXTOTAL;
    let mut weight = 32i32;
    for total in totals.iter_mut().skip(1) {
        *total = weight.max(0) as u32;
        weight -= 2;
    }
    totals
}

fn init_scan(order: &[usize; 16], permute: bool) -> [AdaptiveScan; 16] {
    let totals = scan_totals();
    let mut scan = [AdaptiveScan { total: 0, scan: 0 }; 16];

    for k in 0..16 {
        scan[k].scan = if permute { DCT_INDEX_AC_444[order[k]] } else { order[k] };
        scan[k].total = totals[k];
    }

    scan
}

const VLC_ALPHABET: [usize; NUM_VLC_TABLES] = [5, 4, 8, 7, 7, 12, 6, 6, 12, 6, 6, 7, 7, 12, 6, 6, 12, 6, 6, 7, 7];

impl CodingContext {
    pub fn new(color_format: ColorFormat, trim_flexbits: i32) -> Self {
        let cbp_size = if matches!(color_format, ColorFormat::YOnly | ColorFormat::NComponent | ColorFormat::Cmyk) {
            5
        } else {
            9
        };

        let expt = VLC_ALPHABET.iter().map(|&n| AdaptiveHuffman::new(n)).collect();

        CodingContext {
            cbpcy: AdaptiveHuffman::new(cbp_size),
            cbpcy1: AdaptiveHuffman::new(5),
            expt,
            scan_lowpass: init_scan(&ZIGZAG_INV_4X4_LOWPASS, false),
            scan_horiz: init_scan(&ZIGZAG_INV_4X4_H, true),
            scan_vert: init_scan(&ZIGZAG_INV_4X4_V, true),
            model_ac: AdaptiveModel::new(Band::Ac),
            model_lp: AdaptiveModel::new(Band::Lp),
            model_dc: AdaptiveModel::new(Band::Dc),
            cbp_count_zero: 1,
            cbp_count_max: 1,
            cbp_model: CbpModel::default(),
            trim_flexbits,
        }
    }

    pub fn reset_scan_totals(&mut self) {
        for (scan, total) in self.scan_lowpass.iter_mut().zip(scan_totals()) {
            scan.total = total;
        }
    }

    pub fn reset_highpass_scan_totals(&mut self) {
        for ((horiz, vert), total) in self.scan_horiz.iter_mut().zip(self.scan_vert.iter_mut()).zip(scan_totals()) {
            horiz.total = total;
            vert.total = total;
        }
    }

    pub fn adapt_lowpass(&mut self) {
        for i in 0..(CONTEXTX + CTDC).min(self.expt.len()) {
            self.expt[i].adapt();
        }
    }

    pub fn adapt_highpass(&mut self) {
        self.cbpcy.adapt();
        self.cbpcy1.adapt();
        for i in 0..CONTEXTX {
            let index = i + CONTEXTX + CTDC;
            if index < self.expt.len() {
                self.expt[index].adapt();
            }
        }
    }

    pub fn adapt_dc(&mut self) {
        for i in 2..5 {
            if i < self.expt.len() {
                self.expt[i].adapt();
            }
        }
    }
}

pub fn update_model_mb(
    color_format: ColorFormat,
    channels: usize,
    laplacian_mean: [i32; 2],
    model: &mut AdaptiveModel,
) {
    const WEIGHT0: [i32; 3] = [240, 12, 1];
    const WEIGHT1: [[i32; MAX_CHANNELS]; 3] = [
        [0, 240, 120, 80, 60, 48, 40, 34, 30, 27, 24, 22, 20, 18, 17, 16],
        [0, 12, 6, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1],
        [0, 16, 8, 5, 4, 3, 3, 2, 2, 2, 2, 1, 1, 1, 1, 1],
    ];
    const WEIGHT2: [i32; 6] = [120, 37, 2, 120, 18, 1];

    let band_index = match model.band {
        Band::Dc => 0usize,
        Band::Lp => 1,
        _ => 2,
    };

    let mut lap_mean = laplacian_mean;
    lap_mean[0] *= WEIGHT0[band_index];

    match color_format {
        ColorFormat::Yuv420 => lap_mean[1] *= WEIGHT2[band_index],
        ColorFormat::Yuv422 => lap_mean[1] *= WEIGHT2[3 + band_index],
        _ => {
            let channel_index = channels.clamp(1, MAX_CHANNELS) - 1;
            lap_mean[1] *= WEIGHT1[band_index][channel_index];
            if model.band == Band::Ac {
                lap_mean[1] >>= 4;
            }
        }
    }

    let states = model.flc_state.iter_mut().zip(model.flc_bits.iter_mut()).zip(lap_mean);

    for ((flc_state, flc_bits), mean) in states {
        let mut state = *flc_state;
        let mut delta = (mean - MODELWEIGHT) >> 2;

        if delta <= -8 {
            delta += 4;
            if delta < -16 {
                delta = -16;
            }
            state += delta;
            if state < -8 {
                if *flc_bits == 0 {
                    state = -8;
                } else {
                    state = 0;
                    *flc_bits -= 1;
                }
            }
        } else if delta >= 8 {
            delta -= 4;
            if delta > 15 {
                delta = 15;
            }
            state += delta;
            if state > 8 {
                if *flc_bits >= 15 {
                    *flc_bits = 15;
                    state = 8;
                } else {
                    state = 0;
                    *flc_bits += 1;
                }
            }
        }

        *flc_state = state;

        if color_format == ColorFormat::YOnly {
            break;
        }
    }
}

pub fn decode_significant_abs_level(huff: &mut AdaptiveHuffman, reader: &mut XrBitReader) -> i32 {
    const REMAP: [i32; 6] = [2, 3, 4, 6, 10, 14];
    const FIXED_LENGTH: [u32; 6] = [0, 0, 1, 2, 2, 2];

    let index = get_huff(huff.lookup(), reader);
    huff.discriminant_add(index);

    if index < 2 {
        return index as i32 + 2;
    }

    if index < 6 {
        let fixed = FIXED_LENGTH[index];
        return REMAP[index] + reader.read_i32(fixed);
    }

    let mut fixed = reader.read(4) + 4;
    if fixed == 19 {
        fixed += reader.read(2);
        if fixed == 22 {
            fixed += reader.read(3);
        }
    }

    let mut level = 2i32.wrapping_add((1i64 << fixed.min(30)) as i32);
    level = level.wrapping_add(reader.read_i32(fixed.min(31)));
    level
}

pub fn decode_significant_run(max_run: i32, huff: &mut AdaptiveHuffman, reader: &mut XrBitReader) -> i32 {
    const REMAP: [i32; 15] = [1, 2, 3, 5, 7, 1, 2, 3, 5, 7, 1, 2, 3, 4, 5];

    if max_run < 5 {
        if max_run == 1 {
            return 1;
        }
        if reader.read_bool() {
            return 1;
        }
        if max_run == 2 || reader.read_bool() {
            return 2;
        }
        if max_run == 3 || reader.read_bool() {
            return 3;
        }
        return 4;
    }

    let bin = SIGNIFICANT_RUN_BIN.get(max_run as usize).copied().unwrap_or(0).max(0);
    let index = get_huff_short(huff.lookup(), reader) + (bin as usize) * 5;

    let mut run = REMAP.get(index).copied().unwrap_or(1);
    let flc = SIGNIFICANT_RUN_FIXED_LENGTH.get(index).copied().unwrap_or(0);

    if flc > 0 {
        run += reader.read_i32(flc as u32);
    }

    run
}

pub fn decode_qp_index(reader: &mut XrBitReader, bits: u8) -> u8 {
    if !reader.read_bool() {
        return 0;
    }
    (reader.read(u32::from(bits)) + 1) as u8
}

fn decode_block(
    chroma: bool,
    local_coef: &mut [i32; 32],
    context: &mut CodingContext,
    context_offset: usize,
    reader: &mut XrBitReader,
    mut location: i32,
) -> usize {
    let base = context_offset + usize::from(chroma) * 3;

    let index = decode_first_index_at(context, base, reader);
    let mut sr = (index & 1) as i32;
    let mut srn = (index >> 2) as i32;
    let mut cont = sr & srn;
    let mut sign = reader.read_sign();

    local_coef[1] = if index & 2 != 0 {
        let level = decode_significant_abs_level_at(context, 6 + context_offset + cont as usize, reader);
        (level ^ sign) - sign
    } else {
        1 | sign
    };

    local_coef[0] = 0;
    if sr == 0 {
        local_coef[0] = decode_significant_run_at(context, 0, 15 - location, reader);
    }
    location += local_coef[0] + 1;

    let mut num_nonzero = 1usize;

    while srn != 0 {
        if num_nonzero * 2 + 1 >= local_coef.len() {
            break;
        }

        sr = srn & 1;
        local_coef[num_nonzero * 2] = 0;
        if sr == 0 {
            local_coef[num_nonzero * 2] = decode_significant_run_at(context, 0, 15 - location, reader);
        }
        location += local_coef[num_nonzero * 2] + 1;

        let index = decode_index_at(context, base + cont as usize + 1, location, reader);
        srn = (index >> 1) as i32;
        cont &= srn;
        sign = reader.read_sign();

        local_coef[num_nonzero * 2 + 1] = if index & 1 != 0 {
            let level = decode_significant_abs_level_at(context, 6 + context_offset + cont as usize, reader);
            (level ^ sign) - sign
        } else {
            1 | sign
        };

        num_nonzero += 1;

        if reader.exhausted() {
            break;
        }
    }

    num_nonzero
}

fn decode_block_highpass(
    chroma: bool,
    context: &mut CodingContext,
    reader: &mut XrBitReader,
    qp: i32,
    coeffs: &mut [i32],
    vertical: bool,
) -> usize {
    let context_offset = CTDC + CONTEXTX;
    let base = context_offset + usize::from(chroma) * 3;

    let index = decode_first_index_at(context, base, reader);
    let mut sr = (index & 1) as i32;
    let mut srn = (index >> 2) as i32;
    let mut cont = sr & srn;
    let mut sign = reader.read_sign();

    let mut level = (qp ^ sign) - sign;
    if index & 2 != 0 {
        level =
            level.wrapping_mul(decode_significant_abs_level_at(context, 6 + context_offset + cont as usize, reader));
    }

    let mut loc: i32 = 1;
    if sr == 0 {
        loc += decode_significant_run_at(context, 0, 15 - loc, reader);
    }
    let mut loc = (loc & 0xf) as usize;

    store_highpass(context, coeffs, loc, level, vertical);
    loc = (loc + 1) & 0xf;

    let mut num_nonzero = 1usize;

    while srn != 0 {
        sr = srn & 1;
        if sr == 0 {
            loc += decode_significant_run_at(context, 0, 15 - loc as i32, reader) as usize;
            if loc >= 16 {
                return 16;
            }
        }

        let index = decode_index_at(context, base + cont as usize + 1, loc as i32 + 1, reader);
        srn = (index >> 1) as i32;
        cont &= srn;
        sign = reader.read_sign();

        let mut level = (qp ^ sign) - sign;
        if index & 1 != 0 {
            level = level.wrapping_mul(decode_significant_abs_level_at(
                context,
                6 + context_offset + cont as usize,
                reader,
            ));
        }

        store_highpass(context, coeffs, loc, level, vertical);

        loc = (loc + 1) & 0xf;
        num_nonzero += 1;

        if reader.exhausted() {
            break;
        }
    }

    num_nonzero
}

fn store_highpass(context: &mut CodingContext, coeffs: &mut [i32], loc: usize, level: i32, vertical: bool) {
    let scan = if vertical { &mut context.scan_vert } else { &mut context.scan_horiz };

    if let Some(&AdaptiveScan { scan: position, .. }) = scan.get(loc) {
        if let Some(slot) = coeffs.get_mut(position) {
            *slot = level;
        }
    }

    if loc < scan.len() {
        scan[loc].total += 1;
        if loc > 0 && scan[loc].total > scan[loc - 1].total {
            scan.swap(loc, loc - 1);
        }
    }
}

fn decode_first_index_at(context: &mut CodingContext, index: usize, reader: &mut XrBitReader) -> usize {
    if index >= context.expt.len() {
        return 0;
    }
    let lookup = context.expt[index].lookup();
    let symbol = get_huff(lookup, reader);
    context.expt[index].discriminant_add(symbol);
    context.expt[index].discriminant1_add(symbol);
    symbol
}

fn decode_index_at(context: &mut CodingContext, index: usize, location: i32, reader: &mut XrBitReader) -> usize {
    if location < 15 {
        if index >= context.expt.len() {
            return 0;
        }
        let lookup = context.expt[index].lookup();
        let symbol = get_huff_short(lookup, reader);
        context.expt[index].discriminant_add(symbol);
        context.expt[index].discriminant1_add(symbol);
        return symbol;
    }

    if location == 15 {
        if !reader.read_bool() {
            return 0;
        }
        if !reader.read_bool() {
            return 2;
        }
        return 1 + 2 * usize::from(reader.read_bool());
    }

    reader.read(1) as usize
}

fn decode_significant_abs_level_at(context: &mut CodingContext, index: usize, reader: &mut XrBitReader) -> i32 {
    if index >= context.expt.len() {
        return 1;
    }
    decode_significant_abs_level(&mut context.expt[index], reader)
}

fn decode_significant_run_at(context: &mut CodingContext, index: usize, max_run: i32, reader: &mut XrBitReader) -> i32 {
    if index >= context.expt.len() {
        return 1;
    }
    decode_significant_run(max_run, &mut context.expt[index], reader)
}

pub fn decode_macroblock_dc(
    context: &mut CodingContext,
    reader: &mut XrBitReader,
    mb: &mut MacroblockInfo,
    color_format: ColorFormat,
    channels: usize,
    subband: Subband,
    spatial: bool,
    bits_lp: u8,
    bits_hp: u8,
    num_qp_lp: u8,
    num_qp_hp: u8,
) -> VexelResult<()> {
    for ch in 0..channels.min(MAX_CHANNELS) {
        mb.block_dc[ch] = [0; MB_BLOCKS];
    }

    mb.qindex_lp = 0;
    mb.qindex_hp = 0;

    if spatial && subband != Subband::DcOnly {
        if bits_lp > 0 {
            mb.qindex_lp = decode_qp_index(reader, bits_lp);
        }
        if subband != Subband::NoHighpass && bits_hp > 0 {
            mb.qindex_hp = decode_qp_index(reader, bits_hp);
        }
    }

    if bits_hp == 0 && num_qp_hp > 1 {
        mb.qindex_hp = mb.qindex_lp;
    }

    if mb.qindex_lp >= num_qp_lp.max(1) || mb.qindex_hp >= num_qp_hp.max(1) {
        log_warn!(
            "JPEG XR: QP index out of range (lp {}/{}, hp {}/{}), clamping",
            mb.qindex_lp,
            num_qp_lp,
            mb.qindex_hp,
            num_qp_hp
        );
        mb.qindex_lp = mb.qindex_lp.min(num_qp_lp.saturating_sub(1));
        mb.qindex_hp = mb.qindex_hp.min(num_qp_hp.saturating_sub(1));
    }

    let mut laplacian = [0i32; 2];
    let mut model_bits = context.model_dc.flc_bits[0];

    if matches!(color_format, ColorFormat::YOnly | ColorFormat::Cmyk | ColorFormat::NComponent) {
        let mut slot = 0usize;
        for ch in 0..channels.min(MAX_CHANNELS) {
            let mut value = 0i32;
            if reader.read_bool() {
                value = decode_significant_abs_level_at(context, 3, reader) - 1;
                laplacian[slot] += 1;
            }
            if model_bits > 0 {
                value = value.wrapping_shl(model_bits as u32) | reader.read_i32(model_bits as u32);
            }
            if value != 0 && reader.read_bool() {
                value = -value;
            }
            mb.block_dc[ch][0] = value;

            slot = 1;
            model_bits = context.model_dc.flc_bits[1];
        }
    } else {
        let index = {
            let lookup = context.expt[2].lookup();
            get_huff(lookup, reader)
        };

        let mut dc_y = (index >> 2) as i32;
        let mut dc_u = ((index >> 1) & 1) as i32;
        let mut dc_v = (index & 1) as i32;

        if dc_y != 0 {
            dc_y = decode_significant_abs_level_at(context, 3, reader) - 1;
            laplacian[0] += 1;
        }
        if model_bits > 0 {
            dc_y = dc_y.wrapping_shl(model_bits as u32) | reader.read_i32(model_bits as u32);
        }
        if dc_y != 0 && reader.read_bool() {
            dc_y = -dc_y;
        }
        mb.block_dc[0][0] = dc_y;

        model_bits = context.model_dc.flc_bits[1];

        if dc_u != 0 {
            dc_u = decode_significant_abs_level_at(context, 4, reader) - 1;
            laplacian[1] += 1;
        }
        if model_bits > 0 {
            dc_u = dc_u.wrapping_shl(model_bits as u32) | reader.read_i32(model_bits as u32);
        }
        if dc_u != 0 && reader.read_bool() {
            dc_u = -dc_u;
        }
        mb.block_dc[1][0] = dc_u;

        if dc_v != 0 {
            dc_v = decode_significant_abs_level_at(context, 4, reader) - 1;
            laplacian[1] += 1;
        }
        if model_bits > 0 {
            dc_v = dc_v.wrapping_shl(model_bits as u32) | reader.read_i32(model_bits as u32);
        }
        if dc_v != 0 && reader.read_bool() {
            dc_v = -dc_v;
        }
        mb.block_dc[2][0] = dc_v;
    }

    update_model_mb(color_format, channels, laplacian, &mut context.model_dc);

    Ok(())
}

pub fn decode_macroblock_lowpass(
    context: &mut CodingContext,
    reader: &mut XrBitReader,
    mb: &mut MacroblockInfo,
    color_format: ColorFormat,
    channels: usize,
    spatial: bool,
    bits_lp: u8,
    num_qp_lp: u8,
    reset_totals: bool,
) -> VexelResult<()> {
    let full_planes = if color_format.is_chroma_subsampled() { 2 } else { channels };

    if !spatial && bits_lp > 0 {
        mb.qindex_lp = decode_qp_index(reader, bits_lp);
        if mb.qindex_lp >= num_qp_lp.max(1) {
            return Err(VexelError::Custom("JPEG XR: LP QP index out of range".to_string()));
        }
    }

    if reset_totals {
        context.reset_scan_totals();
    }

    let mut cbp = 0i32;

    if matches!(color_format, ColorFormat::Yuv420 | ColorFormat::Yuv422 | ColorFormat::Yuv444) {
        let mut count_max = context.cbp_count_max;
        let mut count_zero = context.cbp_count_zero;
        let max = full_planes as i32 * 4 - 5;

        if count_zero <= 0 || count_max < 0 {
            cbp = 0;
            if reader.read_bool() {
                cbp = 1;
                let k = reader.read_i32(full_planes as u32 - 1);
                if k != 0 {
                    cbp = k.wrapping_mul(2).wrapping_add(reader.read_i32(1));
                }
            }
            if count_max < count_zero {
                cbp = max - cbp;
            }
        } else {
            cbp = reader.read_i32(full_planes as u32);
        }

        count_max += 1 - 4 * i32::from(cbp == max);
        count_zero += 1 - 4 * i32::from(cbp == 0);
        context.cbp_count_max = count_max.clamp(-8, 7);
        context.cbp_count_zero = count_zero.clamp(-8, 7);
    } else {
        for ch in 0..channels {
            cbp |= reader.read_i32(1).wrapping_shl(ch as u32);
        }
    }

    let mut laplacian = [0i32; 2];
    let mut slot = 0usize;
    let mut model_bits = context.model_lp.flc_bits[0];
    let mut rl_coeffs = [0i32; 32];

    for channel in 0..full_planes {
        if cbp & 1 != 0 {
            let location = 1
                + 9 * i32::from(color_format == ColorFormat::Yuv420 && channel == 1)
                + i32::from(color_format == ColorFormat::Yuv422 && channel == 1);

            let num_nonzero = decode_block(channel > 0, &mut rl_coeffs, context, CTDC, reader, location);

            if color_format.is_chroma_subsampled() && channel > 0 {
                const REMAP: [usize; 7] = [4, 1, 2, 3, 5, 6, 7];
                let offset = usize::from(color_format == ColorFormat::Yuv420);
                let count = if color_format == ColorFormat::Yuv420 { 6 } else { 14 };

                laplacian[slot] += num_nonzero as i32;

                let mut temp = [0i32; 16];
                let mut index = 0usize;
                for k in 0..num_nonzero {
                    index += rl_coeffs[k * 2] as usize;
                    temp[index & 0xf] = rl_coeffs[k * 2 + 1];
                    index += 1;
                }

                for k in 0..count {
                    let target = (k & 1) + 1;
                    let position = REMAP[offset + (k >> 1)];
                    mb.block_dc[target][position] = temp[k];
                }
            } else {
                laplacian[slot] += num_nonzero as i32;
                let mut index = 1usize;

                for k in 0..num_nonzero {
                    index += rl_coeffs[k * 2] as usize;
                    if index >= 16 {
                        break;
                    }
                    let position = context.scan_lowpass[index].scan;
                    mb.block_dc[channel][position] = rl_coeffs[k * 2 + 1];
                    context.scan_lowpass[index].total += 1;
                    if index > 0 && context.scan_lowpass[index].total > context.scan_lowpass[index - 1].total {
                        context.scan_lowpass.swap(index, index - 1);
                    }
                    index += 1;
                }
            }
        }

        if model_bits > 0 {
            if color_format.is_chroma_subsampled() && channel > 0 {
                let count = if color_format == ColorFormat::Yuv420 { 4 } else { 8 };
                for k in 1..count {
                    for target in 1..3usize {
                        let value = mb.block_dc[target][k];
                        mb.block_dc[target][k] = refine_lowpass(value, model_bits, reader);
                    }
                }
            } else {
                for k in 1..16usize {
                    let value = mb.block_dc[channel][k];
                    mb.block_dc[channel][k] = refine_lowpass(value, model_bits, reader);
                }
            }
        }

        slot = 1;
        model_bits = context.model_lp.flc_bits[1];
        cbp >>= 1;
    }

    update_model_mb(color_format, channels, laplacian, &mut context.model_lp);

    Ok(())
}

fn refine_lowpass(value: i32, model_bits: i32, reader: &mut XrBitReader) -> i32 {
    let bits = model_bits as u32;

    let shifted = value.wrapping_shl(bits);

    if value > 0 {
        shifted.wrapping_add(reader.read_i32(bits))
    } else if value < 0 {
        shifted.wrapping_sub(reader.read_i32(bits))
    } else {
        reader.read_zero_or_signed(bits)
    }
}

pub fn decode_cbp(
    context: &mut CodingContext,
    reader: &mut XrBitReader,
    mb: &mut MacroblockInfo,
    color_format: ColorFormat,
    channels: usize,
) {
    let planes = if matches!(color_format, ColorFormat::NComponent | ColorFormat::Cmyk) { channels } else { 1 };

    for i in 0..planes.min(MAX_CHANNELS) {
        let mut cbp_y = 0i32;
        let mut cbp_u = 0i32;
        let mut cbp_v = 0i32;

        let mut num_cbp = {
            let lookup = context.cbpcy1.lookup();
            let symbol = get_huff_short(lookup, reader);
            context.cbpcy1.discriminant_add(symbol);
            symbol as i32
        };

        num_cbp = match num_cbp {
            2 => {
                let code = reader.read_i32(2);
                if code == 0 {
                    3
                } else if code == 1 {
                    5
                } else {
                    const TAB: [i32; 4] = [6, 9, 10, 12];
                    let idx = (code * 2 + i32::from(reader.read_bool()) - 4).clamp(0, 3);
                    TAB[idx as usize]
                }
            }
            1 => 1 << reader.read(2),
            3 => 0xf ^ (1 << reader.read(2)),
            4 => 0xf,
            other => other,
        };

        #[allow(clippy::needless_range_loop)]
        for block in 0..4usize {
            if num_cbp & (1 << block) == 0 {
                continue;
            }

            const FLC0: [u32; 6] = [0, 2, 1, 2, 2, 0];
            const OFF0: [usize; 6] = [0, 4, 2, 8, 12, 1];
            const OUT0: [i32; 16] = [0, 15, 3, 12, 1, 2, 4, 8, 5, 6, 9, 10, 7, 11, 13, 14];

            let symbol = {
                let lookup = context.cbpcy.lookup();
                let symbol = get_huff(lookup, reader);
                context.cbpcy.discriminant_add(symbol);
                symbol
            };

            let mut val = symbol + 1;
            let mut block_cbp = 0i32;

            if val >= 6 {
                if reader.read_bool() {
                    block_cbp = 0x10;
                } else if reader.read_bool() {
                    block_cbp = 0x20;
                } else {
                    block_cbp = 0x30;
                }

                if val == 9 {
                    if reader.read_bool() {
                    } else if reader.read_bool() {
                        val = 10;
                    } else {
                        val = 11;
                    }
                }
                val -= 6;
            }

            let mut code1 = OFF0.get(val).copied().unwrap_or(0);
            if let Some(&flc) = FLC0.get(val) {
                if flc > 0 {
                    code1 += reader.read(flc) as usize;
                }
            }
            block_cbp += OUT0.get(code1 & 0xf).copied().unwrap_or(0);

            match color_format {
                ColorFormat::Yuv444 => {
                    cbp_y |= (block_cbp & 0xf).wrapping_shl(block as u32 * 4);
                    for k in 0..2usize {
                        if (block_cbp >> (k + 4)) & 1 == 0 {
                            continue;
                        }

                        let code = {
                            let lookup = context.expt[1].lookup();
                            get_huff_short(lookup, reader) as i32
                        };

                        let code = match code {
                            1 => {
                                let c = reader.read_i32(2);
                                if c == 0 {
                                    3
                                } else if c == 1 {
                                    5
                                } else {
                                    const TAB: [i32; 4] = [6, 9, 10, 12];
                                    let idx = (c * 2 + i32::from(reader.read_bool()) - 4).clamp(0, 3);
                                    TAB[idx as usize]
                                }
                            }
                            0 => 1 << reader.read(2),
                            2 => 0xf ^ (1 << reader.read(2)),
                            3 => 0xf,
                            other => other,
                        };

                        if k == 0 {
                            cbp_u |= code.wrapping_shl(block as u32 * 4);
                        } else {
                            cbp_v |= code.wrapping_shl(block as u32 * 4);
                        }
                    }
                }
                ColorFormat::Yuv420 => {
                    cbp_y |= (block_cbp & 0xf).wrapping_shl(block as u32 * 4);
                    cbp_u |= ((block_cbp >> 4) & 0x1).wrapping_shl(block as u32);
                    cbp_v |= ((block_cbp >> 5) & 0x1).wrapping_shl(block as u32);
                }
                ColorFormat::Yuv422 => {
                    cbp_y |= (block_cbp & 0xf).wrapping_shl(block as u32 * 4);
                    const SHIFT: [i32; 4] = [0, 1, 4, 5];
                    for k in 0..2usize {
                        if (block_cbp >> (k + 4)) & 1 == 0 {
                            continue;
                        }
                        let mut code = 5i32;
                        if reader.read_bool() {
                            code = 1;
                        } else if reader.read_bool() {
                            code = 4;
                        }
                        code <<= SHIFT[block];
                        if k == 0 {
                            cbp_u |= code;
                        } else {
                            cbp_v |= code;
                        }
                    }
                }
                _ => {
                    cbp_y |= block_cbp.wrapping_shl(block as u32 * 4);
                }
            }
        }

        mb.diff_cbp[i] = cbp_y;

        if matches!(color_format, ColorFormat::Yuv420 | ColorFormat::Yuv422 | ColorFormat::Yuv444) {
            mb.diff_cbp[1] = cbp_u;
            mb.diff_cbp[2] = cbp_v;
        }
    }
}

fn decode_block_adaptive<'a>(
    no_skip: bool,
    chroma: bool,
    context: &mut CodingContext,
    reader: &mut XrBitReader<'a>,
    flex_reader: Option<&mut XrBitReader<'a>>,
    coeffs: &mut [i32],
    model_bits: i32,
    trim: i32,
    qp: i32,
    vertical: bool,
    skip_flexbits: bool,
) -> usize {
    let mut num_nonzero = 0usize;
    let mut flex = model_bits - trim;

    if flex < 0 || skip_flexbits {
        flex = 0;
    }

    if no_skip {
        let scaled_qp = qp.wrapping_shl(model_bits.clamp(0, 30) as u32);
        num_nonzero = decode_block_highpass(chroma, context, reader, scaled_qp, coeffs, vertical);
    }

    if flex > 0 {
        let flex_reader = flex_reader.unwrap_or(reader);
        let bits = flex as u32;

        if qp + trim == 1 {
            for &position in DCT_INDEX_AC_444.iter().skip(1) {
                let value = coeffs[position];
                coeffs[position] = if value < 0 {
                    value.wrapping_sub(flex_reader.read_i32(bits))
                } else if value > 0 {
                    value.wrapping_add(flex_reader.read_i32(bits))
                } else {
                    flex_reader.read_zero_or_signed(bits)
                };
            }
        } else {
            let scaled_qp = qp.wrapping_shl(trim.clamp(0, 30) as u32);
            for &position in DCT_INDEX_AC_444.iter().skip(1) {
                let value = coeffs[position];
                coeffs[position] = if value < 0 {
                    value.wrapping_sub(scaled_qp.wrapping_mul(flex_reader.read_i32(bits)))
                } else if value > 0 {
                    value.wrapping_add(scaled_qp.wrapping_mul(flex_reader.read_i32(bits)))
                } else {
                    scaled_qp.wrapping_mul(flex_reader.read_zero_or_signed(bits))
                };
            }
        }
    }

    num_nonzero
}

pub struct HighpassParams<'a> {
    pub color_format: ColorFormat,
    pub channels: usize,
    pub qp_hp: &'a [[Quantizer; MAX_CHANNELS]],
    pub qindex_hp: usize,
    pub skip_flexbits: bool,
    pub has_flexbits: bool,
}

pub fn decode_coeffs<'a>(
    context: &mut CodingContext,
    reader: &mut XrBitReader<'a>,
    flex_reader: Option<&mut XrBitReader<'a>>,
    mb: &MacroblockInfo,
    coeffs: &mut [Vec<i32>],
    params: &HighpassParams,
) -> VexelResult<()> {
    let cf = params.color_format;
    let channels = params.channels;
    let planes = if cf.is_chroma_subsampled() { 1 } else { channels };

    let mut model_bits = context.model_ac.flc_bits[0];
    let vertical = mb.orientation == 1;
    let mut chroma = false;
    let mut laplacian = [0i32; 2];
    let mut lm_slot = 0usize;

    let mut num_blocks = 4usize;
    let mut cbp_y = mb.cbp[0];

    if cf == ColorFormat::Yuv420 {
        num_blocks = 6;
        cbp_y += (mb.cbp[1] << 16) + (mb.cbp[2] << 20);
    } else if cf == ColorFormat::Yuv422 {
        num_blocks = 8;
        cbp_y += (mb.cbp[1] << 16) + (mb.cbp[2] << 24);
    }

    let mut flex_reader = flex_reader;

    for plane in 0..planes.min(coeffs.len()) {
        let mut index = 0usize;
        let mut cbp = cbp_y;

        for block in 0..num_blocks {
            let qp_channel = if planes > 1 {
                plane
            } else if block > 3 {
                if cf == ColorFormat::Yuv420 {
                    block - 3
                } else {
                    block / 2 - 1
                }
            } else {
                0
            };

            let qp = params
                .qp_hp
                .get(params.qindex_hp)
                .and_then(|set| set.get(qp_channel.min(MAX_CHANNELS - 1)))
                .map(|q| q.qp)
                .unwrap_or(1);

            for subblock in 0..4usize {
                let (target_plane, offset) = if block >= 4 {
                    if cf == ColorFormat::Yuv420 {
                        (block - 3, BLK_OFFSET_UV[subblock])
                    } else {
                        (1 + (1 & (block >> 1)), (block & 1) * 32 + BLK_OFFSET_UV_422[subblock])
                    }
                } else {
                    (plane, BLK_OFFSET[index & 0xf])
                };

                if target_plane < coeffs.len() && offset + 16 <= coeffs[target_plane].len() {
                    let no_skip = cbp & 1 != 0;
                    let block_coeffs = &mut coeffs[target_plane][offset..offset + 16];

                    let nonzero = decode_block_adaptive(
                        no_skip,
                        chroma,
                        context,
                        reader,
                        flex_reader.as_deref_mut(),
                        block_coeffs,
                        model_bits,
                        context.trim_flexbits,
                        qp,
                        vertical,
                        params.skip_flexbits || !params.has_flexbits,
                    );

                    if nonzero > 16 {
                        return Err(VexelError::Custom("JPEG XR: too many nonzero highpass coefficients".to_string()));
                    }

                    laplacian[lm_slot] += nonzero as i32;
                }

                index += 1;
                cbp >>= 1;
            }

            if block == 3 {
                model_bits = context.model_ac.flc_bits[1];
                lm_slot = 1;
                chroma = true;
            }
        }

        cbp_y = mb.cbp[(plane + 1) & 0xf];
    }

    update_model_mb(cf, channels, laplacian, &mut context.model_ac);

    Ok(())
}

pub fn dequantize_macroblock(
    mb: &MacroblockInfo,
    coeffs: &mut [Vec<i32>],
    color_format: ColorFormat,
    channels: usize,
    subband: Subband,
    qp_dc: &[Quantizer; MAX_CHANNELS],
    qp_lp: &[[Quantizer; MAX_CHANNELS]],
    qindex_lp: usize,
) {
    for ch in 0..channels.min(coeffs.len()) {
        let dc_qp = qp_dc.get(ch).map(|q| q.qp).unwrap_or(1);
        coeffs[ch][0] = mb.block_dc[ch][0].wrapping_mul(dc_qp);

        if subband == Subband::DcOnly {
            continue;
        }

        let lp_qp = qp_lp.get(qindex_lp).and_then(|set| set.get(ch)).map(|q| q.qp).unwrap_or(1);

        let block = &mb.block_dc[ch];

        if ch == 0 || !color_format.is_chroma_subsampled() {
            for i in 1..16usize {
                let position = DCT_INDEX_DC_444[i];
                if position < coeffs[ch].len() {
                    coeffs[ch][position] = block[i].wrapping_mul(lp_qp);
                }
            }
        } else if color_format == ColorFormat::Yuv422 {
            const POSITIONS: [usize; 7] = [64, 16, 80, 32, 96, 48, 112];
            for (i, &position) in POSITIONS.iter().enumerate() {
                if position < coeffs[ch].len() {
                    coeffs[ch][position] = block[i + 1].wrapping_mul(lp_qp);
                }
            }
        } else {
            const POSITIONS: [usize; 3] = [32, 16, 48];
            for (i, &position) in POSITIONS.iter().enumerate() {
                if position < coeffs[ch].len() {
                    coeffs[ch][position] = block[i + 1].wrapping_mul(lp_qp);
                }
            }
        }
    }
}
