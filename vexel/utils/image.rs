use crate::log_warn;
use crate::utils::channel_simd;
use serde::Serialize;

fn drop_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    let num_pixels = pixels.len() / 4;
    let mut out = vec![0u8; num_pixels * 3];
    channel_simd::rgba_to_rgb(&pixels, &mut out);
    out
}

fn add_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    let num_pixels = pixels.len() / 3;
    let mut out = vec![0u8; num_pixels * 4];
    channel_simd::rgb_to_rgba(&pixels, &mut out);
    out
}

#[inline(always)]
fn scale_f64(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn scale_f64_slice(src: &[f64]) -> Vec<u8> {
    src.iter().map(|&v| scale_f64(v)).collect()
}

fn scale_u16_slice(src: &[u16]) -> Vec<u8> {
    let mut dst = vec![0u8; src.len()];
    channel_simd::scale_u16_to_u8(src, &mut dst);
    dst
}

fn scale_f32_slice(src: &[f32]) -> Vec<u8> {
    let mut dst = vec![0u8; src.len()];
    channel_simd::scale_f32_to_u8(src, &mut dst);
    dst
}

fn gray_to_rgb(gray: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; gray.len() * 3];
    for (chunk, &g) in out.chunks_exact_mut(3).zip(gray) {
        chunk[0] = g;
        chunk[1] = g;
        chunk[2] = g;
    }
    out
}

fn gray_to_rgba(gray: &[u8]) -> Vec<u8> {
    let mut out = vec![255u8; gray.len() * 4];
    for (chunk, &g) in out.chunks_exact_mut(4).zip(gray) {
        chunk[0] = g;
        chunk[1] = g;
        chunk[2] = g;
    }
    out
}

fn gray_alpha_to_rgb(la: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; la.len() / 2 * 3];
    for (chunk, pair) in out.chunks_exact_mut(3).zip(la.chunks_exact(2)) {
        chunk[0] = pair[0];
        chunk[1] = pair[0];
        chunk[2] = pair[0];
    }
    out
}

fn gray_alpha_to_rgba(la: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; la.len() * 2];
    for (chunk, pair) in out.chunks_exact_mut(4).zip(la.chunks_exact(2)) {
        chunk[0] = pair[0];
        chunk[1] = pair[0];
        chunk[2] = pair[0];
        chunk[3] = pair[1];
    }
    out
}

