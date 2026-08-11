use super::bitstuffer::decode_bit_stuffed;
use super::reader::ByteReader;
use crate::utils::error::{VexelError, VexelResult};

const MAX_HISTO_SIZE: i32 = 1 << 15;
const MAX_NUM_BITS_LUT: u32 = 12;

fn index_wrap_around(i: i32, size: i32) -> i32 {
    i - if i < size { 0 } else { size }
}

#[derive(Default, Clone, Copy)]
struct TreeNode {
    child0: i32,
    child1: i32,
    value: i32,
}

impl TreeNode {
    fn empty() -> Self {
        TreeNode { child0: -1, child1: -1, value: -1 }
    }
}

pub struct Huffman {
    code_table: Vec<(u16, u32)>,
    decode_lut: Vec<(i16, i16)>,
    num_bits_lut: u32,
    num_bits_to_skip_in_tree: u32,
    tree: Vec<TreeNode>,
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

        let count = (i1 - i0) as usize;
        let lengths = decode_bit_stuffed(reader, count, lerc2_version)?;

        let mut code_table = vec![(0u16, 0u32); size as usize];

        for i in i0..i1 {
            let k = index_wrap_around(i, size) as usize;
            let len = lengths.get((i - i0) as usize).copied().unwrap_or(0);
            code_table[k].0 = len.min(u16::MAX as u32) as u16;
        }

        let mut huffman = Huffman {
            code_table,
            decode_lut: Vec::new(),
            num_bits_lut: 0,
            num_bits_to_skip_in_tree: 0,
            tree: Vec::new(),
        };

        huffman.bit_unstuff_codes(reader, i0, i1)?;
        huffman.build_tree_from_codes()?;

        Ok(huffman)
    }

    fn bit_unstuff_codes(&mut self, reader: &mut ByteReader, i0: i32, i1: i32) -> VexelResult<()> {
        let size = self.code_table.len() as i32;
        let start = reader.position();
        let mut offset = 0usize;
        let mut bit_pos = 0u32;

        for i in i0..i1 {
            let k = index_wrap_around(i, size) as usize;
            let len = self.code_table[k].0 as u32;

            if len == 0 {
                continue;
            }

            if len > 32 {
                return Err(VexelError::Custom(format!("LERC Huffman code length {} exceeds 32 bits", len)));
            }

            let word = read_u32_at(reader.tail(), offset);
            self.code_table[k].1 = (word << bit_pos) >> (32 - len);

            if 32 - bit_pos >= len {
                bit_pos += len;
                if bit_pos == 32 {
                    bit_pos = 0;
                    offset += 4;
                }
            } else {
                bit_pos = bit_pos.wrapping_add(len).wrapping_sub(32);
                offset += 4;
                let next = read_u32_at(reader.tail(), offset);
                self.code_table[k].1 |= next >> (32 - bit_pos);
            }
        }

        let consumed = offset + if bit_pos > 0 { 4 } else { 0 };
        reader.seek(start + consumed);

        Ok(())
    }

    fn build_tree_from_codes(&mut self) -> VexelResult<()> {
        let size = self.code_table.len() as i32;
        let (i0, i1, max_len) = self.get_range()?;

        let need_tree = max_len > MAX_NUM_BITS_LUT;
        self.num_bits_lut = max_len.min(MAX_NUM_BITS_LUT);

        let size_lut = 1usize << self.num_bits_lut;
        self.decode_lut = vec![(-1i16, -1i16); size_lut];

        let mut min_num_zero_bits = 32u32;

        for i in i0..i1 {
            let k = index_wrap_around(i, size) as usize;
            let len = self.code_table[k].0 as u32;

            if len == 0 {
                continue;
            }

            let code = self.code_table[k].1;

            if len <= self.num_bits_lut {
                let shifted = (code << (self.num_bits_lut - len)) as usize;
                let num_entries = 1usize << (self.num_bits_lut - len);
                let entry = (len as i16, k as i16);

                for j in 0..num_entries {
                    if let Some(slot) = self.decode_lut.get_mut(shifted | j) {
                        *slot = entry;
                    }
                }
            } else {
                let mut shift = 1u32;
                let mut c = code;
                while {
                    c >>= 1;
                    c != 0
                } {
                    shift += 1;
                }
                min_num_zero_bits = min_num_zero_bits.min(len - shift);
            }
        }

        self.num_bits_to_skip_in_tree = if need_tree { min_num_zero_bits } else { 0 };

        if !need_tree {
            return Ok(());
        }

        self.tree.clear();
        self.tree.push(TreeNode::empty());

        for i in i0..i1 {
            let k = index_wrap_around(i, size) as usize;
            let len = self.code_table[k].0 as u32;

            if len == 0 || len <= self.num_bits_lut {
                continue;
            }

            let code = self.code_table[k].1;
            let mut node = 0usize;
            let mut j = len as i64 - self.num_bits_to_skip_in_tree as i64;

            while j > 0 {
                j -= 1;
                let bit = (code >> j as u32) & 1;

                let next = if bit != 0 { self.tree[node].child1 } else { self.tree[node].child0 };

                let next = if next < 0 {
                    self.tree.push(TreeNode::empty());
                    let created = (self.tree.len() - 1) as i32;
                    if bit != 0 {
                        self.tree[node].child1 = created;
                    } else {
                        self.tree[node].child0 = created;
                    }
                    created
                } else {
                    next
                };

                node = next as usize;

                if j == 0 {
                    self.tree[node].value = k as i32;
                }
            }
        }

        Ok(())
    }

    fn get_range(&self) -> VexelResult<(i32, i32, u32)> {
        let size = self.code_table.len() as i32;

        if size == 0 {
            return Err(VexelError::Custom("Empty LERC Huffman code table".to_string()));
        }

        let mut max_len = 0u32;
        for entry in &self.code_table {
            max_len = max_len.max(entry.0 as u32);
        }

        if max_len == 0 {
            return Err(VexelError::Custom("LERC Huffman code table has no codes".to_string()));
        }

        let mut i0 = 0i32;
        let mut i1 = size;

        let mut lead = 0i32;
        while lead < size && self.code_table[lead as usize].0 == 0 {
            lead += 1;
        }

        let mut trail = size - 1;
        while trail >= 0 && self.code_table[trail as usize].0 == 0 {
            trail -= 1;
        }

        if lead <= trail {
            i0 = lead;
            i1 = trail + 1;
        }

        Ok((i0, i1, max_len))
    }
}

