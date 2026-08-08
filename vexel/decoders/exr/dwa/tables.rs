use std::sync::OnceLock;

use crate::decoders::exr::compression::f32_to_half;
use crate::utils::tiff::color::half_to_f32;

pub const ZIGZAG_TO_RASTER: [usize; 64] = [
    0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11, 18, 24, 31, 40, 44,
    53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34, 37, 47, 50, 56, 59, 61, 35, 36, 48, 49,
    57, 58, 62, 63,
];

const GAMMA: f32 = 2.2;
const LOG_BASE: f32 = 9.025_013;

fn to_linear(bits: u16) -> u16 {
    if bits == 0 || bits & 0x7C00 == 0x7C00 {
        return 0;
    }

    let f = half_to_f32(bits);
    let sign = if f < 0.0 { -1.0f32 } else { 1.0f32 };
    let magnitude = f.abs();

    let (base, exponent) = if magnitude <= 1.0 { (magnitude, GAMMA) } else { (LOG_BASE, magnitude - 1.0) };

    f32_to_half(sign * base.powf(exponent))
}

pub fn to_linear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<Box<[u16; 65536]>> = OnceLock::new();

    TABLE.get_or_init(|| {
        let mut table = Box::new([0u16; 65536]);
        for (i, entry) in table.iter_mut().enumerate() {
            *entry = to_linear(i as u16);
        }
        table
    })
}
