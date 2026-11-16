use std::borrow::Cow;
use std::io::Cursor;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use crate::decoders::dicom::decoder::{frame_delay_ms, read_mono_transform, read_palette, read_pixel_params};
use crate::decoders::dicom::parser::{DataSet, Element, ElementValue};
use crate::decoders::dicom::pixels::{
    color_frame_to_pixels, extract_native_frame, uses_data_scale, FloatMapper, Lut, MonoMapper, MonoTransform,
    PixelParams, SampleRange, StoredFrame,
};
use crate::decoders::dicom::rle::decode_rle_frame;
use crate::decoders::dicom::types::{tags, Photometric, TransferSyntax};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::jpeg2000::Jpeg2000Decoder;
use crate::decoders::jpeg_ls::JpegLsDecoder;
use crate::utils::codecs::deflate::DeflateDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::frames::{FramePrefetch, FrameSource};
use crate::utils::image::{Image, ImageFrame, PixelData};
use crate::{log_warn, Limits};

enum Layout {
    Native { frame_size: usize },
    Float { sample_size: usize },
    Encapsulated { frames: Vec<Range<usize>> },
}

enum Output {
    Color,
    Mono(MonoMapper),
    Float(FloatMapper),
}

enum Decoded {
    Stored(StoredFrame),
    Pixels(PixelData),
}

pub(super) struct FramePlan {
    dataset: Arc<DataSet>,
    pixel_tag: u32,
    transfer_syntax: TransferSyntax,
    params: PixelParams,
    transform: MonoTransform,
    palette: Option<[Lut; 3]>,
    layout: Layout,
    count: usize,
    delay: u32,
    limits: Limits,
    output: Output,
    cache: Vec<Mutex<Option<VexelResult<Decoded>>>>,
}

