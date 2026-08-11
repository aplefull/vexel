use super::bitstuffer::decode_bit_stuffed;
use super::float_lossless::decode_huffman_float;
use super::huffman::{Huffman, HuffmanBitStream};
use super::mask::{rle_decompress, BitMask};
use super::reader::ByteReader;
use super::types::{ImageEncodeMode, Lerc2Header, LercDataType, CURRENT_VERSION, LERC2_MAGIC, MAX_MICRO_BLOCK_SIZE};
use super::values::{read_value, ValueBuffer};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

pub struct DecodedBand {
    pub values: Vec<f64>,
    pub mask: BitMask,
    pub header: Lerc2Header,
}

fn compute_checksum_fletcher32(data: &[u8]) -> u32 {
    let mut sum1: u32 = 0xffff;
    let mut sum2: u32 = 0xffff;
    let mut words = data.len() / 2;
    let mut pos = 0usize;

    while words > 0 {
        let tlen = words.min(359);
        words -= tlen;

        for _ in 0..tlen {
            sum1 = sum1.wrapping_add((data[pos] as u32) << 8);
            pos += 1;
            sum1 = sum1.wrapping_add(data[pos] as u32);
            pos += 1;
            sum2 = sum2.wrapping_add(sum1);
        }

        sum1 = (sum1 & 0xffff) + (sum1 >> 16);
        sum2 = (sum2 & 0xffff) + (sum2 >> 16);
    }

    if data.len() & 1 != 0 {
        sum1 = sum1.wrapping_add((data[pos] as u32) << 8);
        sum2 = sum2.wrapping_add(sum1);
    }

    sum1 = (sum1 & 0xffff) + (sum1 >> 16);
    sum2 = (sum2 & 0xffff) + (sum2 >> 16);

    (sum2 << 16) | sum1
}

pub fn read_header(reader: &mut ByteReader) -> VexelResult<Lerc2Header> {
    let start = reader.position();

    let key = reader.take(LERC2_MAGIC.len())?;
    if key != LERC2_MAGIC {
        return Err(VexelError::Custom("Not a Lerc2 blob".to_string()));
    }

    let mut header = Lerc2Header { version: reader.read_i32()?, ..Default::default() };

    if header.version < 0 || header.version > CURRENT_VERSION {
        return Err(VexelError::UnsupportedFormat(format!("Lerc2 version {}", header.version)));
    }

    if header.version >= 3 {
        header.checksum = reader.read_u32()?;
    }

    let n_ints = 6 + i32::from(header.version >= 4) + i32::from(header.version >= 6);
    let mut ints = Vec::with_capacity(n_ints as usize);
    for _ in 0..n_ints {
        ints.push(reader.read_i32()?);
    }

    let mut bytes = [0u8; 4];
    if header.version >= 6 {
        bytes.copy_from_slice(reader.take(4)?);
    }

    let n_dbls = 3 + if header.version >= 6 { 2 } else { 0 };
    let mut dbls = Vec::with_capacity(n_dbls);
    for _ in 0..n_dbls {
        dbls.push(reader.read_f64()?);
    }

    let mut i = 0usize;
    header.n_rows = ints[i];
    i += 1;
    header.n_cols = ints[i];
    i += 1;
    header.n_depth = if header.version >= 4 {
        let v = ints[i];
        i += 1;
        v
    } else {
        1
    };
    header.num_valid_pixel = ints[i];
    i += 1;
    header.micro_block_size = ints[i];
    i += 1;
    header.blob_size = ints[i];
    i += 1;
    let dt_code = ints[i];
    i += 1;
    header.n_blobs_more = if header.version >= 6 { ints[i] } else { 0 };

    header.pass_no_data_values = bytes[0] != 0;
    header.is_int = bytes[1] != 0;

    header.max_z_error = dbls[0];
    header.z_min = dbls[1];
    header.z_max = dbls[2];
    if header.version >= 6 {
        header.no_data_val = dbls[3];
        header.no_data_val_orig = dbls[4];
    }

    if header.n_rows <= 0 || header.n_cols <= 0 || header.n_depth <= 0 || header.blob_size <= 0 {
        return Err(VexelError::Custom(format!(
            "Invalid LERC dimensions: {}x{}x{}, blob size {}",
            header.n_cols, header.n_rows, header.n_depth, header.blob_size
        )));
    }

    header.dt = LercDataType::from_code(dt_code);
    if header.dt.is_none() {
        return Err(VexelError::Custom(format!("Invalid LERC data type {}", dt_code)));
    }

    if header.micro_block_size <= 0 || header.micro_block_size > MAX_MICRO_BLOCK_SIZE {
        log_warn!("LERC micro block size {} out of range, using 8", header.micro_block_size);
        header.micro_block_size = 8;
    }

    let num_pixel = header.n_rows as i64 * header.n_cols as i64;
    if num_pixel > i32::MAX as i64 {
        return Err(VexelError::Custom(format!("LERC image too large: {} pixels", num_pixel)));
    }

    if header.num_valid_pixel < 0 || header.num_valid_pixel as i64 > num_pixel {
        log_warn!("LERC valid pixel count {} out of range for {} pixels, clamping", header.num_valid_pixel, num_pixel);
        header.num_valid_pixel = header.num_valid_pixel.clamp(0, num_pixel as i32);
    }

    header.header_size = reader.position() - start;

    Ok(header)
}

