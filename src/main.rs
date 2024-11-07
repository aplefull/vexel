extern crate vexel;

use std::path::PathBuf;
use vexel::{Decoders, Vexel};
use clap::Parser;
use glob::glob;

#[derive(Parser, Debug)]
#[clap(name = "vexel")]
struct Cli {
    #[arg(required = true)]
    path: String,

    #[arg(short, long, value_parser = ["bmp", "pgm"])]
    format: Option<String>,

    #[arg(long)]
    info: bool,
}

fn get_files(path: &str) -> Vec<String> {
    let mut files = Vec::new();
    for entry in glob(path).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                if !path.is_file() {
                    continue;
                }

                let path = match path.to_str() {
                    Some(path) => path.to_string(),
                    None => continue,
                };

                files.push(path);
            }
            Err(e) => println!("{:?}", e),
        }
    }

    files
}

fn process_file(file: &str, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("File: {}", file);

    let mut decoder = Vexel::open(file)?;

    if cli.info {
        // Decode the image to fill the decoder struct
        let _ = decoder.decode();
        
        let info = decoder.get_image_info();
        
        println!("{:?}", info);
        
        return Ok(());
    }

    let image = decoder.decode()?;
    let image_path = PathBuf::from(file);

    match &cli.format {
        Some(format) => {
            match format.as_str() {
                "bmp" => {
                    let output_path = image_path.with_extension("bmp");
                    Vexel::write_bmp(output_path, image.width(), image.height(), &image.as_rgb8())?;
                }
                "ppm" => {
                    let output_path = image_path.with_extension("ppm");
                    Vexel::write_ppm(output_path.clone(), image.width(), image.height(), &image.as_rgb8())?;
                }
                _ => {
                    let output_path = image_path.with_extension("bmp");
                    Vexel::write_bmp(output_path, image.width(), image.height(), &image.as_rgb8())?;
                }
            }
        }
        None => {
            let output_path = image_path.with_extension("bmp");
            Vexel::write_bmp(output_path, image.width(), image.height(), &image.as_rgb8())?;
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let files = get_files(&cli.path);

    for file in files {
        if let Err(err) = process_file(&file, &cli) {
            eprintln!("Error processing file: {:?}", err);
            continue;
        }
    }

    Ok(())
}
