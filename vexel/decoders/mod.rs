pub mod bmp;
pub mod dicom;
pub mod dng;
pub mod exr;
pub mod farbfeld;
pub mod gif;
pub mod hdr;
pub mod ico;
mod impls;
pub mod jbig1;
pub mod jbig2;
pub mod jpeg;
pub mod jpeg2000;
pub mod jpeg_ls;
pub mod jpeg_xr;
pub mod jpeg_xs;
pub mod lerc;
pub mod netpbm;
pub mod pgx;
pub mod png;
pub mod rpgmvp;
pub mod tga;
pub mod tiff;
pub mod traits;

macro_rules! define_decoders {
    ($($variant:ident => $decoder:ty, $info:ty, $title:literal),+ $(,)?) => {
        pub(crate) enum Decoders<R: std::io::Read + std::io::Seek> {
            $($variant($decoder),)+
            Unknown,
        }

        impl<R: std::io::Read + std::io::Seek + Sync> Decoders<R> {
            pub(crate) fn as_dyn(&mut self) -> Option<&mut dyn $crate::decoders::traits::ImageDecoder<R>> {
                match self {
                    $(Decoders::$variant(decoder) => Some(decoder),)+
                    Decoders::Unknown => None,
                }
            }

            pub(crate) fn supports_pages(&self) -> bool {
                match self {
                    $(Decoders::$variant(_) => <$decoder as $crate::decoders::traits::ImageDecoder<R>>::supports_pages(),)+
                    Decoders::Unknown => false,
                }
            }

            pub(crate) fn supports_thumbnails(&self) -> bool {
                match self {
                    $(Decoders::$variant(_) => <$decoder as $crate::decoders::traits::ImageDecoder<R>>::supports_thumbnails(),)+
                    Decoders::Unknown => false,
                }
            }

            pub(crate) fn supports_frames(&self) -> bool {
                match self {
                    $(Decoders::$variant(_) => <$decoder as $crate::decoders::traits::ImageDecoder<R>>::supports_frames(),)+
                    Decoders::Unknown => false,
                }
            }
        }

        #[derive(Debug, serde::Serialize, tsify::Tsify)]
        #[tsify(missing_as_null)]
        pub enum ImageInfo {
            $($variant($info),)+
            Unknown,
        }

        impl ImageInfo {
            pub(crate) fn format_info(&self) -> Option<(&dyn $crate::utils::info::AnyFormatInfo, &'static str)> {
                match self {
                    $(ImageInfo::$variant(info) => Some((info, $title)),)+
                    ImageInfo::Unknown => None,
                }
            }
        }
    };
}

define_decoders! {
    Jpeg => crate::decoders::jpeg::decoder::JpegDecoder<R>, crate::decoders::jpeg::JpegInfo, "JPEG Information",
    JpegLs => crate::decoders::jpeg_ls::JpegLsDecoder<R>, crate::decoders::jpeg_ls::JpegLsInfo, "JPEG-LS Information",
    Png => crate::decoders::png::PngDecoder<R>, crate::decoders::png::PngInfo, "PNG Information",
    Gif => crate::decoders::gif::GifDecoder<R>, crate::decoders::gif::GifInfo, "GIF Information",
    Bmp => crate::decoders::bmp::BmpDecoder<R>, crate::decoders::bmp::BmpInfo, "BMP Information",
    Netpbm => crate::decoders::netpbm::NetPbmDecoder<R>, crate::decoders::netpbm::NetpbmInfo, "Netpbm Information",
    Hdr => crate::decoders::hdr::HdrDecoder<R>, crate::decoders::hdr::HdrInfo, "Radiance HDR Information",
    Tiff => crate::decoders::tiff::TiffDecoder<R>, crate::utils::tiff::info::TiffInfo, "TIFF Information",
    Dng => crate::decoders::dng::DngDecoder<R>, crate::decoders::dng::DngInfo, "DNG Information",
    Tga => crate::decoders::tga::TgaDecoder<R>, crate::decoders::tga::TgaInfo, "TGA Information",
    Jbig1 => crate::decoders::jbig1::Jbig1Decoder<R>, crate::decoders::jbig1::Jbig1Info, "JBIG1 Information",
    Jbig2 => crate::decoders::jbig2::Jbig2Decoder<R>, crate::decoders::jbig2::Jbig2Info, "JBIG2 Information",
    Ico => crate::decoders::ico::IcoDecoder<R>, crate::decoders::ico::IcoInfo, "ICO/CUR Information",
    Dicom => crate::decoders::dicom::DicomDecoder<R>, crate::decoders::dicom::DicomInfo, "DICOM Information",
    Exr => crate::decoders::exr::ExrDecoder<R>, crate::decoders::exr::ExrInfo, "OpenEXR Information",
    Jpeg2000 => crate::decoders::jpeg2000::Jpeg2000Decoder<R>, crate::decoders::jpeg2000::Jpeg2000Info, "JPEG 2000 Information",
    Farbfeld => crate::decoders::farbfeld::FarbfeldDecoder<R>, crate::decoders::farbfeld::FarbfeldInfo, "Farbfeld Information",
    Rpgmvp => crate::decoders::rpgmvp::RpgmvpDecoder<R>, crate::decoders::png::PngInfo, "RPG Maker MV Information",
    Pgx => crate::decoders::pgx::PgxDecoder<R>, crate::decoders::pgx::PgxInfo, "PGX Information",
    JpegXs => crate::decoders::jpeg_xs::JpegXsDecoder<R>, crate::decoders::jpeg_xs::JpegXsInfo, "JPEG XS Information",
    JpegXr => crate::decoders::jpeg_xr::JpegXrDecoder<R>, crate::decoders::jpeg_xr::JpegXrInfo, "JPEG XR Information",
    Lerc => crate::decoders::lerc::LercDecoder<R>, crate::decoders::lerc::LercInfo, "LERC Information",
}
