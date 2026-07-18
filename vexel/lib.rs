mod decoders;
mod utils;

use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use serde::Serialize;
use tsify::Tsify;
pub(crate) use utils::bitreader;
pub use utils::error::{VexelError, VexelResult};
pub use utils::icc;
pub use utils::icc::ICCProfile;
pub use utils::image::{Image, ImageFormat, ImageFrame, PixelData, PixelFormat};
pub use utils::info::ImageInfo;
pub use utils::limits::Limits;
pub(crate) use utils::logger::{log_debug, log_error, log_warn};
pub use utils::logger::{set_log_level, LogLevel};
use wasm_bindgen::prelude::wasm_bindgen;
pub use wasm_bindgen_rayon::init_thread_pool;

use crate::decoders::bmp::BmpDecoder;
use crate::decoders::dicom::DicomDecoder;
use crate::decoders::dng::DngDecoder;
use crate::decoders::gif::GifDecoder;
use crate::decoders::hdr::HdrDecoder;
use crate::decoders::ico::IcoDecoder;
use crate::decoders::jbig1::Jbig1Decoder;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::decoders::netpbm::NetPbmDecoder;
use crate::decoders::png::PngDecoder;
use crate::decoders::tga::TgaDecoder;
use crate::decoders::tiff::TiffDecoder;
use crate::utils::format_detect;

macro_rules! impl_decode {
    ($decoder:expr) => {
        $decoder.decode()
    };
}

pub(crate) enum Decoders<R: Read + Seek> {
    Jpeg(JpegDecoder<R>),
    JpegLs(JpegLsDecoder<R>),
    Png(PngDecoder<R>),
    Gif(GifDecoder<R>),
    Bmp(BmpDecoder<R>),
    Netpbm(NetPbmDecoder<R>),
    Hdr(HdrDecoder<R>),
    Tiff(TiffDecoder<R>),
    Dng(DngDecoder<R>),
    Tga(TgaDecoder<R>),
    Jbig1(Jbig1Decoder<R>),
    Ico(IcoDecoder<R>),
    Dicom(DicomDecoder<R>),
    Unknown,
}

/// The main image decoder.
///
/// Wraps any [`Read`] + [`Seek`] source, detects the image format from its
/// magic bytes, and dispatches to the appropriate format-specific decoder.
///
/// # Basic usage
///
/// ```no_run
/// use vexel::Vexel;
///
/// let mut decoder = Vexel::open("image.png")?;
/// let image = decoder.decode()?;
/// let pixels = image.to_rgba8();
/// # Ok::<(), vexel::VexelError>(())
/// ```
pub struct Vexel<R: Read + Seek> {
    decoder: Decoders<R>,
    format: ImageFormat,
    limits: Limits,
}

impl Vexel<File> {
    /// Opens an image file at `path` and returns a decoder ready for use.
    ///
    /// The file is wrapped in a [`BufReader`]. The image
    /// format is detected automatically from the file's magic bytes. Returns an
    /// error if the file cannot be opened or the format cannot be identified.
    pub fn open<P: AsRef<Path>>(path: P) -> VexelResult<Vexel<BufReader<File>>> {
        let file = File::open(path)?;
        Vexel::new(BufReader::with_capacity(256 * 1024, file))
    }
}