fn bits_to_bytes(mut bits: Vec<u8>) -> Vec<u8> {
    for v in &mut bits {
        *v = if *v != 0 { 255 } else { 0 };
    }
    bits
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ImageFormat {
    Jpeg,
    JpegLs,
    Png,
    Gif,
    Bmp,
    NetPbmP1,
    NetPbmP2,
    NetPbmP3,
    NetPbmP4,
    NetPbmP5,
    NetPbmP6,
    NetPbmP7,
    Hdr,
    Tiff,
    Dng,
    Tga,
    Jbig1,
    Ico,
    Cur,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PixelFormat {
    RGB8,
    RGBA8,
    RGB16,
    RGBA16,
    RGB32F,
    RGBA32F,
    RGB64F,
    RGBA64F,
    L1,
    L8,
    L16,
    LA8,
    LA16,
    L32F,
    LA32F,
    L64F,
    LA64F,
}

/// A decoded image, consisting of one or more frames.
///
/// Single-frame formats (JPEG, PNG, BMP, …) always produce exactly one frame.
/// Animated formats (GIF, APNG) produce one frame per animation step, each
/// with its own pixel data and display delay.
///
/// The pixel format reflects what the decoder produced and may vary by format
/// and image content. Use [`into_rgb8`](Image::into_rgb8) or
/// [`into_rgba8`](Image::into_rgba8) to normalize all frames to a known format.
#[derive(Debug)]
pub struct Image {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    frames: Vec<ImageFrame>,
}

impl Image {
    pub(crate) fn new(width: u32, height: u32, pixel_format: PixelFormat, frames: Vec<ImageFrame>) -> Image {
        Image {
            width,
            height,
            pixel_format,
            frames,
        }
    }

    pub fn from_frame(frame: ImageFrame) -> Image {
        Image {
            width: frame.width(),
            height: frame.height(),
            pixel_format: frame.pixel_format(),
            frames: Vec::from([frame]),
        }
    }

    pub(crate) fn from_pixels(width: u32, height: u32, mut pixels: PixelData) -> Image {
        pixels.correct_pixels(width, height);
        let frame = ImageFrame::new(width, height, pixels, 0);
        Image::from_frame(frame)
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
    /// The pixel format describes the channel layout and bit depth of the image data,
    /// such as [`PixelFormat::RGB8`] for 8-bit RGB or [`PixelFormat::RGBA16`] for 16-bit RGBA.
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }

    /// Returns a reference to the first frame's pixel data.
    ///
    /// For single-frame images this is the full image data. For animated images,
    /// only the first frame is returned. Returns an empty [`PixelData::RGB8`] if
    /// the image has no frames. Use [`into_pixels`](Self::into_pixels) to take
    /// ownership of the data without copying.
    pub fn pixels(&self) -> &PixelData {
        static EMPTY: PixelData = PixelData::RGB8(Vec::new());
        self.frames.first().map(|frame| &frame.pixels).unwrap_or(&EMPTY)
    }

    /// Consumes the image and returns the first frame's pixel data without copying.
    ///
    /// Returns an empty [`PixelData::RGB8`] if the image has no frames.
    pub fn into_pixels(self) -> PixelData {
        self.frames
            .into_iter()
            .next()
            .map(|frame| frame.pixels)
            .unwrap_or(PixelData::RGB8(Vec::new()))
    }

    /// Returns `true` if the image's pixel format includes an alpha channel.
    pub fn has_alpha(&self) -> bool {
        matches!(
            self.pixel_format,
            PixelFormat::RGBA8 | PixelFormat::RGBA16 | PixelFormat::RGBA32F | PixelFormat::RGBA64F | PixelFormat::LA8 | PixelFormat::LA16 | PixelFormat::LA32F | PixelFormat::LA64F
        )
    }

    /// Returns all frames of the image.
    ///
    /// Single-frame images have exactly one entry. Animated formats such as GIF
    /// and APNG may have multiple frames, each with its own pixel data and delay.
    pub fn frames(&self) -> &Vec<ImageFrame> {
        &self.frames
    }

    /// Consumes the image and returns all of its frames without copying.
    pub fn into_frames(self) -> Vec<ImageFrame> {
        self.frames
    }

    /// Converts the image to RGB8 format, consuming the original image.
    ///
    /// This method converts all frames to RGB8 format, while [`to_rgb8`](Self::to_rgb8) returns
    /// vector of the first frame's pixels, converted to RGB8 format without
    /// modifying the original image
    pub fn into_rgb8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgb8()).collect();

        Image::new(self.width, self.height, PixelFormat::RGB8, new_frames)
    }

    /// Converts the image to RGBA8 format, consuming the original image.
    ///
    /// This method converts all frames to RGBA8 format, while [`to_rgba8`](Self::to_rgba8) returns
    /// vector of the first frame's pixels, converted to RGBA8 format without
    /// modifying the original image
    pub fn into_rgba8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgba8()).collect();

        Image::new(self.width, self.height, PixelFormat::RGBA8, new_frames)
    }

    /// Returns the first frame's pixels as a vector of RGB8 bytes.
    ///
    /// All pixel formats are converted to 8-bit RGB. Higher bit depths are scaled
    /// down, alpha channels are dropped, and grayscale values are expanded to
    /// three equal channels. Use [`into_rgb8`](Self::into_rgb8) to convert all
    /// frames in place.
    pub fn to_rgb8(&self) -> Vec<u8> {
        self.frames.first().map(|frame| frame.to_rgb8()).unwrap_or_default()
    }

    /// Returns the first frame's pixels as a vector of RGBA8 bytes.
    ///
    /// All pixel formats are converted to 8-bit RGBA. Higher bit depths are scaled
    /// down, opaque images without an alpha channel have 255 added, and grayscale
    /// values are expanded to three equal channels with the appropriate alpha.
    /// Use [`into_rgba8`](Self::into_rgba8) to convert all frames in place.
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.frames.first().map(|frame| frame.to_rgba8()).unwrap_or_default()
    }
}

