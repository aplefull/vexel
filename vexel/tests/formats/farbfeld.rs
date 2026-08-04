use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![TestCase {
        name: "Farbfeld abydos",
        path: "farbfeld/abydos.ff",
        validation: None,
        comparison: Comparison::Exact { reference_path: "farbfeld/abydos.jxl" },
        ..Default::default()
    }]
}
