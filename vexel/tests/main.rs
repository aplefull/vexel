mod harness;
mod corpus;
mod formats;

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

fn run_test_cases(test_cases: Vec<TestCase>) -> Result<(), Box<dyn std::error::Error>> {
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
fn test_jpeg() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::jpeg::test_cases())
}

#[test]
fn test_bmp() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::bmp::test_cases())
}

#[test]
fn test_png() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::png::test_cases())
}

#[test]
fn test_gif() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::gif::test_cases())
}

#[test]
fn test_ico() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::ico::test_cases())
}

#[test]
fn test_tga() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::tga::test_cases())
}

#[test]
fn test_jbig1() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::jbig1::test_cases())
}

#[test]
fn test_jpeg_ls() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::jpeg_ls::test_cases())
}

#[test]
fn test_netpbm() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::netpbm::test_cases())
}

#[test]
fn test_hdr() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::hdr::test_cases())
}

#[test]
fn test_tiff() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::tiff::test_cases())
}

#[test]
fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
    let mut test_cases = Vec::new();
    test_cases.extend(formats::jpeg::test_cases());
    test_cases.extend(formats::jpeg_ls::test_cases());
    test_cases.extend(formats::bmp::test_cases());
    test_cases.extend(formats::png::test_cases());
    test_cases.extend(formats::gif::test_cases());
    test_cases.extend(formats::ico::test_cases());
    test_cases.extend(formats::tga::test_cases());
    test_cases.extend(formats::jbig1::test_cases());
    test_cases.extend(formats::netpbm::test_cases());
    test_cases.extend(formats::hdr::test_cases());
    test_cases.extend(formats::tiff::test_cases());

    run_test_cases(test_cases)
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
