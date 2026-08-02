use std::collections::BTreeMap;

use crate::decoders::jpeg2000::types::*;
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_debug, log_warn};

#[derive(Debug, Clone, Copy)]
pub struct TileSettings {
    pub progression: ProgressionOrder,
    pub num_layers: u32,
    pub mct: bool,
    pub sop: bool,
    pub eph: bool,
}

pub struct TilePart {
    pub tile_index: u32,
    pub data_start: usize,
    pub data_len: usize,
}

pub struct Codestream {
    pub siz: SizInfo,
    pub progression: ProgressionOrder,
    pub num_layers: u32,
    pub mct: bool,
    pub sop: bool,
    pub eph: bool,
    pub defaults: Vec<ComponentCoding>,
    pub tile_overrides: Vec<Option<Vec<ComponentCoding>>>,
    pub tile_parts: Vec<TilePart>,
    pub uses_ht: bool,
    pub ppm_data: Vec<u8>,
    pub ppt_data: Vec<Vec<u8>>,
    pub tile_settings: Vec<Option<TileSettings>>,
    pub progression_changes: Vec<ProgressionChange>,
    pub tile_progression_changes: Vec<Vec<ProgressionChange>>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn u8(&mut self) -> VexelResult<u8> {
        let value = *self.data.get(self.pos).ok_or_else(|| truncated("byte"))?;
        self.pos += 1;
        Ok(value)
    }

    fn u16(&mut self) -> VexelResult<u16> {
        if self.remaining() < 2 {
            return Err(truncated("u16"));
        }
        let value = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(value)
    }

    fn u32(&mut self) -> VexelResult<u32> {
        if self.remaining() < 4 {
            return Err(truncated("u32"));
        }
        let value = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(value)
    }
}

fn truncated(what: &str) -> VexelError {
    VexelError::Custom(format!("JPEG 2000: truncated codestream while reading {}", what))
}

fn merge_ppm(markers: &[Option<Vec<u8>>]) -> Vec<u8> {
    let mut merged = Vec::new();
    let mut remaining = 0usize;

    for fragment in markers.iter().flatten() {
        let mut data = fragment.as_slice();

        if remaining > 0 {
            let take = remaining.min(data.len());
            merged.extend_from_slice(&data[..take]);
            remaining -= take;
            data = &data[take..];
        }

        while !data.is_empty() {
            if data.len() < 4 {
                log_warn!("JPEG 2000: PPM fragment too short for an Nppm field");
                break;
            }

            let n_ppm = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
            data = &data[4..];

            let take = n_ppm.min(data.len());
            merged.extend_from_slice(&data[..take]);
            remaining = n_ppm - take;
            data = &data[take..];
        }
    }

    if remaining != 0 {
        log_warn!("JPEG 2000: PPM markers end mid-block, {} bytes missing", remaining);
    }

    merged
}

fn parse_poc(segment: &[u8], num_components: usize, num_layers: u32) -> Vec<ProgressionChange> {
    let comp_room = if num_components <= 256 { 1 } else { 2 };
    let chunk = 5 + 2 * comp_room;

    let mut changes = Vec::new();
    let mut pos = 0;

    while pos + chunk <= segment.len() {
        let resno0 = segment[pos] as u32;
        pos += 1;

        let compno0 = read_component_index(segment, pos, comp_room);
        pos += comp_room;

        let layno1 = u16::from_be_bytes([segment[pos], segment[pos + 1]]) as u32;
        pos += 2;

        let resno1 = segment[pos] as u32;
        pos += 1;

        let compno1 = read_component_index(segment, pos, comp_room);
        pos += comp_room;

        let order = ProgressionOrder::from_u8(segment[pos]);
        pos += 1;

        let order = match order {
            Some(order) => order,
            None => {
                log_warn!("JPEG 2000: POC entry has an unknown progression order, skipping it");
                continue;
            }
        };

        changes.push(ProgressionChange {
            resno0,
            compno0,
            layno1: layno1.clamp(1, num_layers.max(1)),
            resno1,
            compno1: compno1.min(num_components as u32).max(compno0 + 1),
            order,
        });
    }

    changes
}

