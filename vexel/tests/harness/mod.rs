use std::path::{Path, PathBuf};
use vexel::{Image, Vexel};
use webp::Decoder as WebPDecoder;
use writer::{Writer, WriterImage, WriterImageFrame};

pub const BASE_PATH: &str = "./tests/images/";
pub const REFERENCES_PATH: &str = "./tests/references/";

pub enum Comparison {
    None,
    Exact { reference_path: &'static str },
    Fuzzy {
        reference_path: &'static str,
        tolerance: u8,
        max_differing_pixels: usize,
    },
}

pub struct TestCase {
    pub name: &'static str,
    pub path: &'static str,
    pub validation: Option<Box<dyn Fn(&Image)>>,
    pub save: bool,
    pub comparison: Comparison,
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

pub fn get_out_path(path: &str, ext: Option<&str>) -> PathBuf {
    let path = Path::new(path).with_extension(ext.unwrap_or("webp"));
    Path::new(BASE_PATH).join(path)
}

pub fn image_to_writer_image(image: &Image) -> WriterImage {
    let mut frames = Vec::new();

    for frame in image.frames() {
        frames.push(WriterImageFrame {
            width: frame.width(),
            height: frame.height(),
            has_alpha: true,
            delay: frame.delay(),
            pixels: frame.as_rgba8(),
        });
    }

    WriterImage {
        width: image.width(),
        height: image.height(),
        has_alpha: true,
        frames,
    }
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
    let decoded = WebPDecoder::new(&buf)
        .decode()
        .ok_or_else(|| format!("Failed to decode WebP reference: {}", path.display()))?;

    let rgba = decoded.to_image().to_rgba8();

    Ok(ReferenceImage {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
    })
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

pub fn compare_exact(name: &str, actual: &ReferenceImage, reference: &ReferenceImage) {
    assert_eq!(
        actual.width, reference.width,
        "[{}] Width mismatch: decoded={}, reference={}",
        name, actual.width, reference.width
    );
    assert_eq!(
        actual.height, reference.height,
        "[{}] Height mismatch: decoded={}, reference={}",
        name, actual.height, reference.height
    );
    if actual.pixels != reference.pixels {
        let actual_norm = normalize_transparent_pixels(&actual.pixels);
        let reference_norm = normalize_transparent_pixels(&reference.pixels);
        let total = actual_norm.len();
        let differing = actual_norm.iter().zip(reference_norm.iter()).filter(|(a, b)| a != b).count();
        if differing > 0 {
            panic!(
                "[{}] Pixel data does not match reference exactly: {}/{} bytes differ",
                name, differing, total
            );
        }
    }
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

pub fn compare_fuzzy(name: &str, actual: &ReferenceImage, reference: &ReferenceImage, tolerance: u8, max_differing_pixels: usize) {
    assert_eq!(
        actual.width, reference.width,
        "[{}] Width mismatch: decoded={}, reference={}",
        name, actual.width, reference.width
    );
    assert_eq!(
        actual.height, reference.height,
        "[{}] Height mismatch: decoded={}, reference={}",
        name, actual.height, reference.height
    );
    assert_eq!(
        actual.pixels.len(),
        reference.pixels.len(),
        "[{}] Pixel buffer length mismatch: decoded={}, reference={}",
        name,
        actual.pixels.len(),
        reference.pixels.len()
    );

    let actual_pixels = normalize_transparent_pixels(&actual.pixels);
    let reference_pixels = normalize_transparent_pixels(&reference.pixels);
    let total_bytes = actual_pixels.len();
    let mut differing_bytes = 0usize;
    let mut worst_diff = 0u8;
    let mut worst_byte_index = 0usize;

    for (i, (&a, &b)) in actual_pixels.iter().zip(reference_pixels.iter()).enumerate() {
        let diff = a.abs_diff(b);
        if diff > worst_diff {
            worst_diff = diff;
            worst_byte_index = i;
        }
        if diff > tolerance {
            differing_bytes += 1;
        }
    }

    if differing_bytes > max_differing_pixels {
        let mse = compute_mse(&actual_pixels, &reference_pixels);
        let ssim = compute_ssim(&actual_pixels, &reference_pixels);
        panic!(
            "[{}] Too many bytes exceed tolerance={}: {}/{} bytes differ\n  \
             worst diff={} at byte index {}\n  \
             MSE={:.4}  PSNR={:.2} dB  SSIM={:.6}",
            name,
            tolerance,
            differing_bytes,
            total_bytes,
            worst_diff,
            worst_byte_index,
            mse,
            if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0_f64.powi(2) / mse).log10() },
            ssim,
        );
    }
}

pub fn run_comparison(name: &str, image: &Image, comparison: &Comparison) -> Result<(), Box<dyn std::error::Error>> {
    match comparison {
        Comparison::None => {}
        Comparison::Exact { reference_path } => {
            let reference = read_reference(&get_ref_path(reference_path))?;
            let actual = image_to_reference(image);
            compare_exact(name, &actual, &reference);
        }
        Comparison::Fuzzy { reference_path, tolerance, max_differing_pixels } => {
            let reference = read_reference(&get_ref_path(reference_path))?;
            let actual = image_to_reference(image);
            compare_fuzzy(name, &actual, &reference, *tolerance, *max_differing_pixels);
        }
    }
    Ok(())
}

pub fn test_decode(test_case: TestCase) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = Vexel::open(get_in_path(test_case.path))?;

    match decoder.decode() {
        Ok(image) => {
            if let Some(validate) = test_case.validation {
                validate(&image);
            }

            run_comparison(test_case.name, &image, &test_case.comparison)?;

            if test_case.save {
                Writer::write_webp(&get_out_path(test_case.path, None), &image_to_writer_image(&image))?;
            }

            Ok(())
        }
        Err(e) => {
            println!("Error decoding image: {:?}", e);
            panic!("Failed to decode {}", test_case.name);
        }
    }
}
