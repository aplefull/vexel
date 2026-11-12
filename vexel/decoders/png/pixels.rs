use super::filters::FilterDecoder;
use super::types::{ColorType, CompressionMethod, TransparencyData};
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::VexelResult;
use crate::{log_warn, PixelData};

const ADAM7_COL_START: [usize; 7] = [0, 4, 0, 2, 0, 1, 0];
const ADAM7_ROW_START: [usize; 7] = [0, 0, 4, 0, 2, 0, 1];
const ADAM7_COL_DELTA: [usize; 7] = [8, 8, 4, 4, 2, 2, 1];
const ADAM7_ROW_DELTA: [usize; 7] = [8, 8, 8, 4, 4, 2, 2];

pub fn filtered_data_size(bits_per_pixel: u64, width: u32, height: u32, interlace: bool) -> usize {
    let scanlines_size = |width: u64, height: u64| {
        if width == 0 || height == 0 {
            return 0;
        }
        (bits_per_pixel.saturating_mul(width).div_ceil(8) + 1).saturating_mul(height)
    };

    let total = if interlace {
        (0..7).fold(0u64, |total, pass| {
            let pass_width = (u64::from(width) + (ADAM7_COL_DELTA[pass] - 1 - ADAM7_COL_START[pass]) as u64)
                / ADAM7_COL_DELTA[pass] as u64;
            let pass_height = (u64::from(height) + (ADAM7_ROW_DELTA[pass] - 1 - ADAM7_ROW_START[pass]) as u64)
                / ADAM7_ROW_DELTA[pass] as u64;
            total.saturating_add(scanlines_size(pass_width, pass_height))
        })
    } else {
        scanlines_size(u64::from(width), u64::from(height))
    };

    usize::try_from(total).unwrap_or(usize::MAX)
}

#[derive(Clone)]
pub struct PngFormat {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: ColorType,
    pub compression_method: CompressionMethod,
    pub interlace: bool,
    pub palette: Option<Vec<[u8; 3]>>,
    pub transparency: Option<TransparencyData>,
}

impl PngFormat {
    pub fn bits_per_pixel(&self) -> u64 {
        let channels = match self.color_type {
            ColorType::Grayscale | ColorType::Indexed => 1,
            ColorType::GrayscaleAlpha => 2,
            ColorType::RGB => 3,
            ColorType::RGBA => 4,
        };
        u64::from(self.bit_depth) * channels
    }

    fn pixel_decoder(&self, width: u32) -> PixelDecoder {
        PixelDecoder::new(
            self.bit_depth,
            self.color_type,
            width,
            self.interlace,
            self.palette.clone(),
            self.transparency.clone(),
        )
    }

    pub fn decode_image(&self, compressed: Vec<u8>) -> VexelResult<PixelData> {
        let (width, height) = (self.width, self.height);
        let bits_per_pixel = self.bits_per_pixel();

        let mut data = if self.compression_method == CompressionMethod::Deflate {
            let capacity = filtered_data_size(bits_per_pixel, width, height, self.interlace);
            let decompressed = ZlibDecoder::from_slice(&compressed).decode_expected(capacity);
            drop(compressed);
            decompressed
        } else {
            compressed
        };

        let bytes_per_row = (bits_per_pixel * u64::from(width)).div_ceil(8) + 1;
        if !self.interlace && !data.is_empty() {
            let expected = usize::try_from(bytes_per_row.saturating_mul(u64::from(height))).unwrap_or(usize::MAX);
            if data.len() < expected {
                log_warn!(
                    "Decompressed data covers only {} of {} rows, filling the rest with zeros",
                    data.len() as u64 / bytes_per_row,
                    height
                );
                data.resize(expected, 0);
            }
        }

        if data.is_empty() {
            log_warn!("No decompressed pixel data for {}x{} image, filling with zeros", width, height);
            let length = (width as usize).saturating_mul(height as usize).saturating_mul(4);
            return Ok(PixelData::RGBA8(vec![0u8; length]));
        }

        let decoder = self.pixel_decoder(width);
        let scanlines = decoder.deinterlace_scan_lines_owned(data, width, height)?;
        let mut pixels = decoder.decode_pixels_by_type(scanlines)?;
        pixels.correct_pixels(width, height);

        Ok(pixels)
    }

