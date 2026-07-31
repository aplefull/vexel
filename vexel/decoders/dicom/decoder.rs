use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::decoders::dicom::parser::{element_preview, DataCursor, DataSet, Element, ElementValue, Parser};
use crate::decoders::dicom::pixels::{
    color_frame_to_pixels, expand_segmented_lut, extract_native_frame, float_frames_to_pixels, mono_frames_to_pixels,
    Lut, MonoTransform, PixelParams, StoredFrame,
};
use crate::decoders::dicom::rle::decode_rle_frame;
use crate::decoders::dicom::types::{
    tag_element, tag_group, tag_name, tags, DicomElementInfo, Photometric, TransferSyntax, VoiFunction, Vr,
};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::utils::deflate::DeflateDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, ImageFrame, PixelData};
use crate::utils::info::DicomInfo;
use crate::{log_debug, log_warn, Limits};

pub struct DicomDecoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    sections: Vec<DicomElementInfo>,
    parsed: Option<Parsed>,
}

struct Parsed {
    transfer_syntax: TransferSyntax,
    dataset: DataSet,
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

impl<R: Read + Seek> DicomDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, limits: Limits::default(), sections: Vec::new(), parsed: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> DicomInfo {
        DicomInfo { sections: self.sections.clone() }
    }

    fn load(&mut self) -> VexelResult<()> {
        if self.parsed.is_some() {
            return Ok(());
        }

        self.reader.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        self.reader.read_to_end(&mut data)?;

        let (meta, dataset_start) = if has_dicm_magic(&data) {
            let mut cursor = DataCursor::new(&data, false);
            cursor.set_position(132);
            let meta = Parser::new(true).parse_meta(&mut cursor);
            let start = cursor.position();
            (meta, start)
        } else if data.len() >= 8 && u16::from_le_bytes([data[0], data[1]]) == 0x0002 {
            let mut cursor = DataCursor::new(&data, false);
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
        let (dataset_bytes, explicit, big_endian): (&[u8], bool, bool) = match transfer_syntax {
            TransferSyntax::DeflatedExplicitLittleEndian => {
                let max_output = self.limits.max_alloc.unwrap_or(1 << 30) as usize;
                inflated = DeflateDecoder::from_slice(rest).decode_with_max(max_output);
                (&inflated, true, false)
            }
            TransferSyntax::Unknown => {
                let (explicit, big_endian) = sniff_encoding(rest);
                if ts_uid.is_some() {
                    log_warn!("DICOM: unrecognized transfer syntax, guessing encoding from data");
                }
                (rest, explicit, big_endian)
            }
            ts => {
                let mut explicit = ts.is_explicit_vr();
                if !explicit && rest.len() >= 8 && Vr::from_bytes([rest[4], rest[5]]).is_some() {
                    log_warn!("DICOM: implicit VR transfer syntax but dataset looks explicit, switching");
                    explicit = true;
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

        let mut cursor = DataCursor::new(dataset_bytes, big_endian);
        let mut dataset = Parser::new(explicit).parse_dataset(&mut cursor, 0);

        let mut elements = meta.elements;
        elements.append(&mut dataset.elements);
        let dataset = DataSet { elements };

        self.sections.clear();
        collect_info(&dataset, 0, &mut self.sections);

        self.parsed = Some(Parsed { transfer_syntax, dataset });
        Ok(())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut frames = self.decode_frames()?;

        if frames.is_empty() {
            return Err(VexelError::Custom("DICOM: no frames could be decoded".to_string()));
        }

        Ok(Image::from_frame(frames.remove(0)))
    }

    pub(crate) fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        self.load()?;
        let parsed = self.parsed.take().ok_or_else(|| VexelError::Custom("DICOM: no parsed dataset".to_string()))?;

        let result = self.decode_dataset(&parsed.dataset, parsed.transfer_syntax);
        self.parsed = Some(parsed);
        result
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        self.load()?;
        let parsed = self.parsed.take().ok_or_else(|| VexelError::Custom("DICOM: no parsed dataset".to_string()))?;

        let mut icons = Vec::new();
        find_icon_datasets(&parsed.dataset, 0, &mut icons);

        let mut thumbnails = Vec::new();
        for icon in icons {
            match self.decode_dataset(&icon, TransferSyntax::ImplicitLittleEndian) {
                Ok(frames) => thumbnails.extend(frames.into_iter().map(Image::from_frame)),
                Err(e) => log_warn!("DICOM: failed to decode icon image: {}", e),
            }
        }

        self.parsed = Some(parsed);
        Ok(thumbnails)
    }

    fn decode_dataset(&mut self, dataset: &DataSet, transfer_syntax: TransferSyntax) -> VexelResult<Vec<ImageFrame>> {
        let pixel_element = dataset
            .get(tags::PIXEL_DATA)
            .or_else(|| dataset.get(tags::FLOAT_PIXEL_DATA))
            .or_else(|| dataset.get(tags::DOUBLE_FLOAT_PIXEL_DATA))
            .ok_or_else(|| {
                let sop_class = dataset.string(tags::SOP_CLASS_UID);
                match sop_class {
                    Some(uid) => VexelError::Custom(format!(
                        "DICOM: file contains no pixel data (SOP Class UID: {}) - this is likely a non-image \
                         object such as a structured report or surface/mesh segmentation, not a decodable image",
                        uid
                    )),
                    None => VexelError::Custom(
                        "DICOM: file contains no pixel data - this is likely a non-image object, not a decodable \
                         image"
                            .to_string(),
                    ),
                }
            })?;

        let params = read_pixel_params(dataset, transfer_syntax)?;

        self.limits.check_dimensions(params.cols, params.rows)?;

        let transform = if params.photometric.is_monochrome() {
            read_mono_transform(dataset, &params)
        } else {
            MonoTransform::identity()
        };

        let palette = read_palette(dataset, &params);

        let mut num_frames = dataset.int_value(tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as usize;

        let frame_pixels = match pixel_element {
            Element { tag, value: ElementValue::Bytes(bytes), .. } => {
                if *tag == tags::FLOAT_PIXEL_DATA || *tag == tags::DOUBLE_FLOAT_PIXEL_DATA {
                    self.decode_float_frames(bytes, *tag, &params, &transform, &mut num_frames)?
                } else {
                    self.decode_native_frames(bytes, &params, &transform, palette.as_ref(), &mut num_frames)?
                }
            }
            Element { value: ElementValue::Fragments { offset_table, fragments }, .. } => self.decode_encapsulated(
                transfer_syntax,
                offset_table,
                fragments,
                &params,
                &transform,
                palette.as_ref(),
                num_frames,
            )?,
            _ => return Err(VexelError::Custom("DICOM: pixel data element has unexpected structure".to_string())),
        };

        if frame_pixels.is_empty() {
            return Err(VexelError::Custom("DICOM: no frames could be decoded".to_string()));
        }

        let delay = frame_delay_ms(dataset, frame_pixels.len());

        let mut frames = Vec::with_capacity(frame_pixels.len());
        for mut pixels in frame_pixels {
            pixels.correct_pixels(params.cols, params.rows);
            frames.push(ImageFrame::new(params.cols, params.rows, pixels, delay));
        }

        Ok(frames)
    }

    fn decode_native_frames(
        &mut self,
        bytes: &[u8],
        params: &PixelParams,
        transform: &MonoTransform,
        palette: Option<&[Lut; 3]>,
        num_frames: &mut usize,
    ) -> VexelResult<Vec<PixelData>> {
        let frame_size = params.frame_size_bytes();
        if frame_size == 0 {
            return Err(VexelError::InvalidDimensions { width: params.cols, height: params.rows });
        }

        let available_frames = if params.bits_allocated == 1 {
            (bytes.len() * 8) / params.pixels_per_frame().max(1)
        } else {
            bytes.len() / frame_size
        };

        if available_frames < *num_frames {
            log_warn!("DICOM: pixel data holds {} frames but {} declared", available_frames, num_frames);
            *num_frames = available_frames.max(1);
        }

        self.reserve_output(params, *num_frames)?;

        let mut stored = Vec::with_capacity(*num_frames);
        for i in 0..*num_frames {
            if params.bits_allocated == 1 {
                let pixels = params.pixels_per_frame();
                let bit_start = i * pixels;
                let mut frame_bits = Vec::with_capacity(pixels);
                for b in 0..pixels {
                    let bit = bit_start + b;
                    let byte = bytes.get(bit / 8).copied().unwrap_or(0);
                    frame_bits.push((byte >> (bit % 8)) & 1);
                }
                stored.push(StoredFrame::U8(frame_bits));
            } else {
                let start = (i * frame_size).min(bytes.len());
                let end = ((i + 1) * frame_size).min(bytes.len());
                stored.push(extract_native_frame(&bytes[start..end], params));
            }
        }

        self.stored_frames_to_pixels(&stored, params, transform, palette)
    }

    fn decode_float_frames(
        &mut self,
        bytes: &[u8],
        tag: u32,
        params: &PixelParams,
        transform: &MonoTransform,
        num_frames: &mut usize,
    ) -> VexelResult<Vec<PixelData>> {
        let pixels = params.pixels_per_frame();
        let sample_size = if tag == tags::DOUBLE_FLOAT_PIXEL_DATA { 8 } else { 4 };
        let frame_size = pixels * sample_size;

        if frame_size == 0 {
            return Err(VexelError::InvalidDimensions { width: params.cols, height: params.rows });
        }

        let available = bytes.len() / frame_size;
        if available < *num_frames {
            log_warn!("DICOM: float pixel data holds {} frames but {} declared", available, num_frames);
            *num_frames = available.max(1);
        }

        self.limits.reserve_usize(pixels * sample_size * *num_frames)?;

        let mut stored = Vec::with_capacity(*num_frames);
        for i in 0..*num_frames {
            let start = (i * frame_size).min(bytes.len());
            let end = ((i + 1) * frame_size).min(bytes.len());
            let frame = &bytes[start..end];

            if sample_size == 4 {
                let mut values = vec![0f32; pixels];
                for (j, v) in values.iter_mut().enumerate() {
                    let p = j * 4;
                    if p + 4 <= frame.len() {
                        *v = f32::from_le_bytes([frame[p], frame[p + 1], frame[p + 2], frame[p + 3]]);
                    }
                }
                stored.push(StoredFrame::F32(values));
            } else {
                let mut values = vec![0f64; pixels];
                for (j, v) in values.iter_mut().enumerate() {
                    let p = j * 8;
                    if p + 8 <= frame.len() {
                        *v = f64::from_le_bytes([
                            frame[p],
                            frame[p + 1],
                            frame[p + 2],
                            frame[p + 3],
                            frame[p + 4],
                            frame[p + 5],
                            frame[p + 6],
                            frame[p + 7],
                        ]);
                    }
                }
                stored.push(StoredFrame::F64(values));
            }
        }

        Ok(float_frames_to_pixels(&stored, transform))
    }

    fn decode_encapsulated(
        &mut self,
        transfer_syntax: TransferSyntax,
        offset_table: &[u32],
        fragments: &[Vec<u8>],
        params: &PixelParams,
        transform: &MonoTransform,
        palette: Option<&[Lut; 3]>,
        num_frames: usize,
    ) -> VexelResult<Vec<PixelData>> {
        if fragments.is_empty() {
            return Err(VexelError::Custom("DICOM: encapsulated pixel data has no fragments".to_string()));
        }

        match transfer_syntax {
            TransferSyntax::Jpeg2000 => {
                return Err(VexelError::UnsupportedFormat("DICOM: JPEG 2000 transfer syntax".to_string()))
            }
            TransferSyntax::JpegXl => {
                return Err(VexelError::UnsupportedFormat("DICOM: JPEG XL transfer syntax".to_string()))
            }
            TransferSyntax::Video => {
                return Err(VexelError::UnsupportedFormat("DICOM: video (MPEG/HEVC) transfer syntax".to_string()))
            }
            TransferSyntax::Unknown => {
                log_warn!("DICOM: encapsulated pixel data with unknown transfer syntax, assuming JPEG");
            }
            _ => {}
        }

        let frames_data = group_fragments(offset_table, fragments, num_frames);
        self.reserve_output(params, frames_data.len())?;

        let mut mono_stored: Vec<StoredFrame> = Vec::new();
        let mut color_frames: Vec<PixelData> = Vec::new();
        let mono = params.photometric.is_monochrome() || params.photometric == Photometric::PaletteColor;

        for frame_bytes in &frames_data {
            match transfer_syntax {
                TransferSyntax::Rle => {
                    let bytes_per_sample = (params.bits_allocated as usize).div_ceil(8).max(1);
                    let decoded = decode_rle_frame(
                        frame_bytes,
                        params.pixels_per_frame(),
                        params.samples_per_pixel.max(1) as usize,
                        bytes_per_sample,
                    )?;
                    let mut rle_params = params.clone();
                    rle_params.planar = false;
                    if rle_params.photometric == Photometric::YbrFull422 {
                        rle_params.photometric = Photometric::YbrFull;
                    }
                    let stored = extract_native_frame(&decoded, &rle_params);
                    if mono {
                        mono_stored.push(stored);
                    } else {
                        color_frames.push(color_frame_to_pixels(&stored, &rle_params, palette)?);
                    }
                }
                TransferSyntax::EncapsulatedUncompressed | TransferSyntax::DeflatedImageFrameCompression => {
                    let raw;
                    let data: &[u8] = if transfer_syntax == TransferSyntax::DeflatedImageFrameCompression {
                        let max_output = self.limits.max_alloc.unwrap_or(1 << 30) as usize;
                        raw = DeflateDecoder::from_slice(frame_bytes).decode_with_max(max_output);
                        &raw
                    } else {
                        frame_bytes
                    };
                    let stored = extract_native_frame(data, params);
                    if mono {
                        mono_stored.push(stored);
                    } else {
                        color_frames.push(color_frame_to_pixels(&stored, params, palette)?);
                    }
                }
                TransferSyntax::JpegLs => {
                    let mut decoder = JpegLsDecoder::new(Cursor::new(frame_bytes.clone()));
                    decoder.set_limits(self.limits.clone());
                    let image = decoder.decode()?;
                    let precision = decoder.precision();
                    absorb_codec_frame(image, params, palette, false, precision, &mut mono_stored, &mut color_frames)?;
                }
                _ => {
                    let mut decoder = JpegDecoder::new(Cursor::new(frame_bytes.clone()));
                    decoder.set_limits(self.limits.clone());
                    let image = decoder.decode()?;
                    let dct = transfer_syntax == TransferSyntax::JpegDct;
                    let precision = decoder.precision();
                    absorb_codec_frame(image, params, palette, dct, precision, &mut mono_stored, &mut color_frames)?;
                }
            }
        }

        if mono {
            if params.photometric == Photometric::PaletteColor {
                let mut out = Vec::with_capacity(mono_stored.len());
                for stored in &mono_stored {
                    out.push(color_frame_to_pixels(stored, params, palette)?);
                }
                Ok(out)
            } else {
                Ok(mono_frames_to_pixels(&mono_stored, params, transform))
            }
        } else {
            Ok(color_frames)
        }
    }

    fn stored_frames_to_pixels(
        &mut self,
        stored: &[StoredFrame],
        params: &PixelParams,
        transform: &MonoTransform,
        palette: Option<&[Lut; 3]>,
    ) -> VexelResult<Vec<PixelData>> {
        if params.photometric.is_monochrome() && params.samples_per_pixel <= 1 {
            return Ok(mono_frames_to_pixels(stored, params, transform));
        }

        let mut out = Vec::with_capacity(stored.len());
        for frame in stored {
            out.push(color_frame_to_pixels(frame, params, palette)?);
        }
        Ok(out)
    }

    fn reserve_output(&mut self, params: &PixelParams, num_frames: usize) -> VexelResult<()> {
        let bytes_per_pixel: u64 = if params.photometric.is_monochrome() && params.samples_per_pixel <= 1 {
            if params.bits_stored > 8 {
                2
            } else {
                1
            }
        } else if params.bits_stored > 8 {
            6
        } else {
            3
        };

        let total = params.pixels_per_frame() as u64 * bytes_per_pixel * num_frames as u64;
        self.limits.reserve(total)
    }
}

fn absorb_codec_frame(
    image: Image,
    params: &PixelParams,
    palette: Option<&[Lut; 3]>,
    dct: bool,
    codec_precision: u8,
    mono_stored: &mut Vec<StoredFrame>,
    color_frames: &mut Vec<PixelData>,
) -> VexelResult<()> {
    let mono = params.photometric.is_monochrome() || params.photometric == Photometric::PaletteColor;
    let pixels = image.into_pixels();

    if mono {
        let stored = match pixels {
            PixelData::L8(v) => StoredFrame::U8(v),
            PixelData::L16(v) => {
                let precision = if codec_precision > 8 && codec_precision <= 16 {
                    codec_precision as u16
                } else {
                    params.bits_stored
                };
                let shift = if precision > 8 && precision < 16 { 16 - precision as u32 } else { 0 };
                StoredFrame::U16(v.into_iter().map(|s| s >> shift).collect())
            }
            PixelData::RGB8(v) => StoredFrame::U8(v.chunks_exact(3).map(|c| c[0]).collect()),
            PixelData::RGB16(v) => StoredFrame::U16(v.chunks_exact(3).map(|c| c[0]).collect()),
            other => {
                log_warn!("DICOM: unexpected codec output {:?} for monochrome frame", other.pixel_format());
                StoredFrame::U8(other.into_rgb8_bytes().chunks_exact(3).map(|c| c[0]).collect())
            }
        };
        mono_stored.push(stored);
        return Ok(());
    }

    let needs_ybr = params.photometric.is_ybr() && !dct;
    match pixels {
        PixelData::RGB8(v) if needs_ybr => {
            let mut ybr_params = params.clone();
            ybr_params.planar = false;
            ybr_params.bits_allocated = 8;
            ybr_params.bits_stored = 8;
            ybr_params.high_bit = 7;
            if ybr_params.photometric == Photometric::YbrFull422 {
                ybr_params.photometric = Photometric::YbrFull;
            }
            color_frames.push(color_frame_to_pixels(&StoredFrame::U8(v), &ybr_params, palette)?);
        }
        other => color_frames.push(other),
    }

    Ok(())
}

fn read_pixel_params(dataset: &DataSet, transfer_syntax: TransferSyntax) -> VexelResult<PixelParams> {
    let rows = dataset.u16_value(tags::ROWS).unwrap_or(0) as u32;
    let cols = dataset.u16_value(tags::COLUMNS).unwrap_or(0) as u32;

    if rows == 0 || cols == 0 {
        return Err(VexelError::InvalidDimensions { width: cols, height: rows });
    }

    let samples_per_pixel = dataset.u16_value(tags::SAMPLES_PER_PIXEL).unwrap_or(1).max(1);

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

fn read_mono_transform(dataset: &DataSet, params: &PixelParams) -> MonoTransform {
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

fn read_palette(dataset: &DataSet, params: &PixelParams) -> Option<[Lut; 3]> {
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

fn frame_delay_ms(dataset: &DataSet, num_frames: usize) -> u32 {
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

fn group_fragments(offset_table: &[u32], fragments: &[Vec<u8>], num_frames: usize) -> Vec<Vec<u8>> {
    if num_frames <= 1 {
        if fragments.len() == 1 {
            return vec![fragments[0].clone()];
        }
        return vec![fragments.concat()];
    }

    if fragments.len() == num_frames {
        return fragments.to_vec();
    }

    if offset_table.len() == num_frames {
        let mut cumulative = Vec::with_capacity(fragments.len());
        let mut pos = 0u64;
        for fragment in fragments {
            cumulative.push(pos);
            pos += 8 + fragment.len() as u64;
        }

        let mut frames = Vec::with_capacity(num_frames);
        for f in 0..num_frames {
            let start_offset = offset_table[f] as u64;
            let end_offset = if f + 1 < num_frames { offset_table[f + 1] as u64 } else { u64::MAX };

            let mut frame = Vec::new();
            for (i, fragment) in fragments.iter().enumerate() {
                if cumulative[i] >= start_offset && cumulative[i] < end_offset {
                    frame.extend_from_slice(fragment);
                }
            }
            frames.push(frame);
        }
        return frames;
    }

    let mut frames: Vec<Vec<u8>> = Vec::new();
    for fragment in fragments {
        let starts_new = fragment.len() >= 2 && fragment[0] == 0xFF && fragment[1] == 0xD8;
        if starts_new || frames.is_empty() {
            frames.push(fragment.clone());
        } else if let Some(last) = frames.last_mut() {
            last.extend_from_slice(fragment);
        }
    }

    if frames.len() != num_frames {
        log_warn!(
            "DICOM: could not map {} fragments to {} frames, decoded {}",
            fragments.len(),
            num_frames,
            frames.len()
        );
    }

    frames
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

fn collect_info(dataset: &DataSet, depth: u8, out: &mut Vec<DicomElementInfo>) {
    if depth > 6 {
        return;
    }

    for element in &dataset.elements {
        out.push(DicomElementInfo {
            group: tag_group(element.tag),
            element: tag_element(element.tag),
            vr: element.vr.as_str().to_string(),
            length: element.length,
            name: tag_name(element.tag).unwrap_or("").to_string(),
            value: element_preview(element),
            depth,
        });

        if let ElementValue::Sequence(items) = &element.value {
            for item in items {
                collect_info(item, depth + 1, out);
            }
        }
    }
}
