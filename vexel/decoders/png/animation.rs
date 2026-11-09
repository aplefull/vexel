use super::pixels::{filtered_data_size, PixelDecoder};
use super::types::{ColorType, FctlChunk, PngFrame, TransparencyData};
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, ImageFrame, Limits, PixelData};

pub struct AnimationDecoder {
    width: u32,
    height: u32,
}

impl AnimationDecoder {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn canvas_offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    fn frame_fits(&self, fctl: &FctlChunk) -> bool {
        fctl.width > 0
            && fctl.height > 0
            && u64::from(fctl.x_offset) + u64::from(fctl.width) <= u64::from(self.width)
            && u64::from(fctl.y_offset) + u64::from(fctl.height) <= u64::from(self.height)
    }

    fn compose_frame(&self, pixels: PixelData, fctl: &FctlChunk, canvas: PixelData) -> VexelResult<PixelData> {
        let frame_pixels = pixels.into_rgba8();
        let mut output = canvas;

        let frame_data = frame_pixels.as_bytes();
        let output_data = output.as_bytes_mut();
        let frame_row_bytes = fctl.width as usize * 4;

        if fctl.blend_op == 0 {
            for y in 0..fctl.height {
                let frame_row_start = y as usize * frame_row_bytes;
                let output_row_start = self.canvas_offset(fctl.x_offset, fctl.y_offset + y);

                if frame_row_start + frame_row_bytes > frame_data.len()
                    || output_row_start + frame_row_bytes > output_data.len()
                {
                    continue;
                }

                output_data[output_row_start..output_row_start + frame_row_bytes]
                    .copy_from_slice(&frame_data[frame_row_start..frame_row_start + frame_row_bytes]);
            }
        } else {
            for y in 0..fctl.height {
                let frame_row_start = y as usize * frame_row_bytes;
                let output_row_start = self.canvas_offset(fctl.x_offset, fctl.y_offset + y);

                for x in 0..fctl.width as usize {
                    let fp = frame_row_start + x * 4;
                    let op = output_row_start + x * 4;

                    if fp + 4 > frame_data.len() || op + 4 > output_data.len() {
                        continue;
                    }

                    let src_a = frame_data[fp + 3] as f32 / 255.0;
                    if src_a > 0.0 {
                        let dst_a = output_data[op + 3] as f32 / 255.0;
                        let out_a = src_a + dst_a * (1.0 - src_a);

                        if out_a > 0.0 {
                            for i in 0..3 {
                                let src = frame_data[fp + i] as f32;
                                let dst = output_data[op + i] as f32;
                                output_data[op + i] = ((src * src_a + dst * dst_a * (1.0 - src_a)) / out_a) as u8;
                            }
                            output_data[op + 3] = (out_a * 255.0) as u8;
                        }
                    }
                }
            }
        }

        Ok(output)
    }

    pub fn decode_apng_frames(
        &mut self,
        frames: &[PngFrame],
        limits: &mut Limits,
        bit_depth: u8,
        color_type: ColorType,
        interlace: bool,
        palette: Option<Vec<[u8; 3]>>,
        transparency: Option<TransparencyData>,
    ) -> VexelResult<Vec<ImageFrame>> {
        if frames.iter().filter(|f| f.fctl_info.width > 0 && f.fctl_info.height > 0 && !f.fdat.is_empty()).count() == 0
        {
            return Err(VexelError::Custom("No valid frames found".into()));
        }

        let decoded_pixels = self.pre_decode_frames(frames, bit_depth, color_type, interlace, &palette, &transparency);

        let blank = PixelData::RGBA8(vec![0; self.width as usize * self.height as usize * 4]);
        let mut canvas = blank.clone();
        let mut output_frames: Vec<ImageFrame> = Vec::with_capacity(frames.len());
        let mut restore_canvas: Option<PixelData> = None;
        let mut prev_dispose_op: u8 = 0;
        let mut prev_fctl: Option<&FctlChunk> = None;

        for (frame, pixels) in frames.iter().zip(decoded_pixels) {
            let fctl = &frame.fctl_info;

            if !self.frame_fits(fctl) {
                log_warn!(
                    "Skipping APNG frame with invalid dimensions: {}x{} at ({}, {})",
                    fctl.width,
                    fctl.height,
                    fctl.x_offset,
                    fctl.y_offset
                );
                continue;
            }

            if prev_dispose_op == 2 {
                canvas = match restore_canvas.take() {
                    Some(r) => r,
                    None => blank.clone(),
                };
            } else if prev_dispose_op == 1 {
                if let Some(pf) = prev_fctl {
                    let row_bytes = pf.width as usize * 4;
                    let data = canvas.as_bytes_mut();
                    for y in 0..pf.height {
                        let row_start = self.canvas_offset(pf.x_offset, pf.y_offset + y);
                        if row_start + row_bytes <= data.len() {
                            data[row_start..row_start + row_bytes].fill(0);
                        }
                    }
                }
            }

            if fctl.dispose_op == 2 {
                restore_canvas = Some(canvas.clone());
            } else {
                restore_canvas = None;
            }

            let pixels = match pixels {
                Some(p) => p,
                None => continue,
            };

            if let Err(e) = limits.reserve_buffer(self.width, self.height, 4) {
                if output_frames.is_empty() {
                    return Err(e);
                }
                log_warn!("Stopping APNG decode after {} frames: {}", output_frames.len(), e);
                break;
            }

            canvas = self.compose_frame(pixels, fctl, canvas)?;

            prev_dispose_op = fctl.dispose_op;
            prev_fctl = Some(fctl);

            output_frames.push(ImageFrame::new(
                self.width,
                self.height,
                canvas.clone(),
                if fctl.delay_den == 0 {
                    fctl.delay_num as u32 * 10
                } else {
                    (fctl.delay_num as f32 / fctl.delay_den as f32 * 1000.0).round() as u32
                },
            ));
        }

        if output_frames.is_empty() {
            return Err(VexelError::Custom("No APNG frame fits the canvas".into()));
        }

        Ok(output_frames)
    }

