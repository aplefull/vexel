mod corpus;
mod formats;
mod harness;

use std::path::Path;

use harness::*;
use vexel::{Vexel, VexelError};

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
    let total = test_cases.len();

    let mut passed = 0usize;
    let mut failures: Vec<(&str, String)> = Vec::new();

    for test_case in test_cases {
        let name = test_case.name;
        match test_decode(test_case) {
            Err(e) => {
                println!("  {:<width$}  FAIL  {}", name, e, width = name_width);
                failures.push((name, e.to_string()));
            }
            Ok(harness::TestResult::Fail(msg)) => {
                println!("  {:<width$}  FAIL  {}", name, msg, width = name_width);
                failures.push((name, msg.to_string()));
            }
            Ok(harness::TestResult::Ok { mse: None, ssim: None, psnr: None }) => {
                println!("  {:<width$}  OK", name, width = name_width);
                passed += 1;
            }
            Ok(harness::TestResult::Ok { mse, ssim, psnr }) => {
                let mse_str = mse.map(|v| format!("MSE={:.5}", v)).unwrap_or_default();
                let ssim_str = ssim.map(|v| format!("SSIM={:.6}", v)).unwrap_or_default();
                let psnr_str = psnr
                    .map(|v| match v.is_infinite() {
                        true => "PSNR=∞ dB".to_string(),
                        false => format!("PSNR={:.2} dB", v),
                    })
                    .unwrap_or_default();
                println!("  {:<width$}  OK    {} {} {}", name, mse_str, ssim_str, psnr_str, width = name_width);
                passed += 1;
            }
        }
    }

    println!();
    println!("  {}/{} passed", passed, total);

    if !failures.is_empty() {
        println!();
        println!("  Failed:");
        for (name, reason) in &failures {
            println!("    {} — {}", name, reason);
        }
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
fn test_dng() -> Result<(), Box<dyn std::error::Error>> {
    run_test_cases(formats::dng::test_cases())
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
    test_cases.extend(formats::dng::test_cases());

    run_test_cases(test_cases)
}

#[test]
fn test_icc() -> Result<(), Box<dyn std::error::Error>> {
    use vexel::icc::ICCTagData;

    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let icc_dir = tests_dir.join("icc");
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(&icc_dir)?;
    paths.extend(
        entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("icc"))),
    );
    paths.sort();

    assert!(!paths.is_empty(), "no ICC sample profiles found in {}", icc_dir.display());

    let mut failures = Vec::new();
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = std::fs::read(path)?;

        let profile = match vexel::parse_icc(&data) {
            Ok(profile) => profile,
            Err(e) => {
                println!("  {:<45} FAIL  {}", name, e);
                failures.push(name);
                continue;
            }
        };

        let unknown_count = profile.tags.values().filter(|tag| matches!(tag, ICCTagData::Unknown(_))).count();

        println!(
            "  {:<45} OK    v{} {} tags ({} unknown)",
            name, profile.header.version, profile.tag_table.tag_count, unknown_count
        );

        if profile.header.signature != "acsp" {
            println!("  {:<45} FAIL  bad signature '{}'", name, profile.header.signature);
            failures.push(name.clone());
        }
        if profile.tags.is_empty() {
            println!("  {:<45} FAIL  no tags parsed", name);
            failures.push(name.clone());
        }
        if unknown_count > 0 {
            let unknown: Vec<_> = profile
                .tags
                .iter()
                .filter_map(|(sig, tag)| match tag {
                    ICCTagData::Unknown(u) => Some(format!("{} ({})", sig, u.type_signature)),
                    _ => None,
                })
                .collect();
            println!("  {:<45}       unknown tags: {}", "", unknown.join(", "));
        }
    }

    let srgb_v2 = std::fs::read(icc_dir.join("sRGB-v2-2014.icc"))?;
    let profile = vexel::parse_icc(&srgb_v2)?;
    assert_eq!(profile.header.color_space, "RGB");
    assert_eq!(profile.header.pcs, "XYZ");
    assert!(matches!(profile.tags.get("rTRC"), Some(ICCTagData::Curve(_))));
    let Some(ICCTagData::XYZ(wtpt)) = profile.tags.get("wtpt") else {
        panic!("sRGB-v2-2014.icc should contain a white point tag");
    };
    assert!((wtpt[0].x - 0.9642).abs() < 0.001);
    assert!((wtpt[0].y - 1.0).abs() < 0.001);
    assert!((wtpt[0].z - 0.8249).abs() < 0.001);

    let srgb_v4 = std::fs::read(icc_dir.join("sRGB-v4-ICC_preference.icc"))?;
    let profile = vexel::parse_icc(&srgb_v4)?;
    let Some(ICCTagData::LutAToB(a2b)) = profile.tags.get("A2B0") else {
        panic!("sRGB-v4-ICC_preference.icc should contain an A2B0 lut");
    };
    assert_eq!(a2b.input_channels, 3);
    assert_eq!(a2b.output_channels, 3);
    assert!(!a2b.b_curves.is_empty());
    if let Some(clut) = &a2b.clut {
        let expected: usize = clut.grid_points.iter().take(3).map(|&p| p as usize).product::<usize>() * 3;
        assert_eq!(clut.values.len(), expected);
    }
    assert!(matches!(profile.tags.get("B2A0"), Some(ICCTagData::LutBToA(_))));
    assert!(matches!(profile.tags.get("desc"), Some(ICCTagData::MultiLocalizedUnicode(_))));

    let crayons = std::fs::read(icc_dir.join("crayons.icc"))?;
    let profile = vexel::parse_icc(&crayons)?;
    assert!(matches!(profile.tags.get("chrm"), Some(ICCTagData::Chromaticity(_))));
    assert!(matches!(profile.tags.get("desc"), Some(ICCTagData::MultiLocalizedUnicode(_))));
    assert!(matches!(profile.tags.get("chad"), Some(ICCTagData::S15Fixed16Array(_))));

    let spectral = std::fs::read(icc_dir.join("Spec400_10_700-IllumA-Abs_2deg.icc"))?;
    let profile = vexel::parse_icc(&spectral)?;
    assert!(profile.header.spectral_pcs.is_some());
    assert!(profile.header.spectral_range.is_some());

    let gbd_path = icc_dir.join("sRGB_D65_MAT.icc");
    if gbd_path.exists() {
        let profile = vexel::parse_icc(&std::fs::read(&gbd_path)?)?;
        let Some(ICCTagData::GamutBoundary(gbd)) = profile.tags.get("gbd1") else {
            panic!("sRGB_D65_MAT.icc should contain a gamut boundary tag");
        };
        assert!(!gbd.triangles.is_empty());
        assert_eq!(gbd.pcs_values.len(), gbd.pcs_values.len() / gbd.pcs_channels as usize * gbd.pcs_channels as usize);
    }

    let mut container = srgb_v2[..128].to_vec();
    let embedded_tag: Vec<u8> = [b"ICCp".as_slice(), &[0u8; 4], &srgb_v2].concat();
    container.extend_from_slice(&1u32.to_be_bytes());
    container.extend_from_slice(b"embd");
    container.extend_from_slice(&144u32.to_be_bytes());
    container.extend_from_slice(&(embedded_tag.len() as u32).to_be_bytes());
    container.extend_from_slice(&embedded_tag);
    let profile = vexel::parse_icc(&container)?;
    let Some(ICCTagData::EmbeddedProfile(inner)) = profile.tags.get("embd") else {
        panic!("synthetic container should contain an embedded profile");
    };
    assert_eq!(inner.header.color_space, "RGB");
    assert!(matches!(inner.tags.get("rTRC"), Some(ICCTagData::Curve(_))));

    for path in &paths {
        let data = std::fs::read(path)?;
        for len in [0, 64, 127, 128, 132, data.len() / 2, data.len().saturating_sub(7)] {
            let _ = vexel::parse_icc(&data[..len.min(data.len())]);
        }
        let mut corrupted = data.clone();
        for byte in corrupted.iter_mut().skip(128).step_by(97) {
            *byte = !*byte;
        }
        let _ = vexel::parse_icc(&corrupted);
    }

    assert!(vexel::parse_icc(&[0u8; 64]).is_err());

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} ICC profiles failed to parse: {}", failures.len(), failures.join(", ")).into())
    }
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

                    let avif_data = libavif::encode_rgb8(frame.width(), frame.height(), &frame.to_rgba8())?;
                    std::fs::write(frame_out_path, avif_data.as_ref())?;
                }
            } else {
                let avif_data = libavif::encode_rgb8(image.width(), image.height(), &image.to_rgba8())?;
                std::fs::write(out_path, avif_data.as_ref())?;
            }
        }
        Err(e) => {
            panic!("Error decoding image: {:?}", e);
        }
    }

    Ok(())
}

