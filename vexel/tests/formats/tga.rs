use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "TGA ctc32",
            path: "tga/ctc32.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/ctc32.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA flag_t32",
            path: "tga/flag_t32.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/flag_t32.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA lena3",
            path: "tga/lena3.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/lena3.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA rgb15rle",
            path: "tga/rgb15rle.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/rgb15rle.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA rgb32rle",
            path: "tga/rgb32rle.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/rgb32rle.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA xing_b32",
            path: "tga/xing_b32.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/xing_b32.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TGA cbw8",
            path: "tga/cbw8.tga",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tga/cbw8.avif" },
            ..Default::default()
        },
    ]
}
