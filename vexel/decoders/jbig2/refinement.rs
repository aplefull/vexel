use crate::decoders::jbig2::arith::{ArithDecoder, Cx};
use crate::decoders::jbig2::bitmap::Bitmap;
use crate::log_warn;

pub struct RefinementParams<'a> {
    pub template: u8,
    pub reference: &'a Bitmap,
    pub dx: i32,
    pub dy: i32,
    pub tpgron: bool,
    pub at: [(i8, i8); 2],
}

pub fn stats_size(template: u8) -> usize {
    if template != 0 {
        1 << 10
    } else {
        1 << 13
    }
}

fn build_context(bitmap: &Bitmap, params: &RefinementParams, x: i64, y: i64) -> usize {
    let reference = params.reference;
    let rx = x - params.dx as i64;
    let ry = y - params.dy as i64;

    if params.template == 0 {
        let at = &params.at;
        let mut cx = bitmap.get(x - 1, y);
        cx |= bitmap.get(x + 1, y - 1) << 1;
        cx |= bitmap.get(x, y - 1) << 2;
        cx |= bitmap.get(x + at[0].0 as i64, y + at[0].1 as i64) << 3;
        cx |= reference.get(rx + 1, ry + 1) << 4;
        cx |= reference.get(rx, ry + 1) << 5;
        cx |= reference.get(rx - 1, ry + 1) << 6;
        cx |= reference.get(rx + 1, ry) << 7;
        cx |= reference.get(rx, ry) << 8;
        cx |= reference.get(rx - 1, ry) << 9;
        cx |= reference.get(rx + 1, ry - 1) << 10;
        cx |= reference.get(rx, ry - 1) << 11;
        cx |= reference.get(rx + at[1].0 as i64, ry + at[1].1 as i64) << 12;
        cx as usize
    } else {
        let mut cx = bitmap.get(x - 1, y);
        cx |= bitmap.get(x + 1, y - 1) << 1;
        cx |= bitmap.get(x, y - 1) << 2;
        cx |= bitmap.get(x - 1, y - 1) << 3;
        cx |= reference.get(rx + 1, ry + 1) << 4;
        cx |= reference.get(rx, ry + 1) << 5;
        cx |= reference.get(rx + 1, ry) << 6;
        cx |= reference.get(rx, ry) << 7;
        cx |= reference.get(rx - 1, ry) << 8;
        cx |= reference.get(rx, ry - 1) << 9;
        cx as usize
    }
}

fn typical_prediction(params: &RefinementParams, x: i64, y: i64) -> Option<u32> {
    let reference = params.reference;
    let rx = x - params.dx as i64;
    let ry = y - params.dy as i64;

    let center = reference.get(rx, ry);
    let uniform = [
        reference.get(rx - 1, ry - 1),
        reference.get(rx, ry - 1),
        reference.get(rx + 1, ry - 1),
        reference.get(rx - 1, ry),
        reference.get(rx + 1, ry),
        reference.get(rx - 1, ry + 1),
        reference.get(rx, ry + 1),
        reference.get(rx + 1, ry + 1),
    ]
    .iter()
    .all(|&v| v == center);

    if uniform {
        Some(center)
    } else {
        None
    }
}

pub fn decode_refinement_region(
    arith: &mut ArithDecoder,
    stats: &mut [Cx],
    bitmap: &mut Bitmap,
    params: &RefinementParams,
) {
    let width = bitmap.width as i64;
    let height = bitmap.height as i64;
    let mut ltp = false;

    for y in 0..height {
        if arith.exhausted() {
            log_warn!("JBIG2 refinement region data exhausted at row {} of {}, leaving the rest blank", y, height);
            break;
        }
        if params.tpgron {
            let cx = if params.template != 0 { 0x0040 } else { 0x0100 };
            if let Some(slot) = stats.get_mut(cx) {
                if arith.decode(slot) == 1 {
                    ltp = !ltp;
                }
            }
        }

        for x in 0..width {
            if ltp {
                if let Some(value) = typical_prediction(params, x, y) {
                    bitmap.set(x, y, value);
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
