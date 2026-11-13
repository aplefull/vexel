use crate::decoders::jpeg::color::JpegColorspace;
use crate::decoders::jpeg::idct::SimdLevel;
use crate::decoders::jpeg::sample::Sample;

#[derive(Clone, Copy, Debug)]
pub(super) struct ComponentLayout {
    pub(super) h: usize,
    pub(super) v: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

impl ComponentLayout {
    pub(super) fn new(h: usize, v: usize, hmax: usize, vmax: usize, width: usize, height: usize) -> Self {
        Self { h, v, width: (width * h).div_ceil(hmax), height: (height * v).div_ceil(vmax) }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlaneView<'a, S> {
    pub(super) data: &'a [S],
    pub(super) stride: usize,
    pub(super) first_row: usize,
    pub(super) height: usize,
}

impl<'a, S> PlaneView<'a, S> {
    #[inline(always)]
    fn row(&self, index: usize) -> &'a [S] {
        let index = index.min(self.height.saturating_sub(1)).max(self.first_row);
        let start = (index - self.first_row) * self.stride;
        self.data.get(start..start + self.stride).unwrap_or(&[])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Upsample {
    Full,
    H2V1,
    H1V2,
    H2V2,
    Nearest,
}

#[derive(Clone)]
pub(super) struct RowEmitter<S> {
    level: SimdLevel,
    color: JpegColorspace,
    precision: u8,
    width: usize,
    height: usize,
    hmax: usize,
    vmax: usize,
    layouts: Vec<ComponentLayout>,
    modes: Vec<Upsample>,
    scratch: Vec<Vec<S>>,
    next_row: usize,
}

impl<S: Sample> RowEmitter<S> {
    pub(super) fn new(
        level: SimdLevel,
        color: JpegColorspace,
        precision: u8,
        width: usize,
        height: usize,
        hmax: usize,
        vmax: usize,
        layouts: Vec<ComponentLayout>,
    ) -> Self {
        let modes = layouts
            .iter()
            .map(|c| {
                let rh = if hmax.is_multiple_of(c.h) { hmax / c.h } else { 0 };
                let rv = if vmax.is_multiple_of(c.v) { vmax / c.v } else { 0 };
                match (rh, rv) {
                    (1, 1) => Upsample::Full,
                    (2, 1) => Upsample::H2V1,
                    (1, 2) => Upsample::H1V2,
                    (2, 2) => Upsample::H2V2,
                    _ => Upsample::Nearest,
                }
            })
            .collect::<Vec<_>>();
        let scratch =
            modes.iter().map(|m| if *m == Upsample::Full { Vec::new() } else { vec![S::default(); width] }).collect();
        Self { level, color, precision, width, height, hmax, vmax, layouts, modes, scratch, next_row: 0 }
    }

    pub(super) fn channels(&self) -> usize {
        self.color.channels()
    }

    fn rows_needed(&self, comp: usize, y: usize) -> usize {
        let layout = &self.layouts[comp];
        let needed = match self.modes[comp] {
            Upsample::Full | Upsample::H2V1 => y + 1,
            Upsample::H1V2 | Upsample::H2V2 => {
                let center = y / 2;
                if !y.is_multiple_of(2) {
                    center + 2
                } else {
                    center + 1
                }
            }
            Upsample::Nearest => y * layout.v / self.vmax + 1,
        };
        needed.min(layout.height)
    }

    fn rows_first_needed(&self, comp: usize, y: usize) -> usize {
        let layout = &self.layouts[comp];
        match self.modes[comp] {
            Upsample::Full | Upsample::H2V1 => y,
            Upsample::H1V2 | Upsample::H2V2 => {
                let center = y / 2;
                if y.is_multiple_of(2) {
                    center.saturating_sub(1)
                } else {
                    center
                }
            }
            Upsample::Nearest => y * layout.v / self.vmax,
        }
    }

