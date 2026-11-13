use std::io::{Error, ErrorKind, Read, Seek};

use super::color::JpegColorspace;
use super::decoder::{JpegDecoder, HIERARCHICAL_PIXEL_BUDGET};
use super::planes::{ComponentPlane, RasterPlane};
use crate::decoders::jpeg::types::{
    ColorComponentInfo, JpegCodingMethod, JpegMode, Predictor, QuantizationTable, ScanData,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image, PixelData};

pub(super) struct HierarchicalFrame {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) mode: JpegMode,
    pub(super) coding_method: JpegCodingMethod,
    pub(super) precision: u8,
    pub(super) components: Vec<ColorComponentInfo>,
    pub(super) quantization_tables: Vec<QuantizationTable>,
    pub(super) scans: Vec<ScanData>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn decode_frame_to_pixels(&mut self, frame: &HierarchicalFrame) -> VexelResult<Vec<Vec<i32>>> {
        let saved_width = self.width;
        let saved_height = self.height;
        let saved_mode = self.mode.clone();
        let saved_coding = self.coding_method.clone();
        let saved_precision = self.precision;
        let saved_components = self.components.clone();
        let saved_quantization = self.quantization_tables.clone();
        let saved_scans = std::mem::take(&mut self.scans);
        let saved_restart = self.restart_interval;

        self.width = frame.width;
        self.height = frame.height;
        self.mode = frame.mode.clone();
        self.coding_method = frame.coding_method.clone();
        self.precision = frame.precision;
        self.components = frame.components.clone();
        self.quantization_tables = frame.quantization_tables.clone();
        self.scans = frame.scans.clone();

        let result = self.decode_frame_pixels_internal();

        self.width = saved_width;
        self.height = saved_height;
        self.mode = saved_mode;
        self.coding_method = saved_coding;
        self.precision = saved_precision;
        self.components = saved_components;
        self.quantization_tables = saved_quantization;
        self.scans = saved_scans;
        self.restart_interval = saved_restart;

        result
    }

    pub(super) fn decode_frame_pixels_internal(&mut self) -> VexelResult<Vec<Vec<i32>>> {
        let w = self.width as usize;
        let h = self.height as usize;
        let nc = self.components.len();

        if w == 0 || h == 0 {
            log_warn!("Hierarchical frame has dimensions {}x{}, skipping it", w, h);
            return Ok(vec![Vec::new(); nc]);
        }

        match self.mode.clone() {
            JpegMode::DifferentialLossless => {
                let scan = match self.scans.first() {
                    Some(s) => s.clone(),
                    None => return Ok(vec![vec![0i32; w * h]; nc]),
                };

                let differences = if self.coding_method == JpegCodingMethod::Arithmetic {
                    self.decode_differences_arithmetic(&scan)?
                } else {
                    self.decode_differences(&scan)?
                };

                Ok(differences)
            }

            JpegMode::Lossless => {
                let scan = match self.scans.first() {
                    Some(s) => s.clone(),
                    None => return Ok(vec![vec![0i32; w * h]; nc]),
                };

                let differences = if self.coding_method == JpegCodingMethod::Arithmetic {
                    self.decode_differences_arithmetic(&scan)?
                } else {
                    self.decode_differences(&scan)?
                };

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
                    _ => Predictor::NoPrediction,
                };

                let samples =
                    self.reconstruct_samples(differences, predictor, point_transform, scan.restart_interval)?;
                let samples_bytes = self.width as usize * self.height as usize * samples.len() * 2;
                let pixels: Vec<Vec<i32>> =
                    samples.into_iter().map(|c| c.into_iter().map(|v| v as i32).collect()).collect();
                self.limits.free_usize(samples_bytes);

                Ok(pixels)
            }

            JpegMode::Progressive | JpegMode::DifferentialProgressive => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = self.width.div_ceil(8 * max_h_samp as u32);
                let mcu_h = self.height.div_ceil(8 * max_v_samp as u32);

                let components_snapshot: Vec<_> = self
                    .components
                    .iter()
                    .map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor))
                    .collect();
                let mut planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
                for (h, v) in &components_snapshot {
                    planes.push(ComponentPlane::try_new(
                        mcu_w * 8 * *h as u32,
                        mcu_h * 8 * *v as u32,
                        &mut self.limits,
                    )?);
                }

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_progressive_scans(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_progressive_scans_arithmetic(&mut planes)?,
                }

                let raster_planes = self.dequantize_and_idct_planes(&planes, true);
                for plane in &planes {
                    plane.free(&mut self.limits);
                }
                drop(planes);
                Ok(self.planes_to_component_pixels(&raster_planes))
            }

            _ => {
                let max_h_samp = self.components.iter().map(|c| c.horizontal_sampling_factor).max().unwrap_or(1);
                let max_v_samp = self.components.iter().map(|c| c.vertical_sampling_factor).max().unwrap_or(1);
                let mcu_w = self.width.div_ceil(8 * max_h_samp as u32);
                let mcu_h = self.height.div_ceil(8 * max_v_samp as u32);

                let components_snapshot: Vec<_> = self
                    .components
                    .iter()
                    .map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor))
                    .collect();
                let mut planes: Vec<ComponentPlane> = Vec::with_capacity(components_snapshot.len());
                for (h, v) in &components_snapshot {
                    planes.push(ComponentPlane::try_new(
                        mcu_w * 8 * *h as u32,
                        mcu_h * 8 * *v as u32,
                        &mut self.limits,
                    )?);
                }

                match self.coding_method {
                    JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut planes)?,
                    JpegCodingMethod::Arithmetic => self.decode_arithmetic_to_planes(&mut planes)?,
                }

                let raster_planes = self.dequantize_and_idct_planes(&planes, false);
                for plane in &planes {
                    plane.free(&mut self.limits);
                }
                drop(planes);
                Ok(self.planes_to_component_pixels(&raster_planes))
            }
        }
    }

    pub(super) fn planes_to_component_pixels(&self, planes: &[RasterPlane]) -> Vec<Vec<i32>> {
        let tw = self.width as usize;
        let th = self.height as usize;

        planes
            .iter()
            .map(|plane| {
                let (sw, sh) = (plane.width, plane.height);

                if sw == tw && sh == th {
                    plane.data.iter().map(|&v| v as i32).collect()
                } else {
                    let mut out = vec![0i32; tw * th];
                    for dy in 0..th {
                        for dx in 0..tw {
                            let sx = (dx * sw / tw).min(sw.saturating_sub(1));
                            let sy = (dy * sh / th).min(sh.saturating_sub(1));
                            out[dy * tw + dx] = plane.data.get(sy * sw + sx).copied().unwrap_or(0) as i32;
                        }
                    }
                    out
                }
            })
            .collect()
    }

    pub(super) fn expand_2x(src: &[i32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<i32> {
        if src_w == dst_w && src_h == dst_h && src.len() == dst_w * dst_h {
            return src.to_vec();
        }

        let mut out = vec![0i32; dst_w * dst_h];
        if src_w == 0 || src_h == 0 || dst_w == 0 || src.len() < src_w * src_h {
            return out;
        }

        let expand_h = dst_w > src_w;
        let expand_v = dst_h > src_h;

        let mut h_buf = vec![0i32; dst_w * src_h];
        for (row, h_row) in src.chunks_exact(src_w).zip(h_buf.chunks_exact_mut(dst_w)) {
            if expand_h {
                for x in 0..src_w {
                    let xi2 = x * 2;
                    let next = if x + 1 < src_w { row[x + 1] } else { row[x] };
                    if xi2 < dst_w {
                        h_row[xi2] = row[x];
                    }
                    if xi2 + 1 < dst_w {
                        h_row[xi2 + 1] = (row[x] + next) >> 1;
                    }
                }
            } else {
                h_row.copy_from_slice(&row[..dst_w]);
            }
        }

        if expand_v {
            for (y, out_row) in out.chunks_exact_mut(dst_w).enumerate() {
                let sy = (y / 2).min(src_h - 1);
                let ny = (sy + 1).min(src_h - 1);
                let ra = &h_buf[sy * dst_w..(sy + 1) * dst_w];
                if y % 2 == 0 {
                    out_row.copy_from_slice(ra);
                } else {
                    let rb = &h_buf[ny * dst_w..(ny + 1) * dst_w];
                    for (o, (a, b)) in out_row.iter_mut().zip(ra.iter().zip(rb.iter())) {
                        *o = (a + b) >> 1;
                    }
                }
            }
        } else {
            let len = dst_h * dst_w;
            out.copy_from_slice(&h_buf[..len]);
        }
        out
    }

    pub(super) fn expansion_bytes(components: usize, dst_w: usize, dst_h: usize, src_h: usize) -> usize {
        dst_w.saturating_mul(dst_h.saturating_add(src_h)).saturating_mul(components).saturating_mul(4)
    }

    pub(super) fn decode_hierarchical(&mut self) -> VexelResult<Image> {
        if self.hierarchical_frames.is_empty() {
            return Err(VexelError::from(Error::new(ErrorKind::InvalidData, "No hierarchical frames found")));
        }

        let final_width = self.dhp_width;
        let final_height = self.dhp_height;
        let nc = self.hierarchical_frames[0].components.len();
        let precision = self.hierarchical_frames[0].precision;
        let level_shift = if precision <= 8 { 128i32 } else { 2048i32 };

        let frames = std::mem::take(&mut self.hierarchical_frames);
        let base = &frames[0];
        let raw_pixels = self.decode_frame_to_pixels(base)?;
        let is_base_lossless = matches!(base.mode, JpegMode::Lossless);

        let mut current_pixels: Vec<Vec<i32>> = if is_base_lossless {
            raw_pixels
        } else {
            raw_pixels.into_iter().map(|c| c.into_iter().map(|v| v + level_shift).collect()).collect()
        };

        let mut current_w = base.width as usize;
        let mut current_h = base.height as usize;

        let frame_pixel_budget = u64::from(final_width.max(base.width))
            .saturating_mul(u64::from(final_height.max(base.height)))
            .saturating_mul(HIERARCHICAL_PIXEL_BUDGET);
        let mut frame_pixels = u64::from(base.width).saturating_mul(u64::from(base.height));

        for frame in frames.iter().skip(1) {
            let dst_w = frame.width as usize;
            let dst_h = frame.height as usize;

            frame_pixels = frame_pixels.saturating_add(u64::from(frame.width).saturating_mul(u64::from(frame.height)));
            if frame_pixels > frame_pixel_budget {
                log_warn!("Hierarchical frames exceed the decoding work budget, ignoring the remaining frames");
                break;
            }

            let expand_bytes = Self::expansion_bytes(current_pixels.len(), dst_w, dst_h, current_h);
            self.limits.reserve_usize(expand_bytes)?;
            let mut upsampled: Vec<Vec<i32>> =
                current_pixels.iter().map(|comp| Self::expand_2x(comp, current_w, current_h, dst_w, dst_h)).collect();
            self.limits.free_usize(expand_bytes);

            let diff_raw = self.decode_frame_to_pixels(frame)?;

            let max_val = (1i32 << precision) - 1;
            for (comp_idx, diff) in diff_raw.iter().enumerate() {
                if comp_idx >= upsampled.len() {
                    break;
                }
                let up = &mut upsampled[comp_idx];
                for (u, d) in up.iter_mut().zip(diff.iter()) {
                    *u = (*u + *d).clamp(0, max_val);
                }
            }

            let diff_raw_bytes = diff_raw.iter().map(|v| v.len() * 4).sum::<usize>();
            self.limits.free_usize(diff_raw_bytes);

            current_pixels = upsampled;
            current_w = dst_w;
            current_h = dst_h;
        }

        self.hierarchical_frames = frames;

        let (final_width, final_height) = if final_width == 0 || final_height == 0 {
            log_warn!("DHP dimensions are {}x{}, using the last frame size", final_width, final_height);
            (current_w as u32, current_h as u32)
        } else {
            (final_width, final_height)
        };
        let out_w = final_width as usize;
        let out_h = final_height as usize;

        if out_w == 0 || out_h == 0 {
            return Err(VexelError::InvalidDimensions { width: final_width, height: final_height });
        }

        if current_w != out_w || current_h != out_h {
            let expand_bytes = Self::expansion_bytes(current_pixels.len(), out_w, out_h, current_h);
            self.limits.reserve_usize(expand_bytes)?;
            current_pixels = current_pixels
                .into_iter()
                .map(|comp| Self::expand_2x(&comp, current_w, current_h, out_w, out_h))
                .collect();
            self.limits.free_usize(expand_bytes);
        }

        if current_pixels.len() < 4 {
            current_pixels.resize(4, Vec::new());
        }

        let max_val = if precision <= 8 { 255i32 } else { 65535i32 };
        let npixels = out_w * out_h;
        let colorspace = self.detect_colorspace();

        if nc == 1 {
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels];
                for (i, p) in pixels.iter_mut().enumerate() {
                    *p = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u8;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels];
                for (i, p) in pixels.iter_mut().enumerate() {
                    *p = current_pixels[0].get(i).copied().unwrap_or(0).clamp(0, max_val) as u16;
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::L16(pixels)))
            }
        } else if nc == 4 {
            let mut pixels = vec![0u8; npixels * 3];
            for i in 0..npixels {
                let c0 = current_pixels[0].get(i).copied().unwrap_or(128).clamp(0, 255);
                let c1 = current_pixels[1].get(i).copied().unwrap_or(128).clamp(0, 255);
                let c2 = current_pixels[2].get(i).copied().unwrap_or(128).clamp(0, 255);
                let k_inv = current_pixels[3].get(i).copied().unwrap_or(128).clamp(0, 255);
                let (c_inv, m_inv, y_inv) = if colorspace == JpegColorspace::Ycck {
                    let cb = c1 - 128;
                    let cr = c2 - 128;
                    let y_luma = c0 << 16;
                    let r_inv = (255 << 16) - y_luma - 91881 * cr;
                    let g_inv = (255 << 16) - y_luma + 22554 * cb + 46802 * cr;
                    let b_inv = (255 << 16) - y_luma - 116130 * cb;
                    (
                        ((r_inv + 32768) >> 16).clamp(0, 255),
                        ((g_inv + 32768) >> 16).clamp(0, 255),
                        ((b_inv + 32768) >> 16).clamp(0, 255),
                    )
                } else {
                    (c0.clamp(0, 255), c1.clamp(0, 255), c2.clamp(0, 255))
                };
                pixels[i * 3] = ((c_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                pixels[i * 3 + 1] = ((m_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
                pixels[i * 3 + 2] = ((y_inv * k_inv + 127) / 255).clamp(0, 255) as u8;
            }
            Ok(Image::from_pixels(final_width, final_height, PixelData::RGB8(pixels)))
        } else {
            let convert_ycbcr = colorspace == JpegColorspace::YCbCr;
            if precision <= 8 {
                let mut pixels = vec![0u8; npixels * 3];
                for i in 0..npixels {
                    let c0 = current_pixels[0].get(i).copied().unwrap_or(128).clamp(0, 255);
                    let c1 = current_pixels[1].get(i).copied().unwrap_or(128).clamp(0, 255);
                    let c2 = current_pixels[2].get(i).copied().unwrap_or(128).clamp(0, 255);
                    if convert_ycbcr {
                        let cb = c1 - 128;
                        let cr = c2 - 128;
                        let y128 = c0 << 16;
                        pixels[i * 3] = ((y128 + 91881 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 1] = ((y128 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 255) as u8;
                        pixels[i * 3 + 2] = ((y128 + 116130 * cb + 32768) >> 16).clamp(0, 255) as u8;
                    } else {
                        pixels[i * 3] = c0.clamp(0, 255) as u8;
                        pixels[i * 3 + 1] = c1.clamp(0, 255) as u8;
                        pixels[i * 3 + 2] = c2.clamp(0, 255) as u8;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB8(pixels)))
            } else {
                let mut pixels = vec![0u16; npixels * 3];
                for i in 0..npixels {
                    let c0 = current_pixels[0].get(i).copied().unwrap_or(2048);
                    let c1 = current_pixels[1].get(i).copied().unwrap_or(2048);
                    let c2 = current_pixels[2].get(i).copied().unwrap_or(2048);
                    if convert_ycbcr {
                        let (c0, c1, c2) = (c0.clamp(0, 4095), c1.clamp(0, 4095), c2.clamp(0, 4095));
                        let cb = c1 - 2048;
                        let cr = c2 - 2048;
                        let y2048 = c0 << 16;
                        pixels[i * 3] = (((y2048 + 91881 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 1] =
                            (((y2048 - 22554 * cb - 46802 * cr + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                        pixels[i * 3 + 2] = (((y2048 + 116130 * cb + 32768) >> 16).clamp(0, 4095) as u16) << 4;
                    } else {
                        pixels[i * 3] = (c0.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 1] = (c1.clamp(0, 65535) as u16) << 4;
                        pixels[i * 3 + 2] = (c2.clamp(0, 65535) as u16) << 4;
                    }
                }
                Ok(Image::from_pixels(final_width, final_height, PixelData::RGB16(pixels)))
            }
        }
    }
}
