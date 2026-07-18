use crate::decoders::dicom::types::{tag_group, tags, Vr};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

const MAX_SEQUENCE_DEPTH: u8 = 12;
const PREVIEW_LIMIT: usize = 256;
pub const UNDEFINED_LENGTH: u32 = 0xFFFF_FFFF;

pub struct DataCursor<'a> {
    data: &'a [u8],
    pos: usize,
    pub big_endian: bool,
}

impl<'a> DataCursor<'a> {
    pub fn new(data: &'a [u8], big_endian: bool) -> Self {
        Self { data, pos: 0, big_endian }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos.min(self.data.len());
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn read_u16(&mut self) -> VexelResult<u16> {
        if self.remaining() < 2 {
            return Err(VexelError::Custom("DICOM: unexpected end of data".to_string()));
        }
        let b = [self.data[self.pos], self.data[self.pos + 1]];
        self.pos += 2;
        Ok(if self.big_endian { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) })
    }

    pub fn read_u32(&mut self) -> VexelResult<u32> {
        if self.remaining() < 4 {
            return Err(VexelError::Custom("DICOM: unexpected end of data".to_string()));
        }
        let b = [self.data[self.pos], self.data[self.pos + 1], self.data[self.pos + 2], self.data[self.pos + 3]];
        self.pos += 4;
        Ok(if self.big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    }

    pub fn read_bytes(&mut self, n: usize) -> &'a [u8] {
        let n = n.min(self.remaining());
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        slice
    }

    pub fn peek_u32_at(&self, offset: usize) -> Option<u32> {
        let p = self.pos + offset;
        if p + 4 > self.data.len() {
            return None;
        }
        let b = [self.data[p], self.data[p + 1], self.data[p + 2], self.data[p + 3]];
        Some(if self.big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    }

    fn peek_tag(&self) -> Option<u32> {
        if self.remaining() < 4 {
            return None;
        }
        let g = if self.big_endian {
            u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]])
        } else {
            u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]])
        };
        let e = if self.big_endian {
            u16::from_be_bytes([self.data[self.pos + 2], self.data[self.pos + 3]])
        } else {
            u16::from_le_bytes([self.data[self.pos + 2], self.data[self.pos + 3]])
        };
        Some(((g as u32) << 16) | e as u32)
    }
}

#[derive(Debug, Clone)]
pub enum ElementValue {
    Bytes(Vec<u8>),
    Sequence(Vec<DataSet>),
    Fragments { offset_table: Vec<u32>, fragments: Vec<Vec<u8>> },
}

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: u32,
    pub vr: Vr,
    pub length: u32,
    pub value: ElementValue,
}

#[derive(Debug, Clone, Default)]
pub struct DataSet {
    pub elements: Vec<Element>,
}

impl DataSet {
    pub fn get(&self, tag: u32) -> Option<&Element> {
        self.elements.iter().find(|e| e.tag == tag)
    }

    pub fn bytes(&self, tag: u32) -> Option<&[u8]> {
        match self.get(tag) {
            Some(Element { value: ElementValue::Bytes(b), .. }) => Some(b.as_slice()),
            _ => None,
        }
    }

    pub fn sequence(&self, tag: u32) -> Option<&Vec<DataSet>> {
        match self.get(tag) {
            Some(Element { value: ElementValue::Sequence(items), .. }) => Some(items),
            _ => None,
        }
    }

    pub fn string(&self, tag: u32) -> Option<String> {
        let bytes = self.bytes(tag)?;
        let s: String = bytes.iter().map(|&b| b as char).collect();
        let trimmed = s.trim_matches(|c: char| c == '\0' || c == ' ').to_string();
        Some(trimmed)
    }

