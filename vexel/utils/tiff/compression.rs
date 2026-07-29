use crate::utils::deflate::ZlibDecoder;
use crate::utils::tiff::pixels::{read_msb_bits64, write_msb_bits};
use crate::utils::tiff::types::Predictor;

pub fn predictor_x_factor(predictor: Predictor) -> u32 {
    match predictor {
        Predictor::FloatingPointX2 => 2,
        Predictor::FloatingPointX4 => 4,
        _ => 1,
    }
}

const LZW_CLEAR_CODE: u16 = 256;
const LZW_EOI_CODE: u16 = 257;
const LZW_FIRST_CODE: u16 = 258;
const LZW_MIN_BITS: u32 = 9;
const LZW_MAX_BITS: u32 = 12;
const LZW_MAX_ENTRIES: usize = 1 << LZW_MAX_BITS;

struct LzwTable {
    prefix: Vec<u16>,
    suffix: Vec<u8>,
    free: u16,
    nbits: u32,
    max_code: u16,
}

impl LzwTable {
    fn new() -> Self {
        let mut prefix = vec![0u16; LZW_MAX_ENTRIES + 1];
        let mut suffix = vec![0u8; LZW_MAX_ENTRIES + 1];
        for (i, s) in suffix.iter_mut().enumerate().take(256) {
            *s = i as u8;
        }
        prefix[LZW_CLEAR_CODE as usize] = 0;
        prefix[LZW_EOI_CODE as usize] = 0;
        LzwTable { prefix, suffix, free: LZW_FIRST_CODE, nbits: LZW_MIN_BITS, max_code: (1u16 << LZW_MIN_BITS) - 2 }
    }

    fn reset(&mut self) {
        self.free = LZW_FIRST_CODE;
        self.nbits = LZW_MIN_BITS;
        self.max_code = (1u16 << LZW_MIN_BITS) - 2;
    }

    fn add(&mut self, prefix_code: u16, suffix_byte: u8) {
        let idx = self.free as usize;
        if idx < LZW_MAX_ENTRIES {
            self.prefix[idx] = prefix_code;
            self.suffix[idx] = suffix_byte;
        }
        self.free += 1;
        if self.free > self.max_code && self.nbits < LZW_MAX_BITS {
            self.nbits += 1;
            self.max_code = (1u16 << self.nbits) - 2;
        }
    }

    fn decode_string(&self, code: u16, output: &mut Vec<u8>) -> u8 {
        let start = output.len();
        let mut cur = code as usize;
        let mut limit = LZW_MAX_ENTRIES + 2;
        while cur >= LZW_FIRST_CODE as usize {
            output.push(self.suffix[cur]);
            cur = self.prefix[cur] as usize;
            limit -= 1;
            if limit == 0 {
                break;
            }
        }
        output.push(cur as u8);
        output[start..].reverse();
        output[start]
    }

    fn first_char(&self, code: u16) -> u8 {
        let mut cur = code as usize;
        let mut limit = LZW_MAX_ENTRIES + 2;
        while cur >= LZW_FIRST_CODE as usize {
            cur = self.prefix[cur] as usize;
            limit -= 1;
            if limit == 0 {
                break;
            }
        }
        cur as u8
    }
}

struct BitReaderMsb<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReaderMsb<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReaderMsb { data, pos: 0 }
    }

    fn read(&mut self, nbits: u32) -> Option<u16> {
        let mut code = 0u32;
        for _ in 0..nbits {
            let byte_idx = self.pos / 8;
            if byte_idx >= self.data.len() {
                return None;
            }
            let bit_idx = 7 - (self.pos % 8);
            let bit = ((self.data[byte_idx] >> bit_idx) & 1) as u32;
            code = (code << 1) | bit;
            self.pos += 1;
        }
        Some(code as u16)
    }
}

struct BitReaderLsb<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReaderLsb<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReaderLsb { data, pos: 0 }
    }

    fn read(&mut self, nbits: u32) -> Option<u16> {
        let mut code = 0u32;
        for i in 0..nbits {
            let byte_idx = self.pos / 8;
            if byte_idx >= self.data.len() {
                return None;
            }
            let bit = ((self.data[byte_idx] >> (self.pos % 8)) & 1) as u32;
            code |= bit << i;
            self.pos += 1;
        }
        Some(code as u16)
    }
}

pub fn decompress_lzw(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let is_old_style = data.len() >= 2 && data[0] == 0 && (data[1] & 0x1) != 0;
    if is_old_style {
        decompress_lzw_lsb(data)
    } else {
        decompress_lzw_msb(data)
    }
}