    pub fn decode_frame(&self, compressed: &[u8], width: u32, height: u32) -> Option<PixelData> {
        let decoder = self.pixel_decoder(width);
        let capacity = filtered_data_size(self.bits_per_pixel(), width, height, self.interlace);
        let decompressed = ZlibDecoder::from_slice(compressed).decode_expected(capacity);
        let scanlines = decoder.deinterlace_scan_lines_owned(decompressed, width, height).ok()?;
        let mut pixels = decoder.decode_pixels_by_type(scanlines).ok()?;
        pixels.correct_pixels(width, height);
        Some(pixels)
    }
}

pub struct PixelDecoder {
    bit_depth: u8,
    pub(crate) color_type: ColorType,
    width: u32,
    interlace: bool,
    palette: Option<Vec<[u8; 3]>>,
    transparency: Option<TransparencyData>,
}

impl PixelDecoder {
    pub fn new(
        bit_depth: u8,
        color_type: ColorType,
        width: u32,
        interlace: bool,
        palette: Option<Vec<[u8; 3]>>,
        transparency: Option<TransparencyData>,
    ) -> Self {
        Self { bit_depth, color_type, width, interlace, palette, transparency }
    }

    fn get_bits_per_pixel(&self) -> u32 {
        match self.color_type {
            ColorType::Grayscale => self.bit_depth as u32,
            ColorType::RGB => self.bit_depth as u32 * 3,
            ColorType::Indexed => self.bit_depth as u32,
            ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
            ColorType::RGBA => self.bit_depth as u32 * 4,
        }
    }

    pub fn decode_pixels_by_type(&self, scanlines: Vec<u8>) -> VexelResult<PixelData> {
        match self.color_type {
            ColorType::Indexed => self.decode_indexed(scanlines),
            ColorType::RGB => self.decode_rgb(scanlines),
            ColorType::RGBA => self.decode_rgba(scanlines),
            ColorType::Grayscale => self.decode_grayscale(scanlines),
            ColorType::GrayscaleAlpha => self.decode_grayscale_alpha(scanlines),
        }
    }

    pub fn decode_indexed(&self, input: Vec<u8>) -> VexelResult<PixelData> {
        let palette = match &self.palette {
            Some(palette) => palette,
            None => {
                log_warn!("No palette found for indexed color");
                return Ok(PixelData::RGB8(Vec::new()));
            }
        };

        let trans = if let Some(TransparencyData::Palette(alpha)) = &self.transparency { Some(alpha) } else { None };

        match self.bit_depth {
            8 => Ok(match trans {
                Some(alpha) => PixelData::RGBA8(expand_bytes::<4>(&input, &palette_table(palette, Some(alpha)))),
                None => PixelData::RGB8(expand_bytes::<3>(&input, &palette_table(palette, None))),
            }),
            16 => {
                log_warn!("Invalid bit depth for indexed color: 16, reading 16-bit indices");
                let mut output = Vec::new();
                for pair in input.chunks_exact(2) {
                    let index = usize::from(u16::from_be_bytes([pair[0], pair[1]]));
                    let color = palette.get(index).unwrap_or(&[0, 0, 0]);
                    match trans {
                        Some(alpha) => {
                            output.extend_from_slice(&[color[0], color[1], color[2], *alpha.get(index).unwrap_or(&255)])
                        }
                        None => output.extend_from_slice(&[color[0], color[1], color[2]]),
                    }
                }
                Ok(if trans.is_some() { PixelData::RGBA8(output) } else { PixelData::RGB8(output) })
            }
            bits @ (1 | 2 | 4) => {
                let mut pixels = [[0u8; 4]; 16];
                for (index, pixel) in pixels.iter_mut().enumerate() {
                    let color = palette.get(index).copied().unwrap_or([0, 0, 0]);
                    let alpha = trans.and_then(|alpha| alpha.get(index)).copied().unwrap_or(255);
                    *pixel = [color[0], color[1], color[2], alpha];
                }
                let width = self.width as usize;
                Ok(match trans {
                    Some(_) => PixelData::RGBA8(expand_low_bit(&input, width, bits as usize, 4, &pixels)),
                    None => PixelData::RGB8(expand_low_bit(&input, width, bits as usize, 3, &pixels)),
                })
            }
            other => {
                log_warn!("Invalid bit depth for indexed color: {}", other);
                Ok(if trans.is_some() { PixelData::RGBA8(Vec::new()) } else { PixelData::RGB8(Vec::new()) })
            }
        }
    }

