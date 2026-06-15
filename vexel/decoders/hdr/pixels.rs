use crate::utils::error::VexelResult;
use crate::{log_warn, PixelData};

use super::types::HdrFormat;

pub struct PixelDecoder {
    width: u32,
    height: u32,
    format: HdrFormat,
}

impl PixelDecoder {
    pub fn new(width: u32, height: u32, format: HdrFormat) -> Self {
        PixelDecoder { width, height, format }
    }

    pub fn decode(&self, rgbe_data: &[u8]) -> VexelResult<PixelData> {
        let num_pixels = (self.width * self.height) as usize;
        let mut rgb_data = vec![0f32; num_pixels * 3];

        for i in 0..num_pixels {
            if i * 4 + 3 >= rgbe_data.len() {
                log_warn!("Pixel index out of bounds: {} >= {}", i * 4 + 3, rgbe_data.len());
                continue;
            }

            if i * 3 + 2 >= rgb_data.len() {
                log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, rgb_data.len());
                continue;
            }

            let rgbe = &rgbe_data[i * 4..(i + 1) * 4];
            let rgb = &mut rgb_data[i * 3..(i + 1) * 3];

            if rgbe[3] != 0 {
                let scale = f32::exp2(rgbe[3] as f32 - 128.0 - 8.0);

                rgb[0] = rgbe[0] as f32 * scale;
                rgb[1] = rgbe[1] as f32 * scale;
                rgb[2] = rgbe[2] as f32 * scale;
            } else {
                rgb[0] = 0.0;
                rgb[1] = 0.0;
                rgb[2] = 0.0;
            }
        }

        match self.format {
            HdrFormat::RGBE => Ok(PixelData::RGB32F(rgb_data)),
            HdrFormat::XYZE => {
                let mut final_data = vec![0f32; num_pixels * 3];

                for i in 0..num_pixels {
                    if i * 3 + 2 >= rgb_data.len() {
                        log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, rgb_data.len());
                        continue;
                    }

                    if i * 3 + 2 >= final_data.len() {
                        log_warn!("Pixel index out of bounds: {} >= {}", i * 3 + 2, final_data.len());
                        continue;
                    }

                    let xyz = &rgb_data[i * 3..(i + 1) * 3];
                    let rgb = &mut final_data[i * 3..(i + 1) * 3];

                    rgb[0] = 3.2404542 * xyz[0] - 1.5371385 * xyz[1] - 0.4985314 * xyz[2];
                    rgb[1] = -0.9692660 * xyz[0] + 1.8760108 * xyz[1] + 0.0415560 * xyz[2];
                    rgb[2] = 0.0556434 * xyz[0] - 0.2040259 * xyz[1] + 1.0572252 * xyz[2];

                    rgb[0] = rgb[0].max(0.0);
                    rgb[1] = rgb[1].max(0.0);
                    rgb[2] = rgb[2].max(0.0);
                }

                Ok(PixelData::RGB32F(final_data))
            }
        }
    }
}