impl<R: Read + Seek + Sync> Vexel<R> {
    /// Creates a new decoder from any [`Read`] + [`Seek`] source.
    ///
    /// The image format is detected automatically from the first bytes of the
    /// reader. Use [`open`](Vexel::open) when decoding a file on disk; use this
    /// constructor when working with an in-memory buffer or a custom reader.
    /// Returns an error if the format cannot be identified.
    pub fn new(mut reader: R) -> VexelResult<Vexel<R>> {
        let format = Vexel::try_guess_format(&mut reader)?;

        let decoder = match format {
            ImageFormat::Jpeg => Decoders::Jpeg(JpegDecoder::new(reader)),
            ImageFormat::JpegLs => Decoders::JpegLs(JpegLsDecoder::new(reader)),
            ImageFormat::Gif => Decoders::Gif(GifDecoder::new(reader)),
            ImageFormat::NetPbmP1
            | ImageFormat::NetPbmP2
            | ImageFormat::NetPbmP3
            | ImageFormat::NetPbmP4
            | ImageFormat::NetPbmP5
            | ImageFormat::NetPbmP6
            | ImageFormat::NetPbmP7 => Decoders::Netpbm(NetPbmDecoder::new(reader)),
            ImageFormat::Bmp => Decoders::Bmp(BmpDecoder::new(reader)),
            ImageFormat::Png => Decoders::Png(PngDecoder::new(reader)),
            ImageFormat::Hdr => Decoders::Hdr(HdrDecoder::new(reader)),
            ImageFormat::Tiff => Decoders::Tiff(TiffDecoder::new(reader)),
            ImageFormat::Dng => Decoders::Dng(DngDecoder::new(reader)),
            ImageFormat::Tga => Decoders::Tga(TgaDecoder::new(reader)),
            ImageFormat::Jbig1 => Decoders::Jbig1(Jbig1Decoder::new(reader)),
            ImageFormat::Ico | ImageFormat::Cur => Decoders::Ico(IcoDecoder::new(reader)),
            ImageFormat::Dicom => Decoders::Dicom(DicomDecoder::new(reader)),
            ImageFormat::Unknown => Decoders::Unknown,
        };

        Ok(Vexel { decoder, format, limits: Limits::default() })
    }

