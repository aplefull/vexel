use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "DNG 5G4A9395-compressed-lossy",
            path: "dng/5G4A9395-compressed-lossy.DNG",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "dng/5G4A9395-compressed-lossy.jxl",
            },
        },
        TestCase {
            name: "DNG A002_651_20240229_000001_2k_12bit",
            path: "dng/A002_651_20240229_000001_2k_12bit.DNG",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "dng/A002_651_20240229_000001_2k_12bit.jxl",
            },
        },
        TestCase {
            name: "DNG RAW_LEICA_M8",
            path: "dng/RAW_LEICA_M8.DNG",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "dng/RAW_LEICA_M8.jxl",
            },
        },
        TestCase {
            name: "DNG YDXJ0026",
            path: "dng/YDXJ0026.DNG",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "dng/YDXJ0026.jxl",
            },
        },
    ]
}
