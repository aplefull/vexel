use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
pub enum HdrFormat {
    RGBE,
    XYZE,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HdrHeaderData {
    pub width: u32,
    pub height: u32,
    pub format: HdrFormat,
    pub gamma: Option<f32>,
    pub exposure: Option<f32>,
    pub pixel_aspect_ratio: Option<f32>,
    pub color_correction: Option<[f32; 3]>,
    pub primaries: Option<[f32; 8]>,
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HdrPixelDataInfo {
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum HdrSectionData {
    Header(HdrHeaderData),
    PixelData(HdrPixelDataInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HdrSectionInfo {
    pub start_offset: u64,
    pub data: HdrSectionData,
}
