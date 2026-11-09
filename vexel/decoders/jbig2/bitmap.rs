#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposeOp {
    Or,
    And,
    Xor,
    Xnor,
    Replace,
}

impl ComposeOp {
    pub fn from_bits(bits: u8) -> ComposeOp {
        match bits & 7 {
            0 => ComposeOp::Or,
            1 => ComposeOp::And,
            2 => ComposeOp::Xor,
            3 => ComposeOp::Xnor,
            _ => ComposeOp::Replace,
        }
    }

    fn apply(self, dst: bool, src: bool) -> bool {
        match self {
            ComposeOp::Or => dst | src,
            ComposeOp::And => dst & src,
            ComposeOp::Xor => dst ^ src,
            ComposeOp::Xnor => !(dst ^ src),
            ComposeOp::Replace => src,
        }
    }

    fn apply_byte(self, dst: u8, src: u8) -> u8 {
        match self {
            ComposeOp::Or => dst | src,
            ComposeOp::And => dst & src,
            ComposeOp::Xor => dst ^ src,
            ComposeOp::Xnor => !(dst ^ src),
            ComposeOp::Replace => src,
        }
    }
}

pub struct WorkBudget {
    remaining: u64,
}

impl WorkBudget {
    pub fn new(units: u64) -> WorkBudget {
        WorkBudget { remaining: units }
    }

    pub fn grant(&mut self, units: u64) {
        self.remaining = self.remaining.saturating_add(units);
    }

    pub fn charge(&mut self, units: u64) -> bool {
        if units > self.remaining {
            return false;
        }
        self.remaining -= units;
        true
    }

    pub fn charge_area(&mut self, width: u32, height: u32) -> bool {
        self.charge(u64::from(width).saturating_mul(u64::from(height)).max(1))
    }
}

#[derive(Clone)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub data: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: u32, height: u32) -> Bitmap {
        let stride = (width as usize).div_ceil(8);
        let len = stride.saturating_mul(height as usize);
        Bitmap { width, height, stride, data: vec![0u8; len] }
    }

    pub fn fill(&mut self, value: bool) {
        self.data.fill(if value { 0xff } else { 0x00 });
    }

    #[inline]
    pub fn get(&self, x: i64, y: i64) -> u32 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0;
        }
        let byte = (x as usize >> 3) + y as usize * self.stride;
        match self.data.get(byte) {
            Some(b) => ((b >> (7 - (x as usize & 7))) & 1) as u32,
            None => 0,
        }
    }

    #[inline]
    pub fn set(&mut self, x: i64, y: i64, value: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let byte = (x as usize >> 3) + y as usize * self.stride;
        let bit = 7 - (x as usize & 7);
        if let Some(b) = self.data.get_mut(byte) {
            *b = (*b & !(1 << bit)) | ((value as u8 & 1) << bit);
        }
    }

    pub fn resize_height(&mut self, height: u32, fill: bool) {
        let len = self.stride.saturating_mul(height as usize);
        self.data.resize(len, if fill { 0xff } else { 0x00 });
        self.height = height;
    }

    pub fn compose(&mut self, src: &Bitmap, x: i64, y: i64, op: ComposeOp) {
        if src.width == 0 || src.height == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        if x <= -(src.width as i64) || x >= self.width as i64 || y <= -(src.height as i64) || y >= self.height as i64 {
            return;
        }

        let src_x0 = if x < 0 { -x } else { 0 };
        let src_y0 = if y < 0 { -y } else { 0 };
        let dst_x0 = x.max(0);
        let dst_y0 = y.max(0);

        let w = (src.width as i64 - src_x0).min(self.width as i64 - dst_x0);
        let h = (src.height as i64 - src_y0).min(self.height as i64 - dst_y0);
        if w <= 0 || h <= 0 {
            return;
        }

        if (dst_x0 & 7) == (src_x0 & 7) {
            self.compose_aligned(src, src_x0, src_y0, dst_x0, dst_y0, w, h, op);
        } else {
            for row in 0..h {
                for col in 0..w {
                    let s = src.get(src_x0 + col, src_y0 + row) != 0;
                    let d = self.get(dst_x0 + col, dst_y0 + row) != 0;
                    self.set(dst_x0 + col, dst_y0 + row, op.apply(d, s) as u32);
                }
            }
        }
    }

    fn compose_aligned(
        &mut self,
        src: &Bitmap,
        src_x0: i64,
        src_y0: i64,
        dst_x0: i64,
        dst_y0: i64,
        w: i64,
        h: i64,
        op: ComposeOp,
    ) {
        let first_byte = (dst_x0 >> 3) as usize;
        let last_bit = dst_x0 + w - 1;
        let last_byte = (last_bit >> 3) as usize;
        let src_first_byte = (src_x0 >> 3) as usize;

        let left_mask = 0xffu8 >> (dst_x0 & 7);
        let right_mask = {
            let trailing = 7 - (last_bit & 7);
            0xffu8 << trailing
        };

        for row in 0..h {
            let dst_row = (dst_y0 + row) as usize * self.stride;
            let src_row = (src_y0 + row) as usize * src.stride;

            for byte in first_byte..=last_byte {
                let src_index = src_row + src_first_byte + (byte - first_byte);
                let s = match src.data.get(src_index) {
                    Some(v) => *v,
                    None => 0,
                };
                let dst_index = dst_row + byte;
                let Some(d) = self.data.get_mut(dst_index) else {
                    continue;
                };

                let mut mask = 0xffu8;
                if byte == first_byte {
                    mask &= left_mask;
                }
                if byte == last_byte {
                    mask &= right_mask;
                }

                let combined = op.apply_byte(*d, s);
                *d = (combined & mask) | (*d & !mask);
            }
        }
    }
}