/// A single frame within an [`Image`].
///
/// Contains the frame's dimensions, pixel data, and display delay. For
/// single-frame images the delay is always `0`. Pixel data may be in any
/// [`PixelData`] variant; use [`into_rgb8`](ImageFrame::into_rgb8) or
/// [`into_rgba8`](ImageFrame::into_rgba8) to convert to a known format.
#[derive(Debug, Clone)]
pub struct ImageFrame {
    width: u32,
    height: u32,
    pixels: PixelData,
    delay: u32,
}

impl ImageFrame {
    pub(crate) fn new(width: u32, height: u32, pixels: PixelData, delay: u32) -> ImageFrame {
        ImageFrame {
            width,
            height,
            pixels,
            delay,
        }
    }

    /// Returns the width of the frame in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of the frame in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns a reference to the frame's raw pixel data.
    pub fn pixels(&self) -> &PixelData {
        &self.pixels
    }

    /// Consumes the frame and returns its pixel data without copying.
    pub fn into_pixels(self) -> PixelData {
        self.pixels
    }

    /// Returns the display delay for this frame in milliseconds.
    ///
    /// For single-frame images this is always 0. For animated formats such as
    /// GIF and APNG, this controls how long the frame is shown before advancing
    /// to the next one.
    pub fn delay(&self) -> u32 {
        self.delay
    }

