use super::bitreader::JpegBitReader;
use super::huffman::LosslessHuffman;
use crate::log_warn;
use crate::utils::raster::RasterWindow;

#[cfg(feature = "rayon")]
const PARALLEL_MIN_SAMPLES: usize = 1 << 20;
#[cfg(feature = "rayon")]
const GROUPS_PER_THREAD: usize = 4;
#[cfg(feature = "rayon")]
const SPECULATIVE_MIN_CHUNK_BYTES: usize = 256 * 1024;
#[cfg(feature = "rayon")]
const SPECULATIVE_MARKS: usize = 1024;

pub(super) struct LosslessScan<'a> {
    pub(super) entropy: &'a [u8],
    #[cfg_attr(not(feature = "rayon"), allow(dead_code))]
    pub(super) restart_offsets: &'a [usize],
    pub(super) tables: Vec<&'a LosslessHuffman>,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) predictor: u8,
    pub(super) point_transform: u8,
    pub(super) initial: u16,
    pub(super) restart_interval: usize,
    #[cfg_attr(not(feature = "rayon"), allow(dead_code))]
    pub(super) uniform_tables: bool,
}

struct StreamState {
    next_restart: usize,
    started: bool,
}

impl StreamState {
    fn at(pixel: usize) -> Self {
        Self { next_restart: pixel, started: false }
    }
}

#[inline(always)]
fn predict(predictor: u8, ra: u16, rb: u16, rc: u16) -> u16 {
    let (a, b, c) = (ra as i32, rb as i32, rc as i32);
    (match predictor {
        0 => 0,
        1 => a,
        2 => b,
        3 => c,
        4 => a + b - c,
        5 => a + ((b - c) >> 1),
        6 => b + ((a - c) >> 1),
        _ => (a + b) >> 1,
    }) as u16
}

struct Kernel<'s, const NF: usize> {
    tables: [&'s LosslessHuffman; NF],
    width: usize,
    predictor: u8,
    initial: u16,
    restart_interval: usize,
    point_transform: u8,
}

impl<'s, const NF: usize> Kernel<'s, NF> {
    #[inline(always)]
    fn first_run(&self, reader: &mut JpegBitReader<'_>, cur: &mut [u16], x0: usize, x1: usize) {
        let mut last = [self.initial; NF];
        for px in cur[x0 * NF..x1 * NF].chunks_exact_mut(NF) {
            for c in 0..NF {
                let v = last[c].wrapping_add(self.tables[c].diff(reader));
                px[c] = v;
                last[c] = v;
            }
        }
    }

    #[inline(always)]
    fn normal_run_ra(&self, reader: &mut JpegBitReader<'_>, prev: &[u16], cur: &mut [u16], x1: usize) {
        if x1 == 0 {
            return;
        }
        let mut last = [0u16; NF];
        for c in 0..NF {
            let v = prev[c].wrapping_add(self.tables[c].diff(reader));
            cur[c] = v;
            last[c] = v;
        }
        for px in cur[NF..x1 * NF].chunks_exact_mut(NF) {
            for c in 0..NF {
                let v = last[c].wrapping_add(self.tables[c].diff(reader));
                px[c] = v;
                last[c] = v;
            }
        }
    }

    fn normal_run_general(&self, reader: &mut JpegBitReader<'_>, prev: &[u16], cur: &mut [u16], x1: usize) {
        if x1 == 0 {
            return;
        }
        for c in 0..NF {
            cur[c] = prev[c].wrapping_add(self.tables[c].diff(reader));
        }
        for x in 1..x1 {
            for c in 0..NF {
                let i = x * NF + c;
                let p = predict(self.predictor, cur[i - NF], prev[i], prev[i - NF]);
                cur[i] = p.wrapping_add(self.tables[c].diff(reader));
            }
        }
    }

    #[inline(always)]
    fn decode_row(
        &self,
        reader: &mut JpegBitReader<'_>,
        y: usize,
        prev: &[u16],
        cur: &mut [u16],
        state: &mut StreamState,
    ) {
        let w = self.width;
        let row_start = y * w;
        let row_end = row_start + w;
        let mut x = 0;
        let split = state.next_restart.clamp(row_start, row_end) - row_start;
        if split > 0 {
            if self.predictor == 1 {
                self.normal_run_ra(reader, prev, cur, split);
            } else {
                self.normal_run_general(reader, prev, cur, split);
            }
            x = split;
        }
        while x < w {
            if state.started {
                reader.clear_buffer();
            }
            state.started = true;
            let end = if self.restart_interval > 0 {
                state.next_restart = row_start + x + self.restart_interval;
                (x + self.restart_interval).min(w)
            } else {
                state.next_restart = usize::MAX;
                w
            };
            self.first_run(reader, cur, x, end);
            x = end;
        }
    }

