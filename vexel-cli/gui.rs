use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::ViewportBuilder;
use vexel::{Frames, Image, ImageFormat, ImageFrame, PixelData, Vexel};

struct Page {
    frames: Vec<Arc<egui::ColorImage>>,
    delays: Vec<u32>,
    size: [usize; 2],
}

fn ensure_locale_initialized() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| unsafe {
        let empty = std::ffi::CString::new("").unwrap();
        libc::setlocale(libc::LC_COLLATE, empty.as_ptr());
    });
}

fn collate_key(s: &str) -> Vec<u8> {
    ensure_locale_initialized();
    let cstr = std::ffi::CString::new(s).unwrap_or_default();
    unsafe {
        let len = libc::strxfrm(std::ptr::null_mut(), cstr.as_ptr(), 0);
        let mut buf = vec![0u8; len + 1];
        libc::strxfrm(buf.as_mut_ptr() as *mut libc::c_char, cstr.as_ptr(), buf.len());
        buf.truncate(len);
        buf
    }
}

fn collate_key_for_filename(s: &str) -> Vec<u8> {
    const SENTINEL: &[u8] = b"\x01\x01\x01";

    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() * 2);
    let mut trailing = Vec::new();

    let mut prev = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'.' {
            if prev != i {
                result.extend(collate_key(&s[prev..i]));
            }
            result.extend(SENTINEL);
            result.push(1);
            i += 1;
            prev = i;
        } else if c.is_ascii_digit() {
            if prev != i {
                result.extend(collate_key(&s[prev..i]));
            }
            result.extend(SENTINEL);
            result.push(2);
            prev = i;

            let mut leading_zeros = 0i32;
            let mut digits = if c == b'0' { 0 } else { 1 };
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'0' && digits == 0 {
                    leading_zeros += 1;
                } else if bytes[i].is_ascii_digit() {
                    digits += 1;
                } else {
                    break;
                }
                i += 1;
            }
            if digits == 0 {
                digits += 1;
                leading_zeros -= 1;
            }

            result.extend(std::iter::repeat_n(b':', (digits - 1) as usize));

            if leading_zeros > 0 {
                trailing.push(leading_zeros as u8);
                prev += leading_zeros as usize;
            }

            result.extend(&bytes[prev..i]);
            prev = i;
        } else {
            i += 1;
        }
    }

    if prev != i {
        result.extend(collate_key(&s[prev..i]));
    }

    result.extend(trailing);
    result
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    collate_key_for_filename(a).cmp(&collate_key_for_filename(b))
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
        (Some(a_name), Some(b_name)) => natural_cmp(&a_name.to_string_lossy(), &b_name.to_string_lossy()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    files
}