    pub fn u16_value(&self, tag: u32) -> Option<u16> {
        let element = self.get(tag)?;
        let bytes = match &element.value {
            ElementValue::Bytes(b) => b,
            _ => return None,
        };
        if bytes.len() < 2 {
            return bytes.first().map(|&b| b as u16);
        }
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn u16_list(&self, tag: u32) -> Vec<u16> {
        match self.bytes(tag) {
            Some(bytes) => bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
            None => Vec::new(),
        }
    }

    pub fn f64_value(&self, tag: u32) -> Option<f64> {
        self.f64_list(tag).first().copied()
    }

    pub fn f64_list(&self, tag: u32) -> Vec<f64> {
        let element = match self.get(tag) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let bytes = match &element.value {
            ElementValue::Bytes(b) => b,
            _ => return Vec::new(),
        };
        match element.vr {
            Vr::Fl => bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
            Vr::Fd => bytes
                .chunks_exact(8)
                .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
                .collect(),
            Vr::Us => bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as f64).collect(),
            Vr::Ss => bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f64).collect(),
            Vr::Ul => bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
            Vr::Sl => bytes.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64).collect(),
            _ => {
                let s: String = bytes.iter().map(|&b| b as char).collect();
                s.split('\\').filter_map(|p| p.trim().trim_matches('\0').parse::<f64>().ok()).collect()
            }
        }
    }

    pub fn int_value(&self, tag: u32) -> Option<i64> {
        let element = self.get(tag)?;
        let bytes = match &element.value {
            ElementValue::Bytes(b) => b,
            _ => return None,
        };
        match element.vr {
            Vr::Us if bytes.len() >= 2 => Some(u16::from_le_bytes([bytes[0], bytes[1]]) as i64),
            Vr::Ss if bytes.len() >= 2 => Some(i16::from_le_bytes([bytes[0], bytes[1]]) as i64),
            Vr::Ul if bytes.len() >= 4 => Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64),
            Vr::Sl if bytes.len() >= 4 => Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64),
            _ => {
                let s: String = bytes.iter().map(|&b| b as char).collect();
                let trimmed = s.trim_matches(|c: char| c == '\0' || c == ' ').to_string();
                if let Ok(v) = trimmed.split('\\').next().unwrap_or("").trim().parse::<i64>() {
                    return Some(v);
                }
                if bytes.len() == 2 {
                    return Some(u16::from_le_bytes([bytes[0], bytes[1]]) as i64);
                }
                if bytes.len() == 4 {
                    return Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64);
                }
                None
            }
        }
    }
}

fn keep_full_value(tag: u32) -> bool {
    let group = tag_group(tag);
    group == 0x0002
        || group == 0x0028
        || matches!(
            tag,
            tags::FRAME_TIME
                | tags::FRAME_TIME_VECTOR
                | tags::RECOMMENDED_DISPLAY_FRAME_RATE
                | tags::NUMBER_OF_FRAMES
                | tags::PIXEL_DATA
                | tags::FLOAT_PIXEL_DATA
                | tags::DOUBLE_FLOAT_PIXEL_DATA
        )
}

fn is_short_string_vr(vr: Vr) -> bool {
    vr.is_string() || matches!(vr, Vr::Ds | Vr::Is)
}

pub struct Parser {
    pub explicit_vr: bool,
}

impl Parser {
    pub fn new(explicit_vr: bool) -> Self {
        Self { explicit_vr }
    }

    pub fn parse_meta(&self, cursor: &mut DataCursor) -> DataSet {
        let mut dataset = DataSet::default();

        while cursor.remaining() >= 8 {
            match cursor.peek_tag() {
                Some(t) if tag_group(t) == 0x0002 => {}
                _ => break,
            }

            match self.parse_element(cursor, 0) {
                Ok(Some(element)) => dataset.elements.push(element),
                Ok(None) => break,
                Err(e) => {
                    log_warn!("DICOM: stopping meta parse at offset {}: {}", cursor.position(), e);
                    break;
                }
            }
        }

        dataset
    }

    pub fn parse_dataset(&self, cursor: &mut DataCursor, depth: u8) -> DataSet {
        let mut dataset = DataSet::default();

        while cursor.remaining() >= 8 {
            match cursor.peek_tag() {
                Some(tags::ITEM_DELIMITER) | Some(tags::SEQUENCE_DELIMITER) => break,
                Some(_) => {}
                None => break,
            }

            match self.parse_element(cursor, depth) {
                Ok(Some(element)) => dataset.elements.push(element),
                Ok(None) => break,
                Err(e) => {
                    log_warn!("DICOM: stopping dataset parse at offset {}: {}", cursor.position(), e);
                    break;
                }
            }
        }

        dataset
    }

