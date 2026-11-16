use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use crate::decoders::dicom::frames::FramePlan;
use crate::decoders::dicom::info::{scan, DicomInfo};
use crate::decoders::dicom::parser::{DataCursor, DataSet, ElementValue, Parser};
use crate::decoders::dicom::pixels::{expand_segmented_lut, Lut, MonoTransform, PixelParams};
use crate::decoders::dicom::types::{tags, Photometric, TransferSyntax, VoiFunction, Vr};
use crate::utils::codecs::deflate::DeflateDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::frames::{FrameSource, Frames};
use crate::utils::image::Image;
use crate::{log_debug, log_warn, Limits};

pub struct DicomDecoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    info: Option<DicomInfo>,
    parsed: Option<Parsed>,
}

struct Parsed {
    transfer_syntax: TransferSyntax,
    dataset: Arc<DataSet>,
}

pub fn has_dicm_magic(header: &[u8]) -> bool {
    header.len() >= 132 && &header[128..132] == b"DICM"
}

pub fn looks_like_dicom_dataset(header: &[u8]) -> bool {
    if header.len() < 8 {
        return false;
    }

    let group = u16::from_le_bytes([header[0], header[1]]);
    if !matches!(group, 0x0002 | 0x0008) {
        return false;
    }

    let element = u16::from_le_bytes([header[2], header[3]]);
    if element > 0x1000 {
        return false;
    }

    if Vr::from_bytes([header[4], header[5]]).is_some() {
        return true;
    }

    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    length < 0x10000
}

fn sniff_encoding(data: &[u8]) -> (bool, bool) {
    if data.len() < 8 {
        return (true, false);
    }

    let explicit_le = Vr::from_bytes([data[4], data[5]]).is_some();
    let group_le = u16::from_le_bytes([data[0], data[1]]);
    let group_be = u16::from_be_bytes([data[0], data[1]]);
    let known = [0x0002u16, 0x0008, 0x0010, 0x0018, 0x0020, 0x0028];

    if known.contains(&group_be) && !known.contains(&group_le) {
        return (true, true);
    }

    (explicit_le, false)
}

pub(super) struct Encoding {
    pub explicit: bool,
    pub big_endian: bool,
    pub deflated: bool,
    pub guessed: bool,
    pub explicit_override: bool,
}

pub(super) struct ParsedFile {
    pub has_preamble: bool,
    pub meta: DataSet,
    pub dataset_start: usize,
    pub transfer_syntax_uid: Option<String>,
    pub transfer_syntax: TransferSyntax,
    pub encoding: Encoding,
    pub inflated_length: Option<usize>,
    pub dataset: DataSet,
}

pub(super) fn parse_file(data: &[u8], limits: &Limits) -> ParsedFile {
    let has_preamble = has_dicm_magic(data);
    let (meta, dataset_start) = if has_preamble {
        let mut cursor = DataCursor::new(data, false);
        cursor.set_position(132);
        let meta = Parser::new(true).parse_meta(&mut cursor);
        let start = cursor.position();
        (meta, start)
    } else if data.len() >= 8 && u16::from_le_bytes([data[0], data[1]]) == 0x0002 {
        let mut cursor = DataCursor::new(data, false);
        let meta = Parser::new(true).parse_meta(&mut cursor);
        let start = cursor.position();
        (meta, start)
    } else {
        (DataSet::default(), 0)
    };

    let ts_uid = meta.string(tags::TRANSFER_SYNTAX_UID);
    let mut transfer_syntax = match &ts_uid {
        Some(uid) => TransferSyntax::from_uid(uid),
        None => TransferSyntax::Unknown,
    };

    if let Some(uid) = &ts_uid {
        log_debug!("DICOM: transfer syntax {} ({})", uid, transfer_syntax.name());
    }

    let rest = &data[dataset_start.min(data.len())..];

    let inflated;
    let mut guessed = false;
    let mut explicit_override = false;
    let (dataset_bytes, explicit, big_endian): (&[u8], bool, bool) = match transfer_syntax {
        TransferSyntax::DeflatedExplicitLittleEndian => {
            let max_output = limits.max_alloc.unwrap_or(1 << 30) as usize;
            inflated = DeflateDecoder::from_slice(rest).decode_with_max(max_output);
            (&inflated, true, false)
        }
        TransferSyntax::Unknown => {
            let (explicit, big_endian) = sniff_encoding(rest);
            if ts_uid.is_some() {
                log_warn!("DICOM: unrecognized transfer syntax, guessing encoding from data");
            }
            guessed = true;
            (rest, explicit, big_endian)
        }
        ts => {
            let mut explicit = ts.is_explicit_vr();
            if !explicit && rest.len() >= 8 && Vr::from_bytes([rest[4], rest[5]]).is_some() {
                log_warn!("DICOM: implicit VR transfer syntax but dataset looks explicit, switching");
                explicit = true;
                explicit_override = true;
            }
            (rest, explicit, ts.is_big_endian())
        }
    };

    if transfer_syntax == TransferSyntax::Unknown && ts_uid.is_none() {
        let (explicit, big_endian) = sniff_encoding(dataset_bytes);
        transfer_syntax = match (explicit, big_endian) {
            (_, true) => TransferSyntax::ExplicitBigEndian,
            (true, false) => TransferSyntax::ExplicitLittleEndian,
            (false, false) => TransferSyntax::ImplicitLittleEndian,
        };
    }

    let deflated = transfer_syntax == TransferSyntax::DeflatedExplicitLittleEndian;
    let base = if deflated { 0 } else { dataset_start.min(data.len()) };
    let mut cursor = DataCursor::with_base(dataset_bytes, big_endian, base);
    let dataset = Parser::new(explicit).parse_dataset(&mut cursor, 0);

    ParsedFile {
        has_preamble,
        meta,
        dataset_start,
        transfer_syntax_uid: ts_uid,
        transfer_syntax,
        encoding: Encoding { explicit, big_endian, deflated, guessed, explicit_override },
        inflated_length: deflated.then_some(dataset_bytes.len()),
        dataset,
    }
}

