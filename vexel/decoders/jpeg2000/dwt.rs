use crate::decoders::jpeg2000::simd;

const DWT_ALPHA: f32 = -1.586_134_3;
const DWT_BETA: f32 = -0.052_980_118;
const DWT_GAMMA: f32 = 0.882_911_1;
const DWT_DELTA: f32 = 0.443_506_85;
const DWT_K: f32 = 1.230_174_1;

const DWT_TWO_INV_K: f32 = 1.625_732_4;

pub struct Line {
    pub sn: i32,
    pub dn: i32,
    pub cas: i32,
}

fn s_index(i: i32) -> usize {
    (i * 2) as usize
}

fn d_index(i: i32) -> usize {
    (1 + i * 2) as usize
}

fn clamp_s(a: &[i32], i: i32, sn: i32) -> i32 {
    let i = i.clamp(0, sn - 1);
    a[s_index(i)]
}

fn clamp_d(a: &[i32], i: i32, dn: i32) -> i32 {
    let i = i.clamp(0, dn - 1);
    a[d_index(i)]
}

pub fn decode_line_53(a: &mut [i32], line: &Line) {
    let (sn, dn, cas) = (line.sn, line.dn, line.cas);

    if cas == 0 {
        if dn <= 0 && sn <= 1 {
            return;
        }

        for i in 0..sn {
            let left = clamp_d(a, i - 1, dn);
            let right = clamp_d(a, i, dn);
            a[s_index(i)] -= (left + right + 2) >> 2;
        }

        for i in 0..dn {
            let left = clamp_s(a, i, sn);
            let right = clamp_s(a, i + 1, sn);
            a[d_index(i)] += (left + right) >> 1;
        }
    } else {
        if sn == 0 && dn == 1 {
            a[s_index(0)] /= 2;
            return;
        }

        for i in 0..sn {
            let left = clamp_ss(a, i, dn);
            let right = clamp_ss(a, i + 1, dn);
            a[d_index(i)] -= (left + right + 2) >> 2;
        }

        for i in 0..dn {
            let left = clamp_dd(a, i, sn);
            let right = clamp_dd(a, i - 1, sn);
            a[s_index(i)] += (left + right) >> 1;
        }
    }
}

fn clamp_ss(a: &[i32], i: i32, dn: i32) -> i32 {
    let i = i.clamp(0, dn - 1);
    a[s_index(i)]
}

fn clamp_dd(a: &[i32], i: i32, sn: i32) -> i32 {
    let i = i.clamp(0, sn - 1);
    a[d_index(i)]
}

fn step1(a: &mut [f32], offset: usize, start: i32, end: i32, c: f32) {
    let start = start.max(0);
    let end = end.max(0);

    if start >= end || offset >= a.len() {
        return;
    }

    let available = (a.len() - offset).div_ceil(2);
    let count = ((end - start) as usize).min(available.saturating_sub(start as usize));

    simd::scale_stride2(a, offset + (start as usize) * 2, count, c);
}

fn step2(a: &mut [f32], l_offset: usize, w_offset: usize, end: i32, m: i32, c: f32) {
    let imax = end.min(m).max(0) as usize;

    if w_offset == 0 || l_offset + 2 != w_offset {
        return step2_scalar(a, l_offset, w_offset, end, m, c);
    }

    let count = if w_offset < a.len() { imax.min((a.len() - w_offset).div_ceil(2)) } else { 0 };

    simd::lift_stride2(a, w_offset, count, c);

    let l = if count == 0 { l_offset } else { w_offset + (count - 1) * 2 };
    let w = w_offset + count * 2;

    if count == imax && m < end && l < a.len() && w > 0 && w - 1 < a.len() {
        a[w - 1] += 2.0 * c * a[l];
    }
}

fn step2_scalar(a: &mut [f32], l_offset: usize, w_offset: usize, end: i32, m: i32, c: f32) {
    let imax = end.min(m).max(0);

    let mut l = l_offset;
    let mut w = w_offset;

    for _ in 0..imax {
        if l >= a.len() || w >= a.len() || w == 0 {
            return;
        }
        a[w - 1] += (a[l] + a[w]) * c;
        l = w;
        w += 2;
    }

    if m < end && l < a.len() && w > 0 && w - 1 < a.len() {
        a[w - 1] += 2.0 * c * a[l];
    }
}

pub fn decode_line_97(a: &mut [f32], line: &Line) {
    let (sn, dn, cas) = (line.sn, line.dn, line.cas);

    let (offset_a, offset_b) = if cas == 0 {
        if dn <= 0 && sn <= 1 {
            return;
        }
        (0usize, 1usize)
    } else {
        if sn <= 0 && dn <= 1 {
            return;
        }
        (1usize, 0usize)
    };

    step1(a, offset_a, 0, sn, DWT_K);
    step1(a, offset_b, 0, dn, DWT_TWO_INV_K);

    let ia = offset_a as i32;
    let ib = offset_b as i32;

    step2(a, offset_b, offset_a + 1, sn, sn.min(dn - ia), -DWT_DELTA);
    step2(a, offset_a, offset_b + 1, dn, dn.min(sn - ib), -DWT_GAMMA);
    step2(a, offset_b, offset_a + 1, sn, sn.min(dn - ia), -DWT_BETA);
    step2(a, offset_a, offset_b + 1, dn, dn.min(sn - ib), -DWT_ALPHA);
}

pub fn interleave_h<T: Copy>(dest: &mut [T], src: &[T], sn: usize, dn: usize, cas: usize) {
    for i in 0..sn {
        let index = 2 * i + cas;
        if index < dest.len() && i < src.len() {
            dest[index] = src[i];
        }
    }
    for i in 0..dn {
        let index = 2 * i + 1 - cas;
        let source = sn + i;
        if index < dest.len() && source < src.len() {
            dest[index] = src[source];
        }
    }
}

pub fn interleave_v<T: Copy>(dest: &mut [T], src: &[T], stride: usize, sn: usize, dn: usize, cas: usize) {
    for i in 0..sn {
        let index = 2 * i + cas;
        let source = i * stride;
        if index < dest.len() && source < src.len() {
            dest[index] = src[source];
        }
    }
    for i in 0..dn {
        let index = 2 * i + 1 - cas;
        let source = (sn + i) * stride;
        if index < dest.len() && source < src.len() {
            dest[index] = src[source];
        }
    }
}
