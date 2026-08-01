#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

const IFD_OFFSET: u32 = 8;
const DNG_VERSION_TAG: u16 = 50706;

fn dng_skeleton(data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(data.len() + 26);
    v.extend_from_slice(b"II\x2A\x00");
    v.extend_from_slice(&IFD_OFFSET.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&DNG_VERSION_TAG.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&4u32.to_le_bytes());
    v.extend_from_slice(&[1, 4, 0, 0]);
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(data);
    v
}

fuzz_target!(|data: &[u8]| {
    let is_tiff = data.len() >= 4
        && (data.starts_with(b"II") || data.starts_with(b"MM"))
        && ((data[2] == 42 && data[3] == 0) || (data[2] == 0 && data[3] == 42));
    let input = if is_tiff { data.to_vec() } else { dng_skeleton(data) };
    if let Ok(mut decoder) = vexel::Vexel::new(Cursor::new(input)) {
        let _ = decoder.decode();
    }
});
