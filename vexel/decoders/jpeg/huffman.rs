use std::io::{Read, Seek};

use super::decoder::JpegDecoder;
use crate::decoders::jpeg::bitreader::JpegBitReader;
use crate::decoders::jpeg::types::{HuffmanTable, HUFF_PEEK_BITS, ZIGZAG_MAP};
use crate::log_warn;

#[inline(always)]
pub(super) fn get_next_symbol(reader: &mut JpegBitReader<'_>, table: &HuffmanTable) -> u8 {
    if let Some(peek) = reader.peek_fast() {
        let entry = unsafe { *table.fast_lookup.get_unchecked(peek as usize) };
        if entry >> 16 != 0 {
            reader.consume(entry & 0xFF);
            return ((entry >> 8) & 0xFF) as u8;
        }
        reader.consume(HUFF_PEEK_BITS as u32);
        let mut code = peek;
        for i in HUFF_PEEK_BITS..16 {
            code = (code << 1) | reader.read_bits(1);
            let first = unsafe { *table.first_code.get_unchecked(i) };
            if first != u32::MAX {
                let count = unsafe { table.offsets.get_unchecked(i + 1).wrapping_sub(*table.offsets.get_unchecked(i)) }
                    as usize;
                if code >= first && (code - first) < count as u32 {
                    let idx = unsafe { *table.offsets.get_unchecked(i) } as usize + (code - first) as usize;
                    return unsafe { *table.symbols.get_unchecked(idx) };
                }
            }
        }
        log_warn!("Invalid Huffman code: {}, replacing with 0", code);
        return 0;
    }

    let mut code = 0u32;
    for i in 0..16 {
        code = (code << 1) | reader.read_bits(1);
        let first = table.first_code[i];
        if first != u32::MAX {
            let count = (table.offsets[i + 1] - table.offsets[i]) as usize;
            if code >= first && (code - first) < count as u32 {
                let idx = table.offsets[i] as usize + (code - first) as usize;
                return table.symbols[idx];
            }
        }
    }

    log_warn!("Invalid Huffman code: {}, replacing with 0", code);
    0
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn build_huffman_table_from_bits(id: u8, bits: &[u8; 16], vals: &[u8]) -> HuffmanTable {
        let mut offsets = Vec::with_capacity(17);
        offsets.push(0u32);
        let mut total: u32 = 0;
        for &b in bits.iter() {
            total += b as u32;
            offsets.push(total);
        }

        let symbols: Vec<u8> = vals[..total as usize].to_vec();
        let mut codes = vec![0u32; total as usize];
        let mut first_code = vec![u32::MAX; 16];

        let mut code: u32 = 0;
        for i in 0..16 {
            for k in offsets[i]..offsets[i + 1] {
                codes[k as usize] = code;
                code += 1;
            }
            code <<= 1;
        }

        for i in 0..16 {
            let start = offsets[i] as usize;
            let end = offsets[i + 1] as usize;
            if start < end && start < codes.len() {
                first_code[i] = codes[start];
            }
        }

        let mut t = HuffmanTable { id, symbols, offsets, codes, first_code, fast_lookup: Vec::new() };
        t.build_fast_lookup();
        t
    }

    pub(super) fn ensure_default_huffman_tables(&mut self) {
        #[rustfmt::skip]
        const BITS_DC_LUMA: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
        #[rustfmt::skip]
        const VALS_DC_LUMA: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

        #[rustfmt::skip]
        const BITS_DC_CHROMA: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
        #[rustfmt::skip]
        const VALS_DC_CHROMA: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

        #[rustfmt::skip]
        const BITS_AC_LUMA: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
        #[rustfmt::skip]
        const VALS_AC_LUMA: [u8; 162] = [
            0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12,
            0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
            0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08,
            0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
            0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16,
            0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
            0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
            0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
            0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
            0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
            0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98,
            0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
            0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
            0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
            0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4,
            0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
            0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea,
            0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
            0xf9, 0xfa,
        ];

        #[rustfmt::skip]
        const BITS_AC_CHROMA: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
        #[rustfmt::skip]
        const VALS_AC_CHROMA: [u8; 162] = [
            0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21,
            0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
            0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91,
            0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
            0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34,
            0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
            0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
            0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
            0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
            0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
            0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
            0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96,
            0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
            0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
            0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
            0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2,
            0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
            0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9,
            0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
            0xf9, 0xfa,
        ];

        if !self.dc_huffman_tables.iter().any(|t| t.id == 0) {
            self.dc_huffman_tables.push(Self::build_huffman_table_from_bits(0, &BITS_DC_LUMA, &VALS_DC_LUMA));
        }
        if !self.dc_huffman_tables.iter().any(|t| t.id == 1) {
            self.dc_huffman_tables.push(Self::build_huffman_table_from_bits(1, &BITS_DC_CHROMA, &VALS_DC_CHROMA));
        }
        if !self.ac_huffman_tables.iter().any(|t| t.id == 0) {
            self.ac_huffman_tables.push(Self::build_huffman_table_from_bits(0, &BITS_AC_LUMA, &VALS_AC_LUMA));
        }
        if !self.ac_huffman_tables.iter().any(|t| t.id == 1) {
            self.ac_huffman_tables.push(Self::build_huffman_table_from_bits(1, &BITS_AC_CHROMA, &VALS_AC_CHROMA));
        }
    }

    pub(super) fn default_lossless_dc_table() -> HuffmanTable {
        let mut t = HuffmanTable {
            id: 0,
            symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
            codes: vec![
                0b000,
                0b010,
                0b011,
                0b100,
                0b101,
                0b110,
                0b1110,
                0b11110,
                0b111110,
                0b1111110,
                0b11111110,
                0b111111110,
            ],
            first_code: vec![
                u32::MAX,
                u32::MAX,
                0b000,
                0b011,
                u32::MAX,
                0b100,
                0b101,
                0b110,
                0b1110,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
            ],
            fast_lookup: Vec::new(),
        };
        t.build_fast_lookup();
        t
    }
}