impl<R: Read + Seek> DicomDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, limits: Limits::default(), info: None, parsed: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> DicomInfo {
        if self.info.is_none() {
            let saved_pos = self.reader.stream_position().unwrap_or(0);
            let mut data = Vec::new();
            let read = self.reader.seek(SeekFrom::Start(0)).and_then(|_| self.reader.read_to_end(&mut data));
            let _ = self.reader.seek(SeekFrom::Start(saved_pos));
            match read {
                Ok(_) => self.info = Some(scan(&data, &self.limits)),
                Err(e) => log_warn!("Failed to read DICOM data: {}", e),
            }
        }

        self.info.clone().unwrap_or_default()
    }

    fn load(&mut self) -> VexelResult<()> {
        if self.parsed.is_some() {
            return Ok(());
        }

        self.reader.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        self.reader.read_to_end(&mut data)?;

        let file = parse_file(&data, &self.limits);

        let mut elements = file.meta.elements;
        elements.extend(file.dataset.elements);
        let dataset = DataSet { elements, ..Default::default() };

        self.parsed = Some(Parsed { transfer_syntax: file.transfer_syntax, dataset: Arc::new(dataset) });
        Ok(())
    }

    fn frame_plan(&mut self) -> VexelResult<FramePlan> {
        self.load()?;
        let parsed = self.parsed.as_ref().ok_or_else(|| VexelError::Custom("DICOM: no parsed dataset".to_string()))?;
        FramePlan::new(parsed.dataset.clone(), parsed.transfer_syntax, &mut self.limits)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut frames = self.frame_plan()?.into_source(false);
        frames
            .next_frame()?
            .map(Image::from_frame)
            .ok_or_else(|| VexelError::Custom("DICOM: no frames could be decoded".to_string()))
    }

    // TODO: a multi-frame DICOM object is a cine loop or a volume of
    // slices rather than an animation
    pub fn decode_frames(&mut self) -> VexelResult<Frames> {
        Ok(Frames::new(self.frame_plan()?.into_source(true)))
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        self.load()?;
        let Some(parsed) = self.parsed.as_ref() else {
            return Err(VexelError::Custom("DICOM: no parsed dataset".to_string()));
        };

        let mut icons = Vec::new();
        find_icon_datasets(&parsed.dataset, 0, &mut icons);

        let mut thumbnails = Vec::new();
        for icon in icons {
            let icon = FramePlan::new(Arc::new(icon), TransferSyntax::ImplicitLittleEndian, &mut self.limits)
                .and_then(|plan| plan.into_source(false).collect_images());
            match icon {
                Ok(images) => thumbnails.extend(images),
                Err(e) => log_warn!("DICOM: failed to decode icon image: {}", e),
            }
        }

        Ok(thumbnails)
    }
}