    fn pre_decode_frames(
        &self,
        frames: &[PngFrame],
        bit_depth: u8,
        color_type: ColorType,
        interlace: bool,
        palette: &Option<Vec<[u8; 3]>>,
        transparency: &Option<TransparencyData>,
    ) -> Vec<Option<PixelData>> {
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;

            frames
                .par_iter()
                .map(|frame| {
                    let fctl = &frame.fctl_info;
                    if !self.frame_fits(fctl) {
                        return None;
                    }
                    decode_frame_pixels(
                        &frame.fdat,
                        bit_depth,
                        color_type,
                        fctl.width,
                        fctl.height,
                        interlace,
                        palette,
                        transparency,
                    )
                })
                .collect()
        }

        #[cfg(not(feature = "rayon"))]
        {
            frames
                .iter()
                .map(|frame| {
                    let fctl = &frame.fctl_info;
                    if !self.frame_fits(fctl) {
                        return None;
                    }
                    decode_frame_pixels(
                        &frame.fdat,
                        bit_depth,
                        color_type,
                        fctl.width,
                        fctl.height,
                        interlace,
                        palette,
                        transparency,
                    )
                })
                .collect()
        }
    }
}

fn decode_frame_pixels(
    fdat: &[u8],
    bit_depth: u8,
    color_type: ColorType,
    width: u32,
    height: u32,
    interlace: bool,
    palette: &Option<Vec<[u8; 3]>>,
    transparency: &Option<TransparencyData>,
) -> Option<PixelData> {
    let decoder = PixelDecoder::new(bit_depth, color_type, width, interlace, palette.clone(), transparency.clone());

    let channels = match color_type {
        ColorType::Grayscale | ColorType::Indexed => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::RGB => 3,
        ColorType::RGBA => 4,
    };
    let bits_per_pixel = u64::from(bit_depth) * channels;
    let capacity = filtered_data_size(bits_per_pixel, width, height, interlace);
    let decompressed = ZlibDecoder::from_slice(fdat).decode_with_max(capacity);
    let frame_pixels = decoder.deinterlace_scan_lines_owned(decompressed, width, height).ok()?;
    let mut pixels = decoder.decode_pixels_by_type(frame_pixels).ok()?;
    pixels.correct_pixels(width, height);
    Some(pixels)
}

impl PixelDecoder {
    pub fn decode_pixels_by_type(&self, frame_pixels: Vec<u8>) -> VexelResult<PixelData> {
        let color_type = self.get_color_type();
        match color_type {
            ColorType::Indexed => self.decode_indexed(frame_pixels),
            ColorType::RGB => self.decode_rgb(frame_pixels),
            ColorType::RGBA => self.decode_rgba(frame_pixels),
            ColorType::Grayscale => self.decode_grayscale(frame_pixels),
            ColorType::GrayscaleAlpha => self.decode_grayscale_alpha(frame_pixels),
        }
    }

    pub fn get_color_type(&self) -> ColorType {
        self.color_type
    }
}
