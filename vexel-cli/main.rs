mod writer;

use clap::Parser;
use libc::{rlimit, setrlimit, RLIMIT_AS};
use eframe;
use egui;
use egui::ViewportBuilder;
use glob::glob;
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vexel::{Image, LogLevel, PixelData, Vexel};
use writer::Writer;

const AFTER_HELP: &str = "\
The PATH argument supports glob patterns (e.g. \"*.png\") to process multiple files at once. When no
output format is specified, files are written as JXL next to the source with the same name. Use
--output-dir to specify an output directory, or --output to specify an exact path for a single file.

Examples:
  vexel image.png
  vexel -f webp image.png
  vexel image.png -O output.webp
  vexel \"*.png\" -f webp -o ./converted
  vexel --frames -f webp image.gif
  vexel --info image.png
  vexel --gui image.png
  vexel --gui ./images\
";

#[derive(Parser, Debug)]
#[clap(name = "vexel", after_help = AFTER_HELP)]
struct Cli {
    #[arg(required = true)]
    path: String,

    #[arg(short, long, value_parser = ["ppm", "pam", "webp", "jxl"], help = "Output format [default: jxl]")]
    format: Option<String>,

    #[arg(short = 'o', long = "output-dir", help = "Output directory for converted files")]
    output_dir: Option<String>,

    #[arg(short = 'O', long = "output", help = "Output file path (single file only)", conflicts_with = "output_dir")]
    output: Option<String>,

    #[arg(long, help = "Display the image")]
    gui: bool,

    #[arg(long, help = "Print format-specific metadata for the image")]
    info: bool,

    #[arg(long, help = "Decode the image without writing to a file")]
    void: bool,

    #[arg(long, help = "Write each frame as a separate file")]
    frames: bool,

    #[arg(long, value_parser = ["error", "warn", "info", "debug"], default_value = "error", help = "Minimum log level to display")]
    log_level: String,

    #[arg(long, default_value = "2048", help = "Maximum memory usage in MiB (0 = unlimited). Decoder will abort if this limit is exceeded.")]
    memory_limit: u64,
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

fn collect_gui_files(path: &str) -> (Vec<PathBuf>, usize) {
    let input = Path::new(path);
    if input.is_dir() {
        (get_directory_files(input), 0)
    } else {
        let resolved = if input.is_relative() {
            std::env::current_dir().unwrap_or_default().join(input)
        } else {
            input.to_path_buf()
        };
        let parent = resolved.parent().unwrap_or(Path::new("."));
        let files = get_directory_files(parent);
        let start = files.iter().position(|f| f == &resolved).unwrap_or(0);
        (files, start)
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

    let format = cli.format.as_deref().unwrap_or("jxl");
    let output_path = if let Some(output) = cli.output.as_deref() {
        let p = Path::new(output);
        if p.is_relative() {
            std::env::current_dir()?.join(p)
        } else {
            p.to_path_buf()
        }
    } else {
        get_output_path(file, cli.output_dir.as_deref(), format)?
    };

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    println!("Writing to: {}", output_path.display());
    if cli.frames {
        Writer::write_frames(&output_path, format, &image)?;
    } else {
        Writer::write_image(&image, format, &output_path)?;
    }

    Ok(())
}

fn normalize_pixels(pixels: PixelData) -> PixelData {
    fn color_channels(step: usize) -> usize {
        if step == 4 || step == 2 { step - 1 } else { step }
    }

    fn norm_u8(mut p: Vec<u8>, step: usize) -> Vec<u8> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (u8::MAX, u8::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] { lo = lo.min(v); hi = hi.max(v); }
        }
        if hi > lo {
            let r = (hi - lo) as f32;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] { *v = ((*v - lo) as f32 / r * 255.0).round() as u8; }
            }
        }
        p
    }

    fn norm_u16(mut p: Vec<u16>, step: usize) -> Vec<u16> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (u16::MAX, u16::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] { lo = lo.min(v); hi = hi.max(v); }
        }
        if hi > lo {
            let r = (hi - lo) as f32;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] { *v = ((*v - lo) as f32 / r * 65535.0).round() as u16; }
            }
        }
        p
    }

    fn norm_f32(mut p: Vec<f32>, step: usize) -> Vec<f32> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] { if v < lo { lo = v; } if v > hi { hi = v; } }
        }
        if hi > lo {
            let r = hi - lo;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] { *v = (*v - lo) / r; }
            }
        }
        p
    }

    fn norm_f64(mut p: Vec<f64>, step: usize) -> Vec<f64> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] { if v < lo { lo = v; } if v > hi { hi = v; } }
        }
        if hi > lo {
            let r = hi - lo;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] { *v = (*v - lo) / r; }
            }
        }
        p
    }
    
    match pixels {
        PixelData::RGB8(p)    => PixelData::RGB8(norm_u8(p, 3)),
        PixelData::RGBA8(p)   => PixelData::RGBA8(norm_u8(p, 4)),
        PixelData::RGB16(p)   => PixelData::RGB16(norm_u16(p, 3)),
        PixelData::RGBA16(p)  => PixelData::RGBA16(norm_u16(p, 4)),
        PixelData::RGB32F(p)  => PixelData::RGB32F(norm_f32(p, 3)),
        PixelData::RGBA32F(p) => PixelData::RGBA32F(norm_f32(p, 4)),
        PixelData::RGB64F(p)  => PixelData::RGB64F(norm_f64(p, 3)),
        PixelData::RGBA64F(p) => PixelData::RGBA64F(norm_f64(p, 4)),
        PixelData::L8(p)      => PixelData::L8(norm_u8(p, 1)),
        PixelData::LA8(p)     => PixelData::LA8(norm_u8(p, 2)),
        PixelData::L16(p)     => PixelData::L16(norm_u16(p, 1)),
        PixelData::LA16(p)    => PixelData::LA16(norm_u16(p, 2)),
        PixelData::L32F(p)    => PixelData::L32F(norm_f32(p, 1)),
        PixelData::LA32F(p)   => PixelData::LA32F(norm_f32(p, 2)),
        PixelData::L64F(p)    => PixelData::L64F(norm_f64(p, 1)),
        PixelData::LA64F(p)   => PixelData::LA64F(norm_f64(p, 2)),
        other => other,
    }
}

