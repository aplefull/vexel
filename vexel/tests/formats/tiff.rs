use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "TIFF",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::None,
        },
    ]
}
