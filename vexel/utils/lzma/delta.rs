pub const DIST_MIN: usize = 1;
pub const DIST_MAX: usize = 256;

pub fn decode_in_place(data: &mut [u8], distance: usize) {
    if !(DIST_MIN..=DIST_MAX).contains(&distance) {
        return;
    }

    let mut history = [0u8; DIST_MAX];
    let mut pos = 0usize;

    for byte in data.iter_mut() {
        *byte = byte.wrapping_add(history[distance.wrapping_add(pos) & 0xff]);
        history[pos & 0xff] = *byte;
        pos = pos.wrapping_sub(1);
    }
}
