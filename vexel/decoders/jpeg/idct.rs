const S: [f32; 8] = [
    0.35355339059327373,
    0.4903926402016152,
    0.4619397662556434,
    0.4157348061512726,
    0.3535533905932737,
    0.2777851165098011,
    0.19134171618254492,
    0.09754516100806417,
];

const M1: f32 = 1.4142135623730951;
const M2: f32 = 1.0823922002923940;
const M3: f32 = 1.4142135623730951;
const M4: f32 = 2.6131259297527580;
const M5: f32 = 0.7653668647301796;

pub fn dequantize_and_idct(blocks: &mut [i32], quant: &[u16], level_shift: i32) {
    let scaled = precompute_scaled_quant(quant);

    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return dequantize_and_idct_avx2(blocks, &scaled, level_shift);
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    return dequantize_and_idct_wasm(blocks, &scaled, level_shift);

    dequantize_and_idct_scalar(blocks, &scaled, level_shift);
}

fn precompute_scaled_quant(quant: &[u16]) -> [f32; 64] {
    let mut out = [0.0f32; 64];
    for row in 0..8 {
        for col in 0..8 {
            out[row * 8 + col] = quant[row * 8 + col] as f32 * S[row];
        }
    }
    out
}

// ─── Scalar ───────────────────────────────────────────────────

#[inline(always)]
fn aan_butterfly(
    g0: f32, g1: f32, g2: f32, g3: f32,
    g4: f32, g5: f32, g6: f32, g7: f32,
) -> (f32, f32, f32, f32, f32, f32, f32, f32) {
    let f4 = g4 - g7;
    let f5 = g5 + g6;
    let f6 = g5 - g6;
    let f7 = g4 + g7;
    let e2 = g2 - g3;
    let e3 = g2 + g3;
    let e5 = f5 - f7;
    let e7 = f5 + f7;
    let e8 = f4 + f6;
    let d2 = e2 * M1;
    let d4 = f4 * M2;
    let d5 = e5 * M3;
    let d6 = f6 * M4;
    let d8 = e8 * M5;
    let c0 = g0 + g1;
    let c1 = g0 - g1;
    let c2 = d2 - e3;
    let c4 = d4 + d8;
    let c5 = d5 + e7;
    let c6 = d6 - d8;
    let c8 = c5 - c6;
    let b0 = c0 + e3;
    let b1 = c1 + c2;
    let b2 = c1 - c2;
    let b3 = c0 - e3;
    let b4 = c4 - c8;
    let b6 = c6 - e7;
    (b0 + e7, b1 + b6, b2 + c8, b3 + b4, b3 - b4, b2 - c8, b1 - b6, b0 - e7)
}

fn dequantize_and_idct_scalar(blocks: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    let n = blocks.len() / 64;
    for i in 0..n {
        idct_block_precomputed(&mut blocks[i * 64..(i + 1) * 64], scaled_quant, level_shift);
    }
}

