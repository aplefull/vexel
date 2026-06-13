use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "HDR",
            path: "hdr/sample_HDR.hdr",
            validation: None,
            comparison: Comparison::None,
        },
    ]
}
