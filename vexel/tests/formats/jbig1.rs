use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JBIG1 2x2 Checkerboard",
            path: "jbig1/2x2.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/2x2.avif",
            },
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/ccitt1.avif",
            },
        },
    ]
}
