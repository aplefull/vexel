use crate::utils::error::VexelResult;
use crate::utils::info::WebpInfo;
use crate::Image;
use serde::Serialize;
use std::io::{Read, Seek};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum WebpCompressionType {
    Lossy,
    Lossless,
    Extended,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpAnimationInfo {
    pub frame_count: u32,
    pub loop_count: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpFrame {
    pub width: u32,
    pub height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub duration: u32,
    pub blend: bool,
    pub dispose: bool,
}

pub struct WebpDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    compression_type: WebpCompressionType,
    has_alpha: bool,
    has_animation: bool,
    animation_info: Option<WebpAnimationInfo>,
    frames: Vec<WebpFrame>,
    reader: R,
}

impl<R: Read + Seek> WebpDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            compression_type: WebpCompressionType::Lossy,
            has_alpha: false,
            has_animation: false,
            animation_info: None,
            frames: Vec::new(),
            reader,
        }
    }

    pub fn get_info(&self) -> WebpInfo {
        WebpInfo {
            width: self.width,
            height: self.height,
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        Ok(Image::default())
    }
}
