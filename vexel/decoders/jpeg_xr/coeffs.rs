use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::headers::{ImageHeader, PlaneHeader};
use crate::decoders::jpeg_xr::predict::{pred_ac, pred_cbp, pred_dc_ac, update_pred_info, MacroblockInfo, PredContext};
use crate::decoders::jpeg_xr::segdec::*;
use crate::decoders::jpeg_xr::tile::{
    read_tile_header_dc, read_tile_header_hp, read_tile_header_lp, TileLayout, TileQuantizers,
};
use crate::decoders::jpeg_xr::types::*;
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

pub const COEFFS_PER_PLANE: usize = 256;

pub struct PlaneCoefficients {
    pub planes: Vec<Vec<i32>>,
    pub hp_qp: Vec<[i32; MAX_CHANNELS]>,
    pub mb_width: usize,
    pub overrun: bool,
}

impl PlaneCoefficients {
    pub fn new(channels: usize, mb_width: usize, mb_height: usize) -> Self {
        let per_plane = mb_width * mb_height * COEFFS_PER_PLANE;
        PlaneCoefficients {
            planes: vec![vec![0i32; per_plane]; channels.max(1)],
            hp_qp: vec![[255i32; MAX_CHANNELS]; mb_width * mb_height],
            mb_width,
            overrun: false,
        }
    }

    pub fn macroblock_offset(&self, mb_x: usize, mb_y: usize) -> usize {
        (mb_y * self.mb_width + mb_x) * COEFFS_PER_PLANE
    }
}

pub struct SubbandStreams<'a> {
    pub dc: XrBitReader<'a>,
    pub lp: Option<XrBitReader<'a>>,
    pub ac: Option<XrBitReader<'a>>,
    pub fl: Option<XrBitReader<'a>>,
}

impl<'a> SubbandStreams<'a> {
    pub fn spatial(reader: XrBitReader<'a>) -> Self {
        SubbandStreams { dc: reader, lp: None, ac: None, fl: None }
    }
}

pub fn num_subbands(subband: Subband) -> usize {
    match subband {
        Subband::DcOnly => 1,
        Subband::NoHighpass => 2,
        Subband::NoFlexbits => 3,
        _ => 4,
    }
}

pub fn read_packet_header(reader: &mut XrBitReader) -> VexelResult<()> {
    if reader.read(8) != 0 || reader.read(8) != 0 || reader.read(8) != 1 {
        return Err(VexelError::Custom("JPEG XR: bad packet header".to_string()));
    }
    reader.skip(8);
    Ok(())
}

pub struct IndexTable {
    pub offsets: Vec<u64>,
    pub header_size: usize,
    pub packet_starts: Vec<usize>,
}

pub fn read_index_table(reader: &mut XrBitReader, entries: usize, present: bool) -> VexelResult<IndexTable> {
    let mut offsets = Vec::new();

    if present {
        if reader.read(16) != 1 {
            return Err(VexelError::Custom("JPEG XR: bad index table marker".to_string()));
        }

        let available = ((reader.total_bits() >> 3) as usize).saturating_sub(reader.byte_pos());
        let count = entries.min(available);
        if count < entries {
            log_warn!("JPEG XR: index table declares {} entries but only {} fit in the stream", entries, count);
        }

        offsets.reserve(count);
        for _ in 0..count {
            let (value, _escape) = reader.read_vl_word_esc();
            offsets.push(value);
        }
    }

    let (extra, _) = reader.read_vl_word_esc();
    reader.align_byte();

    let header_size = usize::try_from(extra).unwrap_or(usize::MAX).saturating_add(reader.byte_pos());

    let mut packet_starts: Vec<usize> = offsets.iter().filter_map(|&o| header_size.checked_add(o as usize)).collect();
    packet_starts.sort_unstable();
    packet_starts.dedup();

    Ok(IndexTable { offsets, header_size, packet_starts })
}

struct PlaneState {
    header: PlaneHeader,
    coeffs: PlaneCoefficients,
    contexts: Vec<CodingContext>,
    tiles: Vec<TileQuantizers>,
    pred: PredContext,
    channels: usize,
}

