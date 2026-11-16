mod simd;

const MAX_CODE_LEN: u32 = 15;
const CODELEN_SYMBOLS: usize = 19;
const MAX_MATCH_LEN: usize = 258;
const MAX_DEFLATE_RATIO: usize = 1032;

const LITLEN_BITS: u32 = 11;
const LITLEN_SIZE: usize = 1 << LITLEN_BITS;
const LITLEN_MASK: u64 = LITLEN_SIZE as u64 - 1;

const DIST_BITS: u32 = 9;
const DIST_SIZE: usize = 1 << DIST_BITS;
const DIST_MASK: u64 = DIST_SIZE as u64 - 1;

const CL_BITS: u32 = 7;
const CL_SIZE: usize = 1 << CL_BITS;
const CL_MASK: u64 = CL_SIZE as u64 - 1;

const MAX_EXTRA_DIST_BITS: u32 = 13;
const FAST_IN_MARGIN: usize = 32;
const FAST_OUT_MARGIN: usize = 2 * 2 + MAX_MATCH_LEN + 40;
const FAST_OUT_LOGICAL: usize = 2 * 2 + MAX_MATCH_LEN;
const OUT_SLACK: usize = FAST_OUT_MARGIN;
const PAIRING_MIN_INPUT: usize = 2 * LITLEN_SIZE;
const DEFAULT_INITIAL_MIN: usize = 64 * 1024;

const TOTAL_BITS: u32 = 0x3F;
const SECOND_LITERAL: u32 = 1 << 7;
const LITERAL: u32 = 1 << 12;
const END_OF_BLOCK: u32 = 1 << 13;
const SUBTABLE: u32 = 1 << 14;
const EXCEPTIONAL: u32 = 1 << 15;
const INVALID: u32 = EXCEPTIONAL;

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
    while i < 144 {
        lengths[i] = 8;
        i += 1;
    }
    while i < 256 {
        lengths[i] = 9;
        i += 1;
    }
    while i < 280 {
        lengths[i] = 7;
        i += 1;
    }
    while i < 288 {
        lengths[i] = 8;
        i += 1;
    }
    lengths
};

const FIXED_DIST_LENGTHS: [u8; 30] = [5u8; 30];

#[inline(always)]
fn literal_entry(byte: u32, len: u32) -> u32 {
    (byte << 16) | LITERAL | (len << 8) | len
}

#[inline(always)]
fn base_entry(base: u32, len: u32, extra: u32) -> u32 {
    (base << 16) | (len << 8) | (len + extra)
}

#[inline(always)]
fn end_of_block_entry(len: u32) -> u32 {
    EXCEPTIONAL | END_OF_BLOCK | (len << 8) | len
}

#[inline(always)]
fn subtable_entry(start: usize, sub_bits: u32, primary_bits: u32) -> u32 {
    ((start as u32) << 16) | EXCEPTIONAL | SUBTABLE | (sub_bits << 8) | primary_bits
}

#[inline(always)]
fn total_bits(entry: u32) -> u32 {
    entry & TOTAL_BITS
}

#[inline(always)]
fn code_bits(entry: u32) -> u32 {
    (entry >> 8) & 0xF
}

#[inline(always)]
fn extra_value(saved: u64, entry: u32) -> usize {
    ((saved & ((1u64 << total_bits(entry)) - 1)) >> code_bits(entry)) as usize
}

#[inline(always)]
fn subtable_lookup(entries: &[u32], pointer: u32, bits: u64) -> u32 {
    let start = (pointer >> 16) as usize;
    let index = (bits & ((1u64 << code_bits(pointer)) - 1)) as usize;
    entries.get(start + index).copied().unwrap_or(INVALID)
}

#[inline(always)]
unsafe fn subtable_lookup_unchecked(entries: &[u32], pointer: u32, bits: u64) -> u32 {
    let start = (pointer >> 16) as usize;
    let index = (bits & ((1u64 << code_bits(pointer)) - 1)) as usize;
    *entries.get_unchecked(start + index)
}

#[inline(always)]
fn reverse_bits(code: u32, len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    (code << (32 - len)).reverse_bits()
}