impl FramePlan {
    pub(super) fn new(
        dataset: Arc<DataSet>,
        transfer_syntax: TransferSyntax,
        limits: &mut Limits,
    ) -> VexelResult<FramePlan> {
        let pixel_element = pixel_element(&dataset)?;
        let pixel_tag = pixel_element.tag;

        let params = read_pixel_params(&dataset, transfer_syntax)?;
        limits.check_dimensions(params.cols, params.rows)?;

        let transform = if params.photometric.is_monochrome() {
            read_mono_transform(&dataset, &params)
        } else {
            MonoTransform::identity()
        };
        let palette = read_palette(&dataset, &params);
        let declared_frames = dataset.int_value(tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as usize;

        let (layout, count, kind) = match &pixel_element.value {
            ElementValue::Bytes(bytes)
                if pixel_tag == tags::FLOAT_PIXEL_DATA || pixel_tag == tags::DOUBLE_FLOAT_PIXEL_DATA =>
            {
                let sample_size = if pixel_tag == tags::DOUBLE_FLOAT_PIXEL_DATA { 8 } else { 4 };
                let frame_size = params.pixels_per_frame() * sample_size;
                if frame_size == 0 {
                    return Err(VexelError::InvalidDimensions { width: params.cols, height: params.rows });
                }

                let available = bytes.len() / frame_size;
                let count = if available < declared_frames {
                    log_warn!("DICOM: float pixel data holds {} frames but {} declared", available, declared_frames);
                    available.max(1)
                } else {
                    declared_frames
                };

                limits.reserve_usize(frame_size)?;
                (Layout::Float { sample_size }, count, OutputKind::Float)
            }
            ElementValue::Bytes(bytes) => {
                let frame_size = params.frame_size_bytes();
                if frame_size == 0 {
                    return Err(VexelError::InvalidDimensions { width: params.cols, height: params.rows });
                }

                let available = if params.bits_allocated == 1 {
                    (bytes.len() * 8) / params.pixels_per_frame().max(1)
                } else {
                    bytes.len() / frame_size
                };
                let count = if available < declared_frames {
                    log_warn!("DICOM: pixel data holds {} frames but {} declared", available, declared_frames);
                    available.max(1)
                } else {
                    declared_frames
                };

                let kind = if params.photometric.is_monochrome() && params.samples_per_pixel <= 1 {
                    OutputKind::Mono
                } else {
                    OutputKind::Color
                };
                reserve_output(limits, &params, 1)?;
                (Layout::Native { frame_size }, count, kind)
            }
            ElementValue::Fragments { offset_table, fragments, .. } => {
                check_encapsulated(transfer_syntax, fragments)?;

                let frames = group_fragments(offset_table, fragments, declared_frames);
                let count = frames.len();
                let kind = if params.photometric.is_monochrome() && params.photometric != Photometric::PaletteColor {
                    OutputKind::Mono
                } else {
                    OutputKind::Color
                };
                let held = if kind == OutputKind::Mono && uses_data_scale(&transform) { count } else { 1 };
                reserve_output(limits, &params, held)?;
                (Layout::Encapsulated { frames }, count, kind)
            }
            _ => return Err(VexelError::Custom("DICOM: pixel data element has unexpected structure".to_string())),
        };

        if count == 0 {
            return Err(VexelError::Custom("DICOM: no frames could be decoded".to_string()));
        }

        let delay = frame_delay_ms(&dataset, count);
        let mut plan = FramePlan {
            dataset,
            pixel_tag,
            transfer_syntax,
            params,
            transform,
            palette,
            layout,
            count,
            delay,
            limits: limits.clone(),
            output: Output::Color,
            cache: Vec::new(),
        };

        plan.output = match kind {
            OutputKind::Color => Output::Color,
            OutputKind::Mono if uses_data_scale(&plan.transform) => {
                let range = plan.scan_range(limits, |frame, plan| {
                    SampleRange::of_mono_frame(frame, &plan.params, &plan.transform)
                })?;
                Output::Mono(MonoMapper::new(&plan.params, &plan.transform, Some(range)))
            }
            OutputKind::Mono => Output::Mono(MonoMapper::new(&plan.params, &plan.transform, None)),
            OutputKind::Float => {
                let range = plan.scan_range(limits, |frame, _| SampleRange::of_float_frame(frame))?;
                Output::Float(FloatMapper::new(&plan.transform, range))
            }
        };

        Ok(plan)
    }

    pub(super) fn into_source(self, lookahead: bool) -> DicomFrames {
        let plan = Arc::new(self);
        let frame_bytes = (plan.params.pixels_per_frame() as u64).saturating_mul(output_bytes_per_pixel(&plan.params));
        let costs = vec![usize::try_from(frame_bytes).unwrap_or(usize::MAX); plan.count];
        let shared = plan.clone();
        let prefetch = FramePrefetch::new(costs, lookahead, move |index| shared.decode_frame(index));

        DicomFrames { plan, prefetch, next_index: 0, emitted: 0, first_error: None, exhausted: false }
    }

    fn scan_range(
        &mut self,
        limits: &mut Limits,
        measure: impl Fn(&StoredFrame, &FramePlan) -> SampleRange + Sync,
    ) -> VexelResult<SampleRange> {
        let keep = matches!(self.layout, Layout::Encapsulated { .. });
        let plan: &FramePlan = self;
        let measure_one = |index: usize| {
            let decoded = plan.decode_stage(index);
            let range = match &decoded {
                Ok(Decoded::Stored(frame)) => measure(frame, plan),
                _ => SampleRange::empty(),
            };
            (range, keep.then_some(decoded))
        };

        #[cfg(feature = "rayon")]
        let measured: Vec<(SampleRange, Option<VexelResult<Decoded>>)> = {
            use rayon::prelude::*;
            (0..plan.count).into_par_iter().map(measure_one).collect()
        };

        #[cfg(not(feature = "rayon"))]
        let measured: Vec<(SampleRange, Option<VexelResult<Decoded>>)> = (0..plan.count).map(measure_one).collect();

        let mut range = SampleRange::empty();
        let mut cache = Vec::with_capacity(if keep { self.count } else { 0 });
        for (frame_range, decoded) in measured {
            range = range.merge(frame_range);
            if let Some(decoded) = decoded {
                if let Ok(decoded) = &decoded {
                    charge_cached_frame(limits, decoded, &self.params)?;
                }
                cache.push(Mutex::new(Some(decoded)));
            }
        }

        self.cache = cache;
        Ok(range)
    }

    fn pixel_data(&self) -> Option<&ElementValue> {
        self.dataset.get(self.pixel_tag).map(|element| &element.value)
    }

    fn decode_frame(&self, index: usize) -> VexelResult<PixelData> {
        let cached = self.cache.get(index).and_then(|slot| slot.lock().ok().and_then(|mut slot| slot.take()));
        let decoded = match cached {
            Some(decoded) => decoded?,
            None => self.decode_stage(index)?,
        };

        let mut pixels = match (decoded, &self.output) {
            (Decoded::Pixels(pixels), _) => pixels,
            (Decoded::Stored(frame), Output::Mono(mapper)) => mapper.map(&frame),
            (Decoded::Stored(frame), Output::Float(mapper)) => mapper.map(&frame),
            (Decoded::Stored(frame), Output::Color) => {
                color_frame_to_pixels(&frame, &self.params, self.palette.as_ref())?
            }
        };
        pixels.correct_pixels(self.params.cols, self.params.rows);

        Ok(pixels)
    }

    fn decode_stage(&self, index: usize) -> VexelResult<Decoded> {
        let bytes = match self.pixel_data() {
            Some(ElementValue::Bytes(bytes)) => bytes.as_slice(),
            Some(ElementValue::Fragments { fragments, .. }) => return self.decode_encapsulated(fragments, index),
            _ => return Err(VexelError::Custom("DICOM: pixel data element has unexpected structure".to_string())),
        };

        let params = &self.params;
        let stored = match self.layout {
            Layout::Native { .. } if params.bits_allocated == 1 => {
                let pixels = params.pixels_per_frame();
                let bit_start = index * pixels;
                let frame_bits = (0..pixels)
                    .map(|b| {
                        let bit = bit_start + b;
                        let byte = bytes.get(bit / 8).copied().unwrap_or(0);
                        (byte >> (bit % 8)) & 1
                    })
                    .collect();
                StoredFrame::U8(frame_bits)
            }
            Layout::Native { frame_size } => {
                let start = (index * frame_size).min(bytes.len());
                let end = ((index + 1) * frame_size).min(bytes.len());
                extract_native_frame(&bytes[start..end], params)
            }
            Layout::Float { sample_size } => {
                let pixels = params.pixels_per_frame();
                let frame_size = pixels * sample_size;
                let start = (index * frame_size).min(bytes.len());
                let end = ((index + 1) * frame_size).min(bytes.len());
                read_float_frame(&bytes[start..end], pixels, sample_size)
            }
            Layout::Encapsulated { .. } => {
                return Err(VexelError::Custom("DICOM: pixel data element has unexpected structure".to_string()))
            }
        };

        Ok(Decoded::Stored(stored))
    }

    fn decode_encapsulated(&self, fragments: &[Vec<u8>], index: usize) -> VexelResult<Decoded> {
        let Layout::Encapsulated { frames } = &self.layout else {
            return Err(VexelError::Custom("DICOM: pixel data element has unexpected structure".to_string()));
        };
        let range = frames.get(index).cloned().unwrap_or_default();
        let frame_bytes: Cow<[u8]> = match fragments.get(range.clone()) {
            Some([single]) => Cow::Borrowed(single.as_slice()),
            Some(parts) => Cow::Owned(parts.concat()),
            None => Cow::Borrowed(&[]),
        };

        let params = &self.params;
        let palette = self.palette.as_ref();
        let mono = params.photometric.is_monochrome() || params.photometric == Photometric::PaletteColor;

        match self.transfer_syntax {
            TransferSyntax::Rle => {
                let bytes_per_sample = (params.bits_allocated as usize).div_ceil(8).max(1);
                let decoded = decode_rle_frame(
                    &frame_bytes,
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
                    Ok(Decoded::Stored(stored))
                } else {
                    Ok(Decoded::Pixels(color_frame_to_pixels(&stored, &rle_params, palette)?))
                }
            }
            TransferSyntax::EncapsulatedUncompressed | TransferSyntax::DeflatedImageFrameCompression => {
                let inflated;
                let data: &[u8] = if self.transfer_syntax == TransferSyntax::DeflatedImageFrameCompression {
                    inflated = DeflateDecoder::from_slice(&frame_bytes).decode_with_max(params.frame_size_bytes());
                    &inflated
                } else {
                    &frame_bytes
                };
                let stored = extract_native_frame(data, params);
                if mono {
                    Ok(Decoded::Stored(stored))
                } else {
                    Ok(Decoded::Pixels(color_frame_to_pixels(&stored, params, palette)?))
                }
            }
            TransferSyntax::JpegLs => {
                let mut decoder = JpegLsDecoder::new(Cursor::new(frame_bytes.as_ref()));
                decoder.set_limits(self.limits.clone());
                let image = decoder.decode()?;
                let precision = decoder.precision();
                absorb_codec_frame(image, params, palette, false, precision, false)
            }
            TransferSyntax::Jpeg2000 => {
                let mut decoder = Jpeg2000Decoder::new(Cursor::new(frame_bytes.as_ref()));
                decoder.set_limits(self.limits.clone());
                let image = decoder.decode()?;
                let precision = decoder.precision();
                let signed = decoder.is_signed();
                absorb_codec_frame(image, params, palette, true, precision, signed)
            }
            _ => {
                let mut decoder = JpegDecoder::new(Cursor::new(frame_bytes.as_ref()));
                decoder.set_limits(self.limits.clone());
                let image = decoder.decode()?;
                let dct = self.transfer_syntax == TransferSyntax::JpegDct;
                let precision = decoder.precision();
                absorb_codec_frame(image, params, palette, dct, precision, false)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputKind {
    Color,
    Mono,
    Float,
}

pub(super) struct DicomFrames {
    plan: Arc<FramePlan>,
    prefetch: FramePrefetch<VexelResult<PixelData>>,
    next_index: usize,
    emitted: usize,
    first_error: Option<VexelError>,
    exhausted: bool,
}

impl DicomFrames {
    pub(super) fn collect_images(mut self) -> VexelResult<Vec<Image>> {
        let mut images = Vec::new();
        while let Some(frame) = self.next_frame()? {
            images.push(Image::from_frame(frame));
        }
        Ok(images)
    }
}

impl FrameSource for DicomFrames {
    fn next_frame(&mut self) -> VexelResult<Option<ImageFrame>> {
        while !self.exhausted {
            let index = self.next_index;
            let decoded = match self.prefetch.next() {
                Ok(Some(decoded)) => decoded,
                Ok(None) => {
                    self.exhausted = true;
                    break;
                }
                Err(e) => {
                    self.exhausted = true;
                    if self.emitted == 0 {
                        return Err(e);
                    }
                    log_warn!("DICOM: stopping after {} frames: {}", self.emitted, e);
                    return Ok(None);
                }
            };
            self.next_index += 1;

            match decoded {
                Ok(pixels) => {
                    self.emitted += 1;
                    let params = &self.plan.params;
                    return Ok(Some(ImageFrame::new(params.cols, params.rows, pixels, self.plan.delay)));
                }
                Err(e) => {
                    log_warn!("DICOM: failed to decode frame {}: {}", index, e);
                    self.first_error.get_or_insert(e);
                }
            }
        }

        if self.emitted == 0 {
            if let Some(e) = self.first_error.take() {
                return Err(e);
            }
        }

        Ok(None)
    }

    fn frame_count_hint(&self) -> Option<usize> {
        if self.exhausted {
            return Some(0);
        }
        Some(self.plan.count.saturating_sub(self.next_index))
    }
}

fn pixel_element(dataset: &DataSet) -> VexelResult<&Element> {
    dataset
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
                    "DICOM: file contains no pixel data - this is likely a non-image object, not a decodable image"
                        .to_string(),
                ),
            }
        })
}

fn check_encapsulated(transfer_syntax: TransferSyntax, fragments: &[Vec<u8>]) -> VexelResult<()> {
    if fragments.is_empty() {
        return Err(VexelError::Custom("DICOM: encapsulated pixel data has no fragments".to_string()));
    }

    match transfer_syntax {
        TransferSyntax::JpegXl => Err(VexelError::UnsupportedFormat("DICOM: JPEG XL transfer syntax".to_string())),
        TransferSyntax::Video => {
            Err(VexelError::UnsupportedFormat("DICOM: video (MPEG/HEVC) transfer syntax".to_string()))
        }
        TransferSyntax::Unknown => {
            log_warn!("DICOM: encapsulated pixel data with unknown transfer syntax, assuming JPEG");
            Ok(())
        }
        _ => Ok(()),
    }
}

fn read_float_frame(frame: &[u8], pixels: usize, sample_size: usize) -> StoredFrame {
    if sample_size == 4 {
        let mut values = vec![0f32; pixels];
        for (j, v) in values.iter_mut().enumerate() {
            let p = j * 4;
            if let Some(bytes) = frame.get(p..p + 4) {
                *v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            }
        }
        StoredFrame::F32(values)
    } else {
        let mut values = vec![0f64; pixels];
        for (j, v) in values.iter_mut().enumerate() {
            let p = j * 8;
            if let Some(bytes) = frame.get(p..p + 8) {
                *v = f64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]);
            }
        }
        StoredFrame::F64(values)
    }
}

