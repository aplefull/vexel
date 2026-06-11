use std::io;

mod simd;

const MAX_BITS: usize = 15;
const CODELEN_SYMBOLS: usize = 19;

const LITLEN_PRIMARY_BITS: u32 = 11;
const LITLEN_PRIMARY_SIZE: usize = 1 << LITLEN_PRIMARY_BITS;

const DIST_PRIMARY_BITS: u32 = 9;
const DIST_PRIMARY_SIZE: usize = 1 << DIST_PRIMARY_BITS;

const CL_PRIMARY_BITS: u32 = 7;
const CL_PRIMARY_SIZE: usize = 1 << CL_PRIMARY_BITS;

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


const TAG_INVALID:  u32 = 0;
const TAG_LITERAL:  u32 = 1;
const TAG_LIT_PAIR: u32 = 2;
const TAG_LENGTH:   u32 = 3;
const TAG_DIST:     u32 = 4;
const TAG_EOB:      u32 = 5;
const TAG_REDIRECT: u32 = 6;
const TAG_CL_SYM:   u32 = 7;

#[derive(Clone, Copy, Debug)]
struct Entry(u32);

impl Entry {
    const INVALID: Entry = Entry(TAG_INVALID << 28);

    #[inline(always)]
    fn tag(self) -> u32 { self.0 >> 28 }

    #[inline(always)]
    fn is_invalid(self) -> bool { self.tag() == TAG_INVALID }
    #[inline(always)]
    fn is_literal(self) -> bool { self.tag() == TAG_LITERAL }
    #[inline(always)]
    fn is_lit_pair(self) -> bool { self.tag() == TAG_LIT_PAIR }
    #[inline(always)]
    fn is_length(self) -> bool { self.tag() == TAG_LENGTH }
    #[inline(always)]
    fn is_dist(self) -> bool { self.tag() == TAG_DIST }
    #[inline(always)]
    fn is_eob(self) -> bool { self.tag() == TAG_EOB }
    #[inline(always)]
    fn is_redirect(self) -> bool { self.tag() == TAG_REDIRECT }

    #[inline(always)]
    fn full_len(self) -> u32 { (self.0 >> 6) & 0x3F }
    #[inline(always)]
    fn base_len(self) -> u32 { self.0 & 0x3F }

    #[inline(always)]
    fn lit_a(self) -> u8 { (self.0 >> 20) as u8 }
    #[inline(always)]
    fn lit_b(self) -> u8 { (self.0 >> 12) as u8 }

    #[inline(always)]
    fn base_value(self) -> u32 { (self.0 >> 12) & 0xFFFF }

    #[inline(always)]
    fn redirect_base(self) -> usize { ((self.0 >> 12) & 0xFFFF) as usize }
    #[inline(always)]
    fn redirect_sub_bits(self) -> u32 { self.full_len() - self.base_len() }

    #[inline(always)]
    fn cl_sym(self) -> u8 { (self.0 >> 20) as u8 }

    fn make_literal(byte: u8, code_len: u32) -> Self {
        Entry((TAG_LITERAL << 28) | ((byte as u32) << 20) | (code_len << 6) | code_len)
    }

    fn make_lit_pair(a: u8, b: u8, len_a: u32, len_b: u32) -> Self {
        let total = len_a + len_b;
        Entry(
            (TAG_LIT_PAIR << 28)
                | ((a as u32) << 20)
                | ((b as u32) << 12)
                | (total << 6)
                | total,
        )
    }

    fn make_length(base_val: u32, code_len: u32, extra_bits: u32) -> Self {
        Entry((TAG_LENGTH << 28) | (base_val << 12) | ((code_len + extra_bits) << 6) | code_len)
    }

    fn make_dist(base_val: u32, code_len: u32, extra_bits: u32) -> Self {
        Entry((TAG_DIST << 28) | (base_val << 12) | ((code_len + extra_bits) << 6) | code_len)
    }

    fn make_eob(code_len: u32) -> Self {
        Entry((TAG_EOB << 28) | (code_len << 6) | code_len)
    }

    fn make_redirect(sub_base: u32, sub_bits: u32, primary_bits: u32) -> Self {
        Entry(
            (TAG_REDIRECT << 28)
                | (sub_base << 12)
                | ((primary_bits + sub_bits) << 6)
                | primary_bits,
        )
    }

    fn make_cl_sym(sym: u8, code_len: u32) -> Self {
        Entry((TAG_CL_SYM << 28) | ((sym as u32) << 20) | (code_len << 6) | code_len)
    }
}

#[inline(always)]
fn reverse_bits(code: u32, len: u32) -> u32 {
    if len == 0 { return 0; }
    (code << (32 - len)).reverse_bits()
}

struct HuffTable {
    primary: Vec<Entry>,
    secondary: Vec<Entry>,
    primary_bits: u32,
}

enum SymbolKind { Literal, Dist, CodeLen }

impl HuffTable {
    fn new(primary_bits: u32, primary_size: usize) -> Box<Self> {
        Box::new(HuffTable {
            primary: vec![Entry::INVALID; primary_size],
            secondary: Vec::new(),
            primary_bits,
        })
    }

