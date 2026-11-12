use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "HDR old-style RLE, bottom-up rows, right to left",
            path: "hdr/old_rle_flipped_y.hdr",
            comparison: Comparison::Exact { reference_path: "hdr/old_rle_oriented.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "HDR old-style RLE, column-major scanlines",
            path: "hdr/old_rle_column_major.hdr",
            comparison: Comparison::Exact { reference_path: "hdr/old_rle_oriented.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "HDR common",
            path: "hdr/common.hdr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "hdr/common.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "HDR forest_path",
            path: "hdr/forest_path.hdr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "hdr/forest_path.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "HDR venice_sunset_4k",
            path: "hdr/venice_sunset_4k.hdr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "hdr/venice_sunset_4k.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "HDR venice_sunset_8k",
            path: "hdr/venice_sunset_8k.hdr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "hdr/venice_sunset_8k.jxl" },
            ..Default::default()
        },
    ]
}
