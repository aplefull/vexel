use crate::log_warn;
use crate::utils::zstd::bitstream::ForwardBits;
use crate::utils::zstd::fse::FseTable;
use crate::utils::zstd::huffman::HuffmanTable;
use crate::utils::zstd::sequences::{decode_sequences, decode_table, SeqPart, Sequence};

pub const MAGIC_NUMBER: u32 = 0xFD2F_B528;
const SKIPPABLE_MAGIC_MASK: u32 = 0xFFFF_FFF0;
const SKIPPABLE_MAGIC: u32 = 0x184D_2A50;
const BLOCK_SIZE_MAX: usize = 128 * 1024;

#[derive(Default)]
struct FrameHeader {
    window_size: u64,
    frame_content_size: u64,
    single_segment: bool,
    content_checksum: bool,
}

#[derive(Default)]
struct FrameContext {
    header: FrameHeader,
    literals_table: HuffmanTable,
    ll_table: FseTable,
    of_table: FseTable,
    ml_table: FseTable,
    offset_history: [u64; 3],
}

pub fn decode(input: &[u8], max_output: usize) -> Vec<u8> {
    let mut output = Vec::new();
    let mut pos = 0usize;

    while pos + 4 <= input.len() {
        let magic = u32::from_le_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]]);

        if magic & SKIPPABLE_MAGIC_MASK == SKIPPABLE_MAGIC {
            if pos + 8 > input.len() {
                log_warn!("zstd skippable frame header truncated");
                break;
            }

            let size = u32::from_le_bytes([input[pos + 4], input[pos + 5], input[pos + 6], input[pos + 7]]) as usize;
            let next = (pos + 8).saturating_add(size);

            if next > input.len() {
                log_warn!("zstd skippable frame extends past end of input");
                break;
            }

            pos = next;
            continue;
        }

        if magic != MAGIC_NUMBER {
            if output.is_empty() {
                log_warn!("zstd input does not start with a frame magic number");
            }
            break;
        }

        pos += 4;

        if decode_frame(input, &mut pos, &mut output, max_output) != Some(true) {
            break;
        }

        if output.len() >= max_output {
            break;
        }
    }

    output
}

fn decode_frame(input: &[u8], pos: &mut usize, output: &mut Vec<u8>, max_output: usize) -> Option<bool> {
    let mut ctx = FrameContext { offset_history: [1, 4, 8], ..Default::default() };

    parse_frame_header(input, pos, &mut ctx.header)?;

    let frame_start = output.len();

    loop {
        if *pos + 3 > input.len() {
            log_warn!("zstd block header truncated");
            return Some(false);
        }

        let header = u32::from(input[*pos]) | u32::from(input[*pos + 1]) << 8 | u32::from(input[*pos + 2]) << 16;
        *pos += 3;

        let last_block = header & 1 != 0;
        let block_type = (header >> 1) & 3;
        let block_size = (header >> 3) as usize;

        match block_type {
            0 => {
                let end = (*pos + block_size).min(input.len());
                if end < *pos + block_size {
                    log_warn!("zstd raw block truncated");
                }

                append_limited(output, &input[*pos..end], max_output);
                *pos = end;
            }
            1 => {
                let Some(&byte) = input.get(*pos) else {
                    log_warn!("zstd RLE block byte missing");
                    return Some(false);
                };

                *pos += 1;
                if block_size > BLOCK_SIZE_MAX {
                    log_warn!("zstd RLE block size {} exceeds maximum, clamping", block_size);
                }
                let count = block_size.min(BLOCK_SIZE_MAX).min(max_output.saturating_sub(output.len()));
                output.resize(output.len() + count, byte);
            }
            2 => {
                let end = (*pos + block_size).min(input.len());
                if end < *pos + block_size {
                    log_warn!("zstd compressed block truncated");
                }

                let frame_output = output.len() - frame_start;
                let block_limit = max_output.min(output.len().saturating_add(BLOCK_SIZE_MAX));
                decompress_block(&mut ctx, &input[*pos..end], output, block_limit, frame_output);
                *pos = end;
            }
            _ => {
                log_warn!("zstd reserved block type encountered");
                return Some(false);
            }
        }

        if last_block {
            break;
        }

        if *pos >= input.len() {
            log_warn!("zstd frame ended before its last block");
            return Some(false);
        }

        if output.len() >= max_output {
            log_warn!("zstd output limit reached, stopping at {} bytes", output.len());
            return Some(false);
        }
    }

    if ctx.header.content_checksum {
        *pos = (*pos + 4).min(input.len());
    }

    let produced = (output.len() - frame_start) as u64;
    if ctx.header.frame_content_size != 0 && produced != ctx.header.frame_content_size {
        log_warn!("zstd frame produced {} bytes, header declared {}", produced, ctx.header.frame_content_size);
    }

    Some(true)
}

