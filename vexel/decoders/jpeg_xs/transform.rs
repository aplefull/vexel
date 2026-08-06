use super::ids::Ids;
use super::simd;
use super::types::*;

#[derive(Default)]
struct DwtScratch {
    evens: Vec<i32>,
    odds: Vec<i32>,
}

impl DwtScratch {
    fn split(&mut self, len: usize) -> (&mut Vec<i32>, &mut Vec<i32>) {
        let half = len / 2 + 1;

        if self.evens.len() < half {
            self.evens.resize(half, 0);
        }
        if self.odds.len() < half {
            self.odds.resize(half, 0);
        }

        (&mut self.evens, &mut self.odds)
    }
}

fn inverse_filter_1d(buf: &mut [i32], start: usize, len: usize, inc: usize, scratch: &mut DwtScratch) {
    if len <= 1 || inc == 0 {
        return;
    }

    if start >= buf.len() {
        return;
    }

    let reachable = (buf.len() - start).div_ceil(inc);
    let len = len.min(reachable);

    if len <= 1 {
        return;
    }

    if inc == 1 {
        return inverse_filter_contiguous(&mut buf[start..start + len], scratch);
    }

    inverse_filter_strided(buf, start, len, inc, scratch);
}

fn inverse_filter_contiguous(buf: &mut [i32], scratch: &mut DwtScratch) {
    let len = buf.len();
    let n_even = len.div_ceil(2);
    let n_odd = len / 2;

    let (evens, odds) = scratch.split(len);

    simd::deinterleave_i32(buf, &mut evens[..n_even], &mut odds[..n_odd]);

    apply_lifting(&mut evens[..n_even], &mut odds[..n_odd], len);

    simd::interleave_i32(&evens[..n_even], &odds[..n_odd], buf);
}

fn inverse_filter_strided(buf: &mut [i32], start: usize, len: usize, inc: usize, scratch: &mut DwtScratch) {
    let n_even = len.div_ceil(2);
    let n_odd = len / 2;

    let (evens, odds) = scratch.split(len);

    simd::gather_stride_split(buf, start, inc, &mut evens[..n_even], &mut odds[..n_odd]);

    apply_lifting(&mut evens[..n_even], &mut odds[..n_odd], len);

    simd::scatter_stride_split(buf, start, inc, &evens[..n_even], &odds[..n_odd]);
}

fn apply_lifting(evens: &mut [i32], odds: &mut [i32], len: usize) {
    let n_even = evens.len();
    let n_odd = odds.len();

    if n_odd == 0 || n_even == 0 {
        return;
    }

    evens[0] = evens[0].wrapping_sub((odds[0].wrapping_add(1)) >> 1);

    let even_is_last = len % 2 == 1;
    let predict_end = if even_is_last { n_even - 1 } else { n_even };

    if predict_end > 1 {
        simd::dwt_predict_interior(&mut evens[1..predict_end], odds);
    }

    if even_is_last && n_even > 1 {
        evens[n_even - 1] = evens[n_even - 1].wrapping_sub((odds[n_odd - 1].wrapping_add(1)) >> 1);
    }

    let update_end = if even_is_last { n_odd } else { n_odd - 1 };

    if update_end > 0 {
        simd::dwt_update_interior(&mut odds[..update_end], evens);
    }

    if !even_is_last {
        odds[n_odd - 1] = odds[n_odd - 1].wrapping_add(evens[n_even - 1]);
    }
}

fn transform_horizontal(
    ids: &Ids,
    plane: &mut [i32],
    comp: usize,
    h_level: i32,
    v_level: i32,
    scratch: &mut DwtScratch,
) {
    let comp_w = ids.comp_w[comp].max(0) as usize;
    let comp_h = ids.comp_h[comp].max(0) as usize;

    if comp_w == 0 || comp_h == 0 {
        return;
    }

    let x_inc = 1usize << h_level.clamp(0, 24);
    let y_inc = comp_w << v_level.clamp(0, 24);

    if y_inc == 0 {
        return;
    }

    let row_len = comp_w.div_ceil(x_inc);

    let mut base = 0usize;
    while base < comp_w * comp_h {
        inverse_filter_1d(plane, base, row_len, x_inc, scratch);
        base += y_inc;
    }
}

