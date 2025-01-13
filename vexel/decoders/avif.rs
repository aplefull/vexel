use crate::utils::error::VexelResult;
use crate::utils::info::AvifInfo;
use crate::Image;
use serde::Serialize;
use std::io::{Read, Seek};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum PixelFormat {
    YUV444,
    YUV422,
    YUV420,
    Monochrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum ColorPrimaries {
    BT709,
    BT2020,
    SRGB,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifColorInfo {
    pub depth: u8,
    pub pixel_format: PixelFormat,
    pub primaries: ColorPrimaries,
    pub full_range: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifFrameInfo {
    pub duration: u32,
    pub timescale: u32,
}

pub struct AvifDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    color_info: AvifColorInfo,
    frames: Vec<AvifFrameInfo>,
    is_animated: bool,
    loop_count: u32,
    reader: R,
}

impl<R: Read + Seek> AvifDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            color_info: AvifColorInfo {
                depth: 8,
                pixel_format: PixelFormat::YUV420,
                primaries: ColorPrimaries::Unknown,
                full_range: true,
            },
            frames: Vec::new(),
            is_animated: false,
            loop_count: 0,
            reader,
        }
    }

    pub fn get_info(&self) -> AvifInfo {
        AvifInfo {
            width: self.width,
            height: self.height,
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        Ok(Image::default())
    }
}
