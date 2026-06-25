use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use libavif_sys as avif_sys;
use jpegxl_sys::common::types::{JxlBitDepthType, JxlBitDepth, JxlDataType, JxlEndianness, JxlPixelFormat};
use jpegxl_sys::decode::{
    JxlDecoderCreate, JxlDecoderDestroy, JxlDecoderSubscribeEvents, JxlDecoderSetInput,
    JxlDecoderCloseInput, JxlDecoderProcessInput, JxlDecoderGetBasicInfo,
    JxlDecoderImageOutBufferSize, JxlDecoderSetImageOutBuffer, JxlDecoderSetImageOutBitDepth,
    JxlDecoderStatus,
};
use jpegxl_sys::metadata::codestream_header::JxlBasicInfo;
use vexel::{Image, PixelData, Vexel};

pub const BASE_PATH: &str = "./tests/images/";
pub const REFERENCES_PATH: &str = "./tests/references/";

pub const DEFAULT_MSE_THRESHOLD: f64 = 0.5;
pub const DEFAULT_SSIM_THRESHOLD: f64 = 0.99990;

pub enum Comparison {
    None,
    Exact { reference_path: &'static str },
    Fuzzy {
        reference_path: &'static str,
        mse_threshold: f64,
        ssim_threshold: f64,
    },
    ExactFrames { reference_path: &'static str },
    #[allow(dead_code)]
    FuzzyFrames {
        reference_path: &'static str,
        mse_threshold: f64,
        ssim_threshold: f64,
    },
}

pub struct TestCase {
    pub name: &'static str,
    pub path: &'static str,
    pub validation: Option<Box<dyn Fn(&Image) -> Result<(), String>>>,
    pub comparison: Comparison,
}

pub enum TestResult {
    Ok { mse: Option<f64>, ssim: Option<f64>, psnr: Option<f64> },
    Fail(String),
}

pub enum ReferencePixels {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

pub struct ReferenceImage {
    pub width: u32,
    pub height: u32,
    pub pixels: ReferencePixels,
}

impl ReferencePixels {
    pub fn max_value(&self) -> f64 {
        match self {
            ReferencePixels::U8(_) => 255.0,
            ReferencePixels::U16(_) => 65535.0,
            ReferencePixels::F32(_) => 1.0,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            ReferencePixels::U8(v) => v.len(),
            ReferencePixels::U16(v) => v.len(),
            ReferencePixels::F32(v) => v.len(),
        }
    }

    pub fn as_f64_iter<'a>(&'a self) -> Box<dyn Iterator<Item = f64> + 'a> {
        match self {
            ReferencePixels::U8(v) => Box::new(v.iter().map(|&x| x as f64)),
            ReferencePixels::U16(v) => Box::new(v.iter().map(|&x| x as f64)),
            ReferencePixels::F32(v) => Box::new(v.iter().map(|&x| x as f64)),
        }
    }

}


pub fn get_pixel(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let idx = (y * width + x) * 4;
    [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
}

pub fn get_in_path(path: &str) -> String {
    format!("{}{}", BASE_PATH, path)
}

pub fn get_ref_path(reference_path: &str) -> PathBuf {
    Path::new(REFERENCES_PATH).join(reference_path)
}

pub fn image_to_reference_native(image: &Image) -> ReferenceImage {
    let width = image.width();
    let height = image.height();

    let pixels = match image.pixels() {
        PixelData::L1(p) => {
            let n = (image.width() * image.height()) as usize;
            ReferencePixels::U8(p.iter().flat_map(|&b| (0..8).map(move |i| (b >> (7 - i)) & 1)).take(n).collect())
        }
        PixelData::RGB8(p) | PixelData::RGBA8(p) | PixelData::L8(p) | PixelData::LA8(p) => {
            ReferencePixels::U8(p)
        }
        PixelData::RGB16(p) | PixelData::RGBA16(p) | PixelData::L16(p) | PixelData::LA16(p) => {
            ReferencePixels::U16(p)
        }
        PixelData::RGB32F(p) | PixelData::RGBA32F(p) | PixelData::L32F(p) | PixelData::LA32F(p) => {
            ReferencePixels::F32(p)
        }
        PixelData::RGB64F(p) | PixelData::RGBA64F(p) | PixelData::L64F(p) | PixelData::LA64F(p) => {
            ReferencePixels::F32(p.iter().map(|&v| v as f32).collect())
        }
    };

    ReferenceImage { width, height, pixels }
}

pub fn image_to_reference_rgba8(image: &Image) -> ReferenceImage {
    ReferenceImage {
        width: image.width(),
        height: image.height(),
        pixels: ReferencePixels::U8(image.as_rgba8()),
    }
}

pub fn frame_to_reference_native(image: &Image, frame_idx: usize) -> ReferenceImage {
    let width = image.width();
    let height = image.height();
    let frame = &image.frames()[frame_idx];

    let pixels = match frame.pixels() {
        PixelData::L1(p) => {
            let n = (frame.width() * frame.height()) as usize;
            ReferencePixels::U8(p.iter().flat_map(|&b| (0..8).map(move |i| (b >> (7 - i)) & 1)).take(n).collect())
        }
        PixelData::RGB8(p) | PixelData::RGBA8(p) | PixelData::L8(p) | PixelData::LA8(p) => {
            ReferencePixels::U8(p.clone())
        }
        PixelData::RGB16(p) | PixelData::RGBA16(p) | PixelData::L16(p) | PixelData::LA16(p) => {
            ReferencePixels::U16(p.clone())
        }
        PixelData::RGB32F(p) | PixelData::RGBA32F(p) | PixelData::L32F(p) | PixelData::LA32F(p) => {
            ReferencePixels::F32(p.clone())
        }
        PixelData::RGB64F(p) | PixelData::RGBA64F(p) | PixelData::L64F(p) | PixelData::LA64F(p) => {
            ReferencePixels::F32(p.iter().map(|&v| v as f32).collect())
        }
    };

    ReferenceImage { width, height, pixels }
}

pub fn frame_to_reference_rgba8(image: &Image, frame_idx: usize) -> ReferenceImage {
    let frame = &image.frames()[frame_idx];
    ReferenceImage {
        width: frame.width(),
        height: frame.height(),
        pixels: ReferencePixels::U8(frame.as_rgba8()),
    }
}

fn pixel_format_for_jxl(info: &JxlBasicInfo) -> (JxlPixelFormat, JxlBitDepth, bool) {
    let is_float = info.exponent_bits_per_sample > 0;
    let num_channels = info.num_color_channels + info.num_extra_channels;

    let (data_type, bit_depth_type) = if is_float {
        (JxlDataType::Float, JxlBitDepthType::FromPixelFormat)
    } else if info.bits_per_sample <= 8 {
        (JxlDataType::Uint8, JxlBitDepthType::FromCodestream)
    } else {
        (JxlDataType::Uint16, JxlBitDepthType::FromCodestream)
    };

    let pixel_format = JxlPixelFormat {
        num_channels,
        data_type,
        endianness: JxlEndianness::Native,
        align: 0,
    };

    let bit_depth = JxlBitDepth {
        r#type: bit_depth_type,
        bits_per_sample: info.bits_per_sample,
        exponent_bits_per_sample: info.exponent_bits_per_sample,
    };

    (pixel_format, bit_depth, is_float)
}

pub fn read_reference_jxl(path: &Path) -> Result<ReferenceImage, Box<dyn std::error::Error>> {
    let frames = read_reference_jxl_all_frames(path)?;
    frames.into_iter().next().ok_or_else(|| "JXL file has no frames".into())
}

pub fn read_reference_jxl_all_frames(path: &Path) -> Result<Vec<ReferenceImage>, Box<dyn std::error::Error>> {
    let buf = std::fs::read(path)?;
    let mut frames = Vec::new();

    unsafe {
        let dec = JxlDecoderCreate(std::ptr::null());
        if dec.is_null() {
            return Err("JxlDecoderCreate returned null".into());
        }

        let events = JxlDecoderStatus::BasicInfo as i32 | JxlDecoderStatus::FullImage as i32;
        if JxlDecoderSubscribeEvents(dec, events) != JxlDecoderStatus::Success {
            JxlDecoderDestroy(dec);
            return Err("JxlDecoderSubscribeEvents failed".into());
        }

        if JxlDecoderSetInput(dec, buf.as_ptr(), buf.len()) != JxlDecoderStatus::Success {
            JxlDecoderDestroy(dec);
            return Err("JxlDecoderSetInput failed".into());
        }
        JxlDecoderCloseInput(dec);

        let mut basic_info = MaybeUninit::<JxlBasicInfo>::uninit();
        let mut pixel_format = JxlPixelFormat {
            num_channels: 4,
            data_type: JxlDataType::Uint8,
            endianness: JxlEndianness::Native,
            align: 0,
        };
        let mut bit_depth = JxlBitDepth {
            r#type: JxlBitDepthType::FromPixelFormat,
            bits_per_sample: 8,
            exponent_bits_per_sample: 0,
        };
        let mut is_float = false;
        let mut width = 0u32;
        let mut height = 0u32;

        loop {
            let status = JxlDecoderProcessInput(dec);

            match status {
                JxlDecoderStatus::Error => {
                    JxlDecoderDestroy(dec);
                    return Err("JxlDecoderProcessInput returned error".into());
                }
                JxlDecoderStatus::Success => break,
                JxlDecoderStatus::BasicInfo => {
                    if JxlDecoderGetBasicInfo(dec, basic_info.as_mut_ptr()) != JxlDecoderStatus::Success {
                        JxlDecoderDestroy(dec);
                        return Err("JxlDecoderGetBasicInfo failed".into());
                    }
                    let info = basic_info.assume_init_ref();
                    width = info.xsize;
                    height = info.ysize;
                    let (pf, bd, f) = pixel_format_for_jxl(info);
                    pixel_format = pf;
                    bit_depth = bd;
                    is_float = f;
                }
                JxlDecoderStatus::NeedImageOutBuffer => {
                    let mut buf_size = 0usize;
                    if JxlDecoderImageOutBufferSize(dec, &pixel_format, &mut buf_size) != JxlDecoderStatus::Success {
                        JxlDecoderDestroy(dec);
                        return Err("JxlDecoderImageOutBufferSize failed".into());
                    }

                    macro_rules! decode_frame {
                        ($buf:expr, $variant:expr) => {{
                            if JxlDecoderSetImageOutBuffer(
                                dec, &pixel_format,
                                $buf.as_mut_ptr() as *mut c_void,
                                buf_size,
                            ) != JxlDecoderStatus::Success {
                                JxlDecoderDestroy(dec);
                                return Err("JxlDecoderSetImageOutBuffer failed".into());
                            }
                            if JxlDecoderSetImageOutBitDepth(dec, &bit_depth) != JxlDecoderStatus::Success {
                                JxlDecoderDestroy(dec);
                                return Err("JxlDecoderSetImageOutBitDepth failed".into());
                            }
                            let status = JxlDecoderProcessInput(dec);
                            if status != JxlDecoderStatus::FullImage && status != JxlDecoderStatus::Success {
                                JxlDecoderDestroy(dec);
                                return Err(format!("expected FullImage after setting buffer, got {:?}", status).into());
                            }
                            frames.push(ReferenceImage { width, height, pixels: $variant($buf) });
                        }};
                    }

                    if is_float {
                        let mut pixel_buf = vec![0f32; buf_size / 4];
                        decode_frame!(pixel_buf, ReferencePixels::F32);
                    } else if pixel_format.data_type == JxlDataType::Uint16 {
                        let mut pixel_buf = vec![0u16; buf_size / 2];
                        decode_frame!(pixel_buf, ReferencePixels::U16);
                    } else {
                        let mut pixel_buf = vec![0u8; buf_size];
                        decode_frame!(pixel_buf, ReferencePixels::U8);
                    }
                }
                JxlDecoderStatus::FullImage => {}
                _ => {}
            }
        }

        JxlDecoderDestroy(dec);
    }

    Ok(frames)
}

pub fn read_reference_avif(path: &Path) -> Result<ReferenceImage, Box<dyn std::error::Error>> {
    let buf = std::fs::read(path)?;
    let image = libavif::decode_rgb(&buf)
        .map_err(|e| format!("Failed to decode AVIF reference {}: {:?}", path.display(), e))?;

    let width = image.width();
    let height = image.height();

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let (r, g, b, a) = image.pixel(x, y);
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    Ok(ReferenceImage { width, height, pixels: ReferencePixels::U8(rgba) })
}

pub fn read_reference_avif_all_frames(path: &Path) -> Result<Vec<ReferenceImage>, Box<dyn std::error::Error>> {
    let buf = std::fs::read(path)?;
    unsafe {
        let decoder = avif_sys::avifDecoderCreate();
        if decoder.is_null() {
            return Err("avifDecoderCreate returned null".into());
        }

        let set_io = avif_sys::avifDecoderSetIOMemory(decoder, buf.as_ptr(), buf.len());
        if set_io != avif_sys::AVIF_RESULT_OK {
            avif_sys::avifDecoderDestroy(decoder);
            return Err(format!("avifDecoderSetIOMemory failed: {}", set_io).into());
        }

        let parse = avif_sys::avifDecoderParse(decoder);
        if parse != avif_sys::AVIF_RESULT_OK {
            avif_sys::avifDecoderDestroy(decoder);
            return Err(format!("avifDecoderParse failed: {}", parse).into());
        }

        let mut frames = Vec::new();

        loop {
            let result = avif_sys::avifDecoderNextImage(decoder);
            if result == avif_sys::AVIF_RESULT_NO_IMAGES_REMAINING {
                break;
            }
            if result != avif_sys::AVIF_RESULT_OK {
                avif_sys::avifDecoderDestroy(decoder);
                return Err(format!("avifDecoderNextImage failed: {}", result).into());
            }

            let avif_image = (*decoder).image;
            let width = (*avif_image).width;
            let height = (*avif_image).height;

            let mut rgb = avif_sys::avifRGBImage::default();
            avif_sys::avifRGBImageSetDefaults(&mut rgb, avif_image);
            rgb.format = avif_sys::AVIF_RGB_FORMAT_RGBA;
            rgb.depth = 8;

            let alloc = avif_sys::avifRGBImageAllocatePixels(&mut rgb);
            if alloc != avif_sys::AVIF_RESULT_OK {
                avif_sys::avifDecoderDestroy(decoder);
                return Err(format!("avifRGBImageAllocatePixels failed: {}", alloc).into());
            }

            let convert = avif_sys::avifImageYUVToRGB(avif_image, &mut rgb);
            if convert != avif_sys::AVIF_RESULT_OK {
                avif_sys::avifRGBImageFreePixels(&mut rgb);
                avif_sys::avifDecoderDestroy(decoder);
                return Err(format!("avifImageYUVToRGB failed: {}", convert).into());
            }

            let pixel_count = (width * height * 4) as usize;
            let pixels = std::slice::from_raw_parts(rgb.pixels, pixel_count).to_vec();
            avif_sys::avifRGBImageFreePixels(&mut rgb);

            frames.push(ReferenceImage { width, height, pixels: ReferencePixels::U8(pixels) });
        }

        avif_sys::avifDecoderDestroy(decoder);
        Ok(frames)
    }
}

pub fn read_reference(path: &Path) -> Result<ReferenceImage, Box<dyn std::error::Error>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("jxl") => read_reference_jxl(path),
        _ => read_reference_avif(path),
    }
}

