use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};

use serde::Serialize;
use tsify::Tsify;

use crate::bitreader::BitReader;
use crate::utils::info::{ColorModel, DataExtent, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat};
use crate::utils::tiff::info::{
    compression_name, first_u32, photometric_name, IfdKind, TiffEntry, TiffIfdSection, TiffImageIfd, TiffSection,
    TiffStructure,
};
use crate::utils::tiff::types::TiffTags;

const MAX_OPCODE_LIST_BYTES: usize = 1 << 20;
const MAX_OPCODES: u32 = 256;
const PHOTOMETRIC_CFA: u32 = 32803;
const PHOTOMETRIC_LINEAR_RAW: u32 = 34892;

#[tsify::declare]
pub type DngInfo = FormatInfo<DngSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DngOpcode {
    pub id: u32,
    pub version: u32,
    pub flags: u32,
    pub parameter_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DngOpcodeList {
    pub list: u8,
    pub declared_count: u32,
    pub opcodes: Vec<DngOpcode>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct DngParameters {
    pub raw_ifd: Option<u32>,
    pub preview_ifds: Vec<u32>,
    pub dng_version: Option<Vec<u8>>,
    pub backward_version: Option<Vec<u8>>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub unique_camera_model: Option<String>,
    pub localized_camera_model: Option<String>,
    pub photometric: Option<u32>,
    pub cfa_repeat: Option<[u32; 2]>,
    pub cfa_pattern: Vec<u8>,
    pub cfa_plane_colors: Vec<u8>,
    pub cfa_layout: Option<u32>,
    pub black_level_repeat: Option<[u32; 2]>,
    pub black_level: Vec<f64>,
    pub black_level_count: u32,
    pub black_level_delta_h: u32,
    pub black_level_delta_v: u32,
    pub white_level: Vec<f64>,
    pub linearization_entries: u32,
    pub active_area: Vec<f64>,
    pub default_crop_origin: Vec<f64>,
    pub default_crop_size: Vec<f64>,
    pub default_scale: Vec<f64>,
    pub color_matrix: [Vec<f64>; 2],
    pub forward_matrix: [Vec<f64>; 2],
    pub camera_calibration: [Vec<f64>; 2],
    pub calibration_illuminant: [Option<u32>; 2],
    pub analog_balance: Vec<f64>,
    pub as_shot_neutral: Vec<f64>,
    pub as_shot_white_xy: Vec<f64>,
    pub baseline_exposure: Option<f64>,
    pub baseline_noise: Option<f64>,
    pub baseline_sharpness: Option<f64>,
    pub profile_name: Option<String>,
    pub opcode_lists: Vec<DngOpcodeList>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum DngSection {
    Tiff(TiffSection),
    Parameters(Box<DngParameters>),
}

struct Lookup<'a> {
    primary: Option<&'a TiffIfdSection>,
    raw: Option<&'a TiffIfdSection>,
}

impl<'a> Lookup<'a> {
    fn raster(&self, tag: TiffTags) -> Option<&'a TiffEntry> {
        self.raw.and_then(|ifd| ifd.entry(tag as u16)).or_else(|| self.primary.and_then(|ifd| ifd.entry(tag as u16)))
    }

    fn profile(&self, tag: TiffTags) -> Option<&'a TiffEntry> {
        self.primary.and_then(|ifd| ifd.entry(tag as u16)).or_else(|| self.raw.and_then(|ifd| ifd.entry(tag as u16)))
    }

    fn numbers(&self, tag: TiffTags) -> Vec<f64> {
        self.raster(tag).map(TiffEntry::numbers).unwrap_or_default()
    }

    fn profile_numbers(&self, tag: TiffTags) -> Vec<f64> {
        self.profile(tag).map(TiffEntry::numbers).unwrap_or_default()
    }

    fn profile_number(&self, tag: TiffTags) -> Option<f64> {
        self.profile_numbers(tag).first().copied()
    }

    fn profile_text(&self, tag: TiffTags) -> Option<String> {
        self.profile(tag).and_then(TiffEntry::text).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }

    fn pair(&self, tag: TiffTags) -> Option<[u32; 2]> {
        let values = self.numbers(tag);
        (values.len() >= 2).then(|| [values[0] as u32, values[1] as u32])
    }

    fn version(&self, tag: TiffTags) -> Option<Vec<u8>> {
        let values = self.profile_numbers(tag);
        (!values.is_empty()).then(|| values.iter().map(|&v| v as u8).collect())
    }
}

