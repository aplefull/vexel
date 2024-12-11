extern crate vexel;

use std::fs;
use std::path::{Path, PathBuf};
use vexel::Vexel;
use clap::Parser;
use glob::glob;
use vexel::writer::Writer;
use egui;
use eframe;
use egui::ViewportBuilder;

#[derive(Parser, Debug)]
#[clap(name = "vexel")]
struct Cli {
    #[arg(required = true)]
    path: String,

    #[arg(short, long, value_parser = ["ppm", "pam", "webp"], help = "Output format")]
    format: Option<String>,

    #[arg(short = 'o', long = "output-dir", help = "Output directory for converted files")]
    output_dir: Option<String>,

    #[arg(long, help = "Display the image")]
    gui: bool,

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
    let format = cli.format.as_deref().unwrap_or("webp");
    let output_path = get_output_path(file, cli.output_dir.as_deref(), format)?;

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    println!("Writing to: {}", output_path.display());

    match format {
        "pam" => {
            Writer::write_pam(&output_path, &image)?;
        }
        "ppm" => {
            Writer::write_ppm(&output_path, &image)?;
        }
        "webp" => {
            Writer::write_webp(&output_path, &image)?;
        }
        _ => {
            Writer::write_webp(&output_path, &image)?;
        }
    }

    Ok(())
}

fn display_image(width: u32, height: u32, buffer: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    struct App {
        texture: Option<egui::TextureHandle>,
        image_data: egui::ColorImage,
    }

    impl App {
        fn new(width: u32, height: u32, buffer: &[u8]) -> Self {
            // Convert RGBA buffer to ColorImage
            let image_data = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                buffer,
            );

            Self {
                texture: None,
                image_data,
            }
        }
    }

    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            let mut visuals = ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::from_rgb(0x28, 0x2c, 0x34);
            ctx.set_visuals(visuals);

            if self.texture.is_none() {
                self.texture = Some(ctx.load_texture(
                    "image",
                    self.image_data.clone(),
                    egui::TextureOptions::default(),
                ));
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(texture) = &self.texture {
                    let available_size = ui.available_size();

                    let image_aspect = texture.aspect_ratio();
                    let window_aspect = available_size.x / available_size.y;

                    let display_size = if window_aspect > image_aspect {
                        // Window is wider than the image - fit to height
                        egui::vec2(available_size.y * image_aspect, available_size.y)
                    } else {
                        // Window is taller than the image - fit to width
                        egui::vec2(available_size.x, available_size.x / image_aspect)
                    };

                    ui.with_layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.image((texture.id(), display_size));
                        },
                    );
                }
            });
        }
    }

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder {
            min_inner_size: Some(egui::vec2(300.0, 300.0)),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "Vexel",
        options,
        Box::new(|_cc| Ok(Box::new(App::new(width, height, buffer)))),
    )?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let files = get_files(&cli.path);

    if files.is_empty() {
        eprintln!("No files found matching pattern: {}", cli.path);
        return Ok(());
    }

    if cli.gui {
        let mut decoder = Vexel::open(&files[0])?;
        let image = decoder.decode()?;
        let buffer = if image.has_alpha() {
            image.as_rgba8()
        } else {
            let rgb = image.as_rgb8();

            let mut buffer = Vec::new();

            for i in (0..rgb.len()).step_by(3) {
                buffer.push(rgb[i]);
                buffer.push(rgb[i + 1]);
                buffer.push(rgb[i + 2]);
                buffer.push(255);
            }

            buffer
        };

        display_image(image.width(), image.height(), &buffer)?;

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
