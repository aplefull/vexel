use crate::harness::{Comparison, TestCase, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "DICOM 00000220",
            path: "dicom/00000220.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/00000220.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM ct.002",
            path: "dicom/ct.002.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/ct.002.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM CT.29537.39",
            path: "dicom/CT.29537.39.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/CT.29537.39.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM CT.4032.1",
            path: "dicom/CT.4032.1.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/CT.4032.1.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM MR.29497.30",
            path: "dicom/MR.29497.30.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/MR.29497.30.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM MR.29497.9",
            path: "dicom/MR.29497.9.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/MR.29497.9.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM mr-lspine",
            path: "dicom/mr-lspine.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/mr-lspine.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM SIEMENS_CTpat1_img02",
            path: "dicom/SIEMENS_CTpat1_img02.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/SIEMENS_CTpat1_img02.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM test (RGB)",
            path: "dicom/test.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/test.jxl",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "DICOM US.3395.1",
            path: "dicom/US.3395.1.dcm",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "dicom/US.3395.1.jxl",
                // TODO: Out image is darker than the reference
                mse_threshold: 0.999,
                ssim_threshold: 0.999,
            },
        },
    ]
}
