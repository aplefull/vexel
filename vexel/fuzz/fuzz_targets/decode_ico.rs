#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut input = vec![0x00, 0x00, 0x01, 0x00, 0x01, 0x00];
    input.extend_from_slice(data);
    vexel_fuzz::exercise(&input);
});