fn read_component_index(segment: &[u8], pos: usize, width: usize) -> u32 {
    if width == 1 {
        segment[pos] as u32
    } else {
        u16::from_be_bytes([segment[pos], segment[pos + 1]]) as u32
    }
}

fn merge_ppt(markers: &[Option<Vec<u8>>]) -> Vec<u8> {
    let mut merged = Vec::new();
    for fragment in markers.iter().flatten() {
        merged.extend_from_slice(fragment);
    }
    merged
}

pub fn parse(data: &[u8]) -> VexelResult<Codestream> {
    let mut reader = Reader::new(data);

    if Marker::from_u16(reader.u16()?) != Marker::Soc {
        log_warn!("JPEG 2000: missing SOC marker, parsing from the start of the codestream");
        reader.pos = 0;
    }

    let mut siz: Option<SizInfo> = None;
    let mut defaults: Vec<ComponentCoding> = Vec::new();
    let mut progression = ProgressionOrder::Lrcp;
    let mut num_layers = 1;
    let mut mct = false;
    let mut sop = false;
    let mut eph = false;
    let mut uses_ht = false;
    let mut tile_parts = Vec::new();
    let mut tile_overrides: Vec<Option<Vec<ComponentCoding>>> = Vec::new();
    let mut ppm_markers: Vec<Option<Vec<u8>>> = Vec::new();
    let mut ppt_markers: BTreeMap<u32, Vec<Option<Vec<u8>>>> = BTreeMap::new();
    let mut tile_settings_map: BTreeMap<u32, TileSettings> = BTreeMap::new();
    let mut main_progressions: Vec<ProgressionChange> = Vec::new();
    let mut tile_progressions: BTreeMap<u32, Vec<ProgressionChange>> = BTreeMap::new();

    let mut current_tile: Option<u32> = None;
    let mut current_tile_part_end: Option<usize> = None;

    loop {
        if reader.remaining() < 2 {
            break;
        }

        let marker = Marker::from_u16(reader.u16()?);

        if marker == Marker::Eoc {
            break;
        }

        if marker == Marker::Sod {
            let tile_index = match current_tile.take() {
                Some(index) => index,
                None => {
                    log_warn!("JPEG 2000: SOD outside a tile-part, assuming tile 0");
                    if tile_parts.is_empty() {
                        tile_parts.push(TilePart { tile_index: 0, data_start: 0, data_len: 0 });
                    }
                    0
                }
            };

            let start = reader.pos;

            let end = match current_tile_part_end {
                Some(end) if end > start && end <= data.len() => end,
                _ => find_tile_part_end(data, start),
            };

            if let Some(part) = tile_parts.last_mut() {
                if part.tile_index == tile_index && part.data_len == 0 {
                    part.data_start = start;
                    part.data_len = end - start;
                }
            }

            current_tile_part_end = None;
            reader.pos = end;
            continue;
        }

        if marker == Marker::Soc || marker.is_reserved_delimiter() {
            continue;
        }

        let length = reader.u16()? as usize;

        if length < 2 {
            log_warn!("JPEG 2000: invalid marker length {} for {:?}, skipping marker", length, marker);
            continue;
        }

        let segment_start = reader.pos;
        let segment_end = (segment_start + length - 2).min(data.len());
        let segment = &data[segment_start..segment_end];

        match marker {
            Marker::Siz => {
                let parsed = parse_siz(segment)?;
                let num_components = parsed.components.len();
                defaults = vec![ComponentCoding::default(); num_components];
                let num_tiles = (parsed.num_tiles_x() as usize).saturating_mul(parsed.num_tiles_y() as usize);
                tile_overrides = (0..num_tiles).map(|_| None).collect();
                siz = Some(parsed);
            }
            Marker::Cap => {
                uses_ht = parse_cap_has_ht(segment);
            }
            Marker::Cod => {
                let cod = match parse_cod(segment) {
                    Ok(values) => values,
                    Err(error) => {
                        log_warn!("JPEG 2000: ignoring malformed COD marker: {}", error);
                        reader.pos = segment_end;
                        continue;
                    }
                };

                match current_tile {
                    Some(tile_index) => {
                        tile_settings_map.insert(
                            tile_index,
                            TileSettings {
                                progression: cod.progression,
                                num_layers: cod.num_layers,
                                mct: cod.mct,
                                sop: cod.sop,
                                eph: cod.eph,
                            },
                        );
                    }
                    None => {
                        progression = cod.progression;
                        num_layers = cod.num_layers;
                        mct = cod.mct;
                        sop = cod.sop;
                        eph = cod.eph;
                    }
                }

                let target = coding_target(&mut defaults, &mut tile_overrides, current_tile);
                for component in target.iter_mut() {
                    component.coding = cod.style.clone();
                }
            }
            Marker::Coc => {
                let Some(num_components) = siz.as_ref().map(|siz| siz.components.len()) else {
                    log_warn!("JPEG 2000: COC marker before SIZ, ignoring it");
                    reader.pos = segment_end;
                    continue;
                };

                match parse_coc(segment, num_components) {
                    Ok((index, style)) => {
                        let target = coding_target(&mut defaults, &mut tile_overrides, current_tile);
                        if let Some(component) = target.get_mut(index) {
                            component.coding = style;
                        }
                    }
                    Err(error) => log_warn!("JPEG 2000: ignoring malformed COC marker: {}", error),
                }
            }
            Marker::Qcd => match parse_qcd(segment) {
                Ok(quant) => {
                    let target = coding_target(&mut defaults, &mut tile_overrides, current_tile);
                    for component in target.iter_mut() {
                        component.quant = quant.clone();
                    }
                }
                Err(error) => log_warn!("JPEG 2000: ignoring malformed QCD marker: {}", error),
            },
            Marker::Qcc => {
                let Some(num_components) = siz.as_ref().map(|siz| siz.components.len()) else {
                    log_warn!("JPEG 2000: QCC marker before SIZ, ignoring it");
                    reader.pos = segment_end;
                    continue;
                };

                match parse_qcc(segment, num_components) {
                    Ok((index, quant)) => {
                        let target = coding_target(&mut defaults, &mut tile_overrides, current_tile);
                        if let Some(component) = target.get_mut(index) {
                            component.quant = quant;
                        }
                    }
                    Err(error) => log_warn!("JPEG 2000: ignoring malformed QCC marker: {}", error),
                }
            }
            Marker::Rgn => {
                let Some(num_components) = siz.as_ref().map(|siz| siz.components.len()) else {
                    log_warn!("JPEG 2000: RGN marker before SIZ, ignoring it");
                    reader.pos = segment_end;
                    continue;
                };

                if let Some((index, shift)) = parse_rgn(segment, num_components) {
                    let target = coding_target(&mut defaults, &mut tile_overrides, current_tile);
                    if let Some(component) = target.get_mut(index) {
                        component.roi_shift = shift;
                    }
                }
            }
            Marker::Sot => {
                let mut sot = Reader::new(segment);
                let tile_index = match sot.u16() {
                    Ok(index) => index as u32,
                    Err(_) => {
                        log_warn!("JPEG 2000: truncated SOT marker, assuming tile 0");
                        0
                    }
                };
                let tile_part_length = sot.u32().map(|length| length as usize).unwrap_or(0);

                let tile_index = match tile_overrides.len() {
                    0 => tile_index,
                    count if (tile_index as usize) < count => tile_index,
                    count => {
                        log_warn!("JPEG 2000: SOT tile index {} exceeds the {} tiles in the grid", tile_index, count);
                        tile_index % count as u32
                    }
                };

                let sot_start = segment_start.saturating_sub(4);
                current_tile_part_end = if tile_part_length == 0 {
                    Some(data.len())
                } else {
                    sot_start.checked_add(tile_part_length).filter(|end| *end <= data.len())
                };

                current_tile = Some(tile_index);
                tile_parts.push(TilePart { tile_index, data_start: 0, data_len: 0 });
            }
            Marker::Poc => {
                let Some(num_components) = siz.as_ref().map(|siz| siz.components.len()) else {
                    log_warn!("JPEG 2000: POC marker before SIZ, ignoring it");
                    reader.pos = segment_end;
                    continue;
                };
                let changes = parse_poc(segment, num_components, num_layers);

                match current_tile {
                    Some(tile_index) => {
                        tile_progressions.entry(tile_index).or_default().extend(changes);
                    }
                    None => main_progressions.extend(changes),
                }
            }
            Marker::Com | Marker::Tlm | Marker::Plm | Marker::Plt | Marker::Crg => {}
            Marker::Ppm => {
                if let Some((&z, rest)) = segment.split_first() {
                    let slot = z as usize;
                    if ppm_markers.len() <= slot {
                        ppm_markers.resize(slot + 1, None);
                    }
                    if ppm_markers[slot].is_none() {
                        ppm_markers[slot] = Some(rest.to_vec());
                    } else {
                        log_warn!("JPEG 2000: duplicate PPM marker Zppm={}", z);
                    }
                }
            }
            Marker::Ppt => match current_tile {
                Some(tile_index) => {
                    if let Some((&z, rest)) = segment.split_first() {
                        let per_tile = ppt_markers.entry(tile_index).or_default();
                        let slot = z as usize;
                        if per_tile.len() <= slot {
                            per_tile.resize(slot + 1, None);
                        }
                        if per_tile[slot].is_none() {
                            per_tile[slot] = Some(rest.to_vec());
                        } else {
                            log_warn!("JPEG 2000: duplicate PPT marker Zppt={}", z);
                        }
                    }
                }
                None => log_warn!("JPEG 2000: PPT marker outside a tile-part header"),
            },
            other => {
                log_debug!("JPEG 2000: skipping unhandled marker {:?}", other);
            }
        }

        reader.pos = segment_end;
    }

    let siz = siz.ok_or_else(|| VexelError::Custom("JPEG 2000: codestream has no SIZ marker".to_string()))?;

    if defaults.is_empty() {
        log_warn!("JPEG 2000: codestream has no coding style, falling back to defaults");
        defaults = vec![ComponentCoding::default(); siz.components.len()];
    }

    let ppm_data = merge_ppm(&ppm_markers);
    let mut ppt_data = vec![Vec::new(); tile_overrides.len()];
    for (tile_index, markers) in &ppt_markers {
        if let Some(slot) = ppt_data.get_mut(*tile_index as usize) {
            *slot = merge_ppt(markers);
        }
    }

    let mut tile_settings = vec![None; tile_overrides.len()];
    for (tile_index, settings) in tile_settings_map {
        if let Some(slot) = tile_settings.get_mut(tile_index as usize) {
            *slot = Some(settings);
        }
    }

    let mut tile_progression_changes = vec![Vec::new(); tile_overrides.len()];
    for (tile_index, changes) in tile_progressions {
        if let Some(slot) = tile_progression_changes.get_mut(tile_index as usize) {
            *slot = changes;
        }
    }

    Ok(Codestream {
        siz,
        progression,
        num_layers,
        mct,
        sop,
        eph,
        defaults,
        tile_overrides,
        tile_parts,
        ppm_data,
        ppt_data,
        tile_settings,
        progression_changes: main_progressions,
        tile_progression_changes,
        uses_ht,
    })
}

