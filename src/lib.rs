mod utils;
mod decoders;

use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::decoders::gif::GifDecoder;
use crate::decoders::jpeg::JpegDecoder;
use crate::decoders::netpbm::NetPbmDecoder;

pub use utils::{bitreader, writer, logger};

use std::fmt::{Debug};
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path};
use crate::utils::info_display::ImageInfo;

// TODO move these somewhere
fn drop_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    pixels.chunks(4).map(|chunk| chunk[0..3].to_vec()).flatten().collect()
}

fn add_transparency_channel(pixels: Vec<u8>) -> Vec<u8> {
    pixels.chunks(3).map(|chunk| chunk.to_vec()).flat_map(|v| Vec::from([v[0], v[1], v[2], 255])).collect()
}

fn u16_to_u8_rgb(values: Vec<u16>) -> Vec<u8> {
    let max_val = values.iter().max().unwrap_or(&0);
    let min_val = values.iter().min().unwrap_or(&0);

    if max_val == min_val {
        return values.iter().map(|_| (*min_val >> 8) as u8).collect();
    }
    
    values.iter()
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
    values.iter().map(|v| *v * 255).flat_map(|v| Vec::from([v, v, v])).collect()
}

fn l8_to_u8_rgb(values: Vec<u8>) -> Vec<u8> {
    values.iter().map(|v| *v).flat_map(|v| Vec::from([v, v, v])).collect()
}

fn l16_to_u8_rgb(values: Vec<u16>) -> Vec<u8> {
    let max_val = values.iter().max().unwrap_or(&0);
    let min_val = values.iter().min().unwrap_or(&0);

    if max_val == min_val {
        let gray_value = (*min_val >> 8) as u8;
        return values.iter()
            .flat_map(|_| [gray_value, gray_value, gray_value])
            .collect();
    }

    values.iter()
        .flat_map(|&p| {
            let gray = (255.0 * (p - min_val) as f32 / (max_val - min_val) as f32) as u8;
            [gray, gray, gray]
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    JpegLs,
    Gif,
    NetPbmP1,
    NetPbmP2,
    NetPbmP3,
    NetPbmP4,
    NetPbmP5,
    NetPbmP6,
    NetPbmP7,
    Unknown,
}

#[derive(Debug, Clone)]
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
}

pub enum Decoders<R: Read + Seek> {
    Jpeg(JpegDecoder<R>),
    JpegLs(JpegLsDecoder<R>),
    Gif(GifDecoder<R>),
    Netpbm(NetPbmDecoder<R>),
    Unknown,
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
            PixelData::LA8(pixels) => PixelData::RGBA8(l8_to_u8_rgb(pixels)),
            PixelData::L16(pixels) => PixelData::RGBA8(add_transparency_channel(l16_to_u8_rgb(pixels))),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            PixelData::RGB8(pixels) => pixels,
            PixelData::RGBA8(pixels) => pixels,
            _ => panic!("as_bytes() should only be called on RGB8 or RGBA8 pixel data"),
        }
    }
}

#[derive(Debug)]
pub struct ImageFrame {
    width: u32,
    height: u32,
    pixels: PixelData,
    delay: u32,
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

    pub fn into_rgb8(self) -> ImageFrame {
        ImageFrame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.into_rgb8(),
            delay: self.delay,
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
}

#[derive(Debug)]
pub struct Image {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    pub frames: Vec<ImageFrame>,
}

pub struct Vexel<R: Read + Seek> {
    decoder: Decoders<R>,
    format: ImageFormat,
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

    pub fn from_frame(frame: ImageFrame) -> Image {
        Image {
            width: frame.width(),
            height: frame.height(),
            pixel_format: frame.pixel_format(),
            frames: Vec::from([frame]),
        }
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

    /// Converts the image to RGB8 format, consuming the original image.
    ///
    /// This method converts all frames to RGB8 format, while `as_rgb8` returns
    /// vector of the first frame's pixels, converted to RGB8 format without
    /// modifying the original image
    pub fn into_rgb8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgb8()).collect();

        Image::new(self.width, self.height, PixelFormat::RGB8, new_frames)
    }

    /// Returns the first frame's pixels as a vector of RGB8 bytes
    pub fn as_rgb8(&self) -> Vec<u8> {
        self.pixels().into_rgb8().as_bytes().to_vec()
    }
}

