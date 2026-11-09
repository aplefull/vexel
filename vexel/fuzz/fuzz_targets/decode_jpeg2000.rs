#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(&[0xFF, 0x4F, 0xFF, 0x51]) {
        data.to_vec()
    } else {
        let mut v = vec![0xFF, 0x4F, 0xFF, 0x51];
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(&input[..])) {
        let _ = decoder.get_info();
    }
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