fn reserve_output(limits: &mut Limits, params: &PixelParams, num_frames: usize) -> VexelResult<()> {
    let pixels = params.pixels_per_frame() as u64;
    let output = pixels.saturating_mul(output_bytes_per_pixel(params)).saturating_mul(num_frames as u64);
    let total = output.saturating_add(pixels.saturating_mul(working_bytes_per_pixel(params)));
    limits.reserve(total)
}

fn charge_cached_frame(limits: &mut Limits, decoded: &Decoded, params: &PixelParams) -> VexelResult<()> {
    let expected = (params.pixels_per_frame() as u64).saturating_mul(output_bytes_per_pixel(params));
    let actual = match decoded {
        Decoded::Pixels(pixels) => pixels.as_bytes().len(),
        Decoded::Stored(StoredFrame::U8(v)) => v.len(),
        Decoded::Stored(StoredFrame::U16(v)) => v.len() * 2,
        Decoded::Stored(StoredFrame::U32(v)) => v.len() * 4,
        Decoded::Stored(StoredFrame::F32(v)) => v.len() * 4,
        Decoded::Stored(StoredFrame::F64(v)) => v.len() * 8,
    } as u64;
    limits.reserve(actual.saturating_sub(expected))
}

fn output_bytes_per_pixel(params: &PixelParams) -> u64 {
    if params.photometric.is_monochrome() && params.samples_per_pixel <= 1 {
        if params.bits_stored > 8 {
            2
        } else {
            1
        }
    } else if params.bits_stored > 8 {
        6
    } else {
        3
    }
}

