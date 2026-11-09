mod classifier;
mod dct;
mod tables;

use classifier::{channel_prefix, channel_suffix, Rule, Scheme};
use dct::{csc709_inverse, csc709_inverse_64, inverse_8x8, inverse_8x8_dc_only, zeroed_rows_for};
use tables::{to_linear_table, ZIGZAG_TO_RASTER};

use crate::decoders::exr::compression::f32_to_half;
use crate::decoders::exr::huffman::huf_decompress;
use crate::decoders::exr::types::{ChunkChannel, ExrPixelType};
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::tiff::color::half_to_f32;
use crate::{log_warn, Limits};

const NUM_SIZES_SINGLE: usize = 11;

const VERSION: usize = 0;
const UNKNOWN_UNCOMPRESSED_SIZE: usize = 1;
const UNKNOWN_COMPRESSED_SIZE: usize = 2;
const AC_COMPRESSED_SIZE: usize = 3;
const DC_COMPRESSED_SIZE: usize = 4;
const RLE_COMPRESSED_SIZE: usize = 5;
const RLE_UNCOMPRESSED_SIZE: usize = 6;
const RLE_RAW_SIZE: usize = 7;
const AC_UNCOMPRESSED_COUNT: usize = 8;
const DC_UNCOMPRESSED_COUNT: usize = 9;
const AC_COMPRESSION: usize = 10;

const AC_COMPRESSION_STATIC_HUFFMAN: u64 = 0;
const AC_COMPRESSION_DEFLATE: u64 = 1;

struct Header {
    version: u64,
    unknown_uncompressed_size: usize,
    unknown_compressed_size: usize,
    ac_compressed_size: usize,
    dc_compressed_size: usize,
    rle_compressed_size: usize,
    rle_uncompressed_size: usize,
    rle_raw_size: usize,
    ac_uncompressed_count: usize,
    dc_uncompressed_count: usize,
    ac_compression: u64,
}

fn read_header(data: &[u8]) -> VexelResult<Header> {
    let header_size = NUM_SIZES_SINGLE * 8;
    if data.len() < header_size {
        return Err(VexelError::Custom("EXR: DWA chunk is too short for its header".to_string()));
    }

    let mut counters = [0u64; NUM_SIZES_SINGLE];
    for (i, counter) in counters.iter_mut().enumerate() {
        let o = i * 8;
        *counter = u64::from_le_bytes(data[o..o + 8].try_into().unwrap());

        if *counter > i64::MAX as u64 {
            log_warn!("EXR: DWA header field {} is negative, treating it as zero", i);
            *counter = 0;
        }
    }

    Ok(Header {
        version: counters[VERSION],
        unknown_uncompressed_size: counters[UNKNOWN_UNCOMPRESSED_SIZE] as usize,
        unknown_compressed_size: counters[UNKNOWN_COMPRESSED_SIZE] as usize,
        ac_compressed_size: counters[AC_COMPRESSED_SIZE] as usize,
        dc_compressed_size: counters[DC_COMPRESSED_SIZE] as usize,
        rle_compressed_size: counters[RLE_COMPRESSED_SIZE] as usize,
        rle_uncompressed_size: counters[RLE_UNCOMPRESSED_SIZE] as usize,
        rle_raw_size: counters[RLE_RAW_SIZE] as usize,
        ac_uncompressed_count: counters[AC_UNCOMPRESSED_COUNT] as usize,
        dc_uncompressed_count: counters[DC_UNCOMPRESSED_COUNT] as usize,
        ac_compression: counters[AC_COMPRESSION],
    })
}

fn clamp_section_size(value: &mut usize, cap: usize, what: &str) {
    if *value > cap {
        log_warn!("EXR: DWA {} {} exceeds what the block can hold, clamping to {}", what, value, cap);
        *value = cap;
    }
}

fn clamp_section_sizes(header: &mut Header, unpacked_size: usize) {
    let rle_packed_cap = unpacked_size.saturating_add(unpacked_size / 64).saturating_add(64);
    clamp_section_size(&mut header.unknown_uncompressed_size, unpacked_size, "unknown data size");
    clamp_section_size(&mut header.rle_raw_size, unpacked_size, "RLE raw size");
    clamp_section_size(&mut header.rle_uncompressed_size, rle_packed_cap, "RLE packed size");
    clamp_section_size(&mut header.ac_uncompressed_count, unpacked_size, "AC count");
    clamp_section_size(&mut header.dc_uncompressed_count, unpacked_size, "DC count");
}

