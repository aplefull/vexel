use std::io::{Read, Seek};

use crate::decoders::bmp::BmpDecoder;
use crate::decoders::dicom::DicomDecoder;
use crate::decoders::dng::DngDecoder;
use crate::decoders::exr::ExrDecoder;
use crate::decoders::farbfeld::FarbfeldDecoder;
use crate::decoders::gif::GifDecoder;
use crate::decoders::hdr::HdrDecoder;
use crate::decoders::ico::IcoDecoder;
use crate::decoders::jbig1::Jbig1Decoder;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::jpeg2000::Jpeg2000Decoder;
use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::decoders::jpeg_xs::JpegXsDecoder;
use crate::decoders::netpbm::NetPbmDecoder;
use crate::decoders::pgx::PgxDecoder;
use crate::decoders::png::PngDecoder;
use crate::decoders::rpgmvp::RpgmvpDecoder;
use crate::decoders::tga::TgaDecoder;
use crate::decoders::tiff::TiffDecoder;
use crate::decoders::traits::ImageDecoder;
use crate::utils::error::VexelResult;
use crate::utils::image::{Image, ImageFrame};
use crate::utils::info::ImageInfo;
use crate::utils::limits::Limits;

macro_rules! impl_decoder {
    (
        $decoder:ident, $info:ident
        $(, pages = $pages:literal)?
        $(, thumbnails = $thumbs:literal)?
        $(, frames = $frames:literal)?
        $(,)?
    ) => {
        impl<R: Read + Seek + Sync> ImageDecoder<R> for $decoder<R> {
            fn set_limits(&mut self, limits: Limits) {
                $decoder::set_limits(self, limits);
            }

            fn decode(&mut self) -> VexelResult<Image> {
                $decoder::decode(self)
            }

            fn get_info(&mut self) -> ImageInfo {
                ImageInfo::$info($decoder::get_info(self))
            }

            $(
                fn supports_pages() -> bool {
                    $pages
                }

                fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
                    $decoder::decode_pages(self)
                }
            )?

            $(
                fn supports_thumbnails() -> bool {
                    $thumbs
                }

                fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
                    $decoder::decode_thumbnails(self)
                }
            )?

            $(
                fn supports_frames() -> bool {
                    $frames
                }

                fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
                    $decoder::decode_frames(self)
                }
            )?
        }
    };
}

impl_decoder!(JpegDecoder, Jpeg, thumbnails = true);
impl_decoder!(JpegLsDecoder, JpegLs, thumbnails = true);
impl_decoder!(PngDecoder, Png, frames = true);
impl_decoder!(BmpDecoder, Bmp);
impl_decoder!(NetPbmDecoder, Netpbm, pages = true);
impl_decoder!(HdrDecoder, Hdr);
impl_decoder!(TiffDecoder, Tiff, pages = true, thumbnails = true);
impl_decoder!(TgaDecoder, Tga, thumbnails = true);
impl_decoder!(Jbig1Decoder, Jbig1);
impl_decoder!(IcoDecoder, Ico, pages = true);
impl_decoder!(DicomDecoder, Dicom, thumbnails = true, frames = true);
impl_decoder!(ExrDecoder, Exr, pages = true);
impl_decoder!(Jpeg2000Decoder, Jpeg2000);
impl_decoder!(FarbfeldDecoder, Farbfeld);
impl_decoder!(PgxDecoder, Pgx);
impl_decoder!(JpegXsDecoder, JpegXs);

impl<R: Read + Seek + Sync> ImageDecoder<R> for RpgmvpDecoder<R> {
    fn set_limits(&mut self, limits: Limits) {
        RpgmvpDecoder::set_limits(self, limits);
    }

    fn decode(&mut self) -> VexelResult<Image> {
        RpgmvpDecoder::decode(self)
    }

    fn get_info(&mut self) -> ImageInfo {
        match RpgmvpDecoder::get_info(self) {
            Ok(info) => ImageInfo::Png(info),
            Err(_) => ImageInfo::Unknown,
        }
    }

    fn supports_frames() -> bool {
        true
    }

    fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        RpgmvpDecoder::decode_frames(self)
    }
}

impl<R: Read + Seek + Sync> ImageDecoder<R> for GifDecoder<R> {
    fn set_limits(&mut self, limits: Limits) {
        GifDecoder::set_limits(self, limits);
    }

    fn decode(&mut self) -> VexelResult<Image> {
        GifDecoder::decode(self)
    }

    fn get_info(&mut self) -> ImageInfo {
        ImageInfo::Gif(GifDecoder::get_info(self))
    }

    fn supports_frames() -> bool {
        true
    }
}

impl<R: Read + Seek + Sync> ImageDecoder<R> for DngDecoder<R> {
    fn set_limits(&mut self, limits: Limits) {
        DngDecoder::set_limits(self, limits);
    }

    fn decode(&mut self) -> VexelResult<Image> {
        DngDecoder::decode(self)
    }

    fn supports_thumbnails() -> bool {
        true
    }

    fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        DngDecoder::decode_thumbnails(self)
    }
}
