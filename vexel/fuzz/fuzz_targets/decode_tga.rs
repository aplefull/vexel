#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let header: &[u8] = &[
        0x00,
        0x00,
        0x02,
        0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
        0x00, 0x00,
        0x01, 0x00,
        0x01, 0x00,
        0x18,
        0x00,
    ];
    let mut input = header.to_vec();
    input.extend_from_slice(data);
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