struct Channel {
    width: usize,
    height: usize,
    bpe: usize,
    pixel_type: ExrPixelType,
    p_linear: bool,
    scheme: Scheme,
    row_offsets: Vec<usize>,
}

struct CscSet {
    r: usize,
    g: usize,
    b: usize,
}

fn classify(channels: &[ChunkChannel], names: &[String], rules: &[Rule]) -> (Vec<Scheme>, Vec<CscSet>) {
    let mut schemes = vec![Scheme::Unknown; channels.len()];
    let mut prefixes: Vec<(&str, [Option<usize>; 3])> = Vec::new();

    for (c, channel) in channels.iter().enumerate() {
        let name = names[c].as_str();
        let suffix = channel_suffix(name);
        let prefix = channel_prefix(name);

        let slot = match prefixes.iter().position(|(p, _)| *p == prefix) {
            Some(index) => index,
            None => {
                prefixes.push((prefix, [None; 3]));
                prefixes.len() - 1
            }
        };

        for rule in rules {
            if rule.matches(suffix, channel.pixel_type) {
                schemes[c] = rule.scheme();
                if let Some(csc) = rule.csc_index() {
                    prefixes[slot].1[csc] = Some(c);
                }
            }
        }
    }

    let mut csc_sets = Vec::new();
    for (_, idx) in &prefixes {
        let (Some(r), Some(g), Some(b)) = (idx[0], idx[1], idx[2]) else {
            continue;
        };

        if schemes[r] != Scheme::LossyDct || schemes[g] != Scheme::LossyDct || schemes[b] != Scheme::LossyDct {
            continue;
        }

        let same_shape =
            |other: usize| channels[other].width == channels[r].width && channels[other].rows == channels[r].rows;
        if !same_shape(g) || !same_shape(b) {
            continue;
        }

        csc_sets.push(CscSet { r, g, b });
    }

    (schemes, csc_sets)
}

fn rle_decompress(src: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut pos = 0usize;

    while pos < src.len() && out.len() < expected {
        let control = src[pos] as i8;
        pos += 1;

        if control < 0 {
            let count = (-(control as i32)) as usize;
            let count = count.min(src.len() - pos).min(expected - out.len());
            out.extend_from_slice(&src[pos..pos + count]);
            pos += count;
        } else {
            if pos >= src.len() {
                break;
            }
            let count = (control as usize + 1).min(expected - out.len());
            out.resize(out.len() + count, src[pos]);
            pos += 1;
        }
    }

    if out.len() < expected {
        log_warn!("EXR: DWA RLE produced {} of {} expected bytes, padding with zeros", out.len(), expected);
        out.resize(expected, 0);
    }

    out
}

fn inflate_padded(data: &[u8], expected: usize, what: &str) -> Vec<u8> {
    let mut inflated = ZlibDecoder::from_slice(data).decode_with_max(expected);
    if inflated.len() < expected {
        log_warn!("EXR: DWA {} inflated to {} of {} expected bytes, padding", what, inflated.len(), expected);
        inflated.resize(expected, 0);
    }
    inflated.truncate(expected);
    inflated
}

fn unpredict_dc(data: &mut [u8]) {
    for i in 1..data.len() {
        let d = data[i - 1] as i32 + data[i] as i32 - 128;
        data[i] = d as u8;
    }
}

fn deinterleave_dc(data: &[u8]) -> Vec<u16> {
    let half = data.len().div_ceil(2);
    let (first, second) = data.split_at(half);

    let mut out = Vec::with_capacity(data.len() / 2);
    for i in 0..data.len() / 2 {
        out.push(u16::from_le_bytes([first[i], second[i]]));
    }
    out
}

struct AcReader<'a> {
    data: &'a [u16],
    pos: usize,
    exhausted: bool,
}

impl<'a> AcReader<'a> {
    fn un_rle(&mut self, block: &mut [u16; 64]) -> usize {
        let mut dct_comp = 1usize;
        let mut last_non_zero = 0usize;

        while dct_comp < 64 {
            let Some(&value) = self.data.get(self.pos) else {
                if !self.exhausted {
                    log_warn!("EXR: DWA AC data ran out, leaving the remaining coefficients zeroed");
                    self.exhausted = true;
                }
                break;
            };
            self.pos += 1;

            if value & 0xFF00 == 0xFF00 {
                let count = (value & 0xFF) as usize;
                dct_comp += if count == 0 { 64 } else { count };
            } else {
                last_non_zero = dct_comp;
                block[dct_comp] = value;
                dct_comp += 1;
            }
        }

        last_non_zero
    }
}

