use crate::decoders::jpeg_xr::types::*;
use crate::log_warn;

const PIXEL_FORMAT_BASE: [u8; 15] =
    [0x24, 0xc3, 0xdd, 0x6f, 0x03, 0x4e, 0xfe, 0x4b, 0xb1, 0x85, 0x3d, 0x77, 0x76, 0x8d, 0xc9];

#[derive(Debug, Clone, Default)]
pub struct Container {
    pub image_offset: usize,
    pub image_byte_count: usize,
    pub alpha_offset: usize,
    pub alpha_byte_count: usize,
    pub pixel_format: Option<PixelFormatKind>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub transformation: Option<u32>,
    pub icc_profile: Option<Vec<u8>>,
}

pub fn pixel_format_from_guid(guid: &[u8]) -> PixelFormatKind {
    if guid.len() < 16 {
        return PixelFormatKind::Unknown;
    }

    if guid[..15] != PIXEL_FORMAT_BASE {
        return match guid {
            [0x95, 0x6b, 0x8c, 0xd9, ..] => PixelFormatKind::Rgb128Float,
            [0x8f, 0xd7, 0xfe, 0xe3, ..] => PixelFormatKind::Rgb96Fixed,
            _ => PixelFormatKind::Unknown,
        };
    }

    match guid[15] {
        0x05 => PixelFormatKind::BlackWhite1,
        0x08 => PixelFormatKind::Gray8,
        0x09 => PixelFormatKind::Bgr555,
        0x0a => PixelFormatKind::Bgr565,
        0x0b => PixelFormatKind::Gray16,
        0x0c => PixelFormatKind::Bgr24,
        0x0d => PixelFormatKind::Rgb24,
        0x0e => PixelFormatKind::Bgr32,
        0x0f | 0x10 => PixelFormatKind::Bgra32,
        0x11 => PixelFormatKind::Gray32Float,
        0x12 => PixelFormatKind::Rgb48Fixed,
        0x13 => PixelFormatKind::Gray16Fixed,
        0x14 => PixelFormatKind::Bgr101010,
        0x15 => PixelFormatKind::Rgb48,
        0x16 | 0x17 => PixelFormatKind::Rgba64,
        0x18 => PixelFormatKind::Rgb96Fixed,
        0x19 | 0x1a => PixelFormatKind::Rgba128Float,
        0x1b => PixelFormatKind::Rgb128Float,
        0x1c => PixelFormatKind::Cmyk32,
        0x1d => PixelFormatKind::Rgba64Fixed,
        0x1e => PixelFormatKind::Rgba128Fixed,
        0x1f => PixelFormatKind::Cmyk64,
        0x20..=0x27 => PixelFormatKind::NChannel,
        0x29 => PixelFormatKind::Cmyka40,
        0x2a => PixelFormatKind::Cmyka80,
        0x2b..=0x32 => PixelFormatKind::NChannel,
        0x3a => PixelFormatKind::Rgba64Half,
        0x3b => PixelFormatKind::Rgb48Half,
        0x3d => PixelFormatKind::Rgbe32,
        0x3e => PixelFormatKind::Gray16Half,
        0x3f => PixelFormatKind::Gray32Fixed,
        0x40 => PixelFormatKind::Rgb48Fixed,
        0x41 => PixelFormatKind::Rgb128Float,
        0x42 => PixelFormatKind::Rgb48Half,
        _ => PixelFormatKind::Unknown,
    }
}