fn parse_frame_header(input: &[u8], pos: &mut usize, header: &mut FrameHeader) -> Option<()> {
    let Some(&descriptor) = input.get(*pos) else {
        log_warn!("zstd frame header descriptor missing");
        return None;
    };

    *pos += 1;

    let content_size_flag = descriptor >> 6;
    let single_segment = (descriptor >> 5) & 1 != 0;
    let content_checksum = (descriptor >> 2) & 1 != 0;
    let dictionary_id_flag = descriptor & 3;

    if (descriptor >> 3) & 1 != 0 {
        log_warn!("zstd frame header reserved bit set");
    }

    header.single_segment = single_segment;
    header.content_checksum = content_checksum;

    if !single_segment {
        let Some(&window_descriptor) = input.get(*pos) else {
            log_warn!("zstd window descriptor missing");
            return None;
        };

        *pos += 1;

        let exponent = u64::from(window_descriptor >> 3);
        let mantissa = u64::from(window_descriptor & 7);
        let window_base = 1u64 << (10 + exponent).min(63);

        header.window_size = window_base + (window_base / 8) * mantissa;
    }

    if dictionary_id_flag != 0 {
        let bytes = [0usize, 1, 2, 4][dictionary_id_flag as usize];

        if *pos + bytes > input.len() {
            log_warn!("zstd dictionary id truncated");
            return None;
        }

        let mut id = 0u32;
        for index in 0..bytes {
            id |= u32::from(input[*pos + index]) << (index * 8);
        }

        *pos += bytes;

        if id != 0 {
            log_warn!("zstd frame requires dictionary {}, decoding without it", id);
        }
    }

    if single_segment || content_size_flag != 0 {
        let bytes = [1usize, 2, 4, 8][content_size_flag as usize];

        if *pos + bytes > input.len() {
            log_warn!("zstd frame content size truncated");
            return None;
        }

        let mut size = 0u64;
        for index in 0..bytes {
            size |= u64::from(input[*pos + index]) << (index * 8);
        }

        *pos += bytes;

        header.frame_content_size = if bytes == 2 { size + 256 } else { size };
    }

    if single_segment {
        header.window_size = header.frame_content_size;
    }

    Some(())
}

fn append_limited(output: &mut Vec<u8>, data: &[u8], max_output: usize) -> usize {
    let count = data.len().min(max_output.saturating_sub(output.len()));
    output.extend_from_slice(&data[..count]);
    count
}

fn decompress_block(ctx: &mut FrameContext, data: &[u8], output: &mut Vec<u8>, max_output: usize, frame_output: usize) {
    let mut input = ForwardBits::new(data);

    let literals = decode_literals(ctx, &mut input, data);
    let consumed = input.bytes_consumed();

    if consumed >= data.len() {
        append_limited(output, &literals, max_output);
        return;
    }

    let sequences = decode_sequences_section(ctx, &data[consumed..]);

    execute_sequences(ctx, &literals, &sequences, output, max_output, frame_output);
}

fn decode_literals(ctx: &mut FrameContext, input: &mut ForwardBits, data: &[u8]) -> Vec<u8> {
    let block_type = input.read(2) as u8;
    let size_format = input.read(2) as u8;

    if block_type <= 1 {
        let size = match size_format {
            0 | 2 => {
                input.rewind(1);
                input.read(5) as usize
            }
            1 => input.read(12) as usize,
            _ => input.read(20) as usize,
        };

        let size = size.min(BLOCK_SIZE_MAX);
        let start = input.bytes_consumed().min(data.len());

        if block_type == 0 {
            let end = (start + size).min(data.len());
            if end < start + size {
                log_warn!("zstd raw literals truncated");
            }

            input.skip_bytes(end - start);
            return data[start..end].to_vec();
        }

        let byte = data.get(start).copied().unwrap_or(0);
        input.read(8);

        return vec![byte; size];
    }

    let (num_streams, regenerated_size, compressed_size) = match size_format {
        0 => (1usize, input.read(10) as usize, input.read(10) as usize),
        1 => (4usize, input.read(10) as usize, input.read(10) as usize),
        2 => (4usize, input.read(14) as usize, input.read(14) as usize),
        _ => (4usize, input.read(18) as usize, input.read(18) as usize),
    };

    let regenerated_size = regenerated_size.min(BLOCK_SIZE_MAX);
    let header_end = input.bytes_consumed();
    let block_end = (header_end + compressed_size).min(data.len());

    if block_end < header_end + compressed_size {
        log_warn!("zstd compressed literals truncated");
    }

    if header_end >= data.len() {
        log_warn!("zstd literals section header overran the block");
        return Vec::new();
    }

    let huf_data = &data[header_end..block_end];
    let mut huf_input = ForwardBits::new(huf_data);

    if block_type == 2 {
        match HuffmanTable::parse_description(&mut huf_input, huf_data) {
            Some(table) => ctx.literals_table = table,
            None => {
                input.skip_bytes(block_end - header_end);
                return Vec::new();
            }
        }
    } else if !ctx.literals_table.is_initialized() {
        log_warn!("zstd literals repeat mode without a previous Huffman table");
        input.skip_bytes(block_end - header_end);
        return Vec::new();
    }

    let stream_start = huf_input.bytes_consumed();
    let mut literals = Vec::with_capacity(regenerated_size);

    if stream_start < huf_data.len() {
        let streams = &huf_data[stream_start..];

        if num_streams == 1 {
            ctx.literals_table.decompress_single(streams, &mut literals, regenerated_size);
        } else {
            ctx.literals_table.decompress_four(streams, &mut literals, regenerated_size);
        }
    }

    if literals.len() != regenerated_size {
        log_warn!("zstd decoded {} literals, expected {}", literals.len(), regenerated_size);
        literals.resize(regenerated_size, 0);
    }

    input.skip_bytes(block_end - header_end);

    literals
}