fn transform_vertical(ids: &Ids, plane: &mut [i32], comp: usize, h_level: i32, v_level: i32, scratch: &mut DwtScratch) {
    let comp_w = ids.comp_w[comp].max(0) as usize;
    let comp_h = ids.comp_h[comp].max(0) as usize;

    if comp_w == 0 || comp_h == 0 {
        return;
    }

    let x_inc = 1usize << h_level.clamp(0, 24);
    let y_inc = comp_w << v_level.clamp(0, 24);

    if y_inc == 0 {
        return;
    }

    let col_len = (comp_w * comp_h).div_ceil(y_inc);

    if x_inc == 1 {
        return transform_vertical_rows(plane, comp_w, col_len, y_inc);
    }

    let mut base = 0usize;
    while base < comp_w {
        inverse_filter_1d(plane, base, col_len, y_inc, scratch);
        base += x_inc;
    }
}

fn transform_vertical_rows(plane: &mut [i32], comp_w: usize, col_len: usize, y_inc: usize) {
    let col_len = col_len.min(plane.len().div_ceil(y_inc));

    if col_len <= 1 || comp_w == 0 {
        return;
    }

    let last = col_len - 1;
    let even_is_last = col_len % 2 == 1;

    row_pair_op(plane, 0, 1, comp_w, y_inc, LineOp::PredictEdge);

    let predict_end = if even_is_last { last } else { col_len };

    let mut i = 2;
    while i + 1 < predict_end {
        row_triple_op(plane, i, comp_w, y_inc, true);
        i += 2;
    }

    if even_is_last {
        row_pair_op(plane, last, last - 1, comp_w, y_inc, LineOp::PredictEdge);
    }

    let mut i = 1;
    while i + 1 < col_len {
        row_triple_op(plane, i, comp_w, y_inc, false);
        i += 2;
    }

    if !even_is_last {
        row_pair_op(plane, last, last - 1, comp_w, y_inc, LineOp::UpdateEdge);
    }
}

enum LineOp {
    PredictEdge,
    UpdateEdge,
}

fn row_pair_op(plane: &mut [i32], cur_row: usize, other_row: usize, comp_w: usize, y_inc: usize, op: LineOp) {
    let cur_off = cur_row * y_inc;
    let other_off = other_row * y_inc;

    if cur_off >= plane.len() || other_off >= plane.len() {
        return;
    }

    let n = comp_w.min(plane.len() - cur_off).min(plane.len() - other_off);

    if cur_off < other_off {
        let (a, b) = plane.split_at_mut(other_off);
        let cur = &mut a[cur_off..cur_off + n];
        let other = &b[..n];
        apply_edge(cur, other, op);
    } else {
        let (a, b) = plane.split_at_mut(cur_off);
        let other = &a[other_off..other_off + n];
        let cur = &mut b[..n];
        apply_edge(cur, other, op);
    }
}

fn apply_edge(cur: &mut [i32], other: &[i32], op: LineOp) {
    match op {
        LineOp::PredictEdge => {
            for (c, &o) in cur.iter_mut().zip(other) {
                *c = c.wrapping_sub((o.wrapping_add(1)) >> 1);
            }
        }
        LineOp::UpdateEdge => {
            for (c, &o) in cur.iter_mut().zip(other) {
                *c = c.wrapping_add(o);
            }
        }
    }
}

fn row_triple_op(plane: &mut [i32], cur_row: usize, comp_w: usize, y_inc: usize, predict: bool) {
    let top_off = (cur_row - 1) * y_inc;
    let cur_off = cur_row * y_inc;
    let bottom_off = (cur_row + 1) * y_inc;

    if bottom_off >= plane.len() {
        return;
    }

    let n = comp_w.min(plane.len() - bottom_off);

    let (head, tail) = plane.split_at_mut(bottom_off);
    let bottom = &tail[..n];

    let (top_part, cur_part) = head.split_at_mut(cur_off);
    let top = &top_part[top_off..top_off + n];
    let cur = &mut cur_part[..n];

    if predict {
        simd::dwt_predict_lines(cur, top, bottom);
    } else {
        simd::dwt_update_lines(cur, top, bottom);
    }
}

