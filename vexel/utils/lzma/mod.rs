mod bcj;
mod delta;
mod dictionary;
mod lzma1;
mod lzma2;
mod range_decoder;
mod xz;

const DEFAULT_MAX_OUTPUT: usize = 256 * 1024 * 1024;

pub struct XzDecoder<'a> {
    data: &'a [u8],
}

impl<'a> XzDecoder<'a> {
    pub fn from_slice(data: &'a [u8]) -> Self {
        XzDecoder { data }
    }

    pub fn decode(&self) -> Vec<u8> {
        self.decode_with_capacity(0)
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        let max_output = if capacity_hint > 0 { capacity_hint.saturating_mul(4) } else { DEFAULT_MAX_OUTPUT };

        xz::decode(self.data, capacity_hint, max_output)
    }
}
