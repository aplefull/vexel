use std::io;

const MAX_BITS: usize = 15;
const CODELEN_SYMBOLS: usize = 19;

const PRIMARY_BITS: u32 = 9;
const PRIMARY_TABLE_SIZE: usize = 1 << PRIMARY_BITS;

const MAX_SECONDARY_BITS: u32 = MAX_BITS as u32 - PRIMARY_BITS;

#[rustfmt::skip]
const LENGTH_BASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
    35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];

#[rustfmt::skip]
const LENGTH_EXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
    3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

#[rustfmt::skip]
const DIST_BASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
    257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

#[rustfmt::skip]
const DIST_EXTRA: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
    7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

#[rustfmt::skip]
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const FIXED_LITLEN_LENGTHS: [u8; 288] = {
    let mut lengths = [0u8; 288];
    let mut i = 0;
    while i < 144 { lengths[i] = 8; i += 1; }
    while i < 256 { lengths[i] = 9; i += 1; }
    while i < 280 { lengths[i] = 7; i += 1; }
    while i < 288 { lengths[i] = 8; i += 1; }
    lengths
};

const FIXED_DIST_LENGTHS: [u8; 32] = [5u8; 32];

const TAG_INVALID: u32 = 0;
const TAG_LITERAL: u32 = 1;
const TAG_EOB: u32 = 2;
const TAG_LENGTH: u32 = 3;
const TAG_DIST: u32 = 4;
const TAG_CLSYM: u32 = 5;
const TAG_REDIRECT: u32 = 6;

#[derive(Clone, Copy, Debug)]
struct Entry(u32);

impl Entry {
    const INVALID: Entry = Entry(TAG_INVALID);

    #[inline(always)]
    fn make(tag: u32, payload: u32, extra: u32, code_len: u32) -> Self {
        Entry(tag << 28 | (payload & 0xFFFF) << 12 | (extra & 0xF) << 8 | (code_len & 0xFF))
    }

    fn literal(byte: u8, code_len: u32) -> Self {
        Self::make(TAG_LITERAL, byte as u32, 0, code_len)
    }

    fn end_of_block(code_len: u32) -> Self {
        Self::make(TAG_EOB, 0, 0, code_len)
    }

    fn length_symbol(idx: u32, code_len: u32) -> Self {
        Self::make(TAG_LENGTH, idx, 0, code_len)
    }

    fn dist_symbol(idx: u32, code_len: u32) -> Self {
        Self::make(TAG_DIST, idx, 0, code_len)
    }

    fn cl_symbol(sym: u8, code_len: u32) -> Self {
        Self::make(TAG_CLSYM, sym as u32, 0, code_len)
    }

    fn redirect(sec_base: u32, sec_bits: u32, primary_consumed: u32) -> Self {
        Entry(TAG_REDIRECT << 28 | (sec_base & 0xFFFFF) << 8 | (sec_bits & 0xF) << 4 | (primary_consumed & 0xF))
    }

    #[inline(always)]
    fn tag(self) -> u32 {
        self.0 >> 28
    }

    #[inline(always)]
    fn is_invalid(self) -> bool {
        self.tag() == TAG_INVALID
    }

    #[inline(always)]
    fn is_redirect(self) -> bool {
        self.tag() == TAG_REDIRECT
    }

    #[inline(always)]
    fn is_literal(self) -> bool {
        self.tag() == TAG_LITERAL
    }

    #[inline(always)]
    fn is_end_of_block(self) -> bool {
        self.tag() == TAG_EOB
    }

    #[inline(always)]
    fn is_length(self) -> bool {
        self.tag() == TAG_LENGTH
    }

    #[inline(always)]
    fn code_len(self) -> u32 {
        self.0 & 0xFF
    }

    #[inline(always)]
    fn payload(self) -> u32 {
        (self.0 >> 12) & 0xFFFF
    }