pub fn dwt_inverse(ids: &Ids, planes: &mut [Vec<i32>]) {
    let mut scratch = DwtScratch::default();

    for comp in 0..ids.ncomps.saturating_sub(ids.sd) {
        let Some(plane) = planes.get_mut(comp) else {
            continue;
        };

        let nlx = ids.nlxyp[comp].x;
        let nly = ids.nlxyp[comp].y;

        let mut d = nlx - 1;
        while d >= nly {
            transform_horizontal(ids, plane, comp, d, nly, &mut scratch);
            d -= 1;
        }

        let mut d = nly - 1;
        while d >= 0 {
            transform_horizontal(ids, plane, comp, d, d, &mut scratch);
            transform_vertical(ids, plane, comp, d, d, &mut scratch);
            d -= 1;
        }
    }
}

pub fn mct_inverse_rct(planes: &mut [Vec<i32>], width: usize, height: usize) {
    if planes.len() < 3 {
        return;
    }

    let len = width.saturating_mul(height);
    let len = len.min(planes[0].len()).min(planes[1].len()).min(planes[2].len());

    let (first, rest) = planes.split_at_mut(1);
    let (second, third) = rest.split_at_mut(1);

    simd::mct_rct(&mut first[0][..len], &mut second[0][..len], &mut third[0][..len]);
}