pub(super) const FAST_BITS: u32 = 9;
const FAST_SIZE: usize = 1 << FAST_BITS;

#[rustfmt::skip]
pub(super) const NATURAL_ORDER: [u8; 128] = {
    let mut table = [63u8; 128];
    let mut i = 0;
    while i < 64 {
        table[i] = ZIGZAG_MAP[i];
        i += 1;
    }
    table
};

#[derive(Clone)]
pub(super) struct DctHuffman {
    lookup: [u16; FAST_SIZE],
    fast_ac: [i16; FAST_SIZE],
    maxcode: [u32; 18],
    valoffset: [i32; 18],
    values: [u8; 256],
}

impl DctHuffman {
    pub(super) fn build(table: &HuffmanTable) -> Box<Self> {
        let mut t = Box::new(DctHuffman {
            lookup: [0; FAST_SIZE],
            fast_ac: [0; FAST_SIZE],
            maxcode: [0; 18],
            valoffset: [0; 18],
            values: [0; 256],
        });

        let count = table.symbols.len().min(256);
        t.values[..count].copy_from_slice(&table.symbols[..count]);

        let mut code: u32 = 0;
        let mut k: usize = 0;
        for len in 1..=16usize {
            let start = table.offsets.get(len - 1).copied().unwrap_or(count as u32) as usize;
            let end = table.offsets.get(len).copied().unwrap_or(count as u32) as usize;
            let start = start.min(count);
            let end = end.clamp(start, count);
            t.valoffset[len] = k as i32 - code as i32;
            for _ in start..end {
                if len as u32 <= FAST_BITS {
                    let shift = FAST_BITS - len as u32;
                    let base = (code as usize) << shift;
                    for entry in 0..(1usize << shift) {
                        if let Some(slot) = t.lookup.get_mut(base + entry) {
                            *slot = ((len as u16) << 8) | t.values[k] as u16;
                        }
                    }
                }
                code += 1;
                k += 1;
            }
            t.maxcode[len] = code << (16 - len);
            code <<= 1;
        }
        t.maxcode[17] = u32::MAX;

        for peek in 0..FAST_SIZE {
            let entry = t.lookup[peek];
            if entry == 0 {
                continue;
            }
            let len = (entry >> 8) as u32;
            let symbol = (entry & 0xFF) as u32;
            let run = symbol >> 4;
            let magnitude = symbol & 15;
            if magnitude == 0 || len + magnitude > FAST_BITS {
                continue;
            }
            let raw = ((peek as u32) << len) & (FAST_SIZE as u32 - 1);
            let mut value = (raw >> (FAST_BITS - magnitude)) as i32;
            if value < (1 << (magnitude - 1)) {
                value -= (1 << magnitude) - 1;
            }
            if (-128..=127).contains(&value) {
                t.fast_ac[peek] = ((value << 8) | ((run << 4) | (len + magnitude)) as i32) as i16;
            }
        }

        t
    }

