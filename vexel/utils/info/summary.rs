use std::fmt;

use serde::Serialize;
use tsify::Tsify;

use super::writer::InfoWriter;
use crate::utils::icc::ICCProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum ColorModel {
    Bilevel,
    Gray,
    GrayAlpha,
    Indexed,
    Cfa,
    Rgb,
    Rgba,
    Cmyk,
    Cmyka,
    YCbCr,
    Ycck,
    Lab,
    Xyz,
    Multichannel,
    Unknown,
}

impl fmt::Display for ColorModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ColorModel::Bilevel => "Bilevel",
            ColorModel::Gray => "Grayscale",
            ColorModel::GrayAlpha => "Grayscale + alpha",
            ColorModel::Indexed => "Indexed",
            ColorModel::Cfa => "Color filter array",
            ColorModel::Rgb => "RGB",
            ColorModel::Rgba => "RGBA",
            ColorModel::Cmyk => "CMYK",
            ColorModel::Cmyka => "CMYK + alpha",
            ColorModel::YCbCr => "YCbCr",
            ColorModel::Ycck => "YCCK",
            ColorModel::Lab => "CIELAB",
            ColorModel::Xyz => "CIEXYZ",
            ColorModel::Multichannel => "Multichannel",
            ColorModel::Unknown => "Unknown",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum SampleFormat {
    Unsigned,
    Signed,
    Float,
}

impl fmt::Display for SampleFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            SampleFormat::Unsigned => "Unsigned integer",
            SampleFormat::Signed => "Signed integer",
            SampleFormat::Float => "Floating point",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub struct DataExtent {
    pub expected: u64,
    pub actual: u64,
}

impl DataExtent {
    pub fn is_truncated(&self) -> bool {
        self.actual < self.expected
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ImageSummary {
    pub width: u32,
    pub height: u32,
    pub color_model: ColorModel,
    pub channels: u16,
    pub bits_per_sample: u8,
    pub sample_format: SampleFormat,
    pub has_alpha: bool,
    pub compression: Option<String>,
    pub subsampling: Option<String>,
    pub frames: Option<u32>,
    pub loop_count: Option<u32>,
    pub duration_ms: Option<u64>,
    pub pages: Option<u32>,
    pub icc_profile: Option<ICCProfile>,
    pub data: Option<DataExtent>,
}

impl ImageSummary {
    pub fn new(width: u32, height: u32, color_model: ColorModel, channels: u16, bits_per_sample: u8) -> Self {
        ImageSummary {
            width,
            height,
            color_model,
            channels,
            bits_per_sample,
            sample_format: SampleFormat::Unsigned,
            has_alpha: false,
            compression: None,
            subsampling: None,
            frames: None,
            loop_count: None,
            duration_ms: None,
            pages: None,
            icc_profile: None,
            data: None,
        }
    }

    pub fn describe(&self, out: &mut InfoWriter) {
        out.field("Dimensions", format!("{}x{}", self.width, self.height));
        out.field("Color model", self.color_model);
        out.field("Channels", self.channels);
        out.field("Bits per sample", self.bits_per_sample);
        out.field("Sample format", self.sample_format);
        out.field("Alpha", if self.has_alpha { "yes" } else { "no" });
        out.opt_field("Compression", self.compression.as_deref());
        out.opt_field("Chroma subsampling", self.subsampling.as_deref());
        out.opt_field("Frames", self.frames);
        match self.loop_count {
            Some(0) => {
                out.field("Loops", "infinite");
            }
            Some(count) => {
                out.field("Loops", count);
            }
            None => {}
        }
        out.opt_field("Duration", self.duration_ms.map(|ms| format!("{} ms", ms)));
        out.opt_field("Pages", self.pages);
        if let Some(profile) = &self.icc_profile {
            let header = &profile.header;
            out.field("ICC profile", format!("{} {}, {} bytes", header.color_space, header.profile_class, header.size));
            if let Some(description) = profile.description() {
                out.text("ICC description", &description);
            }
        }
        if let Some(data) = &self.data {
            let suffix = if data.is_truncated() { " (truncated)" } else { "" };
            out.field("Image data", format!("{} of {} bytes{}", data.actual, data.expected, suffix));
        }
    }
}
