use super::types::GifFrameInfo;
use crate::log_warn;

pub trait IndexSink {
    fn push(&mut self, bytes: &[u8]) -> bool;
}

pub struct LinearSink {
    pub out: Vec<u8>,
    cap: usize,
}

impl LinearSink {
    pub fn new(cap: usize) -> LinearSink {
        LinearSink { out: Vec::with_capacity(cap), cap }
    }
}

impl IndexSink for LinearSink {
    #[inline]
    fn push(&mut self, bytes: &[u8]) -> bool {
        let room = self.cap - self.out.len();
        if bytes.len() >= room {
            self.out.extend_from_slice(&bytes[..room]);
            return false;
        }
        self.out.extend_from_slice(bytes);
        true
    }
}

const INTERLACE_PASSES: [(usize, usize); 4] = [(0, 8), (4, 8), (2, 4), (1, 2)];

pub struct WindowSink {
    out: Vec<u8>,
    frame_width: usize,
    frame_height: usize,
    visible_width: usize,
    visible_height: usize,
    interlaced: bool,
    pass: usize,
    row: usize,
    col: usize,
    done: bool,
}

impl WindowSink {
    pub fn new(
        frame_width: usize,
        frame_height: usize,
        visible_width: usize,
        visible_height: usize,
        interlaced: bool,
    ) -> WindowSink {
        WindowSink {
            out: vec![0u8; visible_width * visible_height],
            frame_width,
            frame_height,
            visible_width,
            visible_height,
            interlaced,
            pass: 0,
            row: 0,
            col: 0,
            done: frame_width == 0 || visible_width == 0 || visible_height == 0,
        }
    }

    fn advance_row(&mut self) {
        if !self.interlaced {
            self.row += 1;
            self.done = self.row >= self.visible_height;
            return;
        }

        self.row += INTERLACE_PASSES[self.pass].1;
        while self.row >= self.frame_height {
            self.pass += 1;
            if self.pass >= INTERLACE_PASSES.len() {
                self.done = true;
                return;
            }
            self.row = INTERLACE_PASSES[self.pass].0;
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        if !self.interlaced && !self.done {
            let rows = self.row + usize::from(self.col >= self.visible_width);
            self.out.truncate(rows.min(self.visible_height) * self.visible_width);
        }
        self.out
    }
}

impl IndexSink for WindowSink {
    fn push(&mut self, bytes: &[u8]) -> bool {
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.done {
                return false;
            }

            let take = rest.len().min(self.frame_width - self.col);
            if self.row < self.visible_height && self.col < self.visible_width {
                let count = take.min(self.visible_width - self.col);
                let dst = self.row * self.visible_width + self.col;
                self.out[dst..dst + count].copy_from_slice(&rest[..count]);
            }

            self.col += take;
            rest = &rest[take..];
            if self.col == self.frame_width {
                self.col = 0;
                self.advance_row();
            }
        }
        !self.done
    }
}

pub fn decompress_lzw<S: IndexSink>(frame: &GifFrameInfo, sink: &mut S) {
    let min_code_size = frame.lzw_minimum_code_size.clamp(2, 8) as u32;
    let clear_code = 1u32 << min_code_size;
    let end_code = clear_code + 1;

    let initial_entries = (end_code + 1) as usize;

    let mut arena: Vec<u8> = Vec::with_capacity(65536);
    let mut table: Vec<(u32, u32)> = Vec::with_capacity(4096);

    for i in 0..clear_code {
        let offset = arena.len() as u32;
        arena.push(i as u8);
        table.push((offset, 1));
    }
    table.push((0, 0));
    table.push((0, 0));

    let initial_arena_len = arena.len();

    let data = &frame.data;
    let data_len = data.len();

    let mut bit_buf: u64 = 0;
    let mut n_bits: u32 = 0;
    let mut byte_pos: usize = 0;

    let read_code = |bit_buf: &mut u64, n_bits: &mut u32, byte_pos: &mut usize, width: u32| -> Option<u32> {
        while *n_bits < width {
            if *byte_pos >= data_len {
                return None;
            }
            let space = (64 - *n_bits) / 8;
            let available = (data_len - *byte_pos).min(space as usize).min(4);
            for _ in 0..available {
                *bit_buf |= (data[*byte_pos] as u64) << *n_bits;
                *byte_pos += 1;
                *n_bits += 8;
            }
        }
        let code = (*bit_buf & ((1u64 << width) - 1)) as u32;
        *bit_buf >>= width;
        *n_bits -= width;
        Some(code)
    };

    let mut code_size = min_code_size + 1;
    let mut next_code = end_code + 1;
    let mut prev_code: Option<u32> = None;

    while let Some(code) = read_code(&mut bit_buf, &mut n_bits, &mut byte_pos, code_size) {
        if code == clear_code {
            code_size = min_code_size + 1;
            next_code = end_code + 1;
            table.truncate(initial_entries);
            arena.truncate(initial_arena_len);
            prev_code = None;
            continue;
        }

        if code == end_code {
            break;
        }

        if let Some(prev) = prev_code {
            if code < next_code {
                let (offset, len) = match table.get(code as usize) {
                    Some(&(_, 0)) | None => {
                        log_warn!("Invalid LZW code: {}", code);
                        prev_code = Some(code);
                        if next_code >= (1 << code_size) && code_size < 12 {
                            code_size += 1;
                        }
                        continue;
                    }
                    Some(&entry) => entry,
                };

                let first = arena[offset as usize];
                if !sink.push(&arena[offset as usize..offset as usize + len as usize]) {
                    break;
                }

                if next_code < 4096 {
                    if let Some(&(prev_off, prev_len)) = table.get(prev as usize) {
                        if prev_len > 0 {
                            let new_offset = arena.len() as u32;
                            arena.reserve(prev_len as usize + 1);
                            arena.extend_from_within(prev_off as usize..prev_off as usize + prev_len as usize);
                            arena.push(first);
                            table.push((new_offset, prev_len + 1));
                            next_code += 1;
                        }
                    }
                }
            } else if code == next_code {
                if let Some(&(prev_off, prev_len)) = table.get(prev as usize) {
                    if prev_len > 0 {
                        let new_len = prev_len + 1;
                        let first = arena[prev_off as usize];
                        let new_offset = arena.len() as u32;
                        arena.reserve(new_len as usize);
                        arena.extend_from_within(prev_off as usize..prev_off as usize + prev_len as usize);
                        arena.push(first);
                        if !sink.push(&arena[new_offset as usize..new_offset as usize + new_len as usize]) {
                            break;
                        }
                        if next_code < 4096 {
                            table.push((new_offset, new_len));
                            next_code += 1;
                        }
                    }
                }
            } else {
                log_warn!("Invalid LZW code: {}", code);
            }
        } else if let Some(&(offset, len)) = table.get(code as usize) {
            if !sink.push(&arena[offset as usize..offset as usize + len as usize]) {
                break;
            }
        }

        prev_code = Some(code);

        if next_code >= (1 << code_size) && code_size < 12 {
            code_size += 1;
        }
    }
}
