extern crate core;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use vexel::{Image, Vexel};
    use writer::{Writer, WriterImage, WriterImageFrame};

    const BASE_PATH: &str = "./tests/images/";

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

    #[test]
    pub fn test_jpeg_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_JPEG_BASELINE: &str = "jpeg/cat.jpg";
        const PATH_JPEG_LOSSLESS: &str = "jpeg/2x2_lossless.jpg";

        let mut decoder = Vexel::open(get_in_path(PATH_JPEG_BASELINE))?;

        match decoder.decode() {
            Ok(image) => {
                let pixels = image.as_rgb8();

                assert_eq!(pixels.len(), 680 * 453 * 3);
                assert_eq!(pixels[0], 25);
                assert_eq!(pixels[10], 20);
                assert_eq!(pixels[11111], 125);
                assert_eq!(pixels[900000], 193);
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        decoder = Vexel::open(get_in_path(PATH_JPEG_LOSSLESS))?;

        match decoder.decode() {
            Ok(_) => {
                // Vexel::write_bmp(get_out_path(PATH_JPEG_LOSSLESS), image.width(), image.height(), &image.as_rgb8())?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_jls_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_JPEG_LS_1: &str = "jpeg-ls/test_4x4.jls";

        let mut decoder = Vexel::open(get_in_path(PATH_JPEG_LS_1))?;

        match decoder.decode() {
            Ok(image) => {
                //Writer::write_webp(&get_out_path(PATH_JPEG_LS_1, None), &image)?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_gif_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_GIF_1: &str = "gif/animated.gif";

        let mut decoder = Vexel::open(get_in_path(PATH_GIF_1))?;

        match decoder.decode() {
            Ok(_) => {}
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_netpbm_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_PPM_1: &str = "netpbm/P3_16bit.ppm";

        let mut decoder = Vexel::open(get_in_path(PATH_PPM_1))?;

        match decoder.decode() {
            Ok(image) => {
                //Writer::write_webp(&get_out_path(PATH_PPM_1, None), &image)?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_bmp_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_BMP_1: &str = "bmp/test.bmp";

        let mut decoder = Vexel::open(get_in_path(PATH_BMP_1))?;

        match decoder.decode() {
            Ok(_) => {
                //Vexel::write_ppm(path, image.width(), image.height(), &image.as_rgb8())?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_png_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_PNG_1: &str = "png/342083299-7b50019a-7c6f-4625-99c2-f1e69de95b61.png";

        let mut decoder = Vexel::open(get_in_path(PATH_PNG_1))?;

        match decoder.decode() {
            Ok(image) => {
                // Writer::write_webp(&get_out_path(PATH_PNG_1, None), &image)?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_hdr_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_HDR_1: &str = "hdr/sample_HDR.hdr";

        let mut decoder = Vexel::open(get_in_path(PATH_HDR_1))?;

        match decoder.decode() {
            Ok(image) => {
                // Writer::write_webp(&get_out_path(PATH_HDR_1, None), &image)?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }

    #[test]
    pub fn test_tiff_decode() -> Result<(), Box<dyn std::error::Error>> {
        const PATH_TIFF_1: &str = "tiff/file_example_TIFF_10MB.tiff";

        let mut decoder = Vexel::open(get_in_path(PATH_TIFF_1))?;

        match decoder.decode() {
            Ok(image) => {
                Writer::write_webp(&get_out_path(PATH_TIFF_1, None), &image_to_writer_image(&image))?;
            }
            Err(e) => {
                println!("Error decoding image: {:?}", e);
                assert!(false);
            }
        }

        Ok(())
    }
}
