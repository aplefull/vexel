use crate::decoders::exr::compression::{rle_decompress, unpredict_and_reorder};
use crate::decoders::exr::parser::ByteCursor;
use crate::decoders::exr::types::{ExrCompression, ExrPixelType, PartHeader};
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::tiff::color::half_to_f32;
use crate::{log_warn, Limits};

pub struct DeepChunkPixels {
    pub x0: i64,
    pub y0: i64,
    pub width: usize,
    pub rows: usize,
    pub channels: Vec<Vec<f32>>,
}

struct DeepRegion {
    x0: i64,
    y0: i64,
    width: usize,
    rows: usize,
}

pub fn decode_deep_chunk<'a>(
    limits: &mut Limits,
    header: &PartHeader,
    cursor: &mut ByteCursor<'a>,
    alpha_index: Option<usize>,
    z_index: Option<usize>,
) -> VexelResult<Option<DeepChunkPixels>> {
    let region = match read_deep_region(header, cursor)? {
        Some(r) => r,
        None => return Ok(None),
    };

    let table_size = cursor.read_u64()?;
    let packed_size = cursor.read_u64()?;
    let unpacked_declared = cursor.read_u64()?;

    if table_size as usize > cursor.remaining() {
        return Err(VexelError::Custom(format!("EXR: deep chunk sample count table size {} is invalid", table_size)));
    }
    let table_data = cursor.read_bytes(table_size as usize)?;

    if packed_size as usize > cursor.remaining() {
        return Err(VexelError::Custom(format!("EXR: deep chunk sample data size {} is invalid", packed_size)));
    }
    let sample_data = cursor.read_bytes(packed_size as usize)?;

    let pixel_count = region.width * region.rows;
    let table_bytes = pixel_count * 4;
    limits.reserve_usize(table_bytes)?;
    let table = decompress_deep_blob(header.compression, table_data, table_bytes);
    limits.free_usize(table_bytes);
    let table = table?;

    let (counts, starts, row_totals) = parse_sample_counts(&table, region.width, region.rows);

    let sample_stride: usize = header.channels.iter().map(|c| c.pixel_type.bytes_per_element()).sum();
    let total_samples: u64 = row_totals.iter().sum();
    let unpacked_size = (total_samples as usize)
        .checked_mul(sample_stride)
        .ok_or_else(|| VexelError::Custom("EXR: deep chunk sample data size overflow".to_string()))?;

    if unpacked_declared as usize != unpacked_size {
        log_warn!(
            "EXR: deep chunk declares {} unpacked bytes but sample counts require {}, using computed size",
            unpacked_declared,
            unpacked_size
        );
    }

    let out_bytes = pixel_count * 4 * header.channels.len();
    limits.reserve_usize(unpacked_size + out_bytes)?;
    let result = flatten_deep_chunk(
        header,
        sample_data,
        unpacked_size,
        &region,
        &counts,
        &starts,
        &row_totals,
        alpha_index,
        z_index,
    );
    limits.free_usize(unpacked_size + out_bytes);

    result.map(Some)
}

fn read_deep_region(header: &PartHeader, cursor: &mut ByteCursor) -> VexelResult<Option<DeepRegion>> {
    let dw = header.data_window;

    if header.kind.is_tiled() {
        let tile_x = cursor.read_i32()? as i64;
        let tile_y = cursor.read_i32()? as i64;
        let level_x = cursor.read_i32()?;
        let level_y = cursor.read_i32()?;

        if level_x != 0 || level_y != 0 {
            return Ok(None);
        }

        let tiles = match &header.tiles {
            Some(t) => t,
            None => return Ok(None),
        };

        let tsx = tiles.x_size as i64;
        let tsy = tiles.y_size as i64;
        let tiles_x = (dw.width() + tsx - 1) / tsx;
        let tiles_y = (dw.height() + tsy - 1) / tsy;

        if tile_x < 0 || tile_x >= tiles_x || tile_y < 0 || tile_y >= tiles_y {
            log_warn!("EXR: deep tile coordinates ({}, {}) are out of range, skipping", tile_x, tile_y);
            return Ok(None);
        }

        let x0 = dw.x_min as i64 + tile_x * tsx;
        let y0 = dw.y_min as i64 + tile_y * tsy;
        let x1 = (x0 + tsx - 1).min(dw.x_max as i64);
        let y1 = (y0 + tsy - 1).min(dw.y_max as i64);

        Ok(Some(DeepRegion { x0, y0, width: (x1 - x0 + 1) as usize, rows: (y1 - y0 + 1) as usize }))
    } else {
        let y0 = cursor.read_i32()? as i64;
        if y0 < dw.y_min as i64 || y0 > dw.y_max as i64 {
            log_warn!("EXR: deep scanline chunk y {} is outside the data window, skipping", y0);
            return Ok(None);
        }

        let lines = header.compression.lines_per_block() as i64;
        let y1 = (y0 + lines - 1).min(dw.y_max as i64);

        Ok(Some(DeepRegion { x0: dw.x_min as i64, y0, width: dw.width() as usize, rows: (y1 - y0 + 1) as usize }))
    }
}