    fn emit(&self, row: &[u16], out: &mut [u16]) {
        let n = out.len().min(row.len());
        if self.point_transform == 0 {
            out[..n].copy_from_slice(&row[..n]);
        } else {
            let pt = self.point_transform;
            for (o, &s) in out[..n].iter_mut().zip(row) {
                *o = s << pt;
            }
        }
    }

    #[cfg(feature = "rayon")]
    fn reconstruct_in_place(&self, mut window: RasterWindow<'_, u16>) {
        use rayon::prelude::*;

        let row_len = self.width * NF;
        let pt = self.point_transform;
        if self.predictor != 1 {
            let mut prev = vec![0u16; row_len];
            for y in 0..window.height() {
                let row = &mut window.row_mut(y)[..row_len];
                if y == 0 {
                    let mut last = [self.initial; NF];
                    for px in row.chunks_exact_mut(NF) {
                        for c in 0..NF {
                            px[c] = last[c].wrapping_add(px[c]);
                            last[c] = px[c];
                        }
                    }
                } else {
                    for c in 0..NF {
                        row[c] = prev[c].wrapping_add(row[c]);
                    }
                    for i in NF..row_len {
                        let p = predict(self.predictor, row[i - NF], prev[i], prev[i - NF]);
                        row[i] = p.wrapping_add(row[i]);
                    }
                }
                prev.copy_from_slice(row);
                if pt > 0 {
                    row.iter_mut().for_each(|s| *s <<= pt);
                }
            }
            return;
        }

        let mut above = [self.initial; NF];
        for y in 0..window.height() {
            let row = window.row_mut(y);
            for c in 0..NF {
                row[c] = above[c].wrapping_add(row[c]);
                above[c] = row[c];
            }
        }
        let rows_per_band = window.height().div_ceil(rayon::current_num_threads() * GROUPS_PER_THREAD).max(1);
        window.into_row_bands(rows_per_band).into_par_iter().for_each(|mut band| {
            for y in 0..band.height() {
                let row = &mut band.row_mut(y)[..row_len];
                for i in NF..row_len {
                    row[i] = row[i - NF].wrapping_add(row[i]);
                }
                if pt > 0 {
                    row.iter_mut().for_each(|s| *s <<= pt);
                }
            }
        });
    }

    #[cfg(feature = "rayon")]
    fn decode_intervals_paired(
        &self,
        entropy: &[u8],
        offsets: &[usize],
        first_interval: usize,
        unit_rows: usize,
        band: RasterWindow<'_, u16>,
    ) {
        let samples = self.width * NF;
        let reader_at = |interval: usize| {
            let offset = offsets.get(interval).copied().unwrap_or(entropy.len()).min(entropy.len());
            JpegBitReader::new(&entropy[offset..])
        };
        let mut bufs: [Vec<u16>; 4] = std::array::from_fn(|_| vec![0u16; samples]);
        let intervals = band.height().div_ceil(unit_rows);
        let mut rest = band;
        let mut interval = 0;
        while interval + 1 < intervals && rest.height() >= 2 * unit_rows {
            let (pair, tail) = rest.split_rows(2 * unit_rows);
            rest = tail;
            let (mut a_out, mut b_out) = pair.split_rows(unit_rows);
            let mut ra = reader_at(first_interval + interval);
            let mut rb = reader_at(first_interval + interval + 1);
            let [prev_a, cur_a, prev_b, cur_b] = &mut bufs;
            for j in 0..unit_rows {
                if ra.is_exhausted() || rb.is_exhausted() {
                    log_warn!("Lossless scan data exhausted in restart interval {}", first_interval + interval);
                    break;
                }
                if j == 0 {
                    self.first_row_pair(&mut ra, &mut rb, cur_a, cur_b);
                } else {
                    self.normal_row_pair(&mut ra, &mut rb, prev_a, prev_b, cur_a, cur_b);
                }
                self.emit(cur_a, a_out.row_mut(j));
                self.emit(cur_b, b_out.row_mut(j));
                std::mem::swap(prev_a, cur_a);
                std::mem::swap(prev_b, cur_b);
            }
            interval += 2;
        }
        if rest.height() > 0 {
            let mut reader = reader_at(first_interval + interval);
            self.decode_rows(&mut reader, (first_interval + interval) * unit_rows, rest);
        }
    }

