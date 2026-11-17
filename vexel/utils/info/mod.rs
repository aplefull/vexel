mod format;
mod summary;
mod writer;

use std::fmt;

pub(crate) use format::sealed::DescribeSection;
pub(crate) use format::{AnyFormatInfo, FormatInfo, InfoSection, Section};
use serde::Serialize;
pub use summary::{ColorModel, DataExtent, ImageSummary, SampleFormat};
pub(crate) use writer::InfoWriter;
pub use writer::{InfoGroup, InfoNode, InfoValue};

use crate::decoders::InfoKind;
use crate::utils::image::ImageFormat;

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ImageInfo {
    #[serde(skip)]
    format: ImageFormat,
    kind: InfoKind,
}

impl ImageInfo {
    pub(crate) fn new(format: ImageFormat, kind: InfoKind) -> Self {
        ImageInfo { format, kind }
    }

    pub fn format(&self) -> ImageFormat {
        self.format
    }

    pub fn summary(&self) -> Option<&ImageSummary> {
        self.kind.format_info().and_then(|(info, _)| info.summary())
    }

    pub fn to_tree(&self) -> Option<InfoGroup> {
        self.kind.format_info().map(|(info, title)| info.to_tree(title))
    }
}

#[cfg(feature = "tsify")]
impl tsify::Tsify for ImageInfo {
    type JsType = <InfoKind as tsify::Tsify>::JsType;
    const DECL: &'static str = "";
    const SERIALIZATION_CONFIG: tsify::SerializationConfig = <InfoKind as tsify::Tsify>::SERIALIZATION_CONFIG;
}

impl fmt::Display for ImageInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_tree() {
            Some(tree) => write!(f, "{}", tree),
            None => write!(f, "Unknown format"),
        }
    }
}
