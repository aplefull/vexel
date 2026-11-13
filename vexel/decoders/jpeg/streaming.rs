use std::io::{Read, Seek};

use super::color::JpegColorspace;
use super::decoder::JpegDecoder;
use super::huffman::{decode_ac_sequential, decode_dc_diff, DctHuffman};
#[cfg(feature = "rayon")]
use super::huffman::{skim_ac_sequential, skim_dc_diff};
use super::idct::SimdLevel;
#[cfg(feature = "rayon")]
use super::output::{emit_group_boundary, GroupEdges};
use super::output::{BandWriter, ComponentLayout, RowEmitter};
use super::sample::Sample;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::types::{JpegCodingMethod, JpegMode};
#[cfg(feature = "rayon")]
use crate::log_debug;
use crate::utils::error::VexelResult;
use crate::{log_warn, Image, PixelData};

pub(super) struct ScanComp {
    frame_index: usize,
    dc: Box<DctHuffman>,
    ac: Box<DctHuffman>,
    quant: [i32; 64],
}

pub(super) struct SequentialPlan {
    comps: Vec<ScanComp>,
    sampling: Vec<(usize, usize)>,
    hmax: usize,
    vmax: usize,
    mcus_x: usize,
    mcus_y: usize,
    width: usize,
    height: usize,
    level: SimdLevel,
    color: JpegColorspace,
    precision: u8,
    restart_interval: usize,
}

struct McuCursor<'a> {
    reader: JpegBitReader<'a>,
    predictors: [i32; 4],
    restarts_left: usize,
}

impl<'a> McuCursor<'a> {
    fn new(data: &'a [u8], restart_interval: usize) -> Self {
        Self { reader: JpegBitReader::new(data), predictors: [0; 4], restarts_left: restart_interval }
    }
}

impl SequentialPlan {
    fn layouts(&self) -> Vec<ComponentLayout> {
        self.sampling
            .iter()
            .map(|&(h, v)| ComponentLayout::new(h, v, self.hmax, self.vmax, self.width, self.height))
            .collect()
    }

    fn emitter<S: Sample>(&self) -> RowEmitter<S> {
        RowEmitter::new(
            self.level,
            self.color,
            self.precision,
            self.width,
            self.height,
            self.hmax,
            self.vmax,
            self.layouts(),
        )
    }