    #[cfg(feature = "rayon")]
    #[inline(always)]
    fn first_row_pair(&self, ra: &mut JpegBitReader<'_>, rb: &mut JpegBitReader<'_>, ca: &mut [u16], cb: &mut [u16]) {
        let mut la = [self.initial; NF];
        let mut lb = [self.initial; NF];
        for (pa, pb) in ca.chunks_exact_mut(NF).zip(cb.chunks_exact_mut(NF)) {
            for c in 0..NF {
                let va = la[c].wrapping_add(self.tables[c].diff(ra));
                let vb = lb[c].wrapping_add(self.tables[c].diff(rb));
                pa[c] = va;
                pb[c] = vb;
                la[c] = va;
                lb[c] = vb;
            }
        }
    }

    #[cfg(feature = "rayon")]
    #[inline(always)]
    fn normal_row_pair(
        &self,
        ra: &mut JpegBitReader<'_>,
        rb: &mut JpegBitReader<'_>,
        prev_a: &[u16],
        prev_b: &[u16],
        ca: &mut [u16],
        cb: &mut [u16],
    ) {
        let mut la: [u16; NF] = std::array::from_fn(|c| prev_a[c]);
        let mut lb: [u16; NF] = std::array::from_fn(|c| prev_b[c]);
        for (pa, pb) in ca.chunks_exact_mut(NF).zip(cb.chunks_exact_mut(NF)) {
            for c in 0..NF {
                let va = la[c].wrapping_add(self.tables[c].diff(ra));
                let vb = lb[c].wrapping_add(self.tables[c].diff(rb));
                pa[c] = va;
                pb[c] = vb;
                la[c] = va;
                lb[c] = vb;
            }
        }
    }

    fn decode_rows(&self, reader: &mut JpegBitReader<'_>, first_row: usize, mut window: RasterWindow<'_, u16>) {
        let samples = self.width * NF;
        let mut prev = vec![0u16; samples];
        let mut cur = vec![0u16; samples];
        let mut state = StreamState::at(first_row * self.width);
        for dy in 0..window.height() {
            if reader.is_exhausted() {
                log_warn!("Lossless scan data exhausted at row {}", first_row + dy);
                return;
            }
            self.decode_row(reader, first_row + dy, &prev, &mut cur, &mut state);
            self.emit(&cur, window.row_mut(dy));
            std::mem::swap(&mut prev, &mut cur);
        }
    }
}

