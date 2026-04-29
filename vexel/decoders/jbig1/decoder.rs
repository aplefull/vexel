use crate::bitreader::BitReader;
use crate::decoders::jbig1::arithmetic::ArithDecoder;
use crate::decoders::jbig1::types::{self, *};
use crate::utils::error::VexelResult;
use crate::utils::info::Jbig1Info;
use crate::{log_warn, Image, PixelData};
use std::io::{Read, Seek};

fn ceil_half(x: u32, n: u32) -> u32 {
    (x + (1 << n) - 1) >> n
}

pub struct Jbig1Decoder<R: Read + Seek> {
    dl: u8,
    d: u8,
    planes: u8,
    xd: u32,
    yd: u32,
    l0: u32,
    mx: u8,
    my: u8,
    order: u8,
    options: u8,

    ar_decoders: Vec<Vec<ArithDecoder>>,
    tx: Vec<Vec<i32>>,
    ty: Vec<Vec<i32>>,
    reset_flags: Vec<Vec<bool>>,
    lntp: Vec<Vec<bool>>,

    lhp: Vec<Vec<Vec<u8>>>,

    dppriv: Option<Vec<u8>>,

    ii: [u32; 3],

    current_stripe: u32,
    current_layer: u8,
    current_plane: u8,
    current_line: u32,
    current_x: u32,
    pseudo: bool,

    line_h1: u32,
    line_h2: u32,
    line_h3: u32,
    line_l1: u32,
    line_l2: u32,
    line_l3: u32,

    reader: BitReader<R>,
}

