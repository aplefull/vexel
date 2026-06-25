use crate::log_warn;
use crate::utils::channel_simd;
use serde::Serialize;

fn drop_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    let num_pixels = pixels.len() / 4;
    let mut out = Vec::with_capacity(num_pixels * 3);
    unsafe { out.set_len(num_pixels * 3) };
    channel_simd::rgba_to_rgb(&pixels, &mut out);
    out
}

fn add_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    let num_pixels = pixels.len() / 3;
    let mut out = Vec::with_capacity(num_pixels * 4);
    unsafe { out.set_len(num_pixels * 4) };
    channel_simd::rgb_to_rgba(&pixels, &mut out);
    out
}

#[inline(always)]
fn scale_u16(v: u16) -> u8 {
    (v as f32 * 255.0 / 65535.0).round() as u8
}

#[inline(always)]
fn scale_f32(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[inline(always)]
fn scale_f64(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
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

    pub(crate) fn from_pixels(width: u32, height: u32, pixels: PixelData) -> Image {
        let frame = ImageFrame::new(width, height, pixels, 0);
        Image::from_frame(frame)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }

    pub fn pixels(&self) -> PixelData {
        if self.frames.len() > 0 {
            self.frames[0].pixels.clone()
        } else {
            PixelData::RGB8(Vec::new())
        }
    }

    pub fn has_alpha(&self) -> bool {
        match self.pixel_format {
            PixelFormat::RGBA8 | PixelFormat::RGBA16 | PixelFormat::RGBA32F | PixelFormat::RGBA64F | PixelFormat::LA8 | PixelFormat::LA16 | PixelFormat::LA32F | PixelFormat::LA64F => {
                true
            }
            _ => false,
        }
    }

    pub fn frames(&self) -> &Vec<ImageFrame> {
        &self.frames
    }

    /// Converts the image to RGB8 format, consuming the original image.
    ///
    /// This method converts all frames to RGB8 format, while `as_rgb8` returns
    /// vector of the first frame's pixels, converted to RGB8 format without
    /// modifying the original image
    pub fn into_rgb8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgb8()).collect();

        Image::new(self.width, self.height, PixelFormat::RGB8, new_frames)
    }

    /// Converts the image to RGBA8 format, consuming the original image.
    ///
    /// This method converts all frames to RGBA8 format, while `as_rgba8` returns
    /// vector of the first frame's pixels, converted to RGBA8 format without
    /// modifying the original image
    pub fn into_rgba8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgba8()).collect();

        Image::new(self.width, self.height, PixelFormat::RGBA8, new_frames)
    }

    /// Returns the first frame's pixels as a vector of RGB8 bytes
    pub fn as_rgb8(&self) -> Vec<u8> {
        self.pixels().into_rgb8().as_bytes().to_vec()
    }

    /// Returns the first frame's pixels as a vector of RGBA8 bytes
    pub fn as_rgba8(&self) -> Vec<u8> {
        self.pixels().into_rgba8().as_bytes().to_vec()
    }
}

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

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &PixelData {
        &self.pixels
    }

    pub fn delay(&self) -> u32 {
        self.delay
    }

    pub fn pixel_format(&self) -> PixelFormat {
        self.pixels.pixel_format()
    }

    pub fn has_alpha(&self) -> bool {
        match self.pixels {
            PixelData::RGBA8(_)
            | PixelData::RGBA16(_)
            | PixelData::RGBA32F(_)
            | PixelData::RGBA64F(_)
            | PixelData::LA8(_)
            | PixelData::LA16(_)
            | PixelData::LA32F(_)
            | PixelData::LA64F(_) => true,
            _ => false,
        }
    }

    pub fn into_rgb8(self) -> ImageFrame {
        ImageFrame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.into_rgb8(),
            delay: self.delay,
        }
    }

    pub fn as_rgb8(&self) -> Vec<u8> {
        match &self.pixels {
            PixelData::RGB8(pixels) => pixels.to_vec(),
            _ => self.pixels.clone().into_rgb8().as_bytes().to_vec(),
        }
    }

    pub fn into_rgba8(self) -> ImageFrame {
        ImageFrame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.into_rgba8(),
            delay: self.delay,
        }
    }

    pub fn as_rgba8(&self) -> Vec<u8> {
        match &self.pixels {
            PixelData::RGBA8(pixels) => pixels.to_vec(),
            _ => self.pixels.clone().into_rgba8().as_bytes().to_vec(),
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

    pub fn into_rgb8(self) -> PixelData {
        PixelData::RGB8(match self {
            PixelData::RGB8(p) => p,
            PixelData::RGBA8(p) => drop_transparency_channel(p),
            PixelData::RGB16(p) => {
                let mut dst = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut dst);
                dst
            }
            PixelData::RGBA16(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut scaled);
                drop_transparency_channel(scaled)
            }
            PixelData::RGB32F(p) => {
                let mut dst = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut dst);
                dst
            }
            PixelData::RGBA32F(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut scaled);
                drop_transparency_channel(scaled)
            }
            PixelData::RGB64F(p) => p.iter().map(|&v| scale_f64(v)).collect(),
            PixelData::RGBA64F(p) => drop_transparency_channel(p.iter().map(|&v| scale_f64(v)).collect()),
            PixelData::L1(p) => p.iter().flat_map(|&v| [v * 255, v * 255, v * 255]).collect(),
            PixelData::L8(p) => p.iter().flat_map(|&v| [v, v, v]).collect(),
            PixelData::L16(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut scaled);
                scaled.iter().flat_map(|&g| [g, g, g]).collect()
            }
            PixelData::LA8(p) => p.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0]]).collect(),
            PixelData::LA16(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_u16(c[0]); [g, g, g] }).collect(),
            PixelData::L32F(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut scaled);
                scaled.iter().flat_map(|&g| [g, g, g]).collect()
            }
            PixelData::LA32F(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_f32(c[0]); [g, g, g] }).collect(),
            PixelData::L64F(p) => p.iter().flat_map(|&v| { let g = scale_f64(v); [g, g, g] }).collect(),
            PixelData::LA64F(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_f64(c[0]); [g, g, g] }).collect(),
        })
    }

    pub fn into_rgba8(self) -> PixelData {
        PixelData::RGBA8(match self {
            PixelData::RGB8(p) => add_transparency_channel(p),
            PixelData::RGBA8(p) => p,
            PixelData::RGB16(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut scaled);
                add_transparency_channel(scaled)
            }
            PixelData::RGBA16(p) => {
                let mut dst = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut dst);
                dst
            }
            PixelData::RGB32F(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut scaled);
                add_transparency_channel(scaled)
            }
            PixelData::RGBA32F(p) => {
                let mut dst = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut dst);
                dst
            }
            PixelData::RGB64F(p) => add_transparency_channel(p.iter().map(|&v| scale_f64(v)).collect()),
            PixelData::RGBA64F(p) => p.iter().map(|&v| scale_f64(v)).collect(),
            PixelData::L1(p) => p.iter().flat_map(|&v| [v * 255, v * 255, v * 255, 255]).collect(),
            PixelData::L8(p) => p.iter().flat_map(|&v| [v, v, v, 255]).collect(),
            PixelData::L16(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_u16_to_u8(&p, &mut scaled);
                scaled.iter().flat_map(|&g| [g, g, g, 255]).collect()
            }
            PixelData::LA8(p) => p.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            PixelData::LA16(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_u16(c[0]); let a = scale_u16(c[1]); [g, g, g, a] }).collect(),
            PixelData::L32F(p) => {
                let mut scaled = vec![0u8; p.len()];
                channel_simd::scale_f32_to_u8(&p, &mut scaled);
                scaled.iter().flat_map(|&g| [g, g, g, 255]).collect()
            }
            PixelData::LA32F(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_f32(c[0]); let a = scale_f32(c[1]); [g, g, g, a] }).collect(),
            PixelData::L64F(p) => p.iter().flat_map(|&v| { let g = scale_f64(v); [g, g, g, 255] }).collect(),
            PixelData::LA64F(p) => p.chunks_exact(2).flat_map(|c| { let g = scale_f64(c[0]); let a = scale_f64(c[1]); [g, g, g, a] }).collect(),
        })
    }

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
    pub(crate) fn correct_pixels(&mut self, width: u32, height: u32) -> () {
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

                let default_pixel = vec![T::default(); components_per_pixel];
                while pixels.len() < expected_len {
                    pixels.extend(default_pixel.iter().cloned());
                }
            }
        }
    }
}
