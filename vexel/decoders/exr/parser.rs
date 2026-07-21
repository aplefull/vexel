use crate::decoders::exr::types::{
    Box2i, ExrAttributeInfo, ExrChannel, ExrCompression, ExrPixelType, LevelMode, LineOrder, PartHeader, PartKind,
    TileDesc, EXR_MAGIC, VERSION_FLAG_DEEP, VERSION_FLAG_MULTIPART, VERSION_FLAG_TILED,
};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

const MAX_NAME_LENGTH: usize = 256;
const MAX_CHANNELS: usize = 4096;
const MAX_PARTS: usize = 512;

pub struct ByteCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn read_u8(&mut self) -> VexelResult<u8> {
        if self.pos >= self.data.len() {
            return Err(VexelError::Custom("EXR: unexpected end of data".to_string()));
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn peek_u8(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub fn read_bytes(&mut self, count: usize) -> VexelResult<&'a [u8]> {
        if count > self.remaining() {
            return Err(VexelError::Custom("EXR: unexpected end of data".to_string()));
        }
        let slice = &self.data[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    pub fn skip(&mut self, count: usize) -> VexelResult<()> {
        self.read_bytes(count).map(|_| ())
    }

    pub fn read_i32(&mut self) -> VexelResult<i32> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_u32(&mut self) -> VexelResult<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_u64(&mut self) -> VexelResult<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub fn read_f32(&mut self) -> VexelResult<f32> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    pub fn read_cstring(&mut self, max_length: usize) -> VexelResult<String> {
        let mut bytes = Vec::new();
        loop {
            let b = self.read_u8()?;
            if b == 0 {
                break;
            }
            if bytes.len() >= max_length {
                return Err(VexelError::Custom("EXR: string too long".to_string()));
            }
            bytes.push(b);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

pub struct ExrFile {
    pub version: i32,
    pub multipart: bool,
    pub parts: Vec<ExrPart>,
}

pub struct ExrPart {
    pub header: PartHeader,
    pub chunk_count: u32,
    pub chunk_offsets: Vec<u64>,
}

pub fn parse(data: &[u8]) -> VexelResult<ExrFile> {
    let mut cursor = ByteCursor::new(data);

    let magic = cursor.read_bytes(4)?;
    if magic != EXR_MAGIC {
        return Err(VexelError::Custom("EXR: bad magic bytes".to_string()));
    }

    let version = cursor.read_i32()?;
    let version_number = version & 0xFF;
    if version_number != 2 && version_number != 1 {
        log_warn!("EXR: unexpected version number {}, attempting to decode anyway", version_number);
    }

    let multipart = version & VERSION_FLAG_MULTIPART != 0;
    let single_tiled = version & VERSION_FLAG_TILED != 0;
    let deep_flag = version & VERSION_FLAG_DEEP != 0;

    let mut headers = Vec::new();
    loop {
        let header = parse_header(&mut cursor, multipart, single_tiled, deep_flag)?;
        headers.push(header);

        if !multipart {
            break;
        }

        match cursor.peek_u8() {
            Some(0) => {
                cursor.skip(1)?;
                break;
            }
            Some(_) => {
                if headers.len() >= MAX_PARTS {
                    log_warn!("EXR: too many parts, stopping after {}", MAX_PARTS);
                    break;
                }
            }
            None => {
                log_warn!("EXR: file ends inside multipart header list");
                break;
            }
        }
    }

    let mut parts = Vec::with_capacity(headers.len());
    for header in headers {
        let computed = compute_chunk_count(&header);
        let chunk_count = match header.chunk_count {
            Some(count) => {
                if count as u64 != computed && computed != 0 {
                    log_warn!("EXR: chunkCount attribute {} differs from computed {}", count, computed);
                }
                count as u64
            }
            None => computed,
        };

        let max_entries = (cursor.remaining() / 8) as u64;
        let chunk_count = if chunk_count > max_entries {
            log_warn!("EXR: chunk count {} exceeds remaining file capacity, clamping to {}", chunk_count, max_entries);
            max_entries
        } else {
            chunk_count
        };

        let mut chunk_offsets = Vec::with_capacity(chunk_count as usize);
        for _ in 0..chunk_count {
            chunk_offsets.push(cursor.read_u64()?);
        }

        parts.push(ExrPart { header, chunk_count: chunk_count as u32, chunk_offsets });
    }

    Ok(ExrFile { version, multipart, parts })
}

fn parse_header(
    cursor: &mut ByteCursor,
    multipart: bool,
    single_tiled: bool,
    deep_flag: bool,
) -> VexelResult<PartHeader> {
    let mut channels: Vec<ExrChannel> = Vec::new();
    let mut compression = ExrCompression::Zip;
    let mut have_compression = false;
    let mut data_window = Box2i { x_min: 0, y_min: 0, x_max: 0, y_max: 0 };
    let mut have_data_window = false;
    let mut display_window = Box2i { x_min: 0, y_min: 0, x_max: 0, y_max: 0 };
    let mut line_order = LineOrder::IncreasingY;
    let mut tiles = None;
    let mut chunk_count = None;
    let mut part_type: Option<String> = None;
    let mut name = None;
    let mut chromaticities = None;
    let mut attributes = Vec::new();

    loop {
        let attr_name = cursor.read_cstring(MAX_NAME_LENGTH)?;
        if attr_name.is_empty() {
            break;
        }

        let attr_type = cursor.read_cstring(MAX_NAME_LENGTH)?;
        let size = cursor.read_i32()?;
        let size = if size < 0 || size as usize > cursor.remaining() {
            let recovered = if attr_type == "chlist" {
                scan_chlist_size(&cursor.data[cursor.pos..])
            } else {
                fixed_attr_size(&attr_type).filter(|&s| s <= cursor.remaining())
            };

            match recovered {
                Some(recovered) => {
                    log_warn!(
                        "EXR: attribute '{}' has invalid size {}, recovering with intrinsic size {} for type '{}'",
                        attr_name,
                        size,
                        recovered,
                        attr_type
                    );
                    recovered
                }
                None => {
                    log_warn!(
                        "EXR: attribute '{}' of type '{}' has invalid size {} ({} bytes remain), stopping header parse",
                        attr_name,
                        attr_type,
                        size,
                        cursor.remaining()
                    );
                    break;
                }
            }
        } else {
            size as usize
        };

        attributes.push(ExrAttributeInfo { name: attr_name.clone(), attr_type: attr_type.clone(), size: size as u32 });

        let value = cursor.read_bytes(size)?;
        let mut value_cursor = ByteCursor::new(value);

        match (attr_name.as_str(), attr_type.as_str()) {
            ("channels", "chlist") => {
                channels = parse_channel_list(&mut value_cursor)?;
            }
            ("compression", "compression") => {
                compression = ExrCompression::from_u8(value_cursor.read_u8()?);
                have_compression = true;
            }
            ("dataWindow", "box2i") => {
                data_window = parse_box2i(&mut value_cursor)?;
                have_data_window = true;
            }
            ("displayWindow", "box2i") => {
                display_window = parse_box2i(&mut value_cursor)?;
            }
            ("lineOrder", "lineOrder") => {
                line_order = LineOrder::from_u8(value_cursor.read_u8()?);
            }
            ("tiles", "tiledesc") => {
                let x_size = value_cursor.read_u32()?;
                let y_size = value_cursor.read_u32()?;
                let mode = value_cursor.read_u8()?;
                let level_mode = match mode & 0x0F {
                    1 => LevelMode::Mipmap,
                    2 => LevelMode::Ripmap,
                    _ => LevelMode::OneLevel,
                };
                let round_up = (mode >> 4) == 1;
                if x_size == 0 || y_size == 0 {
                    log_warn!("EXR: invalid tile size {}x{}", x_size, y_size);
                } else {
                    tiles = Some(TileDesc { x_size, y_size, level_mode, round_up });
                }
            }
            ("type", "string") => {
                part_type = Some(String::from_utf8_lossy(value).into_owned());
            }
            ("chunkCount", "int") => {
                let count = value_cursor.read_i32()?;
                if count >= 0 {
                    chunk_count = Some(count as u32);
                }
            }
            ("name", "string") => {
                name = Some(String::from_utf8_lossy(value).into_owned());
            }
            ("chromaticities", "chromaticities") => {
                let mut values = [0f32; 8];
                for v in values.iter_mut() {
                    *v = value_cursor.read_f32()?;
                }
                chromaticities = Some(values);
            }
            _ => {}
        }
    }

    if !have_compression {
        log_warn!("EXR: missing compression attribute, assuming none");
        compression = ExrCompression::None;
    }

    if !have_data_window {
        log_warn!("EXR: missing dataWindow attribute");
    }

    let attr_kind = part_type.as_deref().and_then(PartKind::from_type_string);
    let kind = if multipart {
        match attr_kind {
            Some(kind) => kind,
            None => {
                if let Some(t) = &part_type {
                    log_warn!("EXR: unknown part type '{}', guessing from tiles attribute", t);
                }
                if tiles.is_some() {
                    PartKind::Tiled
                } else {
                    PartKind::Scanline
                }
            }
        }
    } else if deep_flag {
        match attr_kind {
            Some(kind) if kind.is_deep() => kind,
            _ => {
                if tiles.is_some() {
                    PartKind::DeepTiled
                } else {
                    PartKind::DeepScanline
                }
            }
        }
    } else {
        let kind = if single_tiled { PartKind::Tiled } else { PartKind::Scanline };
        if let Some(attr_kind) = attr_kind {
            if attr_kind != kind {
                log_warn!(
                    "EXR: type attribute '{}' contradicts version flags, treating part as {}",
                    attr_kind.name(),
                    kind.name()
                );
            }
        }
        kind
    };

    if kind.is_tiled() && tiles.is_none() {
        log_warn!("EXR: tiled part is missing tiles attribute, treating as 64x64 one-level");
        tiles = Some(TileDesc { x_size: 64, y_size: 64, level_mode: LevelMode::OneLevel, round_up: false });
    }

    Ok(PartHeader {
        name,
        kind,
        channels,
        compression,
        data_window,
        display_window,
        line_order,
        tiles,
        chunk_count,
        chromaticities,
        attributes,
    })
}

fn fixed_attr_size(attr_type: &str) -> Option<usize> {
    match attr_type {
        "compression" | "lineOrder" | "envmap" | "deepImageState" => Some(1),
        "half" => Some(2),
        "int" | "float" => Some(4),
        "double" | "v2i" | "v2f" | "rational" | "timecode" => Some(8),
        "tiledesc" => Some(9),
        "v3i" | "v3f" => Some(12),
        "box2i" | "box2f" | "v2d" => Some(16),
        "v3d" => Some(24),
        "keycode" => Some(28),
        "chromaticities" => Some(32),
        "m33f" => Some(36),
        "m44f" => Some(64),
        "m33d" => Some(72),
        "m44d" => Some(128),
        _ => None,
    }
}

fn scan_chlist_size(data: &[u8]) -> Option<usize> {
    let mut pos = 0;
    loop {
        match data.get(pos) {
            Some(0) => return Some(pos + 1),
            None => return None,
            Some(_) => {}
        }

        let name_length = data[pos..].iter().position(|&b| b == 0)?;
        if name_length > MAX_NAME_LENGTH {
            return None;
        }

        pos += name_length + 1 + 16;
        if pos > data.len() {
            return None;
        }
    }
}

fn parse_box2i(cursor: &mut ByteCursor) -> VexelResult<Box2i> {
    Ok(Box2i {
        x_min: cursor.read_i32()?,
        y_min: cursor.read_i32()?,
        x_max: cursor.read_i32()?,
        y_max: cursor.read_i32()?,
    })
}

fn parse_channel_list(cursor: &mut ByteCursor) -> VexelResult<Vec<ExrChannel>> {
    let mut channels = Vec::new();

    loop {
        if cursor.peek_u8() == Some(0) || cursor.remaining() == 0 {
            break;
        }

        let name = cursor.read_cstring(MAX_NAME_LENGTH)?;
        let pixel_type_raw = cursor.read_i32()?;
        let p_linear = cursor.read_u8()? != 0;
        cursor.skip(3)?;
        let x_sampling = cursor.read_i32()?;
        let y_sampling = cursor.read_i32()?;

        let pixel_type = match ExrPixelType::from_i32(pixel_type_raw) {
            Some(t) => t,
            None => {
                log_warn!("EXR: channel '{}' has unknown pixel type {}, assuming half", name, pixel_type_raw);
                ExrPixelType::Half
            }
        };

        let x_sampling = if x_sampling < 1 {
            log_warn!("EXR: channel '{}' has invalid x sampling {}, using 1", name, x_sampling);
            1
        } else {
            x_sampling
        };
        let y_sampling = if y_sampling < 1 {
            log_warn!("EXR: channel '{}' has invalid y sampling {}, using 1", name, y_sampling);
            1
        } else {
            y_sampling
        };

        channels.push(ExrChannel { name, pixel_type, p_linear, x_sampling, y_sampling });

        if channels.len() > MAX_CHANNELS {
            return Err(VexelError::Custom("EXR: too many channels".to_string()));
        }
    }

    Ok(channels)
}

pub fn level_size(size: i64, level: u32, round_up: bool) -> i64 {
    let divisor = 1i64 << level.min(62);
    let mut result = size / divisor;
    if round_up && result * divisor < size {
        result += 1;
    }
    result.max(1)
}

fn num_levels_for(size: i64, round_up: bool) -> u32 {
    let mut levels = 1u32;
    let mut s = size.max(1);
    while s > 1 {
        if round_up {
            s = (s + 1) / 2;
        } else {
            s /= 2;
        }
        levels += 1;
    }
    levels
}

pub fn tile_level_dimensions(header: &PartHeader) -> Vec<(u32, u32, i64, i64)> {
    let tiles = match &header.tiles {
        Some(t) => t,
        None => return Vec::new(),
    };

    let w = header.data_window.width();
    let h = header.data_window.height();
    let mut levels = Vec::new();

    match tiles.level_mode {
        LevelMode::OneLevel => {
            levels.push((0, 0, w, h));
        }
        LevelMode::Mipmap => {
            let n = num_levels_for(w.max(h), tiles.round_up);
            for l in 0..n {
                levels.push((l, l, level_size(w, l, tiles.round_up), level_size(h, l, tiles.round_up)));
            }
        }
        LevelMode::Ripmap => {
            let nx = num_levels_for(w, tiles.round_up);
            let ny = num_levels_for(h, tiles.round_up);
            for ly in 0..ny {
                for lx in 0..nx {
                    levels.push((lx, ly, level_size(w, lx, tiles.round_up), level_size(h, ly, tiles.round_up)));
                }
            }
        }
    }

    levels
}

fn compute_chunk_count(header: &PartHeader) -> u64 {
    let h = header.data_window.height();
    let w = header.data_window.width();
    if h <= 0 || w <= 0 {
        return 0;
    }

    if header.kind.is_tiled() {
        let tiles = match &header.tiles {
            Some(t) => t,
            None => return 0,
        };
        let tw = tiles.x_size as i64;
        let th = tiles.y_size as i64;

        tile_level_dimensions(header)
            .iter()
            .map(|(_, _, lw, lh)| {
                let nx = (lw + tw - 1) / tw;
                let ny = (lh + th - 1) / th;
                (nx * ny) as u64
            })
            .sum()
    } else {
        let lines = match header.kind {
            PartKind::DeepScanline => 1,
            _ => header.compression.lines_per_block() as i64,
        };
        ((h + lines - 1) / lines) as u64
    }
}
