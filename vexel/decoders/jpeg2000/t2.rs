use crate::decoders::jpeg2000::types::*;

pub struct HeaderReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    ct: u32,
    overrun: bool,
}

impl<'a> HeaderReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, buf: 0, ct: 0, overrun: false }
    }

    fn bytein(&mut self) {
        self.buf = (self.buf << 8) & 0xFFFF;
        self.ct = if self.buf == 0xFF00 { 7 } else { 8 };

        if self.pos < self.data.len() {
            self.buf |= self.data[self.pos] as u32;
            self.pos += 1;
        } else {
            self.overrun = true;
        }
    }

    pub fn exhausted(&self) -> bool {
        self.overrun
    }

    fn getbit(&mut self) -> u32 {
        if self.ct == 0 {
            self.bytein();
        }
        self.ct -= 1;
        (self.buf >> self.ct) & 1
    }

    pub fn read(&mut self, n: u32) -> u32 {
        let mut value = 0;
        for i in (0..n).rev() {
            value |= self.getbit() << i;
        }
        value
    }

    pub fn inalign(&mut self) {
        if self.buf & 0xFF == 0xFF {
            self.bytein();
        }
        self.ct = 0;
    }

    pub fn num_bytes(&self) -> usize {
        self.pos
    }
}

pub struct TagTree {
    values: Vec<u32>,
    lows: Vec<u32>,
    parents: Vec<Option<usize>>,
}

impl TagTree {
    pub fn new(num_leafs_h: u32, num_leafs_v: u32) -> Self {
        let mut level_w = vec![num_leafs_h.max(1) as i64];
        let mut level_h = vec![num_leafs_v.max(1) as i64];
        let mut num_nodes = 0i64;

        loop {
            let level = level_w.len() - 1;
            let n = level_w[level] * level_h[level];
            level_w.push((level_w[level] + 1) / 2);
            level_h.push((level_h[level] + 1) / 2);
            num_nodes += n;
            if n <= 1 {
                break;
            }
        }

        let num_nodes = num_nodes.max(1) as usize;
        let mut parents = vec![None; num_nodes];

        let mut level_start = 0usize;
        let num_levels = level_w.len() - 1;

        for level in 0..num_levels.saturating_sub(1) {
            let w = level_w[level] as usize;
            let h = level_h[level] as usize;
            let parent_start = level_start + w * h;
            let parent_w = level_w[level + 1] as usize;

            for y in 0..h {
                for x in 0..w {
                    let node = level_start + y * w + x;
                    let parent = parent_start + (y / 2) * parent_w + (x / 2);
                    if parent < num_nodes {
                        parents[node] = Some(parent);
                    }
                }
            }

            level_start = parent_start;
        }

        Self { values: vec![u32::MAX; num_nodes], lows: vec![0; num_nodes], parents }
    }

    pub fn reset(&mut self) {
        self.values.iter_mut().for_each(|v| *v = u32::MAX);
        self.lows.iter_mut().for_each(|v| *v = 0);
    }

    pub fn decode(&mut self, reader: &mut HeaderReader<'_>, leafno: usize, threshold: u32) -> bool {
        if leafno >= self.values.len() {
            return false;
        }

        let mut stack = Vec::new();
        let mut node = leafno;

        while let Some(parent) = self.parents[node] {
            stack.push(node);
            node = parent;
        }

        let mut low = 0u32;

        loop {
            if low > self.lows[node] {
                self.lows[node] = low;
            } else {
                low = self.lows[node];
            }

            while low < threshold && low < self.values[node] {
                if reader.exhausted() {
                    break;
                }
                if reader.read(1) != 0 {
                    self.values[node] = low;
                } else {
                    low += 1;
                }
            }

            self.lows[node] = low;

            match stack.pop() {
                Some(next) => node = next,
                None => break,
            }
        }

        self.values[node] < threshold
    }
}

pub fn get_num_passes(reader: &mut HeaderReader<'_>) -> u32 {
    if reader.read(1) == 0 {
        return 1;
    }
    if reader.read(1) == 0 {
        return 2;
    }

    let n = reader.read(2);
    if n != 3 {
        return 3 + n;
    }

    let n = reader.read(5);
    if n != 31 {
        return 6 + n;
    }

    37 + reader.read(7)
}

