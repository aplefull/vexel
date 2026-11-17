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
use vexel::{Frames, Image, ImageFrame, PixelFormat, SampleType};
use webp::{AnimEncoder, AnimFrame, WebPConfig};

fn be_bytes(samples: &[u16]) -> Vec<u8> {
    samples.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn netpbm_payload(image: Image) -> (u32, Vec<u8>) {
    let format = image.format();
    match format.sample_type() {
        SampleType::U8 => (255, image.into_samples::<u8>().unwrap_or_default()),
        _ => {
            let wide = image.convert(format.with_sample_type(SampleType::U16));
            (65535, be_bytes(&wide.into_samples::<u16>().unwrap_or_default()))
        }
    }
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
    pub fn write_jxl(output_path: &PathBuf, image: Image) -> Result<(), Error> {
        let (width, height) = (image.width(), image.height());
        Writer::encode_jxl_file(output_path, width, height, ImageFrame::new(image, 0), &mut std::iter::empty(), false)
    }

    pub fn write_jxl_pages(output_path: &PathBuf, pages: Vec<Image>) -> Result<(), Error> {
        if pages.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "no pages to write"));
        }

        let first_format = pages[0].format();
        let uniform = pages.iter().all(|page| page.format() == first_format);
        let target =
            if pages.iter().any(|page| page.format().has_alpha()) { PixelFormat::RGBA8 } else { PixelFormat::RGB8 };
        if !uniform {
            eprintln!("Warning: pages have differing pixel formats, converting all to {:?}", target);
        }

        let canvas_width = pages.iter().map(|page| page.width()).max().unwrap_or(0);
        let canvas_height = pages.iter().map(|page| page.height()).max().unwrap_or(0);
        let is_animated = pages.len() > 1;

        let mut frames = pages.into_iter().map(|page| {
            let page = if uniform { page } else { page.convert(target) };
            Ok(ImageFrame::new(page, 0))
        });
        let first =
            frames.next().transpose()?.ok_or_else(|| Error::new(ErrorKind::InvalidData, "no pages to write"))?;

        Writer::encode_jxl_file(output_path, canvas_width, canvas_height, first, &mut frames, is_animated)
    }

    fn encode_jxl_file(
        output_path: &PathBuf,
        canvas_width: u32,
        canvas_height: u32,
        first_frame: ImageFrame,
        rest: &mut dyn Iterator<Item = Result<ImageFrame, Error>>,
        is_animated: bool,
    ) -> Result<(), Error> {
        let compressed = unsafe {
            let encoder = JxlEncoderCreate(std::ptr::null());
            if encoder.is_null() {
                return Err(Error::other("failed to create JXL encoder"));
            }

            let result = jxl_encode(encoder, canvas_width, canvas_height, first_frame, rest, is_animated);

            JxlEncoderDestroy(encoder);

            result
        }
        .map_err(Error::other)?;

        let mut file = File::create(output_path)?;
        file.write_all(&compressed)?;

        Ok(())
    }

    fn write_jxl_animation(output_path: &PathBuf, frames: Frames) -> Result<(), Error> {
        let is_animated = frames.frame_count_hint().is_none_or(|count| count > 1);

        let mut frames = frames.map(|frame| frame.map_err(Error::other));
        let first =
            frames.next().transpose()?.ok_or_else(|| Error::new(ErrorKind::InvalidData, "no frames to write"))?;

        let (width, height) = (first.image().width(), first.image().height());

        Writer::encode_jxl_file(output_path, width, height, first, &mut frames, is_animated)
    }

    pub fn write_webp(output_path: &PathBuf, image: Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();
        let has_alpha = image.format().has_alpha();
        let pixels: Vec<u8> = if has_alpha { image.into_rgba() } else { image.into_rgb() };

        let mut config = WebPConfig::new().unwrap();
        config.lossless = 1i32;

        let mut encoder = AnimEncoder::new(width, height, &config);
        let anim_frame = if has_alpha {
            AnimFrame::from_rgba(&pixels, width, height, 1)
        } else {
            AnimFrame::from_rgb(&pixels, width, height, 1)
        };
        encoder.add_frame(anim_frame);

        let data = encoder.encode();

        let mut file = File::create(output_path)?;
        file.write_all(&data)?;

        Ok(())
    }

    fn write_webp_animation(output_path: &PathBuf, frames: Frames) -> Result<(), Error> {
        let mut config = WebPConfig::new().unwrap();
        config.lossless = 1i32;

        let mut timestamp_ms = 0i32;
        let mut canvas: Option<(u32, u32, bool)> = None;
        let mut pixel_bufs: Vec<(Vec<u8>, i32)> = Vec::new();

        for frame in frames {
            let frame = frame.map_err(Error::other)?;
            let delay = frame.delay();
            let frame = frame.into_image();
            Writer::validate_image(&frame)?;
            let (width, height, has_alpha) =
                *canvas.get_or_insert((frame.width(), frame.height(), frame.format().has_alpha()));
            if (frame.width(), frame.height()) != (width, height) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("frame is {}x{}, animation canvas is {}x{}", frame.width(), frame.height(), width, height),
                ));
            }

            let delta = if delay == 0 { 1 } else { i32::try_from(delay).unwrap_or(i32::MAX) };
            timestamp_ms = timestamp_ms.saturating_add(delta);

            let pixels: Vec<u8> = if has_alpha { frame.into_rgba() } else { frame.into_rgb() };

            pixel_bufs.push((pixels, timestamp_ms));
        }

        let Some((width, height, has_alpha)) = canvas else {
            return Err(Error::new(ErrorKind::InvalidData, "no frames to write"));
        };

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

    pub fn write_pam(output_path: &PathBuf, image: Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();
        let format = image.format();
        let tuple_type = match (format.is_gray(), format.has_alpha()) {
            (true, false) => "GRAYSCALE",
            (true, true) => "GRAYSCALE_ALPHA",
            (false, false) => "RGB",
            (false, true) => "RGB_ALPHA",
        };
        let (maxval, payload) = netpbm_payload(image);

        let mut file = File::create(output_path)?;
        let header = format!(
            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH {}\nMAXVAL {}\nTUPLTYPE {}\nENDHDR\n",
            width,
            height,
            format.channels(),
            maxval,
            tuple_type
        );
        file.write_all(header.as_bytes())?;
        file.write_all(&payload)?;

        Ok(())
    }

    pub fn write_ppm(output_path: &PathBuf, image: Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();
        let format = image.format();
        let (magic, layout) = if format.is_gray() { ("P5", PixelFormat::L8) } else { ("P6", PixelFormat::RGB8) };
        let (maxval, payload) = netpbm_payload(image.convert(layout.with_sample_type(format.sample_type())));

        let mut file = File::create(output_path)?;
        let header = format!("{}\n{} {}\n{}\n", magic, width, height, maxval);
        file.write_all(header.as_bytes())?;
        file.write_all(&payload)?;

        Ok(())
    }

    pub fn write_image(image: Image, format: &str, output_path: &PathBuf) -> Result<(), Error> {
        Writer::validate_image(&image)?;

        match format {
            "pam" => Writer::write_pam(output_path, image),
            "ppm" => Writer::write_ppm(output_path, image),
            "webp" => Writer::write_webp(output_path, image),
            "jxl" => Writer::write_jxl(output_path, image),
            _ => Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        }
    }

    pub fn write_animation(output_path: &PathBuf, format: &str, frames: Frames) -> Result<(), Error> {
        match format {
            "webp" => Writer::write_webp_animation(output_path, frames),
            "jxl" => Writer::write_jxl_animation(output_path, frames),
            "pam" | "ppm" => {
                let mut frames = frames;
                let first = frames
                    .next_frame()
                    .map_err(Error::other)?
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "no frames to write"))?;

                if frames.next_frame().map_err(Error::other)?.is_some() {
                    eprintln!("Warning: {} cannot store animation; writing the first frame only", format);
                }

                Writer::write_image(first.into_image(), format, output_path)
            }
            _ => Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        }
    }

    pub fn write_frames(output_path: &PathBuf, format: &str, frames: Frames) -> Result<(), Error> {
        let ext = match format {
            "pam" => "pam",
            "ppm" => "ppm",
            "jxl" => "jxl",
            "webp" => "webp",
            _ => return Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        };

        let output_dir = Path::new(output_path).parent().unwrap_or_else(|| Path::new("."));
        let output_file_name = Path::new(output_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "invalid output file name"))?;

        for (i, frame) in frames.enumerate() {
            let frame_image = frame.map_err(Error::other)?.into_image();
            Writer::validate_image(&frame_image)?;

            let output_path = output_dir.join(format!("{}_frame_{}.{}", output_file_name, i, ext));
            Writer::write_image(frame_image, format, &output_path)?;
        }

        Ok(())
    }

    pub fn write_pages(pages: Vec<Image>, format: &str, output_path: &PathBuf) -> Result<(), Error> {
        if pages.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "no pages to write"));
        }

        for page in &pages {
            Writer::validate_image(page)?;
        }

        if pages.len() == 1 {
            let page = pages.into_iter().next().expect("one page");
            return Writer::write_image(page, format, output_path);
        }

        match format {
            "jxl" => {
                eprintln!(
                    "Warning: JXL cannot represent page structure; writing all {} pages as a flat sequence",
                    pages.len()
                );
                Writer::write_jxl_pages(output_path, pages)
            }
            "pam" | "ppm" | "webp" => {
                let output_dir = Path::new(output_path).parent().unwrap_or_else(|| Path::new("."));
                let output_file_name = Path::new(output_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "invalid output file name"))?;

                for (i, page) in pages.into_iter().enumerate() {
                    let page_path = output_dir.join(format!("{}_page_{}.{}", output_file_name, i, format));
                    Writer::write_image(page, format, &page_path)?;
                }

                Ok(())
            }
            _ => Err(Error::new(ErrorKind::InvalidInput, format!("Invalid format: {}", format))),
        }
    }

    fn validate_image(image: &Image) -> Result<(), Error> {
        if image.width() == 0 || image.height() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("image has zero dimensions: {}x{}", image.width(), image.height()),
            ));
        }

        Ok(())
    }
}

