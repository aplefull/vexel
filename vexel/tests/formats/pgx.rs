use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "PGX signed 4-bit",
            path: "pgx/c0p0_03r0.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c0p0_03r0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX signed 8-bit",
            path: "pgx/c0p0_07.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c0p0_07.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX 1x1",
            path: "pgx/c0p0_13.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c0p0_13.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX crlf header",
            path: "pgx/c0p0_14.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c0p0_14.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX 12-bit",
            path: "pgx/c1p0_06_0.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c1p0_06_0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX narrow",
            path: "pgx/c1p1_07_1.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/c1p1_07_1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PGX opj sample",
            path: "pgx/opj_c1p1_06_0.pgx",
            validation: None,
            comparison: Comparison::Exact { reference_path: "pgx/opj_c1p1_06_0.jxl" },
            ..Default::default()
        },
    ]
}
