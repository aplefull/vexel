mod decoders;
mod utils;

use crate::decoders::bmp::BmpDecoder;
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

pub(crate) use utils::bitreader;
pub use utils::error::{VexelError, VexelResult};
pub use utils::image::Image;
pub use utils::image::ImageFormat;
pub use utils::image::ImageFrame;
pub use utils::image::PixelData;
pub use utils::image::PixelFormat;
pub use utils::info::ImageInfo;
pub use utils::logger::{LogLevel, set_log_level};

use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

pub use wasm_bindgen_rayon::init_thread_pool;

macro_rules! impl_decode {
    ($decoder:expr) => {
        $decoder.decode()
    };
}

pub enum Decoders<R: Read + Seek> {
    Jpeg(JpegDecoder<R>),
    JpegLs(JpegLsDecoder<R>),
    Png(PngDecoder<R>),
    Gif(GifDecoder<R>),
    Bmp(BmpDecoder<R>),
    Netpbm(NetPbmDecoder<R>),
    Hdr(HdrDecoder<R>),
    Tiff(TiffDecoder<R>),
    Tga(TgaDecoder<R>),
    Jbig1(Jbig1Decoder<R>),
    Ico(IcoDecoder<R>),
    Unknown,
}

pub struct Vexel<R: Read + Seek> {
    decoder: Decoders<R>,
    format: ImageFormat,
}

impl Vexel<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> VexelResult<Vexel<BufReader<File>>> {
        let file = File::open(path)?;
        Vexel::new(BufReader::with_capacity(256 * 1024, file))
    }
}

impl<R: Read + Seek + Sync> Vexel<R> {
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
            ImageFormat::Tga => Decoders::Tga(TgaDecoder::new(reader)),
            ImageFormat::Jbig1 => Decoders::Jbig1(Jbig1Decoder::new(reader)),
            ImageFormat::Ico | ImageFormat::Cur => Decoders::Ico(IcoDecoder::new(reader)),
            ImageFormat::Unknown => Decoders::Unknown,
        };

        Ok(Vexel { decoder, format })
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match &mut self.decoder {
            Decoders::Jpeg(decoder) => impl_decode!(decoder),
            Decoders::JpegLs(decoder) => impl_decode!(decoder),
            Decoders::Png(decoder) => impl_decode!(decoder),
            Decoders::Gif(decoder) => impl_decode!(decoder),
            Decoders::Netpbm(decoder) => impl_decode!(decoder),
            Decoders::Bmp(decoder) => impl_decode!(decoder),
            Decoders::Hdr(decoder) => impl_decode!(decoder),
            Decoders::Tiff(decoder) => impl_decode!(decoder),
            Decoders::Tga(decoder) => impl_decode!(decoder),
            Decoders::Jbig1(decoder) => impl_decode!(decoder),
            Decoders::Ico(decoder) => impl_decode!(decoder),
            Decoders::Unknown => Err(VexelError::UnsupportedFormat("Unknown format".to_string())),
        }
    }

    pub fn get_format(&self) -> ImageFormat {
        self.format.clone()
    }

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
            Decoders::Jbig1(jbig1_decoder) => {
                let image_data = jbig1_decoder.get_info();
                ImageInfo::Jbig1(image_data)
            }
            Decoders::Ico(ico_decoder) => {
                let image_data = ico_decoder.get_info();
                ImageInfo::Ico(image_data)
            }
            _ => unimplemented!(),
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

        // TIFF
        if (header.starts_with(b"II") || header.starts_with(b"MM"))
            && ((header[2] == 42 && header[3] == 0) || (header[2] == 0 && header[3] == 42))
        {
            return Ok(ImageFormat::Tiff);
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

        if valid_image_type && valid_color_map && valid_depth && valid_descriptor && palette_consistent && valid_palette_bpp {
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
            && xd > 0 && xd <= 65535
            && yd > 0 && yd <= 65535
            && l0 > 0 && l0 <= 65535
        {
            return Ok(ImageFormat::Jbig1);
        }

        // If all else fails, let's try harder and pray that we get the right format
        Vexel::try_guess_format_harder(reader)
    }

    fn try_guess_format_harder(reader: &mut R) -> VexelResult<ImageFormat> {
        const HEADER_SIZE: usize = 48;
        const FOOTER_SIZE: usize = 12;
        let mut header = [0u8; HEADER_SIZE];
        let mut read_pos = 0;

        while read_pos < HEADER_SIZE {
            match reader.read(&mut header[read_pos..]) {
                Ok(0) | Err(_) => {
                    for i in read_pos..HEADER_SIZE {
                        header[i] = 0;
                    }
                }
                Ok(n) => read_pos += n,
            }
        }

        let mut footer = [0u8; FOOTER_SIZE];
        reader.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;
        reader.read_exact(&mut footer)?;

        reader.seek(SeekFrom::Start(0))?;

        // PNG
        let header_str = String::from_utf8_lossy(&header).to_lowercase();
        let chunks = ["png", "ihdr", "idat", "iend"];

        if chunks.iter().any(|chunk| header_str.contains(chunk)) {
            return Ok(ImageFormat::Png);
        }

        // JPEG
        if footer.ends_with(&[0xFF, 0xD9]) {
            return Ok(ImageFormat::Jpeg);
        }

        // GIF
        for i in 0..4 {
            if header[i..].starts_with(b"GIF87a") || header[i..].starts_with(b"GIF89a") {
                return Ok(ImageFormat::Gif);
            }
            if header[i..].starts_with(b"IF87a") || header[i..].starts_with(b"IF89a") {
                return Ok(ImageFormat::Gif);
            }
        }

        // TODO other formats

        // We tried
        Ok(ImageFormat::Unknown)
    }
}

#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsImage {
    width: u32,
    height: u32,
    image_format: ImageFormat,
    frames: Vec<JsImageFrame>,
}

#[derive(Serialize, Tsify)]
pub struct JsImageFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    delay: u32,
}

#[wasm_bindgen(js_name = getInfo)]
pub fn get_info(data: &[u8]) -> Result<ImageInfo, String> {
    let cursor = Cursor::new(data);
    let mut decoder = Vexel::new(cursor).map_err(|e| e.to_string())?;

    decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.get_info();

    Ok(info)
}

#[wasm_bindgen(js_name = decodeImage)]
pub fn decode_image(data: &[u8]) -> Result<JsImage, String> {
    let cursor = Cursor::new(data);
    let mut decoder = Vexel::new(cursor).map_err(|e| e.to_string())?;

    let image = decoder.decode().map_err(|e| e.to_string())?;

    Ok(JsImage {
        width: image.width(),
        height: image.height(),
        image_format: decoder.get_format(),
        frames: image
            .frames()
            .iter()
            .map(|frame| JsImageFrame {
                width: frame.width(),
                height: frame.height(),
                pixels: frame.as_rgba8(),
                delay: frame.delay(),
            })
            .collect(),
    })
}

#[wasm_bindgen(js_name = tryGuessFormat)]
pub fn try_guess_format(data: &[u8]) -> Result<String, String> {
    let mut cursor = Cursor::new(data);
    let format = Vexel::try_guess_format(&mut cursor).map_err(|e| e.to_string())?;

    Ok(format!("{:?}", format))
}
