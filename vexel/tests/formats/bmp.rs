use crate::harness::{Comparison, TestCase, DEFAULT_MSE_THRESHOLD, DEFAULT_SSIM_THRESHOLD};

pub fn test_cases() -> Vec<TestCase> {
    vec![
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
    ]
}