    fn emit_available(
        &mut self,
        views: &[PlaneView<'_, S>],
        available: &[usize],
        end_row: usize,
        out: &mut [S],
        out_first_row: usize,
    ) {
        let row_bytes = self.width * self.channels();
        let end_row = end_row.min(self.height);
        while self.next_row < end_row {
            let y = self.next_row;
            let ready = (0..self.layouts.len()).all(|c| self.rows_needed(c, y) <= available[c]);
            if !ready {
                break;
            }
            let start = (y - out_first_row) * row_bytes;
            self.emit_row(views, y, &mut out[start..start + row_bytes]);
            self.next_row += 1;
        }
    }

    pub(super) fn emit_rows(&mut self, views: &[PlaneView<'_, S>], first: usize, out: &mut [S]) {
        let row_bytes = self.width * self.channels();
        for (i, dst) in out.chunks_exact_mut(row_bytes).enumerate() {
            self.emit_row(views, first + i, dst);
        }
    }

    fn emit_row(&mut self, views: &[PlaneView<'_, S>], y: usize, dst: &mut [S]) {
        let width = self.width;
        let mut rows: [&[S]; 4] = [&[]; 4];
        let count = self.layouts.len().min(4);
        for c in 0..count {
            let layout = self.layouts[c];
            let view = &views[c];
            let sw = layout.width.min(view.stride);
            match self.modes[c] {
                Upsample::Full => {
                    let row = view.row(y);
                    rows[c] = &row[..width.min(row.len())];
                }
                Upsample::H2V1 => {
                    let row = view.row(y);
                    S::fancy_h2v1(self.level, &row[..sw.min(row.len())], &mut self.scratch[c]);
                }
                Upsample::H1V2 => {
                    let center = y / 2;
                    let (far, bias) = if y.is_multiple_of(2) { (center.saturating_sub(1), 1) } else { (center + 1, 2) };
                    let near = view.row(center);
                    let far = view.row(far);
                    S::fancy_h1v2(&near[..sw.min(near.len())], &far[..sw.min(far.len())], bias, &mut self.scratch[c]);
                }
                Upsample::H2V2 => {
                    let center = y / 2;
                    let far = if y.is_multiple_of(2) { center.saturating_sub(1) } else { center + 1 };
                    let near = view.row(center);
                    let far = view.row(far);
                    S::fancy_h2v2(
                        self.level,
                        &near[..sw.min(near.len())],
                        &far[..sw.min(far.len())],
                        &mut self.scratch[c],
                    );
                }
                Upsample::Nearest => {
                    let row = view.row(y * layout.v / self.vmax);
                    S::nearest(&row[..sw.min(row.len())], layout.h, self.hmax, &mut self.scratch[c]);
                }
            }
        }
        for ((row, mode), scratch) in rows.iter_mut().zip(&self.modes).zip(&self.scratch) {
            if *mode != Upsample::Full {
                *row = scratch;
            }
        }
        S::convert_row(self.level, self.color, &rows, dst, width, self.precision);
    }
}

pub(super) struct GroupEdges<S> {
    first: Vec<Vec<S>>,
    last: Vec<Vec<S>>,
    lead: std::ops::Range<usize>,
    trail: std::ops::Range<usize>,
}

impl<S> GroupEdges<S> {
    #[cfg(feature = "rayon")]
    pub(super) fn empty() -> Self {
        Self { first: Vec::new(), last: Vec::new(), lead: 0..0, trail: 0..0 }
    }
}

pub(super) struct BandWriter<S> {
    emitter: RowEmitter<S>,
    bands: Vec<Vec<S>>,
    strides: Vec<usize>,
    band_rows: Vec<usize>,
    heights: Vec<usize>,
    end_row: usize,
    out_first_row: usize,
    first_mcu_row: usize,
    lead_end: usize,
    first_edges: Vec<Vec<S>>,
}

impl<S: Sample> BandWriter<S> {
    pub(super) fn new(
        mut emitter: RowEmitter<S>,
        sampling: &[(usize, usize)],
        mcus_x: usize,
        first_mcu_row: usize,
        end_mcu_row: usize,
    ) -> Self {
        let rows_per_mcu = 8 * emitter.vmax;
        let first_row = (first_mcu_row * rows_per_mcu).min(emitter.height);
        let end_row = (end_mcu_row * rows_per_mcu).min(emitter.height);
        emitter.next_row = first_row;
        let strides: Vec<usize> = sampling.iter().map(|&(h, _)| mcus_x * h * 8).collect();
        let band_rows: Vec<usize> = sampling.iter().map(|&(_, v)| v * 8).collect();
        let bands =
            strides.iter().zip(&band_rows).map(|(&stride, &rows)| vec![S::default(); stride * (rows + 2)]).collect();
        let heights = emitter.layouts.iter().map(|l| l.height).collect();
        Self {
            emitter,
            bands,
            strides,
            band_rows,
            heights,
            end_row,
            out_first_row: first_row,
            first_mcu_row,
            lead_end: first_row,
            first_edges: Vec::new(),
        }
    }

    pub(super) fn new_detached(
        emitter: RowEmitter<S>,
        sampling: &[(usize, usize)],
        mcus_x: usize,
        first_mcu_row: usize,
        end_mcu_row: usize,
    ) -> Self {
        let mut writer = Self::new(emitter, sampling, mcus_x, first_mcu_row, end_mcu_row);
        if first_mcu_row > 0 {
            let starts: Vec<usize> = writer.band_rows.iter().map(|&rows| first_mcu_row * rows).collect();
            let emitter = &mut writer.emitter;
            while emitter.next_row < writer.end_row
                && (0..starts.len()).any(|c| emitter.rows_first_needed(c, emitter.next_row) < starts[c])
            {
                emitter.next_row += 1;
            }
            writer.lead_end = emitter.next_row;
        }
        writer
    }

    #[inline(always)]
    pub(super) fn band(&mut self, comp: usize) -> (&mut [S], usize) {
        let stride = self.strides[comp];
        (&mut self.bands[comp][2 * stride..], stride)
    }

    pub(super) fn shift(&mut self) {
        for ((band, &stride), &rows) in self.bands.iter_mut().zip(&self.strides).zip(&self.band_rows) {
            band.copy_within(rows * stride..(rows + 2) * stride, 0);
        }
    }

    pub(super) fn finish_row(&mut self, mcu_y: usize, out: &mut [S]) {
        if mcu_y == self.first_mcu_row && self.first_edges.is_empty() {
            self.first_edges = self
                .bands
                .iter()
                .zip(&self.strides)
                .map(|(band, &stride)| band[2 * stride..4 * stride].to_vec())
                .collect();
        }
        let empty = PlaneView { data: &[], stride: 0, first_row: 0, height: 0 };
        let mut views = [empty; 4];
        let mut available = [0usize; 4];
        for c in 0..self.bands.len().min(4) {
            let stride = self.strides[c];
            let rows = self.band_rows[c];
            let height = self.heights[c];
            views[c] = if mcu_y == 0 {
                PlaneView { data: &self.bands[c][2 * stride..], stride, first_row: 0, height }
            } else {
                PlaneView { data: &self.bands[c], stride, first_row: mcu_y * rows - 2, height }
            };
            available[c] = ((mcu_y + 1) * rows).min(height);
        }
        self.emitter.emit_available(&views, &available, self.end_row, out, self.out_first_row);
        self.shift();
    }

    pub(super) fn into_edges(self) -> GroupEdges<S> {
        let last = self.bands.iter().zip(&self.strides).map(|(band, &stride)| band[..2 * stride].to_vec()).collect();
        GroupEdges {
            first: self.first_edges,
            last,
            lead: self.out_first_row..self.lead_end,
            trail: self.emitter.next_row..self.end_row,
        }
    }
}

pub(super) fn emit_group_boundary<S: Sample>(
    emitter: &mut RowEmitter<S>,
    sampling: &[(usize, usize)],
    mcus_x: usize,
    boundary_mcu_row: usize,
    above: &GroupEdges<S>,
    below: &GroupEdges<S>,
    pixels: &mut [S],
) {
    let rows = above.trail.start..below.lead.end;
    if rows.is_empty() || above.last.len() != sampling.len() || below.first.len() != sampling.len() {
        return;
    }
    let windows: Vec<Vec<S>> = above.last.iter().zip(&below.first).map(|(a, b)| [a.as_slice(), b].concat()).collect();
    let views: Vec<PlaneView<'_, S>> = windows
        .iter()
        .enumerate()
        .map(|(c, data)| PlaneView {
            data,
            stride: mcus_x * sampling[c].0 * 8,
            first_row: (boundary_mcu_row * sampling[c].1 * 8).saturating_sub(2),
            height: emitter.layouts[c].height,
        })
        .collect();
    let row_bytes = emitter.width * emitter.channels();
    let start = rows.start * row_bytes;
    let end = (rows.end * row_bytes).min(pixels.len());
    emitter.emit_rows(&views, rows.start, &mut pixels[start..end]);
}
