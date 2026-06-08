mod harness;

use std::path::Path;
use harness::*;
use vexel::Vexel;

#[test]
fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = vec![
        TestCase {
            name: "JPEG Baseline",
            path: "jpeg/cat.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG Lossless",
            path: "jpeg/2x2_lossless.jpg",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JPEG-LS",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF",
            path: "gif/animated.gif",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "BMP",
            path: "bmp/test.bmp",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "PNG",
            path: "png/0b7d50ac449fd59eb3de00647636d0c9.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/0b7d50ac449fd59eb3de00647636d0c9.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/138331052d7c6e4acebfaa92af314e12.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/138331052d7c6e4acebfaa92af314e12.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "HDR",
            path: "hdr/sample_HDR.hdr",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "TIFF",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JBIG1 2x2 Checkerboard",
            path: "jbig1/2x2.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/2x2.avif",
            },
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/ccitt1.avif",
            },
        },
        TestCase {
            name: "TGA ctc32",
            path: "tga/ctc32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/ctc32.avif",
            },
        },
        TestCase {
            name: "TGA flag_t32",
            path: "tga/flag_t32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/flag_t32.avif",
            },
        },
        TestCase {
            name: "TGA lena3",
            path: "tga/lena3.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/lena3.avif",
            },
        },
        TestCase {
            name: "TGA rgb15rle",
            path: "tga/rgb15rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb15rle.avif",
            },
        },
        TestCase {
            name: "TGA rgb32rle",
            path: "tga/rgb32rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb32rle.avif",
            },
        },
        TestCase {
            name: "TGA xing_b32",
            path: "tga/xing_b32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/xing_b32.avif",
            },
        },
        TestCase {
            name: "TGA cbw8",
            path: "tga/cbw8.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/cbw8.avif",
            },
        },
    ];

    for test_case in test_cases {
        test_decode(test_case)?;
    }

    Ok(())
}

#[test]
#[ignore = "dev only"]
// This test is used during development for convenience for any new image formats
pub fn test_image() -> Result<(), Box<dyn std::error::Error>> {
    let in_path = r"/home/aplefull/Repos/vexel/vexel/tests/images/jpeg/cat_arithmetic.jpg";
    let out_path = Path::new(in_path).with_extension("avif");
    let save = true; 

    let mut decoder = Vexel::open(in_path)?;

    match decoder.decode() {
        Ok(image) => {
            if !save {
                println!("Decoded image: {}x{}, {} frames", image.width(), image.height(), image.frames().len());
                return Ok(());
            }

            if image.frames().len() > 1 {
                let frames = image.frames();
                for (i, frame) in frames.iter().enumerate() {
                    let frame_out_path = out_path.with_file_name(format!(
                        "{}_frame{}.avif",
                        out_path.file_stem().unwrap().to_string_lossy(),
                        i
                    ));

                    let avif_data = libavif::encode_rgb8(frame.width(), frame.height(), &frame.as_rgba8())?;
                    std::fs::write(frame_out_path, avif_data.as_ref())?;
                }
            } else {
                let avif_data = libavif::encode_rgb8(image.width(), image.height(), &image.as_rgba8())?;
                std::fs::write(out_path, avif_data.as_ref())?;
            }
        }
        Err(e) => {
            println!("Error decoding image: {:?}", e);
            assert!(false);
        }
    }

    Ok(())
}
