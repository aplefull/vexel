use crate::harness::{Comparison, TestCase, get_pixel};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "ICO image",
            path: "ico/image.cur",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "ico/image.avif",
            },
        },
        TestCase {
            name: "ICO sample",
            path: "ico/sample.ico",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "ico/sample.avif",
            },
        },
        TestCase {
            name: "ICO demo",
            path: "ico/demo.ico",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "ico/demo.avif",
            },
        },
        TestCase {
            name: "ICO demo BMP",
            path: "ico/demo_bmp.ico",
            validation: None,
                comparison: Comparison::ExactFrames {
                reference_path: "ico/demo_bmp.avif",
            },
        },
        TestCase {
            name: "ICO kuromi CUR",
            path: "ico/kuromi-2bd2e238.cur",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 7 {
                    return Err(format!("expected 7 frames, got {}", frames.len()));
                }

                let frame = &frames[0];
                let w = frame.width() as usize;
                let [r, g, b, _] = get_pixel(&frame.as_rgba8(), w, 16, 25);

                if [r, g, b] != [0x2E, 0x28, 0x3A] {
                    return Err(format!(
                        "frame 0: pixel (16, 25) expected #2E283A, got #{:02X}{:02X}{:02X}",
                        r, g, b
                    ));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "ICO max",
            path: "ico/max.ico",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != u16::MAX as usize {
                    return Err(format!("expected {} frames, got {}", u16::MAX, frames.len()));
                }
                for (i, frame) in frames.iter().enumerate() {
                    if frame.width() != 16 || frame.height() != 16 {
                        return Err(format!("frame {}: expected 16x16, got {}x{}", i, frame.width(), frame.height()));
                    }
                    let w = frame.width() as usize;
                    let pixels = frame.as_rgba8();
                    let [r, g, b, _] = get_pixel(&pixels, w, 8, 8);
                    let expected = if i % 2 == 0 { [0xFF, 0x00, 0x00] } else { [0x00, 0x00, 0x00] };
                    if [r, g, b] != expected {
                        return Err(format!(
                            "frame {}: expected #{:02X}{:02X}{:02X}, got #{:02X}{:02X}{:02X}",
                            i, expected[0], expected[1], expected[2], r, g, b
                        ));
                    }
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "ICO totoro",
            path: "ico/totoro.ico",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                if frames.len() != 4 {
                    return Err(format!("expected 4 frames, got {}", frames.len()));
                }
                let expected = [[0x72, 0x6C, 0x5D], [0x69, 0x65, 0x5B], [0x6A, 0x70, 0x55], [0x75, 0x6D, 0x5A]];
                for (i, frame) in frames.iter().enumerate() {
                    let w = frame.width() as usize;
                    let pixels = frame.as_rgba8();
                    let [_, _, _, a] = get_pixel(&pixels, w, 30, 30);
                    if a != 0x00 {
                        return Err(format!("frame {}: pixel (30, 30) expected transparent, got alpha={}", i, a));
                    }
                    let [r, g, b, _] = get_pixel(&pixels, w, 120, 120);
                    if [r, g, b] != expected[i] {
                        return Err(format!(
                            "frame {}: pixel (120, 120) expected #{:02X}{:02X}{:02X}, got #{:02X}{:02X}{:02X}",
                            i, expected[i][0], expected[i][1], expected[i][2], r, g, b
                        ));
                    }
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
    ]
}
