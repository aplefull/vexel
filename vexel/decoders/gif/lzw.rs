use std::ops::Range;

use crate::log_warn;
use crate::utils::lzw::{decode_padded, LzwOptions, INPUT_PADDING};

const OVERSIZED_MIN_BYTES: usize = 64 << 20;

pub struct FrameIndices {
    data: Vec<u8>,
    stride: usize,
    rows: usize,
    interlace_height: Option<usize>,
}

impl FrameIndices {
    pub fn empty() -> FrameIndices {
        FrameIndices { data: Vec::new(), stride: 0, rows: 0, interlace_height: None }
    }

    pub fn into_buffer(self) -> Vec<u8> {
        self.data
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn row(&self, y: usize, width: usize) -> &[u8] {
        let source_row = match self.interlace_height {
            Some(height) => interlaced_row(y, height),
            None => y,
        };
        let start = source_row * self.stride;
        &self.data[start..start + width]
    }
}

fn interlaced_row(y: usize, height: usize) -> usize {
    let pass1 = height.div_ceil(8);
    let pass2 = (height + 3) / 8;
    let pass3 = (height + 1) / 4;
    match y % 8 {
        0 => y / 8,
        4 => pass1 + y / 8,
        2 | 6 => pass1 + pass2 + y / 4,
        _ => pass1 + pass2 + pass3 + y / 2,
    }
}

pub fn gather_sub_blocks(source: &[u8], chain: Range<usize>, payload: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(payload + INPUT_PADDING);
    let mut pos = chain.start;
    while pos < chain.end {
        let size = source[pos] as usize;
        data.extend_from_slice(&source[pos + 1..pos + 1 + size]);
        pos += 1 + size;
    }
    data
}

pub fn decode_frame_indices(
    min_code_size: u8,
    data: Vec<u8>,
    frame_width: usize,
    frame_height: usize,
    visible_width: usize,
    visible_height: usize,
    interlaced: bool,
    mut out: Vec<u8>,
) -> FrameIndices {
    if visible_width == 0 || visible_height == 0 || frame_width == 0 {
        return FrameIndices::empty();
    }

    let stream_rows = if !interlaced {
        visible_height
    } else if visible_height == frame_height {
        frame_height
    } else {
        (0..visible_height).map(|y| interlaced_row(y, frame_height)).max().unwrap_or(0) + 1
    };

    let cap = frame_width.saturating_mul(stream_rows);
    let budget = visible_width.saturating_mul(visible_height).saturating_mul(16).max(OVERSIZED_MIN_BYTES);
    if cap > budget {
        log_warn!(
            "GIF frame is {}x{} for a {}x{} visible area, decoding only part of it",
            frame_width,
            frame_height,
            visible_width,
            visible_height
        );
        if interlaced {
            return FrameIndices::empty();
        }
    }
    let cap = cap.min(budget / frame_width * frame_width);

    let written = decode_padded(LzwOptions::gif(min_code_size), data, &mut out, cap);
    if interlaced {
        out[written..cap].fill(0);
    }

    let rows = if interlaced {
        visible_height
    } else {
        let full = written / frame_width;
        (full + usize::from(written % frame_width >= visible_width)).min(visible_height)
    };

    FrameIndices { data: out, stride: frame_width, rows, interlace_height: interlaced.then_some(frame_height) }
}
