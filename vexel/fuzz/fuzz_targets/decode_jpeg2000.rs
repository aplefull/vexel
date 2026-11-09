#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(&[0xFF, 0x4F, 0xFF, 0x51]) {
        data.to_vec()
    } else {
        let mut v = vec![0xFF, 0x4F, 0xFF, 0x51];
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