pub fn collect_gui_files(path: &str) -> (Vec<PathBuf>, usize) {
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

fn normalize_pixels(pixels: PixelData) -> PixelData {
    fn color_channels(step: usize) -> usize {
        if step == 4 || step == 2 {
            step - 1
        } else {
            step
        }
    }

    fn norm_u8(mut p: Vec<u8>, step: usize) -> Vec<u8> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (u8::MAX, u8::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if hi > lo {
            let r = (hi - lo) as f32;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] {
                    *v = ((*v - lo) as f32 / r * 255.0).round() as u8;
                }
            }
        }
        p
    }

    fn norm_u16(mut p: Vec<u16>, step: usize) -> Vec<u16> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (u16::MAX, u16::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if hi > lo {
            let r = (hi - lo) as f32;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] {
                    *v = ((*v - lo) as f32 / r * 65535.0).round() as u16;
                }
            }
        }
        p
    }

    fn norm_f32(mut p: Vec<f32>, step: usize) -> Vec<f32> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] {
                if v < lo {
                    lo = v;
                }
                if v > hi {
                    hi = v;
                }
            }
        }
        if hi > lo {
            let r = hi - lo;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] {
                    *v = (*v - lo) / r;
                }
            }
        }
        p
    }

    fn norm_f64(mut p: Vec<f64>, step: usize) -> Vec<f64> {
        let n = color_channels(step);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for c in p.chunks_exact(step) {
            for &v in &c[..n] {
                if v < lo {
                    lo = v;
                }
                if v > hi {
                    hi = v;
                }
            }
        }
        if hi > lo {
            let r = hi - lo;
            for c in p.chunks_exact_mut(step) {
                for v in &mut c[..n] {
                    *v = (*v - lo) / r;
                }
            }
        }
        p
    }

    match pixels {
        PixelData::RGB8(p) => PixelData::RGB8(norm_u8(p, 3)),
        PixelData::RGBA8(p) => PixelData::RGBA8(norm_u8(p, 4)),
        PixelData::RGB16(p) => PixelData::RGB16(norm_u16(p, 3)),
        PixelData::RGBA16(p) => PixelData::RGBA16(norm_u16(p, 4)),
        PixelData::RGB32F(p) => PixelData::RGB32F(norm_f32(p, 3)),
        PixelData::RGBA32F(p) => PixelData::RGBA32F(norm_f32(p, 4)),
        PixelData::RGB64F(p) => PixelData::RGB64F(norm_f64(p, 3)),
        PixelData::RGBA64F(p) => PixelData::RGBA64F(norm_f64(p, 4)),
        PixelData::L8(p) => PixelData::L8(norm_u8(p, 1)),
        PixelData::LA8(p) => PixelData::LA8(norm_u8(p, 2)),
        PixelData::L16(p) => PixelData::L16(norm_u16(p, 1)),
        PixelData::LA16(p) => PixelData::LA16(norm_u16(p, 2)),
        PixelData::L32F(p) => PixelData::L32F(norm_f32(p, 1)),
        PixelData::LA32F(p) => PixelData::LA32F(norm_f32(p, 2)),
        PixelData::L64F(p) => PixelData::L64F(norm_f64(p, 1)),
        PixelData::LA64F(p) => PixelData::LA64F(norm_f64(p, 2)),
        other => other,
    }
}

struct Thumbnail {
    image: Arc<egui::ColorImage>,
    size: [usize; 2],
}

struct App {
    texture: Option<egui::TextureHandle>,
    checker_texture: Option<egui::TextureHandle>,
    files: Vec<PathBuf>,
    current_file_index: usize,
    pages: Vec<Page>,
    current_page: usize,
    current_frame: usize,
    last_frame_time: Instant,
    thumbnails: Vec<Thumbnail>,
    viewing_thumbnail: Option<usize>,
    error_message: Option<String>,
    panel_open: bool,
    pixelated: bool,
    checkered_bg: bool,
    normalize: bool,
    limits: vexel::Limits,
}

impl App {
    fn frame_to_texture(frame: ImageFrame, normalize: bool) -> Arc<egui::ColorImage> {
        const MAX_TEXTURE_SIZE: usize = 16384;

        let src_width = frame.width() as usize;
        let src_height = frame.height() as usize;
        let has_alpha = frame.has_alpha();

        let mut pixels = frame.into_pixels();
        if normalize {
            pixels = normalize_pixels(pixels);
        }

        let buffer = pixels.into_rgba8_bytes();

        let scale =
            (MAX_TEXTURE_SIZE as f32 / src_width as f32).min(MAX_TEXTURE_SIZE as f32 / src_height as f32).min(1.0);

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

        let color_image = if has_alpha {
            egui::ColorImage::from_rgba_unmultiplied([dst_width, dst_height], &final_buffer)
        } else {
            egui::ColorImage::from_rgba_premultiplied([dst_width, dst_height], &final_buffer)
        };

        Arc::new(color_image)
    }

    fn frame_delay(delay: u32) -> u32 {
        if delay <= 10 { 100 } else { delay }.max(17)
    }

    fn image_to_page(image: Image, normalize: bool) -> Page {
        let original_size = [image.width() as usize, image.height() as usize];
        let frame = ImageFrame::new_still(image.width(), image.height(), image.into_pixels());

        Page {
            frames: vec![Self::frame_to_texture(frame, normalize)],
            delays: vec![Self::frame_delay(0)],
            size: original_size,
        }
    }

