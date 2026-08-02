use crate::decoders::jpeg2000::codestream::Codestream;
use crate::decoders::jpeg2000::dwt::{self, Line};
use crate::decoders::jpeg2000::ht::{HtCodeBlock, HtDecoder};
use crate::decoders::jpeg2000::t1::{CodeBlock, Segment, T1Decoder};
use crate::decoders::jpeg2000::t2::{self, HeaderReader, TagTree};
use crate::decoders::jpeg2000::types::*;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::limits::Limits;
use crate::{log_debug, log_warn};

struct CblkState {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    num_bps: u32,
    mb: u32,
    num_len_bits: u32,
    included: bool,
    segments: Vec<SegmentState>,
    data: Vec<u8>,
}

struct SegmentState {
    num_passes: u32,
    max_passes: u32,
    data_start: usize,
    data_len: usize,
}

struct Precinct {
    cw: u32,
    ch: u32,
    cblks: Vec<CblkState>,
    incl_tree: TagTree,
    imsb_tree: TagTree,
}

struct Band {
    orient: BandOrient,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    step_size: f32,
    num_bps: i32,
    precincts: Vec<Precinct>,
}

impl Band {
    fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

struct Resolution {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    pw: u32,
    ph: u32,
    pdx: u32,
    pdy: u32,
    bands: Vec<Band>,
}

struct TileComponent {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    resolutions: Vec<Resolution>,
    data: Vec<f32>,
}

impl TileComponent {
    fn width(&self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }

    fn height(&self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }
}

pub struct DecodedTile {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub components: Vec<Vec<f32>>,
}

pub struct TileDecoder<'a> {
    codestream: &'a Codestream,
    limits: &'a mut Limits,
    ppm_cursor: &'a mut usize,
}

impl<'a> TileDecoder<'a> {
    pub fn new(codestream: &'a Codestream, limits: &'a mut Limits, ppm_cursor: &'a mut usize) -> Self {
        Self { codestream, limits, ppm_cursor }
    }

    pub fn decode_tile(&mut self, tile_index: u32, data: &[u8]) -> VexelResult<DecodedTile> {
        let siz = &self.codestream.siz;
        let coding = self.tile_coding(tile_index);

        let tiles_x = siz.num_tiles_x().max(1);

        let tile_x = tile_index % tiles_x;
        let tile_y = tile_index / tiles_x;

        let tx0 = (siz.xt0siz.saturating_add(tile_x.saturating_mul(siz.xtsiz))).max(siz.x0siz) as i32;
        let ty0 = (siz.yt0siz.saturating_add(tile_y.saturating_mul(siz.ytsiz))).max(siz.y0siz) as i32;
        let tx1 = (siz.xt0siz.saturating_add((tile_x + 1).saturating_mul(siz.xtsiz))).min(siz.xsiz) as i32;
        let ty1 = (siz.yt0siz.saturating_add((tile_y + 1).saturating_mul(siz.ytsiz))).min(siz.ysiz) as i32;

        if tx1 <= tx0 || ty1 <= ty0 {
            return Err(VexelError::Custom(format!("JPEG 2000: tile {} is empty", tile_index)));
        }

        let mut components = Vec::with_capacity(siz.components.len());

        for (index, component) in siz.components.iter().enumerate() {
            let params = coding.get(index).cloned().unwrap_or_default();
            let tilec = self.build_component(tx0, ty0, tx1, ty1, component, &params)?;
            components.push(tilec);
        }

        let tile_bounds = t2::TileGeometry {
            tx0: tx0.max(0) as u32,
            ty0: ty0.max(0) as u32,
            tx1: tx1.max(0) as u32,
            ty1: ty1.max(0) as u32,
        };

        self.read_packets(&mut components, &coding, data, tile_index, tile_bounds)?;

        for (index, tilec) in components.iter_mut().enumerate() {
            let params = coding.get(index).cloned().unwrap_or_default();
            decode_code_blocks(tilec, &params);
            inverse_dwt(tilec, &params)?;
        }

        let mut planes = Vec::with_capacity(components.len());
        for tilec in components {
            planes.push(tilec.data);
        }

        let use_mct = self
            .codestream
            .tile_settings
            .get(tile_index as usize)
            .copied()
            .flatten()
            .map(|s| s.mct)
            .unwrap_or(self.codestream.mct);

        if use_mct && planes.len() >= 3 {
            let reversible = coding.first().map(|c| c.coding.reversible).unwrap_or(true);

            if reversible {
                inverse_rct_tile(&mut planes);
            } else {
                inverse_ict_tile(&mut planes);
            }
        }

        Ok(DecodedTile { x0: tx0, y0: ty0, x1: tx1, y1: ty1, components: planes })
    }