fn primary_ifd(structure: &TiffStructure) -> Option<&TiffIfdSection> {
    structure
        .image_ifds
        .iter()
        .filter(|image| image.kind == IfdKind::Page)
        .min_by_key(|image| image.index)
        .and_then(|image| structure.ifd_section(image.index))
}

fn dimension(ifd: &TiffIfdSection, tag: TiffTags) -> u32 {
    ifd.entry(tag as u16).and_then(first_u32).unwrap_or(0)
}

fn select_raw(structure: &TiffStructure) -> Option<&TiffImageIfd> {
    let area = |image: &&TiffImageIfd| {
        structure
            .ifd_section(image.index)
            .map(|ifd| {
                u64::from(dimension(ifd, TiffTags::ImageWidth)) * u64::from(dimension(ifd, TiffTags::ImageLength))
            })
            .unwrap_or(0)
    };
    let is_raw = |image: &&TiffImageIfd| {
        structure
            .ifd_section(image.index)
            .and_then(|ifd| ifd.entry(TiffTags::PhotometricInterpretation as u16))
            .and_then(first_u32)
            .is_some_and(|p| p == PHOTOMETRIC_CFA || p == PHOTOMETRIC_LINEAR_RAW)
    };
    let has_data = |image: &&TiffImageIfd| image.image_data.as_ref().is_some_and(|data| data.chunk_count > 0);

    let images = &structure.image_ifds;
    images
        .iter()
        .filter(|image| is_raw(image) && has_data(image))
        .max_by_key(area)
        .or_else(|| images.iter().filter(has_data).max_by_key(area))
        .or_else(|| images.iter().max_by_key(area))
}

fn read_opcode_list<R: Read + Seek>(reader: &mut BitReader<R>, list: u8, entry: &TiffEntry) -> Option<DngOpcodeList> {
    let length = (entry.count as usize).min(MAX_OPCODE_LIST_BYTES);
    let mut data = vec![0u8; length];
    match entry.value_offset {
        Some(offset) => {
            reader.seek(SeekFrom::Start(u64::from(offset))).ok()?;
            reader.read_exact(&mut data).ok()?;
        }
        None => {
            let bytes = entry.bytes()?;
            data = bytes.to_vec();
        }
    }

    let u32_at =
        |pos: usize| data.get(pos..pos.saturating_add(4)).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let declared_count = u32_at(0)?;
    let mut opcodes = Vec::new();
    let mut pos = 4usize;
    let mut truncated = false;

    for _ in 0..declared_count.min(MAX_OPCODES) {
        let (Some(id), Some(version), Some(flags), Some(parameter_bytes)) =
            (u32_at(pos), u32_at(pos.saturating_add(4)), u32_at(pos.saturating_add(8)), u32_at(pos.saturating_add(12)))
        else {
            truncated = true;
            break;
        };
        opcodes.push(DngOpcode { id, version, flags, parameter_bytes });
        pos = pos.saturating_add(16).saturating_add(parameter_bytes as usize);
    }

    truncated |= (opcodes.len() as u32) < declared_count;
    Some(DngOpcodeList { list, declared_count, opcodes, truncated })
}

