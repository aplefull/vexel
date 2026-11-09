use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::decoder::{frame_delay_ms, parse_file, read_pixel_params, ParsedFile};
use super::parser::{looks_like_text, DataSet, Element, ElementValue, UNDEFINED_LENGTH};
use super::types::{dictionary, tag_element, tag_group, tags, uid_name, Photometric, Vr};
use crate::decoders::jpeg::info::subsampling_name;
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, DataExtent, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat};
use crate::Limits;

const PREAMBLE_LENGTH: usize = 128;
const MAX_VALUES: usize = 64;
const MAX_ITEMS: usize = 32;
const MAX_FRAGMENTS: usize = 64;
const SPECIFIC_CHARACTER_SET: u32 = 0x0008_0005;
const RESCALE_TYPE: u32 = 0x0028_1054;
const PIXEL_SPACING: u32 = 0x0028_0030;
const ICC_PROFILE: u32 = 0x0028_2000;

#[tsify::declare]
pub type DicomInfo = FormatInfo<DicomSection>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum DicomVrSource {
    Explicit,
    Dictionary,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DicomItem {
    pub offset: u64,
    pub length: Option<u32>,
    pub elements: Vec<DicomElement>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DicomFragment {
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum DicomValue {
    Empty,
    Text { values: Vec<String>, truncated: bool },
    Uid { uid: String, name: Option<String> },
    Unsigned { values: Vec<u64>, count: u64 },
    Signed { values: Vec<i64>, count: u64 },
    Float { values: Vec<f64>, count: u64 },
    Tags { values: Vec<u32>, count: u64 },
    Binary,
    Sequence { items: Vec<DicomItem>, total_items: u32 },
    Encapsulated { offset_table: Vec<u32>, fragments: Vec<DicomFragment>, total_fragments: u32, total_bytes: u64 },
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct DicomElement {
    pub offset: u64,
    pub tag: u32,
    pub vr: String,
    pub vr_source: DicomVrSource,
    pub length: Option<u32>,
    pub on_disk_length: u64,
    pub value: DicomValue,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct DicomParameters {
    pub transfer_syntax_uid: Option<String>,
    pub transfer_syntax: String,
    pub explicit_vr: bool,
    pub big_endian: bool,
    pub deflated: bool,
    pub encoding_guessed: bool,
    pub explicit_override: bool,
    pub sop_class_uid: Option<String>,
    pub modality: Option<String>,
    pub rows: Option<u16>,
    pub columns: Option<u16>,
    pub samples_per_pixel: Option<u16>,
    pub photometric: Option<String>,
    pub bits_allocated: Option<u16>,
    pub bits_stored: Option<u16>,
    pub high_bit: Option<u16>,
    pub pixel_representation: Option<u16>,
    pub planar_configuration: Option<u16>,
    pub number_of_frames: Option<i64>,
    pub window_center: Vec<f64>,
    pub window_width: Vec<f64>,
    pub rescale_slope: Option<f64>,
    pub rescale_intercept: Option<f64>,
    pub rescale_type: Option<String>,
    pub pixel_spacing: Vec<f64>,
    pub icon_image: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum DicomSection {
    Preamble { zero_filled: bool },
    FileMeta { elements: Vec<DicomElement> },
    Element(Box<DicomElement>),
    DeflatedDataSet { inflated_length: u64, elements: Vec<DicomElement> },
    Parameters(Box<DicomParameters>),
    TrailingData,
}

fn tag_label(tag: u32) -> String {
    let name = match dictionary(tag) {
        Some((name, _)) => name,
        None if tag_group(tag) % 2 == 1 => "Private Tag",
        None => "Unknown Tag",
    };
    format!("({:04X},{:04X}) {}", tag_group(tag), tag_element(tag), name)
}

fn join<T: ToString>(values: &[T], count: u64) -> String {
    let mut text = values.iter().map(ToString::to_string).collect::<Vec<_>>().join("\\");
    if count > values.len() as u64 {
        text.push_str(&format!(" ... ({} values)", count));
    }
    text
}

fn describe_element(element: &DicomElement, out: &mut InfoWriter) {
    out.section(tag_label(element.tag), element.offset, Some(element.on_disk_length), |out| {
        describe_element_body(element, out)
    });
}

fn describe_element_body(element: &DicomElement, out: &mut InfoWriter) {
    match element.vr_source {
        DicomVrSource::Explicit => out.field("VR", &element.vr),
        DicomVrSource::Dictionary => out.field("VR", format!("{} (implicit, from dictionary)", element.vr)),
        DicomVrSource::Unknown => out.field("VR", "implicit, unknown"),
    };
    if element.length.is_none() {
        out.field("Length", format!("undefined, {} bytes on disk", element.on_disk_length));
    }

    match &element.value {
        DicomValue::Empty => {}
        DicomValue::Text { values, truncated } => {
            let mut text = values.join("\\");
            if *truncated {
                text.push_str(" ... (truncated)");
            }
            out.text("Value", &text);
        }
        DicomValue::Uid { uid, name } => {
            match name {
                Some(name) => out.field("Value", format!("{} ({})", uid, name)),
                None => out.field("Value", uid),
            };
        }
        DicomValue::Unsigned { values, count } => {
            out.field("Value", join(values, *count));
        }
        DicomValue::Signed { values, count } => {
            out.field("Value", join(values, *count));
        }
        DicomValue::Float { values, count } => {
            out.field("Value", join(values, *count));
        }
        DicomValue::Tags { values, count } => {
            let tags: Vec<String> =
                values.iter().map(|tag| format!("({:04X},{:04X})", tag_group(*tag), tag_element(*tag))).collect();
            out.field("Value", join(&tags, *count));
        }
        DicomValue::Binary => {
            out.field("Value", "binary");
        }
        DicomValue::Sequence { items, total_items } => {
            out.field("Items", total_items);
            for (index, item) in items.iter().enumerate() {
                let title = match item.length {
                    Some(length) => format!("Item {} ({} bytes)", index + 1, length),
                    None => format!("Item {} (undefined length)", index + 1),
                };
                out.section(title, item.offset, None, |out| {
                    for element in &item.elements {
                        describe_element(element, out);
                    }
                });
            }
            if *total_items as usize > items.len() {
                out.field("More items", *total_items as usize - items.len());
            }
        }
        DicomValue::Encapsulated { offset_table, fragments, total_fragments, total_bytes } => {
            if offset_table.is_empty() {
                out.field("Basic offset table", "empty");
            } else {
                out.field("Basic offset table", join(offset_table, offset_table.len() as u64));
            }
            out.field("Fragments", format!("{}, {} bytes", total_fragments, total_bytes));
            out.group("Fragment list", |out| {
                for (index, fragment) in fragments.iter().enumerate() {
                    out.field(
                        &format!("Fragment {}", index + 1),
                        format!("{} bytes at 0x{:08X}", fragment.length, fragment.offset),
                    );
                }
                if *total_fragments as usize > fragments.len() {
                    out.field("More fragments", *total_fragments as usize - fragments.len());
                }
            });
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn describe_parameters(parameters: &DicomParameters, out: &mut InfoWriter) {
    match &parameters.transfer_syntax_uid {
        Some(uid) => out.field("Transfer syntax", format!("{} ({})", uid, parameters.transfer_syntax)),
        None => out.field("Transfer syntax", format!("not declared, using {}", parameters.transfer_syntax)),
    };
    let mut encoding = vec![
        if parameters.explicit_vr { "explicit VR" } else { "implicit VR" },
        if parameters.big_endian { "big endian" } else { "little endian" },
    ];
    if parameters.deflated {
        encoding.push("deflated, offsets in the inflated stream");
    }
    if parameters.encoding_guessed {
        encoding.push("guessed from data");
    }
    if parameters.explicit_override {
        encoding.push("switched to explicit VR to match data");
    }
    out.field("Encoding", encoding.join(", "));
    if let Some(uid) = &parameters.sop_class_uid {
        match uid_name(uid) {
            Some(name) => out.field("SOP class", format!("{} ({})", uid, name)),
            None => out.field("SOP class", uid),
        };
    }
    out.opt_field("Modality", parameters.modality.as_deref());
    out.opt_field("Rows", parameters.rows);
    out.opt_field("Columns", parameters.columns);
    out.opt_field("Samples per pixel", parameters.samples_per_pixel);
    out.opt_field("Photometric interpretation", parameters.photometric.as_deref());
    out.opt_field("Bits allocated", parameters.bits_allocated);
    out.opt_field("Bits stored", parameters.bits_stored);
    out.opt_field("High bit", parameters.high_bit);
    out.opt_field(
        "Pixel representation",
        parameters.pixel_representation.map(|value| match value {
            0 => format!("unsigned ({})", value),
            1 => format!("signed, two's complement ({})", value),
            _ => format!("unknown ({})", value),
        }),
    );
    out.opt_field(
        "Planar configuration",
        parameters.planar_configuration.map(|value| match value {
            0 => format!("interleaved ({})", value),
            1 => format!("planar ({})", value),
            _ => format!("unknown ({})", value),
        }),
    );
    out.opt_field("Number of frames", parameters.number_of_frames);
    if !parameters.window_center.is_empty() {
        out.field("Window center", join(&parameters.window_center, parameters.window_center.len() as u64));
    }
    if !parameters.window_width.is_empty() {
        out.field("Window width", join(&parameters.window_width, parameters.window_width.len() as u64));
    }
    out.opt_field("Rescale slope", parameters.rescale_slope);
    out.opt_field("Rescale intercept", parameters.rescale_intercept);
    out.opt_field("Rescale type", parameters.rescale_type.as_deref());
    if !parameters.pixel_spacing.is_empty() {
        out.field("Pixel spacing", join(&parameters.pixel_spacing, parameters.pixel_spacing.len() as u64));
    }
    out.field("Icon image", yes_no(parameters.icon_image));
}

impl InfoSection for DicomSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            DicomSection::Preamble { .. } => Cow::Borrowed("Preamble and DICM Prefix"),
            DicomSection::FileMeta { .. } => Cow::Borrowed("File Meta Information"),
            DicomSection::Element(element) => Cow::Owned(tag_label(element.tag)),
            DicomSection::DeflatedDataSet { .. } => Cow::Borrowed("Deflated Data Set"),
            DicomSection::Parameters(_) => Cow::Borrowed("Image Parameters"),
            DicomSection::TrailingData => Cow::Borrowed("Trailing Data"),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            DicomSection::Preamble { zero_filled } => {
                out.field("Preamble", if *zero_filled { "zero-filled" } else { "application data" });
            }
            DicomSection::FileMeta { elements } => {
                for element in elements {
                    describe_element(element, out);
                }
            }
            DicomSection::Element(element) => describe_element_body(element, out),
            DicomSection::DeflatedDataSet { inflated_length, elements } => {
                out.field("Inflated length", format!("{} bytes", inflated_length));
                for element in elements {
                    describe_element(element, out);
                }
            }
            DicomSection::Parameters(parameters) => describe_parameters(parameters, out),
            DicomSection::TrailingData => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Charset {
    Latin1,
    Utf8,
}

fn charset(dataset: &DataSet) -> Charset {
    match dataset.string(SPECIFIC_CHARACTER_SET) {
        Some(value) if value.split('\\').any(|term| term.trim() == "ISO_IR 192") => Charset::Utf8,
        _ => Charset::Latin1,
    }
}

fn decode_text(bytes: &[u8], charset: Charset) -> String {
    let text = match charset {
        Charset::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Charset::Latin1 => bytes.iter().map(|&byte| byte as char).collect(),
    };
    text.chars()
        .map(|c| if c.is_control() && !matches!(c, '\0' | '\n' | '\r' | '\t' | '\x1B') { '\u{FFFD}' } else { c })
        .collect()
}

fn is_multi_valued(vr: &str) -> bool {
    matches!(vr, "AE" | "AS" | "CS" | "DA" | "DS" | "DT" | "IS" | "LO" | "PN" | "SH" | "TM" | "UC" | "UI")
}

fn is_text(vr: &str) -> bool {
    is_multi_valued(vr) || matches!(vr, "LT" | "ST" | "UT" | "UR")
}

fn numbers<T>(bytes: &[u8], unit: usize, read: impl Fn(&[u8]) -> T) -> (Vec<T>, u64) {
    let count = (bytes.len() / unit) as u64;
    (bytes.chunks_exact(unit).take(MAX_VALUES).map(read).collect(), count)
}

fn convert_value(element: &Element, vr: &str, bytes: &[u8], charset: Charset) -> DicomValue {
    if bytes.is_empty() {
        return DicomValue::Empty;
    }
    let declared = element.length as usize;
    let truncated = element.length != UNDEFINED_LENGTH && bytes.len() < declared;
    let scaled = |count: u64, unit: usize| {
        if truncated {
            (declared / unit) as u64
        } else {
            count
        }
    };

    match vr {
        "UI" => {
            let uid = decode_text(bytes, charset).trim_matches(['\0', ' ']).to_string();
            let name = uid_name(&uid).map(str::to_string);
            DicomValue::Uid { uid, name }
        }
        vr if is_text(vr) => {
            let text = decode_text(bytes, charset);
            let values = if is_multi_valued(vr) {
                text.split('\\').map(|value| value.trim_matches(['\0', ' ']).to_string()).collect()
            } else {
                vec![text.trim_end_matches(['\0', ' ']).to_string()]
            };
            DicomValue::Text { values, truncated }
        }
        "US" => {
            let (values, count) = numbers(bytes, 2, |c| u16::from_le_bytes([c[0], c[1]]) as u64);
            DicomValue::Unsigned { values, count: scaled(count, 2) }
        }
        "UL" => {
            let (values, count) = numbers(bytes, 4, |c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as u64);
            DicomValue::Unsigned { values, count: scaled(count, 4) }
        }
        "UV" => {
            let (values, count) =
                numbers(bytes, 8, |c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
            DicomValue::Unsigned { values, count: scaled(count, 8) }
        }
        "SS" => {
            let (values, count) = numbers(bytes, 2, |c| i16::from_le_bytes([c[0], c[1]]) as i64);
            DicomValue::Signed { values, count: scaled(count, 2) }
        }
        "SL" => {
            let (values, count) = numbers(bytes, 4, |c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64);
            DicomValue::Signed { values, count: scaled(count, 4) }
        }
        "SV" => {
            let (values, count) =
                numbers(bytes, 8, |c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
            DicomValue::Signed { values, count: scaled(count, 8) }
        }
        "FL" => {
            let (values, count) = numbers(bytes, 4, |c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64);
            DicomValue::Float { values, count: scaled(count, 4) }
        }
        "FD" => {
            let (values, count) =
                numbers(bytes, 8, |c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
            DicomValue::Float { values, count: scaled(count, 8) }
        }
        "AT" => {
            let (values, count) = numbers(bytes, 4, |c| {
                ((u16::from_le_bytes([c[0], c[1]]) as u32) << 16) | u16::from_le_bytes([c[2], c[3]]) as u32
            });
            DicomValue::Tags { values, count: scaled(count, 4) }
        }
        "UN" if looks_like_text(bytes) => DicomValue::Text {
            values: vec![decode_text(bytes, charset).trim_end_matches(['\0', ' ']).to_string()],
            truncated,
        },
        _ => DicomValue::Binary,
    }
}

fn convert_element(element: &Element, charset: Charset) -> DicomElement {
    let (vr, vr_source) = match element.vr {
        Vr::Implicit => match dictionary(element.tag) {
            Some((_, vr)) if vr != "--" => (vr, DicomVrSource::Dictionary),
            _ => ("UN", DicomVrSource::Unknown),
        },
        vr => (vr.as_str(), DicomVrSource::Explicit),
    };

    let value = match &element.value {
        ElementValue::Bytes(bytes) => convert_value(element, vr, bytes, charset),
        ElementValue::Sequence(items) => DicomValue::Sequence {
            items: items
                .iter()
                .take(MAX_ITEMS)
                .map(|item| DicomItem {
                    offset: item.offset as u64,
                    length: (item.length != UNDEFINED_LENGTH).then_some(item.length),
                    elements: item.elements.iter().map(|element| convert_element(element, charset)).collect(),
                })
                .collect(),
            total_items: items.len() as u32,
        },
        ElementValue::Fragments { offset_table, items, .. } => {
            let fragment_items = items.iter().skip(1);
            DicomValue::Encapsulated {
                offset_table: offset_table.iter().take(MAX_VALUES).copied().collect(),
                fragments: fragment_items
                    .clone()
                    .take(MAX_FRAGMENTS)
                    .map(|item| DicomFragment { offset: item.offset as u64, length: item.length })
                    .collect(),
                total_fragments: items.len().saturating_sub(1) as u32,
                total_bytes: fragment_items.map(|item| item.length as u64).sum(),
            }
        }
    };

    DicomElement {
        offset: element.offset as u64,
        tag: element.tag,
        vr: vr.to_string(),
        vr_source,
        length: (element.length != UNDEFINED_LENGTH).then_some(element.length),
        on_disk_length: element.end.saturating_sub(element.offset) as u64,
        value,
    }
}

fn has_icon(dataset: &DataSet) -> bool {
    dataset.elements.iter().any(|element| match &element.value {
        ElementValue::Sequence(items) => element.tag == tags::ICON_IMAGE_SEQUENCE || items.iter().any(has_icon),
        _ => false,
    })
}

fn build_parameters(file: &ParsedFile) -> DicomParameters {
    let dataset = &file.dataset;
    let transfer_syntax = file
        .transfer_syntax_uid
        .as_deref()
        .and_then(uid_name)
        .unwrap_or_else(|| file.transfer_syntax.name())
        .to_string();
    DicomParameters {
        transfer_syntax_uid: file.transfer_syntax_uid.clone(),
        transfer_syntax,
        explicit_vr: file.encoding.explicit,
        big_endian: file.encoding.big_endian,
        deflated: file.encoding.deflated,
        encoding_guessed: file.encoding.guessed,
        explicit_override: file.encoding.explicit_override,
        sop_class_uid: dataset.string(tags::SOP_CLASS_UID),
        modality: dataset.string(tags::MODALITY),
        rows: dataset.u16_value(tags::ROWS),
        columns: dataset.u16_value(tags::COLUMNS),
        samples_per_pixel: dataset.u16_value(tags::SAMPLES_PER_PIXEL),
        photometric: dataset.string(tags::PHOTOMETRIC_INTERPRETATION),
        bits_allocated: dataset.u16_value(tags::BITS_ALLOCATED),
        bits_stored: dataset.u16_value(tags::BITS_STORED),
        high_bit: dataset.u16_value(tags::HIGH_BIT),
        pixel_representation: dataset.u16_value(tags::PIXEL_REPRESENTATION),
        planar_configuration: dataset.u16_value(tags::PLANAR_CONFIGURATION),
        number_of_frames: dataset.int_value(tags::NUMBER_OF_FRAMES),
        window_center: dataset.f64_list(tags::WINDOW_CENTER),
        window_width: dataset.f64_list(tags::WINDOW_WIDTH),
        rescale_slope: dataset.f64_value(tags::RESCALE_SLOPE),
        rescale_intercept: dataset.f64_value(tags::RESCALE_INTERCEPT),
        rescale_type: dataset.string(RESCALE_TYPE),
        pixel_spacing: dataset.f64_list(PIXEL_SPACING),
        icon_image: has_icon(dataset),
    }
}

fn color_layout(photometric: &Photometric, samples: u16) -> (ColorModel, Option<String>) {
    let model = match photometric {
        Photometric::Monochrome1 | Photometric::Monochrome2 => ColorModel::Gray,
        Photometric::PaletteColor => ColorModel::Indexed,
        Photometric::Rgb => ColorModel::Rgb,
        Photometric::Argb => ColorModel::Rgba,
        Photometric::Cmyk => ColorModel::Cmyk,
        Photometric::YbrFull
        | Photometric::YbrFull422
        | Photometric::YbrPartial422
        | Photometric::YbrPartial420
        | Photometric::YbrIct
        | Photometric::YbrRct => ColorModel::YCbCr,
        Photometric::Hsv => ColorModel::Multichannel,
        Photometric::Unknown(_) => match samples {
            1 => ColorModel::Gray,
            3 => ColorModel::Rgb,
            _ => ColorModel::Unknown,
        },
    };
    let subsampling = match photometric {
        Photometric::YbrFull422 | Photometric::YbrPartial422 => subsampling_name(&[(2, 1), (1, 1), (1, 1)]),
        Photometric::YbrPartial420 => subsampling_name(&[(2, 2), (1, 1), (1, 1)]),
        _ => None,
    };
    (model, subsampling)
}

fn build_summary(file: &ParsedFile) -> Option<ImageSummary> {
    let dataset = &file.dataset;
    let pixel_element = dataset
        .get(tags::PIXEL_DATA)
        .or_else(|| dataset.get(tags::FLOAT_PIXEL_DATA))
        .or_else(|| dataset.get(tags::DOUBLE_FLOAT_PIXEL_DATA))?;
    let params = read_pixel_params(dataset, file.transfer_syntax).ok()?;

    let (color_model, subsampling) = color_layout(&params.photometric, params.samples_per_pixel);
    let float_bits = match pixel_element.tag {
        tags::FLOAT_PIXEL_DATA => Some(32),
        tags::DOUBLE_FLOAT_PIXEL_DATA => Some(64),
        _ => None,
    };
    let bits = float_bits.unwrap_or(params.bits_stored.min(u8::MAX as u16) as u8);
    let mut summary = ImageSummary::new(params.cols, params.rows, color_model, params.samples_per_pixel, bits);
    summary.sample_format = match (float_bits, params.signed) {
        (Some(_), _) => SampleFormat::Float,
        (None, true) => SampleFormat::Signed,
        (None, false) => SampleFormat::Unsigned,
    };
    summary.has_alpha = color_model == ColorModel::Rgba;
    summary.subsampling = subsampling;
    summary.compression = Some(
        file.transfer_syntax_uid
            .as_deref()
            .and_then(uid_name)
            .unwrap_or_else(|| file.transfer_syntax.name())
            .to_string(),
    );

    let frames = dataset.int_value(tags::NUMBER_OF_FRAMES).unwrap_or(1).clamp(1, i64::from(u32::MAX)) as u64;
    if frames > 1 {
        summary.frames = Some(frames as u32);
        let delay = frame_delay_ms(dataset, frames as usize) as u64;
        if delay > 0 {
            summary.duration_ms = Some(delay.saturating_mul(frames));
        }
    }

    if let Some(profile) = dataset.bytes(ICC_PROFILE).and_then(|bytes| ICCProfile::new(bytes).ok()) {
        summary.icc_profile = Some(profile);
    }

    if let ElementValue::Bytes(bytes) = &pixel_element.value {
        let bits_allocated = float_bits.map_or(params.bits_allocated as u64, |bits| bits as u64);
        let pixels = (params.rows as u64 * params.cols as u64).saturating_mul(frames);
        let halves_per_pixel = match params.photometric {
            Photometric::YbrFull422 | Photometric::YbrPartial422 => 4,
            Photometric::YbrPartial420 => 3,
            _ => params.samples_per_pixel as u64 * 2,
        };
        let expected = pixels.saturating_mul(halves_per_pixel).saturating_mul(bits_allocated).div_ceil(16);
        summary.data = Some(DataExtent { expected, actual: bytes.len() as u64 });
    }

    Some(summary)
}

pub fn scan(data: &[u8], limits: &Limits) -> DicomInfo {
    let mut info = DicomInfo::default();
    let file = parse_file(data, limits);
    let charset = charset(&file.dataset);

    if file.has_preamble {
        let zero_filled = data[..PREAMBLE_LENGTH].iter().all(|&byte| byte == 0);
        info.push(0, Some(PREAMBLE_LENGTH as u64 + 4), DicomSection::Preamble { zero_filled });
    }

    if let (Some(first), Some(last)) = (file.meta.elements.first(), file.meta.elements.last()) {
        let elements = file.meta.elements.iter().map(|element| convert_element(element, charset)).collect();
        info.push(first.offset as u64, Some((last.end - first.offset) as u64), DicomSection::FileMeta { elements });
    }

    let parameters_offset = file.dataset_start.min(data.len()) as u64;
    info.push(parameters_offset, None, DicomSection::Parameters(Box::new(build_parameters(&file))));

    let mut end = file.meta.elements.last().map_or(0, |element| element.end).max(file.dataset_start);
    if file.encoding.deflated {
        let elements = file.dataset.elements.iter().map(|element| convert_element(element, charset)).collect();
        let inflated_length = file.inflated_length.unwrap_or(0) as u64;
        let length = data.len().saturating_sub(file.dataset_start) as u64;
        info.push(file.dataset_start as u64, Some(length), DicomSection::DeflatedDataSet { inflated_length, elements });
        end = data.len();
    } else {
        for element in &file.dataset.elements {
            let converted = convert_element(element, charset);
            info.push(
                element.offset as u64,
                Some(converted.on_disk_length),
                DicomSection::Element(Box::new(converted)),
            );
            end = end.max(element.end);
        }
    }

    if end < data.len() {
        info.push(end as u64, Some((data.len() - end) as u64), DicomSection::TrailingData);
    }

    info.summary = build_summary(&file);
    info
}
