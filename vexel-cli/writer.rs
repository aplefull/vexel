use std::{fs::File, io::{Error, ErrorKind, Write}, mem::MaybeUninit, path::{Path, PathBuf}};
use std::ffi::c_void;

use vexel::{Image, PixelData};

use webp::{AnimEncoder, AnimFrame, WebPConfig};

use jpegxl_sys::color::color_encoding::JxlColorEncoding;
use jpegxl_sys::common::types::{JxlBool, JxlDataType, JxlEndianness, JxlPixelFormat};
use jpegxl_sys::encoder::encode::{
    JxlColorEncodingSetToSRGB, JxlEncoderAddImageFrame, JxlEncoderCloseInput, JxlEncoderCreate,
    JxlEncoderDestroy, JxlEncoderFrameSettingsCreate, JxlEncoderInitBasicInfo,
    JxlEncoderInitFrameHeader, JxlEncoderProcessOutput, JxlEncoderSetBasicInfo,
    JxlEncoderSetCodestreamLevel, JxlEncoderSetColorEncoding, JxlEncoderSetFrameHeader,
    JxlEncoderSetFrameLossless, JxlEncoderStatus, JxlEncoderUseContainer,
};
use jpegxl_sys::metadata::codestream_header::{JxlAnimationHeader, JxlBasicInfo};

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
        let compressed = unsafe { 
            let encoder = JxlEncoderCreate(std::ptr::null());
            if encoder.is_null() {
                return Err(Error::new(ErrorKind::Other, "failed to create JXL encoder"));
            }

            let result = jxl_encode(encoder, image);

            JxlEncoderDestroy(encoder);

            result
        }.map_err(|e| Error::new(ErrorKind::Other, e))?;

        let mut file = File::create(output_path)?;
        file.write_all(&compressed)?;

        Ok(())
    }

    pub fn write_webp(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let width = image.width();
        let height = image.height();

        let has_alpha = image.has_alpha();
        let mut timestamp_ms = 0i32;
        let pixel_bufs: Vec<(Vec<u8>, i32)> = image.frames().iter().map(|frame| {
            let delta = if frame.delay == 0 { 1 } else { frame.delay as i32 };
            timestamp_ms += delta;
            let pixels = if has_alpha {
                frame.clone().into_rgba8().pixels().as_bytes().to_vec()
            } else {
                frame.clone().into_rgb8().pixels().as_bytes().to_vec()
            };
            (pixels, timestamp_ms)
        }).collect();

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
        let mut file = File::create(output_path)?;
        let width = image.width();
        let height = image.height();

        file.write_all(b"P7\n")?;
        file.write_all(format!("WIDTH {}\n", width).as_bytes())?;
        file.write_all(format!("HEIGHT {}\n", height).as_bytes())?;

        match &image.frames()[0].pixels {
            PixelData::RGB8(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 255\nTUPLTYPE RGB\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::RGBA8(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::RGB16(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::RGBA16(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::RGB32F(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::RGBA32F(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::RGB64F(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::RGBA64F(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::L1(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 1\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::L8(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 255\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::L16(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::LA8(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 255\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::LA16(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::L32F(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::LA32F(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::L64F(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::LA64F(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
        }

        Ok(())
    }

    pub fn write_ppm(output_path: &PathBuf, image: &Image) -> Result<(), Error> {
        let mut file = File::create(output_path)?;
        let width = image.width();
        let height = image.height();

        let frame = image.frames()[0].clone();
        let converted = if image.has_alpha() {
            frame.into_rgba8()
        } else {
            frame.into_rgb8()
        };
        let pixels = converted.pixels().as_bytes();

        file.write_all(b"P6\n")?;
        file.write_all(format!("{} {}\n", width, height).as_bytes())?;
        file.write_all(b"255\n")?;

        for y in 0..height {
            for x in 0..width {
                let pixel_index = ((y * width + x) * if image.has_alpha() { 4 } else { 3 }) as usize;

                let r = pixels[pixel_index];
                let g = pixels[pixel_index + 1];
                let b = pixels[pixel_index + 2];

                file.write_all(&[r, g, b])?;
            }
        }

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

    pub fn write_frames(output_path: &PathBuf, format: &str, image: &Image) -> Result<(), Error> {
        Writer::validate_image(image)?;

        for (i, frame) in image.frames().iter().enumerate() {
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

            let frame_image = Image::from_frame(frame.clone());

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
        let actual_bytes = image.pixels().as_bytes().len();

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
        PixelData::RGB8(d) | PixelData::RGBA8(d) => {
            (d.as_ptr() as *const c_void, d.len())
        }
        PixelData::L1(d) | PixelData::L8(d) | PixelData::LA8(d) => {
            (d.as_ptr() as *const c_void, d.len())
        }
        PixelData::RGB16(d) | PixelData::RGBA16(d) => {
            (d.as_ptr() as *const c_void, d.len() * 2)
        }
        PixelData::L16(d) | PixelData::LA16(d) => {
            (d.as_ptr() as *const c_void, d.len() * 2)
        }
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
            bits_per_sample: 8,
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
    image: &Image,
) -> Result<Vec<u8>, String> {
    let is_animated = image.frames().len() > 1;
    let first_frame = &image.frames()[0];
    let info = pixel_info_for(&first_frame.pixels);

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
    basic_info.xsize = image.width();
    basic_info.ysize = image.height();
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

    let pixel_format = JxlPixelFormat {
        num_channels: info.num_channels,
        data_type: info.data_type,
        endianness: JxlEndianness::Native,
        align: 0,
    };

    for frame in image.frames() {
        if is_animated {
            let mut frame_header = MaybeUninit::<jpegxl_sys::metadata::codestream_header::JxlFrameHeader>::uninit();
            JxlEncoderInitFrameHeader(frame_header.as_mut_ptr());
            let mut frame_header = frame_header.assume_init();
            frame_header.duration = if frame.delay == 0 { 1 } else { frame.delay };
            if JxlEncoderSetFrameHeader(frame_settings, &frame_header) != JxlEncoderStatus::Success {
                return Err("failed to set frame header".to_string());
            }
        }

        let f32_converted: Option<Vec<f32>>;
        let (ptr, len) = match &frame.pixels {
            PixelData::RGB64F(d) | PixelData::RGBA64F(d) | PixelData::L64F(d) | PixelData::LA64F(d) => {
                let converted: Vec<f32> = d.iter().map(|&v| v as f32).collect();
                f32_converted = Some(converted);
                let v = f32_converted.as_ref().unwrap();
                (v.as_ptr() as *const c_void, v.len() * 4)
            }
            other => {
                f32_converted = None;
                pixel_data_ptr_and_len(other)
            }
        };

        if JxlEncoderAddImageFrame(frame_settings, &pixel_format, ptr, len)
            != JxlEncoderStatus::Success
        {
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