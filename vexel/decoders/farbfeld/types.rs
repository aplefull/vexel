use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FarbfeldHeaderData {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FarbfeldPixelDataInfo {
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum FarbfeldSectionData {
    Header(FarbfeldHeaderData),
    PixelData(FarbfeldPixelDataInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FarbfeldSectionInfo {
    pub start_offset: u64,
    pub data: FarbfeldSectionData,
}
