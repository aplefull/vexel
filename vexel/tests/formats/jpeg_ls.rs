use crate::harness::{Comparison, TestCase, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG-LS test_2x2_rgbw",
            path: "jpeg-ls/test_2x2_rgbw.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test_2x2_rgbw.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS test_4x4",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test_4x4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS lena_gray",
            path: "jpeg-ls/lena_gray.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/lena_gray.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS lena24b",
            path: "jpeg-ls/lena24b.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/lena24b.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C0E0",
            path: "jpeg-ls/T8C0E0.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C0E0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C0E3",
            path: "jpeg-ls/T8C0E3.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C0E3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C1E0",
            path: "jpeg-ls/T8C1E0.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C1E0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C2E0",
            path: "jpeg-ls/T8C2E0.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C2E0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C2E3",
            path: "jpeg-ls/T8C2E3.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C2E3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8C1E3",
            path: "jpeg-ls/T8C1E3.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8C1E3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8NDE0",
            path: "jpeg-ls/T8NDE0.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8NDE0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8NDE3",
            path: "jpeg-ls/T8NDE3.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T8NDE3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T16E0",
            path: "jpeg-ls/T16E0.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T16E0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T16E3",
            path: "jpeg-ls/T16E3.JLS",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/T16E3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8SSE0",
            path: "jpeg-ls/T8SSE0.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T8SSE0.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T8SSE3",
            path: "jpeg-ls/T8SSE3.JLS",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg-ls/T8SSE3.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS loco_c0_e0_ss_s3_subsampled",
            path: "jpeg-ls/loco_c0_e0_ss_s3_subsampled.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/loco_c0_e0_ss_s3_subsampled.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS bunny_HP1",
            path: "jpeg-ls/bunny_HP1.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/bunny_HP1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS bunny_HP2",
            path: "jpeg-ls/bunny_HP2.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/bunny_HP2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS bunny_HP3",
            path: "jpeg-ls/bunny_HP3.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/bunny_HP3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS rgb_u1",
            path: "jpeg-ls/rgb_u1.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/rgb_u1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS rgb_u2",
            path: "jpeg-ls/rgb_u2.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/rgb_u2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS gray_u1",
            path: "jpeg-ls/gray_u1.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/gray_u1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS gray_u2",
            path: "jpeg-ls/gray_u2.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/gray_u2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS test8_ilv_line_rm_7",
            path: "jpeg-ls/test8_ilv_line_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test8_ilv_line_rm_7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS test8_ilv_none_rm_7",
            path: "jpeg-ls/test8_ilv_none_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test8_ilv_none_rm_7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS test8_ilv_sample_rm_7",
            path: "jpeg-ls/test8_ilv_sample_rm_7.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test8_ilv_sample_rm_7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS T.87 H.4.5 palettised image",
            path: "jpeg-ls/t87_h45_palette.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/t87_h45_palette.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS mapping tables on two of three components",
            path: "jpeg-ls/mapping_rgb_partial.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/mapping_rgb_partial.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS mapping table continuation",
            path: "jpeg-ls/mapping_continuation.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/mapping_continuation.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS 32-bit restart interval",
            path: "jpeg-ls/test8_ilv_sample_rm_7_dri32.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test8_ilv_sample_rm_7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS height from 24-bit DNL",
            path: "jpeg-ls/dnl_24bit.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/dnl_24bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS oversize image dimensions",
            path: "jpeg-ls/oversize_width.jls",
            comparison: Comparison::Exact { reference_path: "jpeg-ls/oversize_width.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS test8_ilv_sample_rm_300",
            path: "jpeg-ls/test8_ilv_sample_rm_300.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/test8_ilv_sample_rm_300.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG-LS land10-10bit-rgb-hp3-invalid.jls",
            path: "jpeg-ls/land10-10bit-rgb-hp3-invalid.jls",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg-ls/land10-10bit-rgb-hp3-invalid.jxl" },
            ..Default::default()
        },
    ]
}
