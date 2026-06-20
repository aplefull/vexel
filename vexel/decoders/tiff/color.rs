use crate::utils::types::ByteOrder;

pub fn u16_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> u16 {
    match byte_order {
        ByteOrder::LittleEndian => u16::from_le_bytes([bytes[0], bytes[1]]),
        ByteOrder::BigEndian => u16::from_be_bytes([bytes[0], bytes[1]]),
    }
}

pub fn u32_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> u32 {
    match byte_order {
        ByteOrder::LittleEndian => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        ByteOrder::BigEndian => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}

pub fn f32_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> f32 {
    let bits = u32_from_bytes(bytes, byte_order);
    f32::from_bits(bits)
}

pub fn float24_to_f32(bytes: &[u8], byte_order: ByteOrder) -> f32 {
    let (b0, b1, b2) = match byte_order {
        ByteOrder::LittleEndian => (bytes[0], bytes[1], bytes[2]),
        ByteOrder::BigEndian => (bytes[2], bytes[1], bytes[0]),
    };

    if (b0 | b1 | b2) == 0 {
        return 0.0;
    }

    let sign_bit = b2 & 0x80;
    let exponent = b2 & 0x7f;
    let exponent_f32 = exponent.wrapping_sub(63).wrapping_add(127);

    let q3 = sign_bit | (exponent_f32 >> 1);
    let q2 = ((exponent_f32 & 1) << 7) | ((b1 & 0xfe) >> 1);
    let q1 = ((b1 & 0x01) << 7) | ((b0 & 0xfe) >> 1);
    let q0 = (b0 & 0x01) << 7;

    f32::from_bits(u32::from_le_bytes([q0, q1, q2, q3]))
}

pub fn f64_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> f64 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => {
            u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
        }
        ByteOrder::BigEndian => {
            u64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
        }
    };
    f64::from_bits(bits)
}

pub fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) as u32) << 31;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mantissa = (bits & 0x3FF) as u32;

    let f32_bits = if exp == 0 {
        if mantissa == 0 {
            sign
        } else {
            let mut m = mantissa;
            let mut e = 127u32 - 14;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            sign | (e << 23) | (m << 13)
        }
    } else if exp == 31 {
        sign | 0x7F800000 | (mantissa << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (mantissa << 13)
    };

    f32::from_bits(f32_bits)
}

pub fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

pub fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> (u8, u8, u8) {
    let c = c as f32 / 255.0;
    let m = m as f32 / 255.0;
    let y = y as f32 / 255.0;
    let k = k as f32 / 255.0;

    let r = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
    let g = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
    let b = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;

    (r, g, b)
}

pub struct YCbCrTables {
    pub y_tab: [i32; 256],
    pub cr_r_tab: [i32; 256],
    pub cb_b_tab: [i32; 256],
    pub cr_g_tab: [i32; 256],
    pub cb_g_tab: [i32; 256],
}

impl YCbCrTables {
    pub fn new(luma: [f32; 3], ref_black_white: [f32; 6]) -> Self {
        const SHIFT: u32 = 16;
        const ONE_HALF: i32 = 1 << (SHIFT - 1);

        fn fix(x: f32) -> i32 {
            (x as f64 * (1i64 << SHIFT) as f64 + 0.5) as i32
        }

        fn code2v(c: f32, rb: f32, rw: f32, cr: f32) -> f32 {
            let range = rw - rb;
            if range.abs() < f32::EPSILON {
                0.0
            } else {
                (c - rb) * cr / range
            }
        }

        let luma_red = luma[0];
        let luma_green = luma[1];
        let luma_blue = luma[2];

        let f1 = 2.0 - 2.0 * luma_red;
        let d1 = fix(f1.clamp(0.0, 2.0));
        let f2 = luma_red * f1 / luma_green;
        let d2 = -fix(f2.clamp(0.0, 2.0));
        let f3 = 2.0 - 2.0 * luma_blue;
        let d3 = fix(f3.clamp(0.0, 2.0));
        let f4 = luma_blue * f3 / luma_green;
        let d4 = -fix(f4.clamp(0.0, 2.0));

        let mut y_tab = [0i32; 256];
        let mut cr_r_tab = [0i32; 256];
        let mut cb_b_tab = [0i32; 256];
        let mut cr_g_tab = [0i32; 256];
        let mut cb_g_tab = [0i32; 256];

        for i in 0..256i32 {
            let x = i - 128;

            let cr = code2v(
                x as f32,
                ref_black_white[4] - 128.0,
                ref_black_white[5] - 128.0,
                127.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0) as i32;

            let cb = code2v(
                x as f32,
                ref_black_white[2] - 128.0,
                ref_black_white[3] - 128.0,
                127.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0) as i32;

            cr_r_tab[i as usize] = (d1 * cr + ONE_HALF) >> SHIFT;
            cb_b_tab[i as usize] = (d3 * cb + ONE_HALF) >> SHIFT;
            cr_g_tab[i as usize] = d2 * cr;
            cb_g_tab[i as usize] = d4 * cb + ONE_HALF;

            let y_val = code2v(
                (x + 128) as f32,
                ref_black_white[0],
                ref_black_white[1],
                255.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0);
            y_tab[i as usize] = y_val as i32;
        }

        Self {
            y_tab,
            cr_r_tab,
            cb_b_tab,
            cr_g_tab,
            cb_g_tab,
        }
    }

