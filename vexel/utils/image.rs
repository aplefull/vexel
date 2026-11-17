use serde::Serialize;

use crate::log_warn;
use crate::utils::convert::{self, Channels};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::sample::{Alpha, Sample, SampleType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[non_exhaustive]
pub enum ImageFormat {
    Jpeg,
    JpegLs,
    Png,
    Gif,
    Bmp,
    Netpbm,
    Hdr,
    Tiff,
    Dng,
    Tga,
    Jbig1,
    Jbig2,
    Ico,
    Dicom,
    Exr,
    Jpeg2000,
    Farbfeld,
    Rpgmvp,
    Pgx,
    JpegXs,
    JpegXr,
    Lerc,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
    RGB8,
    RGBA8,
    RGB16,
    RGBA16,
    RGB32F,
    RGBA32F,
    RGB64F,
    RGBA64F,
    L8,
    L16,
    LA8,
    LA16,
    L32F,
    LA32F,
    L64F,
    LA64F,
}

impl PixelFormat {
    /// Returns the number of channels per pixel.
    ///
    /// 1 for grayscale, 2 for grayscale with alpha, 3 for RGB and 4 for RGBA.
    pub fn channels(self) -> usize {
        self.layout().count()
    }

    /// Returns the type of each stored sample.
    ///
    /// This is the `S` that [`Image::samples`] and [`Image::into_samples`] accept
    /// for an image in this format.
    pub fn sample_type(self) -> SampleType {
        match self {
            PixelFormat::RGB8 | PixelFormat::RGBA8 | PixelFormat::L8 | PixelFormat::LA8 => SampleType::U8,
            PixelFormat::RGB16 | PixelFormat::RGBA16 | PixelFormat::L16 | PixelFormat::LA16 => SampleType::U16,
            PixelFormat::RGB32F | PixelFormat::RGBA32F | PixelFormat::L32F | PixelFormat::LA32F => SampleType::F32,
            PixelFormat::RGB64F | PixelFormat::RGBA64F | PixelFormat::L64F | PixelFormat::LA64F => SampleType::F64,
        }
    }

    /// Returns the number of bytes one pixel occupies in memory.
    ///
    /// Equal to the channel count times the size of one sample, so
    /// [`PixelFormat::RGBA16`] is 8 bytes and [`PixelFormat::L32F`] is 4.
    pub fn bytes_per_pixel(self) -> usize {
        self.channels() * self.sample_type().size()
    }

    /// Returns `true` if the format includes an alpha channel.
    pub fn has_alpha(self) -> bool {
        self.layout().has_alpha()
    }

    /// Returns `true` if the format is grayscale, with or without alpha.
    pub fn is_gray(self) -> bool {
        matches!(self.layout(), Channels::L | Channels::LA)
    }

    /// Returns the same channel layout with a different sample type.
    ///
    /// For example, [`PixelFormat::LA8`] with [`SampleType::U16`] gives
    /// [`PixelFormat::LA16`]. Pass the result to [`Image::convert`] to change only
    /// the bit depth of an image.
    pub fn with_sample_type(self, sample_type: SampleType) -> PixelFormat {
        PixelFormat::from_parts(self.layout(), sample_type)
    }

    pub(crate) fn layout(self) -> Channels {
        match self {
            PixelFormat::L8 | PixelFormat::L16 | PixelFormat::L32F | PixelFormat::L64F => Channels::L,
            PixelFormat::LA8 | PixelFormat::LA16 | PixelFormat::LA32F | PixelFormat::LA64F => Channels::LA,
            PixelFormat::RGB8 | PixelFormat::RGB16 | PixelFormat::RGB32F | PixelFormat::RGB64F => Channels::Rgb,
            PixelFormat::RGBA8 | PixelFormat::RGBA16 | PixelFormat::RGBA32F | PixelFormat::RGBA64F => Channels::Rgba,
        }
    }

    pub(crate) fn from_parts(layout: Channels, sample_type: SampleType) -> PixelFormat {
        match (layout, sample_type) {
            (Channels::L, SampleType::U8) => PixelFormat::L8,
            (Channels::L, SampleType::U16) => PixelFormat::L16,
            (Channels::L, SampleType::F32) => PixelFormat::L32F,
            (Channels::L, SampleType::F64) => PixelFormat::L64F,
            (Channels::LA, SampleType::U8) => PixelFormat::LA8,
            (Channels::LA, SampleType::U16) => PixelFormat::LA16,
            (Channels::LA, SampleType::F32) => PixelFormat::LA32F,
            (Channels::LA, SampleType::F64) => PixelFormat::LA64F,
            (Channels::Rgb, SampleType::U8) => PixelFormat::RGB8,
            (Channels::Rgb, SampleType::U16) => PixelFormat::RGB16,
            (Channels::Rgb, SampleType::F32) => PixelFormat::RGB32F,
            (Channels::Rgb, SampleType::F64) => PixelFormat::RGB64F,
            (Channels::Rgba, SampleType::U8) => PixelFormat::RGBA8,
            (Channels::Rgba, SampleType::U16) => PixelFormat::RGBA16,
            (Channels::Rgba, SampleType::F32) => PixelFormat::RGBA32F,
            (Channels::Rgba, SampleType::F64) => PixelFormat::RGBA64F,
        }
    }
}

/// A single decoded picture.
///
/// An `Image` is always exactly one picture. Animated formats such as GIF and
/// APNG expose their individual steps as [`ImageFrame`]s through
/// [`Vexel::decode_frames`](crate::Vexel::decode_frames). Decoding an animation
/// with [`Vexel::decode`](crate::Vexel::decode) yields only its first frame.
///
/// The pixel format reflects what the decoder produced and may vary by format
/// and image content. Use [`to_rgba`](Image::to_rgba) to get a known layout and
/// sample type, or [`samples`](Image::samples) to read the stored data as is.
#[derive(Debug, Clone)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: PixelData,
}