fn decompress_lzw_msb(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut table = LzwTable::new();
    let mut reader = BitReaderMsb::new(data);
    let mut old_code: Option<u16> = None;

    while let Some(code) = reader.read(table.nbits) {
        if code == LZW_EOI_CODE {
            break;
        }

        if code == LZW_CLEAR_CODE {
            table.reset();

            let first_code = loop {
                let c = match reader.read(table.nbits) {
                    Some(c) => c,
                    None => return output,
                };
                if c == LZW_EOI_CODE {
                    return output;
                }
                if c != LZW_CLEAR_CODE {
                    break c;
                }
            };

            output.push(first_code as u8);
            old_code = Some(first_code);
            continue;
        }

        let prev = match old_code {
            Some(p) => p,
            None => {
                output.push(code as u8);
                old_code = Some(code);
                continue;
            }
        };

        let first_byte = if (code as usize) < table.free as usize {
            table.decode_string(code, &mut output)
        } else if code == table.free {
            let first = table.first_char(prev);
            table.decode_string(prev, &mut output);
            output.push(first);
            first
        } else {
            break;
        };

        table.add(prev, first_byte);
        old_code = Some(code);
    }

    output
}

fn decompress_lzw_lsb(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut table = LzwTable::new();
    let mut reader = BitReaderLsb::new(data);
    let mut old_code: Option<u16> = None;

    while let Some(code) = reader.read(table.nbits) {
        if code == LZW_EOI_CODE {
            break;
        }

        if code == LZW_CLEAR_CODE {
            table.reset();

            let first_code = loop {
                let c = match reader.read(table.nbits) {
                    Some(c) => c,
                    None => return output,
                };
                if c == LZW_EOI_CODE {
                    return output;
                }
                if c != LZW_CLEAR_CODE {
                    break c;
                }
            };

            output.push(first_code as u8);
            old_code = Some(first_code);
            continue;
        }

        let prev = match old_code {
            Some(p) => p,
            None => {
                output.push(code as u8);
                old_code = Some(code);
                continue;
            }
        };

        let first_byte = if (code as usize) < table.free as usize {
            table.decode_string(code, &mut output)
        } else if code == table.free {
            let first = table.first_char(prev);
            table.decode_string(prev, &mut output);
            output.push(first);
            first
        } else {
            break;
        };

        table.add(prev, first_byte);
        old_code = Some(code);
    }

    output
}

pub fn decompress_packbits(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let header = data[i] as i8;
        i += 1;

        if header >= 0 {
            let count = (header as usize) + 1;
            let end = (i + count).min(data.len());
            output.extend_from_slice(&data[i..end]);
            i += count;
        } else if header != -128 {
            let count = (-header as usize) + 1;
            if i < data.len() {
                let byte = data[i];
                i += 1;
                for _ in 0..count {
                    output.push(byte);
                }
            }
        }
    }

    output
}

pub fn decompress_deflate(data: &[u8]) -> Vec<u8> {
    ZlibDecoder::from_slice(data).decode()
}

fn sgilog_decode_row(bp: &[u8], offset: &mut usize, tp: &mut [u32], npixels: usize, shifts: &[u32]) {
    for &shft in shifts {
        let mut i = 0;
        while i < npixels && *offset < bp.len() {
            let b = bp[*offset];
            if b >= 128 {
                if *offset + 1 >= bp.len() {
                    break;
                }
                let rc = (b as usize) + 2 - 128;
                let val = (bp[*offset + 1] as u32) << shft;
                *offset += 2;
                let end = (i + rc).min(npixels);
                for slot in &mut tp[i..end] {
                    *slot |= val;
                }
                i += rc;
            } else {
                let rc = b as usize;
                *offset += 1;
                let available = (bp.len() - *offset).min(rc).min(npixels - i);
                for slot in &mut tp[i..i + available] {
                    *slot |= (bp[*offset] as u32) << shft;
                    *offset += 1;
                }
                i += available;
                if i < npixels && rc > available {
                    i += rc - available;
                }
            }
        }
    }
}

