mod harness;
mod corpus;

use std::path::Path;
use harness::*;
use vexel::Vexel;

fn load_env_file() {
    let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join(".env");
    if let Ok(contents) = std::fs::read_to_string(env_path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if std::env::var(key.trim()).is_err() {
                    std::env::set_var(key.trim(), value.trim());
                }
            }
        }
    }
}

#[test]
fn test_all_formats() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = vec![
        TestCase {
            name: "JPEG Baseline",
            path: "jpeg/cat.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG Lossless",
            path: "jpeg/2x2_lossless.jpg",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JPEG-LS",
            path: "jpeg-ls/test_4x4.jls",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "NetPBM",
            path: "netpbm/P3_16bit.ppm",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "BMP bmp0_test_image",
            path: "bmp/bmp0_test_image.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/bmp0_test_image.avif",
            },
        },
        TestCase {
            name: "BMP lena",
            path: "bmp/lena.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/lena.avif",
            },
        },
        TestCase {
            name: "BMP OS2_2",
            path: "bmp/OS2_2.BMP",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/OS2_2.avif",
            },
        },
        TestCase {
            name: "BMP Parrots",
            path: "bmp/Parrots.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/Parrots.avif",
            },
        },
        TestCase {
            name: "BMP rgb32bfdef",
            path: "bmp/rgb32bfdef.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32bfdef.avif",
            },
        },
        TestCase {
            name: "BMP rgb32",
            path: "bmp/rgb32.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32.avif",
            },
        },
        TestCase {
            name: "BMP rgb32-xbgr",
            path: "bmp/rgb32-xbgr.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32-xbgr.avif",
            },
        },
        TestCase {
            name: "BMP rgba32",
            path: "bmp/rgba32.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32.avif",
            },
        },
        TestCase {
            name: "BMP RLE4_2",
            path: "bmp/RLE4_2.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/RLE4_2.avif",
            },
        },
        TestCase {
            name: "BMP second_picture",
            path: "bmp/second_picture.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/second_picture.avif",
            },
        },
        TestCase {
            name: "BMP terrain2",
            path: "bmp/terrain2.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/terrain2.avif",
            },
        },
        TestCase {
            name: "BMP w3c_home",
            path: "bmp/w3c_home.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/w3c_home.avif",
            },
        },
        TestCase {
            name: "BMP wallpaper-image-cuba-05",
            path: "bmp/wallpaper-image-cuba-05.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/wallpaper-image-cuba-05.avif",
            },
        },
        TestCase {
            name: "BMP rgb16-231",
            path: "bmp/rgb16-231.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16-231.avif",
            },
        },
        TestCase {
            name: "BMP rgb16-3103",
            path: "bmp/rgb16-3103.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16-3103.avif",
            },
        },
        TestCase {
            name: "BMP rgb16-565",
            path: "bmp/rgb16-565.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16-565.avif",
            },
        },
        TestCase {
            name: "BMP rgb16-565pal",
            path: "bmp/rgb16-565pal.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16-565pal.avif",
            },
        },
        TestCase {
            name: "BMP rgb16bfdef",
            path: "bmp/rgb16bfdef.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16bfdef.avif",
            },
        },
        TestCase {
            name: "BMP rgb16",
            path: "bmp/rgb16.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16.avif",
            },
        },
        TestCase {
            name: "BMP rgb16faketrns",
            path: "bmp/rgb16faketrns.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb16faketrns.avif",
            },
        },
        TestCase {
            name: "BMP rgb24",
            path: "bmp/rgb24.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24.avif",
            },
        },
        TestCase {
            name: "BMP rgb24jpeg",
            path: "bmp/rgb24jpeg.bmp",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "bmp/rgb24jpeg.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "BMP rgb24largepal",
            path: "bmp/rgb24largepal.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24largepal.avif",
            },
        },
        TestCase {
            name: "BMP rgb24lprof",
            path: "bmp/rgb24lprof.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24lprof.avif",
            },
        },
        TestCase {
            name: "BMP rgb24pal",
            path: "bmp/rgb24pal.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24pal.avif",
            },
        },
        TestCase {
            name: "BMP rgb24png",
            path: "bmp/rgb24png.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24png.avif",
            },
        },
        TestCase {
            name: "BMP rgb24prof2",
            path: "bmp/rgb24prof2.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24prof2.avif",
            },
        },
        TestCase {
            name: "BMP rgb24prof",
            path: "bmp/rgb24prof.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb24prof.avif",
            },
        },
        TestCase {
            name: "BMP rgb32-111110",
            path: "bmp/rgb32-111110.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32-111110.avif",
            },
        },
        TestCase {
            name: "BMP rgb32-7187",
            path: "bmp/rgb32-7187.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32-7187.avif",
            },
        },
        TestCase {
            name: "BMP rgb32bf",
            path: "bmp/rgb32bf.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32bf.avif",
            },
        },
        TestCase {
            name: "BMP rgb32fakealpha",
            path: "bmp/rgb32fakealpha.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32fakealpha.avif",
            },
        },
        TestCase {
            name: "BMP rgb32h52",
            path: "bmp/rgb32h52.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgb32h52.avif",
            },
        },
        TestCase {
            name: "BMP rgba16-1924",
            path: "bmp/rgba16-1924.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba16-1924.avif",
            },
        },
        TestCase {
            name: "BMP rgba16-4444",
            path: "bmp/rgba16-4444.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba16-4444.avif",
            },
        },
        TestCase {
            name: "BMP rgba16-5551",
            path: "bmp/rgba16-5551.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba16-5551.avif",
            },
        },
        TestCase {
            name: "BMP rgba32-1010102",
            path: "bmp/rgba32-1010102.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32-1010102.avif",
            },
        },
        TestCase {
            name: "BMP rgba32-61754",
            path: "bmp/rgba32-61754.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32-61754.avif",
            },
        },
        TestCase {
            name: "BMP rgba32-81284",
            path: "bmp/rgba32-81284.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32-81284.avif",
            },
        },
        TestCase {
            name: "BMP rgba32abf",
            path: "bmp/rgba32abf.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32abf.avif",
            },
        },
        TestCase {
            name: "BMP rgba32h56",
            path: "bmp/rgba32h56.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/rgba32h56.avif",
            },
        },
        TestCase {
            name: "BMP sample_5184x3456",
            path: "bmp/sample_5184×3456.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/sample_5184×3456.avif",
            },
        },
        TestCase {
            name: "BMP YES2",
            path: "bmp/YES2.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/YES2.avif",
            },
        },
        TestCase {
            name: "BMP 11Bbos20",
            path: "bmp/11Bbos20.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/11Bbos20.avif",
            },
        },
        TestCase {
            name: "BMP 11Bgos20",
            path: "bmp/11Bgos20.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/11Bgos20.avif",
            },
        },
        TestCase {
            name: "BMP 11Bios13",
            path: "bmp/11Bios13.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/11Bios13.avif",
            },
        },
        TestCase {
            name: "BMP 11Bos20",
            path: "bmp/11Bos20.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/11Bos20.avif",
            },
        },
        TestCase {
            name: "BMP 400x300x32_v3_96",
            path: "bmp/400x300x32_v3_96.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/400x300x32_v3_96.avif",
            },
        },
        TestCase {
            name: "BMP 64bpp",
            path: "bmp/64bpp.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/64bpp.avif",
            },
        },
        TestCase {
            name: "BMP ba-bm",
            path: "bmp/ba-bm.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/ba-bm.avif",
            },
        },
        TestCase {
            name: "BMP pal2",
            path: "bmp/pal2.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/pal2.avif",
            },
        },
        TestCase {
            name: "BMP pal2color",
            path: "bmp/pal2color.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/pal2color.avif",
            },
        },
        TestCase {
            name: "BMP pal4rletrns",
            path: "bmp/pal4rletrns.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/pal4rletrns.avif",
            },
        },
        TestCase {
            name: "BMP pal8os2v2-16",
            path: "bmp/pal8os2v2-16.bmp",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "bmp/pal8os2v2-16.avif",
            },
        },
        TestCase {
            name: "PNG",
            path: "png/0b7d50ac449fd59eb3de00647636d0c9.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/0b7d50ac449fd59eb3de00647636d0c9.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/138331052d7c6e4acebfaa92af314e12.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/138331052d7c6e4acebfaa92af314e12.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/gray_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/gray_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_8bit.avif"
            },
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/rgb_alpha_8bit.avif"
            },
        },
        TestCase {
            name: "HDR",
            path: "hdr/sample_HDR.hdr",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "TIFF",
            path: "tiff/file_example_TIFF_10MB.tiff",
            validation: None,
            comparison: Comparison::None,
        },
        TestCase {
            name: "JBIG1 2x2 Checkerboard",
            path: "jbig1/2x2.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/2x2.avif",
            },
        },
        TestCase {
            name: "JBIG1 ccitt1",
            path: "jbig1/ccitt1.jbg",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "jbig1/ccitt1.avif",
            },
        },
        TestCase {
            name: "TGA ctc32",
            path: "tga/ctc32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/ctc32.avif",
            },
        },
        TestCase {
            name: "TGA flag_t32",
            path: "tga/flag_t32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/flag_t32.avif",
            },
        },
        TestCase {
            name: "TGA lena3",
            path: "tga/lena3.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/lena3.avif",
            },
        },
        TestCase {
            name: "TGA rgb15rle",
            path: "tga/rgb15rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb15rle.avif",
            },
        },
        TestCase {
            name: "TGA rgb32rle",
            path: "tga/rgb32rle.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/rgb32rle.avif",
            },
        },
        TestCase {
            name: "TGA xing_b32",
            path: "tga/xing_b32.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/xing_b32.avif",
            },
        },
        TestCase {
            name: "TGA cbw8",
            path: "tga/cbw8.tga",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "tga/cbw8.avif",
            },
        },
        TestCase {
            name: "JPEG arithmetic (cat)",
            path: "jpeg/cat_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/cat_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (2x2)",
            path: "jpeg/2x2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/2x2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (rainbow)",
            path: "jpeg/arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo1)",
            path: "jpeg/demo1_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo1_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (demo2)",
            path: "jpeg/demo2_arithmetic.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/demo2_arithmetic.avif",
                mse_threshold: DEFAULT_MSE_THRESHOLD,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
        TestCase {
            name: "JPEG arithmetic (progressive)",
            path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.jpg",
            validation: None,
            comparison: Comparison::Fuzzy {
                reference_path: "jpeg/9bccc4d2-c0de-11e6-8e21-b3f52f1d0eba.avif",
                // Arithmetic decoder seems to match libjpeg exactly, but we are doing IDCT differently, so final
                // image differs slightly.
                // TODO: Maybe switch to integer IDCT as well?
                mse_threshold: 0.9,
                ssim_threshold: DEFAULT_SSIM_THRESHOLD,
            },
        },
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
        }
    ];

    let name_width = test_cases.iter().map(|t| t.name.len()).max().unwrap_or(0);

    let mut any_failed = false;
    for test_case in test_cases {
        let name = test_case.name;
        match test_decode(test_case) {
            Err(e) => {
                println!("  {:<width$}  FAIL  {}", name, e, width = name_width);
                any_failed = true;
            }
            Ok(harness::TestResult::Fail(msg)) => {
                println!("  {:<width$}  FAIL  {}", name, msg, width = name_width);
                any_failed = true;
            }
            Ok(harness::TestResult::Ok { mse: None, ssim: None, psnr: None }) => {
                println!("  {:<width$}  OK", name, width = name_width);
            }
            Ok(harness::TestResult::Ok { mse, ssim, psnr }) => {
                let mse_str = mse.map(|v| format!("MSE={:.5}", v)).unwrap_or_default();
                let ssim_str = ssim.map(|v| format!("SSIM={:.6}", v)).unwrap_or_default();
                let psnr_str = psnr.map(|v| match v.is_infinite() {
                    true => "PSNR=∞ dB".to_string(),
                    false => format!("PSNR={:.2} dB", v),
                }).unwrap_or_default();
                println!("  {:<width$}  OK    {} {} {}", name, mse_str, ssim_str, psnr_str, width = name_width);
            }
        }
    }

    if any_failed {
        return Err("one or more test cases failed".into());
    }

    Ok(())
}