    fn redirect_base(self) -> usize {
        ((self.0 >> 8) & 0xFFFFF) as usize
    }

    fn redirect_sec_bits(self) -> u32 {
        (self.0 >> 4) & 0xF
    }

    fn redirect_primary_consumed(self) -> u32 {
        self.0 & 0xF
    }
}

#[inline(always)]
fn reverse_bits(code: u32, len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let shifted = code << (32 - len);
    shifted.reverse_bits()
}

struct HuffTable {
    primary: [Entry; PRIMARY_TABLE_SIZE],
    secondary: Vec<Entry>,
    primary_bits: u32,
}

enum SymbolKind {
    Literal,
    Dist,
    CodeLen,
}

impl HuffTable {
    fn new_litlen() -> Box<Self> {
        Box::new(HuffTable {
            primary: [Entry::INVALID; PRIMARY_TABLE_SIZE],
            secondary: Vec::new(),
            primary_bits: PRIMARY_BITS,
        })
    }

    fn new_dist() -> Box<Self> {
        Box::new(HuffTable {
            primary: [Entry::INVALID; PRIMARY_TABLE_SIZE],
            secondary: Vec::new(),
            primary_bits: PRIMARY_BITS,
        })
    }

    fn new_cl() -> Box<Self> {
        Box::new(HuffTable {
            primary: [Entry::INVALID; PRIMARY_TABLE_SIZE],
            secondary: Vec::new(),
            primary_bits: 7,
        })
    }

    fn build(&mut self, lengths: &[u8], kind: SymbolKind) -> Result<(), io::Error> {
        let n_symbols = lengths.len();

        let mut bl_count = [0u32; MAX_BITS + 1];
        for &len in lengths {
            if len as usize > MAX_BITS {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Huffman code length exceeds max"));
            }
            if len > 0 {
                bl_count[len as usize] += 1;
            }
        }

        let mut next_code = [0u32; MAX_BITS + 1];
        let mut code = 0u32;
        for bits in 1..=MAX_BITS {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        for i in 0..n_symbols {
            let len = lengths[i] as u32;
            if len == 0 {
                continue;
            }

            let sym_code = next_code[len as usize];
            next_code[len as usize] += 1;

            let entry = match kind {
                SymbolKind::Literal => {
                    if i < 256 {
                        Entry::literal(i as u8, len)
                    } else if i == 256 {
                        Entry::end_of_block(len)
                    } else if i <= 285 {
                        Entry::length_symbol((i - 257) as u32, len)
                    } else {
                        continue;
                    }
                }
                SymbolKind::Dist => {
                    if i < 30 {
                        Entry::dist_symbol(i as u32, len)
                    } else {
                        continue;
                    }
                }
                SymbolKind::CodeLen => Entry::cl_symbol(i as u8, len),
            };

            self.insert(sym_code, len, entry)?;
        }

        Ok(())
    }

    fn insert(&mut self, code: u32, len: u32, entry: Entry) -> Result<(), io::Error> {
        let pb = self.primary_bits;

        if len <= pb {
            let fill = 1u32 << (pb - len);
            let rev = reverse_bits(code, len);
            for k in 0..fill {
                let idx = (rev | (k << len)) as usize;
                if idx < PRIMARY_TABLE_SIZE {
                    self.primary[idx] = entry;
                }
            }
        } else {
            let overflow_bits = len - pb;
            let primary_code = code >> overflow_bits;
            let primary_rev = reverse_bits(primary_code, pb) as usize;

            let overflow_code = code & ((1 << overflow_bits) - 1);
            let overflow_rev = reverse_bits(overflow_code, overflow_bits);

            let (sec_base, sec_bits) = match self.primary[primary_rev] {
                e if e.is_redirect() => (e.redirect_base(), e.redirect_sec_bits()),
                e if e.is_invalid() => {
                    let base = self.secondary.len();
                    let bits = MAX_SECONDARY_BITS;
                    let size = 1usize << bits;
                    self.secondary.resize(base + size, Entry::INVALID);
                    self.primary[primary_rev] = Entry::redirect(base as u32, bits, pb);
                    (base, bits)
                }
                _ => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Huffman table insert conflict"));
                }
            };

            let fill = 1u32 << (sec_bits - overflow_bits);
            for k in 0..fill {
                let idx = sec_base + ((overflow_rev | (k << overflow_bits)) as usize & ((1 << sec_bits) - 1));
                if idx < sec_base + (1 << sec_bits) && idx < self.secondary.len() {
                    self.secondary[idx] = entry;
                }
            }
        }

        Ok(())
    }

