use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "LERC2 v3 byte, 3 bands",
            path: "lerc/bluemarble_256_256_3_byte.lerc2",
            comparison: Comparison::Exact { reference_path: "lerc/bluemarble_256_256_3_byte.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "LERC2 v3 float with mask",
            path: "lerc/california_400_400_1_float.lerc2",
            comparison: Comparison::Exact { reference_path: "lerc/california_400_400_1_float.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "LERC1 CntZImage",
            path: "lerc/world.lerc1",
            comparison: Comparison::Exact { reference_path: "lerc/world.jxl" },
            ..Default::default()
        },
    ]
}
