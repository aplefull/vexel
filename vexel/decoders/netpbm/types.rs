use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
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