fn working_bytes_per_pixel(params: &PixelParams) -> u64 {
    let samples = params.samples_per_pixel.max(1) as u64;
    let container = match params.bits_allocated {
        0..=8 => 1,
        9..=16 => 2,
        _ => 4,
    };
    let color = if params.photometric.is_monochrome() && params.samples_per_pixel <= 1 { 0 } else { 4 * samples + 8 };
    samples * container * 2 + color
}

fn rebase_signed_samples(samples: Vec<u16>, precision: u16, shift: u32, params: &PixelParams) -> Vec<u16> {
    let bias = 1i32 << (precision.clamp(1, 16) - 1);
    let stored_bits = params.bits_stored.clamp(1, 16) as u32;
    let container_mask = if stored_bits >= 16 { u16::MAX } else { (1u16 << stored_bits) - 1 };

    samples
        .into_iter()
        .map(|s| {
            let value = ((s >> shift) as i32) - bias;
            (value as u16) & container_mask
        })
        .collect()
}

fn absorb_codec_frame(
    image: Image,
    params: &PixelParams,
    palette: Option<&[Lut; 3]>,
    dct: bool,
    codec_precision: u8,
    codec_signed: bool,
) -> VexelResult<Decoded> {
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

                if codec_signed {
                    StoredFrame::U16(rebase_signed_samples(v, precision, shift, params))
                } else {
                    StoredFrame::U16(v.into_iter().map(|s| s >> shift).collect())
                }
            }
            PixelData::RGB8(v) => StoredFrame::U8(v.chunks_exact(3).map(|c| c[0]).collect()),
            PixelData::RGB16(v) => StoredFrame::U16(v.chunks_exact(3).map(|c| c[0]).collect()),
            other => {
                log_warn!("DICOM: unexpected codec output {:?} for monochrome frame", other.pixel_format());
                StoredFrame::U8(other.into_rgb8_bytes().chunks_exact(3).map(|c| c[0]).collect())
            }
        };
        return Ok(Decoded::Stored(stored));
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
            Ok(Decoded::Pixels(color_frame_to_pixels(&StoredFrame::U8(v), &ybr_params, palette)?))
        }
        other => Ok(Decoded::Pixels(other)),
    }
}