fn display_image(files: Vec<PathBuf>, start_index: usize) -> Result<(), Box<dyn std::error::Error>> {
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
        panel_open: bool,
        pixelated: bool,
        checkered_bg: bool,
        normalize: bool,
    }

    impl App {
        fn image_to_frames(image: &Image, normalize: bool) -> (Vec<egui::ColorImage>, Vec<u32>, [usize; 2]) {
            const MAX_TEXTURE_SIZE: usize = 16384;

            let original_size = [image.width() as usize, image.height() as usize];

            let mut frames = Vec::new();
            let mut frame_delays = Vec::new();

            for frame in image.frames() {
                let src_width = frame.width() as usize;
                let src_height = frame.height() as usize;

                let pixels = if normalize {
                    normalize_pixels(frame.pixels().clone())
                } else {
                    frame.pixels().clone()
                };

                let buffer = if frame.has_alpha() {
                    pixels.into_rgba8().as_bytes().to_vec()
                } else {
                    let rgb = pixels.into_rgb8().as_bytes().to_vec();
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

        fn is_small_image(size: [usize; 2]) -> bool {
            size[0] < 64 || size[1] < 64
        }

        fn new(files: Vec<PathBuf>, start_index: usize) -> Self {
            let mut app = Self {
                texture: None,
                files,
                current_file_index: start_index,
                frames: Vec::new(),
                current_frame: 0,
                last_frame_time: Instant::now(),
                frame_delays: Vec::new(),
                original_image_size: [0, 0],
                error_message: None,
                panel_open: true,
                pixelated: false,
                checkered_bg: true,
                normalize: false,
            };

            let _ = app.load_current_file();

            app
        }

        fn load_image_from_path(path: &Path, normalize: bool) -> Result<(Vec<egui::ColorImage>, Vec<u32>, [usize; 2]), String> {
            let mut decoder = Vexel::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
            let image = decoder.decode().map_err(|e| format!("{}: {}", path.display(), e))?;
            Ok(Self::image_to_frames(&image, normalize))
        }

        fn load_current_file(&mut self) -> bool {
            if self.files.is_empty() {
                return false;
            }

            match Self::load_image_from_path(&self.files[self.current_file_index], self.normalize) {
                Ok((frames, frame_delays, original_size)) => {
                    self.frames = frames;
                    self.frame_delays = frame_delays;
                    self.original_image_size = original_size;
                    self.pixelated = Self::is_small_image(original_size);
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
            if self.pixelated {
                egui::TextureOptions::NEAREST
            } else {
                egui::TextureOptions::LINEAR
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
            let panel_bg = egui::Color32::from_rgb(0x1e, 0x22, 0x27);
            let main_bg = egui::Color32::from_rgb(0x28, 0x2c, 0x34);
            let separator_color = egui::Color32::from_rgb(0x3a, 0x3f, 0x4b);
            let label_color = egui::Color32::from_rgb(0x7a, 0x84, 0x96);
            let value_color = egui::Color32::WHITE;
            let handle_color = egui::Color32::from_rgb(0x4a, 0x50, 0x5e);
            let handle_hover_color = egui::Color32::from_rgb(0x6a, 0x72, 0x82);

            let mut visuals = ctx.style().visuals.clone();
            visuals.panel_fill = main_bg;
            visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x3a, 0x3f, 0x4b);
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x4a, 0x50, 0x5e);
            visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x52, 0x58, 0x68);
            ctx.set_visuals(visuals);

            self.handle_dropped_files(ctx);
            self.handle_keyboard_navigation(ctx);
            self.refresh_texture(ctx);
            self.update_frame(ctx);

            let file_name = self.files[self.current_file_index]
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string();

            if self.panel_open {
                let panel_frame = egui::Frame {
                    fill: panel_bg,
                    inner_margin: egui::Margin { left: 20.0, right: 16.0, top: 16.0, bottom: 16.0 },
                    ..Default::default()
                };

                egui::SidePanel::right("info_panel")
                    .resizable(false)
                    .exact_width(220.0)
                    .frame(panel_frame)
                    .show(ctx, |ui| {
                        let add_row = |ui: &mut egui::Ui, label: &str, value: &str| {
                            ui.label(egui::RichText::new(label).color(label_color).size(11.0));
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(value).color(value_color).size(13.0));
                            ui.add_space(10.0);
                        };

                        add_row(ui, "FILENAME", &file_name);

                        let [w, h] = self.original_image_size;
                        add_row(ui, "DIMENSIONS", &format!("{w} × {h}"));

                        let frame_count = self.frames.len();
                        if frame_count > 1 {
                            add_row(ui, "FRAMES", &frame_count.to_string());
                        }

                        ui.add_space(4.0);
                        ui.painter().hline(
                            ui.available_rect_before_wrap().x_range(),
                            ui.cursor().top(),
                            egui::Stroke::new(1.0, separator_color),
                        );
                        ui.add_space(12.0);

                        ui.label(egui::RichText::new("DISPLAY").color(label_color).size(11.0));
                        ui.add_space(6.0);

                        let pixelated_changed = ui
                            .checkbox(
                                &mut self.pixelated,
                                egui::RichText::new("Pixelated").color(value_color).size(13.0),
                            )
                            .changed();

                        if pixelated_changed {
                            let options = self.texture_options();
                            if let Some(texture) = &mut self.texture {
                                texture.set(self.frames[self.current_frame].clone(), options);
                            }
                        }

                        ui.add_space(4.0);
                        ui.checkbox(
                            &mut self.checkered_bg,
                            egui::RichText::new("Checkered background").color(value_color).size(13.0),
                        );

                        ui.add_space(4.0);
                        let normalize_changed = ui
                            .checkbox(
                                &mut self.normalize,
                                egui::RichText::new("Normalize").color(value_color).size(13.0),
                            )
                            .changed();

                        if normalize_changed {
                            self.load_current_file();
                        }

                        let panel_rect = ui.min_rect();
                        let handle_width = 4.0;
                        let handle_rect = egui::Rect::from_min_size(
                            egui::pos2(panel_rect.left() - 20.0, panel_rect.top()),
                            egui::vec2(handle_width, panel_rect.height()),
                        );
                        let handle_id = ui.id().with("panel_handle");
                        let handle_resp = ui.interact(handle_rect, handle_id, egui::Sense::click());
                        let bar_color = if handle_resp.hovered() { handle_hover_color } else { handle_color };
                        let grip_h = 32.0;
                        let grip_y = handle_rect.center().y - grip_h / 2.0;
                        let bar_rect = egui::Rect::from_min_size(
                            egui::pos2(handle_rect.left(), grip_y),
                            egui::vec2(handle_width, grip_h),
                        );
                        ui.painter().rect_filled(bar_rect, egui::Rounding::same(2.0), bar_color);
                        if handle_resp.clicked() {
                            self.panel_open = false;
                        }
                        if handle_resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    });
            } else {
                egui::SidePanel::right("handle_panel")
                    .resizable(false)
                    .exact_width(12.0)
                    .frame(egui::Frame {
                        fill: panel_bg,
                        inner_margin: egui::Margin::ZERO,
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        let rect = ui.available_rect_before_wrap();
                        let handle_width = 4.0;
                        let handle_x = rect.left() + (rect.width() - handle_width) / 2.0;
                        let grip_h = 32.0;
                        let grip_y = rect.center().y - grip_h / 2.0;
                        let bar_rect = egui::Rect::from_min_size(
                            egui::pos2(handle_x, grip_y),
                            egui::vec2(handle_width, grip_h),
                        );
                        let handle_id = ui.id().with("collapsed_handle");
                        let handle_resp = ui.interact(rect, handle_id, egui::Sense::click());
                        let bar_color = if handle_resp.hovered() { handle_hover_color } else { handle_color };
                        ui.painter().rect_filled(bar_rect, egui::Rounding::same(2.0), bar_color);
                        if handle_resp.clicked() {
                            self.panel_open = true;
                        }
                        if handle_resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    });
            }

            let checkered_bg = self.checkered_bg;
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

                    if checkered_bg {
                        let panel_rect = ui.available_rect_before_wrap();
                        let image_min = egui::pos2(
                            panel_rect.center().x - display_size.x / 2.0,
                            panel_rect.center().y - display_size.y / 2.0,
                        );
                        let image_rect = egui::Rect::from_min_size(image_min, display_size);
                        let tile = 8.0;
                        let light = egui::Color32::from_rgb(0xcc, 0xcc, 0xcc);
                        let dark = egui::Color32::from_rgb(0x88, 0x88, 0x88);
                        let col_start = (image_rect.left() / tile).floor() as i32;
                        let col_end = (image_rect.right() / tile).ceil() as i32;
                        let row_start = (image_rect.top() / tile).floor() as i32;
                        let row_end = (image_rect.bottom() / tile).ceil() as i32;
                        let painter = ui.painter();
                        for row in row_start..row_end {
                            for col in col_start..col_end {
                                let color = if (row + col) % 2 == 0 { light } else { dark };
                                let cell_rect = egui::Rect::from_min_size(
                                    egui::pos2(col as f32 * tile, row as f32 * tile),
                                    egui::vec2(tile, tile),
                                );
                                let clipped = cell_rect.intersect(image_rect);
                                if !clipped.is_negative() {
                                    painter.rect_filled(clipped, egui::Rounding::ZERO, color);
                                }
                            }
                        }
                    }

                    ui.with_layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.image((texture.id(), display_size));
                        },
                    );
                } else {
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

    eframe::run_native("Vexel", options, Box::new(move |_cc| Ok(Box::new(App::new(files, start_index)))))?;

    Ok(())
}

fn apply_memory_limit(limit_mib: u64) {
    if limit_mib == 0 {
        return;
    }

    let limit_bytes = limit_mib.saturating_mul(1024 * 1024);
    let rl = rlimit {
        rlim_cur: limit_bytes,
        rlim_max: limit_bytes,
    };

    unsafe {
        if setrlimit(RLIMIT_AS, &rl) != 0 {
            eprintln!("Warning: failed to set memory limit");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    apply_memory_limit(cli.memory_limit);

    let log_level = match cli.log_level.as_str() {
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warning,
        _ => LogLevel::Error,
    };
    vexel::set_log_level(log_level);

    if cli.gui {
        let (files, start_index) = collect_gui_files(&cli.path);
        if files.is_empty() {
            eprintln!("No files found matching path: {}", cli.path);
            return Ok(());
        }

        display_image(files, start_index)?;

        return Ok(());
    }

    let files = get_files(&cli.path);

    if files.is_empty() {
        eprintln!("No files found matching pattern: {}", cli.path);
        return Ok(());
    }

    if cli.output.is_some() && files.len() > 1 {
        eprintln!("--output cannot be used with multiple files; use --output-dir instead");
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
