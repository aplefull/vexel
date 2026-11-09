#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"RPGMV\0\0\0") {
        data.to_vec()
    } else {
        let mut v = b"RPGMV\0\0\0".to_vec();
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