fn tetrix_access(
    comp: usize,
    cf: TetrixMode,
    ct: i32,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (usize, usize) {
    let c = comp as i32;

    let mut t_x = rx + ((ct + c) & 1);
    let mut t_y = ry + ((!c >> 1) & 1);

    let k = (((!t_y) << 1) & 2) | ((ct ^ t_x) & 1);
    let k = k.clamp(0, 3) as usize;

    if cf == TetrixMode::Inline {
        t_y &= 1;
    }

    t_x += x << 1;
    t_y += y << 1;

    if t_x < 0 {
        t_x += 2;
    } else if t_x >= width << 1 {
        t_x -= 2;
    }

    if t_y < 0 {
        t_y += 2;
    } else if t_y >= height << 1 {
        t_y -= 2;
    }

    t_x >>= 1;
    t_y >>= 1;

    let t_x = t_x.clamp(0, width.saturating_sub(1)).max(0) as usize;
    let t_y = t_y.clamp(0, height.saturating_sub(1)).max(0) as usize;

    (k, t_y * width.max(0) as usize + t_x)
}

pub fn mct_inverse_tetrix(planes: &mut [Vec<i32>], width: usize, height: usize, cfa: CfaPattern, params: TetrixParams) {
    if planes.len() < 4 || width == 0 || height == 0 {
        return;
    }

    let w = width as i32;
    let h = height as i32;
    let cf = params.cf;
    let ct = if matches!(cfa, CfaPattern::Rggb | CfaPattern::Bggr) { 0 } else { 1 };
    let e1 = params.e1.min(24) as i32;
    let e2 = params.e2.min(24) as i32;

    let fetch = |planes: &[Vec<i32>], slot: (usize, usize)| -> i32 {
        planes.get(slot.0).and_then(|p| p.get(slot.1)).copied().unwrap_or(0)
    };

    for y in 0..h {
        for x in 0..w {
            let tl = tetrix_access(0, cf, ct, -1, -1, x, y, w, h);
            let tr = tetrix_access(0, cf, ct, 1, -1, x, y, w, h);
            let bl = tetrix_access(0, cf, ct, -1, 1, x, y, w, h);
            let br = tetrix_access(0, cf, ct, 1, 1, x, y, w, h);

            let sum = fetch(planes, tl)
                .wrapping_add(fetch(planes, tr))
                .wrapping_add(fetch(planes, bl))
                .wrapping_add(fetch(planes, br));

            let pos = y as usize * width + x as usize;
            if let Some(slot) = planes.get_mut(0).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_sub(sum >> 3);
            }
        }
    }

    for y in 0..h {
        for x in 0..w {
            let tl = tetrix_access(3, cf, ct, -1, -1, x, y, w, h);
            let tr = tetrix_access(3, cf, ct, 1, -1, x, y, w, h);
            let bl = tetrix_access(3, cf, ct, -1, 1, x, y, w, h);
            let br = tetrix_access(3, cf, ct, 1, 1, x, y, w, h);

            let sum = fetch(planes, tl)
                .wrapping_add(fetch(planes, tr))
                .wrapping_add(fetch(planes, bl))
                .wrapping_add(fetch(planes, br));

            let pos = y as usize * width + x as usize;
            if let Some(slot) = planes.get_mut(3).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_add(sum >> 2);
            }
        }
    }

    for y in 0..h {
        for x in 0..w {
            let bl = tetrix_access(0, cf, ct, -1, 0, x, y, w, h);
            let br = tetrix_access(0, cf, ct, 1, 0, x, y, w, h);
            let rt = tetrix_access(0, cf, ct, 0, -1, x, y, w, h);
            let rb = tetrix_access(0, cf, ct, 0, 1, x, y, w, h);

            let value = ((fetch(planes, bl).wrapping_add(fetch(planes, br))) << e2)
                .wrapping_add((fetch(planes, rt).wrapping_add(fetch(planes, rb))) << e1)
                >> 3;

            let pos = y as usize * width + x as usize;
            if let Some(slot) = planes.get_mut(0).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_sub(value);
            }

            let bt = tetrix_access(3, cf, ct, 0, -1, x, y, w, h);
            let bb = tetrix_access(3, cf, ct, 0, 1, x, y, w, h);
            let rl = tetrix_access(3, cf, ct, -1, 0, x, y, w, h);
            let rr = tetrix_access(3, cf, ct, 1, 0, x, y, w, h);

            let value = ((fetch(planes, bt).wrapping_add(fetch(planes, bb))) << e2)
                .wrapping_add((fetch(planes, rl).wrapping_add(fetch(planes, rr))) << e1)
                >> 3;

            if let Some(slot) = planes.get_mut(3).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_sub(value);
            }
        }
    }

    for y in 0..h {
        for x in 0..w {
            let gl = tetrix_access(1, cf, ct, -1, 0, x, y, w, h);
            let gr = tetrix_access(1, cf, ct, 1, 0, x, y, w, h);
            let gt = tetrix_access(1, cf, ct, 0, -1, x, y, w, h);
            let gb = tetrix_access(1, cf, ct, 0, 1, x, y, w, h);

            let sum = fetch(planes, gl)
                .wrapping_add(fetch(planes, gr))
                .wrapping_add(fetch(planes, gt))
                .wrapping_add(fetch(planes, gb));

            let pos = y as usize * width + x as usize;
            if let Some(slot) = planes.get_mut(1).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_add(sum >> 2);
            }

            let gl = tetrix_access(2, cf, ct, 0, -1, x, y, w, h);
            let gr = tetrix_access(2, cf, ct, 0, 1, x, y, w, h);
            let gt = tetrix_access(2, cf, ct, -1, 0, x, y, w, h);
            let gb = tetrix_access(2, cf, ct, 1, 0, x, y, w, h);

            let sum = fetch(planes, gl)
                .wrapping_add(fetch(planes, gr))
                .wrapping_add(fetch(planes, gt))
                .wrapping_add(fetch(planes, gb));

            if let Some(slot) = planes.get_mut(2).and_then(|p| p.get_mut(pos)) {
                *slot = slot.wrapping_add(sum >> 2);
            }
        }
    }

    planes.swap(0, 2);
    planes.swap(1, 3);
}

fn clamp_to(value: i32, max_value: i32) -> i32 {
    value.clamp(0, max_value)
}

fn clamp64_to(value: i64, max_value: i64) -> i64 {
    value.clamp(0, max_value)
}