    #[inline(always)]
    fn decode_row<F>(&self, cursor: &mut McuCursor<'_>, mut sink: F)
    where
        F: FnMut(&ScanComp, usize, usize, &[i16; 64], bool),
    {
        let mut block = [0i16; 64];
        for mcu_x in 0..self.mcus_x {
            if self.restart_interval > 0 {
                if cursor.restarts_left == 0 {
                    cursor.predictors = [0; 4];
                    cursor.reader.clear_buffer();
                    cursor.restarts_left = self.restart_interval;
                }
                cursor.restarts_left -= 1;
            }

            for (slot, comp) in self.comps.iter().enumerate() {
                let (h, v) = self.sampling[comp.frame_index];
                for bv in 0..v {
                    for bh in 0..h {
                        let predictor = &mut cursor.predictors[slot];
                        *predictor = predictor.wrapping_add(decode_dc_diff(&mut cursor.reader, &comp.dc));
                        block[0] = *predictor as i16;
                        let last = decode_ac_sequential(&mut cursor.reader, &comp.ac, &mut block);
                        sink(comp, mcu_x * h + bh, bv, &block, last == 0);
                        block = [0; 64];
                    }
                }
            }
        }
    }

    fn decode_streaming<S: Sample>(&self, data: &[u8], pixels: &mut [S]) -> bool {
        let level = self.level;
        let mut writer = BandWriter::new(self.emitter(), &self.sampling, self.mcus_x, 0, self.mcus_y);
        let mut cursor = McuCursor::new(data, self.restart_interval);
        for mcu_y in 0..self.mcus_y {
            self.decode_row(&mut cursor, |comp, bx, bv, block, dc_only| {
                let (band, stride) = writer.band(comp.frame_index);
                S::idct(
                    level,
                    block,
                    &comp.quant,
                    &mut band[bv * 8 * stride + bx * 8..],
                    stride,
                    dc_only,
                    self.precision,
                );
            });
            writer.finish_row(mcu_y, pixels);
        }
        cursor.reader.is_exhausted()
    }

    #[cfg(feature = "rayon")]
    fn decode_parallel<S: Sample>(&self, data: &[u8], restart_offsets: &[usize], pixels: &mut [S]) -> bool {
        let row_bytes = self.width * self.color.channels();
        let rows_per_mcu = 8 * self.vmax;
        let target = rayon::current_num_threads() * 4;

        if let Some(step) = self.restart_row_step(restart_offsets) {
            let rows_per_group = step * self.mcus_y.div_ceil(step).div_ceil(target).max(1);
            let starts: Vec<GroupStart> = (0..self.mcus_y.div_ceil(rows_per_group))
                .map(|g| {
                    let interval = g * rows_per_group * self.mcus_x / self.restart_interval;
                    GroupStart { bit: restart_offsets[interval].min(data.len()) as u64 * 8, predictors: [0; 4] }
                })
                .collect();
            let results = self.decode_groups(data, &starts, rows_per_group, pixels);
            return self.stitch_groups(&results, rows_per_group, pixels);
        }

        if self.restart_interval == 0 {
            let rows_per_group = self.mcus_y.div_ceil(target).max(1);
            if let Some(starts) = self.speculative_starts(data, rows_per_group) {
                let results = self.decode_groups(data, &starts, rows_per_group, pixels);
                let consistent = results
                    .windows(2)
                    .zip(&starts[1..])
                    .all(|(pair, next)| pair[0].end_bit == next.bit && pair[0].end_predictors == next.predictors);
                if consistent {
                    return self.stitch_groups(&results, rows_per_group, pixels);
                }
                log_debug!("Speculative entropy decoding failed to verify, decoding sequentially");
            }
        }

        let chunk = PIPELINE_ROWS * rows_per_mcu * row_bytes;
        let results = self.decode_pipelined(data, chunk, pixels);
        self.stitch_groups(&results, PIPELINE_ROWS, pixels)
    }

    #[cfg(feature = "rayon")]
    fn stitch_groups<S: Sample>(&self, results: &[GroupResult<S>], rows_per_group: usize, pixels: &mut [S]) -> bool {
        let mut emitter = self.emitter::<S>();
        let mut exhausted = false;
        for g in 0..results.len() {
            exhausted |= results[g].exhausted;
            if g > 0 {
                emit_group_boundary(
                    &mut emitter,
                    &self.sampling,
                    self.mcus_x,
                    g * rows_per_group,
                    &results[g - 1].edges,
                    &results[g].edges,
                    pixels,
                );
            }
        }
        exhausted
    }

    #[cfg(feature = "rayon")]
    fn skim_mcu(&self, reader: &mut JpegBitReader<'_>, predictors: &mut [i32; 4]) {
        for (slot, comp) in self.comps.iter().enumerate() {
            let (h, v) = self.sampling[comp.frame_index];
            for _ in 0..h * v {
                predictors[slot] = predictors[slot].wrapping_add(skim_dc_diff(reader, &comp.dc));
                skim_ac_sequential(reader, &comp.ac);
            }
        }
    }

    #[cfg(feature = "rayon")]
    fn speculative_starts(&self, data: &[u8], rows_per_group: usize) -> Option<Vec<GroupStart>> {
        use rayon::prelude::*;

        let chunks = (rayon::current_num_threads() * 2).min(data.len() / SPECULATIVE_MIN_CHUNK);
        if chunks < 2 {
            return None;
        }
        let chunk_len = data.len().div_ceil(chunks);
        let total_bits = data.len() as u64 * 8;

        let skims: Vec<(Vec<McuMark>, u64)> = (0..chunks)
            .into_par_iter()
            .map(|i| {
                let end_bit = (((i + 1) * chunk_len).min(data.len()) as u64) * 8;
                let mut reader = JpegBitReader::new(data);
                reader.seek_bits((i * chunk_len) as u64 * 8);
                let mut marks = Vec::with_capacity(chunk_len / 16);
                loop {
                    let bit = reader.bit_position();
                    if bit >= end_bit || reader.is_exhausted() {
                        break (marks, bit);
                    }
                    let mut deltas = [0i32; 4];
                    self.skim_mcu(&mut reader, &mut deltas);
                    marks.push(McuMark { bit, deltas });
                }
            })
            .collect();

        let syncs: Vec<(Vec<McuMark>, usize)> = (0..chunks - 1)
            .into_par_iter()
            .map(|i| {
                let next = &skims[i + 1].0;
                let mut reader = JpegBitReader::new(data);
                reader.seek_bits(skims[i].1);
                let mut overflow = Vec::new();
                loop {
                    let bit = reader.bit_position();
                    if let Ok(index) = next.binary_search_by_key(&bit, |m| m.bit) {
                        return Some((overflow, index));
                    }
                    if bit >= total_bits || reader.is_exhausted() || overflow.len() > SPECULATIVE_MAX_OVERFLOW {
                        return None;
                    }
                    let mut deltas = [0i32; 4];
                    self.skim_mcu(&mut reader, &mut deltas);
                    overflow.push(McuMark { bit, deltas });
                }
            })
            .collect::<Option<Vec<_>>>()?;

        let total_mcus = self.mcus_x * self.mcus_y;
        let group_mcus = rows_per_group * self.mcus_x;
        let mut starts = Vec::with_capacity(self.mcus_y.div_ceil(rows_per_group));
        let mut predictors = [0i32; 4];
        let mut index = 0usize;
        let mut from = 0usize;
        for (chunk, (marks, _)) in skims.iter().enumerate() {
            let overflow: &[McuMark] = if chunk + 1 < chunks { &syncs[chunk].0 } else { &[] };
            for mark in marks.get(from..).unwrap_or(&[]).iter().chain(overflow) {
                if index >= total_mcus {
                    break;
                }
                if index.is_multiple_of(group_mcus) {
                    starts.push(GroupStart { bit: mark.bit, predictors });
                }
                for (p, d) in predictors.iter_mut().zip(mark.deltas) {
                    *p = p.wrapping_add(d);
                }
                index += 1;
            }
            if chunk + 1 < chunks {
                from = syncs[chunk].1;
            }
        }
        (index >= total_mcus).then_some(starts)
    }

    #[cfg(feature = "rayon")]
    fn decode_groups<S: Sample>(
        &self,
        data: &[u8],
        starts: &[GroupStart],
        rows_per_group: usize,
        pixels: &mut [S],
    ) -> Vec<GroupResult<S>> {
        use rayon::prelude::*;

        let level = self.level;
        let chunk = rows_per_group * 8 * self.vmax * self.width * self.color.channels();
        pixels
            .par_chunks_mut(chunk)
            .zip(starts)
            .enumerate()
            .map(|(g, (out, start))| {
                let first_row = g * rows_per_group;
                let end_row = (first_row + rows_per_group).min(self.mcus_y);
                let mut cursor = McuCursor::new(data, self.restart_interval);
                cursor.reader.seek_bits(start.bit);
                cursor.predictors = start.predictors;
                let mut writer =
                    BandWriter::new_detached(self.emitter(), &self.sampling, self.mcus_x, first_row, end_row);
                for mcu_y in first_row..end_row {
                    self.decode_row(&mut cursor, |comp, bx, bv, block, dc_only| {
                        let (band, stride) = writer.band(comp.frame_index);
                        S::idct(
                            level,
                            block,
                            &comp.quant,
                            &mut band[bv * 8 * stride + bx * 8..],
                            stride,
                            dc_only,
                            self.precision,
                        );
                    });
                    writer.finish_row(mcu_y, out);
                }
                GroupResult {
                    exhausted: cursor.reader.is_exhausted(),
                    edges: writer.into_edges(),
                    end_bit: cursor.reader.bit_position(),
                    end_predictors: cursor.predictors,
                }
            })
            .collect()
    }

    #[cfg(feature = "rayon")]
    fn restart_row_step(&self, restart_offsets: &[usize]) -> Option<usize> {
        let interval = self.restart_interval;
        if interval == 0 {
            return None;
        }
        let total = self.mcus_x * self.mcus_y;
        if restart_offsets.len() != total.div_ceil(interval) {
            return None;
        }
        let step = interval / gcd(interval, self.mcus_x);
        (self.mcus_y / step >= 4).then_some(step)
    }

    #[cfg(feature = "rayon")]
    fn decode_pipelined<S: Sample>(&self, data: &[u8], chunk: usize, pixels: &mut [S]) -> Vec<GroupResult<S>> {
        let rows_per_group = PIPELINE_ROWS;
        let blocks_per_row: Vec<usize> = self.sampling.iter().map(|&(h, v)| self.mcus_x * h * v).collect();
        let coefficient_len: Vec<usize> = blocks_per_row.iter().map(|b| b * 64 * rows_per_group).collect();
        let level = self.level;
        let mut cursor = McuCursor::new(data, self.restart_interval);
        let free: std::sync::Mutex<Vec<Vec<Vec<i16>>>> = std::sync::Mutex::new(Vec::new());
        let free = &free;
        let groups: Vec<&mut [S]> = pixels.chunks_mut(chunk).collect();
        let finished: std::sync::Mutex<Vec<Option<GroupEdges<S>>>> =
            std::sync::Mutex::new((0..groups.len()).map(|_| None).collect());
        let finished_ref = &finished;

        rayon::scope(|scope| {
            for (g, out) in groups.into_iter().enumerate() {
                let first_row = g * rows_per_group;
                let end_row = (first_row + rows_per_group).min(self.mcus_y);
                let recycled = free.lock().ok().and_then(|mut list| list.pop());
                let mut coefficients =
                    recycled.unwrap_or_else(|| coefficient_len.iter().map(|&len| vec![0i16; len]).collect());
                for mcu_y in first_row..end_row {
                    let row = mcu_y - first_row;
                    self.decode_row(&mut cursor, |comp, bx, bv, block, _| {
                        let c = comp.frame_index;
                        let (h, v) = self.sampling[c];
                        let index = (row * v + bv) * self.mcus_x * h + bx;
                        coefficients[c][index * 64..index * 64 + 64].copy_from_slice(block);
                    });
                }
                scope.spawn(move |_| {
                    let mut writer =
                        BandWriter::new_detached(self.emitter(), &self.sampling, self.mcus_x, first_row, end_row);
                    for mcu_y in first_row..end_row {
                        let row = mcu_y - first_row;
                        for comp in &self.comps {
                            let c = comp.frame_index;
                            let (h, v) = self.sampling[c];
                            let columns = self.mcus_x * h;
                            let (band, stride) = writer.band(c);
                            for bv in 0..v {
                                let base = (row * v + bv) * columns;
                                for bx in 0..columns {
                                    let index = base + bx;
                                    let block: &mut [i16; 64] =
                                        (&mut coefficients[c][index * 64..index * 64 + 64]).try_into().unwrap();
                                    let ac = block[1..].iter().fold(0i16, |acc, &x| acc | x);
                                    let offset = bv * 8 * stride + bx * 8;
                                    S::idct(
                                        level,
                                        block,
                                        &comp.quant,
                                        &mut band[offset..],
                                        stride,
                                        ac == 0,
                                        self.precision,
                                    );
                                    *block = [0; 64];
                                }
                            }
                        }
                        writer.finish_row(mcu_y, out);
                    }
                    if let Ok(mut list) = finished_ref.lock() {
                        list[g] = Some(writer.into_edges());
                    }
                    if let Ok(mut list) = free.lock() {
                        list.push(coefficients);
                    }
                });
            }
        });
        let exhausted = cursor.reader.is_exhausted();
        finished
            .into_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|edges| GroupResult {
                exhausted,
                edges: edges.unwrap_or_else(GroupEdges::empty),
                end_bit: 0,
                end_predictors: [0; 4],
            })
            .collect()
    }
}

