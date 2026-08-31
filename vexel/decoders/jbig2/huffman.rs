#[derive(Clone, Copy, Debug)]
pub struct HuffmanLine {
    pub pref_len: u32,
    pub range_len: u32,
    pub range_low: i32,
    pub is_lower: bool,
    pub is_oob: bool,
}

impl HuffmanLine {
    const fn normal(pref_len: u32, range_len: u32, range_low: i32) -> HuffmanLine {
        HuffmanLine { pref_len, range_len, range_low, is_lower: false, is_oob: false }
    }

    const fn lower(pref_len: u32, range_low: i32) -> HuffmanLine {
        HuffmanLine { pref_len, range_len: 32, range_low, is_lower: true, is_oob: false }
    }

    const fn upper(pref_len: u32, range_low: i32) -> HuffmanLine {
        HuffmanLine { pref_len, range_len: 32, range_low, is_lower: false, is_oob: false }
    }

    const fn oob(pref_len: u32) -> HuffmanLine {
        HuffmanLine { pref_len, range_len: 0, range_low: 0, is_lower: false, is_oob: true }
    }
}

#[derive(Clone)]
pub struct HuffmanTable {
    lines: Vec<HuffmanLine>,
    by_code: std::collections::HashMap<(u32, u32), usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HuffmanValue {
    Value(i32),
    Oob,
}

impl HuffmanTable {
    pub fn new(lines: Vec<HuffmanLine>) -> HuffmanTable {
        let codes = assign_codes(&lines);
        let by_code = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.pref_len > 0)
            .map(|(index, line)| ((line.pref_len, codes[index]), index))
            .collect();
        HuffmanTable { lines, by_code }
    }

    pub fn decode(&self, reader: &mut HuffmanReader) -> Option<HuffmanValue> {
        let mut len = 0u32;
        let mut code = 0u32;

        while len < 32 {
            code = (code << 1) | reader.read_bit()?;
            len += 1;

            let Some(&index) = self.by_code.get(&(len, code)) else {
                continue;
            };
            let line = self.lines[index];

            if line.is_oob {
                return Some(HuffmanValue::Oob);
            }

            let offset = reader.read_bits(line.range_len)? as i32;
            return Some(HuffmanValue::Value(if line.is_lower {
                line.range_low.saturating_sub(offset)
            } else {
                line.range_low.saturating_add(offset)
            }));
        }

        None
    }
}

fn assign_codes(lines: &[HuffmanLine]) -> Vec<u32> {
    let max_len = lines.iter().map(|l| l.pref_len).max().unwrap_or(0);
    let mut len_count = vec![0u32; max_len as usize + 2];
    for line in lines {
        if line.pref_len > 0 {
            len_count[line.pref_len as usize] += 1;
        }
    }

    let mut codes = vec![0u32; lines.len()];
    let mut first_code = 0u32;
    len_count[0] = 0;

    for cur_len in 1..=max_len {
        first_code = (first_code + len_count[cur_len as usize - 1]) << 1;
        let mut cur_code = first_code;
        for (index, line) in lines.iter().enumerate() {
            if line.pref_len == cur_len {
                codes[index] = cur_code;
                cur_code += 1;
            }
        }
    }

    codes
}

pub struct HuffmanReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> HuffmanReader<'a> {
    pub fn new(data: &'a [u8]) -> HuffmanReader<'a> {
        HuffmanReader { data, bit_pos: 0 }
    }

    pub fn read_bit(&mut self) -> Option<u32> {
        let byte = self.data.get(self.bit_pos / 8)?;
        let bit = (byte >> (7 - (self.bit_pos % 8))) & 1;
        self.bit_pos += 1;
        Some(bit as u32)
    }

    pub fn read_bits(&mut self, count: u32) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | self.read_bit()?;
        }
        Some(value)
    }

    pub fn align(&mut self) {
        self.bit_pos = self.bit_pos.div_ceil(8) * 8;
    }

    pub fn byte_offset(&self) -> usize {
        self.bit_pos.div_ceil(8)
    }

    pub fn advance_bytes(&mut self, count: usize) {
        self.align();
        self.bit_pos += count * 8;
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

pub fn table_b1() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 4, 0),
        HuffmanLine::normal(2, 8, 16),
        HuffmanLine::normal(3, 16, 272),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(3, 65808),
    ])
}

