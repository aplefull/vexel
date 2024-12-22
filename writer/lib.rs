use std::fs::File;
use std::io::{Write, Error};
use std::path::{Path, PathBuf};
use webp::{AnimEncoder, AnimFrame, Encoder, WebPConfig};

pub struct Writer {}

pub enum WriterPixelData {
    RGB8(Vec<u8>),
    RGBA8(Vec<u8>),
    RGB16(Vec<u16>),
    RGBA16(Vec<u16>),
    RGB32F(Vec<f32>),
    RGBA32F(Vec<f32>),
    L1(Vec<u8>),
    L8(Vec<u8>),
    L16(Vec<u16>),
    LA8(Vec<u8>),
    LA16(Vec<u16>),
}

pub struct WriterImageFrame {
    pub width: u32,
    pub height: u32,
    pub delay: u32,
    pub has_alpha: bool,
    pub pixels: Vec<u8>,
}

pub struct WriterImage {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub frames: Vec<WriterImageFrame>,
}

impl Writer {
    pub fn write_webp(output_path: &PathBuf, image: &WriterImage) -> Result<(), Error> {
        Writer::validate_pixel_count(&image)?;

        let width = image.width;
        let height = image.height;
        let frames = &image.frames;

        let mut timestamp_ms = 0i32;
        let anim_frames = frames.iter().map(|frame| {
            let delta = if frame.delay == 0 { 1 } else { frame.delay as i32 };
            timestamp_ms = timestamp_ms + delta;

            if frame.has_alpha {
                AnimFrame::from_rgba(frame.pixels.as_slice(), width, height, timestamp_ms)
            } else {
                AnimFrame::from_rgb(frame.pixels.as_slice(), width, height, timestamp_ms)
            }
        }).collect::<Vec<AnimFrame>>();

        let mut config = WebPConfig::new().unwrap();
        config.lossless = 1i32;

        let mut encoder = AnimEncoder::new(width, height, &config);

        for frame in anim_frames {
            encoder.add_frame(frame);
        }

        let data = encoder.encode();

        let mut file = File::create(output_path)?;
        file.write_all(&data)?;

        Ok(())
    }

    pub fn write_frames(output_path: &PathBuf, image: &WriterImage) -> Result<(), Error> {
        for (i, frame) in image.frames.iter().enumerate() {
            let output_dir = Path::new(output_path).parent().unwrap();
            let output_file_name = Path::new(output_path).file_stem().unwrap().to_str().unwrap();
            let output_path = output_dir.join(format!("{}_frame_{}.webp", output_file_name, i));

            let width = frame.width;
            let height = frame.height;
            let data = &frame.pixels;

            let encoder = if frame.has_alpha {
                Encoder::from_rgba(data.as_slice(), width, height)
            } else {
                Encoder::from_rgb(data.as_slice(), width, height)
            };

            let webp_data = encoder.encode_lossless();

            let mut file = File::create(output_path)?;
            file.write_all(&webp_data)?;
        }

        Ok(())
    }

     pub fn write_ppm(output_path: &PathBuf, image: &WriterImage) -> Result<(), Error> {
         Writer::validate_pixel_count(&image)?;

         let mut file = File::create(output_path)?;
         let width = image.frames[0].width;
         let height = image.frames[0].height;
         let pixels = &image.frames[0].pixels;
         let has_alpha = image.frames[0].has_alpha;

         file.write_all(b"P6\n")?;
         file.write_all(format!("{} {}\n", width, height).as_bytes())?;
         file.write_all(b"255\n")?;

         for y in 0..height {
             for x in 0..width {
                 let pixel_index = ((y * width + x) * if has_alpha { 4 } else { 3 }) as usize;

                 let r = pixels[pixel_index];
                 let g = pixels[pixel_index + 1];
                 let b = pixels[pixel_index + 2];

                 file.write_all(&[r, g, b])?;
             }
         }

         Ok(())
     }

        pub fn write_pam(output_path: &PathBuf, width: u32, height: u32, pixel_data: &WriterPixelData) -> Result<(), Error> {
            let mut file = File::create(output_path)?;

            file.write_all(b"P7\n")?;
            file.write_all(format!("WIDTH {}\n", width).as_bytes())?;
            file.write_all(format!("HEIGHT {}\n", height).as_bytes())?;

            match pixel_data {
                WriterPixelData::RGB8(pixels) => {
                    file.write_all(b"DEPTH 3\nMAXVAL 255\nTUPLTYPE RGB\nENDHDR\n")?;
                    file.write_all(pixels)?;
                }
                WriterPixelData::RGBA8(pixels) => {
                    file.write_all(b"DEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                    file.write_all(pixels)?;
                }
                WriterPixelData::RGB16(pixels) => {
                    file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                    for value in pixels {
                        file.write_all(&value.to_be_bytes())?;
                    }
                }
                WriterPixelData::RGBA16(pixels) => {
                    file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                    for value in pixels {
                        file.write_all(&value.to_be_bytes())?;
                    }
                }
                WriterPixelData::RGB32F(pixels) => {
                    file.write_all(b"DEPTH 3\nMAXVAL 65535\nTUPLTYPE RGB\nENDHDR\n")?;
                    for &value in pixels {
                        let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                        file.write_all(&value16.to_be_bytes())?;
                    }
                }
                WriterPixelData::RGBA32F(pixels) => {
                    file.write_all(b"DEPTH 4\nMAXVAL 65535\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
                    for &value in pixels {
                        let value16 = (value.clamp(0.0, 1.0) * 65535.0) as u16;
                        file.write_all(&value16.to_be_bytes())?;
                    }
                }
                WriterPixelData::L1(pixels) => {
                    file.write_all(b"DEPTH 1\nMAXVAL 1\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                    file.write_all(pixels)?;
                }
                WriterPixelData::L8(pixels) => {
                    file.write_all(b"DEPTH 1\nMAXVAL 255\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                    file.write_all(pixels)?;
                }
                WriterPixelData::L16(pixels) => {
                    file.write_all(b"DEPTH 1\nMAXVAL 65535\nTUPLTYPE GRAYSCALE\nENDHDR\n")?;
                    for value in pixels {
                        file.write_all(&value.to_be_bytes())?;
                    }
                }
                WriterPixelData::LA8(pixels) => {
                    file.write_all(b"DEPTH 2\nMAXVAL 255\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                    file.write_all(pixels)?;
                }
                WriterPixelData::LA16(pixels) => {
                    file.write_all(b"DEPTH 2\nMAXVAL 65535\nTUPLTYPE GRAYSCALE_ALPHA\nENDHDR\n")?;
                    for value in pixels {
                        file.write_all(&value.to_be_bytes())?;
                    }
                }
            }

            Ok(())
        }

    fn validate_pixel_count(image: &WriterImage) -> Result<(), Error> {
        let width = image.width;
        let height = image.height;
        let has_alpha = image.has_alpha;
        let pixels = &image.frames[0].pixels;

        let expected_size = width * height * if has_alpha { 4 } else { 3 };
        let actual_size = pixels.len() as u32;

        if expected_size != actual_size {
            let msg = format!(
                "Invalid pixel data size for {}x{} image with {} channels: expected {} pixels, got {}",
                width,
                height,
                if has_alpha { "RGBA" } else { "RGB" },
                expected_size,
                actual_size
            );

            return Err(Error::new(std::io::ErrorKind::InvalidData, msg));
        }
        Ok(())
    }
}