impl<R: Read + Seek> Jbig1Decoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            dl: 0,
            d: 0,
            planes: 1,
            xd: 0,
            yd: 0,
            l0: 0,
            mx: 0,
            my: 0,
            order: 0,
            options: 0,
            ar_decoders: Vec::new(),
            tx: Vec::new(),
            ty: Vec::new(),
            reset_flags: Vec::new(),
            lntp: Vec::new(),
            lhp: Vec::new(),
            dppriv: None,
            ii: [0u32; 3],
            current_stripe: 0,
            current_layer: 0,
            current_plane: 0,
            current_line: 0,
            current_x: 0,
            pseudo: true,
            line_h1: 0,
            line_h2: 0,
            line_h3: 0,
            line_l1: 0,
            line_l2: 0,
            line_l3: 0,
            reader: BitReader::new(reader),
        }
    }

    pub fn get_info(&self) -> Jbig1Info {
        Jbig1Info {
            width: self.xd,
            height: self.yd,
            planes: self.planes,
            dl: self.dl,
            d: self.d,
            l0: self.l0,
            mx: self.mx,
            my: self.my,
            order: self.order,
            options: self.options,
        }
    }

    fn read_bih(&mut self) -> VexelResult<()> {
        let mut bih = [0u8; 20];
        self.reader.read_exact(&mut bih)?;

        self.dl = bih[0];
        self.d = bih[1];
        self.planes = bih[2];
        self.xd = u32::from_be_bytes([bih[4], bih[5], bih[6], bih[7]]);
        self.yd = u32::from_be_bytes([bih[8], bih[9], bih[10], bih[11]]);
        self.l0 = u32::from_be_bytes([bih[12], bih[13], bih[14], bih[15]]);
        self.mx = bih[16];
        self.my = bih[17];
        self.order = bih[18];
        self.options = bih[19];

        if self.dl > self.d {
            log_warn!("JBIG1: DL ({}) > D ({}), clamping DL to D", self.dl, self.d);
            self.dl = self.d;
        }

        if self.planes == 0 {
            log_warn!("JBIG1: planes=0, defaulting to 1");
            self.planes = 1;
        }

        if self.xd == 0 || self.yd == 0 {
            log_warn!("JBIG1: zero dimensions ({}x{})", self.xd, self.yd);
        }

        if self.l0 == 0 {
            log_warn!("JBIG1: L0=0, defaulting to full image height at layer 0");
            self.l0 = ceil_half(self.yd, self.d as u32);
            if self.l0 == 0 {
                self.l0 = 1;
            }
        }

        let num_layers = (self.d - self.dl + 1) as usize;
        let planes = self.planes as usize;

        self.ar_decoders = (0..planes).map(|_| (0..num_layers).map(|_| ArithDecoder::new()).collect()).collect();
        self.tx = vec![vec![0i32; num_layers]; planes];
        self.ty = vec![vec![0i32; num_layers]; planes];
        self.reset_flags = vec![vec![true; num_layers]; planes];
        self.lntp = vec![vec![true; num_layers]; planes];

        self.lhp = vec![vec![Vec::new(); planes]; 2];

        let order = self.order & 0x07;
        let iindex = Self::iindex_for_order(order);
        self.ii[iindex[0]] = 0;
        self.ii[iindex[1]] = self.dl as u32;
        self.ii[iindex[2]] = 0;
        self.current_stripe = 0;
        self.current_layer = self.dl;
        self.current_plane = 0;
        self.current_line = 0;
        self.current_x = 0;
        self.pseudo = true;

        Ok(())
    }

    fn allocate_layer_buffers(&mut self) {
        let planes = self.planes as usize;
        for layer in self.dl..=self.d {
            let hx = ceil_half(self.xd, (self.d - layer) as u32);
            let hy = ceil_half(self.yd, (self.d - layer) as u32);
            let hbpl = ((hx + 7) / 8) as usize;
            let total = hbpl * hy as usize;
            for plane in 0..planes {
                self.lhp[layer as usize & 1][plane] = vec![0u8; total];
            }
        }
    }

    fn read_all_data(&mut self) -> VexelResult<Vec<u8>> {
        let data = self.reader.read_to_end()?;
        Ok(data)
    }

    fn stripes_for_layer(&self, layer: u8) -> u32 {
        let hy = ceil_half(self.yd, (self.d - layer) as u32);
        let hl = self.l0 << layer;
        (hy + hl - 1) / hl
    }

    fn process_stream(&mut self, data: &[u8]) -> VexelResult<()> {
        let mut pos = 0;
        let mut sde_start = 0;
        let mut at_moves: Vec<AtMove> = Vec::new();
        let mut aborted = false;

        while pos < data.len() {
            if data[pos] != MARKER_ESC {
                pos += 1;
                continue;
            }

            if pos + 1 >= data.len() {
                break;
            }

            let marker = data[pos + 1];

            if marker == MARKER_STUFF {
                pos += 2;
                continue;
            }

            let sde_data = &data[sde_start..pos];

            match marker {
                MARKER_SDNORM | MARKER_SDRST => {
                    let is_reset = marker == MARKER_SDRST;

                    {
                        let plane = self.current_plane as usize;
                        let layer = self.current_layer as usize;
                        if std::env::var("JBIG1_DEBUG").is_ok() {
                            eprintln!("SDE plane={} layer={} stripe={}  len={}", plane, layer, self.current_stripe, sde_data.len());
                        }
                        let at_moves_for_stripe = std::mem::take(&mut at_moves);
                        self.decode_pscd(plane, layer, sde_data, &at_moves_for_stripe);
                    }

                    let plane = self.current_plane as usize;
                    let layer_idx = (self.current_layer - self.dl) as usize;

                    let total_stripes = self.stripes_for_layer(self.current_layer);
                    let is_last_stripe = self.current_stripe + 1 >= total_stripes;
                    let reuse_st = !is_last_stripe && !is_reset;
                    self.ar_decoders[plane][layer_idx].reset(reuse_st);

                    self.reset_flags[plane][layer_idx] = is_reset;
                    if is_reset {
                        self.tx[plane][layer_idx] = 0;
                        self.ty[plane][layer_idx] = 0;
                    }

                    self.advance_stripe();
                    self.pseudo = true;
                    self.current_line = 0;
                    self.current_x = 0;
                    self.line_h1 = 0;
                    self.line_h2 = 0;
                    self.line_h3 = 0;
                    self.line_l1 = 0;
                    self.line_l2 = 0;
                    self.line_l3 = 0;

                    pos += 2;
                    sde_start = pos;
                    at_moves = Vec::new();
                }
                MARKER_ATMOVE => {
                    if pos + 7 >= data.len() {
                        log_warn!("JBIG1: truncated ATMOVE marker");
                        break;
                    }
                    let line = u32::from_be_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]]);
                    let tx = data[pos + 6] as i8;
                    let ty = data[pos + 7] as i8;
                    at_moves.push(AtMove { line, tx, ty });
                    pos += 8;
                }
                MARKER_NEWLEN => {
                    if pos + 5 >= data.len() {
                        log_warn!("JBIG1: truncated NEWLEN marker");
                        break;
                    }
                    if self.options & OPT_VLENGTH != 0 {
                        let new_yd = u32::from_be_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]]);
                        log_warn!("JBIG1: NEWLEN updating height from {} to {}", self.yd, new_yd);
                        self.yd = new_yd;
                        self.allocate_layer_buffers();
                    }
                    pos += 6;
                }
                MARKER_COMMENT => {
                    if pos + 5 >= data.len() {
                        log_warn!("JBIG1: truncated COMMENT marker");
                        break;
                    }
                    let comment_len = u32::from_be_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]]) as usize;
                    pos += 6 + comment_len;
                    if pos > data.len() {
                        pos = data.len();
                    }
                    sde_start = pos;
                }
                MARKER_ABORT => {
                    log_warn!("JBIG1: ABORT marker encountered, stopping decode");
                    aborted = true;
                    break;
                }
                _ => {
                    log_warn!("JBIG1: unknown marker 0xFF {:02X}, skipping", marker);
                    pos += 2;
                }
            }
        }

        if !aborted && sde_start < data.len() {
            let remaining = &data[sde_start..];
            if !remaining.is_empty() {
                let plane = self.current_plane as usize;
                let layer = self.current_layer as usize;
                self.decode_pscd(plane, layer, remaining, &at_moves);
            }
        }

        Ok(())
    }

    fn iindex_for_order(order: u8) -> [usize; 3] {
        match order & 0x07 {
            0 => [2, 1, 0],
            2 => [2, 0, 1],
            3 => [1, 0, 2],
            4 => [0, 2, 1],
            5 => [1, 2, 0],
            6 => [0, 1, 2],
            _ => [1, 0, 2],
        }
    }

    fn advance_stripe(&mut self) {
        let order = self.order & 0x07;
        let iindex = Self::iindex_for_order(order);

        let stripes = self.stripes_for_layer(self.dl);

        let mut is = [0u32; 3];
        let mut ie = [0u32; 3];
        is[iindex[0]] = 0;
        ie[iindex[0]] = stripes - 1;
        is[iindex[1]] = self.dl as u32;
        ie[iindex[1]] = self.d as u32;
        is[iindex[2]] = 0;
        ie[iindex[2]] = self.planes as u32 - 1;

        let mut i = 2i32;
        loop {
            self.ii[i as usize] += 1;
            if self.ii[i as usize] > ie[i as usize] {
                self.ii[i as usize] = is[i as usize];
                i -= 1;
                if i < 0 {
                    break;
                }
            } else {
                break;
            }
        }

        self.current_stripe = self.ii[iindex[0]];
        self.current_layer = self.ii[iindex[1]] as u8;
        self.current_plane = self.ii[iindex[2]] as u8;
    }

    fn decode_pscd(&mut self, plane: usize, layer: usize, data: &[u8], at_moves: &[AtMove]) {
        if layer > self.d as usize {
            return;
        }

        if layer == self.dl as usize {
            self.decode_layer0_pscd(plane, data, at_moves);
        } else {
            self.decode_diff_layer_pscd(plane, layer, data, at_moves);
        }
    }

    fn decode_diff_layer_pscd(&mut self, plane: usize, layer: usize, data: &[u8], at_moves: &[AtMove]) {
        let layer_idx = layer - self.dl as usize;

        let hx = ceil_half(self.xd, (self.d as u32).saturating_sub(layer as u32));
        let hy = ceil_half(self.yd, (self.d as u32).saturating_sub(layer as u32));
        let hbpl = ((hx + 7) / 8) as usize;

        let lx = ceil_half(hx, 1);
        let ly = ceil_half(hy, 1);
        let lbpl = ((lx + 7) / 8) as usize;

        let hl = (self.l0 << layer) as usize;
        let ll = hl >> 1;

        let stripe = self.current_stripe as usize;

        let lhp_hi = layer & 1;
        let lhp_lo = (layer - 1) & 1;

        {
            let total_hi = hbpl * hy as usize;
            if self.lhp[lhp_hi][plane].len() < total_hi {
                self.lhp[lhp_hi][plane].resize(total_hi, 0);
            }
        }

        let total_lo = lbpl * ly as usize;
        if self.lhp[lhp_lo][plane].len() < total_lo {
            self.lhp[lhp_lo][plane].resize(total_lo, 0);
        }

        let stripe_start_line = stripe * hl;
        let stripe_end_line = ((stripe + 1) * hl).min(hy as usize);

        if stripe_start_line >= hy as usize {
            return;
        }

        let is_reset = self.reset_flags[plane][layer_idx];

        let use_tpdon = self.options & OPT_TPDON != 0;
        let use_dpon = self.options & OPT_DPON != 0;

        let dppriv: *const u8 = if use_dpon {
            match &self.dppriv {
                Some(table) => table.as_ptr(),
                None => types::DEFAULT_DPPRIV.as_ptr(),
            }
        } else {
            std::ptr::null()
        };

        let mut pos = 0usize;

        let mut line_h1 = self.line_h1;
        let mut line_h2 = self.line_h2;
        let mut line_h3 = self.line_h3;
        let mut line_l1 = self.line_l1;
        let mut line_l2 = self.line_l2;
        let mut line_l3 = self.line_l3;

        let mut lntp = self.lntp[plane][layer_idx];

        for i in 0..(stripe_end_line - stripe_start_line) {
            let y = stripe_start_line + i;

            let mut tx = self.tx[plane][layer_idx];
            for atm in at_moves {
                if atm.line as usize == i {
                    self.tx[plane][layer_idx] = atm.tx as i32;
                    self.ty[plane][layer_idx] = atm.ty as i32;
                    tx = atm.tx as i32;
                }
            }

            let lo_line = (stripe * ll) + (i >> 1);
            let lo_line_next = lo_line + 1;

            let lp2_off = lo_line * lbpl;
            // C reference clamps lp1=lp2 when at the last lo-res row within the stripe
            // ((i>>1) >= ll-1) OR at the last lo-res row of the image ((lo_line_next >= ly)).
            // Missing the stripe-boundary clamp caused wrong context in the next stripe.
            let lp1_off = if (i >> 1) >= ll - 1 || lo_line_next >= ly as usize {
                lp2_off
            } else {
                lo_line_next * lbpl
            };

            if use_tpdon && self.pseudo {
                let raw = self.ar_decoders[plane][layer_idx].decode(TPDCX, data, &mut pos).unwrap_or(0);
                lntp = raw != 0;
                self.lntp[plane][layer_idx] = lntp;
                if std::env::var("JBIG1_DEBUG").is_ok() {
                    let target = Self::debug_target_rows(layer, self.d as usize);
                    if target.contains(&y) {
                        eprintln!("LNTP layer={} row={:4}  raw={}  lntp={}", layer, y, raw, lntp as u8);
                    }
                }
            }
            self.pseudo = false;

            if self.current_x == 0 {
                line_h1 = 0;
                line_h2 = 0;
                line_h3 = 0;
                line_l1 = 0;
                line_l2 = 0;
                line_l3 = 0;

                if i > 0 || (y > 0 && !is_reset) {
                    let prev_h = (y - 1) * hbpl;
                    if prev_h < self.lhp[lhp_hi][plane].len() {
                        line_h2 = (self.lhp[lhp_hi][plane][prev_h] as u32) << 8;
                        if i > 1 || (y > 1 && !is_reset) {
                            let prev2_h = (y.saturating_sub(2)) * hbpl;
                            if prev2_h < self.lhp[lhp_hi][plane].len() {
                                line_h3 = (self.lhp[lhp_hi][plane][prev2_h] as u32) << 8;
                            }
                        }
                    }
                }
                if i > 1 || (y > 1 && !is_reset) {
                    let prev_l = lp2_off.saturating_sub(lbpl);
                    if prev_l < self.lhp[lhp_lo][plane].len() {
                        line_l3 = (self.lhp[lhp_lo][plane][prev_l] as u32) << 8;
                    }
                }
                if lp2_off < self.lhp[lhp_lo][plane].len() {
                    line_l2 = (self.lhp[lhp_lo][plane][lp2_off] as u32) << 8;
                }
                if lp1_off < self.lhp[lhp_lo][plane].len() {
                    line_l1 = (self.lhp[lhp_lo][plane][lp1_off] as u32) << 8;
                }
            }

            let row_off = y * hbpl;
            let mut x = 0usize;
            let mut byte_idx = 0usize;
            let mut current_byte = 0u8;

            if std::env::var("JBIG1_DEBUG").is_ok() {
                let target = Self::debug_target_rows(layer, self.d as usize);
                if target.contains(&y) {
                    let lo_row = lo_line;
                    let n = lbpl.min(8);
                    let lo_off = lo_row * lbpl;
                    let bits: Vec<String> = self.lhp[lhp_lo][plane]
                        .get(lo_off..lo_off + n)
                        .unwrap_or(&[])
                        .iter()
                        .map(|b| format!("{:08b}", b))
                        .collect();
                    eprintln!("  DIFF_IN  layer={} y={:4}  lo_row={}  lo_bits: {}", layer, y, lo_row, bits.join(" "));
                }
            }

            let mut lp1_byte = 0usize;
            let mut lp2_byte = 0usize;

            while x < hx as usize {
                if (x & 15) == 0 {
                    let lbyte = x >> 4;
                    if lbyte + 1 < lbpl {
                        lp1_byte = lbyte;
                        lp2_byte = lbyte;
                        let lp1_next_off = lp1_off + lbyte + 1;
                        let lp2_next_off = lp2_off + lbyte + 1;
                        if lp2_next_off < self.lhp[lhp_lo][plane].len() {
                            line_l2 |= self.lhp[lhp_lo][plane][lp2_next_off] as u32;
                            if i > 1 || (y > 1 && !is_reset) {
                                let prev_l = lp2_next_off.saturating_sub(lbpl);
                                if prev_l < self.lhp[lhp_lo][plane].len() {
                                    line_l3 |= self.lhp[lhp_lo][plane][prev_l] as u32;
                                }
                            }
                        }
                        if lp1_next_off < self.lhp[lhp_lo][plane].len() {
                            line_l1 |= self.lhp[lhp_lo][plane][lp1_next_off] as u32;
                        }
                    }
                    let _ = (lp1_byte, lp2_byte);
                }

                'pixel_pair: loop {
                    if (x & 7) == 0 && x + 8 < hx as usize {
                        if i > 0 || (y > 0 && !is_reset) {
                            let next_byte_off = (y - 1) * hbpl + byte_idx + 1;
                            if next_byte_off < self.lhp[lhp_hi][plane].len() {
                                line_h2 |= self.lhp[lhp_hi][plane][next_byte_off] as u32;
                                if i > 1 || (y > 1 && !is_reset) {
                                    let prev2_off = (y.saturating_sub(2)) * hbpl + byte_idx + 1;
                                    if prev2_off < self.lhp[lhp_hi][plane].len() {
                                        line_h3 |= self.lhp[lhp_hi][plane][prev2_off] as u32;
                                    }
                                }
                            }
                        }
                    }

                    let tp_cx = if !lntp {
                        Some(
                            (((line_l3 >> 14) & 0x007) |
                             ((line_l2 >> 11) & 0x038) |
                             ((line_l1 >> 8)  & 0x1c0)) as usize
                        )
                    } else {
                        None
                    };

                    if let Some(ctx) = tp_cx {
                        if ctx == 0x000 || ctx == 0x1ff {
                            let pix = (ctx & 1) as u8;
                            line_h1 = (line_h1 << 1) | pix as u32;
                            x += 1;
                            current_byte = (current_byte << 1) | pix;
                            if x & 7 == 0 {
                                let dst = row_off + byte_idx;
                                if dst < self.lhp[lhp_hi][plane].len() {
                                    self.lhp[lhp_hi][plane][dst] = current_byte;
                                }
                                byte_idx += 1;
                                current_byte = 0;
                            }
                            if x & 1 == 0 {
                                line_h2 <<= 2;
                                line_h3 <<= 2;
                                line_l1 <<= 1;
                                line_l2 <<= 1;
                                line_l3 <<= 1;
                            }
                            if x >= hx as usize || (x & 1) == 0 {
                                break 'pixel_pair;
                            }

                            line_h1 = (line_h1 << 1) | pix as u32;
                            x += 1;
                            current_byte = (current_byte << 1) | pix;
                            if x & 7 == 0 {
                                let dst = row_off + byte_idx;
                                if dst < self.lhp[lhp_hi][plane].len() {
                                    self.lhp[lhp_hi][plane][dst] = current_byte;
                                }
                                byte_idx += 1;
                                current_byte = 0;
                            }
                            line_h2 <<= 2;
                            line_h3 <<= 2;
                            line_l1 <<= 1;
                            line_l2 <<= 1;
                            line_l3 <<= 1;
                            break 'pixel_pair;
                        }
                    }

                    let pix = self.decode_diff_pixel(
                        plane, layer_idx, data, &mut pos,
                        x, y, tx,
                        row_off, byte_idx, hbpl, lhp_hi,
                        line_h1, line_h2, line_h3, line_l1, line_l2, line_l3,
                        use_dpon, dppriv,
                    );

                    line_h1 = (line_h1 << 1) | pix as u32;
                    line_h2 <<= 1;
                    line_h3 <<= 1;
                    current_byte = (current_byte << 1) | pix;
                    x += 1;
                    if x & 7 == 0 {
                        let dst = row_off + byte_idx;
                        if dst < self.lhp[lhp_hi][plane].len() {
                            self.lhp[lhp_hi][plane][dst] = current_byte;
                        }
                        byte_idx += 1;
                        current_byte = 0;
                    }

                    if x >= hx as usize || (x & 1) == 0 {
                        if (x & 1) == 0 {
                            line_l1 <<= 1;
                            line_l2 <<= 1;
                            line_l3 <<= 1;
                        }
                        break 'pixel_pair;
                    }

                    let pix2 = self.decode_diff_pixel(
                        plane, layer_idx, data, &mut pos,
                        x, y, tx,
                        row_off, byte_idx, hbpl, lhp_hi,
                        line_h1, line_h2, line_h3, line_l1, line_l2, line_l3,
                        use_dpon, dppriv,
                    );

                    line_h1 = (line_h1 << 1) | pix2 as u32;
                    line_h2 <<= 1;
                    line_h3 <<= 1;
                    current_byte = (current_byte << 1) | pix2;
                    x += 1;
                    if x & 7 == 0 {
                        let dst = row_off + byte_idx;
                        if dst < self.lhp[lhp_hi][plane].len() {
                            self.lhp[lhp_hi][plane][dst] = current_byte;
                        }
                        byte_idx += 1;
                        current_byte = 0;
                    }

                    line_l1 <<= 1;
                    line_l2 <<= 1;
                    line_l3 <<= 1;
                    break 'pixel_pair;
                }
            }

            if hx as usize & 7 != 0 {
                let shift = 8 - (hx as usize & 7);
                current_byte <<= shift;
                let dst = row_off + byte_idx;
                if dst < self.lhp[lhp_hi][plane].len() {
                    self.lhp[lhp_hi][plane][dst] = current_byte;
                }
            }

            if std::env::var("JBIG1_DEBUG").is_ok() {
                let target = Self::debug_target_rows(layer, self.d as usize);
                if target.contains(&y) {
                    Self::dbg_row("DIFF_ROW", layer, y, lntp, &self.lhp[lhp_hi][plane], hbpl);
                }
            }

            if (i & 1) == 1 {
                self.pseudo = true;
            }
        }

        self.line_h1 = line_h1;
        self.line_h2 = line_h2;
        self.line_h3 = line_h3;
        self.line_l1 = line_l1;
        self.line_l2 = line_l2;
        self.line_l3 = line_l3;
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_diff_pixel(
        &mut self,
        plane: usize,
        layer_idx: usize,
        data: &[u8],
        pos: &mut usize,
        x: usize,
        y: usize,
        tx: i32,
        row_off: usize,
        byte_idx: usize,
        _hbpl: usize,
        lhp_hi: usize,
        line_h1: u32,
        line_h2: u32,
        line_h3: u32,
        line_l1: u32,
        line_l2: u32,
        line_l3: u32,
        use_dpon: bool,
        dppriv: *const u8,
    ) -> u8 {
        if use_dpon && !dppriv.is_null() {
            let pix = if (y & 1) == 0 {
                if (x & 1) == 0 {
                    let idx = ((line_l3 >> 15) & 0x003)
                        | ((line_l2 >> 13) & 0x00c)
                        | ((line_h1 << 4) & 0x010)
                        | ((line_h2 >> 9) & 0x0e0);
                    unsafe { *dppriv.add(idx as usize) }
                } else {
                    let idx = ((line_l3 >> 15) & 0x003)
                        | ((line_l2 >> 13) & 0x00c)
                        | ((line_h1 << 4) & 0x030)
                        | ((line_h2 >> 9) & 0x1c0);
                    unsafe { *dppriv.add(256 + idx as usize) }
                }
            } else if (x & 1) == 0 {
                let idx = ((line_l3 >> 15) & 0x003)
                    | ((line_l2 >> 13) & 0x00c)
                    | ((line_h1 << 4) & 0x010)
                    | ((line_h2 >> 9) & 0x0e0)
                    | ((line_h3 >> 6) & 0x700);
                unsafe { *dppriv.add(768 + idx as usize) }
            } else {
                let idx = ((line_l3 >> 15) & 0x003)
                    | ((line_l2 >> 13) & 0x00c)
                    | ((line_h1 << 4) & 0x030)
                    | ((line_h2 >> 9) & 0x1c0)
                    | ((line_h3 >> 6) & 0xe00);
                unsafe { *dppriv.add(2816 + idx as usize) }
            };

            if pix < 2 {
                return pix;
            }
        }

        let cx = if tx != 0 {
            let a = if tx as usize > x {
                0u32
            } else if tx < 8 {
                ((line_h1 << 2) >> (tx as u32).saturating_sub(3)) & 0x010
            } else {
                let o = (x as i32 - tx) & !7i32;
                if o >= 0 {
                    0
                } else {
                    let byte_off = ((-o) >> 3) as usize;
                    let bit_off = 7 - ((-o) & 7);
                    let b_idx = row_off + byte_idx.saturating_sub(byte_off);
                    if b_idx < self.lhp[lhp_hi][plane].len() {
                        ((self.lhp[lhp_hi][plane][b_idx] >> bit_off) as u32 & 1) << 4
                    } else {
                        0
                    }
                }
            };
            (line_h1 & 0x003) | a | ((line_h2 >> 12) & 0x00c) | ((line_h3 >> 10) & 0x020)
        } else {
            (line_h1 & 0x003) | ((line_h2 >> 12) & 0x01c) | ((line_h3 >> 10) & 0x020)
        };

        let cx = if (x & 1) != 0 {
            cx | (((line_l2 >> 8) & 0x0c0) | ((line_l1 >> 6) & 0x300)) | (1u32 << 10)
        } else {
            cx | ((line_l2 >> 9) & 0x0c0) | ((line_l1 >> 7) & 0x300)
        };
        let cx = cx | ((y as u32 & 1) << 11);

        let pix = self.ar_decoders[plane][layer_idx].decode(cx as usize, data, pos).unwrap_or(0);
        if std::env::var("JBIG1_DEBUG").is_ok() {
            let layer_abs = self.dl as usize + layer_idx;
            if layer_abs == self.d as usize && y >= 170 && y <= 185 && x < 50 {
                eprintln!("RUST_PIX  layer={} y={:4} x={:4}  cx=0x{:04x}  pix={}", layer_abs, y, x, cx, pix);
            }
        }
        pix
    }

    fn dbg_row(label: &str, layer: usize, y: usize, lntp: bool, buf: &[u8], hbpl: usize) {
        let off = y * hbpl;
        if off + hbpl > buf.len() {
            return;
        }
        // For layer 3: show bytes 150-166 (around col 1249 = byte 156)
        // For layer 2: show bytes 75-84 (lo-res byte 78 = hi-res col 1248)
        // For layer 1: show bytes 36-45 (lo-res byte ~39)
        let (start, end) = if layer == 3 {
            (150usize, hbpl.min(167))
        } else if layer == 2 {
            (75usize, hbpl.min(85))
        } else if layer == 1 {
            (36usize, hbpl.min(46))
        } else {
            (0usize, hbpl.min(20))
        };
        let bytes: Vec<String> = buf[off + start..off + end].iter().map(|b| format!("{:02x}", b)).collect();
        eprintln!("RUST_ROW  layer={} y={:4}  lntp={}  bytes[{}-{}]: {}", layer, y, lntp as u8, start, end - 1, bytes.join(" "));
    }

    fn debug_target_rows(layer: usize, d: usize) -> Vec<usize> {
        let shift = d.saturating_sub(layer);
        if layer == d {
            let mut rows: Vec<usize> = (155..=165).collect();
            rows.extend(170..=185usize);
            rows.extend(0..=26usize);
            rows.sort();
            rows.dedup();
            rows
        } else if layer + 1 == d {
            // layer 2: log rows corresponding to layer-3 rows 85-93
            let mut rows: Vec<usize> = (77..=82).collect();
            rows.extend(85..=93usize);
            let full_res: &[usize] = &[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26];
            rows.extend(full_res.iter().map(|&r| r >> shift));
            rows.sort();
            rows.dedup();
            rows
        } else if layer + 2 == d {
            // layer 1: log rows 38-48
            let mut rows: Vec<usize> = (38..=48).collect();
            let full_res: &[usize] = &[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26];
            rows.extend(full_res.iter().map(|&r| r >> shift));
            rows.sort();
            rows.dedup();
            rows
        } else {
            let full_res: &[usize] = &[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26];
            let mut rows: Vec<usize> = full_res.iter().map(|&r| r >> shift).collect();
            rows.dedup();
            rows
        }
    }

    fn decode_layer0_pscd(&mut self, plane: usize, data: &[u8], at_moves: &[AtMove]) {
        let layer = self.dl as usize;
        let layer_idx = 0usize;

        let hx = ceil_half(self.xd, (self.d as u32).saturating_sub(layer as u32));
        let hy = ceil_half(self.yd, (self.d as u32).saturating_sub(layer as u32));
        let hbpl = ((hx + 7) / 8) as usize;
        let hl = (self.l0 << layer) as usize;

        let stripe = self.current_stripe as usize;

        let lhp_idx = layer & 1;

        if self.lhp[lhp_idx][plane].is_empty() {
            let total = hbpl * hy as usize;
            self.lhp[lhp_idx][plane] = vec![0u8; total];
        }

        let stripe_start_line = stripe * hl;
        let stripe_end_line = ((stripe + 1) * hl).min(hy as usize);

        if stripe_start_line >= hy as usize {
            return;
        }

        let mut pos = 0usize;
        let use_two_line = self.options & OPT_LRLTWO != 0;
        let use_tpbon = self.options & OPT_TPBON != 0;

        let is_reset = self.reset_flags[plane][layer_idx];

        let mut line_h1: u32 = self.line_h1;
        let mut line_h2: u32 = self.line_h2;
        let mut line_h3: u32 = self.line_h3;

        let mut lntp = self.lntp[plane][layer_idx];

        for i in 0..(stripe_end_line - stripe_start_line) {
            let y = stripe_start_line + i;

            let mut tx = self.tx[plane][layer_idx];
            for atm in at_moves {
                if atm.line as usize == i {
                    self.tx[plane][layer_idx] = atm.tx as i32;
                    self.ty[plane][layer_idx] = atm.ty as i32;
                    tx = atm.tx as i32;
                }
            }

            if use_tpbon && self.pseudo {
                let tp_cx = if use_two_line { TPB2CX } else { TPB3CX };
                let slntp = self.ar_decoders[plane][layer_idx].decode(tp_cx, data, &mut pos);
                let slntp = slntp.unwrap_or(0);
                let old_lntp = lntp;
                lntp = (slntp ^ (lntp as u8)) == 0;
                self.lntp[plane][layer_idx] = lntp;

                if std::env::var("JBIG1_DEBUG").is_ok() && layer == 0 && stripe <= 3 {
                    eprintln!("L0  stripe={} row={:4}  old_lntp={}  slntp={}  lntp={}", stripe, y, old_lntp as u8, slntp, lntp as u8);
                }

                if !lntp {
                    let row_start = y * hbpl;
                    if i == 0 && (stripe == 0 || is_reset) {
                        for b in 0..hbpl {
                            if row_start + b < self.lhp[lhp_idx][plane].len() {
                                self.lhp[lhp_idx][plane][row_start + b] = 0;
                            }
                        }
                    } else if y > 0 {
                        let prev_row = (y - 1) * hbpl;
                        for b in 0..hbpl {
                            let src = if prev_row + b < self.lhp[lhp_idx][plane].len() {
                                self.lhp[lhp_idx][plane][prev_row + b]
                            } else {
                                0
                            };
                            if row_start + b < self.lhp[lhp_idx][plane].len() {
                                self.lhp[lhp_idx][plane][row_start + b] = src;
                            }
                        }
                    }
                    self.pseudo = true;
                    continue;
                }
            }

            self.pseudo = false;

            if self.current_x == 0 {
                line_h1 = 0;
                line_h2 = 0;
                line_h3 = 0;
                if i > 0 || (y > 0 && !is_reset) {
                    let prev = (y - 1) * hbpl;
                    if prev < self.lhp[lhp_idx][plane].len() {
                        line_h2 = (self.lhp[lhp_idx][plane][prev] as u32) << 8;
                    }
                }
                if i > 1 || (y > 1 && !is_reset) {
                    let prev2 = (y.saturating_sub(2)) * hbpl;
                    if prev2 < self.lhp[lhp_idx][plane].len() {
                        line_h3 = (self.lhp[lhp_idx][plane][prev2] as u32) << 8;
                    }
                }
            }

            let row_offset = y * hbpl;
            let mut x = 0usize;
            let mut byte_idx = 0usize;
            let mut current_byte = 0u8;

            while x < hx as usize {
                if x & 7 == 0 {
                    if x < (hbpl * 8).saturating_sub(8) {
                        if i > 0 || (y > 0 && !is_reset) {
                            let off = (y - 1) * hbpl + (x / 8) + 1;
                            if off < self.lhp[lhp_idx][plane].len() {
                                line_h2 |= self.lhp[lhp_idx][plane][off] as u32;
                                if i > 1 || (y > 1 && !is_reset) {
                                    let off3 = (y.saturating_sub(2)) * hbpl + (x / 8) + 1;
                                    if off3 < self.lhp[lhp_idx][plane].len() {
                                        line_h3 |= self.lhp[lhp_idx][plane][off3] as u32;
                                    }
                                }
                            }
                        }
                    }
                }

                let cx = if use_two_line {
                    if tx != 0 {
                        let a = if tx as usize > x {
                            0u32
                        } else if tx < 8 {
                            (line_h1 >> (tx as u32).wrapping_sub(5)) & 0x010
                        } else {
                            let o = (x as i32 - tx) & !7;
                            let byte_off = (-o / 8) as usize;
                            let bit_off = (7 - ((-o) & 7)) as u32;
                            if byte_off < hbpl {
                                let b_idx = row_offset + byte_idx.saturating_sub(byte_off);
                                if b_idx < self.lhp[lhp_idx][plane].len() {
                                    ((self.lhp[lhp_idx][plane][b_idx] >> bit_off) as u32 & 1) << 4
                                } else {
                                    0
                                }
                            } else {
                                0
                            }
                        };
                        ((line_h2 >> 9) & 0x3e0) | a | (line_h1 & 0x00f)
                    } else {
                        ((line_h2 >> 9) & 0x3f0) | (line_h1 & 0x00f)
                    }
                } else {
                    if tx != 0 {
                        let a = if tx as usize > x {
                            0u32
                        } else if tx < 8 {
                            (line_h1 >> (tx as u32).wrapping_sub(3)) & 0x004
                        } else {
                            let o = (x as i32 - tx) & !7;
                            let byte_off = (-o / 8) as usize;
                            let bit_off = (7 - ((-o) & 7)) as u32;
                            if byte_off < hbpl {
                                let b_idx = row_offset + byte_idx.saturating_sub(byte_off);
                                if b_idx < self.lhp[lhp_idx][plane].len() {
                                    ((self.lhp[lhp_idx][plane][b_idx] >> bit_off) as u32 & 1) << 2
                                } else {
                                    0
                                }
                            } else {
                                0
                            }
                        };
                        ((line_h3 >> 7) & 0x380) | ((line_h2 >> 11) & 0x078) | a | (line_h1 & 0x003)
                    } else {
                        ((line_h3 >> 7) & 0x380) | ((line_h2 >> 11) & 0x07c) | (line_h1 & 0x003)
                    }
                };

                let pix = self.ar_decoders[plane][layer_idx].decode(cx as usize, data, &mut pos).unwrap_or(0);

                line_h1 = (line_h1 << 1) | pix as u32;
                line_h2 <<= 1;
                line_h3 <<= 1;

                current_byte = (current_byte << 1) | pix;
                x += 1;

                if x & 7 == 0 {
                    let dst = row_offset + byte_idx;
                    if dst < self.lhp[lhp_idx][plane].len() {
                        self.lhp[lhp_idx][plane][dst] = current_byte;
                    }
                    byte_idx += 1;
                    current_byte = 0;
                }
            }

            if hx as usize & 7 != 0 {
                let shift = 8 - (hx as usize & 7);
                current_byte <<= shift;
                let dst = row_offset + byte_idx;
                if dst < self.lhp[lhp_idx][plane].len() {
                    self.lhp[lhp_idx][plane][dst] = current_byte;
                }
            }

            if std::env::var("JBIG1_DEBUG").is_ok() && y <= 26 {
                Self::dbg_row("RUST_ROW", layer, y, lntp, &self.lhp[lhp_idx][plane], hbpl);
            }

            self.pseudo = true;
        }
        self.line_h2 = line_h2;
        self.line_h3 = line_h3;
    }

    fn build_image(&self) -> VexelResult<Image> {
        let layer = self.d as usize;
        let lhp_idx = layer & 1;

        let hx = self.xd;
        let hy = self.yd;
        let hbpl = ((hx + 7) / 8) as usize;

        if self.planes == 1 {
            let plane_data = if plane_data_available(&self.lhp[lhp_idx], 0) {
                self.lhp[lhp_idx][0].clone()
            } else {
                vec![0u8; hbpl * hy as usize]
            };

            let mut pixels = vec![0u8; (hx * hy) as usize];
            for row in 0..hy as usize {
                for col in 0..hx as usize {
                    let byte_idx = row * hbpl + col / 8;
                    let bit_idx = 7 - (col % 8);
                    if byte_idx < plane_data.len() {
                        pixels[row * hx as usize + col] = 1 ^ ((plane_data[byte_idx] >> bit_idx) & 1);
                    }
                }
            }

            let mut pixel_data = PixelData::L1(pixels);
            pixel_data.correct_pixels(hx, hy);
            Ok(Image::from_pixels(hx, hy, pixel_data))
        } else {
            let num_planes = self.planes as usize;

            if num_planes > 8 {
                log_warn!("JBIG1: {} planes > 8, decoding plane 0 only", num_planes);
                return self.build_single_plane_image(0, layer, hx, hy, hbpl);
            }

            let mut gray_pixels = vec![0u8; (hx * hy) as usize];

            for p in 0..num_planes {
                if !plane_data_available(&self.lhp[lhp_idx], p) {
                    continue;
                }
                let plane_data = &self.lhp[lhp_idx][p];
                let shift = (num_planes - 1 - p) as u32;
                for row in 0..hy as usize {
                    for col in 0..hx as usize {
                        let byte_idx = row * hbpl + col / 8;
                        let bit_idx = 7 - (col % 8);
                        if byte_idx < plane_data.len() {
                            let bit = (plane_data[byte_idx] >> bit_idx) & 1;
                            gray_pixels[row * hx as usize + col] |= bit << shift;
                        }
                    }
                }
            }

            let mut pixel_data = PixelData::L8(gray_pixels);
            pixel_data.correct_pixels(hx, hy);
            Ok(Image::from_pixels(hx, hy, pixel_data))
        }
    }

    fn build_single_plane_image(&self, plane: usize, layer: usize, hx: u32, hy: u32, hbpl: usize) -> VexelResult<Image> {
        let lhp_idx = layer & 1;
        let plane_data = if plane_data_available(&self.lhp[lhp_idx], plane) {
            self.lhp[lhp_idx][plane].clone()
        } else {
            vec![0u8; hbpl * hy as usize]
        };

        let mut pixels = vec![0u8; (hx * hy) as usize];
        for row in 0..hy as usize {
            for col in 0..hx as usize {
                let byte_idx = row * hbpl + col / 8;
                let bit_idx = 7 - (col % 8);
                if byte_idx < plane_data.len() {
                    pixels[row * hx as usize + col] = 1 ^ ((plane_data[byte_idx] >> bit_idx) & 1);
                }
            }
        }

        let mut pixel_data = PixelData::L1(pixels);
        pixel_data.correct_pixels(hx, hy);
        Ok(Image::from_pixels(hx, hy, pixel_data))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_bih()?;

        if self.options & OPT_DPPRIV != 0 && self.options & OPT_DPLAST == 0 {
            let mut dptable = vec![0u8; 1728];
            match self.reader.read_exact(&mut dptable) {
                Ok(_) => {
                    let internal = dppriv_to_internal(&dptable);
                    self.dppriv = Some(internal);
                }
                Err(_) => {
                    log_warn!("JBIG1: failed to read DPTABLE, using default prediction");
                }
            }
        }

        self.allocate_layer_buffers();

        let data = self.read_all_data()?;
        self.process_stream(&data)?;

        self.build_image()
    }
}

