use std::fmt;

use serde::Serialize;

use super::writer::InfoWriter;
use crate::utils::exif::ExifData;
use crate::utils::icc::ICCProfile;
use crate::utils::metadata::{Cicp, ContentLightLevel, IptcData, MasteringDisplay, Orientation, XmpData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[non_exhaustive]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[non_exhaustive]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[non_exhaustive]
pub struct DataExtent {
    pub expected: u64,
    pub actual: u64,
}

impl DataExtent {
    pub fn is_truncated(&self) -> bool {
        self.actual < self.expected
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(missing_as_null))]
#[non_exhaustive]
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
    pub cicp: Option<Cicp>,
    pub mastering_display: Option<MasteringDisplay>,
    pub content_light_level: Option<ContentLightLevel>,
    pub orientation: Option<Orientation>,
    pub exif: Option<ExifData>,
    pub xmp: Option<XmpData>,
    pub iptc: Option<IptcData>,
    pub data: Option<DataExtent>,
}

impl ImageSummary {
    pub(crate) fn new(width: u32, height: u32, color_model: ColorModel, channels: u16, bits_per_sample: u8) -> Self {
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
            cicp: None,
            mastering_display: None,
            content_light_level: None,
            orientation: None,
            exif: None,
            xmp: None,
            iptc: None,
            data: None,
        }
    }

    pub(crate) fn describe(&self, out: &mut InfoWriter) {
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
        out.opt_field("CICP", self.cicp);
        if let Some(display) = &self.mastering_display {
            out.field("Mastering primaries", display.primaries_text());
            out.field("Mastering luminance", display.luminance_text());
        }
        out.opt_field("Content light level", self.content_light_level);
        out.opt_field("Orientation", self.orientation);
        if let Some(exif) = &self.exif {
            let ifds: Vec<&str> = exif.ifds().map(|(name, _)| name).collect();
            out.field("Exif", format!("{} entries ({})", exif.entry_count(), ifds.join(", ")));
            for (label, value) in exif.highlights() {
                out.text(label, &value);
            }
        }
        if let Some(xmp) = &self.xmp {
            let mut size = format!("{} bytes", xmp.packet.len());
            if let Some(extended) = &xmp.extended {
                size = format!("{} + {} bytes extended", size, extended.len());
            }
            out.field("XMP", size);
            if !xmp.properties.is_empty() {
                out.group(format!("XMP ({} properties)", xmp.properties.len()), |out| xmp.describe_properties(out));
            }
        }
        if let Some(iptc) = &self.iptc {
            out.group(format!("IPTC ({} datasets)", iptc.datasets.len()), |out| {
                for dataset in &iptc.datasets {
                    out.text(&dataset.label(), &dataset.value.to_string());
                }
            });
        }
        if let Some(data) = &self.data {
            let suffix = if data.is_truncated() { " (truncated)" } else { "" };
            out.field("Image data", format!("{} of {} bytes{}", data.actual, data.expected, suffix));
        }
    }
}
