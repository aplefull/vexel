use super::filter_simd;
use super::types::{ColorType, FilterType};
use crate::log_warn;
use crate::utils::error::VexelResult;

const BATCH_ROWS: usize = 16;

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

    pub fn unfilter_scanlines_owned(&self, mut data: Vec<u8>, pass_width: u32) -> VexelResult<Vec<u8>> {
        let len = self.unfilter_in_place(&mut data, pass_width);
        data.truncate(len);
        Ok(data)
    }

    pub fn unfilter_in_place(&self, data: &mut [u8], pass_width: u32) -> usize {
        self.unfilter_rows(data, pass_width, true) * self.bytes_per_row(pass_width)
    }

    pub fn unfilter_strided(&self, data: &mut [u8], pass_width: u32) -> usize {
        self.unfilter_rows(data, pass_width, false)
    }

    fn unfilter_rows(&self, data: &mut [u8], pass_width: u32, compact: bool) -> usize {
        let bytes_per_row = self.bytes_per_row(pass_width);
        if bytes_per_row == 0 {
            return 0;
        }

        let bytes_per_pixel = (self.get_bits_per_pixel() as usize).div_ceil(8).max(1);
        let stride = bytes_per_row + 1;
        let num_rows = data.len() / stride;
        let zero_row = vec![0u8; bytes_per_row];
        let base = data.as_mut_ptr();

        let mut filters = [FilterType::None; BATCH_ROWS];
        let mut rows = [std::ptr::null_mut::<u8>(); BATCH_ROWS];
        let mut first = 0;
        while first < num_rows {
            let count = (num_rows - first).min(BATCH_ROWS);
            for i in 0..count {
                let start = (first + i) * stride;
                filters[i] = parse_filter(unsafe { *base.add(start) });
                rows[i] = unsafe { base.add(start + 1) };
            }

            let above = match (first, compact) {
                (0, _) => zero_row.as_ptr(),
                (_, true) => unsafe { base.add((first - 1) * bytes_per_row) }.cast_const(),
                (_, false) => unsafe { base.add((first - 1) * stride + 1) }.cast_const(),
            };
            unsafe {
                filter_simd::unfilter_rows(&filters[..count], &rows[..count], above, bytes_per_row, bytes_per_pixel)
            };

            if compact {
                for row in first..first + count {
                    unsafe { std::ptr::copy(base.add(row * stride + 1), base.add(row * bytes_per_row), bytes_per_row) };
                }
            }
            first += count;
        }

        num_rows
    }

    fn bytes_per_row(&self, pass_width: u32) -> usize {
        (pass_width as usize * self.get_bits_per_pixel() as usize).div_ceil(8)
    }
}

fn parse_filter(byte: u8) -> FilterType {
    match byte {
        0 => FilterType::None,
        1 => FilterType::Sub,
        2 => FilterType::Up,
        3 => FilterType::Average,
        4 => FilterType::Paeth,
        _ => {
            log_warn!("Invalid filter type: {}", byte);
            FilterType::None
        }
    }
}