    fn tile_coding(&self, tile_index: u32) -> Vec<ComponentCoding> {
        self.codestream
            .tile_overrides
            .get(tile_index as usize)
            .and_then(|entry| entry.clone())
            .unwrap_or_else(|| self.codestream.defaults.clone())
    }

    fn build_component(
        &mut self,
        tx0: i32,
        ty0: i32,
        tx1: i32,
        ty1: i32,
        component: &ComponentInfo,
        params: &ComponentCoding,
    ) -> VexelResult<TileComponent> {
        let dx = component.dx as i32;
        let dy = component.dy as i32;

        let x0 = ceil_div_i32(tx0, dx);
        let y0 = ceil_div_i32(ty0, dy);
        let x1 = ceil_div_i32(tx1, dx);
        let y1 = ceil_div_i32(ty1, dy);

        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;

        self.limits.check_dimensions(width, height)?;
        self.limits.reserve_usize((width as usize) * (height as usize) * 4)?;

        let num_resolutions = params.coding.num_resolutions.max(1);
        let mut resolutions = Vec::with_capacity(num_resolutions as usize);

        for resno in 0..num_resolutions {
            let level = num_resolutions - 1 - resno;

            let rx0 = ceil_div_pow2(x0, level);
            let ry0 = ceil_div_pow2(y0, level);
            let rx1 = ceil_div_pow2(x1, level);
            let ry1 = ceil_div_pow2(y1, level);

            let (pdx, pdy) = params.coding.precinct_exponents(resno as usize);

            let prc_x0 = floor_div_pow2_signed(rx0, pdx) << pdx;
            let prc_y0 = floor_div_pow2_signed(ry0, pdy) << pdy;

            let (ppx, ppy, tlcbgx, tlcbgy) = if resno == 0 {
                (pdx, pdy, prc_x0, prc_y0)
            } else {
                (pdx - 1, pdy - 1, ceil_div_pow2_signed(prc_x0, 1), ceil_div_pow2_signed(prc_y0, 1))
            };

            let pw =
                if rx1 > rx0 { (ceil_div_pow2_signed(rx1, pdx) - floor_div_pow2_signed(rx0, pdx)) as u32 } else { 0 };
            let ph =
                if ry1 > ry0 { (ceil_div_pow2_signed(ry1, pdy) - floor_div_pow2_signed(ry0, pdy)) as u32 } else { 0 };

            let num_bands = if resno == 0 { 1 } else { 3 };
            let mut bands = Vec::with_capacity(num_bands);

            for bandno in 0..num_bands {
                let orient = BandOrient::from_index(if resno == 0 { 0 } else { bandno as u32 + 1 });

                let (bx0, by0, bx1, by1) = if resno == 0 {
                    (
                        ceil_div_pow2(x0, level),
                        ceil_div_pow2(y0, level),
                        ceil_div_pow2(x1, level),
                        ceil_div_pow2(y1, level),
                    )
                } else {
                    let x0b = orient.horizontal_high_pass() as i64;
                    let y0b = orient.vertical_high_pass() as i64;
                    (
                        ceil_div_pow2_i64(x0 as i64 - (x0b << level), level + 1),
                        ceil_div_pow2_i64(y0 as i64 - (y0b << level), level + 1),
                        ceil_div_pow2_i64(x1 as i64 - (x0b << level), level + 1),
                        ceil_div_pow2_i64(y1 as i64 - (y0b << level), level + 1),
                    )
                };

                let band_index = if resno == 0 { 0 } else { (3 * (resno - 1) + bandno as u32 + 1) as usize };
                let step = step_size_for(params, band_index);

                let log2_gain = if params.coding.reversible { orient.log2_gain() } else { 0 };

                let rb = component.prec as i32 + log2_gain;
                let step_size = ((1.0 + step.mant as f64 / 2048.0) * 2f64.powi(rb - step.expn)) as f32;
                let num_bps = step.expn + params.quant.guard_bits as i32 - 1;

                let precincts = build_precincts(bx0, by0, bx1, by1, pw, ph, tlcbgx, tlcbgy, ppx, ppy, params);

                bands.push(Band { orient, x0: bx0, y0: by0, x1: bx1, y1: by1, step_size, num_bps, precincts });
            }

            resolutions.push(Resolution { x0: rx0, y0: ry0, x1: rx1, y1: ry1, pw, ph, pdx, pdy, bands });
        }

        let data = vec![0f32; (width as usize) * (height as usize)];

        Ok(TileComponent { x0, y0, x1, y1, resolutions, data })
    }