    pub fn convert(&self, y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
        let y_idx = y as usize;
        let cb_idx = cb as usize;
        let cr_idx = cr as usize;

        let r = self.y_tab[y_idx] + self.cr_r_tab[cr_idx];
        let g = self.y_tab[y_idx] + ((self.cb_g_tab[cb_idx] + self.cr_g_tab[cr_idx]) >> 16);
        let b = self.y_tab[y_idx] + self.cb_b_tab[cb_idx];

        (clamp_u8(r), clamp_u8(g), clamp_u8(b))
    }
}

pub const SRGB_MATRIX: [[f32; 3]; 3] = [
    [3.2410, -1.5374, -0.4986],
    [-0.9692, 1.8760, 0.0416],
    [0.0556, -0.2040, 1.0570],
];

pub const D50_WHITE: [f32; 3] = [0.9642, 1.0000, 0.8251];
pub const D65_WHITE: [f32; 3] = [0.9505, 1.0000, 1.0890];

pub fn lab_to_xyz(l: f32, a: f32, b: f32, white: [f32; 3]) -> (f32, f32, f32) {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;

    const DELTA: f32 = 6.0 / 29.0;
    const DELTA2: f32 = DELTA * DELTA;

    let x = if fx > DELTA { fx * fx * fx } else { 3.0 * DELTA2 * (fx - 4.0 / 29.0) };
    let y = if fy > DELTA { fy * fy * fy } else { 3.0 * DELTA2 * (fy - 4.0 / 29.0) };
    let z = if fz > DELTA { fz * fz * fz } else { 3.0 * DELTA2 * (fz - 4.0 / 29.0) };

    (x * white[0], y * white[1], z * white[2])
}

pub fn xyz_to_srgb(x: f32, y: f32, z: f32) -> (u8, u8, u8) {
    let m = &SRGB_MATRIX;
    let r_lin = m[0][0] * x + m[0][1] * y + m[0][2] * z;
    let g_lin = m[1][0] * x + m[1][1] * y + m[1][2] * z;
    let b_lin = m[2][0] * x + m[2][1] * y + m[2][2] * z;

    fn linear_to_srgb(v: f32) -> u8 {
        let v = v.clamp(0.0, 1.0);
        let srgb = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    }

    (linear_to_srgb(r_lin), linear_to_srgb(g_lin), linear_to_srgb(b_lin))
}

pub fn xyz_to_srgb_f32(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let m = &SRGB_MATRIX;
    let r_lin = m[0][0] * x + m[0][1] * y + m[0][2] * z;
    let g_lin = m[1][0] * x + m[1][1] * y + m[1][2] * z;
    let b_lin = m[2][0] * x + m[2][1] * y + m[2][2] * z;

    fn linear_to_srgb_f32(v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    }

    (linear_to_srgb_f32(r_lin), linear_to_srgb_f32(g_lin), linear_to_srgb_f32(b_lin))
}

pub fn cmyk_to_rgb_f32(c: f32, m: f32, y: f32, k: f32) -> (f32, f32, f32) {
    ((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

pub fn cielab_to_rgb(l_raw: u8, a_raw: i8, b_raw: i8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32;
    let b = b_raw as f32;
    let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
    xyz_to_srgb(x, y, z)
}

pub fn icclab_to_rgb(l_raw: u8, a_raw: u8, b_raw: u8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32 - 128.0;
    let b = b_raw as f32 - 128.0;
    let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
    xyz_to_srgb(x, y, z)
}

pub fn itulab_to_rgb(l_raw: u8, a_raw: u8, b_raw: u8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32 - 128.0;
    let b = b_raw as f32 - 128.0;
    let (x, y, z) = lab_to_xyz(l, a, b, D65_WHITE);
    xyz_to_srgb(x, y, z)
}

const UVSCALE: f64 = 410.0;

fn logl16_to_y(p16: u32) -> f64 {
    let le = p16 & 0x7fff;
    if le == 0 {
        return 0.0;
    }
    let y = (std::f64::consts::LN_2 / 256.0 * (le as f64 + 0.5) - std::f64::consts::LN_2 * 64.0).exp();
    if p16 & 0x8000 != 0 { -y } else { y }
}

fn logluv_gamma(c: f64) -> u8 {
    if c <= 0.0 { 0 } else if c >= 1.0 { 255 } else { (256.0 * c.sqrt()) as u8 }
}

pub fn logluv32_to_rgb(p: u32) -> (u8, u8, u8) {
    let l = logl16_to_y(p >> 16);
    if l <= 0.0 {
        return (0, 0, 0);
    }
    let u = (((p >> 8) & 0xff) as f64 + 0.5) / UVSCALE;
    let v = ((p & 0xff) as f64 + 0.5) / UVSCALE;
    let s = 1.0 / (6.0 * u - 16.0 * v + 12.0);
    let x = 9.0 * u * s;
    let yc = 4.0 * v * s;
    let xyz_x = x / yc * l;
    let xyz_z = (1.0 - x - yc) / yc * l;
    let r = 2.690 * xyz_x - 1.276 * l - 0.414 * xyz_z;
    let g = -1.022 * xyz_x + 1.978 * l + 0.044 * xyz_z;
    let b = 0.061 * xyz_x - 0.224 * l + 1.163 * xyz_z;
    (logluv_gamma(r), logluv_gamma(g), logluv_gamma(b))
}
