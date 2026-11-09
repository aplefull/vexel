use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::{NetpbmFormat, TupleType};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type NetpbmInfo = FormatInfo<NetpbmSection>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum NetpbmEncoding {
    Plain,
    Raw,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct NetpbmHeader {
    pub image: u32,
    pub format: NetpbmFormat,
    pub magic_valid: bool,
    pub width: u32,
    pub height: u32,
    pub max_value: Option<u32>,
    pub depth: Option<u32>,
    pub tuple_type: Option<TupleType>,
    pub tuple_type_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct NetpbmPixelData {
    pub image: u32,
    pub encoding: NetpbmEncoding,
    pub samples: u64,
    pub bytes_per_sample: Option<u8>,
    pub expected_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NetpbmComment {
    pub image: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum NetpbmSection {
    Header(NetpbmHeader),
    Comment(NetpbmComment),
    PixelData(NetpbmPixelData),
}

pub fn format_name(format: &NetpbmFormat) -> &'static str {
    match format {
        NetpbmFormat::P1 => "P1 (PBM, plain)",
        NetpbmFormat::P2 => "P2 (PGM, plain)",
        NetpbmFormat::P3 => "P3 (PPM, plain)",
        NetpbmFormat::P4 => "P4 (PBM, raw)",
        NetpbmFormat::P5 => "P5 (PGM, raw)",
        NetpbmFormat::P6 => "P6 (PPM, raw)",
        NetpbmFormat::P7 => "P7 (PAM)",
    }
}

pub fn tuple_type_name(tuple_type: &TupleType) -> &'static str {
    match tuple_type {
        TupleType::BlackAndWhite => "BLACKANDWHITE",
        TupleType::Grayscale => "GRAYSCALE",
        TupleType::RGB => "RGB",
        TupleType::BlackAndWhiteAlpha => "BLACKANDWHITE_ALPHA",
        TupleType::GrayscaleAlpha => "GRAYSCALE_ALPHA",
        TupleType::RGBAlpha => "RGB_ALPHA",
        TupleType::CMYK => "CMYK",
        TupleType::CMYKAlpha => "CMYK_ALPHA",
    }
}

impl NetpbmSection {
    fn image(&self) -> u32 {
        match self {
            NetpbmSection::Header(header) => header.image,
            NetpbmSection::Comment(comment) => comment.image,
            NetpbmSection::PixelData(pixel_data) => pixel_data.image,
        }
    }
}

impl InfoSection for NetpbmSection {
    fn title(&self) -> Cow<'_, str> {
        let name = match self {
            NetpbmSection::Header(_) => "Header",
            NetpbmSection::Comment(_) => "Comment",
            NetpbmSection::PixelData(_) => "Pixel Data",
        };
        match self.image() {
            0 => Cow::Borrowed(name),
            image => Cow::Owned(format!("{} (image {})", name, image + 1)),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            NetpbmSection::Header(header) => {
                let format = if header.magic_valid {
                    format_name(&header.format).to_string()
                } else {
                    format!("{} (invalid magic, assumed)", format_name(&header.format))
                };
                out.field("Format", format);
                out.field("Width", header.width);
                out.field("Height", header.height);
                out.opt_field("Max value", header.max_value);
                out.opt_field("Depth", header.depth);
                match (&header.tuple_type, &header.tuple_type_raw) {
                    (Some(tuple_type), _) => {
                        out.field("Tuple type", tuple_type_name(tuple_type));
                    }
                    (None, Some(raw)) => {
                        out.text("Tuple type", &format!("{} (unrecognized)", raw));
                    }
                    (None, None) => {}
                }
            }
            NetpbmSection::Comment(comment) => {
                out.text("Text", &comment.text);
            }
            NetpbmSection::PixelData(pixel_data) => {
                let encoding = match pixel_data.encoding {
                    NetpbmEncoding::Plain => "Plain (ASCII)",
                    NetpbmEncoding::Raw => "Raw (binary)",
                };
                out.field("Encoding", encoding);
                out.field("Samples", pixel_data.samples);
                out.opt_field("Bytes per sample", pixel_data.bytes_per_sample);
                out.opt_field("Expected length", pixel_data.expected_length.map(|n| format!("{} bytes", n)));
            }
        }
    }
}
