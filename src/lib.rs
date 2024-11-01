mod utils;
mod decoders;

use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::decoders::gif::GifDecoder;
use crate::decoders::jpeg::JpegDecoder;

pub use utils::{bitreader, writer};

use std::fmt::{Debug};
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path};

#[derive(Debug, Clone, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    JpegLs,
    Gif,
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
    LA8,
}

#[derive(Debug)]
pub enum Decoders<R: Read + Seek> {
    Jpeg(JpegDecoder<R>),
    JpegLs(JpegLsDecoder<R>),
    Gif(GifDecoder<R>),
    Unknown,
}

#[derive(Debug)]
pub struct ImageFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    delay: u32,
}

impl ImageFrame {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>, delay: u32) -> ImageFrame {
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

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn delay(&self) -> u32 {
        self.delay
    }

    // TODO this is temporary, until pixels are not generic
    // when pixels become generic, it'll be possible to convert correctly, knowing 
    // the actual pixel format
    pub fn into_rgb8(self) -> ImageFrame {
        let pixels = self.pixels.chunks_exact(4).map(|pixel| pixel[0..3].to_vec()).flatten().collect();

        ImageFrame {
            width: self.width,
            height: self.height,
            pixels,
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
    pixels: Vec<u8>,
}

#[derive(Debug)]
pub struct Vexel<R: Read + Seek> {
    decoder: Decoders<R>,
    format: ImageFormat,
}

impl Image {
    pub fn new(width: u32, height: u32, pixel_format: PixelFormat, frames: Vec<ImageFrame>) -> Image {
        let pixels = if frames.len() > 0 {
            frames[0].pixels.clone()
        } else {
            Vec::new()
        };
        
        Image {
            width,
            height,
            pixel_format,
            frames,
            pixels,
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
    
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    
    /// Converts the image to RGB8 format, consuming the original image
    pub fn into_rgb8(mut self) -> Image {
        let new_frames = self.frames.drain(..).map(|frame| frame.into_rgb8()).collect();

        Image {
            width: self.width,
            height: self.height,
            pixel_format: PixelFormat::RGB8,
            frames: new_frames,
            pixels: self.pixels,
        }
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
                let pixels = jpeg_decoder.decode()?;
                let frames = Vec::from(
                    [
                        ImageFrame::new(
                            jpeg_decoder.width(),
                            jpeg_decoder.height(),
                            pixels,
                            0,
                        )
                    ]
                );

                Ok(
                    Image::new(
                        jpeg_decoder.width(),
                        jpeg_decoder.height(),
                        PixelFormat::RGB8,
                        frames,
                    )
                )
            }
            
            Decoders::JpegLs(jpeg_ls_decoder) => {
                let pixels = jpeg_ls_decoder.decode()?;
                let frames = Vec::from(
                    [
                        ImageFrame::new(
                            jpeg_ls_decoder.width(),
                            jpeg_ls_decoder.height(),
                            pixels,
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

            Decoders::Unknown => Err(Error::new(ErrorKind::InvalidData, "Unknown image format")),
        }
    }

    pub fn decoder(&self) -> &Decoders<R> {
        &self.decoder
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

        Ok(ImageFormat::Unknown)
    }
}