impl PlaneState {
    fn new(header: &PlaneHeader, layout: &TileLayout, tile_cols: usize) -> Self {
        let channels = header.num_channels;
        PlaneState {
            header: header.clone(),
            coeffs: PlaneCoefficients::new(channels, layout.mb_width, layout.mb_height),
            contexts: (0..tile_cols).map(|_| CodingContext::new(header.internal_color_format, 0)).collect(),
            tiles: (0..tile_cols).map(|_| crate::decoders::jpeg_xr::tile::init_uniform_quantizers(header)).collect(),
            pred: PredContext::new(channels, layout.mb_width),
            channels,
        }
    }
}

#[derive(Default)]
pub struct DecodeConfig {
    pub skip_flexbits: bool,
}

pub fn decode_coefficients(
    stream: &[u8],
    header_end: usize,
    image: &ImageHeader,
    plane: &PlaneHeader,
    alpha_plane: Option<&PlaneHeader>,
    layout: &TileLayout,
    config: &DecodeConfig,
) -> VexelResult<(PlaneCoefficients, Option<PlaneCoefficients>)> {
    let spatial = image.bitstream_format == BitstreamFormat::Spatial;
    let subbands = num_subbands(plane.subband);
    let tile_cols = layout.cols();
    let tile_rows = layout.rows();

    let packets_per_row = if spatial { tile_cols } else { tile_cols * subbands };

    let mut cursor = XrBitReader::new(stream);
    cursor.seek_bytes(header_end);

    let entries = packets_per_row * tile_rows;
    let table = read_index_table(&mut cursor, entries, image.index_table_present)?;
    let base = table.header_size;
    let index = if image.index_table_present { Some(&table) } else { None };

    let mut states: Vec<PlaneState> = Vec::with_capacity(2);
    states.push(PlaneState::new(plane, layout, tile_cols));
    if let Some(alpha) = alpha_plane {
        states.push(PlaneState::new(alpha, layout, tile_cols));
    }

    for tile_row in 0..tile_rows {
        let row_start = layout.row_starts[tile_row];
        let row_end = layout.row_starts[tile_row + 1];

        let mut streams =
            open_row_streams(stream, base, index, tile_row, tile_cols, subbands, spatial, packets_per_row)?;

        #[allow(clippy::needless_range_loop)]
        for tile_col in 0..tile_cols {
            for state in states.iter_mut() {
                state.contexts[tile_col] = CodingContext::new(state.header.internal_color_format, 0);
            }

            let set = &mut streams[tile_col];
            let mut trim_flexbits = 0;

            if spatial {
                read_packet_header(&mut set.dc)?;
                trim_flexbits = if image.trim_flexbits_flag { set.dc.read_i32(4) } else { 0 };
            } else {
                read_packet_header(&mut set.dc)?;
                if let Some(lp) = set.lp.as_mut() {
                    read_packet_header(lp)?;
                }
                if let Some(ac) = set.ac.as_mut() {
                    read_packet_header(ac)?;
                }
                if let Some(fl) = set.fl.as_mut() {
                    let _ = read_packet_header(fl);
                    trim_flexbits = if image.trim_flexbits_flag { fl.read_i32(4) } else { 0 };
                }
            }

            for state in states.iter_mut() {
                state.contexts[tile_col].trim_flexbits = trim_flexbits;
            }
        }

        for mb_y in row_start..row_end {
            let at_top_row = mb_y == row_start;

            #[allow(clippy::needless_range_loop)]
            for tile_col in 0..tile_cols {
                let col_start = layout.col_starts[tile_col];
                let col_end = layout.col_starts[tile_col + 1];
                let set = &mut streams[tile_col];

                for mb_x in col_start..col_end {
                    let at_left_col = mb_x == col_start;
                    let reset_totals = (mb_x - col_start) & 0xf == 0;
                    let reset_context = reset_totals || mb_x + 1 == col_end;

                    if at_left_col && at_top_row {
                        for state in states.iter_mut() {
                            let tile = &mut state.tiles[tile_col];
                            let header = &state.header;
                            read_tile_header_dc(tile, &mut set.dc, header);
                            if subbands > 1 {
                                let lp = set.lp.as_mut().unwrap_or(&mut set.dc);
                                read_tile_header_lp(tile, lp, header)?;
                            }
                            if subbands > 2 {
                                let ac = set.ac.as_mut().unwrap_or(&mut set.dc);
                                read_tile_header_hp(tile, ac, header)?;
                            }
                        }
                    }

                    for state in states.iter_mut() {
                        let plane = &state.header;
                        let channels = state.channels;
                        let context = &mut state.contexts[tile_col];
                        let tile = &mut state.tiles[tile_col];
                        let coeffs = &mut state.coeffs;
                        let pred = &mut state.pred;
                        let subbands = num_subbands(plane.subband);

                        let mut mb = MacroblockInfo::default();

                        decode_macroblock_dc(
                            context,
                            &mut set.dc,
                            &mut mb,
                            plane.internal_color_format,
                            channels,
                            plane.subband,
                            spatial,
                            tile.bits_lp,
                            tile.bits_hp,
                            tile.num_qp_lp,
                            tile.num_qp_hp,
                        )?;

                        if plane.subband == Subband::DcOnly && reset_context {
                            context.adapt_dc();
                        }

                        if subbands > 1 {
                            let lp_reader: &mut XrBitReader = match set.lp.as_mut() {
                                Some(lp) => lp,
                                None => &mut set.dc,
                            };
                            decode_macroblock_lowpass(
                                context,
                                lp_reader,
                                &mut mb,
                                plane.internal_color_format,
                                channels,
                                spatial,
                                tile.bits_lp,
                                tile.num_qp_lp,
                                reset_totals,
                            )?;

                            if reset_context {
                                context.adapt_lowpass();
                            }
                        }

                        pred_dc_ac(&mut mb, pred, mb_x, at_left_col, at_top_row, plane.internal_color_format, channels);

                        let offset = coeffs.macroblock_offset(mb_x, mb_y);
                        let mut mb_planes = extract_macroblock(coeffs, offset);

                        dequantize_macroblock(
                            &mb,
                            &mut mb_planes,
                            plane.internal_color_format,
                            channels,
                            plane.subband,
                            &tile.dc,
                            &tile.lp,
                            usize::from(mb.qindex_lp),
                        );

                        if subbands > 2 {
                            if reset_totals {
                                context.reset_highpass_scan_totals();
                            }

                            if !spatial && tile.bits_hp > 0 {
                                let ac_reader: &mut XrBitReader = match set.ac.as_mut() {
                                    Some(ac) => ac,
                                    None => &mut set.dc,
                                };
                                mb.qindex_hp = decode_qp_index(ac_reader, tile.bits_hp);
                                if mb.qindex_hp >= tile.num_qp_hp.max(1) {
                                    return Err(VexelError::Custom("JPEG XR: HP QP index out of range".to_string()));
                                }
                            } else if tile.bits_hp == 0 && tile.num_qp_hp > 1 {
                                mb.qindex_hp = mb.qindex_lp;
                            }

                            {
                                let ac_reader: &mut XrBitReader = match set.ac.as_mut() {
                                    Some(ac) => ac,
                                    None => &mut set.dc,
                                };
                                decode_cbp(context, ac_reader, &mut mb, plane.internal_color_format, channels);
                            }

                            let mut cbp_model = context.cbp_model;
                            pred_cbp(
                                &mut mb,
                                pred,
                                mb_x,
                                at_left_col,
                                at_top_row,
                                plane.internal_color_format,
                                channels,
                                &mut cbp_model,
                            );
                            context.cbp_model = cbp_model;

                            let params = HighpassParams {
                                color_format: plane.internal_color_format,
                                channels,
                                qp_hp: &tile.hp,
                                qindex_hp: usize::from(mb.qindex_hp),
                                skip_flexbits: config.skip_flexbits,
                                has_flexbits: subbands > 3,
                            };

                            let (ac_reader, flex_reader) = split_ac_flex(set);

                            decode_coeffs(context, ac_reader, flex_reader, &mb, &mut mb_planes, &params)?;

                            pred_ac(&mut mb_planes, &mb, plane.internal_color_format, channels);

                            if reset_context {
                                context.adapt_highpass();
                            }
                        }

                        store_macroblock(coeffs, offset, mb_planes);

                        if plane.subband.has_highpass() {
                            let qp_set = &tile.hp[usize::from(mb.qindex_hp).min(tile.hp.len() - 1)];
                            let slot = &mut coeffs.hp_qp[mb_y * layout.mb_width + mb_x];
                            for ch in 0..MAX_CHANNELS {
                                slot[ch] = qp_set[ch].qp;
                            }
                        }

                        update_pred_info(&mb, pred, mb_x, plane.internal_color_format, channels);
                    }
                }
            }

            for state in states.iter_mut() {
                state.pred.advance_row();
            }
        }

        for set in streams.iter() {
            if set.dc.overrun()
                || set.lp.as_ref().is_some_and(|r| r.overrun())
                || set.ac.as_ref().is_some_and(|r| r.overrun())
                || set.fl.as_ref().is_some_and(|r| r.overrun())
            {
                for state in states.iter_mut() {
                    state.coeffs.overrun = true;
                }
            }
        }
    }

    let mut drain = states.into_iter();
    let primary = drain.next().expect("primary plane state").coeffs;
    let alpha = drain.next().map(|state| state.coeffs);

    Ok((primary, alpha))
}