fn from_half_zigzag(block: &[u16; 64], out: &mut [f32; 64]) {
    for (raster, &zig) in ZIGZAG_TO_RASTER.iter().enumerate() {
        out[raster] = half_to_f32(block[zig]);
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_lossy_dct(
    channels: &mut [Channel],
    indices: &[usize],
    ac: &mut AcReader,
    dc: &[u16],
    dc_offset: &mut usize,
    to_linear: Option<&[u16; 65536]>,
    rows: &mut [u8],
    width: usize,
    height: usize,
) -> VexelResult<()> {
    let num_comp = indices.len();
    let blocks_x = width.div_ceil(8);
    let blocks_y = height.div_ceil(8);

    if blocks_x == 0 || blocks_y == 0 {
        return Ok(());
    }

    let needed = num_comp * blocks_x * blocks_y;
    if dc.len() < *dc_offset + needed {
        log_warn!(
            "EXR: DWA DC data holds {} of {} values needed, missing blocks decode as zero",
            dc.len().saturating_sub(*dc_offset),
            needed
        );
    }

    let dc_base = *dc_offset;
    let dc_plane = blocks_x * blocks_y;
    let mut dc_cursor = [0usize; 3];
    for (comp, cursor) in dc_cursor.iter_mut().enumerate().take(num_comp) {
        *cursor = dc_base + comp * dc_plane;
    }

    let leftover_y = height - (blocks_y - 1) * 8;

    let mut row_block = vec![0u16; num_comp * blocks_x * 64];
    let mut dct_data = [[0f32; 64]; 3];
    let mut half_zig = [[0u16; 64]; 3];

    for blocky in 0..blocks_y {
        let max_y = if blocky == blocks_y - 1 { leftover_y } else { 8 };

        for blockx in 0..blocks_x {
            let mut block_is_constant = true;

            for comp in 0..num_comp {
                let zig = &mut half_zig[comp];
                zig.fill(0);
                zig[0] = dc.get(dc_cursor[comp]).copied().unwrap_or(0);
                dc_cursor[comp] += 1;

                let last_non_zero = ac.un_rle(zig);

                if last_non_zero == 0 {
                    dct_data[comp][0] = half_to_f32(zig[0]);
                    inverse_8x8_dc_only(&mut dct_data[comp]);
                } else {
                    block_is_constant = false;
                    from_half_zigzag(zig, &mut dct_data[comp]);
                    inverse_8x8(&mut dct_data[comp], zeroed_rows_for(last_non_zero));
                }
            }

            if num_comp == 3 {
                let (head, tail) = dct_data.split_at_mut(1);
                let (mid, last) = tail.split_at_mut(1);
                if block_is_constant {
                    let (r, g, b) = csc709_inverse(head[0][0], mid[0][0], last[0][0]);
                    head[0][0] = r;
                    mid[0][0] = g;
                    last[0][0] = b;
                } else {
                    csc709_inverse_64(&mut head[0], &mut mid[0], &mut last[0]);
                }
            }

            for comp in 0..num_comp {
                let dst = &mut row_block[comp * blocks_x * 64 + blockx * 64..][..64];
                if block_is_constant {
                    dst.fill(f32_to_half(dct_data[comp][0]));
                } else {
                    for (d, &s) in dst.iter_mut().zip(dct_data[comp].iter()) {
                        *d = f32_to_half(s);
                    }
                }
            }
        }

        for (comp, &channel_index) in indices.iter().enumerate() {
            let bpe = channels[channel_index].bpe;

            for y in 8 * blocky..8 * blocky + max_y {
                let row_start = channels[channel_index].row_offsets[y];
                let src_row = (y & 7) * 8;

                for blockx in 0..blocks_x {
                    let count = if blockx == blocks_x - 1 { width - blockx * 8 } else { 8 };
                    let src = &row_block[comp * blocks_x * 64 + blockx * 64 + src_row..][..count];
                    let dst_start = row_start + blockx * 8 * bpe;

                    for (i, &value) in src.iter().enumerate() {
                        let value = match to_linear {
                            Some(lut) => lut[value as usize],
                            None => value,
                        };
                        let o = dst_start + i * bpe;
                        rows[o..o + 2].copy_from_slice(&value.to_le_bytes());
                    }
                }
            }
        }
    }

    *dc_offset = dc_base + num_comp * dc_plane;

    for &channel_index in indices {
        if channels[channel_index].pixel_type != ExrPixelType::Float {
            continue;
        }

        for y in 0..height {
            let row_start = channels[channel_index].row_offsets[y];
            for x in (0..width).rev() {
                let src = row_start + x * 2;
                let bits = u16::from_le_bytes([rows[src], rows[src + 1]]);
                let value = half_to_f32(bits);
                let dst = row_start + x * 4;
                rows[dst..dst + 4].copy_from_slice(&value.to_bits().to_le_bytes());
            }
        }
    }

    Ok(())
}

pub fn dwa_decompress(
    limits: &mut Limits,
    data: &[u8],
    channels: &[ChunkChannel],
    names: &[String],
    unpacked_size: usize,
) -> VexelResult<Vec<u8>> {
    let mut header = read_header(data)?;
    clamp_section_sizes(&mut header, unpacked_size);

    if header.version > 2 {
        log_warn!("EXR: DWA version {} is newer than 2, decoding as version 2", header.version);
    }

    let mut offset = NUM_SIZES_SINGLE * 8;

    let rules = if header.version < 2 {
        classifier::legacy_rules()
    } else {
        match classifier::read_rules(&data[offset..]) {
            Ok((rules, consumed)) => {
                offset += consumed;
                if rules.is_empty() {
                    log_warn!("EXR: DWA block carries no channel rules, falling back to the defaults");
                    classifier::default_rules()
                } else {
                    rules
                }
            }
            Err(e) => {
                log_warn!("EXR: could not read DWA channel rules ({}), falling back to the defaults", e);
                classifier::default_rules()
            }
        }
    };

    let compressed_total = header
        .unknown_compressed_size
        .saturating_add(header.ac_compressed_size)
        .saturating_add(header.dc_compressed_size)
        .saturating_add(header.rle_compressed_size);

    if data.len() < offset.saturating_add(compressed_total) {
        log_warn!(
            "EXR: DWA chunk declares {} bytes of sections but only {} remain, sections will be clamped",
            compressed_total,
            data.len().saturating_sub(offset)
        );
    }

    let unknown_start = offset.min(data.len());
    let ac_start = unknown_start.saturating_add(header.unknown_compressed_size).min(data.len());
    let dc_start = ac_start.saturating_add(header.ac_compressed_size).min(data.len());
    let rle_start = dc_start.saturating_add(header.dc_compressed_size).min(data.len());

    let (schemes, csc_sets) = classify(channels, names, &rules);

    let mut planes: Vec<Channel> = channels
        .iter()
        .zip(&schemes)
        .map(|(channel, &scheme)| Channel {
            width: channel.width,
            height: channel.rows,
            bpe: channel.bpe,
            pixel_type: channel.pixel_type,
            p_linear: channel.p_linear,
            scheme,
            row_offsets: Vec::new(),
        })
        .collect();

    let mut cursor = 0usize;
    for plane in planes.iter_mut() {
        plane.row_offsets = (0..plane.height).map(|_| 0).collect();
        for row in 0..plane.height {
            plane.row_offsets[row] = cursor + row * plane.width * plane.bpe;
        }
        cursor += plane.width * plane.height * plane.bpe;
    }

    if cursor != unpacked_size {
        log_warn!("EXR: DWA channel layout covers {} bytes but the block expects {}", cursor, unpacked_size);
    }

    let total = data.len();
    let clamp = |start: usize, len: usize| (start, len.min(total.saturating_sub(start)));

    limits.reserve_usize(unpacked_size)?;
    let result =
        decode_sections(limits, data, &header, &mut planes, &csc_sets, unpacked_size, |section| match section {
            Section::Unknown => clamp(unknown_start, header.unknown_compressed_size),
            Section::Ac => clamp(ac_start, header.ac_compressed_size),
            Section::Dc => clamp(dc_start, header.dc_compressed_size),
            Section::Rle => clamp(rle_start, header.rle_compressed_size),
        });
    limits.free_usize(unpacked_size);

    result
}

enum Section {
    Unknown,
    Ac,
    Dc,
    Rle,
}

fn decode_sections(
    limits: &mut Limits,
    data: &[u8],
    header: &Header,
    planes: &mut [Channel],
    csc_sets: &[CscSet],
    unpacked_size: usize,
    section: impl Fn(Section) -> (usize, usize),
) -> VexelResult<Vec<u8>> {
    let mut out = vec![0u8; unpacked_size];

    let unknown = {
        let (start, len) = section(Section::Unknown);
        if len > 0 {
            limits.reserve_usize(header.unknown_uncompressed_size)?;
            let result = inflate_padded(&data[start..start + len], header.unknown_uncompressed_size, "unknown data");
            limits.free_usize(header.unknown_uncompressed_size);
            result
        } else {
            Vec::new()
        }
    };

    let ac = {
        let (start, len) = section(Section::Ac);
        let count = header.ac_uncompressed_count;
        if len > 0 {
            limits.reserve_usize(count * 2)?;
            let slice = &data[start..start + len];
            let mut values = match header.ac_compression {
                AC_COMPRESSION_DEFLATE => inflate_padded(slice, count * 2, "AC data")
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect(),
                other => {
                    if other != AC_COMPRESSION_STATIC_HUFFMAN {
                        log_warn!("EXR: DWA has unknown AC compression {}, assuming static huffman", other);
                    }
                    huf_decompress(slice, count).unwrap_or_else(|e| {
                        log_warn!("EXR: DWA AC huffman decode failed ({}), leaving coefficients zeroed", e);
                        Vec::new()
                    })
                }
            };
            values.resize(count, 0);
            limits.free_usize(count * 2);
            values
        } else {
            Vec::new()
        }
    };

    let dc = {
        let (start, len) = section(Section::Dc);
        let count = header.dc_uncompressed_count;
        if len > 0 {
            limits.reserve_usize(count * 2)?;
            let mut bytes = inflate_padded(&data[start..start + len], count * 2, "DC data");
            unpredict_dc(&mut bytes);
            let values = deinterleave_dc(&bytes);
            limits.free_usize(count * 2);
            values
        } else {
            if count != 0 {
                log_warn!("EXR: DWA declares {} DC values with no DC data, blocks decode as zero", count);
            }
            Vec::new()
        }
    };

    let rle = {
        let (start, len) = section(Section::Rle);
        if header.rle_raw_size > 0 {
            limits.reserve_usize(header.rle_uncompressed_size + header.rle_raw_size)?;
            let packed = inflate_padded(&data[start..start + len], header.rle_uncompressed_size, "RLE data");
            let values = rle_decompress(&packed, header.rle_raw_size);
            limits.free_usize(header.rle_uncompressed_size + header.rle_raw_size);
            values
        } else {
            Vec::new()
        }
    };

    let mut ac_reader = AcReader { data: &ac, pos: 0, exhausted: false };
    let mut dc_offset = 0usize;
    let to_linear = to_linear_table();
    let mut processed = vec![false; planes.len()];

    for set in csc_sets {
        let indices = [set.r, set.g, set.b];
        let width = planes[set.r].width;
        let height = planes[set.r].height;

        decode_lossy_dct(
            planes,
            &indices,
            &mut ac_reader,
            &dc,
            &mut dc_offset,
            Some(to_linear),
            &mut out,
            width,
            height,
        )?;

        for index in indices {
            processed[index] = true;
        }
    }

    let mut unknown_cursor = 0usize;
    let mut rle_cursor = 0usize;

    for index in 0..planes.len() {
        if processed[index] {
            continue;
        }

        let width = planes[index].width;
        let height = planes[index].height;
        let bpe = planes[index].bpe;

        if width == 0 || height == 0 {
            processed[index] = true;
            continue;
        }

        match planes[index].scheme {
            Scheme::LossyDct => {
                let to_linear = if planes[index].p_linear { None } else { Some(to_linear) };
                decode_lossy_dct(
                    planes,
                    &[index],
                    &mut ac_reader,
                    &dc,
                    &mut dc_offset,
                    to_linear,
                    &mut out,
                    width,
                    height,
                )?;
            }
            Scheme::Rle => {
                let plane_size = width * height;
                if rle_cursor + plane_size * bpe > rle.len() {
                    log_warn!("EXR: DWA RLE data is short for channel {}, decoding what is present", index);
                }

                for row in 0..height {
                    let dst_start = planes[index].row_offsets[row];
                    for x in 0..width {
                        for byte in 0..bpe {
                            let src = rle_cursor + byte * plane_size + row * width + x;
                            out[dst_start + x * bpe + byte] = rle.get(src).copied().unwrap_or(0);
                        }
                    }
                }

                rle_cursor += plane_size * bpe;
            }
            Scheme::Unknown => {
                let row_bytes = width * bpe;
                if unknown_cursor + row_bytes * height > unknown.len() {
                    log_warn!("EXR: DWA unknown data is short for channel {}, decoding what is present", index);
                }

                for row in 0..height {
                    let dst_start = planes[index].row_offsets[row];
                    let source = unknown.get(unknown_cursor..).unwrap_or(&[]);
                    let available = source.len().min(row_bytes);
                    out[dst_start..dst_start + available].copy_from_slice(&source[..available]);
                    unknown_cursor += row_bytes;
                }
            }
        }

        processed[index] = true;
    }

    Ok(out)
}
