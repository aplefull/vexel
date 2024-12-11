use std::fs::File;
use std::io::{Write};
use std::path::{Path, PathBuf};
use webp::{Encoder};
use crate::{Image, ImageFrame, PixelData};

pub struct Writer {}

impl Writer {
    pub fn write_webp(output_path: &PathBuf, image: &Image) -> Result<(), std::io::Error> {
        Writer::validate_pixel_count(&image)?;

        let width = image.width();
        let height = image.height();

        let mut encoder = webp_animation::prelude::Encoder::new((width, height)).unwrap();

        let mut timestamp_ms = 0i32;
        image.frames().iter().try_for_each(|frame| {
            let rgba_data = &frame.as_rgba8();
            // webp_animation requires that every new frame has a timestamp greater than the previous one
            let delta = if frame.delay() == 0 { 1 } else { frame.delay() as i32 };
            timestamp_ms = timestamp_ms + delta;

            encoder.add_frame(rgba_data, timestamp_ms).unwrap();

            Ok::<(), std::io::Error>(())
        })?;

        let data = encoder.finalize(timestamp_ms).unwrap();

        std::fs::write(output_path, data)?;

        Ok(())
    }

    pub fn write_frames(output_path: &str, image: &Image) -> Result<(), std::io::Error> {
        for (i, frame) in image.frames().iter().enumerate() {
            let output_dir = Path::new(output_path).parent().unwrap();
            let output_file_name = Path::new(output_path).file_stem().unwrap().to_str().unwrap();
            let output_path = output_dir.join(format!("{}_frame_{}.webp", output_file_name, i));

            Writer::write_single_frame(&output_path.to_str().unwrap(), frame)?;
        }

        Ok(())
    }

    pub fn write_ppm(output_path: &PathBuf, image: &Image) -> Result<(), std::io::Error> {
        Writer::validate_pixel_count(&image)?;

        let mut file = File::create(output_path)?;
        let width = image.width();
        let height = image.height();
        let pixels = if image.has_alpha() {
            image.as_rgba8()
        } else {
            image.as_rgb8()
        };

        file.write_all(b"P6\n")?;
        file.write_all(format!("{} {}\n", width, height).as_bytes())?;
        file.write_all(b"255\n")?;

        for y in 0..height {
            for x in 0..width {
                let pixel_index = ((y * width + x) * if image.has_alpha() { 4 } else { 3 }) as usize;

                let r = pixels[pixel_index];
                let g = pixels[pixel_index + 1];
                let b = pixels[pixel_index + 2];

                if image.has_alpha() {
                    let a = pixels[pixel_index + 3];
                    file.write_all(&[r, g, b, a])?;
                } else {
                    file.write_all(&[r, g, b])?;
                }
            }
        }

        Ok(())
    }

    pub fn write_pam(output_path: &PathBuf, image: &Image) -> Result<(), std::io::Error> {
        Writer::validate_pixel_count(&image)?;

        let width = image.width();
        let height = image.height();

        let mut file = File::create(output_path)?;

        file.write_all(b"P7\n")?;
        file.write_all(format!("WIDTH {}\n", width).as_bytes())?;
        file.write_all(format!("HEIGHT {}\n", height).as_bytes())?;

        match &image.pixels() {
            PixelData::RGB8(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 255\nTUPLTYPE RGB\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::RGBA8(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::RGB16(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::RGBA16(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::RGB32F(pixels) => {
                file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::RGBA32F(pixels) => {
                file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                for &value in pixels {
                    let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                    file.write_all(&value16.to_be_bytes())?;
                }
            }
            PixelData::L1(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 1\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::L8(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 255\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::L16(pixels) => {
                file.write_all(b"DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
            PixelData::LA8(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 255\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                file.write_all(pixels)?;
            }
            PixelData::LA16(pixels) => {
                file.write_all(b"DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                for value in pixels {
                    file.write_all(&value.to_be_bytes())?;
                }
            }
        }

        Ok(())
    }

    fn write_single_frame(output_path: &str, frame: &ImageFrame) -> Result<(), std::io::Error> {
        let width = frame.width();
        let height = frame.height();

        let data = if frame.has_alpha() {
            frame.as_rgba8()
        } else {
            frame.as_rgb8()
        };

        let encoder = if frame.has_alpha() {
            Encoder::from_rgba(data.as_slice(), width, height)
        } else {
            Encoder::from_rgb(data.as_slice(), width, height)
        };

        let webp_data = encoder.encode_lossless();

        let mut file = File::create(output_path)?;
        file.write_all(&webp_data)?;

        Ok(())
    }

    fn validate_pixel_count(image: &Image) -> Result<(), std::io::Error> {
        let width = image.width();
        let height = image.height();
        let has_alpha = image.has_alpha();
        let pixels = if has_alpha {
            image.as_rgba8()
        } else {
            image.as_rgb8()
        };

        let expected_size = width * height * if has_alpha { 4 } else { 3 };
        let actual_size = pixels.len() as u32;

        if expected_size != actual_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Invalid pixel data size for {}x{} image with {} channels: expected {} pixels, got {}",
                    width,
                    height,
                    if has_alpha { "RGBA" } else { "RGB" },
                    expected_size,
                    actual_size
                ),
            ));
        }
        Ok(())
    }
}