use std::fmt;

use serde::Serialize;

use crate::log_warn;

const TAG_MARKER: u8 = 0x1C;
const MAX_DATASETS: usize = 4096;
const UTF8_ESCAPE: &[u8] = b"\x1b%G";

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum IptcValue {
    Text(String),
    Number(u32),
    Binary(u32),
}

impl fmt::Display for IptcValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IptcValue::Text(text) => f.write_str(text),
            IptcValue::Number(value) => write!(f, "{}", value),
            IptcValue::Binary(length) => write!(f, "{} bytes", length),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(missing_as_null))]
pub struct IptcDataset {
    pub record: u8,
    pub dataset: u8,
    pub name: Option<String>,
    pub value: IptcValue,
}

impl IptcDataset {
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => format!("{}:{}", self.record, self.dataset),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct IptcData {
    pub datasets: Vec<IptcDataset>,
}

impl IptcData {
    pub fn parse(data: &[u8]) -> Option<IptcData> {
        let mut datasets = Vec::new();
        let mut pos = 0usize;

        while pos < data.len() {
            if data[pos] != TAG_MARKER {
                if data[pos..].iter().all(|&b| b == 0) {
                    break;
                }
                log_warn!("IPTC: expected a tag marker at offset {}, found 0x{:02X}, stopping", pos, data[pos]);
                break;
            }
            if datasets.len() >= MAX_DATASETS {
                log_warn!("IPTC: more than {} datasets, ignoring the rest", MAX_DATASETS);
                break;
            }
            let Some(header) = data.get(pos + 1..pos + 5) else {
                log_warn!("IPTC: truncated dataset header at offset {}", pos);
                break;
            };
            let record = header[0];
            let dataset = header[1];
            let declared = u16::from_be_bytes([header[2], header[3]]);
            let mut value_start = pos + 5;

            let length = if declared & 0x8000 != 0 {
                let size_bytes = usize::from(declared & 0x7FFF);
                let Some(size) = data.get(value_start..value_start.saturating_add(size_bytes)) else {
                    log_warn!("IPTC: truncated extended length at offset {}", pos);
                    break;
                };
                value_start += size_bytes;
                size.iter().fold(0usize, |acc, &b| acc.saturating_mul(256).saturating_add(usize::from(b)))
            } else {
                usize::from(declared)
            };

            let value_end = value_start.saturating_add(length);
            if value_end > data.len() {
                log_warn!(
                    "IPTC: dataset {}:{} is truncated ({} of {} bytes)",
                    record,
                    dataset,
                    data.len() - value_start.min(data.len()),
                    length
                );
            }
            let value = &data[value_start.min(data.len())..value_end.min(data.len())];

            datasets.push(IptcDataset {
                record,
                dataset,
                name: dataset_name(record, dataset).map(str::to_string),
                value: parse_value(record, dataset, value),
            });
            pos = value_end;
        }

        (!datasets.is_empty()).then_some(IptcData { datasets })
    }