pub fn table_b2() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 0, 0),
        HuffmanLine::normal(2, 0, 1),
        HuffmanLine::normal(3, 0, 2),
        HuffmanLine::normal(4, 3, 3),
        HuffmanLine::normal(5, 6, 11),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(6, 75),
        HuffmanLine::oob(6),
    ])
}

pub fn table_b3() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(8, 8, -256),
        HuffmanLine::normal(1, 0, 0),
        HuffmanLine::normal(2, 0, 1),
        HuffmanLine::normal(3, 0, 2),
        HuffmanLine::normal(4, 3, 3),
        HuffmanLine::normal(5, 6, 11),
        HuffmanLine::lower(8, -257),
        HuffmanLine::upper(7, 75),
        HuffmanLine::oob(6),
    ])
}

pub fn table_b4() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 0, 1),
        HuffmanLine::normal(2, 0, 2),
        HuffmanLine::normal(3, 0, 3),
        HuffmanLine::normal(4, 3, 4),
        HuffmanLine::normal(5, 6, 12),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(5, 76),
    ])
}

pub fn table_b5() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(7, 8, -255),
        HuffmanLine::normal(1, 0, 1),
        HuffmanLine::normal(2, 0, 2),
        HuffmanLine::normal(3, 0, 3),
        HuffmanLine::normal(4, 3, 4),
        HuffmanLine::normal(5, 6, 12),
        HuffmanLine::lower(7, -256),
        HuffmanLine::upper(6, 76),
    ])
}

pub fn table_b6() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(5, 10, -2048),
        HuffmanLine::normal(4, 9, -1024),
        HuffmanLine::normal(4, 8, -512),
        HuffmanLine::normal(4, 7, -256),
        HuffmanLine::normal(5, 6, -128),
        HuffmanLine::normal(5, 5, -64),
        HuffmanLine::normal(4, 5, -32),
        HuffmanLine::normal(2, 7, 0),
        HuffmanLine::normal(3, 7, 128),
        HuffmanLine::normal(3, 8, 256),
        HuffmanLine::normal(4, 9, 512),
        HuffmanLine::normal(4, 10, 1024),
        HuffmanLine::lower(6, -2049),
        HuffmanLine::upper(6, 2048),
    ])
}

pub fn table_b7() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(4, 9, -1024),
        HuffmanLine::normal(3, 8, -512),
        HuffmanLine::normal(4, 7, -256),
        HuffmanLine::normal(5, 6, -128),
        HuffmanLine::normal(5, 5, -64),
        HuffmanLine::normal(4, 5, -32),
        HuffmanLine::normal(4, 5, 0),
        HuffmanLine::normal(5, 5, 32),
        HuffmanLine::normal(5, 6, 64),
        HuffmanLine::normal(4, 7, 128),
        HuffmanLine::normal(3, 8, 256),
        HuffmanLine::normal(3, 9, 512),
        HuffmanLine::normal(3, 10, 1024),
        HuffmanLine::lower(5, -1025),
        HuffmanLine::upper(5, 2048),
    ])
}

