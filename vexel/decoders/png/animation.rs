use std::ops::Range;
use std::sync::Arc;

use super::pixels::PngFormat;
use super::types::{FctlChunk, PngFrame};
use crate::utils::error::VexelResult;
use crate::utils::frames::{reserve_canvas, FramePrefetch, FrameSource, OutputBudget};
use crate::{log_warn, ImageFrame, Limits, PixelData};

const DISPOSE_BACKGROUND: u8 = 1;
const DISPOSE_PREVIOUS: u8 = 2;
const BLEND_SOURCE: u8 = 0;

impl PngFrame {
    pub fn new(fctl_info: FctlChunk) -> Self {
        Self { fctl_info, fdat: Vec::new(), idat: None }
    }

    pub fn append_idat(&mut self, idat: &[u8], range: Range<usize>) {
        if !self.fdat.is_empty() {
            self.fdat.extend_from_slice(&idat[range]);
            return;
        }

        self.idat = Some(match self.idat.take() {
            Some(existing) => existing.start..range.end,
            None => range,
        });
    }

    pub fn take_fdat(&mut self, idat: &[u8]) -> Vec<u8> {
        match self.idat.take() {
            Some(range) => idat.get(range).unwrap_or_default().to_vec(),
            None => std::mem::take(&mut self.fdat),
        }
    }

    pub fn has_data(&self) -> bool {
        !self.fdat.is_empty() || self.idat.as_ref().is_some_and(|range| !range.is_empty())
    }

    pub fn is_drawable(&self) -> bool {
        self.fctl_info.width > 0 && self.fctl_info.height > 0 && self.has_data()
    }

    fn data<'a>(&'a self, idat: &'a [u8]) -> &'a [u8] {
        match &self.idat {
            Some(range) => idat.get(range.clone()).unwrap_or_default(),
            None => &self.fdat,
        }
    }
}

pub struct PngData {
    pub format: PngFormat,
    pub frames: Vec<PngFrame>,
    pub idat: Vec<u8>,
}

impl PngData {
    fn frame_fits(&self, fctl: &FctlChunk) -> bool {
        fctl.width > 0
            && fctl.height > 0
            && u64::from(fctl.x_offset) + u64::from(fctl.width) <= u64::from(self.format.width)
            && u64::from(fctl.y_offset) + u64::from(fctl.height) <= u64::from(self.format.height)
    }

    fn decode_frame(&self, index: usize) -> Option<Vec<u8>> {
        let frame = &self.frames[index];
        let fctl = &frame.fctl_info;
        if !self.frame_fits(fctl) {
            return None;
        }

        let pixels = self.format.decode_frame(frame.data(&self.idat), fctl.width, fctl.height)?;
        Some(pixels.into_rgba8_bytes())
    }
}

pub struct PngFrames {
    data: Arc<PngData>,
    animation: Option<Animation>,
    still_pending: bool,
}

impl PngFrames {
    pub fn new(
        data: PngData,
        animated: bool,
        lookahead: bool,
        limits: &mut Limits,
        max_alloc: Option<u64>,
    ) -> PngFrames {
        let data = Arc::new(data);

        let animation = if animated && data.frames.iter().any(PngFrame::is_drawable) {
            match Animation::new(&data, lookahead, limits, max_alloc) {
                Ok(animation) => Some(animation),
                Err(e) => {
                    log_warn!("Error decoding APNG frames: {:?}", e);
                    None
                }
            }
        } else {
            None
        };

        PngFrames { data, animation, still_pending: true }
    }

    pub fn next_animation_frame(&mut self) -> Option<ImageFrame> {
        let animation = self.animation.as_mut()?;
        match animation.next_frame(&self.data) {
            Ok(frame) => frame,
            Err(e) => {
                log_warn!("Error decoding APNG frames: {:?}", e);
                None
            }
        }
    }

    fn animation_emitted(&self) -> bool {
        self.animation.as_ref().is_some_and(|animation| animation.emitted > 0)
    }
}

impl FrameSource for PngFrames {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        if let Some(frame) = self.next_animation_frame() {
            return Ok(Some(frame));
        }

        if !self.still_pending || self.animation_emitted() {
            return Ok(None);
        }
        self.still_pending = false;

        let idat = match Arc::get_mut(&mut self.data) {
            Some(data) => std::mem::take(&mut data.idat),
            None => self.data.idat.clone(),
        };
        let format = &self.data.format;
        let pixels = format.decode_image(idat)?;
        Ok(Some(ImageFrame::new(format.width, format.height, pixels, 0)))
    }

    fn frame_count_hint(&self) -> Option<usize> {
        let animated = self.animation.as_ref().map_or(0, |animation| animation.remaining(&self.data));
        let still = (self.still_pending && !self.animation_emitted()) as usize;
        Some(animated.max(still))
    }
}

struct Animation {
    prefetch: FramePrefetch<Option<Vec<u8>>>,
    canvas: Vec<u8>,
    saved: Vec<u8>,
    previous: Option<usize>,
    next_index: usize,
    emitted: usize,
    exhausted: bool,
    budget: OutputBudget,
}

