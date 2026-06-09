mod harness;
mod corpus;

use std::path::Path;
use harness::*;
use vexel::Vexel;

fn load_env_file() {
    let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join(".env");
    if let Ok(contents) = std::fs::read_to_string(env_path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if std::env::var(key.trim()).is_err() {
                    std::env::set_var(key.trim(), value.trim());
                }
            }
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
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG Lossless",
            path: "jpeg/2x2_lossless.jpg",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JPEG-LS",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF",
            path: "gif/animated.gif",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "BMP",
            path: "bmp/test.bmp",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "PNG",
            path: "png/0b7d50ac449fd59eb3de00647636d0c9.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/0b7d50ac449fd59eb3de00647636d0c9.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/138331052d7c6e4acebfaa92af314e12.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/138331052d7c6e4acebfaa92af314e12.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "HDR",
            path: "hdr/sample_HDR.hdr",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "TIFF",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JBIG1 2x2 Checkerboard",
            path: "jbig1/2x2.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/2x2.avif",
            },
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/ccitt1.avif",
            },
        },
        TestCase {
            name: "TGA ctc32",
            path: "tga/ctc32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/ctc32.avif",
            },
        },
        TestCase {
            name: "TGA flag_t32",
            path: "tga/flag_t32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/flag_t32.avif",
            },
        },
        TestCase {
            name: "TGA lena3",
            path: "tga/lena3.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/lena3.avif",
            },
        },
        TestCase {
            name: "TGA rgb15rle",
            path: "tga/rgb15rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb15rle.avif",
            },
        },
        TestCase {
            name: "TGA rgb32rle",
            path: "tga/rgb32rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb32rle.avif",
            },
        },
        TestCase {
            name: "TGA xing_b32",
            path: "tga/xing_b32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/xing_b32.avif",
            },
        },
        TestCase {
            name: "TGA cbw8",
            path: "tga/cbw8.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/cbw8.avif",
            },
        },
        TestCase {
            name: "JPEG arithmetic (cat)",
            path: "jpeg/cat_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (2x2)",
            path: "jpeg/2x2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/2x2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (rainbow)",
            path: "jpeg/arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo1)",
            path: "jpeg/demo1_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo1_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo2)",
            path: "jpeg/demo2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (progressive)",
            path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.avif",
                // Arithmetic decoder seems to match libjpeg exactly, but we are doing IDCT differently, so final
                // image differs slightly.
                // TODO: Maybe switch to integer IDCT as well?
                mse_threshold: 0.9,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        }
    ];

    let name_width = test_cases.iter().map(|t| t.name.len()).max().unwrap_or(0);

    let mut any_failed = false;
    for test_case in test_cases {
        let name = test_case.name;
        match test_decode(test_case) {
            Err(e) => {
                println!("  {:<width$}  FAIL  {}", name, e, width = name_width);
                any_failed = true;
            }
            Ok(harness::TestResult::Fail(msg)) => {
                println!("  {:<width$}  FAIL  {}", name, msg, width = name_width);
                any_failed = true;
            }
            Ok(harness::TestResult::Ok { mse: None, ssim: None, psnr: None }) => {
                println!("  {:<width$}  OK", name, width = name_width);
            }
            Ok(harness::TestResult::Ok { mse, ssim, psnr }) => {
                let mse_str = mse.map(|v| format!("MSE={:.5}", v)).unwrap_or_default();
                let ssim_str = ssim.map(|v| format!("SSIM={:.6}", v)).unwrap_or_default();
                let psnr_str = psnr.map(|v| match v.is_infinite() {
                    true => "PSNR=∞ dB".to_string(),
                    false => format!("PSNR={:.2} dB", v),
                }).unwrap_or_default();
                println!("  {:<width$}  OK    {} {} {}", name, mse_str, ssim_str, psnr_str, width = name_width);
            }
        }
    }

    if any_failed {
        return Err("one or more test cases failed".into());
    }

    Ok(())
}

#[test]
#[ignore = "dev only"]
// This test is used during development for convenience for any new image formats
pub fn test_image() -> Result<(), Box<dyn std::error::Error>> {
    let in_path = r"/home/aplefull/Repos/vexel/vexel/tests/images/jpeg";
    let out_path = Path::new(in_path).with_extension("avif");
    let save = true; 

    let mut decoder = Vexel::open(in_path)?;

    match decoder.decode() {
        Ok(image) => {
            if !save {
                println!("Decoded image: {}x{}, {} frames", image.width(), image.height(), image.frames().len());
                return Ok(());
            }

            if image.frames().len() > 1 {
                let frames = image.frames();
                for (i, frame) in frames.iter().enumerate() {
                    let frame_out_path = out_path.with_file_name(format!(
                        "{}_frame{}.avif",
                        out_path.file_stem().unwrap().to_string_lossy(),
                        i
                    ));

                    let avif_data = libavif::encode_rgb8(frame.width(), frame.height(), &frame.as_rgba8())?;
                    std::fs::write(frame_out_path, avif_data.as_ref())?;
                }
            } else {
                let avif_data = libavif::encode_rgb8(image.width(), image.height(), &image.as_rgba8())?;
                std::fs::write(out_path, avif_data.as_ref())?;
            }
        }
        Err(e) => {
            println!("Error decoding image: {:?}", e);
            assert!(false);
        }
    }

    Ok(())
}

#[test]
#[ignore = "corpus bench"]
pub fn corpus_bench() -> Result<(), Box<dyn std::error::Error>> {
    load_env_file();
    let corpus_path = std::env::var("VEXEL_CORPUS")
        .map_err(|_| "VEXEL_CORPUS is not set. Add it to .env")?;
    corpus::run(&corpus_path)
}