    pub fn decode_grayscale(&self, input: Vec<u8>) -> VexelResult<PixelData> {
        let trans_key = match &self.transparency {
            Some(TransparencyData::Grayscale(key)) => Some(*key),
            _ => None,
        };

        match self.bit_depth {
            8 => match trans_key {
                Some(key) => {
                    let key8 = key as u8;
                    let mut table = [[0u8; 4]; 256];
                    for (value, entry) in table.iter_mut().enumerate() {
                        *entry = [value as u8, if value as u8 == key8 { 0 } else { 255 }, 0, 0];
                    }
                    Ok(PixelData::LA8(expand_bytes::<2>(&input, &table)))
                }
                None => Ok(PixelData::L8(input)),
            },
            16 => match trans_key {
                Some(key) => {
                    let samples = be_u16_samples(&input, 2);
                    let mut output = vec![0u16; samples.len() * 2];
                    for (dst, &value) in output.chunks_exact_mut(2).zip(&samples) {
                        dst[0] = value;
                        dst[1] = if value == key { 0 } else { 65535 };
                    }
                    Ok(PixelData::LA16(output))
                }
                None => Ok(PixelData::L16(be_u16_samples(&input, 2))),
            },
            bits @ (1 | 2 | 4) => {
                let max_value = (1u16 << bits) - 1;
                let mut pixels = [[0u8; 4]; 16];
                for (value, pixel) in pixels.iter_mut().enumerate().take(usize::from(max_value) + 1) {
                    let scaled = (value as u16 * 255 / max_value) as u8;
                    let alpha = match trans_key {
                        Some(key) if value as u8 == key as u8 => 0,
                        _ => 255,
                    };
                    *pixel = [scaled, alpha, 0, 0];
                }
                let width = self.width as usize;
                Ok(match trans_key {
                    Some(_) => PixelData::LA8(expand_low_bit(&input, width, bits as usize, 2, &pixels)),
                    None => PixelData::L8(expand_low_bit(&input, width, bits as usize, 1, &pixels)),
                })
            }
            other => {
                log_warn!("Invalid bit depth for grayscale: {}, assuming 8 bits", other);
                Ok(PixelData::L8(input))
            }
        }
    }