pub fn read_reference_all_frames(path: &Path) -> Result<Vec<ReferenceImage>, Box<dyn std::error::Error>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("jxl") => read_reference_jxl_all_frames(path),
        _ => read_reference_avif_all_frames(path),
    }
}

fn normalize_transparent_pixels_u8(pixels: &[u8]) -> Vec<u8> {
    let mut out = pixels.to_vec();
    for chunk in out.chunks_exact_mut(4) {
        if chunk[3] == 0 {
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
        }
    }
    out
}

fn same_type(a: &ReferencePixels, b: &ReferencePixels) -> bool {
    matches!(
        (a, b),
        (ReferencePixels::U8(_), ReferencePixels::U8(_))
            | (ReferencePixels::U16(_), ReferencePixels::U16(_))
            | (ReferencePixels::F32(_), ReferencePixels::F32(_))
    )
}

pub fn compare_exact(actual: &ReferenceImage, reference: &ReferenceImage) -> Result<(), String> {
    if actual.width != reference.width {
        return Err(format!("width mismatch: decoded={}, reference={}", actual.width, reference.width));
    }
    if actual.height != reference.height {
        return Err(format!("height mismatch: decoded={}, reference={}", actual.height, reference.height));
    }
    if !same_type(&actual.pixels, &reference.pixels) {
        return Err(format!(
            "pixel type mismatch: decoded is {}, reference is {}",
            pixel_type_name(&actual.pixels),
            pixel_type_name(&reference.pixels),
        ));
    }
    if actual.pixels.len() != reference.pixels.len() {
        return Err(format!(
            "pixel buffer length mismatch: decoded={}, reference={}",
            actual.pixels.len(),
            reference.pixels.len()
        ));
    }

    match (&actual.pixels, &reference.pixels) {
        (ReferencePixels::U8(a), ReferencePixels::U8(b)) => {
            let a_norm = normalize_transparent_pixels_u8(a);
            let b_norm = normalize_transparent_pixels_u8(b);
            let differing = a_norm.iter().zip(b_norm.iter()).filter(|(x, y)| x != y).count();
            if differing > 0 {
                return Err(format!("pixel data mismatch: {}/{} values differ", differing, a.len()));
            }
        }
        (ReferencePixels::U16(a), ReferencePixels::U16(b)) => {
            let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
            if differing > 0 {
                return Err(format!("pixel data mismatch: {}/{} values differ", differing, a.len()));
            }
        }
        (ReferencePixels::F32(a), ReferencePixels::F32(b)) => {
            let differing = a.iter().zip(b.iter()).filter(|(x, y)| x.to_bits() != y.to_bits()).count();
            if differing > 0 {
                return Err(format!("pixel data mismatch: {}/{} values differ", differing, a.len()));
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn pixel_type_name(p: &ReferencePixels) -> &'static str {
    match p {
        ReferencePixels::U8(_) => "U8",
        ReferencePixels::U16(_) => "U16",
        ReferencePixels::F32(_) => "F32",
    }
}

pub fn compute_mse(a: &ReferencePixels, b: &ReferencePixels) -> f64 {
    let n = a.len();
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = a.as_f64_iter().zip(b.as_f64_iter()).map(|(x, y)| (x - y).powi(2)).sum();
    sum / n as f64
}

pub fn compute_ssim(a: &ReferencePixels, b: &ReferencePixels) -> f64 {
    let n = a.len() as f64;
    if n == 0.0 {
        return 1.0;
    }

    let mean_a: f64 = a.as_f64_iter().sum::<f64>() / n;
    let mean_b: f64 = b.as_f64_iter().sum::<f64>() / n;

    let var_a: f64 = a.as_f64_iter().map(|v| (v - mean_a).powi(2)).sum::<f64>() / n;
    let var_b: f64 = b.as_f64_iter().map(|v| (v - mean_b).powi(2)).sum::<f64>() / n;

    let cov: f64 = a.as_f64_iter().zip(b.as_f64_iter())
        .map(|(x, y)| (x - mean_a) * (y - mean_b))
        .sum::<f64>() / n;

    let max_val = a.max_value();
    let c1 = (0.01 * max_val).powi(2);
    let c2 = (0.03 * max_val).powi(2);

    (2.0 * mean_a * mean_b + c1) * (2.0 * cov + c2)
        / ((mean_a.powi(2) + mean_b.powi(2) + c1) * (var_a + var_b + c2))
}

pub fn compare_fuzzy(
    actual: &ReferenceImage,
    reference: &ReferenceImage,
    mse_threshold: f64,
    ssim_threshold: f64,
) -> Result<(f64, f64, f64), String> {
    if actual.width != reference.width {
        return Err(format!("width mismatch: decoded={}, reference={}", actual.width, reference.width));
    }
    if actual.height != reference.height {
        return Err(format!("height mismatch: decoded={}, reference={}", actual.height, reference.height));
    }
    if actual.pixels.len() != reference.pixels.len() {
        return Err(format!(
            "pixel buffer length mismatch: decoded={}, reference={}",
            actual.pixels.len(),
            reference.pixels.len()
        ));
    }

    let mse = compute_mse(&actual.pixels, &reference.pixels);
    let ssim = compute_ssim(&actual.pixels, &reference.pixels);
    let max_val = actual.pixels.max_value();
    let psnr = if mse > 0.0 {
        10.0 * (max_val.powi(2) / mse).log10()
    } else {
        f64::INFINITY
    };

    if mse > mse_threshold || ssim < ssim_threshold {
        return Err(format!(
            "quality thresholds not met: MSE={:.5} (max {:.5}), SSIM={:.6} (min {:.6}), PSNR={:.2} dB",
            mse, mse_threshold, ssim, ssim_threshold, psnr
        ));
    }

    Ok((mse, ssim, psnr))
}

pub fn run_comparison(image: &Image, comparison: &Comparison) -> Result<TestResult, Box<dyn std::error::Error>> {
    match comparison {
        Comparison::None => Ok(TestResult::Ok { mse: None, ssim: None, psnr: None }),
        Comparison::Exact { reference_path } => {
            let ref_path = get_ref_path(reference_path);
            let is_jxl = ref_path.extension().and_then(|e| e.to_str()) == Some("jxl");
            let reference = read_reference(&ref_path)?;
            let actual = if is_jxl { image_to_reference_native(image) } else { image_to_reference_rgba8(image) };
            match compare_exact(&actual, &reference) {
                Ok(()) => Ok(TestResult::Ok { mse: None, ssim: None, psnr: None }),
                Err(msg) => Ok(TestResult::Fail(msg)),
            }
        }
        Comparison::Fuzzy { reference_path, mse_threshold, ssim_threshold } => {
            let ref_path = get_ref_path(reference_path);
            let is_jxl = ref_path.extension().and_then(|e| e.to_str()) == Some("jxl");
            let reference = read_reference(&ref_path)?;
            let actual = if is_jxl { image_to_reference_native(image) } else { image_to_reference_rgba8(image) };
            match compare_fuzzy(&actual, &reference, *mse_threshold, *ssim_threshold) {
                Ok((mse, ssim, psnr)) => Ok(TestResult::Ok { mse: Some(mse), ssim: Some(ssim), psnr: Some(psnr) }),
                Err(msg) => Ok(TestResult::Fail(msg)),
            }
        }
        Comparison::ExactFrames { reference_path } => {
            let ref_path = get_ref_path(reference_path);
            let is_jxl = ref_path.extension().and_then(|e| e.to_str()) == Some("jxl");
            let ref_frames = read_reference_all_frames(&ref_path)?;
            let dec_frames = image.frames();
            if dec_frames.len() != ref_frames.len() {
                return Ok(TestResult::Fail(format!(
                    "frame count mismatch: decoded={}, reference={}",
                    dec_frames.len(),
                    ref_frames.len()
                )));
            }
            for (i, ref_frame) in ref_frames.iter().enumerate() {
                let actual = if is_jxl { frame_to_reference_native(image, i) } else { frame_to_reference_rgba8(image, i) };
                if let Err(msg) = compare_exact(&actual, ref_frame) {
                    return Ok(TestResult::Fail(format!("frame {}: {}", i, msg)));
                }
            }
            Ok(TestResult::Ok { mse: None, ssim: None, psnr: None })
        }
        Comparison::FuzzyFrames { reference_path, mse_threshold, ssim_threshold } => {
            let ref_path = get_ref_path(reference_path);
            let is_jxl = ref_path.extension().and_then(|e| e.to_str()) == Some("jxl");
            let ref_frames = read_reference_all_frames(&ref_path)?;
            let dec_frames = image.frames();
            if dec_frames.len() != ref_frames.len() {
                return Ok(TestResult::Fail(format!(
                    "frame count mismatch: decoded={}, reference={}",
                    dec_frames.len(),
                    ref_frames.len()
                )));
            }
            let mut total_mse = 0.0f64;
            let mut total_ssim = 0.0f64;
            let mut total_psnr = 0.0f64;
            let n = dec_frames.len();
            for (i, ref_frame) in ref_frames.iter().enumerate() {
                let actual = if is_jxl { frame_to_reference_native(image, i) } else { frame_to_reference_rgba8(image, i) };
                match compare_fuzzy(&actual, ref_frame, *mse_threshold, *ssim_threshold) {
                    Ok((mse, ssim, psnr)) => {
                        total_mse += mse;
                        total_ssim += ssim;
                        total_psnr += psnr;
                    }
                    Err(msg) => return Ok(TestResult::Fail(format!("frame {}: {}", i, msg))),
                }
            }
            let count = n.max(1) as f64;
            Ok(TestResult::Ok {
                mse: Some(total_mse / count),
                ssim: Some(total_ssim / count),
                psnr: Some(total_psnr / count),
            })
        }
    }
}

pub fn test_decode(test_case: TestCase) -> Result<TestResult, Box<dyn std::error::Error>> {
    let mut decoder = Vexel::open(get_in_path(test_case.path))?;

    match decoder.decode() {
        Ok(image) => {
            if let Some(validate) = test_case.validation {
                if let Err(msg) = validate(&image) {
                    return Ok(TestResult::Fail(msg));
                }
            }
            run_comparison(&image, &test_case.comparison)
        }
        Err(e) => Ok(TestResult::Fail(format!("decode error: {:?}", e))),
    }
}
