use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG-LS",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::None,
        },
    ]
}
