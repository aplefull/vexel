mod color;
mod iptc;
pub(crate) mod jp2;
mod orientation;
pub(crate) mod photoshop;
pub(crate) mod xml;
pub(crate) mod xmp;

pub use color::{Cicp, ContentLightLevel, MasteringDisplay};
pub use iptc::{IptcData, IptcDataset, IptcValue};
pub use orientation::Orientation;
use xmp::ExtendedXmpAssembler;
pub use xmp::{XmpArrayKind, XmpData, XmpProperty, XmpValue};

use crate::log_warn;
use crate::utils::exif::{ExifData, ExifReader};
use crate::utils::icc::ICCProfile;
use crate::utils::info::ImageSummary;

const EXIF_ORIENTATION: u16 = 0x0112;

#[derive(Debug, Clone, Default)]
pub(crate) struct MetadataCollector {
    exif: Option<ExifData>,
    xmp: Option<String>,
    fallback_xmp: Option<String>,
    extended_xmp: ExtendedXmpAssembler,
    iptc: Option<IptcData>,
    icc: Option<Vec<u8>>,
    cicp: Option<Cicp>,
    mastering_display: Option<MasteringDisplay>,
    content_light_level: Option<ContentLightLevel>,
    orientation: Option<Orientation>,
}

pub(crate) fn strip_exif_header(data: &[u8]) -> &[u8] {
    match data {
        [b'E', b'x', b'i', b'f', 0, 0, rest @ ..] => rest,
        [b'E', b'x', b'i', b'f', 0, rest @ ..] => rest,
        _ => data,
    }
}

impl MetadataCollector {
    pub fn add_exif(&mut self, data: &[u8]) {
        if self.exif.is_some() {
            return;
        }
        match ExifReader::parse(strip_exif_header(data)) {
            Some(exif) => self.add_exif_data(&exif),
            None => log_warn!("Failed to parse {} bytes of Exif data", data.len()),
        }
    }

    pub fn add_exif_data(&mut self, exif: &ExifData) {
        if self.exif.is_some() {
            return;
        }
        if exif.entry_count() == 0 {
            log_warn!("Ignoring Exif data without any readable entries");
            return;
        }
        self.exif = Some(exif.clone());
    }

    pub fn add_xmp(&mut self, data: &[u8]) {
        if self.xmp.is_some() {
            return;
        }
        self.xmp = xmp::decode_packet(xmp::trim_to_packet(data));
    }

    pub fn add_fallback_xmp(&mut self, data: &[u8]) {
        if self.fallback_xmp.is_none() {
            self.fallback_xmp = xmp::decode_packet(xmp::trim_to_packet(data));
        }
    }

    pub fn add_extended_xmp(&mut self, guid: String, full_length: u32, offset: u32, data: &[u8]) {
        self.extended_xmp.add(guid, full_length, offset, data);
    }

    pub fn add_iptc(&mut self, data: &[u8]) {
        if data.starts_with(b"8BIM") {
            self.add_photoshop_resources(data);
        } else {
            self.add_iim(data);
        }
    }

    fn add_iim(&mut self, data: &[u8]) {
        if self.iptc.is_none() {
            self.iptc = IptcData::parse(data);
        }
    }

    pub fn add_photoshop_resources(&mut self, data: &[u8]) {
        for block in photoshop::parse_resources(data) {
            match block.resource.id {
                photoshop::IPTC_RESOURCE => self.add_iim(block.data),
                photoshop::EXIF_RESOURCE => self.add_exif(block.data),
                photoshop::XMP_RESOURCE => self.add_xmp(block.data),
                photoshop::ICC_RESOURCE => self.add_icc(block.data),
                _ => {}
            }
        }
    }

    pub fn add_icc(&mut self, data: &[u8]) {
        if self.icc.is_none() && !data.is_empty() {
            self.icc = Some(data.to_vec());
        }
    }

    pub fn set_cicp(&mut self, cicp: Cicp) {
        self.cicp.get_or_insert(cicp);
    }

    pub fn set_mastering_display(&mut self, display: MasteringDisplay) {
        self.mastering_display.get_or_insert(display);
    }

    pub fn set_content_light_level(&mut self, level: ContentLightLevel) {
        self.content_light_level.get_or_insert(level);
    }

    pub fn set_orientation(&mut self, value: u32, applied: bool) {
        if self.orientation.is_some() {
            return;
        }
        match Orientation::new(value, applied) {
            Some(orientation) => self.orientation = Some(orientation),
            None => log_warn!("Ignoring invalid orientation value {}", value),
        }
    }

    pub fn apply(&self, summary: &mut ImageSummary) {
        let packet = self.xmp.clone().or_else(|| self.fallback_xmp.clone());
        let extended = (!self.extended_xmp.is_empty()).then(|| self.extended_xmp.assemble(packet.as_deref())).flatten();
        let xmp = match (packet, extended) {
            (Some(packet), extended) => Some(XmpData::new(packet, extended)),
            (None, Some(extended)) => {
                log_warn!("Extended XMP without a main XMP packet");
                Some(XmpData::new(String::new(), Some(extended)))
            }
            (None, None) => None,
        };

        summary.orientation = self.orientation.or_else(|| self.exif_orientation()).or_else(|| {
            let value = xmp.as_ref()?.find(xmp::TIFF_NS, "Orientation")?.as_text()?;
            Orientation::new(value.trim().parse().ok()?, false)
        });

        if summary.icc_profile.is_none() {
            if let Some(icc) = &self.icc {
                match ICCProfile::new(icc) {
                    Ok(profile) => summary.icc_profile = Some(profile),
                    Err(e) => log_warn!("Failed to parse embedded ICC profile: {}", e),
                }
            }
        }

        summary.exif = self.exif.clone();
        summary.xmp = xmp;
        summary.iptc = self.iptc.clone();
        summary.cicp = self.cicp;
        summary.mastering_display = self.mastering_display;
        summary.content_light_level = self.content_light_level;
    }

    fn exif_orientation(&self) -> Option<Orientation> {
        let value = self.exif.as_ref()?.ifd0.number(EXIF_ORIENTATION)?;
        let orientation = Orientation::new(value, false);
        if orientation.is_none() {
            log_warn!("Ignoring invalid Exif orientation value {}", value);
        }
        orientation
    }
}