fn read_u16(data: &[u8], pos: usize) -> Option<u16> {
    data.get(pos..pos + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn entry_type_size(kind: u16) -> usize {
    match kind {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

pub fn looks_like_container(header: &[u8]) -> bool {
    header.len() >= 8 && header[..4] == CONTAINER_MAGIC
}

pub fn looks_like_codestream(header: &[u8]) -> bool {
    header.len() >= 8 && &header[..8] == CODESTREAM_MAGIC
}

pub fn parse_container(data: &[u8]) -> Option<Container> {
    if !looks_like_container(data) {
        return None;
    }

    let first_ifd = read_u32(data, 4)? as usize;
    let mut container = Container::default();
    let mut ifd_offset = first_ifd;
    let mut visited = Vec::new();

    while ifd_offset != 0 {
        if visited.contains(&ifd_offset) {
            log_warn!("JPEG XR: IFD cycle detected at offset {}", ifd_offset);
            break;
        }
        visited.push(ifd_offset);

        if visited.len() > 32 {
            log_warn!("JPEG XR: too many IFDs, stopping");
            break;
        }

        let count = match read_u16(data, ifd_offset) {
            Some(count) => count as usize,
            None => break,
        };

        for i in 0..count {
            let entry = ifd_offset + 2 + i * 12;
            let Some(tag) = read_u16(data, entry) else { break };
            let Some(kind) = read_u16(data, entry + 2) else { break };
            let Some(items) = read_u32(data, entry + 4) else { break };
            let Some(value) = read_u32(data, entry + 8) else { break };

            let byte_len = entry_type_size(kind).saturating_mul(items as usize);
            let inline = byte_len <= 4;
            let payload_at = if inline { entry + 8 } else { value as usize };

            match tag {
                TAG_PIXEL_FORMAT => {
                    if let Some(guid) = data.get(payload_at..payload_at + 16) {
                        container.pixel_format = Some(pixel_format_from_guid(guid));
                    }
                }
                TAG_IMAGE_WIDTH => container.width = Some(value),
                TAG_IMAGE_HEIGHT => container.height = Some(value),
                TAG_TRANSFORMATION => container.transformation = Some(value),
                TAG_IMAGE_OFFSET => container.image_offset = value as usize,
                TAG_IMAGE_BYTE_COUNT => container.image_byte_count = value as usize,
                TAG_ALPHA_OFFSET => container.alpha_offset = value as usize,
                TAG_ALPHA_BYTE_COUNT => container.alpha_byte_count = value as usize,
                TAG_ICC_PROFILE => {
                    if let Some(profile) = data.get(payload_at..payload_at + byte_len) {
                        container.icc_profile = Some(profile.to_vec());
                    }
                }
                _ => {}
            }
        }

        ifd_offset = match read_u32(data, ifd_offset + 2 + count * 12) {
            Some(next) => next as usize,
            None => 0,
        };
    }

    if container.image_offset == 0 {
        if let Some(pos) = find_codestream(data) {
            log_warn!("JPEG XR: no ImageOffset tag, using found codestream at {}", pos);
            container.image_offset = pos;
        }
    }

    Some(container)
}

pub fn find_codestream(data: &[u8]) -> Option<usize> {
    data.windows(CODESTREAM_MAGIC.len()).position(|w| w == CODESTREAM_MAGIC)
}

pub fn parse_jpx_boxes(data: &[u8]) -> Option<Container> {
    let mut pos = 0usize;
    let mut best: Option<Container> = None;

    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes(data.get(pos..pos + 4)?.try_into().ok()?) as usize;
        let kind = data.get(pos + 4..pos + 8)?;

        let (payload_start, payload_len) = if len == 1 {
            let ext = u64::from_be_bytes(data.get(pos + 8..pos + 16)?.try_into().ok()?) as usize;
            (pos + 16, ext.checked_sub(16)?)
        } else if len == 0 {
            (pos + 8, data.len() - (pos + 8))
        } else {
            (pos + 8, len.checked_sub(8)?)
        };

        let payload_end = payload_start.checked_add(payload_len)?.min(data.len());
        let payload = data.get(payload_start..payload_end)?;

        match kind {
            b"jp2c" | b"jxrc" => {
                if let Some(rel) = find_codestream(payload) {
                    return Some(Container {
                        image_offset: payload_start + rel,
                        image_byte_count: payload_end - (payload_start + rel),
                        ..Default::default()
                    });
                }
            }
            b"jp2h" | b"jpch" | b"jplh" => {
                if let Some(inner) = parse_jpx_boxes(payload) {
                    best = Some(inner);
                }
            }
            _ => {}
        }

        let advance = if len == 0 { data.len() - pos } else { (payload_end - pos).max(8) };
        pos = pos.checked_add(advance)?;
    }

    best
}
