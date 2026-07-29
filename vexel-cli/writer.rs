use std::borrow::Cow;
use std::ffi::c_void;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};

use jpegxl_sys::color::color_encoding::JxlColorEncoding;
use jpegxl_sys::common::types::{JxlBitDepth, JxlBitDepthType, JxlBool, JxlDataType, JxlEndianness, JxlPixelFormat};
use jpegxl_sys::encoder::encode::{
    JxlColorEncodingSetToSRGB, JxlEncoderAddImageFrame, JxlEncoderCloseInput, JxlEncoderCreate, JxlEncoderDestroy,
    JxlEncoderFrameSettingsCreate, JxlEncoderInitBasicInfo, JxlEncoderInitFrameHeader, JxlEncoderProcessOutput,
    JxlEncoderSetBasicInfo, JxlEncoderSetCodestreamLevel, JxlEncoderSetColorEncoding, JxlEncoderSetFrameBitDepth,
    JxlEncoderSetFrameHeader, JxlEncoderSetFrameLossless, JxlEncoderStatus, JxlEncoderUseContainer,
};
use jpegxl_sys::metadata::codestream_header::{JxlAnimationHeader, JxlBasicInfo};
use vexel::{Image, ImageFrame, PixelData};
use webp::{AnimEncoder, AnimFrame, WebPConfig};

fn scale_to_u16(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0) as u16
}

fn u16_be_bytes(values: &[u16], step: usize, take: usize) -> Vec<u8> {
    let mut out = vec![0u8; values.len() / step * take * 2];
    for (chunk, group) in out.chunks_exact_mut(take * 2).zip(values.chunks_exact(step)) {
        for (dst, v) in chunk.chunks_exact_mut(2).zip(&group[..take]) {
            dst.copy_from_slice(&v.to_be_bytes());
        }
    }
    out
}

fn f32_to_u16_be_bytes(values: &[f32], step: usize, take: usize) -> Vec<u8> {
    let mut out = vec![0u8; values.len() / step * take * 2];
    for (chunk, group) in out.chunks_exact_mut(take * 2).zip(values.chunks_exact(step)) {
        for (dst, &v) in chunk.chunks_exact_mut(2).zip(&group[..take]) {
            dst.copy_from_slice(&scale_to_u16(v as f64).to_be_bytes());
        }
    }
    out
}

fn f64_to_u16_be_bytes(values: &[f64], step: usize, take: usize) -> Vec<u8> {
    let mut out = vec![0u8; values.len() / step * take * 2];
    for (chunk, group) in out.chunks_exact_mut(take * 2).zip(values.chunks_exact(step)) {
        for (dst, &v) in chunk.chunks_exact_mut(2).zip(&group[..take]) {
            dst.copy_from_slice(&scale_to_u16(v).to_be_bytes());
        }
    }
    out
}

fn strip_channels(values: &[u8], step: usize, take: usize) -> Vec<u8> {
    let mut out = vec![0u8; values.len() / step * take];
    for (chunk, group) in out.chunks_exact_mut(take).zip(values.chunks_exact(step)) {
        chunk.copy_from_slice(&group[..take]);
    }
    out
}

struct PixelInfo {
    data_type: JxlDataType,
    num_channels: u32,
    bits_per_sample: u32,
    exponent_bits_per_sample: u32,
    has_alpha: bool,
    is_gray: bool,
}
pub struct Writer {}

impl Writer {
    pub fn write_jxl(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let frames: Vec<&ImageFrame> = image.frames().iter().collect();
        Writer::encode_jxl_file(output_path, image.width(), image.height(), &frames)
    }

