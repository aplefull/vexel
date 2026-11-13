use std::io::{Read, Seek};

use super::decoder::{JpegDecoder, PROGRESSIVE_PASS_BUDGET};
use super::huffman::{decode_ac_first, decode_dc_diff, DctHuffman};
use super::planes::ComponentPlane;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::idct::nonzero_mask;
use crate::decoders::jpeg::types::{ColorComponentInfo, JpegCodingMethod, JpegMode, ScanData};
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image};

pub(super) struct ProgressiveScanComp<'a> {
    pub(super) plane_index: usize,
    pub(super) h_blocks: u8,
    pub(super) v_blocks: u8,
    pub(super) dc_table: Option<&'a DctHuffman>,
    pub(super) ac_table: Option<&'a DctHuffman>,
}

pub(super) fn run_progressive_blocks<'a, 'b, F>(
    planes: &mut [ComponentPlane],
    infos: &[ProgressiveScanComp<'b>],
    mcu_width: u32,
    mcu_height: u32,
    restart_interval: u16,
    reader: &mut JpegBitReader<'a>,
    previous_dc: &mut [i32],
    mut body: F,
) -> u64
where
    F: FnMut(&mut JpegBitReader<'a>, &mut [i16; 64], &ProgressiveScanComp<'b>, &mut [i32]),
{
    let mut restart_counter = restart_interval;
    let mut processed_mcus = 0u64;

    for mcu_y in 0..mcu_height {
        for mcu_x in 0..mcu_width {
            if reader.is_exhausted() {
                log_warn!("Scan data exhausted at MCU ({}, {}), leaving the rest of the scan empty", mcu_x, mcu_y);
                return processed_mcus;
            }
            processed_mcus += 1;

            if restart_interval > 0 {
                if restart_counter == 0 {
                    previous_dc.fill(0);
                    reader.clear_buffer();
                    restart_counter = restart_interval;
                }
                restart_counter = restart_counter.saturating_sub(1);
            }

            for info in infos {
                let h_blocks = info.h_blocks as u32;
                let v_blocks = info.v_blocks as u32;
                let plane = &mut planes[info.plane_index];
                let blocks_per_line = plane.blocks_per_line;

                for v in 0..v_blocks {
                    for h in 0..h_blocks {
                        let block_x = mcu_x * h_blocks + h;
                        let block_y = mcu_y * v_blocks + v;

                        if block_x >= blocks_per_line {
                            continue;
                        }

                        if let Some(block) = plane.get_block_mut(block_x, block_y) {
                            body(reader, block, info, previous_dc);
                        }
                    }
                }
            }
        }
    }

    processed_mcus
}

#[inline(always)]
pub(super) fn prog_dc_first(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    dc_table: Option<&DctHuffman>,
    prev_dc: &mut i32,
    successive_low: u8,
    is_differential: bool,
) -> bool {
    let Some(dc_table) = dc_table else {
        log_warn!("DC table missing for block, skipping");
        return false;
    };

    let diff = decode_dc_diff(reader, dc_table);
    let value = if is_differential { diff } else { diff.wrapping_add(*prev_dc) };
    *prev_dc = value;

    block[0] = value.wrapping_shl(successive_low as u32) as i16;
    true
}

#[inline(always)]
pub(super) fn prog_dc_refine(reader: &mut JpegBitReader<'_>, block: &mut [i16; 64], successive_low: u8) {
    let bit = reader.read_bits(1);
    block[0] = (block[0] as i32 | ((bit as i32) << successive_low)) as i16;
}

#[inline(always)]
pub(super) fn prog_ac_first(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    ac_table: Option<&DctHuffman>,
    skips: &mut u32,
    start_spectral: usize,
    end_spectral: usize,
    successive_low: u8,
    max_ac_len: u8,
) {
    if *skips > 0 {
        *skips -= 1;
        return;
    }

    let Some(ac_table) = ac_table else {
        log_warn!("AC table missing for block, skipping");
        return;
    };

    decode_ac_first(reader, ac_table, block, start_spectral, end_spectral, successive_low, max_ac_len as u32, skips);
}

#[inline(always)]
fn apply_corrections(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    mut mask: u64,
    positive: i32,
    negative: i32,
) {
    while mask != 0 {
        let count = mask.count_ones().min(24);
        let bits = reader.read_bits(count);
        for i in (0..count).rev() {
            let pos = mask.trailing_zeros() as usize & 63;
            mask &= mask - 1;
            let value = block[pos] as i32;
            let apply = ((bits >> i) & 1) as i32 & ((value & positive) == 0) as i32;
            let delta = if value >= 0 { positive } else { negative };
            block[pos] = (value + delta * apply) as i16;
        }
    }
}

#[inline(always)]
pub(super) fn prog_ac_refine(
    reader: &mut JpegBitReader<'_>,
    block: &mut [i16; 64],
    ac_table: Option<&DctHuffman>,
    skips: &mut u32,
    start_spectral: usize,
    end_spectral: usize,
    positive: i32,
    negative: i32,
) {
    let mut k = start_spectral;

    if start_spectral > end_spectral {
        if *skips > 0 {
            *skips -= 1;
        }
        return;
    }

    let mask = nonzero_mask(block) & (!0u64 >> (63 - end_spectral)) & (!0u64 << start_spectral);

    if *skips == 0 {
        let ac_table = match ac_table {
            Some(table) => table,
            None => {
                log_warn!("AC table missing for block (refining), skipping");
                return;
            }
        };

        while k <= end_spectral {
            let symbol = ac_table.symbol(reader);

            let num_zeros = symbol >> 4;
            let length = symbol & 0xF;
            let mut coefficient = 0;

            if length != 0 {
                if length != 1 {
                    log_warn!("Invalid AC coefficient length (refining): {}", length);
                    break;
                }

                coefficient = negative + (reader.read_bits(1) as i32) * (positive - negative);
            } else if num_zeros != 15 {
                *skips = 1 << num_zeros;
                *skips += reader.read_bits(num_zeros);
                break;
            }

            let mut zeros_left = num_zeros as usize;
            loop {
                let m = mask >> k;
                let ones = m.trailing_ones() as usize;
                if ones > 0 {
                    let run = if ones >= 64 { !0u64 } else { (1u64 << ones) - 1 };
                    apply_corrections(reader, block, run << k, positive, negative);
                    k += ones;
                    if k > end_spectral {
                        break;
                    }
                    continue;
                }

                if zeros_left == 0 {
                    break;
                }

                let zero_run = m.trailing_zeros() as usize;
                let skip = zeros_left.min(zero_run).min(end_spectral + 1 - k);
                zeros_left -= skip;
                k += skip;

                if k > end_spectral {
                    break;
                }
            }

            if coefficient != 0 && k <= end_spectral {
                unsafe {
                    *block.get_unchecked_mut(k) = coefficient as i16;
                }
            }

            k += 1;
        }
    }

    if *skips > 0 {
        if k <= end_spectral {
            apply_corrections(reader, block, mask & (!0u64 << k), positive, negative);
        }

        *skips -= 1;
    }
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn decode_progressive(&mut self) -> VexelResult<Image> {
        let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
        let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);

        let mcu_width = self.width.div_ceil(8 * max_h_samp as u32);
        let mcu_height = self.height.div_ceil(8 * max_v_samp as u32);

        let components_snapshot: Vec<_> =
            self.components.iter().map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor)).collect();
        let mut component_planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
        for (h, v) in &components_snapshot {
            let comp_width = mcu_width * 8 * *h as u32;
            let comp_height = mcu_height * 8 * *v as u32;
            component_planes.push(ComponentPlane::try_new(comp_width, comp_height, &mut self.limits)?);
        }
        for plane in &mut component_planes {
            plane.prefault();
        }

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut component_planes)?,
        }
        let pixel_data = self.planes_to_pixels(&component_planes, true);
        for plane in &component_planes {
            plane.free(&mut self.limits);
        }
        let mut pixel_data = pixel_data.ok_or_else(|| VexelError::Custom("Unsupported JPEG sample layout".into()))?;
        pixel_data.correct_pixels(self.width, self.height);
        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    pub(super) fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let ctx = ProgressiveContext {
            entropy: &self.entropy,
            components: &self.components,
            width: self.width,
            height: self.height,
            is_differential: matches!(self.mode, JpegMode::DifferentialProgressive | JpegMode::DifferentialSequential),
            max_ac_len: if self.precision > 8 { 15 } else { 10 },
        };

        #[cfg(feature = "rayon")]
        if decode_progressive_scans_parallel(&ctx, &self.scans, planes) {
            return Ok(());
        }

        let mut previous_dc = vec![0i32; planes.len()];
        let block_budget = Self::progressive_block_budget(planes);
        let mut blocks_visited = 0u64;
        for scan in &self.scans {
            if blocks_visited > block_budget {
                log_warn!("Progressive scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }
            let visited = decode_progressive_scan(&ctx, scan, planes, 0, &mut previous_dc);
            blocks_visited = blocks_visited.saturating_add(visited);
        }
        Ok(())
    }

    pub(super) fn progressive_block_budget(planes: &[ComponentPlane]) -> u64 {
        let total_blocks: u64 = planes.iter().map(|plane| plane.data.len() as u64 / 64).sum();
        total_blocks.saturating_mul(PROGRESSIVE_PASS_BUDGET)
    }
}

