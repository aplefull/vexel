use crate::log_warn;
use serde::Serialize;

fn drop_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    pixels.chunks(4).map(|chunk| chunk[0..3].to_vec()).flatten().collect()
}

fn add_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    pixels
        .chunks(3)
        .map(|chunk| chunk.to_vec())
        .flat_map(|v| Vec::from([v[0], v[1], v[2], 255]))
        .collect()
}

fn u16_to_u8_rgb(values: Vec<u16>) -> Vec<u8> {
    let max_val = values.iter().max().unwrap_or(&0);
    let min_val = values.iter().min().unwrap_or(&0);

    if max_val == min_val {
        return values.iter().map(|_| (*min_val >> 8) as u8).collect();
    }

    values
        .iter()
        .map(|&p| {
            let scaled = (255.0 * (p - min_val) as f32 / (max_val - min_val) as f32) as u8;
            scaled
        })
        .collect()
}

fn f32_to_u8_rgb(values: Vec<f32>) -> Vec<u8> {
    values.iter().map(|v| ((*v).clamp(0.0, 1.0) * 255.0) as u8).collect()
}

fn l1_to_u8_rgb(values: Vec<u8>) -> Vec<u8> {
    values
        .iter()
        .map(|v| *v * 255)
        .flat_map(|v| Vec::from([v, v, v]))
        .collect()
}

fn l8_to_u8_rgb(values: Vec<u8>) -> Vec<u8> {
    values.iter().map(|v| *v).flat_map(|v| Vec::from([v, v, v])).collect()
}

fn la8_to_u8_rgba(values: Vec<u8>) -> Vec<u8> {
    values
        .chunks_exact(2)
        .map(|chunk| Vec::from([chunk[0], chunk[0], chunk[0], chunk[1]]))
        .flatten()
        .collect()
}

fn l16_to_u8_rgb(values: Vec<u16>) -> Vec<u8> {
    let max_val = values.iter().max().unwrap_or(&0);
    let min_val = values.iter().min().unwrap_or(&0);

    if max_val == min_val {
        let gray_value = (*min_val >> 8) as u8;
        return values
            .iter()
            .flat_map(|_| [gray_value, gray_value, gray_value])
            .collect();
    }

    values
        .iter()
        .flat_map(|&p| {
            let gray = (255.0 * (p - min_val) as f32 / (max_val - min_val) as f32) as u8;
            [gray, gray, gray]
        })
        .collect()
}

fn la16_to_u8_rgba(values: Vec<u16>) -> Vec<u8> {
    let lum_values: Vec<&u16> = values.iter().step_by(2).collect();

    let max_val = lum_values.iter().max().unwrap_or(&&0);
    let min_val = lum_values.iter().min().unwrap_or(&&0);

    if max_val == min_val {
        return values
            .chunks(2)
            .flat_map(|chunk| {
                let gray = (*min_val >> 8) as u8;
                let alpha = (chunk[1] >> 8) as u8;
                [gray, gray, gray, alpha]
            })
            .collect();
    }

    values
        .chunks(2)
        .flat_map(|chunk| {
            let gray = (255.0 * (chunk[0] - *min_val) as f32 / (*max_val - *min_val) as f32) as u8;
            let alpha = (chunk[1] >> 8) as u8;
            [gray, gray, gray, alpha]
        })
        .collect()
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
    L1,
    L8,
    L16,
    LA8,
    LA16,
}

#[derive(Debug)]
pub struct Image {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    frames: Vec<ImageFrame>,
}

impl Image {
    pub fn new(width: u32, height: u32, pixel_format: PixelFormat, frames: Vec<ImageFrame>) -> Image {
        Image {
            width,
            height,
            pixel_format,
            frames,
        }
    }