    /// Sets resource limits that the decoder will enforce during decoding.
    ///
    /// Limits can cap the maximum image dimensions and the total number of bytes
    /// allocated for pixel data. Call this before [`decode`](Self::decode); limits
    /// applied after decoding has started have no effect.
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits.clone();
        match &mut self.decoder {
            Decoders::Jpeg(d) => d.set_limits(limits),
            Decoders::JpegLs(d) => d.set_limits(limits),
            Decoders::Png(d) => d.set_limits(limits),
            Decoders::Gif(d) => d.set_limits(limits),
            Decoders::Netpbm(d) => d.set_limits(limits),
            Decoders::Bmp(d) => d.set_limits(limits),
            Decoders::Hdr(d) => d.set_limits(limits),
            Decoders::Tiff(d) => d.set_limits(limits),
            Decoders::Dng(d) => d.set_limits(limits),
            Decoders::Tga(d) => d.set_limits(limits),
            Decoders::Jbig1(d) => d.set_limits(limits),
            Decoders::Ico(d) => d.set_limits(limits),
            Decoders::Dicom(d) => d.set_limits(limits),
            Decoders::Unknown => {}
        }
    }

    /// Decodes the image and returns an [`Image`] containing all frames.
    ///
    /// For multi-page formats (ICO, multi-page TIFF) only the primary page is decoded.
    /// Use [`decode_pages`](Self::decode_pages) to get every contained image.
    ///
    /// # Errors
    ///
    /// - [`VexelError::IoError`] — underlying read or seek failed
    /// - [`VexelError::UnsupportedFormat`] — format was not recognized at construction time
    /// - [`VexelError::InvalidDimensions`] — decoded width or height is zero or otherwise invalid
    /// - [`VexelError::LimitExceeded`] — a [`Limits`] constraint was breached during decoding
    /// - [`VexelError::Custom`] — a format-specific error that does not fit another variant
    /// - [`VexelError::Panic`] — the decoder panicked internally; the panic message is captured
    pub fn decode(&mut self) -> VexelResult<Image> {
        macro_rules! dispatch {
            () => {
                match &mut self.decoder {
                    Decoders::Jpeg(decoder) => impl_decode!(decoder),
                    Decoders::JpegLs(decoder) => impl_decode!(decoder),
                    Decoders::Png(decoder) => impl_decode!(decoder),
                    Decoders::Gif(decoder) => impl_decode!(decoder),
                    Decoders::Netpbm(decoder) => impl_decode!(decoder),
                    Decoders::Bmp(decoder) => impl_decode!(decoder),
                    Decoders::Hdr(decoder) => impl_decode!(decoder),
                    Decoders::Tiff(decoder) => impl_decode!(decoder),
                    Decoders::Dng(decoder) => impl_decode!(decoder),
                    Decoders::Tga(decoder) => impl_decode!(decoder),
                    Decoders::Jbig1(decoder) => impl_decode!(decoder),
                    Decoders::Ico(decoder) => impl_decode!(decoder),
                    Decoders::Dicom(decoder) => impl_decode!(decoder),
                    Decoders::Unknown => Err(VexelError::UnsupportedFormat("Unknown format".to_string())),
                }
            };
        }

        #[cfg(fuzzing)]
        {
            return dispatch!();
        }

        #[cfg(not(fuzzing))]
        {
            let result = panic::catch_unwind(AssertUnwindSafe(|| dispatch!()));

            result.unwrap_or_else(|payload| {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");

                Err(VexelError::Panic(msg.to_string()))
            })
        }
    }

    /// Decodes every page contained in the file, returning one [`Image`] per page.
    ///
    /// Multi-page formats yield one entry per contained image.
    /// Every other format yields a single entry equal to the
    /// result of [`decode`](Self::decode). Animation frames are not pages: an
    /// animated GIF is a single page whose `Image` holds all frames.
    ///
    /// # Errors
    ///
    /// Same as [`decode`](Self::decode); decoding fails only if no page could
    /// be decoded at all.
    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        macro_rules! dispatch_pages {
            () => {
                match &mut self.decoder {
                    Decoders::Ico(decoder) => decoder.decode_pages(),
                    Decoders::Tiff(decoder) => decoder.decode_pages(),
                    Decoders::Netpbm(decoder) => decoder.decode_pages(),
                    _ => unreachable!(),
                }
            };
        }

        if !matches!(self.decoder, Decoders::Ico(_) | Decoders::Tiff(_) | Decoders::Netpbm(_)) {
            return self.decode().map(|image| vec![image]);
        }

        #[cfg(fuzzing)]
        {
            return dispatch_pages!();
        }

        #[cfg(not(fuzzing))]
        {
            let result = panic::catch_unwind(AssertUnwindSafe(|| dispatch_pages!()));

            result.unwrap_or_else(|payload| {
                let msg = payload
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");

                Err(VexelError::Panic(msg.to_string()))
            })
        }
    }

    /// Decodes all embedded thumbnails and preview images found in the file.
    /// Formats without thumbnail support, and files that contain none,
    /// return an empty vector.
    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        macro_rules! dispatch_thumbnails {
            () => {
                match &mut self.decoder {
                    Decoders::Jpeg(decoder) => decoder.decode_thumbnails(),
                    Decoders::JpegLs(decoder) => decoder.decode_thumbnails(),
                    Decoders::Tiff(decoder) => decoder.decode_thumbnails(),
                    Decoders::Dng(decoder) => decoder.decode_thumbnails(),
                    Decoders::Tga(decoder) => decoder.decode_thumbnails(),
                    Decoders::Dicom(decoder) => decoder.decode_thumbnails(),
                    _ => Ok(Vec::new()),
                }
            };
        }

        #[cfg(fuzzing)]
        let result = dispatch_thumbnails!();

        #[cfg(not(fuzzing))]
        let result = panic::catch_unwind(AssertUnwindSafe(|| dispatch_thumbnails!())).unwrap_or_else(|payload| {
            let msg = payload
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");

            Err(VexelError::Panic(msg.to_string()))
        });

        result.map(|mut thumbnails| {
            thumbnails.sort_by_key(|t| t.width() as u64 * t.height() as u64);
            thumbnails
        })
    }

    /// Returns the detected image format.
    ///
    /// The format is determined during construction and does not change. If the
    /// format could not be identified, [`ImageFormat::Unknown`] is returned.
    pub fn get_format(&self) -> ImageFormat {
        self.format.clone()
    }

    /// Returns format-specific metadata collected during the last [`decode`](Self::decode) call.
    ///
    /// Metadata is populated as a side effect of decoding, so [`decode`](Self::decode) must be
    /// called first. Calling `get_info` before `decode` returns an empty or zeroed struct.
    /// The returned [`ImageInfo`] variant matches the detected format.
    pub fn get_info(&mut self) -> ImageInfo {
        match &mut self.decoder {
            Decoders::Jpeg(jpeg_decoder) => {
                let image_data = jpeg_decoder.get_info();

                ImageInfo::Jpeg(image_data)
            }
            Decoders::Png(png_decoder) => {
                let image_data = png_decoder.get_info();

                ImageInfo::Png(image_data)
            }
            Decoders::Bmp(bmp_decoder) => {
                let image_data = bmp_decoder.get_info();

                ImageInfo::Bmp(image_data)
            }
            Decoders::Gif(gif_decoder) => {
                let image_data = gif_decoder.get_info();

                ImageInfo::Gif(image_data)
            }
            Decoders::Netpbm(netpbm_decoder) => {
                let image_data = netpbm_decoder.get_info();

                ImageInfo::Netpbm(image_data)
            }
            Decoders::Hdr(hdr_decoder) => {
                let image_data = hdr_decoder.get_info();

                ImageInfo::Hdr(image_data)
            }
            Decoders::JpegLs(jpeg_ls_decoder) => {
                let image_data = jpeg_ls_decoder.get_info();
                ImageInfo::JpegLs(image_data)
            }
            Decoders::Jbig1(jbig1_decoder) => {
                let image_data = jbig1_decoder.get_info();
                ImageInfo::Jbig1(image_data)
            }
            Decoders::Ico(ico_decoder) => {
                let image_data = ico_decoder.get_info();
                ImageInfo::Ico(image_data)
            }
            Decoders::Tga(tga_decoder) => {
                let image_data = tga_decoder.get_info();
                ImageInfo::Tga(image_data)
            }
            Decoders::Tiff(tiff_decoder) => {
                let image_data = tiff_decoder.get_info();
                ImageInfo::Tiff(image_data)
            }
            Decoders::Dicom(dicom_decoder) => {
                let image_data = dicom_decoder.get_info();
                ImageInfo::Dicom(image_data)
            }
            Decoders::Dng(_) => ImageInfo::Unknown,
            Decoders::Unknown => ImageInfo::Unknown,
        }
    }

    fn try_guess_format(reader: &mut R) -> VexelResult<ImageFormat> {
        let mut header = [0u8; 32];
        let mut read_pos = 0;

        while read_pos < header.len() {
            match reader.read(&mut header[read_pos..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => read_pos += n,
            }
        }

        reader.seek(SeekFrom::Start(0))?;

        // DICOM
        let mut dicm = [0u8; 4];
        if reader.seek(SeekFrom::Start(128)).is_ok() && reader.read_exact(&mut dicm).is_ok() && &dicm == b"DICM" {
            reader.seek(SeekFrom::Start(0))?;
            return Ok(ImageFormat::Dicom);
        }
        reader.seek(SeekFrom::Start(0))?;

        // JPEG-LS
        if header.starts_with(&[0xFF, 0xD8]) {
            if header.windows(2).any(|window| window == [0xFF, 0xF7]) {
                return Ok(ImageFormat::JpegLs);
            }

            if header.len() >= 12 && header[2] == 0xFF && header[3] == 0xE8 && &header[6..12] == b"SPIFF\0" {
                return Ok(ImageFormat::JpegLs);
            }
        }

        // JPEG
        if header.starts_with(&[0xFF, 0xD8]) {
            return Ok(ImageFormat::Jpeg);
        }

        // PNG
        if header.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Ok(ImageFormat::Png);
        }

        // GIF
        if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }

        // Netpbm
        if header.starts_with(b"P") {
            match header[1] {
                b'1' => return Ok(ImageFormat::NetPbmP1),
                b'2' => return Ok(ImageFormat::NetPbmP2),
                b'3' => return Ok(ImageFormat::NetPbmP3),
                b'4' => return Ok(ImageFormat::NetPbmP4),
                b'5' => return Ok(ImageFormat::NetPbmP5),
                b'6' => return Ok(ImageFormat::NetPbmP6),
                b'7' => return Ok(ImageFormat::NetPbmP7),
                _ => {}
            }
        }

        // BMP
        match &header[0..2] {
            b"BM" | b"BA" | b"CI" | b"CP" | b"IC" | b"PT" => return Ok(ImageFormat::Bmp),
            _ => {}
        }

        if format_detect::looks_like_headerless_dib(&header) {
            return Ok(ImageFormat::Bmp);
        }

        // ICO / CUR
        let ico_count = u16::from_le_bytes([header[4], header[5]]);
        if header.starts_with(&[0x00, 0x00, 0x01, 0x00]) && ico_count > 0 {
            return Ok(ImageFormat::Ico);
        }

        if header.starts_with(&[0x00, 0x00, 0x02, 0x00]) && ico_count > 0 {
            return Ok(ImageFormat::Cur);
        }

        // HDR
        if header.starts_with(b"#?RADIANCE") {
            return Ok(ImageFormat::Hdr);
        }

        // TIFF / DNG
        if (header.starts_with(b"II") || header.starts_with(b"MM"))
            && ((header[2] == 42 && header[3] == 0) || (header[2] == 0 && header[3] == 42))
        {
            let is_le = header.starts_with(b"II");
            let first_ifd_offset = if is_le {
                u32::from_le_bytes([header[4], header[5], header[6], header[7]])
            } else {
                u32::from_be_bytes([header[4], header[5], header[6], header[7]])
            };
            let is_raw = format_detect::tiff_is_camera_raw(reader, first_ifd_offset, is_le);
            reader.seek(SeekFrom::Start(0))?;
            return if is_raw { Ok(ImageFormat::Dng) } else { Ok(ImageFormat::Tiff) };
        }

        // TODO: maybe move to try_harder
        // DICOM
        if decoders::dicom::decoder::looks_like_dicom_dataset(&header) {
            return Ok(ImageFormat::Dicom);
        }

        // TGA
        // Targa does not have a magic number, so we have to check the header manually.
        let image_type = header[2];
        let color_map_type = header[1];
        let palette_bpp = header[7];

        let valid_image_type = matches!(image_type, 0 | 1 | 2 | 3 | 9 | 10 | 11 | 32 | 33);
        let valid_color_map = matches!(color_map_type, 0 | 1);

        let pixel_depth = header[16];
        let valid_depth = matches!(pixel_depth, 1 | 4 | 8 | 15 | 16 | 24 | 32);

        let descriptor = header[17];
        let valid_descriptor = (descriptor & 0xC0) == 0;

        let is_paletted_type = matches!(image_type, 1 | 9);
        let palette_consistent = !is_paletted_type || color_map_type == 1;
        let valid_palette_bpp = color_map_type != 1 || matches!(palette_bpp, 15 | 16 | 24 | 32);

        if valid_image_type
            && valid_color_map
            && valid_depth
            && valid_descriptor
            && palette_consistent
            && valid_palette_bpp
        {
            return Ok(ImageFormat::Tga);
        }

        // JBIG1 - no magic bytes; validate the 20-byte BIH header
        // Byte 0 (DL) <= byte 1 (D), byte 2 (planes) in 1..=8, byte 3 = 0 (reserved)
        // XD, YD, L0 (big-endian u32 at offsets 4, 8, 12) must be non-zero and <= 65535
        let xd = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let yd = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
        let l0 = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
        if header[3] == 0
            && (1..=8).contains(&header[2])
            && header[0] <= header[1]
            && xd > 0
            && xd <= 65535
            && yd > 0
            && yd <= 65535
            && l0 > 0
            && l0 <= 65535
        {
            return Ok(ImageFormat::Jbig1);
        }

        // If all else fails, let's try harder and pray that we get the right format
        format_detect::try_guess_format_harder(reader)
    }
}

