use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "DNG 5G4A9395-compressed-lossy",
            path: "dng/5G4A9395-compressed-lossy.DNG",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/5G4A9395-compressed-lossy.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG A002_651_20240229_000001_2k_12bit",
            path: "dng/A002_651_20240229_000001_2k_12bit.DNG",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/A002_651_20240229_000001_2k_12bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG RAW_LEICA_M8",
            path: "dng/RAW_LEICA_M8.DNG",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/RAW_LEICA_M8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG YDXJ0026",
            path: "dng/YDXJ0026.DNG",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/YDXJ0026.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG iphone12pro-linearraw",
            path: "dng/iphone12pro-linearraw.DNG",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/iphone12pro-linearraw.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG small_cfa_uncompressed",
            path: "dng/small_cfa_uncompressed.dng",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/small_cfa_uncompressed.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG small_cfa_lossless_jpeg",
            path: "dng/small_cfa_lossless_jpeg.dng",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/small_cfa_lossless_jpeg.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "DNG small_linear_raw",
            path: "dng/small_linear_raw.dng",
            validation: None,
            comparison: Comparison::Exact { reference_path: "dng/small_linear_raw.jxl" },
            ..Default::default()
        },
    ]
}
