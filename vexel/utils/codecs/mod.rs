pub mod ccitt;
pub mod deflate;
pub mod lzma;
pub mod lzw;
pub mod rle;
pub mod zstd;

const DEFAULT_MAX_OUTPUT: usize = 256 * 1024 * 1024;

pub fn output_cap(capacity_hint: usize) -> usize {
    if capacity_hint > 0 {
        capacity_hint.saturating_mul(4)
    } else {
        DEFAULT_MAX_OUTPUT
    }
}

pub fn pad_to(mut data: Vec<u8>, expected: usize, what: &str) -> Vec<u8> {
    if data.len() < expected {
        crate::log_warn!("{} produced {} of {} expected bytes, padding with zeros", what, data.len(), expected);
        data.resize(expected, 0);
    }
    data.truncate(expected);
    data
}
