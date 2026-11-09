#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"farbfeld") {
        data.to_vec()
    } else {
        let mut v = b"farbfeld".to_vec();
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
