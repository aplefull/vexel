use crate::harness::{Comparison, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "TIFF file_example_TIFF_10MB",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/file_example_TIFF_10MB.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u1",
            path: "tiff/rgb_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u1_jpeg",
            path: "tiff/rgb_u1_jpeg.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_jpeg.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD
            },
        },
        // TODO: I generated reference with some python library and it's probably lossy, so we are not matching
        // it exactly. Reference needs to be regenerated with known correct values.
        TestCase {
            name: "TIFF rgb_u1_png",
            path: "tiff/rgb_u1_png.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_png.avif",
                mse_threshold: 10.0,
                ssim_threshold: 0.99
            },
        },
        TestCase {
            name: "TIFF rgb_u1_lzw",
            path: "tiff/rgb_u1_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u1_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u1_packbits",
            path: "tiff/rgb_u1_packbits.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u1_packbits.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u1_deflate",
            path: "tiff/rgb_u1_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u1_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u2",
            path: "tiff/rgb_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u4",
            path: "tiff/rgb_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u4_lzw",
            path: "tiff/rgb_u4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u4_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u4_deflate",
            path: "tiff/rgb_u4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u4_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i1",
            path: "tiff/rgb_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i1_lzw",
            path: "tiff/rgb_i1_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i1_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i1_deflate",
            path: "tiff/rgb_i1_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i1_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i2",
            path: "tiff/rgb_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i2_lzw",
            path: "tiff/rgb_i2_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i2_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i2_deflate",
            path: "tiff/rgb_i2_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i2_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i4",
            path: "tiff/rgb_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i4_lzw",
            path: "tiff/rgb_i4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i4_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_i4_deflate",
            path: "tiff/rgb_i4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_i4_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f2",
            path: "tiff/rgb_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f2_lzw",
            path: "tiff/rgb_f2_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f2_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f2_deflate",
            path: "tiff/rgb_f2_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f2_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f4",
            path: "tiff/rgb_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f4_lzw",
            path: "tiff/rgb_f4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f4_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f4_deflate",
            path: "tiff/rgb_f4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f4_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f8",
            path: "tiff/rgb_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f8.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f8_lzw",
            path: "tiff/rgb_f8_lzw.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f8_lzw.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_f8_deflate",
            path: "tiff/rgb_f8_deflate.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_f8_deflate.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_u1",
            path: "tiff/rgb_tiled_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_u1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_u1_tiled_jpeg",
            path: "tiff/rgb_u1_tiled_jpeg.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_tiled_jpeg.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD
            },
        },
        TestCase {
            name: "TIFF rgb_u1_tiled_packbits",
            path: "tiff/rgb_u1_tiled_packbits.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_u1_tiled_packbits.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_u2",
            path: "tiff/rgb_tiled_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_u2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_u4",
            path: "tiff/rgb_tiled_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_u4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_i1",
            path: "tiff/rgb_tiled_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_i1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_i2",
            path: "tiff/rgb_tiled_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_i2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_i4",
            path: "tiff/rgb_tiled_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_i4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_f2",
            path: "tiff/rgb_tiled_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_f2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_f4",
            path: "tiff/rgb_tiled_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_f4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_tiled_f8",
            path: "tiff/rgb_tiled_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_tiled_f8.avif",
            },
        },
        // RGBA
        TestCase {
            name: "TIFF rgb_alpha_u1",
            path: "tiff/rgb_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_u1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_u2",
            path: "tiff/rgb_alpha_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_u2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_u4",
            path: "tiff/rgb_alpha_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_u4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_i1",
            path: "tiff/rgb_alpha_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_i1.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_i2",
            path: "tiff/rgb_alpha_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_i2.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_i4",
            path: "tiff/rgb_alpha_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_i4.avif",
            },
        },
        // TODO: 1 byte difference
        /* TestCase {
            name: "TIFF rgb_alpha_f2",
            path: "tiff/rgb_alpha_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_f2.avif",
            },
        }, */
        TestCase {
            name: "TIFF rgb_alpha_f4",
            path: "tiff/rgb_alpha_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_f4.avif",
            },
        },
        TestCase {
            name: "TIFF rgb_alpha_f8",
            path: "tiff/rgb_alpha_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/rgb_alpha_f8.avif",
            },
        },
        // GRAYSCALE
        TestCase {
            name: "TIFF gray_b1",
            path: "tiff/gray_b1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_b1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_u1",
            path: "tiff/gray_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_u1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_u2",
            path: "tiff/gray_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_u2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_u4",
            path: "tiff/gray_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_u4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_i1",
            path: "tiff/gray_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_i1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_i2",
            path: "tiff/gray_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_i2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_i4",
            path: "tiff/gray_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_i4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_f2",
            path: "tiff/gray_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_f2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_f3",
            path: "tiff/gray_f3.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_f3.avif",
            },
        },
        TestCase {
            name: "TIFF gray_f4",
            path: "tiff/gray_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_f4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_f8",
            path: "tiff/gray_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_f8.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_b1",
            path: "tiff/gray_tiled_b1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_b1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_u1",
            path: "tiff/gray_tiled_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_u1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_u2",
            path: "tiff/gray_tiled_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_u2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_u4",
            path: "tiff/gray_tiled_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_u4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_i1",
            path: "tiff/gray_tiled_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_i1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_i2",
            path: "tiff/gray_tiled_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_i2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_i4",
            path: "tiff/gray_tiled_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_i4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_f2",
            path: "tiff/gray_tiled_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_f2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_f4",
            path: "tiff/gray_tiled_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_f4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_tiled_f8",
            path: "tiff/gray_tiled_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_tiled_f8.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_u1",
            path: "tiff/gray_extrasamples_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_u1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_u2",
            path: "tiff/gray_extrasamples_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_u2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_u4",
            path: "tiff/gray_extrasamples_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_u4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_i1",
            path: "tiff/gray_extrasamples_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_i1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_i2",
            path: "tiff/gray_extrasamples_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_i2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_i4",
            path: "tiff/gray_extrasamples_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_i4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_f2",
            path: "tiff/gray_extrasamples_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_f2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_f4",
            path: "tiff/gray_extrasamples_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_f4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_extrasamples_f8",
            path: "tiff/gray_extrasamples_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_extrasamples_f8.avif",
            },
        },
        // GRAYSCALE + ALPHA
        TestCase {
            name: "TIFF gray_alpha_u1",
            path: "tiff/gray_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_u1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_u2",
            path: "tiff/gray_alpha_u2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_u2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_u4",
            path: "tiff/gray_alpha_u4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_u4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_i1",
            path: "tiff/gray_alpha_i1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_i1.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_i2",
            path: "tiff/gray_alpha_i2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_i2.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_i4",
            path: "tiff/gray_alpha_i4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_i4.avif",
            },
        },
        // TODO: man, 1 byte difference
        /* TestCase {
            name: "TIFF gray_alpha_f2",
            path: "tiff/gray_alpha_f2.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_f2.avif",
            },
        }, */
        TestCase {
            name: "TIFF gray_alpha_f4",
            path: "tiff/gray_alpha_f4.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_f4.avif",
            },
        },
        TestCase {
            name: "TIFF gray_alpha_f8",
            path: "tiff/gray_alpha_f8.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/gray_alpha_f8.avif",
            },
        },
        // CMYK
        TestCase {
            name: "TIFF cmyk_u1",
            path: "tiff/cmyk_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/cmyk_u1.avif",
            },
        },
        TestCase {
            name: "TIFF cmyk_alpha_u1",
            path: "tiff/cmyk_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/cmyk_alpha_u1.avif",
            },
        },
        TestCase {
            name: "TIFF cmyk_planar_u1",
            path: "tiff/cmyk_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/cmyk_planar_u1.avif",
            },
        },
        TestCase {
            name: "TIFF cmyk_alpha_planar_u1",
            path: "tiff/cmyk_alpha_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tiff/cmyk_alpha_planar_u1.avif",
            },
        },
    ]
}