fn coding_target<'a>(
    defaults: &'a mut Vec<ComponentCoding>,
    tile_overrides: &'a mut [Option<Vec<ComponentCoding>>],
    current_tile: Option<u32>,
) -> &'a mut Vec<ComponentCoding> {
    match current_tile {
        Some(index) => {
            let slot = &mut tile_overrides[index as usize % tile_overrides.len().max(1)];
            slot.get_or_insert_with(|| defaults.clone())
        }
        None => defaults,
    }
}

fn find_tile_part_end(data: &[u8], start: usize) -> usize {
    let mut pos = start;

    while pos + 1 < data.len() {
        if data[pos] == 0xFF {
            let next = data[pos + 1];
            if next == 0x90 || next == 0xD9 {
                return pos;
            }
        }
        pos += 1;
    }

    data.len()
}

fn parse_siz(segment: &[u8]) -> VexelResult<SizInfo> {
    if segment.len() < 36 {
        return Err(VexelError::Custom("JPEG 2000: SIZ marker too short".to_string()));
    }

    let mut reader = Reader::new(segment);
    let _rsiz = reader.u16()?;
    let xsiz = reader.u32()?;
    let ysiz = reader.u32()?;
    let x0siz = reader.u32()?;
    let y0siz = reader.u32()?;
    let xtsiz = reader.u32()?;
    let ytsiz = reader.u32()?;
    let xt0siz = reader.u32()?;
    let yt0siz = reader.u32()?;
    let num_components = reader.u16()? as usize;

    if num_components == 0 {
        return Err(VexelError::Custom("JPEG 2000: SIZ declares zero components".to_string()));
    }

    let declared_components = num_components;
    let num_components = num_components.min(16384).min(reader.remaining() / 3);

    if num_components == 0 {
        return Err(VexelError::Custom("JPEG 2000: SIZ component parameters truncated".to_string()));
    }

    if num_components != declared_components {
        log_warn!("JPEG 2000: SIZ declares {} components but only {} are present", declared_components, num_components);
    }

    let mut components = Vec::with_capacity(num_components);
    for _ in 0..num_components {
        let ssiz = reader.u8()?;
        let dx = reader.u8()? as u32;
        let dy = reader.u8()? as u32;

        components.push(ComponentInfo {
            dx: dx.max(1),
            dy: dy.max(1),
            prec: ((ssiz & 0x7F) as u32 + 1).min(38),
            signed: ssiz & 0x80 != 0,
        });
    }

    if xsiz <= x0siz || ysiz <= y0siz {
        return Err(VexelError::InvalidDimensions {
            width: xsiz.saturating_sub(x0siz),
            height: ysiz.saturating_sub(y0siz),
        });
    }

    let xtsiz = if xtsiz == 0 {
        log_warn!("JPEG 2000: zero tile width, treating the image as one tile");
        xsiz
    } else {
        xtsiz
    };
    let ytsiz = if ytsiz == 0 {
        log_warn!("JPEG 2000: zero tile height, treating the image as one tile");
        ysiz
    } else {
        ytsiz
    };

    let xt0siz = if xt0siz > x0siz {
        log_warn!("JPEG 2000: tile origin x {} past image origin {}, clamping", xt0siz, x0siz);
        x0siz
    } else {
        xt0siz
    };
    let yt0siz = if yt0siz > y0siz {
        log_warn!("JPEG 2000: tile origin y {} past image origin {}, clamping", yt0siz, y0siz);
        y0siz
    } else {
        yt0siz
    };

    let mut siz = SizInfo { xsiz, ysiz, x0siz, y0siz, xtsiz, ytsiz, xt0siz, yt0siz, components };

    let num_tiles = (siz.num_tiles_x() as u64).saturating_mul(siz.num_tiles_y() as u64);
    if num_tiles > u16::MAX as u64 {
        log_warn!(
            "JPEG 2000: implied tile count {} exceeds the maximum of {}, treating the image as one tile",
            num_tiles,
            u16::MAX
        );
        siz.xtsiz = siz.xsiz;
        siz.ytsiz = siz.ysiz;
        siz.xt0siz = siz.x0siz;
        siz.yt0siz = siz.y0siz;
    }

    Ok(siz)
}

