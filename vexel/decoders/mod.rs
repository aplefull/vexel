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
pub mod jpeg;
pub mod jpeg2000;
pub mod jpeg_ls;
pub mod jpeg_xr;
pub mod jpeg_xs;
pub mod netpbm;
pub mod pgx;
pub mod png;
pub mod rpgmvp;
pub mod tga;
pub mod tiff;
pub mod traits;

macro_rules! define_decoders {
    ($($variant:ident => $decoder:ty),+ $(,)?) => {
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
    };
}

define_decoders! {
    Jpeg => crate::decoders::jpeg::decoder::JpegDecoder<R>,
    JpegLs => crate::decoders::jpeg_ls::JpegLsDecoder<R>,
    Png => crate::decoders::png::PngDecoder<R>,
    Gif => crate::decoders::gif::GifDecoder<R>,
    Bmp => crate::decoders::bmp::BmpDecoder<R>,
    Netpbm => crate::decoders::netpbm::NetPbmDecoder<R>,
    Hdr => crate::decoders::hdr::HdrDecoder<R>,
    Tiff => crate::decoders::tiff::TiffDecoder<R>,
    Dng => crate::decoders::dng::DngDecoder<R>,
    Tga => crate::decoders::tga::TgaDecoder<R>,
    Jbig1 => crate::decoders::jbig1::Jbig1Decoder<R>,
    Ico => crate::decoders::ico::IcoDecoder<R>,
    Dicom => crate::decoders::dicom::DicomDecoder<R>,
    Exr => crate::decoders::exr::ExrDecoder<R>,
    Jpeg2000 => crate::decoders::jpeg2000::Jpeg2000Decoder<R>,
    Farbfeld => crate::decoders::farbfeld::FarbfeldDecoder<R>,
    Rpgmvp => crate::decoders::rpgmvp::RpgmvpDecoder<R>,
    Pgx => crate::decoders::pgx::PgxDecoder<R>,
    JpegXs => crate::decoders::jpeg_xs::JpegXsDecoder<R>,
    JpegXr => crate::decoders::jpeg_xr::JpegXrDecoder<R>,
}
