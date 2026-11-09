use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type HdrInfo = FormatInfo<HdrSection>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum HdrFormat {
    RGBE,
    XYZE,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum HdrScanlineEncoding {
    Rle,
    Flat,
}

#[derive(Debug, Clone, Default, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct HdrHeader {
    pub program: Option<String>,
    pub format: Option<String>,
    pub gamma: Option<f32>,
    pub exposure: Option<f32>,
    pub pixel_aspect_ratio: Option<f32>,
    pub color_correction: Option<[f32; 3]>,
    pub primaries: Option<[f32; 8]>,
    pub software: Option<String>,
    pub view: Option<String>,
    pub comments: Vec<String>,
    pub other_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HdrResolution {
    pub line: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct HdrPixelData {
    pub encoding: Option<HdrScanlineEncoding>,
    pub decoded_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum HdrSection {
    Header(HdrHeader),
    Resolution(HdrResolution),
    PixelData(HdrPixelData),
}

impl InfoSection for HdrSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            HdrSection::Header(_) => Cow::Borrowed("Header"),
            HdrSection::Resolution(_) => Cow::Borrowed("Resolution"),
            HdrSection::PixelData(_) => Cow::Borrowed("Pixel Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            HdrSection::Header(header) => {
                out.opt_field("Program", header.program.as_deref());
                match &header.format {
                    Some(format) => out.field("Format", format),
                    None => out.field("Format", "missing (assumed 32-bit_rle_rgbe)"),
                };
                out.opt_field("Exposure", header.exposure);
                if let Some([r, g, b]) = header.color_correction {
                    out.field("Color correction", format!("R={} G={} B={}", r, g, b));
                }
                out.opt_field("Pixel aspect ratio", header.pixel_aspect_ratio);
                if let Some(p) = header.primaries {
                    out.field("Primaries red", format!("({}, {})", p[0], p[1]));
                    out.field("Primaries green", format!("({}, {})", p[2], p[3]));
                    out.field("Primaries blue", format!("({}, {})", p[4], p[5]));
                    out.field("White point", format!("({}, {})", p[6], p[7]));
                }
                out.opt_field("Gamma (non-standard)", header.gamma);
                if let Some(software) = &header.software {
                    out.text("Software", software);
                }
                if let Some(view) = &header.view {
                    out.text("View", view);
                }
                for comment in &header.comments {
                    out.text("Comment", comment);
                }
                for line in &header.other_lines {
                    out.text("Other line", line);
                }
            }
            HdrSection::Resolution(resolution) => {
                out.text("Line", &resolution.line);
                out.field("Width", resolution.width);
                out.field("Height", resolution.height);
            }
            HdrSection::PixelData(pixel_data) => {
                let encoding = match pixel_data.encoding {
                    Some(HdrScanlineEncoding::Rle) => "Adaptive RLE",
                    Some(HdrScanlineEncoding::Flat) => "Flat (uncompressed or old-style RLE)",
                    None => "unknown (no data)",
                };
                out.field("Encoding", encoding);
                out.field("Decoded size", format!("{} bytes", pixel_data.decoded_length));
            }
        }
    }
}
