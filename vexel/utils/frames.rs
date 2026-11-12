use std::collections::VecDeque;
use std::sync::Arc;

use crate::utils::error::VexelResult;
use crate::utils::image::{Image, ImageFrame};
use crate::utils::limits::Limits;

const LOOKAHEAD_BATCH: usize = 16;
const LOOKAHEAD_BATCH_BYTES: usize = 64 * 1024 * 1024;
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
        SingleFrame { frame: Some(ImageFrame::new(width, height, image.into_pixels(), 0)) }
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
    lookahead: bool,
    next: usize,
    batch: VecDeque<T>,
    #[cfg(feature = "rayon")]
    decoded_upto: usize,
    #[cfg(feature = "rayon")]
    pending: Option<std::sync::mpsc::Receiver<VexelResult<Vec<T>>>>,
}

impl<T: Send + 'static> FramePrefetch<T> {
    pub(crate) fn new(
        costs: Vec<usize>,
        lookahead: bool,
        decode: impl Fn(usize) -> T + Send + Sync + 'static,
    ) -> FramePrefetch<T> {
        FramePrefetch {
            decode: Arc::new(decode),
            costs,
            lookahead,
            next: 0,
            batch: VecDeque::new(),
            #[cfg(feature = "rayon")]
            decoded_upto: 0,
            #[cfg(feature = "rayon")]
            pending: None,
        }
    }

    pub(crate) fn next(&mut self) -> VexelResult<Option<T>> {
        if self.next >= self.costs.len() {
            return Ok(None);
        }

        if self.batch.is_empty() {
            self.fill_batch()?;
        }

        let item = self.batch.pop_front();
        if item.is_some() {
            self.next += 1;
        }
        Ok(item)
    }

    fn batch_end(&self, start: usize) -> usize {
        let batch = if cfg!(feature = "rayon") && self.lookahead { LOOKAHEAD_BATCH } else { 1 };
        let limit = (start + batch).min(self.costs.len());
        let mut bytes = 0usize;
        let mut end = start;
        while end < limit {
            bytes = bytes.saturating_add(self.costs[end]);
            if end > start && bytes > LOOKAHEAD_BATCH_BYTES {
                break;
            }
            end += 1;
        }
        end
    }

    fn decode_range(decode: &DecodeFn<T>, start: usize, end: usize) -> Vec<T> {
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            (start..end).into_par_iter().map(|index| decode(index)).collect()
        }

        #[cfg(not(feature = "rayon"))]
        (start..end).map(|index| decode(index)).collect()
    }

    fn fill_batch(&mut self) -> VexelResult<()> {
        #[cfg(feature = "rayon")]
        if let Some(rx) = self.pending.take() {
            let decoded = rx
                .recv()
                .map_err(|_| crate::utils::error::VexelError::Custom("Frame prefetch worker disappeared".into()))??;
            self.batch = decoded.into();
            self.start_prefetch();
            return Ok(());
        }

        let start = self.next;
        let end = self.batch_end(start);
        self.batch = Self::decode_range(&self.decode, start, end).into();

        #[cfg(feature = "rayon")]
        {
            self.decoded_upto = end;
            self.start_prefetch();
        }

        Ok(())
    }

    #[cfg(feature = "rayon")]
    fn start_prefetch(&mut self) {
        if !self.lookahead || self.pending.is_some() || self.decoded_upto >= self.costs.len() {
            return;
        }

        let start = self.decoded_upto;
        let end = self.batch_end(start);
        self.decoded_upto = end;

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.pending = Some(rx);

        let decode = self.decode.clone();
        rayon::spawn(move || {
            let decoded = crate::catch_decoder_panic(|| Ok(Self::decode_range(&decode, start, end)));
            let _ = tx.send(decoded);
        });
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
///     println!("{}x{} for {}ms", frame.width(), frame.height(), frame.delay());
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
