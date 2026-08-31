use crate::decoders::jbig2::arith::{ArithDecoder, Cx};
use crate::decoders::jbig2::bitmap::{Bitmap, ComposeOp};
use crate::decoders::jbig2::generic::{decode_generic_region, GenericParams};
use crate::log_warn;
use crate::utils::tiff::fax;

pub struct PatternDict {
    pub patterns: Vec<Bitmap>,
    pub width: u32,
    pub height: u32,
}

pub struct PatternDictParams {
    pub mmr: bool,
    pub template: u8,
    pub width: u32,
    pub height: u32,
    pub gray_max: u32,
}

pub fn decode_pattern_dict(data: &[u8], params: &PatternDictParams, gb_stats: &mut [Cx]) -> Option<PatternDict> {
    let count = params.gray_max.checked_add(1)?;
    if params.width == 0 || params.height == 0 {
        return None;
    }

    let collective_width = params.width.checked_mul(count)?;
    let mut collective = Bitmap::new(collective_width, params.height);

    let generic_params = GenericParams {
        template: params.template,
        tpgdon: false,
        ext_template: false,
        at: {
            let mut at = crate::decoders::jbig2::generic::NOMINAL_AT;
            at[0] = (-(params.width as i32) as i8, 0);
            at[1] = (-3, -1);
            at[2] = (2, -2);
            at[3] = (-2, -2);
            at
        },
        skip: None,
    };

    if params.mmr {
        let (decoded, _) = fax::decode_mmr(data, collective_width, params.height as usize);
        copy_packed(&mut collective, &decoded, collective_width, params.height);
    } else {
        let mut arith = ArithDecoder::new(data);
        decode_generic_region(&mut arith, gb_stats, &mut collective, &generic_params);
    }

    let patterns = (0..count)
        .map(|index| {
            let mut pattern = Bitmap::new(params.width, params.height);
            pattern.compose(&collective, -((index as i64) * params.width as i64), 0, ComposeOp::Replace);
            pattern
        })
        .collect();

    Some(PatternDict { patterns, width: params.width, height: params.height })
}

pub struct HalftoneParams {
    pub mmr: bool,
    pub template: u8,
    pub enable_skip: bool,
    pub comb_op: ComposeOp,
    pub def_pixel: bool,
    pub grid_width: u32,
    pub grid_height: u32,
    pub grid_x: i32,
    pub grid_y: i32,
    pub vector_x: u16,
    pub vector_y: u16,
}

pub fn decode_halftone_region(
    data: &[u8],
    params: &HalftoneParams,
    patterns: &PatternDict,
    bitmap: &mut Bitmap,
    gb_stats: &mut [Cx],
) {
    bitmap.fill(params.def_pixel);

    if patterns.patterns.is_empty() || params.grid_width == 0 || params.grid_height == 0 {
        return;
    }

    let num_patterns = patterns.patterns.len() as u32;
    let mut bits_per_value = 0u32;
    while (1u32 << bits_per_value) < num_patterns {
        bits_per_value += 1;
    }
    if bits_per_value == 0 {
        bits_per_value = 1;
    }
    if bits_per_value > 16 {
        log_warn!("JBIG2 halftone region requires {} bitplanes, too many", bits_per_value);
        return;
    }

    let skip = if params.enable_skip { Some(build_skip_bitmap(params, patterns, bitmap)) } else { None };

    let Some(gray) = decode_gray_scale_image(data, params, bits_per_value, skip.as_ref(), gb_stats) else {
        return;
    };

    for m in 0..params.grid_height as i64 {
        for n in 0..params.grid_width as i64 {
            if let Some(skip) = &skip {
                if skip.get(n, m) != 0 {
                    continue;
                }
            }

            let x = (params.grid_x as i64 + m * params.vector_y as i64 + n * params.vector_x as i64) >> 8;
            let y = (params.grid_y as i64 + m * params.vector_x as i64 - n * params.vector_y as i64) >> 8;

            let index = gray[m as usize * params.grid_width as usize + n as usize];
            let index = (index as usize).min(patterns.patterns.len() - 1);
            bitmap.compose(&patterns.patterns[index], x, y, params.comb_op);
        }
    }
}

fn build_skip_bitmap(params: &HalftoneParams, patterns: &PatternDict, bitmap: &Bitmap) -> Bitmap {
    let mut skip = Bitmap::new(params.grid_width, params.grid_height);

    for m in 0..params.grid_height as i64 {
        for n in 0..params.grid_width as i64 {
            let x = (params.grid_x as i64 + m * params.vector_y as i64 + n * params.vector_x as i64) >> 8;
            let y = (params.grid_y as i64 + m * params.vector_x as i64 - n * params.vector_y as i64) >> 8;

            let outside = x + patterns.width as i64 <= 0
                || x >= bitmap.width as i64
                || y + patterns.height as i64 <= 0
                || y >= bitmap.height as i64;
            skip.set(n, m, outside as u32);
        }
    }

    skip
}

fn decode_gray_scale_image(
    data: &[u8],
    params: &HalftoneParams,
    bits_per_value: u32,
    skip: Option<&Bitmap>,
    gb_stats: &mut [Cx],
) -> Option<Vec<u16>> {
    let width = params.grid_width;
    let height = params.grid_height;

    let generic_params = GenericParams {
        template: params.template,
        tpgdon: false,
        ext_template: false,
        at: {
            let mut at = crate::decoders::jbig2::generic::NOMINAL_AT;
            at[0] = (if params.template <= 1 { 3 } else { 2 }, -1);
            at[1] = (-3, -1);
            at[2] = (2, -2);
            at[3] = (-2, -2);
            at
        },
        skip: skip.cloned(),
    };

    let mut planes: Vec<Bitmap> = (0..bits_per_value).map(|_| Bitmap::new(width, height)).collect();

    if params.mmr {
        let mut consumed = 0usize;
        for index in (0..bits_per_value as usize).rev() {
            let slice = data.get(consumed..).unwrap_or(&[]);
            let (decoded, used) = fax::decode_mmr(slice, width, height as usize);
            copy_packed(&mut planes[index], &decoded, width, height);
            consumed += used;
        }
    } else {
        let mut arith = ArithDecoder::new(data);
        for index in (0..bits_per_value as usize).rev() {
            decode_generic_region(&mut arith, gb_stats, &mut planes[index], &generic_params);
        }
    }

    for index in (0..bits_per_value as usize - 1).rev() {
        let (lower, upper) = planes.split_at_mut(index + 1);
        let target = &mut lower[index];
        let previous = &upper[0];
        for (byte, prev) in target.data.iter_mut().zip(previous.data.iter()) {
            *byte ^= *prev;
        }
    }

    let mut values = vec![0u16; width as usize * height as usize];
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let mut value = 0u32;
            for (index, plane) in planes.iter().enumerate() {
                value |= plane.get(x, y) << index;
            }
            values[y as usize * width as usize + x as usize] = value.min(u16::MAX as u32) as u16;
        }
    }

    Some(values)
}

fn copy_packed(bitmap: &mut Bitmap, decoded: &[u8], width: u32, height: u32) {
    let stride = (width as usize).div_ceil(8);
    for row in 0..height as usize {
        let src = row * stride;
        let dst = row * bitmap.stride;
        if src + stride > decoded.len() || dst + stride > bitmap.data.len() {
            break;
        }
        bitmap.data[dst..dst + stride].copy_from_slice(&decoded[src..src + stride]);
    }
}