fn idct_block_precomputed(block: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    let mut temp = [0.0f32; 64];

    for col in 0..8 {
        let g0 = block[0 * 8 + col] as f32 * scaled_quant[0 * 8 + col];
        let g1 = block[4 * 8 + col] as f32 * scaled_quant[4 * 8 + col];
        let g2 = block[2 * 8 + col] as f32 * scaled_quant[2 * 8 + col];
        let g3 = block[6 * 8 + col] as f32 * scaled_quant[6 * 8 + col];
        let g4 = block[5 * 8 + col] as f32 * scaled_quant[5 * 8 + col];
        let g5 = block[1 * 8 + col] as f32 * scaled_quant[1 * 8 + col];
        let g6 = block[7 * 8 + col] as f32 * scaled_quant[7 * 8 + col];
        let g7 = block[3 * 8 + col] as f32 * scaled_quant[3 * 8 + col];
        let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly(g0, g1, g2, g3, g4, g5, g6, g7);
        temp[0 * 8 + col] = r0;
        temp[1 * 8 + col] = r1;
        temp[2 * 8 + col] = r2;
        temp[3 * 8 + col] = r3;
        temp[4 * 8 + col] = r4;
        temp[5 * 8 + col] = r5;
        temp[6 * 8 + col] = r6;
        temp[7 * 8 + col] = r7;
    }

    for row in 0..8 {
        let g0 = temp[row * 8 + 0] * S[0];
        let g1 = temp[row * 8 + 4] * S[4];
        let g2 = temp[row * 8 + 2] * S[2];
        let g3 = temp[row * 8 + 6] * S[6];
        let g4 = temp[row * 8 + 5] * S[5];
        let g5 = temp[row * 8 + 1] * S[1];
        let g6 = temp[row * 8 + 7] * S[7];
        let g7 = temp[row * 8 + 3] * S[3];
        let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly(g0, g1, g2, g3, g4, g5, g6, g7);
        block[row * 8 + 0] = (r0.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 1] = (r1.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 2] = (r2.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 3] = (r3.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 4] = (r4.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 5] = (r5.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 6] = (r6.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
        block[row * 8 + 7] = (r7.round() as i32).clamp(-level_shift, level_shift * 2 - 1);
    }
}

// ─── AVX2 ─────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
fn dequantize_and_idct_avx2(blocks: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    let n = blocks.len() / 64;
    let mut i = 0;
    while i + 8 <= n {
        unsafe { idct_8blocks_avx2(&mut blocks[i * 64..(i + 8) * 64], scaled_quant, level_shift) };
        i += 8;
    }
    while i < n {
        idct_block_precomputed(&mut blocks[i * 64..(i + 1) * 64], scaled_quant, level_shift);
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn idct_8blocks_avx2(blocks: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    use std::arch::x86_64::*;

    let mut staging = [0.0f32; 512];

    for row in 0..8usize {
        let mut rows = [_mm256_setzero_ps(); 8];
        let vsq = _mm256_loadu_ps(scaled_quant.as_ptr().add(row * 8));

        for b in 0..8usize {
            let vi32 = _mm256_loadu_si256(blocks.as_ptr().add(b * 64 + row * 8) as *const __m256i);
            rows[b] = _mm256_mul_ps(_mm256_cvtepi32_ps(vi32), vsq);
        }

        let t = transpose8x8_ps(rows);
        for col in 0..8usize {
            _mm256_storeu_ps(staging.as_mut_ptr().add((row * 8 + col) * 8), t[col]);
        }
    }

    let mut temp = [0.0f32; 512];

    for col in 0..8usize {
        macro_rules! load_row {
            ($r:expr) => { _mm256_loadu_ps(staging.as_ptr().add(($r * 8 + col) * 8)) };
        }
        let g0 = load_row!(0);
        let g1 = load_row!(4);
        let g2 = load_row!(2);
        let g3 = load_row!(6);
        let g4 = load_row!(5);
        let g5 = load_row!(1);
        let g6 = load_row!(7);
        let g7 = load_row!(3);

        let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly_avx2(g0, g1, g2, g3, g4, g5, g6, g7);

        macro_rules! store_row {
            ($r:expr, $v:expr) => { _mm256_storeu_ps(temp.as_mut_ptr().add(($r * 8 + col) * 8), $v) };
        }
        store_row!(0, r0);
        store_row!(1, r1);
        store_row!(2, r2);
        store_row!(3, r3);
        store_row!(4, r4);
        store_row!(5, r5);
        store_row!(6, r6);
        store_row!(7, r7);
    }

    let vs = [
        _mm256_set1_ps(S[0]), _mm256_set1_ps(S[1]), _mm256_set1_ps(S[2]),
        _mm256_set1_ps(S[3]), _mm256_set1_ps(S[4]), _mm256_set1_ps(S[5]),
        _mm256_set1_ps(S[6]), _mm256_set1_ps(S[7]),
    ];

    let vhalf     = _mm256_set1_ps(0.5f32);
    let vsign_bit = _mm256_set1_ps(-0.0f32);
    let vmin      = _mm256_set1_ps(-level_shift as f32);
    let vmax      = _mm256_set1_ps((level_shift * 2 - 1) as f32);

    macro_rules! round_clamp_to_i32 {
        ($v:expr) => {{
            let sign = _mm256_and_ps($v, vsign_bit);
            let half = _mm256_or_ps(vhalf, sign);
            let r    = _mm256_add_ps($v, half);
            let c    = _mm256_min_ps(_mm256_max_ps(r, vmin), vmax);
            _mm256_cvttps_epi32(c)
        }};
    }

    for row in 0..8usize {
        macro_rules! load_col {
            ($c:expr) => { _mm256_loadu_ps(temp.as_ptr().add((row * 8 + $c) * 8)) };
        }
        let g0 = _mm256_mul_ps(load_col!(0), vs[0]);
        let g1 = _mm256_mul_ps(load_col!(4), vs[4]);
        let g2 = _mm256_mul_ps(load_col!(2), vs[2]);
        let g3 = _mm256_mul_ps(load_col!(6), vs[6]);
        let g4 = _mm256_mul_ps(load_col!(5), vs[5]);
        let g5 = _mm256_mul_ps(load_col!(1), vs[1]);
        let g6 = _mm256_mul_ps(load_col!(7), vs[7]);
        let g7 = _mm256_mul_ps(load_col!(3), vs[3]);

        let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly_avx2(g0, g1, g2, g3, g4, g5, g6, g7);

        let out = [
            round_clamp_to_i32!(r0), round_clamp_to_i32!(r1),
            round_clamp_to_i32!(r2), round_clamp_to_i32!(r3),
            round_clamp_to_i32!(r4), round_clamp_to_i32!(r5),
            round_clamp_to_i32!(r6), round_clamp_to_i32!(r7),
        ];

        let t = transpose8x8_epi32(out);
        for b in 0..8usize {
            _mm256_storeu_si256(blocks.as_mut_ptr().add(b * 64 + row * 8) as *mut __m256i, t[b]);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn transpose8x8_ps(rows: [std::arch::x86_64::__m256; 8]) -> [std::arch::x86_64::__m256; 8] {
    use std::arch::x86_64::*;

    let t0 = _mm256_unpacklo_ps(rows[0], rows[1]);
    let t1 = _mm256_unpackhi_ps(rows[0], rows[1]);
    let t2 = _mm256_unpacklo_ps(rows[2], rows[3]);
    let t3 = _mm256_unpackhi_ps(rows[2], rows[3]);
    let t4 = _mm256_unpacklo_ps(rows[4], rows[5]);
    let t5 = _mm256_unpackhi_ps(rows[4], rows[5]);
    let t6 = _mm256_unpacklo_ps(rows[6], rows[7]);
    let t7 = _mm256_unpackhi_ps(rows[6], rows[7]);

    let u0 = _mm256_shuffle_ps::<0x44>(t0, t2);
    let u1 = _mm256_shuffle_ps::<0xEE>(t0, t2);
    let u2 = _mm256_shuffle_ps::<0x44>(t1, t3);
    let u3 = _mm256_shuffle_ps::<0xEE>(t1, t3);
    let u4 = _mm256_shuffle_ps::<0x44>(t4, t6);
    let u5 = _mm256_shuffle_ps::<0xEE>(t4, t6);
    let u6 = _mm256_shuffle_ps::<0x44>(t5, t7);
    let u7 = _mm256_shuffle_ps::<0xEE>(t5, t7);

    [
        _mm256_permute2f128_ps::<0x20>(u0, u4),
        _mm256_permute2f128_ps::<0x20>(u1, u5),
        _mm256_permute2f128_ps::<0x20>(u2, u6),
        _mm256_permute2f128_ps::<0x20>(u3, u7),
        _mm256_permute2f128_ps::<0x31>(u0, u4),
        _mm256_permute2f128_ps::<0x31>(u1, u5),
        _mm256_permute2f128_ps::<0x31>(u2, u6),
        _mm256_permute2f128_ps::<0x31>(u3, u7),
    ]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn transpose8x8_epi32(
    rows: [std::arch::x86_64::__m256i; 8],
) -> [std::arch::x86_64::__m256i; 8] {
    use std::arch::x86_64::*;

    let f: [__m256; 8] = std::mem::transmute(rows);
    let t = transpose8x8_ps(f);
    std::mem::transmute(t)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn aan_butterfly_avx2(
    g0: std::arch::x86_64::__m256,
    g1: std::arch::x86_64::__m256,
    g2: std::arch::x86_64::__m256,
    g3: std::arch::x86_64::__m256,
    g4: std::arch::x86_64::__m256,
    g5: std::arch::x86_64::__m256,
    g6: std::arch::x86_64::__m256,
    g7: std::arch::x86_64::__m256,
) -> (
    std::arch::x86_64::__m256, std::arch::x86_64::__m256,
    std::arch::x86_64::__m256, std::arch::x86_64::__m256,
    std::arch::x86_64::__m256, std::arch::x86_64::__m256,
    std::arch::x86_64::__m256, std::arch::x86_64::__m256,
) {
    use std::arch::x86_64::*;

    let vm1 = _mm256_set1_ps(M1);
    let vm2 = _mm256_set1_ps(M2);
    let vm3 = _mm256_set1_ps(M3);
    let vm4 = _mm256_set1_ps(M4);
    let vm5 = _mm256_set1_ps(M5);

    let f4 = _mm256_sub_ps(g4, g7);
    let f5 = _mm256_add_ps(g5, g6);
    let f6 = _mm256_sub_ps(g5, g6);
    let f7 = _mm256_add_ps(g4, g7);
    let e2 = _mm256_sub_ps(g2, g3);
    let e3 = _mm256_add_ps(g2, g3);
    let e5 = _mm256_sub_ps(f5, f7);
    let e7 = _mm256_add_ps(f5, f7);
    let e8 = _mm256_add_ps(f4, f6);
    let d2 = _mm256_mul_ps(e2, vm1);
    let d4 = _mm256_mul_ps(f4, vm2);
    let d5 = _mm256_mul_ps(e5, vm3);
    let d6 = _mm256_mul_ps(f6, vm4);
    let d8 = _mm256_mul_ps(e8, vm5);
    let c0 = _mm256_add_ps(g0, g1);
    let c1 = _mm256_sub_ps(g0, g1);
    let c2 = _mm256_sub_ps(d2, e3);
    let c4 = _mm256_add_ps(d4, d8);
    let c5 = _mm256_add_ps(d5, e7);
    let c6 = _mm256_sub_ps(d6, d8);
    let c8 = _mm256_sub_ps(c5, c6);
    let b0 = _mm256_add_ps(c0, e3);
    let b1 = _mm256_add_ps(c1, c2);
    let b2 = _mm256_sub_ps(c1, c2);
    let b3 = _mm256_sub_ps(c0, e3);
    let b4 = _mm256_sub_ps(c4, c8);
    let b6 = _mm256_sub_ps(c6, e7);
    (
        _mm256_add_ps(b0, e7), _mm256_add_ps(b1, b6),
        _mm256_add_ps(b2, c8), _mm256_add_ps(b3, b4),
        _mm256_sub_ps(b3, b4), _mm256_sub_ps(b2, c8),
        _mm256_sub_ps(b1, b6), _mm256_sub_ps(b0, e7),
    )
}

// ─── WASM SIMD128 ─────────────────────────────────────────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn dequantize_and_idct_wasm(blocks: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    let n = blocks.len() / 64;
    let mut i = 0;
    while i + 4 <= n {
        idct_4blocks_wasm(&mut blocks[i * 64..(i + 4) * 64], scaled_quant, level_shift);
        i += 4;
    }
    while i < n {
        idct_block_precomputed(&mut blocks[i * 64..(i + 1) * 64], scaled_quant, level_shift);
        i += 1;
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn idct_4blocks_wasm(blocks: &mut [i32], scaled_quant: &[f32; 64], level_shift: i32) {
    use std::arch::wasm32::*;

    let mut staging = [0.0f32; 256];

    for row in 0..8usize {
        let sq_ptr = scaled_quant.as_ptr().add(row * 8);
        let vsq0 = v128_load(sq_ptr as *const v128);
        let vsq1 = v128_load(sq_ptr.add(4) as *const v128);

        for b in 0..4usize {
            unsafe {
                let src_ptr = blocks.as_ptr().add(b * 64 + row * 8);
                let vi0 = v128_load(src_ptr as *const v128);
                let vi1 = v128_load(src_ptr.add(4) as *const v128);
                let vf0 = f32x4_mul(f32x4_convert_i32x4(vi0), vsq0);
                let vf1 = f32x4_mul(f32x4_convert_i32x4(vi1), vsq1);
                let dst_ptr = staging.as_mut_ptr().add((row * 4 + b) * 8);
                v128_store(dst_ptr as *mut v128, vf0);
                v128_store(dst_ptr.add(4) as *mut v128, vf1);
            }
        }
    }

    let mut temp = [0.0f32; 256];

    for col in 0..8usize {
        unsafe {
            macro_rules! load_row4 {
                ($r:expr) => {{
                    let base = ($r * 4) * 8 + col;
                    f32x4(
                        *staging.get_unchecked(base),
                        *staging.get_unchecked(base + 8),
                        *staging.get_unchecked(base + 16),
                        *staging.get_unchecked(base + 24),
                    )
                }};
            }

            let g0 = load_row4!(0);
            let g1 = load_row4!(4);
            let g2 = load_row4!(2);
            let g3 = load_row4!(6);
            let g4 = load_row4!(5);
            let g5 = load_row4!(1);
            let g6 = load_row4!(7);
            let g7 = load_row4!(3);

            let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly_wasm(g0, g1, g2, g3, g4, g5, g6, g7);

            macro_rules! store_row4 {
                ($r:expr, $v:expr) => {{
                    let base = ($r * 4) * 8 + col;
                    *temp.get_unchecked_mut(base)      = f32x4_extract_lane::<0>($v);
                    *temp.get_unchecked_mut(base + 8)  = f32x4_extract_lane::<1>($v);
                    *temp.get_unchecked_mut(base + 16) = f32x4_extract_lane::<2>($v);
                    *temp.get_unchecked_mut(base + 24) = f32x4_extract_lane::<3>($v);
                }};
            }

            store_row4!(0, r0);
            store_row4!(1, r1);
            store_row4!(2, r2);
            store_row4!(3, r3);
            store_row4!(4, r4);
            store_row4!(5, r5);
            store_row4!(6, r6);
            store_row4!(7, r7);
        }
    }

    let vs = [
        f32x4_splat(S[0]), f32x4_splat(S[1]), f32x4_splat(S[2]), f32x4_splat(S[3]),
        f32x4_splat(S[4]), f32x4_splat(S[5]), f32x4_splat(S[6]), f32x4_splat(S[7]),
    ];

    let vhalf     = f32x4_splat(0.5f32);
    let vsign_bit = f32x4_splat(-0.0f32);
    let vmin      = f32x4_splat(-level_shift as f32);
    let vmax      = f32x4_splat((level_shift * 2 - 1) as f32);

    macro_rules! round_clamp_to_i32_wasm {
        ($v:expr) => {{
            let sign = v128_and($v, vsign_bit);
            let half = v128_or(vhalf, sign);
            let r    = f32x4_add($v, half);
            let c    = f32x4_min(f32x4_max(r, vmin), vmax);
            i32x4_trunc_sat_f32x4(c)
        }};
    }

    for row in 0..8usize {
        unsafe {
            macro_rules! load_col4 {
                ($c:expr) => {{
                    let base = (row * 4) * 8 + $c;
                    f32x4(
                        *temp.get_unchecked(base),
                        *temp.get_unchecked(base + 8),
                        *temp.get_unchecked(base + 16),
                        *temp.get_unchecked(base + 24),
                    )
                }};
            }

            let g0 = f32x4_mul(load_col4!(0), vs[0]);
            let g1 = f32x4_mul(load_col4!(4), vs[4]);
            let g2 = f32x4_mul(load_col4!(2), vs[2]);
            let g3 = f32x4_mul(load_col4!(6), vs[6]);
            let g4 = f32x4_mul(load_col4!(5), vs[5]);
            let g5 = f32x4_mul(load_col4!(1), vs[1]);
            let g6 = f32x4_mul(load_col4!(7), vs[7]);
            let g7 = f32x4_mul(load_col4!(3), vs[3]);

            let (r0, r1, r2, r3, r4, r5, r6, r7) = aan_butterfly_wasm(g0, g1, g2, g3, g4, g5, g6, g7);

            let out = [
                round_clamp_to_i32_wasm!(r0), round_clamp_to_i32_wasm!(r1),
                round_clamp_to_i32_wasm!(r2), round_clamp_to_i32_wasm!(r3),
                round_clamp_to_i32_wasm!(r4), round_clamp_to_i32_wasm!(r5),
                round_clamp_to_i32_wasm!(r6), round_clamp_to_i32_wasm!(r7),
            ];

            for b in 0..4usize {
                macro_rules! extract {
                    ($v:expr) => {
                        match b {
                            0 => i32x4_extract_lane::<0>($v),
                            1 => i32x4_extract_lane::<1>($v),
                            2 => i32x4_extract_lane::<2>($v),
                            _ => i32x4_extract_lane::<3>($v),
                        }
                    };
                }
                for c in 0..8usize {
                    *blocks.get_unchecked_mut(b * 64 + row * 8 + c) = extract!(out[c]);
                }
            }
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline]
fn aan_butterfly_wasm(
    g0: std::arch::wasm32::v128,
    g1: std::arch::wasm32::v128,
    g2: std::arch::wasm32::v128,
    g3: std::arch::wasm32::v128,
    g4: std::arch::wasm32::v128,
    g5: std::arch::wasm32::v128,
    g6: std::arch::wasm32::v128,
    g7: std::arch::wasm32::v128,
) -> (
    std::arch::wasm32::v128, std::arch::wasm32::v128,
    std::arch::wasm32::v128, std::arch::wasm32::v128,
    std::arch::wasm32::v128, std::arch::wasm32::v128,
    std::arch::wasm32::v128, std::arch::wasm32::v128,
) {
    use std::arch::wasm32::*;

    let vm1 = f32x4_splat(M1);
    let vm2 = f32x4_splat(M2);
    let vm3 = f32x4_splat(M3);
    let vm4 = f32x4_splat(M4);
    let vm5 = f32x4_splat(M5);

    let f4 = f32x4_sub(g4, g7);
    let f5 = f32x4_add(g5, g6);
    let f6 = f32x4_sub(g5, g6);
    let f7 = f32x4_add(g4, g7);
    let e2 = f32x4_sub(g2, g3);
    let e3 = f32x4_add(g2, g3);
    let e5 = f32x4_sub(f5, f7);
    let e7 = f32x4_add(f5, f7);
    let e8 = f32x4_add(f4, f6);
    let d2 = f32x4_mul(e2, vm1);
    let d4 = f32x4_mul(f4, vm2);
    let d5 = f32x4_mul(e5, vm3);
    let d6 = f32x4_mul(f6, vm4);
    let d8 = f32x4_mul(e8, vm5);
    let c0 = f32x4_add(g0, g1);
    let c1 = f32x4_sub(g0, g1);
    let c2 = f32x4_sub(d2, e3);
    let c4 = f32x4_add(d4, d8);
    let c5 = f32x4_add(d5, e7);
    let c6 = f32x4_sub(d6, d8);
    let c8 = f32x4_sub(c5, c6);
    let b0 = f32x4_add(c0, e3);
    let b1 = f32x4_add(c1, c2);
    let b2 = f32x4_sub(c1, c2);
    let b3 = f32x4_sub(c0, e3);
    let b4 = f32x4_sub(c4, c8);
    let b6 = f32x4_sub(c6, e7);
    (
        f32x4_add(b0, e7), f32x4_add(b1, b6),
        f32x4_add(b2, c8), f32x4_add(b3, b4),
        f32x4_sub(b3, b4), f32x4_sub(b2, c8),
        f32x4_sub(b1, b6), f32x4_sub(b0, e7),
    )
}