const CAP_PART15_BIT: u32 = 1 << (32 - 15);

fn parse_cap_has_ht(segment: &[u8]) -> bool {
    if segment.len() < 4 {
        return false;
    }

    let pcap = u32::from_be_bytes([segment[0], segment[1], segment[2], segment[3]]);
    pcap & CAP_PART15_BIT != 0
}

struct CodParams {
    style: CodingStyle,
    num_layers: u32,
    progression: ProgressionOrder,
    mct: bool,
    sop: bool,
    eph: bool,
}

fn parse_cod(segment: &[u8]) -> VexelResult<CodParams> {
    if segment.len() < 10 {
        return Err(VexelError::Custom("JPEG 2000: COD marker too short".to_string()));
    }

    let mut reader = Reader::new(segment);
    let scod = reader.u8()?;
    let prog = reader.u8()?;
    let num_layers = reader.u16()? as u32;
    let mct = reader.u8()? != 0;

    let uses_precincts = scod & 0x01 != 0;
    let sop = scod & 0x02 != 0;
    let eph = scod & 0x04 != 0;

    let progression = ProgressionOrder::from_u8(prog).unwrap_or_else(|| {
        log_warn!("JPEG 2000: unknown progression order {}, assuming LRCP", prog);
        ProgressionOrder::Lrcp
    });

    let style = parse_coding_style(&mut reader, uses_precincts)?;

    Ok(CodParams { style, num_layers: num_layers.max(1), progression, mct, sop, eph })
}

