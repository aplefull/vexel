extern crate vexel;

use std::fs;
use std::path::{Path, PathBuf};
use vexel::Vexel;
use clap::Parser;
use glob::glob;

#[derive(Parser, Debug)]
#[clap(name = "vexel")]
struct Cli {
    #[arg(required = true)]
    path: String,

    #[arg(short, long, value_parser = ["bmp", "ppm"])]
    format: Option<String>,

    #[arg(short = 'o', long = "output-dir", help = "Output directory for converted files")]
    output_dir: Option<String>,

    #[arg(long)]
    info: bool,
}

fn get_files(path: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let absolute_pattern = if Path::new(path).is_relative() {
        base_dir.join(path).to_string_lossy().into_owned()
    } else {
        path.to_string()
    };

    for entry in glob(&absolute_pattern).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                if !path.is_file() {
                    continue;
                }

                files.push(path);
            }
            Err(e) => println!("{:?}", e),
        }
    }

    files
}

fn get_output_path(
    file: &Path,
    output_dir: Option<&str>,
    format: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_stem = file.file_stem()
        .ok_or("Invalid file name")?
        .to_str()
        .ok_or("Invalid file stem")?;

    let output_path = if let Some(dir) = output_dir {
        let output_dir = Path::new(dir);

        // Create output directory if it doesn't exist
        if !output_dir.exists() {
            fs::create_dir_all(output_dir)?;
        }

        // If output_dir is relative, make it relative to current directory
        let output_dir = if output_dir.is_relative() {
            std::env::current_dir()?.join(output_dir)
        } else {
            output_dir.to_path_buf()
        };

        output_dir.join(format!("{}.{}", file_stem, format))
    } else {
        // If no output directory specified, use the input file's directory
        file.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{}.{}", file_stem, format))
    };

    Ok(output_path)
}

fn process_file(file: &Path, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("File: {}", file.display());

    let mut decoder = Vexel::open(file)?;

    if cli.info {
        let _ = decoder.decode();
        let info = decoder.get_image_info();
        println!("{:?}", info);
        return Ok(());
    }

    let image = decoder.decode()?;

    // Determine output format
    let format = cli.format.as_deref().unwrap_or("bmp");
    let output_path = get_output_path(file, cli.output_dir.as_deref(), format)?;

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    println!("Writing to: {}", output_path.display());

    match format {
        "bmp" => {
            Vexel::write_bmp(&output_path, image.width(), image.height(), &image.as_rgb8())?;
        }
        "ppm" => {
            Vexel::write_ppm(&output_path, image.width(), image.height(), &image.as_rgb8())?;
        }
        _ => {
            Vexel::write_bmp(&output_path, image.width(), image.height(), &image.as_rgb8())?;
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let files = get_files(&cli.path);

    if files.is_empty() {
        eprintln!("No files found matching pattern: {}", cli.path);
        return Ok(());
    }

    for file in files {
        if let Err(err) = process_file(&file, &cli) {
            eprintln!("Error processing file: {:?}", err);
            continue;
        }
    }

    Ok(())
}