/// Parses a standalone ICC color profile from raw bytes.
///
/// Returns a structured representation of the profile header, tag table, and
/// all recognized tag data.
///
/// ```no_run
/// let data = std::fs::read("profile.icc")?;
/// let profile = vexel::parse_icc(&data)?;
/// println!("{}", profile.header.color_space);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_icc(data: &[u8]) -> VexelResult<ICCProfile> {
    ICCProfile::new(data)
}

/// WebAssembly entry point that mirrors the native [`Vexel`] API.
#[wasm_bindgen(js_name = Decoder)]
pub struct WasmDecoder {
    decoder: Vexel<Cursor<Vec<u8>>>,
}

#[wasm_bindgen(js_class = Decoder)]
impl WasmDecoder {
    /// Creates a new decoder from raw image bytes.
    ///
    /// The image format is detected automatically from the magic bytes in `data`.
    /// Returns an error if the format cannot be identified.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<WasmDecoder, String> {
        let cursor = Cursor::new(data.to_vec());
        let decoder = Vexel::new(cursor).map_err(|e| e.to_string())?;
        Ok(WasmDecoder { decoder })
    }

    /// Sets resource limits that the decoder will enforce during decoding.
    ///
    /// `max_image_width` and `max_image_height` are in pixels; `max_alloc` is in bytes.
    /// Pass `null` for any field to leave that constraint unconstrained. The default
    /// when `setLimits` is not called caps `max_alloc` at 1 GiB with no dimension limits.
    #[wasm_bindgen(js_name = setLimits)]
    pub fn set_limits(&mut self, max_image_width: Option<u32>, max_image_height: Option<u32>, max_alloc: Option<f64>) {
        self.decoder.set_limits(Limits {
            max_image_width,
            max_image_height,
            max_alloc: max_alloc.map(|v| v as u64),
            max_pixel_count: None,
        });
    }

    /// Decodes the image and returns a [`JsImage`] with all frames as RGBA8.
    ///
    /// Pixel data for each frame is a flat byte vector of length `width * height * 4`.
    #[wasm_bindgen(js_name = decode)]
    pub fn decode(&mut self) -> Result<JsImage, String> {
        let image = self.decoder.decode().map_err(|e| e.to_string())?;

        Ok(JsImage {
            width: image.width(),
            height: image.height(),
            image_format: self.decoder.get_format(),
            frames: image
                .frames()
                .iter()
                .map(|frame| JsImageFrame {
                    width: frame.width(),
                    height: frame.height(),
                    pixels: frame.to_rgba8(),
                    delay: frame.delay(),
                })
                .collect(),
        })
    }

    /// Decodes every page contained in the file, returning one [`JsImage`] per page.
    ///
    /// Multi-page formats yield one entry per contained image; all other formats yield a single entry.
    #[wasm_bindgen(js_name = decodePages)]
    pub fn decode_pages(&mut self) -> Result<JsPages, String> {
        let format = self.decoder.get_format();
        let pages = self.decoder.decode_pages().map_err(|e| e.to_string())?;

        Ok(JsPages {
            pages: pages
                .into_iter()
                .map(|image| JsImage {
                    width: image.width(),
                    height: image.height(),
                    image_format: format.clone(),
                    frames: image
                        .frames()
                        .iter()
                        .map(|frame| JsImageFrame {
                            width: frame.width(),
                            height: frame.height(),
                            pixels: frame.to_rgba8(),
                            delay: frame.delay(),
                        })
                        .collect(),
                })
                .collect(),
        })
    }

    /// Decodes all embedded thumbnails and preview images found in the file.
    #[wasm_bindgen(js_name = decodeThumbnails)]
    pub fn decode_thumbnails(&mut self) -> Result<JsThumbnails, String> {
        let format = self.decoder.get_format();
        let thumbnails = self.decoder.decode_thumbnails().map_err(|e| e.to_string())?;

        Ok(JsThumbnails {
            thumbnails: thumbnails
                .into_iter()
                .map(|image| JsImage {
                    width: image.width(),
                    height: image.height(),
                    image_format: format.clone(),
                    frames: image
                        .frames()
                        .iter()
                        .map(|frame| JsImageFrame {
                            width: frame.width(),
                            height: frame.height(),
                            pixels: frame.to_rgba8(),
                            delay: frame.delay(),
                        })
                        .collect(),
                })
                .collect(),
        })
    }

    /// Returns format-specific metadata collected during the last `decode` call.
    ///
    /// `decode` must be called first; calling `getInfo` before it returns an empty struct.
    #[wasm_bindgen(js_name = getInfo)]
    pub fn get_info(&mut self) -> ImageInfo {
        self.decoder.get_info()
    }

    /// Detects the image format from `data` without decoding it.
    ///
    /// Returns the format name as a string, or `"Unknown"` if the format cannot be identified.
    #[wasm_bindgen(js_name = tryGuessFormat)]
    pub fn try_guess_format(data: &[u8]) -> Result<String, String> {
        let mut cursor = Cursor::new(data);
        let format = Vexel::try_guess_format(&mut cursor).map_err(|e| e.to_string())?;

        Ok(format!("{:?}", format))
    }
}

