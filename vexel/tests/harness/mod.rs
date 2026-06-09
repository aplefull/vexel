use std::path::{Path, PathBuf};
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
}

pub struct TestCase {
    pub name: &'static str,
    pub path: &'static str,
    pub validation: Option<Box<dyn Fn(&Image)>>,
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

pub fn image_to_reference(image: &Image) -> ReferenceImage {
    ReferenceImage {
        width: image.width(),
        height: image.height(),
        pixels: image.as_rgba8(),
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
    }
}

pub fn test_decode(test_case: TestCase) -> Result<TestResult, Box<dyn std::error::Error>> {
    let mut decoder = Vexel::open(get_in_path(test_case.path))?;

    match decoder.decode() {
        Ok(image) => {
            if let Some(validate) = test_case.validation {
                validate(&image);
            }
            run_comparison(&image, &test_case.comparison)
        }
        Err(e) => Ok(TestResult::Fail(format!("decode error: {:?}", e))),
    }
}