fn read_u32_at(data: &[u8], offset: usize) -> u32 {
    let mut bytes = [0u8; 4];
    let available = data.len().saturating_sub(offset).min(4);
    if available > 0 {
        bytes[..available].copy_from_slice(&data[offset..offset + available]);
    }
    u32::from_le_bytes(bytes)
}

pub struct HuffmanBitStream<'a> {
    data: &'a [u8],
    offset: usize,
    bit_pos: u32,
}

impl<'a> HuffmanBitStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        HuffmanBitStream { data, offset: 0, bit_pos: 0 }
    }

    fn advance_word(&mut self) {
        self.offset = self.offset.saturating_add(4);
    }

    fn current(&self) -> u32 {
        read_u32_at(self.data, self.offset)
    }

    fn next_word(&self) -> u32 {
        read_u32_at(self.data, self.offset + 4)
    }

    pub fn decode_one(&mut self, huffman: &Huffman) -> VexelResult<i32> {
        if self.offset >= self.data.len() {
            return Err(VexelError::Custom("LERC Huffman stream exhausted".to_string()));
        }

        let num_bits_lut = huffman.num_bits_lut;
        let temp = self.current();
        let mut val_tmp = ((temp << self.bit_pos) >> (32 - num_bits_lut)) as usize;

        if 32 - self.bit_pos < num_bits_lut {
            let next = self.next_word();
            val_tmp |= (next >> (64 - self.bit_pos - num_bits_lut)) as usize;
        }

        if let Some(&(len, value)) = huffman.decode_lut.get(val_tmp) {
            if len >= 0 {
                self.bit_pos += len as u32;
                if self.bit_pos >= 32 {
                    self.bit_pos -= 32;
                    self.advance_word();
                }
                return Ok(value as i32);
            }
        }

        if huffman.tree.is_empty() {
            return Err(VexelError::Custom("LERC Huffman code not in LUT and no tree available".to_string()));
        }

        self.bit_pos += huffman.num_bits_to_skip_in_tree;
        if self.bit_pos >= 32 {
            self.bit_pos -= 32;
            self.advance_word();
        }

        let mut node = 0usize;

        loop {
            if self.offset >= self.data.len() {
                return Err(VexelError::Custom("LERC Huffman stream exhausted in tree walk".to_string()));
            }

            let temp = self.current();
            let bit = (temp << self.bit_pos) >> 31;
            self.bit_pos += 1;
            if self.bit_pos == 32 {
                self.bit_pos = 0;
                self.advance_word();
            }

            let next = if bit != 0 { huffman.tree[node].child1 } else { huffman.tree[node].child0 };

            if next < 0 {
                return Err(VexelError::Custom("Invalid LERC Huffman code".to_string()));
            }

            node = next as usize;

            if huffman.tree[node].value >= 0 {
                return Ok(huffman.tree[node].value);
            }
        }
    }

    pub fn bytes_consumed(&self) -> usize {
        self.offset + if self.bit_pos > 0 { 4 } else { 0 } + 4
    }
}