fn group_fragments(offset_table: &[u32], fragments: &[Vec<u8>], num_frames: usize) -> Vec<Range<usize>> {
    if num_frames <= 1 {
        return std::iter::once(0..fragments.len()).collect();
    }

    if fragments.len() == num_frames {
        return (0..num_frames).map(|index| index..index + 1).collect();
    }

    if offset_table.len() == num_frames && offset_table.windows(2).all(|w| w[0] <= w[1]) {
        let mut frames: Vec<Range<usize>> = Vec::new();
        let mut current = None;
        let mut frame_index = 0;
        let mut pos = 0u64;
        for (index, fragment) in fragments.iter().enumerate() {
            while frame_index + 1 < num_frames && pos >= offset_table[frame_index + 1] as u64 {
                frame_index += 1;
            }
            if pos >= offset_table[frame_index] as u64 {
                if current != Some(frame_index) {
                    frames.push(index..index);
                    current = Some(frame_index);
                }
                if let Some(frame) = frames.last_mut() {
                    frame.end = index + 1;
                }
            }
            pos += 8 + fragment.len() as u64;
        }
        if frames.len() != num_frames {
            log_warn!("DICOM: offset table maps fragments to {} of {} frames", frames.len(), num_frames);
        }
        return frames;
    }

    let mut frames: Vec<Range<usize>> = Vec::new();
    for (index, fragment) in fragments.iter().enumerate() {
        let starts_new = fragment.len() >= 2 && fragment[0] == 0xFF && fragment[1] == 0xD8;
        match frames.last_mut() {
            Some(last) if !starts_new => last.end = index + 1,
            _ => frames.push(index..index + 1),
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
