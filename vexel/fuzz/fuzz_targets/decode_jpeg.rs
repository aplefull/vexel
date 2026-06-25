#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(&[0xFF, 0xD8]) {
        data.to_vec()
    } else {
        let mut v = vec![0xFF, 0xD8];
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