fn parse_coc(segment: &[u8], num_components: usize) -> VexelResult<(usize, CodingStyle)> {
    let mut reader = Reader::new(segment);

    let index = if num_components < 257 { reader.u8()? as usize } else { reader.u16()? as usize };
    let scoc = reader.u8()?;
    let style = parse_coding_style(&mut reader, scoc & 0x01 != 0)?;

    Ok((index, style))
}

fn parse_coding_style(reader: &mut Reader<'_>, uses_precincts: bool) -> VexelResult<CodingStyle> {
    let decomposition_levels = reader.u8()? as u32;
    let cblk_width_exp = reader.u8()? as u32 + 2;
    let cblk_height_exp = reader.u8()? as u32 + 2;
    let cblk_style = CodeBlockStyle::from_bits(reader.u8()? as u32);
    let transform = reader.u8()?;

    let decomposition_levels = if decomposition_levels > 32 {
        log_warn!("JPEG 2000: decomposition level count {} out of range, clamping to 32", decomposition_levels);
        32
    } else {
        decomposition_levels
    };

    let (cblk_width_exp, cblk_height_exp) =
        if cblk_width_exp > 10 || cblk_height_exp > 10 || cblk_width_exp + cblk_height_exp > 12 {
            log_warn!(
                "JPEG 2000: illegal code-block size 2^{} x 2^{}, falling back to 2^6 x 2^6",
                cblk_width_exp,
                cblk_height_exp
            );
            (6, 6)
        } else {
            (cblk_width_exp, cblk_height_exp)
        };

    let num_resolutions = decomposition_levels + 1;

    let mut precinct_sizes = Vec::new();
    if uses_precincts {
        for _ in 0..num_resolutions {
            match reader.u8() {
                Ok(value) => precinct_sizes.push(value as u32),
                Err(_) => break,
            }
        }
    }

    Ok(CodingStyle {
        num_resolutions,
        cblk_width_exp,
        cblk_height_exp,
        cblk_style,
        reversible: transform == 1,
        precinct_sizes,
        uses_precincts,
    })
}

