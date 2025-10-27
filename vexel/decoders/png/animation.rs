use crate::utils::error::{VexelError, VexelResult};
use crate::{ImageFrame, PixelData};
use super::types::{ColorType, FctlChunk, PngFrame};
use super::pixels::PixelDecoder;
use flate2::read::ZlibDecoder;
use std::io::Read;

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
        prev_frame: Option<PixelData>,
        dispose_op: u8,
        prev_fctl: Option<&FctlChunk>,
    ) -> VexelResult<PixelData> {
        let frame_pixels = pixels.clone().into_rgba8();

        let mut output = match (dispose_op, prev_frame) {
            (_, None) => PixelData::RGBA8(vec![0; (self.width * self.height * 4) as usize]),

            (0, Some(prev)) => prev.clone().into_rgba8(),

            (1, Some(prev)) => {
                let mut output = prev.clone().into_rgba8();
                let output_data = output.as_bytes_mut();

                if let Some(prev_fctl) = prev_fctl {
                    for y in 0..prev_fctl.height {
                        let row_start = ((y + prev_fctl.y_offset) * self.width + prev_fctl.x_offset) as usize * 4;
                        for x in 0..prev_fctl.width {
                            let pixel_start = row_start + (x as usize * 4);
                            if pixel_start + 3 < output_data.len() {
                                output_data[pixel_start] = 0;
                                output_data[pixel_start + 1] = 0;
                                output_data[pixel_start + 2] = 0;
                                output_data[pixel_start + 3] = 0;
                            }
                        }
                    }
                }
                output
            }

            (2, Some(prev)) => {
                let mut output = prev.clone().into_rgba8();
                let output_data = output.as_bytes_mut();

                if let Some(prev_fctl) = prev_fctl {
                    for y in 0..prev_fctl.height {
                        let row_start = ((y + prev_fctl.y_offset) * self.width + prev_fctl.x_offset) as usize * 4;
                        for x in 0..prev_fctl.width {
                            let pixel_start = row_start + (x as usize * 4);
                            if pixel_start + 3 < output_data.len() {
                                output_data[pixel_start] = 0;
                                output_data[pixel_start + 1] = 0;
                                output_data[pixel_start + 2] = 0;
                                output_data[pixel_start + 3] = 0;
                            }
                        }
                    }
                }
                output
            }

            _ => PixelData::RGBA8(vec![0; (self.width * self.height * 4) as usize]),
        };

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
        pixel_decoder: &PixelDecoder,
    ) -> VexelResult<Vec<ImageFrame>> {
        let mut decoded_frames: Vec<ImageFrame> = Vec::new();
        let mut previous_frame = None;
        let mut prev_dispose_op = 0;
        let mut prev_fctl = None;

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

            let mut decoder = ZlibDecoder::new(&frame.fdat[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;

            let frame_pixels = pixel_decoder.deinterlace_scan_lines(&decompressed, fctl.width, fctl.height)?;

            let mut pixels = pixel_decoder.decode_pixels_by_type(&frame_pixels)?;

            pixels.correct_pixels(fctl.width, fctl.height);

            let out = self.compose_frame(&pixels, fctl, previous_frame, prev_dispose_op, prev_fctl)?;

            previous_frame = Some(out.clone());
            prev_dispose_op = fctl.dispose_op;
            prev_fctl = Some(fctl);

            decoded_frames.push(ImageFrame {
                width: self.width,
                height: self.height,
                delay: if fctl.delay_den == 0 {
                    (fctl.delay_num as f32 / 100.0) as u32
                } else {
                    (fctl.delay_num as f32 / fctl.delay_den as f32) as u32
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