pub fn table_b8() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(8, 3, -15),
        HuffmanLine::normal(9, 1, -7),
        HuffmanLine::normal(8, 1, -5),
        HuffmanLine::normal(9, 0, -3),
        HuffmanLine::normal(7, 0, -2),
        HuffmanLine::normal(4, 0, -1),
        HuffmanLine::normal(2, 1, 0),
        HuffmanLine::normal(5, 0, 2),
        HuffmanLine::normal(6, 0, 3),
        HuffmanLine::normal(3, 4, 4),
        HuffmanLine::normal(6, 1, 20),
        HuffmanLine::normal(4, 4, 22),
        HuffmanLine::normal(4, 5, 38),
        HuffmanLine::normal(5, 6, 70),
        HuffmanLine::normal(5, 7, 134),
        HuffmanLine::normal(6, 7, 262),
        HuffmanLine::normal(7, 8, 390),
        HuffmanLine::normal(6, 10, 646),
        HuffmanLine::lower(9, -16),
        HuffmanLine::upper(9, 1670),
        HuffmanLine::oob(2),
    ])
}

pub fn table_b9() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(8, 4, -31),
        HuffmanLine::normal(9, 2, -15),
        HuffmanLine::normal(8, 2, -11),
        HuffmanLine::normal(9, 1, -7),
        HuffmanLine::normal(7, 1, -5),
        HuffmanLine::normal(4, 1, -3),
        HuffmanLine::normal(3, 1, -1),
        HuffmanLine::normal(3, 1, 1),
        HuffmanLine::normal(5, 1, 3),
        HuffmanLine::normal(6, 1, 5),
        HuffmanLine::normal(3, 5, 7),
        HuffmanLine::normal(6, 2, 39),
        HuffmanLine::normal(4, 5, 43),
        HuffmanLine::normal(4, 6, 75),
        HuffmanLine::normal(5, 7, 139),
        HuffmanLine::normal(5, 8, 267),
        HuffmanLine::normal(6, 8, 523),
        HuffmanLine::normal(7, 9, 779),
        HuffmanLine::normal(6, 11, 1291),
        HuffmanLine::lower(9, -32),
        HuffmanLine::upper(9, 3339),
        HuffmanLine::oob(2),
    ])
}

pub fn table_b10() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(7, 4, -21),
        HuffmanLine::normal(8, 0, -5),
        HuffmanLine::normal(7, 0, -4),
        HuffmanLine::normal(5, 0, -3),
        HuffmanLine::normal(2, 2, -2),
        HuffmanLine::normal(5, 0, 2),
        HuffmanLine::normal(6, 0, 3),
        HuffmanLine::normal(7, 0, 4),
        HuffmanLine::normal(8, 0, 5),
        HuffmanLine::normal(2, 6, 6),
        HuffmanLine::normal(5, 5, 70),
        HuffmanLine::normal(6, 5, 102),
        HuffmanLine::normal(6, 6, 134),
        HuffmanLine::normal(6, 7, 198),
        HuffmanLine::normal(6, 8, 326),
        HuffmanLine::normal(6, 9, 582),
        HuffmanLine::normal(6, 10, 1094),
        HuffmanLine::normal(7, 11, 2118),
        HuffmanLine::lower(8, -22),
        HuffmanLine::upper(8, 4166),
        HuffmanLine::oob(2),
    ])
}

pub fn table_b11() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 0, 1),
        HuffmanLine::normal(2, 1, 2),
        HuffmanLine::normal(4, 0, 4),
        HuffmanLine::normal(4, 1, 5),
        HuffmanLine::normal(5, 1, 7),
        HuffmanLine::normal(5, 2, 9),
        HuffmanLine::normal(6, 2, 13),
        HuffmanLine::normal(7, 2, 17),
        HuffmanLine::normal(7, 3, 21),
        HuffmanLine::normal(7, 4, 29),
        HuffmanLine::normal(7, 5, 45),
        HuffmanLine::normal(7, 6, 77),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(7, 141),
    ])
}

pub fn table_b12() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 0, 1),
        HuffmanLine::normal(2, 0, 2),
        HuffmanLine::normal(3, 1, 3),
        HuffmanLine::normal(5, 0, 5),
        HuffmanLine::normal(5, 1, 6),
        HuffmanLine::normal(6, 1, 8),
        HuffmanLine::normal(7, 0, 10),
        HuffmanLine::normal(7, 1, 11),
        HuffmanLine::normal(7, 2, 13),
        HuffmanLine::normal(7, 3, 17),
        HuffmanLine::normal(7, 4, 25),
        HuffmanLine::normal(8, 5, 41),
        HuffmanLine::normal(8, 32, 73),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(0, 0),
    ])
}