fn parameters<R: Read + Seek>(
    structure: &TiffStructure,
    raw: Option<&TiffImageIfd>,
    reader: &mut BitReader<R>,
) -> DngParameters {
    let primary = primary_ifd(structure);
    let raw_section = raw.and_then(|image| structure.ifd_section(image.index));
    let lookup = Lookup { primary, raw: raw_section };

    let count = |tag: TiffTags| lookup.raster(tag).map(|entry| entry.count).unwrap_or(0);

    let mut params = DngParameters {
        raw_ifd: raw.map(|image| image.index),
        preview_ifds: structure
            .image_ifds
            .iter()
            .filter(|image| Some(image.index) != raw.map(|r| r.index))
            .filter(|image| image.image_data.as_ref().is_some_and(|data| data.chunk_count > 0))
            .map(|image| image.index)
            .collect(),
        dng_version: lookup.version(TiffTags::DNGVersion),
        backward_version: lookup.version(TiffTags::DNGBackwardVersion),
        make: lookup.profile_text(TiffTags::Make),
        model: lookup.profile_text(TiffTags::Model),
        unique_camera_model: lookup.profile_text(TiffTags::UniqueCameraModel),
        localized_camera_model: lookup.profile_text(TiffTags::LocalizedCameraModel),
        photometric: raw_section
            .and_then(|ifd| ifd.entry(TiffTags::PhotometricInterpretation as u16))
            .and_then(first_u32),
        cfa_repeat: lookup.pair(TiffTags::CFARepeatPatternDim),
        cfa_pattern: lookup.numbers(TiffTags::CFAPattern).iter().map(|&v| v as u8).collect(),
        cfa_plane_colors: lookup.numbers(TiffTags::CFAPlaneColor).iter().map(|&v| v as u8).collect(),
        cfa_layout: lookup.raster(TiffTags::CFALayout).and_then(first_u32),
        black_level_repeat: lookup.pair(TiffTags::BlackLevelRepeatDim),
        black_level: lookup.numbers(TiffTags::BlackLevel),
        black_level_count: count(TiffTags::BlackLevel),
        black_level_delta_h: count(TiffTags::BlackLevelDeltaH),
        black_level_delta_v: count(TiffTags::BlackLevelDeltaV),
        white_level: lookup.numbers(TiffTags::WhiteLevel),
        linearization_entries: count(TiffTags::LinearizationTable),
        active_area: lookup.numbers(TiffTags::ActiveArea),
        default_crop_origin: lookup.numbers(TiffTags::DefaultCropOrigin),
        default_crop_size: lookup.numbers(TiffTags::DefaultCropSize),
        default_scale: lookup.numbers(TiffTags::DefaultScale),
        color_matrix: [lookup.profile_numbers(TiffTags::ColorMatrix1), lookup.profile_numbers(TiffTags::ColorMatrix2)],
        forward_matrix: [
            lookup.profile_numbers(TiffTags::ForwardMatrix1),
            lookup.profile_numbers(TiffTags::ForwardMatrix2),
        ],
        camera_calibration: [
            lookup.profile_numbers(TiffTags::CameraCalibration1),
            lookup.profile_numbers(TiffTags::CameraCalibration2),
        ],
        calibration_illuminant: [
            lookup.profile(TiffTags::CalibrationIlluminant1).and_then(first_u32),
            lookup.profile(TiffTags::CalibrationIlluminant2).and_then(first_u32),
        ],
        analog_balance: lookup.profile_numbers(TiffTags::AnalogBalance),
        as_shot_neutral: lookup.profile_numbers(TiffTags::AsShotNeutral),
        as_shot_white_xy: lookup.profile_numbers(TiffTags::AsShotWhiteXY),
        baseline_exposure: lookup.profile_number(TiffTags::BaselineExposure),
        baseline_noise: lookup.profile_number(TiffTags::BaselineNoise),
        baseline_sharpness: lookup.profile_number(TiffTags::BaselineSharpness),
        profile_name: lookup.profile_text(TiffTags::ProfileName),
        opcode_lists: Vec::new(),
    };

    for (list, tag) in [(1, TiffTags::OpcodeList1), (2, TiffTags::OpcodeList2), (3, TiffTags::OpcodeList3)] {
        if let Some(entry) = lookup.raster(tag) {
            if let Some(opcodes) = read_opcode_list(reader, list, entry) {
                params.opcode_lists.push(opcodes);
            }
        }
    }

    params
}

