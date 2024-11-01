extern crate vexel;

use std::env;
use std::path::PathBuf;
use vexel::Vexel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().collect();

    let image_path = match args.get(1) {
        Some(path) => path,
        None => {
            println!("Usage: {} <image_path>", args[0].to_string_lossy());
            std::process::exit(0);
        }
    };

    let mut decoder = match Vexel::open(image_path) {
        Ok(decoder) => decoder,
        Err(e) => {
            eprintln!("Error opening image: {:?}", e);
            std::process::exit(1);
        }
    };

    match decoder.decode() {
        Ok(image) => {
            let image_path = PathBuf::from(image_path);
            let output_path = image_path.with_extension("ppm");

            Vexel::write_ppm(output_path.clone(), image.width(), image.height(), &image.pixels())?;

            println!("Image written to {:?}", output_path);
        }
        Err(e) => {
            eprintln!("Error decoding image: {:?}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
