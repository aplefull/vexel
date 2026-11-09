use std::io::Cursor;

use vexel::{Image, ImageFrame, Vexel};

const MAX_CONVERTED_BYTES: usize = 16 * 1024 * 1024;

fn open(data: &[u8]) -> Option<Vexel<Cursor<&[u8]>>> {
    Vexel::new(Cursor::new(data)).ok()
}

fn touch_image(image: &Image) {
    let _ = (image.width(), image.height(), image.pixel_format(), image.has_alpha());
    if image.pixels().as_bytes().len() <= MAX_CONVERTED_BYTES {
        let _ = image.to_rgba8();
        let _ = image.to_rgb8();
    }
}

fn touch_frame(frame: &ImageFrame) {
    let _ = (frame.width(), frame.height(), frame.delay(), frame.pixel_format(), frame.has_alpha());
    if frame.pixels().as_bytes().len() <= MAX_CONVERTED_BYTES {
        let _ = frame.to_rgba8();
        let _ = frame.to_rgb8();
    }
}

pub fn exercise(data: &[u8]) {
    let Some(mut decoder) = open(data) else {
        return;
    };

    let _ = (decoder.get_format(), decoder.supports_pages(), decoder.supports_thumbnails(), decoder.supports_frames());

    let info = decoder.get_info();
    let _ = info.summary();
    let _ = info.to_tree();
    let _ = info.to_string();
    drop(info);
    drop(decoder);

    if let Some(mut decoder) = open(data) {
        if let Ok(image) = decoder.decode() {
            touch_image(&image);
        }
    }

    if let Some(mut decoder) = open(data) {
        if let Ok(pages) = decoder.decode_pages() {
            pages.iter().for_each(touch_image);
        }
    }

    if let Some(mut decoder) = open(data) {
        if let Ok(thumbnails) = decoder.decode_thumbnails() {
            thumbnails.iter().for_each(touch_image);
        }
    }

    if let Some(mut decoder) = open(data) {
        if let Ok(mut frames) = decoder.decode_frames() {
            let _ = frames.frame_count_hint();
            while let Ok(Some(frame)) = frames.next_frame() {
                touch_frame(&frame);
            }
        }
    }
}
