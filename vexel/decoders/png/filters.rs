use super::filter_simd;
use super::types::{ColorType, FilterType};
use crate::log_warn;
use crate::utils::error::VexelResult;

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
        let bytes_per_row = self.bytes_per_row(pass_width);
        if bytes_per_row == 0 {
            return Ok(Vec::new());
        }

        let scanline_bytes = 1 + bytes_per_row;
        let num_rows = data.len() / scanline_bytes;

        for row_idx in 0..num_rows {
            let src_start = row_idx * scanline_bytes;
            let filter_byte = data[src_start];
            let dst_start = row_idx * bytes_per_row;

            data.copy_within(src_start + 1..src_start + scanline_bytes, dst_start);
            self.unfilter_row_in_place(&mut data, dst_start, bytes_per_row, filter_byte, row_idx > 0);
        }

        data.truncate(num_rows * bytes_per_row);
        Ok(data)
    }

    pub fn unfilter_scanlines(&self, data: &[u8], pass_width: u32) -> VexelResult<Vec<u8>> {
        let bytes_per_row = self.bytes_per_row(pass_width);
        if bytes_per_row == 0 {
            return Ok(Vec::new());
        }

        let scanline_bytes = 1 + bytes_per_row;
        let num_rows = data.len() / scanline_bytes;
        let mut unfiltered = vec![0u8; num_rows * bytes_per_row];

        for (row_idx, scanline) in data.chunks(scanline_bytes).enumerate() {
            if scanline.len() < scanline_bytes {
                log_warn!("Invalid scanline length: {}, expected: {}", scanline.len(), scanline_bytes);
                unfiltered.truncate(row_idx * bytes_per_row);
                break;
            }

            let dst_start = row_idx * bytes_per_row;
            unfiltered[dst_start..dst_start + bytes_per_row].copy_from_slice(&scanline[1..]);
            self.unfilter_row_in_place(&mut unfiltered, dst_start, bytes_per_row, scanline[0], row_idx > 0);
        }

        Ok(unfiltered)
    }

    fn bytes_per_row(&self, pass_width: u32) -> usize {
        (pass_width as usize * self.get_bits_per_pixel() as usize).div_ceil(8)
    }

    fn unfilter_row_in_place(
        &self,
        buf: &mut [u8],
        row_start: usize,
        bytes_per_row: usize,
        filter_byte: u8,
        has_prior: bool,
    ) {
        let bytes_per_pixel = (self.get_bits_per_pixel() as usize).div_ceil(8);

        let filter_type = match filter_byte {
            0 => FilterType::None,
            1 => FilterType::Sub,
            2 => FilterType::Up,
            3 => FilterType::Average,
            4 => FilterType::Paeth,
            _ => {
                log_warn!("Invalid filter type: {}", filter_byte);
                FilterType::None
            }
        };

        if matches!(filter_type, FilterType::None) {
            return;
        }

        if !has_prior {
            let row = &mut buf[row_start..row_start + bytes_per_row];
            match filter_type {
                FilterType::Sub => filter_simd::unfilter_sub(row, bytes_per_pixel),
                FilterType::Average => filter_simd::unfilter_average_no_prior(row, bytes_per_pixel),
                FilterType::Paeth => filter_simd::unfilter_sub(row, bytes_per_pixel),
                _ => {}
            }
            return;
        }

        let (prior, row) = buf[row_start - bytes_per_row..row_start + bytes_per_row].split_at_mut(bytes_per_row);

        match filter_type {
            FilterType::Sub => filter_simd::unfilter_sub(row, bytes_per_pixel),
            FilterType::Up => filter_simd::unfilter_up(row, prior),
            FilterType::Average => filter_simd::unfilter_average(row, prior, bytes_per_pixel),
            FilterType::Paeth => filter_simd::unfilter_paeth(row, prior, bytes_per_pixel),
            FilterType::None => {}
        }
    }
}
