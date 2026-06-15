use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "HDR common",
            path: "hdr/common.hdr",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "hdr/common.avif",
            },
        },
        TestCase {
            name: "HDR forest_path",
            path: "hdr/forest_path.hdr",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "hdr/forest_path.avif",
            },
        },
        TestCase {
            name: "HDR venice_sunset_4k",
            path: "hdr/venice_sunset_4k.hdr",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "hdr/venice_sunset_4k.avif",
            },
        },
        TestCase {
            name: "HDR venice_sunset_8k",
            path: "hdr/venice_sunset_8k.hdr",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "hdr/venice_sunset_8k.avif",
            },
        },
    ]
}