    fn parse_element(&self, cursor: &mut DataCursor, depth: u8) -> VexelResult<Option<Element>> {
        let group = cursor.read_u16()?;
        let element = cursor.read_u16()?;
        let tag = ((group as u32) << 16) | element as u32;

        let (vr, length) = self.read_vr_and_length(cursor, tag)?;

        if tag == tags::PIXEL_DATA && length == UNDEFINED_LENGTH {
            let (offset_table, fragments) = self.parse_fragments(cursor)?;
            return Ok(Some(Element { tag, vr, length, value: ElementValue::Fragments { offset_table, fragments } }));
        }

        let is_sequence = vr == Vr::Sq
            || (length == UNDEFINED_LENGTH)
            || (length != 0
                && length >= 8
                && vr == Vr::Implicit
                && tag_group(tag) != 0x7FE0
                && cursor.peek_u32_at(0).map(|t| tag_from_words(t, cursor.big_endian) == tags::ITEM).unwrap_or(false)
                && cursor
                    .peek_u32_at(4)
                    .map(|len| len == UNDEFINED_LENGTH || (len as usize) <= length as usize)
                    .unwrap_or(false));

        if is_sequence {
            if depth >= MAX_SEQUENCE_DEPTH {
                log_warn!("DICOM: sequence nesting too deep at tag {:08X}, skipping", tag);
                if length != UNDEFINED_LENGTH {
                    cursor.read_bytes(length as usize);
                    return Ok(Some(Element { tag, vr, length, value: ElementValue::Sequence(Vec::new()) }));
                }
                return Ok(None);
            }

            let implicit_content = vr == Vr::Un;
            let items = self.parse_sequence_items(cursor, length, depth + 1, implicit_content)?;
            return Ok(Some(Element { tag, vr, length, value: ElementValue::Sequence(items) }));
        }

        let len = length as usize;
        if len > cursor.remaining() {
            log_warn!(
                "DICOM: element {:08X} length {} exceeds remaining data {}, truncating",
                tag,
                len,
                cursor.remaining()
            );
        }

        let mut value = if keep_full_value(tag) {
            cursor.read_bytes(len).to_vec()
        } else {
            let keep = len.min(PREVIEW_LIMIT);
            let bytes = cursor.read_bytes(len);
            bytes[..keep.min(bytes.len())].to_vec()
        };

        if cursor.big_endian {
            swap_endian_units(&mut value, endian_unit_size(vr));
        }

        Ok(Some(Element { tag, vr, length, value: ElementValue::Bytes(value) }))
    }

    fn read_vr_and_length(&self, cursor: &mut DataCursor, tag: u32) -> VexelResult<(Vr, u32)> {
        if tag_group(tag) == 0xFFFE {
            return Ok((Vr::Implicit, cursor.read_u32()?));
        }

        if !self.explicit_vr {
            return Ok((Vr::Implicit, cursor.read_u32()?));
        }

        if cursor.remaining() < 2 {
            return Err(VexelError::Custom("DICOM: unexpected end of data".to_string()));
        }

        let vr_bytes = [cursor.read_bytes(1)[0], cursor.read_bytes(1)[0]];
        match Vr::from_bytes(vr_bytes) {
            Some(vr) if vr.has_long_length() => {
                cursor.read_u16()?;
                Ok((vr, cursor.read_u32()?))
            }
            Some(vr) => Ok((vr, cursor.read_u16()? as u32)),
            None => {
                cursor.set_position(cursor.position() - 2);
                Ok((Vr::Implicit, cursor.read_u32()?))
            }
        }
    }

    fn parse_sequence_items(
        &self,
        cursor: &mut DataCursor,
        length: u32,
        depth: u8,
        implicit_content: bool,
    ) -> VexelResult<Vec<DataSet>> {
        let mut items = Vec::new();
        let end =
            if length == UNDEFINED_LENGTH { usize::MAX } else { cursor.position().saturating_add(length as usize) };

        while cursor.remaining() >= 8 && cursor.position() < end {
            let item_tag = cursor.read_u16().map(|g| (g as u32) << 16)? | cursor.read_u16()? as u32;
            let item_len = cursor.read_u32()?;

            match item_tag {
                tags::SEQUENCE_DELIMITER => break,
                tags::ITEM => {
                    let item_parser = if implicit_content { Parser::new(false) } else { Parser::new(self.explicit_vr) };
                    if item_len == UNDEFINED_LENGTH {
                        let item = item_parser.parse_dataset(cursor, depth);
                        items.push(item);
                        if cursor.peek_tag() == Some(tags::ITEM_DELIMITER) {
                            cursor.read_bytes(8);
                        }
                    } else {
                        let item_len = (item_len as usize).min(cursor.remaining());
                        let item_end = cursor.position() + item_len;
                        let mut sub = DataCursor::new(cursor.read_bytes(item_len), cursor.big_endian);
                        let item = item_parser.parse_dataset(&mut sub, depth);
                        items.push(item);
                        cursor.set_position(item_end);
                    }
                }
                _ => {
                    log_warn!("DICOM: unexpected tag {:08X} inside sequence, stopping", item_tag);
                    break;
                }
            }
        }

        Ok(items)
    }