fn decompress_deep_blob(compression: ExrCompression, data: &[u8], unpacked: usize) -> VexelResult<Vec<u8>> {
    if data.len() == unpacked {
        return Ok(data.to_vec());
    }

    if data.len() > unpacked {
        log_warn!(
            "EXR: deep chunk blob is larger than its uncompressed size ({} > {}), treating as uncompressed",
            data.len(),
            unpacked
        );
        return Ok(data[..unpacked].to_vec());
    }

    match compression {
        ExrCompression::None => {
            log_warn!("EXR: uncompressed deep blob is truncated ({} of {} bytes), padding", data.len(), unpacked);
            let mut out = data.to_vec();
            out.resize(unpacked, 0);
            Ok(out)
        }
        ExrCompression::Rle => {
            let decoded = rle_decompress(data, unpacked)?;
            Ok(unpredict_and_reorder(decoded))
        }
        ExrCompression::Zips | ExrCompression::Zip => {
            let mut inflated = ZlibDecoder::from_slice(data).decode_with_max(unpacked);
            if inflated.len() < unpacked {
                log_warn!("EXR: deep zip blob inflated to {} of {} expected bytes, padding", inflated.len(), unpacked);
                inflated.resize(unpacked, 0);
            }
            Ok(unpredict_and_reorder(inflated))
        }
        other => Err(VexelError::UnsupportedFormat(format!(
            "EXR: {} compression is not supported for deep data",
            other.name()
        ))),
    }
}

fn parse_sample_counts(table: &[u8], width: usize, rows: usize) -> (Vec<u32>, Vec<u32>, Vec<u64>) {
    let mut counts = vec![0u32; width * rows];
    let mut starts = vec![0u32; width * rows];
    let mut row_totals = vec![0u64; rows];
    let mut warned = false;

    for (r, row_total) in row_totals.iter_mut().enumerate().take(rows) {
        let mut prev = 0i64;
        for x in 0..width {
            let idx = r * width + x;
            let o = idx * 4;
            let raw = if o + 4 <= table.len() {
                i32::from_le_bytes([table[o], table[o + 1], table[o + 2], table[o + 3]]) as i64
            } else {
                prev
            };

            let cumulative = if raw < prev || raw > i32::MAX as i64 {
                if !warned {
                    log_warn!("EXR: deep chunk sample counts are not monotonic, clamping");
                    warned = true;
                }
                prev
            } else {
                raw
            };

            starts[idx] = prev as u32;
            counts[idx] = (cumulative - prev) as u32;
            prev = cumulative;
        }
        *row_total = prev as u64;
    }

    (counts, starts, row_totals)
}

fn read_sample(data: &[u8], base: usize, index: usize, pixel_type: ExrPixelType) -> f32 {
    let bpe = pixel_type.bytes_per_element();
    let o = base + index * bpe;
    if o + bpe > data.len() {
        return 0.0;
    }

    match pixel_type {
        ExrPixelType::Half => half_to_f32(u16::from_le_bytes([data[o], data[o + 1]])),
        ExrPixelType::Float => f32::from_bits(u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])),
        ExrPixelType::Uint => u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as f32,
    }
}

fn flatten_deep_chunk(
    header: &PartHeader,
    packed: &[u8],
    unpacked_size: usize,
    region: &DeepRegion,
    counts: &[u32],
    starts: &[u32],
    row_totals: &[u64],
    alpha_index: Option<usize>,
    z_index: Option<usize>,
) -> VexelResult<DeepChunkPixels> {
    let data = decompress_deep_blob(header.compression, packed, unpacked_size)?;

    let nch = header.channels.len();
    let pixel_count = region.width * region.rows;
    let mut channels = vec![vec![0f32; pixel_count]; nch];

    let mut order: Vec<usize> = Vec::new();
    let mut z_values: Vec<f32> = Vec::new();
    let mut alpha_values: Vec<f32> = Vec::new();

    let mut row_start = 0usize;
    for (r, &row_total) in row_totals.iter().enumerate().take(region.rows) {
        let row_samples = row_total as usize;

        let mut channel_bases = Vec::with_capacity(nch);
        let mut off = row_start;
        for channel in &header.channels {
            channel_bases.push(off);
            off += row_samples * channel.pixel_type.bytes_per_element();
        }
        row_start = off;

        for x in 0..region.width {
            let idx = r * region.width + x;
            let n = counts[idx] as usize;
            if n == 0 {
                continue;
            }
            let first = starts[idx] as usize;

            order.clear();
            order.extend(0..n);

            if let Some(zi) = z_index {
                let channel = &header.channels[zi];
                let base = channel_bases[zi] + first * channel.pixel_type.bytes_per_element();
                z_values.clear();
                z_values.extend((0..n).map(|s| read_sample(&data, base, s, channel.pixel_type)));
                order.sort_unstable_by(|&a, &b| z_values[a].total_cmp(&z_values[b]));
            }

            match alpha_index {
                Some(ai) => {
                    let channel = &header.channels[ai];
                    let base = channel_bases[ai] + first * channel.pixel_type.bytes_per_element();
                    alpha_values.clear();
                    alpha_values
                        .extend((0..n).map(|s| read_sample(&data, base, s, channel.pixel_type).clamp(0.0, 1.0)));

                    for (c, channel) in header.channels.iter().enumerate() {
                        let base = channel_bases[c] + first * channel.pixel_type.bytes_per_element();
                        let mut acc = 0f32;
                        let mut out = 0f32;

                        if c == ai {
                            for &s in &order {
                                acc += (1.0 - acc) * alpha_values[s];
                            }
                            out = acc;
                        } else if Some(c) == z_index {
                            out = z_values[order[0]];
                        } else {
                            for &s in &order {
                                let w = 1.0 - acc;
                                out += w * read_sample(&data, base, s, channel.pixel_type);
                                acc += w * alpha_values[s];
                            }
                        }

                        channels[c][idx] = out;
                    }
                }
                None => {
                    let front = order[0];
                    for (c, channel) in header.channels.iter().enumerate() {
                        let base = channel_bases[c] + first * channel.pixel_type.bytes_per_element();
                        channels[c][idx] = read_sample(&data, base, front, channel.pixel_type);
                    }
                }
            }
        }
    }

    Ok(DeepChunkPixels { x0: region.x0, y0: region.y0, width: region.width, rows: region.rows, channels })
}