    pub fn decode_grayscale_alpha(&self, input: Vec<u8>) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => Ok(PixelData::LA8(input)),
            16 => Ok(PixelData::LA16(be_u16_samples(&input, 4))),
            _ => {
                log_warn!("Invalid bit depth for grayscale alpha: {}, assuming 8 bits", self.bit_depth);

                Ok(PixelData::LA8(input))
            }
        }
    }

    pub fn decode_rgb(&self, input: Vec<u8>) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => match &self.transparency {
                Some(TransparencyData::RGB(key_r, key_g, key_b)) => {
                    let key = [*key_r as u8, *key_g as u8, *key_b as u8];
                    let mut output = vec![0u8; input.len() / 3 * 4];
                    for (dst, rgb) in output.chunks_exact_mut(4).zip(input.chunks_exact(3)) {
                        let alpha = if rgb == key { 0 } else { 255 };
                        dst.copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
                    }
                    Ok(PixelData::RGBA8(output))
                }
                _ => Ok(PixelData::RGB8(input)),
            },
            16 => match &self.transparency {
                Some(TransparencyData::RGB(key_r, key_g, key_b)) => {
                    let key = [*key_r, *key_g, *key_b];
                    let samples = be_u16_samples(&input, 6);
                    let mut output = vec![0u16; samples.len() / 3 * 4];
                    for (dst, rgb) in output.chunks_exact_mut(4).zip(samples.chunks_exact(3)) {
                        let alpha = if rgb == key { 0 } else { 65535 };
                        dst.copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
                    }
                    Ok(PixelData::RGBA16(output))
                }
                _ => Ok(PixelData::RGB16(be_u16_samples(&input, 6))),
            },
            _ => {
                log_warn!("Invalid bit depth for RGB color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGB8(input))
            }
        }
    }

    pub fn decode_rgba(&self, input: Vec<u8>) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => Ok(PixelData::RGBA8(input)),
            16 => Ok(PixelData::RGBA16(be_u16_samples(&input, 8))),
            _ => {
                log_warn!("Invalid bit depth for RGBA color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGBA8(input))
            }
        }
    }

    pub fn deinterlace_scan_lines_owned(&self, mut data: Vec<u8>, width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let filter_decoder = FilterDecoder::new(self.bit_depth, self.color_type);

        if !self.interlace {
            return filter_decoder.unfilter_scanlines_owned(data, width);
        }

        let bits_per_pixel = self.get_bits_per_pixel();

        let out_bytes = (bits_per_pixel as usize * width as usize).div_ceil(8);
        let mut output = vec![0u8; out_bytes * height as usize];
        let mut data_offset = 0;

        for pass in 0..7 {
            let pass_width =
                (width as usize + ADAM7_COL_DELTA[pass] - 1 - ADAM7_COL_START[pass]) / ADAM7_COL_DELTA[pass];
            let pass_height =
                (height as usize + ADAM7_ROW_DELTA[pass] - 1 - ADAM7_ROW_START[pass]) / ADAM7_ROW_DELTA[pass];

            if pass_width == 0 || pass_height == 0 {
                continue;
            }

            let pass_bits_per_row = bits_per_pixel as usize * pass_width;
            let pass_bytes_per_row = pass_bits_per_row.div_ceil(8);
            let pass_size = (pass_bytes_per_row + 1) * pass_height;

            if data_offset + pass_size > data.len() {
                log_warn!("Insufficient data for interlaced image");
                break;
            }

            let pass_data = &mut data[data_offset..data_offset + pass_size];
            let pass_rows = filter_decoder.unfilter_strided(pass_data, pass_width as u32);
            let stride = pass_bytes_per_row + 1;

            for row in 0..pass_height.min(pass_rows) {
                let unfiltered = &pass_data[row * stride + 1..(row + 1) * stride];
                let out_y = row * ADAM7_ROW_DELTA[pass] + ADAM7_ROW_START[pass];
                if out_y >= height as usize {
                    break;
                }

                if bits_per_pixel < 8 {
                    let pixels_per_byte = 8 / bits_per_pixel as usize;
                    let bit_mask = (1 << bits_per_pixel) - 1;

                    for col in 0..pass_width {
                        let out_x = col * ADAM7_COL_DELTA[pass] + ADAM7_COL_START[pass];
                        if out_x >= width as usize {
                            break;
                        }

                        let in_byte_idx = col / pixels_per_byte;
                        let in_bit_shift = (pixels_per_byte - 1 - (col % pixels_per_byte)) * bits_per_pixel as usize;

                        if in_byte_idx >= unfiltered.len() {
                            log_warn!("Invalid byte index: {} > {}", in_byte_idx, unfiltered.len());
                            continue;
                        }

                        let in_pixel = (unfiltered[in_byte_idx] >> in_bit_shift) & bit_mask;

                        let out_byte_idx = (out_y * out_bytes) + (out_x / pixels_per_byte);
                        let out_bit_shift = (pixels_per_byte - 1 - (out_x % pixels_per_byte)) * bits_per_pixel as usize;

                        if out_byte_idx < output.len() {
                            output[out_byte_idx] &= !(bit_mask << out_bit_shift);
                            output[out_byte_idx] |= in_pixel << out_bit_shift;
                        }
                    }
                } else {
                    let bytes_per_pixel = (bits_per_pixel as usize) / 8;
                    let src = &unfiltered[..(pass_width * bytes_per_pixel).min(unfiltered.len())];
                    let dst = &mut output[out_y * out_bytes..(out_y + 1) * out_bytes];
                    let (start, step) = (ADAM7_COL_START[pass], ADAM7_COL_DELTA[pass]);
                    match bytes_per_pixel {
                        1 => scatter_row::<1>(src, dst, start, step),
                        2 => scatter_row::<2>(src, dst, start, step),
                        3 => scatter_row::<3>(src, dst, start, step),
                        4 => scatter_row::<4>(src, dst, start, step),
                        6 => scatter_row::<6>(src, dst, start, step),
                        8 => scatter_row::<8>(src, dst, start, step),
                        _ => {
                            for (out, pixel) in dst
                                .chunks_exact_mut(bytes_per_pixel)
                                .skip(start)
                                .step_by(step)
                                .zip(src.chunks_exact(bytes_per_pixel))
                            {
                                out.copy_from_slice(pixel);
                            }
                        }
                    }
                }
            }

            data_offset += pass_size;
        }

        Ok(output)
    }
}

fn be_u16_samples(input: &[u8], group_bytes: usize) -> Vec<u16> {
    let usable = input.len() / group_bytes * group_bytes;
    input[..usable].chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect()
}

