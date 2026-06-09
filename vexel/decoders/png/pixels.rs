use crate::log_warn;
use crate::utils::error::VexelResult;
use crate::utils::traits::SafeAccess;
use crate::PixelData;
use super::types::{ColorType, TransparencyData};
use super::filters::FilterDecoder;

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
        Self {
            bit_depth,
            color_type,
            width,
            interlace,
            palette,
            transparency,
        }
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

    pub fn decode_indexed(&self, input: &[u8]) -> VexelResult<PixelData> {
        let palette = match &self.palette {
            Some(palette) => palette,
            None => {
                log_warn!("No palette found for indexed color");
                return Ok(PixelData::RGB8(Vec::new()));
            }
        };

        let trans = if let Some(TransparencyData::Palette(alpha)) = &self.transparency {
            Some(alpha)
        } else {
            None
        };

        let mut output = Vec::new();
        let has_trans = trans.is_some();

        match self.bit_depth {
            8 => {
                for &index in input {
                    let color = palette.get(index as usize).unwrap_or(&[0, 0, 0]);
                    if has_trans {
                        let alpha = trans.as_ref().unwrap().get(index as usize).unwrap_or(&255);
                        output.extend_from_slice(&[color[0], color[1], color[2], *alpha]);
                    } else {
                        output.extend_from_slice(&[color[0], color[1], color[2]]);
                    }
                }
            }
            1 | 2 | 4 => {
                let bits_per_pixel = self.bit_depth as usize;
                let pixels_per_byte = 8 / bits_per_pixel;
                let mask = (1 << bits_per_pixel) - 1;
                let width = self.width as usize;
                let mut pixel_count = 0;

                for &byte in input {
                    for shift in (0..pixels_per_byte).rev() {
                        if pixel_count >= width {
                            break;
                        }

                        let index = (byte >> (shift * bits_per_pixel)) & mask;
                        let color = palette.get(index as usize).unwrap_or(&[0, 0, 0]);

                        if has_trans {
                            let alpha = trans.as_ref().unwrap().get(index as usize).unwrap_or(&255);
                            output.extend_from_slice(&[color[0], color[1], color[2], *alpha]);
                        } else {
                            output.extend_from_slice(&[color[0], color[1], color[2]]);
                        }

                        pixel_count += 1;
                    }
                    if pixel_count >= width {
                        pixel_count = 0;
                    }
                }
            }
            _ => unreachable!(),
        };

        if has_trans {
            Ok(PixelData::RGBA8(output))
        } else {
            Ok(PixelData::RGB8(output))
        }
    }

    pub fn decode_grayscale(&self, input: &[u8]) -> VexelResult<PixelData> {
        let trans_key = match &self.transparency {
            Some(TransparencyData::Grayscale(key)) => Some(*key),
            _ => None,
        };

        match self.bit_depth {
            8 => {
                if let Some(key) = trans_key {
                    let key8 = key as u8;
                    let mut output = Vec::with_capacity(input.len() * 2);
                    for &v in input {
                        output.push(v);
                        output.push(if v == key8 { 0 } else { 255 });
                    }
                    Ok(PixelData::LA8(output))
                } else {
                    Ok(PixelData::L8(input.to_vec()))
                }
            }
            16 => {
                match trans_key {
                    Some(key) => {
                        let mut output = Vec::with_capacity(input.len());
                        for gray in input.chunks_exact(2) {
                            let v = u16::from_be_bytes([
                                *gray.get_safe(0).unwrap_or_else(|_| &0),
                                *gray.get_safe(1).unwrap_or_else(|_| &0),
                            ]);
                            let alpha: u16 = if v == key { 0 } else { 65535 };
                            output.push(v);
                            output.push(alpha);
                        }
                        Ok(PixelData::LA16(output))
                    }
                    None => {
                        let mut output = Vec::with_capacity(input.len() / 2);
                        for gray in input.chunks_exact(2) {
                            output.push(u16::from_be_bytes([
                                *gray.get_safe(0).unwrap_or_else(|_| &0),
                                *gray.get_safe(1).unwrap_or_else(|_| &0),
                            ]));
                        }
                        Ok(PixelData::L16(output))
                    }
                }
            }
            1 | 2 | 4 => {
                let bits_per_pixel = self.bit_depth as usize;
                let pixels_per_byte = 8 / bits_per_pixel;
                let mask = (1 << bits_per_pixel) - 1;
                let max_value = mask;
                let width = self.width as usize;
                let mut pixel_count = 0;

                if let Some(key) = trans_key {
                    let key_raw = key as u8;
                    let mut output = Vec::new();
                    for &byte in input {
                        for shift in (0..pixels_per_byte).rev() {
                            if pixel_count >= width {
                                break;
                            }
                            let value = (byte >> (shift * bits_per_pixel)) & mask;
                            let scaled = (value as u16 * 255 / max_value as u16) as u8;
                            output.push(scaled);
                            output.push(if value == key_raw { 0 } else { 255 });
                            pixel_count += 1;
                        }
                        if pixel_count >= width {
                            pixel_count = 0;
                        }
                    }
                    Ok(PixelData::LA8(output))
                } else {
                    let mut output = Vec::new();
                    for &byte in input {
                        for shift in (0..pixels_per_byte).rev() {
                            if pixel_count >= width {
                                break;
                            }
                            let value = (byte >> (shift * bits_per_pixel)) & mask;
                            let scaled = (value as u16 * 255 / max_value as u16) as u8;
                            output.push(scaled);
                            pixel_count += 1;
                        }
                        if pixel_count >= width {
                            pixel_count = 0;
                        }
                    }
                    Ok(PixelData::L8(output))
                }
            }
            _ => unreachable!(),
        }
    }

    pub fn decode_grayscale_alpha(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => Ok(PixelData::LA8(input.to_vec())),
            16 => {
                let mut output = Vec::with_capacity((input.len() / 4) * 2);

                for ga in input.chunks_exact(4) {
                    output.push(u16::from_be_bytes([ga[0], ga[1]]));
                    output.push(u16::from_be_bytes([ga[2], ga[3]]));
                }

                Ok(PixelData::LA16(output))
            }
            _ => {
                log_warn!(
                    "Invalid bit depth for grayscale alpha: {}, assuming 8 bits",
                    self.bit_depth
                );

                Ok(PixelData::LA8(input.to_vec()))
            }
        }
    }

    pub fn decode_rgb(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => {
                match &self.transparency {
                    Some(TransparencyData::RGB(key_r, key_g, key_b)) => {
                        let (kr, kg, kb) = (*key_r as u8, *key_g as u8, *key_b as u8);
                        let mut output = Vec::with_capacity(input.len() / 3 * 4);
                        for rgb in input.chunks_exact(3) {
                            let alpha = if rgb[0] == kr && rgb[1] == kg && rgb[2] == kb { 0 } else { 255 };
                            output.extend_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
                        }
                        Ok(PixelData::RGBA8(output))
                    }
                    _ => Ok(PixelData::RGB8(input.to_vec())),
                }
            }
            16 => {
                match &self.transparency {
                    Some(TransparencyData::RGB(key_r, key_g, key_b)) => {
                        let (kr, kg, kb) = (*key_r, *key_g, *key_b);
                        let mut output = Vec::with_capacity((input.len() / 6) * 4);
                        for rgb in input.chunks_exact(6) {
                            let r = u16::from_be_bytes([rgb[0], rgb[1]]);
                            let g = u16::from_be_bytes([rgb[2], rgb[3]]);
                            let b = u16::from_be_bytes([rgb[4], rgb[5]]);
                            let alpha: u16 = if r == kr && g == kg && b == kb { 0 } else { 65535 };
                            output.push(r);
                            output.push(g);
                            output.push(b);
                            output.push(alpha);
                        }
                        Ok(PixelData::RGBA16(output))
                    }
                    _ => {
                        let mut output = Vec::with_capacity((input.len() / 6) * 3);
                        for rgb in input.chunks_exact(6) {
                            output.push(u16::from_be_bytes([rgb[0], rgb[1]]));
                            output.push(u16::from_be_bytes([rgb[2], rgb[3]]));
                            output.push(u16::from_be_bytes([rgb[4], rgb[5]]));
                        }
                        Ok(PixelData::RGB16(output))
                    }
                }
            }
            _ => {
                log_warn!("Invalid bit depth for RGB color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGB8(input.to_vec()))
            }
        }
    }

    pub fn decode_rgba(&self, input: &[u8]) -> VexelResult<PixelData> {
        match self.bit_depth {
            8 => Ok(PixelData::RGBA8(input.to_vec())),
            16 => {
                let mut output = Vec::with_capacity((input.len() / 8) * 4);
                for rgba in input.chunks_exact(8) {
                    output.push(u16::from_be_bytes([rgba[0], rgba[1]]));
                    output.push(u16::from_be_bytes([rgba[2], rgba[3]]));
                    output.push(u16::from_be_bytes([rgba[4], rgba[5]]));
                    output.push(u16::from_be_bytes([rgba[6], rgba[7]]));
                }

                Ok(PixelData::RGBA16(output))
            }
            _ => {
                log_warn!("Invalid bit depth for RGBA color: {}, assuming 8 bits", self.bit_depth);
                Ok(PixelData::RGBA8(input.to_vec()))
            }
        }
    }

    pub fn deinterlace_scan_lines(&self, data: &[u8], width: u32, height: u32) -> VexelResult<Vec<u8>> {
        let filter_decoder = FilterDecoder::new(self.bit_depth, self.color_type);

        if !self.interlace {
            return filter_decoder.unfilter_scanlines(data, width);
        }

        const ADAM7_COL_START: [usize; 7] = [0, 4, 0, 2, 0, 1, 0];
        const ADAM7_ROW_START: [usize; 7] = [0, 0, 4, 0, 2, 0, 1];
        const ADAM7_COL_DELTA: [usize; 7] = [8, 8, 4, 4, 2, 2, 1];
        const ADAM7_ROW_DELTA: [usize; 7] = [8, 8, 8, 4, 4, 2, 2];

        let bits_per_pixel = self.get_bits_per_pixel();

        let out_bytes = (bits_per_pixel as usize * width as usize + 7) / 8;
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
            let pass_bytes_per_row = (pass_bits_per_row + 7) / 8;
            let pass_size = (pass_bytes_per_row + 1) * pass_height;

            if data_offset + pass_size > data.len() {
                log_warn!("Insufficient data for interlaced image");
                break;
            }

            let pass_data = &data[data_offset..data_offset + pass_size];
            let unfiltered = filter_decoder.unfilter_scanlines(pass_data, pass_width as u32)?;
            let mut unfiltered_idx = 0;

            for row in 0..pass_height {
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

                        let in_byte_idx = unfiltered_idx + (col / pixels_per_byte);
                        let in_bit_shift = (pixels_per_byte - 1 - (col % pixels_per_byte)) * bits_per_pixel as usize;

                        if in_byte_idx > unfiltered.len() {
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

                    for col in 0..pass_width {
                        let out_x = col * ADAM7_COL_DELTA[pass] + ADAM7_COL_START[pass];
                        if out_x >= width as usize {
                            break;
                        }

                        let out_pos = (out_y * out_bytes) + (out_x * bytes_per_pixel);
                        let in_pos = unfiltered_idx + (col * bytes_per_pixel);

                        if out_pos + bytes_per_pixel <= output.len() && in_pos + bytes_per_pixel <= unfiltered.len() {
                            output[out_pos..out_pos + bytes_per_pixel]
                                .copy_from_slice(&unfiltered[in_pos..in_pos + bytes_per_pixel]);
                        }
                    }
                }

                unfiltered_idx += pass_bytes_per_row;
            }

            data_offset += pass_size;
        }

        Ok(output)
    }
}