#[derive(Clone, Copy)]
enum SymbolKind {
    Literal,
    Dist,
    CodeLen,
}

impl SymbolKind {
    fn entry(self, symbol: usize, len: u32) -> Option<u32> {
        match self {
            SymbolKind::Literal => match symbol {
                0..=255 => Some(literal_entry(symbol as u32, len)),
                256 => Some(end_of_block_entry(len)),
                257..=285 => Some(base_entry(LENGTH_BASE[symbol - 257], len, LENGTH_EXTRA[symbol - 257])),
                _ => None,
            },
            SymbolKind::Dist => {
                (symbol < DIST_BASE.len()).then(|| base_entry(DIST_BASE[symbol], len, DIST_EXTRA[symbol]))
            }
            SymbolKind::CodeLen => Some(base_entry(symbol as u32, len, 0)),
        }
    }
}

struct HuffTable {
    entries: Vec<u32>,
    primary_bits: u32,
    length_counts: [u32; MAX_CODE_LEN as usize + 1],
}

impl HuffTable {
    fn new(primary_bits: u32) -> Self {
        HuffTable {
            entries: vec![INVALID; 1 << primary_bits],
            primary_bits,
            length_counts: [0; MAX_CODE_LEN as usize + 1],
        }
    }

    fn reset(&mut self) {
        self.entries.truncate(1 << self.primary_bits);
        self.entries.fill(INVALID);
    }

    fn build(&mut self, lengths: &[u8], kind: SymbolKind) -> bool {
        let mut bl_count = [0u32; MAX_CODE_LEN as usize + 1];
        for &l in lengths {
            if u32::from(l) > MAX_CODE_LEN {
                return false;
            }
            if l > 0 {
                bl_count[l as usize] += 1;
            }
        }

        let mut next_code = [0u32; MAX_CODE_LEN as usize + 1];
        let mut code = 0u32;
        for bits in 1..=MAX_CODE_LEN as usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        self.length_counts = bl_count;
        let kraft: u32 = (1..=MAX_CODE_LEN as usize).map(|len| bl_count[len] << (MAX_CODE_LEN as usize - len)).sum();
        if kraft <= 1 << MAX_CODE_LEN {
            return self.build_prefix_free(lengths, kind, &bl_count, &mut next_code);
        }

        self.reset();
        let primary_bits = self.primary_bits;
        for (symbol, &l) in lengths.iter().enumerate() {
            let len = u32::from(l);
            if len == 0 {
                continue;
            }
            let symbol_code = next_code[len as usize];
            next_code[len as usize] += 1;

            let Some(full) = kind.entry(symbol, len) else {
                continue;
            };
            let inserted = if len <= primary_bits {
                self.insert_primary(symbol_code, len, full);
                true
            } else {
                let sub = kind.entry(symbol, len - primary_bits).unwrap_or(INVALID);
                self.insert_secondary(symbol_code, len, sub)
            };
            if !inserted {
                return false;
            }
        }
        true
    }

