use crate::utils::deflate::ZlibDecoder;

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
        for i in 0..256usize {
            suffix[i] = i as u8;
        }
        prefix[LZW_CLEAR_CODE as usize] = 0;
        prefix[LZW_EOI_CODE as usize] = 0;
        LzwTable {
            prefix,
            suffix,
            free: LZW_FIRST_CODE,
            nbits: LZW_MIN_BITS,
            max_code: (1u16 << LZW_MIN_BITS) - 2,
        }
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

    loop {
        let code = match reader.read(table.nbits) {
            Some(c) => c,
            None => break,
        };

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

    loop {
        let code = match reader.read(table.nbits) {
            Some(c) => c,
            None => break,
        };

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
    ZlibDecoder::from_bytes(data.to_vec()).decode()
}

pub fn apply_predictor_horizontal(data: &mut Vec<u8>, width: u32, samples_per_pixel: u16, bits_per_sample: u16) {
    if bits_per_sample == 8 {
        let spp = samples_per_pixel as usize;
        let row_stride = width as usize * spp;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..row_stride {
                let idx = row_start + col;
                if idx < data.len() {
                    data[idx] = data[idx].wrapping_add(data[idx - spp]);
                }
            }
        }
    } else if bits_per_sample == 16 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 2usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 1 < data.len() && prev_idx + 1 < data.len() {
                    let val = u16::from_ne_bytes([data[idx], data[idx + 1]]);
                    let prev = u16::from_ne_bytes([data[prev_idx], data[prev_idx + 1]]);
                    let result = val.wrapping_add(prev).to_ne_bytes();
                    data[idx] = result[0];
                    data[idx + 1] = result[1];
                }
            }
        }
    } else if bits_per_sample == 32 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 4usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 3 < data.len() && prev_idx + 3 < data.len() {
                    let val = u32::from_ne_bytes([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]);
                    let prev = u32::from_ne_bytes([data[prev_idx], data[prev_idx + 1], data[prev_idx + 2], data[prev_idx + 3]]);
                    let result = val.wrapping_add(prev).to_ne_bytes();
                    data[idx] = result[0];
                    data[idx + 1] = result[1];
                    data[idx + 2] = result[2];
                    data[idx + 3] = result[3];
                }
            }
        }
    } else if bits_per_sample == 64 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 8usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 7 < data.len() && prev_idx + 7 < data.len() {
                    let val = u64::from_ne_bytes([
                        data[idx], data[idx + 1], data[idx + 2], data[idx + 3],
                        data[idx + 4], data[idx + 5], data[idx + 6], data[idx + 7],
                    ]);
                    let prev = u64::from_ne_bytes([
                        data[prev_idx], data[prev_idx + 1], data[prev_idx + 2], data[prev_idx + 3],
                        data[prev_idx + 4], data[prev_idx + 5], data[prev_idx + 6], data[prev_idx + 7],
                    ]);
                    let result = val.wrapping_add(prev).to_ne_bytes();
                    for i in 0..8 {
                        data[idx + i] = result[i];
                    }
                }
            }
        }
    }
}

pub fn apply_predictor_horizontal_be(data: &mut Vec<u8>, width: u32, samples_per_pixel: u16, bits_per_sample: u16) {
    if bits_per_sample == 8 {
        apply_predictor_horizontal(data, width, samples_per_pixel, bits_per_sample);
        return;
    }

    if bits_per_sample == 16 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 2usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 1 < data.len() && prev_idx + 1 < data.len() {
                    let val = u16::from_be_bytes([data[idx], data[idx + 1]]);
                    let prev = u16::from_be_bytes([data[prev_idx], data[prev_idx + 1]]);
                    let result = val.wrapping_add(prev).to_be_bytes();
                    data[idx] = result[0];
                    data[idx + 1] = result[1];
                }
            }
        }
    } else if bits_per_sample == 32 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 4usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 3 < data.len() && prev_idx + 3 < data.len() {
                    let val = u32::from_be_bytes([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]);
                    let prev = u32::from_be_bytes([data[prev_idx], data[prev_idx + 1], data[prev_idx + 2], data[prev_idx + 3]]);
                    let result = val.wrapping_add(prev).to_be_bytes();
                    data[idx] = result[0];
                    data[idx + 1] = result[1];
                    data[idx + 2] = result[2];
                    data[idx + 3] = result[3];
                }
            }
        }
    } else if bits_per_sample == 64 {
        let spp = samples_per_pixel as usize;
        let bytes_per_sample = 8usize;
        let row_stride = width as usize * spp * bytes_per_sample;
        if row_stride == 0 {
            return;
        }
        let rows = data.len() / row_stride;

        for row in 0..rows {
            let row_start = row * row_stride;
            for col in spp..width as usize * spp {
                let idx = row_start + col * bytes_per_sample;
                let prev_idx = idx - spp * bytes_per_sample;
                if idx + 7 < data.len() && prev_idx + 7 < data.len() {
                    let val = u64::from_be_bytes([
                        data[idx], data[idx + 1], data[idx + 2], data[idx + 3],
                        data[idx + 4], data[idx + 5], data[idx + 6], data[idx + 7],
                    ]);
                    let prev = u64::from_be_bytes([
                        data[prev_idx], data[prev_idx + 1], data[prev_idx + 2], data[prev_idx + 3],
                        data[prev_idx + 4], data[prev_idx + 5], data[prev_idx + 6], data[prev_idx + 7],
                    ]);
                    let result = val.wrapping_add(prev).to_be_bytes();
                    for i in 0..8 {
                        data[idx + i] = result[i];
                    }
                }
            }
        }
    }
}

pub fn apply_predictor_float(data: &mut Vec<u8>, width: u32, samples_per_pixel: u16, bits_per_sample: u16, big_endian: bool) {
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
