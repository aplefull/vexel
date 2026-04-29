extern crate core;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use vexel::{Image, Vexel};
    use webp::Decoder as WebPDecoder;
    use writer::{Writer, WriterImage, WriterImageFrame};

    const BASE_PATH: &str = "./tests/images/";
    const REFERENCES_PATH: &str = "./tests/references/";

    enum Comparison {
        None,
        Exact { reference_path: &'static str },
        Fuzzy {
            reference_path: &'static str,
            tolerance: u8,
            max_differing_pixels: usize,
        },
    }

    struct TestCase {
        name: &'static str,
        path: &'static str,
        validation: Option<Box<dyn Fn(&Image)>>,
        save: bool,
        comparison: Comparison,
    }

    struct ReferenceImage {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    }

    fn get_in_path(path: &str) -> String {
        format!("{}{}", BASE_PATH, path)
    }

    fn get_ref_path(reference_path: &str) -> PathBuf {
        Path::new(REFERENCES_PATH).join(reference_path)
    }

    fn get_out_path(path: &str, ext: Option<&str>) -> PathBuf {
        let path = Path::new(path).with_extension(ext.unwrap_or("webp"));
        Path::new(BASE_PATH).join(path)
    }

    fn image_to_writer_image(image: &Image) -> WriterImage {
        let mut frames = Vec::new();

        for frame in image.frames() {
            frames.push(WriterImageFrame {
                width: frame.width(),
                height: frame.height(),
                has_alpha: frame.has_alpha(),
                delay: frame.delay(),
                pixels: if frame.has_alpha() {
                    frame.as_rgba8()
                } else {
                    frame.as_rgb8()
                },
            });
        }

        WriterImage {
            width: image.width(),
            height: image.height(),
            has_alpha: image.has_alpha(),
            frames,
        }
    }

    fn image_to_reference(image: &Image) -> ReferenceImage {
        let pixels = if image.has_alpha() {
            image.as_rgba8()
        } else {
            image.as_rgb8()
        };

        ReferenceImage {
            width: image.width(),
            height: image.height(),
            pixels,
        }
    }

    fn read_reference(path: &Path) -> Result<ReferenceImage, Box<dyn std::error::Error>> {
        let buf = std::fs::read(path)?;
        let decoded = WebPDecoder::new(&buf)
            .decode()
            .ok_or_else(|| format!("Failed to decode WebP reference: {}", path.display()))?;

        Ok(ReferenceImage {
            width: decoded.width(),
            height: decoded.height(),
            pixels: decoded.to_vec(),
        })
    }

    fn compare_exact(name: &str, actual: &ReferenceImage, reference: &ReferenceImage) {
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
            let total = actual.pixels.len();
            let differing = actual.pixels.iter().zip(reference.pixels.iter()).filter(|(a, b)| a != b).count();
            panic!(
                "[{}] Pixel data does not match reference exactly: {}/{} bytes differ",
                name, differing, total
            );
        }
    }

    fn compute_mse(a: &[u8], b: &[u8]) -> f64 {
        if a.is_empty() {
            return 0.0;
        }
        let sum: f64 = a.iter().zip(b.iter()).map(|(&x, &y)| (x as f64 - y as f64).powi(2)).sum();
        sum / a.len() as f64
    }

    fn compute_ssim(a: &[u8], b: &[u8]) -> f64 {
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

    fn compare_fuzzy(name: &str, actual: &ReferenceImage, reference: &ReferenceImage, tolerance: u8, max_differing_pixels: usize) {
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
            actual.pixels.len(), reference.pixels.len(),
            "[{}] Pixel buffer length mismatch: decoded={}, reference={}",
            name, actual.pixels.len(), reference.pixels.len()
        );

        let total_bytes = actual.pixels.len();
        let mut differing_bytes = 0usize;
        let mut worst_diff = 0u8;
        let mut worst_byte_index = 0usize;

        for (i, (&a, &b)) in actual.pixels.iter().zip(reference.pixels.iter()).enumerate() {
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
            let mse = compute_mse(&actual.pixels, &reference.pixels);
            let ssim = compute_ssim(&actual.pixels, &reference.pixels);
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

    fn run_comparison(name: &str, image: &Image, comparison: &Comparison) -> Result<(), Box<dyn std::error::Error>> {
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

    fn test_decode(test_case: TestCase) -> Result<(), Box<dyn std::error::Error>> {
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

    #[test]
    fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
        let test_cases = vec![
            TestCase {
                name: "JPEG Baseline",
                path: "jpeg/cat.jpg",
                validation: None,
                save: false,
                comparison: Comparison::Fuzzy { 
                    reference_path: "jpeg/cat.webp",
                    tolerance: 5,
                    max_differing_pixels: 10,
                 }
            },
            TestCase {
                name: "JPEG Lossless",
                path: "jpeg/2x2_lossless.jpg",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "JPEG-LS",
                path: "jpeg-ls/test_4x4.jls",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "GIF",
                path: "gif/animated.gif",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "NetPBM",
                path: "netpbm/P3_16bit.ppm",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "BMP",
                path: "bmp/test.bmp",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "PNG",
                path: "png/rgb_16bit.png",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "HDR",
                path: "hdr/sample_HDR.hdr",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "TIFF",
                path: "tiff/file_example_TIFF_10MB.tiff",
                validation: None,
                save: false,
                comparison: Comparison::None,
            },
            TestCase {
                name: "JBIG1 2x2 Checkerboard",
                path: "jbig1/2x2.jbg",
                validation: None,
                save: false,
                comparison: Comparison::Exact {
                    reference_path: "jbig1/2x2.webp",
                },
            },
            TestCase {
                name: "JBIG1 ccitt1",
                path: "jbig1/ccitt1.jbg",
                validation: None,
                save: false,
                comparison: Comparison::Exact {
                    reference_path: "jbig1/ccitt1.webp",
                },
            },
        ];

        for test_case in test_cases {
            test_decode(test_case)?;
        }

        Ok(())
    }

    #[test]
    #[ignore = "dev only"]
    // This test is used during development for convenience for any new image formats
    pub fn test_image() -> Result<(), Box<dyn std::error::Error>> {
        let in_path = r"/home/aplefull/Repos/vexel/vexel/tests/images/jpeg/cat_arithmetic.jpg";
        let out_path = Path::new(in_path).with_extension("webp");

        let mut decoder = Vexel::open(in_path)?;

        match decoder.decode() {
            Ok(image) => {
                Writer::write_webp(&out_path, &image_to_writer_image(&image))?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }
}