fn decode_sequences_section(ctx: &mut FrameContext, data: &[u8]) -> Vec<Sequence> {
    let mut input = ForwardBits::new(data);
    let first = input.read(8) as usize;

    let count = if first < 128 {
        first
    } else if first < 255 {
        ((first - 128) << 8) + input.read(8) as usize
    } else {
        input.read(16) as usize + 0x7F00
    };

    if count == 0 {
        return Vec::new();
    }

    let modes = input.read(8) as u8;

    if modes & 3 != 0 {
        log_warn!("zstd sequence compression modes reserved bits set");
    }

    let ll_mode = (modes >> 6) & 3;
    let of_mode = (modes >> 4) & 3;
    let ml_mode = (modes >> 2) & 3;

    let mut ll_table = std::mem::take(&mut ctx.ll_table);
    let mut of_table = std::mem::take(&mut ctx.of_table);
    let mut ml_table = std::mem::take(&mut ctx.ml_table);

    let tables_ok = decode_table(&mut ll_table, &mut input, data, SeqPart::LiteralLength, ll_mode).is_some()
        && decode_table(&mut of_table, &mut input, data, SeqPart::Offset, of_mode).is_some()
        && decode_table(&mut ml_table, &mut input, data, SeqPart::MatchLength, ml_mode).is_some();

    ctx.ll_table = ll_table;
    ctx.of_table = of_table;
    ctx.ml_table = ml_table;

    if !tables_ok {
        return Vec::new();
    }

    let start = input.bytes_consumed();
    if start >= data.len() {
        log_warn!("zstd sequence bitstream missing");
        return Vec::new();
    }

    decode_sequences(&data[start..], count, &ctx.ll_table, &ctx.of_table, &ctx.ml_table)
}

fn compute_offset(sequence: &Sequence, history: &mut [u64; 3]) -> u64 {
    if sequence.offset > 3 {
        let offset = u64::from(sequence.offset) - 3;
        history[2] = history[1];
        history[1] = history[0];
        history[0] = offset;
        return offset;
    }

    let mut index = sequence.offset as usize - 1;
    if sequence.literal_length == 0 {
        index += 1;
    }

    if index == 0 {
        return history[0];
    }

    let offset = if index < 3 { history[index] } else { history[0].saturating_sub(1) }.max(1);

    if index > 1 {
        history[2] = history[1];
    }
    history[1] = history[0];
    history[0] = offset;

    offset
}

fn execute_sequences(
    ctx: &mut FrameContext,
    literals: &[u8],
    sequences: &[Sequence],
    output: &mut Vec<u8>,
    max_output: usize,
    frame_output: usize,
) {
    let mut literal_pos = 0usize;
    let mut total_output = frame_output;
    let mut history = ctx.offset_history;

    for sequence in sequences {
        let literal_length = (sequence.literal_length as usize).min(literals.len() - literal_pos);
        if literal_length < sequence.literal_length as usize {
            log_warn!("zstd sequence requests more literals than available");
        }

        let appended = append_limited(output, &literals[literal_pos..literal_pos + literal_length], max_output);
        literal_pos += literal_length;
        total_output += appended;

        if output.len() >= max_output {
            break;
        }

        let offset = compute_offset(sequence, &mut history) as usize;
        let match_length = sequence.match_length as usize;

        if offset == 0 || offset > total_output {
            log_warn!("zstd match offset {} exceeds available output {}", offset, total_output);
            continue;
        }

        let count = match_length.min(max_output.saturating_sub(output.len()));
        let start = output.len() - offset;

        output.reserve(count);
        for src in (start..).take(count) {
            let byte = output[src];
            output.push(byte);
        }

        total_output += count;

        if output.len() >= max_output {
            break;
        }
    }

    ctx.offset_history = history;

    if literal_pos < literals.len() {
        append_limited(output, &literals[literal_pos..], max_output);
    }
}
