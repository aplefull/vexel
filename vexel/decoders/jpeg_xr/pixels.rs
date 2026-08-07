use crate::decoders::jpeg_xr::tables::{IDX_CC, IDX_CC_420};
use crate::decoders::jpeg_xr::types::*;
use crate::utils::image::PixelData;

pub const SHIFT_SCALED: i32 = SHIFTZERO + QPFRACBITS;

#[derive(Debug, Clone, Copy)]
pub struct OutputParams {
    pub width: usize,
    pub height: usize,
    pub source_color_format: ColorFormat,
    pub internal_color_format: ColorFormat,
    pub bit_depth: BitDepth,
    pub channels: usize,
    pub scaled_arith: bool,
    pub mantissa_len: u8,
    pub exp_bias: i8,
    pub rb_swapped: bool,
    pub black_white: bool,
    pub has_alpha: bool,
    pub window_top: usize,
    pub window_left: usize,
}

impl OutputParams {
    fn shift(&self) -> i32 {
        if self.scaled_arith {
            SHIFT_SCALED
        } else {
            0
        }
    }

    fn round_bias(&self) -> i32 {
        let shift = self.shift();
        if shift == 0 {
            0
        } else {
            1 << (shift - 1)
        }
    }

    fn scaled_bias(&self) -> i32 {
        if self.scaled_arith {
            (1 << (SHIFT_SCALED - 1)) - 1
        } else {
            0
        }
    }
}

pub struct PlaneReader<'a> {
    data: &'a [i32],
    mb_width: usize,
    stride: usize,
}

impl<'a> PlaneReader<'a> {
    pub fn new(data: &'a [i32], mb_width: usize, stride: usize) -> Self {
        PlaneReader { data, mb_width, stride }
    }

    #[inline]
    fn row_base(&self, row: usize) -> usize {
        (row >> 4) * self.mb_width * self.stride
    }

    #[inline]
    fn sample(&self, row: usize, column: usize) -> i32 {
        let index = self.row_base(row) + ((column >> 4) << 8) + usize::from(IDX_CC[row & 15][column & 15]);
        self.data.get(index).copied().unwrap_or(0)
    }

    #[inline]
    fn sample_422(&self, row: usize, column: usize) -> i32 {
        let index = self.row_base(row) + ((column >> 4) << 7) + usize::from(IDX_CC[row & 15][(column >> 1) & 7]);
        self.data.get(index).copied().unwrap_or(0)
    }

    #[inline]
    fn sample_420(&self, row: usize, column: usize) -> i32 {
        let index =
            self.row_base(row) + ((column >> 4) << 6) + usize::from(IDX_CC_420[(row >> 1) & 7][(column >> 1) & 7]);
        self.data.get(index).copied().unwrap_or(0)
    }
}

#[inline]
fn inverse_color(r: &mut i32, g: &mut i32, b: &mut i32) {
    *g = g.wrapping_sub(*r >> 1);
    *r = r.wrapping_sub((b.wrapping_add(1) >> 1).wrapping_sub(*g));
    *b = b.wrapping_add(*r);
}

#[inline]
fn inverse_color_cmyk(c: &mut i32, m: &mut i32, y: &mut i32, k: &mut i32) {
    *k = k.wrapping_sub(m.wrapping_add(1) >> 1);
    *m = m.wrapping_sub((*c >> 1).wrapping_sub(*k));
    *c = c.wrapping_sub((y.wrapping_add(1) >> 1).wrapping_sub(*m));
    *y = y.wrapping_add(*c);
}

#[inline]
fn clip8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[inline]
fn clip_u16(value: i32) -> u16 {
    value.clamp(0, 65535) as u16
}

#[inline]
fn clip_i16(value: i32) -> i16 {
    value.clamp(-32768, 32767) as i16
}

fn inverse_convert(value: i32) -> (u8, u8) {
    if value <= 0 {
        (0, 0)
    } else if (value >> 7) > 1 {
        ((value & 0x7f | 0x80) as u8, (value >> 7) as u8)
    } else {
        (value as u8, 1)
    }
}

