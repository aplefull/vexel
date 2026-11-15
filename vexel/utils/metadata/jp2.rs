use super::{xmp, Cicp, MetadataCollector};
use crate::log_warn;

const XMP_UUID: [u8; 16] =
    [0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF, 0xAC];
const EXIF_UUID: [u8; 16] = *b"JpgTiffExif->JP2";
const PHOTOSHOP_EXIF_UUID: [u8; 16] =
    [0x05, 0x37, 0xCD, 0xAB, 0x9D, 0x0C, 0x44, 0x31, 0xA7, 0x2A, 0xFA, 0x56, 0x1F, 0x2A, 0x11, 0x3E];
const PHOTOSHOP_IPTC_UUID: [u8; 16] =
    [0x09, 0xA1, 0x4E, 0x97, 0xC0, 0xB4, 0x42, 0xE0, 0xBE, 0xBF, 0x36, 0xDF, 0x6F, 0x0C, 0xE3, 0x6F];
const PHOTOSHOP_UUID: [u8; 16] =
    [0x2C, 0x4C, 0x01, 0x00, 0x85, 0x04, 0x40, 0xB9, 0xA0, 0x3E, 0x56, 0x21, 0x48, 0xD6, 0xDF, 0xEB];
const IPTC_UUID: [u8; 16] =
    [0x33, 0xC7, 0xA4, 0xD2, 0xB8, 0x1D, 0x47, 0x23, 0xA0, 0xBA, 0xF1, 0xA3, 0xE0, 0x97, 0xAD, 0x38];
const GEOJP2_UUID: [u8; 16] =
    [0xB1, 0x4B, 0xF8, 0xBD, 0x08, 0x3D, 0x4B, 0x43, 0xA5, 0xAE, 0x8C, 0xD7, 0xD5, 0xA6, 0xCE, 0x03];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UuidKind {
    Xmp,
    Exif,
    Iptc,
    Photoshop,
    GeoJp2,
}

impl UuidKind {
    pub fn name(self) -> &'static str {
        match self {
            UuidKind::Xmp => "XMP",
            UuidKind::Exif => "Exif",
            UuidKind::Iptc => "IPTC",
            UuidKind::Photoshop => "Photoshop image resources",
            UuidKind::GeoJp2 => "GeoJP2",
        }
    }
}

pub fn uuid_kind(body: &[u8]) -> Option<UuidKind> {
    let uuid: [u8; 16] = body.get(..16)?.try_into().ok()?;
    match uuid {
        XMP_UUID => Some(UuidKind::Xmp),
        EXIF_UUID | PHOTOSHOP_EXIF_UUID => Some(UuidKind::Exif),
        IPTC_UUID | PHOTOSHOP_IPTC_UUID => Some(UuidKind::Iptc),
        PHOTOSHOP_UUID => Some(UuidKind::Photoshop),
        GEOJP2_UUID => Some(UuidKind::GeoJp2),
        _ => None,
    }
}

pub fn parse_cicp(body: &[u8]) -> Option<Cicp> {
    let Some(fields) = body.get(..7) else {
        log_warn!("Parameterized colr box is {} bytes instead of 7, ignoring", body.len());
        return None;
    };
    Some(Cicp {
        color_primaries: u16::from_be_bytes([fields[0], fields[1]]),
        transfer_characteristics: u16::from_be_bytes([fields[2], fields[3]]),
        matrix_coefficients: u16::from_be_bytes([fields[4], fields[5]]),
        full_range: fields[6] & 0x80 != 0,
    })
}

pub fn collect_box(box_type: &[u8], body: &[u8], metadata: &mut MetadataCollector) {
    match box_type {
        b"uuid" => match uuid_kind(body) {
            Some(UuidKind::Xmp) => metadata.add_xmp(&body[16..]),
            Some(UuidKind::Exif) => metadata.add_exif(&body[16..]),
            Some(UuidKind::Iptc) => metadata.add_iptc(&body[16..]),
            Some(UuidKind::Photoshop) => metadata.add_photoshop_resources(&body[16..]),
            Some(UuidKind::GeoJp2) | None => {}
        },
        b"xml " if xmp::looks_like_xmp(body) => metadata.add_fallback_xmp(body),
        _ => {}
    }
}
