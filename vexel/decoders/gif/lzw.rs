use crate::utils::error::VexelResult;
use crate::{log_warn};

use super::types::GifFrameInfo;

pub fn decompress_lzw(frame: &GifFrameInfo) -> VexelResult<Vec<u8>> {
    let min_code_size = frame.lzw_minimum_code_size.clamp(2, 8) as u32;
    let clear_code = 1u32 << min_code_size;
    let end_code = clear_code + 1;

    let initial_entries = (end_code + 1) as usize;

    let mut arena: Vec<u8> = Vec::with_capacity(65536);
    let mut table: Vec<(u32, u32)> = Vec::with_capacity(4096);

    for i in 0..clear_code {
        let offset = arena.len() as u32;
        arena.push(i as u8);
        table.push((offset, 1));
    }
    table.push((0, 0));
    table.push((0, 0));

    let initial_arena_len = arena.len();

    let data = &frame.data;
    let data_len = data.len();

    let mut bit_buf: u64 = 0;
    let mut n_bits: u32 = 0;
    let mut byte_pos: usize = 0;

    let read_code = |bit_buf: &mut u64, n_bits: &mut u32, byte_pos: &mut usize, width: u32| -> Option<u32> {
        while *n_bits < width {
            if *byte_pos >= data_len {
                return None;
            }
            let space = (64 - *n_bits) / 8;
            let available = (data_len - *byte_pos).min(space as usize).min(4);
            for _ in 0..available {
                *bit_buf |= (data[*byte_pos] as u64) << *n_bits;
                *byte_pos += 1;
                *n_bits += 8;
            }
        }
        let code = (*bit_buf & ((1u64 << width) - 1)) as u32;
        *bit_buf >>= width;
        *n_bits -= width;
        Some(code)
    };

    let mut result: Vec<u8> = Vec::with_capacity(frame.width as usize * frame.height as usize);

    let mut code_size = min_code_size + 1;
    let mut next_code = end_code + 1;
    let mut prev_code: Option<u32> = None;

    loop {
        let code = match read_code(&mut bit_buf, &mut n_bits, &mut byte_pos, code_size) {
            Some(c) => c,
            None => break,
        };

        if code == clear_code {
            code_size = min_code_size + 1;
            next_code = end_code + 1;
            table.truncate(initial_entries);
            arena.truncate(initial_arena_len);
            prev_code = None;
            continue;
        }

        if code == end_code {
            break;
        }

        if let Some(prev) = prev_code {
            if code < next_code {
                let (offset, len) = match table.get(code as usize) {
                    Some(&(_, 0)) | None => {
                        log_warn!("Invalid LZW code: {}", code);
                        prev_code = Some(code);
                        if next_code >= (1 << code_size) && code_size < 12 {
                            code_size += 1;
                        }
                        continue;
                    }
                    Some(&entry) => entry,
                };

                let first = arena[offset as usize];
                result.extend_from_slice(&arena[offset as usize..offset as usize + len as usize]);

                if next_code < 4096 {
                    if let Some(&(prev_off, prev_len)) = table.get(prev as usize) {
                        if prev_len > 0 {
                            let new_offset = arena.len() as u32;
                            arena.extend_from_slice(&arena[prev_off as usize..prev_off as usize + prev_len as usize].to_vec());
                            arena.push(first);
                            table.push((new_offset, prev_len + 1));
                            next_code += 1;
                        }
                    }
                }
            } else if code == next_code {
                if let Some(&(prev_off, prev_len)) = table.get(prev as usize) {
                    if prev_len > 0 {
                        let first = arena[prev_off as usize];
                        let new_offset = arena.len() as u32;
                        arena.extend_from_slice(&arena[prev_off as usize..prev_off as usize + prev_len as usize].to_vec());
                        arena.push(first);
                        let new_len = prev_len + 1;
                        result.extend_from_slice(&arena[new_offset as usize..new_offset as usize + new_len as usize]);
                        if next_code < 4096 {
                            table.push((new_offset, new_len));
                            next_code += 1;
                        }
                    }
                }
            } else {
                log_warn!("Invalid LZW code: {}", code);
            }
        } else {
            if let Some(&(offset, len)) = table.get(code as usize) {
                result.extend_from_slice(&arena[offset as usize..offset as usize + len as usize]);
            }
        }

        prev_code = Some(code);

        if next_code >= (1 << code_size) && code_size < 12 {
            code_size += 1;
        }
    }

    Ok(result)
}