    fn frames_to_page(frames: Frames<'_>, normalize: bool) -> Page {
        let mut textures = Vec::new();
        let mut delays = Vec::new();
        let mut size = [0usize, 0usize];

        for frame in frames {
            let Ok(frame) = frame else { break };

            if textures.is_empty() {
                size = [frame.width() as usize, frame.height() as usize];
            }

            delays.push(Self::frame_delay(frame.delay()));
            textures.push(Self::frame_to_texture(frame, normalize));
        }

        Page { frames: textures, delays, size }
    }

    fn thumbnail_from_image(image: Image, normalize: bool) -> Option<Thumbnail> {
        let page = Self::image_to_page(image, normalize);
        page.frames.into_iter().next().map(|image| Thumbnail { image, size: page.size })
    }

    fn is_small_image(size: [usize; 2]) -> bool {
        size[0] < 64 || size[1] < 64
    }

    fn new(files: Vec<PathBuf>, start_index: usize, limits: vexel::Limits) -> Self {
        let mut app = Self {
            texture: None,
            checker_texture: None,
            files,
            current_file_index: start_index,
            pages: Vec::new(),
            current_page: 0,
            current_frame: 0,
            last_frame_time: Instant::now(),
            thumbnails: Vec::new(),
            viewing_thumbnail: None,
            error_message: None,
            panel_open: true,
            pixelated: false,
            checkered_bg: true,
            normalize: false,
            limits,
        };

        let _ = app.load_current_file();

        app
    }

    fn load_image_from_path(
        path: &Path,
        normalize: bool,
        limits: vexel::Limits,
    ) -> (Result<Vec<Page>, String>, Vec<Thumbnail>) {
        let mut decoder = match Vexel::open(path) {
            Ok(decoder) => decoder,
            Err(e) => return (Err(format!("{}: {}", path.display(), e)), Vec::new()),
        };
        decoder.set_limits(limits);

        let thumbnails = decoder
            .decode_thumbnails()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|image| Self::thumbnail_from_image(image, normalize))
            .collect();

        let is_multi_page = matches!(
            decoder.get_format(),
            ImageFormat::Ico
                | ImageFormat::Cur
                | ImageFormat::Tiff
                | ImageFormat::Exr
                | ImageFormat::NetPbmP1
                | ImageFormat::NetPbmP2
                | ImageFormat::NetPbmP3
                | ImageFormat::NetPbmP4
                | ImageFormat::NetPbmP5
                | ImageFormat::NetPbmP6
                | ImageFormat::NetPbmP7
        );

        let result = if is_multi_page {
            decoder.decode_pages().map_err(|e| format!("{}: {}", path.display(), e)).map(|images| {
                images.into_iter().map(|image| Self::image_to_page(image, normalize)).collect::<Vec<Page>>()
            })
        } else {
            decoder
                .decode_frames()
                .map_err(|e| format!("{}: {}", path.display(), e))
                .map(|frames| vec![Self::frames_to_page(frames, normalize)])
        };

        let result = result.and_then(|pages| {
            let pages: Vec<Page> = pages.into_iter().filter(|page| !page.frames.is_empty()).collect();

            if pages.is_empty() {
                Err(format!("{}: decoded image has no frames", path.display()))
            } else {
                Ok(pages)
            }
        });

