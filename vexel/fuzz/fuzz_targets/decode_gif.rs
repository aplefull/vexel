#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        data.to_vec()
    } else {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
