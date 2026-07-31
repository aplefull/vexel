use crate::utils::error::VexelResult;
use crate::utils::image::{Image, ImageFrame};

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

pub(crate) struct BufferedFrames {
    frames: std::vec::IntoIter<ImageFrame>,
}

impl BufferedFrames {
    pub(crate) fn new(frames: Vec<ImageFrame>) -> BufferedFrames {
        BufferedFrames { frames: frames.into_iter() }
    }
}

impl FrameSource for BufferedFrames {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        Ok(self.frames.next())
    }

    fn frame_count_hint(&self) -> Option<usize> {
        Some(self.frames.len())
    }
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
pub struct Frames<'a> {
    source: Box<dyn FrameSource + 'a>,
    finished: bool,
}

impl<'a> Frames<'a> {
    pub(crate) fn new(source: impl FrameSource + 'a) -> Frames<'a> {
        Frames { source: Box::new(source), finished: false }
    }

    pub fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        if self.finished {
            return Ok(None);
        }

        match self.source.next_frame() {
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
        self.source.frame_count_hint()
    }
}

impl Iterator for Frames<'_> {
    type Item = VexelResult<ImageFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_frame().transpose()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.frame_count_hint() {
            Some(n) => (n, Some(n)),
            None => (0, None),
        }
    }
}
