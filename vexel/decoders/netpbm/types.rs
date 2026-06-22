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
