#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        data.to_vec()
    } else {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(data);
        v
    };
    vexel_fuzz::exercise(&input);
});
