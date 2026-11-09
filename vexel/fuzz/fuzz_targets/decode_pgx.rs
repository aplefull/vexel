#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"PG ML ") || data.starts_with(b"PG LM ") {
        data.to_vec()
    } else {
        let mut v = b"PG ML 8 ".to_vec();
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