        (result, thumbnails)
    }

    fn load_current_file(&mut self) -> bool {
        if self.files.is_empty() {
            return false;
        }

        let (result, thumbnails) =
            Self::load_image_from_path(&self.files[self.current_file_index], self.normalize, self.limits.clone());

        self.thumbnails = thumbnails;
        self.viewing_thumbnail = None;

        match result {
            Ok(pages) => {
                self.pixelated = Self::is_small_image(pages[0].size);
                self.pages = pages;
                self.current_page = 0;
                self.current_frame = 0;
                self.last_frame_time = Instant::now();
                self.error_message = None;
                let options = self.texture_options();
                if let Some(texture) = &mut self.texture {
                    texture.set(self.pages[0].frames[0].clone(), options);
                }
                true
            }
            Err(e) => {
                eprintln!("{}", e);
                self.pages.clear();
                self.current_page = 0;
                self.texture = None;
                self.error_message = Some(e);
                false
            }
        }
    }

    fn page(&self) -> Option<&Page> {
        self.pages.get(self.current_page)
    }

    fn current_display_image(&self) -> Option<Arc<egui::ColorImage>> {
        match self.viewing_thumbnail {
            Some(index) => self.thumbnails.get(index).map(|t| t.image.clone()),
            None => self.page().and_then(|page| page.frames.get(self.current_frame).cloned()),
        }
    }

    fn select_view(&mut self, view: Option<usize>) {
        if self.viewing_thumbnail == view {
            return;
        }

        self.viewing_thumbnail = view;
        self.current_frame = 0;
        self.last_frame_time = Instant::now();
        self.update_texture();
    }

    fn select_page(&mut self, index: usize) {
        if self.viewing_thumbnail.is_none() && self.current_page == index {
            return;
        }

        self.viewing_thumbnail = None;
        self.current_page = index.min(self.pages.len().saturating_sub(1));
        self.current_frame = 0;
        self.last_frame_time = Instant::now();
        self.update_texture();
    }

    fn update_texture(&mut self) {
        match self.current_display_image() {
            Some(image) => {
                let options = self.texture_options();
                if let Some(texture) = &mut self.texture {
                    texture.set(image, options);
                }
            }
            None => self.texture = None,
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
        let dropped_paths: Vec<PathBuf> =
            ctx.input(|input| input.raw.dropped_files.iter().filter_map(|file| file.path.clone()).collect());

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
        let next =
            ctx.input(|input| input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::ArrowDown));
        if next {
            self.try_move_file_index(1);
        }

        let previous =
            ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::ArrowUp));
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
        if self.texture.is_some() {
            return;
        }

        if let Some(image) = self.current_display_image() {
            let options = self.texture_options();
            self.texture = Some(ctx.load_texture("image", image, options));
        }
    }

    fn update_frame(&mut self, ctx: &egui::Context) {
        if self.viewing_thumbnail.is_some() {
            return;
        }

        let Some(page) = self.pages.get(self.current_page) else {
            return;
        };

        if page.frames.len() <= 1 {
            return;
        }

        let elapsed = self.last_frame_time.elapsed().as_millis() as u32;
        let current_delay = page.delays.get(self.current_frame).copied().unwrap_or(100);

        if elapsed >= current_delay {
            let next_frame = (self.current_frame + 1) % page.frames.len();
            let frame = page.frames[next_frame].clone();

            self.current_frame = next_frame;
            self.last_frame_time = Instant::now();

            let options = self.texture_options();
            if let Some(texture) = &mut self.texture {
                texture.set(frame, options);
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
        let separator_color = egui::Color32::from_rgb(0x3a, 0x3f, 0x4b);
        let label_color = egui::Color32::from_rgb(0x7a, 0x84, 0x96);
        let value_color = egui::Color32::WHITE;
        let handle_color = egui::Color32::from_rgb(0x4a, 0x50, 0x5e);
        let handle_hover_color = egui::Color32::from_rgb(0x6a, 0x72, 0x82);

        self.handle_dropped_files(ctx);
        self.handle_keyboard_navigation(ctx);
        self.refresh_texture(ctx);
        self.update_frame(ctx);

        let file_name =
            self.files[self.current_file_index].file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();

        if self.panel_open {
            let panel_frame = egui::Frame {
                fill: panel_bg,
                inner_margin: egui::Margin { left: 20.0, right: 16.0, top: 16.0, bottom: 16.0 },
                ..Default::default()
            };

            egui::SidePanel::right("info_panel").resizable(false).exact_width(220.0).frame(panel_frame).show(
                ctx,
                |ui| {
                    let add_row = |ui: &mut egui::Ui, label: &str, value: &str| {
                        ui.label(egui::RichText::new(label).color(label_color).size(11.0));
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(value).color(value_color).size(13.0));
                        ui.add_space(10.0);
                    };

                    add_row(ui, "FILENAME", &file_name);

                    let [w, h] = match self.viewing_thumbnail {
                        Some(index) => self.thumbnails[index].size,
                        None => self.page().map(|page| page.size).unwrap_or([0, 0]),
                    };
                    add_row(ui, "DIMENSIONS", &format!("{w} × {h}"));

                    if self.pages.len() > 1 {
                        add_row(ui, "PAGES", &self.pages.len().to_string());
                    }

                    let frame_count = self.page().map(|page| page.frames.len()).unwrap_or(0);
                    if frame_count > 1 {
                        add_row(ui, "FRAMES", &frame_count.to_string());
                    }

                    let add_separator = |ui: &mut egui::Ui| {
                        ui.add_space(4.0);
                        ui.painter().hline(
                            ui.available_rect_before_wrap().x_range(),
                            ui.cursor().top(),
                            egui::Stroke::new(1.0_f32, separator_color),
                        );
                        ui.add_space(12.0);
                    };

                    if self.pages.len() > 1 {
                        add_separator(ui);

                        ui.label(egui::RichText::new("PAGES").color(label_color).size(11.0));
                        ui.add_space(6.0);

                        let mut selected_page = None;

                        for (index, page) in self.pages.iter().enumerate() {
                            let [pw, ph] = page.size;
                            let selected = self.viewing_thumbnail.is_none() && self.current_page == index;
                            let text = format!("Page {} · {} × {}", index + 1, pw, ph);
                            let label = egui::RichText::new(text).color(value_color).size(13.0);
                            if ui.selectable_label(selected, label).clicked() {
                                selected_page = Some(index);
                            }
                            ui.add_space(2.0);
                        }

                        if let Some(index) = selected_page {
                            self.select_page(index);
                        }

                        ui.add_space(8.0);
                    }

                    if !self.thumbnails.is_empty() {
                        add_separator(ui);

                        ui.label(egui::RichText::new("THUMBNAILS").color(label_color).size(11.0));
                        ui.add_space(6.0);

                        let mut selection = None;

                        if !self.pages.is_empty() {
                            let selected = self.viewing_thumbnail.is_none();
                            let label = egui::RichText::new("Main image").color(value_color).size(13.0);
                            if ui.selectable_label(selected, label).clicked() {
                                selection = Some(None);
                            }
                            ui.add_space(2.0);
                        }

                        for (index, thumbnail) in self.thumbnails.iter().enumerate() {
                            let [tw, th] = thumbnail.size;
                            let selected = self.viewing_thumbnail == Some(index);
                            let text = format!("Thumbnail {} × {}", tw, th);
                            let label = egui::RichText::new(text).color(value_color).size(13.0);
                            if ui.selectable_label(selected, label).clicked() {
                                selection = Some(Some(index));
                            }
                            ui.add_space(2.0);
                        }

                        if let Some(view) = selection {
                            self.select_view(view);
                        }

                        ui.add_space(8.0);
                    }

                    add_separator(ui);

                    ui.label(egui::RichText::new("DISPLAY").color(label_color).size(11.0));
                    ui.add_space(6.0);

                    let pixelated_changed = ui
                        .checkbox(&mut self.pixelated, egui::RichText::new("Pixelated").color(value_color).size(13.0))
                        .changed();

                    if pixelated_changed {
                        let options = self.texture_options();
                        let image = self.current_display_image();
                        if let (Some(texture), Some(image)) = (&mut self.texture, image) {
                            texture.set(image, options);
                        }
                    }

                    ui.add_space(4.0);
                    ui.checkbox(
                        &mut self.checkered_bg,
                        egui::RichText::new("Checkered background").color(value_color).size(13.0),
                    );

                    ui.add_space(4.0);
                    let normalize_changed = ui
                        .checkbox(&mut self.normalize, egui::RichText::new("Normalize").color(value_color).size(13.0))
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
                },
            );
        } else {
            egui::SidePanel::right("handle_panel")
                .resizable(false)
                .exact_width(12.0)
                .frame(egui::Frame { fill: panel_bg, inner_margin: egui::Margin::ZERO, ..Default::default() })
                .show(ctx, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    let handle_width = 4.0;
                    let handle_x = rect.left() + (rect.width() - handle_width) / 2.0;
                    let grip_h = 32.0;
                    let grip_y = rect.center().y - grip_h / 2.0;
                    let bar_rect =
                        egui::Rect::from_min_size(egui::pos2(handle_x, grip_y), egui::vec2(handle_width, grip_h));
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
                    let checker = self.checker_texture.get_or_insert_with(|| {
                        let tile = 8usize;
                        let light = egui::Color32::from_rgb(0xcc, 0xcc, 0xcc);
                        let dark = egui::Color32::from_rgb(0x88, 0x88, 0x88);
                        let size = tile * 2;
                        let mut checker_image = egui::ColorImage::new([size, size], light);
                        for y in 0..size {
                            for x in 0..size {
                                if (x / tile + y / tile) % 2 == 1 {
                                    checker_image.pixels[y * size + x] = dark;
                                }
                            }
                        }
                        ctx.load_texture(
                            "checker",
                            checker_image,
                            egui::TextureOptions {
                                magnification: egui::TextureFilter::Nearest,
                                minification: egui::TextureFilter::Nearest,
                                wrap_mode: egui::TextureWrapMode::Repeat,
                                ..Default::default()
                            },
                        )
                    });
                    let tex_size = checker.size_vec2();
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(image_rect.left() / tex_size.x, image_rect.top() / tex_size.y),
                        egui::pos2(image_rect.right() / tex_size.x, image_rect.bottom() / tex_size.y),
                    );
                    ui.painter().image(checker.id(), image_rect, uv, egui::Color32::WHITE);
                }

                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.image((texture.id(), display_size));
                });
            } else {
                let error = self.error_message.as_deref().unwrap_or("failed to decode image");

                let available = ui.available_rect_before_wrap();
                let max_width = (available.width() * 0.8).min(520.0);
                let center = available.center();

                ui.painter().rect_filled(available, egui::Rounding::ZERO, egui::Color32::TRANSPARENT);

                let content_height = 13.0 + 6.0 + 15.0 + 20.0;
                let top = center.y - content_height / 2.0;

                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::pos2(center.x - max_width / 2.0, top),
                        egui::vec2(max_width, available.height()),
                    )),
                    |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(&file_name)
                                    .color(egui::Color32::from_rgb(0xff, 0x6b, 0x6b))
                                    .size(13.0)
                                    .monospace(),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(error).color(egui::Color32::from_rgb(0xff, 0xa0, 0xa0)).size(15.0),
                            );
                            if !self.thumbnails.is_empty() {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("Embedded thumbnails are available in the side panel")
                                        .color(egui::Color32::from_rgb(0x7a, 0x84, 0x96))
                                        .size(13.0),
                                );
                            }
                        });
                    },
                );
            }
        });
    }
}

fn load_icon() -> Option<Arc<egui::viewport::IconData>> {
    const ICON_BYTES: &[u8] = include_bytes!("../icon/sizes/vexel-256.png");
    eframe::icon_data::from_png_bytes(ICON_BYTES).ok().map(Arc::new)
}

pub fn display_image(
    files: Vec<PathBuf>,
    start_index: usize,
    limits: vexel::Limits,
) -> Result<(), Box<dyn std::error::Error>> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder {
            min_inner_size: Some(egui::vec2(300.0, 300.0)),
            icon: load_icon(),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "Vexel",
        options,
        Box::new(move |cc| {
            let mut visuals = cc.egui_ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::from_rgb(0x28, 0x2c, 0x34);
            visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x3a, 0x3f, 0x4b);
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x4a, 0x50, 0x5e);
            visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x52, 0x58, 0x68);
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(App::new(files, start_index, limits)))
        }),
    )?;

    Ok(())
}
