use std::io::{Read, Seek};

use super::decoder::JpegDecoder;
use super::huffman::{decode_ac_sequential, decode_dc_diff, DctHuffman};
use super::planes::ComponentPlane;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::types::{JpegCodingMethod, JpegMode};
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image};

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn decode_huffman_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.is_empty() {
            log_warn!("No scans found in JPEG data");
            return Ok(());
        }

        let is_differential = matches!(self.mode, JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive);
        let hmax = self.components.iter().map(|c| c.horizontal_sampling_factor.max(1) as usize).max().unwrap_or(1);
        let vmax = self.components.iter().map(|c| c.vertical_sampling_factor.max(1) as usize).max().unwrap_or(1);
        let width = self.width as usize;
        let height = self.height as usize;
        let fallback = Self::default_lossless_dc_table();

        for scan in &self.scans {
            struct Target {
                plane: usize,
                h: usize,
                v: usize,
                dc: Box<DctHuffman>,
                ac: Box<DctHuffman>,
            }

            let interleaved = scan.components.len() > 1;
            let targets: Vec<Target> = scan
                .components
                .iter()
                .enumerate()
                .filter_map(|(slot, sc)| {
                    let plane = self.scan_component_index(scan, slot);
                    let Some(plane) = plane.filter(|&p| p < planes.len()) else {
                        log_warn!("Scan references unknown component {}", sc.component_id);
                        return None;
                    };
                    let comp = &self.components[plane];
                    let dc = scan.dc_tables.iter().find(|t| t.id == sc.dc_table_selector).unwrap_or_else(|| {
                        log_warn!("DC table {} not found, substituting default", sc.dc_table_selector);
                        &fallback
                    });
                    let ac = scan.ac_tables.iter().find(|t| t.id == sc.ac_table_selector).unwrap_or_else(|| {
                        log_warn!("AC table {} not found, substituting default", sc.ac_table_selector);
                        &fallback
                    });
                    let (h, v) = if interleaved {
                        (comp.horizontal_sampling_factor.max(1) as usize, comp.vertical_sampling_factor.max(1) as usize)
                    } else {
                        (1, 1)
                    };
                    Some(Target { plane, h, v, dc: DctHuffman::build(dc), ac: DctHuffman::build(ac) })
                })
                .collect();
            let Some(first) = targets.first() else { continue };

            let (mcus_x, mcus_y) = if interleaved {
                (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax))
            } else if self.components.len() == 1 {
                (width.div_ceil(8), height.div_ceil(8))
            } else {
                let comp = &self.components[first.plane];
                let comp_width = (width * comp.horizontal_sampling_factor.max(1) as usize).div_ceil(hmax);
                let comp_height = (height * comp.vertical_sampling_factor.max(1) as usize).div_ceil(vmax);
                (comp_width.div_ceil(8), comp_height.div_ceil(8))
            };

            let mut reader = JpegBitReader::new(&self.entropy[scan.data.clone()]);
            let mut predictors = [0i32; 4];
            let restart_interval = scan.restart_interval as usize;
            let mut restarts_left = restart_interval;
            let mut scratch;

            for mcu_y in 0..mcus_y {
                for mcu_x in 0..mcus_x {
                    if restart_interval > 0 {
                        if restarts_left == 0 {
                            predictors = [0; 4];
                            reader.clear_buffer();
                            restarts_left = restart_interval;
                        }
                        restarts_left -= 1;
                    }
                    for (slot, target) in targets.iter().enumerate() {
                        for bv in 0..target.v {
                            for bh in 0..target.h {
                                let block_x = (mcu_x * target.h + bh) as u32;
                                let block_y = (mcu_y * target.v + bv) as u32;
                                let block = match planes[target.plane].get_block_mut(block_x, block_y) {
                                    Some(block) => block,
                                    None => {
                                        scratch = [0; 64];
                                        &mut scratch
                                    }
                                };
                                let diff = decode_dc_diff(&mut reader, &target.dc);
                                let dc = if is_differential { diff } else { predictors[slot].wrapping_add(diff) };
                                predictors[slot] = dc;
                                block[0] = dc as i16;
                                decode_ac_sequential(&mut reader, &target.ac, block);
                            }
                        }
                    }
                }
            }

            if reader.is_exhausted() {
                log_warn!("Scan data exhausted before the end of the scan, the remainder is padded");
            }
        }

        Ok(())
    }

    pub(super) fn decode_sequential(&mut self) -> VexelResult<Image> {
        if let Some(image) = self.decode_sequential_fast() {
            return image;
        }

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

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut component_planes)?,
        }

        let pixel_data = self.planes_to_pixels(&component_planes, false);
        for plane in &component_planes {
            plane.free(&mut self.limits);
        }
        let mut pixel_data = pixel_data.ok_or_else(|| VexelError::Custom("Unsupported JPEG sample layout".into()))?;
        pixel_data.correct_pixels(self.width, self.height);
        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