impl LosslessScan<'_> {
    pub(super) fn decode(&self, window: RasterWindow<'_, u16>) {
        match self.tables.len() {
            1 => self.decode_nf::<1>(window),
            2 => self.decode_nf::<2>(window),
            3 => self.decode_nf::<3>(window),
            4 => self.decode_nf::<4>(window),
            n => log_warn!("Lossless scan with {} components is not supported", n),
        }
    }

    fn kernel<const NF: usize>(&self) -> Kernel<'_, NF> {
        Kernel {
            tables: std::array::from_fn(|c| self.tables[c]),
            width: self.width,
            predictor: self.predictor,
            initial: self.initial,
            restart_interval: self.restart_interval,
            point_transform: self.point_transform,
        }
    }

    fn decode_nf<const NF: usize>(&self, window: RasterWindow<'_, u16>) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let kernel = self.kernel::<NF>();
        let (window, _) = window.split_rows(self.height);

        #[cfg(feature = "rayon")]
        let window = match self.decode_restart_groups(&kernel, window) {
            Ok(()) => return,
            Err(window) => window,
        };
        #[cfg(feature = "rayon")]
        let window = match self.decode_speculative(&kernel, window) {
            Ok(()) => return,
            Err(window) => window,
        };

        let mut reader = JpegBitReader::new(self.entropy);
        kernel.decode_rows(&mut reader, 0, window);
    }

    #[cfg(feature = "rayon")]
    fn decode_restart_groups<'w, const NF: usize>(
        &self,
        kernel: &Kernel<'_, NF>,
        window: RasterWindow<'w, u16>,
    ) -> Result<(), RasterWindow<'w, u16>> {
        use rayon::prelude::*;

        let ri = self.restart_interval;
        let w = self.width;
        if ri == 0 || !(ri.is_multiple_of(w) || w.is_multiple_of(ri)) {
            return Err(window);
        }
        if w * self.height * NF < PARALLEL_MIN_SAMPLES || window.height() < 2 {
            return Err(window);
        }
        let intervals = (w * self.height).div_ceil(ri);
        if self.restart_offsets.len() != intervals {
            return Err(window);
        }

        let unit_rows = if ri.is_multiple_of(w) { ri / w } else { 1 };
        let units = window.height().div_ceil(unit_rows);
        let groups = (rayon::current_num_threads() * GROUPS_PER_THREAD).clamp(1, units);
        let rows_per_group = units.div_ceil(groups) * unit_rows;

        let interleave = self.predictor == 1 && ri.is_multiple_of(w);
        let bands = window.into_row_bands(rows_per_group);
        bands.into_par_iter().enumerate().for_each(|(g, band)| {
            let first_row = g * rows_per_group;
            let interval = first_row * w / ri;
            if interleave {
                kernel.decode_intervals_paired(self.entropy, self.restart_offsets, interval, unit_rows, band);
                return;
            }
            let Some(&offset) = self.restart_offsets.get(interval) else {
                return;
            };
            let mut reader = JpegBitReader::new(&self.entropy[offset.min(self.entropy.len())..]);
            kernel.decode_rows(&mut reader, first_row, band);
        });
        Ok(())
    }

    #[cfg(feature = "rayon")]
    fn decode_speculative<'w, const NF: usize>(
        &self,
        kernel: &Kernel<'_, NF>,
        mut window: RasterWindow<'w, u16>,
    ) -> Result<(), RasterWindow<'w, u16>> {
        use rayon::prelude::*;

        let row_len = self.width * NF;
        let total = row_len * self.height;
        let data = self.entropy;
        if self.restart_interval != 0
            || !self.uniform_tables
            || total < PARALLEL_MIN_SAMPLES
            || window.width() < row_len
            || window.height() < self.height
        {
            return Err(window);
        }
        let threads = rayon::current_num_threads();
        let chunks = (threads * GROUPS_PER_THREAD).min(data.len() / SPECULATIVE_MIN_CHUNK_BYTES);
        if threads < 2 || chunks < 2 {
            return Err(window);
        }
        let table = self.tables[0];
        let starts: Vec<u64> = (0..chunks).map(|c| (c * data.len() / chunks) as u64 * 8).collect();

        let marks: Vec<Vec<u64>> = (0..chunks)
            .into_par_iter()
            .map(|c| {
                if c == 0 {
                    return Vec::new();
                }
                let mut reader = JpegBitReader::new(data);
                reader.seek_bits(starts[c]);
                let mut marks = Vec::with_capacity(SPECULATIVE_MARKS);
                for _ in 0..SPECULATIVE_MARKS {
                    marks.push(reader.bit_position());
                    table.skip(&mut reader);
                }
                marks
            })
            .collect();

        let limit = total.saturating_add(SPECULATIVE_MARKS);
        let search = |c: usize| SyncSearch::new(data, starts[c], &marks[c + 1], starts[c + 1], limit);
        let syncs: Vec<Option<(usize, usize)>> = (0..(chunks - 1).div_ceil(2))
            .into_par_iter()
            .flat_map_iter(|p| {
                let first = 2 * p;
                let mut a = search(first);
                if first + 1 < chunks - 1 {
                    let mut b = search(first + 1);
                    SyncSearch::skim_pair(&mut a, &mut b, table);
                    vec![a.finish(table), b.finish(table)]
                } else {
                    vec![a.finish(table)]
                }
            })
            .collect();

        let mut begin = vec![(0usize, 0u64)];
        let mut skipped_prev = 0usize;
        for c in 1..chunks {
            let Some((count, mark)) = syncs[c - 1] else {
                return Err(window);
            };
            let (prev_start, _) = begin[c - 1];
            let start = prev_start + count - skipped_prev;
            if start >= total {
                break;
            }
            begin.push((start, marks[c][mark]));
            skipped_prev = mark;
        }

        let stride = window.stride();
        let base = SharedSamples(window.as_mut_ptr());
        let base = &base;
        let writer = |c: usize| {
            let (first, position) = begin[c];
            let end = begin.get(c + 1).map(|b| b.0).unwrap_or(total);
            DiffWriter::new(data, position, first, end, row_len, stride)
        };
        (0..begin.len().div_ceil(2)).into_par_iter().for_each(|p| {
            let mut a = writer(2 * p);
            if 2 * p + 1 < begin.len() {
                let mut b = writer(2 * p + 1);
                DiffWriter::decode_pair(&mut a, &mut b, table, base.0);
            } else {
                a.decode(table, base.0);
            }
        });

        kernel.reconstruct_in_place(window);
        Ok(())
    }
}

