#[cfg(feature = "rayon")]
use std::collections::VecDeque;
use std::sync::Arc;

use crate::utils::error::VexelResult;
use crate::utils::image::{Image, ImageFrame};
use crate::utils::limits::Limits;

#[cfg(feature = "rayon")]
const LOOKAHEAD_FRAMES: usize = 64;
#[cfg(feature = "rayon")]
const LOOKAHEAD_BYTES: usize = 64 * 1024 * 1024;
const OUTPUT_ALLOC_FACTOR: u64 = 8;
const OUTPUT_PER_INPUT_BYTE: u64 = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameExtent {
    All,
    First,
}

pub(crate) trait FrameSource {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>>;

    fn frame_count_hint(&self) -> Option<usize> {
        None
    }
}

pub(crate) struct SingleFrame {
    frame: Option<ImageFrame>,
}

impl SingleFrame {
    pub(crate) fn new(image: Image) -> SingleFrame {
        let width = image.width();
        let height = image.height();
        SingleFrame { frame: Some(ImageFrame::from_pixels(width, height, image.into_pixels(), 0)) }
    }
}

impl FrameSource for SingleFrame {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        Ok(self.frame.take())
    }

    fn frame_count_hint(&self) -> Option<usize> {
        Some(self.frame.is_some() as usize)
    }
}

type DecodeFn<T> = Arc<dyn Fn(usize) -> T + Send + Sync>;

pub(crate) struct FramePrefetch<T: Send + 'static> {
    decode: DecodeFn<T>,
    costs: Vec<usize>,
    #[cfg(feature = "rayon")]
    lookahead: bool,
    next: usize,
    #[cfg(feature = "rayon")]
    spawned: usize,
    #[cfg(feature = "rayon")]
    in_flight: VecDeque<(usize, std::sync::mpsc::Receiver<VexelResult<T>>)>,
    #[cfg(feature = "rayon")]
    in_flight_bytes: usize,
}

impl<T: Send + 'static> FramePrefetch<T> {
    pub(crate) fn new(
        costs: Vec<usize>,
        lookahead: bool,
        decode: impl Fn(usize) -> T + Send + Sync + 'static,
    ) -> FramePrefetch<T> {
        #[cfg(not(feature = "rayon"))]
        let _ = lookahead;

        FramePrefetch {
            decode: Arc::new(decode),
            costs,
            #[cfg(feature = "rayon")]
            lookahead,
            next: 0,
            #[cfg(feature = "rayon")]
            spawned: 0,
            #[cfg(feature = "rayon")]
            in_flight: VecDeque::new(),
            #[cfg(feature = "rayon")]
            in_flight_bytes: 0,
        }
    }

    pub(crate) fn next(&mut self) -> VexelResult<Option<T>> {
        if self.next >= self.costs.len() {
            return Ok(None);
        }

        #[cfg(feature = "rayon")]
        if self.lookahead
            && (!self.in_flight.is_empty()
                || (rayon::current_thread_index().is_none() && rayon::current_num_threads() > 1))
        {
            self.refill();
            if let Some((cost, rx)) = self.in_flight.pop_front() {
                self.in_flight_bytes -= cost;
                self.next += 1;
                let item = rx
                    .recv()
                    .map_err(|_| crate::utils::error::VexelError::Custom("Frame prefetch worker disappeared".into()))?;
                self.refill();
                return item.map(Some);
            }
        }

        let item = crate::catch_decoder_panic(|| Ok((self.decode)(self.next)))?;
        self.next += 1;
        #[cfg(feature = "rayon")]
        {
            self.spawned = self.spawned.max(self.next);
        }
        Ok(Some(item))
    }

    #[cfg(feature = "rayon")]
    fn refill(&mut self) {
        let window = (2 * rayon::current_num_threads()).clamp(2, LOOKAHEAD_FRAMES);
        while self.spawned < self.costs.len() && self.in_flight.len() < window {
            let cost = self.costs[self.spawned];
            if !self.in_flight.is_empty() && self.in_flight_bytes.saturating_add(cost) > LOOKAHEAD_BYTES {
                break;
            }

            let index = self.spawned;
            let decode = self.decode.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            rayon::spawn(move || {
                let _ = tx.send(crate::catch_decoder_panic(|| Ok(decode(index))));
            });

            self.in_flight.push_back((cost, rx));
            self.in_flight_bytes = self.in_flight_bytes.saturating_add(cost);
            self.spawned += 1;
        }
    }
}

pub(crate) struct OutputBudget {
    remaining: Option<u64>,
}

impl OutputBudget {
    pub(crate) fn new(max_alloc: Option<u64>, compressed_bytes: u64) -> OutputBudget {
        let remaining = max_alloc.map(|max_alloc| {
            max_alloc
                .saturating_mul(OUTPUT_ALLOC_FACTOR)
                .saturating_add(compressed_bytes.saturating_mul(OUTPUT_PER_INPUT_BYTE))
        });
        OutputBudget { remaining }
    }

    pub(crate) fn charge(&mut self, cost: u64, emitted: usize) -> bool {
        let Some(remaining) = self.remaining.as_mut() else {
            return true;
        };

        if cost > *remaining && emitted > 0 {
            return false;
        }

        *remaining = remaining.saturating_sub(cost);
        true
    }
}

pub(crate) fn reserve_canvas(limits: &mut Limits, width: u32, height: u32) -> VexelResult<Vec<u8>> {
    limits.check_dimensions(width, height)?;
    let size = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    limits.reserve_usize(size)?;
    Ok(vec![0u8; size])
}

/// A streaming iterator over the frames of an animated image.
///
/// Each [`ImageFrame`] is owned by the caller and its memory is released when
/// the frame is dropped, so animations of any length can be processed in
/// constant memory.
///
/// Frames can be pulled explicitly with [`next_frame`](Frames::next_frame),
/// which surfaces per-frame errors, or through the [`Iterator`] implementation,
/// which yields `VexelResult<ImageFrame>`.
///
/// ```no_run
/// use vexel::Vexel;
///
/// let mut decoder = Vexel::open("animation.gif")?;
/// let mut frames = decoder.decode_frames()?;
///
/// while let Some(frame) = frames.next_frame()? {
///     println!("{}x{} for {}ms", frame.image().width(), frame.image().height(), frame.delay());
/// }
/// # Ok::<(), vexel::VexelError>(())
/// ```
pub struct Frames {
    source: Box<dyn FrameSource>,
    finished: bool,
}

impl Frames {
    pub(crate) fn new(source: impl FrameSource + 'static) -> Frames {
        Frames { source: Box::new(source), finished: false }
    }

    pub(crate) fn single(image: Image) -> Frames {
        Frames::new(SingleFrame::new(image))
    }

    pub fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        if self.finished {
            return Ok(None);
        }

        let source = &mut self.source;

        match crate::catch_decoder_panic(move || source.next_frame()) {
            Ok(Some(frame)) => Ok(Some(frame)),
            Ok(None) => {
                self.finished = true;
                Ok(None)
            }
            Err(e) => {
                self.finished = true;
                Err(e)
            }
        }
    }

    pub fn frame_count_hint(&self) -> Option<usize> {
        if self.finished {
            return Some(0);
        }
        self.source.frame_count_hint()
    }
}

impl Iterator for Frames {
    type Item = VexelResult<ImageFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_frame().transpose()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.frame_count_hint())
    }
}