    fn build(&mut self, lengths: &[u8], kind: SymbolKind) -> Result<(), io::Error> {
        let n = lengths.len();
        let mut bl_count = [0u32; MAX_BITS + 1];
        for &l in lengths {
            if l as usize > MAX_BITS {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "code length > 15"));
            }
            if l > 0 { bl_count[l as usize] += 1; }
        }

        let mut next_code = [0u32; MAX_BITS + 1];
        let mut code = 0u32;
        for bits in 1..=MAX_BITS {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        for i in 0..n {
            let len = lengths[i] as u32;
            if len == 0 { continue; }
            let sym_code = next_code[len as usize];
            next_code[len as usize] += 1;

            let entry = match kind {
                SymbolKind::Literal => {
                    if i < 256 {
                        Entry::make_literal(i as u8, len)
                    } else if i == 256 {
                        Entry::make_eob(len)
                    } else if i <= 285 {
                        let idx = i - 257;
                        Entry::make_length(LENGTH_BASE[idx], len, LENGTH_EXTRA[idx])
                    } else {
                        continue;
                    }
                }
                SymbolKind::Dist => {
                    if i < 30 {
                        Entry::make_dist(DIST_BASE[i], len, DIST_EXTRA[i])
                    } else {
                        continue;
                    }
                }
                SymbolKind::CodeLen => Entry::make_cl_sym(i as u8, len),
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
                if idx < self.primary.len() {
                    self.primary[idx] = entry;
                }
            }
        } else {
            let overflow_bits = len - pb;
            let primary_rev = reverse_bits(code >> overflow_bits, pb) as usize;
            let overflow_rev = reverse_bits(code & ((1 << overflow_bits) - 1), overflow_bits);

            let (sub_base, sub_bits) = if self.primary[primary_rev].is_redirect() {
                let e = self.primary[primary_rev];
                (e.redirect_base(), e.redirect_sub_bits())
            } else if self.primary[primary_rev].is_invalid() {
                let base = self.secondary.len();
                let bits = (MAX_BITS as u32) - pb;
                let size = 1usize << bits;
                self.secondary.resize(base + size, Entry::INVALID);
                self.primary[primary_rev] = Entry::make_redirect(base as u32, bits, pb);
                (base, bits)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Huffman conflict"));
            };

            let fill = 1u32 << (sub_bits - overflow_bits);
            for k in 0..fill {
                let idx = sub_base + ((overflow_rev | (k << overflow_bits)) as usize & ((1 << sub_bits) - 1));
                if idx < sub_base + (1 << sub_bits) && idx < self.secondary.len() {
                    self.secondary[idx] = entry;
                }
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn lookup(&self, bits: u64) -> (Entry, u32) {
        let mask = (1u64 << self.primary_bits) - 1;
        let idx = (bits & mask) as usize;
        let e = self.primary[idx];

        if e.is_redirect() {
            let sub_base = e.redirect_base();
            let sub_bits = e.redirect_sub_bits();
            let pb = e.base_len();
            let sub_mask = (1u64 << sub_bits) - 1;
            let sub_idx = sub_base + (((bits >> pb) & sub_mask) as usize);
            let se = if sub_idx < self.secondary.len() { self.secondary[sub_idx] } else { Entry::INVALID };
            (se, se.base_len())
        } else {
            (e, e.base_len())
        }
    }
}

fn build_fast_table(table: &mut HuffTable) {
    let primary_bits = table.primary_bits;
    let size = table.primary.len();

    for i in 0..size {
        let e = table.primary[i];
        if !e.is_literal() { continue; }
        let len_a = e.base_len();
        if len_a == 0 || len_a >= primary_bits { continue; }

        let next_idx = (i >> len_a) & (size - 1);
        let e2 = table.primary[next_idx];
        if !e2.is_literal() { continue; }
        let len_b = e2.base_len();
        if len_a + len_b > primary_bits { continue; }

        table.primary[i] = Entry::make_lit_pair(e.lit_a(), e2.lit_a(), len_a, len_b);
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

    let mut cl_table = HuffTable::new(CL_PRIMARY_BITS, CL_PRIMARY_SIZE);
    cl_table.build(&cl_lengths, SymbolKind::CodeLen)?;

    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        if bits.count < 7 { bits.fill(); }
        let (entry, consumed) = cl_table.lookup(bits.buf);
        if consumed == 0 || entry.is_invalid() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad cl code"));
        }
        bits.consume(consumed);

        let sym = entry.cl_sym();
        match sym {
            0..=15 => { lengths[i] = sym; i += 1; }
            16 => {
                if i == 0 { return Err(io::Error::new(io::ErrorKind::InvalidData, "Repeat w/o prior")); }
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
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad cl sym")),
        }
    }

    let mut litlen = HuffTable::new(LITLEN_PRIMARY_BITS, LITLEN_PRIMARY_SIZE);
    litlen.build(&lengths[..hlit], SymbolKind::Literal)?;
    build_fast_table(&mut litlen);

    let mut dist = HuffTable::new(DIST_PRIMARY_BITS, DIST_PRIMARY_SIZE);
    if hdist > 0 && lengths[hlit..].iter().any(|&l| l != 0) {
        dist.build(&lengths[hlit..], SymbolKind::Dist)?;
    }

    Ok((litlen, dist))
}

fn build_fixed_tables() -> (Box<HuffTable>, Box<HuffTable>) {
    let mut litlen = HuffTable::new(LITLEN_PRIMARY_BITS, LITLEN_PRIMARY_SIZE);
    litlen.build(&FIXED_LITLEN_LENGTHS, SymbolKind::Literal).expect("fixed litlen");
    build_fast_table(&mut litlen);

    let mut dist = HuffTable::new(DIST_PRIMARY_BITS, DIST_PRIMARY_SIZE);
    dist.build(&FIXED_DIST_LENGTHS[..30], SymbolKind::Dist).expect("fixed dist");

    (litlen, dist)
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
        (self.buf & ((1u64 << n) - 1)) as u32
    }

    #[inline(always)]
    fn consume(&mut self, n: u32) {
        self.buf >>= n;
        self.count = self.count.wrapping_sub(n);
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
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"));
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

    fn read_bits(&mut self, n: u32) -> Result<u32, io::Error> {
        if n == 0 { return Ok(0); }
        if self.count < n { self.fill(); }
        if self.count < n {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Not enough bits"));
        }
        let val = self.peek(n);
        self.consume(n);
        Ok(val)
    }

    #[inline(always)]
    fn decode_litlen(&mut self, table: &HuffTable) -> Result<Entry, io::Error> {
        if self.count < 15 { self.fill(); }
        let (entry, consumed) = table.lookup(self.buf);
        if entry.is_invalid() || consumed == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad litlen code"));
        }
        self.consume(consumed);
        Ok(entry)
    }

    #[inline(always)]
    fn decode_dist(&mut self, table: &HuffTable) -> Result<Entry, io::Error> {
        if self.count < 15 { self.fill(); }
        let (entry, consumed) = table.lookup(self.buf);
        if entry.is_invalid() || consumed == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad dist code"));
        }
        self.consume(consumed);
        Ok(entry)
    }
}

