mod format;
mod summary;
mod writer;

use std::fmt;

pub(crate) use format::AnyFormatInfo;
pub use format::{FormatInfo, InfoSection, Section};
pub use summary::{ColorModel, DataExtent, ImageSummary, SampleFormat};
pub use writer::{InfoGroup, InfoNode, InfoValue, InfoWriter};

pub use crate::decoders::bmp::BmpInfo;
pub use crate::decoders::dicom::DicomInfo;
pub use crate::decoders::dng::DngInfo;
pub use crate::decoders::exr::ExrInfo;
pub use crate::decoders::farbfeld::FarbfeldInfo;
pub use crate::decoders::gif::GifInfo;
pub use crate::decoders::hdr::HdrInfo;
pub use crate::decoders::ico::IcoInfo;
pub use crate::decoders::jbig1::Jbig1Info;
pub use crate::decoders::jbig2::Jbig2Info;
pub use crate::decoders::jpeg::JpegInfo;
pub use crate::decoders::jpeg2000::Jpeg2000Info;
pub use crate::decoders::jpeg_ls::JpegLsInfo;
pub use crate::decoders::jpeg_xr::JpegXrInfo;
pub use crate::decoders::jpeg_xs::JpegXsInfo;
pub use crate::decoders::lerc::LercInfo;
pub use crate::decoders::netpbm::NetpbmInfo;
pub use crate::decoders::pgx::PgxInfo;
pub use crate::decoders::png::PngInfo;
pub use crate::decoders::tga::TgaInfo;
pub use crate::decoders::ImageInfo;
pub use crate::utils::tiff::info::TiffInfo;

impl ImageInfo {
    pub fn summary(&self) -> Option<&ImageSummary> {
        self.format_info().and_then(|(info, _)| info.summary())
    }

    pub fn to_tree(&self) -> Option<InfoGroup> {
        self.format_info().map(|(info, title)| info.to_tree(title))
    }
}

impl fmt::Display for ImageInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_tree() {
            Some(tree) => write!(f, "{}", tree),
            None => write!(f, "Unknown format"),
        }
    }
}