fn read_mask(reader: &mut ByteReader, header: &Lerc2Header) -> VexelResult<BitMask> {
    let w = header.n_cols as usize;
    let h = header.n_rows as usize;
    let num_valid = header.num_valid_pixel as usize;

    let num_bytes_mask = reader.read_i32()?;

    if num_bytes_mask < 0 {
        log_warn!("LERC mask has negative byte count {}, treating as all valid", num_bytes_mask);
        return Ok(BitMask::all_valid(w, h));
    }

    if num_valid == 0 {
        reader.skip(num_bytes_mask as usize);
        return Ok(BitMask::new(w, h));
    }

    if num_valid == w * h {
        reader.skip(num_bytes_mask as usize);
        return Ok(BitMask::all_valid(w, h));
    }

    if num_bytes_mask == 0 {
        log_warn!("LERC blob has a partial mask but stores no mask data, treating all pixels as valid");
        return Ok(BitMask::all_valid(w, h));
    }

    let mut mask = BitMask::new(w, h);
    let available = reader.remaining().min(num_bytes_mask as usize);
    let src = reader.peek(available);

    if !rle_decompress(src, mask.bits_mut()) {
        log_warn!("LERC mask could not be decoded, treating all pixels as valid");
        reader.skip(available);
        return Ok(BitMask::all_valid(w, h));
    }

    reader.skip(available);

    Ok(mask)
}

fn fill_const_image(values: &mut ValueBuffer, header: &Lerc2Header, mask: &BitMask, z_mins: &[f64]) {
    let n_cols = header.n_cols as usize;
    let n_rows = header.n_rows as usize;
    let n_depth = header.n_depth as usize;

    for i in 0..n_rows {
        for j in 0..n_cols {
            let k = i * n_cols + j;

            if !mask.is_valid(k) {
                continue;
            }

            let m = k * n_depth;

            for d in 0..n_depth {
                let z = z_mins.get(d).copied().unwrap_or(header.z_min);
                values.set(m + d, z);
            }
        }
    }
}

fn read_min_max_ranges(
    reader: &mut ByteReader,
    header: &Lerc2Header,
    dt: LercDataType,
) -> VexelResult<(Vec<f64>, Vec<f64>)> {
    let n_depth = header.n_depth as usize;

    let mut z_mins = Vec::with_capacity(n_depth);
    for _ in 0..n_depth {
        z_mins.push(read_value(reader, dt)?);
    }

    let mut z_maxs = Vec::with_capacity(n_depth);
    for _ in 0..n_depth {
        z_maxs.push(read_value(reader, dt)?);
    }

    Ok((z_mins, z_maxs))
}

