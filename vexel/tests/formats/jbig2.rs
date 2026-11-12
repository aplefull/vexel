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
        TestCase {
            name: "JBIG2 bitmap-composite-and-xnor",
            path: "jbig2/bitmap-composite-and-xnor.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-composite-or-xor-replace-halftone",
            path: "jbig2/bitmap-composite-or-xor-replace-halftone.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-composite-or-xor-replace-refine",
            path: "jbig2/bitmap-composite-or-xor-replace-refine.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-composite-or-xor-replace-text",
            path: "jbig2/bitmap-composite-or-xor-replace-text.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-composite-or-xor-replace",
            path: "jbig2/bitmap-composite-or-xor-replace.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-refine-page-subrect",
            path: "jbig2/bitmap-refine-page-subrect.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-refine-template1-tpgron",
            path: "jbig2/bitmap-refine-template1-tpgron.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-context-reuse-huffman-refagg",
            path: "jbig2/bitmap-symbol-context-reuse-huffman-refagg.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-context-reuse-refagg",
            path: "jbig2/bitmap-symbol-context-reuse-refagg.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-context-reuse",
            path: "jbig2/bitmap-symbol-context-reuse.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-manysyms",
            path: "jbig2/bitmap-symbol-manysyms.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-symhuffrefine-textrefine-export",
            path: "jbig2/bitmap-symbol-symhuffrefine-textrefine-export.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-symbol-symhuffrefineseveral",
            path: "jbig2/bitmap-symbol-symhuffrefineseveral.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-trailing-7fff-stripped-harder-refine",
            path: "jbig2/bitmap-trailing-7fff-stripped-harder-refine.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 bitmap-trailing-7fff-stripped-harder",
            path: "jbig2/bitmap-trailing-7fff-stripped-harder.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/bitmap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 power 042_13",
            path: "jbig2/042_13.jb2",
            comparison: Comparison::Exact { reference_path: "jbig2/042.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 power 042_14",
            path: "jbig2/042_14.jb2",
            comparison: Comparison::Exact { reference_path: "jbig2/042.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 generic_unknown_length_custom_at",
            path: "jbig2/generic_unknown_length_custom_at.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/truchet.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 huffman_custom_table_htlow_min",
            path: "jbig2/huffman_custom_table_htlow_min.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 huffman_custom_table_long_prefixes",
            path: "jbig2/huffman_custom_table_long_prefixes.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 lenient_colour_region",
            path: "jbig2/lenient_colour_region.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 lenient_failing_region",
            path: "jbig2/lenient_failing_region.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 lenient_symbol_out_of_range",
            path: "jbig2/lenient_symbol_out_of_range.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 lenient_truncated_segment",
            path: "jbig2/lenient_truncated_segment.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/truchet.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 lenient_unknown_length_intermediate",
            path: "jbig2/lenient_unknown_length_intermediate.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/truchet.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JBIG2 text_instance_far_left",
            path: "jbig2/text_instance_far_left.jbig2",
            comparison: Comparison::Exact { reference_path: "jbig2/honeycomb.jxl" },
            ..Default::default()
        },
    ]
}