    fn read_packets(
        &mut self,
        components: &mut [TileComponent],
        coding: &[ComponentCoding],
        data: &[u8],
        tile_index: u32,
        tile_bounds: t2::TileGeometry,
    ) -> VexelResult<()> {
        let geometry: Vec<t2::ComponentGeometry> = components
            .iter()
            .zip(self.codestream.siz.components.iter())
            .map(|(tilec, info)| t2::ComponentGeometry {
                dx: info.dx,
                dy: info.dy,
                resolutions: tilec
                    .resolutions
                    .iter()
                    .map(|r| t2::ResolutionGeometry { pdx: r.pdx, pdy: r.pdy, pw: r.pw, ph: r.ph })
                    .collect(),
            })
            .collect();

        let changes = match self.codestream.tile_progression_changes.get(tile_index as usize) {
            Some(changes) if !changes.is_empty() => changes.as_slice(),
            _ => self.codestream.progression_changes.as_slice(),
        };

        let settings = self.codestream.tile_settings.get(tile_index as usize).copied().flatten();
        let progression = settings.map(|s| s.progression).unwrap_or(self.codestream.progression);
        let num_layers = settings.map(|s| s.num_layers).unwrap_or(self.codestream.num_layers);

        let positions = if changes.is_empty() {
            t2::enumerate_packets(progression, num_layers, &tile_bounds, &geometry)
        } else {
            t2::enumerate_packets_with_changes(changes, num_layers, &tile_bounds, &geometry)
        };

        let packed: Option<&[u8]> = if !self.codestream.ppm_data.is_empty() {
            Some(&self.codestream.ppm_data)
        } else {
            match self.codestream.ppt_data.get(tile_index as usize) {
                Some(data) if !data.is_empty() => Some(data.as_slice()),
                _ => None,
            }
        };

        let mut pos = 0usize;
        let mut ppt_cursor = 0usize;
        let uses_ppm = !self.codestream.ppm_data.is_empty();

        for packet in positions {
            let header_pos = if uses_ppm { *self.ppm_cursor } else { ppt_cursor };

            let header_exhausted = match packed {
                Some(buffer) => header_pos >= buffer.len(),
                None => pos >= data.len(),
            };

            if header_exhausted || pos > data.len() {
                break;
            }

            let params = coding.get(packet.component as usize).cloned().unwrap_or_default();

            match read_packet(
                components,
                &packet,
                &params,
                data,
                pos,
                packed,
                header_pos,
                settings.map(|s| s.sop).unwrap_or(self.codestream.sop),
                settings.map(|s| s.eph).unwrap_or(self.codestream.eph),
            ) {
                Ok(consumed) => {
                    pos += consumed.body;
                    if uses_ppm {
                        *self.ppm_cursor += consumed.header;
                    } else {
                        ppt_cursor += consumed.header;
                    }
                }
                Err(error) => {
                    log_warn!("JPEG 2000: stopping packet parsing: {}", error);
                    break;
                }
            }
        }

        Ok(())
    }
}

fn ceil_div_pow2_i64(value: i64, exp: u32) -> i32 {
    let divisor = 1i64 << exp;
    ((value + divisor - 1) >> exp) as i32
}

fn ceil_div_pow2_signed(value: i32, exp: u32) -> i32 {
    ceil_div_pow2_i64(value as i64, exp)
}

fn floor_div_pow2_signed(value: i32, exp: u32) -> i32 {
    value >> exp
}