#[cfg(feature = "rayon")]
const PIPELINE_ROWS: usize = 4;
#[cfg(feature = "rayon")]
const SPECULATIVE_MIN_CHUNK: usize = 64 * 1024;
#[cfg(feature = "rayon")]
const SPECULATIVE_MAX_OVERFLOW: usize = 1 << 16;

#[cfg(feature = "rayon")]
struct McuMark {
    bit: u64,
    deltas: [i32; 4],
}

#[cfg(feature = "rayon")]
struct GroupStart {
    bit: u64,
    predictors: [i32; 4],
}

#[cfg(feature = "rayon")]
struct GroupResult<S> {
    exhausted: bool,
    edges: GroupEdges<S>,
    end_bit: u64,
    end_predictors: [i32; 4],
}
#[cfg(feature = "rayon")]
const PARALLEL_MIN_BLOCKS: usize = 16384;

#[cfg(feature = "rayon")]
fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

impl<R: Read + Seek> JpegDecoder<R> {
    fn sequential_plan(&self) -> Option<SequentialPlan> {
        if !(8..=16).contains(&self.precision)
            || self.coding_method != JpegCodingMethod::Huffman
            || !matches!(self.mode, JpegMode::Baseline | JpegMode::ExtendedSequential)
            || self.components.is_empty()
            || self.components.len() > 4
        {
            return None;
        }
        let scan = self.scans.first()?;
        if scan.components.len() != self.components.len() {
            return None;
        }

        let (sampling, hmax, vmax) = self.component_sampling();
        if sampling.iter().map(|(h, v)| h * v).sum::<usize>() > 10 {
            return None;
        }

        let fallback = Self::default_lossless_dc_table();
        let mut comps: Vec<ScanComp> = Vec::with_capacity(scan.components.len());
        for (slot, sc) in scan.components.iter().enumerate() {
            let frame_index = self.scan_component_index(scan, slot)?;
            if comps.iter().any(|c| c.frame_index == frame_index) {
                return None;
            }
            let dc = scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).unwrap_or_else(|| {
                log_warn!("DC table {} not found in baseline mode, substituting default", sc.dc_table_selector);
                &fallback
            });
            let ac = scan.ac_tables.iter().find(|t| t.id == sc.ac_table_selector).unwrap_or_else(|| {
                log_warn!("AC table {} not found in baseline mode, substituting default", sc.ac_table_selector);
                &fallback
            });
            let quant = self.quant_table_i32(self.components[frame_index].quantization_table_id);
            comps.push(ScanComp { frame_index, dc: DctHuffman::build(dc), ac: DctHuffman::build(ac), quant });
        }

