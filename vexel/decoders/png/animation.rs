use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::{ImageFrame, PixelData};
use super::types::{ColorType, FctlChunk, PngFrame, TransparencyData};
use super::pixels::PixelDecoder;

pub struct AnimationDecoder {
    width: u32,
    height: u32,
}

impl AnimationDecoder {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn compose_frame(
        &self,
        pixels: &PixelData,
        fctl: &FctlChunk,
        canvas: PixelData,
    ) -> VexelResult<PixelData> {
        let frame_pixels = pixels.clone().into_rgba8();
        let mut output = canvas;

        let frame_data = frame_pixels.as_bytes();
        let output_data = output.as_bytes_mut();

        for y in 0..fctl.height {
            let frame_row_start = (y * fctl.width) as usize * 4;
            let output_row_start = ((y + fctl.y_offset) * self.width + fctl.x_offset) as usize * 4;

            for x in 0..fctl.width {
                let frame_pixel_start = frame_row_start + (x as usize * 4);
                let output_pixel_start = output_row_start + (x as usize * 4);

                if frame_pixel_start + 4 > frame_data.len() || output_pixel_start + 4 > output_data.len() {
                    continue;
                }

                if fctl.blend_op == 0 {
                    output_data[output_pixel_start..output_pixel_start + 4]
                        .copy_from_slice(&frame_data[frame_pixel_start..frame_pixel_start + 4]);
                } else {
                    let src_a = frame_data[frame_pixel_start + 3] as f32 / 255.0;
                    if src_a > 0.0 {
                        let dst_a = output_data[output_pixel_start + 3] as f32 / 255.0;
                        let out_a = src_a + dst_a * (1.0 - src_a);

                        if out_a > 0.0 {
                            for i in 0..3 {
                                let src = frame_data[frame_pixel_start + i] as f32;
                                let dst = output_data[output_pixel_start + i] as f32;
                                let blended = ((src * src_a + dst * dst_a * (1.0 - src_a)) / out_a) as u8;
                                output_data[output_pixel_start + i] = blended;
                            }
                            output_data[output_pixel_start + 3] = (out_a * 255.0) as u8;
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
        bit_depth: u8,
        color_type: ColorType,
        interlace: bool,
        palette: Option<Vec<[u8; 3]>>,
        transparency: Option<TransparencyData>,
    ) -> VexelResult<Vec<ImageFrame>> {
        let mut decoded_frames: Vec<ImageFrame> = Vec::new();
        let blank = PixelData::RGBA8(vec![0; (self.width * self.height * 4) as usize]);
        let mut canvas = blank.clone();
        let mut restore_canvas: Option<PixelData> = None;
        let mut prev_dispose_op: u8 = 0;
        let mut prev_fctl: Option<&FctlChunk> = None;

        if frames
            .iter()
            .filter(|f| f.fctl_info.width > 0 && f.fctl_info.height > 0 && !f.fdat.is_empty())
            .count()
            == 0
        {
            return Err(VexelError::Custom("No valid frames found".into()));
        }

        for frame in frames.iter() {
            let fctl = &frame.fctl_info;

            if fctl.width == 0
                || fctl.height == 0
                || fctl.x_offset + fctl.width > self.width
                || fctl.y_offset + fctl.height > self.height
            {
                return Err(VexelError::Custom("Invalid frame dimensions".into()));
            }

            let pre_compose_canvas = if prev_dispose_op == 2 {
                match restore_canvas.take() {
                    Some(r) => r,
                    None => blank.clone(),
                }
            } else if prev_dispose_op == 1 {
                let mut cleared = canvas.clone();
                if let Some(pf) = prev_fctl {
                    let data = cleared.as_bytes_mut();
                    for y in 0..pf.height {
                        let row_start = ((y + pf.y_offset) * self.width + pf.x_offset) as usize * 4;
                        for x in 0..pf.width {
                            let px = row_start + x as usize * 4;
                            if px + 3 < data.len() {
                                data[px] = 0;
                                data[px + 1] = 0;
                                data[px + 2] = 0;
                                data[px + 3] = 0;
                            }
                        }
                    }
                }
                cleared
            } else {
                canvas.clone()
            };

            if fctl.dispose_op == 2 {
                restore_canvas = Some(pre_compose_canvas.clone());
            } else {
                restore_canvas = None;
            }

            let frame_pixel_decoder = PixelDecoder::new(
                bit_depth,
                color_type,
                fctl.width,
                interlace,
                palette.clone(),
                transparency.clone(),
            );

            let decompressed = ZlibDecoder::from_bytes(frame.fdat.clone()).decode();

            let frame_pixels = frame_pixel_decoder.deinterlace_scan_lines(&decompressed, fctl.width, fctl.height)?;

            let mut pixels = frame_pixel_decoder.decode_pixels_by_type(&frame_pixels)?;

            pixels.correct_pixels(fctl.width, fctl.height);

            let out = self.compose_frame(&pixels, fctl, pre_compose_canvas)?;

            canvas = out.clone();
            prev_dispose_op = fctl.dispose_op;
            prev_fctl = Some(fctl);

            decoded_frames.push(ImageFrame {
                width: self.width,
                height: self.height,
                delay: if fctl.delay_den == 0 {
                    fctl.delay_num as u32
                } else {
                    (fctl.delay_num as f32 / fctl.delay_den as f32 * 100.0).round() as u32
                },
                pixels: out,
            });
        }

        Ok(decoded_frames)
    }
}

impl PixelDecoder {
    pub fn decode_pixels_by_type(&self, frame_pixels: &[u8]) -> VexelResult<PixelData> {
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
