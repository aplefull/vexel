pub const FILTER_X86: u64 = 0x04;
pub const FILTER_POWERPC: u64 = 0x05;
pub const FILTER_IA64: u64 = 0x06;
pub const FILTER_ARM: u64 = 0x07;
pub const FILTER_ARMTHUMB: u64 = 0x08;
pub const FILTER_SPARC: u64 = 0x09;

pub fn is_supported(id: u64) -> bool {
    matches!(id, FILTER_X86 | FILTER_POWERPC | FILTER_IA64 | FILTER_ARM | FILTER_ARMTHUMB | FILTER_SPARC)
}

pub fn decode(id: u64, data: &mut [u8], start_offset: u32) {
    match id {
        FILTER_X86 => decode_x86(data, start_offset),
        FILTER_POWERPC => decode_powerpc(data, start_offset),
        FILTER_IA64 => decode_ia64(data, start_offset),
        FILTER_ARM => decode_arm(data, start_offset),
        FILTER_ARMTHUMB => decode_armthumb(data, start_offset),
        FILTER_SPARC => decode_sparc(data, start_offset),
        _ => {}
    }
}

fn test_x86_msbyte(byte: u8) -> bool {
    byte == 0x00 || byte == 0xff
}

fn decode_x86(data: &mut [u8], start_offset: u32) {
    const MASK_TO_ALLOWED_STATUS: [bool; 8] = [true, true, true, false, true, false, false, false];
    const MASK_TO_BIT_NUMBER: [u32; 8] = [0, 1, 2, 2, 3, 3, 3, 3];

    if data.len() < 5 {
        return;
    }

    let mut prev_mask = 0u32;
    let mut prev_pos = start_offset.wrapping_sub(5);
    let limit = data.len() - 5;
    let mut pos = 0usize;

    while pos <= limit {
        let byte = data[pos];
        if byte != 0xe8 && byte != 0xe9 {
            pos += 1;
            continue;
        }

        let current = start_offset.wrapping_add(pos as u32);
        let offset = current.wrapping_sub(prev_pos);
        prev_pos = current;

        if offset > 5 {
            prev_mask = 0;
        } else {
            for _ in 0..offset {
                prev_mask &= 0x77;
                prev_mask <<= 1;
            }
        }

        let high = data[pos + 4];

        if test_x86_msbyte(high) && MASK_TO_ALLOWED_STATUS[((prev_mask >> 1) & 0x7) as usize] && (prev_mask >> 1) < 0x10
        {
            let mut src = (u32::from(high) << 24)
                | (u32::from(data[pos + 3]) << 16)
                | (u32::from(data[pos + 2]) << 8)
                | u32::from(data[pos + 1]);

            let dest = loop {
                let dest = src.wrapping_sub(current.wrapping_add(5));

                if prev_mask == 0 {
                    break dest;
                }

                let index = MASK_TO_BIT_NUMBER[(prev_mask >> 1) as usize];
                let byte = (dest >> (24 - index * 8)) as u8;

                if !test_x86_msbyte(byte) {
                    break dest;
                }

                src = dest ^ ((1u32 << (32 - index * 8)).wrapping_sub(1));
            };

            data[pos + 4] = !(((dest >> 24) & 1).wrapping_sub(1)) as u8;
            data[pos + 3] = (dest >> 16) as u8;
            data[pos + 2] = (dest >> 8) as u8;
            data[pos + 1] = dest as u8;
            pos += 5;
            prev_mask = 0;
        } else {
            pos += 1;
            prev_mask |= 1;
            if test_x86_msbyte(high) {
                prev_mask |= 0x10;
            }
        }
    }
}

fn decode_powerpc(data: &mut [u8], start_offset: u32) {
    for (index, chunk) in data.chunks_exact_mut(4).enumerate() {
        if chunk[0] >> 2 != 0x12 || chunk[3] & 3 != 1 {
            continue;
        }

        let src = (u32::from(chunk[0] & 3) << 24)
            | (u32::from(chunk[1]) << 16)
            | (u32::from(chunk[2]) << 8)
            | u32::from(chunk[3] & !3);

        let dest = src.wrapping_sub(start_offset.wrapping_add(index as u32 * 4));

        chunk[0] = 0x48 | ((dest >> 24) & 0x03) as u8;
        chunk[1] = (dest >> 16) as u8;
        chunk[2] = (dest >> 8) as u8;
        chunk[3] = (chunk[3] & 0x03) | dest as u8;
    }
}

