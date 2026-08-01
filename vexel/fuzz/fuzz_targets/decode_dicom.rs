#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let has_preamble = data.len() >= 132 && &data[128..132] == b"DICM";
    let input = if has_preamble {
        data.to_vec()
    } else {
        let mut v = vec![0u8; 128];
        v.extend_from_slice(b"DICM");
        v.extend_from_slice(data);
        v
    };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