    #[inline(always)]
    pub(super) fn symbol(&self, reader: &mut JpegBitReader<'_>) -> u32 {
        reader.fill_to(32);
        self.decode_symbol(reader)
    }

    #[inline(always)]
    fn decode_symbol(&self, reader: &mut JpegBitReader<'_>) -> u32 {
        let peek = reader.peek_unchecked(FAST_BITS) as usize;
        self.decode_symbol_peeked(reader, peek)
    }

    #[inline(always)]
    fn decode_symbol_peeked(&self, reader: &mut JpegBitReader<'_>, peek: usize) -> u32 {
        let entry = self.lookup[peek & (FAST_SIZE - 1)];
        if entry != 0 {
            reader.consume((entry >> 8) as u32);
            return (entry & 0xFF) as u32;
        }
        let (next, symbol) = self.decode_symbol_slow(*reader);
        *reader = next;
        symbol
    }

    #[cfg(feature = "rayon")]
    #[inline(always)]
    fn decode_symbol_quiet(&self, reader: &mut JpegBitReader<'_>, peek: usize) -> u32 {
        let entry = self.lookup[peek & (FAST_SIZE - 1)];
        if entry != 0 {
            reader.consume((entry >> 8) as u32);
            return (entry & 0xFF) as u32;
        }
        let (next, symbol) = self.decode_symbol_slow_quiet(*reader);
        *reader = next;
        symbol
    }

    #[cfg(feature = "rayon")]
    #[cold]
    #[inline(never)]
    fn decode_symbol_slow_quiet<'a>(&self, mut reader: JpegBitReader<'a>) -> (JpegBitReader<'a>, u32) {
        let code = reader.peek_unchecked(16);
        let mut len = FAST_BITS as usize + 1;
        while len < 17 && code >= self.maxcode[len] {
            len += 1;
        }
        if len == 17 {
            reader.consume(16);
            return (reader, 0);
        }
        reader.consume(len as u32);
        let index = ((code >> (16 - len)) as i32).wrapping_add(self.valoffset[len]) as usize & 255;
        (reader, self.values[index] as u32)
    }

    #[cold]
    #[inline(never)]
    fn decode_symbol_slow<'a>(&self, mut reader: JpegBitReader<'a>) -> (JpegBitReader<'a>, u32) {
        let code = reader.peek_unchecked(16);
        let mut len = FAST_BITS as usize + 1;
        while len < 17 && code >= self.maxcode[len] {
            len += 1;
        }
        if len == 17 {
            reader.consume(16);
            log_warn!("Invalid Huffman code, replacing with 0");
            return (reader, 0);
        }
        reader.consume(len as u32);
        let index = ((code >> (16 - len)) as i32).wrapping_add(self.valoffset[len]) as usize & 255;
        (reader, self.values[index] as u32)
    }
}

#[inline(always)]
pub(super) fn extend(value: u32, size: u32) -> i32 {
    let value = value as i32;
    value + ((value - (1 << (size - 1))) >> 31 & ((-1i32 << size) + 1))
}

#[inline(always)]
pub(super) fn decode_dc_diff(reader: &mut JpegBitReader<'_>, dc: &DctHuffman) -> i32 {
    reader.fill_to(32);
    let size = dc.decode_symbol(reader);
    if size == 0 {
        return 0;
    }
    if size > 16 {
        return decode_dc_invalid(size);
    }
    extend(reader.read_bits_unchecked(size), size)
}

#[cold]
fn decode_dc_invalid(size: u32) -> i32 {
    log_warn!("Invalid DC coefficient length: {}", size);
    0
}

#[inline(always)]
pub(super) fn decode_ac_sequential(reader: &mut JpegBitReader<'_>, ac: &DctHuffman, block: &mut [i16; 64]) -> usize {
    let mut k = 1usize;
    let mut last = 0usize;
    while k < 64 {
        reader.fill_to(32);
        let peek = reader.peek_unchecked(FAST_BITS) as usize & (FAST_SIZE - 1);
        let fast = ac.fast_ac[peek];
        if fast != 0 {
            k += ((fast >> 4) & 15) as usize;
            reader.consume((fast & 15) as u32);
            block[NATURAL_ORDER[k & 127] as usize & 63] = fast >> 8;
            last = k;
            k += 1;
            continue;
        }
        let symbol = ac.decode_symbol_peeked(reader, peek);
        let run = (symbol >> 4) as usize;
        let size = symbol & 15;
        if size != 0 {
            k += run;
            let value = extend(reader.read_bits_unchecked(size), size);
            block[NATURAL_ORDER[k & 127] as usize & 63] = value as i16;
            last = k;
            k += 1;
        } else if run == 15 {
            k += 16;
        } else {
            break;
        }
    }
    last
}

