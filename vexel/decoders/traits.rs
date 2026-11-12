use std::io::{Read, Seek};

use crate::utils::error::VexelResult;
use crate::utils::frames::Frames;
use crate::utils::image::Image;
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

    fn decode_frames(&mut self) -> VexelResult<Frames> {
        self.decode().map(Frames::single)
    }
}
