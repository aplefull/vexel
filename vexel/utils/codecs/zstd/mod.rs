mod bitstream;
mod frame;
mod fse;
mod huffman;
mod sequences;

pub struct ZstdDecoder<'a> {
    data: &'a [u8],
}

impl<'a> ZstdDecoder<'a> {
    pub fn from_slice(data: &'a [u8]) -> Self {
        ZstdDecoder { data }
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        let max_output = super::output_cap(capacity_hint);

        frame::decode(self.data, max_output)
    }
}