fn summary(structure: &TiffStructure, raw: &TiffImageIfd, params: &DngParameters) -> Option<ImageSummary> {
    let ifd = structure.ifd_section(raw.index)?;
    let mut width = dimension(ifd, TiffTags::ImageWidth);
    let mut height = dimension(ifd, TiffTags::ImageLength);

    if let [top, left, bottom, right] = params.active_area[..] {
        let (top, left) = (top as u32, left as u32);
        let (bottom, right) = ((bottom as u32).min(height), (right as u32).min(width));
        if top < bottom && left < right {
            width = right - left;
            height = bottom - top;
        }
    }

    let primary = primary_ifd(structure);
    let orientation = [primary, Some(ifd)].into_iter().flatten().find_map(|section| {
        section.entry(TiffTags::Orientation as u16).and_then(first_u32).filter(|o| (1..=8).contains(o))
    });
    if orientation.is_some_and(|o| o >= 5) {
        std::mem::swap(&mut width, &mut height);
    }

    let channels = ifd.entry(TiffTags::SamplesPerPixel as u16).and_then(first_u32).unwrap_or(1) as u16;
    let bits = ifd
        .entry(TiffTags::BitsPerSample as u16)
        .map(TiffEntry::numbers)
        .and_then(|v| v.first().copied())
        .unwrap_or(16.0) as u8;
    let color_model = match (params.photometric, channels) {
        (Some(PHOTOMETRIC_CFA), _) => ColorModel::Cfa,
        (_, 1) => ColorModel::Gray,
        (_, 3) => ColorModel::Rgb,
        (_, 4) => ColorModel::Rgba,
        _ => ColorModel::Multichannel,
    };

    let mut summary = ImageSummary::new(width, height, color_model, channels, bits);
    let sample_format = ifd.entry(TiffTags::SampleFormat as u16).and_then(first_u32);
    summary.sample_format = match sample_format {
        Some(2) => SampleFormat::Signed,
        Some(3) => SampleFormat::Float,
        _ => SampleFormat::Unsigned,
    };
    let compression = ifd.entry(TiffTags::Compression as u16).and_then(first_u32).unwrap_or(1);
    summary.compression = Some(format!("{} ({})", compression_name(compression), compression));
    summary.data =
        raw.image_data.as_ref().map(|data| DataExtent { expected: data.declared_bytes, actual: data.available_bytes });
    Some(summary)
}

pub fn build_info<R: Read + Seek>(structure: TiffStructure, reader: &mut BitReader<R>) -> DngInfo {
    let raw = select_raw(&structure);
    let params = parameters(&structure, raw, reader);
    let summary = raw.and_then(|raw| summary(&structure, raw, &params));
    let raw_offset = raw.map(|image| u64::from(image.ifd.offset)).unwrap_or(0);

    let mut info = DngInfo { summary, sections: Vec::new() };
    for section in structure.sections {
        info.push(section.offset, section.length, DngSection::Tiff(section.data));
    }
    let position = info.sections.iter().position(|section| section.offset > raw_offset).unwrap_or(info.sections.len());
    info.sections
        .insert(position, crate::utils::info::Section::new(raw_offset, None, DngSection::Parameters(Box::new(params))));
    info
}

fn version_text(version: &[u8]) -> String {
    version.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(".")
}

fn numbers_text(values: &[f64]) -> String {
    values.iter().map(|v| format!("{}", (v * 10000.0).round() / 10000.0)).collect::<Vec<_>>().join(", ")
}

fn matrix_text(values: &[f64]) -> String {
    values.chunks(3).map(numbers_text).collect::<Vec<_>>().join(" | ")
}

fn cfa_color_letter(color: u8) -> char {
    match color {
        0 => 'R',
        1 => 'G',
        2 => 'B',
        3 => 'C',
        4 => 'M',
        5 => 'Y',
        6 => 'W',
        _ => '?',
    }
}

