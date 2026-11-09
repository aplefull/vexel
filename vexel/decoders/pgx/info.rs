use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type PgxInfo = FormatInfo<PgxSection>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum PgxByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PgxHeader {
    pub byte_order: PgxByteOrder,
    pub signed: bool,
    pub precision: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PgxPixelData {
    pub bytes_per_sample: u8,
    pub expected_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum PgxSection {
    Header(PgxHeader),
    PixelData(PgxPixelData),
}

impl InfoSection for PgxSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            PgxSection::Header(_) => Cow::Borrowed("Header"),
            PgxSection::PixelData(_) => Cow::Borrowed("Pixel Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            PgxSection::Header(header) => {
                let byte_order = match header.byte_order {
                    PgxByteOrder::BigEndian => "Big endian (ML)",
                    PgxByteOrder::LittleEndian => "Little endian (LM)",
                };
                out.field("Byte order", byte_order);
                out.field("Signed", if header.signed { "yes" } else { "no" });
                if (1..=31).contains(&header.precision) {
                    out.field("Precision", format!("{} bits", header.precision));
                } else {
                    out.field("Precision", format!("{} bits (invalid)", header.precision));
                }
                out.field("Width", header.width);
                out.field("Height", header.height);
            }
            PgxSection::PixelData(pixel_data) => {
                out.field("Bytes per sample", pixel_data.bytes_per_sample);
                out.field("Expected length", format!("{} bytes", pixel_data.expected_length));
            }
        }
    }
}