fn inverse_convert_rgbe(r: i32, g: i32, b: i32) -> [u8; 4] {
    let (mut red, red_exp) = inverse_convert(r);
    let (mut green, green_exp) = inverse_convert(g);
    let (mut blue, blue_exp) = inverse_convert(b);

    let exponent = red_exp.max(green_exp).max(blue_exp);

    let rescale = |value: u8, channel_exp: u8| -> u8 {
        let shift = (u32::from(exponent.wrapping_sub(channel_exp)).wrapping_add(1)) & 31;
        (((i32::from(value) * 2 + 1) >> shift) & 0xff) as u8
    };

    if exponent > red_exp {
        red = rescale(red, red_exp);
    }
    if exponent > green_exp {
        green = rescale(green, green_exp);
    }
    if exponent > blue_exp {
        blue = rescale(blue, blue_exp);
    }

    [red, green, blue, exponent]
}

pub fn pixel_to_float(value: i32, exp_bias: i8, mantissa_len: u8) -> f32 {
    let lm = u32::from(mantissa_len);
    let lm_shift = 1i32 << lm;

    let sign = value >> 31;
    let magnitude = (value ^ sign).wrapping_sub(sign);

    let mut exponent = ((magnitude as u32) >> lm) as i32;
    let mut mantissa = (magnitude & (lm_shift - 1)) | lm_shift;

    if exponent == 0 {
        mantissa ^= lm_shift;
        exponent = 1;
    }

    exponent += 127 - i32::from(exp_bias);

    while mantissa < lm_shift && exponent > 1 && mantissa > 0 {
        exponent -= 1;
        mantissa <<= 1;
    }

    if mantissa < lm_shift {
        exponent = 0;
    } else {
        mantissa ^= lm_shift;
    }

    mantissa <<= 23 - lm;

    let bits = ((sign as u32) & 0x8000_0000) | ((exponent as u32) << 23) | (mantissa as u32);
    f32::from_bits(bits)
}

pub fn backward_half(value: i32) -> u16 {
    let sign = value >> 31;
    (((value & 0x7fff) ^ sign).wrapping_sub(sign)) as u16
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits >> 15) << 31;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x3ff);

    if exponent == 0 {
        if mantissa == 0 {
            return f32::from_bits(sign);
        }

        let mut exponent = -1i32;
        let mut mantissa = mantissa;
        while mantissa & 0x400 == 0 {
            mantissa <<= 1;
            exponent -= 1;
        }
        let mantissa = mantissa & 0x3ff;
        let biased = (exponent + 127 - 15 + 1) as u32;
        return f32::from_bits(sign | (biased << 23) | (mantissa << 13));
    }

    if exponent == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (mantissa << 13));
    }

    let biased = exponent + 127 - 15;
    f32::from_bits(sign | (biased << 23) | (mantissa << 13))
}

struct Chroma {
    u: Vec<i32>,
    v: Vec<i32>,
}

