use serde::Serialize;
use tsify::Tsify;

pub const IPTC_RESOURCE: u16 = 0x0404;
pub const ICC_RESOURCE: u16 = 0x040F;
pub const EXIF_RESOURCE: u16 = 0x0422;
pub const XMP_RESOURCE: u16 = 0x0424;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PhotoshopResource {
    pub signature: String,
    pub id: u16,
    pub name: String,
    pub size: u32,
}

pub struct ResourceBlock<'a> {
    pub resource: PhotoshopResource,
    pub data: &'a [u8],
}

fn printable_ascii(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }).collect()
}

fn u16_at(body: &[u8], at: usize) -> u16 {
    let byte = |i: usize| body.get(i).copied().unwrap_or(0);
    u16::from_be_bytes([byte(at), byte(at.saturating_add(1))])
}

fn u32_at(body: &[u8], at: usize) -> u32 {
    let byte = |i: usize| body.get(i).copied().unwrap_or(0);
    u32::from_be_bytes([byte(at), byte(at.saturating_add(1)), byte(at.saturating_add(2)), byte(at.saturating_add(3))])
}

pub fn parse_resources(body: &[u8]) -> Vec<ResourceBlock<'_>> {
    let mut blocks = Vec::new();
    let mut pos = 0usize;

    while body.len().saturating_sub(pos) >= 4 {
        let signature = &body[pos..pos + 4];
        if !matches!(signature, b"8BIM" | b"PHUT" | b"AgHg" | b"DCSR" | b"MeSa") {
            break;
        }
        let id = u16_at(body, pos + 4);
        let name_length = usize::from(body.get(pos + 6).copied().unwrap_or(0));
        let name_start = (pos + 7).min(body.len());
        let name_end = (name_start + name_length).min(body.len());
        let name = String::from_utf8_lossy(&body[name_start..name_end]).into_owned();
        let size_at = pos + 6 + ((1 + name_length + 1) & !1);
        let size = u32_at(body, size_at);

        let data_start = (size_at + 4).min(body.len());
        let data_end = data_start.saturating_add(size as usize).min(body.len());
        blocks.push(ResourceBlock {
            resource: PhotoshopResource { signature: printable_ascii(signature), id, name, size },
            data: &body[data_start..data_end],
        });

        let padded = (size as usize).saturating_add(size as usize & 1);
        pos = (size_at + 4).saturating_add(padded);
    }

    blocks
}

pub fn resource_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x03E9 => "Print info",
        0x03ED => "Resolution info",
        0x03F3 => "Print flags",
        0x03F5 => "Color halftoning",
        0x03F8 => "Color transfer functions",
        0x0400 => "Layer state",
        0x0402 => "Layer groups",
        IPTC_RESOURCE => "IPTC-NAA record",
        0x0406 => "JPEG quality",
        0x0408 => "Grid and guides",
        0x0409 | 0x040C => "Thumbnail",
        0x040A => "Copyright flag",
        0x040B => "URL",
        0x040D => "Global angle",
        ICC_RESOURCE => "ICC profile",
        0x0411 => "ICC untagged",
        0x0414 => "Document IDs seed",
        0x0419 => "Global altitude",
        0x041A => "Slices",
        0x041E => "URL list",
        0x0421 => "Version info",
        EXIF_RESOURCE => "Exif data",
        0x0423 => "Exif data 3",
        XMP_RESOURCE => "XMP metadata",
        0x0425 => "IPTC digest",
        0x0426 => "Print scale",
        0x0428 => "Pixel aspect ratio",
        0x0429 => "Layer comps",
        0x2710 => "Print flags info",
        _ => return None,
    })
}
