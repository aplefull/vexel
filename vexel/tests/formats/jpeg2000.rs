use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG 2000 gray_u1",
            path: "jpeg2000/gray_u1.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/gray_u1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 gray_i4",
            path: "jpeg2000/gray_i4.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/gray_i4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 rgb_u2",
            path: "jpeg2000/rgb_u2.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/rgb_u2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 rgb_alpha_i1",
            path: "jpeg2000/rgb_alpha_i1.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/rgb_alpha_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 gray_alpha_u4",
            path: "jpeg2000/gray_alpha_u4.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/gray_alpha_u4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Rome",
            path: "jpeg2000/Rome.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Rome.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 balloon",
            path: "jpeg2000/balloon.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/balloon.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 dwsample-jp2-1920",
            path: "jpeg2000/dwsample-jp2-1920.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/dwsample-jp2-1920.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 abydos",
            path: "jpeg2000/abydos.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/abydos.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 World_Atlas_V5_Manual_001_0000",
            path: "jpeg2000/World_Atlas_V5_Manual_001_0000.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/World_Atlas_V5_Manual_001_0000.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 a4_colr",
            path: "jpeg2000/a4_colr.j2c",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/a4_colr.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 a6_mono_colr",
            path: "jpeg2000/a6_mono_colr.j2c",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/a6_mono_colr.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 e2_colr",
            path: "jpeg2000/e2_colr.j2c",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/e2_colr.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 profile-cinema4k",
            path: "jpeg2000/profile-cinema4k.j2c",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/profile-cinema4k.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_0",
            path: "jpeg2000/Bretagne2_0.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne2_0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_bypass",
            path: "jpeg2000/Bretagne2_bypass.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne2_bypass.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_termall",
            path: "jpeg2000/Bretagne2_termall.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne2_termall.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_vsc",
            path: "jpeg2000/Bretagne2_vsc.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne2_vsc.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_segsym",
            path: "jpeg2000/Bretagne2_segsym.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne2_segsym.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 Bretagne2_bypass_vsc_reset_termall_pterm_segsym",
            path: "jpeg2000/Bretagne2_bypass_vsc_reset_termall_pterm_segsym.j2k",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jpeg2000/Bretagne2_bypass_vsc_reset_termall_pterm_segsym.jxl",
            },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_01",
            path: "jpeg2000/p0_01.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_09",
            path: "jpeg2000/p0_09.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_09.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_10",
            path: "jpeg2000/p0_10.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_10.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_11",
            path: "jpeg2000/p0_11.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_11.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_12",
            path: "jpeg2000/p0_12.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_12.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p0_13",
            path: "jpeg2000/p0_13.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p0_13.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p1_02",
            path: "jpeg2000/p1_02.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p1_02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 p1_06",
            path: "jpeg2000/p1_06.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/p1_06.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 oj-rgn-tilepart-header-1",
            path: "jpeg2000/oj-rgn-tilepart-header-1.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/oj-rgn-tilepart-header-1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 subsampling_2",
            path: "jpeg2000/subsampling_2.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/subsampling_2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 palettedImage",
            path: "jpeg2000/palettedImage.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/palettedImage.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 file2",
            path: "jpeg2000/file2.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/file2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 file3",
            path: "jpeg2000/file3.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/file3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT reversible 5/3 with RCT",
            path: "jpeg2000/Bretagne1_ht.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne1_ht.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT irreversible 9/7 with ICT",
            path: "jpeg2000/Bretagne1_ht_lossy.j2k",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/Bretagne1_ht_lossy.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT vertically causal context",
            path: "jpeg2000/byte_causal.jhc",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/byte_causal.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT grayscale",
            path: "jpeg2000/oj-ht-byte.jph",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/oj-ht-byte.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT 10-bit CPRL with tile-parts",
            path: "jpeg2000/htj2k_cpf_broadcast.jhc",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/htj2k_cpf_broadcast.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT tile grid offset from image origin",
            path: "jpeg2000/tileoffset-ojph.jhc",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/tileoffset-ojph.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 HT 16-bit",
            path: "jpeg2000/ht.j2c",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/ht.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 palette with incomplete component mapping",
            path: "jpeg2000/issue235.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/issue235.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 CIELab colorspace",
            path: "jpeg2000/issue559-eci-090-CIELab.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/issue559-eci-090-CIELab.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 corrupted tile index out of range",
            path: "jpeg2000/corrupted/2.pdf.SIGFPE.706.1112.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/corrupted/2.pdf.SIGFPE.706.1112.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 corrupted segment length bit count",
            path: "jpeg2000/corrupted/1802.pdf.SIGSEGV.36e.894.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/corrupted/1802.pdf.SIGSEGV.36e.894.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 corrupted codestream",
            path: "jpeg2000/corrupted/issue429.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/corrupted/issue429.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG 2000 corrupted truncated at byte 5000",
            path: "jpeg2000/corrupted/truncated_at_byte_5000.jp2",
            validation: None,
            comparison: Comparison::Exact { reference_path: "jpeg2000/corrupted/truncated_at_byte_5000.jxl" },
            ..Default::default()
        },
    ]
}
