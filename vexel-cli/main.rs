use clap::Parser;
use eframe;
use egui;
use egui::ViewportBuilder;
use glob::glob;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vexel::{Image, PixelData, Vexel};
use writer::{Writer, WriterImage, WriterImageFrame, WriterPixelData};

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

    #[arg(long, help = "Decode the image without writing to a file")]
    void: bool,
}

fn get_files(path: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".."));
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

fn get_output_path(file: &Path, output_dir: Option<&str>, format: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_stem = file
        .file_stem()
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
            .unwrap_or_else(|| Path::new(".."))
            .join(format!("{}.{}", file_stem, format))
    };

    Ok(output_path)
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

fn process_file(file: &Path, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("File: {}", file.display());

    let mut decoder = Vexel::open(file)?;

    if cli.void {
        let _ = decoder.decode();
        return Ok(());
    }

    if cli.info {
        let _ = decoder.decode();
        let info = decoder.get_info();
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
    let writer_image = image_to_writer_image(&image);
    match format {
        "pam" => {
            let pixel_data = match image.pixels() {
                PixelData::RGB8(data) => WriterPixelData::RGB8(data),
                PixelData::RGBA8(data) => WriterPixelData::RGBA8(data),
                PixelData::RGB16(data) => WriterPixelData::RGB16(data),
                PixelData::RGBA16(data) => WriterPixelData::RGBA16(data),
                PixelData::RGB32F(data) => WriterPixelData::RGB32F(data),
                PixelData::RGBA32F(data) => WriterPixelData::RGBA32F(data),
                PixelData::L1(data) => WriterPixelData::L1(data),
                PixelData::L8(data) => WriterPixelData::L8(data),
                PixelData::L16(data) => WriterPixelData::L16(data),
                PixelData::LA8(data) => WriterPixelData::LA8(data),
                PixelData::LA16(data) => WriterPixelData::LA16(data),
            };

            Writer::write_pam(&output_path, image.width(), image.height(), &pixel_data)?;
        }
        "ppm" => {
            Writer::write_ppm(&output_path, &writer_image)?;
        }
        "webp" => {
            Writer::write_webp(&output_path, &writer_image)?;
        }
        _ => {
            Writer::write_webp(&output_path, &writer_image)?;
        }
    }

    Ok(())
}

fn display_image(image: &Image) -> Result<(), Box<dyn std::error::Error>> {
    struct App {
        texture: Option<egui::TextureHandle>,
        frames: Vec<egui::ColorImage>,
        current_frame: usize,
        last_frame_time: Instant,
        frame_delays: Vec<u32>,
    }

    impl App {
        fn new(image: &Image) -> Self {
            let mut frames = Vec::new();
            let mut frame_delays = Vec::new();

            for frame in image.frames() {
                let buffer = if frame.has_alpha() {
                    frame.as_rgba8()
                } else {
                    let rgb = frame.as_rgb8();
                    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
                    for i in (0..rgb.len()).step_by(3) {
                        rgba.extend_from_slice(&[rgb[i], rgb[i + 1], rgb[i + 2], 255]);
                    }

                    rgba
                };

                frames.push(egui::ColorImage::from_rgba_unmultiplied(
                    [frame.width() as usize, frame.height() as usize],
                    &buffer,
                ));

                frame_delays.push((frame.delay() * 10).max(17));
            }

            Self {
                texture: None,
                frames,
                current_frame: 0,
                last_frame_time: Instant::now(),
                frame_delays,
            }
        }

        fn update_frame(&mut self, ctx: &egui::Context) {
            if self.frames.len() <= 1 {
                return;
            }

            let elapsed = self.last_frame_time.elapsed().as_millis() as u32;
            let current_delay = self.frame_delays[self.current_frame];

            if elapsed >= current_delay {
                self.current_frame = (self.current_frame + 1) % self.frames.len();
                self.last_frame_time = Instant::now();

                // Update texture with new frame
                if let Some(texture) = &mut self.texture {
                    texture.set(self.frames[self.current_frame].clone(), egui::TextureOptions::default());
                }

                // Request a repaint
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis((current_delay - elapsed) as u64));
            }
        }
    }

    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            let mut visuals = ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::from_rgb(0x28, 0x2c, 0x34);
            ctx.set_visuals(visuals);

            if self.texture.is_none() {
                self.texture = Some(ctx.load_texture("image", self.frames[0].clone(), egui::TextureOptions::default()));
            }

            // Update animation frame
            self.update_frame(ctx);

            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(texture) = &self.texture {
                    let available_size = ui.available_size();
                    let image_aspect = texture.aspect_ratio();
                    let window_aspect = available_size.x / available_size.y;

                    let display_size = if window_aspect > image_aspect {
                        egui::vec2(available_size.y * image_aspect, available_size.y)
                    } else {
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

    eframe::run_native("Vexel", options, Box::new(|_cc| Ok(Box::new(App::new(image)))))?;

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

        display_image(&image)?;

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
