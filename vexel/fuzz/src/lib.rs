use std::io::Cursor;
use std::sync::Once;

use vexel::{Image, ImageFormat, ImageFrame, Limits, Vexel};

const MAX_CONVERTED_BYTES: usize = 16 * 1024 * 1024;

struct Profile {
    threads: usize,
    max_pixel_count: u64,
    max_alloc: u64,
}

const STANDARD: Profile = Profile { threads: 1, max_pixel_count: 1 << 20, max_alloc: 128 * 1024 * 1024 };

const PARALLEL: Profile = Profile { threads: 2, max_pixel_count: 1 << 22, max_alloc: 256 * 1024 * 1024 };

enum Operation {
    Info,
    Decode,
    Pages,
    Thumbnails,
    Frames,
}

impl Operation {
    fn select(data: &[u8]) -> Self {
        match data.last().copied().unwrap_or(0) % 5 {
            0 => Operation::Info,
            1 => Operation::Decode,
            2 => Operation::Pages,
            3 => Operation::Thumbnails,
            _ => Operation::Frames,
        }
    }
}

fn init_thread_pool(threads: usize) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rayon::ThreadPoolBuilder::new().num_threads(threads).build_global();
    });
}

fn touch_image(image: &Image) {
    let _ = (image.width(), image.height(), image.pixel_format(), image.has_alpha());
    if image.pixels().as_bytes().len() <= MAX_CONVERTED_BYTES {
        let _ = image.to_rgba8();
    }
}

fn touch_frame(frame: &ImageFrame) {
    let _ = (frame.width(), frame.height(), frame.delay(), frame.pixel_format(), frame.has_alpha());
    if frame.pixels().as_bytes().len() <= MAX_CONVERTED_BYTES {
        let _ = frame.to_rgba8();
    }
}

fn run(mut decoder: Vexel<Cursor<&[u8]>>, profile: &Profile, operation: Operation) {
    decoder.set_limits(Limits {
        max_pixel_count: Some(profile.max_pixel_count),
        max_alloc: Some(profile.max_alloc),
        ..Limits::default()
    });

    match operation {
        Operation::Info => {
            let _ = (
                decoder.get_format(),
                decoder.supports_pages(),
                decoder.supports_thumbnails(),
                decoder.supports_frames(),
            );
            let info = decoder.get_info();
            let _ = info.summary();
            let _ = info.to_tree();
            let _ = info.to_string();
        }
        Operation::Decode => {
            if let Ok(image) = decoder.decode() {
                touch_image(&image);
            }
        }
        Operation::Pages => {
            if let Ok(pages) = decoder.decode_pages() {
                pages.iter().for_each(touch_image);
            }
        }
        Operation::Thumbnails => {
            if let Ok(thumbnails) = decoder.decode_thumbnails() {
                thumbnails.iter().for_each(touch_image);
            }
        }
        Operation::Frames => {
            if let Ok(mut frames) = decoder.decode_frames() {
                let _ = frames.frame_count_hint();
                while let Ok(Some(frame)) = frames.next_frame() {
                    touch_frame(&frame);
                }
            }
        }
    }
}

fn exercise_detected(data: &[u8], profile: &Profile) {
    init_thread_pool(profile.threads);
    if let Ok(decoder) = Vexel::new(Cursor::new(data)) {
        run(decoder, profile, Operation::select(data));
    }
}

pub fn exercise(data: &[u8]) {
    exercise_detected(data, &STANDARD);
}

pub fn exercise_parallel(data: &[u8]) {
    exercise_detected(data, &PARALLEL);
}

pub fn exercise_format(data: &[u8], format: ImageFormat) {
    init_thread_pool(STANDARD.threads);
    run(Vexel::with_format(Cursor::new(data), format), &STANDARD, Operation::select(data));
}
