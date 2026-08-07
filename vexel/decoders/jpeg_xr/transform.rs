use crate::decoders::jpeg_xr::types::{ColorFormat, Overlap};

#[inline]
fn rotate1(a: &mut i32, b: &mut i32) {
    *a = a.wrapping_sub((b.wrapping_add(1)) >> 1);
    *b = b.wrapping_add((a.wrapping_add(1)) >> 1);
}

#[inline]
fn rotate2(a: &mut i32, b: &mut i32) {
    *a = a.wrapping_sub((a_mul3(*b).wrapping_add(4)) >> 3);
    *b = b.wrapping_add((a_mul3(*a).wrapping_add(4)) >> 3);
}

#[inline]
fn a_mul3(v: i32) -> i32 {
    v.wrapping_mul(3)
}

pub struct Plane {
    pub data: Vec<i32>,
    pub stride: usize,
    pub row_len: usize,
}

impl Plane {
    fn bases(&self, previous_row: usize, current_row: usize, mb_x: usize) -> (isize, isize) {
        let column = mb_x * self.stride;
        ((previous_row * self.row_len + column) as isize, (current_row * self.row_len + column) as isize)
    }

    fn get(&self, index: isize) -> i32 {
        if index < 0 {
            return 0;
        }
        self.data.get(index as usize).copied().unwrap_or(0)
    }

    fn set(&mut self, index: isize, value: i32) {
        if index < 0 {
            return;
        }
        if let Some(slot) = self.data.get_mut(index as usize) {
            *slot = value;
        }
    }
}

macro_rules! load4 {
    ($p:expr, $ia:expr, $ib:expr, $ic:expr, $id:expr) => {{
        ($p.get($ia), $p.get($ib), $p.get($ic), $p.get($id))
    }};
}

macro_rules! store4 {
    ($p:expr, $ia:expr, $ib:expr, $ic:expr, $id:expr, $a:expr, $b:expr, $c:expr, $d:expr) => {{
        $p.set($ia, $a);
        $p.set($ib, $b);
        $p.set($ic, $c);
        $p.set($id, $d);
    }};
}

