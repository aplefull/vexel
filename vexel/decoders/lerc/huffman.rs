use super::bitstuffer::decode_bit_stuffed;
use super::reader::ByteReader;
use crate::utils::bits::MsbBitReader;
use crate::utils::entropy::prefix::PrefixCodeTable;
use crate::utils::error::{VexelError, VexelResult};

const MAX_HISTO_SIZE: i32 = 1 << 15;
const MAX_NUM_BITS_LUT: u32 = 12;
const MAX_CODE_LENGTH: u32 = 32;

fn index_wrap_around(i: i32, size: i32) -> i32 {
    i - if i < size { 0 } else { size }
}

pub fn word_swapped(data: &[u8]) -> Vec<u8> {
    let mut words = vec![0u8; data.len().div_ceil(4) * 4];
    words[..data.len()].copy_from_slice(data);
    for word in words.chunks_exact_mut(4) {
        word.reverse();
    }
    words
}

pub fn bytes_consumed(bits: &MsbBitReader) -> usize {
    (bits.position().div_ceil(32) as usize + 1) * 4
}

pub struct Huffman {
    table: PrefixCodeTable<u16>,
}

impl Huffman {
    pub fn read_code_table(reader: &mut ByteReader, lerc2_version: i32) -> VexelResult<Self> {
        let _version = reader.read_i32()?;
        let size = reader.read_i32()?;
        let i0 = reader.read_i32()?;
        let i1 = reader.read_i32()?;

        if i0 >= i1 || i0 < 0 || !(0..=MAX_HISTO_SIZE).contains(&size) {
            return Err(VexelError::Custom(format!(
                "Invalid LERC Huffman code table range: size {}, [{}, {})",
                size, i0, i1
            )));
        }

        if index_wrap_around(i0, size) >= size || index_wrap_around(i1 - 1, size) >= size {
            return Err(VexelError::Custom("LERC Huffman code table indexes out of range".to_string()));
        }

        let lengths = decode_bit_stuffed(reader, (i1 - i0) as usize, lerc2_version)?;

        let words = word_swapped(reader.tail());
        let mut bits = MsbBitReader::new(&words);
        let mut codes = Vec::new();

        for (i, &len) in (i0..i1).zip(lengths.iter().chain(std::iter::repeat(&0))) {
            if len == 0 {
                continue;
            }
            if len > MAX_CODE_LENGTH {
                return Err(VexelError::Custom(format!("LERC Huffman code length {} exceeds 32 bits", len)));
            }
            let symbol = index_wrap_around(i, size) as u16;
            codes.push((len, u64::from(bits.read(len)), symbol));
        }

        reader.skip(bits.position().div_ceil(32) as usize * 4);

        if codes.is_empty() {
            return Err(VexelError::Custom("LERC Huffman code table has no codes".to_string()));
        }

        Ok(Huffman { table: PrefixCodeTable::new(codes, MAX_NUM_BITS_LUT) })
    }

    pub fn decode(&self, bits: &mut MsbBitReader) -> VexelResult<i32> {
        if bits.exhausted() {
            return Err(VexelError::Custom("LERC Huffman stream exhausted".to_string()));
        }
        self.table
            .decode(bits)
            .map(i32::from)
            .ok_or_else(|| VexelError::Custom("Invalid LERC Huffman code".to_string()))
    }
}