impl Image {
    /// Creates an image from raw samples.
    ///
    /// `samples` holds the pixels row by row with channels interleaved, in the
    /// layout given by `format`. Returns an error if `S` does not match
    /// `format.sample_type()` or if `samples.len()` is not
    /// `width * height * format.channels()`.
    pub fn from_samples<S: Sample>(
        width: u32,
        height: u32,
        format: PixelFormat,
        samples: Vec<S>,
    ) -> VexelResult<Image> {
        if format.sample_type() != S::TYPE {
            return Err(VexelError::Custom(format!("{:?} is not stored as {:?} samples", format, S::TYPE)));
        }

        let expected =
            u64::from(width).checked_mul(u64::from(height)).and_then(|p| p.checked_mul(format.channels() as u64));
        if expected != Some(samples.len() as u64) {
            return Err(VexelError::Custom(format!(
                "{}x{} {:?} image needs {} samples, got {}",
                width,
                height,
                format,
                expected.map_or_else(|| "too many".to_string(), |n| n.to_string()),
                samples.len()
            )));
        }

        Ok(Image { width, height, pixels: S::wrap(format.layout(), samples) })
    }

    pub(crate) fn from_pixels(width: u32, height: u32, mut pixels: PixelData) -> Image {
        pixels.correct_pixels(width, height);
        Image { width, height, pixels }
    }

    /// Returns the width of the image in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of the image in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the pixel format of the image.
    ///
    /// The pixel format describes the channel layout and sample type of the image data,
    /// such as [`PixelFormat::RGB8`] for 8-bit RGB or [`PixelFormat::RGBA16`] for 16-bit RGBA.
    pub fn format(&self) -> PixelFormat {
        self.pixels.pixel_format()
    }

    /// Returns the stored pixel data as a byte slice.
    ///
    /// For multi-byte sample types (`u16`, `f32`, `f64`) the slice reinterprets the
    /// underlying memory directly with no endian conversion, so the byte order
    /// matches the native platform.
    pub fn as_bytes(&self) -> &[u8] {
        self.pixels.as_bytes()
    }

    /// Returns the stored samples without converting them.
    ///
    /// The samples are in the layout given by [`format`](Self::format). Returns
    /// `None` if the image is not stored as `S`; check `format().sample_type()`, or
    /// call [`convert`](Self::convert) first to get a known format.
    pub fn samples<S: Sample>(&self) -> Option<&[S]> {
        S::view(&self.pixels)
    }

    /// Consumes the image and returns its stored samples without copying.
    ///
    /// Same rules as [`samples`](Self::samples). If the image is not stored as `S`,
    /// the image is returned unchanged in `Err`.
    pub fn into_samples<S: Sample>(self) -> Result<Vec<S>, Image> {
        let Image { width, height, pixels } = self;
        S::take(pixels).map_err(|pixels| Image { width, height, pixels })
    }

    /// Returns the image's pixels converted to RGB in sample type `S`.
    ///
    /// Use [`into_rgb`](Self::into_rgb) to avoid the copy when the image is already RGB in `S`.
    pub fn to_rgb<S: Sample>(&self) -> Vec<S> {
        convert::collect(&self.pixels, Channels::Rgb)
    }

    /// Returns the image's pixels converted to RGBA in sample type `S`.
    ///
    /// Use [`into_rgba`](Self::into_rgba) to avoid the copy when the image is already
    /// RGBA in `S`.
    pub fn to_rgba<S: Sample>(&self) -> Vec<S> {
        convert::collect(&self.pixels, Channels::Rgba)
    }