impl Animation {
    fn new(
        data: &Arc<PngData>,
        lookahead: bool,
        limits: &mut Limits,
        max_alloc: Option<u64>,
    ) -> VexelResult<Animation> {
        let (width, height) = (data.format.width, data.format.height);
        let canvas = reserve_canvas(limits, width, height)?;
        let saved = if data.frames.iter().any(|frame| frame.fctl_info.dispose_op == DISPOSE_PREVIOUS) {
            reserve_canvas(limits, width, height)?
        } else {
            Vec::new()
        };

        let compressed_bytes = data.frames.iter().map(|frame| frame.fdat.len() as u64).sum::<u64>();
        let budget = OutputBudget::new(max_alloc, compressed_bytes.saturating_add(data.idat.len() as u64));

        let costs = data
            .frames
            .iter()
            .map(|frame| (frame.fctl_info.width as usize).saturating_mul(frame.fctl_info.height as usize) * 4)
            .collect();
        let shared = data.clone();
        let prefetch = FramePrefetch::new(costs, lookahead, move |index| shared.decode_frame(index));

        Ok(Animation { prefetch, canvas, saved, previous: None, next_index: 0, emitted: 0, exhausted: false, budget })
    }

    fn remaining(&self, data: &PngData) -> usize {
        if self.exhausted {
            return 0;
        }

        data.frames[self.next_index..].iter().filter(|frame| data.frame_fits(&frame.fctl_info)).count()
    }

    fn next_frame(&mut self, data: &PngData) -> VexelResult<Option<ImageFrame>> {
        let (width, height) = (data.format.width, data.format.height);

        while !self.exhausted {
            let pixels = match self.prefetch.next() {
                Ok(Some(pixels)) => pixels,
                Ok(None) => {
                    self.exhausted = true;
                    break;
                }
                Err(e) => {
                    self.exhausted = true;
                    if self.emitted == 0 {
                        return Err(e);
                    }
                    log_warn!("Stopping APNG decode after {} frames: {:?}", self.emitted, e);
                    break;
                }
            };

            let index = self.next_index;
            self.next_index += 1;
            let fctl = &data.frames[index].fctl_info;

            if !data.frame_fits(fctl) {
                log_warn!(
                    "Skipping APNG frame with invalid dimensions: {}x{} at ({}, {})",
                    fctl.width,
                    fctl.height,
                    fctl.x_offset,
                    fctl.y_offset
                );
                continue;
            }

            let Some(pixels) = pixels else {
                continue;
            };

            let copies = if fctl.dispose_op == DISPOSE_PREVIOUS { 3 } else { 1 };
            if !self.budget.charge((self.canvas.len() as u64).saturating_mul(copies), self.emitted) {
                log_warn!("APNG frames exceed the composition work budget, stopping after {} frames", self.emitted);
                self.exhausted = true;
                break;
            }

            self.dispose_previous(data);
            if fctl.dispose_op == DISPOSE_PREVIOUS && !self.saved.is_empty() {
                self.saved.copy_from_slice(&self.canvas);
            }
            self.compose(&pixels, fctl, width);
            self.previous = Some(index);
            self.emitted += 1;

            let frame = PixelData::RGBA8(self.canvas.clone());
            return Ok(Some(ImageFrame::new(width, height, frame, frame_delay_ms(fctl))));
        }

        Ok(None)
    }

    fn dispose_previous(&mut self, data: &PngData) {
        let Some(previous) = self.previous else {
            return;
        };
        let fctl = &data.frames[previous].fctl_info;

        match fctl.dispose_op {
            DISPOSE_BACKGROUND => {
                let row_bytes = fctl.width as usize * 4;
                for y in 0..fctl.height {
                    let row_start = canvas_offset(data.format.width, fctl.x_offset, fctl.y_offset + y);
                    if let Some(row) = self.canvas.get_mut(row_start..row_start + row_bytes) {
                        row.fill(0);
                    }
                }
            }
            DISPOSE_PREVIOUS if !self.saved.is_empty() => self.canvas.copy_from_slice(&self.saved),
            _ => {}
        }
    }

    fn compose(&mut self, frame: &[u8], fctl: &FctlChunk, canvas_width: u32) {
        let row_bytes = fctl.width as usize * 4;

        for y in 0..fctl.height {
            let frame_start = y as usize * row_bytes;
            let canvas_start = canvas_offset(canvas_width, fctl.x_offset, fctl.y_offset + y);
            let Some(source) = frame.get(frame_start..frame_start + row_bytes) else {
                continue;
            };
            let Some(target) = self.canvas.get_mut(canvas_start..canvas_start + row_bytes) else {
                continue;
            };

            if fctl.blend_op == BLEND_SOURCE {
                target.copy_from_slice(source);
            } else {
                for (src, dst) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
                    blend_over(src, dst);
                }
            }
        }
    }
}

fn blend_over(src: &[u8], dst: &mut [u8]) {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return;
    }

    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }

    for i in 0..3 {
        let s = src[i] as f32;
        let d = dst[i] as f32;
        dst[i] = ((s * src_a + d * dst_a * (1.0 - src_a)) / out_a) as u8;
    }
    dst[3] = (out_a * 255.0) as u8;
}

fn canvas_offset(canvas_width: u32, x: u32, y: u32) -> usize {
    (y as usize * canvas_width as usize + x as usize) * 4
}

fn frame_delay_ms(fctl: &FctlChunk) -> u32 {
    if fctl.delay_den == 0 {
        fctl.delay_num as u32 * 10
    } else {
        (fctl.delay_num as f32 / fctl.delay_den as f32 * 1000.0).round() as u32
    }
}
