use crate::decoders::bmp::simd;
use crate::decoders::bmp::types::ColorEntry;
use crate::utils::codecs::ccitt::{self, FaxMode};
use crate::{log_warn, Image, PixelData};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RleKind {
    Rle4,
    Rle8,
    Rle24,
}

impl RleKind {
    fn output_bytes(self) -> usize {
        if self == RleKind::Rle24 {
            3
        } else {
            1
        }
    }

    fn literal_bytes(self, pixels: usize) -> usize {
        match self {
            RleKind::Rle4 => pixels.div_ceil(2),
            RleKind::Rle8 => pixels,
            RleKind::Rle24 => pixels * 3,
        }
    }

    fn literal_pixel(self, literal: &[u8], index: usize) -> [u8; 3] {
        match self {
            RleKind::Rle4 => [nibble(literal[index / 2], index), 0, 0],
            RleKind::Rle8 => [literal[index], 0, 0],
            RleKind::Rle24 => [literal[index * 3], literal[index * 3 + 1], literal[index * 3 + 2]],
        }
    }

    fn run_pixel(self, run: [u8; 3], index: usize) -> [u8; 3] {
        match self {
            RleKind::Rle4 => [nibble(run[0], index), 0, 0],
            RleKind::Rle8 | RleKind::Rle24 => run,
        }
    }
}

fn nibble(byte: u8, index: usize) -> u8 {
    if index.is_multiple_of(2) {
        byte >> 4
    } else {
        byte & 0x0F
    }
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let bytes = input.get(..count)?;
    *input = &input[count..];
    Some(bytes)
}

pub struct RleDecoder;

impl RleDecoder {
    pub fn decode_rle(data: &[u8], width: u32, height: u32, kind: RleKind) -> Vec<u8> {
        let (width, height) = (width as usize, height as usize);
        let pixel_bytes = kind.output_bytes();
        let stride = if kind == RleKind::Rle24 { (width * 3 + 3) & !3 } else { width };
        let mut decoded = vec![0u8; stride * height];
        let mut input = data;
        let (mut x, mut y) = (0usize, 0usize);

        let put = |decoded: &mut [u8], x: usize, y: usize, pixel: [u8; 3]| {
            if x < width && y < height {
                let pos = y * stride + x * pixel_bytes;
                decoded[pos..pos + pixel_bytes].copy_from_slice(&pixel[..pixel_bytes]);
            }
        };

        while y < height {
            let Some(&[count, value]) = take(&mut input, 2) else {
                break;
            };

            if count == 0 {
                match value {
                    0 => {
                        x = 0;
                        y += 1;
                    }
                    1 => break,
                    2 => {
                        let Some(&[dx, dy]) = take(&mut input, 2) else {
                            break;
                        };
                        x = x.saturating_add(dx as usize);
                        y += dy as usize;
                    }
                    n => {
                        let pixels = n as usize;
                        let bytes = kind.literal_bytes(pixels);
                        let literal = &input[..bytes.min(input.len())];
                        let complete = literal.len() == bytes;
                        let available = if complete { pixels } else { literal.len() * pixels / bytes };
                        input = &input[literal.len()..];

                        for index in 0..available {
                            put(&mut decoded, x + index, y, kind.literal_pixel(literal, index));
                        }
                        x = x.saturating_add(pixels);

                        if !complete {
                            break;
                        }
                        if bytes % 2 == 1 {
                            take(&mut input, 1);
                        }
                    }
                }
                continue;
            }

            let run = match kind {
                RleKind::Rle24 => match take(&mut input, 2) {
                    Some(&[g, r]) => [value, g, r],
                    _ => break,
                },
                _ => [value, 0, 0],
            };

            let count = count as usize;
            if kind == RleKind::Rle8 {
                let row_end = y * width + width;
                let start = (y * width + x).min(row_end);
                decoded[start..row_end.min(start + count)].fill(value);
            } else {
                for index in 0..count {
                    put(&mut decoded, x + index, y, kind.run_pixel(run, index));
                }
            }
            x = x.saturating_add(count);
        }

        decoded
    }

    pub fn decode_huffman1d(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let rows = height as usize;
        let mode = FaxMode::Group3 { two_dimensional: false };
        let mut decoded = ccitt::decode(data, width, rows, mode);

        if decoded.clean_rows < rows {
            let reversed: Vec<u8> = data.iter().map(|byte| byte.reverse_bits()).collect();
            let decoded_reversed = ccitt::decode(&reversed, width, rows, mode);

            if decoded_reversed.clean_rows > decoded.clean_rows {
                log_warn!(
                    "Huffman 1D bitstream appears to be packed LSB-first, decoded {}/{} rows after bit reversal (MSB-first gave {})",
                    decoded_reversed.clean_rows,
                    rows,
                    decoded.clean_rows
                );
                decoded = decoded_reversed;
            }
        }
        let packed = decoded.pixels;

        let src_stride = (width as usize).div_ceil(8);
        let dst_stride = (src_stride + 3) & !3;
        let mut padded = vec![0u8; dst_stride * height as usize];

        for row in 0..height as usize {
            let src_offset = row * src_stride;
            let dst_offset = row * dst_stride;
            if let Some(src) = packed.get(src_offset..src_offset + src_stride) {
                padded[dst_offset..dst_offset + src_stride].copy_from_slice(src);
            }
        }

        padded
    }

    pub fn decode_indexed_image(
        data: &[u8],
        width: u32,
        height: u32,
        bottom_up: bool,
        color_table: &[ColorEntry],
    ) -> Image {
        let width_usize = width as usize;
        let height_usize = height as usize;

        let mut image_data = vec![0u8; width_usize * height_usize * 3];

        for dst_row in 0..height_usize {
            let src_row = if bottom_up { height_usize - 1 - dst_row } else { dst_row };
            let src_offset = src_row * width_usize;
            let dst_offset = dst_row * width_usize * 3;
            let src_end = (src_offset + width_usize).min(data.len());
            let indices = if src_offset < data.len() { &data[src_offset..src_end] } else { &[] };
            simd::apply_palette_row(
                indices,
                color_table,
                &mut image_data[dst_offset..dst_offset + width_usize * 3],
                width_usize,
            );
        }

        Image::from_pixels(width, height, PixelData::RGB8(image_data))
    }
}