fn interpolate_uv(
    u: &PlaneReader,
    v: &PlaneReader,
    internal: ColorFormat,
    mb_width: usize,
    mb_height: usize,
) -> Chroma {
    let width = mb_width * MB_SIZE;
    let height = mb_height * MB_SIZE;
    let size = mb_width * mb_height * 256;
    let mut out_u = vec![0i32; size];
    let mut out_v = vec![0i32; size];

    let index_of = |row: usize, column: usize| -> usize {
        (row >> 4) * mb_width * 256 + ((column >> 4) << 8) + usize::from(IDX_CC[row & 15][column & 15])
    };

    let store = |target: &mut Vec<i32>, row: usize, column: usize, value: i32| {
        if let Some(slot) = target.get_mut(index_of(row, column)) {
            *slot = value;
        }
    };

    let load = |target: &Vec<i32>, row: usize, column: usize| -> i32 {
        target.get(index_of(row, column)).copied().unwrap_or(0)
    };

    if internal == ColorFormat::Yuv422 {
        for row in 0..height {
            let mut column = 0usize;
            while column < width {
                store(&mut out_u, row, column, u.sample_422(row, column));
                store(&mut out_v, row, column, v.sample_422(row, column));

                if column > 0 {
                    let left = column - 2;
                    let center = column - 1;

                    let mid_u = (load(&out_u, row, left) + load(&out_u, row, column) + 1) >> 1;
                    let mid_v = (load(&out_v, row, left) + load(&out_v, row, column) + 1) >> 1;
                    store(&mut out_u, row, center, mid_u);
                    store(&mut out_v, row, center, mid_v);
                }

                column += 2;
            }

            let last = width - 1;
            let previous = width - 2;
            let value_u = load(&out_u, row, previous);
            let value_v = load(&out_v, row, previous);
            store(&mut out_u, row, last, value_u);
            store(&mut out_v, row, last, value_v);
        }
    } else {
        let mut column = 0usize;
        while column < width {
            let mut row = 0usize;
            while row < height {
                store(&mut out_u, row, column, u.sample_420(row, column));
                store(&mut out_v, row, column, v.sample_420(row, column));

                if row > 0 {
                    let top = row - 2;
                    let center = row - 1;

                    let mid_u = (load(&out_u, top, column) + load(&out_u, row, column) + 1) >> 1;
                    let mid_v = (load(&out_v, top, column) + load(&out_v, row, column) + 1) >> 1;
                    store(&mut out_u, center, column, mid_u);
                    store(&mut out_v, center, column, mid_v);
                }

                row += 2;
            }

            if height >= 2 {
                let value_u = load(&out_u, height - 2, column);
                let value_v = load(&out_v, height - 2, column);
                store(&mut out_u, height - 1, column, value_u);
                store(&mut out_v, height - 1, column, value_v);
            }

            column += 2;
        }

        for row in 0..height {
            let mut column = 1usize;
            while column + 1 < width {
                let left = load(&out_u, row, column - 1);
                let right = load(&out_u, row, column + 1);
                store(&mut out_u, row, column, (left + right + 1) >> 1);

                let left = load(&out_v, row, column - 1);
                let right = load(&out_v, row, column + 1);
                store(&mut out_v, row, column, (left + right + 1) >> 1);

                column += 2;
            }

            if width >= 2 {
                let value_u = load(&out_u, row, width - 2);
                let value_v = load(&out_v, row, width - 2);
                store(&mut out_u, row, width - 1, value_u);
                store(&mut out_v, row, width - 1, value_v);
            }
        }
    }

    Chroma { u: out_u, v: out_v }
}

struct Sampler<'a> {
    planes: Vec<PlaneReader<'a>>,
    chroma: Option<Chroma>,
    mb_width: usize,
    window_top: usize,
    window_left: usize,
}

impl<'a> Sampler<'a> {
    fn origin(&self, row: usize, column: usize) -> (usize, usize) {
        (row + self.window_top, column + self.window_left)
    }

    fn luma(&self, row: usize, column: usize) -> i32 {
        let (row, column) = self.origin(row, column);
        self.planes[0].sample(row, column)
    }

    fn chroma_index(&self, row: usize, column: usize) -> usize {
        (row >> 4) * self.mb_width * 256 + ((column >> 4) << 8) + usize::from(IDX_CC[row & 15][column & 15])
    }

    fn chroma_u(&self, row: usize, column: usize) -> i32 {
        let (row, column) = self.origin(row, column);
        match &self.chroma {
            Some(chroma) => chroma.u.get(self.chroma_index(row, column)).copied().unwrap_or(0),
            None => self.planes.get(1).map(|p| p.sample(row, column)).unwrap_or(0),
        }
    }

    fn chroma_v(&self, row: usize, column: usize) -> i32 {
        let (row, column) = self.origin(row, column);
        match &self.chroma {
            Some(chroma) => chroma.v.get(self.chroma_index(row, column)).copied().unwrap_or(0),
            None => self.planes.get(2).map(|p| p.sample(row, column)).unwrap_or(0),
        }
    }

