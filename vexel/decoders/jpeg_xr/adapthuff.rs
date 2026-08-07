use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::tables::*;

const THRESHOLD: i32 = 8;
const MEMORY: i32 = 8;
const HUFFMAN_DECODE_ROOT_BITS: u32 = 5;
const HUFFMAN_DECODE_ROOT_BITS_LOG: u32 = 3;

pub struct AdaptiveHuffman {
    symbols: usize,
    table_index: usize,
    initialized: bool,
    discriminant: i32,
    discriminant1: i32,
    upper_bound: i32,
    lower_bound: i32,
    lookup: &'static [i16],
    delta: Option<&'static [i32]>,
    delta1: Option<&'static [i32]>,
}

impl AdaptiveHuffman {
    pub fn new(symbols: usize) -> Self {
        let mut huff = AdaptiveHuffman {
            symbols,
            table_index: 0,
            initialized: false,
            discriminant: 0,
            discriminant1: 0,
            upper_bound: 0,
            lower_bound: 0,
            lookup: &[],
            delta: None,
            delta1: None,
        };
        huff.adapt();
        huff
    }

    pub fn lookup(&self) -> &'static [i16] {
        self.lookup
    }

    pub fn discriminant_add(&mut self, symbol: usize) {
        if let Some(delta) = self.delta {
            if let Some(value) = delta.get(symbol) {
                self.discriminant += value;
            }
        }
    }

    pub fn discriminant1_add(&mut self, symbol: usize) {
        if let Some(delta) = self.delta1 {
            if let Some(value) = delta.get(symbol) {
                self.discriminant1 += value;
            }
        }
    }

    pub fn adapt(&mut self) {
        let symbols = self.symbols;
        let max_tables = MAX_TABLES.get(symbols).copied().unwrap_or(0);
        let second_disc = SECOND_DISC.get(symbols).copied().unwrap_or(0);

        if !self.initialized {
            self.initialized = true;
            self.discriminant = 0;
            self.discriminant1 = 0;
            self.table_index = second_disc;
        }

        let low = self.discriminant;
        let high = if second_disc != 0 { self.discriminant1 } else { self.discriminant };

        let mut changed = false;
        if low < self.lower_bound {
            self.table_index = self.table_index.saturating_sub(1);
            changed = true;
        } else if high > self.upper_bound {
            if self.table_index + 1 < max_tables {
                self.table_index += 1;
            }
            changed = true;
        }

        if changed {
            self.discriminant = 0;
            self.discriminant1 = 0;
        }

        let limit = THRESHOLD * MEMORY;
        self.discriminant = self.discriminant.clamp(-limit, limit);
        self.discriminant1 = self.discriminant1.clamp(-limit, limit);

        let table = self.table_index.min(max_tables.saturating_sub(1));
        self.table_index = table;

        self.lower_bound = if table == 0 { i32::MIN } else { -THRESHOLD };
        self.upper_bound = if table + 1 >= max_tables { 1 << 30 } else { THRESHOLD };

        self.bind_tables(table, max_tables);
    }

    fn bind_tables(&mut self, table: usize, max_tables: usize) {
        let symbols = self.symbols;

        match symbols {
            4 => {
                self.lookup = &G4_HUFF_LOOKUP;
                self.delta = None;
                self.delta1 = None;
            }
            5 => {
                self.lookup = row(&G5_HUFF_LOOKUP, table, 42);
                self.delta = Some(&G5_DELTA_TABLE);
                self.delta1 = None;
            }
            6 => {
                self.lookup = row(&G6_HUFF_LOOKUP, table, 44);
                self.bind_dual_deltas(table, max_tables, &G6_DELTA_TABLE);
            }
            7 => {
                self.lookup = row(&G7_HUFF_LOOKUP, table, 46);
                self.delta = Some(&G7_DELTA_TABLE);
                self.delta1 = None;
            }
            8 => {
                self.lookup = row(&G8_HUFF_LOOKUP, 0, 48);
                self.delta = Some(&G8_DELTA_TABLE);
                self.delta1 = None;
            }
            9 => {
                self.lookup = row(&G9_HUFF_LOOKUP, table, 50);
                self.delta = Some(&G9_DELTA_TABLE);
                self.delta1 = None;
            }
            12 => {
                self.lookup = row(&G12_HUFF_LOOKUP, table, 56);
                self.bind_dual_deltas(table, max_tables, &G12_DELTA_TABLE);
            }
            _ => {
                self.lookup = &[];
                self.delta = None;
                self.delta1 = None;
            }
        }
    }

    fn bind_dual_deltas(&mut self, table: usize, max_tables: usize, deltas: &'static [i32]) {
        let symbols = self.symbols;

        let d1_index = table - usize::from(table + 1 == max_tables);
        self.delta1 = slice_row(deltas, d1_index, symbols);

        let d_index = table.saturating_sub(1) + usize::from(table == 0);
        self.delta = slice_row(deltas, d_index, symbols);
    }
}

fn row(table: &'static [i16], index: usize, stride: usize) -> &'static [i16] {
    let start = index * stride;
    table.get(start..start + stride).unwrap_or(&table[..stride.min(table.len())])
}

fn slice_row(table: &'static [i32], index: usize, stride: usize) -> Option<&'static [i32]> {
    let start = index * stride;
    table.get(start..start + stride)
}

pub fn get_huff(lookup: &[i16], reader: &mut XrBitReader) -> usize {
    if lookup.is_empty() {
        return 0;
    }

    let root = reader.peek(HUFFMAN_DECODE_ROOT_BITS) as usize;
    let entry = lookup.get(root).copied().unwrap_or(0);

    let flush =
        if entry < 0 { HUFFMAN_DECODE_ROOT_BITS } else { (entry as u32) & ((1 << HUFFMAN_DECODE_ROOT_BITS_LOG) - 1) };
    reader.skip(u64::from(flush));

    let mut symbol = entry >> HUFFMAN_DECODE_ROOT_BITS_LOG;

    if symbol < 0 {
        let mut cursor = entry;
        loop {
            let index = (cursor as i32) + (1 << 15) + reader.read(1) as i32;
            let next = match usize::try_from(index) {
                Ok(i) => lookup.get(i).copied().unwrap_or(0),
                Err(_) => 0,
            };
            cursor = next;
            if next >= 0 {
                symbol = next;
                break;
            }
            if reader.exhausted() {
                return 0;
            }
        }
    }

    symbol.max(0) as usize
}

pub fn get_huff_short(lookup: &[i16], reader: &mut XrBitReader) -> usize {
    if lookup.is_empty() {
        return 0;
    }

    let root = reader.peek(HUFFMAN_DECODE_ROOT_BITS) as usize;
    let entry = lookup.get(root).copied().unwrap_or(0);
    reader.skip(u64::from((entry as u32) & ((1 << HUFFMAN_DECODE_ROOT_BITS_LOG) - 1)));

    (entry >> HUFFMAN_DECODE_ROOT_BITS_LOG).max(0) as usize
}