        let width = self.width as usize;
        let height = self.height as usize;
        Some(SequentialPlan {
            comps,
            sampling,
            hmax,
            vmax,
            mcus_x: width.div_ceil(8 * hmax),
            mcus_y: height.div_ceil(8 * vmax),
            width,
            height,
            level: SimdLevel::detect(),
            color: self.detect_colorspace(),
            precision: self.precision,
            restart_interval: scan.restart_interval as usize,
        })
    }

    pub(super) fn decode_sequential_fast(&mut self) -> Option<VexelResult<Image>> {
        let plan = self.sequential_plan()?;
        let scan = self.scans.first()?;
        let data = &self.entropy[scan.data.clone()];
        let mut pixel_data = if plan.precision == 8 {
            plan.decode::<u8>(data, &scan.restart_offsets)
        } else {
            plan.decode::<u16>(data, &scan.restart_offsets)
        };
        pixel_data.correct_pixels(self.width, self.height);
        Some(Ok(Image::from_pixels(self.width, self.height, pixel_data)))
    }
}

impl SequentialPlan {
    fn decode<S: Sample>(&self, data: &[u8], restart_offsets: &[usize]) -> PixelData {
        let mut pixels = vec![S::default(); self.width * self.height * self.color.channels()];

        #[cfg(feature = "rayon")]
        let exhausted = {
            let blocks: usize = self.sampling.iter().map(|&(h, v)| h * v).sum::<usize>() * self.mcus_x * self.mcus_y;
            if blocks >= PARALLEL_MIN_BLOCKS && rayon::current_num_threads() > 1 {
                self.decode_parallel(data, restart_offsets, &mut pixels)
            } else {
                self.decode_streaming(data, &mut pixels)
            }
        };
        #[cfg(not(feature = "rayon"))]
        let exhausted = {
            let _ = restart_offsets;
            self.decode_streaming(data, &mut pixels)
        };

        if exhausted {
            log_warn!("Scan data exhausted before the end of the image, the remainder is padded");
        }
        S::into_pixels(pixels, self.color, self.precision)
    }
}
