use std::io::{Read, Seek};

use crate::utils::error::VexelResult;
use crate::utils::image::{Image, ImageFrame};
use crate::utils::info::ImageInfo;
use crate::utils::limits::Limits;

pub(crate) trait ImageDecoder<R: Read + Seek + Sync> {
    fn set_limits(&mut self, limits: Limits);

    fn decode(&mut self) -> VexelResult<Image>;

    fn get_info(&mut self) -> ImageInfo;

    fn supports_pages() -> bool
    where
        Self: Sized,
    {
        false
    }

    fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        self.decode().map(|image| vec![image])
    }

    fn supports_thumbnails() -> bool
    where
        Self: Sized,
    {
        false
    }

    fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        Ok(Vec::new())
    }

    fn supports_frames() -> bool
    where
        Self: Sized,
    {
        false
    }

    fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        self.decode().map(|image| {
            let (width, height) = (image.width(), image.height());
            vec![ImageFrame::new(width, height, image.into_pixels(), 0)]
        })
    }
}