fn extract_macroblock(coeffs: &mut PlaneCoefficients, offset: usize) -> Vec<Vec<i32>> {
    coeffs
        .planes
        .iter()
        .map(|plane| {
            plane
                .get(offset..offset + COEFFS_PER_PLANE)
                .map(|s| s.to_vec())
                .unwrap_or_else(|| vec![0; COEFFS_PER_PLANE])
        })
        .collect()
}

fn store_macroblock(coeffs: &mut PlaneCoefficients, offset: usize, blocks: Vec<Vec<i32>>) {
    for (plane, block) in coeffs.planes.iter_mut().zip(blocks) {
        if let Some(target) = plane.get_mut(offset..offset + COEFFS_PER_PLANE) {
            target.copy_from_slice(&block);
        }
    }
}

fn split_ac_flex<'a, 'b>(
    set: &'a mut SubbandStreams<'b>,
) -> (&'a mut XrBitReader<'b>, Option<&'a mut XrBitReader<'b>>) {
    match (set.ac.as_mut(), set.fl.as_mut()) {
        (Some(ac), fl) => (ac, fl),
        (None, _) => (&mut set.dc, None),
    }
}

#[allow(clippy::too_many_arguments)]
fn open_row_streams<'a>(
    stream: &'a [u8],
    base: usize,
    index: Option<&IndexTable>,
    tile_row: usize,
    tile_cols: usize,
    subbands: usize,
    spatial: bool,
    packets_per_row: usize,
) -> VexelResult<Vec<SubbandStreams<'a>>> {
    let mut out = Vec::with_capacity(tile_cols);

    for tile_col in 0..tile_cols {
        if let Some(table) = index {
            let packet_index = tile_row * packets_per_row + if spatial { tile_col } else { tile_col * subbands };

            let open = |slot: usize| -> Option<XrBitReader<'a>> {
                let offset = table.offsets.get(packet_index + slot).copied()?;
                let start = base.checked_add(offset as usize)?;
                if start >= stream.len() {
                    return None;
                }

                let next = table.packet_starts.partition_point(|&o| o <= start);
                let end = table.packet_starts.get(next).copied().unwrap_or(stream.len());

                let mut reader = XrBitReader::new(stream);
                reader.set_limit_bytes(end);
                reader.seek_bytes(start);
                Some(reader)
            };

            let dc = open(0).ok_or_else(|| {
                VexelError::Custom(format!("JPEG XR: missing packet for tile ({}, {})", tile_row, tile_col))
            })?;

            if spatial {
                out.push(SubbandStreams::spatial(dc));
            } else {
                out.push(SubbandStreams {
                    dc,
                    lp: if subbands > 1 { open(1) } else { None },
                    ac: if subbands > 2 { open(2) } else { None },
                    fl: if subbands > 3 { open(3) } else { None },
                });
            }
        } else {
            if tile_cols != 1 || !spatial {
                return Err(VexelError::Custom(
                    "JPEG XR: missing index table for tiled or frequency-order stream".to_string(),
                ));
            }

            let mut reader = XrBitReader::new(stream);
            reader.seek_bytes(base);
            out.push(SubbandStreams::spatial(reader));
        }
    }

    if out.is_empty() {
        log_warn!("JPEG XR: no tile streams opened");
    }

    Ok(out)
}