    /// Converts the image to RGB in sample type `S`, consuming it.
    ///
    /// If the image is already RGB
    /// in `S`, the existing buffer is returned without copying.
    pub fn into_rgb<S: Sample>(self) -> Vec<S> {
        convert::into_layout(self.pixels, Channels::Rgb)
    }

    /// Converts the image to RGBA in sample type `S`, consuming it.
    ///
    /// If the image is already
    /// RGBA in `S`, the existing buffer is returned without copying.
    pub fn into_rgba<S: Sample>(self) -> Vec<S> {
        convert::into_layout(self.pixels, Channels::Rgba)
    }

    /// Converts the image to RGBA in sample type `S` and writes it into `dst`.
    ///
    /// With [`Alpha::Premultiplied`] each color value is multiplied by its alpha; formats
    /// without alpha produce the same output either way.
    ///
    /// Returns an error if `dst` is not exactly `width * height * 4` samples long.
    pub fn write_rgba<S: Sample>(&self, dst: &mut [S], alpha: Alpha) -> VexelResult<()> {
        let expected = self.pixels.pixel_count() * 4;
        if dst.len() != expected {
            return Err(VexelError::Custom(format!(
                "write_rgba: {}x{} image needs a buffer of {} samples, got {}",
                self.width,
                self.height,
                expected,
                dst.len()
            )));
        }

        convert::write(&self.pixels, Channels::Rgba, dst, alpha == Alpha::Premultiplied);
        Ok(())
    }

    /// Converts the image to another pixel format, consuming it.
    ///
    pub fn convert(self, format: PixelFormat) -> Image {
        if self.format() == format {
            return self;
        }

        let pixels = convert::build(&self.pixels, format);
        Image { width: self.width, height: self.height, pixels }
    }

    pub(crate) fn pixels(&self) -> &PixelData {
        &self.pixels
    }

    pub(crate) fn into_pixels(self) -> PixelData {
        self.pixels
    }
}

/// One frame of an animation: a picture plus its display delay.
///
/// Yielded by [`Vexel::decode_frames`](crate::Vexel::decode_frames). Each frame
/// is owned by the caller and its memory is released as soon as it is dropped,
/// so an animation of any length can be processed in constant memory.
#[derive(Debug, Clone)]
pub struct ImageFrame {
    image: Image,
    delay: u32,
}

impl ImageFrame {
    /// Creates a frame from an image and a display delay in milliseconds.
    pub fn new(image: Image, delay: u32) -> ImageFrame {
        ImageFrame { image, delay }
    }

    pub(crate) fn from_pixels(width: u32, height: u32, pixels: PixelData, delay: u32) -> ImageFrame {
        ImageFrame { image: Image::from_pixels(width, height, pixels), delay }
    }

    /// Returns a reference to the frame's picture.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// Consumes the frame and returns its picture, dropping the delay.
    pub fn into_image(self) -> Image {
        self.image
    }