    fn channel(&self, index: usize, row: usize, column: usize) -> i32 {
        let (row, column) = self.origin(row, column);
        self.planes.get(index).map(|p| p.sample(row, column)).unwrap_or(0)
    }
}

fn plane_stride(color_format: ColorFormat, channel: usize) -> usize {
    if channel == 0 {
        return 256;
    }

    match color_format {
        ColorFormat::Yuv420 => 64,
        ColorFormat::Yuv422 => 128,
        _ => 256,
    }
}

fn build_sampler<'a>(planes: &'a [Vec<i32>], params: &OutputParams, mb_width: usize, mb_height: usize) -> Sampler<'a> {
    let readers: Vec<PlaneReader<'a>> = planes
        .iter()
        .enumerate()
        .map(|(channel, plane)| {
            let stride = plane_stride(params.internal_color_format, channel);
            PlaneReader::new(plane, mb_width, stride)
        })
        .collect();

    let chroma = if params.internal_color_format.is_chroma_subsampled() && readers.len() >= 3 {
        Some(interpolate_uv(&readers[1], &readers[2], params.internal_color_format, mb_width, mb_height))
    } else {
        None
    };

    Sampler { planes: readers, chroma, mb_width, window_top: params.window_top, window_left: params.window_left }
}

fn rgb_triplet(sampler: &Sampler, row: usize, column: usize, bias: i32) -> (i32, i32, i32) {
    let mut g = sampler.luma(row, column).wrapping_add(bias);
    let mut r = sampler.chroma_u(row, column).wrapping_neg();
    let mut b = sampler.chroma_v(row, column);

    inverse_color(&mut r, &mut g, &mut b);
    (r, g, b)
}

fn cmyk_quad(sampler: &Sampler, row: usize, column: usize, bias1: i32, bias2: i32) -> (i32, i32, i32, i32) {
    let mut m = sampler.luma(row, column).wrapping_neg().wrapping_add(bias1);
    let mut c = sampler.chroma_u(row, column);
    let mut y = sampler.chroma_v(row, column).wrapping_neg();
    let mut k = sampler.channel(3, row, column).wrapping_add(bias2);

    inverse_color_cmyk(&mut c, &mut m, &mut y, &mut k);
    (c, m, y, k)
}

fn cmyk_to_rgb(c: i32, m: i32, y: i32, k: i32, max: i32) -> (i32, i32, i32) {
    let inv_k = i64::from(max - k.clamp(0, max));
    let divisor = i64::from(max.max(1));
    let channel = |v: i32| (i64::from(max - v.clamp(0, max)) * inv_k / divisor) as i32;
    (channel(c), channel(m), channel(y))
}