pub(super) fn read_pixel_params(dataset: &DataSet, transfer_syntax: TransferSyntax) -> VexelResult<PixelParams> {
    let rows = dataset.u16_value(tags::ROWS).unwrap_or(0) as u32;
    let cols = dataset.u16_value(tags::COLUMNS).unwrap_or(0) as u32;

    if rows == 0 || cols == 0 {
        return Err(VexelError::InvalidDimensions { width: cols, height: rows });
    }

    let samples_per_pixel = match dataset.u16_value(tags::SAMPLES_PER_PIXEL) {
        Some(v) if v > 4 => {
            log_warn!("DICOM: unusual SamplesPerPixel {}, clamping to 4", v);
            4
        }
        Some(v) => v.max(1),
        None => 1,
    };

    let photometric = match dataset.string(tags::PHOTOMETRIC_INTERPRETATION) {
        Some(s) if !s.is_empty() => Photometric::from_str(&s),
        _ => {
            if samples_per_pixel >= 3 {
                Photometric::Rgb
            } else {
                Photometric::Monochrome2
            }
        }
    };

    let bits_allocated = match dataset.u16_value(tags::BITS_ALLOCATED) {
        Some(v) if v == 1 || (8..=32).contains(&v) => v,
        Some(v) => {
            log_warn!("DICOM: unusual BitsAllocated {}, clamping", v);
            v.clamp(1, 32)
        }
        None => 8,
    };

    let bits_stored = match dataset.u16_value(tags::BITS_STORED) {
        Some(v) if v >= 1 && v <= bits_allocated => v,
        _ => bits_allocated,
    };

    let high_bit = match dataset.u16_value(tags::HIGH_BIT) {
        Some(v) if v >= bits_stored - 1 && v < bits_allocated => v,
        _ => bits_stored - 1,
    };

    let signed = dataset.u16_value(tags::PIXEL_REPRESENTATION).unwrap_or(0) == 1;
    let planar = dataset.u16_value(tags::PLANAR_CONFIGURATION).unwrap_or(0) == 1;

    Ok(PixelParams {
        rows,
        cols,
        bits_allocated,
        bits_stored,
        high_bit,
        signed,
        samples_per_pixel,
        planar,
        photometric,
        pixel_big_endian: transfer_syntax.pixel_data_big_endian(),
    })
}

fn read_lut(item: &DataSet, signed_input: bool) -> Option<Lut> {
    let descriptor = item.u16_list(tags::LUT_DESCRIPTOR);
    if descriptor.len() < 3 {
        return None;
    }

    let entries_count = if descriptor[0] == 0 { 65536usize } else { descriptor[0] as usize };
    let first_mapped = if signed_input { descriptor[1] as i16 as i32 } else { descriptor[1] as i32 };
    let bits = descriptor[2].clamp(1, 16);

    let data = item.bytes(tags::LUT_DATA)?;
    let mut entries: Vec<u16> = if data.len() == entries_count && data.len() != entries_count * 2 {
        data.iter().map(|&b| b as u16).collect()
    } else {
        data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
    };

    if entries.is_empty() {
        return None;
    }

    if entries.len() > entries_count {
        entries.truncate(entries_count);
    }

    Some(Lut { first_mapped, bits, entries })
}

fn functional_group_items(dataset: &DataSet, group_tag: u32) -> Option<&DataSet> {
    dataset
        .sequence(tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE)
        .and_then(|items| items.first())
        .and_then(|frame| frame.sequence(group_tag))
        .and_then(|items| items.first())
        .or_else(|| {
            dataset
                .sequence(tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE)
                .and_then(|items| items.first())
                .and_then(|shared| shared.sequence(group_tag))
                .and_then(|items| items.first())
        })
}