pub fn table_b13() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(1, 0, 1),
        HuffmanLine::normal(3, 0, 2),
        HuffmanLine::normal(4, 0, 3),
        HuffmanLine::normal(5, 0, 4),
        HuffmanLine::normal(4, 1, 5),
        HuffmanLine::normal(3, 3, 7),
        HuffmanLine::normal(6, 1, 15),
        HuffmanLine::normal(6, 2, 17),
        HuffmanLine::normal(6, 3, 21),
        HuffmanLine::normal(6, 4, 29),
        HuffmanLine::normal(6, 5, 45),
        HuffmanLine::normal(7, 6, 77),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(7, 141),
    ])
}

pub fn table_b14() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(3, 0, -2),
        HuffmanLine::normal(3, 0, -1),
        HuffmanLine::normal(1, 0, 0),
        HuffmanLine::normal(3, 0, 1),
        HuffmanLine::normal(3, 0, 2),
        HuffmanLine::lower(0, -1),
        HuffmanLine::upper(0, 3),
    ])
}

pub fn table_b15() -> HuffmanTable {
    HuffmanTable::new(vec![
        HuffmanLine::normal(7, 4, -24),
        HuffmanLine::normal(6, 2, -8),
        HuffmanLine::normal(5, 1, -4),
        HuffmanLine::normal(4, 0, -2),
        HuffmanLine::normal(3, 0, -1),
        HuffmanLine::normal(1, 0, 0),
        HuffmanLine::normal(3, 0, 1),
        HuffmanLine::normal(4, 0, 2),
        HuffmanLine::normal(5, 1, 3),
        HuffmanLine::normal(6, 2, 5),
        HuffmanLine::normal(7, 4, 9),
        HuffmanLine::lower(7, -25),
        HuffmanLine::upper(7, 25),
    ])
}

pub fn parse_custom_table(data: &[u8]) -> Option<HuffmanTable> {
    if data.len() < 9 {
        return None;
    }

    let flags = data[0];
    let htoob = flags & 1 != 0;
    let htps = ((flags >> 1) & 7) as u32 + 1;
    let htrs = ((flags >> 4) & 7) as u32 + 1;

    let htlow = i32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    let hthigh = i32::from_be_bytes([data[5], data[6], data[7], data[8]]);

    if htlow >= hthigh {
        return None;
    }

    let mut reader = HuffmanReader::new(&data[9..]);
    let mut lines = Vec::new();
    let mut cur_range_low = htlow;

    while cur_range_low < hthigh {
        let pref_len = reader.read_bits(htps)?;
        let range_len = reader.read_bits(htrs)?;
        lines.push(HuffmanLine::normal(pref_len, range_len, cur_range_low));
        if range_len >= 32 {
            break;
        }
        cur_range_low = cur_range_low.saturating_add(1i32.checked_shl(range_len).unwrap_or(i32::MAX));
    }

    let lower_pref = reader.read_bits(htps)?;
    lines.push(HuffmanLine::lower(lower_pref, htlow.saturating_sub(1)));

    let upper_pref = reader.read_bits(htps)?;
    lines.push(HuffmanLine::upper(upper_pref, hthigh));

    if htoob {
        let oob_pref = reader.read_bits(htps)?;
        lines.push(HuffmanLine::oob(oob_pref));
    }

    Some(HuffmanTable::new(lines))
}

pub fn build_runcode_table(lengths: &[u32]) -> HuffmanTable {
    let lines =
        lengths.iter().enumerate().map(|(index, &pref_len)| HuffmanLine::normal(pref_len, 0, index as i32)).collect();
    HuffmanTable::new(lines)
}
