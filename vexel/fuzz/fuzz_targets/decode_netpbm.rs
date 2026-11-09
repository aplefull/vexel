#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let variant = (data[0] % 7) + 1;
    let magic = format!("P{}\n", variant);
    let mut input = magic.into_bytes();
    input.extend_from_slice(&data[1..]);
    vexel_fuzz::exercise(&input);
});
