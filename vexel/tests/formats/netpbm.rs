use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "NetPBM",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            comparison: Comparison::None,
        },
    ]
}
