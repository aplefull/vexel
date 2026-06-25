#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(PNG_MAGIC) {
        data.to_vec()
    } else {
        let mut v = PNG_MAGIC.to_vec();
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
