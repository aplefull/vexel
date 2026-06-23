use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
pub enum NetpbmFormat {
    P1, // ASCII bitmap
    P2, // ASCII graymap
    P3, // ASCII pixmap
    P4, // Binary bitmap
    P5, // Binary graymap
    P6, // Binary pixmap
    P7, // PAM
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Tsify)]
pub enum TupleType {
    BlackAndWhite,
    Grayscale,
    RGB,
    BlackAndWhiteAlpha,
    GrayscaleAlpha,
    RGBAlpha,
    CMYK,
    CMYKAlpha,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NetpbmHeaderData {
    pub format: NetpbmFormat,
    pub width: u32,
    pub height: u32,
    pub max_value: u32,
    pub depth: Option<u8>,
    pub tuple_type: Option<TupleType>,
    pub tuple_type_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NetpbmPixelDataInfo {
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum NetpbmSectionData {
    Comment(String),
    Header(NetpbmHeaderData),
    PixelData(NetpbmPixelDataInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NetpbmSectionInfo {
    pub start_offset: u64,
    pub data: NetpbmSectionData,
}
