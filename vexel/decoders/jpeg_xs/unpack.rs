use super::bitreader::XsBitReader;
use super::ids::Ids;
use super::precinct::{compute_gtli_tables, Precinct, SIGN_BIT_MASK};
use super::types::*;

pub struct Unpacker {
    pub gtli_data: Vec<i32>,
    pub gtli_gcli: Vec<i32>,
    gclis_pred: Vec<i32>,
    inclusion_mask: Vec<u8>,
    line_masks: Vec<Vec<u8>>,
    line_mask_valid: Vec<bool>,
    sig_flags: Vec<u8>,
    gclis_scratch: Vec<i8>,
    use_sign_subpkt: bool,
    s_s: usize,
    qpih: u8,
    rm: u8,
}

pub struct PrecinctResult {
    pub gtli_gcli: Vec<i32>,
    pub truncated: bool,
}

impl Unpacker {
    pub fn new(config: &Config, ids: &Ids) -> Unpacker {
        let max_width = ids.band_max_width.max(0) as usize;
        let n_g = config.n_g.max(1) as usize;
        let groups = max_width.div_ceil(n_g) + 1;

        Unpacker {
            gtli_data: Vec::with_capacity(ids.nbands),
            gtli_gcli: Vec::with_capacity(ids.nbands),
            gclis_pred: vec![0; max_width + n_g],
            inclusion_mask: vec![0; max_width + n_g],
            line_masks: vec![Vec::new(); ids.pi.len()],
            line_mask_valid: vec![false; ids.pi.len()],
            sig_flags: vec![0; groups + 1],
            gclis_scratch: Vec::with_capacity(groups + 1),
            use_sign_subpkt: config.fs == 1,
            s_s: config.s_s.max(1) as usize,
            qpih: config.qpih,
            rm: config.rm,
        }
    }

