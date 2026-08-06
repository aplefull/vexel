use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG XS conformance 8-bit lossless",
            path: "jpeg-xs/rgb_u1.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/rgb_u1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS conformance 12-bit lossless",
            path: "jpeg-xs/rgb_u2.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/rgb_u2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS mathematically lossless",
            path: "jpeg-xs/mls_lossless.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/mls_lossless.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS reversible color transform",
            path: "jpeg-xs/mls_rct.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/mls_rct.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS lossy main 4:4:4",
            path: "jpeg-xs/main444_lossy.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/main444_lossy.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS separate sign packets",
            path: "jpeg-xs/sign_separate.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/sign_separate.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS run mode zero coefficients",
            path: "jpeg-xs/runmode_zero_coeff.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/runmode_zero_coeff.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS odd dimensions single decomposition",
            path: "jpeg-xs/odd_dims_nlx1.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/odd_dims_nlx1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS multiple precinct columns",
            path: "jpeg-xs/multi_precinct_cw1.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/multi_precinct_cw1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS 4:2:2 subsampled",
            path: "jpeg-xs/main422_subsampled.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/main422_subsampled.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS 4:2:0 subsampled",
            path: "jpeg-xs/main420_subsampled.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/main420_subsampled.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS Bayer TETRIX RGGB",
            path: "jpeg-xs/bayer_rggb_tetrix.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/bayer_rggb_tetrix.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS Bayer TETRIX inline",
            path: "jpeg-xs/bayer_tetrix_inline.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/bayer_tetrix_inline.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS extended non-linear transform",
            path: "jpeg-xs/nlt_extended_10bit.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/nlt_extended_10bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS quadratic non-linear transform",
            path: "jpeg-xs/nlt_quadratic_10bit.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/nlt_quadratic_10bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS 10-bit",
            path: "jpeg-xs/mls_10bit.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/mls_10bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS 12-bit",
            path: "jpeg-xs/mls_12bit.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/mls_12bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS SVT reference sample 4:2:2",
            path: "jpeg-xs/svt_sample_422_16x16.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/svt_sample_422_16x16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XS four components",
            path: "jpeg-xs/main4444_4comp.jxs",
            comparison: Comparison::Exact { reference_path: "jpeg-xs/main4444_4comp.jxl" },
            ..Default::default()
        },
    ]
}
