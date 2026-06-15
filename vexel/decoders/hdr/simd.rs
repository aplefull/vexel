pub fn xyz_to_rgb(src: &[f32], dst: &mut [f32]) {
    debug_assert_eq!(src.len(), dst.len());
    debug_assert_eq!(src.len() % 3, 0);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { xyz_to_rgb_avx2(src, dst) };
    }

    xyz_to_rgb_scalar(src, dst);
}

pub fn xyz_to_rgb_scalar(src: &[f32], dst: &mut [f32]) {
    let num_pixels = src.len() / 3;

    for i in 0..num_pixels {
        let x = src[i * 3];
        let y = src[i * 3 + 1];
        let z = src[i * 3 + 2];

        dst[i * 3] = (3.2404542 * x - 1.5371385 * y - 0.4985314 * z).max(0.0);
        dst[i * 3 + 1] = (-0.9692660 * x + 1.8760108 * y + 0.0415560 * z).max(0.0);
        dst[i * 3 + 2] = (0.0556434 * x - 0.2040259 * y + 1.0572252 * z).max(0.0);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn xyz_to_rgb_avx2(src: &[f32], dst: &mut [f32]) {
    use std::arch::x86_64::*;

    let num_pixels = src.len() / 3;
    let mut i = 0;

    let m00 = _mm256_set1_ps(3.2404542);
    let m01 = _mm256_set1_ps(-1.5371385);
    let m02 = _mm256_set1_ps(-0.4985314);
    let m10 = _mm256_set1_ps(-0.9692660);
    let m11 = _mm256_set1_ps(1.8760108);
    let m12 = _mm256_set1_ps(0.0415560);
    let m20 = _mm256_set1_ps(0.0556434);
    let m21 = _mm256_set1_ps(-0.2040259);
    let m22 = _mm256_set1_ps(1.0572252);

    let zero = _mm256_setzero_ps();

    while i + 8 <= num_pixels {
        let base = i * 3;
        let r0 = _mm256_loadu_ps(src.as_ptr().add(base));
        let r1 = _mm256_loadu_ps(src.as_ptr().add(base + 8));
        let r2 = _mm256_loadu_ps(src.as_ptr().add(base + 16));

        let xp0 = _mm256_permutevar8x32_ps(r0, _mm256_set_epi32(0, 0, 0, 0, 0, 6, 3, 0));
        let xp1 = _mm256_permutevar8x32_ps(r1, _mm256_set_epi32(0, 0, 7, 4, 1, 0, 0, 0));
        let xp2 = _mm256_permutevar8x32_ps(r2, _mm256_set_epi32(5, 2, 0, 0, 0, 0, 0, 0));
        let x = _mm256_blend_ps(_mm256_blend_ps(xp0, xp1, 0x38), xp2, 0xC0);

        let yp0 = _mm256_permutevar8x32_ps(r0, _mm256_set_epi32(0, 0, 0, 0, 0, 7, 4, 1));
        let yp1 = _mm256_permutevar8x32_ps(r1, _mm256_set_epi32(0, 0, 0, 5, 2, 0, 0, 0));
        let yp2 = _mm256_permutevar8x32_ps(r2, _mm256_set_epi32(6, 3, 0, 0, 0, 0, 0, 0));
        let y = _mm256_blend_ps(_mm256_blend_ps(yp0, yp1, 0x18), yp2, 0xE0);

        let zp0 = _mm256_permutevar8x32_ps(r0, _mm256_set_epi32(0, 0, 0, 0, 0, 0, 5, 2));
        let zp1 = _mm256_permutevar8x32_ps(r1, _mm256_set_epi32(0, 0, 0, 6, 3, 0, 0, 0));
        let zp2 = _mm256_permutevar8x32_ps(r2, _mm256_set_epi32(7, 4, 1, 0, 0, 0, 0, 0));
        let z = _mm256_blend_ps(_mm256_blend_ps(zp0, zp1, 0x1C), zp2, 0xE0);

        let r_out = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(m00, x), _mm256_mul_ps(m01, y)),
            _mm256_mul_ps(m02, z),
        );
        let g_out = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(m10, x), _mm256_mul_ps(m11, y)),
            _mm256_mul_ps(m12, z),
        );
        let b_out = _mm256_add_ps(
            _mm256_add_ps(_mm256_mul_ps(m20, x), _mm256_mul_ps(m21, y)),
            _mm256_mul_ps(m22, z),
        );

        let r_c = _mm256_max_ps(r_out, zero);
        let g_c = _mm256_max_ps(g_out, zero);
        let b_c = _mm256_max_ps(b_out, zero);

        let rp0 = _mm256_permutevar8x32_ps(r_c, _mm256_set_epi32(0, 2, 0, 0, 1, 0, 0, 0));
        let gp0 = _mm256_permutevar8x32_ps(g_c, _mm256_set_epi32(2, 0, 0, 1, 0, 0, 0, 0));
        let bp0 = _mm256_permutevar8x32_ps(b_c, _mm256_set_epi32(0, 0, 1, 0, 0, 0, 0, 0));
        let out0 = _mm256_blend_ps(_mm256_blend_ps(rp0, gp0, 0x92), bp0, 0x24);

        let rp1 = _mm256_permutevar8x32_ps(r_c, _mm256_set_epi32(5, 0, 0, 4, 0, 0, 3, 0));
        let gp1 = _mm256_permutevar8x32_ps(g_c, _mm256_set_epi32(0, 0, 4, 0, 3, 0, 0, 0));
        let bp1 = _mm256_permutevar8x32_ps(b_c, _mm256_set_epi32(0, 4, 0, 0, 3, 0, 0, 2));
        let out1 = _mm256_blend_ps(_mm256_blend_ps(bp1, rp1, 0x92), gp1, 0x24);

        let rp2 = _mm256_permutevar8x32_ps(r_c, _mm256_set_epi32(0, 0, 7, 0, 6, 0, 0, 0));
        let gp2 = _mm256_permutevar8x32_ps(g_c, _mm256_set_epi32(0, 7, 0, 0, 6, 0, 0, 5));
        let bp2 = _mm256_permutevar8x32_ps(b_c, _mm256_set_epi32(7, 0, 0, 6, 0, 5, 0, 0));
        let out2 = _mm256_blend_ps(_mm256_blend_ps(gp2, rp2, 0x24), bp2, 0x92);

        let out_base = i * 3;
        _mm256_storeu_ps(dst.as_mut_ptr().add(out_base), out0);
        _mm256_storeu_ps(dst.as_mut_ptr().add(out_base + 8), out1);
        _mm256_storeu_ps(dst.as_mut_ptr().add(out_base + 16), out2);

        i += 8;
    }

    if i < num_pixels {
        xyz_to_rgb_scalar(&src[i * 3..], &mut dst[i * 3..]);
    }
}
