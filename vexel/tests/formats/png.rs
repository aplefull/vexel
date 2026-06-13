use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "PNG",
            path: "png/0b7d50ac449fd59eb3de00647636d0c9.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/0b7d50ac449fd59eb3de00647636d0c9.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/138331052d7c6e4acebfaa92af314e12.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/138331052d7c6e4acebfaa92af314e12.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_alpha_8bit.avif"
            },
        },
    ]
}
