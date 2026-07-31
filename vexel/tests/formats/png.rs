use crate::harness::{Comparison, TestCase};

pub fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "PNG",
            path: "png/0b7d50ac449fd59eb3de00647636d0c9.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/0b7d50ac449fd59eb3de00647636d0c9.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/138331052d7c6e4acebfaa92af314e12.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/138331052d7c6e4acebfaa92af314e12.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/gray_8bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/gray_8bit.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/gray_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/gray_alpha_8bit.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_8bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/rgb_8bit.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/rgb_alpha_8bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/rgb_alpha_8bit.avif" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted truncated IDAT",
            path: "png/corrupted/broken.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/broken.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted stub IDAT",
            path: "png/corrupted/298272247-0f754cbc-b6ce-4e08-9e40-a2e1b427e705.png",
            validation: None,
            comparison: Comparison::Exact {
                reference_path: "png/corrupted/298272247-0f754cbc-b6ce-4e08-9e40-a2e1b427e705.jxl",
            },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted indexed",
            path: "png/corrupted/94e1bdbb03c42581d8407602634636ea.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/94e1bdbb03c42581d8407602634636ea.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted indexed",
            path: "png/corrupted/c-94e1bdbb03c42581d8407602634636ea.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-94e1bdbb03c42581d8407602634636ea.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted indexed",
            path: "png/corrupted/c-c53911b0385c34a8204c30fdc14ea5cc.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-c53911b0385c34a8204c30fdc14ea5cc.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted indexed",
            path: "png/corrupted/c-m1-559dcf17d281e285b7f09f943b9706de.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-m1-559dcf17d281e285b7f09f943b9706de.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted grayscale",
            path: "png/corrupted/c-m1-6bfb149151f58d124d6fa76eaad75520.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-m1-6bfb149151f58d124d6fa76eaad75520.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted RGB",
            path: "png/corrupted/c-m1-8f2b481b7fd9bd745e620b7c01a18df2.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-m1-8f2b481b7fd9bd745e620b7c01a18df2.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted interlaced grayscale",
            path: "png/corrupted/c-m2-0699098e769a2d80e60f33dbb3332b61.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/c-m2-0699098e769a2d80e60f33dbb3332b61.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted 16-bit grayscale alpha",
            path: "png/corrupted/gray_alpha_16bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/gray_alpha_16bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted unknown ancillary chunk",
            path: "png/corrupted/invalid-unknown-ancillary.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/invalid-unknown-ancillary.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted RGBA",
            path: "png/corrupted/p90_narrow_aisle_mainpic_1_large.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/p90_narrow_aisle_mainpic_1_large.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted 16-bit RGBA",
            path: "png/corrupted/rgb_alpha_16bit.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/rgb_alpha_16bit.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted color type 1",
            path: "png/corrupted/xc1n0g08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xc1n0g08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted color type 9",
            path: "png/corrupted/xc9n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xc9n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted CRC in IDAT",
            path: "png/corrupted/xcrn0g04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xcrn0g04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted CRC in IHDR",
            path: "png/corrupted/xcsn0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xcsn0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted bit depth 0",
            path: "png/corrupted/xd0n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xd0n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted bit depth 3",
            path: "png/corrupted/xd3n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xd3n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted bit depth 99",
            path: "png/corrupted/xd9n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xd9n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted missing IDAT",
            path: "png/corrupted/xdtn0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xdtn0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted incorrect IHDR checksum",
            path: "png/corrupted/xhdn0g08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xhdn0g08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted chunk length",
            path: "png/corrupted/xlfn0g04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xlfn0g04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted signature byte 1",
            path: "png/corrupted/xs1n0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xs1n0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted signature byte 2",
            path: "png/corrupted/xs2n0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xs2n0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted signature byte 4",
            path: "png/corrupted/xs4n0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xs4n0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG corrupted signature byte 7",
            path: "png/corrupted/xs7n0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/corrupted/xs7n0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/342083299-7b50019a-7c6f-4625-99c2-f1e69de95b61.png",
            validation: None,
            comparison: Comparison::ExactFrames {
                reference_path: "png/342083299-7b50019a-7c6f-4625-99c2-f1e69de95b61.jxl",
            },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/apng04.png",
            validation: None,
            comparison: Comparison::ExactFrames { reference_path: "png/apng04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/apng08.png",
            validation: None,
            comparison: Comparison::ExactFrames { reference_path: "png/apng08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/apng14.png",
            validation: None,
            comparison: Comparison::ExactFrames { reference_path: "png/apng14.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/apng18.png",
            validation: None,
            comparison: Comparison::ExactFrames { reference_path: "png/apng18.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG APNG animation",
            path: "png/apng.png",
            validation: None,
            comparison: Comparison::ExactFrames { reference_path: "png/apng.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/wm_upload_wikimedia_org_3a9fa5185de5c6c8.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/wm_upload_wikimedia_org_3a9fa5185de5c6c8.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG",
            path: "png/wm_upload_wikimedia_org_c8a458b0cef3d942.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/wm_upload_wikimedia_org_c8a458b0cef3d942.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG zlib compression level",
            path: "png/z00n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/z00n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG zlib compression level",
            path: "png/z03n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/z03n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG zlib compression level",
            path: "png/z06n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/z06n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG zlib compression level",
            path: "png/z09n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/z09n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi0g02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi0g02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi0g04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi0g04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi0g08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi0g08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi0g16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi0g16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi2c16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi2c16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi4a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi4a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi4a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi4a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi6a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi6a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG interlaced",
            path: "png/basi6a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basi6a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn0g01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn0g01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn0g02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn0g02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn0g04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn0g04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn0g08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn0g08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn0g16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn0g16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn2c16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn2c16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn3p04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn3p04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn4a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn4a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn4a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn4a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn6a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn6a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG basic",
            path: "png/basn6a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/basn6a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgai4a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgai4a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgai4a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgai4a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgan6a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgan6a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgan6a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgan6a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgbn4a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgbn4a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bggn4a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bggn4a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgwn6a08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgwn6a08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with bKGD background",
            path: "png/bgyn6a16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/bgyn6a16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with hIST/palette histogram",
            path: "png/ch1n3p04.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/ch1n3p04.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with hIST/palette histogram",
            path: "png/ch2n3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/ch2n3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs3n2c16.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs3n2c16.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs3n3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs3n3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs5n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs5n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs5n3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs5n3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs8n2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs8n2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with sBIT significant bits",
            path: "png/cs8n3p08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/cs8n3p08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG with EXIF",
            path: "png/exif2c08.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/exif2c08.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s01i3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s01i3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s01n3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s01n3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s02i3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s02i3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s02n3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s02n3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s03i3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s03i3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s03n3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s03n3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s04i3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s04i3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s04n3p01.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s04n3p01.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s05i3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s05i3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s05n3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s05n3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s06i3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s06i3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s06n3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s06n3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s07i3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s07i3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s07n3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s07n3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s08i3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s08i3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s08n3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s08n3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s09i3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s09i3p02.jxl" },
            ..Default::default()
        },
        TestCase {
            name: "PNG odd size",
            path: "png/s09n3p02.png",
            validation: None,
            comparison: Comparison::Exact { reference_path: "png/s09n3p02.jxl" },
            ..Default::default()
        },
    ]
}