    pub fn write_jxl_pages(output_path: &PathBuf, pages: &[Image]) -> Result<(), Error> {
        if pages.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "no pages to write"));
        }

        let mut all_frames = pages.iter().flat_map(|page| page.frames());
        let first_format = all_frames.next().map(|frame| frame.pixel_format());
        let uniform = match &first_format {
            Some(format) => all_frames.all(|frame| frame.pixel_format() == *format),
            None => return Err(Error::new(ErrorKind::InvalidData, "no frames to write")),
        };

        let converted: Vec<ImageFrame>;
        let frames: Vec<&ImageFrame> = if uniform {
            pages.iter().flat_map(|page| page.frames()).collect()
        } else {
            let any_alpha = pages.iter().flat_map(|page| page.frames()).any(|frame| frame.has_alpha());
            eprintln!(
                "Warning: pages have differing pixel formats, converting all to {}",
                if any_alpha { "RGBA8" } else { "RGB8" }
            );
            converted = pages
                .iter()
                .flat_map(|page| page.frames())
                .map(|frame| {
                    let frame = frame.clone();
                    if any_alpha {
                        frame.into_rgba8()
                    } else {
                        frame.into_rgb8()
                    }
                })
                .collect();
            converted.iter().collect()
        };

        let canvas_width = frames.iter().map(|frame| frame.width()).max().unwrap_or(0);
        let canvas_height = frames.iter().map(|frame| frame.height()).max().unwrap_or(0);

        Writer::encode_jxl_file(output_path, canvas_width, canvas_height, &frames)
    }

    fn encode_jxl_file(
        output_path: &PathBuf,
        canvas_width: u32,
        canvas_height: u32,
        frames: &[&ImageFrame],
    ) -> Result<(), Error> {
        let compressed = unsafe {
            let encoder = JxlEncoderCreate(std::ptr::null());
            if encoder.is_null() {
                return Err(Error::other("failed to create JXL encoder"));
            }

            let result = jxl_encode(encoder, canvas_width, canvas_height, frames);

            JxlEncoderDestroy(encoder);

            result
        }
        .map_err(Error::other)?;

        let mut file = File::create(output_path)?;
        file.write_all(&compressed)?;

        Ok(())
    }

    pub fn write_webp(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();

        let has_alpha = image.has_alpha();
        let mut timestamp_ms = 0i32;
        let pixel_bufs: Vec<(Cow<[u8]>, i32)> = image
            .frames()
            .iter()
            .map(|frame| {
                let delta = if frame.delay() == 0 { 1 } else { frame.delay() as i32 };
                timestamp_ms += delta;
                let pixels: Cow<[u8]> = match (has_alpha, frame.pixels()) {
                    (true, PixelData::RGBA8(p)) => Cow::Borrowed(p.as_slice()),
                    (true, other) => Cow::Owned(other.clone().into_rgba8_bytes()),
                    (false, PixelData::RGB8(p)) => Cow::Borrowed(p.as_slice()),
                    (false, other) => Cow::Owned(other.clone().into_rgb8_bytes()),
                };
                (pixels, timestamp_ms)
            })
            .collect();

        let mut config = WebPConfig::new().unwrap();
        config.lossless = 1i32;

        let mut encoder = AnimEncoder::new(width, height, &config);

        for (pixels, ts) in &pixel_bufs {
            let anim_frame = if has_alpha {
                AnimFrame::from_rgba(pixels, width, height, *ts)
            } else {
                AnimFrame::from_rgb(pixels, width, height, *ts)
            };
            encoder.add_frame(anim_frame);
        }

        let data = encoder.encode();

        let mut file = File::create(output_path)?;
        file.write_all(&data)?;

        Ok(())
    }

    pub fn write_pam(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();

        let (meta, payload): (&str, Cow<[u8]>) = match image.frames()[0].pixels() {
            PixelData::RGB8(p) => ("DEPTH 3\nMAXVAL 255\nTUPLTYPE RGB\n", Cow::Borrowed(p.as_slice())),
            PixelData::RGBA8(p) => ("DEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\n", Cow::Borrowed(p.as_slice())),
            PixelData::RGB16(p) => ("DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\n", Cow::Owned(u16_be_bytes(p, 1, 1))),
            PixelData::RGBA16(p) => ("DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\n", Cow::Owned(u16_be_bytes(p, 1, 1))),
            PixelData::RGB32F(p) => ("DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\n", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1))),
            PixelData::RGBA32F(p) => {
                ("DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\n", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1)))
            }
            PixelData::RGB64F(p) => ("DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\n", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1))),
            PixelData::RGBA64F(p) => {
                ("DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\n", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1)))
            }
            PixelData::L1(p) => ("DEPTH 1\nMAXVAL 1\nTUPLTYPE GRAYSCALE\n", Cow::Borrowed(p.as_slice())),
            PixelData::L8(p) => ("DEPTH 1\nMAXVAL 255\nTUPLTYPE GRAYSCALE\n", Cow::Borrowed(p.as_slice())),
            PixelData::L16(p) => ("DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\n", Cow::Owned(u16_be_bytes(p, 1, 1))),
            PixelData::LA8(p) => ("DEPTH 2\nMAXVAL 255\nTUPLTYPE GRAYSCALE_ALPHA\n", Cow::Borrowed(p.as_slice())),
            PixelData::LA16(p) => {
                ("DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\n", Cow::Owned(u16_be_bytes(p, 1, 1)))
            }
            PixelData::L32F(p) => {
                ("DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\n", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1)))
            }
            PixelData::LA32F(p) => {
                ("DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\n", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1)))
            }
            PixelData::L64F(p) => {
                ("DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\n", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1)))
            }
            PixelData::LA64F(p) => {
                ("DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\n", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1)))
            }
        };

        let mut file = File::create(output_path)?;
        let header = format!("P7\nWIDTH {}\nHEIGHT {}\n{}ENDHDR\n", width, height, meta);
        file.write_all(header.as_bytes())?;
        file.write_all(&payload)?;

        Ok(())
    }

    pub fn write_ppm(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();

        let (magic, maxval, payload): (&str, &str, Cow<[u8]>) = match image.frames()[0].pixels() {
            PixelData::RGB8(p) => ("P6", "255", Cow::Borrowed(p.as_slice())),
            PixelData::RGBA8(p) => ("P6", "255", Cow::Owned(strip_channels(p, 4, 3))),
            PixelData::RGB16(p) => ("P6", "65535", Cow::Owned(u16_be_bytes(p, 1, 1))),
            PixelData::RGBA16(p) => ("P6", "65535", Cow::Owned(u16_be_bytes(p, 4, 3))),
            PixelData::RGB32F(p) => ("P6", "65535", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1))),
            PixelData::RGBA32F(p) => ("P6", "65535", Cow::Owned(f32_to_u16_be_bytes(p, 4, 3))),
            PixelData::RGB64F(p) => ("P6", "65535", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1))),
            PixelData::RGBA64F(p) => ("P6", "65535", Cow::Owned(f64_to_u16_be_bytes(p, 4, 3))),
            PixelData::L1(p) => ("P5", "255", Cow::Owned(p.iter().map(|&v| if v != 0 { 255 } else { 0 }).collect())),
            PixelData::L8(p) => ("P5", "255", Cow::Borrowed(p.as_slice())),
            PixelData::L16(p) => ("P5", "65535", Cow::Owned(u16_be_bytes(p, 1, 1))),
            PixelData::LA8(p) => ("P5", "255", Cow::Owned(strip_channels(p, 2, 1))),
            PixelData::LA16(p) => ("P5", "65535", Cow::Owned(u16_be_bytes(p, 2, 1))),
            PixelData::L32F(p) => ("P5", "65535", Cow::Owned(f32_to_u16_be_bytes(p, 1, 1))),
            PixelData::LA32F(p) => ("P5", "65535", Cow::Owned(f32_to_u16_be_bytes(p, 2, 1))),
            PixelData::L64F(p) => ("P5", "65535", Cow::Owned(f64_to_u16_be_bytes(p, 1, 1))),
            PixelData::LA64F(p) => ("P5", "65535", Cow::Owned(f64_to_u16_be_bytes(p, 2, 1))),
        };

        let mut file = File::create(output_path)?;
        let header = format!("{}\n{} {}\n{}\n", magic, width, height, maxval);
        file.write_all(header.as_bytes())?;
        file.write_all(&payload)?;

        Ok(())
    }

    pub fn write_image(image: &Image, format: &str, output_path: &PathBuf) -> Result<(), Error> {
        Writer::validate_image(image)?;

        match format {
            "pam" => Writer::write_pam(output_path, image),
            "ppm" => Writer::write_ppm(output_path, image),
            "webp" => Writer::write_webp(output_path, image),
            "jxl" => Writer::write_jxl(output_path, image),
            _ => Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        }
    }

    pub fn write_frames(output_path: &PathBuf, format: &str, image: Image) -> Result<(), Error> {
        Writer::validate_image(&image)?;

        for (i, frame) in image.into_frames().into_iter().enumerate() {
            let ext = match format {
                "pam" => "pam",
                "ppm" => "ppm",
                "jxl" => "jxl",
                "webp" => "webp",
                _ => return Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
            };

            let output_dir = Path::new(output_path).parent().unwrap();
            let output_file_name = Path::new(output_path).file_stem().unwrap().to_str().unwrap();
            let output_path = output_dir.join(format!("{}_frame_{}.{}", output_file_name, i, ext));

            let frame_image = Image::from_frame(frame);

            match format {
                "pam" => Writer::write_pam(&output_path, &frame_image)?,
                "ppm" => Writer::write_ppm(&output_path, &frame_image)?,
                "jxl" => Writer::write_jxl(&output_path, &frame_image)?,
                "webp" => Writer::write_webp(&output_path, &frame_image)?,
                _ => return Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
            }
        }

        Ok(())
    }

    pub fn write_pages(pages: &[Image], format: &str, output_path: &PathBuf) -> Result<(), Error> {
        if pages.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "no pages to write"));
        }

        for page in pages {
            Writer::validate_image(page)?;
        }

        if pages.len() == 1 {
            return Writer::write_image(&pages[0], format, output_path);
        }

        match format {
            "jxl" => {
                let total_frames: usize = pages.iter().map(|page| page.frames().len()).sum();
                if total_frames > pages.len() {
                    eprintln!(
                        "Warning: JXL cannot represent page structure; writing all {} frames from {} pages as a flat sequence",
                        total_frames,
                        pages.len()
                    );
                }
                Writer::write_jxl_pages(output_path, pages)
            }
            "pam" | "ppm" | "webp" => {
                let output_dir = Path::new(output_path).parent().unwrap_or_else(|| Path::new("."));
                let output_file_name = Path::new(output_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "invalid output file name"))?;

                for (i, page) in pages.iter().enumerate() {
                    let page_path = output_dir.join(format!("{}_page_{}.{}", output_file_name, i, format));
                    Writer::write_image(page, format, &page_path)?;
                }

                Ok(())
            }
            _ => Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        }
    }

    fn validate_image(image: &Image) -> Result<(), Error> {
        use vexel::PixelFormat;

        if image.frames().is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "image has no frames"));
        }

        if image.width() == 0 || image.height() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("image has zero dimensions: {}x{}", image.width(), image.height()),
            ));
        }

        let width = image.width();
        let height = image.height();
        let expected_pixels = (width * height) as usize;

        let (channels, bytes_per_element) = match image.pixel_format() {
            PixelFormat::RGB8 => (3usize, 1usize),
            PixelFormat::RGBA8 => (4, 1),
            PixelFormat::RGB16 => (3, 2),
            PixelFormat::RGBA16 => (4, 2),
            PixelFormat::RGB32F => (3, 4),
            PixelFormat::RGBA32F => (4, 4),
            PixelFormat::RGB64F => (3, 8),
            PixelFormat::RGBA64F => (4, 8),
            PixelFormat::L1 => (1, 1),
            PixelFormat::L8 => (1, 1),
            PixelFormat::L16 => (1, 2),
            PixelFormat::LA8 => (2, 1),
            PixelFormat::LA16 => (2, 2),
            PixelFormat::L32F => (1, 4),
            PixelFormat::LA32F => (2, 4),
            PixelFormat::L64F => (1, 8),
            PixelFormat::LA64F => (2, 8),
        };

        let expected_bytes = expected_pixels * channels * bytes_per_element;
        let actual_bytes = image.frames()[0].pixels().as_bytes().len();

        if actual_bytes != expected_bytes {
            let msg = format!(
                "Invalid pixel data size for {}x{} {:?} image: expected {} bytes, got {}",
                width,
                height,
                image.pixel_format(),
                expected_bytes,
                actual_bytes
            );

            return Err(Error::new(ErrorKind::InvalidData, msg));
        }

        Ok(())
    }
}

