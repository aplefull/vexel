use std::path::{Path, PathBuf};
use libavif_sys as avif_sys;
use vexel::{Image, Vexel};

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

pub struct ReferenceImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn get_in_path(path: &str) -> String {
    format!("{}{}", BASE_PATH, path)
}

pub fn get_ref_path(reference_path: &str) -> PathBuf {
    Path::new(REFERENCES_PATH).join(reference_path)
}

pub fn get_pixel(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let idx = (y * width + x) * 4;
    [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
}

pub fn image_to_reference(image: &Image) -> ReferenceImage {
    ReferenceImage {
        width: image.width(),
        height: image.height(),
        pixels: image.as_rgba8(),
    }
}

pub fn read_reference_all_frames(path: &Path) -> Result<Vec<ReferenceImage>, Box<dyn std::error::Error>> {
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

        let frame_count = (*decoder).imageCount as usize;
        let mut frames = Vec::with_capacity(frame_count);

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

            frames.push(ReferenceImage { width, height, pixels });
        }

        avif_sys::avifDecoderDestroy(decoder);
        Ok(frames)
    }
}

pub fn read_reference(path: &Path) -> Result<ReferenceImage, Box<dyn std::error::Error>> {
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

    Ok(ReferenceImage { width, height, pixels: rgba })
}

fn normalize_transparent_pixels(pixels: &[u8]) -> Vec<u8> {
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

pub fn compare_exact(actual: &ReferenceImage, reference: &ReferenceImage) -> Result<(), String> {
    if actual.width != reference.width {
        return Err(format!("width mismatch: decoded={}, reference={}", actual.width, reference.width));
    }
    if actual.height != reference.height {
        return Err(format!("height mismatch: decoded={}, reference={}", actual.height, reference.height));
    }
    if actual.pixels != reference.pixels {
        let actual_norm = normalize_transparent_pixels(&actual.pixels);
        let reference_norm = normalize_transparent_pixels(&reference.pixels);
        let total = actual_norm.len();
        let differing = actual_norm.iter().zip(reference_norm.iter()).filter(|(a, b)| a != b).count();
        if differing > 0 {
            return Err(format!("pixel data mismatch: {}/{} bytes differ", differing, total));
        }
    }
    Ok(())
}

pub fn compute_mse(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a.iter().zip(b.iter()).map(|(&x, &y)| (x as f64 - y as f64).powi(2)).sum();
    sum / a.len() as f64
}

pub fn compute_ssim(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len() as f64;
    if n == 0.0 {
        return 1.0;
    }

    let mean_a: f64 = a.iter().map(|&v| v as f64).sum::<f64>() / n;
    let mean_b: f64 = b.iter().map(|&v| v as f64).sum::<f64>() / n;

    let var_a: f64 = a.iter().map(|&v| (v as f64 - mean_a).powi(2)).sum::<f64>() / n;
    let var_b: f64 = b.iter().map(|&v| (v as f64 - mean_b).powi(2)).sum::<f64>() / n;

    let cov: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as f64 - mean_a) * (y as f64 - mean_b))
        .sum::<f64>()
        / n;

    let c1 = (0.01 * 255.0_f64).powi(2);
    let c2 = (0.03 * 255.0_f64).powi(2);

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

    let actual_pixels = normalize_transparent_pixels(&actual.pixels);
    let reference_pixels = normalize_transparent_pixels(&reference.pixels);

    let mse = compute_mse(&actual_pixels, &reference_pixels);
    let ssim = compute_ssim(&actual_pixels, &reference_pixels);
    let psnr = if mse > 0.0 {
        10.0 * (255.0_f64.powi(2) / mse).log10()
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
            let reference = read_reference(&get_ref_path(reference_path))?;
            let actual = image_to_reference(image);
            match compare_exact(&actual, &reference) {
                Ok(()) => Ok(TestResult::Ok { mse: None, ssim: None, psnr: None }),
                Err(msg) => Ok(TestResult::Fail(msg)),
            }
        }
        Comparison::Fuzzy { reference_path, mse_threshold, ssim_threshold } => {
            let reference = read_reference(&get_ref_path(reference_path))?;
            let actual = image_to_reference(image);
            match compare_fuzzy(&actual, &reference, *mse_threshold, *ssim_threshold) {
                Ok((mse, ssim, psnr)) => Ok(TestResult::Ok { mse: Some(mse), ssim: Some(ssim), psnr: Some(psnr) }),
                Err(msg) => Ok(TestResult::Fail(msg)),
            }
        }
        Comparison::ExactFrames { reference_path } => {
            let ref_frames = read_reference_all_frames(&get_ref_path(reference_path))?;
            let dec_frames = image.frames();
            if dec_frames.len() != ref_frames.len() {
                return Ok(TestResult::Fail(format!(
                    "frame count mismatch: decoded={}, reference={}",
                    dec_frames.len(),
                    ref_frames.len()
                )));
            }
            for (i, (dec_frame, ref_frame)) in dec_frames.iter().zip(ref_frames.iter()).enumerate() {
                let actual = ReferenceImage {
                    width: dec_frame.width(),
                    height: dec_frame.height(),
                    pixels: dec_frame.as_rgba8(),
                };
                if let Err(msg) = compare_exact(&actual, ref_frame) {
                    return Ok(TestResult::Fail(format!("frame {}: {}", i, msg)));
                }
            }
            Ok(TestResult::Ok { mse: None, ssim: None, psnr: None })
        }
        Comparison::FuzzyFrames { reference_path, mse_threshold, ssim_threshold } => {
            let ref_frames = read_reference_all_frames(&get_ref_path(reference_path))?;
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
            for (i, (dec_frame, ref_frame)) in dec_frames.iter().zip(ref_frames.iter()).enumerate() {
                let actual = ReferenceImage {
                    width: dec_frame.width(),
                    height: dec_frame.height(),
                    pixels: dec_frame.as_rgba8(),
                };
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