fn parse_qcd(segment: &[u8]) -> VexelResult<QuantInfo> {
    let mut reader = Reader::new(segment);
    parse_quant(&mut reader)
}

fn parse_qcc(segment: &[u8], num_components: usize) -> VexelResult<(usize, QuantInfo)> {
    let mut reader = Reader::new(segment);
    let index = if num_components < 257 { reader.u8()? as usize } else { reader.u16()? as usize };
    let quant = parse_quant(&mut reader)?;
    Ok((index, quant))
}

fn parse_quant(reader: &mut Reader<'_>) -> VexelResult<QuantInfo> {
    let sqcd = reader.u8()?;
    let guard_bits = (sqcd >> 5) as u32;

    let raw_style = sqcd & 0x1F;
    let style = QuantStyle::from_bits(raw_style).unwrap_or_else(|| {
        log_warn!("JPEG 2000: unknown quantization style {}, assuming expounded scalar", raw_style);
        QuantStyle::ScalarExpounded
    });

    let mut step_sizes = Vec::new();

    match style {
        QuantStyle::NoQuantization => {
            while let Ok(value) = reader.u8() {
                step_sizes.push(StepSize { expn: (value >> 3) as i32, mant: 0 });
            }
        }
        QuantStyle::ScalarDerived | QuantStyle::ScalarExpounded => {
            while let Ok(value) = reader.u16() {
                step_sizes.push(StepSize { expn: (value >> 11) as i32, mant: (value & 0x7FF) as i32 });
            }
        }
    }

    if step_sizes.is_empty() {
        log_warn!("JPEG 2000: quantization marker carries no step sizes, using defaults");
    }

    Ok(QuantInfo { style, guard_bits, step_sizes })
}

