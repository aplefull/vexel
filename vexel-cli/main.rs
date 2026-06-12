use clap::Parser;
use eframe;
use egui;
use egui::ViewportBuilder;
use glob::glob;
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vexel::{Image, LogLevel, PixelData, Vexel};
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

    #[arg(long, value_parser = ["error", "warn", "info", "debug"], default_value = "error", help = "Minimum log level to display")]
    log_level: String,
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

fn get_directory_files(path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_file() {
                files.push(entry_path);
            }
        }
    }

    files.sort_by(|a, b| match (a.file_name(), b.file_name()) {
        (Some(a_name), Some(b_name)) => a_name.cmp(b_name),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    files
}

fn collect_gui_files(path: &str) -> Vec<PathBuf> {
    let input = Path::new(path);
    if input.is_dir() {
        get_directory_files(input)
    } else {
        get_files(path)
    }
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
        println!("{}", info);
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

fn display_image(files: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    struct App {
        texture: Option<egui::TextureHandle>,
        files: Vec<PathBuf>,
        current_file_index: usize,
        frames: Vec<egui::ColorImage>,
        current_frame: usize,
        last_frame_time: Instant,
        frame_delays: Vec<u32>,
        original_image_size: [usize; 2],
        error_message: Option<String>,
    }

    impl App {
        fn image_to_frames(image: &Image) -> (Vec<egui::ColorImage>, Vec<u32>, [usize; 2]) {
            const MAX_TEXTURE_SIZE: usize = 16384;

            let original_size = [image.width() as usize, image.height() as usize];

            let mut frames = Vec::new();
            let mut frame_delays = Vec::new();

            for frame in image.frames() {
                let src_width = frame.width() as usize;
                let src_height = frame.height() as usize;

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

                let scale = (MAX_TEXTURE_SIZE as f32 / src_width as f32)
                    .min(MAX_TEXTURE_SIZE as f32 / src_height as f32)
                    .min(1.0);

                let (dst_width, dst_height, final_buffer) = if scale < 1.0 {
                    let dst_w = ((src_width as f32 * scale) as usize).max(1);
                    let dst_h = ((src_height as f32 * scale) as usize).max(1);
                    let mut dst = vec![0u8; dst_w * dst_h * 4];

                    for dy in 0..dst_h {
                        for dx in 0..dst_w {
                            let sx = ((dx as f32 + 0.5) / scale) as usize;
                            let sy = ((dy as f32 + 0.5) / scale) as usize;
                            let sx = sx.min(src_width - 1);
                            let sy = sy.min(src_height - 1);
                            let src_idx = (sy * src_width + sx) * 4;
                            let dst_idx = (dy * dst_w + dx) * 4;
                            dst[dst_idx..dst_idx + 4].copy_from_slice(&buffer[src_idx..src_idx + 4]);
                        }
                    }

                    (dst_w, dst_h, dst)
                } else {
                    (src_width, src_height, buffer)
                };

                frames.push(egui::ColorImage::from_rgba_unmultiplied(
                    [dst_width, dst_height],
                    &final_buffer,
                ));

                let delay = frame.delay();
                frame_delays.push(if delay <= 10 { 100 } else { delay }.max(17));
            }

            (frames, frame_delays, original_size)
        }

        fn new(files: Vec<PathBuf>) -> Self {
            let mut app = Self {
                texture: None,
                files,
                current_file_index: 0,
                frames: Vec::new(),
                current_frame: 0,
                last_frame_time: Instant::now(),
                frame_delays: Vec::new(),
                original_image_size: [0, 0],
                error_message: None,
            };

            let _ = app.load_current_file();

            app
        }

        fn load_image_from_path(path: &Path) -> Result<(Vec<egui::ColorImage>, Vec<u32>, [usize; 2]), String> {
            let mut decoder = Vexel::open(path).map_err(|e| e.to_string())?;
            let image = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoder.decode()))
                .map_err(|_| "decoder panicked".to_string())?
                .map_err(|e| e.to_string())?;
            Ok(Self::image_to_frames(&image))
        }

        fn load_current_file(&mut self) -> bool {
            if self.files.is_empty() {
                return false;
            }

            match Self::load_image_from_path(&self.files[self.current_file_index]) {
                Ok((frames, frame_delays, original_size)) => {
                    self.frames = frames;
                    self.frame_delays = frame_delays;
                    self.original_image_size = original_size;
                    self.current_frame = 0;
                    self.last_frame_time = Instant::now();
                    self.error_message = None;
                    let options = self.texture_options();
                    if let Some(texture) = &mut self.texture {
                        texture.set(self.frames[0].clone(), options);
                    }
                    true
                }
                Err(e) => {
                    self.frames.clear();
                    self.texture = None;
                    self.error_message = Some(e);
                    false
                }
            }
        }

        fn try_move_file_index(&mut self, step: isize) {
            if self.files.len() <= 1 {
                return;
            }

            let total = self.files.len();

            if step >= 0 {
                self.current_file_index = (self.current_file_index + 1) % total;
            } else {
                self.current_file_index = (self.current_file_index + total - 1) % total;
            }

            self.load_current_file();
        }

        fn load_files(&mut self, files: Vec<PathBuf>, start_index: usize) {
            if files.is_empty() {
                return;
            }

            self.files = files;
            self.current_file_index = start_index.min(self.files.len() - 1);
            self.load_current_file();
        }

        fn handle_dropped_files(&mut self, ctx: &egui::Context) {
            let dropped_paths: Vec<PathBuf> = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .filter_map(|file| file.path.clone())
                    .collect()
            });

            if dropped_paths.is_empty() {
                return;
            }

            let path = &dropped_paths[0];
            if path.is_dir() {
                let files = get_directory_files(path);
                self.load_files(files, 0);
            } else if path.is_file() {
                self.load_files(vec![path.clone()], 0);
            }
        }

        fn handle_keyboard_navigation(&mut self, ctx: &egui::Context) {
            let next = ctx.input(|input| {
                input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::ArrowDown)
            });
            if next {
                self.try_move_file_index(1);
            }

            let previous = ctx.input(|input| {
                input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::ArrowUp)
            });
            if previous {
                self.try_move_file_index(-1);
            }
        }

        fn texture_options(&self) -> egui::TextureOptions {
            const PIXELATED_THRESHOLD: usize = 64;
            if self.original_image_size[0] < PIXELATED_THRESHOLD || self.original_image_size[1] < PIXELATED_THRESHOLD {
                egui::TextureOptions::NEAREST
            } else {
                egui::TextureOptions::default()
            }
        }

        fn refresh_texture(&mut self, ctx: &egui::Context) {
            if self.frames.is_empty() {
                return;
            }

            if self.texture.is_none() {
                let options = self.texture_options();
                self.texture = Some(ctx.load_texture("image", self.frames[0].clone(), options));
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

                let options = self.texture_options();
                if let Some(texture) = &mut self.texture {
                    texture.set(self.frames[self.current_frame].clone(), options);
                }

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

            self.handle_dropped_files(ctx);
            self.handle_keyboard_navigation(ctx);
            self.refresh_texture(ctx);

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

                    let file_name = self.files[self.current_file_index]
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Unknown");

                    let painter = ui.painter();
                    let text_pos = egui::pos2(10.0, 10.0);
                    painter.text(
                        text_pos,
                        egui::Align2::LEFT_TOP,
                        file_name,
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );
                } else {
                    let file_name = self.files[self.current_file_index]
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Unknown");

                    let error = self
                        .error_message
                        .as_deref()
                        .unwrap_or("failed to decode image");

                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{file_name}: {error}"))
                                .color(egui::Color32::from_rgb(0xff, 0x6b, 0x6b))
                                .size(16.0),
                        );
                    });
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

    eframe::run_native("Vexel", options, Box::new(|_cc| Ok(Box::new(App::new(files)))))?;

    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let log_level = match cli.log_level.as_str() {
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warning,
        _ => LogLevel::Error,
    };
    vexel::set_log_level(log_level);

    if cli.gui {
        let files = collect_gui_files(&cli.path);
        if files.is_empty() {
            eprintln!("No files found matching path: {}", cli.path);
            return Ok(());
        }

        display_image(files)?;

        return Ok(());
    }

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
