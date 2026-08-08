use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JBIG1 2x2 checkerboard",
            path: "jbig1/2x2.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/2x2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt2",
            path: "jbig1/ccitt2.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt3",
            path: "jbig1/ccitt3.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt4",
            path: "jbig1/ccitt4.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt5",
            path: "jbig1/ccitt5.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt5.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt6",
            path: "jbig1/ccitt6.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt6.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt7",
            path: "jbig1/ccitt7.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 ccitt8",
            path: "jbig1/ccitt8.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/ccitt8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 multi-resolution fox",
            path: "jbig1/fox.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/fox.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 adaptive template sweep",
            path: "jbig1/mx.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/mx.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG1 multi-resolution xvlogo",
            path: "jbig1/xvlogo.jbg",
            comparison: Comparison::Exact { reference_path: "jbig1/xvlogo.jxl" },
            ..Default::default()
        },
    ]
}