    fn parse_fragments(&self, cursor: &mut DataCursor) -> VexelResult<(Vec<u32>, Vec<Vec<u8>>)> {
        let mut offset_table = Vec::new();
        let mut fragments = Vec::new();
        let mut first = true;

        while cursor.remaining() >= 8 {
            let item_tag = cursor.read_u16().map(|g| (g as u32) << 16)? | cursor.read_u16()? as u32;
            let item_len = cursor.read_u32()?;

            match item_tag {
                tags::SEQUENCE_DELIMITER => break,
                tags::ITEM => {
                    let len = (item_len as usize).min(cursor.remaining());
                    let bytes = cursor.read_bytes(len);
                    if first {
                        offset_table =
                            bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                        first = false;
                    } else {
                        fragments.push(bytes.to_vec());
                    }
                }
                _ => {
                    log_warn!("DICOM: unexpected tag {:08X} in pixel data fragments, stopping", item_tag);
                    break;
                }
            }
        }

        Ok((offset_table, fragments))
    }
}

fn endian_unit_size(vr: Vr) -> usize {
    match vr {
        Vr::Us | Vr::Ss | Vr::Ow | Vr::At => 2,
        Vr::Ul | Vr::Sl | Vr::Fl | Vr::Of | Vr::Ol => 4,
        Vr::Fd | Vr::Od | Vr::Sv | Vr::Uv | Vr::Ov => 8,
        _ => 1,
    }
}

fn swap_endian_units(bytes: &mut [u8], unit: usize) {
    if unit <= 1 {
        return;
    }
    for chunk in bytes.chunks_exact_mut(unit) {
        chunk.reverse();
    }
}

fn tag_from_words(raw: u32, big_endian: bool) -> u32 {
    if big_endian {
        raw
    } else {
        let g = raw & 0xFFFF;
        let e = raw >> 16;
        (g << 16) | e
    }
}

pub fn element_preview(element: &Element) -> String {
    match &element.value {
        ElementValue::Sequence(items) => format!("sequence, {} item(s)", items.len()),
        ElementValue::Fragments { fragments, .. } => format!("encapsulated, {} fragment(s)", fragments.len()),
        ElementValue::Bytes(bytes) => {
            if bytes.is_empty() {
                return String::new();
            }
            if is_short_string_vr(element.vr) || (element.vr == Vr::Implicit && looks_like_text(bytes)) {
                let s: String =
                    bytes.iter().take(64).map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' }).collect();
                let s = s.trim_matches(|c: char| c == '.' || c == ' ').to_string();
                if !s.is_empty() {
                    return s;
                }
            }
            match element.vr {
                Vr::Us | Vr::Ss | Vr::Ul | Vr::Sl | Vr::Fl | Vr::Fd => {
                    let values: Vec<String> = match element.vr {
                        Vr::Us => bytes
                            .chunks_exact(2)
                            .take(8)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]).to_string())
                            .collect(),
                        Vr::Ss => bytes
                            .chunks_exact(2)
                            .take(8)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]).to_string())
                            .collect(),
                        Vr::Ul => bytes
                            .chunks_exact(4)
                            .take(8)
                            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
                            .collect(),
                        Vr::Sl => bytes
                            .chunks_exact(4)
                            .take(8)
                            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
                            .collect(),
                        Vr::Fl => bytes
                            .chunks_exact(4)
                            .take(8)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
                            .collect(),
                        _ => bytes
                            .chunks_exact(8)
                            .take(8)
                            .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]).to_string())
                            .collect(),
                    };
                    values.join(", ")
                }
                _ => format!("<{} bytes>", element.length),
            }
        }
    }
}

fn looks_like_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(32)];
    sample.iter().all(|&b| (0x20..0x7F).contains(&b) || b == 0 || b == b'\\' || b == 0x1B)
        && sample.iter().any(|&b| b.is_ascii_alphanumeric())
}
