pub fn half_to_f32(bits: u16) -> f32 {
    let sign = (bits >> 15) as u32 & 1;
    let exponent = (bits >> 10) as u32 & 0x1F;
    let mantissa = bits as u32 & 0x3FF;

    let bits32 = match (exponent, mantissa) {
        (0, 0) => sign << 31,
        (0, m) => {
            let shift = m.leading_zeros() - 21;
            let exponent = 113 - shift;
            let mantissa = (m << shift) & 0x3FF;
            (sign << 31) | (exponent << 23) | (mantissa << 13)
        }
        (31, m) => (sign << 31) | (0xFF << 23) | (m << 13),
        (e, m) => (sign << 31) | ((e + 112) << 23) | (m << 13),
    };
    f32::from_bits(bits32)
}

pub fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x007F_FFFF;

    if exp == 0xFF {
        if mantissa != 0 {
            return sign | 0x7C00 | ((mantissa >> 13) as u16).max(1);
        }
        return sign | 0x7C00;
    }

    let unbiased = exp - 127;
    if unbiased > 15 {
        return sign | 0x7C00;
    }

    if unbiased >= -14 {
        let half_exp = (unbiased + 15) as u32;
        let mut half_mant = mantissa >> 13;
        let round_bits = mantissa & 0x1FFF;
        if round_bits > 0x1000 || (round_bits == 0x1000 && half_mant & 1 == 1) {
            half_mant += 1;
        }
        let combined = (half_exp << 10) + half_mant;
        if combined >= 0x7C00 {
            return sign | 0x7C00;
        }
        return sign | combined as u16;
    }

    if unbiased < -25 {
        return sign;
    }

    let full_mant = mantissa | 0x0080_0000;
    let shift = (-14 - unbiased + 13) as u32;
    let mut half_mant = full_mant >> shift;
    let rem = full_mant & ((1u32 << shift) - 1);
    let halfway = 1u32 << (shift - 1);
    if rem > halfway || (rem == halfway && half_mant & 1 == 1) {
        half_mant += 1;
    }
    sign | half_mant as u16
}