fn parse_rgn(segment: &[u8], num_components: usize) -> Option<(usize, u32)> {
    let mut reader = Reader::new(segment);

    let index = if num_components < 257 { reader.u8().ok()? as usize } else { reader.u16().ok()? as usize };
    let style = reader.u8().ok()?;
    let shift = reader.u8().ok()? as u32;

    if style != 0 {
        log_warn!("JPEG 2000: unsupported ROI style {}", style);
        return None;
    }

    Some((index, shift))
}

pub struct Jp2Container {
    pub codestream: Vec<u8>,
    pub palette: Option<Palette>,
    pub channel_definitions: Vec<ChannelDefinition>,
    pub enumerated_colorspace: Option<Colorspace>,
}

#[derive(Debug, Clone, Copy)]
pub struct ChannelDefinition {
    pub channel: u16,
    pub kind: ChannelType,
    pub association: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Color,
    Opacity,
    PremultipliedOpacity,
    Unknown(u16),
}

impl ChannelType {
    fn from_u16(value: u16) -> Self {
        match value {
            0 => ChannelType::Color,
            1 => ChannelType::Opacity,
            2 => ChannelType::PremultipliedOpacity,
            other => ChannelType::Unknown(other),
        }
    }

    pub fn is_alpha(&self) -> bool {
        matches!(self, ChannelType::Opacity | ChannelType::PremultipliedOpacity)
    }
}

pub struct Palette {
    pub entries: Vec<Vec<i32>>,
    pub column_depths: Vec<u32>,
    pub column_signed: Vec<bool>,
    pub component_mapping: Vec<ComponentMapping>,
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentMapping {
    pub component: u16,
    pub kind: MappingType,
    pub palette_column: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingType {
    Direct,
    Palette,
}

pub fn parse_jp2(data: &[u8]) -> VexelResult<Jp2Container> {
    let mut container = Jp2Container {
        codestream: Vec::new(),
        palette: None,
        channel_definitions: Vec::new(),
        enumerated_colorspace: None,
    };

    walk_boxes(data, &mut container, 0);

    if container.codestream.is_empty() {
        match find_raw_codestream(data) {
            Some(start) => {
                log_warn!("JPEG 2000: no codestream box found, using the codestream at offset {}", start);
                container.codestream = data[start..].to_vec();
            }
            None => return Err(VexelError::Custom("JPEG 2000: JP2 file contains no codestream box".to_string())),
        }
    }

    Ok(container)
}

fn find_raw_codestream(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == [0xFF, 0x4F, 0xFF, 0x51])
}

fn walk_boxes(data: &[u8], container: &mut Jp2Container, depth: u32) {
    if depth > 8 {
        return;
    }

    let mut pos = 0;

    while pos + 8 <= data.len() {
        let mut length = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as u64;
        let box_type =
            BoxType::from_u32(u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]));
        let mut header_len = 8usize;