impl Vexel<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let file = File::open(path)?;
        Vexel::new(file)
    }

    pub fn write_bmp<P: AsRef<Path>>(path: P, width: u32, height: u32, pixels: &[u8]) -> Result<(), Error> {
        let path = match path.as_ref().to_str() {
            Some(path) => path,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid path")),
        };

        writer::write_bmp(path, width, height, &pixels.to_vec())
    }

    pub fn write_ppm<P: AsRef<Path>>(path: P, width: u32, height: u32, pixels: &[u8]) -> Result<(), Error> {
        let path = match path.as_ref().to_str() {
            Some(path) => path,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid path")),
        };

        writer::write_ppm(path, width, height, &pixels.to_vec())
    }
}

impl<R: Read + Seek> Vexel<R> {
    pub fn new(mut reader: R) -> Result<Vexel<R>, Error> {
        let format = Vexel::try_guess_format(&mut reader)?;

        let decoder = match format {
            ImageFormat::Jpeg => Decoders::Jpeg(JpegDecoder::new(reader)),
            ImageFormat::JpegLs => Decoders::JpegLs(JpegLsDecoder::new(reader)),
            ImageFormat::Gif => Decoders::Gif(GifDecoder::new(reader)),
            ImageFormat::NetPbmP1 |
            ImageFormat::NetPbmP2 |
            ImageFormat::NetPbmP3 |
            ImageFormat::NetPbmP4 |
            ImageFormat::NetPbmP5 |
            ImageFormat::NetPbmP6 |
            ImageFormat::NetPbmP7 => Decoders::Netpbm(NetPbmDecoder::new(reader)),
            ImageFormat::Unknown => Decoders::Unknown,
        };

        Ok(Vexel {
            decoder,
            format,
        })
    }

    pub fn decode(&mut self) -> Result<Image, Error> {
        match &mut self.decoder {
            Decoders::Jpeg(jpeg_decoder) => {
                let image = jpeg_decoder.decode()?;
                
                Ok(image)
            }

            Decoders::JpegLs(jpeg_ls_decoder) => {
                let pixels = jpeg_ls_decoder.decode()?;
                let frames = Vec::from(
                    [
                        ImageFrame::new(
                            jpeg_ls_decoder.width(),
                            jpeg_ls_decoder.height(),
                            PixelData::RGB8(pixels),
                            0,
                        )
                    ]
                );

                Ok(
                    Image::new(
                        jpeg_ls_decoder.width(),
                        jpeg_ls_decoder.height(),
                        PixelFormat::RGB8,
                        frames,
                    )
                )
            }

            Decoders::Gif(gif_decoder) => {
                let image = gif_decoder.decode()?;

                Ok(image)
            }

            Decoders::Netpbm(netpbm_decoder) => {
                let image = netpbm_decoder.decode()?;

                Ok(image)
            }

            Decoders::Unknown => Err(Error::new(ErrorKind::InvalidData, "Unknown image format")),
        }
    }

    pub fn get_format(&self) -> ImageFormat {
        self.format.clone()
    }
    
    pub fn get_image_info(&mut self) -> ImageInfo {
        match &mut self.decoder {
            Decoders::Jpeg(jpeg_decoder) => {
                let image_data = jpeg_decoder.get_data();

                ImageInfo::Jpeg(image_data)
            }
            _ => unimplemented!(),
        }
    }

    fn try_guess_format(reader: &mut R) -> Result<ImageFormat, Error> {
        let mut header = [0u8; 12];
        reader.read_exact(&mut header)?;
        reader.seek(SeekFrom::Start(0))?;

        // JPEG-LS
        if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
            if header.windows(4).any(|window| window == [0xFF, 0xF7, 0x00, 0x0B]) {
                return Ok(ImageFormat::JpegLs);
            }
        }

        // JPEG
        if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Ok(ImageFormat::Jpeg);
        }

        // GIF87a and GIF89a
        if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }

        // Netpbm
        if header.starts_with(b"P") {
            match header[1] {
                b'1' => return Ok(ImageFormat::NetPbmP1),
                b'2' => return Ok(ImageFormat::NetPbmP2),
                b'3' => return Ok(ImageFormat::NetPbmP3),
                b'4' => return Ok(ImageFormat::NetPbmP4),
                b'5' => return Ok(ImageFormat::NetPbmP5),
                b'6' => return Ok(ImageFormat::NetPbmP6),
                b'7' => return Ok(ImageFormat::NetPbmP7),
                _ => {}
            }
        }

        Ok(ImageFormat::Unknown)
    }
}
