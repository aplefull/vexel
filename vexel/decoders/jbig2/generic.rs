use crate::decoders::jbig2::arith::{ArithDecoder, Cx};
use crate::decoders::jbig2::bitmap::Bitmap;
use crate::log_warn;

pub struct GenericParams {
    pub template: u8,
    pub tpgdon: bool,
    pub ext_template: bool,
    pub at: [(i8, i8); 12],
    pub skip: Option<Bitmap>,
}

pub const NOMINAL_AT: [(i8, i8); 12] =
    [(3, -1), (-3, -1), (2, -2), (-2, -2), (1, -2), (2, -1), (-3, 0), (-4, 0), (2, -2), (3, -1), (-2, -2), (-3, -1)];

impl Default for GenericParams {
    fn default() -> Self {
        GenericParams { template: 0, tpgdon: false, ext_template: false, at: NOMINAL_AT, skip: None }
    }
}

pub fn stats_size(template: u8) -> usize {
    match template {
        0 => 1 << 16,
        1 => 1 << 13,
        _ => 1 << 10,
    }
}

fn tpgdon_context(params: &GenericParams) -> usize {
    if params.ext_template {
        return 0x9b25;
    }
    match params.template {
        0 => 0x9b25,
        1 => 0x0795,
        2 => 0x00e5,
        _ => 0x0195,
    }
}

fn build_ext_context(bitmap: &Bitmap, params: &GenericParams, x: i64, y: i64) -> usize {
    let at = &params.at;
    let mut cx = bitmap.get(x - 1, y);
    cx |= bitmap.get(x + 1, y - 1) << 1;
    cx |= bitmap.get(x, y - 1) << 2;
    cx |= bitmap.get(x - 1, y - 1) << 3;
    for (index, entry) in at.iter().enumerate() {
        cx |= bitmap.get(x + entry.0 as i64, y + entry.1 as i64) << (4 + index);
    }
    cx as usize
}

fn build_context(bitmap: &Bitmap, params: &GenericParams, x: i64, y: i64) -> usize {
    if params.ext_template {
        return build_ext_context(bitmap, params, x, y);
    }
    let at = &params.at;
    match params.template {
        0 => {
            let mut cx = bitmap.get(x - 1, y);
            cx |= bitmap.get(x - 2, y) << 1;
            cx |= bitmap.get(x - 3, y) << 2;
            cx |= bitmap.get(x - 4, y) << 3;
            cx |= bitmap.get(x + at[0].0 as i64, y + at[0].1 as i64) << 4;
            cx |= bitmap.get(x + 2, y - 1) << 5;
            cx |= bitmap.get(x + 1, y - 1) << 6;
            cx |= bitmap.get(x, y - 1) << 7;
            cx |= bitmap.get(x - 1, y - 1) << 8;
            cx |= bitmap.get(x - 2, y - 1) << 9;
            cx |= bitmap.get(x + at[1].0 as i64, y + at[1].1 as i64) << 10;
            cx |= bitmap.get(x + at[2].0 as i64, y + at[2].1 as i64) << 11;
            cx |= bitmap.get(x + 1, y - 2) << 12;
            cx |= bitmap.get(x, y - 2) << 13;
            cx |= bitmap.get(x - 1, y - 2) << 14;
            cx |= bitmap.get(x + at[3].0 as i64, y + at[3].1 as i64) << 15;
            cx as usize
        }
        1 => {
            let mut cx = bitmap.get(x - 1, y);
            cx |= bitmap.get(x - 2, y) << 1;
            cx |= bitmap.get(x - 3, y) << 2;
            cx |= bitmap.get(x + at[0].0 as i64, y + at[0].1 as i64) << 3;
            cx |= bitmap.get(x + 2, y - 1) << 4;
            cx |= bitmap.get(x + 1, y - 1) << 5;
            cx |= bitmap.get(x, y - 1) << 6;
            cx |= bitmap.get(x - 1, y - 1) << 7;
            cx |= bitmap.get(x - 2, y - 1) << 8;
            cx |= bitmap.get(x + 2, y - 2) << 9;
            cx |= bitmap.get(x + 1, y - 2) << 10;
            cx |= bitmap.get(x, y - 2) << 11;
            cx |= bitmap.get(x - 1, y - 2) << 12;
            cx as usize
        }
        2 => {
            let mut cx = bitmap.get(x - 1, y);
            cx |= bitmap.get(x - 2, y) << 1;
            cx |= bitmap.get(x + at[0].0 as i64, y + at[0].1 as i64) << 2;
            cx |= bitmap.get(x + 1, y - 1) << 3;
            cx |= bitmap.get(x, y - 1) << 4;
            cx |= bitmap.get(x - 1, y - 1) << 5;
            cx |= bitmap.get(x - 2, y - 1) << 6;
            cx |= bitmap.get(x + 1, y - 2) << 7;
            cx |= bitmap.get(x, y - 2) << 8;
            cx |= bitmap.get(x - 1, y - 2) << 9;
            cx as usize
        }
        _ => {
            let mut cx = bitmap.get(x - 1, y);
            cx |= bitmap.get(x - 2, y) << 1;
            cx |= bitmap.get(x - 3, y) << 2;
            cx |= bitmap.get(x - 4, y) << 3;
            cx |= bitmap.get(x + at[0].0 as i64, y + at[0].1 as i64) << 4;
            cx |= bitmap.get(x + 1, y - 1) << 5;
            cx |= bitmap.get(x, y - 1) << 6;
            cx |= bitmap.get(x - 1, y - 1) << 7;
            cx |= bitmap.get(x - 2, y - 1) << 8;
            cx |= bitmap.get(x - 3, y - 1) << 9;
            cx as usize
        }
    }
}

pub fn decode_generic_region(arith: &mut ArithDecoder, stats: &mut [Cx], bitmap: &mut Bitmap, params: &GenericParams) {
    let width = bitmap.width as i64;
    let height = bitmap.height as i64;
    let mut ltp = false;

    for y in 0..height {
        if arith.exhausted() {
            log_warn!("JBIG2 generic region data exhausted at row {} of {}, leaving the rest blank", y, height);
            break;
        }
        if params.tpgdon {
            let cx = tpgdon_context(params);
            if let Some(slot) = stats.get_mut(cx) {
                if arith.decode(slot) == 1 {
                    ltp = !ltp;
                }
            }
            if ltp {
                if y > 0 {
                    let (prev, cur) = bitmap.data.split_at_mut(y as usize * bitmap.stride);
                    let prev_row = &prev[(y as usize - 1) * bitmap.stride..];
                    cur[..bitmap.stride].copy_from_slice(&prev_row[..bitmap.stride]);
                }
                continue;
            }
        }

        for x in 0..width {
            if let Some(skip) = &params.skip {
                if skip.get(x, y) != 0 {
                    bitmap.set(x, y, 0);
                    continue;
                }
            }

            let cx = build_context(bitmap, params, x, y);
            let bit = match stats.get_mut(cx) {
                Some(slot) => arith.decode(slot),
                None => 0,
            };
            bitmap.set(x, y, bit);
        }
    }
}
