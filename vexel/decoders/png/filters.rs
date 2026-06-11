use crate::log_warn;
use crate::utils::error::VexelResult;
use super::types::{FilterType, ColorType};

pub struct FilterDecoder {
    bit_depth: u8,
    color_type: ColorType,
}

impl FilterDecoder {
    pub fn new(bit_depth: u8, color_type: ColorType) -> Self {
        Self { bit_depth, color_type }
    }

    pub fn get_bits_per_pixel(&self) -> u32 {
        match self.color_type {
            ColorType::Grayscale => self.bit_depth as u32,
            ColorType::RGB => self.bit_depth as u32 * 3,
            ColorType::Indexed => self.bit_depth as u32,
            ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
            ColorType::RGBA => self.bit_depth as u32 * 4,
        }
    }

    fn decode_sub_filter(&self, src: &[u8], dst: &mut [u8], bytes_per_pixel: usize) {
        let len = src.len();
        if dst.len() < len || bytes_per_pixel > len {
            log_warn!("Invalid range for sub filter: {}", bytes_per_pixel);
            return;
        }
        dst[..bytes_per_pixel].copy_from_slice(&src[..bytes_per_pixel]);
        for i in bytes_per_pixel..len {
            dst[i] = src[i].wrapping_add(dst[i - bytes_per_pixel]);
        }
    }

    fn decode_up_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8]) {
        let len = src.len();
        if dst.len() < len || prior.len() < len {
            log_warn!("Invalid range for up filter");
            return;
        }
        for i in 0..len {
            dst[i] = src[i].wrapping_add(prior[i]);
        }
    }

    fn decode_average_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
        let len = src.len();
        if dst.len() < len || prior.len() < len || bytes_per_pixel > len {
            log_warn!("Invalid range for average filter: {}", bytes_per_pixel);
            return;
        }
        for i in 0..bytes_per_pixel {
            dst[i] = src[i].wrapping_add(prior[i] >> 1);
        }
        for i in bytes_per_pixel..len {
            let left = dst[i - bytes_per_pixel] as u16;
            let above = prior[i] as u16;
            dst[i] = src[i].wrapping_add(((left + above) >> 1) as u8);
        }
    }

    fn decode_paeth_filter(&self, src: &[u8], dst: &mut [u8], prior: &[u8], bytes_per_pixel: usize) {
        let len = src.len();
        if dst.len() < len || prior.len() < len || bytes_per_pixel > len {
            log_warn!("Invalid range for paeth filter: {}", bytes_per_pixel);
            return;
        }
        for i in 0..bytes_per_pixel {
            dst[i] = src[i].wrapping_add(prior[i]);
        }
        for i in bytes_per_pixel..len {
            let left = dst[i - bytes_per_pixel];
            let above = prior[i];
            let upper_left = prior[i - bytes_per_pixel];
            dst[i] = src[i].wrapping_add(paeth_predictor(left, above, upper_left));
        }
    }

    pub fn unfilter_scanlines(&self, data: &[u8], pass_width: u32) -> VexelResult<Vec<u8>> {
        let bits_per_pixel = self.get_bits_per_pixel();

        let bytes_per_pixel = (bits_per_pixel as usize + 7) / 8;
        let bytes_per_row = (pass_width as usize * bits_per_pixel as usize + 7) / 8;

        let scanline_bytes = 1 + bytes_per_row;
        let mut unfiltered = Vec::new();
        let mut prior_scanline = vec![0u8; scanline_bytes - 1];

        for (_, scanline) in data.chunks(scanline_bytes).enumerate() {
            if scanline.len() < scanline_bytes {
                log_warn!(
                    "Invalid scanline length: {}, expected: {}",
                    scanline.len(),
                    scanline_bytes
                );
                break;
            }

            let filter_type = match scanline[0] {
                0 => FilterType::None,
                1 => FilterType::Sub,
                2 => FilterType::Up,
                3 => FilterType::Average,
                4 => FilterType::Paeth,
                _ => {
                    log_warn!("Invalid filter type: {}", scanline[0]);
                    FilterType::None
                }
            };

            if scanline.len() < 1 {
                log_warn!("Invalid scanline length: {}", scanline.len());
                continue;
            }

            let filtered = &scanline[1..];
            let mut decoded = vec![0u8; filtered.len()];

            match filter_type {
                FilterType::None => {
                    if decoded.len() != filtered.len() {
                        log_warn!(
                            "Length mismatch for unfiltered scanline: {} != {}",
                            decoded.len(),
                            filtered.len()
                        );
                        continue;
                    }

                    decoded.copy_from_slice(filtered);
                }
                FilterType::Sub => {
                    self.decode_sub_filter(filtered, &mut decoded, bytes_per_pixel);
                }
                FilterType::Up => {
                    self.decode_up_filter(filtered, &mut decoded, &prior_scanline);
                }
                FilterType::Average => {
                    self.decode_average_filter(filtered, &mut decoded, &prior_scanline, bytes_per_pixel);
                }
                FilterType::Paeth => {
                    self.decode_paeth_filter(filtered, &mut decoded, &prior_scanline, bytes_per_pixel);
                }
            }

            prior_scanline.copy_from_slice(&decoded);
            unfiltered.extend_from_slice(&decoded);
        }

        Ok(unfiltered)
    }
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i16;
    let b = b as i16;
    let c = c as i16;

    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();

    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}
