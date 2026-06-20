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
            comparison: Comparison::None,
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
    ]
}