pub fn nlt_inverse(planes: &mut [Vec<i32>], config: &Config, image: &ImageParams, ids: &Ids) {
    let depth = image.depth.clamp(1, 31);
    let bw = config.bw.clamp(1, 31);

    match config.tnlt {
        NltType::None => nlt_inverse_linear(planes, bw, depth, image, ids),
        NltType::Quadratic => nlt_inverse_quadratic(planes, bw, depth, config, image, ids),
        NltType::Extended => nlt_inverse_extended(planes, bw, depth, config, image, ids),
    }
}

fn sample_count(image: &ImageParams, ids: &Ids, comp: usize) -> usize {
    let w = ids.comp_w.get(comp).copied().unwrap_or(0).max(0) as usize;
    let h = ids.comp_h.get(comp).copied().unwrap_or(0).max(0) as usize;
    let _ = image;
    w.saturating_mul(h)
}

fn nlt_inverse_linear(planes: &mut [Vec<i32>], bw: u8, depth: u8, image: &ImageParams, ids: &Ids) {
    let s = bw.saturating_sub(depth).min(31);
    let dclev_and_rounding = ((1i64 << bw) >> 1) + ((1i64 << s) >> 1);
    let max_val = ((1i64 << depth) - 1) as i32;

    for comp in 0..image.ncomps.min(planes.len()) {
        let count = sample_count(image, ids, comp).min(planes[comp].len());

        simd::nlt_linear(&mut planes[comp][..count], dclev_and_rounding, s as u32, max_val);
    }
}

fn nlt_inverse_quadratic(planes: &mut [Vec<i32>], bw: u8, depth: u8, config: &Config, image: &ImageParams, ids: &Ids) {
    let vdco = config.quadratic.alpha as i32 - config.quadratic.sigma as i32 * 32768;
    let s = ((bw as u32) << 1).saturating_sub(depth as u32).min(62);
    let dclev = (1i32 << bw) >> 1;
    let s_r = (1i64 << s) >> 1;
    let max_val = (1i64 << depth) - 1;
    let max_coef = (1i32 << bw) - 1;

    for comp in 0..image.ncomps.min(planes.len()) {
        let count = sample_count(image, ids, comp).min(planes[comp].len());

        for value in &mut planes[comp][..count] {
            let v = clamp_to(value.wrapping_add(dclev), max_coef) as i64;
            let out = ((v * v + s_r) >> s) + vdco as i64;
            *value = clamp64_to(out, max_val) as i32;
        }
    }
}

fn nlt_inverse_extended(planes: &mut [Vec<i32>], bw: u8, depth: u8, config: &Config, image: &ImageParams, ids: &Ids) {
    let e = bw.saturating_sub(config.extended.e).clamp(1, 31) as u32;
    let t1 = config.extended.t1 as i64;
    let t2 = config.extended.t2 as i64;

    let b2 = t1.saturating_mul(t1);
    let a1 = b2 + (t1 << e.min(40)) + (1i64 << (2 * e).saturating_sub(2).min(62));
    let b1 = t1 + (1i64 << e.saturating_sub(1).min(62));
    let a3 = b2 + (t2 << e.min(40)) - (1i64 << (2 * e).saturating_sub(2).min(62));
    let b3 = t2 - (1i64 << e.saturating_sub(1).min(62));

    let s = ((bw as u32) << 1).saturating_sub(depth as u32).min(62);
    let s_r = (1i64 << s) >> 1;
    let max_val = (1i64 << depth) - 1;
    let max_coef = (1i64 << bw) - 1;
    let dclev = (1i32 << bw) >> 1;

    for comp in 0..image.ncomps.min(planes.len()) {
        let count = sample_count(image, ids, comp).min(planes[comp].len());

        for value in &mut planes[comp][..count] {
            let mut v = (*value as i64).wrapping_add(dclev as i64);

            if v < t1 {
                v = clamp64_to(b1 - v, max_coef);
                v = a1 - v.saturating_mul(v);
            } else if v < t2 {
                v = (v << e.min(40)) + b2;
            } else {
                v = clamp64_to(v - b3, max_coef);
                v = a3 + v.saturating_mul(v);
            }

            *value = clamp64_to((v + s_r) >> s, max_val) as i32;
        }
    }
}
