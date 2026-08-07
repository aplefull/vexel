use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "JPEG XR Y-only DC subband only",
            path: "jpeg-xr/yonly_sb_dconly.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yonly_sb_dconly.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR Y-only no highpass subband",
            path: "jpeg-xr/yonly_sb_nohighpass.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yonly_sb_nohighpass.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR Y-only no flexbits",
            path: "jpeg-xr/yonly_sb_noflexbits.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yonly_sb_noflexbits.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV444 no overlap",
            path: "jpeg-xr/yuv444_overlap0.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv444_overlap0.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV444 one level overlap",
            path: "jpeg-xr/yuv444_overlap1.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv444_overlap1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV444 two level overlap",
            path: "jpeg-xr/yuv444_overlap2.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv444_overlap2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV420 one level overlap",
            path: "jpeg-xr/yuv420_overlap1.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv420_overlap1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV422 two level overlap",
            path: "jpeg-xr/yuv422_overlap2.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv422_overlap2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV422 4x4 tiles",
            path: "jpeg-xr/yuv422_4x4tiles.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv422_4x4tiles.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV422 2x2 tiles frequency order",
            path: "jpeg-xr/yuv422_2x2tiles_freq.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv422_2x2tiles_freq.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR hard tile boundaries YUV444",
            path: "jpeg-xr/hard_tiles_yuv444.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/hard_tiles_yuv444.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR hard tile boundaries Y-only",
            path: "jpeg-xr/hard_tiles_yonly.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/hard_tiles_yonly.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR windowing",
            path: "jpeg-xr/windowed1.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/windowed1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR windowing offset margins",
            path: "jpeg-xr/windowed6.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/windowed6.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR bitstream orientation is ignored",
            path: "jpeg-xr/orientation_rcw.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/orientation_rcw.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR chroma centering 420",
            path: "jpeg-xr/chroma_centering_420.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/chroma_centering_420.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR BGRA planar alpha",
            path: "jpeg-xr/bgra32_planar_alpha.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/bgra32_planar_alpha.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR BGRA interleaved alpha",
            path: "jpeg-xr/bgra32_interleaved_alpha.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/bgra32_interleaved_alpha.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR N-component three channels with alpha",
            path: "jpeg-xr/ncomponent3_alpha.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/ncomponent3_alpha.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR N-component four channels",
            path: "jpeg-xr/ncomponent4.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/ncomponent4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR CMYK 32-bit",
            path: "jpeg-xr/cmyk32.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/cmyk32.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR CMYK 64-bit",
            path: "jpeg-xr/cmyk64.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/cmyk64.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR BGR 24-bit",
            path: "jpeg-xr/bgr24.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/bgr24.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR BGR565 packed",
            path: "jpeg-xr/bgr565.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/bgr565.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR YUV444 16-bit output",
            path: "jpeg-xr/yuv444_16bit.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/yuv444_16bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR trimmed flexbits",
            path: "jpeg-xr/trim_flexbits.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/trim_flexbits.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR macroblock level quantizers",
            path: "jpeg-xr/mb_level_qp.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/mb_level_qp.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR index table",
            path: "jpeg-xr/index_table.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/index_table.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR shared tile index",
            path: "jpeg-xr/shared_tile.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/shared_tile.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR tag based container",
            path: "jpeg-xr/tag_container.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/tag_container.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR level 16 columns",
            path: "jpeg-xr/level16_cols.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/level16_cols.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "JPEG XR level 128 rows frequency order",
            path: "jpeg-xr/level128_rows_freq.jxr",
            comparison: Comparison::Exact { reference_path: "jpeg-xr/level128_rows_freq.jxl" },
            ..Default::default()
        },
    ]
}
