#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        data.to_vec()
    } else {
        let mut v = vec![0x76, 0x2F, 0x31, 0x01];
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