fn step_size_for(params: &ComponentCoding, band_index: usize) -> StepSize {
    let sizes = &params.quant.step_sizes;

    match params.quant.style {
        QuantStyle::ScalarDerived => {
            let base = sizes.first().copied().unwrap_or_default();
            if band_index == 0 {
                return base;
            }
            let reduction = ((band_index - 1) / 3) as i32;
            StepSize { expn: (base.expn - reduction).max(0), mant: base.mant }
        }
        _ => sizes.get(band_index).copied().unwrap_or_else(|| sizes.last().copied().unwrap_or_default()),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_precincts(
    bx0: i32,
    by0: i32,
    bx1: i32,
    by1: i32,
    pw: u32,
    ph: u32,
    tlcbgx: i32,
    tlcbgy: i32,
    ppx: u32,
    ppy: u32,
    params: &ComponentCoding,
) -> Vec<Precinct> {
    let mut precincts = Vec::new();

    if pw == 0 || ph == 0 {
        return precincts;
    }

    let cblk_w_exp = params.coding.cblk_width_exp.min(ppx);
    let cblk_h_exp = params.coding.cblk_height_exp.min(ppy);

    for precno in 0..pw * ph {
        let cbgx0 = tlcbgx + ((precno % pw) as i32) * (1 << ppx);
        let cbgy0 = tlcbgy + ((precno / pw) as i32) * (1 << ppy);

        let px0 = cbgx0.max(bx0);
        let py0 = cbgy0.max(by0);
        let px1 = (cbgx0 + (1 << ppx)).min(bx1);
        let py1 = (cbgy0 + (1 << ppy)).min(by1);

        let (cw, ch, cblks) = if px1 > px0 && py1 > py0 {
            let cbx0 = floor_div_pow2_signed(px0, cblk_w_exp) << cblk_w_exp;
            let cby0 = floor_div_pow2_signed(py0, cblk_h_exp) << cblk_h_exp;
            let cbx1 = ceil_div_pow2_signed(px1, cblk_w_exp) << cblk_w_exp;
            let cby1 = ceil_div_pow2_signed(py1, cblk_h_exp) << cblk_h_exp;

            let cw = ((cbx1 - cbx0) >> cblk_w_exp).max(0) as u32;
            let ch = ((cby1 - cby0) >> cblk_h_exp).max(0) as u32;

            let mut cblks = Vec::with_capacity((cw as usize) * (ch as usize));

            for cy in 0..ch {
                for cx in 0..cw {
                    let x0 = (cbx0 + ((cx as i32) << cblk_w_exp)).max(px0);
                    let y0 = (cby0 + ((cy as i32) << cblk_h_exp)).max(py0);
                    let x1 = (cbx0 + (((cx + 1) as i32) << cblk_w_exp)).min(px1);
                    let y1 = (cby0 + (((cy + 1) as i32) << cblk_h_exp)).min(py1);

                    cblks.push(CblkState {
                        x0,
                        y0,
                        x1,
                        y1,
                        num_bps: 0,
                        mb: 0,
                        num_len_bits: 3,
                        included: false,
                        segments: Vec::new(),
                        data: Vec::new(),
                    });
                }
            }

            (cw, ch, cblks)
        } else {
            (0, 0, Vec::new())
        };

        precincts.push(Precinct {
            cw,
            ch,
            cblks,
            incl_tree: TagTree::new(cw.max(1), ch.max(1)),
            imsb_tree: TagTree::new(cw.max(1), ch.max(1)),
        });
    }

    precincts
}

struct PacketConsumed {
    body: usize,
    header: usize,
}

struct SegmentAddition {
    segment: usize,
    length: usize,
}

struct CblkContribution {
    band: usize,
    precinct: usize,
    cblk: usize,
    additions: Vec<SegmentAddition>,
}

#[allow(clippy::too_many_arguments)]
fn read_packet(
    components: &mut [TileComponent],
    packet: &t2::PacketPosition,
    params: &ComponentCoding,
    data: &[u8],
    start: usize,
    packed: Option<&[u8]>,
    header_start: usize,
    expect_sop: bool,
    expect_eph: bool,
) -> VexelResult<PacketConsumed> {
    let component = components
        .get_mut(packet.component as usize)
        .ok_or_else(|| VexelError::Custom("JPEG 2000: packet references an unknown component".to_string()))?;

    let resolution = component
        .resolutions
        .get_mut(packet.resolution as usize)
        .ok_or_else(|| VexelError::Custom("JPEG 2000: packet references an unknown resolution".to_string()))?;

    let mut pos = start;

    if expect_sop && pos + 6 <= data.len() && data[pos] == 0xFF && data[pos + 1] == 0x91 {
        pos += 6;
    }

    let (header_bytes, header_offset) = match packed {
        Some(buffer) => (buffer, header_start),
        None => (data, pos),
    };

    if header_offset >= header_bytes.len() {
        return Err(VexelError::Custom("JPEG 2000: packet header data exhausted".to_string()));
    }

    if packet.layer == 0 {
        for band in resolution.bands.iter_mut() {
            if let Some(precinct) = band.precincts.get_mut(packet.precinct as usize) {
                precinct.incl_tree.reset();
                precinct.imsb_tree.reset();

                for cblk in precinct.cblks.iter_mut() {
                    cblk.segments.clear();
                }
            }
        }
    }

    let mut reader = HeaderReader::new(&header_bytes[header_offset..]);

    if reader.read(1) == 0 {
        reader.inalign();
        let mut consumed = reader.num_bytes();

        if expect_eph {
            let eph_at = header_offset + consumed;
            if eph_at + 2 <= header_bytes.len() && header_bytes[eph_at] == 0xFF && header_bytes[eph_at + 1] == 0x92 {
                consumed += 2;
            }
        }

        return Ok(match packed {
            Some(_) => PacketConsumed { body: pos - start, header: consumed },
            None => PacketConsumed { body: pos - start + consumed, header: 0 },
        });
    }

    let mut contributions: Vec<CblkContribution> = Vec::new();

    for (bandno, band) in resolution.bands.iter_mut().enumerate() {
        if band.is_empty() {
            continue;
        }

        let band_num_bps = band.num_bps;

        let precinct = match band.precincts.get_mut(packet.precinct as usize) {
            Some(precinct) => precinct,
            None => continue,
        };

        let num_cblks = (precinct.cw * precinct.ch) as usize;

        for cblkno in 0..num_cblks {
            let first_inclusion = precinct.cblks[cblkno].segments.is_empty();

            let included = if first_inclusion {
                precinct.incl_tree.decode(&mut reader, cblkno, packet.layer + 1)
            } else {
                reader.read(1) != 0
            };

            if !included {
                continue;
            }

            if first_inclusion {
                let mut zero_planes = 0u32;
                while !precinct.imsb_tree.decode(&mut reader, cblkno, zero_planes) {
                    zero_planes += 1;
                    if zero_planes > 74 {
                        return Err(VexelError::Custom("JPEG 2000: implausible zero-bitplane count".to_string()));
                    }
                }

                let cblk = &mut precinct.cblks[cblkno];
                cblk.included = true;
                cblk.num_bps = (band_num_bps + 1 - zero_planes as i32) as u32;
                cblk.mb = band_num_bps.max(0) as u32;
                cblk.num_len_bits = 3;
            }

            let mut new_passes = t2::get_num_passes(&mut reader);
            let increment = t2::get_comma_code(&mut reader);

            let cblk = &mut precinct.cblks[cblkno];
            cblk.num_len_bits += increment;

            let mut added: Vec<SegmentAddition> = Vec::new();

            loop {
                let (index, room) = match cblk.segments.last() {
                    Some(last) if last.num_passes < last.max_passes => {
                        (cblk.segments.len() - 1, last.max_passes - last.num_passes)
                    }
                    _ => {
                        let first = cblk.segments.is_empty();
                        let previous_max = cblk.segments.last().map(|s| s.max_passes).unwrap_or(0);
                        let max_passes = t2::segment_max_passes(params.coding.cblk_style, first, previous_max);

                        cblk.segments.push(SegmentState { num_passes: 0, max_passes, data_start: 0, data_len: 0 });

                        (cblk.segments.len() - 1, max_passes)
                    }
                };

                let passes_here = new_passes.min(room);
                if passes_here == 0 {
                    break;
                }

                let bit_count = cblk.num_len_bits + int_floorlog2(passes_here);
                if bit_count > 32 {
                    return Err(VexelError::Custom("JPEG 2000: invalid segment length bit count".to_string()));
                }

                let length = reader.read(bit_count) as usize;

                let segment = &mut cblk.segments[index];
                segment.num_passes += passes_here;

                match added.last_mut() {
                    Some(last) if last.segment == index => last.length += length,
                    _ => added.push(SegmentAddition { segment: index, length }),
                }

                new_passes -= passes_here;
                if new_passes == 0 {
                    break;
                }
            }

            contributions.push(CblkContribution {
                band: bandno,
                precinct: packet.precinct as usize,
                cblk: cblkno,
                additions: added,
            });
        }
    }

    reader.inalign();
    let mut consumed = reader.num_bytes();

    if expect_eph {
        let eph_at = header_offset + consumed;
        if eph_at + 2 <= header_bytes.len() && header_bytes[eph_at] == 0xFF && header_bytes[eph_at + 1] == 0x92 {
            consumed += 2;
        }
    }

    let mut body = match packed {
        Some(_) => pos,
        None => pos + consumed,
    };

    for contribution in contributions {
        let Some(band) = resolution.bands.get_mut(contribution.band) else {
            continue;
        };
        let Some(precinct) = band.precincts.get_mut(contribution.precinct) else {
            continue;
        };
        let Some(cblk) = precinct.cblks.get_mut(contribution.cblk) else {
            continue;
        };

        for addition in contribution.additions {
            let take = addition.length.min(data.len().saturating_sub(body));

            let start = cblk.data.len();
            cblk.data.extend_from_slice(&data[body..body + take]);
            body += take;

            if let Some(segment) = cblk.segments.get_mut(addition.segment) {
                if segment.data_len == 0 {
                    segment.data_start = start;
                }
                segment.data_len += take;
            }
        }
    }

    Ok(match packed {
        Some(_) => PacketConsumed { body: body - start, header: consumed },
        None => PacketConsumed { body: body - start, header: 0 },
    })
}

fn uses_ht_block(cblk_style: CodeBlockStyle, block: &CodeBlock) -> bool {
    if !cblk_style.high_throughput() {
        return false;
    }

    if !cblk_style.high_throughput_mixed() {
        return true;
    }

    let Some(first) = block.segments.iter().find(|segment| segment.data_len > 0) else {
        return false;
    };

    let start = first.data_start.min(block.data.len());
    let end = start.saturating_add(first.data_len).min(block.data.len());
    let segment = &block.data[start..end];

    if segment.len() < 2 {
        return false;
    }

    let lcup = segment.len();
    let scup = ((segment[lcup - 1] as usize) << 4) + (segment[lcup - 2] & 0xF) as usize;

    (2..=lcup.min(4079)).contains(&scup)
}

fn decode_ht_block(
    ht: &mut HtDecoder,
    block: &CodeBlock,
    mb: u32,
    cblk_style: CodeBlockStyle,
    samples: &mut Vec<i32>,
) -> bool {
    let width = block.width() as usize;
    let height = block.height() as usize;

    let mut segments = block.segments.iter().filter(|segment| segment.num_passes > 0);

    let Some(cleanup) = segments.next() else {
        return false;
    };
    let refine = segments.next();

    let cleanup_start = cleanup.data_start.min(block.data.len());
    let cleanup_len = cleanup.data_len.min(block.data.len() - cleanup_start);

    let refine_len = match refine {
        Some(refine) if refine.data_start == cleanup_start + cleanup_len => {
            refine.data_len.min(block.data.len() - refine.data_start)
        }
        _ => 0,
    };

    let num_passes = cleanup.num_passes + refine.map(|r| r.num_passes).unwrap_or(0);

    let cblk = HtCodeBlock {
        width,
        height,
        num_bps: block.num_bps,
        mb,
        data: &block.data[cleanup_start..],
        cleanup_len,
        refine_len,
        num_passes,
        stripe_causal: cblk_style.vertically_causal(),
    };

    if !ht.decode(&cblk) {
        log_warn!("JPEG 2000: malformed HT code-block, keeping partial result");
    }

    samples.clear();
    samples.extend(ht.data().iter().map(|&value| value as i32));
    true
}

fn decode_code_blocks(tilec: &mut TileComponent, params: &ComponentCoding) {
    let mut t1 = T1Decoder::new();
    let reversible = params.coding.reversible;
    let roi_shift = params.roi_shift;
    let cblk_style = params.coding.cblk_style;

    let mut ht = HtDecoder::new();
    let mut ht_samples: Vec<i32> = Vec::new();

    let width = tilec.width() as usize;
    if width == 0 {
        return;
    }

    let num_resolutions = tilec.resolutions.len();

    for resno in 0..num_resolutions {
        let prev = if resno == 0 {
            None
        } else {
            Some((
                tilec.resolutions[resno - 1].x1 - tilec.resolutions[resno - 1].x0,
                tilec.resolutions[resno - 1].y1 - tilec.resolutions[resno - 1].y0,
            ))
        };

        let num_bands = tilec.resolutions[resno].bands.len();

        for bandno in 0..num_bands {
            let band_orient = tilec.resolutions[resno].bands[bandno].orient;
            let band_x0 = tilec.resolutions[resno].bands[bandno].x0;
            let band_y0 = tilec.resolutions[resno].bands[bandno].y0;
            let step_size = tilec.resolutions[resno].bands[bandno].step_size;

            let num_precincts = tilec.resolutions[resno].bands[bandno].precincts.len();

            for precno in 0..num_precincts {
                let num_cblks = tilec.resolutions[resno].bands[bandno].precincts[precno].cblks.len();

                for cblkno in 0..num_cblks {
                    let cblk = &tilec.resolutions[resno].bands[bandno].precincts[precno].cblks[cblkno];

                    if !cblk.included || cblk.data.is_empty() {
                        continue;
                    }

                    let block = CodeBlock {
                        x0: cblk.x0,
                        y0: cblk.y0,
                        x1: cblk.x1,
                        y1: cblk.y1,
                        num_bps: cblk.num_bps,
                        segments: cblk
                            .segments
                            .iter()
                            .filter(|s| s.num_passes > 0)
                            .map(|s| Segment {
                                data_start: s.data_start,
                                data_len: s.data_len,
                                num_passes: s.num_passes,
                            })
                            .collect(),
                        data: cblk.data.clone(),
                    };

                    let (cx0, cy0) = (block.x0, block.y0);
                    let cw = block.width() as usize;
                    let ch = block.height() as usize;

                    if cw == 0 || ch == 0 {
                        continue;
                    }

                    let decoded_ht = if uses_ht_block(cblk_style, &block) {
                        decode_ht_block(&mut ht, &block, cblk.mb, cblk_style, &mut ht_samples)
                    } else {
                        false
                    };

                    if !decoded_ht && !t1.decode_cblk(&block, band_orient, roi_shift, cblk_style) {
                        continue;
                    }

                    let samples: &mut [i32] = if decoded_ht { &mut ht_samples } else { t1.data_mut() };

                    if roi_shift > 0 {
                        if roi_shift >= 31 {
                            samples.iter_mut().for_each(|value| *value = 0);
                        } else {
                            let threshold = 1i32 << roi_shift;
                            for value in samples.iter_mut() {
                                let magnitude = value.abs();
                                if magnitude >= threshold {
                                    let scaled = magnitude >> roi_shift;
                                    *value = if *value < 0 { -scaled } else { scaled };
                                }
                            }
                        }
                    }

                    let (offset_x, offset_y) = band_offset(band_orient, prev);

                    let samples: &[i32] = if decoded_ht { &ht_samples } else { t1.data() };

                    for y in 0..ch {
                        for x in 0..cw {
                            let value = samples[y * cw + x];

                            let dest_x = offset_x + (cx0 - band_x0) + x as i32;
                            let dest_y = offset_y + (cy0 - band_y0) + y as i32;

                            if dest_x < 0 || dest_y < 0 {
                                continue;
                            }

                            let index = dest_y as usize * width + dest_x as usize;
                            if index >= tilec.data.len() {
                                continue;
                            }

                            tilec.data[index] = dequantize(value, reversible, step_size);
                        }
                    }
                }
            }
        }
    }
}

fn band_offset(orient: BandOrient, prev: Option<(i32, i32)>) -> (i32, i32) {
    let Some((prev_w, prev_h)) = prev else {
        return (0, 0);
    };

    match orient {
        BandOrient::Ll => (0, 0),
        BandOrient::Hl => (prev_w, 0),
        BandOrient::Lh => (0, prev_h),
        BandOrient::Hh => (prev_w, prev_h),
    }
}

fn dequantize(value: i32, reversible: bool, step_size: f32) -> f32 {
    if reversible {
        (value / 2) as f32
    } else {
        (value as f32 / 2.0) * step_size
    }
}

fn inverse_dwt(tilec: &mut TileComponent, params: &ComponentCoding) -> VexelResult<()> {
    let width = tilec.width() as usize;
    let height = tilec.height() as usize;

    if width == 0 || height == 0 {
        return Ok(());
    }

    let num_resolutions = tilec.resolutions.len();
    if num_resolutions <= 1 {
        return Ok(());
    }

    if params.coding.reversible {
        inverse_dwt_53(tilec, width, height);
    } else {
        inverse_dwt_97(tilec, width, height);
    }

    Ok(())
}

#[allow(clippy::needless_range_loop)]
fn inverse_dwt_53(tilec: &mut TileComponent, width: usize, height: usize) {
    let num_resolutions = tilec.resolutions.len();

    for resno in 1..num_resolutions {
        let rw = (tilec.resolutions[resno].x1 - tilec.resolutions[resno].x0).max(0) as usize;
        let rh = (tilec.resolutions[resno].y1 - tilec.resolutions[resno].y0).max(0) as usize;

        let prev_w = (tilec.resolutions[resno - 1].x1 - tilec.resolutions[resno - 1].x0).max(0) as usize;
        let prev_h = (tilec.resolutions[resno - 1].y1 - tilec.resolutions[resno - 1].y0).max(0) as usize;

        if rw == 0 || rh == 0 {
            continue;
        }

        let cas_x = (tilec.resolutions[resno].x0 % 2).unsigned_abs() as usize;
        let cas_y = (tilec.resolutions[resno].y0 % 2).unsigned_abs() as usize;

        let mut row = vec![0i32; rw.max(1) + 8];
        let mut column = vec![0i32; rh.max(1) + 8];

        for y in 0..rh.min(height) {
            let src: Vec<i32> = (0..rw).map(|x| tilec.data.get(y * width + x).copied().unwrap_or(0.0) as i32).collect();
            dwt::interleave_h(&mut row[..rw], &src, prev_w, rw - prev_w, cas_x);

            dwt::decode_line_53(
                &mut row[..rw],
                &Line { sn: prev_w as i32, dn: (rw - prev_w) as i32, cas: cas_x as i32 },
            );

            for x in 0..rw {
                if let Some(slot) = tilec.data.get_mut(y * width + x) {
                    *slot = row[x] as f32;
                }
            }
        }

        for x in 0..rw.min(width) {
            let src: Vec<i32> = (0..rh).map(|y| tilec.data.get(y * width + x).copied().unwrap_or(0.0) as i32).collect();
            dwt::interleave_v(&mut column[..rh], &src, 1, prev_h, rh - prev_h, cas_y);

            dwt::decode_line_53(
                &mut column[..rh],
                &Line { sn: prev_h as i32, dn: (rh - prev_h) as i32, cas: cas_y as i32 },
            );

            for y in 0..rh {
                if let Some(slot) = tilec.data.get_mut(y * width + x) {
                    *slot = column[y] as f32;
                }
            }
        }
    }
}

#[allow(clippy::needless_range_loop)]
fn inverse_dwt_97(tilec: &mut TileComponent, width: usize, height: usize) {
    let num_resolutions = tilec.resolutions.len();

    for resno in 1..num_resolutions {
        let rw = (tilec.resolutions[resno].x1 - tilec.resolutions[resno].x0).max(0) as usize;
        let rh = (tilec.resolutions[resno].y1 - tilec.resolutions[resno].y0).max(0) as usize;

        let prev_w = (tilec.resolutions[resno - 1].x1 - tilec.resolutions[resno - 1].x0).max(0) as usize;
        let prev_h = (tilec.resolutions[resno - 1].y1 - tilec.resolutions[resno - 1].y0).max(0) as usize;

        if rw == 0 || rh == 0 {
            continue;
        }

        let cas_x = (tilec.resolutions[resno].x0 % 2).unsigned_abs() as usize;
        let cas_y = (tilec.resolutions[resno].y0 % 2).unsigned_abs() as usize;

        let mut row = vec![0f32; rw.max(1) + 8];
        let mut column = vec![0f32; rh.max(1) + 8];

        for y in 0..rh.min(height) {
            let src: Vec<f32> = (0..rw).map(|x| tilec.data.get(y * width + x).copied().unwrap_or(0.0)).collect();
            dwt::interleave_h(&mut row[..rw], &src, prev_w, rw - prev_w, cas_x);

            dwt::decode_line_97(
                &mut row[..rw],
                &Line { sn: prev_w as i32, dn: (rw - prev_w) as i32, cas: cas_x as i32 },
            );

            for x in 0..rw {
                if let Some(slot) = tilec.data.get_mut(y * width + x) {
                    *slot = row[x];
                }
            }
        }

        for x in 0..rw.min(width) {
            let src: Vec<f32> = (0..rh).map(|y| tilec.data.get(y * width + x).copied().unwrap_or(0.0)).collect();
            dwt::interleave_v(&mut column[..rh], &src, 1, prev_h, rh - prev_h, cas_y);

            dwt::decode_line_97(
                &mut column[..rh],
                &Line { sn: prev_h as i32, dn: (rh - prev_h) as i32, cas: cas_y as i32 },
            );

            for y in 0..rh {
                if let Some(slot) = tilec.data.get_mut(y * width + x) {
                    *slot = column[y];
                }
            }
        }
    }
}

#[allow(clippy::needless_range_loop)]
fn inverse_rct_tile(planes: &mut [Vec<f32>]) {
    if planes.len() < 3 {
        return;
    }

    let len = planes[0].len().min(planes[1].len()).min(planes[2].len());

    for i in 0..len {
        let y = planes[0][i] as i32;
        let u = planes[1][i] as i32;
        let v = planes[2][i] as i32;

        let g = y - ((u + v) >> 2);
        let r = v + g;
        let b = u + g;

        planes[0][i] = r as f32;
        planes[1][i] = g as f32;
        planes[2][i] = b as f32;
    }
}

#[allow(clippy::needless_range_loop)]
fn inverse_ict_tile(planes: &mut [Vec<f32>]) {
    if planes.len() < 3 {
        return;
    }

    let len = planes[0].len().min(planes[1].len()).min(planes[2].len());

    for i in 0..len {
        let y = planes[0][i];
        let u = planes[1][i];
        let v = planes[2][i];

        planes[0][i] = y + 1.402 * v;
        planes[1][i] = y - 0.344_13 * u - 0.714_14 * v;
        planes[2][i] = y + 1.772 * u;
    }
}

pub fn log_tile_summary(tile_index: u32, tile: &DecodedTile) {
    log_debug!(
        "JPEG 2000: decoded tile {} ({}x{}, {} components)",
        tile_index,
        tile.x1 - tile.x0,
        tile.y1 - tile.y0,
        tile.components.len()
    );
}