#[test]
fn test_fuzz_artifacts() -> Result<(), Box<dyn std::error::Error>> {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/artifacts");

    if !artifacts_dir.exists() {
        return Ok(());
    }

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for entry in walkdir::WalkDir::new(&artifacts_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let data = std::fs::read(path)?;
        let name = path.strip_prefix(&artifacts_dir).unwrap_or(path).display().to_string();

        checked += 1;

        let cursor = std::io::Cursor::new(data);
        let start = std::time::Instant::now();
        if let Ok(mut decoder) = Vexel::new(cursor) {
            if let Err(VexelError::Panic(msg)) = decoder.decode() {
                let elapsed = start.elapsed();
                println!("  FAIL  {}  (panic: {})  [{:.1}ms]", name, msg, elapsed.as_secs_f64() * 1000.0);
                failures.push(name);
                continue;
            }
        }
        let elapsed = start.elapsed();

        println!("  OK    {}  [{:.1}ms]", name, elapsed.as_secs_f64() * 1000.0);
    }

    println!();
    println!("  {}/{} artifacts passed", checked - failures.len(), checked);

    if !failures.is_empty() {
        return Err(format!("{} tests failed: {}", failures.len(), failures.join(", ")).into());
    }

    Ok(())
}

#[test]
#[ignore = "corpus bench"]
pub fn corpus_bench() -> Result<(), Box<dyn std::error::Error>> {
    load_env_file();
    let corpus_path = std::env::var("VEXEL_CORPUS").map_err(|_| "VEXEL_CORPUS is not set. Add it to .env")?;
    corpus::run(&corpus_path)
}
