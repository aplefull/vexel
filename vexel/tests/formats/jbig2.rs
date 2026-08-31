use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JBIG2 002",
            path: "jbig2/002.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/002.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 003",
            path: "jbig2/003.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/003.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 005",
            path: "jbig2/005.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/005.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 006",
            path: "jbig2/006.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/006.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110001",
            path: "jbig2/20123110001.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110001.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110002",
            path: "jbig2/20123110002.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110002.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110003",
            path: "jbig2/20123110003.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110003.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110004",
            path: "jbig2/20123110004.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110004.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110005",
            path: "jbig2/20123110005.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110005.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110006",
            path: "jbig2/20123110006.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110006.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110007",
            path: "jbig2/20123110007.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110007.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110008",
            path: "jbig2/20123110008.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110008.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110009",
            path: "jbig2/20123110009.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110009.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 20123110010",
            path: "jbig2/20123110010.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/20123110010.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 F01_200_TT10",
            path: "jbig2/F01_200_TT10.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/F01_200_TT10.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 F01_200_TT9",
            path: "jbig2/F01_200_TT9.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/F01_200_TT9.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 annex-h",
            path: "jbig2/annex-h.jbig2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/annex-h.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT1",
            path: "jbig2/codeStreamTest1_TT1.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT2",
            path: "jbig2/codeStreamTest1_TT2.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT3",
            path: "jbig2/codeStreamTest1_TT3.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT4",
            path: "jbig2/codeStreamTest1_TT4.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT5",
            path: "jbig2/codeStreamTest1_TT5.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT5.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest1_TT7",
            path: "jbig2/codeStreamTest1_TT7.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest1_TT7.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest2_TT6",
            path: "jbig2/codeStreamTest2_TT6.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest2_TT6.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 codeStreamTest3_TT8",
            path: "jbig2/codeStreamTest3_TT8.jb2",
            comparison: Comparison::ExactPages { reference_path: "jbig2/codeStreamTest3_TT8.jxl" },
            ..Default::default()
        },
    ]
}