    pub fn default() -> Image {
        Image {
            width: 0,
            height: 0,
            pixel_format: PixelFormat::RGB8,
            frames: Vec::new(),
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

    pub fn from_pixels(width: u32, height: u32, pixels: PixelData) -> Image {
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
            PixelFormat::RGBA8 | PixelFormat::RGBA16 | PixelFormat::RGBA32F | PixelFormat::LA8 | PixelFormat::LA16 => {
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
    pub width: u32,
    pub height: u32,
    pub pixels: PixelData,
    pub delay: u32,
}

impl ImageFrame {
    pub fn new(width: u32, height: u32, pixels: PixelData, delay: u32) -> ImageFrame {
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
            | PixelData::LA8(_)
            | PixelData::LA16(_) => true,
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
        self.pixels.clone().into_rgb8().as_bytes().to_vec()
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
        self.pixels.clone().into_rgba8().as_bytes().to_vec()
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
    L1(Vec<u8>),
    L8(Vec<u8>),
    L16(Vec<u16>),
    LA8(Vec<u8>),
    LA16(Vec<u16>),
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
            PixelData::L1(_) => PixelFormat::L1,
            PixelData::L8(_) => PixelFormat::L8,
            PixelData::L16(_) => PixelFormat::L16,
            PixelData::LA8(_) => PixelFormat::LA8,
            PixelData::LA16(_) => PixelFormat::LA16,
        }
    }

    pub fn into_rgb8(self) -> PixelData {
        match self {
            PixelData::RGB8(pixels) => PixelData::RGB8(pixels),
            PixelData::RGBA8(pixels) => PixelData::RGB8(drop_transparency_channel(pixels)),
            PixelData::RGB16(pixels) => PixelData::RGB8(u16_to_u8_rgb(pixels)),
            PixelData::RGBA16(pixels) => PixelData::RGB8(drop_transparency_channel(u16_to_u8_rgb(pixels))),
            PixelData::RGB32F(pixels) => PixelData::RGB8(f32_to_u8_rgb(pixels)),
            PixelData::RGBA32F(pixels) => PixelData::RGB8(drop_transparency_channel(f32_to_u8_rgb(pixels))),
            PixelData::L1(pixels) => PixelData::RGB8(l1_to_u8_rgb(pixels)),
            PixelData::L8(pixels) => PixelData::RGB8(l8_to_u8_rgb(pixels)),
            PixelData::LA8(pixels) => PixelData::RGB8(drop_transparency_channel(l8_to_u8_rgb(pixels))),
            PixelData::L16(pixels) => PixelData::RGB8(l16_to_u8_rgb(pixels)),
            PixelData::LA16(pixels) => PixelData::RGB8(drop_transparency_channel(l16_to_u8_rgb(pixels))),
        }
    }

    pub fn into_rgba8(self) -> PixelData {
        match self {
            PixelData::RGB8(pixels) => PixelData::RGBA8(add_transparency_channel(pixels)),
            PixelData::RGBA8(pixels) => PixelData::RGBA8(pixels),
            PixelData::RGB16(pixels) => PixelData::RGBA8(add_transparency_channel(u16_to_u8_rgb(pixels))),
            PixelData::RGBA16(pixels) => PixelData::RGBA8(u16_to_u8_rgb(pixels)),
            PixelData::RGB32F(pixels) => PixelData::RGBA8(add_transparency_channel(f32_to_u8_rgb(pixels))),
            PixelData::RGBA32F(pixels) => PixelData::RGBA8(f32_to_u8_rgb(pixels)),
            PixelData::L1(pixels) => PixelData::RGBA8(add_transparency_channel(l1_to_u8_rgb(pixels))),
            PixelData::L8(pixels) => PixelData::RGBA8(add_transparency_channel(l8_to_u8_rgb(pixels))),
            PixelData::LA8(pixels) => PixelData::RGBA8(la8_to_u8_rgba(pixels)),
            PixelData::L16(pixels) => PixelData::RGBA8(add_transparency_channel(l16_to_u8_rgb(pixels))),
            PixelData::LA16(pixels) => PixelData::RGBA8(la16_to_u8_rgba(pixels)),
        }
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
            PixelData::L1(pixels) => pixels,
            PixelData::L8(pixels) => pixels,
            PixelData::L16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
            },
            PixelData::LA8(pixels) => pixels,
            PixelData::LA16(pixels) => unsafe {
                std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 2)
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
            PixelData::L1(pixels) => pixels,
            PixelData::L8(pixels) => pixels,
            PixelData::L16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
            PixelData::LA8(pixels) => pixels,
            PixelData::LA16(pixels) => unsafe {
                std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 2)
            },
        }
    }

    // Used as a last resort to correct the number of pixels in the image
    // in case something went wrong during decoding
    pub fn correct_pixels(&mut self, width: u32, height: u32) -> () {
        let components_per_pixel = match self {
            PixelData::RGB8(_) | PixelData::RGB16(_) | PixelData::RGB32F(_) => 3,
            PixelData::RGBA8(_) | PixelData::RGBA16(_) | PixelData::RGBA32F(_) => 4,
            PixelData::L1(_) | PixelData::L8(_) | PixelData::L16(_) => 1,
            PixelData::LA8(_) | PixelData::LA16(_) => 2,
        };

        let expected_len = (width * height) as usize * components_per_pixel;

        match self {
            PixelData::RGB8(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA8(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB16(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA16(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::RGB32F(pixels) => correct_vec(pixels, expected_len, 3),
            PixelData::RGBA32F(pixels) => correct_vec(pixels, expected_len, 4),
            PixelData::L1(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::L8(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::L16(pixels) => correct_vec(pixels, expected_len, 1),
            PixelData::LA8(pixels) => correct_vec(pixels, expected_len, 2),
            PixelData::LA16(pixels) => correct_vec(pixels, expected_len, 2),
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