    pub fn unpack_precinct(
        &mut self,
        reader: &mut XsBitReader,
        ids: &Ids,
        precinct: &mut Precinct,
        precinct_top: Option<&Precinct>,
        gtli_table_top: &[i32],
        config: &Config,
    ) -> PrecinctResult {
        precinct.clear();
        self.line_mask_valid.iter_mut().for_each(|v| *v = false);

        let position_count = ids.pi.len();
        let long_headers = ids.use_long_precinct_headers;

        let lprc = (reader.read(PREC_HDR_PREC_SIZE) as i64) << 3;
        let quantization = reader.read(PREC_HDR_QUANTIZATION_SIZE) as i32;
        let refinement = reader.read(PREC_HDR_REFINEMENT_SIZE) as i32;

        let mut gcli_methods = Vec::with_capacity(precinct.band_count);
        for _ in 0..precinct.band_count {
            let signaling = reader.read(GCLI_METHOD_NBITS) as u8;
            gcli_methods.push(GcliMethod::from_signaling(signaling, self.rm));
        }

        reader.align(PREC_HDR_ALIGNMENT);

        let bitpos_at_prc_data = reader.consumed_bits();

        compute_gtli_tables(
            quantization,
            refinement,
            precinct.band_count,
            &config.lvl_gains,
            &config.lvl_priorities,
            &mut self.gtli_data,
            &mut self.gtli_gcli,
        );

        let mut truncated = false;
        let mut idx_start = 0usize;

        for idx_stop in 0..position_count {
            let same_subpkt = idx_stop + 1 < position_count
                && ids.pi.get(idx_stop).map(|p| p.subpkt) == ids.pi.get(idx_stop + 1).map(|p| p.subpkt);

            if same_subpkt {
                continue;
            }

            let Some(first) = ids.pi.get(idx_start) else {
                break;
            };

            let first_band = first.band;
            let first_ypos = (first.y - ids.l0.get(first_band).copied().unwrap_or(0)).max(0) as usize;

            if !self.is_line_present(ids, precinct, first_band, first_ypos) {
                idx_start = idx_stop + 1;
                continue;
            }

            if reader.exhausted() {
                truncated = true;
                break;
            }

            let uses_raw_fallback = reader.read(1) & 1 != 0;

            let data_len = reader.read(if long_headers { PKT_HDR_DATA_SIZE_LONG } else { PKT_HDR_DATA_SIZE_SHORT });
            let gcli_len = reader.read(if long_headers { PKT_HDR_GCLI_SIZE_LONG } else { PKT_HDR_GCLI_SIZE_SHORT });
            let sign_len = reader.read(if long_headers { PKT_HDR_SIGN_SIZE_LONG } else { PKT_HDR_SIGN_SIZE_SHORT });

            reader.align(PKT_HDR_ALIGNMENT);

            if !uses_raw_fallback {
                self.unpack_significance(reader, ids, precinct, &gcli_methods, idx_start, idx_stop);
            }
            reader.align(SUBPKT_ALIGNMENT);

            let gcli_start = reader.consumed_bits();

            for idx in idx_start..=idx_stop.min(position_count.saturating_sub(1)) {
                let Some(packet) = ids.pi.get(idx) else {
                    continue;
                };

                let band = packet.band;
                let ypos = (packet.y - ids.l0.get(band).copied().unwrap_or(0)).max(0) as usize;

                if !self.is_line_present(ids, precinct, band, ypos) {
                    continue;
                }

                let method = if uses_raw_fallback {
                    GcliMethod::RAW
                } else {
                    gcli_methods.get(band).copied().unwrap_or(GcliMethod::RAW)
                };

                self.unpack_gclis(reader, precinct, precinct_top, gtli_table_top, band, ypos, idx, method);
            }

            reader.align(SUBPKT_ALIGNMENT);

            if reader.consumed_bits() - gcli_start != gcli_len * 8 {
                truncated = true;
                reader.seek_bits(gcli_start + gcli_len * 8);
            }

            let data_start = reader.consumed_bits();

            for idx in idx_start..=idx_stop.min(position_count.saturating_sub(1)) {
                let Some(packet) = ids.pi.get(idx) else {
                    continue;
                };

                let band = packet.band;
                let ypos = (packet.y - ids.l0.get(band).copied().unwrap_or(0)).max(0) as usize;

                if !self.is_line_present(ids, precinct, band, ypos) {
                    continue;
                }

                let gtli = self.gtli_data.get(band).copied().unwrap_or(0);
                self.unpack_data(reader, precinct, band, ypos, gtli);
            }

            reader.align(SUBPKT_ALIGNMENT);

            if reader.consumed_bits() - data_start != data_len * 8 {
                truncated = true;
                reader.seek_bits(data_start + data_len * 8);
            }

            if self.use_sign_subpkt {
                let sign_start = reader.consumed_bits();

                for idx in idx_start..=idx_stop.min(position_count.saturating_sub(1)) {
                    let Some(packet) = ids.pi.get(idx) else {
                        continue;
                    };

                    let band = packet.band;
                    let ypos = (packet.y - ids.l0.get(band).copied().unwrap_or(0)).max(0) as usize;

                    if !self.is_line_present(ids, precinct, band, ypos) {
                        continue;
                    }

                    self.unpack_sign(reader, precinct, band, ypos);
                }

                reader.align(SUBPKT_ALIGNMENT);

                if reader.consumed_bits() - sign_start != sign_len * 8 {
                    truncated = true;
                    reader.seek_bits(sign_start + sign_len * 8);
                }
            }

            idx_start = idx_stop + 1;
        }

        if lprc > 0 {
            let target = bitpos_at_prc_data.saturating_add(lprc as u64);
            if target >= reader.consumed_bits() {
                reader.seek_bits(target);
            } else {
                truncated = true;
            }
        }

        if reader.overrun() {
            truncated = true;
        }

        precinct.dequantize(&self.gtli_data, self.qpih);

        PrecinctResult { gtli_gcli: self.gtli_gcli.clone(), truncated }
    }

    fn is_line_present(&self, ids: &Ids, precinct: &Precinct, band: usize, ypos: usize) -> bool {
        let is_last_y = precinct.y_idx + 1 >= ids.npy;
        (ypos as i32) < ids.band_height_in_precinct(band, is_last_y) && precinct.index_of(band, ypos).is_some()
    }

