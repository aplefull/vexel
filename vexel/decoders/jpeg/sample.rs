use crate::decoders::jpeg::color::{convert_row, convert_row_wide, JpegColorspace};
use crate::decoders::jpeg::idct::{idct_islow_u8, idct_islow_wide, SimdLevel};
use crate::decoders::jpeg::upsample::{
    fancy_h1v2, fancy_h2v1, fancy_h2v1_u8, fancy_h2v2, fancy_h2v2_u8, nearest, Pixel,
};
use crate::PixelData;

pub(super) trait Sample: Pixel + Send + Sync + 'static {
    fn idct(
        level: SimdLevel,
        coef: &[i16; 64],
        quant: &[i32; 64],
        out: &mut [Self],
        stride: usize,
        dc_only: bool,
        precision: u8,
    );

    fn fancy_h2v1(level: SimdLevel, src: &[Self], dst: &mut [Self]);

    fn fancy_h2v2(level: SimdLevel, near: &[Self], far: &[Self], dst: &mut [Self]);

    fn fancy_h1v2(near: &[Self], far: &[Self], bias: u32, dst: &mut [Self]) {
        fancy_h1v2(near, far, bias, dst);
    }

    fn nearest(src: &[Self], h: usize, hmax: usize, dst: &mut [Self]) {
        nearest(src, h, hmax, dst);
    }

    fn convert_row(
        level: SimdLevel,
        color: JpegColorspace,
        rows: &[&[Self]; 4],
        out: &mut [Self],
        width: usize,
        precision: u8,
    );

    fn into_pixels(pixels: Vec<Self>, color: JpegColorspace, precision: u8) -> PixelData;
}

impl Sample for u8 {
    #[inline(always)]
    fn idct(
        level: SimdLevel,
        coef: &[i16; 64],
        quant: &[i32; 64],
        out: &mut [u8],
        stride: usize,
        dc_only: bool,
        _precision: u8,
    ) {
        idct_islow_u8(level, coef, quant, out, stride, dc_only);
    }

    fn fancy_h2v1(level: SimdLevel, src: &[u8], dst: &mut [u8]) {
        fancy_h2v1_u8(level, src, dst);
    }

    fn fancy_h2v2(level: SimdLevel, near: &[u8], far: &[u8], dst: &mut [u8]) {
        fancy_h2v2_u8(level, near, far, dst);
    }

    fn convert_row(
        level: SimdLevel,
        color: JpegColorspace,
        rows: &[&[u8]; 4],
        out: &mut [u8],
        width: usize,
        _precision: u8,
    ) {
        convert_row(level, color, rows, out, width);
    }

    fn into_pixels(pixels: Vec<u8>, color: JpegColorspace, _precision: u8) -> PixelData {
        match color {
            JpegColorspace::Grayscale => PixelData::L8(pixels),
            JpegColorspace::Rgba => PixelData::RGBA8(pixels),
            _ => PixelData::RGB8(pixels),
        }
    }
}

impl Sample for u16 {
    #[inline(always)]
    fn idct(
        _level: SimdLevel,
        coef: &[i16; 64],
        quant: &[i32; 64],
        out: &mut [u16],
        stride: usize,
        dc_only: bool,
        precision: u8,
    ) {
        idct_islow_wide(coef, quant, out, stride, dc_only, precision);
    }

    fn fancy_h2v1(_level: SimdLevel, src: &[u16], dst: &mut [u16]) {
        fancy_h2v1(src, dst, 0);
    }

    fn fancy_h2v2(_level: SimdLevel, near: &[u16], far: &[u16], dst: &mut [u16]) {
        fancy_h2v2(near, far, dst, 0);
    }

    fn convert_row(
        _level: SimdLevel,
        color: JpegColorspace,
        rows: &[&[u16]; 4],
        out: &mut [u16],
        width: usize,
        precision: u8,
    ) {
        convert_row_wide(color, rows, out, width, precision);
    }

    fn into_pixels(mut pixels: Vec<u16>, color: JpegColorspace, precision: u8) -> PixelData {
        let shift = 16u32.saturating_sub(precision as u32);
        if shift > 0 {
            for value in &mut pixels {
                *value <<= shift;
            }
        }
        match color {
            JpegColorspace::Grayscale => PixelData::L16(pixels),
            JpegColorspace::Rgba => PixelData::RGBA16(pixels),
            _ => PixelData::RGB16(pixels),
        }
    }
}