    /// Returns the display delay for this frame in milliseconds.
    ///
    /// For single-frame images this is always 0. For animated formats such as
    /// GIF and APNG, this controls how long the frame is shown before advancing
    /// to the next one.
    pub fn delay(&self) -> u32 {
        self.delay
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PixelData {
    RGB8(Vec<u8>),
    RGBA8(Vec<u8>),
    RGB16(Vec<u16>),
    RGBA16(Vec<u16>),
    RGB32F(Vec<f32>),
    RGBA32F(Vec<f32>),
    RGB64F(Vec<f64>),
    RGBA64F(Vec<f64>),
    L8(Vec<u8>),
    L16(Vec<u16>),
    LA8(Vec<u8>),
    LA16(Vec<u16>),
    L32F(Vec<f32>),
    LA32F(Vec<f32>),
    L64F(Vec<f64>),
    LA64F(Vec<f64>),
}

impl PixelData {
    /// Returns the [`PixelFormat`] that corresponds to this variant.
    pub fn pixel_format(&self) -> PixelFormat {
        match self {
            PixelData::RGB8(_) => PixelFormat::RGB8,
            PixelData::RGBA8(_) => PixelFormat::RGBA8,
            PixelData::RGB16(_) => PixelFormat::RGB16,
            PixelData::RGBA16(_) => PixelFormat::RGBA16,
            PixelData::RGB32F(_) => PixelFormat::RGB32F,
            PixelData::RGBA32F(_) => PixelFormat::RGBA32F,
            PixelData::RGB64F(_) => PixelFormat::RGB64F,
            PixelData::RGBA64F(_) => PixelFormat::RGBA64F,
            PixelData::L8(_) => PixelFormat::L8,
            PixelData::L16(_) => PixelFormat::L16,
            PixelData::LA8(_) => PixelFormat::LA8,
            PixelData::LA16(_) => PixelFormat::LA16,
            PixelData::L32F(_) => PixelFormat::L32F,
            PixelData::LA32F(_) => PixelFormat::LA32F,
            PixelData::L64F(_) => PixelFormat::L64F,
            PixelData::LA64F(_) => PixelFormat::LA64F,
        }
    }

    /// Returns the number of whole pixels in this pixel data.
    pub fn pixel_count(&self) -> usize {
        self.sample_count() / self.components_per_pixel()
    }

    fn sample_count(&self) -> usize {
        match self {
            PixelData::RGB8(p) | PixelData::RGBA8(p) | PixelData::L8(p) | PixelData::LA8(p) => p.len(),
            PixelData::RGB16(p) | PixelData::RGBA16(p) | PixelData::L16(p) | PixelData::LA16(p) => p.len(),
            PixelData::RGB32F(p) | PixelData::RGBA32F(p) | PixelData::L32F(p) | PixelData::LA32F(p) => p.len(),
            PixelData::RGB64F(p) | PixelData::RGBA64F(p) | PixelData::L64F(p) | PixelData::LA64F(p) => p.len(),
        }
    }

    fn components_per_pixel(&self) -> usize {
        match self {
            PixelData::RGB8(_) | PixelData::RGB16(_) | PixelData::RGB32F(_) | PixelData::RGB64F(_) => 3,
            PixelData::RGBA8(_) | PixelData::RGBA16(_) | PixelData::RGBA32F(_) | PixelData::RGBA64F(_) => 4,
            PixelData::L8(_) | PixelData::L16(_) | PixelData::L32F(_) | PixelData::L64F(_) => 1,
            PixelData::LA8(_) | PixelData::LA16(_) | PixelData::LA32F(_) | PixelData::LA64F(_) => 2,
        }
    }

    /// Returns the raw pixel data as a byte slice.
    ///
    /// For multi-byte element types (`u16`, `f32`, `f64`) the slice reinterprets
    /// the underlying memory directly with no endian conversion, so the byte order
    /// matches the native platform.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            PixelData::RGB8(pixels) => pixels,
            PixelData::RGBA8(pixels) => pixels,
            PixelData::RGB16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
            },
            PixelData::RGBA16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
            },
            PixelData::RGB32F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4)
            },
            PixelData::RGBA32F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4)
            },
            PixelData::RGB64F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 8)
            },
            PixelData::RGBA64F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 8)
            },
            PixelData::L8(pixels) => pixels,
            PixelData::L16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
            },
            PixelData::LA8(pixels) => pixels,
            PixelData::LA16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
            },
            PixelData::L32F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4)
            },
            PixelData::LA32F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4)
            },
            PixelData::L64F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 8)
            },
            PixelData::LA64F(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 8)
            },
        }
    }

    // Used as a last resort to correct the number of pixels in the image
    // in case something went wrong during decoding
    pub(crate) fn correct_pixels(&mut self, width: u32, height: u32) {
        let components_per_pixel = self.components_per_pixel();

        let Some(expected_len) = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(components_per_pixel as u64))
            .and_then(|len| usize::try_from(len).ok())
        else {
            log_warn!("Cannot correct pixel count: {}x{} image size overflows", width, height);
            return;
        };

        match self {
            PixelData::RGB8(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA8(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB16(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA16(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB32F(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA32F(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB64F(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA64F(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::L8(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::L16(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::L32F(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::L64F(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::LA8(pixels) => correct_vec(pixels, expected_len, 2),
            PixelData::LA16(pixels) => correct_vec(pixels, expected_len, 2),
            PixelData::LA32F(pixels) => correct_vec(pixels, expected_len, 2),
            PixelData::LA64F(pixels) => correct_vec(pixels, expected_len, 2),
        }

        fn correct_vec<T: Clone + Default>(pixels: &mut Vec<T>, expected_len: usize, components_per_pixel: usize) {
            let current_len = pixels.len();

            if current_len == expected_len {
                return;
            }

            if current_len > expected_len {
                log_warn!(
                    "Truncating excess pixels. Received from decoder: {}, Expected: {}",
                    current_len / components_per_pixel,
                    expected_len / components_per_pixel
                );

                pixels.truncate(expected_len);
            } else {
                log_warn!(
                    "Adding missing pixels. Received from decoder: {}, Expected: {}",
                    current_len / components_per_pixel,
                    expected_len / components_per_pixel
                );

                pixels.resize(expected_len, T::default());
            }
        }
    }
}
