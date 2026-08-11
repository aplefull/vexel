use super::bitstuffer::decode_bit_stuffed_lerc1;
use super::mask::{rle_decompress, BitMask};
use super::reader::ByteReader;
use super::types::{Lerc1Header, LERC1_MAGIC};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

const LERC1_VERSION: i32 = 11;
const LERC1_MAX_DIM: i32 = 40000;

pub struct Lerc1Band {
    pub values: Vec<f64>,
    pub mask: BitMask,
    pub header: Lerc1Header,
}

struct CntZImage {
    cnt: Vec<f32>,
    z: Vec<f32>,
    width: usize,
    height: usize,
    decoder_can_ignore_mask: bool,
}

fn read_i32_le(reader: &mut ByteReader) -> VexelResult<i32> {
    reader.read_i32()
}

fn read_f32_le(reader: &mut ByteReader) -> VexelResult<f32> {
    let bytes = reader.take(4)?;
    Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_f64_le(reader: &mut ByteReader) -> VexelResult<f64> {
    reader.read_f64()
}

fn read_flt(reader: &mut ByteReader, num_bytes: usize) -> VexelResult<f32> {
    match num_bytes {
        1 => Ok(reader.read_u8()? as i8 as f32),
        2 => {
            let bytes = reader.take(2)?;
            Ok(i16::from_le_bytes([bytes[0], bytes[1]]) as f32)
        }
        4 => read_f32_le(reader),
        _ => Err(VexelError::Custom(format!("Invalid LERC1 float width {}", num_bytes))),
    }
}

pub fn read_header(reader: &mut ByteReader) -> VexelResult<Lerc1Header> {
    let key = reader.take(LERC1_MAGIC.len())?;

    if key != LERC1_MAGIC {
        return Err(VexelError::Custom("Not a CntZImage blob".to_string()));
    }

    let version = read_i32_le(reader)?;
    let image_type = read_i32_le(reader)?;
    let height = read_i32_le(reader)?;
    let width = read_i32_le(reader)?;
    let max_z_error = read_f64_le(reader)?;

    if version != LERC1_VERSION {
        return Err(VexelError::UnsupportedFormat(format!("CntZImage version {}", version)));
    }

    if image_type != 8 {
        log_warn!("Unexpected CntZImage type {}, decoding as count/z image", image_type);
    }

    if width <= 0 || height <= 0 || width > LERC1_MAX_DIM || height > LERC1_MAX_DIM {
        return Err(VexelError::Custom(format!("Invalid LERC1 dimensions {}x{}", width, height)));
    }

    Ok(Lerc1Header { width, height, max_z_error })
}

fn read_cnt_tile(
    reader: &mut ByteReader,
    image: &mut CntZImage,
    i0: usize,
    i1: usize,
    j0: usize,
    j1: usize,
) -> VexelResult<()> {
    let compr_flag = reader.read_u8()?;

    if compr_flag == 2 {
        return Ok(());
    }

    if compr_flag == 3 || compr_flag == 4 {
        let value = if compr_flag == 3 { -1.0f32 } else { 1.0f32 };

        for i in i0..i1 {
            for j in j0..j1 {
                image.cnt[i * image.width + j] = value;
                image.z[i * image.width + j] = 0.0;
            }
        }

        return Ok(());
    }

    if (compr_flag & 63) > 4 {
        return Err(VexelError::Custom(format!("Invalid LERC1 count tile flag {}", compr_flag)));
    }

    if compr_flag == 0 {
        for i in i0..i1 {
            for j in j0..j1 {
                let value = read_f32_le(reader)?;
                image.cnt[i * image.width + j] = value;
            }
        }

        return Ok(());
    }

    let bits67 = compr_flag >> 6;
    let n = if bits67 == 0 { 4 } else { 3 - bits67 as usize };
    let offset = read_flt(reader, n)?;

    let buffer = decode_bit_stuffed_lerc1(reader)?;
    let mut idx = 0usize;

    for i in i0..i1 {
        for j in j0..j1 {
            let quant = buffer.get(idx).copied().unwrap_or(0);
            idx += 1;
            image.cnt[i * image.width + j] = offset + quant as f32;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_z_tile(
    reader: &mut ByteReader,
    image: &mut CntZImage,
    i0: usize,
    i1: usize,
    j0: usize,
    j1: usize,
    max_z_error: f64,
    max_z_in_img: f32,
) -> VexelResult<()> {
    let compr_flag = reader.read_u8()?;
    let bits67 = compr_flag >> 6;
    let compr_flag = compr_flag & 63;

    if compr_flag == 2 {
        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * image.width + j;
                if image.cnt[k] > 0.0 {
                    image.z[k] = 0.0;
                }
            }
        }

        return Ok(());
    }

    if compr_flag > 3 {
        return Err(VexelError::Custom(format!("Invalid LERC1 z tile flag {}", compr_flag)));
    }

    if compr_flag == 0 {
        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * image.width + j;

                if image.cnt[k] > 0.0 {
                    image.z[k] = read_f32_le(reader)?;
                }
            }
        }

        return Ok(());
    }

    let n = if bits67 == 0 { 4 } else { 3 - bits67 as usize };
    let offset = read_flt(reader, n)?;

    if compr_flag == 3 {
        for i in i0..i1 {
            for j in j0..j1 {
                let k = i * image.width + j;

                if image.cnt[k] > 0.0 {
                    image.z[k] = offset;
                }
            }
        }

        return Ok(());
    }

    let buffer = decode_bit_stuffed_lerc1(reader)?;
    let inv_scale = 2.0 * max_z_error;
    let mut idx = 0usize;

    for i in i0..i1 {
        for j in j0..j1 {
            let k = i * image.width + j;

            if !image.decoder_can_ignore_mask && image.cnt[k] <= 0.0 {
                continue;
            }

            let quant = buffer.get(idx).copied().unwrap_or(0);
            idx += 1;

            let z = (offset as f64 + quant as f64 * inv_scale) as f32;
            image.z[k] = z.min(max_z_in_img);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_tiles(
    reader: &mut ByteReader,
    image: &mut CntZImage,
    z_part: bool,
    num_tiles_vert: i32,
    num_tiles_hori: i32,
    max_z_error: f64,
    max_val_in_img: f32,
) -> VexelResult<()> {
    let height = image.height as i32;
    let width = image.width as i32;

    if num_tiles_vert <= 0 || num_tiles_hori <= 0 || num_tiles_vert > height || num_tiles_hori > width {
        return Err(VexelError::Custom(format!("Invalid LERC1 tiling {}x{}", num_tiles_hori, num_tiles_vert)));
    }

    for i_tile in 0..=num_tiles_vert {
        let mut tile_h = height / num_tiles_vert;
        let i0 = i_tile * tile_h;

        if i_tile == num_tiles_vert {
            tile_h = height % num_tiles_vert;
        }

        if tile_h == 0 {
            continue;
        }

        for j_tile in 0..=num_tiles_hori {
            let mut tile_w = width / num_tiles_hori;
            let j0 = j_tile * tile_w;

            if j_tile == num_tiles_hori {
                tile_w = width % num_tiles_hori;
            }

            if tile_w == 0 {
                continue;
            }

            let result = if z_part {
                read_z_tile(
                    reader,
                    image,
                    i0 as usize,
                    (i0 + tile_h) as usize,
                    j0 as usize,
                    (j0 + tile_w) as usize,
                    max_z_error,
                    max_val_in_img,
                )
            } else {
                read_cnt_tile(reader, image, i0 as usize, (i0 + tile_h) as usize, j0 as usize, (j0 + tile_w) as usize)
            };

            if let Err(e) = result {
                log_warn!("LERC1 tile at row {} column {} failed: {}, keeping partial image", i0, j0, e);
                return Ok(());
            }
        }
    }

    Ok(())
}

fn read_image(
    reader: &mut ByteReader,
    header: &Lerc1Header,
    only_z_part: bool,
    image: &mut CntZImage,
) -> VexelResult<()> {
    for i_part in 0..2 {
        let z_part = i_part != 0;

        if !z_part && only_z_part {
            continue;
        }

        let num_tiles_vert = read_i32_le(reader)?;
        let num_tiles_hori = read_i32_le(reader)?;
        let num_bytes = read_i32_le(reader)?;
        let max_val_in_img = read_f32_le(reader)?;

        if num_bytes < 0 {
            log_warn!("Invalid LERC1 part size {}, reading to the end of the blob", num_bytes);
        }

        let part_start = reader.position();
        let num_bytes = num_bytes.max(0);

        if !z_part && num_tiles_vert == 0 && num_tiles_hori == 0 {
            if num_bytes == 0 {
                image.cnt.fill(max_val_in_img);

                if max_val_in_img > 0.0 {
                    image.decoder_can_ignore_mask = true;
                }
            } else {
                let mut mask = BitMask::new(image.width, image.height);
                let available = reader.remaining().min(num_bytes as usize);
                let src = reader.peek(available);

                if rle_decompress(src, mask.bits_mut()) {
                    for k in 0..image.width * image.height {
                        image.cnt[k] = if mask.is_valid(k) { 1.0 } else { 0.0 };
                    }
                } else {
                    log_warn!("LERC1 count mask could not be decoded, treating all pixels as valid");
                    image.cnt.fill(1.0);
                }
            }
        } else if let Err(e) =
            read_tiles(reader, image, z_part, num_tiles_vert, num_tiles_hori, header.max_z_error, max_val_in_img)
        {
            log_warn!("LERC1 {} part failed: {}", if z_part { "z" } else { "count" }, e);
        }

        reader.seek(part_start.saturating_add(num_bytes as usize));
    }

    Ok(())
}

pub fn decode_bands(data: &[u8]) -> VexelResult<Vec<Lerc1Band>> {
    let mut reader = ByteReader::new(data);
    let header = read_header(&mut reader)?;

    let width = header.width as usize;
    let height = header.height as usize;

    let mut bands = Vec::new();
    let mut only_z_part = false;

    loop {
        let mut image = CntZImage {
            cnt: vec![0.0; width * height],
            z: vec![0.0; width * height],
            width,
            height,
            decoder_can_ignore_mask: false,
        };

        if let Some(previous) = bands.last() {
            let previous: &Lerc1Band = previous;
            for (k, cnt) in image.cnt.iter_mut().enumerate() {
                *cnt = if previous.mask.is_valid(k) { 1.0 } else { 0.0 };
            }
        }

        if let Err(e) = read_image(&mut reader, &header, only_z_part, &mut image) {
            if bands.is_empty() {
                return Err(e);
            }

            log_warn!("LERC1 band {} failed to decode: {}", bands.len(), e);
            break;
        }

        let mut mask = BitMask::new(width, height);
        let bits = mask.bits_mut();

        for k in 0..width * height {
            if image.cnt[k] > 0.0 {
                bits[k >> 3] |= (1u8 << 7) >> (k & 7);
            }
        }

        bands.push(Lerc1Band { values: image.z.iter().map(|&z| z as f64).collect(), mask, header: header.clone() });

        only_z_part = true;

        if reader.remaining() < LERC1_MAGIC.len() + 20 {
            break;
        }
    }

    if bands.is_empty() {
        return Err(VexelError::Custom("No LERC1 bands could be decoded".to_string()));
    }

    Ok(bands)
}