pub(super) fn read_mono_transform(dataset: &DataSet, params: &PixelParams) -> MonoTransform {
    let pixel_transform = functional_group_items(dataset, tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE);
    let frame_voi = functional_group_items(dataset, tags::FRAME_VOI_LUT_SEQUENCE);

    let rescale_source = pixel_transform.filter(|d| d.get(tags::RESCALE_SLOPE).is_some()).unwrap_or(dataset);
    let rescale =
        match (rescale_source.f64_value(tags::RESCALE_SLOPE), rescale_source.f64_value(tags::RESCALE_INTERCEPT)) {
            (Some(slope), Some(intercept)) if slope != 0.0 => Some((slope, intercept)),
            _ => None,
        };

    let modality_lut = pixel_transform
        .unwrap_or(dataset)
        .sequence(tags::MODALITY_LUT_SEQUENCE)
        .or_else(|| dataset.sequence(tags::MODALITY_LUT_SEQUENCE))
        .and_then(|items| items.first())
        .and_then(|item| read_lut(item, params.signed));

    let voi_source = frame_voi.filter(|d| d.get(tags::WINDOW_CENTER).is_some()).unwrap_or(dataset);
    let voi_lut = frame_voi
        .unwrap_or(dataset)
        .sequence(tags::VOI_LUT_SEQUENCE)
        .or_else(|| dataset.sequence(tags::VOI_LUT_SEQUENCE))
        .and_then(|items| items.first())
        .and_then(|item| read_lut(item, params.signed));

    let centers = voi_source.f64_list(tags::WINDOW_CENTER);
    let widths = voi_source.f64_list(tags::WINDOW_WIDTH);
    let window = match (centers.first(), widths.first()) {
        (Some(&c), Some(&w)) if w > 0.0 => Some((c, w)),
        _ => None,
    };

    let voi_function = voi_source.string(tags::VOI_LUT_FUNCTION).map(|s| match s.trim() {
        "LINEAR_EXACT" => VoiFunction::LinearExact,
        "SIGMOID" => VoiFunction::Sigmoid,
        _ => VoiFunction::Linear,
    });

    MonoTransform {
        rescale,
        modality_lut,
        voi_lut,
        window,
        voi_function,
        invert: params.photometric == Photometric::Monochrome1,
    }
}

fn read_palette_component(dataset: &DataSet, descriptor_tag: u32, data_tag: u32, segmented_tag: u32) -> Option<Lut> {
    let descriptor = dataset.u16_list(descriptor_tag);
    if descriptor.len() < 3 {
        return None;
    }

    let entries_count = if descriptor[0] == 0 { 65536usize } else { descriptor[0] as usize };
    let first_mapped = descriptor[1] as i32;
    let bits = descriptor[2].clamp(1, 16);

    let entries: Vec<u16> = if let Some(data) = dataset.bytes(data_tag) {
        if data.len() == entries_count && data.len() != entries_count * 2 {
            data.iter().map(|&b| b as u16).collect()
        } else {
            data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
        }
    } else {
        expand_segmented_lut(dataset.bytes(segmented_tag)?, bits)
    };

    if entries.is_empty() {
        return None;
    }

    Some(Lut { first_mapped, bits, entries })
}

pub(super) fn read_palette(dataset: &DataSet, params: &PixelParams) -> Option<[Lut; 3]> {
    if !matches!(params.photometric, Photometric::PaletteColor | Photometric::Argb) {
        return None;
    }

    let red = read_palette_component(
        dataset,
        tags::RED_PALETTE_DESCRIPTOR,
        tags::RED_PALETTE_DATA,
        tags::SEGMENTED_RED_PALETTE_DATA,
    )?;
    let green = read_palette_component(
        dataset,
        tags::GREEN_PALETTE_DESCRIPTOR,
        tags::GREEN_PALETTE_DATA,
        tags::SEGMENTED_GREEN_PALETTE_DATA,
    )?;
    let blue = read_palette_component(
        dataset,
        tags::BLUE_PALETTE_DESCRIPTOR,
        tags::BLUE_PALETTE_DATA,
        tags::SEGMENTED_BLUE_PALETTE_DATA,
    )?;

    Some([red, green, blue])
}

pub(super) fn frame_delay_ms(dataset: &DataSet, num_frames: usize) -> u32 {
    if num_frames <= 1 {
        return 0;
    }

    if let Some(frame_time) = dataset.f64_value(tags::FRAME_TIME) {
        if frame_time > 0.0 {
            return frame_time.round() as u32;
        }
    }

    let vector = dataset.f64_list(tags::FRAME_TIME_VECTOR);
    if let Some(&first) = vector.iter().find(|&&v| v > 0.0) {
        return first.round() as u32;
    }

    if let Some(rate) = dataset.f64_value(tags::RECOMMENDED_DISPLAY_FRAME_RATE) {
        if rate > 0.0 {
            return (1000.0 / rate).round() as u32;
        }
    }

    0
}

fn find_icon_datasets(dataset: &DataSet, depth: u8, out: &mut Vec<DataSet>) {
    if depth > 6 {
        return;
    }

    for element in &dataset.elements {
        if let ElementValue::Sequence(items) = &element.value {
            if element.tag == tags::ICON_IMAGE_SEQUENCE {
                for item in items {
                    out.push(item.clone());
                }
            } else {
                for item in items {
                    find_icon_datasets(item, depth + 1, out);
                }
            }
        }
    }
}
