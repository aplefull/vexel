const A: f32 = 0.353_553_62;
const B: f32 = 0.490_392_66;
const C: f32 = 0.461_939_84;
const D: f32 = 0.415_734_95;
const E: f32 = 0.277_785_45;
const F: f32 = 0.191_342_16;
const G: f32 = 0.097_545_706;

const DC_SCALE: f32 = 3.535_536e-1;

pub fn inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let value = data[0] * DC_SCALE * DC_SCALE;
    data.fill(value);
}

fn butterfly(s0: f32, s1: f32, s2: f32, s3: f32, s4: f32, s5: f32, s6: f32, s7: f32) -> [f32; 8] {
    let alpha = [C * s2, F * s2, C * s6, F * s6];

    let beta = [
        B * s1 + D * s3 + E * s5 + G * s7,
        D * s1 - G * s3 - B * s5 - E * s7,
        E * s1 - B * s3 + G * s5 + D * s7,
        G * s1 - E * s3 + D * s5 - B * s7,
    ];

    let theta = [A * (s0 + s4), alpha[0] + alpha[3], alpha[1] - alpha[2], A * (s0 - s4)];

    let gamma = [theta[0] + theta[1], theta[3] + theta[2], theta[3] - theta[2], theta[0] - theta[1]];

    [
        gamma[0] + beta[0],
        gamma[1] + beta[1],
        gamma[2] + beta[2],
        gamma[3] + beta[3],
        gamma[3] - beta[3],
        gamma[2] - beta[2],
        gamma[1] - beta[1],
        gamma[0] - beta[0],
    ]
}

pub fn inverse_8x8(data: &mut [f32; 64], zeroed_rows: usize) {
    for row in 0..8 - zeroed_rows.min(8) {
        let o = row * 8;
        let r = butterfly(
            data[o],
            data[o + 1],
            data[o + 2],
            data[o + 3],
            data[o + 4],
            data[o + 5],
            data[o + 6],
            data[o + 7],
        );
        data[o..o + 8].copy_from_slice(&r);
    }

    for column in 0..8 {
        let r = butterfly(
            data[column],
            data[8 + column],
            data[16 + column],
            data[24 + column],
            data[32 + column],
            data[40 + column],
            data[48 + column],
            data[56 + column],
        );
        for (i, &v) in r.iter().enumerate() {
            data[i * 8 + column] = v;
        }
    }
}

pub fn zeroed_rows_for(last_non_zero: usize) -> usize {
    match last_non_zero {
        0..=1 => 7,
        2 => 6,
        3..=8 => 5,
        9 => 4,
        10..=19 => 3,
        20 => 2,
        21..=34 => 1,
        _ => 0,
    }
}

pub fn csc709_inverse(y: f32, cb: f32, cr: f32) -> (f32, f32, f32) {
    (y + 1.5747 * cr, y - 0.1873 * cb - 0.4682 * cr, y + 1.8556 * cb)
}

pub fn csc709_inverse_64(comp0: &mut [f32; 64], comp1: &mut [f32; 64], comp2: &mut [f32; 64]) {
    for i in 0..64 {
        let (r, g, b) = csc709_inverse(comp0[i], comp1[i], comp2[i]);
        comp0[i] = r;
        comp1[i] = g;
        comp2[i] = b;
    }
}
