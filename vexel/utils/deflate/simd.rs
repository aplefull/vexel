#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[rustfmt::skip]
const REPEAT_TABLE: [[u8; 16]; 16] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1],
    [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0],
    [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3],
    [0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0], 
    [0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3], 
    [0, 1, 2, 3, 4, 5, 6, 0, 1, 2, 3, 4, 5, 6, 0, 1], 
    [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 1, 2, 3, 4, 5, 6], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10, 0, 1, 2, 3, 4], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11, 0, 1, 2, 3], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12, 0, 1, 2], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13, 0, 1], 
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14, 0], 
];

#[rustfmt::skip]
const ROTATE_TABLE: [[u8; 16]; 16] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1], 
    [1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1], 
    [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3], 
    [1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1], 
    [4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1], 
    [2, 3, 4, 5, 6, 0, 1, 2, 3, 4, 5, 6, 0, 1, 2, 3], 
    [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7], 
    [7, 8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 1, 2, 3, 4], 
    [6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1], 
    [5, 6, 7, 8, 9,10, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 
    [4, 5, 6, 7, 8, 9,10,11, 0, 1, 2, 3, 4, 5, 6, 7], 
    [3, 4, 5, 6, 7, 8, 9,10,11,12, 0, 1, 2, 3, 4, 5], 
    [2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13, 0, 1, 2, 3], 
    [1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14, 0, 1], 
];

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,ssse3")]
unsafe fn copy_match_avx2(output: &mut Vec<u8>, match_dist: usize, match_len: usize) {
    let start = output.len() - match_dist;

    if match_dist >= 16 {
        output.reserve(match_len);
        let mut remaining = match_len;
        while remaining > 0 {
            let chunk = remaining.min(match_dist);
            let base = output.len() - match_dist;
            output.extend_from_within(base..base + chunk);
            remaining -= chunk;
        }
        return;
    }

    output.reserve(match_len + 32);

    let repeat_shuf = _mm_loadu_si128(REPEAT_TABLE[match_dist].as_ptr() as *const __m128i);
    let rotate_shuf = _mm_loadu_si128(ROTATE_TABLE[match_dist].as_ptr() as *const __m128i);

    let mut tmp = [0u8; 16];
    tmp[..match_dist].copy_from_slice(&output[start..start + match_dist]);
    let mut reg = _mm_shuffle_epi8(_mm_loadu_si128(tmp.as_ptr() as *const __m128i), repeat_shuf);

    let dst_ptr = output.as_mut_ptr().add(output.len());
    let mut written = 0usize;

    while written + 32 <= match_len {
        let hi = _mm_shuffle_epi8(reg, rotate_shuf);
        _mm256_storeu_si256(dst_ptr.add(written) as *mut __m256i, _mm256_set_m128i(hi, reg));
        written += 32;
        reg = _mm_shuffle_epi8(hi, rotate_shuf);
    }

    if written + 16 <= match_len {
        _mm_storeu_si128(dst_ptr.add(written) as *mut __m128i, reg);
        written += 16;
        reg = _mm_shuffle_epi8(reg, rotate_shuf);
    }

    let reg_bytes = std::mem::transmute::<__m128i, [u8; 16]>(reg);
    for i in 0..(match_len - written) {
        *dst_ptr.add(written + i) = reg_bytes[i];
    }

    output.set_len(output.len() + match_len);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn copy_match_ssse3(output: &mut Vec<u8>, match_dist: usize, match_len: usize) {
    let start = output.len() - match_dist;

    if match_dist >= 16 {
        output.reserve(match_len);
        let mut remaining = match_len;
        while remaining > 0 {
            let chunk = remaining.min(match_dist);
            let base = output.len() - match_dist;
            output.extend_from_within(base..base + chunk);
            remaining -= chunk;
        }
        return;
    }

    output.reserve(match_len + 16);

    let repeat_shuf = _mm_loadu_si128(REPEAT_TABLE[match_dist].as_ptr() as *const __m128i);
    let rotate_shuf = _mm_loadu_si128(ROTATE_TABLE[match_dist].as_ptr() as *const __m128i);

    let mut tmp = [0u8; 16];
    let src_slice = &output[start..start + match_dist];
    tmp[..match_dist].copy_from_slice(src_slice);
    let mut reg = _mm_loadu_si128(tmp.as_ptr() as *const __m128i);

    reg = _mm_shuffle_epi8(reg, repeat_shuf);

    let dst_ptr = output.as_mut_ptr().add(output.len());
    let mut written = 0usize;

    while written + 16 <= match_len {
        _mm_storeu_si128(dst_ptr.add(written) as *mut __m128i, reg);
        written += 16;
        reg = _mm_shuffle_epi8(reg, rotate_shuf);
    }

    let tail_ptr = dst_ptr.add(written);
    let reg_bytes = std::mem::transmute::<__m128i, [u8; 16]>(reg);
    for i in 0..(match_len - written) {
        *tail_ptr.add(i) = reg_bytes[i];
    }

    output.set_len(output.len() + match_len);
}

fn copy_match_scalar(output: &mut Vec<u8>, match_dist: usize, match_len: usize) {
    output.reserve(match_len);
    let mut remaining = match_len;
    while remaining > 0 {
        let chunk = remaining.min(match_dist);
        let base = output.len() - match_dist;
        output.extend_from_within(base..base + chunk);
        remaining -= chunk;
    }
}

pub fn copy_match(output: &mut Vec<u8>, match_dist: usize, match_len: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { copy_match_avx2(output, match_dist, match_len) };
        }
        if is_x86_feature_detected!("ssse3") {
            return unsafe { copy_match_ssse3(output, match_dist, match_len) };
        }
    }

    copy_match_scalar(output, match_dist, match_len);
}
