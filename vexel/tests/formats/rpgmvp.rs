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
        TestCase {
            name: "RPGMVP basn0g02",
            path: "rpgmvp/basn0g02.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/basn0g02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "RPGMVP basn2c08",
            path: "rpgmvp/basn2c08.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/basn2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "RPGMVP basn3p02",
            path: "rpgmvp/basn3p02.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/basn3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "RPGMVP basn4a08",
            path: "rpgmvp/basn4a08.rpgmvp",
            validation: None,
            comparison: Comparison::Exact { reference_path: "rpgmvp/basn4a08.jxl" },
            ..Default::default()
        },
    ]
}
