mod tags;
mod types;

use std::collections::HashMap;

pub use types::*;

use crate::log_warn;
use crate::utils::bytes::{be_u16, be_u32};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::float::half_to_f32;

pub(crate) fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

pub(crate) fn read_s15_fixed16(data: &[u8], offset: usize) -> Option<f64> {
    let raw = be_u32(data, offset)? as i32;
    Some(raw as f64 / 65536.0)
}

pub(crate) fn read_f32_be(data: &[u8], offset: usize) -> Option<f32> {
    Some(f32::from_bits(be_u32(data, offset)?))
}

pub(crate) fn read_f16_be(data: &[u8], offset: usize) -> Option<f32> {
    Some(half_to_f32(be_u16(data, offset)?))
}

pub(crate) fn read_spectral_range(data: &[u8], offset: usize) -> Option<SpectralRange> {
    Some(SpectralRange {
        start: read_f16_be(data, offset)?,
        end: read_f16_be(data, offset.checked_add(2)?)?,
        steps: be_u16(data, offset.checked_add(4)?)?,
    })
}

pub(crate) fn sig_to_string(bytes: &[u8]) -> String {
    if bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        return String::from_utf8_lossy(bytes).to_string();
    }
    bytes
        .iter()
        .map(|&b| if (0x20..=0x7E).contains(&b) { (b as char).to_string() } else { format!("\\x{:02X}", b) })
        .collect()
}

pub(crate) fn sig_at(data: &[u8], offset: usize) -> String {
    match offset.checked_add(4).and_then(|end| data.get(offset..end)) {
        Some(bytes) => sig_to_string(bytes),
        None => String::new(),
    }
}

pub(crate) fn decode_utf16_be(data: &[u8]) -> String {
    let words: Vec<u16> = data.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&words).trim_end_matches('\0').to_string()
}

pub(crate) fn parse_xyz_number(data: &[u8], offset: usize) -> Option<XYZNumber> {
    Some(XYZNumber {
        x: read_s15_fixed16(data, offset)?,
        y: read_s15_fixed16(data, offset.checked_add(4)?)?,
        z: read_s15_fixed16(data, offset.checked_add(8)?)?,
    })
}