#[inline(always)]
pub(super) fn decode_ac_first(
    reader: &mut JpegBitReader<'_>,
    ac: &DctHuffman,
    block: &mut [i16; 64],
    start: usize,
    end: usize,
    shift: u8,
    max_size: u32,
    eob_run: &mut u32,
) {
    let mut k = start;
    while k <= end {
        reader.fill_to(32);
        let peek = reader.peek_unchecked(FAST_BITS) as usize & (FAST_SIZE - 1);
        let fast = ac.fast_ac[peek];
        if fast != 0 {
            k += ((fast >> 4) & 15) as usize;
            reader.consume((fast & 15) as u32);
            if k > 63 {
                return ac_first_overrun(k);
            }
            block[k] = ((fast >> 8) as i32).wrapping_shl(shift as u32) as i16;
            k += 1;
            continue;
        }
        let symbol = ac.decode_symbol_peeked(reader, peek);
        let run = symbol >> 4;
        let size = symbol & 15;
        if size != 0 {
            k += run as usize;
            if k > 63 {
                return ac_first_overrun(k);
            }
            if size > max_size {
                log_warn!("Invalid AC coefficient length (>{}): {}", max_size, size);
                return;
            }
            let value = extend(reader.read_bits_unchecked(size), size);
            block[k] = value.wrapping_shl(shift as u32) as i16;
            k += 1;
        } else if run == 15 {
            if k + 15 > end {
                return ac_first_overrun(k + 15);
            }
            k += 16;
        } else {
            *eob_run = (1u32 << run) - 1 + reader.read_bits(run);
            return;
        }
    }
}

#[cold]
fn ac_first_overrun(k: usize) {
    log_warn!("Zero run-length exceeded spectral selection: {}", k);
}

#[cfg(feature = "rayon")]
#[inline(always)]
pub(super) fn skim_dc_diff(reader: &mut JpegBitReader<'_>, dc: &DctHuffman) -> i32 {
    reader.fill_to(32);
    let peek = reader.peek_unchecked(FAST_BITS) as usize;
    let size = dc.decode_symbol_quiet(reader, peek);
    if size == 0 || size > 16 {
        return 0;
    }
    extend(reader.read_bits_unchecked(size), size)
}

#[cfg(feature = "rayon")]
#[inline(always)]
pub(super) fn skim_ac_sequential(reader: &mut JpegBitReader<'_>, ac: &DctHuffman) {
    let mut k = 1usize;
    while k < 64 {
        reader.fill_to(32);
        let peek = reader.peek_unchecked(FAST_BITS) as usize & (FAST_SIZE - 1);
        let fast = ac.fast_ac[peek];
        if fast != 0 {
            k += ((fast >> 4) & 15) as usize + 1;
            reader.consume((fast & 15) as u32);
            continue;
        }
        let symbol = ac.decode_symbol_quiet(reader, peek);
        let run = (symbol >> 4) as usize;
        let size = symbol & 15;
        if size != 0 {
            k += run + 1;
            reader.consume(size);
        } else if run == 15 {
            k += 16;
        } else {
            break;
        }
    }
}

pub(super) const LOSSLESS_BITS: u32 = 12;
const LOSSLESS_SIZE: usize = 1 << LOSSLESS_BITS;
const LOSSLESS_FULL: u32 = 0x80;
const LOSSLESS_LEN_MASK: u32 = 0x1F;

pub(super) struct LosslessHuffman {
    lookup: [u32; LOSSLESS_SIZE],
    maxcode: [u32; 18],
    valoffset: [i32; 18],
    values: [u8; 256],
    ssss16_is_min: bool,
}

