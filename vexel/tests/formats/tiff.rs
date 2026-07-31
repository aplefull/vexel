use crate::harness::{Comparison, TestCase, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "TIFF file_example_TIFF_10MB",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/file_example_TIFF_10MB.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF RAW_KODAK_DCS460D_FILEVERSION_3",
            path: "tiff/RAW_KODAK_DCS460D_FILEVERSION_3.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/RAW_KODAK_DCS460D_FILEVERSION_3.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF RAW_KODAK_DCS560C",
            path: "tiff/RAW_KODAK_DCS560C.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/RAW_KODAK_DCS560C.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF RAW_NIKON_D800_M",
            path: "tiff/RAW_NIKON_D800_M.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/RAW_NIKON_D800_M.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF memorial_luv32",
            path: "tiff/memorial_luv32.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/memorial_luv32.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1",
            path: "tiff/rgb_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_jpeg",
            path: "tiff/rgb_u1_jpeg.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_jpeg.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        // TODO: I generated reference with some python library and it's probably lossy, so we are not matching
        // it exactly. Reference needs to be regenerated with known correct values.
        TestCase {
            name: "TIFF rgb_u1_png",
            path: "tiff/rgb_u1_png.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_png.avif",
                mse_threshold: 10.0,
                ssim_threshold: 0.99,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_lzw",
            path: "tiff/rgb_u1_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u1_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_packbits",
            path: "tiff/rgb_u1_packbits.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u1_packbits.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_deflate",
            path: "tiff/rgb_u1_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u1_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u2",
            path: "tiff/rgb_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u2_lzw",
            path: "tiff/rgb_u2_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u2_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u2_packbits",
            path: "tiff/rgb_u2_packbits.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u2_packbits.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u4",
            path: "tiff/rgb_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u4_lzw",
            path: "tiff/rgb_u4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u4_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u4_deflate",
            path: "tiff/rgb_u4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u4_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u8",
            path: "tiff/rgb_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u8_lzw",
            path: "tiff/rgb_u8_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u8_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u8_deflate",
            path: "tiff/rgb_u8_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u8_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i1",
            path: "tiff/rgb_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i1_lzw",
            path: "tiff/rgb_i1_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i1_lzw.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i1_deflate",
            path: "tiff/rgb_i1_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i1_deflate.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i2",
            path: "tiff/rgb_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i2_lzw",
            path: "tiff/rgb_i2_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i2_lzw.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i2_deflate",
            path: "tiff/rgb_i2_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i2_deflate.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i4",
            path: "tiff/rgb_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i4_lzw",
            path: "tiff/rgb_i4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i4_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i4_deflate",
            path: "tiff/rgb_i4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i4_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i8",
            path: "tiff/rgb_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i8_lzw",
            path: "tiff/rgb_i8_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i8_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_i8_deflate",
            path: "tiff/rgb_i8_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_i8_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f2",
            path: "tiff/rgb_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f2_lzw",
            path: "tiff/rgb_f2_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f2_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f2_deflate",
            path: "tiff/rgb_f2_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f2_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f4",
            path: "tiff/rgb_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f4_lzw",
            path: "tiff/rgb_f4_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f4_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f4_deflate",
            path: "tiff/rgb_f4_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f4_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f8",
            path: "tiff/rgb_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f8_lzw",
            path: "tiff/rgb_f8_lzw.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f8_lzw.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_f8_deflate",
            path: "tiff/rgb_f8_deflate.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_f8_deflate.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_u1",
            path: "tiff/rgb_tiled_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_tiled_jpeg",
            path: "tiff/rgb_u1_tiled_jpeg.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_u1_tiled_jpeg.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_u1_tiled_packbits",
            path: "tiff/rgb_u1_tiled_packbits.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_u1_tiled_packbits.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_u2",
            path: "tiff/rgb_tiled_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_u4",
            path: "tiff/rgb_tiled_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_u8",
            path: "tiff/rgb_tiled_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_i1",
            path: "tiff/rgb_tiled_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_i2",
            path: "tiff/rgb_tiled_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_u1",
            path: "tiff/rgb_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_u2",
            path: "tiff/rgb_planar_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_u4",
            path: "tiff/rgb_planar_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_u8",
            path: "tiff/rgb_planar_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_i1",
            path: "tiff/rgb_planar_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_i2",
            path: "tiff/rgb_planar_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_i4",
            path: "tiff/rgb_planar_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_i8",
            path: "tiff/rgb_planar_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_f2",
            path: "tiff/rgb_planar_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_f4",
            path: "tiff/rgb_planar_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_f8",
            path: "tiff/rgb_planar_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_u1",
            path: "tiff/rgb_planar_tiled_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_u2",
            path: "tiff/rgb_planar_tiled_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_u4",
            path: "tiff/rgb_planar_tiled_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_u8",
            path: "tiff/rgb_planar_tiled_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_i1",
            path: "tiff/rgb_planar_tiled_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_i2",
            path: "tiff/rgb_planar_tiled_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_i4",
            path: "tiff/rgb_planar_tiled_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_i8",
            path: "tiff/rgb_planar_tiled_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_f2",
            path: "tiff/rgb_planar_tiled_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_f4",
            path: "tiff/rgb_planar_tiled_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_planar_tiled_f8",
            path: "tiff/rgb_planar_tiled_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_planar_tiled_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_i4",
            path: "tiff/rgb_tiled_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_i8",
            path: "tiff/rgb_tiled_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_f2",
            path: "tiff/rgb_tiled_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_f4",
            path: "tiff/rgb_tiled_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_tiled_f8",
            path: "tiff/rgb_tiled_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_tiled_f8.avif" },
            ..Default::default()
        },
        // RGBA
        TestCase {
            name: "TIFF rgb_alpha_u1",
            path: "tiff/rgb_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_u2",
            path: "tiff/rgb_alpha_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_u4",
            path: "tiff/rgb_alpha_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_u8",
            path: "tiff/rgb_alpha_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_i1",
            path: "tiff/rgb_alpha_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_i2",
            path: "tiff/rgb_alpha_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_i4",
            path: "tiff/rgb_alpha_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_i8",
            path: "tiff/rgb_alpha_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_f2",
            path: "tiff/rgb_alpha_f2.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_alpha_f2.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_f4",
            path: "tiff/rgb_alpha_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_f8",
            path: "tiff/rgb_alpha_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_u1",
            path: "tiff/rgb_alpha_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_u2",
            path: "tiff/rgb_alpha_planar_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_u4",
            path: "tiff/rgb_alpha_planar_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_u8",
            path: "tiff/rgb_alpha_planar_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_i1",
            path: "tiff/rgb_alpha_planar_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_i2",
            path: "tiff/rgb_alpha_planar_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_i4",
            path: "tiff/rgb_alpha_planar_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_i8",
            path: "tiff/rgb_alpha_planar_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_f2",
            path: "tiff/rgb_alpha_planar_f2.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/rgb_alpha_planar_f2.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_f4",
            path: "tiff/rgb_alpha_planar_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_alpha_planar_f8",
            path: "tiff/rgb_alpha_planar_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/rgb_alpha_planar_f8.avif" },
            ..Default::default()
        },
        // GRAYSCALE
        TestCase {
            name: "TIFF gray_b1",
            path: "tiff/gray_b1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_b1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_u1",
            path: "tiff/gray_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_u2",
            path: "tiff/gray_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_u4",
            path: "tiff/gray_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_u8",
            path: "tiff/gray_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_i1",
            path: "tiff/gray_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_i2",
            path: "tiff/gray_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_i4",
            path: "tiff/gray_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_i8",
            path: "tiff/gray_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_f2",
            path: "tiff/gray_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_f3",
            path: "tiff/gray_f3.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_f3.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_f4",
            path: "tiff/gray_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_f8",
            path: "tiff/gray_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_b1",
            path: "tiff/gray_tiled_b1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_b1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_u1",
            path: "tiff/gray_tiled_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_u2",
            path: "tiff/gray_tiled_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_u4",
            path: "tiff/gray_tiled_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_u8",
            path: "tiff/gray_tiled_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_i1",
            path: "tiff/gray_tiled_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_i2",
            path: "tiff/gray_tiled_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_i4",
            path: "tiff/gray_tiled_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_i8",
            path: "tiff/gray_tiled_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_f2",
            path: "tiff/gray_tiled_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_f4",
            path: "tiff/gray_tiled_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_tiled_f8",
            path: "tiff/gray_tiled_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_tiled_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_u1",
            path: "tiff/gray_extrasamples_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_u2",
            path: "tiff/gray_extrasamples_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_u4",
            path: "tiff/gray_extrasamples_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_u8",
            path: "tiff/gray_extrasamples_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_i1",
            path: "tiff/gray_extrasamples_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_i2",
            path: "tiff/gray_extrasamples_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_i4",
            path: "tiff/gray_extrasamples_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_i8",
            path: "tiff/gray_extrasamples_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_f2",
            path: "tiff/gray_extrasamples_f2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_f4",
            path: "tiff/gray_extrasamples_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_extrasamples_f8",
            path: "tiff/gray_extrasamples_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_extrasamples_f8.avif" },
            ..Default::default()
        },
        // GRAYSCALE + ALPHA
        TestCase {
            name: "TIFF gray_alpha_u1",
            path: "tiff/gray_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_u2",
            path: "tiff/gray_alpha_u2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_u4",
            path: "tiff/gray_alpha_u4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_u8",
            path: "tiff/gray_alpha_u8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_i1",
            path: "tiff/gray_alpha_i1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_i2",
            path: "tiff/gray_alpha_i2.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_i4",
            path: "tiff/gray_alpha_i4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_i8",
            path: "tiff/gray_alpha_i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_f2",
            path: "tiff/gray_alpha_f2.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/gray_alpha_f2.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_f4",
            path: "tiff/gray_alpha_f4.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_alpha_f8",
            path: "tiff/gray_alpha_f8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/gray_alpha_f8.avif" },
            ..Default::default()
        },
        // CMYK
        TestCase {
            name: "TIFF cmyk_u1",
            path: "tiff/cmyk_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk_alpha_u1",
            path: "tiff/cmyk_alpha_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk_alpha_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk_planar_u1",
            path: "tiff/cmyk_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk_planar_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk_alpha_planar_u1",
            path: "tiff/cmyk_alpha_planar_u1.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk_alpha_planar_u1.avif" },
            ..Default::default()
        },
        // FRAMES
        TestCase {
            name: "TIFF rgb_frames_u1",
            path: "tiff/rgb_frames_u1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_u2",
            path: "tiff/rgb_frames_u2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_u4",
            path: "tiff/rgb_frames_u4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_u8",
            path: "tiff/rgb_frames_u8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_i1",
            path: "tiff/rgb_frames_i1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_i2",
            path: "tiff/rgb_frames_i2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_i4",
            path: "tiff/rgb_frames_i4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_i8",
            path: "tiff/rgb_frames_i8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_f2",
            path: "tiff/rgb_frames_f2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_f4",
            path: "tiff/rgb_frames_f4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF rgb_frames_f8",
            path: "tiff/rgb_frames_f8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/rgb_frames_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_u1",
            path: "tiff/gray_frames_u1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_u2",
            path: "tiff/gray_frames_u2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_u4",
            path: "tiff/gray_frames_u4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_u8",
            path: "tiff/gray_frames_u8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_i1",
            path: "tiff/gray_frames_i1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_i2",
            path: "tiff/gray_frames_i2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_i4",
            path: "tiff/gray_frames_i4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_i8",
            path: "tiff/gray_frames_i8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_f2",
            path: "tiff/gray_frames_f2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_f4",
            path: "tiff/gray_frames_f4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_frames_f8",
            path: "tiff/gray_frames_f8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_frames_f8.avif" },
            ..Default::default()
        },
        // VOLUMETRIC
        TestCase {
            name: "TIFF gray_volumetric_u1",
            path: "tiff/gray_volumetric_u1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_u1.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_u2",
            path: "tiff/gray_volumetric_u2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_u2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_u4",
            path: "tiff/gray_volumetric_u4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_u4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_u8",
            path: "tiff/gray_volumetric_u8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_u8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_i1",
            path: "tiff/gray_volumetric_i1.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_i1.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_i2",
            path: "tiff/gray_volumetric_i2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_i2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_i4",
            path: "tiff/gray_volumetric_i4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_i4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_i8",
            path: "tiff/gray_volumetric_i8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_i8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_f2",
            path: "tiff/gray_volumetric_f2.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_f2.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_f4",
            path: "tiff/gray_volumetric_f4.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_f4.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF gray_volumetric_f8",
            path: "tiff/gray_volumetric_f8.tif",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "tiff/gray_volumetric_f8.avif" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk-3c-8b",
            path: "tiff/cmyk-3c-8b.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk-3c-8b.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk-3c-16b",
            path: "tiff/cmyk-3c-16b.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk-3c-16b.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk-3c-32b-float",
            path: "tiff/cmyk-3c-32b-float.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk-3c-32b-float.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF cmyk-4c-8b",
            path: "tiff/cmyk-4c-8b.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/cmyk-4c-8b.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF fax2d",
            path: "tiff/fax2d.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/fax2d.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF fax4",
            path: "tiff/fax4.tiff",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/fax4.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-02",
            path: "tiff/flower-minisblack-02.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-04",
            path: "tiff/flower-minisblack-04.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-06",
            path: "tiff/flower-minisblack-06.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-06.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-08",
            path: "tiff/flower-minisblack-08.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-10",
            path: "tiff/flower-minisblack-10.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-10.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-12",
            path: "tiff/flower-minisblack-12.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-12.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-14",
            path: "tiff/flower-minisblack-14.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-14.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-16",
            path: "tiff/flower-minisblack-16.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-24",
            path: "tiff/flower-minisblack-24.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-24.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-minisblack-32",
            path: "tiff/flower-minisblack-32.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-minisblack-32.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-palette-04",
            path: "tiff/flower-palette-04.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-palette-04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-palette-08",
            path: "tiff/flower-palette-08.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-palette-08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-palette-16",
            path: "tiff/flower-palette-16.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-palette-16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-contig-02",
            path: "tiff/flower-rgb-contig-02.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-contig-02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-contig-08",
            path: "tiff/flower-rgb-contig-08.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-contig-08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-contig-12",
            path: "tiff/flower-rgb-contig-12.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-contig-12.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-contig-32",
            path: "tiff/flower-rgb-contig-32.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-contig-32.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-planar-02",
            path: "tiff/flower-rgb-planar-02.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-planar-02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-planar-08",
            path: "tiff/flower-rgb-planar-08.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-planar-08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-planar-16",
            path: "tiff/flower-rgb-planar-16.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-planar-16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-rgb-planar-32",
            path: "tiff/flower-rgb-planar-32.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-rgb-planar-32.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-separated-contig-16",
            path: "tiff/flower-separated-contig-16.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-separated-contig-16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF flower-separated-planar-16",
            path: "tiff/flower-separated-planar-16.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/flower-separated-planar-16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF g3test",
            path: "tiff/g3test.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/g3test.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF ojpeg_chewey_subsamp21_multi_strip",
            path: "tiff/ojpeg_chewey_subsamp21_multi_strip.tiff",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/ojpeg_chewey_subsamp21_multi_strip.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF ojpeg_single_strip_no_rowsperstrip",
            path: "tiff/ojpeg_single_strip_no_rowsperstrip.tiff",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/ojpeg_single_strip_no_rowsperstrip.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF tiled-cmyk-i8",
            path: "tiff/tiled-cmyk-i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/tiled-cmyk-i8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF tiled-jpeg-rgb-u8",
            path: "tiff/tiled-jpeg-rgb-u8.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/tiled-jpeg-rgb-u8.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF tiled-jpeg-ycbcr",
            path: "tiff/tiled-jpeg-ycbcr.tif",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "tiff/tiled-jpeg-ycbcr.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
            ..Default::default()
        },
        TestCase {
            name: "TIFF tiled-oversize-gray-i8",
            path: "tiff/tiled-oversize-gray-i8.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/tiled-oversize-gray-i8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TIFF ycbcr-cat",
            path: "tiff/ycbcr-cat.tif",
            validation: None,
            comparison: Comparison::Exact { reference_path: "tiff/ycbcr-cat.jxl" },
            ..Default::default()
        },
    ]
}