/// WebAssembly-compatible image returned by [`WasmDecoder::decode`].
///
/// Pixels in each frame are always RGBA8, regardless of the source format.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsImage {
    pub width: u32,
    pub height: u32,
    pub image_format: ImageFormat,
    pub frames: Vec<JsImageFrame>,
}

/// A single frame within a [`JsImage`].
///
/// `pixels` is a flat RGBA8 byte vector with length `width * height * 4`.
/// `delay` is the display duration in milliseconds; `0` for non-animated images.
#[derive(Serialize, Tsify)]
pub struct JsImageFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub delay: u32,
}

/// Pages returned by [`WasmDecoder::decode_pages`], one [`JsImage`] per
/// image contained in the file, in file order.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsPages {
    pub pages: Vec<JsImage>,
}

/// Embedded thumbnails returned by [`WasmDecoder::decode_thumbnails`], sorted
/// from smallest to largest by pixel count.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsThumbnails {
    pub thumbnails: Vec<JsImage>,
}

/// Parses a standalone ICC color profile from raw bytes.
///
/// Returns a structured representation of the profile header, tag table, and
/// all recognized tag data.
#[wasm_bindgen(js_name = parseIcc)]
pub fn wasm_parse_icc(data: &[u8]) -> Result<ICCProfile, String> {
    parse_icc(data).map_err(|e| e.to_string())
}

/// Sets the minimum log level for all decoders.
///
/// Accepts `"Debug"`, `"Warning"`, `"Error"`, or `"None"` (case-sensitive).
/// Returns an error string if the value is not recognised.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn wasm_set_log_level(level: &str) -> Result<(), String> {
    let level = match level {
        "Debug" => LogLevel::Debug,
        "Warning" => LogLevel::Warning,
        "Error" => LogLevel::Error,
        "None" => LogLevel::None,
        other => return Err(format!("unknown log level: {}", other)),
    };
    set_log_level(level);
    Ok(())
}
