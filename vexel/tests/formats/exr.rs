use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "EXR AllHalfValues",
            path: "exr/AllHalfValues.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/AllHalfValues.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Cannon",
            path: "exr/Cannon.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Cannon.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Carrots",
            path: "exr/Carrots.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Carrots.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_b44",
            path: "exr/comp_b44.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_b44.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_b44_piz",
            path: "exr/comp_b44_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_b44_piz.jxl" },
            ..Default::default()
        },
        // TODO: DWA compression is not yet supported
        TestCase {
            name: "EXR comp_dwaa_piz",
            path: "exr/comp_dwaa_piz.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_dwaa_v1",
            path: "exr/comp_dwaa_v1.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_dwaa_v2",
            path: "exr/comp_dwaa_v2.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_dwab_piz",
            path: "exr/comp_dwab_piz.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_dwab_v1",
            path: "exr/comp_dwab_v1.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_dwab_v2",
            path: "exr/comp_dwab_v2.exr",
            validation: None,
            comparison: Comparison::None,
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_piz",
            path: "exr/comp_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_piz.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_rle",
            path: "exr/comp_rle.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_rle.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_zip",
            path: "exr/comp_zip.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_zip.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR comp_zips",
            path: "exr/comp_zips.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/comp_zips.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Diffuse",
            path: "exr/Diffuse.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Diffuse.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Flowers",
            path: "exr/Flowers.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Flowers.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR lineOrder_decreasing",
            path: "exr/lineOrder_decreasing.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/lineOrder_decreasing.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR lineOrder_increasing",
            path: "exr/lineOrder_increasing.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/lineOrder_increasing.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR PeriodicPattern",
            path: "exr/PeriodicPattern.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/PeriodicPattern.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Rec709",
            path: "exr/Rec709.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Rec709.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Rec709_YC",
            path: "exr/Rec709_YC.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Rec709_YC.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR RgbRampsDiagonal",
            path: "exr/RgbRampsDiagonal.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/RgbRampsDiagonal.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR row-res-left-view-composited",
            path: "exr/row-res-left-view-composited.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/row-res-left-view-composited.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR scanline_with_deepscanline_type",
            path: "exr/scanline_with_deepscanline_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/scanline_with_deepscanline_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR scanline_with_deeptiled_type",
            path: "exr/scanline_with_deeptiled_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/scanline_with_deeptiled_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR scanline_with_tiledimage_type",
            path: "exr/scanline_with_tiledimage_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/scanline_with_tiledimage_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR Specular",
            path: "exr/Specular.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/Specular.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR StarField",
            path: "exr/StarField.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/StarField.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR test_native1",
            path: "exr/test_native1.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/test_native1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR test_native2",
            path: "exr/test_native2.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/test_native2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR tiled",
            path: "exr/tiled.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/tiled.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR tiled_with_deepscanline_type",
            path: "exr/tiled_with_deepscanline_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/tiled_with_deepscanline_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR tiled_with_deeptile_type",
            path: "exr/tiled_with_deeptile_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/tiled_with_deeptile_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR tiled_with_scanlineimage_type",
            path: "exr/tiled_with_scanlineimage_type.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/tiled_with_scanlineimage_type.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR uffizi_latlong_env-128",
            path: "exr/uffizi_latlong_env-128.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/uffizi_latlong_env-128.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR v1.7.test.1",
            path: "exr/v1.7.test.1.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/v1.7.test.1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR v1.7.test.interleaved",
            path: "exr/v1.7.test.interleaved.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/v1.7.test.interleaved.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR v1.7.test.planar",
            path: "exr/v1.7.test.planar.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/v1.7.test.planar.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR v1.7.test.tiled",
            path: "exr/v1.7.test.tiled.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/v1.7.test.tiled.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR WavyLinesSphere",
            path: "exr/WavyLinesSphere.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/WavyLinesSphere.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR WideColorGamut",
            path: "exr/WideColorGamut.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/WideColorGamut.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR WideFloatRange",
            path: "exr/WideFloatRange.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/WideFloatRange.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR XYZ",
            path: "exr/XYZ.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/XYZ.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR XYZ_YC",
            path: "exr/XYZ_YC.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/XYZ_YC.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_bad_bounds_piz",
            path: "exr/corrupted/comp_bad_bounds_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_bad_bounds_piz.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_bad_neg_bounds_pxr24",
            path: "exr/corrupted/comp_bad_neg_bounds_pxr24.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_bad_neg_bounds_pxr24.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_bad_pos_bounds_piz",
            path: "exr/corrupted/comp_bad_pos_bounds_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_bad_pos_bounds_piz.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_bad_pos_bounds_pxr24",
            path: "exr/corrupted/comp_bad_pos_bounds_pxr24.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_bad_pos_bounds_pxr24.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_early_eof_piz",
            path: "exr/corrupted/comp_early_eof_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_early_eof_piz.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "EXR corrupted comp_short_decode_piz",
            path: "exr/corrupted/comp_short_decode_piz.exr",
            validation: None,
            comparison: Comparison::Exact { reference_path: "exr/corrupted/comp_short_decode_piz.jxl" },
            ..Default::default()
        },
    ]
}
