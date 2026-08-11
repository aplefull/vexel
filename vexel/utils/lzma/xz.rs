use crate::log_warn;
use crate::utils::lzma::dictionary::Dictionary;
use crate::utils::lzma::{bcj, delta, lzma2};

pub const STREAM_MAGIC: [u8; 6] = [0xfd, b'7', b'z', b'X', b'Z', 0x00];
const FOOTER_MAGIC: [u8; 2] = *b"YZ";

const FOOTER_SIZE: usize = 12;

const FILTER_DELTA: u64 = 0x03;
const FILTER_LZMA2: u64 = 0x21;

const INDEX_INDICATOR: u8 = 0x00;

fn check_size(check_id: u8) -> usize {
    match check_id & 0x0f {
        0x00 => 0,
        0x01..=0x03 => 4,
        0x04..=0x06 => 8,
        0x07..=0x09 => 16,
        0x0a..=0x0c => 32,
        _ => 64,
    }
}

fn read_multibyte(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;

    for i in 0..9 {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << (i * 7);

        if byte & 0x80 == 0 {
            return Some(value);
        }
    }

    None
}

struct Filter {
    id: u64,
    properties: Vec<u8>,
}

struct BlockHeader {
    filters: Vec<Filter>,
    compressed_size: Option<u64>,
    size: usize,
}

fn parse_block_header(data: &[u8]) -> Option<BlockHeader> {
    let encoded_size = *data.first()?;
    let size = (usize::from(encoded_size) + 1) * 4;

    if size > data.len() {
        return None;
    }

    let header = &data[..size];
    let flags = *header.get(1)?;
    let filter_count = usize::from(flags & 0x03) + 1;
    let mut pos = 2usize;

    let compressed_size = if flags & 0x40 != 0 { Some(read_multibyte(header, &mut pos)?) } else { None };

    if flags & 0x80 != 0 {
        read_multibyte(header, &mut pos)?;
    }

    let mut filters = Vec::with_capacity(filter_count);

    for _ in 0..filter_count {
        let id = read_multibyte(header, &mut pos)?;
        let props_size = read_multibyte(header, &mut pos)? as usize;

        if pos + props_size > header.len() {
            return None;
        }

        filters.push(Filter { id, properties: header[pos..pos + props_size].to_vec() });
        pos += props_size;
    }

    Some(BlockHeader { filters, compressed_size, size })
}

fn apply_non_last_filters(filters: &[Filter], output: &mut [u8]) {
    for filter in filters.iter().rev() {
        match filter.id {
            FILTER_DELTA => {
                let distance = usize::from(*filter.properties.first().unwrap_or(&0)) + 1;
                delta::decode_in_place(output, distance);
            }
            id if bcj::is_supported(id) => {
                let start_offset = match filter.properties.get(..4) {
                    Some(bytes) => u32::from_le_bytes(bytes.try_into().unwrap()),
                    None => 0,
                };
                bcj::decode(id, output, start_offset);
            }
            other => log_warn!("xz unsupported filter {:#x}, output left unfiltered", other),
        }
    }
}

pub fn decode(input: &[u8], capacity_hint: usize, max_output: usize) -> Vec<u8> {
    if input.len() < STREAM_MAGIC.len() || input[..STREAM_MAGIC.len()] != STREAM_MAGIC {
        log_warn!("xz stream header magic not found");
        return Vec::new();
    }

    let mut output = Vec::with_capacity(capacity_hint.min(max_output));
    let mut pos = 0usize;

    while pos < input.len() {
        if input[pos..].len() < STREAM_MAGIC.len() || input[pos..pos + STREAM_MAGIC.len()] != STREAM_MAGIC {
            break;
        }

        let consumed = decode_stream(&input[pos..], capacity_hint, max_output, &mut output);

        match consumed {
            Some(consumed) if consumed > 0 => pos += consumed,
            _ => break,
        }

        if output.len() >= max_output {
            break;
        }

        pos += skip_stream_padding(&input[pos..]);
    }

    output
}

fn skip_stream_padding(input: &[u8]) -> usize {
    let mut count = 0usize;

    while input.len() >= count + 4 && input[count..count + 4] == [0, 0, 0, 0] {
        count += 4;
    }

    count
}

fn decode_stream(input: &[u8], capacity_hint: usize, max_output: usize, output: &mut Vec<u8>) -> Option<usize> {
    if input.len() < 12 {
        log_warn!("xz stream truncated in header");
        return None;
    }

    let check = input[7];
    let mut pos = 12usize;

    while pos < input.len() {
        if input[pos] == INDEX_INDICATOR {
            return Some(pos + index_and_footer_size(&input[pos..]));
        }

        let Some(header) = parse_block_header(&input[pos..]) else {
            log_warn!("xz malformed block header, keeping {} decoded bytes", output.len());
            return None;
        };

        pos += header.size;

        let Some(last) = header.filters.last() else {
            log_warn!("xz block has no filters");
            return None;
        };

        if last.id != FILTER_LZMA2 {
            log_warn!("xz unsupported compression filter {:#x}", last.id);
            return None;
        }

        let end = match header.compressed_size {
            Some(size) => usize::try_from(size)
                .ok()
                .and_then(|size| pos.checked_add(size))
                .unwrap_or(input.len())
                .min(input.len()),
            None => input.len(),
        };

        let block_start = output.len();
        let remaining = max_output.saturating_sub(output.len());

        if remaining == 0 {
            return None;
        }

        let mut dict = Dictionary::new(remaining.min(capacity_hint.max(1 << 16)), remaining);
        let consumed = lzma2::decode_into(&input[pos..end], &mut dict);
        let decoded = dict.into_vec();
        output.extend_from_slice(&decoded);

        apply_non_last_filters(&header.filters[..header.filters.len() - 1], &mut output[block_start..]);

        let compressed_size = match header.compressed_size {
            Some(size) => usize::try_from(size).ok(),
            None => Some(consumed),
        };

        let advance = compressed_size
            .and_then(|size| size.checked_add(3).map(|padded| padded & !3))
            .and_then(|padded| pos.checked_add(padded))
            .and_then(|next| next.checked_add(check_size(check)));

        match advance {
            Some(next) if next > pos => pos = next,
            _ => return None,
        }

        if output.len() >= max_output {
            return None;
        }
    }

    None
}

fn index_and_footer_size(input: &[u8]) -> usize {
    let mut pos = 1usize;

    let index_size = match read_multibyte(input, &mut pos) {
        Some(record_count) => {
            let records_ok = (0..record_count)
                .all(|_| read_multibyte(input, &mut pos).is_some() && read_multibyte(input, &mut pos).is_some());

            match records_ok {
                true => Some(((pos + 3) & !3) + 4),
                false => None,
            }
        }
        None => None,
    };

    let Some(index_size) = index_size else {
        return input.len();
    };

    let total = index_size + FOOTER_SIZE;

    if input.len() < total || input[total - 2..total] != FOOTER_MAGIC {
        return match backward_size_total(input) {
            Some(total) => total,
            None => input.len(),
        };
    }

    total
}

fn backward_size_total(input: &[u8]) -> Option<usize> {
    let mut offset = 4usize;

    while offset + FOOTER_SIZE <= input.len() {
        if input[offset + FOOTER_SIZE - 2..offset + FOOTER_SIZE] == FOOTER_MAGIC {
            let stored = u32::from_le_bytes(input[offset + 4..offset + 8].try_into().ok()?);
            let index_size = (stored as usize).checked_add(1)?.checked_mul(4)?;

            if index_size == offset {
                return Some(offset + FOOTER_SIZE);
            }
        }

        offset += 4;
    }

    None
}