impl LosslessHuffman {
    pub(super) fn build(table: &HuffmanTable, ssss16_is_min: bool) -> Box<Self> {
        let mut t = Box::new(LosslessHuffman {
            lookup: [0; LOSSLESS_SIZE],
            maxcode: [0; 18],
            valoffset: [0; 18],
            values: [0; 256],
            ssss16_is_min,
        });

        let count = table.symbols.len().min(256);
        t.values[..count].copy_from_slice(&table.symbols[..count]);

        let mut code: u32 = 0;
        let mut k: usize = 0;
        for len in 1..=16u32 {
            let start = table.offsets.get(len as usize - 1).copied().unwrap_or(count as u32) as usize;
            let end = table.offsets.get(len as usize).copied().unwrap_or(count as u32) as usize;
            let start = start.min(count);
            let end = end.clamp(start, count);
            t.valoffset[len as usize] = k as i32 - code as i32;
            for _ in start..end {
                if len <= LOSSLESS_BITS {
                    let shift = LOSSLESS_BITS - len;
                    let base = (code as usize) << shift;
                    let ssss = t.values[k] as u32;
                    for extra in 0..(1usize << shift) {
                        if let Some(slot) = t.lookup.get_mut(base + extra) {
                            *slot = Self::entry(len, ssss, extra as u32, shift, ssss16_is_min);
                        }
                    }
                }
                code += 1;
                k += 1;
            }
            t.maxcode[len as usize] = code << (16 - len);
            code <<= 1;
        }
        t.maxcode[17] = u32::MAX;
        t
    }

    fn entry(len: u32, ssss: u32, extra: u32, spare: u32, ssss16_is_min: bool) -> u32 {
        let full = |diff: u16, total: u32| ((diff as u32) << 16) | LOSSLESS_FULL | total;
        match ssss {
            0 => full(0, len),
            16 if ssss16_is_min => full(32768, len),
            1..=16 if ssss <= spare => {
                let bits = extra >> (spare - ssss);
                full(extend(bits, ssss) as u16, len + ssss)
            }
            1..=16 => (ssss << 8) | len,
            _ => full(0, len),
        }
    }

    #[inline(always)]
    pub(super) fn diff(&self, reader: &mut JpegBitReader<'_>) -> u16 {
        reader.fill_to(32);
        let entry = self.lookup[reader.peek_unchecked(LOSSLESS_BITS) as usize & (LOSSLESS_SIZE - 1)];
        if entry & LOSSLESS_FULL != 0 {
            reader.consume(entry & LOSSLESS_LEN_MASK);
            return (entry >> 16) as u16;
        }
        if entry != 0 {
            reader.consume(entry & LOSSLESS_LEN_MASK);
            let ssss = (entry >> 8) & LOSSLESS_LEN_MASK;
            return extend(reader.read_bits_unchecked(ssss), ssss) as u16;
        }
        let (next, diff) = self.diff_slow::<true>(*reader);
        *reader = next;
        diff
    }

    #[cfg(feature = "rayon")]
    #[inline(always)]
    pub(super) fn skip(&self, reader: &mut JpegBitReader<'_>) {
        reader.fill_to(32);
        let entry = self.lookup[reader.peek_unchecked(LOSSLESS_BITS) as usize & (LOSSLESS_SIZE - 1)];
        if entry & LOSSLESS_FULL != 0 {
            reader.consume(entry & LOSSLESS_LEN_MASK);
        } else if entry != 0 {
            reader.consume((entry & LOSSLESS_LEN_MASK) + ((entry >> 8) & LOSSLESS_LEN_MASK));
        } else {
            *reader = self.diff_slow::<false>(*reader).0;
        }
    }

    #[cold]
    #[inline(never)]
    fn diff_slow<'a, const WARN: bool>(&self, mut reader: JpegBitReader<'a>) -> (JpegBitReader<'a>, u16) {
        let code = reader.peek_unchecked(16);
        let mut len = LOSSLESS_BITS as usize + 1;
        while len < 17 && code >= self.maxcode[len] {
            len += 1;
        }
        if len == 17 {
            reader.consume(16);
            if WARN {
                log_warn!("Invalid Huffman code: {}, replacing with 0", code);
            }
            return (reader, 0);
        }
        reader.consume(len as u32);
        let index = ((code >> (16 - len)) as i32).wrapping_add(self.valoffset[len]) as usize & 255;
        let ssss = self.values[index] as u32;
        let diff = match ssss {
            0 => 0,
            16 if self.ssss16_is_min => 32768,
            1..=16 => extend(reader.read_bits_unchecked(ssss), ssss) as u16,
            _ => {
                if WARN {
                    log_warn!("Invalid difference: {}", ssss);
                }
                0
            }
        };
        (reader, diff)
    }
}
