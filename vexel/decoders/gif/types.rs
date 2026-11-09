use std::io::{Read, Seek};

use serde::Serialize;
use tsify::Tsify;

use super::info::GifInfo;
use crate::bitreader::BitReader;
use crate::utils::icc::ICCProfile;
use crate::Limits;

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct GifFrameInfo {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub local_color_table_flag: bool,
    pub interlace_flag: bool,
    pub sort_flag: bool,
    pub size_of_local_color_table: u8,
    pub local_color_table: Vec<u8>,
    pub lzw_minimum_code_size: u8,
    pub transparent_index: Option<u8>,
    pub disposal_method: DisposalMethod,
    pub delay: u32,
    pub user_input: bool,
    pub data: Vec<u8>,
}

pub struct GifDecoder<R: Read + Seek> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) limits: Limits,
    pub(super) canvas_width: u32,
    pub(super) canvas_height: u32,
    pub(super) version: String,
    pub(super) global_color_table_flag: bool,
    pub(super) color_resolution: u8,
    pub(super) sort_flag: bool,
    pub(super) size_of_global_color_table: u8,
    pub(super) background_color_index: u8,
    pub(super) pixel_aspect_ratio: u8,
    pub(super) global_color_table: Vec<u8>,
    pub(super) frames: Vec<GifFrameInfo>,
    pub(super) collected: GifInfo,
    pub(super) info: Option<GifInfo>,
    pub(super) icc_profile: Option<ICCProfile>,
    pub(super) loop_count: Option<u16>,
    pub(super) collect_frame_data: bool,
    pub(super) frame_index: u32,
    pub(super) reader: BitReader<R>,
}

#[derive(Debug, Clone)]
pub struct GraphicsControlExtension {
    pub disposal_method: DisposalMethod,
    pub user_input: bool,
    pub transparency: bool,
    pub delay: u32,
    pub transparent_color_index: u8,
}

#[derive(Debug, Copy, Clone, Serialize, Tsify)]
pub enum DisposalMethod {
    None,
    Background,
    Previous,
}
