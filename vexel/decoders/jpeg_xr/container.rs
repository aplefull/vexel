use crate::decoders::jpeg_xr::types::*;
use crate::log_warn;
use crate::utils::bytes::{le_u16, le_u32};
use crate::utils::metadata::{jp2, MetadataCollector};

const PIXEL_FORMAT_BASE: [u8; 15] =
    [0x24, 0xc3, 0xdd, 0x6f, 0x03, 0x4e, 0xfe, 0x4b, 0xb1, 0x85, 0x3d, 0x77, 0x76, 0x8d, 0xc9];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContainerKind {
    #[default]
    Tiff,
    Jpx,
}

#[derive(Debug, Clone, Default)]
pub struct Container {
    pub kind: ContainerKind,
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

pub fn effective_orientation(container: Option<&Container>, spatial_transform: u32) -> Orientation {
    match container {
        Some(container) if container.kind == ContainerKind::Tiff => {
            container.transformation.map(Orientation::from_bits).unwrap_or(Orientation::None)
        }
        _ => Orientation::from_bits(spatial_transform),
    }
}

pub fn pixel_format_from_guid(guid: &[u8]) -> PixelFormatKind {
    use PixelFormatKind::*;

    let Some(guid) = guid.get(..16) else {
        return Unknown;
    };

    if guid[..15] != PIXEL_FORMAT_BASE {
        return match guid {
            [0x95, 0x6b, 0x8c, 0xd9, 0xfe, 0x3e, 0xd6, 0x47, 0xbb, 0x25, 0xeb, 0x17, 0x48, 0xab, 0x0c, 0xf1] => Rgb32,
            [0x2d, 0xad, 0xc7, 0xf5, 0x8d, 0x6a, 0xdd, 0x43, 0xa7, 0xa8, 0xa2, 0x99, 0x35, 0x26, 0x1a, 0xe9] => Rgba32,
            [0x50, 0xa6, 0xc4, 0x3c, 0x27, 0xa5, 0x37, 0x4d, 0xa9, 0x16, 0x31, 0x42, 0xc7, 0xeb, 0xed, 0xba] => Prgba32,
            [0x8f, 0xd7, 0xfe, 0xe3, 0xdb, 0xe8, 0xcf, 0x4a, 0x84, 0xc1, 0xe9, 0x7f, 0x61, 0x36, 0xb3, 0x27] => {
                Rgb96Float
            }
            _ => Unknown,
        };
    }

    match guid[15] {
        0x05 => BlackWhite,
        0x08 => Gray8,
        0x09 => Bgr555,
        0x0a => Bgr565,
        0x0b => Gray16,
        0x0c => Bgr24,
        0x0d => Rgb24,
        0x0e => Bgr32,
        0x0f => Bgra32,
        0x10 => Pbgra32,
        0x11 => Gray32Float,
        0x12 => Rgb48Fixed,
        0x13 => Gray16Fixed,
        0x14 => Bgr101010,
        0x15 => Rgb48,
        0x16 => Rgba64,
        0x17 => Prgba64,
        0x18 => Rgb96Fixed,
        0x19 => Rgba128Float,
        0x1a => Prgba128Float,
        0x1b => Rgb128Float,
        0x1c => Cmyk32,
        0x1d => Rgba64Fixed,
        0x1e => Rgba128Fixed,
        0x1f => Cmyk64,
        0x20..=0x2b => NChannels,
        0x2c => Cmyka40,
        0x2d => Cmyka80,
        0x2e..=0x39 => NChannelsAlpha,
        0x3a => Rgba64Half,
        0x3b => Rgb48Half,
        0x3d => Rgbe32,
        0x3e => Gray16Half,
        0x3f => Gray32Fixed,
        0x40 => Rgb64Fixed,
        0x41 => Rgb128Fixed,
        0x42 => Rgb64Half,
        0x43 => CmykDirectAlpha80,
        0x44..=0x4b => Ycc,
        0x4c..=0x53 => YccAlpha,
        0x54 => CmykDirect32,
        0x55 => CmykDirect64,
        0x56 => CmykDirectAlpha40,
        _ => Unknown,
    }
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

    let first_ifd = le_u32(data, 4)? as usize;
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

        let count = match le_u16(data, ifd_offset) {
            Some(count) => count as usize,
            None => break,
        };

        for i in 0..count {
            let Some(entry) = ifd_offset.checked_add(2 + i * 12) else { break };
            let Some(tag) = le_u16(data, entry) else { break };
            let Some(kind) = le_u16(data, entry + 2) else { break };
            let Some(items) = le_u32(data, entry + 4) else { break };
            let Some(value) = le_u32(data, entry + 8) else { break };

            let byte_len = entry_type_size(kind).saturating_mul(items as usize);
            let inline = byte_len <= 4;
            let payload_at = if inline { entry + 8 } else { value as usize };

            match tag {
                TAG_PIXEL_FORMAT => {
                    if let Some(guid) = payload_at.checked_add(16).and_then(|end| data.get(payload_at..end)) {
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
                    if let Some(profile) = payload_at.checked_add(byte_len).and_then(|end| data.get(payload_at..end)) {
                        container.icc_profile = Some(profile.to_vec());
                    }
                }
                _ => {}
            }
        }

        ifd_offset = match ifd_offset.checked_add(2 + count * 12).and_then(|at| le_u32(data, at)) {
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

pub const MAX_BOX_DEPTH: usize = 16;

pub fn is_superbox(kind: &[u8]) -> bool {
    matches!(kind, b"jp2h" | b"jpch" | b"jplh" | b"res " | b"asoc")
}

pub struct JpxBox<'a> {
    pub start: usize,
    pub kind: &'a [u8],
    pub payload_start: usize,
    pub payload: &'a [u8],
}

pub fn jpx_boxes(data: &[u8]) -> Vec<JpxBox<'_>> {
    let mut boxes = Vec::new();
    let mut pos = 0usize;

    while pos + 8 <= data.len() {
        let Some(len) = data.get(pos..pos + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize) else {
            break;
        };
        let kind = &data[pos + 4..pos + 8];

        let (payload_start, payload_end) = match len {
            0 => (pos + 8, data.len()),
            1 => {
                let Some(ext) = data.get(pos + 8..pos + 16) else { break };
                let ext = u64::from_be_bytes(ext.try_into().unwrap_or([0; 8]));
                let end = usize::try_from(ext).ok().and_then(|ext| pos.checked_add(ext)).unwrap_or(usize::MAX);
                (pos + 16, end.max(pos + 16).min(data.len()))
            }
            2..=7 => {
                log_warn!("JPEG XR: JPX box at {} has invalid length {}", pos, len);
                break;
            }
            _ => (pos + 8, pos.saturating_add(len).min(data.len())),
        };

        boxes.push(JpxBox { start: pos, kind, payload_start, payload: &data[payload_start..payload_end] });
        pos = payload_end;
    }

    boxes
}

fn collect_jpx_codestreams(data: &[u8], base: usize, depth: usize, out: &mut Vec<(usize, usize)>, opacity: &mut bool) {
    for item in jpx_boxes(data) {
        match item.kind {
            b"jp2c" | b"jxrc" => match find_codestream(item.payload) {
                Some(rel) => out.push((base + item.payload_start + rel, item.payload.len() - rel)),
                None => {
                    log_warn!("JPEG XR: JPX codestream box at {} has no WMPHOTO signature", base + item.payload_start)
                }
            },
            b"cdef" => {
                let count = item.payload.get(..2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as usize);
                *opacity |= item
                    .payload
                    .get(2..)
                    .unwrap_or_default()
                    .chunks_exact(6)
                    .take(count)
                    .any(|entry| matches!(u16::from_be_bytes([entry[2], entry[3]]), 1 | 2));
            }
            b"jp2h" | b"jpch" | b"jplh" | b"res " | b"asoc" if depth < MAX_BOX_DEPTH => {
                collect_jpx_codestreams(item.payload, base + item.payload_start, depth + 1, out, opacity);
            }
            _ => {}
        }
    }
}

pub fn collect_jpx_metadata(data: &[u8], depth: usize, metadata: &mut MetadataCollector) {
    for item in jpx_boxes(data) {
        match item.kind {
            b"jp2h" | b"jpch" | b"jplh" | b"res " | b"asoc" if depth < MAX_BOX_DEPTH => {
                collect_jpx_metadata(item.payload, depth + 1, metadata);
            }
            b"colr" => match item.payload.first() {
                Some(2 | 3) => metadata.add_icc(item.payload.get(3..).unwrap_or_default()),
                Some(5) => {
                    if let Some(cicp) = jp2::parse_cicp(item.payload.get(3..).unwrap_or_default()) {
                        metadata.set_cicp(cicp);
                    }
                }
                _ => {}
            },
            kind => jp2::collect_box(kind, item.payload, metadata),
        }
    }
}

pub fn parse_jpx_boxes(data: &[u8]) -> Option<Container> {
    let mut codestreams = Vec::new();
    let mut opacity = false;
    collect_jpx_codestreams(data, 0, 0, &mut codestreams, &mut opacity);

    let &(image_offset, image_byte_count) = codestreams.first()?;
    let mut container = Container { kind: ContainerKind::Jpx, image_offset, image_byte_count, ..Default::default() };

    if let Some(&(alpha_offset, alpha_byte_count)) = codestreams.get(1) {
        if opacity {
            container.alpha_offset = alpha_offset;
            container.alpha_byte_count = alpha_byte_count;
        } else {
            log_warn!(
                "JPEG XR: JPX file has {} codestreams but no opacity channel, using the first",
                codestreams.len()
            );
        }
    }

    Some(container)
}