    /// Returns the pixel format of this frame.
    ///
    /// The pixel format describes the channel layout and bit depth of the frame
    /// data, such as [`PixelFormat::RGB8`] for 8-bit RGB or [`PixelFormat::RGBA16`]
    /// for 16-bit RGBA.
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixels.pixel_format()
    }

    /// Returns `true` if this frame's pixel format includes an alpha channel.
    pub fn has_alpha(&self) -> bool {
        matches!(
            self.pixels,
            PixelData::RGBA8(_)
                | PixelData::RGBA16(_)
                | PixelData::RGBA32F(_)
                | PixelData::RGBA64F(_)
                | PixelData::LA8(_)
                | PixelData::LA16(_)
                | PixelData::LA32F(_)
                | PixelData::LA64F(_)
        )
    }

    /// Converts this frame to RGB8 format, consuming the original frame.
    ///
    /// This method converts the frame's pixel data to RGB8 format, while
    /// [`to_rgb8`](Self::to_rgb8) returns a vector of the frame's pixels
    /// converted to RGB8 without consuming the frame.
    pub fn into_rgb8(self) -> ImageFrame {
        ImageFrame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.into_rgb8(),
            delay: self.delay,
        }
    }

    /// Returns this frame's pixels as a vector of RGB8 bytes.
    ///
    /// All pixel formats are converted to 8-bit RGB. Higher bit depths are scaled
    /// down, alpha channels are dropped, and grayscale values are expanded to
    /// three equal channels. Use [`into_rgb8`](Self::into_rgb8) to convert the
    /// frame in place.
    pub fn to_rgb8(&self) -> Vec<u8> {
        match &self.pixels {
            PixelData::RGB8(pixels) => pixels.to_vec(),
            _ => self.pixels.clone().into_rgb8_bytes(),
        }
    }

    /// Converts this frame to RGBA8 format, consuming the original frame.
    ///
    /// This method converts the frame's pixel data to RGBA8 format, while
    /// [`to_rgba8`](Self::to_rgba8) returns a vector of the frame's pixels
    /// converted to RGBA8 without consuming the frame.
    pub fn into_rgba8(self) -> ImageFrame {
        ImageFrame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.into_rgba8(),
            delay: self.delay,
        }
    }

    /// Returns this frame's pixels as a vector of RGBA8 bytes.
    ///
    /// All pixel formats are converted to 8-bit RGBA. Higher bit depths are scaled
    /// down, opaque images without an alpha channel have 255 added, and grayscale
    /// values are expanded to three equal channels with the appropriate alpha.
    /// Use [`into_rgba8`](Self::into_rgba8) to convert the frame in place.
    pub fn to_rgba8(&self) -> Vec<u8> {
        match &self.pixels {
            PixelData::RGBA8(pixels) => pixels.to_vec(),
            _ => self.pixels.clone().into_rgba8_bytes(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PixelData {
    RGB8(Vec<u8>),
    RGBA8(Vec<u8>),
    RGB16(Vec<u16>),
    RGBA16(Vec<u16>),
    RGB32F(Vec<f32>),
    RGBA32F(Vec<f32>),
    RGB64F(Vec<f64>),
    RGBA64F(Vec<f64>),
    L1(Vec<u8>),
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
            PixelData::L1(_) => PixelFormat::L1,
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

    /// Converts this pixel data to [`PixelData::RGB8`], consuming the original.
    ///
    /// Conversion rules by source format:
    /// - `RGB8` — returned as-is
    /// - `RGBA8` — alpha channel is dropped
    /// - `RGB16` / `RGBA16` — values are scaled from `u16` to `u8`; alpha is dropped for RGBA
    /// - `RGB32F` / `RGBA32F` — float values in `[0.0, 1.0]` are scaled to `[0, 255]`; alpha is dropped for RGBA
    /// - `RGB64F` / `RGBA64F` — same as 32F variants
    /// - `L1` — bit values `0`/`1` are expanded to `[0, 0, 0]` / `[255, 255, 255]`
    /// - `L8` / `LA8` — gray value is replicated to all three channels; alpha is dropped
    /// - `L16` / `LA16` — gray value is scaled to `u8`, then replicated; alpha is dropped
    /// - `L32F` / `LA32F` / `L64F` / `LA64F` — float gray is scaled to `u8`, then replicated; alpha is dropped
    pub fn into_rgb8(self) -> PixelData {
        PixelData::RGB8(self.into_rgb8_bytes())
    }

    /// Converts this pixel data to raw RGB8 bytes, consuming the original.
    ///
    /// Same conversion rules as [`into_rgb8`](Self::into_rgb8), but returns the
    /// byte vector directly without wrapping it in a [`PixelData`].
    pub fn into_rgb8_bytes(self) -> Vec<u8> {
        match self {
            PixelData::RGB8(p) => p,
            PixelData::RGBA8(p) => drop_transparency_channel(p),
            PixelData::RGB16(p) => scale_u16_slice(&p),
            PixelData::RGBA16(p) => drop_transparency_channel(scale_u16_slice(&p)),
            PixelData::RGB32F(p) => scale_f32_slice(&p),
            PixelData::RGBA32F(p) => drop_transparency_channel(scale_f32_slice(&p)),
            PixelData::RGB64F(p) => scale_f64_slice(&p),
            PixelData::RGBA64F(p) => drop_transparency_channel(scale_f64_slice(&p)),
            PixelData::L1(p) => gray_to_rgb(&bits_to_bytes(p)),
            PixelData::L8(p) => gray_to_rgb(&p),
            PixelData::L16(p) => gray_to_rgb(&scale_u16_slice(&p)),
            PixelData::LA8(p) => gray_alpha_to_rgb(&p),
            PixelData::LA16(p) => gray_alpha_to_rgb(&scale_u16_slice(&p)),
            PixelData::L32F(p) => gray_to_rgb(&scale_f32_slice(&p)),
            PixelData::LA32F(p) => gray_alpha_to_rgb(&scale_f32_slice(&p)),
            PixelData::L64F(p) => gray_to_rgb(&scale_f64_slice(&p)),
            PixelData::LA64F(p) => gray_alpha_to_rgb(&scale_f64_slice(&p)),
        }
    }

    /// Converts this pixel data to [`PixelData::RGBA8`], consuming the original.
    ///
    /// Conversion rules by source format:
    /// - `RGBA8` — returned as-is
    /// - `RGB8` — alpha channel is added with value `255`
    /// - `RGB16` / `RGBA16` — values are scaled from `u16` to `u8`; alpha is added as `255` for RGB16
    /// - `RGB32F` / `RGBA32F` — float values in `[0.0, 1.0]` are scaled to `[0, 255]`; alpha is added as `255` for RGB32F
    /// - `RGB64F` / `RGBA64F` — same as 32F variants
    /// - `L1` — bit values `0`/`1` are expanded to `[0, 0, 0, 255]` / `[255, 255, 255, 255]`
    /// - `L8` / `LA8` — gray value is replicated to all three channels; alpha is `255` for L8, preserved for LA8
    /// - `L16` / `LA16` — gray value is scaled to `u8`, then replicated; alpha is scaled and preserved for LA16
    /// - `L32F` / `LA32F` / `L64F` / `LA64F` — float gray is scaled to `u8`, then replicated; alpha is scaled and preserved
    pub fn into_rgba8(self) -> PixelData {
        PixelData::RGBA8(self.into_rgba8_bytes())
    }

    /// Converts this pixel data to raw RGBA8 bytes, consuming the original.
    ///
    /// Same conversion rules as [`into_rgba8`](Self::into_rgba8), but returns the
    /// byte vector directly without wrapping it in a [`PixelData`].
    pub fn into_rgba8_bytes(self) -> Vec<u8> {
        match self {
            PixelData::RGB8(p) => add_transparency_channel(p),
            PixelData::RGBA8(p) => p,
            PixelData::RGB16(p) => add_transparency_channel(scale_u16_slice(&p)),
            PixelData::RGBA16(p) => scale_u16_slice(&p),
            PixelData::RGB32F(p) => add_transparency_channel(scale_f32_slice(&p)),
            PixelData::RGBA32F(p) => scale_f32_slice(&p),
            PixelData::RGB64F(p) => add_transparency_channel(scale_f64_slice(&p)),
            PixelData::RGBA64F(p) => scale_f64_slice(&p),
            PixelData::L1(p) => gray_to_rgba(&bits_to_bytes(p)),
            PixelData::L8(p) => gray_to_rgba(&p),
            PixelData::L16(p) => gray_to_rgba(&scale_u16_slice(&p)),
            PixelData::LA8(p) => gray_alpha_to_rgba(&p),
            PixelData::LA16(p) => gray_alpha_to_rgba(&scale_u16_slice(&p)),
            PixelData::L32F(p) => gray_to_rgba(&scale_f32_slice(&p)),
            PixelData::LA32F(p) => gray_alpha_to_rgba(&scale_f32_slice(&p)),
            PixelData::L64F(p) => gray_to_rgba(&scale_f64_slice(&p)),
            PixelData::LA64F(p) => gray_alpha_to_rgba(&scale_f64_slice(&p)),
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
            PixelData::L1(pixels) => pixels,
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

    /// Returns the raw pixel data as a mutable byte slice.
    ///
    /// Same layout and endian rules as [`as_bytes`](Self::as_bytes).
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        match self {
            PixelData::RGB8(pixels) => pixels,
            PixelData::RGBA8(pixels) => pixels,
            PixelData::RGB16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
            PixelData::RGBA16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
            PixelData::RGB32F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4)
            },
            PixelData::RGBA32F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4)
            },
            PixelData::RGB64F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 8)
            },
            PixelData::RGBA64F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 8)
            },
            PixelData::L1(pixels) => pixels,
            PixelData::L8(pixels) => pixels,
            PixelData::L16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
            PixelData::LA8(pixels) => pixels,
            PixelData::LA16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
            PixelData::L32F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4)
            },
            PixelData::LA32F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4)
            },
            PixelData::L64F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 8)
            },
            PixelData::LA64F(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 8)
            },
        }
    }

    // Used as a last resort to correct the number of pixels in the image
    // in case something went wrong during decoding
    pub(crate) fn correct_pixels(&mut self, width: u32, height: u32) {
        let components_per_pixel = match self {
            PixelData::RGB8(_) | PixelData::RGB16(_) | PixelData::RGB32F(_) | PixelData::RGB64F(_) => 3,
            PixelData::RGBA8(_) | PixelData::RGBA16(_) | PixelData::RGBA32F(_) | PixelData::RGBA64F(_) => 4,
            PixelData::L1(_) | PixelData::L8(_) | PixelData::L16(_) | PixelData::L32F(_) | PixelData::L64F(_) => 1,
            PixelData::LA8(_) | PixelData::LA16(_) | PixelData::LA32F(_) | PixelData::LA64F(_) => 2,
        };

        let expected_len = (width * height) as usize * components_per_pixel;

        match self {
            PixelData::RGB8(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA8(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB16(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA16(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB32F(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA32F(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB64F(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA64F(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::L1(pixels) => correct_vec(pixels, expected_len, 1),
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
