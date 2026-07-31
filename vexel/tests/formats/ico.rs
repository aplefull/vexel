use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "ICO image",
            path: "ico/image.cur",
            validation: None,
            comparison: Comparison::Exact { reference_path: "ico/image.avif" },
            ..Default::default()
        },
        TestCase {
            name: "ICO sample",
            path: "ico/sample.ico",
            validation: None,
            comparison: Comparison::Exact { reference_path: "ico/sample.avif" },
            ..Default::default()
        },
        TestCase {
            name: "ICO demo",
            path: "ico/demo.ico",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "ico/demo.avif" },
            ..Default::default()
        },
        TestCase {
            name: "ICO demo BMP",
            path: "ico/demo_bmp.ico",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "ico/demo_bmp.avif" },
            ..Default::default()
        },
        TestCase {
            name: "ICO kuromi CUR",
            path: "ico/kuromi-2bd2e238.cur",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "ico/kuromi-2bd2e238.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "ICO max",
            path: "ico/max.ico",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "ico/max.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "ICO totoro",
            path: "ico/totoro.ico",
            validation: None,
            comparison: Comparison::ExactPages { reference_path: "ico/totoro.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "ICO 32bpp BMP reported as 8bpp in directory",
            path: "ico/32bpp_dir_reports_8bpp.ico",
            validation: None,
            comparison: Comparison::Exact { reference_path: "ico/32bpp_dir_reports_8bpp.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "ICO bad smile with incorrect image length",
            path: "ico/bad_smile_incorrect_image_length.ico",
            validation: None,
            comparison: Comparison::Exact { reference_path: "ico/bad_smile_incorrect_image_length.jxl" },
            ..Default::default()
        },
    ]
}