fn illuminant_name(value: u32) -> String {
    let name = match value {
        0 => "Unknown",
        1 => "Daylight",
        2 => "Fluorescent",
        3 => "Tungsten",
        4 => "Flash",
        9 => "Fine weather",
        10 => "Cloudy",
        11 => "Shade",
        12 => "Daylight fluorescent",
        13 => "Day white fluorescent",
        14 => "Cool white fluorescent",
        15 => "White fluorescent",
        16 => "Warm white fluorescent",
        17 => "Standard light A",
        18 => "Standard light B",
        19 => "Standard light C",
        20 => "D55",
        21 => "D65",
        22 => "D75",
        23 => "D50",
        24 => "ISO studio tungsten",
        255 => "Other",
        _ => "Unknown",
    };
    format!("{} ({})", name, value)
}

fn opcode_name(id: u32) -> &'static str {
    match id {
        1 => "WarpRectilinear",
        2 => "WarpFisheye",
        3 => "FixVignetteRadial",
        4 => "FixBadPixelsConstant",
        5 => "FixBadPixelsList",
        6 => "TrimBounds",
        7 => "MapTable",
        8 => "MapPolynomial",
        9 => "GainMap",
        10 => "DeltaPerRow",
        11 => "DeltaPerColumn",
        12 => "ScalePerRow",
        13 => "ScalePerColumn",
        14 => "WarpRectilinear2",
        _ => "Unknown",
    }
}

