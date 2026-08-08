use crate::decoders::exr::types::{ChunkChannel, ExrPixelType};
use crate::decoders::jpeg2000::codestream;
use crate::decoders::jpeg2000::decoder::{component_dimensions, decode_codestream_planes};
use crate::utils::error::VexelResult;
use crate::{log_warn, Limits};

const HT_MAGIC: u16 = 0x4854;
const HEADER_FIXED_SIZE: usize = 8;

struct ChunkHeader {
    codestream_offset: usize,
    channel_map: Vec<usize>,
}

fn read_chunk_header(data: &[u8], channel_count: usize) -> ChunkHeader {
    let fallback = || ChunkHeader { codestream_offset: 0, channel_map: (0..channel_count).collect() };

    if data.len() < HEADER_FIXED_SIZE {
        log_warn!("EXR: HTJ2K chunk is too short for its header, assuming a bare codestream");
        return fallback();
    }

    let magic = u16::from_be_bytes([data[0], data[1]]);
    if magic != HT_MAGIC {
        log_warn!("EXR: HTJ2K chunk is missing its 'HT' magic, assuming a bare codestream");
        return fallback();
    }

    let payload_length = u32::from_be_bytes([data[2], data[3], data[4], data[5]]) as usize;
    let entries = u16::from_be_bytes([data[6], data[7]]) as usize;

    let map_end = HEADER_FIXED_SIZE + entries * 2;
    if map_end > data.len() {
        log_warn!("EXR: HTJ2K channel map runs past the chunk, mapping channels in order");
        return fallback();
    }

    let mut channel_map = Vec::with_capacity(entries);
    for i in 0..entries {
        let o = HEADER_FIXED_SIZE + i * 2;
        let index = u16::from_be_bytes([data[o], data[o + 1]]) as usize;

        if index >= channel_count {
            log_warn!("EXR: HTJ2K channel map entry {} is out of range, mapping channels in order", index);
            return fallback();
        }

        channel_map.push(index);
    }

    let codestream_offset = HEADER_FIXED_SIZE.saturating_add(payload_length).min(data.len());

    ChunkHeader { codestream_offset, channel_map }
}

fn store_sample(out: &mut [u8], offset: usize, value: i64, pixel_type: ExrPixelType) {
    match pixel_type {
        ExrPixelType::Half => {
            let bits = (value as u64 & 0xFFFF) as u16;
            out[offset..offset + 2].copy_from_slice(&bits.to_le_bytes());
        }
        ExrPixelType::Float | ExrPixelType::Uint => {
            out[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
        }
    }
}

pub fn htj2k_decompress(
    limits: &mut Limits,
    data: &[u8],
    channels: &[ChunkChannel],
    unpacked_size: usize,
) -> VexelResult<Vec<u8>> {
    let header = read_chunk_header(data, channels.len());
    let codestream_data = &data[header.codestream_offset..];

    let parsed = codestream::parse(codestream_data)?;
    let planes = decode_codestream_planes(&parsed, codestream_data, limits)?;

    let mut out = vec![0u8; unpacked_size];
    let mut channel_starts = Vec::with_capacity(channels.len());
    let mut cursor = 0usize;
    for channel in channels {
        channel_starts.push(cursor);
        cursor += channel.width * channel.rows * channel.bpe;
    }

    if cursor != unpacked_size {
        log_warn!("EXR: HTJ2K channel layout covers {} bytes but the block expects {}", cursor, unpacked_size);
    }

    for (component, plane) in planes.iter().enumerate() {
        let Some(&channel_index) = header.channel_map.get(component) else {
            log_warn!(
                "EXR: HTJ2K codestream has more components than the channel map, ignoring component {}",
                component
            );
            continue;
        };

        let Some(channel) = channels.get(channel_index) else {
            continue;
        };

        let component_width = parsed
            .siz
            .components
            .get(component)
            .map(|c| component_dimensions(&parsed.siz, c).0)
            .unwrap_or(channel.width);

        if component_width == 0 {
            continue;
        }

        let shift = parsed
            .siz
            .components
            .get(component)
            .filter(|c| !c.signed)
            .map(|c| 1i64 << (c.prec.clamp(1, 32) - 1))
            .unwrap_or(0);

        let base = channel_starts[channel_index];
        let rows = channel.rows.min(plane.len() / component_width.max(1));

        for y in 0..rows {
            let src_row = y * component_width;
            let dst_row = base + y * channel.width * channel.bpe;

            let count = channel.width.min(component_width);
            for x in 0..count {
                let value = plane[src_row + x].wrapping_add(shift);
                store_sample(&mut out, dst_row + x * channel.bpe, value, channel.pixel_type);
            }
        }
    }

    if planes.len() < channels.len() {
        log_warn!(
            "EXR: HTJ2K codestream decoded {} of {} channels, the rest stay zeroed",
            planes.len(),
            channels.len()
        );
    }

    Ok(out)
}
