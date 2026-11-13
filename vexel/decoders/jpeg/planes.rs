use std::fmt::Debug;
use std::io::{Read, Seek};

use super::decoder::JpegDecoder;
use super::idct::SimdLevel;
use super::output::{emit_group_boundary, BandWriter, ComponentLayout, RowEmitter};
use super::sample::Sample;
use crate::decoders::jpeg::idct::idct_islow_i16;
use crate::decoders::jpeg::types::ZIGZAG_MAP;
use crate::utils::error::VexelResult;
use crate::{Limits, PixelData};

#[derive(Debug, Clone)]
pub(super) struct ComponentPlane {
    pub(super) data: Vec<i16>,
    pub(super) blocks_per_line: u32,
}

impl ComponentPlane {
    pub(super) fn try_new(width: u32, height: u32, limits: &mut Limits) -> VexelResult<Self> {
        let blocks_per_line = width.div_ceil(8);
        let block_lines = height.div_ceil(8);
        let elem_count = (blocks_per_line as usize).saturating_mul(block_lines as usize).saturating_mul(64);
        let byte_count = elem_count.saturating_mul(2);
        limits.reserve_usize(byte_count)?;
        Ok(Self { blocks_per_line, data: vec![0; elem_count] })
    }

    pub(super) fn prefault(&mut self) {
        #[cfg(feature = "rayon")]
        if self.data.len() >= PREFAULT_MIN_ELEMENTS && rayon::current_num_threads() > 1 {
            use rayon::prelude::*;
            self.data.par_chunks_mut(PAGE_ELEMENTS * 64).for_each(|chunk| {
                for page in chunk.chunks_mut(PAGE_ELEMENTS) {
                    page[0] = 0;
                }
                std::hint::black_box(&chunk[0]);
            });
        }
    }

    pub(super) fn free(&self, limits: &mut Limits) {
        let byte_count = self.data.len().saturating_mul(2);
        limits.free_usize(byte_count);
    }

    pub(super) fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i16; 64]> {
        let block_idx = block_y as usize * self.blocks_per_line as usize + block_x as usize;
        let start = block_idx * 64;
        if start + 64 <= self.data.len() {
            (&mut self.data[start..start + 64]).try_into().ok()
        } else {
            None
        }
    }
}

pub(super) struct RasterPlane {
    pub(super) data: Vec<i16>,
    pub(super) width: usize,
    pub(super) height: usize,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn dequantize_and_idct_planes(&self, planes: &[ComponentPlane], zigzag_order: bool) -> Vec<RasterPlane> {
        let (sampling, hmax, vmax) = self.component_sampling();
        let width = self.width as usize;
        let height = self.height as usize;
        planes
            .iter()
            .enumerate()
            .map(|(ci, plane)| {
                let (h, v) = sampling.get(ci).copied().unwrap_or((1, 1));
                let layout = ComponentLayout::new(h, v, hmax, vmax, width, height);
                let (sw, sh) = (layout.width, layout.height);
                let quant = self
                    .components
                    .get(ci)
                    .map(|c| self.quant_table_i32(c.quantization_table_id))
                    .unwrap_or_else(|| self.quant_table_i32(0));
                let mut data = vec![0i16; sw * sh];
                let bpl = (plane.blocks_per_line as usize).max(1);
                let mut block = [0i16; 64];
                let mut pixels = [0i16; 64];
                for by in 0..sh.div_ceil(8) {
                    for bx in 0..sw.div_ceil(8).min(bpl) {
                        let start = (by * bpl + bx) * 64;
                        let Some(src) = plane.data.get(start..start + 64) else { continue };
                        if zigzag_order {
                            for (zz, &c) in src.iter().enumerate() {
                                block[ZIGZAG_MAP[zz] as usize] = c;
                            }
                        } else {
                            block.copy_from_slice(src);
                        }
                        idct_islow_i16(&block, &quant, &mut pixels, 8, self.precision);
                        let rows = (sh - by * 8).min(8);
                        let cols = (sw - bx * 8).min(8);
                        for row in 0..rows {
                            let dst = (by * 8 + row) * sw + bx * 8;
                            data[dst..dst + cols].copy_from_slice(&pixels[row * 8..row * 8 + cols]);
                        }
                    }
                }
                RasterPlane { data, width: sw, height: sh }
            })
            .collect()
    }

    pub(super) fn planes_to_pixels(&self, planes: &[ComponentPlane], zigzag: bool) -> Option<PixelData> {
        if !(8..=16).contains(&self.precision)
            || planes.is_empty()
            || planes.len() > 4
            || planes.len() != self.components.len()
        {
            return None;
        }
        Some(if self.precision == 8 {
            self.planes_to_samples::<u8>(planes, zigzag)
        } else {
            self.planes_to_samples::<u16>(planes, zigzag)
        })
    }