    fn build_prefix_free(
        &mut self,
        lengths: &[u8],
        kind: SymbolKind,
        bl_count: &[u32; MAX_CODE_LEN as usize + 1],
        next_code: &mut [u32; MAX_CODE_LEN as usize + 1],
    ) -> bool {
        let mut offsets = [0usize; MAX_CODE_LEN as usize + 2];
        for len in 1..=MAX_CODE_LEN as usize {
            offsets[len + 1] = offsets[len] + bl_count[len] as usize;
        }
        let mut sorted = [0u16; 320];
        let mut cursor = offsets;
        for (symbol, &l) in lengths.iter().enumerate() {
            if l > 0 {
                sorted[cursor[l as usize]] = symbol as u16;
                cursor[l as usize] += 1;
            }
        }

        let primary_bits = self.primary_bits;
        self.entries.clear();
        self.entries.extend_from_slice(&[INVALID, INVALID]);
        for len in 1..=primary_bits {
            if len > 1 {
                self.entries.extend_from_within(..);
            }
            for &symbol in &sorted[offsets[len as usize]..offsets[len as usize + 1]] {
                let symbol_code = next_code[len as usize];
                next_code[len as usize] += 1;
                if let Some(entry) = kind.entry(usize::from(symbol), len) {
                    self.entries[reverse_bits(symbol_code, len) as usize] = entry;
                }
            }
        }

        for len in primary_bits + 1..=MAX_CODE_LEN {
            for &symbol in &sorted[offsets[len as usize]..offsets[len as usize + 1]] {
                let symbol_code = next_code[len as usize];
                next_code[len as usize] += 1;
                if let Some(entry) = kind.entry(usize::from(symbol), len - primary_bits) {
                    if !self.insert_secondary(symbol_code, len, entry) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn insert_primary(&mut self, code: u32, len: u32, entry: u32) {
        let reversed = reverse_bits(code, len) as usize;
        for k in 0..1usize << (self.primary_bits - len) {
            self.entries[reversed | (k << len)] = entry;
        }
    }

    fn insert_secondary(&mut self, code: u32, len: u32, entry: u32) -> bool {
        let primary_bits = self.primary_bits;
        let overflow_bits = len - primary_bits;
        let primary_index = reverse_bits(code >> overflow_bits, primary_bits) as usize;
        let overflow_reversed = reverse_bits(code & ((1 << overflow_bits) - 1), overflow_bits) as usize;

        let current = self.entries[primary_index];
        let (start, sub_bits) = if current & SUBTABLE != 0 {
            ((current >> 16) as usize, code_bits(current))
        } else if current == INVALID {
            let start = self.entries.len();
            let sub_bits = MAX_CODE_LEN - primary_bits;
            self.entries.resize(start + (1 << sub_bits), INVALID);
            self.entries[primary_index] = subtable_entry(start, sub_bits, primary_bits);
            (start, sub_bits)
        } else {
            return false;
        };

        let sub_mask = (1usize << sub_bits) - 1;
        for k in 0..1usize << (sub_bits - overflow_bits) {
            self.entries[start + ((overflow_reversed | (k << overflow_bits)) & sub_mask)] = entry;
        }
        true
    }

    fn pair_literals(&mut self, literal_counts: &[u32; MAX_CODE_LEN as usize + 1]) -> bool {
        let bits = self.primary_bits;
        let mut paired = 0u64;
        for a in 1..bits {
            for b in 1..=bits - a {
                paired +=
                    (u64::from(literal_counts[a as usize]) * u64::from(literal_counts[b as usize])) << (bits - a - b);
            }
        }
        if paired < (1 << bits) / 4 {
            return false;
        }

        let Some(primary) = self.entries.first_chunk_mut::<LITLEN_SIZE>() else {
            return false;
        };
        for i in (0..LITLEN_SIZE).rev() {
            let first = primary[i];
            let first_len = total_bits(first);
            let second = primary[i >> first_len];
            let total = first_len + total_bits(second);
            let fits = (first & second & LITERAL != 0) & (total <= bits);
            let paired = (first & !TOTAL_BITS) | ((second & 0xFF_0000) << 8) | SECOND_LITERAL | total;
            primary[i] = if fits { paired } else { first };
        }
        true
    }

    #[inline(always)]
    fn primary_litlen(&self) -> &[u32; LITLEN_SIZE] {
        self.entries[..LITLEN_SIZE].try_into().unwrap()
    }

    #[inline(always)]
    fn primary_dist(&self) -> &[u32; DIST_SIZE] {
        self.entries[..DIST_SIZE].try_into().unwrap()
    }
}

struct BlockTables {
    litlen: HuffTable,
    dist: HuffTable,
    paired: bool,
}

impl BlockTables {
    fn new() -> Self {
        BlockTables { litlen: HuffTable::new(LITLEN_BITS), dist: HuffTable::new(DIST_BITS), paired: false }
    }

    fn fixed() -> Self {
        let mut tables = BlockTables::new();
        tables.litlen.build(&FIXED_LITLEN_LENGTHS, SymbolKind::Literal);
        tables.dist.build(&FIXED_DIST_LENGTHS, SymbolKind::Dist);
        tables
    }
}

enum Step {
    Continue,
    EndOfBlock,
    Stop,
}

enum FastExit {
    Margin,
    EndOfBlock,
    Stop,
}

struct Inflater<'a> {
    input: &'a [u8],
    in_pos: usize,
    bitbuf: u64,
    bits_left: u32,
    out: Vec<u8>,
    max_output: usize,
}

impl<'a> Inflater<'a> {
    fn new(input: &'a [u8], initial: usize, max_output: usize) -> Self {
        let initial = initial.min(max_output).min(input.len().saturating_mul(MAX_DEFLATE_RATIO));
        Inflater {
            input,
            in_pos: 0,
            bitbuf: 0,
            bits_left: 0,
            out: Vec::with_capacity(initial.saturating_add(OUT_SLACK)),
            max_output,
        }
    }

    #[inline(always)]
    fn refill(&mut self) {
        if self.in_pos + 8 <= self.input.len() {
            let word = u64::from_le_bytes(self.input[self.in_pos..self.in_pos + 8].try_into().unwrap());
            self.bitbuf |= word << self.bits_left;
            self.in_pos += 7 - ((self.bits_left >> 3) & 7) as usize;
            self.bits_left |= 56;
            return;
        }
        while self.bits_left < 56 && self.in_pos < self.input.len() {
            self.bitbuf |= u64::from(self.input[self.in_pos]) << self.bits_left;
            self.bits_left += 8;
            self.in_pos += 1;
        }
    }

    #[inline(always)]
    fn consume(&mut self, n: u32) {
        self.bitbuf >>= n;
        self.bits_left -= n;
    }

    fn read_bits(&mut self, n: u32) -> Option<u32> {
        if self.bits_left < n {
            self.refill();
            if self.bits_left < n {
                return None;
            }
        }
        let value = (self.bitbuf & ((1u64 << n) - 1)) as u32;
        self.consume(n);
        Some(value)
    }

    fn ensure_room(&mut self, needed: usize) {
        let len = self.out.len();
        let want = len.saturating_add(needed).min(self.max_output).saturating_add(OUT_SLACK);
        if want <= self.out.capacity() {
            return;
        }
        let ceiling = self.max_output.saturating_add(OUT_SLACK);
        let grown = self.out.capacity().saturating_mul(2).max(DEFAULT_INITIAL_MIN);
        self.out.reserve_exact(want.max(grown).min(ceiling) - len);
    }

    fn fast_out_end(&self) -> usize {
        self.out.capacity().saturating_sub(FAST_OUT_MARGIN).min(self.max_output.saturating_sub(FAST_OUT_LOGICAL))
    }

    fn inflate(mut self) -> Vec<u8> {
        let mut dynamic: Option<Box<BlockTables>> = None;
        let mut fixed: Option<Box<BlockTables>> = None;
        let mut cl = HuffTable::new(CL_BITS);
        let mut lengths = Vec::new();

        loop {
            if self.out.len() >= self.max_output {
                break;
            }
            let Some(bfinal) = self.read_bits(1) else {
                break;
            };
            let Some(btype) = self.read_bits(2) else {
                break;
            };

            let ok = match btype {
                0 => self.stored_block(),
                1 => {
                    let tables = fixed.get_or_insert_with(|| Box::new(BlockTables::fixed()));
                    self.huffman_block(tables)
                }
                2 => {
                    let tables = dynamic.get_or_insert_with(|| Box::new(BlockTables::new()));
                    self.read_dynamic_tables(tables, &mut cl, &mut lengths) && self.huffman_block(tables)
                }
                _ => false,
            };

            if bfinal == 1 || !ok {
                break;
            }
        }

        self.out
    }

    fn stored_block(&mut self) -> bool {
        let partial = self.bits_left & 7;
        self.consume(partial);
        self.in_pos -= (self.bits_left >> 3) as usize;
        self.bitbuf = 0;
        self.bits_left = 0;

        let Some(header) = self.input.get(self.in_pos..self.in_pos + 4) else {
            return false;
        };
        let len = u16::from_le_bytes([header[0], header[1]]);
        let nlen = u16::from_le_bytes([header[2], header[3]]);
        if len != !nlen {
            return false;
        }
        self.in_pos += 4;

        let available = self.input.len() - self.in_pos;
        let room = self.max_output - self.out.len();
        let count = usize::from(len).min(available).min(room);
        self.ensure_room(count);
        self.out.extend_from_slice(&self.input[self.in_pos..self.in_pos + count]);
        self.in_pos += count;
        count == usize::from(len) || count == room
    }

    fn read_dynamic_tables(&mut self, tables: &mut BlockTables, cl: &mut HuffTable, lengths: &mut Vec<u8>) -> bool {
        let (Some(hlit), Some(hdist), Some(hclen)) = (self.read_bits(5), self.read_bits(5), self.read_bits(4)) else {
            return false;
        };
        let hlit = hlit as usize + 257;
        let hdist = hdist as usize + 1;
        let hclen = hclen as usize + 4;

        let mut cl_lengths = [0u8; CODELEN_SYMBOLS];
        for &symbol in &CODE_LENGTH_ORDER[..hclen] {
            let Some(len) = self.read_bits(3) else {
                return false;
            };
            cl_lengths[symbol] = len as u8;
        }

        if !cl.build(&cl_lengths, SymbolKind::CodeLen) {
            return false;
        }

        let total = hlit + hdist;
        lengths.clear();
        lengths.resize(total, 0);
        let mut i = 0;
        while i < total {
            self.refill();
            let entry = cl.entries[(self.bitbuf & CL_MASK) as usize];
            let consumed = total_bits(entry);
            if entry & EXCEPTIONAL != 0 || consumed == 0 || consumed > self.bits_left {
                return false;
            }
            self.consume(consumed);

            let symbol = (entry >> 16) as u8;
            let (fill, repeat) = match symbol {
                0..=15 => {
                    lengths[i] = symbol;
                    i += 1;
                    continue;
                }
                16 => {
                    if i == 0 {
                        return false;
                    }
                    (lengths[i - 1], self.read_bits(2).map(|r| r as usize + 3))
                }
                17 => (0, self.read_bits(3).map(|r| r as usize + 3)),
                18 => (0, self.read_bits(7).map(|r| r as usize + 11)),
                _ => return false,
            };
            let Some(repeat) = repeat else {
                return false;
            };
            let end = (i + repeat).min(total);
            lengths[i..end].fill(fill);
            i = end;
        }

        if !tables.litlen.build(&lengths[..hlit], SymbolKind::Literal) {
            return false;
        }

        let mut literal_counts = tables.litlen.length_counts;
        for &l in lengths[256..hlit].iter().filter(|&&l| l != 0) {
            literal_counts[l as usize] -= 1;
        }
        let remaining_input = self.input.len() - self.in_pos;
        tables.paired = remaining_input >= PAIRING_MIN_INPUT && tables.litlen.pair_literals(&literal_counts);

        if lengths[hlit..].iter().all(|&l| l == 0) {
            tables.dist.reset();
        } else if !tables.dist.build(&lengths[hlit..], SymbolKind::Dist) {
            return false;
        }

        true
    }

    fn huffman_block(&mut self, tables: &BlockTables) -> bool {
        loop {
            if self.in_pos + FAST_IN_MARGIN < self.input.len() {
                if self.out.len() >= self.fast_out_end() {
                    self.ensure_room(FAST_OUT_MARGIN.max(self.out.capacity() - self.out.len()));
                }
                let out_end = self.fast_out_end();
                if self.out.len() < out_end {
                    let exit = self.run_fast_loop(tables, out_end);
                    match exit {
                        FastExit::Margin => {}
                        FastExit::EndOfBlock => return true,
                        FastExit::Stop => return false,
                    }
                }
            }

            match self.slow_symbol(tables) {
                Step::Continue => {}
                Step::EndOfBlock => return true,
                Step::Stop => return false,
            }
        }
    }

    fn push_literal(&mut self, byte: u8) -> bool {
        if self.out.len() >= self.max_output {
            return false;
        }
        self.ensure_room(1);
        self.out.push(byte);
        true
    }

    fn slow_symbol(&mut self, tables: &BlockTables) -> Step {
        let litlen = &tables.litlen.entries;
        self.refill();
        let mut entry = litlen[(self.bitbuf & LITLEN_MASK) as usize];
        if entry & SUBTABLE != 0 {
            if total_bits(entry) > self.bits_left {
                return Step::Stop;
            }
            self.consume(total_bits(entry));
            entry = subtable_lookup(litlen, entry, self.bitbuf);
        }

        if entry & EXCEPTIONAL != 0 {
            if entry & END_OF_BLOCK != 0 && total_bits(entry) <= self.bits_left {
                self.consume(total_bits(entry));
                return Step::EndOfBlock;
            }
            return Step::Stop;
        }

        if entry & LITERAL != 0 {
            let first_len = code_bits(entry);
            if first_len > self.bits_left || !self.push_literal((entry >> 16) as u8) {
                return Step::Stop;
            }
            if entry & SECOND_LITERAL == 0 {
                self.consume(first_len);
                return Step::Continue;
            }
            if total_bits(entry) > self.bits_left || !self.push_literal((entry >> 24) as u8) {
                return Step::Stop;
            }
            self.consume(total_bits(entry));
            return Step::Continue;
        }

        if total_bits(entry) > self.bits_left {
            return Step::Stop;
        }
        let saved = self.bitbuf;
        self.consume(total_bits(entry));
        let length = (entry >> 16) as usize + extra_value(saved, entry);

        let dist = &tables.dist.entries;
        self.refill();
        let mut entry = dist[(self.bitbuf & DIST_MASK) as usize];
        if entry & SUBTABLE != 0 {
            if total_bits(entry) > self.bits_left {
                return Step::Stop;
            }
            self.consume(total_bits(entry));
            entry = subtable_lookup(dist, entry, self.bitbuf);
        }
        if entry & EXCEPTIONAL != 0 || total_bits(entry) > self.bits_left {
            return Step::Stop;
        }
        let saved = self.bitbuf;
        self.consume(total_bits(entry));
        let distance = (entry >> 16) as usize + extra_value(saved, entry);

        let out_len = self.out.len();
        if distance > out_len || out_len >= self.max_output {
            return Step::Stop;
        }
        let length = length.min(self.max_output - out_len);
        self.ensure_room(length);
        for i in out_len..out_len + length {
            let byte = self.out[i - distance];
            self.out.push(byte);
        }
        Step::Continue
    }

    #[inline(always)]
    fn fast_loop<const PAIRED: bool>(&mut self, tables: &BlockTables, out_end: usize) -> FastExit {
        let litlen_entries = tables.litlen.entries.as_slice();
        let litlen = tables.litlen.primary_litlen();
        let dist_entries = tables.dist.entries.as_slice();
        let dist = tables.dist.primary_dist();

        let input = self.input.as_ptr();
        let in_end = unsafe { input.add(self.input.len() - FAST_IN_MARGIN) };
        let mut in_next = unsafe { input.add(self.in_pos) };
        let out = self.out.as_mut_ptr();
        let out_end = unsafe { out.add(out_end) };
        let mut out_next = unsafe { out.add(self.out.len()) };
        let mut bitbuf = self.bitbuf;
        let mut bits_left = self.bits_left;
        let mut saved;
        let exit;

        macro_rules! refill {
            () => {
                bitbuf |= u64::from_le(unsafe { (in_next as *const u64).read_unaligned() }) << bits_left;
                in_next = unsafe { in_next.add(7 - ((bits_left >> 3) & 7) as usize) };
                bits_left |= 56;
            };
        }

        macro_rules! consume {
            ($entry:expr) => {
                saved = bitbuf;
                bitbuf >>= total_bits($entry);
                bits_left -= total_bits($entry);
            };
        }

        macro_rules! write_literal {
            ($entry:expr) => {
                if PAIRED {
                    unsafe { (out_next as *mut u16).write_unaligned(((($entry) >> 16) as u16).to_le()) };
                    out_next = unsafe { out_next.add(1 + ((($entry) & SECOND_LITERAL) >> 7) as usize) };
                } else {
                    unsafe { *out_next = (($entry) >> 16) as u8 };
                    out_next = unsafe { out_next.add(1) };
                }
            };
        }

        refill!();
        let mut entry = litlen[(bitbuf & LITLEN_MASK) as usize];
        loop {
            consume!(entry);

            if entry & LITERAL != 0 {
                let first = entry;
                entry = litlen[(bitbuf & LITLEN_MASK) as usize];
                consume!(entry);
                write_literal!(first);
                if entry & LITERAL != 0 {
                    let second = entry;
                    entry = litlen[(bitbuf & LITLEN_MASK) as usize];
                    consume!(entry);
                    write_literal!(second);
                    if entry & LITERAL != 0 {
                        let third = entry;
                        entry = litlen[(bitbuf & LITLEN_MASK) as usize];
                        refill!();
                        write_literal!(third);
                        if in_next < in_end && out_next < out_end {
                            continue;
                        }
                        exit = FastExit::Margin;
                        break;
                    }
                }
            }

            if entry & EXCEPTIONAL != 0 {
                if entry & END_OF_BLOCK != 0 {
                    exit = FastExit::EndOfBlock;
                    break;
                }
                if entry & SUBTABLE == 0 {
                    exit = FastExit::Stop;
                    break;
                }
                entry = unsafe { subtable_lookup_unchecked(litlen_entries, entry, bitbuf) };
                consume!(entry);
                if entry & LITERAL != 0 {
                    let literal = entry;
                    entry = litlen[(bitbuf & LITLEN_MASK) as usize];
                    refill!();
                    unsafe { *out_next = (literal >> 16) as u8 };
                    out_next = unsafe { out_next.add(1) };
                    if in_next < in_end && out_next < out_end {
                        continue;
                    }
                    exit = FastExit::Margin;
                    break;
                }
                if entry & EXCEPTIONAL != 0 {
                    exit = if entry & END_OF_BLOCK != 0 { FastExit::EndOfBlock } else { FastExit::Stop };
                    break;
                }
            }

            let length = (entry >> 16) as usize + extra_value(saved, entry);

            entry = dist[(bitbuf & DIST_MASK) as usize];
            if entry & EXCEPTIONAL != 0 {
                if entry & SUBTABLE == 0 {
                    exit = FastExit::Stop;
                    break;
                }
                if bits_left < MAX_CODE_LEN + MAX_EXTRA_DIST_BITS + LITLEN_BITS - 1 {
                    refill!();
                }
                bitbuf >>= total_bits(entry);
                bits_left -= total_bits(entry);
                entry = unsafe { subtable_lookup_unchecked(dist_entries, entry, bitbuf) };
                if entry & EXCEPTIONAL != 0 {
                    exit = FastExit::Stop;
                    break;
                }
            } else if bits_left < DIST_BITS + MAX_EXTRA_DIST_BITS + LITLEN_BITS - 1 {
                refill!();
            }
            consume!(entry);
            let distance = (entry >> 16) as usize + extra_value(saved, entry);

            if distance > unsafe { out_next.offset_from(out) } as usize {
                exit = FastExit::Stop;
                break;
            }

            let dst = out_next;
            let src = unsafe { dst.sub(distance) };
            out_next = unsafe { out_next.add(length) };

            entry = litlen[(bitbuf & LITLEN_MASK) as usize];
            refill!();

            unsafe { copy_match(src, dst, out_next, distance, length) };

            if in_next < in_end && out_next < out_end {
                continue;
            }
            exit = FastExit::Margin;
            break;
        }

        self.in_pos = unsafe { in_next.offset_from(input) } as usize;
        self.bitbuf = bitbuf;
        self.bits_left = bits_left;
        unsafe { self.out.set_len(out_next.offset_from(out) as usize) };
        exit
    }
}

#[inline(always)]
unsafe fn copy_match(mut src: *const u8, mut dst: *mut u8, end: *mut u8, distance: usize, length: usize) {
    if distance >= 8 {
        loop {
            std::ptr::copy_nonoverlapping(src, dst, 8);
            std::ptr::copy_nonoverlapping(src.add(8), dst.add(8), 8);
            std::ptr::copy_nonoverlapping(src.add(16), dst.add(16), 8);
            std::ptr::copy_nonoverlapping(src.add(24), dst.add(24), 8);
            std::ptr::copy_nonoverlapping(src.add(32), dst.add(32), 8);
            src = src.add(40);
            dst = dst.add(40);
            if dst >= end {
                break;
            }
        }
    } else if distance == 1 {
        let word = u64::from(*src) * 0x0101_0101_0101_0101;
        loop {
            (dst as *mut u64).write_unaligned(word);
            (dst.add(8) as *mut u64).write_unaligned(word);
            (dst.add(16) as *mut u64).write_unaligned(word);
            (dst.add(24) as *mut u64).write_unaligned(word);
            dst = dst.add(32);
            if dst >= end {
                break;
            }
        }
    } else if length < 16 {
        loop {
            std::ptr::copy(src, dst, 8);
            src = src.add(distance);
            dst = dst.add(distance);
            std::ptr::copy(src, dst, 8);
            src = src.add(distance);
            dst = dst.add(distance);
            if dst >= end {
                break;
            }
        }
    } else {
        let mut pattern = u128::from(read_pattern(src, distance));
        let mut covered = distance;
        while covered < 16 {
            pattern |= pattern << (8 * covered);
            covered *= 2;
        }
        let stride = 16 - 16 % distance;
        let pattern = pattern.to_le();
        loop {
            (dst as *mut u128).write_unaligned(pattern);
            dst = dst.add(stride);
            if dst >= end {
                break;
            }
        }
    }
}

#[inline(always)]
unsafe fn read_pattern(src: *const u8, distance: usize) -> u64 {
    if distance >= 4 {
        let low = u64::from(u32::from_le((src as *const u32).read_unaligned()));
        let high = u64::from(u32::from_le((src.add(distance - 4) as *const u32).read_unaligned()));
        low | (high << (8 * (distance - 4)))
    } else {
        let low = u64::from(u16::from_le((src as *const u16).read_unaligned()));
        let high = u64::from(u16::from_le((src.add(distance - 2) as *const u16).read_unaligned()));
        low | (high << (8 * (distance - 2)))
    }
}

pub struct DeflateDecoder<'a> {
    data: &'a [u8],
}

impl<'a> DeflateDecoder<'a> {
    pub fn from_slice(data: &'a [u8]) -> Self {
        DeflateDecoder { data }
    }

    pub fn decode_with_max(&self, max_output: usize) -> Vec<u8> {
        let initial = self.data.len().saturating_mul(4).max(DEFAULT_INITIAL_MIN);
        Inflater::new(self.data, initial, max_output).inflate()
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        let max_output = super::output_cap(capacity_hint);
        let initial = if capacity_hint > 0 { capacity_hint } else { self.data.len().saturating_mul(4) };
        Inflater::new(self.data, initial, max_output).inflate()
    }

    pub fn decode_expected(&self, expected_len: usize) -> Vec<u8> {
        Inflater::new(self.data, expected_len, expected_len).inflate()
    }
}

pub struct ZlibDecoder<'a> {
    data: &'a [u8],
}

impl<'a> ZlibDecoder<'a> {
    pub fn from_slice(data: &'a [u8]) -> Self {
        ZlibDecoder { data }
    }

    pub fn decode_with_max(&self, max_output: usize) -> Vec<u8> {
        self.inner().map(|inner| inner.decode_with_max(max_output)).unwrap_or_default()
    }

    pub fn decode_with_capacity(&self, capacity_hint: usize) -> Vec<u8> {
        self.inner().map(|inner| inner.decode_with_capacity(capacity_hint)).unwrap_or_default()
    }

    pub fn decode_expected(&self, expected_len: usize) -> Vec<u8> {
        self.inner().map(|inner| inner.decode_expected(expected_len)).unwrap_or_default()
    }

    fn inner(&self) -> Option<DeflateDecoder<'a>> {
        if self.data.len() < 2 {
            return None;
        }
        let cmf = self.data[0];
        let flg = self.data[1];
        if cmf & 0x0F != 8 {
            return None;
        }
        let fdict = (flg >> 5) & 1;
        let offset = if fdict != 0 && self.data.len() >= 6 { 6 } else { 2 };
        if offset >= self.data.len() {
            return None;
        }
        Some(DeflateDecoder::from_slice(&self.data[offset..]))
    }
}
