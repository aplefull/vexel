use crate::harness::{Comparison, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG-LS test_2x2_rgbw",
            path: "jpeg-ls/test_2x2_rgbw.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test_2x2_rgbw.avif",
            },
        },
        TestCase {
            name: "JPEG-LS test_4x4",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test_4x4.avif",
            },
        },
        TestCase {
            name: "JPEG-LS lena_gray",
            path: "jpeg-ls/lena_gray.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/lena_gray.avif",
            }
        },
        TestCase {
            name: "JPEG-LS lena24b",
            path: "jpeg-ls/lena24b.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/lena24b.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C0E0",
            path: "jpeg-ls/T8C0E0.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C0E0.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C0E3",
            path: "jpeg-ls/T8C0E3.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C0E3.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C1E0",
            path: "jpeg-ls/T8C1E0.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C1E0.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C2E0",
            path: "jpeg-ls/T8C2E0.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C2E0.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C2E3",
            path: "jpeg-ls/T8C2E3.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C2E3.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8C1E3",
            path: "jpeg-ls/T8C1E3.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8C1E3.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8NDE0",
            path: "jpeg-ls/T8NDE0.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8NDE0.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T8NDE3",
            path: "jpeg-ls/T8NDE3.JLS",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/T8NDE3.avif",
            }
        },
        TestCase {
            name: "JPEG-LS T16E0",
            path: "jpeg-ls/T16E0.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T16E0.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS T16E3",
            path: "jpeg-ls/T16E3.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T16E3.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS T8SSE0",
            path: "jpeg-ls/T8SSE0.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T8SSE0.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS T8SSE3",
            path: "jpeg-ls/T8SSE3.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T8SSE3.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS loco_c0_e0_ss_s3_subsampled",
            path: "jpeg-ls/loco_c0_e0_ss_s3_subsampled.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/loco_c0_e0_ss_s3_subsampled.avif",
            }
        },
        TestCase {
            name: "JPEG-LS bunny_HP1",
            path: "jpeg-ls/bunny_HP1.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/bunny_HP1.avif",
            }
        },
        TestCase {
            name: "JPEG-LS bunny_HP2",
            path: "jpeg-ls/bunny_HP2.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/bunny_HP2.avif",
            }
        },
        TestCase {
            name: "JPEG-LS bunny_HP3",
            path: "jpeg-ls/bunny_HP3.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/bunny_HP3.avif",
            }
        },
        TestCase {
            name: "JPEG-LS rgb_u1",
            path: "jpeg-ls/rgb_u1.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/rgb_u1.avif",
            }
        },
        TestCase {
            name: "JPEG-LS rgb_u2",
            path: "jpeg-ls/rgb_u2.jls",
            validation: None,
            // We have MSE of 0 here, but that is because reference is generated by Vexel.
            // It uses fuzzing because it's a 16bit image and we scale it down to 8bit when comparing and
            // results may vary if it were to use reference generated by another decoder.
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/rgb_u2.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS gray_u1",
            path: "jpeg-ls/gray_u1.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/gray_u1.avif",
            }
        },
        TestCase {
            name: "JPEG-LS gray_u2",
            path: "jpeg-ls/gray_u2.jls",
            validation: None,
            // Same as rgb_u2.
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/gray_u2.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            }
        },
        TestCase {
            name: "JPEG-LS test8_ilv_line_rm_7",
            path: "jpeg-ls/test8_ilv_line_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test8_ilv_line_rm_7.avif",
            }
        },
        TestCase {
            name: "JPEG-LS test8_ilv_none_rm_7",
            path: "jpeg-ls/test8_ilv_none_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test8_ilv_none_rm_7.avif",
            }
        },
        TestCase {
            name: "JPEG-LS test8_ilv_sample_rm_7",
            path: "jpeg-ls/test8_ilv_sample_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test8_ilv_sample_rm_7.avif",
            }
        },
        TestCase {
            name: "JPEG-LS test8_ilv_sample_rm_300",
            path: "jpeg-ls/test8_ilv_sample_rm_300.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/test8_ilv_sample_rm_300.avif",
            }
        },
        TestCase {
            name: "JPEG-LS land10-10bit-rgb-hp3-invalid.jls",
            path: "jpeg-ls/land10-10bit-rgb-hp3-invalid.jls",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg-ls/land10-10bit-rgb-hp3-invalid.avif",
            },
        }
    ]
}