fn dct2x2dn(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, cc, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_sub(cc);
    let t = (a.wrapping_sub(b)) >> 1;
    let c = t.wrapping_sub(d);
    d = t.wrapping_sub(cc);
    a = a.wrapping_sub(d);
    b = b.wrapping_add(c);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

fn dct2x2dn_dec(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, cc, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_sub(cc);
    let t = (a.wrapping_sub(b)) >> 1;
    let c = t.wrapping_sub(d);
    d = t.wrapping_sub(cc);
    a = a.wrapping_sub(d);
    b = b.wrapping_add(c);

    store4!(p, ia, ib, ic, id, a.wrapping_mul(2), b.wrapping_mul(2), c.wrapping_mul(2), d.wrapping_mul(2));
}

fn dct2x2up(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, cc, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_sub(cc);
    let t = (a.wrapping_sub(b).wrapping_add(1)) >> 1;
    let c = t.wrapping_sub(d);
    d = t.wrapping_sub(cc);
    a = a.wrapping_sub(d);
    b = b.wrapping_add(c);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

fn inv_odd(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    b = b.wrapping_add(d);
    a = a.wrapping_sub(c);
    d = d.wrapping_sub(b >> 1);
    c = c.wrapping_add((a.wrapping_add(1)) >> 1);

    rotate2(&mut a, &mut b);
    rotate2(&mut c, &mut d);

    c = c.wrapping_sub((b.wrapping_add(1)) >> 1);
    d = ((a.wrapping_add(1)) >> 1).wrapping_sub(d);
    b = b.wrapping_add(c);
    a = a.wrapping_sub(d);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

fn inv_odd_odd(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    d = d.wrapping_add(a);
    c = c.wrapping_sub(b);
    let t1 = d >> 1;
    a = a.wrapping_sub(t1);
    let t2 = c >> 1;
    b = b.wrapping_add(t2);

    a = a.wrapping_sub((a_mul3(b).wrapping_add(3)) >> 3);
    b = b.wrapping_add((a_mul3(a).wrapping_add(3)) >> 2);
    a = a.wrapping_sub((a_mul3(b).wrapping_add(4)) >> 3);

    b = b.wrapping_sub(t2);
    a = a.wrapping_add(t1);
    c = c.wrapping_add(b);
    d = d.wrapping_sub(a);

    store4!(p, ia, ib, ic, id, a, b.wrapping_neg(), c.wrapping_neg(), d);
}

fn inv_odd_odd_post(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    d = d.wrapping_add(a);
    c = c.wrapping_sub(b);
    let t1 = d >> 1;
    a = a.wrapping_sub(t1);
    let t2 = c >> 1;
    b = b.wrapping_add(t2);

    a = a.wrapping_sub((a_mul3(b).wrapping_add(6)) >> 3);
    b = b.wrapping_add((a_mul3(a).wrapping_add(2)) >> 2);
    a = a.wrapping_sub((a_mul3(b).wrapping_add(4)) >> 3);

    b = b.wrapping_sub(t2);
    a = a.wrapping_add(t1);
    c = c.wrapping_add(b);
    d = d.wrapping_sub(a);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

fn hst_dec1(p: &mut Plane, ia: isize, id: isize) {
    let mut a = p.get(ia);
    let mut d = p.get(id);

    a = a.wrapping_add(d);
    d = (a >> 1).wrapping_sub(d);
    a = a.wrapping_add(a_mul3(d) >> 3);
    d = d.wrapping_add(a_mul3(a) >> 4);

    p.set(ia, a);
    p.set(id, d);
}

fn hst_dec1_alternate(p: &mut Plane, ia: isize, id: isize) {
    let mut a = p.get(ia);
    let mut d = p.get(id);

    a = a.wrapping_add(d);
    d = (a >> 1).wrapping_sub(d);
    a = a.wrapping_add(a_mul3(d) >> 3);
    d = d.wrapping_add(a_mul3(a) >> 4);

    d = d.wrapping_add(a >> 7);
    d = d.wrapping_sub(a >> 10);

    p.set(ia, a);
    p.set(id, d);
}

fn hst_dec1_edge(a: &mut i32, d: &mut i32) {
    *a = a.wrapping_add(*d);
    *d = (*a >> 1).wrapping_sub(*d);
    *a = a.wrapping_add(a_mul3(*d) >> 3);
    *d = d.wrapping_add(a_mul3(*a) >> 4);

    *d = d.wrapping_add(*a >> 7);
    *d = d.wrapping_sub(*a >> 10);

    *a = a.wrapping_add((a_mul3(*d).wrapping_add(4)) >> 3);
    *d = d.wrapping_sub(*a >> 1);
    *a = a.wrapping_add(*d);

    *d = d.wrapping_neg();
}

fn hst_dec(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, c0, mut d) = load4!(p, ia, ib, ic, id);

    b = b.wrapping_sub(c0);
    a = a.wrapping_add((a_mul3(d).wrapping_add(4)) >> 3);

    d = d.wrapping_sub(b >> 1);
    let c = ((a.wrapping_sub(b)) >> 1).wrapping_sub(c0);

    p.set(ic, d);
    p.set(id, c);
    p.set(ia, a.wrapping_sub(c));
    p.set(ib, b.wrapping_add(d));
}

pub fn idct4x4_stage1(p: &mut Plane, base: isize) {
    dct2x2up(p, base, base + 1, base + 2, base + 3);
    inv_odd(p, base + 5, base + 4, base + 7, base + 6);
    inv_odd(p, base + 10, base + 8, base + 11, base + 9);
    inv_odd_odd(p, base + 15, base + 14, base + 13, base + 12);

    dct2x2dn(p, base, base + 4, base + 8, base + 12);
    dct2x2dn(p, base + 1, base + 5, base + 9, base + 13);
    dct2x2dn(p, base + 2, base + 6, base + 10, base + 14);
    dct2x2dn(p, base + 3, base + 7, base + 11, base + 15);
}

pub fn idct4x4_stage2(p: &mut Plane, base: isize) {
    inv_odd(p, base + 32, base + 48, base + 96, base + 112);
    inv_odd(p, base + 128, base + 192, base + 144, base + 208);
    inv_odd_odd(p, base + 160, base + 224, base + 176, base + 240);
    dct2x2up(p, base, base + 64, base + 16, base + 80);

    dct2x2dn(p, base, base + 192, base + 48, base + 240);
    dct2x2dn(p, base + 64, base + 128, base + 112, base + 176);
    dct2x2dn(p, base + 16, base + 208, base + 32, base + 224);
    dct2x2dn(p, base + 80, base + 144, base + 96, base + 160);
}

pub fn normalize_dec(p: &mut Plane, base: isize, chroma: bool) {
    if !chroma {
        return;
    }

    let mut i = 0isize;
    while i < 256 {
        let value = p.get(base + i);
        p.set(base + i, value.wrapping_add(value));
        i += 16;
    }
}

pub fn post2(p: &mut Plane, ia: isize, ib: isize) {
    let mut a = p.get(ia);
    let mut b = p.get(ib);

    b = b.wrapping_add((a.wrapping_add(4)) >> 3);
    a = a.wrapping_add((b.wrapping_add(2)) >> 2);
    b = b.wrapping_add((a.wrapping_add(4)) >> 3);

    p.set(ia, a);
    p.set(ib, b);
}

pub fn post2_alternate(p: &mut Plane, ia: isize, ib: isize) {
    let mut a = p.get(ia);
    let mut b = p.get(ib);

    b = b.wrapping_add((a.wrapping_add(2)) >> 2);
    a = a.wrapping_add((b.wrapping_add(1)) >> 1);
    a = a.wrapping_add(b >> 5);
    a = a.wrapping_add(b >> 9);
    a = a.wrapping_add(b >> 13);
    b = b.wrapping_add((a.wrapping_add(2)) >> 2);

    p.set(ia, a);
    p.set(ib, b);
}

pub fn post2x2(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_add(c);
    d = d.wrapping_sub((a.wrapping_add(1)) >> 1);
    c = c.wrapping_sub((b.wrapping_add(1)) >> 1);

    b = b.wrapping_add((a.wrapping_add(2)) >> 2);
    a = a.wrapping_add((b.wrapping_add(1)) >> 1);
    b = b.wrapping_add((a.wrapping_add(2)) >> 2);

    d = d.wrapping_add((a.wrapping_add(1)) >> 1);
    c = c.wrapping_add((b.wrapping_add(1)) >> 1);
    a = a.wrapping_sub(d);
    b = b.wrapping_sub(c);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

pub fn post2x2_alternate(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_add(c);
    d = d.wrapping_sub((a.wrapping_add(1)) >> 1);
    c = c.wrapping_sub((b.wrapping_add(1)) >> 1);

    b = b.wrapping_add((a.wrapping_add(2)) >> 2);
    a = a.wrapping_add((b.wrapping_add(1)) >> 1);
    a = a.wrapping_add(b >> 5);
    a = a.wrapping_add(b >> 9);
    a = a.wrapping_add(b >> 13);
    b = b.wrapping_add((a.wrapping_add(2)) >> 2);

    d = d.wrapping_add((a.wrapping_add(1)) >> 1);
    c = c.wrapping_add((b.wrapping_add(1)) >> 1);
    a = a.wrapping_sub(d);
    b = b.wrapping_sub(c);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

pub fn post4(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_add(c);
    d = d.wrapping_sub((a.wrapping_add(1)) >> 1);
    c = c.wrapping_sub((b.wrapping_add(1)) >> 1);

    rotate1(&mut c, &mut d);

    d = d.wrapping_add((a.wrapping_add(1)) >> 1);
    c = c.wrapping_add((b.wrapping_add(1)) >> 1);

    a = a.wrapping_sub(d.wrapping_sub((a_mul3(d).wrapping_add(16)) >> 5));
    b = b.wrapping_sub(c.wrapping_sub((a_mul3(c).wrapping_add(16)) >> 5));
    d = d.wrapping_add((a_mul3(a).wrapping_add(8)) >> 4);
    c = c.wrapping_add((a_mul3(b).wrapping_add(8)) >> 4);
    a = a.wrapping_add((a_mul3(d).wrapping_add(16)) >> 5);
    b = b.wrapping_add((a_mul3(c).wrapping_add(16)) >> 5);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

pub fn post4_alternate(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize) {
    let (mut a, mut b, mut c, mut d) = load4!(p, ia, ib, ic, id);

    a = a.wrapping_add(d);
    b = b.wrapping_add(c);
    d = d.wrapping_sub((a.wrapping_add(1)) >> 1);
    c = c.wrapping_sub((b.wrapping_add(1)) >> 1);

    hst_dec1_edge(&mut a, &mut d);
    hst_dec1_edge(&mut b, &mut c);
    rotate1(&mut c, &mut d);

    d = d.wrapping_add((a.wrapping_add(1)) >> 1);
    c = c.wrapping_add((b.wrapping_add(1)) >> 1);

    a = a.wrapping_sub(d);
    b = b.wrapping_sub(c);

    store4!(p, ia, ib, ic, id, a, b, c, d);
}

fn clip_dcl(dcl: i32, alt: i32) -> i32 {
    if dcl > 0 {
        if alt > 0 {
            dcl.min(alt)
        } else {
            0
        }
    } else if dcl < 0 {
        if alt < 0 {
            dcl.max(alt)
        } else {
            0
        }
    } else {
        0
    }
}

fn dc_compensate(p: &mut Plane, ia: isize, ib: isize, ic: isize, id: isize, dc: i32) {
    let dc = dc >> 1;

    p.set(ia, p.get(ia).wrapping_sub(dc));
    p.set(id, p.get(id).wrapping_sub(dc));
    p.set(ib, p.get(ib).wrapping_add(dc));
    p.set(ic, p.get(ic).wrapping_add(dc));
}

pub fn post4x4_stage1_split(p: &mut Plane, b0: isize, b1: isize, offset: isize, hp_qp: i32, hp_absent: bool) {
    let p2 = b0 + 72 - offset;
    let p3 = b1 + 64 - offset;
    let p0 = b0 + 12;
    let p1 = b1 + 4;

    for k in 0..4isize {
        dct2x2dn(p, p0 + k, p2 + k, p1 + k, p3 + k);
    }

    inv_odd_odd_post(p, p3, p3 + 1, p3 + 2, p3 + 3);

    let pair = |ia: isize, ib: isize, p: &mut Plane| {
        let mut a = p.get(ia);
        let mut b = p.get(ib);
        rotate1(&mut a, &mut b);
        p.set(ia, a);
        p.set(ib, b);
    };

    pair(p1 + 2, p1 + 3, p);
    pair(p1, p1 + 1, p);
    pair(p2 + 1, p2 + 3, p);
    pair(p2, p2 + 2, p);

    for k in 0..4isize {
        hst_dec1(p, p0 + k, p3 + k);
    }
    for k in 0..4isize {
        hst_dec(p, p0 + k, p2 + k, p1 + k, p3 + k);
    }

    for k in 0..4isize {
        let v0 = p.get(p0 + k);
        let v1 = p.get(p1 + k);
        let v2 = p.get(p2 + k);
        let v3 = p.get(p3 + k);

        let tmp = v0.wrapping_add(v1).wrapping_add(v2).wrapping_add(v3) >> 1;
        let mut dcl = (tmp.wrapping_mul(595).wrapping_add(65536)) >> 17;

        if (dcl.abs() < hp_qp && hp_qp > 20) || hp_absent {
            let alt = v0.wrapping_sub(v1).wrapping_sub(v2).wrapping_add(v3) >> 1;
            dcl = clip_dcl(dcl, alt);
            dc_compensate(p, p0 + k, p2 + k, p1 + k, p3 + k, dcl);
        }
    }
}

pub fn post4x4_stage1(p: &mut Plane, base: isize, offset: isize, hp_qp: i32, hp_absent: bool) {
    post4x4_stage1_split(p, base, base + 16, offset, hp_qp, hp_absent);
}

pub fn post4x4_stage1_split_alternate(p: &mut Plane, b0: isize, b1: isize, offset: isize) {
    let p2 = b0 + 72 - offset;
    let p3 = b1 + 64 - offset;
    let p0 = b0 + 12;
    let p1 = b1 + 4;

    for k in 0..4isize {
        dct2x2dn(p, p0 + k, p2 + k, p1 + k, p3 + k);
    }

    inv_odd_odd_post(p, p3, p3 + 1, p3 + 2, p3 + 3);

    let pair = |ia: isize, ib: isize, p: &mut Plane| {
        let mut a = p.get(ia);
        let mut b = p.get(ib);
        rotate1(&mut a, &mut b);
        p.set(ia, a);
        p.set(ib, b);
    };

    pair(p1 + 2, p1 + 3, p);
    pair(p1, p1 + 1, p);
    pair(p2 + 1, p2 + 3, p);
    pair(p2, p2 + 2, p);

    for k in 0..4isize {
        hst_dec1_alternate(p, p0 + k, p3 + k);
    }
    for k in 0..4isize {
        hst_dec(p, p0 + k, p2 + k, p1 + k, p3 + k);
    }
}

pub fn post4x4_stage1_alternate(p: &mut Plane, base: isize, offset: isize) {
    post4x4_stage1_split_alternate(p, base, base + 16, offset);
}

pub fn post4x4_stage2_split(p: &mut Plane, b0: isize, b1: isize) {
    dct2x2dn(p, b0 - 96, b0 + 96, b1 - 112, b1 + 80);
    dct2x2dn(p, b0 - 32, b0 + 32, b1 - 48, b1 + 16);
    dct2x2dn(p, b0 - 80, b0 + 112, b1 - 128, b1 + 64);
    dct2x2dn(p, b0 - 16, b0 + 48, b1 - 64, b1);

    inv_odd_odd_post(p, b1, b1 + 64, b1 + 16, b1 + 80);

    let pair = |ia: isize, ib: isize, p: &mut Plane| {
        let mut a = p.get(ia);
        let mut b = p.get(ib);
        rotate1(&mut a, &mut b);
        p.set(ia, a);
        p.set(ib, b);
    };

    pair(b0 + 48, b0 + 32, p);
    pair(b0 + 112, b0 + 96, p);
    pair(b1 - 64, b1 - 128, p);
    pair(b1 - 48, b1 - 112, p);

    hst_dec1(p, b0 - 96, b1 + 80);
    hst_dec1(p, b0 - 32, b1 + 16);
    hst_dec1(p, b0 - 80, b1 + 64);
    hst_dec1(p, b0 - 16, b1);

    hst_dec(p, b0 - 96, b1 - 112, b0 + 96, b1 + 80);
    hst_dec(p, b0 - 32, b1 - 48, b0 + 32, b1 + 16);
    hst_dec(p, b0 - 80, b1 - 128, b0 + 112, b1 + 64);
    hst_dec(p, b0 - 16, b1 - 64, b0 + 48, b1);
}

pub fn post4x4_stage2_split_alternate(p: &mut Plane, b0: isize, b1: isize) {
    dct2x2dn(p, b0 - 96, b0 + 96, b1 - 112, b1 + 80);
    dct2x2dn(p, b0 - 32, b0 + 32, b1 - 48, b1 + 16);
    dct2x2dn(p, b0 - 80, b0 + 112, b1 - 128, b1 + 64);
    dct2x2dn(p, b0 - 16, b0 + 48, b1 - 64, b1);

    inv_odd_odd_post(p, b1, b1 + 64, b1 + 16, b1 + 80);

    let pair = |ia: isize, ib: isize, p: &mut Plane| {
        let mut a = p.get(ia);
        let mut b = p.get(ib);
        rotate1(&mut a, &mut b);
        p.set(ia, a);
        p.set(ib, b);
    };

    pair(b0 + 48, b0 + 32, p);
    pair(b0 + 112, b0 + 96, p);
    pair(b1 - 64, b1 - 128, p);
    pair(b1 - 48, b1 - 112, p);

    hst_dec1_alternate(p, b0 - 96, b1 + 80);
    hst_dec1_alternate(p, b0 - 32, b1 + 16);
    hst_dec1_alternate(p, b0 - 80, b1 + 64);
    hst_dec1_alternate(p, b0 - 16, b1);

    hst_dec(p, b0 - 96, b1 - 112, b0 + 96, b1 + 80);
    hst_dec(p, b0 - 32, b1 - 48, b0 + 32, b1 + 16);
    hst_dec(p, b0 - 80, b1 - 128, b0 + 112, b1 + 64);
    hst_dec(p, b0 - 16, b1 - 64, b0 + 48, b1);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MacroblockPosition {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    pub left_adjacent: bool,
    pub right_adjacent: bool,
    pub vertical_tile_boundary: bool,
    pub horizontal_tile_boundary: bool,
    pub one_mb_left_vertical_tb: bool,
    pub one_mb_right_vertical_tb: bool,
}

impl MacroblockPosition {
    fn left_or_right(&self) -> bool {
        self.left || self.right
    }

    fn top_or_bottom(&self) -> bool {
        self.top || self.bottom
    }

    fn bottom_or_right(&self) -> bool {
        self.bottom || self.right
    }

    fn left_edge(&self) -> bool {
        self.left || self.vertical_tile_boundary
    }

    fn right_edge(&self) -> bool {
        self.right || self.vertical_tile_boundary
    }

    fn top_edge(&self) -> bool {
        self.top || self.horizontal_tile_boundary
    }

    fn bottom_edge(&self) -> bool {
        self.bottom || self.horizontal_tile_boundary
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CornerPrediction {
    pub before: [[i32; 2]; 2],
    pub after: [[i32; 2]; 2],
}

pub struct TransformParams {
    pub overlap: Overlap,
    pub color_format: ColorFormat,
    pub channels: usize,
    pub scaled_arith: bool,
    pub altered_operators: bool,
    pub hp_absent: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MacroblockCursor {
    pub previous_row: usize,
    pub current_row: usize,
    pub mb_x: usize,
}

impl MacroblockCursor {
    fn bases(&self, plane: &Plane) -> (isize, isize) {
        plane.bases(self.previous_row, self.current_row, self.mb_x)
    }
}

pub fn inverse_transform_macroblock(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    hp_qp: &[i32],
    corner: &mut CornerPrediction,
) {
    if params.altered_operators {
        inverse_transform_hard(planes, cursor, position, params, corner);
        return;
    }

    inverse_transform_soft(planes, cursor, position, params, hp_qp);
}

fn luma_channel_count(params: &TransformParams) -> usize {
    if params.color_format.is_chroma_subsampled() {
        1
    } else {
        params.channels
    }
}

fn inverse_transform_soft(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    hp_qp: &[i32],
) {
    let overlap = params.overlap;
    let channels = luma_channel_count(params).min(planes.len());

    for (channel, plane) in planes.iter_mut().enumerate().take(channels) {
        let (base0, base1) = cursor.bases(plane);
        let qp = if params.hp_absent { 255 } else { hp_qp.get(channel).copied().unwrap_or(255) };

        if !position.bottom_or_right() {
            idct4x4_stage2(plane, base1);
            if params.scaled_arith {
                normalize_dec(plane, base1, channel != 0);
            }
        }

        if overlap == Overlap::Two {
            if position.left_or_right() && !position.top_or_bottom() {
                let j = if position.left { 0 } else { -128 };
                post4(plane, base0 + j + 32, base0 + j + 48, base1 + j, base1 + j + 16);
                post4(plane, base0 + j + 96, base0 + j + 112, base1 + j + 64, base1 + j + 80);
            }

            if !position.left_or_right() {
                if position.top_or_bottom() {
                    let p = if position.top { base1 } else { base0 + 32 };
                    post4(plane, p - 128, p - 64, p, p + 64);
                    post4(plane, p - 112, p - 48, p + 16, p + 80);
                } else {
                    post4x4_stage2_split(plane, base0, base1);
                }
            }
        }

        if !position.top {
            let start = if position.left { 32 } else { -96 };
            let end = if position.right { 32 } else { 160 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                idct4x4_stage1(plane, base0 + j + 16);
                j += 64;
            }
        }

        if !position.bottom {
            let start = if position.left { 0 } else { -128 };
            let end = if position.right { 0 } else { 128 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                idct4x4_stage1(plane, base1 + j + 16);
                j += 64;
            }
        }

        if overlap == Overlap::None {
            continue;
        }

        if position.left_or_right() {
            let j = if position.left { 10 } else { -64 + 14 };

            if !position.top {
                let p = base0 + 16 + j;
                post4(plane, p, p - 2, p + 6, p + 8);
                post4(plane, p + 1, p - 1, p + 7, p + 9);
                post4(plane, p + 16, p + 14, p + 22, p + 24);
                post4(plane, p + 17, p + 15, p + 23, p + 25);
            }

            if !position.bottom {
                let p = base1 + j;
                post4(plane, p, p - 2, p + 6, p + 8);
                post4(plane, p + 1, p - 1, p + 7, p + 9);
            }

            if !position.top_or_bottom() {
                post4(plane, base0 + 48 + j, base0 + 48 + j - 2, base1 - 10 + j, base1 - 8 + j);
                post4(plane, base0 + 48 + j + 1, base0 + 48 + j - 1, base1 - 9 + j, base1 - 7 + j);
            }
        }

        let start = if position.left { 0 } else { -192 };
        let end = if position.right { -64 } else { 64 };

        if position.top {
            let mut j = start;
            while j < end {
                let p = base1 + j;
                post4(plane, p + 5, p + 4, p + 64, p + 65);
                post4(plane, p + 7, p + 6, p + 66, p + 67);
                post4x4_stage1(plane, base1 + j, 0, qp, params.hp_absent);
                j += 64;
            }
        } else if position.bottom {
            let mut j = start;
            while j < end {
                post4x4_stage1(plane, base0 + 16 + j, 0, qp, params.hp_absent);
                post4x4_stage1(plane, base0 + 32 + j, 0, qp, params.hp_absent);

                let p = base0 + 48 + j;
                post4(plane, p + 15, p + 14, p + 74, p + 75);
                post4(plane, p + 13, p + 12, p + 72, p + 73);
                j += 64;
            }
        } else {
            let mut j = start;
            while j < end {
                post4x4_stage1(plane, base0 + 16 + j, 0, qp, params.hp_absent);
                post4x4_stage1(plane, base0 + 32 + j, 0, qp, params.hp_absent);
                post4x4_stage1_split(plane, base0 + 48 + j, base1 + j, 0, qp, params.hp_absent);
                post4x4_stage1(plane, base1 + j, 0, qp, params.hp_absent);
                j += 64;
            }
        }
    }

    match params.color_format {
        ColorFormat::Yuv420 => transform_chroma_420(planes, cursor, position, params, hp_qp),
        ColorFormat::Yuv422 => transform_chroma_422(planes, cursor, position, params, hp_qp),
        _ => {}
    }
}

fn transform_chroma_420(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    hp_qp: &[i32],
) {
    for i in 0..2usize {
        let Some(plane) = planes.get_mut(1 + i) else { continue };
        let (base0, base1) = cursor.bases(plane);
        let qp = if params.hp_absent { 255 } else { hp_qp.get(i).copied().unwrap_or(255) };

        if !position.bottom_or_right() {
            if params.scaled_arith {
                dct2x2dn_dec(plane, base1, base1 + 32, base1 + 16, base1 + 48);
            } else {
                dct2x2dn(plane, base1, base1 + 32, base1 + 16, base1 + 48);
            }
        }

        if params.overlap == Overlap::Two {
            if position.left_or_right() && !position.top_or_bottom() {
                let j = if position.left { 0 } else { -32 };
                post2(plane, base0 + j + 16, base1 + j);
            }

            if !position.left_or_right() {
                if position.top_or_bottom() {
                    let p = if position.top { base1 } else { base0 + 16 };
                    post2(plane, p - 32, p);
                } else {
                    post2x2(plane, base0 - 16, base0 + 16, base1 - 32, base1);
                }
            }
        }

        if !position.top {
            let start = if position.left { 16 } else { -16 };
            let end = if position.right { 16 } else { 48 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                j += 32;
            }
        }

        if !position.bottom {
            let start = if position.left { 0 } else { -32 };
            let end = if position.right { 0 } else { 32 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                j += 32;
            }
        }

        if params.overlap == Overlap::None {
            continue;
        }

        if !position.left && !position.top {
            let end = if position.right { -16 } else { 16 };

            if position.bottom {
                let mut j = -48;
                while j < end {
                    let p = base0 + j;
                    post4(plane, p + 15, p + 14, p + 42, p + 43);
                    post4(plane, p + 13, p + 12, p + 40, p + 41);
                    j += 32;
                }
            } else {
                let mut j = -48;
                while j < end {
                    post4x4_stage1_split(plane, base0 + j, base1 - 16 + j, 32, qp, params.hp_absent);
                    j += 32;
                }
            }

            if position.right {
                if !position.bottom {
                    post4(plane, base0 - 2, base0 - 4, base1 - 28, base1 - 26);
                    post4(plane, base0 - 1, base0 - 3, base1 - 27, base1 - 25);
                }

                post4(plane, base0 - 18, base0 - 20, base0 - 12, base0 - 10);
                post4(plane, base0 - 17, base0 - 19, base0 - 11, base0 - 9);
            } else {
                post4x4_stage1(plane, base0 - 32, 32, qp, params.hp_absent);
            }

            post4x4_stage1(plane, base0 - 64, 32, qp, params.hp_absent);
        } else if position.top {
            let start = if position.left { 0 } else { -64 };
            let end = if position.right { -32 } else { 0 };
            let mut j = start;
            while j < end {
                let p = base1 + j + 4;
                post4(plane, p + 1, p, p + 28, p + 29);
                post4(plane, p + 3, p + 2, p + 30, p + 31);
                j += 32;
            }
        } else if position.left {
            if !position.bottom {
                post4(plane, base0 + 26, base0 + 24, base1, base1 + 2);
                post4(plane, base0 + 27, base0 + 25, base1 + 1, base1 + 3);
            }

            post4(plane, base0 + 10, base0 + 8, base0 + 16, base0 + 18);
            post4(plane, base0 + 11, base0 + 9, base0 + 17, base0 + 19);
        }
    }
}

fn transform_chroma_422(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    hp_qp: &[i32],
) {
    for i in 0..2usize {
        let Some(plane) = planes.get_mut(1 + i) else { continue };
        let (base0, base1) = cursor.bases(plane);
        let qp = if params.hp_absent { 255 } else { hp_qp.get(i).copied().unwrap_or(255) };

        if !position.bottom_or_right() {
            let v0 = plane.get(base1);
            let v32 = plane.get(base1 + 32);
            let updated = v0.wrapping_sub((v32.wrapping_add(1)) >> 1);
            plane.set(base1, updated);
            plane.set(base1 + 32, v32.wrapping_add(updated));

            if params.scaled_arith {
                dct2x2dn_dec(plane, base1, base1 + 64, base1 + 16, base1 + 80);
                dct2x2dn_dec(plane, base1 + 32, base1 + 96, base1 + 48, base1 + 112);
            } else {
                dct2x2dn(plane, base1, base1 + 64, base1 + 16, base1 + 80);
                dct2x2dn(plane, base1 + 32, base1 + 96, base1 + 48, base1 + 112);
            }
        }

        if params.overlap == Overlap::Two {
            if !position.bottom {
                if position.left_or_right() {
                    if !position.top {
                        let j = if position.left { 0 } else { -64 };
                        post2(plane, base0 + 48 + j, base1 + j);
                    }

                    let j = if position.left { 16 } else { -48 };
                    post2(plane, base1 + j, base1 + j + 16);
                } else {
                    if position.top {
                        post2(plane, base1 - 64, base1);
                    } else {
                        post2x2(plane, base0 - 16, base0 + 48, base1 - 64, base1);
                    }

                    post2x2(plane, base1 - 48, base1 + 16, base1 - 32, base1 + 32);
                }
            } else if !position.left_or_right() {
                post2(plane, base0 - 16, base0 + 48);
            }
        }

        if !position.top {
            let start = if position.left { 48 } else { -16 };
            let end = if position.right { 48 } else { 112 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                j += 64;
            }
        }

        if !position.bottom {
            let start = if position.left { 0 } else { -64 };
            let end = if position.right { 0 } else { 64 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                idct4x4_stage1(plane, base1 + j + 16);
                idct4x4_stage1(plane, base1 + j + 32);
                j += 64;
            }
        }

        if params.overlap == Overlap::None {
            continue;
        }

        let start = if position.left { 0 } else { -128 };
        let end = if position.right { -64 } else { 0 };

        if !position.top {
            if position.left_or_right() {
                let j = if position.left { 32 + 10 } else { -32 + 14 };
                let p = base0 + j;
                post4(plane, p, p - 2, p + 6, p + 8);
                post4(plane, p + 1, p - 1, p + 7, p + 9);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1(plane, base0 + j + 32, 0, qp, params.hp_absent);
                j += 64;
            }
        }

        if !position.bottom {
            if position.left_or_right() {
                let j = if position.left { 10 } else { -64 + 14 };
                let p = base1 + j;
                post4(plane, p, p - 2, p + 6, p + 8);
                post4(plane, p + 1, p - 1, p + 7, p + 9);

                let p = p + 16;
                post4(plane, p, p - 2, p + 6, p + 8);
                post4(plane, p + 1, p - 1, p + 7, p + 9);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1(plane, base1 + j, 0, qp, params.hp_absent);
                post4x4_stage1(plane, base1 + j + 16, 0, qp, params.hp_absent);
                j += 64;
            }
        }

        if position.top_or_bottom() {
            let p = if position.top { base1 + 5 } else { base0 + 48 + 13 };
            let mut j = start;
            while j < end {
                post4(plane, p + j, p + j - 1, p + j + 59, p + j + 60);
                post4(plane, p + j + 2, p + j + 1, p + j + 61, p + j + 62);
                j += 64;
            }
        } else {
            if position.left_or_right() {
                let j = if position.left { 0 } else { -64 + 4 };
                post4(plane, base0 + j + 48 + 10, base0 + j + 48 + 10 - 2, base1 + j, base1 + j + 2);
                post4(plane, base0 + j + 48 + 10 + 1, base0 + j + 48 + 10 - 1, base1 + j + 1, base1 + j + 3);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1_split(plane, base0 + j + 48, base1 + j, 0, qp, params.hp_absent);
                j += 64;
            }
        }
    }
}

fn inverse_transform_hard(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    corner: &mut CornerPrediction,
) {
    let overlap = params.overlap;
    let channels = luma_channel_count(params).min(planes.len());

    for (channel, plane) in planes.iter_mut().enumerate().take(channels) {
        let (base0, base1) = cursor.bases(plane);

        if !position.bottom_or_right() {
            idct4x4_stage2(plane, base1);
            if params.scaled_arith {
                normalize_dec(plane, base1, channel != 0);
            }
        }

        if overlap == Overlap::Two {
            if position.top_edge() && position.left_edge() {
                post4_alternate(plane, base1, base1 + 64, base1 + 16, base1 + 80);
            }
            if position.top_edge() && position.right_edge() {
                post4_alternate(plane, base1 - 128, base1 - 64, base1 - 112, base1 - 48);
            }
            if position.bottom_edge() && position.left_edge() {
                post4_alternate(plane, base0 + 32, base0 + 96, base0 + 48, base0 + 112);
            }
            if position.bottom_edge() && position.right_edge() {
                post4_alternate(plane, base0 - 96, base0 - 32, base0 - 80, base0 - 16);
            }

            if (position.left_or_right() || position.vertical_tile_boundary)
                && !position.top_or_bottom()
                && !position.horizontal_tile_boundary
            {
                if position.left_edge() {
                    post4_alternate(plane, base0 + 32, base0 + 48, base1, base1 + 16);
                    post4_alternate(plane, base0 + 96, base0 + 112, base1 + 64, base1 + 80);
                }
                if position.right_edge() {
                    let j = -128;
                    post4_alternate(plane, base0 + j + 32, base0 + j + 48, base1 + j, base1 + j + 16);
                    post4_alternate(plane, base0 + j + 96, base0 + j + 112, base1 + j + 64, base1 + j + 80);
                }
            }

            if !position.left_or_right() {
                if (position.top_or_bottom() || position.horizontal_tile_boundary) && !position.vertical_tile_boundary {
                    if position.top_edge() {
                        post4_alternate(plane, base1 - 128, base1 - 64, base1, base1 + 64);
                        post4_alternate(plane, base1 - 112, base1 - 48, base1 + 16, base1 + 80);
                    }
                    if position.bottom_edge() {
                        let p = base0 + 32;
                        post4_alternate(plane, p - 128, p - 64, p, p + 64);
                        post4_alternate(plane, p - 112, p - 48, p + 16, p + 80);
                    }
                }

                if !position.top_or_bottom() && !position.horizontal_tile_boundary && !position.vertical_tile_boundary {
                    post4x4_stage2_split_alternate(plane, base0, base1);
                }
            }
        }

        if !position.top {
            let start = if position.left { 32 } else { -96 };
            let end = if position.right { 32 } else { 160 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                idct4x4_stage1(plane, base0 + j + 16);
                j += 64;
            }
        }

        if !position.bottom {
            let start = if position.left { 0 } else { -128 };
            let end = if position.right { 0 } else { 128 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                idct4x4_stage1(plane, base1 + j + 16);
                j += 64;
            }
        }

        if overlap == Overlap::None {
            continue;
        }

        if position.left_or_right() || position.vertical_tile_boundary {
            if position.top_edge() && position.left_edge() {
                post4_alternate(plane, base1, base1 + 1, base1 + 2, base1 + 3);
            }
            if position.top_edge() && position.right_edge() {
                post4_alternate(plane, base1 - 59, base1 - 60, base1 - 57, base1 - 58);
            }
            if position.bottom_edge() && position.left_edge() {
                post4_alternate(plane, base0 + 58, base0 + 59, base0 + 56, base0 + 57);
            }
            if position.bottom_edge() && position.right_edge() {
                post4_alternate(plane, base0 - 1, base0 - 2, base0 - 3, base0 - 4);
            }

            for side in 0..2usize {
                let active = if side == 0 { position.left_edge() } else { position.right_edge() };
                if !active {
                    continue;
                }

                let j = if side == 0 { 10 } else { -64 + 14 };

                if !position.top {
                    let p = base0 + 16 + j;
                    post4_alternate(plane, p, p - 2, p + 6, p + 8);
                    post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
                    post4_alternate(plane, p + 16, p + 14, p + 22, p + 24);
                    post4_alternate(plane, p + 17, p + 15, p + 23, p + 25);
                }

                if !position.bottom {
                    let p = base1 + j;
                    post4_alternate(plane, p, p - 2, p + 6, p + 8);
                    post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
                }

                if !position.top_or_bottom() && !position.horizontal_tile_boundary {
                    post4_alternate(plane, base0 + 48 + j, base0 + 48 + j - 2, base1 - 10 + j, base1 - 8 + j);
                    post4_alternate(plane, base0 + 48 + j + 1, base0 + 48 + j - 1, base1 - 9 + j, base1 - 7 + j);
                }
            }
        }

        let start = if position.left { 0 } else { -192 };
        let end = if position.right { -64 } else { 64 };

        if position.top_edge() {
            let mut j = start;
            while j < end {
                if !position.vertical_tile_boundary || j != -64 {
                    let p = base1 + j;
                    post4_alternate(plane, p + 5, p + 4, p + 64, p + 65);
                    post4_alternate(plane, p + 7, p + 6, p + 66, p + 67);
                    post4x4_stage1_alternate(plane, base1 + j, 0);
                }
                j += 64;
            }
        }

        if position.bottom_edge() {
            let mut j = start;
            while j < end {
                if !position.vertical_tile_boundary || j != -64 {
                    post4x4_stage1_alternate(plane, base0 + 16 + j, 0);
                    post4x4_stage1_alternate(plane, base0 + 32 + j, 0);

                    let p = base0 + 48 + j;
                    post4_alternate(plane, p + 15, p + 14, p + 74, p + 75);
                    post4_alternate(plane, p + 13, p + 12, p + 72, p + 73);
                }
                j += 64;
            }
        }

        if !position.top && !position.bottom && !position.horizontal_tile_boundary {
            let mut j = start;
            while j < end {
                if !position.vertical_tile_boundary || j != -64 {
                    post4x4_stage1_alternate(plane, base0 + 16 + j, 0);
                    post4x4_stage1_alternate(plane, base0 + 32 + j, 0);
                    post4x4_stage1_split_alternate(plane, base0 + 48 + j, base1 + j, 0);
                    post4x4_stage1_alternate(plane, base1 + j, 0);
                }
                j += 64;
            }
        }
    }

    match params.color_format {
        ColorFormat::Yuv420 => transform_chroma_420_hard(planes, cursor, position, params, corner),
        ColorFormat::Yuv422 => transform_chroma_422_hard(planes, cursor, position, params, corner),
        _ => {}
    }
}

fn transform_chroma_420_hard(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    corner: &mut CornerPrediction,
) {
    let left_delayed = position.left_adjacent || position.one_mb_right_vertical_tb;
    let right_delayed = position.right_adjacent || position.one_mb_left_vertical_tb;

    for i in 0..2usize {
        let Some(plane) = planes.get_mut(1 + i) else { continue };
        let (base0, base1) = cursor.bases(plane);

        if !position.bottom_or_right() {
            if params.scaled_arith {
                dct2x2dn_dec(plane, base1, base1 + 32, base1 + 16, base1 + 48);
            } else {
                dct2x2dn(plane, base1, base1 + 32, base1 + 16, base1 + 48);
            }
        }

        if params.overlap == Overlap::Two {
            if left_delayed && position.top_edge() {
                let value = plane.get(base1 - 64 + 32);
                plane.set(base1 - 64, plane.get(base1 - 64).wrapping_sub(value));
            }
            if right_delayed && position.top_edge() {
                corner.before[i][0] = plane.get(base1);
            }
            if position.right_edge() && position.top_edge() {
                let value = corner.before[i][0];
                plane.set(base1 - 64 + 32, plane.get(base1 - 64 + 32).wrapping_sub(value));
            }
            if left_delayed && position.bottom_edge() {
                let value = plane.get(base0 - 64 + 48);
                plane.set(base0 - 64 + 16, plane.get(base0 - 64 + 16).wrapping_sub(value));
            }
            if right_delayed && position.bottom_edge() {
                corner.before[i][1] = plane.get(base0 + 16);
            }
            if position.right_edge() && position.bottom_edge() {
                let value = corner.before[i][1];
                plane.set(base0 - 64 + 48, plane.get(base0 - 64 + 48).wrapping_sub(value));
            }

            if (position.left_or_right() || position.vertical_tile_boundary)
                && !position.top_or_bottom()
                && !position.horizontal_tile_boundary
            {
                if position.left_edge() {
                    post2_alternate(plane, base0 + 16, base1);
                }
                if position.right_edge() {
                    post2_alternate(plane, base0 - 32 + 16, base1 - 32);
                }
            }

            if !position.left_or_right() {
                if (position.top_or_bottom() || position.horizontal_tile_boundary) && !position.vertical_tile_boundary {
                    if position.top_edge() {
                        post2_alternate(plane, base1 - 32, base1);
                    }
                    if position.bottom_edge() {
                        post2_alternate(plane, base0 + 16 - 32, base0 + 16);
                    }
                } else if !position.top_or_bottom()
                    && !position.horizontal_tile_boundary
                    && !position.vertical_tile_boundary
                {
                    post2x2_alternate(plane, base0 - 16, base0 + 16, base1 - 32, base1);
                }
            }

            if left_delayed && position.top_edge() {
                let value = plane.get(base1 - 64 + 32);
                plane.set(base1 - 64, plane.get(base1 - 64).wrapping_add(value));
            }
            if right_delayed && position.top_edge() {
                corner.after[i][0] = plane.get(base1);
            }
            if position.right_edge() && position.top_edge() {
                let value = corner.after[i][0];
                plane.set(base1 - 64 + 32, plane.get(base1 - 64 + 32).wrapping_add(value));
            }
            if left_delayed && position.bottom_edge() {
                let value = plane.get(base0 - 64 + 48);
                plane.set(base0 - 64 + 16, plane.get(base0 - 64 + 16).wrapping_add(value));
            }
            if right_delayed && position.bottom_edge() {
                corner.after[i][1] = plane.get(base0 + 16);
            }
            if position.right_edge() && position.bottom_edge() {
                let value = corner.after[i][1];
                plane.set(base0 - 64 + 48, plane.get(base0 - 64 + 48).wrapping_add(value));
            }
        }

        if !position.top {
            let start = if position.left {
                48
            } else if left_delayed {
                -48
            } else {
                -16
            };
            let end = if position.right_edge() { 16 } else { 48 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                j += 32;
            }
        }

        if !position.bottom {
            let start = if position.left {
                32
            } else if left_delayed {
                -64
            } else {
                -32
            };
            let end = if position.right_edge() { 0 } else { 32 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                j += 32;
            }
        }

        if params.overlap == Overlap::None {
            continue;
        }

        if position.top_edge() && left_delayed {
            post4_alternate(plane, base1 - 64, base1 - 63, base1 - 62, base1 - 61);
        }
        if position.top_edge() && position.right_edge() {
            post4_alternate(plane, base1 - 27, base1 - 28, base1 - 25, base1 - 26);
        }
        if position.bottom_edge() && left_delayed {
            post4_alternate(plane, base0 - 38, base0 - 37, base0 - 40, base0 - 39);
        }
        if position.bottom_edge() && position.right_edge() {
            post4_alternate(plane, base0 - 1, base0 - 2, base0 - 3, base0 - 4);
        }

        if !position.left && !position.top {
            if left_delayed {
                if !position.bottom && !position.horizontal_tile_boundary {
                    post4_alternate(plane, base0 - 38, base0 - 40, base1 - 64, base1 - 62);
                    post4_alternate(plane, base0 - 37, base0 - 39, base1 - 63, base1 - 61);
                }

                post4_alternate(plane, base0 - 54, base0 - 56, base0 - 48, base0 - 46);
                post4_alternate(plane, base0 - 53, base0 - 55, base0 - 47, base0 - 45);
            }

            if position.bottom_edge() {
                let p = base0 - 48;
                post4_alternate(plane, p + 15, p + 14, p + 42, p + 43);
                post4_alternate(plane, p + 13, p + 12, p + 40, p + 41);

                if !position.right && !position.vertical_tile_boundary {
                    let p = base0 - 16;
                    post4_alternate(plane, p + 15, p + 14, p + 42, p + 43);
                    post4_alternate(plane, p + 13, p + 12, p + 40, p + 41);
                }
            } else {
                post4x4_stage1_split_alternate(plane, base0 - 48, base1 - 64, 32);

                if !position.right && !position.vertical_tile_boundary {
                    post4x4_stage1_split_alternate(plane, base0 - 16, base1 - 32, 32);
                }
            }

            if position.right_edge() {
                if !position.bottom && !position.horizontal_tile_boundary {
                    post4_alternate(plane, base0 - 2, base0 - 4, base1 - 28, base1 - 26);
                    post4_alternate(plane, base0 - 1, base0 - 3, base1 - 27, base1 - 25);
                }

                post4_alternate(plane, base0 - 18, base0 - 20, base0 - 12, base0 - 10);
                post4_alternate(plane, base0 - 17, base0 - 19, base0 - 11, base0 - 9);
            } else {
                post4x4_stage1_alternate(plane, base0 - 32, 32);
            }

            post4x4_stage1_alternate(plane, base0 - 64, 32);
        }

        if position.top_edge() {
            if !position.left {
                let p = base1 - 64 + 4;
                post4_alternate(plane, p + 1, p, p + 28, p + 29);
                post4_alternate(plane, p + 3, p + 2, p + 30, p + 31);
            }

            if !position.left && !position.right && !position.vertical_tile_boundary {
                let p = base1 - 32 + 4;
                post4_alternate(plane, p + 1, p, p + 28, p + 29);
                post4_alternate(plane, p + 3, p + 2, p + 30, p + 31);
            }
        }
    }
}

fn transform_chroma_422_hard(
    planes: &mut [Plane],
    cursor: MacroblockCursor,
    position: &MacroblockPosition,
    params: &TransformParams,
    corner: &mut CornerPrediction,
) {
    let left_delayed = position.left_adjacent || position.one_mb_right_vertical_tb;
    let right_delayed = position.right_adjacent || position.one_mb_left_vertical_tb;

    for i in 0..2usize {
        let Some(plane) = planes.get_mut(1 + i) else { continue };
        let (base0, base1) = cursor.bases(plane);

        if !position.bottom_or_right() {
            let low = plane.get(base1);
            let high = plane.get(base1 + 32);
            plane.set(base1, low.wrapping_sub((high.wrapping_add(1)) >> 1));
            plane.set(base1 + 32, high.wrapping_add(plane.get(base1)));

            if params.scaled_arith {
                dct2x2dn_dec(plane, base1, base1 + 64, base1 + 16, base1 + 80);
                dct2x2dn_dec(plane, base1 + 32, base1 + 96, base1 + 48, base1 + 112);
            } else {
                dct2x2dn(plane, base1, base1 + 64, base1 + 16, base1 + 80);
                dct2x2dn(plane, base1 + 32, base1 + 96, base1 + 48, base1 + 112);
            }
        }

        if params.overlap == Overlap::Two {
            if left_delayed && position.top_edge() {
                let value = plane.get(base1 - 128 + 64);
                plane.set(base1 - 128, plane.get(base1 - 128).wrapping_sub(value));
            }
            if right_delayed && position.top_edge() {
                corner.before[i][0] = plane.get(base1);
            }
            if position.right_edge() && position.top_edge() {
                let value = corner.before[i][0];
                plane.set(base1 - 128 + 64, plane.get(base1 - 128 + 64).wrapping_sub(value));
            }
            if left_delayed && position.bottom_edge() {
                let value = plane.get(base0 - 128 + 112);
                plane.set(base0 - 128 + 48, plane.get(base0 - 128 + 48).wrapping_sub(value));
            }
            if right_delayed && position.bottom_edge() {
                corner.before[i][1] = plane.get(base0 + 48);
            }
            if position.right_edge() && position.bottom_edge() {
                let value = corner.before[i][1];
                plane.set(base0 - 128 + 112, plane.get(base0 - 128 + 112).wrapping_sub(value));
            }

            if !position.bottom {
                if position.left_or_right() || position.vertical_tile_boundary {
                    if !position.top && !position.horizontal_tile_boundary {
                        if position.left_edge() {
                            post2_alternate(plane, base0 + 48, base1);
                        }
                        if position.right_edge() {
                            post2_alternate(plane, base0 + 48 - 64, base1 - 64);
                        }
                    }

                    if position.left_edge() {
                        post2_alternate(plane, base1 + 16, base1 + 32);
                    }
                    if position.right_edge() {
                        post2_alternate(plane, base1 - 48, base1 - 32);
                    }
                }

                if !position.left_or_right() && !position.vertical_tile_boundary {
                    if position.top_edge() {
                        post2_alternate(plane, base1 - 64, base1);
                    } else {
                        post2x2_alternate(plane, base0 - 16, base0 + 48, base1 - 64, base1);
                    }

                    post2x2_alternate(plane, base1 - 48, base1 + 16, base1 - 32, base1 + 32);
                }
            }

            if position.bottom_edge() && !position.left_or_right() && !position.vertical_tile_boundary {
                post2_alternate(plane, base0 - 16, base0 + 48);
            }

            if left_delayed && position.top_edge() {
                let value = plane.get(base1 - 128 + 64);
                plane.set(base1 - 128, plane.get(base1 - 128).wrapping_add(value));
            }
            if right_delayed && position.top_edge() {
                corner.after[i][0] = plane.get(base1);
            }
            if position.right_edge() && position.top_edge() {
                let value = corner.after[i][0];
                plane.set(base1 - 128 + 64, plane.get(base1 - 128 + 64).wrapping_add(value));
            }
            if left_delayed && position.bottom_edge() {
                let value = plane.get(base0 - 128 + 112);
                plane.set(base0 - 128 + 48, plane.get(base0 - 128 + 48).wrapping_add(value));
            }
            if right_delayed && position.bottom_edge() {
                corner.after[i][1] = plane.get(base0 + 48);
            }
            if position.right_edge() && position.bottom_edge() {
                let value = corner.after[i][1];
                plane.set(base0 - 128 + 112, plane.get(base0 - 128 + 112).wrapping_add(value));
            }
        }

        if !position.top {
            let start = if position.left {
                112
            } else if left_delayed {
                -80
            } else {
                -16
            };
            let end = if position.right_edge() { 48 } else { 112 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base0 + j);
                j += 64;
            }
        }

        if !position.bottom {
            let start = if position.left {
                64
            } else if left_delayed {
                -128
            } else {
                -64
            };
            let end = if position.right_edge() { 0 } else { 64 };
            let mut j = start;
            while j < end {
                idct4x4_stage1(plane, base1 + j);
                idct4x4_stage1(plane, base1 + j + 16);
                idct4x4_stage1(plane, base1 + j + 32);
                j += 64;
            }
        }

        if params.overlap == Overlap::None {
            continue;
        }

        if position.top_edge() && left_delayed {
            post4_alternate(plane, base1 - 128, base1 - 127, base1 - 126, base1 - 125);
        }
        if position.top_edge() && position.right_edge() {
            post4_alternate(plane, base1 - 59, base1 - 60, base1 - 57, base1 - 58);
        }
        if position.bottom_edge() && left_delayed {
            post4_alternate(plane, base0 - 70, base0 - 69, base0 - 72, base0 - 71);
        }
        if position.bottom_edge() && position.right_edge() {
            post4_alternate(plane, base0 - 1, base0 - 2, base0 - 3, base0 - 4);
        }

        let start = if position.left { 0 } else { -128 };
        let end = if position.right_edge() { -64 } else { 0 };

        if !position.top {
            if left_delayed {
                let p = base0 + 32 + 10 - 128;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
            }

            if position.right_edge() {
                let p = base0 - 32 + 14;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1_alternate(plane, base0 + j + 32, 0);
                j += 64;
            }
        }

        if !position.bottom {
            if left_delayed {
                let p = base1 + 10 - 128;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
                let p = p + 16;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
            }

            if position.right_edge() {
                let p = base1 - 64 + 14;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
                let p = p + 16;
                post4_alternate(plane, p, p - 2, p + 6, p + 8);
                post4_alternate(plane, p + 1, p - 1, p + 7, p + 9);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1_alternate(plane, base1 + j, 0);
                post4x4_stage1_alternate(plane, base1 + j + 16, 0);
                j += 64;
            }
        }

        if position.top_or_bottom() || position.horizontal_tile_boundary {
            if position.top_edge() {
                let p = base1 + 5;
                let mut j = start;
                while j < end {
                    post4_alternate(plane, p + j, p + j - 1, p + j + 59, p + j + 60);
                    post4_alternate(plane, p + j + 2, p + j + 1, p + j + 61, p + j + 62);
                    j += 64;
                }
            }

            if position.bottom_edge() {
                let p = base0 + 48 + 13;
                let mut j = start;
                while j < end {
                    post4_alternate(plane, p + j, p + j - 1, p + j + 59, p + j + 60);
                    post4_alternate(plane, p + j + 2, p + j + 1, p + j + 61, p + j + 62);
                    j += 64;
                }
            }
        } else {
            if left_delayed {
                let j = -128isize;
                post4_alternate(plane, base0 + j + 58, base0 + j + 56, base1 + j, base1 + j + 2);
                post4_alternate(plane, base0 + j + 59, base0 + j + 57, base1 + j + 1, base1 + j + 3);
            }

            if position.right_edge() {
                let j = -60isize;
                post4_alternate(plane, base0 + j + 58, base0 + j + 56, base1 + j, base1 + j + 2);
                post4_alternate(plane, base0 + j + 59, base0 + j + 57, base1 + j + 1, base1 + j + 3);
            }

            let mut j = start;
            while j < end {
                post4x4_stage1_split_alternate(plane, base0 + j + 48, base1 + j, 0);
                j += 64;
            }
        }
    }
}