    fn unpack_significance(
        &mut self,
        reader: &mut XsBitReader,
        ids: &Ids,
        precinct: &Precinct,
        methods: &[GcliMethod],
        idx_start: usize,
        idx_stop: usize,
    ) {
        for idx in idx_start..=idx_stop.min(ids.pi.len().saturating_sub(1)) {
            let Some(packet) = ids.pi.get(idx) else {
                continue;
            };

            let band = packet.band;
            let ypos = (packet.y - ids.l0.get(band).copied().unwrap_or(0)).max(0) as usize;

            if !self.is_line_present(ids, precinct, band, ypos) {
                continue;
            }

            let Some(method) = methods.get(band) else {
                continue;
            };

            if !method.uses_sig_flags() {
                continue;
            }

            let gcli_width = precinct.gcli_width(band);
            let groups = gcli_width.div_ceil(self.s_s);

            if self.sig_flags.len() < groups {
                self.sig_flags.resize(groups, 0);
            }

            for i in 0..groups {
                let bit = reader.read(1);
                self.sig_flags[i] = if bit == 0 { 1 } else { 0 };
            }

            if idx >= self.line_masks.len() {
                self.line_masks.resize(idx + 1, Vec::new());
                self.line_mask_valid.resize(idx + 1, false);
            }

            let mask = &mut self.line_masks[idx];
            mask.clear();
            mask.reserve(gcli_width);

            for i in 0..gcli_width {
                mask.push(self.sig_flags[i / self.s_s]);
            }

            self.line_mask_valid[idx] = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn unpack_gclis(
        &mut self,
        reader: &mut XsBitReader,
        precinct: &mut Precinct,
        precinct_top: Option<&Precinct>,
        gtli_table_top: &[i32],
        band: usize,
        ypos: usize,
        position: usize,
        method: GcliMethod,
    ) {
        let gcli_width = precinct.gcli_width(band);

        if gcli_width == 0 {
            return;
        }

        let Some(idx) = precinct.index_of(band, ypos) else {
            return;
        };

        if method.is_raw() {
            let Some(gclis) = precinct.gclis.get_mut(idx) else {
                return;
            };

            for value in gclis.iter_mut().take(gcli_width) {
                *value = (reader.read(4) & 0xf) as i8;
            }

            return;
        }

        let gtli = self.gtli_gcli.get(band).copied().unwrap_or(0);
        let gtli_top = if ypos == 0 { gtli_table_top.get(band).copied().unwrap_or(gtli) } else { gtli };

        let top_loc = Self::gclis_top_for(precinct, precinct_top, band, ypos);

        if self.inclusion_mask.len() < gcli_width {
            self.inclusion_mask.resize(gcli_width, 0);
        }

        if method.uses_sig_flags() {
            let mask = if self.line_mask_valid.get(position).copied().unwrap_or(false) {
                self.line_masks.get(position).map(|m| m.as_slice()).unwrap_or(&[])
            } else {
                &[]
            };

            for i in 0..gcli_width {
                self.inclusion_mask[i] = mask.get(i).copied().unwrap_or(0);
            }
        } else {
            for value in self.inclusion_mask.iter_mut().take(gcli_width) {
                *value = 1;
            }
        }

        if self.gclis_pred.len() < gcli_width {
            self.gclis_pred.resize(gcli_width, 0);
        }

        let has_top = top_loc.is_some();
        let no_prediction = method.prediction == Prediction::None
            || (method.prediction == Prediction::Vertical
                && method.alphabet == Alphabet::UnaryUnsignedBounded
                && !has_top);

        if no_prediction {
            for i in 0..gcli_width {
                self.gclis_pred[i] = if self.inclusion_mask[i] != 0 { reader.read_unary_unsigned() } else { 0 };
            }
        } else {
            let top = match top_loc {
                Some((true, tidx)) => precinct_top.and_then(|p| p.gclis.get(tidx)).map(|v| v.as_slice()).unwrap_or(&[]),
                Some((false, tidx)) => precinct.gclis.get(tidx).map(|v| v.as_slice()).unwrap_or(&[]),
                None => &[],
            };

            for i in 0..gcli_width {
                if self.inclusion_mask[i] == 0 {
                    self.gclis_pred[i] = 0;
                    continue;
                }

                let top_value = top.get(i).copied().unwrap_or(0) as i32;
                let predictor = predictor_of(top_value, gtli_top, gtli);
                let min_allowed = -(predictor - gtli).max(0);

                self.gclis_pred[i] = reader.read_bounded_code(min_allowed);
            }
        }

        let zrcsf = method.run == RunMode::SigFlagsZrcsf;

        if method.prediction == Prediction::Vertical && has_top {
            self.gclis_scratch.clear();

            match top_loc {
                Some((true, tidx)) => {
                    if let Some(v) = precinct_top.and_then(|p| p.gclis.get(tidx)) {
                        self.gclis_scratch.extend_from_slice(v);
                    }
                }
                Some((false, tidx)) => {
                    if let Some(v) = precinct.gclis.get(tidx) {
                        self.gclis_scratch.extend_from_slice(v);
                    }
                }
                None => {}
            }

            let top = std::mem::take(&mut self.gclis_scratch);

            let Some(gclis) = precinct.gclis.get_mut(idx) else {
                self.gclis_scratch = top;
                return;
            };

            for i in 0..gcli_width.min(gclis.len()) {
                let top_value = top.get(i).copied().unwrap_or(0) as i32;
                let predictor = predictor_of(top_value, gtli_top, gtli);
                let mut value = predictor + self.gclis_pred[i];

                if value <= gtli {
                    value -= gtli;
                }

                if zrcsf && self.inclusion_mask[i] == 0 {
                    value = 0;
                }

                gclis[i] = value.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
            }

            self.gclis_scratch = top;
        } else {
            let Some(gclis) = precinct.gclis.get_mut(idx) else {
                return;
            };

            for i in 0..gcli_width.min(gclis.len()) {
                let value = if self.gclis_pred[i] > 0 { self.gclis_pred[i] + gtli } else { 0 };
                gclis[i] = value.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
            }
        }
    }

    fn gclis_top_for(
        precinct: &Precinct,
        precinct_top: Option<&Precinct>,
        band: usize,
        ypos: usize,
    ) -> Option<(bool, usize)> {
        if ypos == 0 {
            let top = precinct_top?;
            let mut ylast = None;

            for y in (0..top.precinct_height).rev() {
                if top.index_of(band, y).is_some() {
                    ylast = Some(y);
                    break;
                }
            }

            let idx = top.index_of(band, ylast?)?;
            top.gclis.get(idx)?;
            Some((true, idx))
        } else {
            let idx = precinct.index_of(band, ypos - 1)?;
            precinct.gclis.get(idx)?;
            Some((false, idx))
        }
    }

    fn unpack_data(&mut self, reader: &mut XsBitReader, precinct: &mut Precinct, band: usize, ypos: usize, gtli: i32) {
        let Some(idx) = precinct.index_of(band, ypos) else {
            return;
        };

        let width = precinct.line_width(band);
        let group_size = precinct.group_size();
        let sign_packed = self.use_sign_subpkt;

        if precinct.gclis.get(idx).is_none() {
            return;
        }

        let mut gclis = std::mem::take(&mut self.gclis_scratch);
        gclis.clear();
        gclis.extend_from_slice(&precinct.gclis[idx]);

        let Some(data) = precinct.data.get_mut(idx) else {
            self.gclis_scratch = gclis;
            return;
        };

        let group_count = width.div_ceil(group_size);

        for group in 0..group_count {
            let start = group * group_size;
            let end = (start + group_size).min(data.len());

            for value in &mut data[start..end] {
                *value = 0;
            }

            let gcli = gclis.get(group).copied().unwrap_or(0) as i32;

            if gcli <= gtli {
                continue;
            }

            let in_range = start + group_size <= data.len();

            if !sign_packed {
                if in_range && group_size <= 57 {
                    let bits = reader.read(group_size as u32);
                    for (k, value) in data[start..start + group_size].iter_mut().enumerate() {
                        let bit = (bits >> (group_size - 1 - k)) & 1;
                        *value |= (bit as u32) << 31;
                    }
                } else {
                    for i in start..(start + group_size) {
                        let bit = reader.read(1);
                        if let Some(value) = data.get_mut(i) {
                            *value |= (bit as u32) << 31;
                        }
                    }
                }
            }

            let mut bp = gcli - 1;
            while bp >= gtli {
                if in_range && group_size <= 57 && bp < 31 {
                    let bits = reader.read(group_size as u32);
                    for (k, value) in data[start..start + group_size].iter_mut().enumerate() {
                        let bit = (bits >> (group_size - 1 - k)) & 1;
                        *value |= (bit as u32) << bp;
                    }
                } else {
                    for i in start..(start + group_size) {
                        let bit = reader.read(1);
                        if let Some(value) = data.get_mut(i) {
                            if bp < 31 {
                                *value |= ((bit & 1) as u32) << bp;
                            }
                        }
                    }
                }
                bp -= 1;
            }
        }

        self.gclis_scratch = gclis;
    }

    fn unpack_sign(&mut self, reader: &mut XsBitReader, precinct: &mut Precinct, band: usize, ypos: usize) {
        let Some(idx) = precinct.index_of(band, ypos) else {
            return;
        };

        let width = precinct.line_width(band);
        let group_size = precinct.group_size();

        let Some(data) = precinct.data.get_mut(idx) else {
            return;
        };

        let group_count = width.div_ceil(group_size);

        for group in 0..group_count {
            let start = group * group_size;

            for i in start..(start + group_size) {
                let Some(&value) = data.get(i) else {
                    continue;
                };

                if value == 0 {
                    continue;
                }

                let bit = reader.read(1);

                if i < width {
                    if let Some(slot) = data.get_mut(i) {
                        *slot |= ((bit & 1) as u32) << 31;
                    }
                }
            }
        }
    }
}

fn predictor_of(gcli_top: i32, gtli_top: i32, gtli: i32) -> i32 {
    gcli_top.max(gtli.max(gtli_top))
}

pub fn sign_magnitude_to_value(value: u32) -> i32 {
    let magnitude = (value & !SIGN_BIT_MASK) as i32;
    if value & SIGN_BIT_MASK != 0 {
        -magnitude
    } else {
        magnitude
    }
}
