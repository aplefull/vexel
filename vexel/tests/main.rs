extern crate core;

#[cfg(test)]
mod tests {
use std::path::{Path, PathBuf};
    use vexel::{Image, Vexel};
    use writer::{Writer, WriterImage, WriterImageFrame};

    const BASE_PATH: &str = "./tests/images/";

    struct TestCase {
        name: &'static str,
        path: &'static str,
        validation: Option<Box<dyn Fn(&Image)>>,
        save: bool,
    }

    fn get_in_path(path: &str) -> String {
        format!("{}{}", BASE_PATH, path)
    }

    fn get_out_path(path: &str, ext: Option<&str>) -> PathBuf {
        let path = Path::new(path).with_extension(ext.unwrap_or("webp"));
        let out_path = Path::new(BASE_PATH).join(path);

        out_path
    }

    fn image_to_writer_image(image: &Image) -> WriterImage {
        let mut frames = Vec::new();

        for frame in image.frames() {
            frames.push(WriterImageFrame {
                width: frame.width(),
                height: frame.height(),
                has_alpha: frame.has_alpha(),
                delay: frame.delay(),
                pixels: if frame.has_alpha() { frame.as_rgba8() } else { frame.as_rgb8() },
            });
        }

        WriterImage {
            width: image.width(),
            height: image.height(),
            has_alpha: image.has_alpha(),
            frames,
        }
    }

    fn test_decode(test_case: TestCase) -> Result<(), Box<dyn std::error::Error>> {
        let mut decoder = Vexel::open(get_in_path(test_case.path))?;

        match decoder.decode() {
            Ok(image) => {
                if let Some(validate) = test_case.validation {
                    validate(&image);
                }

                if test_case.save {
                    Writer::write_webp(
                        &get_out_path(test_case.path, None),
                        &image_to_writer_image(&image),
                    )?;
                }
                
                Ok(())
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                panic!("Failed to decode {}", test_case.name);
            }
        }
    }

    #[test]
    fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
        let test_cases = vec![
            TestCase {
                name: "JPEG Baseline",
                path: "jpeg/cat.jpg",
                validation: Some(Box::new(|image: &Image| {
                    let pixels = image.as_rgb8();
                    
                    assert_eq!(pixels.len(), 680 * 453 * 3);
                    assert_eq!(pixels[0], 25);
                    assert_eq!(pixels[10], 20);
                    assert_eq!(pixels[11111], 125);
                    assert_eq!(pixels[900000], 193);
                })),
                save: false,
            },
            TestCase {
                name: "JPEG Lossless",
                path: "jpeg/2x2_lossless.jpg",
                validation: None,
                save: false,
            },
            TestCase {
                name: "JPEG-LS",
                path: "jpeg-ls/test_4x4.jls",
                validation: None,
                save: false,
            },
            TestCase {
                name: "GIF",
                path: "gif/animated.gif",
                validation: None,
                save: false,
            },
            TestCase {
                name: "NetPBM",
                path: "netpbm/P3_16bit.ppm",
                validation: None,
                save: false,
            },
            TestCase {
                name: "BMP",
                path: "bmp/test.bmp",
                validation: None,
                save: false,
            },
            TestCase {
                name: "PNG",
                path: "png/342083299-7b50019a-7c6f-4625-99c2-f1e69de95b61.png",
                validation: None,
                save: false,
            },
            TestCase {
                name: "HDR",
                path: "hdr/sample_HDR.hdr",
                validation: None,
                save: false,
            },
            TestCase {
                name: "TIFF",
                path: "tiff/file_example_TIFF_10MB.tiff",
                validation: None,
                save: true,
            },
        ];

        for test_case in test_cases {
            test_decode(test_case)?;
        }

        Ok(())
    }
    
    #[test]
    // This test is used during development for convenience for any new image formats
    pub fn test_image() -> Result<(), Box<dyn std::error::Error>> {
        const PATH: &str = "";

        let mut decoder = Vexel::open(get_in_path(PATH))?;

        match decoder.decode() {
            Ok(image) => {
                Writer::write_webp(&get_out_path(PATH, None), &image_to_writer_image(&image))?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }
}