    fn decode(&self, bits: u32, n_bits: u32) -> (Entry, u32) {
        if n_bits == 0 {
            return (Entry::INVALID, 0);
        }

        let primary_mask = (1u32 << self.primary_bits) - 1;
        let idx = (bits & primary_mask) as usize;
        let entry = self.primary[idx];

        if entry.is_redirect() {
            let sec_base = entry.redirect_base();
            let sec_bits = entry.redirect_sec_bits();
            let consumed_primary = entry.redirect_primary_consumed();

            if n_bits < consumed_primary {
                return (Entry::INVALID, 0);
            }

            let sec_mask = (1u32 << sec_bits) - 1;
            let sec_idx = sec_base + ((bits >> consumed_primary) & sec_mask) as usize;

            let sec_entry = if sec_idx < self.secondary.len() {
                self.secondary[sec_idx]
            } else {
                Entry::INVALID
            };

            if sec_entry.is_invalid() {
                return (Entry::INVALID, 0);
            }

            let total = sec_entry.code_len();
            if total > n_bits {
                return (Entry::INVALID, 0);
            }

            (sec_entry, total)
        } else if entry.is_invalid() {
            (Entry::INVALID, 0)
        } else {
            let code_len = entry.code_len();
            if code_len > n_bits {
                return (Entry::INVALID, 0);
            }
            (entry, code_len)
        }
    }
}

struct BitBuffer {
    buf: u64,
    count: u32,
    src: Vec<u8>,
    pos: usize,
}

impl BitBuffer {
    fn new(src: Vec<u8>) -> Self {
        BitBuffer { buf: 0, count: 0, src, pos: 0 }
    }

    #[inline(always)]
    fn fill(&mut self) {
        while self.count <= 56 && self.pos < self.src.len() {
            self.buf |= (self.src[self.pos] as u64) << self.count;
            self.count += 8;
            self.pos += 1;
        }
    }

    #[inline(always)]
    fn peek(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.buf & ((1u64 << n) - 1)) as u32
    }

    #[inline(always)]
    fn consume(&mut self, n: u32) {
        debug_assert!(n <= self.count);
        self.buf >>= n;
        self.count -= n;
    }

    #[inline(always)]
    fn read_bits(&mut self, n: u32) -> Result<u32, io::Error> {
        if n == 0 {
            return Ok(0);
        }
        self.fill();
        if self.count < n {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Not enough bits"));
        }
        let val = self.peek(n);
        self.consume(n);
        Ok(val)
    }

    fn align_to_byte(&mut self) {
        let rem = self.count % 8;
        if rem != 0 {
            self.buf >>= rem;
            self.count -= rem;
        }
    }

    fn read_byte_aligned(&mut self) -> Result<u8, io::Error> {
        if self.count >= 8 {
            let b = (self.buf & 0xFF) as u8;
            self.buf >>= 8;
            self.count -= 8;
            return Ok(b);
        }
        if self.pos >= self.src.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF"));
        }
        let b = self.src[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u16_le_aligned(&mut self) -> Result<u16, io::Error> {
        let lo = self.read_byte_aligned()? as u16;
        let hi = self.read_byte_aligned()? as u16;
        Ok(lo | (hi << 8))
    }

    fn decode_symbol(&mut self, table: &HuffTable) -> Result<Entry, io::Error> {
        self.fill();
        let avail = self.count.min(15);
        let (entry, consumed) = table.decode(self.peek(avail), avail);

        if entry.is_invalid() || consumed == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad Huffman code"));
        }

        self.consume(consumed);
        Ok(entry)
    }
}