    fn planes_to_samples<S: Sample>(&self, planes: &[ComponentPlane], zigzag: bool) -> PixelData {
        let (sampling, hmax, vmax) = self.component_sampling();
        let width = self.width as usize;
        let height = self.height as usize;
        let level = SimdLevel::detect();
        let color = self.detect_colorspace();
        let layouts: Vec<ComponentLayout> =
            sampling.iter().map(|&(h, v)| ComponentLayout::new(h, v, hmax, vmax, width, height)).collect();
        let total_blocks: usize = planes.iter().map(|p| p.data.len() / 64).sum();
        let mcus_x = width.div_ceil(8 * hmax);
        let mcus_y = height.div_ceil(8 * vmax);
        let quants: Vec<[i32; 64]> =
            self.components.iter().map(|c| self.quant_table_i32(c.quantization_table_id)).collect();
        let precision = self.precision;
        let emitter = RowEmitter::<S>::new(level, color, precision, width, height, hmax, vmax, layouts);
        let row_bytes = width * emitter.channels();
        let mut pixels = vec![S::default(); row_bytes * height];
        if row_bytes == 0 || mcus_y == 0 {
            return S::into_pixels(pixels, color, precision);
        }

        let fill = |writer: &mut BandWriter<S>, mcu_y: usize| {
            let mut block = [0i16; 64];
            for (ci, plane) in planes.iter().enumerate() {
                let (_, v) = sampling[ci];
                let bpl = (plane.blocks_per_line as usize).max(1);
                let lines = plane.data.len() / 64 / bpl;
                let quant = &quants[ci];
                let (band, stride) = writer.band(ci);
                let columns = bpl.min(stride / 8);
                for bv in 0..v {
                    let by = mcu_y * v + bv;
                    if by >= lines {
                        break;
                    }
                    for bx in 0..columns {
                        let start = (by * bpl + bx) * 64;
                        let src = &plane.data[start..start + 64];
                        let ac = src[1..].iter().fold(0i16, |acc, &c| acc | c);
                        if zigzag {
                            for (zz, &c) in src.iter().enumerate() {
                                block[ZIGZAG_MAP[zz] as usize] = c;
                            }
                        } else {
                            block.copy_from_slice(src);
                        }
                        S::idct(
                            level,
                            &block,
                            quant,
                            &mut band[bv * 8 * stride + bx * 8..],
                            stride,
                            ac == 0,
                            precision,
                        );
                    }
                }
            }
        };

        let rows_per_mcu = 8 * vmax;
        let tasks = if total_blocks >= PARALLEL_MIN_BLOCKS { (mcus_y / 4).clamp(1, 64) } else { 1 };
        let mcus_per_task = mcus_y.div_ceil(tasks);
        let chunk_bytes = mcus_per_task * rows_per_mcu * row_bytes;
        let edges = run_chunks(tasks > 1, &mut pixels, chunk_bytes, |i, out| {
            let first = i * mcus_per_task;
            let end = (first + mcus_per_task).min(mcus_y);
            let mut writer = BandWriter::new_detached(emitter.clone(), &sampling, mcus_x, first, end);
            for mcu_y in first..end {
                fill(&mut writer, mcu_y);
                writer.finish_row(mcu_y, out);
            }
            writer.into_edges()
        });
        let mut emitter = emitter;
        for g in 1..edges.len() {
            emit_group_boundary(
                &mut emitter,
                &sampling,
                mcus_x,
                g * mcus_per_task,
                &edges[g - 1],
                &edges[g],
                &mut pixels,
            );
        }
        S::into_pixels(pixels, color, precision)
    }
}

const PARALLEL_MIN_BLOCKS: usize = 4096;
#[cfg(feature = "rayon")]
const PREFAULT_MIN_ELEMENTS: usize = 1 << 20;
#[cfg(feature = "rayon")]
const PAGE_ELEMENTS: usize = 2048;

fn run_chunks<P: Send, T, F>(parallel: bool, data: &mut [P], chunk: usize, work: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize, &mut [P]) -> T + Sync + Send,
{
    #[cfg(feature = "rayon")]
    if parallel {
        use rayon::prelude::*;
        return data.par_chunks_mut(chunk).enumerate().map(|(i, c)| work(i, c)).collect();
    }
    let _ = parallel;
    data.chunks_mut(chunk).enumerate().map(|(i, c)| work(i, c)).collect()
}