pub(super) struct ProgressiveContext<'a> {
    entropy: &'a [u8],
    components: &'a [ColorComponentInfo],
    width: u32,
    height: u32,
    is_differential: bool,
    max_ac_len: u8,
}

fn decode_progressive_scan(
    ctx: &ProgressiveContext<'_>,
    scan: &ScanData,
    planes: &mut [ComponentPlane],
    plane_base: usize,
    previous_dc: &mut [i32],
) -> u64 {
    let mut reader = JpegBitReader::new(&ctx.entropy[scan.data.clone()]);
    let mut skips: u32 = 0;
    let restart_interval = scan.restart_interval;

    let is_non_interleaved = scan.components.len() == 1;
    let max_h_samp = ctx.components.iter().map(|c| c.horizontal_sampling_factor.max(1) as u32).max().unwrap_or(1);
    let max_v_samp = ctx.components.iter().map(|c| c.vertical_sampling_factor.max(1) as u32).max().unwrap_or(1);
    let (mcu_width, mcu_height) = match scan.components.first().filter(|_| is_non_interleaved) {
        Some(sc) => match ctx.components.iter().find(|c| c.id == sc.component_id) {
            Some(c) if ctx.components.len() > 1 => {
                let w = (ctx.width * c.horizontal_sampling_factor.max(1) as u32).div_ceil(max_h_samp);
                let h = (ctx.height * c.vertical_sampling_factor.max(1) as u32).div_ceil(max_v_samp);
                (w.div_ceil(8), h.div_ceil(8))
            }
            _ => (ctx.width.div_ceil(8), ctx.height.div_ceil(8)),
        },
        None => (ctx.width.div_ceil(8 * max_h_samp), ctx.height.div_ceil(8 * max_v_samp)),
    };

    let needs_dc = scan.start_spectral == 0 && scan.successive_high == 0;
    let needs_ac = scan.end_spectral > 0;
    let dc_fast: Vec<(u8, Box<DctHuffman>)> =
        if needs_dc { scan.dc_tables.iter().map(|t| (t.id, DctHuffman::build(t))).collect() } else { Vec::new() };
    let ac_fast: Vec<(u8, Box<DctHuffman>)> =
        if needs_ac { scan.ac_tables.iter().map(|t| (t.id, DctHuffman::build(t))).collect() } else { Vec::new() };

    let scan_comp_infos: Vec<ProgressiveScanComp<'_>> = scan
        .components
        .iter()
        .filter_map(|scan_comp| {
            let (plane_index, comp) =
                match ctx.components.iter().enumerate().find(|(_, c)| c.id == scan_comp.component_id) {
                    Some((i, c)) => (i, c),
                    None => {
                        log_warn!("Component not found: {}", scan_comp.component_id);
                        return None;
                    }
                };

            let plane_index = match plane_index.checked_sub(plane_base) {
                Some(index) if index < planes.len() => index,
                _ => {
                    if plane_base == 0 {
                        log_warn!("Invalid plane index: {}", plane_index);
                    }
                    return None;
                }
            };

            let h_blocks = if is_non_interleaved { 1 } else { comp.horizontal_sampling_factor };
            let v_blocks = if is_non_interleaved { 1 } else { comp.vertical_sampling_factor };

            let dc_table = dc_fast.iter().find(|t| t.0 == scan_comp.dc_table_selector).map(|t| &*t.1);
            let ac_table = ac_fast.iter().find(|t| t.0 == scan_comp.ac_table_selector).map(|t| &*t.1);

            if scan.start_spectral == 0 && scan.successive_high == 0 && dc_table.is_none() {
                log_warn!("DC table not found: {}", scan_comp.dc_table_selector);
            }
            if scan.end_spectral > 0 && ac_table.is_none() {
                log_warn!("AC table not found: {}", scan_comp.ac_table_selector);
            }

            Some(ProgressiveScanComp { plane_index, h_blocks, v_blocks, dc_table, ac_table })
        })
        .collect();

    let start_spectral = scan.start_spectral as usize;
    let end_spectral = (scan.end_spectral as usize).min(63);
    let successive_low = scan.successive_low;
    let is_first_pass = scan.successive_high == 0;
    let has_dc = scan.start_spectral == 0;
    let has_ac = scan.end_spectral > 0;
    let positive = 1 << successive_low;
    let negative = -1 << successive_low;

    macro_rules! run_scan {
        ($body:expr) => {
            run_progressive_blocks(
                planes,
                &scan_comp_infos,
                mcu_width,
                mcu_height,
                restart_interval,
                &mut reader,
                previous_dc,
                $body,
            )
        };
    }

    let blocks_per_mcu: u64 =
        scan_comp_infos.iter().map(|info| u64::from(info.h_blocks) * u64::from(info.v_blocks)).sum();

    let processed_mcus = match (has_dc, has_ac, is_first_pass) {
        (true, false, true) => run_scan!(|reader, block, info, previous_dc| {
            prog_dc_first(
                reader,
                block,
                info.dc_table,
                &mut previous_dc[info.plane_index],
                successive_low,
                ctx.is_differential,
            );
        }),
        (true, false, false) => run_scan!(|reader, block, _info, _previous_dc| {
            prog_dc_refine(reader, block, successive_low);
        }),
        (false, true, true) => run_scan!(|reader, block, info, _previous_dc| {
            prog_ac_first(
                reader,
                block,
                info.ac_table,
                &mut skips,
                start_spectral,
                end_spectral,
                successive_low,
                ctx.max_ac_len,
            );
        }),
        (false, true, false) => run_scan!(|reader, block, info, _previous_dc| {
            prog_ac_refine(reader, block, info.ac_table, &mut skips, start_spectral, end_spectral, positive, negative);
        }),
        (true, true, true) => run_scan!(|reader, block, info, previous_dc| {
            if prog_dc_first(
                reader,
                block,
                info.dc_table,
                &mut previous_dc[info.plane_index],
                successive_low,
                ctx.is_differential,
            ) {
                prog_ac_first(
                    reader,
                    block,
                    info.ac_table,
                    &mut skips,
                    start_spectral,
                    end_spectral,
                    successive_low,
                    ctx.max_ac_len,
                );
            }
        }),
        (true, true, false) => run_scan!(|reader, block, info, _previous_dc| {
            prog_dc_refine(reader, block, successive_low);
            prog_ac_refine(reader, block, info.ac_table, &mut skips, start_spectral, end_spectral, positive, negative);
        }),
        (false, false, _) => 0,
    };
    processed_mcus.saturating_mul(blocks_per_mcu)
}

