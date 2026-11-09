#![no_main]

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    const FILE_ID: &[u8] = &[0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];

    let mut input = Vec::with_capacity(FILE_ID.len() + data.len() + 1);
    if data.starts_with(FILE_ID) {
        input.extend_from_slice(data);
    } else {
        input.extend_from_slice(FILE_ID);
        input.push(0x01);
        input.extend_from_slice(data);
    }

    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(&input[..])) {
        let _ = decoder.get_info();
    }
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode_pages();
    }
});
