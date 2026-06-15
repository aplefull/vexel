use crate::utils::error::VexelResult;
use crate::PixelData;

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

fn build_exp2_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for e in 1u8..=255 {
        table[e as usize] = f32::exp2(e as f32 - 128.0 - 8.0);
    }
    table
}

fn decode_rgbe(rgbe_data: &[u8], rgb_data: &mut [f32], width: usize) {
    let exp2_table = build_exp2_table();

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;

        rgb_data
            .par_chunks_mut(width * 3)
            .enumerate()
            .for_each(|(row, rgb_row)| {
                let rgbe_row = &rgbe_data[row * width * 4..];
                decode_rgbe_row(rgbe_row, rgb_row, width, &exp2_table);
            });
    }

    #[cfg(not(feature = "rayon"))]
    {
        for row in 0..rgb_data.len() / (width * 3) {
            let rgbe_row = &rgbe_data[row * width * 4..];
            let rgb_row = &mut rgb_data[row * width * 3..(row + 1) * width * 3];
            decode_rgbe_row(rgbe_row, rgb_row, width, &exp2_table);
        }
    }
}

fn decode_rgbe_row(rgbe_row: &[u8], rgb_row: &mut [f32], width: usize, exp2_table: &[f32; 256]) {
    let pixels = &rgbe_row[..width * 4];
    let out = &mut rgb_row[..width * 3];

    for (rgbe, rgb) in pixels.chunks_exact(4).zip(out.chunks_exact_mut(3)) {
        let e = rgbe[3];
        if e != 0 {
            let scale = exp2_table[e as usize];
            rgb[0] = rgbe[0] as f32 * scale;
            rgb[1] = rgbe[1] as f32 * scale;
            rgb[2] = rgbe[2] as f32 * scale;
        } else {
            rgb[0] = 0.0;
            rgb[1] = 0.0;
            rgb[2] = 0.0;
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