pub fn build_image_data(
    planes: &[Vec<i32>],
    alpha: Option<&[Vec<i32>]>,
    params: &OutputParams,
    mb_width: usize,
    mb_height: usize,
) -> PixelData {
    let sampler = build_sampler(planes, params, mb_width, mb_height);
    let alpha_sampler = alpha.map(|planes| build_sampler(planes, params, mb_width, mb_height));

    match params.bit_depth {
        BitDepth::Bd1White | BitDepth::Bd1Black => build_bilevel(&sampler, params),
        BitDepth::Bd8 => build_8bit(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd16 => build_16bit(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd16S => build_16bit_signed(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd16F => build_half(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd32 | BitDepth::Bd32S => build_32bit(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd32F => build_float(&sampler, alpha_sampler.as_ref(), params),
        BitDepth::Bd5 => build_packed555(&sampler, params),
        BitDepth::Bd565 => build_packed565(&sampler, params),
        BitDepth::Bd10 => build_packed101010(&sampler, params),
    }
}

fn build_bilevel(sampler: &Sampler, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let threshold = if shift > 0 { 1 << (shift - 1) } else { 1 };
    let black_white = i32::from(params.black_white);

    let mut out = Vec::with_capacity(params.width * params.height);

    for row in 0..params.height {
        for column in 0..params.width {
            let value = sampler.luma(row, column);
            let bit = (black_white + i32::from(value >= threshold)) & 1;
            out.push(if bit != 0 { 1u8 } else { 0u8 });
        }
    }

    PixelData::L1(out)
}

fn build_8bit(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias1 = 128 << shift;
    let bias2 = params.scaled_bias();
    let bias = bias1 + bias2;

    match params.source_color_format {
        ColorFormat::Rgbe => {
            let mut out = Vec::with_capacity(params.width * params.height * 4);
            for row in 0..params.height {
                for column in 0..params.width {
                    let mut g = sampler.luma(row, column).wrapping_add(bias2);
                    let mut r = sampler.chroma_u(row, column).wrapping_neg();
                    let mut b = sampler.chroma_v(row, column);
                    inverse_color(&mut r, &mut g, &mut b);

                    let rgbe = inverse_convert_rgbe(r >> shift, g >> shift, b >> shift);
                    out.extend_from_slice(&rgbe);
                }
            }
            PixelData::RGBA8(out)
        }

        ColorFormat::Cmyk => {
            let mut out = Vec::with_capacity(params.width * params.height * 4);
            for row in 0..params.height {
                for column in 0..params.width {
                    let (c, m, y, k) = cmyk_quad(sampler, row, column, bias1, bias2);
                    let (r, g, b) = cmyk_to_rgb(c >> shift, m >> shift, y >> shift, k >> shift, 255);

                    let a = alpha.map(|s| (s.luma(row, column).wrapping_add(bias)) >> shift).unwrap_or(255);

                    out.push(clip8(r));
                    out.push(clip8(g));
                    out.push(clip8(b));
                    out.push(clip8(a));
                }
            }
            PixelData::RGBA8(out)
        }

        ColorFormat::YOnly => {
            let mut out = Vec::with_capacity(params.width * params.height);
            for row in 0..params.height {
                for column in 0..params.width {
                    let value = (sampler.luma(row, column).wrapping_add(bias)) >> shift;
                    out.push(clip8(value));
                }
            }
            PixelData::L8(out)
        }

        ColorFormat::NComponent => build_ncomponent_8(sampler, alpha, params, bias, shift),

        _ => {
            let with_alpha = params.has_alpha || alpha.is_some();
            let channels = if with_alpha { 4 } else { 3 };
            let mut out = Vec::with_capacity(params.width * params.height * channels);

            for row in 0..params.height {
                for column in 0..params.width {
                    let (r, g, b) = rgb_triplet(sampler, row, column, bias);
                    let (mut r, g, mut b) = (r >> shift, g >> shift, b >> shift);

                    if params.rb_swapped {
                        std::mem::swap(&mut r, &mut b);
                    }

                    out.push(clip8(r));
                    out.push(clip8(g));
                    out.push(clip8(b));

                    if with_alpha {
                        let a = alpha.map(|s| (s.luma(row, column).wrapping_add(bias)) >> shift).unwrap_or(255);
                        out.push(clip8(a));
                    }
                }
            }

            if with_alpha {
                PixelData::RGBA8(out)
            } else {
                PixelData::RGB8(out)
            }
        }
    }
}

fn build_ncomponent_8(
    sampler: &Sampler,
    alpha: Option<&Sampler>,
    params: &OutputParams,
    bias: i32,
    shift: i32,
) -> PixelData {
    let channels = params.channels;

    if channels >= 3 {
        let with_alpha = channels >= 4 || alpha.is_some();
        let mut out = Vec::with_capacity(params.width * params.height * if with_alpha { 4 } else { 3 });
        for row in 0..params.height {
            for column in 0..params.width {
                for channel in 0..3usize {
                    let value = (sampler.channel(channel, row, column).wrapping_add(bias)) >> shift;
                    out.push(clip8(value));
                }
                if with_alpha {
                    let value = match alpha {
                        Some(s) => (s.luma(row, column).wrapping_add(bias)) >> shift,
                        None => (sampler.channel(3, row, column).wrapping_add(bias)) >> shift,
                    };
                    out.push(clip8(value));
                }
            }
        }
        return if with_alpha { PixelData::RGBA8(out) } else { PixelData::RGB8(out) };
    }

    let mut out = Vec::with_capacity(params.width * params.height);
    for row in 0..params.height {
        for column in 0..params.width {
            let value = (sampler.channel(0, row, column).wrapping_add(bias)) >> shift;
            out.push(clip8(value));
        }
    }
    PixelData::L8(out)
}

fn build_16bit(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let len = u32::from(params.mantissa_len);
    let bias = (((1i32 << 15) >> len) << shift) + params.round_bias();
    let alpha_bias = (1 << (shift + 15)) + params.round_bias();

    match params.source_color_format {
        ColorFormat::YOnly => {
            let mut out = Vec::with_capacity(params.width * params.height);
            for row in 0..params.height {
                for column in 0..params.width {
                    let value = ((sampler.luma(row, column).wrapping_add(bias)) >> shift) << len;
                    out.push(clip_u16(value));
                }
            }
            PixelData::L16(out)
        }

        ColorFormat::Cmyk => {
            let bias1 = (32768 >> len) << shift;
            let bias2 = bias - bias1;
            let mut out = Vec::with_capacity(params.width * params.height * 4);
            for row in 0..params.height {
                for column in 0..params.width {
                    let (c, m, y, k) = cmyk_quad(sampler, row, column, bias1, bias2);
                    let (c, m, y, k) =
                        ((c >> shift) << len, (m >> shift) << len, (y >> shift) << len, (k >> shift) << len);
                    let (r, g, b) = cmyk_to_rgb(c, m, y, k, 65535);

                    let a = alpha
                        .map(|s| ((s.luma(row, column).wrapping_add(alpha_bias)) >> shift) << len)
                        .unwrap_or(65535);

                    out.push(clip_u16(r));
                    out.push(clip_u16(g));
                    out.push(clip_u16(b));
                    out.push(clip_u16(a));
                }
            }
            PixelData::RGBA16(out)
        }

        ColorFormat::NComponent if params.channels < 3 => {
            let mut out = Vec::with_capacity(params.width * params.height);
            for row in 0..params.height {
                for column in 0..params.width {
                    let value = ((sampler.channel(0, row, column).wrapping_add(bias)) >> shift) << len;
                    out.push(clip_u16(value));
                }
            }
            PixelData::L16(out)
        }

        ColorFormat::NComponent => {
            let with_alpha = params.channels >= 4 || alpha.is_some();
            let mut out = Vec::with_capacity(params.width * params.height * if with_alpha { 4 } else { 3 });
            for row in 0..params.height {
                for column in 0..params.width {
                    for channel in 0..3usize {
                        let value = ((sampler.channel(channel, row, column).wrapping_add(bias)) >> shift) << len;
                        out.push(clip_u16(value));
                    }
                    if with_alpha {
                        let value = match alpha {
                            Some(s) => ((s.luma(row, column).wrapping_add(alpha_bias)) >> shift) << len,
                            None => ((sampler.channel(3, row, column).wrapping_add(bias)) >> shift) << len,
                        };
                        out.push(clip_u16(value));
                    }
                }
            }
            if with_alpha {
                PixelData::RGBA16(out)
            } else {
                PixelData::RGB16(out)
            }
        }

        _ => {
            let with_alpha = params.has_alpha || alpha.is_some();
            let mut out = Vec::with_capacity(params.width * params.height * 4);

            for row in 0..params.height {
                for column in 0..params.width {
                    let (r, g, b) = rgb_triplet(sampler, row, column, bias);
                    let (mut r, g, mut b) = ((r >> shift) << len, (g >> shift) << len, (b >> shift) << len);

                    if params.rb_swapped {
                        std::mem::swap(&mut r, &mut b);
                    }

                    out.push(clip_u16(r));
                    out.push(clip_u16(g));
                    out.push(clip_u16(b));

                    if with_alpha {
                        let a = alpha
                            .map(|s| ((s.luma(row, column).wrapping_add(alpha_bias)) >> shift) << len)
                            .unwrap_or(65535);
                        out.push(clip_u16(a));
                    }
                }
            }

            if with_alpha {
                PixelData::RGBA16(out)
            } else {
                PixelData::RGB16(out)
            }
        }
    }
}

fn build_16bit_signed(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let len = u32::from(params.mantissa_len);
    let bias = params.round_bias();

    let normalize = |value: i16| -> u16 { ((i32::from(value) - i32::from(i16::MIN)) as u32).min(65535) as u16 };

    let with_alpha = params.has_alpha || alpha.is_some();

    if params.source_color_format == ColorFormat::YOnly
        || (params.source_color_format == ColorFormat::NComponent && params.channels < 3)
    {
        let mut out = Vec::with_capacity(params.width * params.height);
        for row in 0..params.height {
            for column in 0..params.width {
                let value = ((sampler.channel(0, row, column).wrapping_add(bias)) >> shift) << len;
                out.push(normalize(clip_i16(value)));
            }
        }
        return PixelData::L16(out);
    }

    let mut out = Vec::with_capacity(params.width * params.height * 4);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = ((r >> shift) << len, (g >> shift) << len, (b >> shift) << len);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(normalize(clip_i16(r)));
            out.push(normalize(clip_i16(g)));
            out.push(normalize(clip_i16(b)));

            if with_alpha {
                let a = alpha
                    .map(|s| ((s.luma(row, column).wrapping_add(bias)) >> shift) << len)
                    .unwrap_or(i32::from(i16::MAX));
                out.push(normalize(clip_i16(a)));
            }
        }
    }

    if with_alpha {
        PixelData::RGBA16(out)
    } else {
        PixelData::RGB16(out)
    }
}

fn build_half(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias = params.scaled_bias();
    let alpha_bias = params.round_bias();
    let with_alpha = params.has_alpha || alpha.is_some();

    if params.source_color_format == ColorFormat::YOnly
        || (params.source_color_format == ColorFormat::NComponent && params.channels < 3)
    {
        let mut out = Vec::with_capacity(params.width * params.height);
        for row in 0..params.height {
            for column in 0..params.width {
                let value = (sampler.channel(0, row, column).wrapping_add(bias)) >> shift;
                out.push(half_to_f32(backward_half(value)));
            }
        }
        return PixelData::L32F(out);
    }

    let mut out = Vec::with_capacity(params.width * params.height * 4);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = (r >> shift, g >> shift, b >> shift);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(half_to_f32(backward_half(r)));
            out.push(half_to_f32(backward_half(g)));
            out.push(half_to_f32(backward_half(b)));

            if with_alpha {
                let a = alpha.map(|s| (s.luma(row, column).wrapping_add(alpha_bias)) >> shift).unwrap_or(0x3c00);
                out.push(half_to_f32(backward_half(a)));
            }
        }
    }

    if with_alpha {
        PixelData::RGBA32F(out)
    } else {
        PixelData::RGB32F(out)
    }
}

fn build_32bit(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let len = u32::from(params.mantissa_len);
    let signed = params.bit_depth == BitDepth::Bd32S;

    let bias = if signed {
        params.round_bias()
    } else {
        (((1i64 << 31) >> len) as i32).wrapping_shl(shift as u32).wrapping_add(params.round_bias())
    };

    let scale = |value: i32| -> f32 {
        let shifted = (value >> shift).wrapping_shl(len);
        if signed {
            (shifted as f64 / i32::MAX as f64) as f32
        } else {
            ((shifted as u32) as f64 / u32::MAX as f64) as f32
        }
    };

    let with_alpha = params.has_alpha || alpha.is_some();

    if params.source_color_format == ColorFormat::YOnly
        || (params.source_color_format == ColorFormat::NComponent && params.channels < 3)
    {
        let mut out = Vec::with_capacity(params.width * params.height);
        for row in 0..params.height {
            for column in 0..params.width {
                let value = sampler.channel(0, row, column).wrapping_add(bias);
                out.push(scale(value));
            }
        }
        return PixelData::L32F(out);
    }

    let mut out = Vec::with_capacity(params.width * params.height * 4);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = (r, g, b);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(scale(r));
            out.push(scale(g));
            out.push(scale(b));

            if with_alpha {
                let a = alpha.map(|s| s.luma(row, column).wrapping_add(bias)).unwrap_or(i32::MAX);
                out.push(scale(a));
            }
        }
    }

    if with_alpha {
        PixelData::RGBA32F(out)
    } else {
        PixelData::RGB32F(out)
    }
}

