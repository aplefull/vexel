use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type FarbfeldInfo = FormatInfo<FarbfeldSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FarbfeldHeader {
    pub magic_valid: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FarbfeldPixelData {
    pub expected_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum FarbfeldSection {
    Header(FarbfeldHeader),
    PixelData(FarbfeldPixelData),
}

impl InfoSection for FarbfeldSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            FarbfeldSection::Header(_) => Cow::Borrowed("Header"),
            FarbfeldSection::PixelData(_) => Cow::Borrowed("Pixel Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            FarbfeldSection::Header(header) => {
                out.field("Magic", if header.magic_valid { "farbfeld" } else { "invalid" });
                out.field("Width", header.width);
                out.field("Height", header.height);
            }
            FarbfeldSection::PixelData(pixel_data) => {
                out.field("Expected length", format!("{} bytes", pixel_data.expected_length));
            }
        }
    }
}
