use crate::utils::error::VexelResult;
use crate::{log_warn, PixelData};

use super::simd;
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
        let width = self.width as usize;
        let height = self.height as usize;
        let num_pixels = width * height;
        let mut rgb_data = vec![0f32; num_pixels * 3];

        decode_rgbe(rgbe_data, &mut rgb_data, width);

        match self.format {
            HdrFormat::RGBE => Ok(PixelData::RGB32F(rgb_data)),
            HdrFormat::XYZE => {
                let mut final_data = vec![0f32; num_pixels * 3];
                convert_xyze_to_rgb(&rgb_data, &mut final_data, width);
                Ok(PixelData::RGB32F(final_data))
            }
        }
    }
}

fn decode_rgbe(rgbe_data: &[u8], rgb_data: &mut [f32], width: usize) {
    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;

        rgb_data
            .par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(row, rgb_row)| {
                let rgbe_row = &rgbe_data[row * width * 4..];
                decode_rgbe_row(rgbe_row, rgb_row, width);
            });
    }

    #[cfg(not(feature = "rayon"))]
    {
        for row in 0..rgb_data.len() / (width * 3) {
            let rgbe_row = &rgbe_data[row * width * 4..];
            let rgb_row = &mut rgb_data[row * width * 3..(row + 1) * width * 3];
            decode_rgbe_row(rgbe_row, rgb_row, width);
        }
    }
}

fn decode_rgbe_row(rgbe_row: &[u8], rgb_row: &mut [f32], width: usize) {
    for x in 0..width {
        let src = x * 4;
        let dst = x * 3;

        if src + 3 >= rgbe_row.len() {
            log_warn!("RGBE row index out of bounds: {} >= {}", src + 3, rgbe_row.len());
            continue;
        }

        if dst + 2 >= rgb_row.len() {
            log_warn!("RGB row index out of bounds: {} >= {}", dst + 2, rgb_row.len());
            continue;
        }

        let e = rgbe_row[src + 3];

        if e != 0 {
            let scale = f32::exp2(e as f32 - 128.0 - 8.0);
            rgb_row[dst] = rgbe_row[src] as f32 * scale;
            rgb_row[dst + 1] = rgbe_row[src + 1] as f32 * scale;
            rgb_row[dst + 2] = rgbe_row[src + 2] as f32 * scale;
        } else {
            rgb_row[dst] = 0.0;
            rgb_row[dst + 1] = 0.0;
            rgb_row[dst + 2] = 0.0;
        }
    }
}

fn convert_xyze_to_rgb(xyz_data: &[f32], rgb_data: &mut [f32], width: usize) {
    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;

        let row_floats = width * 3;

        xyz_data
            .par_chunks(row_floats)
            .zip(rgb_data.par_chunks_mut(row_floats))
            .for_each(|(src_row, dst_row)| {
                simd::xyz_to_rgb(src_row, dst_row);
            });
    }

    #[cfg(not(feature = "rayon"))]
    {
        simd::xyz_to_rgb(xyz_data, rgb_data);
    }
}