fn build_dynamic_tables(bits: &mut BitBuffer) -> Result<(Box<HuffTable>, Box<HuffTable>), io::Error> {
    let hlit = bits.read_bits(5)? as usize + 257;
    let hdist = bits.read_bits(5)? as usize + 1;
    let hclen = bits.read_bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; CODELEN_SYMBOLS];
    for i in 0..hclen {
        cl_lengths[CODE_LENGTH_ORDER[i]] = bits.read_bits(3)? as u8;
    }

    let mut cl_table = HuffTable::new_cl();
    cl_table.build(&cl_lengths, SymbolKind::CodeLen)?;

    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        bits.fill();
        let avail = bits.count.min(7);
        if avail == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Not enough bits for code lengths"));
        }
        let (entry, consumed) = cl_table.decode(bits.peek(avail), avail);
        if entry.is_invalid() || consumed == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad code-length Huffman code"));
        }
        bits.consume(consumed);

        let sym = entry.payload() as u8;

        match sym {
            0..=15 => {
                lengths[i] = sym;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Repeat with no previous length"));
                }
                let rep = bits.read_bits(2)? as usize + 3;
                let prev = lengths[i - 1];
                let end = (i + rep).min(total);
                lengths[i..end].fill(prev);
                i = end;
            }
            17 => {
                let rep = bits.read_bits(3)? as usize + 3;
                let end = (i + rep).min(total);
                lengths[i..end].fill(0);
                i = end;
            }
            18 => {
                let rep = bits.read_bits(7)? as usize + 11;
                let end = (i + rep).min(total);
                lengths[i..end].fill(0);
                i = end;
            }
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad code-length symbol"));
            }
        }
    }

    let mut litlen = HuffTable::new_litlen();
    litlen.build(&lengths[..hlit], SymbolKind::Literal)?;

    let mut dist = HuffTable::new_dist();
    if hdist > 0 && lengths[hlit..].iter().any(|&l| l != 0) {
        dist.build(&lengths[hlit..], SymbolKind::Dist)?;
    }

    Ok((litlen, dist))
}

fn build_fixed_tables() -> (Box<HuffTable>, Box<HuffTable>) {
    let mut litlen = HuffTable::new_litlen();
    litlen.build(&FIXED_LITLEN_LENGTHS, SymbolKind::Literal).expect("Fixed litlen table");

    let mut dist = HuffTable::new_dist();
    dist.build(&FIXED_DIST_LENGTHS[..30], SymbolKind::Dist).expect("Fixed dist table");

    (litlen, dist)
}

fn decode_block(bits: &mut BitBuffer, litlen: &HuffTable, dist: &HuffTable, output: &mut Vec<u8>) -> bool {
    loop {
        let entry = match bits.decode_symbol(litlen) {
            Ok(e) => e,
            Err(_) => return false,
        };

        if entry.is_literal() {
            output.push(entry.payload() as u8);
            continue;
        }

        if entry.is_end_of_block() {
            return true;
        }

        if entry.is_length() {
            let lc = entry.payload() as usize;
            if lc >= 29 {
                return false;
            }
            let base = LENGTH_BASE[lc];
            let extra_bits = LENGTH_EXTRA[lc];
            let match_len = if extra_bits > 0 {
                bits.fill();
                match bits.read_bits(extra_bits) {
                    Ok(e) => base + e,
                    Err(_) => return false,
                }
            } else {
                base
            } as usize;

            let dist_entry = match bits.decode_symbol(dist) {
                Ok(e) => e,
                Err(_) => return false,
            };
            let dc = dist_entry.payload() as usize;
            if dc >= 30 {
                return false;
            }
            let dist_base = DIST_BASE[dc];
            let dist_extra = DIST_EXTRA[dc];
            let match_dist = if dist_extra > 0 {
                bits.fill();
                match bits.read_bits(dist_extra) {
                    Ok(e) => dist_base + e,
                    Err(_) => return false,
                }
            } else {
                dist_base
            } as usize;

            if match_dist == 0 || match_dist > output.len() {
                return false;
            }

            let start = output.len() - match_dist;
            output.reserve(match_len);
            for k in 0..match_len {
                let byte = output[start + (k % match_dist)];
                output.push(byte);
            }
            continue;
        }

        return false;
    }
}