fn palette_table(palette: &[[u8; 3]], alpha: Option<&Vec<u8>>) -> [[u8; 4]; 256] {
    let mut table = [[0u8; 4]; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let [r, g, b] = palette.get(index).copied().unwrap_or([0, 0, 0]);
        let a = alpha.and_then(|alpha| alpha.get(index)).copied().unwrap_or(255);
        *entry = [r, g, b, a];
    }
    table
}

fn expand_bytes<const C: usize>(input: &[u8], table: &[[u8; 4]; 256]) -> Vec<u8> {
    let len = input.len() * C;
    let mut output = Vec::with_capacity(len + 4);
    let dst = output.spare_capacity_mut().as_mut_ptr() as *mut u8;
    for (i, &index) in input.iter().enumerate() {
        unsafe { (dst.add(i * C) as *mut [u8; 4]).write_unaligned(table[index as usize]) };
    }
    unsafe { output.set_len(len) };
    output
}

fn expand_low_bit(input: &[u8], width: usize, bits: usize, pixel_bytes: usize, pixels: &[[u8; 4]; 16]) -> Vec<u8> {
    match (8 / bits) * pixel_bytes {
        2 => expand_packed::<2, 2>(input, width, bits, pixel_bytes, pixels),
        4 => expand_packed::<4, 4>(input, width, bits, pixel_bytes, pixels),
        6 => expand_packed::<6, 8>(input, width, bits, pixel_bytes, pixels),
        8 => expand_packed::<8, 8>(input, width, bits, pixel_bytes, pixels),
        12 => expand_packed::<12, 16>(input, width, bits, pixel_bytes, pixels),
        16 => expand_packed::<16, 16>(input, width, bits, pixel_bytes, pixels),
        24 => expand_packed::<24, 32>(input, width, bits, pixel_bytes, pixels),
        32 => expand_packed::<32, 32>(input, width, bits, pixel_bytes, pixels),
        _ => Vec::new(),
    }
}

fn expand_packed<const N: usize, const STORE: usize>(
    input: &[u8],
    width: usize,
    bits: usize,
    pixel_bytes: usize,
    pixels: &[[u8; 4]; 16],
) -> Vec<u8> {
    let per_byte = 8 / bits;
    let mask = (1usize << bits) - 1;
    let mut byte_table = [[0u8; STORE]; 256];
    for (byte, entry) in byte_table.iter_mut().enumerate() {
        for k in 0..per_byte {
            let index = (byte >> (8 - bits * (k + 1))) & mask;
            entry[k * pixel_bytes..(k + 1) * pixel_bytes].copy_from_slice(&pixels[index][..pixel_bytes]);
        }
    }

    let row_bytes = (width * bits).div_ceil(8);
    if row_bytes == 0 {
        return Vec::new();
    }
    let out_row = width * pixel_bytes;
    let rows = input.len() / row_bytes;
    let whole_len = rows * out_row;

    let mut output = Vec::with_capacity(whole_len + STORE);
    let dst = output.spare_capacity_mut().as_mut_ptr() as *mut u8;
    for (row, src) in input.chunks_exact(row_bytes).enumerate() {
        let row_dst = unsafe { dst.add(row * out_row) };
        for (j, &byte) in src.iter().enumerate() {
            unsafe { (row_dst.add(j * N) as *mut [u8; STORE]).write_unaligned(byte_table[byte as usize]) };
        }
    }
    unsafe { output.set_len(whole_len) };

    let partial = &input[rows * row_bytes..];
    let count = (partial.len() * per_byte).min(width);
    for x in 0..count {
        let entry = &byte_table[partial[x / per_byte] as usize];
        output.extend_from_slice(&entry[(x % per_byte) * pixel_bytes..][..pixel_bytes]);
    }
    output
}

fn scatter_row<const BPP: usize>(src: &[u8], dst: &mut [u8], start: usize, step: usize) {
    if step == 1 {
        let offset = (start * BPP).min(dst.len());
        let count = src.len().min(dst.len() - offset) / BPP * BPP;
        dst[offset..offset + count].copy_from_slice(&src[..count]);
        return;
    }
    let pixels = dst.len() / BPP;
    for (col, pixel) in src.chunks_exact(BPP).enumerate() {
        let x = start + col * step;
        if x >= pixels {
            break;
        }
        dst[x * BPP..x * BPP + BPP].copy_from_slice(pixel);
    }
}