    pub fn find(&self, record: u8, dataset: u8) -> impl Iterator<Item = &IptcDataset> {
        self.datasets.iter().filter(move |entry| entry.record == record && entry.dataset == dataset)
    }
}

fn parse_value(record: u8, dataset: u8, value: &[u8]) -> IptcValue {
    let numeric = matches!(
        (record, dataset),
        (1, 0) | (1, 20) | (1, 22) | (1, 120) | (1, 122) | (2, 0) | (2, 200) | (2, 201) | (7, _) | (9, _)
    );
    let binary = matches!((record, dataset), (2, 202) | (8, _));

    if numeric && value.len() <= 4 {
        return IptcValue::Number(value.iter().fold(0u32, |acc, &b| (acc << 8) | u32::from(b)));
    }
    if binary || numeric {
        return IptcValue::Binary(value.len() as u32);
    }
    if (record, dataset) == (1, 90) && value == UTF8_ESCAPE {
        return IptcValue::Text("UTF-8 (ESC % G)".to_string());
    }

    let text = match std::str::from_utf8(value) {
        Ok(text) => text.to_string(),
        Err(_) => value.iter().map(|&b| b as char).collect(),
    };
    IptcValue::Text(text.trim_end_matches('\0').to_string())
}

fn dataset_name(record: u8, dataset: u8) -> Option<&'static str> {
    Some(match (record, dataset) {
        (1, 0) => "EnvelopeRecordVersion",
        (1, 5) => "Destination",
        (1, 20) => "FileFormat",
        (1, 22) => "FileVersion",
        (1, 30) => "ServiceIdentifier",
        (1, 40) => "EnvelopeNumber",
        (1, 50) => "ProductID",
        (1, 60) => "EnvelopePriority",
        (1, 70) => "DateSent",
        (1, 80) => "TimeSent",
        (1, 90) => "CodedCharacterSet",
        (1, 100) => "UniqueObjectName",
        (1, 120) => "ARMIdentifier",
        (1, 122) => "ARMVersion",
        (2, 0) => "ApplicationRecordVersion",
        (2, 3) => "ObjectTypeReference",
        (2, 4) => "ObjectAttributeReference",
        (2, 5) => "ObjectName",
        (2, 7) => "EditStatus",
        (2, 8) => "EditorialUpdate",
        (2, 10) => "Urgency",
        (2, 12) => "SubjectReference",
        (2, 15) => "Category",
        (2, 20) => "SupplementalCategories",
        (2, 22) => "FixtureIdentifier",
        (2, 25) => "Keywords",
        (2, 26) => "ContentLocationCode",
        (2, 27) => "ContentLocationName",
        (2, 30) => "ReleaseDate",
        (2, 35) => "ReleaseTime",
        (2, 37) => "ExpirationDate",
        (2, 38) => "ExpirationTime",
        (2, 40) => "SpecialInstructions",
        (2, 42) => "ActionAdvised",
        (2, 45) => "ReferenceService",
        (2, 47) => "ReferenceDate",
        (2, 50) => "ReferenceNumber",
        (2, 55) => "DateCreated",
        (2, 60) => "TimeCreated",
        (2, 62) => "DigitalCreationDate",
        (2, 63) => "DigitalCreationTime",
        (2, 65) => "OriginatingProgram",
        (2, 70) => "ProgramVersion",
        (2, 75) => "ObjectCycle",
        (2, 80) => "By-line",
        (2, 85) => "By-lineTitle",
        (2, 90) => "City",
        (2, 92) => "Sub-location",
        (2, 95) => "Province-State",
        (2, 100) => "Country-PrimaryLocationCode",
        (2, 101) => "Country-PrimaryLocationName",
        (2, 103) => "OriginalTransmissionReference",
        (2, 105) => "Headline",
        (2, 110) => "Credit",
        (2, 115) => "Source",
        (2, 116) => "CopyrightNotice",
        (2, 118) => "Contact",
        (2, 120) => "Caption-Abstract",
        (2, 121) => "LocalCaption",
        (2, 122) => "Writer-Editor",
        (2, 125) => "RasterizedCaption",
        (2, 130) => "ImageType",
        (2, 131) => "ImageOrientation",
        (2, 135) => "LanguageIdentifier",
        (2, 150) => "AudioType",
        (2, 151) => "AudioSamplingRate",
        (2, 152) => "AudioSamplingResolution",
        (2, 153) => "AudioDuration",
        (2, 154) => "AudioOutcue",
        (2, 184) => "JobID",
        (2, 187) => "MasterDocumentID",
        (2, 188) => "ShortDocumentID",
        (2, 189) => "UniqueDocumentID",
        (2, 190) => "OwnerID",
        (2, 200) => "ObjectPreviewFileFormat",
        (2, 201) => "ObjectPreviewFileVersion",
        (2, 202) => "ObjectPreviewData",
        (2, 221) => "Prefs",
        (2, 225) => "ClassifyState",
        (2, 228) => "SimilarityIndex",
        (2, 230) => "DocumentNotes",
        (2, 231) => "DocumentHistory",
        (2, 232) => "ExifCameraInfo",
        (2, 255) => "CatalogSets",
        (7, 10) => "SizeMode",
        (7, 20) => "MaxSubfileSize",
        (7, 90) => "ObjectSizeAnnounced",
        (7, 95) => "MaximumObjectSize",
        (8, 10) => "SubFile",
        (9, 10) => "ConfirmedObjectSize",
        _ => return None,
    })
}
