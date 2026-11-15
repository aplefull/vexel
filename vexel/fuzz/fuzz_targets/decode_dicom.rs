#![no_main]

use libfuzzer_sys::fuzz_target;
use vexel::ImageFormat;

fuzz_target!(|data: &[u8]| {
    vexel_fuzz::exercise_format(data, ImageFormat::Dicom);
});
