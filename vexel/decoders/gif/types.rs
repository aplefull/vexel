use crate::bitreader::BitReader;
use serde::Serialize;
use std::io::{Read, Seek};
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifHeaderData {
    pub version: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub global_color_table_flag: bool,
    pub color_resolution: u8,
    pub sort_flag: bool,
    pub size_of_global_color_table: u8,
    pub background_color_index: u8,
    pub pixel_aspect_ratio: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifColorTableInfo {
    pub entry_count: u32,
    pub length: u32,
    pub entries: Vec<[u8; 3]>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifGraphicsControlData {
    pub disposal_method: DisposalMethod,
    pub user_input: bool,
    pub transparent_color_index: Option<u8>,
    pub delay: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifImageDescriptorData {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub local_color_table_flag: bool,
    pub interlace_flag: bool,
    pub sort_flag: bool,
    pub size_of_local_color_table: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifImageDataInfo {
    pub lzw_minimum_code_size: u8,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifCommentData {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifApplicationExtensionData {
    pub identifier: String,
    pub auth_code: String,
    pub loop_count: Option<u16>,
    pub buffer_size: Option<u8>,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifPlainTextData {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cell_width: u8,
    pub cell_height: u8,
    pub foreground_color: u8,
    pub background_color: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum GifSectionData {
    Header(GifHeaderData),
    GlobalColorTable(GifColorTableInfo),
    GraphicsControlExtension(GifGraphicsControlData),
    ImageDescriptor(GifImageDescriptorData),
    LocalColorTable(GifColorTableInfo),
    ImageData(GifImageDataInfo),
    CommentExtension(GifCommentData),
    ApplicationExtension(GifApplicationExtensionData),
    PlainTextExtension(GifPlainTextData),
    Trailer,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GifSectionInfo {
    pub start_offset: u64,
    pub data: GifSectionData,
}

#[derive(Debug, Clone, Serialize, Tsify)]
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
    pub delay: u16,
    pub user_input: bool,
    pub data: Vec<u8>,
}

pub struct GifDecoder<R: Read + Seek> {
    pub(super) width: u32,
    pub(super) height: u32,
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
    pub(super) comments: Vec<String>,
    pub(super) app_extensions: Vec<ApplicationExtension>,
    pub(super) plain_text_extensions: Vec<PlainTextExtension>,
    pub(super) sections: Vec<GifSectionInfo>,
    pub(super) reader: BitReader<R>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ApplicationExtension {
    pub loop_count: Option<u16>,
    pub buffer_size: Option<u8>,
    pub identifier: String,
    pub auth_code: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct GraphicsControlExtension {
    pub disposal_method: DisposalMethod,
    pub user_input: bool,
    pub transparency: bool,
    pub delay: u16,
    pub transparent_color_index: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PlainTextExtension {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cell_width: u8,
    pub cell_height: u8,
    pub foreground_color: u8,
    pub background_color: u8,
    pub text: String,
}

#[derive(Debug, Copy, Clone, Serialize, Tsify)]
pub enum DisposalMethod {
    None,
    Background,
    Previous,
}