fn pixel_data_ptr_and_len(pixel_data: &PixelData) -> (*const c_void, usize) {
    match pixel_data {
        PixelData::RGB8(d) | PixelData::RGBA8(d) => (d.as_ptr() as *const c_void, d.len()),
        PixelData::L1(d) | PixelData::L8(d) | PixelData::LA8(d) => (d.as_ptr() as *const c_void, d.len()),
        PixelData::RGB16(d) | PixelData::RGBA16(d) => (d.as_ptr() as *const c_void, d.len() * 2),
        PixelData::L16(d) | PixelData::LA16(d) => (d.as_ptr() as *const c_void, d.len() * 2),
        PixelData::RGB32F(d) | PixelData::RGBA32F(d) | PixelData::L32F(d) | PixelData::LA32F(d) => {
            (d.as_ptr() as *const c_void, d.len() * 4)
        }
        PixelData::RGB64F(d) | PixelData::RGBA64F(d) | PixelData::L64F(d) | PixelData::LA64F(d) => {
            (d.as_ptr() as *const c_void, d.len() * 8)
        }
    }
}

fn pixel_info_for(pixel_data: &PixelData) -> PixelInfo {
    match pixel_data {
        PixelData::RGB8(_) => PixelInfo {
            data_type: JxlDataType::Uint8,
            num_channels: 3,
            bits_per_sample: 8,
            exponent_bits_per_sample: 0,
            has_alpha: false,
            is_gray: false,
        },
        PixelData::RGBA8(_) => PixelInfo {
            data_type: JxlDataType::Uint8,
            num_channels: 4,
            bits_per_sample: 8,
            exponent_bits_per_sample: 0,
            has_alpha: true,
            is_gray: false,
        },
        PixelData::RGB16(_) => PixelInfo {
            data_type: JxlDataType::Uint16,
            num_channels: 3,
            bits_per_sample: 16,
            exponent_bits_per_sample: 0,
            has_alpha: false,
            is_gray: false,
        },
        PixelData::RGBA16(_) => PixelInfo {
            data_type: JxlDataType::Uint16,
            num_channels: 4,
            bits_per_sample: 16,
            exponent_bits_per_sample: 0,
            has_alpha: true,
            is_gray: false,
        },
        PixelData::RGB32F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 3,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: false,
            is_gray: false,
        },
        PixelData::RGBA32F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 4,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: true,
            is_gray: false,
        },
        PixelData::RGB64F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 3,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: false,
            is_gray: false,
        },
        PixelData::RGBA64F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 4,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: true,
            is_gray: false,
        },
        PixelData::L1(_) => PixelInfo {
            data_type: JxlDataType::Uint8,
            num_channels: 1,
            bits_per_sample: 1,
            exponent_bits_per_sample: 0,
            has_alpha: false,
            is_gray: true,
        },
        PixelData::L8(_) => PixelInfo {
            data_type: JxlDataType::Uint8,
            num_channels: 1,
            bits_per_sample: 8,
            exponent_bits_per_sample: 0,
            has_alpha: false,
            is_gray: true,
        },
        PixelData::LA8(_) => PixelInfo {
            data_type: JxlDataType::Uint8,
            num_channels: 2,
            bits_per_sample: 8,
            exponent_bits_per_sample: 0,
            has_alpha: true,
            is_gray: true,
        },
        PixelData::L16(_) => PixelInfo {
            data_type: JxlDataType::Uint16,
            num_channels: 1,
            bits_per_sample: 16,
            exponent_bits_per_sample: 0,
            has_alpha: false,
            is_gray: true,
        },
        PixelData::LA16(_) => PixelInfo {
            data_type: JxlDataType::Uint16,
            num_channels: 2,
            bits_per_sample: 16,
            exponent_bits_per_sample: 0,
            has_alpha: true,
            is_gray: true,
        },
        PixelData::L32F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 1,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: false,
            is_gray: true,
        },
        PixelData::LA32F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 2,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: true,
            is_gray: true,
        },
        PixelData::L64F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 1,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: false,
            is_gray: true,
        },
        PixelData::LA64F(_) => PixelInfo {
            data_type: JxlDataType::Float,
            num_channels: 2,
            bits_per_sample: 32,
            exponent_bits_per_sample: 8,
            has_alpha: true,
            is_gray: true,
        },
    }
}