fn read_data_one_sweep(
    reader: &mut ByteReader,
    header: &Lerc2Header,
    mask: &BitMask,
    values: &mut ValueBuffer,
    dt: LercDataType,
) -> VexelResult<()> {
    let n_cols = header.n_cols as usize;
    let n_rows = header.n_rows as usize;
    let n_depth = header.n_depth as usize;

    for i in 0..n_rows {
        for j in 0..n_cols {
            let k = i * n_cols + j;

            if !mask.is_valid(k) {
                continue;
            }

            let m = k * n_depth;

            for d in 0..n_depth {
                match read_value(reader, dt) {
                    Ok(value) => values.set_raw(m + d, value),
                    Err(e) => {
                        log_warn!("LERC one-sweep data ended early: {}", e);
                        return Ok(());
                    }
                }
            }
        }
    }

    Ok(())
}

fn decode_huffman_int(
    reader: &mut ByteReader,
    header: &Lerc2Header,
    mask: &BitMask,
    values: &mut ValueBuffer,
    mode: ImageEncodeMode,
    dt: LercDataType,
) -> VexelResult<()> {
    let huffman = Huffman::read_code_table(reader, header.version)?;

    let offset = if dt == LercDataType::Char { 128i32 } else { 0 };
    let n_cols = header.n_cols as usize;
    let n_rows = header.n_rows as usize;
    let n_depth = header.n_depth as usize;

    let mut stream = HuffmanBitStream::new(reader.tail());
    let all_valid = header.num_valid_pixel as usize == n_cols * n_rows;

    let wrap = |v: i64| -> f64 {
        match dt {
            LercDataType::Char => (v as i8) as f64,
            LercDataType::Byte => (v as u8) as f64,
            _ => v as f64,
        }
    };

    match mode {
        ImageEncodeMode::DeltaHuffman => {
            for d in 0..n_depth {
                let mut prev_val = 0i64;

                for i in 0..n_rows {
                    for j in 0..n_cols {
                        let k = i * n_cols + j;

                        if !all_valid && !mask.is_valid(k) {
                            continue;
                        }

                        let val = match stream.decode_one(&huffman) {
                            Ok(v) => v,
                            Err(e) => {
                                log_warn!("LERC Huffman data ended early: {}", e);
                                reader.skip(stream.bytes_consumed());
                                return Ok(());
                            }
                        };

                        let m = k * n_depth + d;
                        let mut delta = val as i64 - offset as i64;

                        if j > 0 && (all_valid || mask.is_valid(k - 1)) {
                            delta += prev_val;
                        } else if i > 0 && (all_valid || mask.is_valid(k - n_cols)) {
                            delta += values.get(m - n_cols * n_depth) as i64;
                        } else {
                            delta += prev_val;
                        }

                        let stored = wrap(delta);
                        values.set_raw(m, stored);
                        prev_val = stored as i64;
                    }
                }
            }
        }
        ImageEncodeMode::Huffman => {
            for i in 0..n_rows {
                for j in 0..n_cols {
                    let k = i * n_cols + j;

                    if !all_valid && !mask.is_valid(k) {
                        continue;
                    }

                    let m0 = k * n_depth;

                    for d in 0..n_depth {
                        let val = match stream.decode_one(&huffman) {
                            Ok(v) => v,
                            Err(e) => {
                                log_warn!("LERC Huffman data ended early: {}", e);
                                reader.skip(stream.bytes_consumed());
                                return Ok(());
                            }
                        };

                        values.set_raw(m0 + d, wrap(val as i64 - offset as i64));
                    }
                }
            }
        }
        _ => return Err(VexelError::Custom(format!("Unsupported LERC Huffman image encode mode {:?}", mode))),
    }

    reader.skip(stream.bytes_consumed());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_tile(
    reader: &mut ByteReader,
    header: &Lerc2Header,
    mask: &BitMask,
    values: &mut ValueBuffer,
    dt: LercDataType,
    z_maxs: &[f64],
    i0: usize,
    i1: usize,
    j0: usize,
    j1: usize,
    depth: usize,
) -> VexelResult<()> {
    let n_cols = header.n_cols as usize;
    let n_depth = header.n_depth as usize;

    let compr_flag = reader.read_u8()?;

    let diff_enc = if header.version >= 5 { (compr_flag & 4) != 0 } else { false };
    let pattern: u8 = if header.version >= 5 { 14 } else { 15 };

    if ((compr_flag >> 2) & pattern) != ((j0 as u8 >> 3) & pattern) {
        return Err(VexelError::Custom(format!("LERC tile integrity check failed at column {}", j0)));
    }

    let diff_enc = if diff_enc && depth == 0 {
        log_warn!("LERC tile at row {} column {} is diff encoded at depth 0, decoding without diff", i0, j0);
        false
    } else {
        diff_enc
    };

    let bits67 = compr_flag >> 6;
    let compr_flag = compr_flag & 3;

    if compr_flag == 2 {
        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * n_cols + j;

                if !mask.is_valid(k) {
                    continue;
                }

                let m = k * n_depth + depth;
                let value = if diff_enc { values.get(m - 1) } else { 0.0 };
                values.set_raw(m, value);
            }
        }

        return Ok(());
    }

    if compr_flag == 0 {
        if diff_enc {
            log_warn!("LERC raw tile at row {} column {} is marked diff encoded, ignoring the flag", i0, j0);
        }

        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * n_cols + j;

                if !mask.is_valid(k) {
                    continue;
                }

                let m = k * n_depth + depth;

                match read_value(reader, dt) {
                    Ok(value) => values.set_raw(m, value),
                    Err(e) => {
                        log_warn!("LERC raw tile ended early: {}", e);
                        return Ok(());
                    }
                }
            }
        }

        return Ok(());
    }

    let base_dt = if diff_enc && !dt.is_float() { LercDataType::Int } else { dt };

    let dt_used = base_dt.reduced(bits67 as i32).unwrap_or_else(|| {
        log_warn!("Invalid LERC reduced data type code {}, using the blob data type", bits67);
        base_dt
    });

    let offset = read_value(reader, dt_used)?;

    let z_max = if header.version >= 4 && n_depth > 1 {
        z_maxs.get(depth).copied().unwrap_or(header.z_max)
    } else {
        header.z_max
    };

    if compr_flag == 3 {
        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * n_cols + j;

                if !mask.is_valid(k) {
                    continue;
                }

                let m = k * n_depth + depth;

                if diff_enc {
                    let z = offset + values.get(m - 1);
                    values.set(m, z.min(z_max));
                } else {
                    values.set(m, offset);
                }
            }
        }

        return Ok(());
    }

    let max_element_count = (i1 - i0) * (j1 - j0);
    let buffer = decode_bit_stuffed(reader, max_element_count, header.version)?;

    let inv_scale = 2.0 * header.max_z_error;
    let all_in_tile = buffer.len() == max_element_count;
    let mut idx = 0usize;

    for i in i0..i1 {
        for j in j0..j1 {
            let k = i * n_cols + j;

            if !all_in_tile && !mask.is_valid(k) {
                continue;
            }

            let Some(&quant) = buffer.get(idx) else {
                log_warn!("LERC tile ran out of quantized values, keeping {} decoded", idx);
                return Ok(());
            };
            idx += 1;

            let m = k * n_depth + depth;
            let mut z = offset + quant as f64 * inv_scale;

            if diff_enc {
                z += values.get(m - 1);
            }

            values.set(m, z.min(z_max));
        }
    }

    Ok(())
}

