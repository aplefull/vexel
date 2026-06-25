#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let input = if data.starts_with(b"BM")
        || data.starts_with(b"BA")
        || data.starts_with(b"CI")
        || data.starts_with(b"CP")
        || data.starts_with(b"IC")
        || data.starts_with(b"PT")
    {
        data.to_vec()
    } else {
        let mut v = b"BM".to_vec();
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
