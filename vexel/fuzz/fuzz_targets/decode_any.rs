#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(data)) {
        let _ = decoder.decode();
    }
});
