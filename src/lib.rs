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

#[derive(Debug)]
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
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub delay: u32,
}

#[derive(Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub frames: Vec<ImageFrame>,
}

#[derive(Debug)]
pub struct Vexel<R: Read + Seek> {
    decoder: Decoders<R>,
    format: ImageFormat,
    width: u32,
    height: u32,
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
            width: 0,
            height: 0,
        })
    }

    pub fn decode(&mut self) -> Result<Image, Error> {
        // Each decoder must return a vector of pixels and update the width and height
        match &mut self.decoder {
            Decoders::Jpeg(jpeg_decoder) => {
                let pixels = jpeg_decoder.decode()?;

                self.width = jpeg_decoder.width();
                self.height = jpeg_decoder.height();

                Ok(Image {
                    width: self.width,
                    height: self.height,
                    pixel_format: PixelFormat::RGB8,
                    frames: vec![ImageFrame {
                        width: self.width,
                        height: self.height,
                        pixels,
                        delay: 0,
                    }],
                })
            }
            Decoders::JpegLs(jpeg_ls_decoder) => {
                let pixels = jpeg_ls_decoder.decode()?;

                self.width = jpeg_ls_decoder.width();
                self.height = jpeg_ls_decoder.height();

                Ok(Image {
                    width: self.width,
                    height: self.height,
                    pixel_format: PixelFormat::RGB8,
                    frames: vec![ImageFrame {
                        width: self.width,
                        height: self.height,
                        pixels,
                        delay: 0,
                    }],
                })
            }
            Decoders::Gif(gif_decoder) => {
                let image = gif_decoder.decode()?;

                self.width = gif_decoder.width();
                self.height = gif_decoder.height();

                Ok(image)
            }
            Decoders::Unknown => Err(Error::new(ErrorKind::InvalidData, "Unknown image format")),
        }
    }

    pub fn decoder(&self) -> &Decoders<R> {
        &self.decoder
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
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
