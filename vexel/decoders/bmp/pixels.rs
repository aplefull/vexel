use crate::decoders::bmp::simd as simd;
use crate::decoders::bmp::types::ColorEntry;
use crate::{log_warn, Image, PixelData};

pub struct PixelDecoder;

impl PixelDecoder {
    pub fn decode_1bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool, color_table: &[ColorEntry]) -> Image {
        if color_table.len() < 2 {
            log_warn!("Invalid color table for 1-bit image");
        }

        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize + 7) / 8) + 3) & !3;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width_usize {
                        let byte_index = row_offset + x / 8;
                        let bit_offset = 7 - (x % 8);
                        let pixel_value = if byte_index < data.len() {
                            ((data[byte_index] >> bit_offset) & 1) as usize
                        } else {
                            0
                        };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;

                for x in 0..width_usize {
                    let byte_index = row_offset + x / 8;
                    let bit_offset = 7 - (x % 8);
                    let pixel_value = if byte_index < data.len() {
                        ((data[byte_index] >> bit_offset) & 1) as usize
                    } else {
                        0
                    };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_2bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool, color_table: &[ColorEntry]) -> Image {
        if color_table.len() < 4 {
            log_warn!("Invalid color table for 2-bit image");
        }

        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize + 3) / 4) + 3) & !3;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width_usize {
                        let byte_index = row_offset + x / 4;
                        let pixel_value = if byte_index < data.len() {
                            let byte = data[byte_index];
                            let bit_offset = 6 - (x % 4) * 2;
                            ((byte >> bit_offset) & 0x03) as usize
                        } else {
                            0
                        };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;

                for x in 0..width_usize {
                    let byte_index = row_offset + x / 4;
                    let pixel_value = if byte_index < data.len() {
                        let byte = data[byte_index];
                        let bit_offset = 6 - (x % 4) * 2;
                        ((byte >> bit_offset) & 0x03) as usize
                    } else {
                        0
                    };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_4bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool, color_table: &[ColorEntry]) -> Image {
        if color_table.len() < 16 {
            log_warn!("Invalid color table for 4-bit image");
        }

        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize + 1) / 2) + 3) & !3;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width_usize {
                        let byte_index = row_offset + x / 2;
                        let pixel_value = if byte_index < data.len() {
                            let byte = data[byte_index];
                            if x % 2 == 0 { ((byte >> 4) & 0x0F) as usize } else { (byte & 0x0F) as usize }
                        } else {
                            0
                        };
                        let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                            red: 0,
                            green: 0,
                            blue: 0,
                            reserved: 0,
                        });
                        dst[x * 3] = color.red;
                        dst[x * 3 + 1] = color.green;
                        dst[x * 3 + 2] = color.blue;
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;

                for x in 0..width_usize {
                    let byte_index = row_offset + x / 2;
                    let pixel_value = if byte_index < data.len() {
                        let byte = data[byte_index];
                        if x % 2 == 0 { ((byte >> 4) & 0x0F) as usize } else { (byte & 0x0F) as usize }
                    } else {
                        0
                    };
                    let color = color_table.get(pixel_value).unwrap_or(&ColorEntry {
                        red: 0,
                        green: 0,
                        blue: 0,
                        reserved: 0,
                    });
                    image_data[dst_offset + x * 3] = color.red;
                    image_data[dst_offset + x * 3 + 1] = color.green;
                    image_data[dst_offset + x * 3 + 2] = color.blue;
                }
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_8bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool, color_table: &[ColorEntry]) -> Image {
        if color_table.len() < 256 {
            log_warn!("Invalid color table for 8-bit image");
        }

        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (width_usize + 3) & !3;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let row_end = (row_offset + width_usize).min(data.len());
                    let indices = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::apply_palette_row(indices, color_table, dst, width_usize);
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;
                let row_end = (row_offset + width_usize).min(data.len());
                let indices = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                simd::apply_palette_row(indices, color_table, &mut image_data[dst_offset..dst_offset + width_usize * 3], width_usize);
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_16bit_image_masked(
        data: &[u8],
        width: u32,
        height: u32,
        bottom_up: bool,
        red_mask: u32,
        green_mask: u32,
        blue_mask: u32,
        alpha_mask: u32,
    ) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize * 2) + 3) & !3) as usize;
        let has_alpha = alpha_mask != 0;

        let r_shift = if red_mask != 0 { red_mask.trailing_zeros() } else { 0 };
        let r_bits = red_mask.count_ones();
        let g_shift = if green_mask != 0 { green_mask.trailing_zeros() } else { 0 };
        let g_bits = green_mask.count_ones();
        let b_shift = if blue_mask != 0 { blue_mask.trailing_zeros() } else { 0 };
        let b_bits = blue_mask.count_ones();
        let a_shift = if alpha_mask != 0 { alpha_mask.trailing_zeros() } else { 0 };
        let a_bits = alpha_mask.count_ones();

        let channels = if has_alpha { 4 } else { 3 };
        let mut image_data = vec![0u8; width_usize * height_usize * channels];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * channels)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let row_end = (row_offset + width_usize * 2).min(data.len());
                    let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::expand_rgb16_masked_row(src, dst, width_usize, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits, a_shift, a_bits, has_alpha);
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * channels;
                let row_end = (row_offset + width_usize * 2).min(data.len());
                let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                simd::expand_rgb16_masked_row(
                    src,
                    &mut image_data[dst_offset..dst_offset + width_usize * channels],
                    width_usize,
                    r_shift, r_bits, g_shift, g_bits, b_shift, b_bits, a_shift, a_bits, has_alpha,
                );
            }
        }

        if has_alpha {
            Image::from_pixels(width, height, PixelData::RGBA8(image_data))
        } else {
            Image::from_pixels(width, height, PixelData::RGB8(image_data))
        }
    }

    pub fn decode_16bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize * 2) + 3) & !3) as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let row_end = (row_offset + width_usize * 2).min(data.len());
                    let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::expand_rgb555_row(src, dst, width_usize);
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;
                let row_end = (row_offset + width_usize * 2).min(data.len());
                let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                simd::expand_rgb555_row(src, &mut image_data[dst_offset..dst_offset + width_usize * 3], width_usize);
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_24bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = (((width_usize * 3) + 3) & !3) as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 3)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let row_end = (row_offset + width_usize * 3).min(data.len());
                    let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::bgr_to_rgb_row(src, dst, width_usize);
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 3;
                let row_end = (row_offset + width_usize * 3).min(data.len());
                let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                simd::bgr_to_rgb_row(src, &mut image_data[dst_offset..dst_offset + width_usize * 3], width_usize);
            }
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }

    pub fn decode_32bit_image(
        data: &[u8],
        width: u32,
        height: u32,
        bottom_up: bool,
        red_mask: u32,
        green_mask: u32,
        blue_mask: u32,
        alpha_mask: u32,
    ) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = width_usize * 4;
        let has_alpha = alpha_mask != 0;

        let r_shift = if red_mask != 0 { red_mask.trailing_zeros() } else { 0 };
        let r_bits = red_mask.count_ones();
        let g_shift = if green_mask != 0 { green_mask.trailing_zeros() } else { 0 };
        let g_bits = green_mask.count_ones();
        let b_shift = if blue_mask != 0 { blue_mask.trailing_zeros() } else { 0 };
        let b_bits = blue_mask.count_ones();
        let a_shift = if alpha_mask != 0 { alpha_mask.trailing_zeros() } else { 0 };
        let a_bits = alpha_mask.count_ones();

        if has_alpha {
            let mut image_data = vec![0u8; width_usize * height_usize * 4];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                image_data
                    .par_chunks_mut(width_usize * 4)
                    .enumerate()
                    .for_each(|(dst_row, dst)| {
                        let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                        let row_offset = src_row * src_stride;
                        let row_end = (row_offset + width_usize * 4).min(data.len());
                        let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                        simd::extract_channels_rgba_row(src, dst, width_usize, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits, a_shift, a_bits);
                    });
            }

            #[cfg(not(feature = "rayon"))]
            {
                for dst_row in 0..height_usize {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let dst_offset = dst_row * width_usize * 4;
                    let row_end = (row_offset + width_usize * 4).min(data.len());
                    let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::extract_channels_rgba_row(src, &mut image_data[dst_offset..dst_offset + width_usize * 4], width_usize, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits, a_shift, a_bits);
                }
            }

            Image::from_pixels(width, height, PixelData::RGBA8(image_data))
        } else {
            let mut image_data = vec![0u8; width_usize * height_usize * 3];

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                image_data
                    .par_chunks_mut(width_usize * 3)
                    .enumerate()
                    .for_each(|(dst_row, dst)| {
                        let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                        let row_offset = src_row * src_stride;
                        let row_end = (row_offset + width_usize * 4).min(data.len());
                        let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                        simd::extract_channels_rgb_row(src, dst, width_usize, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits);
                    });
            }

            #[cfg(not(feature = "rayon"))]
            {
                for dst_row in 0..height_usize {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;
                    let dst_offset = dst_row * width_usize * 3;
                    let row_end = (row_offset + width_usize * 4).min(data.len());
                    let src = if row_offset < data.len() { &data[row_offset..row_end] } else { &[] };
                    simd::extract_channels_rgb_row(src, &mut image_data[dst_offset..dst_offset + width_usize * 3], width_usize, r_shift, r_bits, g_shift, g_bits, b_shift, b_bits);
                }
            }

            Image::from_pixels(width, height, PixelData::RGB8(image_data))
        }
    }

    pub fn decode_64bit_image(data: &[u8], width: u32, height: u32, bottom_up: bool) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let src_stride = width_usize * 8;

        let mut image_data = vec![0u8; width_usize * height_usize * 4];

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            image_data
                .par_chunks_mut(width_usize * 4)
                .enumerate()
                .for_each(|(dst_row, dst)| {
                    let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                    let row_offset = src_row * src_stride;

                    for x in 0..width_usize {
                        let byte_offset = row_offset + x * 8;
                        if byte_offset + 7 < data.len() {
                            dst[x * 4] = data[byte_offset + 5];
                            dst[x * 4 + 1] = data[byte_offset + 3];
                            dst[x * 4 + 2] = data[byte_offset + 1];
                            dst[x * 4 + 3] = data[byte_offset + 7];
                        }
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for dst_row in 0..height_usize {
                let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
                let row_offset = src_row * src_stride;
                let dst_offset = dst_row * width_usize * 4;

                for x in 0..width_usize {
                    let byte_offset = row_offset + x * 8;
                    if byte_offset + 7 < data.len() {
                        image_data[dst_offset + x * 4] = data[byte_offset + 5];
                        image_data[dst_offset + x * 4 + 1] = data[byte_offset + 3];
                        image_data[dst_offset + x * 4 + 2] = data[byte_offset + 1];
                        image_data[dst_offset + x * 4 + 3] = data[byte_offset + 7];
                    }
                }
            }
        }

        Image::from_pixels(width, height, PixelData::RGBA8(image_data))
    }
}