#[cfg(feature = "rayon")]
fn decode_progressive_scans_parallel(
    ctx: &ProgressiveContext<'_>,
    scans: &[ScanData],
    planes: &mut [ComponentPlane],
) -> bool {
    use rayon::prelude::*;

    let total_blocks: usize = planes.iter().map(|p| p.data.len() / 64).sum();
    if planes.len() < 2 || total_blocks < PARALLEL_SCAN_MIN_BLOCKS || rayon::current_num_threads() < 2 {
        return false;
    }

    let plane_of = |scan: &ScanData| -> Option<usize> {
        let [sc] = scan.components.as_slice() else { return None };
        ctx.components.iter().position(|c| c.id == sc.component_id).filter(|&p| p < planes.len())
    };
    let mut chains: Vec<Vec<&ScanData>> = vec![Vec::new(); planes.len()];
    let mut dc_scans = Vec::new();
    for scan in scans {
        if scan.start_spectral == 0 {
            dc_scans.push(scan);
            continue;
        }
        match plane_of(scan) {
            Some(p) => chains[p].push(scan),
            None => return false,
        }
    }
    if chains.iter().filter(|c| !c.is_empty()).count() < 2 {
        return false;
    }

    let mut previous_dc = vec![0i32; planes.len()];
    let block_budget =
        planes.iter().map(|p| p.data.len() as u64 / 64).sum::<u64>().saturating_mul(PROGRESSIVE_PASS_BUDGET);
    let mut blocks_visited = 0u64;
    for scan in dc_scans {
        if blocks_visited > block_budget {
            log_warn!("Progressive scans exceed the decoding work budget, ignoring the remaining scans");
            break;
        }
        blocks_visited = blocks_visited.saturating_add(decode_progressive_scan(ctx, scan, planes, 0, &mut previous_dc));
    }

    planes.par_iter_mut().zip(chains).enumerate().for_each(|(p, (plane, chain))| {
        let budget = (plane.data.len() as u64 / 64).saturating_mul(PROGRESSIVE_PASS_BUDGET);
        let mut visited = 0u64;
        let mut unused_dc = [0i32; 1];
        for scan in chain {
            if visited > budget {
                log_warn!("Progressive scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }
            visited = visited.saturating_add(decode_progressive_scan(
                ctx,
                scan,
                std::slice::from_mut(plane),
                p,
                &mut unused_dc,
            ));
        }
    });
    true
}

#[cfg(feature = "rayon")]
const PARALLEL_SCAN_MIN_BLOCKS: usize = 16384;