pub fn get_comma_code(reader: &mut HeaderReader<'_>) -> u32 {
    let mut n = 0;
    while reader.read(1) != 0 {
        n += 1;
        if n > 32 {
            break;
        }
    }
    n
}

pub fn segment_max_passes(cblksty: CodeBlockStyle, first: bool, previous_max: u32) -> u32 {
    if cblksty.high_throughput() {
        if first {
            1
        } else {
            109
        }
    } else if cblksty.terminate_all() {
        1
    } else if cblksty.lazy() {
        if first {
            10
        } else if previous_max == 1 || previous_max == 10 {
            2
        } else {
            1
        }
    } else {
        109
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketPosition {
    pub layer: u32,
    pub resolution: u32,
    pub component: u32,
    pub precinct: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolutionGeometry {
    pub pdx: u32,
    pub pdy: u32,
    pub pw: u32,
    pub ph: u32,
}

#[derive(Debug, Clone)]
pub struct ComponentGeometry {
    pub dx: u32,
    pub dy: u32,
    pub resolutions: Vec<ResolutionGeometry>,
}

#[derive(Debug, Clone, Copy)]
pub struct TileGeometry {
    pub tx0: u32,
    pub ty0: u32,
    pub tx1: u32,
    pub ty1: u32,
}

fn ceil_div_u64(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    numerator.div_ceil(denominator) as u32
}

fn floor_div_pow2(value: u32, exp: u32) -> u32 {
    if exp >= 32 {
        0
    } else {
        value >> exp
    }
}

fn precinct_at(tile: &TileGeometry, comp: &ComponentGeometry, resno: u32, x: u32, y: u32) -> Option<u32> {
    let res = comp.resolutions.get(resno as usize)?;
    let num_resolutions = comp.resolutions.len() as u32;
    let levelno = num_resolutions - 1 - resno;

    if levelno >= 32 {
        return None;
    }

    let dx_shifted = (comp.dx as u64) << levelno;
    let dy_shifted = (comp.dy as u64) << levelno;

    let trx0 = ceil_div_u64(tile.tx0 as u64, dx_shifted);
    let try0 = ceil_div_u64(tile.ty0 as u64, dy_shifted);
    let trx1 = ceil_div_u64(tile.tx1 as u64, dx_shifted);
    let try1 = ceil_div_u64(tile.ty1 as u64, dy_shifted);

    let rpx = res.pdx + levelno;
    let rpy = res.pdy + levelno;

    if rpx >= 32 || rpy >= 32 {
        return None;
    }

    let x_step = (comp.dx as u64) << rpx;
    let y_step = (comp.dy as u64) << rpy;

    let on_y = (y as u64).is_multiple_of(y_step)
        || (y == tile.ty0 && !(((try0 as u64) << levelno).is_multiple_of(1u64 << rpy)));
    let on_x = (x as u64).is_multiple_of(x_step)
        || (x == tile.tx0 && !(((trx0 as u64) << levelno).is_multiple_of(1u64 << rpx)));

    if !on_y || !on_x {
        return None;
    }

    if res.pw == 0 || res.ph == 0 || trx0 == trx1 || try0 == try1 {
        return None;
    }

    let prci = floor_div_pow2(ceil_div_u64(x as u64, dx_shifted), res.pdx) - floor_div_pow2(trx0, res.pdx);
    let prcj = floor_div_pow2(ceil_div_u64(y as u64, dy_shifted), res.pdy) - floor_div_pow2(try0, res.pdy);

    prcj.checked_mul(res.pw)?.checked_add(prci)
}

fn position_step(components: &[ComponentGeometry], only: Option<usize>) -> (u32, u32) {
    let mut step_x = 0u32;
    let mut step_y = 0u32;

    for (index, comp) in components.iter().enumerate() {
        if let Some(wanted) = only {
            if index != wanted {
                continue;
            }
        }

        let num_resolutions = comp.resolutions.len() as u32;

        for (resno, res) in comp.resolutions.iter().enumerate() {
            let levelno = num_resolutions - 1 - resno as u32;

            let shift_x = res.pdx + levelno;
            if shift_x < 32 {
                if let Some(dx) = comp.dx.checked_mul(1u32 << shift_x) {
                    step_x = if step_x == 0 { dx } else { step_x.min(dx) };
                }
            }

            let shift_y = res.pdy + levelno;
            if shift_y < 32 {
                if let Some(dy) = comp.dy.checked_mul(1u32 << shift_y) {
                    step_y = if step_y == 0 { dy } else { step_y.min(dy) };
                }
            }
        }
    }

    (step_x, step_y)
}

fn positions_along(start: u32, end: u32, step: u32) -> impl Iterator<Item = u32> {
    let first = if step == 0 || start >= end { None } else { Some(start) };

    std::iter::successors(first, move |&value| value.checked_add(step - (value % step)).filter(|&next| next < end))
}

#[derive(Debug, Clone, Copy)]
pub struct EnumerationLimit {
    pub max_positions: usize,
    pub max_work: u64,
}

pub struct Enumeration {
    pub positions: Vec<PacketPosition>,
    pub truncated: bool,
}

struct Collector {
    positions: Vec<PacketPosition>,
    seen: Option<std::collections::HashSet<(u32, u32, u32, u32)>>,
    max_positions: usize,
    work_left: u64,
    truncated: bool,
}

impl Collector {
    fn new(limit: EnumerationLimit, deduplicate: bool) -> Self {
        Self {
            positions: Vec::new(),
            seen: deduplicate.then(std::collections::HashSet::new),
            max_positions: limit.max_positions,
            work_left: limit.max_work,
            truncated: false,
        }
    }

    fn tick(&mut self) -> bool {
        if self.work_left == 0 {
            self.truncated = true;
            return false;
        }
        self.work_left -= 1;
        true
    }

    fn push(&mut self, layer: u32, resolution: u32, component: u32, precinct: u32) -> bool {
        if !self.tick() {
            return false;
        }

        if let Some(seen) = self.seen.as_mut() {
            if !seen.insert((layer, resolution, component, precinct)) {
                return true;
            }
        }

        if self.positions.len() >= self.max_positions {
            self.truncated = true;
            return false;
        }

        self.positions.push(PacketPosition { layer, resolution, component, precinct });
        true
    }

    fn finish(self) -> Enumeration {
        Enumeration { positions: self.positions, truncated: self.truncated }
    }
}

pub fn enumerate_packets(
    order: ProgressionOrder,
    num_layers: u32,
    tile: &TileGeometry,
    components: &[ComponentGeometry],
    limit: EnumerationLimit,
) -> Enumeration {
    let max_resolutions = components.iter().map(|c| c.resolutions.len() as u32).max().unwrap_or(0);

    let mut collector = Collector::new(limit, false);
    enumerate_range(
        order,
        tile,
        components,
        0,
        components.len() as u32,
        0,
        max_resolutions,
        num_layers,
        &mut collector,
    );
    collector.finish()
}

pub fn enumerate_packets_with_changes(
    changes: &[ProgressionChange],
    num_layers: u32,
    tile: &TileGeometry,
    components: &[ComponentGeometry],
    limit: EnumerationLimit,
) -> Enumeration {
    let mut collector = Collector::new(limit, true);

    for change in changes {
        if !collector.tick() {
            break;
        }

        let completed = enumerate_range(
            change.order,
            tile,
            components,
            change.compno0,
            change.compno1,
            change.resno0,
            change.resno1,
            change.layno1.min(num_layers),
            &mut collector,
        );

        if !completed {
            break;
        }
    }

    collector.finish()
}

#[allow(clippy::too_many_arguments)]
fn enumerate_range(
    order: ProgressionOrder,
    tile: &TileGeometry,
    components: &[ComponentGeometry],
    compno0: u32,
    compno1: u32,
    resno0: u32,
    resno1: u32,
    num_layers: u32,
    collector: &mut Collector,
) -> bool {
    let num_components = components.len() as u32;
    let compno1 = compno1.min(num_components);
    let max_resolutions = components.iter().map(|c| c.resolutions.len() as u32).max().unwrap_or(0).min(resno1);

    let precinct_count = |component: usize, resolution: usize| -> u32 {
        components
            .get(component)
            .and_then(|c| c.resolutions.get(resolution))
            .map(|r| r.pw.saturating_mul(r.ph))
            .unwrap_or(0)
    };

    let resolution_count =
        |component: usize| -> u32 { components.get(component).map(|c| c.resolutions.len() as u32).unwrap_or(0) };

    match order {
        ProgressionOrder::Lrcp => {
            for layer in 0..num_layers {
                if !collector.tick() {
                    return false;
                }
                for resolution in resno0..max_resolutions {
                    for component in compno0..compno1 {
                        if !collector.tick() {
                            return false;
                        }
                        if resolution >= resolution_count(component as usize) {
                            continue;
                        }
                        for precinct in 0..precinct_count(component as usize, resolution as usize) {
                            if !collector.push(layer, resolution, component, precinct) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        ProgressionOrder::Rlcp => {
            for resolution in resno0..max_resolutions {
                for layer in 0..num_layers {
                    if !collector.tick() {
                        return false;
                    }
                    for component in compno0..compno1 {
                        if !collector.tick() {
                            return false;
                        }
                        if resolution >= resolution_count(component as usize) {
                            continue;
                        }
                        for precinct in 0..precinct_count(component as usize, resolution as usize) {
                            if !collector.push(layer, resolution, component, precinct) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        ProgressionOrder::Rpcl => {
            let (step_x, step_y) = position_step(components, None);

            for resolution in resno0..max_resolutions {
                for y in positions_along(tile.ty0, tile.ty1, step_y) {
                    if !collector.tick() {
                        return false;
                    }
                    for x in positions_along(tile.tx0, tile.tx1, step_x) {
                        for (index, comp) in components.iter().enumerate() {
                            if !collector.tick() {
                                return false;
                            }
                            let index = index as u32;
                            if index < compno0 || index >= compno1 {
                                continue;
                            }
                            if resolution >= comp.resolutions.len() as u32 {
                                continue;
                            }
                            if let Some(precinct) = precinct_at(tile, comp, resolution, x, y) {
                                for layer in 0..num_layers {
                                    if !collector.push(layer, resolution, index, precinct) {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        ProgressionOrder::Pcrl => {
            let (step_x, step_y) = position_step(components, None);

            for y in positions_along(tile.ty0, tile.ty1, step_y) {
                if !collector.tick() {
                    return false;
                }
                for x in positions_along(tile.tx0, tile.tx1, step_x) {
                    for (index, comp) in components.iter().enumerate() {
                        if !collector.tick() {
                            return false;
                        }
                        let index = index as u32;
                        if index < compno0 || index >= compno1 {
                            continue;
                        }
                        for resolution in resno0..(comp.resolutions.len() as u32).min(resno1) {
                            if !collector.tick() {
                                return false;
                            }
                            if let Some(precinct) = precinct_at(tile, comp, resolution, x, y) {
                                for layer in 0..num_layers {
                                    if !collector.push(layer, resolution, index, precinct) {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        ProgressionOrder::Cprl => {
            for (index, comp) in components.iter().enumerate() {
                if !collector.tick() {
                    return false;
                }
                if (index as u32) < compno0 || (index as u32) >= compno1 {
                    continue;
                }

                let (step_x, step_y) = position_step(components, Some(index));

                for y in positions_along(tile.ty0, tile.ty1, step_y) {
                    if !collector.tick() {
                        return false;
                    }
                    for x in positions_along(tile.tx0, tile.tx1, step_x) {
                        for resolution in resno0..(comp.resolutions.len() as u32).min(resno1) {
                            if !collector.tick() {
                                return false;
                            }
                            if let Some(precinct) = precinct_at(tile, comp, resolution, x, y) {
                                for layer in 0..num_layers {
                                    if !collector.push(layer, resolution, index as u32, precinct) {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    true
}