        if length == 1 {
            if pos + 16 > data.len() {
                return;
            }
            length = u64::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]);
            header_len = 16;
        } else if length == 0 {
            length = (data.len() - pos) as u64;
        }

        if length < header_len as u64 {
            return;
        }

        let end = (pos as u64 + length).min(data.len() as u64) as usize;
        let body_start = pos + header_len;
        if body_start > end {
            return;
        }

        let body = &data[body_start..end];

        match box_type {
            BoxType::Codestream => {
                if container.codestream.is_empty() {
                    container.codestream = body.to_vec();
                }
            }
            BoxType::Header => walk_boxes(body, container, depth + 1),
            BoxType::Colr => {
                if body.len() >= 7 && body[0] == 1 {
                    container.enumerated_colorspace =
                        Some(Colorspace::from_u32(u32::from_be_bytes([body[3], body[4], body[5], body[6]])));
                }
            }
            BoxType::Palette => {
                container.palette = parse_palette(body);
            }
            BoxType::ComponentMapping => {
                if let Some(palette) = container.palette.as_mut() {
                    palette.component_mapping = parse_component_mapping(body);
                }
            }
            BoxType::ChannelDefinition => {
                container.channel_definitions = parse_channel_definitions(body);
            }
            _ => {}
        }

        if end <= pos {
            return;
        }
        pos = end;
    }
}

fn parse_palette(body: &[u8]) -> Option<Palette> {
    if body.len() < 3 {
        return None;
    }

    let num_entries = u16::from_be_bytes([body[0], body[1]]) as usize;
    let num_columns = body[2] as usize;

    if num_entries == 0 || num_columns == 0 || body.len() < 3 + num_columns {
        return None;
    }

    let mut depths = Vec::with_capacity(num_columns);
    for i in 0..num_columns {
        let value = body[3 + i];
        depths.push(((value & 0x7F) as u32 + 1, value & 0x80 != 0));
    }

    let mut pos = 3 + num_columns;
    let mut entries = Vec::with_capacity(num_entries);

    for _ in 0..num_entries {
        let mut row = Vec::with_capacity(num_columns);

        for &(depth, signed) in &depths {
            let bytes = depth.div_ceil(8) as usize;
            if pos + bytes > body.len() {
                return None;
            }

            let mut value: u32 = 0;
            for k in 0..bytes {
                value = (value << 8) | body[pos + k] as u32;
            }
            pos += bytes;

            let mut sample = value as i32;
            if signed && depth < 32 && value & (1 << (depth - 1)) != 0 {
                sample = value as i32 - (1i32 << depth);
            }
            row.push(sample);
        }

        entries.push(row);
    }

    Some(Palette {
        entries,
        column_depths: depths.iter().map(|(depth, _)| *depth).collect(),
        column_signed: depths.iter().map(|(_, signed)| *signed).collect(),
        component_mapping: Vec::new(),
    })
}

fn parse_component_mapping(body: &[u8]) -> Vec<ComponentMapping> {
    let mut mapping = Vec::new();
    let mut pos = 0;

    while pos + 4 <= body.len() {
        mapping.push(ComponentMapping {
            component: u16::from_be_bytes([body[pos], body[pos + 1]]),
            kind: if body[pos + 2] == 0 { MappingType::Direct } else { MappingType::Palette },
            palette_column: body[pos + 3],
        });
        pos += 4;
    }

    mapping
}

fn parse_channel_definitions(body: &[u8]) -> Vec<ChannelDefinition> {
    if body.len() < 2 {
        return Vec::new();
    }

    let count = u16::from_be_bytes([body[0], body[1]]) as usize;
    let mut definitions = Vec::with_capacity(count);
    let mut pos = 2;

    for _ in 0..count {
        if pos + 6 > body.len() {
            break;
        }
        definitions.push(ChannelDefinition {
            channel: u16::from_be_bytes([body[pos], body[pos + 1]]),
            kind: ChannelType::from_u16(u16::from_be_bytes([body[pos + 2], body[pos + 3]])),
            association: u16::from_be_bytes([body[pos + 4], body[pos + 5]]),
        });
        pos += 6;
    }

    definitions
}