fn describe_parameters(params: &DngParameters, out: &mut InfoWriter) {
    match &params.dng_version {
        Some(version) => out.field("DNG version", version_text(version)),
        None => out.field("DNG version", "none (TIFF-based camera raw)"),
    };
    out.opt_field("Backward version", params.backward_version.as_deref().map(version_text));
    out.opt_field("Make", params.make.as_deref());
    out.opt_field("Model", params.model.as_deref());
    out.opt_field("Unique camera model", params.unique_camera_model.as_deref());
    out.opt_field("Localized camera model", params.localized_camera_model.as_deref());
    out.opt_field("Profile name", params.profile_name.as_deref());

    match params.raw_ifd {
        Some(index) => out.field("Raw image", format!("IFD {}", index)),
        None => out.field("Raw image", "not found"),
    };
    if !params.preview_ifds.is_empty() {
        let previews = params.preview_ifds.iter().map(|i| format!("IFD {}", i)).collect::<Vec<_>>().join(", ");
        out.field("Other images", previews);
    }

    if let Some(photometric) = params.photometric {
        let name = match photometric {
            PHOTOMETRIC_CFA => "Color filter array".to_string(),
            PHOTOMETRIC_LINEAR_RAW => "Linear raw".to_string(),
            other => format!("Not raw, {}", photometric_name(other)),
        };
        out.field("Raw photometric", format!("{} ({})", name, photometric));
    }

    if !params.cfa_pattern.is_empty() {
        let [rows, cols] = params.cfa_repeat.unwrap_or([2, 2]);
        let colors = if params.cfa_plane_colors.is_empty() { vec![0, 1, 2] } else { params.cfa_plane_colors.clone() };
        let letters: String = params
            .cfa_pattern
            .iter()
            .map(|&index| colors.get(index as usize).copied().map(cfa_color_letter).unwrap_or('?'))
            .collect();
        let pattern = if rows <= 2 && cols <= 2 {
            letters
        } else {
            letters
                .as_bytes()
                .chunks(cols.max(1) as usize)
                .map(|row| String::from_utf8_lossy(row).into_owned())
                .collect::<Vec<_>>()
                .join(" / ")
        };
        out.field("CFA pattern", format!("{} ({}x{})", pattern, rows, cols));
    }
    if let Some(layout) = params.cfa_layout {
        let name = if layout == 1 { "Rectangular" } else { "Staggered or other" };
        out.field("CFA layout", format!("{} ({})", name, layout));
    }

    if params.black_level_count > 0 {
        let repeat = params.black_level_repeat.map(|[r, c]| format!(" (repeat {}x{})", r, c)).unwrap_or_default();
        let mut text = numbers_text(&params.black_level[..params.black_level.len().min(16)]);
        if params.black_level_count as usize > 16 {
            text = format!("{}, ... ({} values)", text, params.black_level_count);
        }
        out.field("Black level", format!("{}{}", text, repeat));
    }
    if params.black_level_delta_h > 0 || params.black_level_delta_v > 0 {
        out.field(
            "Black level deltas",
            format!("{} horizontal, {} vertical", params.black_level_delta_h, params.black_level_delta_v),
        );
    }
    if !params.white_level.is_empty() {
        out.field("White level", numbers_text(&params.white_level));
    }
    if params.linearization_entries > 0 {
        out.field("Linearization table", format!("{} entries", params.linearization_entries));
    }
    if params.active_area.len() == 4 {
        let a = &params.active_area;
        out.field("Active area", format!("top {}, left {}, bottom {}, right {}", a[0], a[1], a[2], a[3]));
    }
    if params.default_crop_origin.len() >= 2 {
        out.field("Default crop origin", numbers_text(&params.default_crop_origin));
    }
    if params.default_crop_size.len() >= 2 {
        out.field("Default crop size", numbers_text(&params.default_crop_size));
    }
    if !params.default_scale.is_empty() {
        out.field("Default scale", numbers_text(&params.default_scale));
    }

    for i in 0..2 {
        if let Some(illuminant) = params.calibration_illuminant[i] {
            out.field(&format!("Calibration illuminant {}", i + 1), illuminant_name(illuminant));
        }
        if !params.color_matrix[i].is_empty() {
            out.field(&format!("Color matrix {}", i + 1), matrix_text(&params.color_matrix[i]));
        }
        if !params.forward_matrix[i].is_empty() {
            out.field(&format!("Forward matrix {}", i + 1), matrix_text(&params.forward_matrix[i]));
        }
        if !params.camera_calibration[i].is_empty() {
            out.field(&format!("Camera calibration {}", i + 1), matrix_text(&params.camera_calibration[i]));
        }
    }
    if !params.analog_balance.is_empty() {
        out.field("Analog balance", numbers_text(&params.analog_balance));
    }
    if !params.as_shot_neutral.is_empty() {
        out.field("As-shot neutral", numbers_text(&params.as_shot_neutral));
    }
    if !params.as_shot_white_xy.is_empty() {
        out.field("As-shot white xy", numbers_text(&params.as_shot_white_xy));
    }
    out.opt_field("Baseline exposure", params.baseline_exposure.map(|v| format!("{:+.2} EV", v)));
    out.opt_field("Baseline noise", params.baseline_noise);
    out.opt_field("Baseline sharpness", params.baseline_sharpness);

    for list in &params.opcode_lists {
        out.group(format!("Opcode list {}", list.list), |out| {
            if list.truncated {
                out.field("Opcodes", format!("{} of {} declared (truncated)", list.opcodes.len(), list.declared_count));
            } else {
                out.field("Opcodes", list.declared_count);
            }
            for opcode in &list.opcodes {
                let mut flags = Vec::new();
                if opcode.flags & 1 != 0 {
                    flags.push("optional");
                }
                if opcode.flags & 2 != 0 {
                    flags.push("skip for preview");
                }
                let flags = if flags.is_empty() { String::new() } else { format!(", {}", flags.join(", ")) };
                out.field(
                    &format!("{} ({})", opcode_name(opcode.id), opcode.id),
                    format!("{} parameter bytes{}", opcode.parameter_bytes, flags),
                );
            }
        });
    }
}

impl InfoSection for DngSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            DngSection::Tiff(section) => section.title(),
            DngSection::Parameters(params) => match params.raw_ifd {
                Some(index) => Cow::Owned(format!("DNG Parameters (raw image IFD {})", index)),
                None => Cow::Borrowed("DNG Parameters"),
            },
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            DngSection::Tiff(section) => section.describe(out),
            DngSection::Parameters(params) => describe_parameters(params, out),
        }
    }
}
