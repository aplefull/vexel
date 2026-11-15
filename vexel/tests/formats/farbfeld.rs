use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "Farbfeld abydos",
            path: "farbfeld/abydos.ff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "farbfeld/abydos.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "Farbfeld gradient_1x1",
            path: "farbfeld/gradient_1x1.ff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "farbfeld/gradient_1x1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "Farbfeld gradient_4x4",
            path: "farbfeld/gradient_4x4.ff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "farbfeld/gradient_4x4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "Farbfeld gradient_17x3",
            path: "farbfeld/gradient_17x3.ff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "farbfeld/gradient_17x3.jxl" },
            ..Default::default()
        },
    ]
}