fn build_float(sampler: &Sampler, alpha: Option<&Sampler>, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias = params.scaled_bias();
    let alpha_bias = params.round_bias();
    let exp_bias = params.exp_bias;
    let len = params.mantissa_len;
    let with_alpha = params.has_alpha || alpha.is_some();

    if params.source_color_format == ColorFormat::YOnly
        || (params.source_color_format == ColorFormat::NComponent && params.channels < 3)
    {
        let mut out = Vec::with_capacity(params.width * params.height);
        for row in 0..params.height {
            for column in 0..params.width {
                let value = (sampler.channel(0, row, column).wrapping_add(bias)) >> shift;
                out.push(pixel_to_float(value, exp_bias, len));
            }
        }
        return PixelData::L32F(out);
    }

    let mut out = Vec::with_capacity(params.width * params.height * 4);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = (r >> shift, g >> shift, b >> shift);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(pixel_to_float(r, exp_bias, len));
            out.push(pixel_to_float(g, exp_bias, len));
            out.push(pixel_to_float(b, exp_bias, len));

            if with_alpha {
                let a = alpha.map(|s| (s.luma(row, column).wrapping_add(alpha_bias)) >> shift).unwrap_or(0);
                out.push(if alpha.is_some() { pixel_to_float(a, exp_bias, len) } else { 1.0 });
            }
        }
    }

    if with_alpha {
        PixelData::RGBA32F(out)
    } else {
        PixelData::RGB32F(out)
    }
}

