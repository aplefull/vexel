use std::io::{Error, ErrorKind, Read, Seek};

use super::arithmetic::ArithmeticDecoder;
#[cfg(feature = "rayon")]
use super::decoder::PARALLEL_LOSSLESS_MIN_SAMPLES;
use super::decoder::{JpegDecoder, LOSSLESS_PASS_BUDGET};
use super::huffman::get_next_symbol;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::types::{HuffmanTable, JpegCodingMethod, Predictor, ScanData, HUFF_PEEK_BITS};
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image, PixelData};

pub(super) struct LosslessComponent<'t> {
    pub(super) pos: usize,
    pub(super) stride: usize,
    pub(super) row_stride: usize,
    pub(super) table: &'t HuffmanTable,
    pub(super) ra: i32,
    pub(super) rb: i32,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn decode_differences(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let mut reader = JpegBitReader::new(&self.entropy[scan.data.clone()]);

        let width = self.width as usize;
        let height = self.height as usize;
        let num_components = scan.components.len();
        let buf_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(4);
        self.limits.reserve_usize(buf_bytes)?;

        let mut differences: Vec<Vec<i32>> = vec![vec![]; num_components];

        for diffs in &mut differences {
            diffs.reserve(width * height);
        }

        let restart_interval = scan.restart_interval as usize;
        let mut mcus_until_restart = restart_interval;

        let default_table = Self::default_lossless_dc_table();
        let tables: Vec<&HuffmanTable> = scan
            .components
            .iter()
            .enumerate()
            .map(|(i, scan_component)| {
                scan.dc_tables.iter().find(|t| t.id == scan_component.dc_table_selector).unwrap_or_else(|| {
                    log_warn!("No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.", i);
                    &default_table
                })
            })
            .collect();

        for _ in 0..height {
            if reader.is_exhausted() {
                log_warn!("Lossless scan data exhausted, filling the remaining differences with zeros");
                break;
            }

            for _ in 0..width {
                if restart_interval > 0 {
                    if mcus_until_restart == 0 {
                        reader.clear_buffer();
                        mcus_until_restart = restart_interval;
                    }
                    mcus_until_restart -= 1;
                }

                for (i, &dc_table) in tables.iter().enumerate() {
                    let bits_to_read = get_next_symbol(&mut reader, dc_table);

                    let diff = match bits_to_read {
                        0 => 0,
                        1..=15 => {
                            let additional_bits = reader.read_bits(bits_to_read as u32) as i32;

                            if additional_bits < (1 << (bits_to_read - 1)) {
                                additional_bits + (-1 << bits_to_read) + 1
                            } else {
                                additional_bits
                            }
                        }
                        16 if self.lossless_ssss16_is_min => 32768,
                        16 => {
                            let v = reader.read_bits(16) as i32;
                            if v < (1 << 15) {
                                v - 65535
                            } else {
                                v
                            }
                        }
                        _ => {
                            log_warn!("Invalid difference: {}", bits_to_read);
                            0
                        }
                    };

                    differences[i].push(diff);
                }
            }
        }

        for diffs in &mut differences {
            diffs.resize(width * height, 0);
        }

        Ok(differences)
    }

    pub(super) fn decode_differences_arithmetic(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let num_components = scan.components.len();
        let width = self.width as usize;
        let height = self.height as usize;

        let diff_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(4);
        let db_bytes = width.saturating_mul(num_components).saturating_mul(4);
        self.limits.reserve_usize(diff_bytes.saturating_add(db_bytes))?;

        let mut differences: Vec<Vec<i32>> = vec![vec![0i32; width * height]; num_components];

        let l_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.length)
                    .unwrap_or(0)
            })
            .collect();

        let u_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(1)
            })
            .collect();

        let ctx_table_indices: Vec<usize> =
            scan.components.iter().map(|sc| (sc.dc_table_selector & 0x3) as usize).collect();

        let mut all_contexts = vec![[0u8; 160]; 4];
        let mut arith = ArithmeticDecoder::new(&self.entropy[scan.data.clone()]);

        let mut da: Vec<i32> = vec![0i32; num_components];
        let mut db: Vec<Vec<i32>> = vec![vec![0i32; width]; num_components];

        for y in 0..height {
            if arith.is_exhausted() {
                log_warn!("Lossless arithmetic scan data exhausted, filling the remaining differences with zeros");
                break;
            }

            da.iter_mut().for_each(|d| *d = 0);

            for x in 0..width {
                #[allow(clippy::needless_range_loop)]
                for c in 0..num_components {
                    let l = l_values[c];
                    let u = u_values[c];
                    let ctx_idx = ctx_table_indices[c];

                    let v = arith.decode_lossless_difference(&mut all_contexts[ctx_idx], da[c], db[c][x], l, u);

                    differences[c][y * width + x] = v;
                    da[c] = v;
                    db[c][x] = v;
                }
            }
        }

        self.limits.free_usize(db_bytes);

        Ok(differences)
    }

    pub(super) fn predict(
        ra: i32,
        rb: i32,
        rc: i32,
        predictor: Predictor,
        point_transform: u8,
        input_precision: u8,
        at_interval_start: bool,
        on_interval_first_line: bool,
        x: usize,
    ) -> i32 {
        if at_interval_start {
            if input_precision > point_transform + 1 {
                1 << (input_precision - point_transform - 1)
            } else {
                0
            }
        } else if on_interval_first_line {
            ra
        } else if x == 0 {
            rb
        } else {
            match predictor {
                Predictor::NoPrediction => 0,
                Predictor::Ra => ra,
                Predictor::Rb => rb,
                Predictor::Rc => rc,
                Predictor::RaRbRc1 => ra + rb - rc,
                Predictor::RaRbRc2 => ra + ((rb - rc) >> 1),
                Predictor::RaRbRc3 => rb + ((ra - rc) >> 1),
                Predictor::RaRb => (ra + rb) / 2,
            }
        }
    }

    pub(super) fn reconstruct_samples(
        &mut self,
        differences: Vec<Vec<i32>>,
        predictor: Predictor,
        point_transform: u8,
        restart_interval: u16,
    ) -> VexelResult<Vec<Vec<u16>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = differences.len();

        let diff_bytes = width.saturating_mul(height).saturating_mul(components_count).saturating_mul(4);
        let samples_bytes = width.saturating_mul(height).saturating_mul(components_count).saturating_mul(2);
        self.limits.reserve_usize(samples_bytes)?;

        let mut samples = vec![vec![0u16; width * height]; components_count];

        let restart_interval = restart_interval as usize;

        if predictor == Predictor::Ra && restart_interval == 0 {
            for component_index in 0..components_count {
                let default_prediction = 1 << (self.precision - point_transform - 1);

                let first_diff = differences[component_index][0];
                samples[component_index][0] = (((default_prediction + first_diff) & 0xFFFF) as u16) << point_transform;

                for y in 1..height {
                    let diff = differences[component_index][y * width];
                    let rb = samples[component_index][(y - 1) * width] as i32;
                    samples[component_index][y * width] = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                }

                for y in 0..height {
                    for x in 1..width {
                        let index = y * width + x;
                        let diff = differences[component_index][index];
                        let ra = samples[component_index][index - 1] as i32;

                        samples[component_index][index] = (((ra + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;

                    let (at_interval_start, on_interval_first_line) = if restart_interval > 0 {
                        let rel = index % restart_interval;
                        (rel == 0, y == (index - rel) / width)
                    } else {
                        (index == 0, y == 0)
                    };

                    for component_index in 0..components_count {
                        let diff = differences[component_index][index];

                        let ra = if x > 0 { samples[component_index][index - 1] as i32 } else { 0 };
                        let rb = if y > 0 { samples[component_index][(y - 1) * width + x] as i32 } else { 0 };
                        let rc =
                            if x > 0 && y > 0 { samples[component_index][(y - 1) * width + (x - 1)] as i32 } else { 0 };

                        let prediction = Self::predict(
                            ra,
                            rb,
                            rc,
                            predictor,
                            point_transform,
                            self.precision,
                            at_interval_start,
                            on_interval_first_line,
                            x,
                        );

                        samples[component_index][index] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        }

        self.limits.free_usize(diff_bytes);

        Ok(samples)
    }

    pub(super) fn interleaved_samples_to_image(
        &mut self,
        mut samples: Vec<u16>,
        components_count: usize,
    ) -> VexelResult<Image> {
        let sample_bytes = samples.len().saturating_mul(2);

        let ids: Vec<u8> = self.components.iter().map(|c| c.id).collect();
        let explicit_ycbcr = self.has_jfif || self.adobe_color_transform.is_some_and(|ct| ct != 0);
        if components_count == 3 && ids != [0x52, 0x47, 0x42] && explicit_ycbcr && !self.lossless_skip_color_transform {
            Self::lossless_ycbcr_to_rgb(&mut samples, self.precision);
        }

        let pixel_data = if self.precision <= 8 {
            let precision_correction = 8 - self.precision;
            self.limits.reserve_usize(samples.len())?;
            let pixels: Vec<u8> = samples.iter().map(|&s| (s as u8) << precision_correction).collect();
            drop(samples);
            self.limits.free_usize(sample_bytes);

            match components_count {
                1 => PixelData::L8(pixels),
                2 => PixelData::LA8(pixels),
                4 => PixelData::RGBA8(pixels),
                _ => PixelData::RGB8(pixels),
            }
        } else {
            let precision_correction = 16 - self.precision;
            let mut pixels = samples;
            if precision_correction > 0 {
                let shift = |chunk: &mut [u16]| {
                    for v in chunk {
                        *v <<= precision_correction;
                    }
                };

                #[cfg(feature = "rayon")]
                {
                    use rayon::prelude::*;
                    pixels.par_chunks_mut(1 << 16).for_each(shift);
                }
                #[cfg(not(feature = "rayon"))]
                shift(&mut pixels);
            }

            match components_count {
                1 => PixelData::L16(pixels),
                2 => PixelData::LA16(pixels),
                4 => PixelData::RGBA16(pixels),
                _ => PixelData::RGB16(pixels),
            }
        };

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    pub(super) fn lossless_ycbcr_to_rgb(samples: &mut [u16], precision: u8) {
        let half = 1i32 << (precision.saturating_sub(1));
        let maxval = (1i32 << precision) - 1;

        for pixel in samples.chunks_exact_mut(3) {
            let y = pixel[0] as i32;
            let cb = pixel[1] as i32 - half;
            let cr = pixel[2] as i32 - half;

            let r = y as f32 + 1.402 * cr as f32;
            let g = y as f32 - 0.3441363 * cb as f32 - 0.7141363 * cr as f32;
            let b = y as f32 + 1.772 * cb as f32;

            pixel[0] = r.round().clamp(0.0, maxval as f32) as u16;
            pixel[1] = g.round().clamp(0.0, maxval as f32) as u16;
            pixel[2] = b.round().clamp(0.0, maxval as f32) as u16;
        }
    }

    pub(super) fn decode_lossless(&mut self) -> VexelResult<Image> {
        if self.scans.is_empty() {
            return Err(VexelError::from(Error::new(ErrorKind::InvalidData, "No scan data found")));
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let num_components = self.components.len().max(1);

        let output_bytes = width.saturating_mul(height).saturating_mul(num_components).saturating_mul(2);
        self.limits.reserve_usize(output_bytes)?;
        let mut output = vec![0u16; width * height * num_components];

        let h_max = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1).max(1) as usize;
        let v_max = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1).max(1) as usize;

        let sample_budget = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(num_components as u64)
            .saturating_mul(LOSSLESS_PASS_BUDGET);
        let mut samples_scheduled = 0u64;

        let scans = std::mem::take(&mut self.scans);
        for scan in &scans {
            if scan.components.is_empty() {
                continue;
            }

            samples_scheduled = samples_scheduled.saturating_add(
                (width as u64).saturating_mul(height as u64).saturating_mul(scan.components.len() as u64),
            );
            if samples_scheduled > sample_budget {
                log_warn!("Lossless scans exceed the decoding work budget, ignoring the remaining scans");
                break;
            }

            let component_indices: Vec<usize> = scan
                .components
                .iter()
                .enumerate()
                .map(|(i, sc)| {
                    self.components.iter().position(|c| c.id == sc.component_id).unwrap_or_else(|| {
                        log_warn!(
                            "Scan component id {} not found among frame components, using scan order",
                            sc.component_id
                        );
                        i.min(num_components - 1)
                    })
                })
                .collect();

            if scan.components.len() == 1 {
                let ci = component_indices[0];
                let (h_i, v_i) = self
                    .components
                    .get(ci)
                    .map(|c| (c.horizontal_sampling_factor.max(1) as usize, c.vertical_sampling_factor.max(1) as usize))
                    .unwrap_or((1, 1));
                let comp_width = width.saturating_mul(h_i).div_ceil(h_max);
                let comp_height = height.saturating_mul(v_i).div_ceil(v_max);

                if comp_width == width && comp_height == height {
                    self.decode_lossless_scan(scan, width, height, &[(ci, num_components)], &mut output);
                } else {
                    let plane_bytes = comp_width.saturating_mul(comp_height).saturating_mul(2);
                    if self.limits.reserve_usize(plane_bytes).is_err() {
                        log_warn!("Subsampled lossless component plane exceeds limits, skipping");
                        continue;
                    }
                    let mut plane = vec![0u16; comp_width * comp_height];
                    self.decode_lossless_scan(scan, comp_width, comp_height, &[(0, 1)], &mut plane);
                    for y in 0..height {
                        let sy = (y * comp_height / height.max(1)).min(comp_height.saturating_sub(1));
                        for x in 0..width {
                            let sx = (x * comp_width / width.max(1)).min(comp_width.saturating_sub(1));
                            output[(y * width + x) * num_components + ci] = plane[sy * comp_width + sx];
                        }
                    }
                    drop(plane);
                    self.limits.free_usize(plane_bytes);
                }
            } else {
                let sampling: Vec<(usize, usize)> = component_indices
                    .iter()
                    .map(|&ci| {
                        self.components
                            .get(ci)
                            .map(|c| {
                                (
                                    c.horizontal_sampling_factor.max(1) as usize,
                                    c.vertical_sampling_factor.max(1) as usize,
                                )
                            })
                            .unwrap_or((1, 1))
                    })
                    .collect();
                let mixed_sampling = sampling.iter().any(|&(h, v)| (h, v) != (h_max, v_max));

                if mixed_sampling {
                    self.decode_lossless_scan_subsampled(
                        scan,
                        width,
                        height,
                        h_max,
                        v_max,
                        &component_indices,
                        &sampling,
                        num_components,
                        &mut output,
                    );
                } else {
                    let views: Vec<(usize, usize)> = component_indices.iter().map(|&ci| (ci, num_components)).collect();
                    self.decode_lossless_scan(scan, width, height, &views, &mut output);
                }
            }
        }
        self.scans = scans;

        self.interleaved_samples_to_image(output, num_components)
    }

    pub(super) fn decode_lossless_scan_subsampled(
        &mut self,
        scan: &ScanData,
        width: usize,
        height: usize,
        h_max: usize,
        v_max: usize,
        component_indices: &[usize],
        sampling: &[(usize, usize)],
        num_components: usize,
        output: &mut [u16],
    ) {
        let mcu_cols = width.div_ceil(h_max);
        let mcu_rows = height.div_ceil(v_max);

        let plane_dims: Vec<(usize, usize)> = sampling.iter().map(|&(h, v)| (mcu_cols * h, mcu_rows * v)).collect();

        let total_plane_samples =
            plane_dims.iter().fold(0usize, |total, &(w, h)| total.saturating_add(w.saturating_mul(h)));
        if self.limits.reserve_usize(total_plane_samples.saturating_mul(2)).is_err() {
            log_warn!("Subsampled interleaved lossless scan exceeds limits, skipping");
            return;
        }

        let mut planes: Vec<Vec<u16>> = plane_dims.iter().map(|&(w, h)| vec![0u16; w * h]).collect();

        let point_transform = scan.successive_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        let tables: Vec<HuffmanTable> = (0..sampling.len())
            .map(|i| {
                scan.components
                    .get(i)
                    .and_then(|sc| scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).cloned())
                    .unwrap_or_else(|| {
                        log_warn!(
                            "No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.",
                            i
                        );
                        Self::default_lossless_dc_table()
                    })
            })
            .collect();

        let default_prediction =
            if self.precision > point_transform + 1 { 1i32 << (self.precision - point_transform - 1) } else { 0 };
        let ssss16_is_min = self.lossless_ssss16_is_min;
        let restart_interval = scan.restart_interval as usize;

        let mut reader = JpegBitReader::new(&self.entropy[scan.data.clone()]);

        let total_mcus = mcu_cols * mcu_rows;
        let interval = if restart_interval > 0 { restart_interval } else { total_mcus };
        let mut mcu = 0usize;
        let mut restart_row_start = 0usize;

        'mcus: while mcu < total_mcus {
            if mcu > 0 {
                reader.clear_buffer();
                restart_row_start = mcu / mcu_cols;
            }
            let end = (mcu + interval).min(total_mcus);

            while mcu < end {
                if reader.is_exhausted() {
                    log_warn!("Lossless scan data exhausted after {} of {} MCUs", mcu, total_mcus);
                    break 'mcus;
                }
                let mcu_x = mcu % mcu_cols;
                let mcu_y = mcu / mcu_cols;

                for (ci, &(sh, sv)) in sampling.iter().enumerate() {
                    let (pw, _) = plane_dims[ci];
                    let plane = &mut planes[ci];
                    let base_x = mcu_x * sh;
                    let base_y = mcu_y * sv;

                    for dy in 0..sv {
                        let y = base_y + dy;
                        for dx in 0..sh {
                            let x = base_x + dx;
                            let diff = Self::decode_lossless_diff(&mut reader, &tables[ci], ssss16_is_min);

                            let at_restart_start = y == restart_row_start * sv && x == 0;
                            let on_first_line_of_interval = y == restart_row_start * sv;

                            let predicted = if at_restart_start {
                                default_prediction
                            } else if x == 0 {
                                plane[(y - 1) * pw] as i32
                            } else if on_first_line_of_interval {
                                plane[y * pw + x - 1] as i32
                            } else {
                                let a = plane[y * pw + x - 1] as i32;
                                let b = plane[(y - 1) * pw + x] as i32;
                                let c = plane[(y - 1) * pw + x - 1] as i32;
                                match predictor {
                                    Predictor::NoPrediction => 0,
                                    Predictor::Ra => a,
                                    Predictor::Rb => b,
                                    Predictor::Rc => c,
                                    Predictor::RaRbRc1 => a + b - c,
                                    Predictor::RaRbRc2 => a + ((b - c) >> 1),
                                    Predictor::RaRbRc3 => b + ((a - c) >> 1),
                                    Predictor::RaRb => (a + b) / 2,
                                }
                            };

                            let value = (((predicted + diff) & 0xFFFF) as u16) << point_transform;
                            plane[y * pw + x] = value;
                        }
                    }
                }

                mcu += 1;
            }
        }

        for (idx, &ci) in component_indices.iter().enumerate() {
            let (pw, ph) = plane_dims[idx];
            let plane = &planes[idx];
            for y in 0..height {
                let sy = (y * ph / height.max(1)).min(ph.saturating_sub(1));
                let row = &plane[sy * pw..sy * pw + pw];
                for x in 0..width {
                    let sx = (x * pw / width.max(1)).min(pw.saturating_sub(1));
                    output[(y * width + x) * num_components + ci] = row[sx];
                }
            }
        }

        self.limits.free_usize(total_plane_samples.saturating_mul(2));
    }

    pub(super) fn decode_lossless_scan(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
    ) {
        let point_transform = scan.successive_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        if self.coding_method == JpegCodingMethod::Arithmetic {
            self.decode_lossless_scan_arithmetic(scan, sw, sh, views, buf, predictor, point_transform);
        } else {
            self.decode_lossless_scan_huffman(scan, sw, sh, views, buf, predictor, point_transform);
        }
    }

    pub(super) fn decode_lossless_scan_huffman(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
    ) {
        let total = sw.saturating_mul(sh);
        if total == 0 || views.is_empty() {
            return;
        }

        let views_in_bounds = views.iter().all(|&(offset, stride)| {
            (total - 1)
                .checked_mul(stride)
                .and_then(|last| last.checked_add(offset))
                .is_some_and(|last| last < buf.len())
        });

        if !views_in_bounds {
            log_warn!("Lossless scan views exceed sample buffer bounds, skipping scan");
            return;
        }

        let tables: Vec<HuffmanTable> = (0..views.len())
            .map(|i| {
                scan.components
                    .get(i)
                    .and_then(|sc| scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).cloned())
                    .unwrap_or_else(|| {
                        log_warn!(
                            "No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.",
                            i
                        );
                        Self::default_lossless_dc_table()
                    })
            })
            .collect();

        let mut reader = JpegBitReader::new(&self.entropy[scan.data.clone()]);
        let restart_interval = scan.restart_interval as usize;
        let default_prediction =
            if self.precision > point_transform + 1 { 1i32 << (self.precision - point_transform - 1) } else { 0 };

        let make = |i: usize| LosslessComponent {
            pos: views[i].0,
            stride: views[i].1,
            row_stride: sw * views[i].1,
            table: &tables[i],
            ra: 0,
            rb: 0,
        };

        let ssss16_is_min = self.lossless_ssss16_is_min;

        #[cfg(feature = "rayon")]
        if Self::decode_lossless_scan_huffman_parallel(
            scan,
            &self.entropy,
            sw,
            sh,
            views,
            buf,
            predictor,
            point_transform,
            default_prediction,
            ssss16_is_min,
            restart_interval,
            &tables,
        ) {
            return;
        }

        macro_rules! run {
            ($components:expr, $predict:expr) => {
                Self::lossless_huffman_loop(
                    &mut reader,
                    sw,
                    total,
                    buf,
                    restart_interval,
                    point_transform,
                    default_prediction,
                    ssss16_is_min,
                    $components,
                    $predict,
                )
            };
        }

        macro_rules! dispatch {
            ($predict:expr) => {
                match views.len() {
                    1 => run!([make(0)], $predict),
                    2 => run!([make(0), make(1)], $predict),
                    3 => run!([make(0), make(1), make(2)], $predict),
                    4 => run!([make(0), make(1), make(2), make(3)], $predict),
                    n => run!((0..n).map(make).collect::<Vec<_>>(), $predict),
                }
            };
        }

        match predictor {
            Predictor::NoPrediction => dispatch!(|_, _, _| 0),
            Predictor::Ra => dispatch!(|ra, _, _| ra),
            Predictor::Rb => dispatch!(|_, rb, _| rb),
            Predictor::Rc => dispatch!(|_, _, rc| rc),
            Predictor::RaRbRc1 => dispatch!(|ra, rb, rc| ra + rb - rc),
            Predictor::RaRbRc2 => dispatch!(|ra, rb, rc| ra + ((rb - rc) >> 1)),
            Predictor::RaRbRc3 => dispatch!(|ra, rb, rc| rb + ((ra - rc) >> 1)),
            Predictor::RaRb => dispatch!(|ra, rb, _| (ra + rb) / 2),
        }
    }

    #[cfg(feature = "rayon")]
    pub(super) fn decode_lossless_scan_huffman_parallel(
        scan: &ScanData,
        entropy: &[u8],
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
        default_prediction: i32,
        ssss16_is_min: bool,
        restart_interval: usize,
        tables: &[HuffmanTable],
    ) -> bool {
        use rayon::prelude::*;

        if restart_interval == 0 || sw == 0 || !restart_interval.is_multiple_of(sw) {
            return false;
        }
        if sw.saturating_mul(sh) < PARALLEL_LOSSLESS_MIN_SAMPLES {
            return false;
        }
        let rows_per_segment = restart_interval / sw;

        let ncomp = views.len();
        if ncomp == 0 || !views.iter().enumerate().all(|(i, &(off, stride))| off == i && stride == ncomp) {
            return false;
        }
        if buf.len() < sw * sh * ncomp {
            return false;
        }

        let starts = &scan.restart_offsets;
        let expected = sh.div_ceil(rows_per_segment);
        if starts.len() < 2 || starts.len() != expected {
            return false;
        }

        let samples_per_segment = rows_per_segment * sw * ncomp;
        let data = &entropy[scan.data.clone()];

        buf[..sw * sh * ncomp].par_chunks_mut(samples_per_segment).enumerate().for_each(|(seg, out)| {
            let begin = starts[seg];
            let end = starts.get(seg + 1).copied().unwrap_or(data.len());
            let rows = out.len() / (sw * ncomp);
            if rows == 0 || begin >= end {
                return;
            }

            let mut reader = JpegBitReader::new(&data[begin..end]);
            let make = |i: usize| LosslessComponent {
                pos: i,
                stride: ncomp,
                row_stride: sw * ncomp,
                table: &tables[i],
                ra: 0,
                rb: 0,
            };

            macro_rules! run {
                ($components:expr, $predict:expr) => {
                    Self::lossless_huffman_loop(
                        &mut reader,
                        sw,
                        rows * sw,
                        out,
                        0,
                        point_transform,
                        default_prediction,
                        ssss16_is_min,
                        $components,
                        $predict,
                    )
                };
            }

            macro_rules! dispatch {
                ($predict:expr) => {
                    match ncomp {
                        1 => run!([make(0)], $predict),
                        2 => run!([make(0), make(1)], $predict),
                        3 => run!([make(0), make(1), make(2)], $predict),
                        4 => run!([make(0), make(1), make(2), make(3)], $predict),
                        n => run!((0..n).map(make).collect::<Vec<_>>(), $predict),
                    }
                };
            }

            match predictor {
                Predictor::NoPrediction => dispatch!(|_, _, _| 0),
                Predictor::Ra => dispatch!(|ra, _, _| ra),
                Predictor::Rb => dispatch!(|_, rb, _| rb),
                Predictor::Rc => dispatch!(|_, _, rc| rc),
                Predictor::RaRbRc1 => dispatch!(|ra, rb, rc| ra + rb - rc),
                Predictor::RaRbRc2 => dispatch!(|ra, rb, rc| ra + ((rb - rc) >> 1)),
                Predictor::RaRbRc3 => dispatch!(|ra, rb, rc| rb + ((ra - rc) >> 1)),
                Predictor::RaRb => dispatch!(|ra, rb, _| (ra + rb) / 2),
            }
        });

        true
    }

    pub(super) fn lossless_huffman_loop<'t, C, F>(
        reader: &mut JpegBitReader<'_>,
        sw: usize,
        total: usize,
        buf: &mut [u16],
        restart_interval: usize,
        point_transform: u8,
        default_prediction: i32,
        ssss16_is_min: bool,
        mut components: C,
        predict: F,
    ) where
        C: AsMut<[LosslessComponent<'t>]>,
        F: Fn(i32, i32, i32) -> i32,
    {
        let components = components.as_mut();

        let interval = if restart_interval > 0 { restart_interval } else { total };
        let mut mcu = 0usize;

        while mcu < total {
            if reader.is_exhausted() {
                log_warn!("Lossless scan data exhausted after {} of {} samples", mcu, total);
                return;
            }
            if mcu > 0 {
                reader.clear_buffer();
            }
            let end = (mcu + interval).min(total);

            for c in components.iter_mut() {
                let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                let value = (((default_prediction + diff) & 0xFFFF) as u16) << point_transform;
                unsafe { *buf.get_unchecked_mut(c.pos) = value };
                c.pos += c.stride;
                c.ra = value as i32;
            }
            mcu += 1;

            let first_row_end = (((mcu - 1) / sw + 1) * sw).min(end);
            while mcu < first_row_end {
                for c in components.iter_mut() {
                    let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                    let value = (((c.ra + diff) & 0xFFFF) as u16) << point_transform;
                    unsafe { *buf.get_unchecked_mut(c.pos) = value };
                    c.pos += c.stride;
                    c.ra = value as i32;
                }
                mcu += 1;
            }

            while mcu < end {
                if reader.is_exhausted() {
                    log_warn!("Lossless scan data exhausted after {} of {} samples", mcu, total);
                    return;
                }
                let row_end = (mcu + sw).min(end);

                for c in components.iter_mut() {
                    let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                    let rb = unsafe { *buf.get_unchecked(c.pos - c.row_stride) } as i32;
                    let value = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                    unsafe { *buf.get_unchecked_mut(c.pos) = value };
                    c.pos += c.stride;
                    c.ra = value as i32;
                    c.rb = rb;
                }
                mcu += 1;

                while mcu < row_end {
                    for c in components.iter_mut() {
                        let diff = Self::decode_lossless_diff(reader, c.table, ssss16_is_min);
                        let rb = unsafe { *buf.get_unchecked(c.pos - c.row_stride) } as i32;
                        let value = (((predict(c.ra, rb, c.rb) + diff) & 0xFFFF) as u16) << point_transform;
                        unsafe { *buf.get_unchecked_mut(c.pos) = value };
                        c.pos += c.stride;
                        c.ra = value as i32;
                        c.rb = rb;
                    }
                    mcu += 1;
                }
            }
        }
    }

    #[inline(always)]
    pub(super) fn decode_lossless_diff(
        reader: &mut JpegBitReader<'_>,
        table: &HuffmanTable,
        ssss16_is_min: bool,
    ) -> i32 {
        if !reader.ensure(32) {
            return Self::decode_lossless_diff_tail(reader, table, ssss16_is_min);
        }

        let window = reader.peek_unchecked(32);
        let entry = unsafe { *table.fast_lookup.get_unchecked((window >> (32 - HUFF_PEEK_BITS)) as usize) };
        let ssss = if entry >> 16 != 0 {
            let len = entry & 0xFF;
            let ssss = (entry >> 8) & 0xFF;
            if ssss.wrapping_sub(1) < 15 {
                reader.consume(len + ssss);
                let v = ((window >> (32 - len - ssss)) & ((1 << ssss) - 1)) as i32;
                return v + (((v >> (ssss - 1)) - 1) & ((-1i32 << ssss) + 1));
            }
            reader.consume(len);
            ssss
        } else {
            reader.consume(HUFF_PEEK_BITS as u32);
            let mut code = window >> (32 - HUFF_PEEK_BITS);
            let mut symbol = u32::MAX;
            for i in HUFF_PEEK_BITS..16 {
                code = (code << 1) | reader.read_bits_unchecked(1);
                let first = unsafe { *table.first_code.get_unchecked(i) };
                if first != u32::MAX {
                    let count =
                        unsafe { table.offsets.get_unchecked(i + 1).wrapping_sub(*table.offsets.get_unchecked(i)) };
                    if code >= first && code - first < count {
                        let idx = unsafe { *table.offsets.get_unchecked(i) } as usize + (code - first) as usize;
                        symbol = unsafe { *table.symbols.get_unchecked(idx) } as u32;
                        break;
                    }
                }
            }
            if symbol == u32::MAX {
                log_warn!("Invalid Huffman code: {}, replacing with 0", code);
                0
            } else {
                symbol
            }
        };

        match ssss {
            0 => 0,
            1..=15 => {
                let v = reader.read_bits(ssss) as i32;
                v + (((v >> (ssss - 1)) - 1) & ((-1i32 << ssss) + 1))
            }
            16 if ssss16_is_min => 32768,
            16 => {
                let v = reader.read_bits(16) as i32;
                v + (((v >> 15) - 1) & -65535)
            }
            _ => {
                log_warn!("Invalid difference: {}", ssss);
                0
            }
        }
    }

    #[cold]
    pub(super) fn decode_lossless_diff_tail(
        reader: &mut JpegBitReader<'_>,
        table: &HuffmanTable,
        ssss16_is_min: bool,
    ) -> i32 {
        let ssss = get_next_symbol(reader, table);
        match ssss {
            0 => 0,
            1..=15 => {
                let v = reader.read_bits(ssss as u32) as i32;
                if v < (1 << (ssss - 1)) {
                    v + (-1i32 << ssss) + 1
                } else {
                    v
                }
            }
            16 if ssss16_is_min => 32768,
            16 => {
                let v = reader.read_bits(16) as i32;
                if v < (1 << 15) {
                    v - 65535
                } else {
                    v
                }
            }
            _ => {
                log_warn!("Invalid difference: {}", ssss);
                0
            }
        }
    }

    pub(super) fn decode_lossless_scan_arithmetic(
        &mut self,
        scan: &ScanData,
        sw: usize,
        sh: usize,
        views: &[(usize, usize)],
        buf: &mut [u16],
        predictor: Predictor,
        point_transform: u8,
    ) {
        let num_components = scan.components.len();

        let l_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.length)
                    .unwrap_or(0)
            })
            .collect();

        let u_values: Vec<u8> = scan
            .components
            .iter()
            .map(|sc| {
                scan.arith_dc_tables
                    .iter()
                    .find(|t| t.identifier == sc.dc_table_selector)
                    .and_then(|t| t.values.first())
                    .map(|v| v.value)
                    .unwrap_or(1)
            })
            .collect();

        let ctx_table_indices: Vec<usize> =
            scan.components.iter().map(|sc| (sc.dc_table_selector & 0x3) as usize).collect();

        let mut all_contexts = vec![[0u8; 160]; 4];
        let mut arith = ArithmeticDecoder::new(&self.entropy[scan.data.clone()]);

        let mut da: Vec<i32> = vec![0i32; num_components];
        let mut db: Vec<Vec<i32>> = vec![vec![0i32; sw]; num_components];

        let restart_interval = scan.restart_interval as usize;
        let mut mcus_until_restart = restart_interval;
        let mut interval_start_row = 0usize;

        let mut positions: Vec<usize> = views.iter().map(|&(offset, _)| offset).collect();
        let row_strides: Vec<usize> = views.iter().map(|&(_, stride)| sw * stride).collect();

        for y in 0..sh {
            if arith.is_exhausted() {
                log_warn!("Lossless arithmetic scan data exhausted at row {} of {}", y, sh);
                break;
            }

            for v in da.iter_mut() {
                *v = 0;
            }

            #[allow(clippy::needless_range_loop)]
            for x in 0..sw {
                let (at_interval_start, on_interval_first_line) = if restart_interval > 0 {
                    if mcus_until_restart == 0 {
                        mcus_until_restart = restart_interval;
                        interval_start_row = y;
                    }
                    let at_start = mcus_until_restart == restart_interval;
                    mcus_until_restart -= 1;
                    (at_start, y == interval_start_row)
                } else {
                    (x == 0 && y == 0, y == 0)
                };

                for (j, &(_, stride)) in views.iter().enumerate() {
                    let diff = arith.decode_lossless_difference(
                        &mut all_contexts[ctx_table_indices[j]],
                        da[j],
                        db[j][x],
                        l_values[j],
                        u_values[j],
                    );

                    da[j] = diff;
                    db[j][x] = diff;

                    Self::reconstruct_lossless_sample(
                        buf,
                        positions[j],
                        stride,
                        row_strides[j],
                        x,
                        y,
                        diff,
                        predictor,
                        point_transform,
                        self.precision,
                        at_interval_start,
                        on_interval_first_line,
                    );
                    positions[j] += stride;
                }
            }
        }
    }

    pub(super) fn reconstruct_lossless_sample(
        buf: &mut [u16],
        pos: usize,
        stride: usize,
        row_stride: usize,
        x: usize,
        y: usize,
        diff: i32,
        predictor: Predictor,
        point_transform: u8,
        precision: u8,
        at_interval_start: bool,
        on_interval_first_line: bool,
    ) {
        let ra = if x > 0 { buf[pos - stride] as i32 } else { 0 };
        let rb = if y > 0 { buf[pos - row_stride] as i32 } else { 0 };
        let rc = if x > 0 && y > 0 { buf[pos - row_stride - stride] as i32 } else { 0 };

        let prediction = Self::predict(
            ra,
            rb,
            rc,
            predictor,
            point_transform,
            precision,
            at_interval_start,
            on_interval_first_line,
            x,
        );

        buf[pos] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
    }
}