fn decode_arm(data: &mut [u8], start_offset: u32) {
    for (index, chunk) in data.chunks_exact_mut(4).enumerate() {
        if chunk[3] != 0xeb {
            continue;
        }

        let src = ((u32::from(chunk[2]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[0])) << 2;
        let dest = src.wrapping_sub(start_offset.wrapping_add(index as u32 * 4).wrapping_add(8)) >> 2;

        chunk[2] = (dest >> 16) as u8;
        chunk[1] = (dest >> 8) as u8;
        chunk[0] = dest as u8;
    }
}

fn decode_armthumb(data: &mut [u8], start_offset: u32) {
    if data.len() < 4 {
        return;
    }

    let mut pos = 0usize;

    while pos + 4 <= data.len() {
        if data[pos + 1] & 0xf8 != 0xf0 || data[pos + 3] & 0xf8 != 0xf8 {
            pos += 2;
            continue;
        }

        let src = ((u32::from(data[pos + 1] & 0x7) << 19)
            | (u32::from(data[pos]) << 11)
            | (u32::from(data[pos + 3] & 0x7) << 8)
            | u32::from(data[pos + 2]))
            << 1;

        let dest = src.wrapping_sub(start_offset.wrapping_add(pos as u32).wrapping_add(4)) >> 1;

        data[pos + 1] = 0xf0 | ((dest >> 19) & 0x7) as u8;
        data[pos] = (dest >> 11) as u8;
        data[pos + 3] = 0xf8 | ((dest >> 8) & 0x7) as u8;
        data[pos + 2] = dest as u8;
        pos += 4;
    }
}

fn decode_sparc(data: &mut [u8], start_offset: u32) {
    for (index, chunk) in data.chunks_exact_mut(4).enumerate() {
        let is_branch = (chunk[0] == 0x40 && chunk[1] & 0xc0 == 0x00) || (chunk[0] == 0x7f && chunk[1] & 0xc0 == 0xc0);

        if !is_branch {
            continue;
        }

        let src = ((u32::from(chunk[0]) << 24)
            | (u32::from(chunk[1]) << 16)
            | (u32::from(chunk[2]) << 8)
            | u32::from(chunk[3]))
            << 2;

        let dest = src.wrapping_sub(start_offset.wrapping_add(index as u32 * 4)) >> 2;
        let dest = ((0u32.wrapping_sub((dest >> 22) & 1) << 22) & 0x3fff_ffff) | (dest & 0x3f_ffff) | 0x4000_0000;

        chunk[0] = (dest >> 24) as u8;
        chunk[1] = (dest >> 16) as u8;
        chunk[2] = (dest >> 8) as u8;
        chunk[3] = dest as u8;
    }
}

const IA64_BRANCH_TABLE: [u32; 32] =
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 6, 6, 0, 0, 7, 7, 4, 4, 0, 0, 4, 4, 0, 0];

fn decode_ia64(data: &mut [u8], start_offset: u32) {
    for (index, chunk) in data.chunks_exact_mut(16).enumerate() {
        let template = (chunk[0] & 0x1f) as usize;
        let mask = IA64_BRANCH_TABLE[template];

        for slot in 0..3 {
            if (mask >> slot) & 1 == 0 {
                continue;
            }

            let bit_pos = 5 + slot * 41;
            let byte_pos = bit_pos >> 3;
            let bit_res = bit_pos & 7;

            let mut instruction = 0u64;
            for (offset, byte) in chunk[byte_pos..6 + byte_pos].iter().enumerate() {
                instruction |= u64::from(*byte) << (8 * offset);
            }

            let norm = instruction >> bit_res;

            if (norm >> 37) & 0xf != 5 || (norm >> 9) & 0x7 != 0 {
                continue;
            }

            let mut src = (((norm >> 13) & 0xf_ffff) | (((norm >> 36) & 1) << 20)) as u32;
            src <<= 4;

            let dest = src.wrapping_sub(start_offset.wrapping_add(index as u32 * 16)) >> 4;

            let mut norm = norm;
            norm &= !(0x8f_ffff << 13);
            norm |= u64::from(dest & 0xf_ffff) << 13;
            norm |= u64::from((dest >> 20) & 1) << 36;

            let mut instruction = instruction & ((1u64 << bit_res) - 1);
            instruction |= norm << bit_res;

            for offset in 0..6 {
                chunk[byte_pos + offset] = (instruction >> (8 * offset)) as u8;
            }
        }
    }
}
