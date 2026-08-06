use super::types::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct XyVal {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DimVal {
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PacketInclusion {
    pub band: usize,
    pub y: i32,
    pub subpkt: usize,
}

#[derive(Debug, Clone)]
pub struct Ids {
    pub ncomps: usize,
    pub w: i32,
    pub h: i32,
    pub comp_w: [i32; MAX_NCOMPS],
    pub comp_h: [i32; MAX_NCOMPS],

    pub nbands: usize,
    pub sd: usize,
    pub nb: usize,

    pub nlxy: XyVal,
    pub nlxyp: [XyVal; MAX_NCOMPS],
    pub band_idx: [[i32; MAX_NFILTER_TYPES]; MAX_NCOMPS],
    pub band_to_comp: Vec<usize>,
    pub band_to_filter: Vec<usize>,
    pub band_d: [[XyVal; MAX_NFILTER_TYPES]; MAX_NCOMPS],
    pub band_is_high: [XyVal; MAX_NFILTER_TYPES],
    pub band_dim: [[DimVal; MAX_NFILTER_TYPES]; MAX_NCOMPS],
    pub band_max_width: i32,

    pub cs: i32,
    pub npx: usize,
    pub npy: usize,
    pub pw: [i32; 2],
    pub ph: i32,
    pub pwb: [Vec<i32>; 2],
    pub l0: Vec<i32>,
    pub l1: [Vec<i32>; 2],

    pub pi: Vec<PacketInclusion>,
    pub npc: usize,

    pub use_long_precinct_headers: bool,
}

impl Ids {
    pub fn construct(image: &ImageParams, nlx: i32, nly: i32, sd: i32, cw: i32, lh: u8) -> Option<Ids> {
        let ncomps = image.ncomps;

        if ncomps == 0 || ncomps > MAX_NCOMPS {
            return None;
        }

        if image.width == 0 || image.height == 0 {
            return None;
        }

        let nlx = nlx.clamp(0, MAX_NDECOMP_H as i32);
        let nly = nly.clamp(0, MAX_NDECOMP_V as i32).min(nlx);
        let sd = sd.clamp(0, ncomps as i32);

        let nb = (2 * nly + nlx + 1) as usize;

        if nb > MAX_NFILTER_TYPES {
            return None;
        }

        let mut ids = Ids {
            ncomps,
            w: image.width as i32,
            h: image.height as i32,
            comp_w: [0; MAX_NCOMPS],
            comp_h: [0; MAX_NCOMPS],
            nbands: 0,
            sd: sd as usize,
            nb,
            nlxy: XyVal { x: nlx, y: nly },
            nlxyp: [XyVal::default(); MAX_NCOMPS],
            band_idx: [[-1; MAX_NFILTER_TYPES]; MAX_NCOMPS],
            band_to_comp: Vec::new(),
            band_to_filter: Vec::new(),
            band_d: [[XyVal::default(); MAX_NFILTER_TYPES]; MAX_NCOMPS],
            band_is_high: [XyVal::default(); MAX_NFILTER_TYPES],
            band_dim: [[DimVal::default(); MAX_NFILTER_TYPES]; MAX_NCOMPS],
            band_max_width: 0,
            cs: 0,
            npx: 0,
            npy: 0,
            pw: [0; 2],
            ph: 0,
            pwb: [Vec::new(), Vec::new()],
            l0: Vec::new(),
            l1: [Vec::new(), Vec::new()],
            pi: Vec::new(),
            npc: 0,
            use_long_precinct_headers: false,
        };

        ids.use_long_precinct_headers = !(ids.w.saturating_mul(ncomps as i32) < 32752 && lh == 0);

        for b in 1..=(nlx - nly) as usize {
            if b < MAX_NFILTER_TYPES {
                ids.band_is_high[b].x = 1;
            }
        }

        let mut b = (nlx - nly + 1) as usize;
        while b + 2 < ids.nb {
            ids.band_is_high[b].x = 1;
            ids.band_is_high[b + 1].y = 1;
            ids.band_is_high[b + 2].x = 1;
            ids.band_is_high[b + 2].y = 1;
            b += 3;
        }

        for c in 0..ncomps {
            let sx = if image.sx[c] == 2 { 2 } else { 1 };
            let sy = if image.sy[c] == 2 { 2 } else { 1 };

            ids.comp_w[c] = ids.w / sx;
            ids.comp_h[c] = ids.h / sy;

            if ids.comp_w[c] <= 0 || ids.comp_h[c] <= 0 {
                return None;
            }

            if c < ncomps - ids.sd {
                ids.nlxyp[c].x = nlx;
                ids.nlxyp[c].y = nly - (sy >> 1);
            }

            if ids.nlxyp[c].y < 0 {
                ids.nlxyp[c].y = 0;
            }

            ids.band_idx[c][0] = 1;
            ids.band_d[c][0].x = ids.nlxyp[c].x;
            ids.band_d[c][0].y = ids.nlxyp[c].y;

            for b in 1..=(ids.nlxyp[c].x - ids.nlxyp[c].y).max(0) as usize {
                if b >= MAX_NFILTER_TYPES {
                    break;
                }
                ids.band_idx[c][b] = 1;
                ids.band_d[c][b].x = ids.nlxyp[c].x + 1 - b as i32;
                ids.band_d[c][b].y = ids.nlxyp[c].y;
            }

            let lo = (ids.nlxyp[c].x - ids.nlxyp[c].y + 1).max(0) as usize;
            let hi = (ids.nb as i32 - 3 * ids.nlxyp[c].y).max(0) as usize;
            for b in lo..hi.min(MAX_NFILTER_TYPES) {
                ids.band_idx[c][b] = -1;
                ids.band_d[c][b].x = -1;
                ids.band_d[c][b].y = -1;
            }

            let mut i = 3 * ids.nlxyp[c].y;
            while i > 0 {
                let b = ids.nb as i32 - i;
                let d = (i + 2) / 3;
                if b >= 0 && (b as usize) < MAX_NFILTER_TYPES {
                    ids.band_idx[c][b as usize] = 1;
                    ids.band_d[c][b as usize].x = d;
                    ids.band_d[c][b as usize].y = d;
                }
                i -= 1;
            }
        }

        for b in 0..ids.nb {
            for c in 0..ncomps.saturating_sub(ids.sd) {
                if ids.band_idx[c][b] == 1 {
                    let idx = ids.nbands;
                    if idx >= MAX_NBANDS {
                        return None;
                    }
                    ids.nbands += 1;
                    ids.band_idx[c][b] = idx as i32;
                    ids.band_to_comp.push(c);
                    ids.band_to_filter.push(b);
                }
            }
        }

        for c in ncomps.saturating_sub(ids.sd)..ncomps {
            if ids.band_idx[c][0] == 1 {
                let idx = ids.nbands;
                if idx >= MAX_NBANDS {
                    return None;
                }
                ids.nbands += 1;
                ids.band_idx[c][0] = idx as i32;
                ids.band_to_comp.push(c);
                ids.band_to_filter.push(0);
            }
        }

        if ids.nbands == 0 {
            return None;
        }

        for c in 0..ncomps {
            let comp_w = ids.comp_w[c];
            let comp_h = ids.comp_h[c];

            for b in 0..ids.nb {
                if ids.band_idx[c][b] < 0 {
                    continue;
                }

                let dx = ids.band_d[c][b].x;
                let dy = ids.band_d[c][b].y;

                if !(0..=30).contains(&dx) || !(0..=30).contains(&dy) {
                    return None;
                }

                ids.band_dim[c][b].w = if ids.band_is_high[b].x != 0 {
                    let d = 1i32 << (dx - 1).max(0);
                    (comp_w + d - 1) / d / 2
                } else {
                    let d = 1i32 << dx;
                    (comp_w + d - 1) / d
                };

                ids.band_dim[c][b].h = if ids.band_is_high[b].y != 0 {
                    let d = 1i32 << (dy - 1).max(0);
                    (comp_h + d - 1) / d / 2
                } else {
                    let d = 1i32 << dy;
                    (comp_h + d - 1) / d
                };

                ids.band_max_width = ids.band_max_width.max(ids.band_dim[c][b].w);
            }
        }

        ids.cs = calculate_cs(image, nlx, cw);

        if ids.cs <= 0 {
            return None;
        }

        ids.ph = 1i32 << nly;
        ids.npx = ((ids.w + ids.cs - 1) / ids.cs).max(0) as usize;
        ids.npy = ((ids.h + ids.ph - 1) / ids.ph).max(0) as usize;

        if ids.npx == 0 || ids.npy == 0 || ids.npx > MAX_PREC_COLS {
            return None;
        }

        ids.pw[0] = ids.cs;
        ids.pw[1] = (ids.w - 1).rem_euclid(ids.cs) + 1;

        ids.pwb[0] = vec![0; ids.nbands];
        ids.pwb[1] = vec![0; ids.nbands];
        ids.l0 = vec![0; ids.nbands];
        ids.l1[0] = vec![0; ids.nbands];
        ids.l1[1] = vec![0; ids.nbands];

        for band in 0..ids.nbands {
            let c = ids.band_to_comp[band];
            let b = ids.band_to_filter[band];

            let sx_shift = (image.sx[c].max(1) - 1) as i32;
            let pw_in_b_0 = (ids.pw[0] + sx_shift) >> sx_shift;
            let pw_in_b_1 = (ids.pw[1] + sx_shift) >> sx_shift;

            if ids.band_is_high[b].x != 0 {
                let d = 1i32 << (ids.band_d[c][b].x - 1).max(0);
                ids.pwb[0][band] = (pw_in_b_0 + d - 1) / d / 2;
                ids.pwb[1][band] = (pw_in_b_1 + d - 1) / d / 2;
            } else {
                let d = 1i32 << ids.band_d[c][b].x.max(0);
                ids.pwb[0][band] = (pw_in_b_0 + d - 1) / d;
                ids.pwb[1][band] = (pw_in_b_1 + d - 1) / d;
            }

            if band < ids.nbands - ids.sd {
                let tmp = 1i32 << (ids.nlxyp[c].y - ids.band_d[c][b].y).max(0);
                ids.l0[band] = tmp * ids.band_is_high[b].y;
                ids.l1[0][band] = ids.l0[band] + tmp;
                ids.l1[1][band] = ids.l0[band] + (ids.band_dim[c][b].h - (ids.npy as i32 - 1) * tmp);
            } else {
                ids.l0[band] = 0;
                ids.l1[0][band] = ids.ph;
                ids.l1[1][band] = (ids.band_dim[c][b].h + ids.ph - 1).rem_euclid(ids.ph.max(1)) + 1;
            }

            if ids.l1[1][band] < ids.l0[band] {
                ids.l1[1][band] = ids.l0[band];
            }
        }

        ids.compute_packet_inclusion();

        if ids.pi.is_empty() {
            return None;
        }

        Some(ids)
    }

    fn compute_packet_inclusion(&mut self) {
        let mut s: usize = 0;
        let beta1 = self.nlxy.x - self.nlxy.y + 1;

        for beta in 0..beta1.max(0) as usize {
            for i in 0..self.ncomps.saturating_sub(self.sd) {
                if beta >= MAX_NFILTER_TYPES {
                    continue;
                }

                let b = self.band_idx[i][beta];
                if b < 0 {
                    continue;
                }

                self.pi.push(PacketInclusion { band: b as usize, y: 0, subpkt: s });
            }
        }

        let mut beta0 = beta1.max(0) as usize;
        while beta0 < self.nb {
            let nlines = 1i32 << (self.nlxy.y - self.band_d[0][beta0.min(MAX_NFILTER_TYPES - 1)].y).clamp(0, 30);

            for lambda in 0..nlines {
                for beta in beta0..(beta0 + 3).min(self.nb) {
                    if beta >= MAX_NFILTER_TYPES {
                        continue;
                    }

                    let mut r = 1usize;

                    for i in 0..self.ncomps.saturating_sub(self.sd) {
                        let b = self.band_idx[i][beta];
                        if b < 0 {
                            continue;
                        }

                        let b = b as usize;
                        if b >= self.l0.len() {
                            continue;
                        }

                        if self.l0[b] + lambda < self.l1[0][b] {
                            s += r;
                            self.pi.push(PacketInclusion { band: b, y: self.l0[b] + lambda, subpkt: s });
                            r = 0;
                        }
                    }
                }
            }

            beta0 += 3;
        }

        for lambda in 0..self.ph {
            for i in self.ncomps.saturating_sub(self.sd)..self.ncomps {
                let b = self.band_idx[i][0];
                if b < 0 {
                    continue;
                }

                let b = b as usize;
                if b >= self.l0.len() {
                    continue;
                }

                if self.l0[b] + lambda < self.l1[0][b] {
                    s += 1;
                    self.pi.push(PacketInclusion { band: b, y: self.l0[b] + lambda, subpkt: s });
                }
            }
        }

        if self.pi.len() > MAX_PACKETS {
            self.pi.truncate(MAX_PACKETS);
        }

        self.npc = s + 1;
    }

    pub fn band_height_in_precinct(&self, band: usize, is_last_y: bool) -> i32 {
        let idx = if is_last_y { 1 } else { 0 };
        match (self.l1[idx].get(band), self.l0.get(band)) {
            (Some(&l1), Some(&l0)) => (l1 - l0).max(0),
            _ => 0,
        }
    }

    pub fn band_width_in_precinct(&self, band: usize, is_last_column: bool) -> i32 {
        let idx = if is_last_column { 1 } else { 0 };
        self.pwb[idx].get(band).copied().unwrap_or(0).max(0)
    }
}

fn calculate_cs(image: &ImageParams, ndecomp_h: i32, cw: i32) -> i32 {
    if cw > 0 {
        let max_sx = (0..image.ncomps).map(|c| image.sx[c] as i32).max().unwrap_or(1);
        let shift = ndecomp_h.clamp(0, 24);
        return 8i32.saturating_mul(cw).saturating_mul(max_sx).saturating_mul(1 << shift);
    }

    image.width as i32
}
