use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "TGA truecolor 24-bit uncompressed",
            path: "tga/24-bit-odd.tga",
            comparison: Comparison::Exact { reference_path: "tga/24-bit-odd.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 24-bit top-left",
            path: "tga/rgb24_top_left.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb24_top_left.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 24-bit bottom-right origin",
            path: "tga/true-24-bottom-right.tga",
            comparison: Comparison::Exact { reference_path: "tga/true-24-bottom-right.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 24-bit RLE",
            path: "tga/true-24-rle.tga",
            comparison: Comparison::Exact { reference_path: "tga/true-24-rle.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 24-bit RLE bottom-left",
            path: "tga/rgb24_bottom_left_rle.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb24_bottom_left_rle.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 15-bit uncompressed",
            path: "tga/rgb15.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb15.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 15-bit RLE",
            path: "tga/rgb15rle.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb15rle.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 16-bit with alpha bit",
            path: "tga/utc16.tga",
            comparison: Comparison::Exact { reference_path: "tga/utc16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 32-bit bottom-left alpha",
            path: "tga/rgb32_bottom_left.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb32_bottom_left.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 32-bit top-left no alpha bits",
            path: "tga/flag_t32.tga",
            comparison: Comparison::Exact { reference_path: "tga/flag_t32.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 32-bit RLE mixed packets",
            path: "tga/true-32-mixed.tga",
            comparison: Comparison::Exact { reference_path: "tga/true-32-mixed.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA truecolor 32-bit RLE top-left",
            path: "tga/font.tga",
            comparison: Comparison::Exact { reference_path: "tga/font.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA grayscale 8-bit",
            path: "tga/monochrome8_bottom_left.tga",
            comparison: Comparison::Exact { reference_path: "tga/monochrome8_bottom_left.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA grayscale 8-bit RLE",
            path: "tga/cbw8.tga",
            comparison: Comparison::Exact { reference_path: "tga/cbw8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA grayscale 16-bit top-left",
            path: "tga/monochrome16_top_left.tga",
            comparison: Comparison::Exact { reference_path: "tga/monochrome16_top_left.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA grayscale 16-bit RLE top-left",
            path: "tga/monochrome16_top_left_rle.tga",
            comparison: Comparison::Exact { reference_path: "tga/monochrome16_top_left_rle.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 24-bit palette",
            path: "tga/rgb8.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 24-bit palette grayscale ramp",
            path: "tga/grey.tga",
            comparison: Comparison::Exact { reference_path: "tga/grey.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 24-bit partial palette",
            path: "tga/rgb24_top_left_colormap.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb24_top_left_colormap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 16-bit palette",
            path: "tga/ucm8.tga",
            comparison: Comparison::Exact { reference_path: "tga/ucm8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 16-bit palette RLE",
            path: "tga/ccm8.tga",
            comparison: Comparison::Exact { reference_path: "tga/ccm8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 24-bit palette RLE with alpha",
            path: "tga/colormap-odd.tga",
            comparison: Comparison::Exact { reference_path: "tga/colormap-odd.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA colormap 32-bit palette RLE top-left",
            path: "tga/rgb32_top_left_rle_colormap.tga",
            comparison: Comparison::Exact { reference_path: "tga/rgb32_top_left_rle_colormap.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "TGA single row 32-bit RLE",
            path: "tga/1d.tga",
            comparison: Comparison::Exact { reference_path: "tga/1d.jxl" },
            ..Default::default()
        },
    ]
}
