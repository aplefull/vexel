use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "NetPBM P1",
            path: "netpbm/P1.pbm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P1.avif",
            },
        },
        TestCase {
            name: "NetPBM P2 8bit",
            path: "netpbm/P2_8bit.pgm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P2_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P2 16bit",
            path: "netpbm/P2_16bit.pgm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P2_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P3 8bit",
            path: "netpbm/P3_8bit.ppm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P3_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P3 16bit",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P3_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P4",
            path: "netpbm/P4.pbm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P4.avif",
            },
        },
        TestCase {
            name: "NetPBM P5 8bit",
            path: "netpbm/P5_8bit.pgm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P5_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P5 16bit",
            path: "netpbm/P5_16bit.pgm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P5_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P6 8bit",
            path: "netpbm/P6_8bit.ppm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P6_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P6 16bit",
            path: "netpbm/P6_16bit.ppm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P6_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 monochrome",
            path: "netpbm/P7_monochrome.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_monochrome.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 monochrome alpha",
            path: "netpbm/P7_monochrome_alpha.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_monochrome_alpha.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 grayscale 8bit",
            path: "netpbm/P7_grayscale_8bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_grayscale_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 grayscale 16bit",
            path: "netpbm/P7_grayscale_16bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_grayscale_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 grayscale alpha 8bit",
            path: "netpbm/P7_grayscale_alpha_8bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_grayscale_alpha_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 grayscale alpha 16bit",
            path: "netpbm/P7_grayscale_alpha_16bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_grayscale_alpha_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 rgb 8bit",
            path: "netpbm/P7_rgb_8bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_rgb_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 rgb 16bit",
            path: "netpbm/P7_rgb_16bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_rgb_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 rgb alpha 8bit",
            path: "netpbm/P7_rgb_alpha_8bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_rgb_alpha_8bit.avif",
            },
        },
        TestCase {
            name: "NetPBM P7 rgb alpha 16bit",
            path: "netpbm/P7_rgb_alpha_16bit.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/P7_rgb_alpha_16bit.avif",
            },
        },
        TestCase {
            name: "NetPBM 1c-1b",
            path: "netpbm/1c-1b.pbm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/1c-1b.avif",
            },
        },
        TestCase {
            name: "NetPBM 1c-8b",
            path: "netpbm/1c-8b.pgm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/1c-8b.avif",
            },
        },
        TestCase {
            name: "NetPBM 3c-8b",
            path: "netpbm/3c-8b.ppm",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/3c-8b.avif",
            },
        },
        TestCase {
            name: "NetPBM P1 multiframe",
            path: "netpbm/P1_multiframe.pbm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P2 multiframe",
            path: "netpbm/P2_multiframe.pgm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P3 multiframe",
            path: "netpbm/P3_multiframe.ppm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P4 multiframe",
            path: "netpbm/P4_multiframe.pbm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P5 multiframe",
            path: "netpbm/P5_multiframe.pgm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P6 multiframe",
            path: "netpbm/P6_multiframe.ppm",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P7 multiframe",
            path: "netpbm/P7_multiframe.pam",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 3 {
                    return Err(format!("expected 3 frames, got {}", frames.len()));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM P7 CMYK",
            path: "netpbm/cmyk.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/cmyk.jxl",
            },
        },
        TestCase {
            name: "NetPBM P7 CMYKA",
            path: "netpbm/cmyk_alpha.pam",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "netpbm/cmyk_alpha.jxl",
            },
        },
    ]
}