fn decode_rendering_intent(value: u32) -> String {
    match value & 0xFFFF {
        0 => "Perceptual".to_string(),
        1 => "Relative Colorimetric".to_string(),
        2 => "Saturation".to_string(),
        3 => "Absolute Colorimetric".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn decode_profile_class(sig: &str) -> String {
    match sig.trim_end_matches('\0').trim() {
        "scnr" => "Input Device Profile".to_string(),
        "mntr" => "Display Device Profile".to_string(),
        "prtr" => "Output Device Profile".to_string(),
        "link" => "DeviceLink Profile".to_string(),
        "abst" => "Abstract Profile".to_string(),
        "spac" => "ColorSpace Profile".to_string(),
        "nmcl" => "Named Color Profile".to_string(),
        "cenc" => "Color Encoding Profile".to_string(),
        "mid " => "Multiplex Identification Profile".to_string(),
        "mlnk" => "Multiplex Link Profile".to_string(),
        "mvis" => "Multiplex Visualization Profile".to_string(),
        _ => sig.to_string(),
    }
}

fn decode_color_space(sig: &str) -> String {
    let trimmed = sig.trim_end_matches('\0').trim_end();
    match trimmed {
        "XYZ" => "XYZ".to_string(),
        "Lab" => "CIE L*a*b*".to_string(),
        "RGB" => "RGB".to_string(),
        "GRAY" => "Grayscale".to_string(),
        "CMYK" => "CMYK".to_string(),
        "CMY" => "CMY".to_string(),
        "HSV" => "HSV".to_string(),
        "HLS" => "HLS".to_string(),
        "YCbr" => "YCbCr".to_string(),
        "Yxy" => "CIE Yxy".to_string(),
        "Luv" => "CIE L*u*v*".to_string(),
        "" => "None".to_string(),
        _ => {
            if let Some(channels) = trimmed.strip_suffix("CLR") {
                if let Ok(n) = u8::from_str_radix(channels, 16) {
                    return format!("{}-channel Color", n);
                }
            }
            sig.to_string()
        }
    }
}

fn decode_platform(sig: &str) -> String {
    match sig.trim_end_matches('\0').trim() {
        "APPL" => "Macintosh".to_string(),
        "MSFT" => "Microsoft".to_string(),
        "SUNW" => "Solaris".to_string(),
        "SGI" => "SGI".to_string(),
        "TGNT" => "Taligent".to_string(),
        "" => "Unspecified".to_string(),
        _ => sig.to_string(),
    }
}

fn decode_spectral_pcs(bytes: &[u8]) -> String {
    if let [a, b, hi, lo] = *bytes {
        let channels = u16::from_be_bytes([hi, lo]);
        let kind = match &[a, b] {
            b"rs" => Some("Reflectance Spectrum"),
            b"ts" => Some("Transmission Spectrum"),
            b"es" => Some("Radiant/Emission Spectrum"),
            b"bs" => Some("Bi-Spectral Reflectance"),
            b"sm" => Some("Sparse Matrix Reflectance"),
            _ => None,
        };
        if let Some(kind) = kind {
            return format!("{} ({} channels)", kind, channels);
        }
    }
    sig_to_string(bytes)
}

const HEADER_SIZE: usize = 128;
const MAX_TAG_COUNT: u32 = 1024;

impl ICCProfile {
    pub(crate) fn new(data: &[u8]) -> VexelResult<ICCProfile> {
        let mut budget = tags::TagBudget::for_profile(data.len());
        Self::parse_with_depth(data, 0, &mut budget)
    }

    fn parse_with_depth(data: &[u8], depth: u8, budget: &mut tags::TagBudget) -> VexelResult<ICCProfile> {
        if data.len() < HEADER_SIZE {
            return Err(VexelError::Custom(format!(
                "ICC profile data too small: {} bytes, at least {} required",
                data.len(),
                HEADER_SIZE
            )));
        }

        let header = ICCProfile::parse_header(data);

        if header.signature != "acsp" {
            log_warn!("ICC profile signature is '{}', expected 'acsp', attempting to parse anyway", header.signature);
        }

        let tag_table = ICCProfile::parse_tag_table(data);
        let tags = ICCProfile::parse_tags(data, &tag_table, depth, budget);

        Ok(ICCProfile { header, tag_table, tags })
    }

    fn parse_header(data: &[u8]) -> ProfileHeader {
        let size = be_u32(data, 0).unwrap_or(0);
        let preferred_cmm_type = sig_at(data, 4);

        let raw_version = be_u32(data, 8).unwrap_or(0);
        let major = (raw_version >> 24) & 0xFF;
        let minor = (raw_version >> 20) & 0x0F;
        let patch = (raw_version >> 16) & 0x0F;
        let version = format!("{}.{}.{}", major, minor, patch);

        let profile_class = decode_profile_class(&sig_at(data, 12));
        let color_space = decode_color_space(&sig_at(data, 16));
        let pcs = decode_color_space(&sig_at(data, 20));

        let creation_date = DateTimeNumber {
            year: be_u16(data, 24).unwrap_or(0),
            month: be_u16(data, 26).unwrap_or(0),
            day: be_u16(data, 28).unwrap_or(0),
            hours: be_u16(data, 30).unwrap_or(0),
            minutes: be_u16(data, 32).unwrap_or(0),
            seconds: be_u16(data, 34).unwrap_or(0),
        };

        let signature = sig_at(data, 36);
        let platform = decode_platform(&sig_at(data, 40));

        let raw_flags = be_u32(data, 44).unwrap_or(0);
        let flags = ProfileFlags {
            raw: raw_flags,
            embedded_profile: (raw_flags & 0x01) != 0,
            use_with_embedded_data_only: (raw_flags & 0x02) != 0,
        };

        let manufacturer = sig_at(data, 48);
        let model = sig_at(data, 52);

        let mut attributes_raw = [0u8; 8];
        attributes_raw.copy_from_slice(&data[56..64]);
        let attr_low = be_u32(data, 60).unwrap_or(0);
        let attributes = DeviceAttributes {
            raw: attributes_raw,
            reflective: (attr_low & 0x01) == 0,
            glossy: (attr_low & 0x02) == 0,
            positive: (attr_low & 0x04) == 0,
            color: (attr_low & 0x08) == 0,
        };

        let rendering_intent = decode_rendering_intent(be_u32(data, 64).unwrap_or(0));

        let illuminant = parse_xyz_number(data, 68).unwrap_or(XYZNumber { x: 0.0, y: 0.0, z: 0.0 });

        let creator = sig_at(data, 80);

        let mut profile_id = [0u8; 16];
        profile_id.copy_from_slice(&data[84..100]);

        let spectral_pcs_raw = be_u32(data, 100).unwrap_or(0);
        let spectral_pcs = if spectral_pcs_raw != 0 { Some(decode_spectral_pcs(&data[100..104])) } else { None };
        let spectral_range = read_spectral_range(data, 104).filter(|r| r.steps != 0);
        let bi_spectral_range = read_spectral_range(data, 110).filter(|r| r.steps != 0);

        let mcs_raw = be_u32(data, 116).unwrap_or(0);
        let mcs = if mcs_raw != 0 { Some(sig_at(data, 116)) } else { None };

        let sub_class_raw = be_u32(data, 120).unwrap_or(0);
        let device_sub_class = if sub_class_raw != 0 { Some(sig_at(data, 120)) } else { None };

        ProfileHeader {
            size,
            preferred_cmm_type,
            version,
            profile_class,
            color_space,
            pcs,
            creation_date,
            signature,
            platform,
            flags,
            manufacturer,
            model,
            attributes,
            rendering_intent,
            illuminant,
            creator,
            profile_id,
            spectral_pcs,
            spectral_range,
            bi_spectral_range,
            mcs,
            device_sub_class,
        }
    }

    fn parse_tag_table(data: &[u8]) -> TagTable {
        let Some(raw_count) = be_u32(data, HEADER_SIZE) else {
            log_warn!("ICC profile has no tag table, returning header-only profile");
            return TagTable { tag_count: 0, entries: Vec::new() };
        };

        let mut tag_count = raw_count;
        if tag_count > MAX_TAG_COUNT {
            log_warn!("ICC tag count {} exceeds limit, clamping to {}", tag_count, MAX_TAG_COUNT);
            tag_count = MAX_TAG_COUNT;
        }

        let max_entries = data.len().saturating_sub(HEADER_SIZE + 4) / 12;
        if (tag_count as usize) > max_entries {
            log_warn!("ICC tag count {} exceeds available data, clamping to {}", tag_count, max_entries);
            tag_count = max_entries as u32;
        }

        let mut entries = Vec::with_capacity(tag_count as usize);
        for i in 0..tag_count as usize {
            let entry_offset = HEADER_SIZE + 4 + i * 12;
            let (Some(signature), Some(offset), Some(size)) = (
                data.get(entry_offset..entry_offset + 4).map(sig_to_string),
                be_u32(data, entry_offset + 4),
                be_u32(data, entry_offset + 8),
            ) else {
                break;
            };
            entries.push(TagEntry { signature, offset, size });
        }

        TagTable { tag_count: raw_count, entries }
    }

    fn parse_tags(
        data: &[u8],
        tag_table: &TagTable,
        depth: u8,
        budget: &mut tags::TagBudget,
    ) -> HashMap<String, ICCTagData> {
        let mut tags: HashMap<String, ICCTagData> = HashMap::new();
        let mut seen_offsets: HashMap<u32, String> = HashMap::new();

        for entry in &tag_table.entries {
            let start = entry.offset as usize;
            let end = start.saturating_add(entry.size as usize);

            if start >= data.len() || end > data.len() || end < start + 4 {
                log_warn!(
                    "ICC tag '{}' has invalid bounds (offset {}, size {}), skipping",
                    entry.signature,
                    entry.offset,
                    entry.size
                );
                continue;
            }

            if let Some(existing_sig) = seen_offsets.get(&entry.offset) {
                if let Some(existing) = tags.get(existing_sig) {
                    if !budget.charge(end - start) {
                        log_warn!("ICC tag '{}' exceeds the profile parse budget, skipping", entry.signature);
                        continue;
                    }
                    let existing = existing.clone();
                    tags.insert(entry.signature.clone(), existing);
                    continue;
                }
            }

            let tag_data = &data[start..end];
            let type_sig = sig_to_string(&tag_data[0..4]);
            let Some(parsed) = tags::parse_tag(&type_sig, tag_data, depth, budget) else {
                log_warn!("ICC tag '{}' exceeds the profile parse budget, skipping", entry.signature);
                continue;
            };
            seen_offsets.insert(entry.offset, entry.signature.clone());
            tags.insert(entry.signature.clone(), parsed);
        }

        tags
    }

    pub fn description(&self) -> Option<String> {
        let text = match self.tags.get("desc")? {
            ICCTagData::TextDescription(desc) => {
                if desc.unicode.trim().is_empty() {
                    desc.ascii.clone()
                } else {
                    desc.unicode.clone()
                }
            }
            ICCTagData::MultiLocalizedUnicode(strings) => strings
                .iter()
                .find(|s| s.language == "en")
                .or_else(|| strings.first())
                .map(|s| s.text.clone())
                .unwrap_or_default(),
            ICCTagData::Text(text) => text.clone(),
            _ => return None,
        };
        let text = text.trim_end_matches('\0').trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}
