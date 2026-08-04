use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "RPGMVP Actor1_4",
            path: "rpgmvp/Actor1_4.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/Actor1_4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "RPGMVP Mountains4",
            path: "rpgmvp/Mountains4.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/Mountains4.jxl" },
            ..Default::default()
        },
    ]
}