fn read_tiles(
    reader: &mut ByteReader,
    header: &Lerc2Header,
    mask: &BitMask,
    values: &mut ValueBuffer,
    dt: LercDataType,
    z_maxs: &[f64],
) -> VexelResult<()> {
    let mb_size = header.micro_block_size as usize;
    let n_rows = header.n_rows as usize;
    let n_cols = header.n_cols as usize;
    let n_depth = header.n_depth as usize;

    let num_tiles_vert = n_rows.div_ceil(mb_size);
    let num_tiles_hori = n_cols.div_ceil(mb_size);

    for i_tile in 0..num_tiles_vert {
        let i0 = i_tile * mb_size;
        let tile_h = if i_tile == num_tiles_vert - 1 { n_rows - i0 } else { mb_size };

        for j_tile in 0..num_tiles_hori {
            let j0 = j_tile * mb_size;
            let tile_w = if j_tile == num_tiles_hori - 1 { n_cols - j0 } else { mb_size };

            for depth in 0..n_depth {
                if let Err(e) =
                    read_tile(reader, header, mask, values, dt, z_maxs, i0, i0 + tile_h, j0, j0 + tile_w, depth)
                {
                    log_warn!(
                        "LERC tile at row {} column {} depth {} failed: {}, keeping partial image",
                        i0,
                        j0,
                        depth,
                        e
                    );
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

fn remap_no_data(values: &mut ValueBuffer, header: &Lerc2Header, mask: &BitMask) {
    if !header.pass_no_data_values {
        return;
    }

    let old = header.no_data_val;
    let new = header.no_data_val_orig;

    if old == new {
        return;
    }

    let n_cols = header.n_cols as usize;
    let n_rows = header.n_rows as usize;
    let n_depth = header.n_depth as usize;

    for i in 0..n_rows {
        for j in 0..n_cols {
            let k = i * n_cols + j;

            if !mask.is_valid(k) {
                continue;
            }

            let m = k * n_depth;

            for d in 0..n_depth {
                if values.get(m + d) == old {
                    values.set_raw(m + d, new);
                }
            }
        }
    }
}

pub fn decode_band(data: &[u8], offset: usize) -> VexelResult<DecodedBand> {
    let mut reader = ByteReader::at(data, offset);
    let header = read_header(&mut reader)?;
    let dt = header.data_type()?;

    if header.version >= 3 {
        let skip = LERC2_MAGIC.len() + 4 + 4;
        let blob_end = offset.saturating_add(header.blob_size as usize).min(data.len());
        let start = offset.saturating_add(skip);

        if start < blob_end {
            let checksum = compute_checksum_fletcher32(&data[start..blob_end]);

            if checksum != header.checksum {
                log_warn!(
                    "LERC blob checksum mismatch (got {:#x}, expected {:#x}), decoding anyway",
                    checksum,
                    header.checksum
                );
            }
        }
    }

    let mask = read_mask(&mut reader, &header)?;
    let mut values = ValueBuffer::new(header.num_values(), dt);

    if header.num_valid_pixel == 0 {
        return Ok(DecodedBand { values: values.into_values(), mask, header });
    }

    if header.z_min == header.z_max {
        fill_const_image(&mut values, &header, &mask, &[]);
        return Ok(DecodedBand { values: values.into_values(), mask, header });
    }

    let mut z_maxs: Vec<f64> = Vec::new();

    if header.version >= 4 {
        let (z_mins, maxs) = read_min_max_ranges(&mut reader, &header, dt)?;

        if z_mins == maxs {
            fill_const_image(&mut values, &header, &mask, &z_mins);
            remap_no_data(&mut values, &header, &mask);
            return Ok(DecodedBand { values: values.into_values(), mask, header });
        }

        z_maxs = maxs;
    }

    let read_data_one_sweep_flag = reader.read_u8()?;

    if read_data_one_sweep_flag != 0 {
        read_data_one_sweep(&mut reader, &header, &mask, &mut values, dt)?;
        remap_no_data(&mut values, &header, &mask);
        return Ok(DecodedBand { values: values.into_values(), mask, header });
    }

    if header.try_huffman_int() || header.try_huffman_flt() {
        let flag = reader.read_u8()?;

        let mode = ImageEncodeMode::from_flag(flag)
            .filter(|_| !(flag > 2 && header.version < 6) && !(flag > 1 && header.version < 4));

        let mode = mode.unwrap_or_else(|| {
            log_warn!("Invalid LERC image encode mode {} for version {}, falling back to tiling", flag, header.version);
            ImageEncodeMode::Tiling
        });

        if mode != ImageEncodeMode::Tiling {
            if header.try_huffman_int()
                && (mode == ImageEncodeMode::DeltaHuffman || (header.version >= 4 && mode == ImageEncodeMode::Huffman))
            {
                if let Err(e) = decode_huffman_int(&mut reader, &header, &mask, &mut values, mode, dt) {
                    log_warn!("LERC Huffman decode failed: {}, keeping what was decoded", e);
                }

                remap_no_data(&mut values, &header, &mask);
                return Ok(DecodedBand { values: values.into_values(), mask, header });
            }

            if header.try_huffman_flt() && mode == ImageEncodeMode::DeltaDeltaHuffman {
                match decode_huffman_float(
                    &mut reader,
                    dt == LercDataType::Double,
                    header.n_cols as usize,
                    header.n_rows as usize,
                    header.n_depth as usize,
                ) {
                    Ok(decoded) => {
                        for (i, value) in decoded.into_iter().enumerate() {
                            values.set_raw(i, value);
                        }
                    }
                    Err(e) => log_warn!("LERC float Huffman decode failed: {}, image will be empty", e),
                }

                remap_no_data(&mut values, &header, &mask);
                return Ok(DecodedBand { values: values.into_values(), mask, header });
            }

            log_warn!("LERC image encode mode {:?} does not match data type {:?}, falling back to tiling", mode, dt);
        }
    }

    read_tiles(&mut reader, &header, &mask, &mut values, dt, &z_maxs)?;
    remap_no_data(&mut values, &header, &mask);

    Ok(DecodedBand { values: values.into_values(), mask, header })
}

pub fn decode_bands(data: &[u8]) -> VexelResult<Vec<DecodedBand>> {
    let mut bands = Vec::new();
    let mut offset = 0usize;

    loop {
        if offset >= data.len() {
            break;
        }

        if data.len() - offset < LERC2_MAGIC.len() || &data[offset..offset + LERC2_MAGIC.len()] != LERC2_MAGIC {
            break;
        }

        let band = match decode_band(data, offset) {
            Ok(band) => band,
            Err(e) => {
                if bands.is_empty() {
                    return Err(e);
                }

                log_warn!("LERC band {} failed to decode: {}, keeping {} bands", bands.len(), e, bands.len());
                break;
            }
        };

        let blob_size = band.header.blob_size as usize;

        if let Some(first) = bands.first() {
            let first: &DecodedBand = first;
            if band.header.n_cols != first.header.n_cols
                || band.header.n_rows != first.header.n_rows
                || band.header.n_depth != first.header.n_depth
            {
                log_warn!("LERC band {} has mismatched dimensions, stopping", bands.len());
                break;
            }
        }

        let more = band.header.version <= 5 || band.header.n_blobs_more > 0;
        bands.push(band);

        if blob_size == 0 {
            break;
        }

        offset = offset.saturating_add(blob_size);

        if !more {
            break;
        }
    }

    if bands.is_empty() {
        return Err(VexelError::Custom("No LERC bands could be decoded".to_string()));
    }

    Ok(bands)
}