#[cfg(feature = "rayon")]
struct SharedSamples(*mut u16);

#[cfg(feature = "rayon")]
unsafe impl Sync for SharedSamples {}

#[cfg(feature = "rayon")]
struct SyncSearch<'a> {
    reader: JpegBitReader<'a>,
    next: &'a [u64],
    boundary: u64,
    count: usize,
    limit: usize,
}

#[cfg(feature = "rayon")]
impl<'a> SyncSearch<'a> {
    fn new(data: &'a [u8], start: u64, next: &'a [u64], boundary: u64, limit: usize) -> Self {
        let mut reader = JpegBitReader::new(data);
        reader.seek_bits(start);
        Self { reader, next, boundary, count: 0, limit }
    }

    fn skim_pair(a: &mut Self, b: &mut Self, table: &LosslessHuffman) {
        let (mut ra, mut rb) = (a.reader, b.reader);
        let (mut ca, mut cb) = (a.count, b.count);
        while ra.bit_position() < a.boundary && rb.bit_position() < b.boundary && ca < a.limit && cb < b.limit {
            table.skip(&mut ra);
            table.skip(&mut rb);
            ca += 1;
            cb += 1;
        }
        (a.reader, b.reader, a.count, b.count) = (ra, rb, ca, cb);
    }

    fn finish(&mut self, table: &LosslessHuffman) -> Option<(usize, usize)> {
        let mut reader = self.reader;
        let mut count = self.count;
        while reader.bit_position() < self.boundary && count < self.limit {
            table.skip(&mut reader);
            count += 1;
        }
        let mut k = 0;
        while count < self.limit && !reader.is_exhausted() {
            let position = reader.bit_position();
            while k < self.next.len() && self.next[k] < position {
                k += 1;
            }
            if k == self.next.len() {
                return None;
            }
            if self.next[k] == position {
                return Some((count, k));
            }
            table.skip(&mut reader);
            count += 1;
        }
        None
    }
}

#[cfg(feature = "rayon")]
struct DiffWriter<'a> {
    reader: JpegBitReader<'a>,
    index: usize,
    end: usize,
    row_len: usize,
    stride: usize,
}

#[cfg(feature = "rayon")]
impl<'a> DiffWriter<'a> {
    fn new(data: &'a [u8], position: u64, first: usize, end: usize, row_len: usize, stride: usize) -> Self {
        let mut reader = JpegBitReader::new(data);
        reader.seek_bits(position);
        Self { reader, index: first, end, row_len, stride }
    }

    fn segment(&self, base: *mut u16) -> &'a mut [u16] {
        let (row, col) = (self.index / self.row_len, self.index % self.row_len);
        let len = (self.row_len - col).min(self.end - self.index);
        unsafe { std::slice::from_raw_parts_mut(base.add(row * self.stride + col), len) }
    }

    fn decode(&mut self, table: &LosslessHuffman, base: *mut u16) {
        let mut reader = self.reader;
        while self.index < self.end {
            let out = self.segment(base);
            for sample in out.iter_mut() {
                *sample = table.diff(&mut reader);
            }
            self.index += out.len();
        }
        self.reader = reader;
    }

    fn decode_pair(a: &mut Self, b: &mut Self, table: &LosslessHuffman, base: *mut u16) {
        let (mut ra, mut rb) = (a.reader, b.reader);
        while a.index < a.end && b.index < b.end {
            let (sa, sb) = (a.segment(base), b.segment(base));
            let n = sa.len().min(sb.len());
            for (x, y) in sa[..n].iter_mut().zip(&mut sb[..n]) {
                *x = table.diff(&mut ra);
                *y = table.diff(&mut rb);
            }
            a.index += n;
            b.index += n;
        }
        (a.reader, b.reader) = (ra, rb);
        a.decode(table, base);
        b.decode(table, base);
    }
}
