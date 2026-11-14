use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
pub enum NetpbmFormat {
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    P7,
    PfmColor,
    PfmGray,
}

impl NetpbmFormat {
    pub fn is_pfm(&self) -> bool {
        matches!(self, NetpbmFormat::PfmColor | NetpbmFormat::PfmGray)
    }
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
