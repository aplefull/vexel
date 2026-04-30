mod harness;

use std::path::Path;
use harness::*;
use vexel::Vexel;
use writer::Writer;

#[test]
fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = vec![
        TestCase {
            name: "JPEG Baseline",
            path: "jpeg/cat.jpg",
            validation: None,
            save: false,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat.webp",
                tolerance: 5,
                max_differing_pixels: 10,
            },
        },
        TestCase {
            name: "JPEG Lossless",
            path: "jpeg/2x2_lossless.jpg",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JPEG-LS",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF",
            path: "gif/animated.gif",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "BMP",
            path: "bmp/test.bmp",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_16bit.png",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "HDR",
            path: "hdr/sample_HDR.hdr",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "TIFF",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            save: false,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JBIG1 2x2 Checkerboard",
            path: "jbig1/2x2.jbg",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "jbig1/2x2.webp",
            },
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "jbig1/ccitt1.webp",
            },
        },
        TestCase {
            name: "TGA ctc32",
            path: "tga/ctc32.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/ctc32.webp",
            },
        },
        TestCase {
            name: "TGA flag_t32",
            path: "tga/flag_t32.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/flag_t32.webp",
            },
        },
        TestCase {
            name: "TGA lena3",
            path: "tga/lena3.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/lena3.webp",
            },
        },
        TestCase {
            name: "TGA rgb15rle",
            path: "tga/rgb15rle.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb15rle.webp",
            },
        },
        TestCase {
            name: "TGA rgb32rle",
            path: "tga/rgb32rle.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb32rle.webp",
            },
        },
        TestCase {
            name: "TGA xing_b32",
            path: "tga/xing_b32.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/xing_b32.webp",
            },
        },
        TestCase {
            name: "TGA cbw8",
            path: "tga/cbw8.tga",
            validation: None,
            save: false,
            comparison: Comparison::Exact {
                reference_path: "tga/cbw8.webp",
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
    let out_path = Path::new(in_path).with_extension("webp");

    let mut decoder = Vexel::open(in_path)?;

    match decoder.decode() {
        Ok(image) => {
            Writer::write_webp(&out_path, &image_to_writer_image(&image))?;
        }
        Err(e) => {
            println!("Error decoding image: {:?}", e);
            assert!(false);
        }
    }

    Ok(())
}