pub struct DeflateDecoder {
    data: Vec<u8>,
}

impl DeflateDecoder {
    pub fn from_bytes(data: Vec<u8>) -> Self {
        DeflateDecoder { data }
    }

    pub fn decode(&self) -> Vec<u8> {
        let mut bits = BitBuffer::new(self.data.clone());
        let mut output = Vec::new();

        let (fixed_litlen, fixed_dist) = build_fixed_tables();

        loop {
            bits.fill();
            let bfinal = match bits.read_bits(1) {
                Ok(v) => v,
                Err(_) => break,
            };
            let btype = match bits.read_bits(2) {
                Ok(v) => v,
                Err(_) => break,
            };

            let ok = match btype {
                0 => {
                    bits.align_to_byte();
                    let len = match bits.read_u16_le_aligned() {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    let nlen = match bits.read_u16_le_aligned() {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    if len != !nlen {
                        break;
                    }
                    output.reserve(len as usize);
                    let mut ok = true;
                    for _ in 0..len {
                        match bits.read_byte_aligned() {
                            Ok(b) => output.push(b),
                            Err(_) => { ok = false; break; }
                        }
                    }
                    ok
                }
                1 => decode_block(&mut bits, &fixed_litlen, &fixed_dist, &mut output),
                2 => {
                    match build_dynamic_tables(&mut bits) {
                        Ok((litlen, dist)) => decode_block(&mut bits, &litlen, &dist, &mut output),
                        Err(_) => false,
                    }
                }
                _ => break,
            };

            if bfinal == 1 || !ok {
                break;
            }
        }

        output
    }
}

pub struct ZlibDecoder {
    data: Vec<u8>,
}

impl ZlibDecoder {
    pub fn from_bytes(data: Vec<u8>) -> Self {
        ZlibDecoder { data }
    }

    pub fn decode(&self) -> Vec<u8> {
        if self.data.len() < 2 {
            return Vec::new();
        }

        let cmf = self.data[0];
        let flg = self.data[1];

        let cm = cmf & 0x0F;
        if cm != 8 {
            return Vec::new();
        }

        let fdict = (flg >> 5) & 1;
        let offset = if fdict != 0 && self.data.len() >= 6 { 6 } else { 2 };

        if offset >= self.data.len() {
            return Vec::new();
        }

        DeflateDecoder::from_bytes(self.data[offset..].to_vec()).decode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idat_decompress() {
        let idat = vec![
            0x78u8, 0x9c, 0x63, 0x60, 0x00, 0x02, 0x01, 0x20, 0x7e, 0xc0, 0x60, 0xc7, 0xa0,
            0x05, 0xa4, 0xf7, 0x30, 0xcc, 0x61, 0x58, 0xc1, 0xa0, 0xb5, 0x82, 0x41, 0xb7, 0x82,
            0x41, 0xff, 0x07, 0x50, 0x00, 0x00, 0x50, 0xd7, 0x05, 0xf7,
        ];
        let result = ZlibDecoder::from_bytes(idat).decode();
        println!("Result len: {}, bytes: {:x?}", result.len(), result);
        assert_eq!(result.len(), 34);
    }
}