fn plane_data_available(lhp: &[Vec<u8>], plane: usize) -> bool {
    plane < lhp.len() && !lhp[plane].is_empty()
}

fn dppriv_to_internal(dptable: &[u8]) -> Vec<u8> {
    let mut internal = vec![0u8; 6912];

    let trans0: [usize; 8] = [1, 0, 3, 2, 7, 6, 5, 4];
    let trans1: [usize; 9] = [1, 0, 3, 2, 8, 7, 6, 5, 4];
    let trans2: [usize; 11] = [1, 0, 3, 2, 10, 9, 8, 7, 6, 5, 4];
    let trans3: [usize; 12] = [1, 0, 3, 2, 11, 10, 9, 8, 7, 6, 5, 4];

    fill_internal(&mut internal, dptable, 0, 256, &trans0);
    fill_internal(&mut internal, dptable, 256, 512, &trans1);
    fill_internal(&mut internal, dptable, 768, 2048, &trans2);
    fill_internal(&mut internal, dptable, 2816, 4096, &trans3);

    internal
}

fn fill_internal(internal: &mut [u8], dptable: &[u8], offset: usize, len: usize, trans: &[usize]) {
    for i in 0..len {
        let mut k = 0usize;
        let mut tmp = i;
        let mut j = 0;
        while tmp > 0 {
            if j < trans.len() {
                k |= (tmp & 1) << trans[j];
            }
            tmp >>= 1;
            j += 1;
        }
        let src_idx = i + offset;
        if src_idx / 4 < dptable.len() && k + offset < internal.len() {
            internal[k + offset] = (dptable[src_idx / 4] >> ((3 - (src_idx & 3)) << 1)) & 3;
        }
    }
}
