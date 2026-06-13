use crate::harness::{Comparison, TestCase, get_pixel};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "GIF gray frames u1",
            path: "gif/gray_frames_u1.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/gray_frames_u1.avif",
            },
        },
        TestCase {
            name: "GIF totoro",
            path: "gif/totoro.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/totoro.avif",
            },
        },
        TestCase {
            name: "GIF totoro interlaced",
            path: "gif/totoro_interlaced.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/totoro_interlaced.avif",
            },
        },
        TestCase {
            name: "GIF totoro still",
            path: "gif/totoro_still.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/totoro_still.avif",
            },
        },
        TestCase {
            name: "GIF Australia history",
            path: "gif/Australia_history.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/Australia_history.avif",
            },
        },
        TestCase {
            name: "GIF 0646caeb9b9161c777f117007921a687",
            path: "gif/0646caeb9b9161c777f117007921a687.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/0646caeb9b9161c777f117007921a687.avif",
            },
        },
        TestCase {
            name: "GIF jazz-chromecast-ultra",
            path: "gif/jazz-chromecast-ultra.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/jazz-chromecast-ultra.avif",
            },
        },
        TestCase {
            name: "GIF 139770827-18e25c4e-eb0a-4058-ba48-ddc3849090ee",
            path: "gif/_corrupted/139770827-18e25c4e-eb0a-4058-ba48-ddc3849090ee.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/_corrupted/139770827-18e25c4e-eb0a-4058-ba48-ddc3849090ee.avif",
            },
        },
        TestCase {
            name: "GIF adaf0da1764aafb7039440dbe098569b",
            path: "gif/_corrupted/adaf0da1764aafb7039440dbe098569b.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/_corrupted/adaf0da1764aafb7039440dbe098569b.avif",
            },
        },
        TestCase {
            name: "GIF 243d9798466d64aba0acaa41f980bea6",
            path: "gif/_corrupted/243d9798466d64aba0acaa41f980bea6.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 7 {
                    return Err(format!("expected 7 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 7092f253998c1b6b869707ad7ae92854",
            path: "gif/_corrupted/7092f253998c1b6b869707ad7ae92854.gif",
            validation: Some(Box::new(|image| {
                let frames = image.frames();
                let count = frames.len();
                if count != 7 {
                    return Err(format!("expected 7 frames, got {}", count));
                }

                let w = frames[0].width() as usize;
                let h = frames[0].height() as usize;
                let (mx, my) = (w / 2, h / 2);

                let [r, g, b, a] = get_pixel(&frames[1].as_rgba8(), w, mx, my);
                if [r, g, b] != [0xE0, 0xE0, 0xB4] {
                    return Err(format!("frame 1 middle pixel RGB should be E0E0B4, got {:02X}{:02X}{:02X}", r, g, b));
                }

                if a != 0xFF {
                    return Err(format!("frame 1 middle pixel should be opaque, got alpha {}", a));
                }

                let [_, _, _, a_last] = get_pixel(&frames[5].as_rgba8(), w, mx, my);
                if a_last != 0x00 {
                    return Err(format!("frame 6 middle pixel should be transparent, got alpha {}", a_last));
                }

                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 6d939393058de0579fca1bbf10ecff25",
            path: "gif/_corrupted/6d939393058de0579fca1bbf10ecff25.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 3 {
                    return Err(format!("expected 3 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF ea754e040929b7f9c157efc88c4d0eaf",
            path: "gif/_corrupted/ea754e040929b7f9c157efc88c4d0eaf.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 11 {
                    return Err(format!("expected 11 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF ee6d1133f9264dc6467990e53d0bf104",
            path: "gif/_corrupted/ee6d1133f9264dc6467990e53d0bf104.gif",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "gif/_corrupted/ee6d1133f9264dc6467990e53d0bf104.avif",
            },
        },
        TestCase {
            name: "GIF f88b6907ee086c4c8ac4b8c395748c49",
            path: "gif/_corrupted/f88b6907ee086c4c8ac4b8c395748c49.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 67 {
                    return Err(format!("expected 67 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF fc3e2b992c559055267e26dc23e484c0",
            path: "gif/_corrupted/fc3e2b992c559055267e26dc23e484c0.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF adf6f850b13dff73ebb22862c6ab028b",
            path: "gif/_corrupted/adf6f850b13dff73ebb22862c6ab028b.gif",
            validation: None,
            comparison: Comparison::FuzzyFrames {
                reference_path: "gif/_corrupted/adf6f850b13dff73ebb22862c6ab028b.avif",
                // This one is corrupted, so there is a small pixel difference closer to the end of the image.
                mse_threshold: 17.0,
                ssim_threshold: 0.997,
            },
        },
        TestCase {
            name: "GIF bc7af0616c4ae99144c8600e7b39beea",
            path: "gif/_corrupted/bc7af0616c4ae99144c8600e7b39beea.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 5 {
                    return Err(format!("expected 5 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 9f8f6046eaf9ffa2d9c5d6db05c5f881",
            path: "gif/_corrupted/9f8f6046eaf9ffa2d9c5d6db05c5f881.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 55abb3cc464305dd554171c3d44cb61f",
            path: "gif/_corrupted/55abb3cc464305dd554171c3d44cb61f.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 2b5bc31d84703bfb9f371925f0e3e57d",
            path: "gif/_corrupted/2b5bc31d84703bfb9f371925f0e3e57d.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF 5f09a896c191db3fa7ea6bdd5ebe9485",
            path: "gif/_corrupted/5f09a896c191db3fa7ea6bdd5ebe9485.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF ce774930ac70449f38a18789c70095b8",
            path: "gif/_corrupted/ce774930ac70449f38a18789c70095b8.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 6 {
                    return Err(format!("expected 6 frames, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
        TestCase {
            name: "GIF d5a0175c07418852152ef33a886a5029",
            path: "gif/_corrupted/d5a0175c07418852152ef33a886a5029.gif",
            validation: Some(Box::new(|image| {
                let count = image.frames().len();
                if count != 1 {
                    return Err(format!("expected 1 frame, got {}", count));
                }
                Ok(())
            })),
            comparison: Comparison::None,
        },
    ]
}