#[test]
#[ignore = "dev only"]
// This test is used during development for convenience for any new image formats
pub fn test_image() -> Result<(), Box<dyn std::error::Error>> {
    let in_path = r"/home/aplefull/Repos/vexel/vexel/tests/images/jpeg";
    let out_path = Path::new(in_path).with_extension("avif");
    let save = true; 

    let mut decoder = Vexel::open(in_path)?;

    match decoder.decode() {
        Ok(image) => {
            if !save {
                println!("Decoded image: {}x{}, {} frames", image.width(), image.height(), image.frames().len());
                return Ok(());
            }

            if image.frames().len() > 1 {
                let frames = image.frames();
                for (i, frame) in frames.iter().enumerate() {
                    let frame_out_path = out_path.with_file_name(format!(
                        "{}_frame{}.avif",
                        out_path.file_stem().unwrap().to_string_lossy(),
                        i
                    ));

                    let avif_data = libavif::encode_rgb8(frame.width(), frame.height(), &frame.as_rgba8())?;
                    std::fs::write(frame_out_path, avif_data.as_ref())?;
                }
            } else {
                let avif_data = libavif::encode_rgb8(image.width(), image.height(), &image.as_rgba8())?;
                std::fs::write(out_path, avif_data.as_ref())?;
            }
        }
        Err(e) => {
            println!("Error decoding image: {:?}", e);
            assert!(false);
        }
    }

    Ok(())
}

#[test]
#[ignore = "corpus bench"]
pub fn corpus_bench() -> Result<(), Box<dyn std::error::Error>> {
    load_env_file();
    let corpus_path = std::env::var("VEXEL_CORPUS")
        .map_err(|_| "VEXEL_CORPUS is not set. Add it to .env")?;
    corpus::run(&corpus_path)
}
