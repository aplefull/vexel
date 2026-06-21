use crate::harness::{Comparison, TestCase, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD};

pub fn test_cases() -> Vec<TestCase> {
    vec![
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
            comparison: Comparison::Exact {
                reference_path: "jpeg/2x2_lossless.avif",
            },
        },
        TestCase {
            name: "JPEG Lossless 16-bit",
            path: "jpeg/jpeg_lossless16bit.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/jpeg_lossless16bit.jxl",
            },
        },
        TestCase {
            name: "JPEG Lossless 8-bit RGB",
            path: "jpeg/jpeg_lossless_sel1-rgb.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/jpeg_lossless_sel1-rgb.jxl",
            },
        },
        TestCase {
            name: "JPEG Lossless 10-bit",
            path: "jpeg/jpeg-lossless-XA1.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/jpeg-lossless-XA1.jxl",
            },
        },
        TestCase {
            name: "JPEG Lossless 12-bit",
            path: "jpeg/jpeg-lossless-MR4.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/jpeg-lossless-MR4.jxl",
            },
        },
        TestCase {
            name: "JPEG lossless 4 components",
            path: "jpeg/rgb_alpha_u1_lossless.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/rgb_alpha_u1_lossless.jxl",
            },
        },
        TestCase {
            name: "JPEG arithmetic (cat)",
            path: "jpeg/cat_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (2x2)",
            path: "jpeg/2x2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/2x2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (rainbow)",
            path: "jpeg/arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo1)",
            path: "jpeg/demo1_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo1_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo2)",
            path: "jpeg/demo2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (progressive)",
            path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.avif",
                // Arithmetic decoder seems to match libjpeg exactly, but we are doing IDCT differently, so final
                // image differs slightly.
                // TODO: Maybe switch to integer IDCT as well?
                mse_threshold: 0.9,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG lossless arithmetic",
            path: "jpeg/2x2_lossless_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/2x2_lossless_arithmetic.avif",
            },
        },
        TestCase {
            name: "JPEG lossless arithmetic 2",
            path: "jpeg/cat_lossless_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg/cat_lossless_arithmetic.avif",
            },
        },
        TestCase {
            name: "JPEG extended sequential",
            path: "jpeg/lena_extended_sequential.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/lena_extended_sequential.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG extended sequential",
            path: "jpeg/rose_extended_sequential.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/rose_extended_sequential.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG progressive 12-bit",
            path: "jpeg/rose_progressive_12bit.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/rose_progressive_12bit.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG YCCK",
            path: "jpeg/ycck.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/ycck.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG asymmetric subsampling",
            path: "jpeg/flower.png.im_q85_asymmetric.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/flower.png.im_q85_asymmetric.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG blue channel subsampling",
            path: "jpeg/flower.png.im_q85_rgb_subsample_blue.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/flower.png.im_q85_rgb_subsample_blue.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
    ]
}
