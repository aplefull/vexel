#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let is_tiff = (data.starts_with(b"II") || data.starts_with(b"MM"))
        && data.len() >= 4
        && ((data[2] == 42 && data[3] == 0) || (data[2] == 0 && data[3] == 42));
    let input = if is_tiff {
        data.to_vec()
    } else {
        let mut v = vec![b'I', b'I', 42, 0];
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