unsafe fn jxl_encode(
    enc: *mut jpegxl_sys::encoder::encode::JxlEncoder,
    canvas_width: u32,
    canvas_height: u32,
    frames: &[&ImageFrame],
) -> Result<Vec<u8>, String> {
    let first_frame = *frames.first().ok_or("no frames to encode")?;
    let is_animated = frames.len() > 1;
    let info = pixel_info_for(first_frame.pixels());

    if is_animated {
        if JxlEncoderUseContainer(enc, JxlBool::True) != JxlEncoderStatus::Success {
            return Err("failed to enable container".to_string());
        }
        if JxlEncoderSetCodestreamLevel(enc, 10) != JxlEncoderStatus::Success {
            return Err("failed to set codestream level".to_string());
        }
    }

    let mut basic_info = MaybeUninit::<JxlBasicInfo>::uninit();
    JxlEncoderInitBasicInfo(basic_info.as_mut_ptr());
    let mut basic_info = basic_info.assume_init();
    basic_info.xsize = canvas_width;
    basic_info.ysize = canvas_height;
    basic_info.bits_per_sample = info.bits_per_sample;
    basic_info.exponent_bits_per_sample = info.exponent_bits_per_sample;
    basic_info.num_color_channels = if info.is_gray { 1 } else { 3 };

    basic_info.uses_original_profile = JxlBool::True;

    if info.has_alpha {
        basic_info.num_extra_channels = 1;
        basic_info.alpha_bits = info.bits_per_sample;
        basic_info.alpha_exponent_bits = info.exponent_bits_per_sample;
    }

    if is_animated {
        basic_info.have_animation = JxlBool::True;
        basic_info.animation = JxlAnimationHeader {
            tps_numerator: 1000,
            tps_denominator: 1,
            num_loops: 0,
            have_timecodes: JxlBool::False,
        };
    }

    if JxlEncoderSetBasicInfo(enc, &basic_info) != JxlEncoderStatus::Success {
        return Err("failed to set basic info".to_string());
    }

    let mut color_encoding = MaybeUninit::<JxlColorEncoding>::uninit();
    JxlColorEncodingSetToSRGB(color_encoding.as_mut_ptr(), if info.is_gray { JxlBool::True } else { JxlBool::False });
    let color_encoding = color_encoding.assume_init();
    if JxlEncoderSetColorEncoding(enc, &color_encoding) != JxlEncoderStatus::Success {
        return Err("failed to set color encoding".to_string());
    }

    let frame_settings = JxlEncoderFrameSettingsCreate(enc, std::ptr::null());
    if frame_settings.is_null() {
        return Err("failed to create frame settings".to_string());
    }

    if JxlEncoderSetFrameLossless(frame_settings, JxlBool::True) != JxlEncoderStatus::Success {
        return Err("failed to set lossless mode".to_string());
    }

    let bit_depth = JxlBitDepth {
        r#type: if info.exponent_bits_per_sample > 0 {
            JxlBitDepthType::FromPixelFormat
        } else {
            JxlBitDepthType::FromCodestream
        },
        bits_per_sample: info.bits_per_sample,
        exponent_bits_per_sample: info.exponent_bits_per_sample,
    };
    if JxlEncoderSetFrameBitDepth(frame_settings, &bit_depth) != JxlEncoderStatus::Success {
        return Err("failed to set frame bit depth".to_string());
    }

    let pixel_format = JxlPixelFormat {
        num_channels: info.num_channels,
        data_type: info.data_type,
        endianness: JxlEndianness::Native,
        align: 0,
    };

    for &frame in frames {
        let has_crop = frame.width() != canvas_width || frame.height() != canvas_height;

        if is_animated || has_crop {
            let mut frame_header = MaybeUninit::<jpegxl_sys::metadata::codestream_header::JxlFrameHeader>::uninit();
            JxlEncoderInitFrameHeader(frame_header.as_mut_ptr());
            let mut frame_header = frame_header.assume_init();

            if is_animated {
                frame_header.duration = if frame.delay() == 0 { 1 } else { frame.delay() };
            }

            if has_crop {
                frame_header.layer_info.have_crop = JxlBool::True;
                frame_header.layer_info.crop_x0 = 0;
                frame_header.layer_info.crop_y0 = 0;
                frame_header.layer_info.xsize = frame.width();
                frame_header.layer_info.ysize = frame.height();
            }

            if JxlEncoderSetFrameHeader(frame_settings, &frame_header) != JxlEncoderStatus::Success {
                return Err("failed to set frame header".to_string());
            }
        }

        let f32_converted: Option<Vec<f32>> = match frame.pixels() {
            PixelData::RGB64F(d) | PixelData::RGBA64F(d) | PixelData::L64F(d) | PixelData::LA64F(d) => {
                Some(d.iter().map(|&v| v as f32).collect())
            }
            _ => None,
        };
        let (ptr, len) = match &f32_converted {
            Some(v) => (v.as_ptr() as *const c_void, v.len() * 4),
            None => pixel_data_ptr_and_len(frame.pixels()),
        };

        if JxlEncoderAddImageFrame(frame_settings, &pixel_format, ptr, len) != JxlEncoderStatus::Success {
            return Err("failed to add image frame".to_string());
        }
    }

    JxlEncoderCloseInput(enc);

    let mut compressed = Vec::new();
    let mut output_buf = vec![0u8; 65536];

    loop {
        let mut next_out = output_buf.as_mut_ptr();
        let mut avail_out = output_buf.len();

        let status = JxlEncoderProcessOutput(enc, &mut next_out, &mut avail_out);
        let written = output_buf.len() - avail_out;
        compressed.extend_from_slice(&output_buf[..written]);

        match status {
            JxlEncoderStatus::Success => break,
            JxlEncoderStatus::NeedMoreOutput => continue,
            JxlEncoderStatus::Error => return Err("JXL encoder error during output processing".to_string()),
        }
    }

    Ok(compressed)
}
