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

                    for x in 0..width_usize {
                        let byte_index = row_offset + x;
                        let pixel_value = if byte_index < data.len() { data[byte_index] as usize } else { 0 };
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
                    let byte_index = row_offset + x;
                    let pixel_value = if byte_index < data.len() { data[byte_index] as usize } else { 0 };
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

                    for x in 0..width_usize {
                        let byte_offset = row_offset + x * 2;
                        let pixel = if byte_offset + 1 < data.len() {
                            u16::from_le_bytes([data[byte_offset], data[byte_offset + 1]])
                        } else {
                            0
                        };

                        let r = ((pixel >> 10) & 0x1F) as u8;
                        let g = ((pixel >> 5) & 0x1F) as u8;
                        let b = (pixel & 0x1F) as u8;

                        dst[x * 3] = (r << 3) | (r >> 2);
                        dst[x * 3 + 1] = (g << 3) | (g >> 2);
                        dst[x * 3 + 2] = (b << 3) | (b >> 2);
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
                    let byte_offset = row_offset + x * 2;
                    let pixel = if byte_offset + 1 < data.len() {
                        u16::from_le_bytes([data[byte_offset], data[byte_offset + 1]])
                    } else {
                        0
                    };

                    let r = ((pixel >> 10) & 0x1F) as u8;
                    let g = ((pixel >> 5) & 0x1F) as u8;
                    let b = (pixel & 0x1F) as u8;

                    image_data[dst_offset + x * 3] = (r << 3) | (r >> 2);
                    image_data[dst_offset + x * 3 + 1] = (g << 3) | (g >> 2);
                    image_data[dst_offset + x * 3 + 2] = (b << 3) | (b >> 2);
                }
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

                    for x in 0..width_usize {
                        let byte_offset = row_offset + x * 3;
                        if byte_offset + 2 < data.len() {
                            dst[x * 3] = data[byte_offset + 2];
                            dst[x * 3 + 1] = data[byte_offset + 1];
                            dst[x * 3 + 2] = data[byte_offset];
                        }
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
                    let byte_offset = row_offset + x * 3;
                    if byte_offset + 2 < data.len() {
                        image_data[dst_offset + x * 3] = data[byte_offset + 2];
                        image_data[dst_offset + x * 3 + 1] = data[byte_offset + 1];
                        image_data[dst_offset + x * 3 + 2] = data[byte_offset];
                    }
                }
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

        fn extract_channel(pixel: u32, mask: u32) -> u8 {
            if mask == 0 {
                return 0;
            }
            let shift = mask.trailing_zeros();
            let bits = mask.count_ones();
            let raw = (pixel & mask) >> shift;
            if bits >= 8 {
                (raw >> (bits - 8)) as u8
            } else {
                let scaled = (raw as u32 * 255) / ((1u32 << bits) - 1);
                scaled as u8
            }
        }

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

                        for x in 0..width_usize {
                            let byte_offset = row_offset + x * 4;
                            if byte_offset + 3 < data.len() {
                                let pixel = u32::from_le_bytes([
                                    data[byte_offset],
                                    data[byte_offset + 1],
                                    data[byte_offset + 2],
                                    data[byte_offset + 3],
                                ]);
                                dst[x * 4] = extract_channel(pixel, red_mask);
                                dst[x * 4 + 1] = extract_channel(pixel, green_mask);
                                dst[x * 4 + 2] = extract_channel(pixel, blue_mask);
                                dst[x * 4 + 3] = extract_channel(pixel, alpha_mask);
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
                        let byte_offset = row_offset + x * 4;
                        if byte_offset + 3 < data.len() {
                            let pixel = u32::from_le_bytes([
                                data[byte_offset],
                                data[byte_offset + 1],
                                data[byte_offset + 2],
                                data[byte_offset + 3],
                            ]);
                            image_data[dst_offset + x * 4] = extract_channel(pixel, red_mask);
                            image_data[dst_offset + x * 4 + 1] = extract_channel(pixel, green_mask);
                            image_data[dst_offset + x * 4 + 2] = extract_channel(pixel, blue_mask);
                            image_data[dst_offset + x * 4 + 3] = extract_channel(pixel, alpha_mask);
                        }
                    }
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

                        for x in 0..width_usize {
                            let byte_offset = row_offset + x * 4;
                            if byte_offset + 3 < data.len() {
                                let pixel = u32::from_le_bytes([
                                    data[byte_offset],
                                    data[byte_offset + 1],
                                    data[byte_offset + 2],
                                    data[byte_offset + 3],
                                ]);
                                dst[x * 3] = extract_channel(pixel, red_mask);
                                dst[x * 3 + 1] = extract_channel(pixel, green_mask);
                                dst[x * 3 + 2] = extract_channel(pixel, blue_mask);
                            }
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
                        let byte_offset = row_offset + x * 4;
                        if byte_offset + 3 < data.len() {
                            let pixel = u32::from_le_bytes([
                                data[byte_offset],
                                data[byte_offset + 1],
                                data[byte_offset + 2],
                                data[byte_offset + 3],
                            ]);
                            image_data[dst_offset + x * 3] = extract_channel(pixel, red_mask);
                            image_data[dst_offset + x * 3 + 1] = extract_channel(pixel, green_mask);
                            image_data[dst_offset + x * 3 + 2] = extract_channel(pixel, blue_mask);
                        }
                    }
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
