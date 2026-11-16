use std::io::{Read, Seek, SeekFrom};

use crate::log_warn;
use crate::utils::exif::{tag_name, ExifData, ExifEntry, ExifIfd, IfdType};
use crate::utils::limits::Limits;
use crate::utils::metadata::MetadataCollector;
use crate::utils::stream::StreamReader;
use crate::utils::tiff::ifd::{type_size, Ifd};
use crate::utils::tiff::info::{IfdKind, TiffIfdSection, TiffSection, TiffStructure};
use crate::utils::tiff::types::TiffTags;

const MAX_BLOB_BYTES: u64 = 64 << 20;

pub fn collect_metadata<R: Read + Seek>(
    structure: &TiffStructure,
    ifd_index: u32,
    ifd: &Ifd,
    reader: &mut StreamReader<R>,
    limits: &mut Limits,
    metadata: &mut MetadataCollector,
) {
    if let Some(exif) = exif_data(structure, ifd_index, ifd) {
        metadata.add_exif_data(&exif);
    }
    if let Some(xmp) = read_blob(ifd, TiffTags::XMP, reader, limits) {
        metadata.add_xmp(&xmp);
    }
    if let Some(iptc) = read_blob(ifd, TiffTags::IPTCNAA, reader, limits) {
        metadata.add_iptc(&iptc);
    }
    if let Some(resources) = read_blob(ifd, TiffTags::Photoshop, reader, limits) {
        metadata.add_photoshop_resources(&resources);
    }
}

fn read_blob<R: Read + Seek>(
    ifd: &Ifd,
    tag: TiffTags,
    reader: &mut StreamReader<R>,
    limits: &mut Limits,
) -> Option<Vec<u8>> {
    let entry = ifd.entry(tag)?;
    let total = entry.count.saturating_mul(type_size(entry.field_type).unwrap_or(1) as u64);
    if total == 0 {
        return None;
    }
    if total <= ifd.format.inline_capacity() as u64 {
        return Some(ifd.inline_bytes(entry)[..total as usize].to_vec());
    }

    if total > MAX_BLOB_BYTES {
        log_warn!("TIFF tag {} holds {} bytes, reading the first {}", entry.tag, total, MAX_BLOB_BYTES);
    }
    reader.seek(SeekFrom::Start(entry.value_offset)).ok()?;
    let available = reader.bytes_left().unwrap_or(0);
    let length = total.min(MAX_BLOB_BYTES).min(available);
    if length < total.min(MAX_BLOB_BYTES) {
        log_warn!("TIFF tag {} is truncated: {} of {} bytes present", entry.tag, length, total);
    }
    if length == 0 {
        return None;
    }

    let length = length as usize;
    if let Err(e) = limits.reserve_usize(length) {
        log_warn!("Skipping TIFF tag {}: {}", entry.tag, e);
        return None;
    }
    let data = reader.read_bytes(length);
    limits.free_usize(length);
    data.ok()
}

fn child_ifd(structure: &TiffStructure, parent: u32, kind: IfdKind) -> Option<&TiffIfdSection> {
    structure.sections.iter().find_map(|section| match &section.data {
        TiffSection::Ifd(ifd) if ifd.kind == kind && ifd.parent == Some(parent) => Some(ifd),
        _ => None,
    })
}

fn convert(section: &TiffIfdSection, ifd_type: IfdType) -> ExifIfd {
    let entries = section
        .entries
        .iter()
        .filter_map(|entry| {
            Some(ExifEntry {
                tag: entry.tag,
                tag_name: tag_name(entry.tag, ifd_type).map(str::to_string),
                value: entry.value.clone()?,
            })
        })
        .collect();
    ExifIfd { entries }
}

fn exif_data(structure: &TiffStructure, ifd_index: u32, ifd: &Ifd) -> Option<ExifData> {
    let exif = child_ifd(structure, ifd_index, IfdKind::Exif);
    let gps = child_ifd(structure, ifd_index, IfdKind::Gps);
    if exif.is_none() && gps.is_none() {
        return None;
    }

    let interop = exif
        .and_then(|exif| child_ifd(structure, exif.index, IfdKind::Interop))
        .or_else(|| child_ifd(structure, ifd_index, IfdKind::Interop));
    let ifd0 = structure.ifd_section(ifd_index)?;

    Some(ExifData {
        byte_order: ifd.byte_order,
        ifd0: convert(ifd0, IfdType::Ifd0),
        exif_ifd: exif.map(|section| convert(section, IfdType::ExifIfd)),
        gps_ifd: gps.map(|section| convert(section, IfdType::GpsIfd)),
        interop_ifd: interop.map(|section| convert(section, IfdType::InteropIfd)),
        ifd1: None,
    })
}
