use std::io::Write as _;
use std::path::Path;
use std::time::Instant;

use vexel::Vexel;

struct Row {
    path: String,
    decode_ms: f64,
    convert_ms: f64,
    width: u32,
    height: u32,
    mse: Option<f64>,
    error: Option<String>,
}

pub fn run(corpus_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: Vec<std::path::PathBuf> = walkdir::WalkDir::new(corpus_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    entries.sort();

    if entries.is_empty() {
        println!("No images found in {corpus_path}");
        return Ok(());
    }

    let total = entries.len();
    println!("Found {total} images in {corpus_path}");
    println!("{:-<100}", "");
    println!(
        "{:<6}  {:<50}  {:>10}  {:>10}  {:>12}  {:>8}",
        "#", "path", "vexel (ms)", "conv (ms)", "resolution", "MSE"
    );
    println!("{:-<100}", "");

    let mut results: Vec<Row> = Vec::with_capacity(total);

    for (i, path) in entries.iter().enumerate() {
        let path_str = path.display().to_string();
        let rel = path_str
            .strip_prefix(corpus_path)
            .unwrap_or(&path_str)
            .trim_start_matches('/')
            .to_string();

        let t0 = Instant::now();
        let decoded = match Vexel::open(path) {
            Ok(mut dec) => match dec.decode() {
                Ok(img) => Ok(img),
                Err(e) => Err(format!("decode error: {e:?}")),
            },
            Err(e) => Err(format!("open error: {e:?}")),
        };
        let decode_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let counter = format!("{}/{}", i + 1, total);

        let row = match decoded {
            Err(e) => {
                println!(
                    "{:<6}  {:<50}  {:>10.2}  {:>10}  {:>12}  {}",
                    counter, rel, decode_ms, "n/a", "ERROR", e
                );
                Row {
                    path: path_str,
                    decode_ms,
                    convert_ms: 0.0,
                    width: 0,
                    height: 0,
                    mse: None,
                    error: Some(e),
                }
            }
            Ok(image) => {
                let width = image.width();
                let height = image.height();
                let vexel_pixels = image.as_rgba8();

                let (mse, convert_ms) = match decode_via_convert(path, width, height) {
                    Ok((ref_pixels, conv_ms)) => {
                        (Some(compute_mse(&vexel_pixels, &ref_pixels)), conv_ms)
                    }
                    Err(_) => (None, 0.0),
                };

                let res = format!("{}x{}", width, height);
                let mse_str = mse.map(|v: f64| format!("{:.4}", v)).unwrap_or_else(|| "n/a".into());
                println!(
                    "{:<6}  {:<50}  {:>10.2}  {:>10.2}  {:>12}  {:>8}",
                    counter, rel, decode_ms, convert_ms, res, mse_str
                );

                Row {
                    path: path_str,
                    decode_ms,
                    convert_ms,
                    width,
                    height,
                    mse,
                    error: None,
                }
            }
        };

        results.push(row);
    }

    let ok_count = results.iter().filter(|r| r.error.is_none()).count();
    let err_count = results.len() - ok_count;
    let mse_values: Vec<f64> = results.iter().filter_map(|r| r.mse).collect();
    let avg_mse = if mse_values.is_empty() {
        None
    } else {
        Some(mse_values.iter().sum::<f64>() / mse_values.len() as f64)
    };

    println!("{:-<100}", "");
    println!(
        "Total: {}  OK: {}  Errors: {}  Avg MSE: {}",
        results.len(),
        ok_count,
        err_count,
        avg_mse.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "n/a".into())
    );

    let results_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("corpus-results");
    std::fs::create_dir_all(&results_dir)?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let csv_path = results_dir.join(format!("corpus_bench_{}.csv", timestamp));
    let mut csv = std::fs::File::create(&csv_path)?;
    writeln!(csv, "path,decode_ms,convert_ms,width,height,mse,error")?;
    for row in &results {
        writeln!(
            csv,
            "{},{:.4},{:.4},{},{},{},{}",
            row.path,
            row.decode_ms,
            row.convert_ms,
            row.width,
            row.height,
            row.mse.map(|v| format!("{:.6}", v)).unwrap_or_default(),
            row.error.as_deref().unwrap_or(""),
        )?;
    }
    println!("Results saved to {}", csv_path.display());

    Ok(())
}

fn compute_mse(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a.iter().zip(b.iter()).map(|(&x, &y)| (x as f64 - y as f64).powi(2)).sum();
    sum / a.len() as f64
}

fn decode_via_convert(path: &Path, expected_width: u32, expected_height: u32) -> Result<(Vec<u8>, f64), String> {
    use std::process::Command;

    let t0 = Instant::now();
    let output = Command::new("convert")
        .arg(path)
        .arg("-strip")
        .arg("-depth")
        .arg("8")
        .arg("rgba:-")
        .output()
        .map_err(|e| format!("failed to run convert: {e}"))?;
    let convert_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if !output.status.success() {
        return Err(format!("convert exited with {}", output.status));
    }

    let expected_len = (expected_width * expected_height * 4) as usize;
    if output.stdout.len() != expected_len {
        return Err(format!(
            "convert output size mismatch: got {} bytes, expected {} ({}x{}x4)",
            output.stdout.len(),
            expected_len,
            expected_width,
            expected_height
        ));
    }

    Ok((output.stdout, convert_ms))
}
