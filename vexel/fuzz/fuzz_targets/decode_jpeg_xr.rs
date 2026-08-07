#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"WMPHOTO\0") || data.starts_with(&[0x49, 0x49, 0xbc, 0x01]) {
        data.to_vec()
    } else {
        let mut v = b"WMPHOTO\0".to_vec();
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