pub fn decompress_sgilog(data: &[u8], width: usize, rows: usize) -> Vec<u8> {
    let npixels_total = width * rows;
    let mut raw = vec![0u32; npixels_total];
    let mut offset = 0;

    for row in 0..rows {
        let row_start = row * width;
        let tp = &mut raw[row_start..row_start + width];
        for v in tp.iter_mut() {
            *v = 0;
        }
        sgilog_decode_row(data, &mut offset, tp, width, &[24, 16, 8, 0]);
    }

    let mut out = Vec::with_capacity(npixels_total * 4);
    for v in raw {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

pub fn decompress_sgilog_l16(data: &[u8], width: usize, rows: usize) -> Vec<u8> {
    let npixels_total = width * rows;
    let mut raw = vec![0u32; npixels_total];
    let mut offset = 0;

    for row in 0..rows {
        let row_start = row * width;
        let tp = &mut raw[row_start..row_start + width];
        for v in tp.iter_mut() {
            *v = 0;
        }
        sgilog_decode_row(data, &mut offset, tp, width, &[8, 0]);
    }

    let mut out = Vec::with_capacity(npixels_total * 2);
    for v in raw {
        out.extend_from_slice(&(v as u16).to_be_bytes());
    }
    out
}

pub fn decompress_sgilog24(data: &[u8], width: usize, rows: usize) -> Vec<u8> {
    let npixels_total = width * rows;
    let mut raw = vec![0u16; npixels_total];
    let mut offset = 0;

    for row in 0..rows {
        let row_start = row * width;
        let mut tp32 = vec![0u32; width];
        sgilog_decode_row(data, &mut offset, &mut tp32, width, &[8, 0]);
        for (i, v) in tp32.iter().enumerate() {
            raw[row_start + i] = *v as u16;
        }
    }

    let mut out = Vec::with_capacity(npixels_total * 2);
    for v in raw {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

fn read_uint(bytes: &[u8], big_endian: bool) -> u64 {
    let mut value = 0u64;
    if big_endian {
        for &b in bytes {
            value = (value << 8) | b as u64;
        }
    } else {
        for &b in bytes.iter().rev() {
            value = (value << 8) | b as u64;
        }
    }
    value
}

fn write_uint(bytes: &mut [u8], value: u64, big_endian: bool) {
    let n = bytes.len();
    for (i, b) in bytes.iter_mut().enumerate() {
        let shift = if big_endian { (n - 1 - i) * 8 } else { i * 8 };
        *b = (value >> shift) as u8;
    }
}

pub fn apply_predictor_horizontal(
    data: &mut [u8],
    width: u32,
    samples_per_pixel: u16,
    bits_per_sample: u16,
    big_endian: bool,
) {
    let spp = samples_per_pixel as usize;
    if spp == 0 || bits_per_sample == 0 || bits_per_sample > 64 {
        return;
    }

    if !bits_per_sample.is_multiple_of(8) {
        let bps = bits_per_sample as usize;
        let samples = width as usize * spp;
        let row_stride = (samples * bps).div_ceil(8);
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;
        let mask = (1u64 << bps) - 1;

        for row in 0..rows {
            let row_data = &mut data[row * row_stride..(row + 1) * row_stride];
            for col in spp..samples {
                let prev = read_msb_bits64(row_data, (col - spp) * bps, bits_per_sample);
                let val = read_msb_bits64(row_data, col * bps, bits_per_sample);
                write_msb_bits(row_data, col * bps, bits_per_sample, val.wrapping_add(prev) & mask);
            }
        }
        return;
    }

    let bytes_per_sample = (bits_per_sample as usize) / 8;
    let row_stride = width as usize * spp * bytes_per_sample;
    if bytes_per_sample > 8 || row_stride == 0 {
        return;
    }
    let rows = data.len() / row_stride;

    if bytes_per_sample == 1 {
        for row in 0..rows {
            let row_data = &mut data[row * row_stride..(row + 1) * row_stride];
            for col in spp..row_stride {
                row_data[col] = row_data[col].wrapping_add(row_data[col - spp]);
            }
        }
        return;
    }

    let mask = if bits_per_sample >= 64 { u64::MAX } else { (1u64 << bits_per_sample) - 1 };
    let sample_stride = spp * bytes_per_sample;

    for row in 0..rows {
        let row_data = &mut data[row * row_stride..(row + 1) * row_stride];
        for col in spp..width as usize * spp {
            let idx = col * bytes_per_sample;
            let prev = read_uint(&row_data[idx - sample_stride..idx - sample_stride + bytes_per_sample], big_endian);
            let val = read_uint(&row_data[idx..idx + bytes_per_sample], big_endian);
            write_uint(&mut row_data[idx..idx + bytes_per_sample], val.wrapping_add(prev) & mask, big_endian);
        }
    }
}

const THUNDER_2BIT_DELTAS: [i32; 4] = [0, 1, 0, -1];
const THUNDER_3BIT_DELTAS: [i32; 8] = [0, 1, 2, 3, 0, -3, -2, -1];

pub fn decompress_thunderscan(data: &[u8], width: usize, rows: usize) -> Vec<u8> {
    let row_bytes = width.div_ceil(2);
    let mut out = vec![0u8; row_bytes * rows];
    let mut ip = 0usize;

    for row_data in out.chunks_exact_mut(row_bytes) {
        let mut npixels = 0usize;
        let mut lastpixel = 0u8;

        let set_pixel = |row_data: &mut [u8], npixels: &mut usize, v: u8| {
            if *npixels < width {
                if *npixels & 1 == 1 {
                    row_data[*npixels / 2] |= v & 0xf;
                } else {
                    row_data[*npixels / 2] = (v & 0xf) << 4;
                }
                *npixels += 1;
            }
        };

        while ip < data.len() && npixels < width {
            let n = data[ip];
            ip += 1;
            match n & 0xc0 {
                0x00 => {
                    for _ in 0..(n & 0x3f) {
                        set_pixel(row_data, &mut npixels, lastpixel);
                    }
                }
                0x40 => {
                    for shift in [4u8, 2, 0] {
                        let delta = ((n >> shift) & 3) as usize;
                        if delta != 2 {
                            lastpixel = ((lastpixel as i32 + THUNDER_2BIT_DELTAS[delta]) & 0xf) as u8;
                            set_pixel(row_data, &mut npixels, lastpixel);
                        }
                    }
                }
                0x80 => {
                    for shift in [3u8, 0] {
                        let delta = ((n >> shift) & 7) as usize;
                        if delta != 4 {
                            lastpixel = ((lastpixel as i32 + THUNDER_3BIT_DELTAS[delta]) & 0xf) as u8;
                            set_pixel(row_data, &mut npixels, lastpixel);
                        }
                    }
                }
                _ => {
                    lastpixel = n & 0xf;
                    set_pixel(row_data, &mut npixels, lastpixel);
                }
            }
        }
    }

    out
}

pub fn decompress_next(data: &[u8], width: usize, rows: usize) -> Vec<u8> {
    let row_bytes = (width * 2).div_ceil(8);
    let mut out = vec![0xffu8; row_bytes * rows];
    let mut ip = 0usize;

    for row_data in out.chunks_exact_mut(row_bytes) {
        if ip >= data.len() {
            break;
        }
        let n = data[ip];
        ip += 1;

        match n {
            0x00 => {
                let end = (ip + row_bytes).min(data.len());
                row_data[..end - ip].copy_from_slice(&data[ip..end]);
                ip = end;
            }
            0x40 => {
                if ip + 4 > data.len() {
                    break;
                }
                let off = ((data[ip] as usize) << 8) | data[ip + 1] as usize;
                let count = ((data[ip + 2] as usize) << 8) | data[ip + 3] as usize;
                ip += 4;
                if off + count > row_bytes || ip + count > data.len() {
                    break;
                }
                row_data[off..off + count].copy_from_slice(&data[ip..ip + count]);
                ip += count;
            }
            mut code => {
                let mut npixels = 0usize;
                loop {
                    let grey = (code >> 6) & 0x3;
                    for _ in 0..(code & 0x3f) {
                        if npixels >= width {
                            break;
                        }
                        let shift = 6 - (npixels & 3) * 2;
                        let idx = npixels / 4;
                        row_data[idx] = (row_data[idx] & !(0x3 << shift)) | (grey << shift);
                        npixels += 1;
                    }
                    if npixels >= width || ip >= data.len() {
                        break;
                    }
                    code = data[ip];
                    ip += 1;
                }
            }
        }
    }

    out
}

pub fn apply_predictor_float(
    data: &mut [u8],
    width: u32,
    samples_per_pixel: u16,
    bits_per_sample: u16,
    big_endian: bool,
) {
    let bps = (bits_per_sample as usize) / 8;
    let spp = samples_per_pixel as usize;
    let row_stride = width as usize * spp * bps;
    let stride = spp;

    if row_stride == 0 || bps == 0 || stride == 0 {
        return;
    }

    let rows = data.len() / row_stride;

    for row in 0..rows {
        let row_start = row * row_stride;
        let row_data = &mut data[row_start..row_start + row_stride];

        let mut count = row_stride;
        let mut cp = 0usize;
        while count > stride {
            for _ in 0..stride {
                row_data[cp + stride] = row_data[cp + stride].wrapping_add(row_data[cp]);
                cp += 1;
            }
            count -= stride;
        }

        let tmp = row_data.to_vec();
        let wc = width as usize * spp;

        for sample in 0..wc {
            for byte in 0..bps {
                let plane = if big_endian { byte } else { bps - byte - 1 };
                row_data[sample * bps + byte] = tmp[plane * wc + sample];
            }
        }
    }
}