fn build_packed555(sampler: &Sampler, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias = (16 << shift) + params.scaled_bias();
    let mut out = Vec::with_capacity(params.width * params.height * 3);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = (r >> shift, g >> shift, b >> shift);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(expand5(r.clamp(0, 31)));
            out.push(expand5(g.clamp(0, 31)));
            out.push(expand5(b.clamp(0, 31)));
        }
    }

    PixelData::RGB8(out)
}

fn build_packed565(sampler: &Sampler, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias = (32 << shift) + params.scaled_bias();
    let mut out = Vec::with_capacity(params.width * params.height * 3);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let g = g >> shift;
            let (mut r, mut b) = (r >> (shift + 1), b >> (shift + 1));

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(expand5(r.clamp(0, 31)));
            out.push(expand6(g.clamp(0, 63)));
            out.push(expand5(b.clamp(0, 31)));
        }
    }

    PixelData::RGB8(out)
}

fn build_packed101010(sampler: &Sampler, params: &OutputParams) -> PixelData {
    let shift = params.shift();
    let bias = (512 << shift) + params.scaled_bias();
    let mut out = Vec::with_capacity(params.width * params.height * 3);

    for row in 0..params.height {
        for column in 0..params.width {
            let (r, g, b) = rgb_triplet(sampler, row, column, bias);
            let (mut r, g, mut b) = (r >> shift, g >> shift, b >> shift);

            if params.rb_swapped {
                std::mem::swap(&mut r, &mut b);
            }

            out.push(expand10(r.clamp(0, 1023)));
            out.push(expand10(g.clamp(0, 1023)));
            out.push(expand10(b.clamp(0, 1023)));
        }
    }

    PixelData::RGB16(out)
}

#[inline]
fn expand5(value: i32) -> u8 {
    ((value << 3) | (value >> 2)) as u8
}

#[inline]
fn expand6(value: i32) -> u8 {
    ((value << 2) | (value >> 4)) as u8
}

#[inline]
fn expand10(value: i32) -> u16 {
    ((value << 6) | (value >> 4)) as u16
}
