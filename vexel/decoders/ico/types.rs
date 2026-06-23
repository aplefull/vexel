use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum IcoType {
    Ico = 1,
    Cur = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum IcoImageFormat {
    Bmp,
    Png,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IconDirEntry {
    pub width: u32,
    pub height: u32,
    pub color_count: u8,
    pub planes: u16,
    pub bit_count: u16,
    pub bytes_in_res: u32,
    pub image_offset: u32,
    pub hotspot_x: u16,
    pub hotspot_y: u16,
    pub image_format: IcoImageFormat,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IcoIconDirData {
    pub ico_type: IcoType,
    pub image_count: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IcoIconDirEntryData {
    pub width: u32,
    pub height: u32,
    pub color_count: u8,
    pub planes: u16,
    pub bit_count: u16,
    pub bytes_in_res: u32,
    pub image_offset: u32,
    pub hotspot_x: u16,
    pub hotspot_y: u16,
    pub image_format: IcoImageFormat,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IcoImageDataInfo {
    pub length: u32,
    pub image_format: IcoImageFormat,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum IcoSectionData {
    IconDir(IcoIconDirData),
    IconDirEntry(IcoIconDirEntryData),
    ImageData(IcoImageDataInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IcoSectionInfo {
    pub start_offset: u64,
    pub data: IcoSectionData,
}