fn decode_block(bits: &mut BitBuffer, litlen: &HuffTable, dist: &HuffTable, output: &mut Vec<u8>) -> bool {
    loop {
        let entry = match bits.decode_litlen(litlen) {
            Ok(e) => e,
            Err(_) => return false,
        };

        if entry.is_literal() {
            if output.len() == output.capacity() {
                output.reserve(4096);
            }
            unsafe {
                let end = output.as_mut_ptr().add(output.len());
                end.write(entry.lit_a());
                output.set_len(output.len() + 1);
            }
            continue;
        }

        if entry.is_lit_pair() {
            if output.len() + 2 > output.capacity() {
                output.reserve(4096);
            }
            unsafe {
                let len = output.len();
                let end = output.as_mut_ptr().add(len);
                end.write(entry.lit_a());
                end.add(1).write(entry.lit_b());
                output.set_len(len + 2);
            }
            continue;
        }

        if entry.is_eob() {
            return true;
        }

        if entry.is_length() {
            let base_val = entry.base_value() as usize;
            let base_len_bits = entry.base_len();
            let extra_bits = entry.full_len() - base_len_bits;
            let extra = if extra_bits > 0 {
                if bits.count < extra_bits { bits.fill(); }
                let v = bits.peek(extra_bits);
                bits.consume(extra_bits);
                v as usize
            } else {
                0
            };
            let match_len = base_val + extra;

            let dist_entry = match bits.decode_dist(dist) {
                Ok(e) => e,
                Err(_) => return false,
            };
            if !dist_entry.is_dist() { return false; }

            let dist_base = dist_entry.base_value() as usize;
            let dist_base_bits = dist_entry.base_len();
            let dist_extra_bits = dist_entry.full_len() - dist_base_bits;
            let dist_extra = if dist_extra_bits > 0 {
                if bits.count < dist_extra_bits { bits.fill(); }
                let v = bits.peek(dist_extra_bits);
                bits.consume(dist_extra_bits);
                v as usize
            } else {
                0
            };
            let match_dist = dist_base + dist_extra;

            if match_dist == 0 || match_dist > output.len() {
                return false;
            }

            output.reserve(match_len);
            simd::copy_match(output, match_dist, match_len);
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
        self.decode_with_capacity(0)
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        let mut bits = BitBuffer::new(self.data.clone());
        let mut output = Vec::with_capacity(capacity_hint);

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
                    if len != !nlen { break; }
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
        self.decode_with_capacity(0)
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        if self.data.len() < 2 { return Vec::new(); }
        let cmf = self.data[0];
        let flg = self.data[1];
        if cmf & 0x0F != 8 { return Vec::new(); }
        let fdict = (flg >> 5) & 1;
        let offset = if fdict != 0 && self.data.len() >= 6 { 6 } else { 2 };
        if offset >= self.data.len() { return Vec::new(); }
        DeflateDecoder::from_bytes(self.data[offset..].to_vec()).decode_with_capacity(capacity_hint)
    }
}