fn pixel_info_for(format: PixelFormat) -> PixelInfo {
    let (data_type, bits_per_sample, exponent_bits_per_sample) = match format.sample_type() {
        SampleType::U8 => (JxlDataType::Uint8, 8, 0),
        SampleType::U16 => (JxlDataType::Uint16, 16, 0),
        SampleType::F32 | SampleType::F64 => (JxlDataType::Float, 32, 8),
    };

    PixelInfo {
        data_type,
        num_channels: format.channels() as u32,
        bits_per_sample,
        exponent_bits_per_sample,
        has_alpha: format.has_alpha(),
        is_gray: format.is_gray(),
    }
}

unsafe fn jxl_encode(
    enc: *mut jpegxl_sys::encoder::encode::JxlEncoder,
    canvas_width: u32,
    canvas_height: u32,
    first_frame: ImageFrame,
    rest: &mut dyn Iterator<Item = Result<ImageFrame, Error>>,
    is_animated: bool,
) -> Result<Vec<u8>, String> {
    let info = pixel_info_for(first_frame.image().format());

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

    let add_frame = |frame: ImageFrame| -> Result<(), String> {
        let delay = frame.delay();
        let mut image = frame.into_image();
        let has_crop = image.width() != canvas_width || image.height() != canvas_height;

        if is_animated || has_crop {
            let mut frame_header = MaybeUninit::<jpegxl_sys::metadata::codestream_header::JxlFrameHeader>::uninit();
            JxlEncoderInitFrameHeader(frame_header.as_mut_ptr());
            let mut frame_header = frame_header.assume_init();

            if is_animated {
                frame_header.duration = if delay == 0 { 1 } else { delay };
            }

            if has_crop {
                frame_header.layer_info.have_crop = JxlBool::True;
                frame_header.layer_info.crop_x0 = 0;
                frame_header.layer_info.crop_y0 = 0;
                frame_header.layer_info.xsize = image.width();
                frame_header.layer_info.ysize = image.height();
            }

            if JxlEncoderSetFrameHeader(frame_settings, &frame_header) != JxlEncoderStatus::Success {
                return Err("failed to set frame header".to_string());
            }
        }

        let format = image.format();
        if format.sample_type() == SampleType::F64 {
            image = image.convert(format.with_sample_type(SampleType::F32));
        }
        let bytes = image.as_bytes();
        let (ptr, len) = (bytes.as_ptr() as *const c_void, bytes.len());

        if JxlEncoderAddImageFrame(frame_settings, &pixel_format, ptr, len) != JxlEncoderStatus::Success {
            return Err("failed to add image frame".to_string());
        }

        Ok(())
    };

    add_frame(first_frame)?;

    for frame in rest {
        add_frame(frame.map_err(|e| e.to_string())?)?;
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
