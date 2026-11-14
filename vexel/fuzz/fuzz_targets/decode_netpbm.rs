#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let magic = match data[0] % 9 {
        7 => "PF\n".to_string(),
        8 => "Pf\n".to_string(),
        variant => format!("P{}\n", variant + 1),
    };
    let mut input = magic.into_bytes();
    input.extend_from_slice(&data[1..]);
    vexel_fuzz::exercise(&input);
});
