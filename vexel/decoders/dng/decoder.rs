use std::io::{Cursor, Read, Seek, SeekFrom};

use super::info::{build_info, DngInfo};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::float::half_to_f32;
use crate::utils::image::PixelData;
use crate::utils::raster::RasterWindow;
use crate::utils::stream::StreamReader;
use crate::utils::tiff::color::{f32_from_bytes, float24_to_f32, u16_from_bytes};
use crate::utils::tiff::compression::{apply_predictor, decompress_generic};
use crate::utils::tiff::ifd::{self, Ifd, IfdCycleGuard, IfdFormat};
use crate::utils::tiff::info::walk_structure;
use crate::utils::tiff::jpeg::splice_jpeg_tables;
use crate::utils::tiff::types::{
    Compression, PhotometricInterpretation, Predictor, SampleFormat, TiffHeader, TiffTags,
};
use crate::utils::types::ByteOrder;
use crate::{log_warn, Image, Limits};

const DNG_PHOTOMETRIC_CFA: u32 = 32803;
const DNG_PHOTOMETRIC_LINEAR_RAW: u32 = 34892;
const MAX_DNG_IFDS: usize = 64;
const MAX_BLACK_REPEAT_VALUES: u32 = 4096;
const CHUNK_JPEG_ALLOC_SLACK: usize = 16 * 1024 * 1024;
const CHUNK_READ_SLACK: u64 = 1024 * 1024;
const TILE_PADDING_SLACK: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CfaPattern {
    Rggb,
    Bggr,
    Grbg,
    Gbrg,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum BlackLevel {
    Uniform(f32),
    PerChannel(Vec<f32>),
    Spatial { values: Vec<f32>, repeat_rows: u32, repeat_cols: u32 },
}

#[derive(Debug, Clone)]
pub struct MapPolynomial {
    pub plane: u32,
    pub planes: u32,
    pub coefficients: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct DngMetadata {
    pub dng_version: u32,
    pub cfa_pattern: Option<CfaPattern>,
    pub cfa_plane_color: Vec<u8>,
    pub linearization_table: Option<Vec<u16>>,
    pub gray_response_curve: Option<Vec<u16>>,
    pub black_level: Option<BlackLevel>,
    pub black_level_repeat_rows: u32,
    pub black_level_repeat_cols: u32,
    pub white_level: Vec<u32>,
    pub color_matrix1: Option<[[f32; 3]; 3]>,
    pub color_matrix2: Option<[[f32; 3]; 3]>,
    pub as_shot_neutral: Vec<f32>,
    pub active_area: Option<[u32; 4]>,
    pub default_crop_origin: Option<[u32; 2]>,
    pub default_crop_size: Option<[u32; 2]>,
    pub photometric_raw: bool,
    pub photometric_cfa: bool,
    pub orientation: u16,
    pub map_polynomials: Vec<MapPolynomial>,
    pub make: String,
    pub model: String,
}

struct IfdQueueFilter {
    seen: std::collections::HashSet<u64>,
}

impl IfdQueueFilter {
    fn new(first: u64) -> Self {
        Self { seen: std::collections::HashSet::from([first]) }
    }

    fn extend(&mut self, queue: &mut Vec<u64>, offsets: impl IntoIterator<Item = u64>) {
        for offset in offsets {
            if self.seen.len() >= MAX_DNG_IFDS {
                return;
            }
            if offset != 0 && self.seen.insert(offset) {
                queue.push(offset);
            }
        }
    }
}

struct IfdSummary {
    ifd: Ifd,
    width: u32,
    height: u32,
    is_raw: bool,
    has_data: bool,
    sub_ifd_offsets: Vec<u64>,
}

pub struct DngDecoder<R: Read + Seek> {
    reader: StreamReader<R>,
    limits: Limits,
    byte_order: ByteOrder,
    ifd_format: IfdFormat,
    header: TiffHeader,
    dng: DngMetadata,
    width: u32,
    height: u32,
    cfa_phase: (u32, u32),
    lossy_curves: Option<Box<[[u16; 256]; 3]>>,
    float_scale: Option<f32>,
    chunk_alloc_ceiling: Option<u64>,
    info: Option<DngInfo>,
}

impl<R: Read + Seek> DngDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: StreamReader::new(reader),
            limits: Limits::default(),
            byte_order: ByteOrder::LittleEndian,
            ifd_format: IfdFormat::classic(ByteOrder::LittleEndian),
            header: TiffHeader::default(),
            dng: DngMetadata::default(),
            width: 0,
            height: 0,
            cfa_phase: (0, 0),
            lossy_curves: None,
            float_scale: None,
            chunk_alloc_ceiling: None,
            info: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> DngInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            let saved_pos = self.reader.stream_position().unwrap_or(0);
            let structure = walk_structure(&mut self.reader, &mut self.limits);
            match structure {
                Ok(structure) => self.info = Some(build_info(structure, &mut self.reader, &mut self.limits)),
                Err(e) => log_warn!("Failed to read DNG structure: {}", e),
            }
            self.limits = saved_limits;
            let _ = self.reader.seek(SeekFrom::Start(saved_pos));
        }

        self.info.clone().unwrap_or_default()
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        let saved_pos = self.reader.stream_position().unwrap_or(0);
        let saved_header = std::mem::take(&mut self.header);
        let saved_dng = std::mem::take(&mut self.dng);
        let saved_byte_order = self.byte_order;
        let saved_ifd_format = self.ifd_format;
        let (saved_width, saved_height) = (self.width, self.height);
        let saved_limits = self.limits.clone();

        let result = self.decode_preview_ifds();

        self.limits = saved_limits;
        self.header = saved_header;
        self.dng = saved_dng;
        self.byte_order = saved_byte_order;
        self.ifd_format = saved_ifd_format;
        self.width = saved_width;
        self.height = saved_height;
        let _ = self.reader.seek(SeekFrom::Start(saved_pos));

        result
    }

    fn decode_preview_ifds(&mut self) -> VexelResult<Vec<Image>> {
        self.reader.seek(SeekFrom::Start(0))?;
        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;
        self.ifd_format = file_header.format;

        let mut previews = Vec::new();
        let mut guard = IfdCycleGuard::new(MAX_DNG_IFDS);
        let mut queue = vec![file_header.first_ifd_offset];
        let mut queued = IfdQueueFilter::new(file_header.first_ifd_offset);

        while let Some(offset) = queue.pop() {
            if !guard.enter(offset) {
                continue;
            }

            let Ok(ifd) = Ifd::read(&mut self.reader, offset, self.ifd_format, &mut self.limits) else {
                continue;
            };

            let summary = self.summarize_ifd(ifd);
            queued.extend(&mut queue, summary.sub_ifd_offsets.iter().copied());
            queued.extend(&mut queue, [summary.ifd.next_ifd_offset]);

            if summary.has_data && !summary.is_raw && summary.width > 0 && summary.height > 0 {
                if let Some(image) = self.decode_preview_ifd(&summary.ifd, summary.width, summary.height) {
                    previews.push(image);
                }
            }
        }

        Ok(previews)
    }

    fn decode_preview_ifd(&mut self, ifd: &Ifd, width: u32, height: u32) -> Option<Image> {
        const COMPRESSION_NONE: u32 = 1;
        const COMPRESSION_OLD_JPEG: u32 = 6;
        const COMPRESSION_JPEG: u32 = 7;
        const COMPRESSION_LOSSY_JPEG: u32 = 34892;

        let compression = ifd.value::<u32, _>(TiffTags::Compression, &mut self.reader).unwrap_or(COMPRESSION_NONE);
        let spp = ifd.value::<u32, _>(TiffTags::SamplesPerPixel, &mut self.reader).unwrap_or(3);
        let bps = ifd
            .values::<u32, _>(TiffTags::BitsPerSample, &mut self.reader, &mut self.limits)
            .and_then(|v| v.first().copied())
            .unwrap_or(8);

        let (offsets, counts) = if ifd.entry(TiffTags::StripOffsets).is_some() {
            (
                ifd.values::<u64, _>(TiffTags::StripOffsets, &mut self.reader, &mut self.limits).unwrap_or_default(),
                ifd.values::<u64, _>(TiffTags::StripByteCounts, &mut self.reader, &mut self.limits).unwrap_or_default(),
            )
        } else {
            (
                ifd.values::<u64, _>(TiffTags::TileOffsets, &mut self.reader, &mut self.limits).unwrap_or_default(),
                ifd.values::<u64, _>(TiffTags::TileByteCounts, &mut self.reader, &mut self.limits).unwrap_or_default(),
            )
        };

        if offsets.is_empty() {
            return None;
        }

        let is_jpeg = matches!(compression, COMPRESSION_JPEG | COMPRESSION_OLD_JPEG | COMPRESSION_LOSSY_JPEG);
        if !is_jpeg {
            if compression != COMPRESSION_NONE || bps != 8 {
                log_warn!("Unsupported DNG preview compression {} with {} bits per sample", compression, bps);
                return None;
            }
            if !matches!(spp, 1 | 3 | 4) {
                log_warn!("Unsupported DNG preview samples per pixel: {}", spp);
                return None;
            }
        }

        self.limits.reserve_buffer(width, height, 4).ok()?;

        let channels = if is_jpeg { 3 } else { spp as usize };
        let target_bytes = (width as usize).saturating_mul(height as usize).saturating_mul(channels);
        let mut read_budget = self.chunk_read_budget();

        let jpeg_tables = if is_jpeg {
            ifd.byte_data(TiffTags::JPEGTables, &mut self.reader, &mut self.limits).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut data: Vec<u8> = Vec::new();
        for (i, &offset) in offsets.iter().enumerate() {
            if data.len() >= target_bytes || read_budget == 0 {
                break;
            }
            let mut count = counts.get(i).copied().unwrap_or(0) as usize;
            if !is_jpeg {
                count = count.min(target_bytes - data.len());
            }
            count = count.min(usize::try_from(read_budget).unwrap_or(usize::MAX));
            if count == 0 || self.reader.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }
            if self.limits.reserve_usize(count).is_err() {
                continue;
            }
            let chunk = self.reader.read_bytes(count);
            self.limits.free_usize(count);
            let Ok(chunk) = chunk else { continue };
            read_budget = read_budget.saturating_sub(chunk.len() as u64);

            if !is_jpeg {
                data.extend_from_slice(&chunk);
                continue;
            }

            let jpeg_data = if jpeg_tables.len() > 4 { splice_jpeg_tables(&jpeg_tables, &chunk) } else { chunk };
            let mut decoder = JpegDecoder::new(Cursor::new(jpeg_data));
            decoder.set_limits(self.limits.clone());
            match decoder.decode() {
                Ok(image) => data.extend(image.into_pixels().into_rgb8_bytes()),
                Err(e) => log_warn!("Failed to decode DNG preview JPEG chunk: {}", e),
            }
        }

        if data.is_empty() {
            return None;
        }
        data.truncate(target_bytes);

        let mut pixel_data = match channels {
            1 => PixelData::L8(data),
            4 => PixelData::RGBA8(data),
            _ => PixelData::RGB8(data),
        };

        pixel_data.correct_pixels(width, height);
        Some(Image::from_pixels(width, height, pixel_data))
    }

    fn chunk_read_budget(&mut self) -> u64 {
        let saved_pos = self.reader.stream_position().unwrap_or(0);
        let stream_len = self.reader.seek(SeekFrom::End(0)).unwrap_or(0);
        let _ = self.reader.seek(SeekFrom::Start(saved_pos));
        stream_len.saturating_mul(2).saturating_add(CHUNK_READ_SLACK)
    }

    fn summarize_ifd(&mut self, ifd: Ifd) -> IfdSummary {
        let width = ifd.value(TiffTags::ImageWidth, &mut self.reader).unwrap_or(0);
        let height = ifd.value(TiffTags::ImageLength, &mut self.reader).unwrap_or(0);

        let is_raw = matches!(
            ifd.value::<u32, _>(TiffTags::PhotometricInterpretation, &mut self.reader),
            Some(DNG_PHOTOMETRIC_CFA | DNG_PHOTOMETRIC_LINEAR_RAW)
        );

        let has_data = ifd.entry(TiffTags::StripOffsets).is_some_and(|e| e.count > 0)
            || ifd.entry(TiffTags::TileOffsets).is_some_and(|e| e.count > 0);

        let sub_ifd_offsets = ifd
            .values::<u64, _>(TiffTags::SubIFDs, &mut self.reader, &mut self.limits)
            .unwrap_or_default()
            .into_iter()
            .filter(|&o| o > 0)
            .collect();

        IfdSummary { ifd, width, height, is_raw, has_data, sub_ifd_offsets }
    }

    fn read_ifd(&mut self, ifd: &Ifd, is_raw_ifd: bool) {
        if let Some(v) = ifd.value::<u32, _>(TiffTags::ImageWidth, &mut self.reader) {
            if v > 0 {
                self.header.image_width = v;
            }
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::ImageLength, &mut self.reader) {
            if v > 0 {
                self.header.image_length = v;
            }
        }
        if let Some(v) = ifd.values(TiffTags::BitsPerSample, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.bits_per_sample = v;
            }
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::Compression, &mut self.reader) {
            self.header.compression = Compression::try_from(v as u16).unwrap_or(Compression::None);
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::PhotometricInterpretation, &mut self.reader) {
            self.dng.photometric_raw = v == DNG_PHOTOMETRIC_CFA || v == DNG_PHOTOMETRIC_LINEAR_RAW;
            self.dng.photometric_cfa = v == DNG_PHOTOMETRIC_CFA;
            self.header.photometric_interpretation =
                PhotometricInterpretation::try_from(u64::from(v)).unwrap_or(PhotometricInterpretation::BlackIsZero);
        }
        if self.dng.make.is_empty() {
            if let Some(v) = ifd.string(TiffTags::Make, &mut self.reader) {
                self.dng.make = v;
            }
        }
        if self.dng.model.is_empty() {
            if let Some(v) = ifd.string(TiffTags::Model, &mut self.reader) {
                self.dng.model = v;
            }
        }
        if let Some(v) = ifd.values(TiffTags::StripOffsets, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.strip_offsets = v;
            }
        }
        if self.dng.orientation == 0 {
            if let Some(v) = ifd.value::<u32, _>(TiffTags::Orientation, &mut self.reader) {
                if (1..=8).contains(&v) {
                    self.dng.orientation = v as u16;
                }
            }
        }
        if let Some(v) = ifd.value(TiffTags::SamplesPerPixel, &mut self.reader) {
            self.header.samples_per_pixel = v;
        }
        if let Some(v) = ifd.value(TiffTags::RowsPerStrip, &mut self.reader) {
            self.header.rows_per_strip = v;
        }
        if let Some(v) = ifd.values(TiffTags::StripByteCounts, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.strip_byte_counts = v;
            }
        }
        if let Some(v) = ifd.rational(TiffTags::XResolution, &mut self.reader) {
            self.header.x_resolution = v;
        }
        if let Some(v) = ifd.rational(TiffTags::YResolution, &mut self.reader) {
            self.header.y_resolution = v;
        }
        if let Some(v) = ifd.value(TiffTags::PlanarConfiguration, &mut self.reader) {
            self.header.planar_configuration = v;
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::Predictor, &mut self.reader) {
            self.header.predictor = Predictor::try_from(u64::from(v)).unwrap_or(Predictor::None);
        }
        if let Some(v) = ifd.value(TiffTags::TileWidth, &mut self.reader) {
            self.header.tile_width = Some(v);
        }
        if let Some(v) = ifd.value(TiffTags::TileLength, &mut self.reader) {
            self.header.tile_length = Some(v);
        }
        if let Some(v) = ifd.values(TiffTags::TileOffsets, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.tile_offsets = v;
            }
        }
        if let Some(v) = ifd.values(TiffTags::TileByteCounts, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.tile_byte_counts = v;
            }
        }
        if let Some(v) = ifd.byte_data(TiffTags::JPEGTables, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.jpeg_tables = v;
            }
        }
        if is_raw_ifd || self.dng.gray_response_curve.is_none() {
            if let Some(entry) = ifd.entry(TiffTags::GrayResponseCurve) {
                if entry.count >= 2 {
                    if let Some(table) =
                        ifd.values::<u16, _>(TiffTags::GrayResponseCurve, &mut self.reader, &mut self.limits)
                    {
                        if !table.is_empty() {
                            self.dng.gray_response_curve = Some(table);
                        }
                    }
                }
            }
        }

        if let Some(v) = ifd.values::<u32, _>(TiffTags::DNGVersion, &mut self.reader, &mut self.limits) {
            if v.len() >= 4 {
                self.dng.dng_version = (v[0] << 24) | (v[1] << 16) | (v[2] << 8) | v[3];
            }
        }
        if let Some(vals) = ifd.signed_rationals(TiffTags::ColorMatrix1, &mut self.reader, &mut self.limits) {
            if self.dng.color_matrix1.is_none() && vals.len() >= 9 {
                self.dng.color_matrix1 =
                    Some([[vals[0], vals[1], vals[2]], [vals[3], vals[4], vals[5]], [vals[6], vals[7], vals[8]]]);
            }
        }
        if let Some(vals) = ifd.signed_rationals(TiffTags::ColorMatrix2, &mut self.reader, &mut self.limits) {
            if self.dng.color_matrix2.is_none() && vals.len() >= 9 {
                self.dng.color_matrix2 =
                    Some([[vals[0], vals[1], vals[2]], [vals[3], vals[4], vals[5]], [vals[6], vals[7], vals[8]]]);
            }
        }
        if self.dng.as_shot_neutral.is_empty() {
            if let Some(vals) = ifd.rationals(TiffTags::AsShotNeutral, &mut self.reader, &mut self.limits) {
                self.dng.as_shot_neutral = vals;
            }
        }

        if is_raw_ifd {
            self.read_raster_calibration_tags(ifd);
        }
    }

    fn read_raster_calibration_tags(&mut self, ifd: &Ifd) {
        if let Some(v) = ifd.values::<u32, _>(TiffTags::SampleFormat, &mut self.reader, &mut self.limits) {
            if !v.is_empty() {
                self.header.sample_format = v
                    .into_iter()
                    .map(|f| SampleFormat::try_from(u64::from(f)).unwrap_or(SampleFormat::UnsignedInt))
                    .collect();
            }
        }
        if let Some(v) = ifd.values::<u32, _>(TiffTags::CFAPlaneColor, &mut self.reader, &mut self.limits) {
            self.dng.cfa_plane_color = v.into_iter().map(|x| x as u8).collect();
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::CFALayout, &mut self.reader) {
            if v != 1 {
                log_warn!("DNG CFALayout {} not supported, treating as rectangular", v);
            }
        }
        if self.dng.linearization_table.is_none() {
            if let Some(table) = ifd.values::<u16, _>(TiffTags::LinearizationTable, &mut self.reader, &mut self.limits)
            {
                if !table.is_empty() {
                    self.dng.linearization_table = Some(table);
                }
            }
        }
        if let Some(dims) = ifd.values::<u32, _>(TiffTags::BlackLevelRepeatDim, &mut self.reader, &mut self.limits) {
            self.dng.black_level_repeat_rows = dims.first().copied().unwrap_or(1);
            self.dng.black_level_repeat_cols = dims.get(1).copied().unwrap_or(1);
        }
        if self.dng.black_level.is_none() {
            if let Some(entry) = ifd.entry(TiffTags::BlackLevel) {
                let mut repeat_rows = self.dng.black_level_repeat_rows.max(1);
                let mut repeat_cols = self.dng.black_level_repeat_cols.max(1);
                if repeat_rows.saturating_mul(repeat_cols) > MAX_BLACK_REPEAT_VALUES {
                    log_warn!("DNG BlackLevelRepeatDim {}x{} too large, using 1x1", repeat_rows, repeat_cols);
                    repeat_rows = 1;
                    repeat_cols = 1;
                }
                let spatial_count = repeat_rows * repeat_cols;

                let values: Vec<f32> = if entry.field_type == 5 {
                    ifd.rationals(TiffTags::BlackLevel, &mut self.reader, &mut self.limits).unwrap_or_default()
                } else {
                    ifd.values::<u32, _>(TiffTags::BlackLevel, &mut self.reader, &mut self.limits)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|v| v as f32)
                        .collect()
                };

                if !values.is_empty() {
                    self.dng.black_level = Some(if values.len() == 1 {
                        BlackLevel::Uniform(values[0])
                    } else if values.len() <= 4 && (spatial_count <= 1 || values.len() < spatial_count as usize) {
                        BlackLevel::PerChannel(values)
                    } else {
                        BlackLevel::Spatial { values, repeat_rows, repeat_cols }
                    });
                }
            }
        }

        let delta_avg = self.read_black_level_delta_avg(ifd, TiffTags::BlackLevelDeltaH)
            + self.read_black_level_delta_avg(ifd, TiffTags::BlackLevelDeltaV);
        if delta_avg != 0.0 {
            self.dng.black_level = Some(match self.dng.black_level.take() {
                None => BlackLevel::Uniform(delta_avg),
                Some(BlackLevel::Uniform(v)) => BlackLevel::Uniform(v + delta_avg),
                Some(BlackLevel::PerChannel(v)) => {
                    BlackLevel::PerChannel(v.into_iter().map(|x| x + delta_avg).collect())
                }
                Some(BlackLevel::Spatial { values, repeat_rows, repeat_cols }) => BlackLevel::Spatial {
                    values: values.into_iter().map(|x| x + delta_avg).collect(),
                    repeat_rows,
                    repeat_cols,
                },
            });
        }

        if self.dng.white_level.is_empty() {
            if let Some(v) = ifd.values(TiffTags::WhiteLevel, &mut self.reader, &mut self.limits) {
                self.dng.white_level = v;
            }
        }
        if self.dng.default_crop_origin.is_none() {
            if let Some(v) = ifd.values::<u32, _>(TiffTags::DefaultCropOrigin, &mut self.reader, &mut self.limits) {
                if v.len() >= 2 {
                    self.dng.default_crop_origin = Some([v[0], v[1]]);
                }
            }
        }
        if self.dng.default_crop_size.is_none() {
            if let Some(v) = ifd.values::<u32, _>(TiffTags::DefaultCropSize, &mut self.reader, &mut self.limits) {
                if v.len() >= 2 {
                    self.dng.default_crop_size = Some([v[0], v[1]]);
                }
            }
        }
        if self.dng.active_area.is_none() {
            if let Some(v) = ifd.values::<u32, _>(TiffTags::ActiveArea, &mut self.reader, &mut self.limits) {
                if v.len() >= 4 {
                    self.dng.active_area = Some([v[0], v[1], v[2], v[3]]);
                }
            }
        }
        if self.dng.map_polynomials.is_empty() {
            if let Some(entry) = ifd.entry(TiffTags::OpcodeList2) {
                let (count, value_offset) = (entry.count, entry.value_offset);
                self.read_opcode_list(count, value_offset);
            }
        }
    }

    fn read_black_level_delta_avg(&mut self, ifd: &Ifd, tag: TiffTags) -> f32 {
        let Some(entry) = ifd.entry(tag) else { return 0.0 };
        let values = if entry.field_type == 10 {
            ifd.signed_rationals(tag, &mut self.reader, &mut self.limits)
        } else {
            ifd.rationals(tag, &mut self.reader, &mut self.limits)
        }
        .unwrap_or_default();

        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f32>() / values.len() as f32
        }
    }

    fn read_opcode_list(&mut self, count: u64, value_offset: u64) {
        let len = count.min(1 << 20) as usize;
        if len < 4 || self.reader.seek(SeekFrom::Start(value_offset)).is_err() {
            return;
        }
        let mut data = vec![0u8; len];
        if self.reader.read_exact(&mut data).is_err() {
            return;
        }

        let u32_at = |pos: usize| -> Option<u32> {
            data.get(pos..pos + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        };

        let opcode_count = match u32_at(0) {
            Some(c) => c.min(64),
            None => return,
        };

        let mut pos = 4usize;
        for _ in 0..opcode_count {
            let (Some(opcode_id), Some(param_bytes)) = (u32_at(pos), u32_at(pos + 12)) else {
                return;
            };
            let params_start = pos + 16;
            pos = match params_start.checked_add(param_bytes as usize) {
                Some(next) if next <= data.len() => next,
                _ => return,
            };

            if opcode_id != 8 {
                continue;
            }

            let (Some(plane), Some(planes), Some(degree)) =
                (u32_at(params_start + 16), u32_at(params_start + 20), u32_at(params_start + 32))
            else {
                continue;
            };
            if degree > 8 {
                continue;
            }

            let mut coefficients = Vec::with_capacity(degree as usize + 1);
            for i in 0..=degree as usize {
                let coeff_pos = params_start + 36 + i * 8;
                match data.get(coeff_pos..coeff_pos + 8) {
                    Some(b) => coefficients.push(f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])),
                    None => return,
                }
            }

            self.dng.map_polynomials.push(MapPolynomial { plane, planes, coefficients });
        }
    }

    fn detect_cfa_pattern(&mut self, ifd: &Ifd) {
        if self.dng.cfa_pattern.is_some() {
            return;
        }

        if ifd.has(TiffTags::CFAPattern) {
            let vals =
                ifd.values::<u32, _>(TiffTags::CFAPattern, &mut self.reader, &mut self.limits).unwrap_or_default();
            if vals.len() >= 4 {
                self.dng.cfa_pattern = Some(match (vals[0], vals[1], vals[2], vals[3]) {
                    (0, 1, 1, 2) => CfaPattern::Rggb,
                    (2, 1, 1, 0) => CfaPattern::Bggr,
                    (1, 0, 2, 1) => CfaPattern::Grbg,
                    (1, 2, 0, 1) => CfaPattern::Gbrg,
                    _ => CfaPattern::Unknown,
                });
            }
            return;
        }

        self.dng.cfa_pattern = Some(CfaPattern::Rggb);
    }

    fn get_black(&self, x: u32, y: u32, channel: usize) -> f32 {
        match &self.dng.black_level {
            None => 0.0,
            Some(BlackLevel::Uniform(v)) => *v,
            Some(BlackLevel::PerChannel(v)) => v.get(channel).copied().unwrap_or(0.0),
            Some(BlackLevel::Spatial { values, repeat_rows, repeat_cols }) => {
                let row = y % repeat_rows;
                let col = x % repeat_cols;
                values.get((row * repeat_cols + col) as usize).copied().unwrap_or(0.0)
            }
        }
    }

    fn get_black_common(&self) -> f32 {
        match &self.dng.black_level {
            None => 0.0,
            Some(BlackLevel::Uniform(v)) => *v,
            Some(BlackLevel::PerChannel(v)) => v.iter().copied().fold(f32::INFINITY, f32::min).max(0.0),
            Some(BlackLevel::Spatial { values, .. }) => values.iter().copied().fold(f32::INFINITY, f32::min).max(0.0),
        }
    }

    fn get_white_level(&self) -> f32 {
        if let Some(&w) = self.dng.white_level.first() {
            return w as f32;
        }
        if let Some(last) = self.dng.linearization_table.as_ref().and_then(|t| t.last()) {
            return *last as f32;
        }
        let bps = self.header.bits_per_sample.first().copied().unwrap_or(16).min(16) as u32;
        ((1u64 << bps) - 1) as f32
    }

    fn cfa_color(&self, x: u32, y: u32) -> usize {
        let pattern = self.dng.cfa_pattern.unwrap_or(CfaPattern::Rggb);
        let row = ((y & 1) ^ self.cfa_phase.0) as usize;
        let col = ((x & 1) ^ self.cfa_phase.1) as usize;
        match pattern {
            CfaPattern::Rggb => [[0usize, 1], [1, 2]][row][col],
            CfaPattern::Bggr => [[2usize, 1], [1, 0]][row][col],
            CfaPattern::Grbg => [[1usize, 0], [2, 1]][row][col],
            CfaPattern::Gbrg => [[1usize, 2], [0, 1]][row][col],
            CfaPattern::Unknown => 1,
        }
    }

    fn strip_context(&self, piece_bytes: usize) -> StripContext<'_> {
        let mut jpeg_limits = self.limits.clone();
        jpeg_limits.max_alloc = self
            .chunk_alloc_ceiling
            .map(|ceiling| ceiling.min(piece_bytes.saturating_mul(8).saturating_add(CHUNK_JPEG_ALLOC_SLACK) as u64));
        StripContext {
            header: &self.header,
            dng: &self.dng,
            byte_order: self.byte_order,
            lossy_curves: self.lossy_curves.as_deref(),
            float_scale: self.float_scale,
            jpeg_limits,
        }
    }

    fn prepare_float_scale(&mut self) {
        let bps = self.header.bits_per_sample.first().copied().unwrap_or(16);
        let is_float = self.header.sample_format.first().copied() == Some(SampleFormat::Float);
        if !is_float || !matches!(bps, 16 | 24 | 32) {
            return;
        }

        self.dng.linearization_table = None;

        let tmax = self.dng.white_level.first().copied().map(|w| w as f32).unwrap_or(1.0).max(f32::MIN_POSITIVE);
        if (4096.0..=32767.0).contains(&tmax) {
            self.float_scale = Some(1.0);
            return;
        }

        let multip = 16383.0 / tmax;
        self.float_scale = Some(multip);
        self.dng.white_level = vec![16383];
        if let Some(black) = &mut self.dng.black_level {
            match black {
                BlackLevel::Uniform(v) => *v *= multip,
                BlackLevel::PerChannel(v) => v.iter_mut().for_each(|x| *x *= multip),
                BlackLevel::Spatial { values, .. } => values.iter_mut().for_each(|x| *x *= multip),
            }
        }
    }

    fn prepare_lossy_curves(&mut self) {
        if self.header.compression == Compression::LossyJPEG && self.lossy_curves.is_none() {
            let curves = self.strip_context(0).build_lossy_curves();
            self.lossy_curves = Some(curves);
        }
    }

    fn decode_pieces(
        &mut self,
        pieces: &[Piece],
        windows: Vec<RasterWindow<'_, u16>>,
        bps: u16,
        piece_bytes: usize,
        context_bytes: usize,
    ) -> VexelResult<()> {
        let mut read_budget = self.chunk_read_budget();
        let batch = batch_len(piece_bytes);
        let mut windows = windows.into_iter();
        let mut index = 0;
        while index < pieces.len() {
            let batch_end = (index + batch).min(pieces.len());
            let mut work = Vec::with_capacity(batch_end - index);
            let mut batch_bytes = 0usize;
            for piece in &pieces[index..batch_end] {
                if batch_bytes >= TARGET_BATCH_BYTES {
                    break;
                }
                let data = self.read_chunk(piece.offset, piece.count, &mut read_budget);
                batch_bytes += data.len();
                let Some(window) = windows.next() else {
                    break;
                };
                work.push((data, piece.rows, piece.width, window));
            }
            if work.is_empty() {
                break;
            }
            index += work.len();

            let ctx = self.strip_context(context_bytes);
            let decode = |(data, rows, width, window): (Vec<u8>, usize, usize, RasterWindow<'_, u16>)| {
                ctx.decode_piece_into(data, rows, width, bps, window)
            };

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                work.into_par_iter().map(decode).collect::<VexelResult<()>>()?;
            }
            #[cfg(not(feature = "rayon"))]
            work.into_iter().try_for_each(decode)?;
        }
        Ok(())
    }

    fn read_chunk(&mut self, offset: u64, count: u64, budget: &mut u64) -> Vec<u8> {
        if *budget == 0 || count == 0 || self.reader.seek(SeekFrom::Start(offset)).is_err() {
            return Vec::new();
        }
        let available = self.reader.bytes_left().unwrap_or(0);
        let len = count.min(available).min(*budget);
        if len < count {
            log_warn!("DNG chunk at offset {} truncated from {} to {} bytes", offset, count, len);
        }
        let mut data = vec![0u8; len as usize];
        prefault(&mut data);
        let reader = self.reader.get_mut();
        let mut filled = 0;
        while filled < data.len() {
            match reader.read(&mut data[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        if filled < data.len() {
            data = Vec::new();
        }
        *budget = budget.saturating_sub(data.len() as u64);
        data
    }

    fn read_raw_strip_data(&mut self) -> VexelResult<Vec<u16>> {
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let bps = self.header.bits_per_sample.first().copied().unwrap_or(16);
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = image_width * image_height * spp;

        self.prepare_lossy_curves();

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();

        let num_strips = offsets.len().max(1);
        let mut rows_per_strip = (self.header.rows_per_strip as usize).max(1).min(image_height.max(1));
        if image_height.div_ceil(rows_per_strip) != num_strips {
            rows_per_strip = image_height.div_ceil(num_strips).max(1);
        }

        let used_strips = offsets.len().min(byte_counts.len()).min(image_height.div_ceil(rows_per_strip));
        let piece_bytes = rows_per_strip * image_width * spp * 2;

        let pieces: Vec<Piece> = (0..used_strips)
            .map(|i| Piece {
                offset: offsets[i],
                count: byte_counts[i],
                rows: rows_per_strip.min(image_height - i * rows_per_strip),
                width: image_width,
            })
            .collect();

        let mut raw = vec![0u16; sample_count];
        let row_samples = image_width * spp;
        let windows =
            RasterWindow::new(&mut raw, row_samples, row_samples, image_height).into_row_bands(rows_per_strip);
        self.decode_pieces(&pieces, windows, bps, piece_bytes, piece_bytes)?;
        Ok(raw)
    }

    fn read_raw_tile_data(&mut self) -> VexelResult<Vec<u16>> {
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let bps = self.header.bits_per_sample.first().copied().unwrap_or(16);
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let tile_width = (self.header.tile_width.unwrap_or(self.width) as usize).max(1);
        let tile_height = (self.header.tile_length.unwrap_or(self.height) as usize).max(1);
        let tiles_x = image_width.div_ceil(tile_width);
        let tiles_y = image_height.div_ceil(tile_height);
        let sample_count = image_width * image_height * spp;

        let offsets = self.header.tile_offsets.clone();
        let byte_counts = self.header.tile_byte_counts.clone();

        let num_tiles = offsets.len().min(tiles_x.saturating_mul(tiles_y));

        let tile_rows = tile_height.min(image_height);
        let tile_limit_error = || {
            VexelError::LimitExceeded(format!(
                "DNG tile layout {}x{} is too large for a {}x{} image",
                tile_width, tile_height, image_width, image_height
            ))
        };
        let piece_bytes = tile_rows
            .checked_mul(tile_width)
            .and_then(|n| n.checked_mul(spp))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(tile_limit_error)?;
        let padded_bytes = piece_bytes.saturating_mul(num_tiles);
        let jpeg_piece_bytes = tile_height
            .saturating_mul(tile_width)
            .saturating_mul(spp)
            .saturating_mul(2)
            .min(piece_bytes.saturating_mul(4));
        if padded_bytes > sample_count.saturating_mul(2 * 4).saturating_add(TILE_PADDING_SLACK) {
            return Err(tile_limit_error());
        }

        self.prepare_lossy_curves();

        let pieces: Vec<Piece> = (0..num_tiles)
            .map(|i| Piece {
                offset: offsets[i],
                count: byte_counts.get(i).copied().unwrap_or(0),
                rows: tile_rows,
                width: tile_width,
            })
            .collect();

        let mut raw = vec![0u16; sample_count];
        let row_samples = image_width * spp;
        let windows: Vec<RasterWindow<'_, u16>> = RasterWindow::new(&mut raw, row_samples, row_samples, image_height)
            .into_row_bands(tile_height)
            .into_iter()
            .flat_map(|band| band.into_column_tiles(tile_width * spp, tiles_x))
            .take(num_tiles)
            .collect();
        self.decode_pieces(&pieces, windows, bps, piece_bytes, jpeg_piece_bytes)?;
        Ok(raw)
    }

    fn crop_raw(&mut self, raw: &mut Vec<u16>, spp: usize, top: u32, left: u32, bottom: u32, right: u32) {
        let bottom = bottom.min(self.height);
        let right = right.min(self.width);
        if top >= bottom || left >= right || (top == 0 && left == 0 && bottom == self.height && right == self.width) {
            return;
        }

        let full_width = self.width as usize;
        let new_width = (right - left) as usize;
        let new_height = (bottom - top) as usize;

        let row_samples = new_width * spp;
        let mut written = 0;
        for y in top as usize..bottom as usize {
            let row_start = (y * full_width + left as usize) * spp;
            if row_start + row_samples > raw.len() {
                break;
            }
            raw.copy_within(row_start..row_start + row_samples, written);
            written += row_samples;
        }
        raw.truncate(written);
        raw.resize(row_samples * new_height, 0);
        self.width = new_width as u32;
        self.height = new_height as u32;
        self.cfa_phase = ((self.cfa_phase.0 + top) & 1, (self.cfa_phase.1 + left) & 1);
    }

    fn crop_to_active_area(&mut self, raw: &mut Vec<u16>, spp: usize) {
        if let Some([top, left, bottom, right]) = self.dng.active_area {
            self.crop_raw(raw, spp, top, left, bottom, right);
        }
    }

    fn kodak_dcs_model(&self) -> Option<String> {
        if !self.dng.make.to_uppercase().starts_with("KODAK") {
            return None;
        }
        let model = self.dng.model.to_uppercase();
        if model.starts_with("DCS") || model.starts_with("EOSDCS") || model.starts_with("NC2000") {
            Some(model)
        } else {
            None
        }
    }

    fn apply_kodak_dcs_defaults(&mut self, ifd: &Ifd) {
        let Some(model) = self.kodak_dcs_model() else { return };

        if !self.dng.photometric_raw && self.header.samples_per_pixel <= 1 {
            self.dng.photometric_raw = true;
            self.dng.photometric_cfa = true;
        }

        if self.dng.photometric_cfa && self.dng.cfa_pattern.is_none() && !ifd.has(TiffTags::CFAPattern) {
            self.dng.cfa_pattern = Some(CfaPattern::Grbg);
        }

        if self.dng.color_matrix1.is_none() && self.dng.color_matrix2.is_none() {
            for (prefix, black, matrix) in KODAK_DCS_COLOR_TABLE {
                if model.starts_with(prefix) {
                    let mut cam_xyz = [[0.0f32; 3]; 3];
                    for (i, &v) in matrix.iter().enumerate() {
                        cam_xyz[i / 3][i % 3] = v as f32 / 10000.0;
                    }
                    self.dng.color_matrix1 = Some(cam_xyz);
                    if *black > 0 && self.dng.black_level.is_none() {
                        self.dng.black_level = Some(BlackLevel::Uniform(*black as f32));
                    }
                    break;
                }
            }
        }
    }

    fn apply_camera_quirks(&mut self, raw: &mut Vec<u16>, spp: usize) {
        if !self.dng.photometric_cfa {
            return;
        }

        let make = self.dng.make.to_uppercase();
        let model = self.dng.model.to_uppercase();
        let is_pentax = make.starts_with("PENTAX");
        let is_samsung = make.starts_with("SAMSUNG");
        let is_ricoh = make.starts_with("RICOH");
        let is_kodak = make.starts_with("KODAK");

        if is_kodak
            && (model.starts_with("DCS4") || model.starts_with("EOSDCS") || model.starts_with("NC2000"))
            && self.width > 4
        {
            self.crop_raw(raw, spp, 0, 2, self.height, self.width - 2);
        }

        if is_pentax || is_samsung {
            if self.width == 3936 && self.height == 2624 {
                self.crop_raw(raw, spp, 0, 0, 2616, 3896);
            } else if self.width == 4864 && self.height == 3136 {
                self.crop_raw(raw, spp, 0, 0, 3124, 4688);
            }
        }

        if is_pentax {
            if self.width == 4352 && (model.contains("K-R") || model.contains("K-X")) {
                self.crop_raw(raw, spp, 0, 0, self.height, 4309);
            } else if self.width >= 4960 && (model.contains("K-5")) {
                self.crop_raw(raw, spp, 0, 10, self.height, 10 + 4950);
            } else if self.width == 6080 && model.contains("K-70") {
                self.crop_raw(raw, spp, 32, 60, 32 + 4016, 60 + 6020);
            } else if self.width == 4736 && model.contains("K-7") && !model.contains("K-70") {
                self.crop_raw(raw, spp, 2, 0, 2 + 3122, 4684);
            } else if self.width == 6080 && model.contains("KF") {
                self.crop_raw(raw, spp, 28, 56, 28 + 4024, 56 + 6024);
            } else if self.width == 6080 && model.contains("K-3 II") && !model.contains("K-3 III") {
                self.crop_raw(raw, spp, 0, 4, self.height, 4 + 6040);
            } else if self.width == 6304 && model.contains("K-3 III") {
                self.crop_raw(raw, spp, 34, 26, 34 + 4160, 26 + 6224);
            } else if self.width == 6112 && model.contains("KP") {
                let top = 28;
                self.crop_raw(raw, spp, top, 54, self.height, 54 + 6028);
            } else if self.width == 6080
                && model.contains("K-3")
                && !model.contains("K-3 II")
                && !model.contains("K-3 III")
            {
                self.crop_raw(raw, spp, 0, 4, self.height, 4 + 6040);
            } else if self.width == 7424 && model.contains("645D") {
                self.crop_raw(raw, spp, 29, 48, 29 + 5502, 48 + 7328);
            }
        }

        if is_ricoh && self.width == 4096 && self.height == 3014 {
            self.crop_raw(raw, spp, 0, 0, self.height, 4014);
        }
    }

    fn develop_cfa(
        &self,
        raw: &[u16],
        pre_mul: [f32; 3],
        rgb_cam: Option<[[f32; 3]; 3]>,
        orientation: Orientation,
    ) -> (Vec<u16>, Vec<[u32; HISTOGRAM_SIZE]>) {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut rgb = vec![0u16; w * h * 3];
        let plan = self.scale_plan(raw, pre_mul, true);
        let cfa = CfaView { pattern: self.dng.cfa_pattern.unwrap_or(CfaPattern::Rggb), phase: self.cfa_phase };
        let band_rows = (DEVELOP_BAND_SAMPLES / w.max(1)).clamp(1, h.max(1));

        let scale_into = |y: usize, dst: &mut [u16]| {
            dst.copy_from_slice(&raw[y * w..(y + 1) * w]);
            if let Some(plan) = &plan {
                plan.apply_row(y, dst);
            }
        };

        let develop_band = |mut scratch: DevelopScratch, (bi, mut out): (usize, RasterWindow<'_, u16>)| {
            scratch.ensure(w);
            let y0 = bi * band_rows;
            let y1 = (y0 + band_rows).min(h);
            let mut scaled_until = y0.saturating_sub(1);
            for y in y0..y1 {
                let need = (y + 2).min(h);
                while scaled_until < need {
                    scale_into(scaled_until, &mut scratch.ring[scaled_until % 3]);
                    scaled_until += 1;
                }
                let DevelopScratch { ring, planes, hist } = &mut scratch;
                let cfa_rows = CfaRows {
                    y,
                    up: (y > 0).then(|| ring[(y + 2) % 3].as_slice()),
                    mid: ring[y % 3].as_slice(),
                    down: (y + 1 < h).then(|| ring[(y + 1) % 3].as_slice()),
                };
                let [r, g, b] = planes;
                cfa.demosaic_row(&cfa_rows, [r.as_mut_slice(), g.as_mut_slice(), b.as_mut_slice()]);
                if let Some(m) = rgb_cam.as_ref() {
                    apply_cam_matrix_planes(m, r, g, b);
                }
                accumulate_histogram(r, g, b, hist);
                orientation.write_row(r, g, b, y, y0, y1, &mut out);
            }
            scratch
        };

        if orientation.transposed() {
            prefault(&mut rgb);
        }
        let bands = orientation.band_windows(&mut rgb, band_rows);

        #[cfg(feature = "rayon")]
        let histogram = {
            use rayon::prelude::*;
            bands
                .into_par_iter()
                .enumerate()
                .fold(DevelopScratch::default, develop_band)
                .map(|scratch| scratch.hist)
                .reduce(|| vec![[0u32; HISTOGRAM_SIZE]; 3], merge_histograms)
        };
        #[cfg(not(feature = "rayon"))]
        let histogram = bands.into_iter().enumerate().fold(DevelopScratch::default(), develop_band).hist;

        (rgb, histogram)
    }

    fn develop_rgb(
        &self,
        rgb: &mut [u16],
        pre_mul: [f32; 3],
        rgb_cam: Option<[[f32; 3]; 3]>,
    ) -> Vec<[u32; HISTOGRAM_SIZE]> {
        let w = self.width as usize;
        let row_samples = w * 3;
        if row_samples == 0 {
            return vec![[0u32; HISTOGRAM_SIZE]; 3];
        }
        let plan = self.scale_plan(rgb, pre_mul, false);
        let band_rows = (DEVELOP_BAND_SAMPLES / row_samples).max(1);

        let develop_band = |mut scratch: DevelopScratch, (bi, band): (usize, &mut [u16])| {
            scratch.ensure(w);
            for (dy, row) in band.chunks_exact_mut(row_samples).enumerate() {
                if let Some(plan) = &plan {
                    plan.apply_row(bi * band_rows + dy, row);
                }
                let DevelopScratch { planes, hist, .. } = &mut scratch;
                let [r, g, b] = planes;
                for (((px, r), g), b) in row.chunks_exact(3).zip(r.iter_mut()).zip(g.iter_mut()).zip(b.iter_mut()) {
                    (*r, *g, *b) = (px[0], px[1], px[2]);
                }
                if let Some(m) = rgb_cam.as_ref() {
                    apply_cam_matrix_planes(m, r, g, b);
                }
                interleave_with_histogram(r, g, b, row, hist);
            }
            scratch
        };

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            rgb.par_chunks_mut(band_rows * row_samples)
                .enumerate()
                .fold(DevelopScratch::default, develop_band)
                .map(|scratch| scratch.hist)
                .reduce(|| vec![[0u32; HISTOGRAM_SIZE]; 3], merge_histograms)
        }
        #[cfg(not(feature = "rayon"))]
        rgb.chunks_mut(band_rows * row_samples).enumerate().fold(DevelopScratch::default(), develop_band).hist
    }

    fn finish_rgb(
        &mut self,
        rgb: Vec<u16>,
        histogram: &[[u32; HISTOGRAM_SIZE]],
        deep: bool,
        orientation: u16,
    ) -> PixelData {
        let pixel_count = rgb.len() / 3;
        let t_white = auto_bright_white(histogram, pixel_count, 3);
        let curve = gamma_curve_bt709(t_white << 3);
        self.finish_with_curve(rgb, &curve, 3, deep, orientation, PixelData::RGB16, PixelData::RGB8)
    }

    fn finish_with_curve(
        &mut self,
        mut samples: Vec<u16>,
        curve: &[u16],
        channels: usize,
        deep: bool,
        orientation: u16,
        wide: impl FnOnce(Vec<u16>) -> PixelData,
        narrow: impl FnOnce(Vec<u8>) -> PixelData,
    ) -> PixelData {
        let w = self.width as usize;
        let h = self.height as usize;
        let rotate = (2..=8).contains(&orientation) && samples.len() == w * h * channels;
        if rotate {
            let (dw, dh) = if orientation >= 5 { (h, w) } else { (w, h) };
            self.width = dw as u32;
            self.height = dh as u32;
        }
        let curve: &[u16; 0x10000] = curve.try_into().expect("gamma curve has 65536 entries");
        match (deep, rotate) {
            (true, false) => {
                map_through_curve(&mut samples, curve);
                wide(samples)
            }
            (true, true) => wide(remap_oriented(&samples, w, h, channels, orientation, |v| curve[v as usize])),
            (false, false) => narrow(remap_oriented(&samples, w, h, channels, 1, |v| (curve[v as usize] >> 8) as u8)),
            (false, true) => {
                narrow(remap_oriented(&samples, w, h, channels, orientation, |v| (curve[v as usize] >> 8) as u8))
            }
        }
    }
}

enum ScalePlan<'a> {
    Lut { combined: Vec<Vec<u16>>, lut_index: Vec<Vec<usize>>, row_pattern: Vec<RowPattern> },
    Direct { scale_rows: Vec<Vec<f32>>, black_rows: Vec<Vec<f32>>, lin_table: Option<&'a [u16]> },
}

impl ScalePlan<'_> {
    fn apply_row(&self, y: usize, raw_row: &mut [u16]) {
        match self {
            ScalePlan::Lut { combined, lut_index, row_pattern } => {
                let y = y % lut_index.len();
                match row_pattern[y] {
                    RowPattern::Uniform(li) => {
                        let lut = &combined[li];
                        for r in raw_row.iter_mut() {
                            *r = lut[*r as usize];
                        }
                    }
                    RowPattern::Alternating(l0, l1) => {
                        let (a, b) = (&combined[l0], &combined[l1]);
                        let mut it = raw_row.chunks_exact_mut(2);
                        for px in &mut it {
                            px[0] = a[px[0] as usize];
                            px[1] = b[px[1] as usize];
                        }
                        if let [last] = it.into_remainder() {
                            *last = a[*last as usize];
                        }
                    }
                    RowPattern::General => {
                        let idx_row = &lut_index[y];
                        for (r, &li) in raw_row.iter_mut().zip(idx_row) {
                            *r = combined[li][*r as usize];
                        }
                    }
                }
            }
            ScalePlan::Direct { scale_rows, black_rows, lin_table } => {
                let black_row = &black_rows[y % black_rows.len()];
                let scale_row = &scale_rows[y % scale_rows.len()];
                for ((r, &b), &s) in raw_row.iter_mut().zip(black_row).zip(scale_row) {
                    let linear = match lin_table {
                        Some(table) => (table[(*r as usize).min(table.len() - 1)] as f32 - b).max(0.0),
                        None => (*r as f32 - b).max(0.0),
                    };
                    *r = (linear * s).clamp(0.0, 65535.0) as u16;
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RowPattern {
    Uniform(usize),
    Alternating(usize, usize),
    General,
}

impl RowPattern {
    fn of(idx: &[usize]) -> Self {
        match idx.first() {
            None => RowPattern::General,
            Some(&first) if idx.iter().all(|&li| li == first) => RowPattern::Uniform(first),
            Some(&first) if idx.len() >= 2 && idx.iter().enumerate().all(|(i, &li)| li == idx[i % 2]) => {
                RowPattern::Alternating(first, idx[1])
            }
            _ => RowPattern::General,
        }
    }
}

fn max_u16(values: &[u16]) -> u16 {
    let mut acc = [0u16; 32];
    let mut chunks = values.chunks_exact(32);
    for chunk in &mut chunks {
        for (a, &v) in acc.iter_mut().zip(chunk) {
            *a = (*a).max(v);
        }
    }
    chunks.remainder().iter().chain(&acc).copied().fold(0, u16::max)
}

fn max_u16_pairs(values: &[u16]) -> (u16, u16) {
    let mut acc = [0u16; 32];
    let mut chunks = values.chunks_exact(32);
    for chunk in &mut chunks {
        for (a, &v) in acc.iter_mut().zip(chunk) {
            *a = (*a).max(v);
        }
    }
    let mut even = acc.iter().step_by(2).copied().fold(0, u16::max);
    let mut odd = acc.iter().skip(1).step_by(2).copied().fold(0, u16::max);
    let done = values.len() - chunks.remainder().len();
    for (i, &v) in chunks.remainder().iter().enumerate() {
        if (done + i).is_multiple_of(2) {
            even = even.max(v);
        } else {
            odd = odd.max(v);
        }
    }
    (even, odd)
}

#[derive(Clone, Copy)]
struct CfaView {
    pattern: CfaPattern,
    phase: (u32, u32),
}

struct CfaRows<'r> {
    y: usize,
    up: Option<&'r [u16]>,
    mid: &'r [u16],
    down: Option<&'r [u16]>,
}

impl CfaRows<'_> {
    fn at(&self, x: i32, dy: i32) -> Option<u16> {
        let row = match dy {
            -1 => self.up?,
            0 => self.mid,
            _ => self.down?,
        };
        if x < 0 {
            return None;
        }
        row.get(x as usize).copied()
    }
}

#[derive(Default)]
struct DevelopScratch {
    ring: [Vec<u16>; 3],
    planes: [Vec<u16>; 3],
    hist: Vec<[u32; HISTOGRAM_SIZE]>,
}

impl DevelopScratch {
    fn ensure(&mut self, w: usize) {
        for row in self.ring.iter_mut().chain(self.planes.iter_mut()) {
            row.resize(w, 0);
        }
        if self.hist.is_empty() {
            self.hist = vec![[0u32; HISTOGRAM_SIZE]; 3];
        }
    }
}

#[derive(Clone, Copy)]
enum CfaSite {
    Green { horizontal: usize },
    Chroma { own: usize },
}

impl CfaView {
    fn color(&self, x: usize, y: usize) -> usize {
        let row = ((y as u32 & 1) ^ self.phase.0) as usize;
        let col = ((x as u32 & 1) ^ self.phase.1) as usize;
        match self.pattern {
            CfaPattern::Rggb => [[0usize, 1], [1, 2]][row][col],
            CfaPattern::Bggr => [[2usize, 1], [1, 0]][row][col],
            CfaPattern::Grbg => [[1usize, 0], [2, 1]][row][col],
            CfaPattern::Gbrg => [[1usize, 2], [0, 1]][row][col],
            CfaPattern::Unknown => 1,
        }
    }

    fn site(&self, x: usize, y: usize) -> CfaSite {
        match self.color(x, y) {
            1 => CfaSite::Green { horizontal: self.color(x + 1, y) },
            own => CfaSite::Chroma { own },
        }
    }

    fn demosaic_row(&self, rows: &CfaRows<'_>, planes: [&mut [u16]; 3]) {
        let w = rows.mid.len();
        let [r, g, b] = planes;
        let interior = match (rows.up, rows.down) {
            (Some(up), Some(down)) if w >= 3 && self.pattern != CfaPattern::Unknown => Some((up, down)),
            _ => None,
        };
        let Some((up, down)) = interior else {
            for x in 0..w {
                (r[x], g[x], b[x]) = self.interpolate_pixel(rows, x);
            }
            return;
        };

        for x in [0, w - 1] {
            (r[x], g[x], b[x]) = self.interpolate_pixel(rows, x);
        }
        let mid = &rows.mid[..w];
        let (up, down) = (&up[..w], &down[..w]);
        let (r, g, b) = (&mut r[..w], &mut g[..w], &mut b[..w]);
        for start in [1, 2] {
            match self.site(start, rows.y) {
                CfaSite::Green { horizontal } => {
                    let (hp, vp) = if horizontal == 0 { (&mut *r, &mut *b) } else { (&mut *b, &mut *r) };
                    for x in (start..w - 1).step_by(2) {
                        g[x] = mid[x];
                        hp[x] = ((mid[x - 1] as u32 + mid[x + 1] as u32) / 2) as u16;
                        vp[x] = ((up[x] as u32 + down[x] as u32) / 2) as u16;
                    }
                }
                CfaSite::Chroma { own } => {
                    let (op, xp) = if own == 0 { (&mut *r, &mut *b) } else { (&mut *b, &mut *r) };
                    for x in (start..w - 1).step_by(2) {
                        op[x] = mid[x];
                        g[x] = ((mid[x - 1] as u32 + mid[x + 1] as u32 + up[x] as u32 + down[x] as u32) / 4) as u16;
                        xp[x] = ((up[x - 1] as u32 + up[x + 1] as u32 + down[x - 1] as u32 + down[x + 1] as u32) / 4)
                            as u16;
                    }
                }
            }
        }
    }

    fn avg_in_bounds(&self, rows: &CfaRows<'_>, x: i32, offsets: &[(i32, i32)]) -> u16 {
        let mut sum = 0u32;
        let mut count = 0u32;
        for &(dx, dy) in offsets {
            if let Some(v) = rows.at(x + dx, dy) {
                sum += v as u32;
                count += 1;
            }
        }
        if count == 0 {
            return 0;
        }
        (sum / count) as u16
    }

    fn interpolate_pixel(&self, rows: &CfaRows<'_>, x: usize) -> (u16, u16, u16) {
        let xi = x as i32;
        let center = rows.mid[x];
        let s = |_: i32, _: i32| center;
        let a = |offsets: &[(i32, i32)]| self.avg_in_bounds(rows, xi, offsets);
        let pattern = self.pattern;
        let row_parity = ((rows.y as u32 & 1) ^ self.phase.0) as usize;
        let col_parity = ((x as u32 & 1) ^ self.phase.1) as usize;

        let (r, g, b) = match pattern {
            CfaPattern::Rggb => match (row_parity, col_parity) {
                (0, 0) => (s(0, 0), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)])),
                (0, 1) => (a(&[(-1, 0), (1, 0)]), s(0, 0), a(&[(0, -1), (0, 1)])),
                (1, 0) => (a(&[(0, -1), (0, 1)]), s(0, 0), a(&[(-1, 0), (1, 0)])),
                (1, 1) => (a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)]), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), s(0, 0)),
                _ => (s(0, 0), s(0, 0), s(0, 0)),
            },
            CfaPattern::Bggr => match (row_parity, col_parity) {
                (0, 0) => (a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)]), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), s(0, 0)),
                (0, 1) => (a(&[(0, -1), (0, 1)]), s(0, 0), a(&[(-1, 0), (1, 0)])),
                (1, 0) => (a(&[(-1, 0), (1, 0)]), s(0, 0), a(&[(0, -1), (0, 1)])),
                (1, 1) => (s(0, 0), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)])),
                _ => (s(0, 0), s(0, 0), s(0, 0)),
            },
            CfaPattern::Grbg => match (row_parity, col_parity) {
                (0, 0) => (a(&[(-1, 0), (1, 0)]), s(0, 0), a(&[(0, -1), (0, 1)])),
                (0, 1) => (s(0, 0), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)])),
                (1, 0) => (a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)]), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), s(0, 0)),
                (1, 1) => (a(&[(0, -1), (0, 1)]), s(0, 0), a(&[(-1, 0), (1, 0)])),
                _ => (s(0, 0), s(0, 0), s(0, 0)),
            },
            CfaPattern::Gbrg => match (row_parity, col_parity) {
                (0, 0) => (a(&[(0, -1), (0, 1)]), s(0, 0), a(&[(-1, 0), (1, 0)])),
                (0, 1) => (a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)]), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), s(0, 0)),
                (1, 0) => (s(0, 0), a(&[(-1, 0), (1, 0), (0, -1), (0, 1)]), a(&[(-1, -1), (1, -1), (-1, 1), (1, 1)])),
                (1, 1) => (a(&[(-1, 0), (1, 0)]), s(0, 0), a(&[(0, -1), (0, 1)])),
                _ => (s(0, 0), s(0, 0), s(0, 0)),
            },
            CfaPattern::Unknown => {
                let v = s(0, 0);
                (v, v, v)
            }
        };
        (r, g, b)
    }
}

impl<R: Read + Seek> DngDecoder<R> {
    fn build_color_transform(&self) -> ColorTransform {
        let cam_mul: Option<[f32; 3]> = if self.dng.as_shot_neutral.len() >= 3
            && self.dng.as_shot_neutral.iter().take(3).all(|v| v.abs() > 0.0001)
        {
            Some([
                1.0 / self.dng.as_shot_neutral[0],
                1.0 / self.dng.as_shot_neutral[1],
                1.0 / self.dng.as_shot_neutral[2],
            ])
        } else {
            None
        };

        if let Some(cam_xyz) = self.dng.color_matrix2.or(self.dng.color_matrix1) {
            let mut cam_rgb = [[0.0f32; 3]; 3];
            for i in 0..3 {
                for j in 0..3 {
                    for k in 0..3 {
                        cam_rgb[i][j] += cam_xyz[i][k] * XYZ_RGB[k][j];
                    }
                }
            }

            let mut matrix_pre_mul = [1.0f32; 3];
            for i in 0..3 {
                let num: f32 = cam_rgb[i].iter().sum();
                if num > 0.00001 {
                    for v in cam_rgb[i].iter_mut() {
                        *v /= num;
                    }
                    matrix_pre_mul[i] = 1.0 / num;
                } else {
                    cam_rgb[i] = [0.0; 3];
                    matrix_pre_mul[i] = 1.0;
                }
            }

            ColorTransform { rgb_cam: Some(invert_3x3(&cam_rgb)), pre_mul: cam_mul.unwrap_or(matrix_pre_mul) }
        } else {
            ColorTransform { rgb_cam: None, pre_mul: cam_mul.unwrap_or([1.0; 3]) }
        }
    }

    fn scale_plan_direct<'a>(
        &self,
        raw: &[u16],
        pre_mul: [f32; 3],
        row_samples: usize,
        channel_rows: &[Vec<usize>],
        black_rows: Vec<Vec<f32>>,
        lin_table: Option<&'a [u16]>,
    ) -> ScalePlan<'a> {
        let linearize = |r: u16, b: f32| -> f32 {
            match lin_table {
                Some(table) => (table[(r as usize).min(table.len() - 1)] as f32 - b).max(0.0),
                None => (r as f32 - b).max(0.0),
            }
        };

        let row_max = |(y, raw_row): (usize, &[u16])| -> f32 {
            let black_row = &black_rows[y % black_rows.len()];
            let mut m = 0.0f32;
            for (&r, &b) in raw_row.iter().zip(black_row) {
                m = m.max(linearize(r, b));
            }
            m
        };

        #[cfg(feature = "rayon")]
        let data_max = {
            use rayon::prelude::*;
            raw.par_chunks(row_samples).enumerate().map(row_max).reduce(|| 0.0f32, f32::max)
        };
        #[cfg(not(feature = "rayon"))]
        let data_max = raw.chunks(row_samples).enumerate().map(row_max).fold(0.0f32, f32::max);

        let mut maximum = (self.get_white_level() - self.get_black_common()).max(1.0);
        if data_max > 0.0 && data_max < maximum && data_max > maximum * 0.75 {
            maximum = data_max;
        }

        let dmin = pre_mul.iter().copied().fold(f32::INFINITY, f32::min);
        let mut scale_mul = [1.0f32; 3];
        if dmin > 0.00001 && maximum > 0.0 {
            for c in 0..3 {
                scale_mul[c] = (pre_mul[c] / dmin) * 65535.0 / maximum;
            }
        }

        let scale_rows: Vec<Vec<f32>> =
            channel_rows.iter().map(|row| row.iter().map(|&ch| scale_mul[ch]).collect()).collect();

        ScalePlan::Direct { scale_rows, black_rows, lin_table }
    }

    fn scale_colors(&self, raw: &mut [u16], pre_mul: [f32; 3], is_cfa: bool) {
        let Some(plan) = self.scale_plan(raw, pre_mul, is_cfa) else {
            return;
        };
        let row_samples = self.scale_row_samples(is_cfa);
        let band_rows = (PARALLEL_CHUNK / row_samples).max(1);
        let apply_band = |(bi, band): (usize, &mut [u16])| {
            for (dy, row) in band.chunks_mut(row_samples).enumerate() {
                plan.apply_row(bi * band_rows + dy, row);
            }
        };

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            raw.par_chunks_mut(band_rows * row_samples).enumerate().for_each(apply_band);
        }
        #[cfg(not(feature = "rayon"))]
        raw.chunks_mut(band_rows * row_samples).enumerate().for_each(apply_band);
    }

    fn scale_row_samples(&self, is_cfa: bool) -> usize {
        let spp = if is_cfa { 1 } else { (self.header.samples_per_pixel as usize).max(1) };
        self.width as usize * spp
    }

    fn scale_plan(&self, raw: &[u16], pre_mul: [f32; 3], is_cfa: bool) -> Option<ScalePlan<'_>> {
        let w = self.width as usize;
        let spp = if is_cfa { 1 } else { (self.header.samples_per_pixel as usize).max(1) };
        let row_samples = w * spp;
        if raw.is_empty() || row_samples == 0 {
            return None;
        }

        let channel_rows: Vec<Vec<usize>> = if is_cfa {
            (0..2).map(|y| (0..row_samples).map(|x| self.cfa_color(x as u32, y)).collect()).collect()
        } else {
            vec![(0..row_samples).map(|i| (i % spp).min(2)).collect()]
        };

        let black_row_count = match &self.dng.black_level {
            None | Some(BlackLevel::Uniform(_)) => 1,
            Some(BlackLevel::PerChannel(_)) => channel_rows.len(),
            Some(BlackLevel::Spatial { repeat_rows, .. }) => {
                (*repeat_rows as usize).min(raw.len() / row_samples).max(1)
            }
        };
        let black_rows: Vec<Vec<f32>> = (0..black_row_count)
            .map(|y| {
                (0..row_samples)
                    .map(|s| self.get_black((s / spp) as u32, y as u32, channel_rows[y % channel_rows.len()][s]))
                    .collect()
            })
            .collect();

        let lin_table = self.dng.linearization_table.as_deref().filter(|t| !t.is_empty());

        let linearized = |r: u16| -> f32 {
            match lin_table {
                Some(table) => table[(r as usize).min(table.len() - 1)] as f32,
                None => r as f32,
            }
        };

        let mut distinct_blacks: Vec<f32> = Vec::new();
        for &b in black_rows.iter().flatten() {
            if let Err(pos) = distinct_blacks.binary_search_by(|p| p.total_cmp(&b)) {
                distinct_blacks.insert(pos, b);
                if distinct_blacks.len() > MAX_BLACK_LUTS {
                    break;
                }
            }
        }

        if distinct_blacks.len() > MAX_BLACK_LUTS || lcm(black_rows.len(), channel_rows.len()) > MAX_LUT_ROWS {
            return Some(self.scale_plan_direct(raw, pre_mul, row_samples, &channel_rows, black_rows, lin_table));
        }

        let black_index: Vec<Vec<usize>> = black_rows
            .iter()
            .map(|row| row.iter().map(|b| distinct_blacks.binary_search_by(|p| p.total_cmp(b)).unwrap_or(0)).collect())
            .collect();

        let black_lut: Vec<Vec<f32>> =
            distinct_blacks.iter().map(|&b| (0..=u16::MAX).map(|r| (linearized(r) - b).max(0.0)).collect()).collect();

        type RawMax = [u16; MAX_BLACK_LUTS];

        let black_pattern: Vec<RowPattern> = black_index.iter().map(|idx| RowPattern::of(idx)).collect();

        let row_max = |(y, raw_row): (usize, &[u16])| -> RawMax {
            let y = y % black_index.len();
            let mut m: RawMax = [0; MAX_BLACK_LUTS];
            match black_pattern[y] {
                RowPattern::Uniform(bi) => m[bi] = max_u16(raw_row),
                RowPattern::Alternating(b0, b1) => {
                    let (even, odd) = max_u16_pairs(raw_row);
                    m[b0] = even;
                    m[b1] = m[b1].max(odd);
                }
                RowPattern::General => {
                    for (&r, &bi) in raw_row.iter().zip(&black_index[y]) {
                        if r > m[bi] {
                            m[bi] = r;
                        }
                    }
                }
            }
            m
        };

        let merge = |mut a: RawMax, b: RawMax| {
            for (x, y) in a.iter_mut().zip(&b) {
                *x = (*x).max(*y);
            }
            a
        };

        let band_rows = (PARALLEL_CHUNK / row_samples.max(1)).max(1);
        let band_samples = band_rows * row_samples;

        let band_max = |(bi, band): (usize, &[u16])| -> RawMax {
            let base = bi * band_rows;
            band.chunks(row_samples)
                .enumerate()
                .map(|(dy, row)| row_max((base + dy, row)))
                .fold([0u16; MAX_BLACK_LUTS], merge)
        };

        #[cfg(feature = "rayon")]
        let raw_max = {
            use rayon::prelude::*;
            raw.par_chunks(band_samples).enumerate().map(band_max).reduce(|| [0u16; MAX_BLACK_LUTS], merge)
        };
        #[cfg(not(feature = "rayon"))]
        let raw_max = raw.chunks(band_samples).enumerate().map(band_max).fold([0u16; MAX_BLACK_LUTS], merge);

        let data_max = raw_max
            .iter()
            .take(distinct_blacks.len())
            .enumerate()
            .fold(0.0f32, |m, (bi, &r)| m.max(black_lut[bi][r as usize]));

        let mut maximum = (self.get_white_level() - self.get_black_common()).max(1.0);
        if data_max > 0.0 && data_max < maximum && data_max > maximum * 0.75 {
            maximum = data_max;
        }

        let dmin = pre_mul.iter().copied().fold(f32::INFINITY, f32::min);
        let mut scale_mul = [1.0f32; 3];
        if dmin > 0.00001 && maximum > 0.0 {
            for c in 0..3 {
                scale_mul[c] = (pre_mul[c] / dmin) * 65535.0 / maximum;
            }
        }

        let combined: Vec<Vec<u16>> = distinct_blacks
            .iter()
            .flat_map(|&b| {
                scale_mul.iter().map(move |&s| {
                    (0..=u16::MAX).map(|r| (((linearized(r) - b).max(0.0)) * s).clamp(0.0, 65535.0) as u16).collect()
                })
            })
            .collect();

        let lut_rows = lcm(black_index.len(), channel_rows.len());
        let lut_index: Vec<Vec<usize>> = (0..lut_rows)
            .map(|y| {
                let idx_row = &black_index[y % black_index.len()];
                let ch_row = &channel_rows[y % channel_rows.len()];
                idx_row.iter().zip(ch_row).map(|(&bi, &ch)| bi * 3 + ch).collect()
            })
            .collect();

        let row_pattern: Vec<RowPattern> = lut_index.iter().map(|idx| RowPattern::of(idx)).collect();

        Some(ScalePlan::Lut { combined, lut_index, row_pattern })
    }

    fn reserve_decode_buffers(&mut self, spp: usize, is_cfa: bool) -> VexelResult<()> {
        self.chunk_alloc_ceiling = self.limits.max_alloc;
        self.limits.check_dimensions(self.width, self.height)?;
        if self.width > i32::MAX as u32 || self.height > i32::MAX as u32 {
            return Err(VexelError::LimitExceeded(format!(
                "DNG dimensions {}x{} are too large",
                self.width, self.height
            )));
        }

        let rgb_samples_per_pixel = if is_cfa || spp > 3 { 3 } else { 0 };
        let bytes_per_pixel = (spp as u64 + rgb_samples_per_pixel) * 2;
        let total = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|n| n.checked_mul(bytes_per_pixel))
            .filter(|&n| usize::try_from(n).is_ok())
            .ok_or_else(|| {
                VexelError::LimitExceeded(format!(
                    "DNG buffer for {}x{} with {} samples per pixel is too large",
                    self.width, self.height, spp
                ))
            })?;
        self.limits.reserve(total)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;
        self.ifd_format = file_header.format;
        let first_ifd_offset = file_header.first_ifd_offset;

        let mut queue: Vec<u64> = vec![first_ifd_offset];
        let mut guard = IfdCycleGuard::new(MAX_DNG_IFDS);
        let mut queued = IfdQueueFilter::new(first_ifd_offset);
        let mut all_ifds: Vec<IfdSummary> = Vec::new();

        while let Some(ifd_offset) = queue.first().cloned() {
            queue.remove(0);
            if !guard.enter(ifd_offset) {
                continue;
            }

            let Ok(parsed) = Ifd::read(&mut self.reader, ifd_offset, self.ifd_format, &mut self.limits) else {
                continue;
            };

            let summary = self.summarize_ifd(parsed);

            queued.extend(&mut queue, summary.sub_ifd_offsets.iter().copied());
            queued.extend(&mut queue, [summary.ifd.next_ifd_offset]);
            all_ifds.push(summary);
        }

        let best = all_ifds
            .iter()
            .filter(|s| s.is_raw && s.has_data)
            .max_by_key(|s| s.width as u64 * s.height as u64)
            .or_else(|| all_ifds.iter().filter(|s| s.has_data).max_by_key(|s| s.width as u64 * s.height as u64))
            .or_else(|| all_ifds.iter().max_by_key(|s| s.width as u64 * s.height as u64));

        let best_offset = best.map(|s| s.ifd.offset).unwrap_or(first_ifd_offset);

        self.header = TiffHeader::default();
        self.dng = DngMetadata::default();

        for summary in &all_ifds {
            if summary.ifd.offset != best_offset {
                self.read_ifd(&summary.ifd, false);
            }
        }

        let parsed_best;
        let best_ifd = match all_ifds.iter().find(|s| s.ifd.offset == best_offset) {
            Some(summary) => &summary.ifd,
            None => {
                parsed_best = Ifd::read(&mut self.reader, best_offset, self.ifd_format, &mut self.limits)?;
                &parsed_best
            }
        };

        self.header = TiffHeader::default();
        self.read_ifd(best_ifd, true);
        self.apply_kodak_dcs_defaults(best_ifd);
        self.detect_cfa_pattern(best_ifd);

        if self.dng.linearization_table.is_none() {
            self.dng.linearization_table = self.dng.gray_response_curve.take();
        }

        if self.header.compression == Compression::LossyJPEG {
            self.dng.white_level = vec![65535];
            self.dng.black_level = None;
            self.dng.linearization_table = None;
        }

        self.prepare_float_scale();

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        let spp = (self.header.samples_per_pixel as usize).max(1);
        let is_cfa = self.dng.photometric_cfa && spp == 1 && self.dng.cfa_pattern.is_some();
        self.reserve_decode_buffers(spp, is_cfa)?;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();

        let mut raw_pixels = if is_tiled { self.read_raw_tile_data()? } else { self.read_raw_strip_data()? };

        self.crop_to_active_area(&mut raw_pixels, spp);
        self.apply_camera_quirks(&mut raw_pixels, spp);

        let w = self.width as usize;
        let h = self.height as usize;

        let transform = self.build_color_transform();

        let deep = self.header.bits_per_sample.first().copied().unwrap_or(16) > 8;

        let pixel_data = if is_cfa {
            let orientation = Orientation::new(self.dng.orientation, w, h);
            let (rgb, histogram) = self.develop_cfa(&raw_pixels, transform.pre_mul, transform.rgb_cam, orientation);
            drop(std::mem::take(&mut raw_pixels));
            let (dw, dh) = orientation.dest_dims();
            self.width = dw as u32;
            self.height = dh as u32;
            self.finish_rgb(rgb, &histogram, deep, 1)
        } else if spp == 3 {
            let mut rgb = std::mem::take(&mut raw_pixels);
            rgb.truncate(w * h * 3);
            let histogram = self.develop_rgb(&mut rgb, transform.pre_mul, transform.rgb_cam);
            self.finish_rgb(rgb, &histogram, deep, self.dng.orientation)
        } else if spp > 3 {
            self.scale_colors(&mut raw_pixels, transform.pre_mul, false);
            let pixel_count = w * h;
            let mut rgb = vec![0u16; pixel_count * 3];
            for (dst, src) in rgb.chunks_exact_mut(3).zip(raw_pixels.chunks_exact(spp)) {
                dst.copy_from_slice(&src[..3]);
            }
            drop(std::mem::take(&mut raw_pixels));
            let histogram = apply_cam_matrix_and_histogram(&mut rgb, transform.rgb_cam.as_ref());
            self.finish_rgb(rgb, &histogram, deep, self.dng.orientation)
        } else {
            self.scale_colors(&mut raw_pixels, [1.0, 1.0, 1.0], false);
            let gray = std::mem::take(&mut raw_pixels);
            let mut histogram = vec![[0u32; HISTOGRAM_SIZE]; 1];
            for &v in gray.iter() {
                histogram[0][(v >> 3) as usize] += 1;
            }
            let t_white = auto_bright_white(&histogram, gray.len(), 1);
            let curve = gamma_curve_bt709(t_white << 3);
            let orientation = self.dng.orientation;
            self.finish_with_curve(gray, &curve, 1, deep, orientation, PixelData::L16, PixelData::L8)
        };

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}

const TARGET_BATCH_BYTES: usize = 32 * 1024 * 1024;

#[cfg(feature = "rayon")]
fn batch_len(piece_bytes: usize) -> usize {
    (TARGET_BATCH_BYTES / piece_bytes.max(1)).max(rayon::current_num_threads()).max(1)
}

#[cfg(not(feature = "rayon"))]
fn batch_len(_piece_bytes: usize) -> usize {
    1
}

fn prefault<T: Copy + Default + Send>(buf: &mut [T]) {
    #[cfg(feature = "rayon")]
    if std::mem::size_of_val(buf) >= PREFAULT_MIN_BYTES {
        use rayon::prelude::*;
        let page = (4096 / std::mem::size_of::<T>().max(1)).max(1);
        buf.par_chunks_mut(page * 256).for_each(|chunk| {
            for slot in chunk.iter_mut().step_by(page) {
                *slot = T::default();
            }
        });
    }
    #[cfg(not(feature = "rayon"))]
    let _ = buf;
}

struct Piece {
    offset: u64,
    count: u64,
    rows: usize,
    width: usize,
}

fn copy_rows(src: &[u16], src_stride: usize, mut window: RasterWindow<'_, u16>) {
    for y in 0..window.height() {
        let start = y * src_stride;
        if start >= src.len() {
            break;
        }
        let row = window.row_mut(y);
        let n = row.len().min(src.len() - start);
        row[..n].copy_from_slice(&src[start..start + n]);
    }
}

struct StripContext<'a> {
    header: &'a TiffHeader,
    dng: &'a DngMetadata,
    byte_order: ByteOrder,
    lossy_curves: Option<&'a [[u16; 256]; 3]>,
    float_scale: Option<f32>,
    jpeg_limits: Limits,
}

impl StripContext<'_> {
    fn build_lossy_curves(&self) -> Box<[[u16; 256]; 3]> {
        let mut curves = Box::new([[0u16; 256]; 3]);

        let mut filled = [false; 3];
        for poly in &self.dng.map_polynomials {
            let start = poly.plane as usize;
            let end = poly.plane.saturating_add(poly.planes.max(1)) as usize;
            for c in start..end.min(3) {
                for i in 0..256 {
                    let x = i as f64 / 255.0;
                    let mut total = 0.0;
                    for (j, &coeff) in poly.coefficients.iter().enumerate() {
                        total += coeff * x.powi(j as i32);
                    }
                    curves[c][i] = (total * 65535.0).clamp(0.0, 65535.0) as u16;
                }
                filled[c] = true;
            }
        }

        for c in 0..3 {
            if !filled[c] {
                for i in 0..256 {
                    let v = i as f64 / 255.0;
                    let linear = if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
                    curves[c][i] = (linear * 65535.0).clamp(0.0, 65535.0) as u16;
                }
            }
        }

        curves
    }

    fn decompress_strip(&self, data: Vec<u8>, rows: usize, width: usize, bps: u16) -> VexelResult<Vec<u16>> {
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let count = rows * width * spp;
        let expected_bytes = count.saturating_mul(bps.clamp(16, 32) as usize / 8).max(1);
        let compression = self.header.compression;

        let decompressed_bytes = match compression {
            Compression::None => data,
            Compression::JPEG | Compression::OldJPEG => {
                let mut samples = vec![0u16; count];
                self.decode_jpeg_lossless_into(data, rows, width, RasterWindow::contiguous(&mut samples, width * spp));
                return Ok(samples);
            }
            Compression::LossyJPEG => return self.decompress_jpeg_lossy(data, rows, width),
            _ => match decompress_generic(compression, &data, expected_bytes) {
                Some(mut decompressed) => {
                    if compression != Compression::PackBits {
                        let big_endian = self.byte_order == ByteOrder::BigEndian;
                        let spp = self.header.samples_per_pixel.max(1);
                        apply_predictor(self.header.predictor, &mut decompressed, width as u32, spp, bps, big_endian);
                    }
                    decompressed
                }
                None => {
                    log_warn!("Unsupported DNG compression {:?}, using raw bytes", compression);
                    data
                }
            },
        };

        match self.float_scale {
            Some(scale) => Ok(self.unpack_float_pixels(&decompressed_bytes, count, bps, scale)),
            None => Ok(self.unpack_pixels(&decompressed_bytes, count, bps)),
        }
    }

    fn unpack_float_pixels(&self, data: &[u8], count: usize, bps: u16, scale: f32) -> Vec<u16> {
        let mut out = Vec::with_capacity(count);
        let mut push = |v: f32| out.push((v * scale).clamp(0.0, 65535.0) as u16);

        match bps {
            16 => {
                for chunk in data.chunks_exact(2).take(count) {
                    push(half_to_f32(u16_from_bytes(chunk, self.byte_order)));
                }
            }
            24 => {
                for chunk in data.chunks_exact(3).take(count) {
                    push(float24_to_f32(chunk, self.byte_order));
                }
            }
            32 => {
                for chunk in data.chunks_exact(4).take(count) {
                    push(f32_from_bytes(chunk, self.byte_order));
                }
            }
            _ => return self.unpack_pixels(data, count, bps),
        }

        out.resize(count, 0);
        out
    }

    fn decode_piece_into(
        &self,
        data: Vec<u8>,
        rows: usize,
        width: usize,
        bps: u16,
        window: RasterWindow<'_, u16>,
    ) -> VexelResult<()> {
        if matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG) {
            self.decode_jpeg_lossless_into(data, rows, width, window);
            return Ok(());
        }
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let samples = self.decompress_strip(data, rows, width, bps)?;
        copy_rows(&samples, width * spp, window);
        Ok(())
    }

    fn decode_jpeg_lossless_into(&self, strip_data: Vec<u8>, rows: usize, width: usize, window: RasterWindow<'_, u16>) {
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = rows * width * spp;

        let jpeg_data = if !self.header.jpeg_tables.is_empty() {
            splice_jpeg_tables(&self.header.jpeg_tables, &strip_data)
        } else {
            strip_data
        };

        let mut jpeg_decoder = JpegDecoder::new(Cursor::new(jpeg_data));
        jpeg_decoder.set_limits(self.jpeg_limits.clone());
        jpeg_decoder.set_lossless_ssss16_is_min(self.dng.dng_version == 0 || self.dng.dng_version >= 0x01010000);
        jpeg_decoder.set_lossless_skip_color_transform(true);

        match jpeg_decoder.read_lossless_frame() {
            Ok(Some(frame)) if frame.width * frame.components == width * spp => {
                jpeg_decoder.decode_lossless_into(window);
                return;
            }
            Ok(_) => {}
            Err(e) => {
                log_warn!("DNG JPEG lossless decode failed: {}", e);
                return;
            }
        }

        let samples = Self::lossless_image_samples(&mut jpeg_decoder, sample_count);
        copy_rows(&samples, width * spp, window);
    }

    fn lossless_image_samples(jpeg_decoder: &mut JpegDecoder<Cursor<Vec<u8>>>, sample_count: usize) -> Vec<u16> {
        let image = match jpeg_decoder.decode() {
            Ok(img) => img,
            Err(e) => {
                log_warn!("DNG JPEG lossless decode failed: {}", e);
                return vec![0u16; sample_count];
            }
        };

        let precision = jpeg_decoder.precision() as u16;

        let (raw_samples, stored_bits): (Vec<u16>, u16) = match image.into_pixels() {
            crate::utils::image::PixelData::L16(v) => (v, 16),
            crate::utils::image::PixelData::LA16(v) => (v, 16),
            crate::utils::image::PixelData::RGB16(v) => (v, 16),
            crate::utils::image::PixelData::RGBA16(v) => (v, 16),
            crate::utils::image::PixelData::L8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            crate::utils::image::PixelData::LA8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            crate::utils::image::PixelData::RGB8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            _ => return vec![0u16; sample_count],
        };

        let precision_shift = stored_bits.saturating_sub(precision.clamp(1, stored_bits));

        let mut out = raw_samples;
        out.resize(sample_count, 0);
        if precision_shift > 0 {
            let shift = |chunk: &mut [u16]| {
                for s in chunk {
                    *s >>= precision_shift;
                }
            };

            #[cfg(feature = "rayon")]
            {
                use rayon::prelude::*;
                out.par_chunks_mut(PARALLEL_CHUNK).for_each(shift);
            }
            #[cfg(not(feature = "rayon"))]
            shift(&mut out);
        }
        out
    }

    fn decompress_jpeg_lossy(&self, tile_data: Vec<u8>, rows: usize, width: usize) -> VexelResult<Vec<u16>> {
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = rows * width * spp;

        let cursor = Cursor::new(tile_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);
        jpeg_decoder.set_limits(self.jpeg_limits.clone());
        let image = match jpeg_decoder.decode() {
            Ok(img) => img,
            Err(e) => {
                log_warn!("DNG lossy JPEG decode failed: {}", e);
                return Ok(vec![0u16; sample_count]);
            }
        };

        let bytes: Vec<u8> = match image.into_pixels() {
            crate::utils::image::PixelData::RGB8(v) => v,
            crate::utils::image::PixelData::L8(v) => v,
            crate::utils::image::PixelData::RGBA8(v) => v,
            other => match other.into_rgb8() {
                crate::utils::image::PixelData::RGB8(v) => v,
                _ => return Ok(vec![0u16; sample_count]),
            },
        };

        let built;
        let curves = match self.lossy_curves {
            Some(c) => c,
            None => {
                built = self.build_lossy_curves();
                &*built
            }
        };

        let mut out = Vec::with_capacity(sample_count);
        for (i, &b) in bytes.iter().take(sample_count).enumerate() {
            let ch = (i % spp).min(2);
            out.push(curves[ch][b as usize]);
        }
        while out.len() < sample_count {
            out.push(0);
        }
        Ok(out)
    }

    fn unpack_pixels(&self, data: &[u8], count: usize, bps: u16) -> Vec<u16> {
        let mut out = Vec::with_capacity(count);
        match bps {
            16 => {
                for chunk in data.chunks_exact(2) {
                    let v = match self.byte_order {
                        ByteOrder::LittleEndian => u16::from_le_bytes([chunk[0], chunk[1]]),
                        ByteOrder::BigEndian => u16::from_be_bytes([chunk[0], chunk[1]]),
                    };
                    out.push(v);
                    if out.len() >= count {
                        break;
                    }
                }
            }
            14 | 12 | 10 => {
                let mut bit_buf: u32 = 0;
                let mut bits_left: u32 = 0;
                for &byte in data {
                    bit_buf = (bit_buf << 8) | byte as u32;
                    bits_left += 8;
                    while bits_left >= bps as u32 && out.len() < count {
                        bits_left -= bps as u32;
                        out.push(((bit_buf >> bits_left) & ((1 << bps) - 1)) as u16);
                    }
                }
            }
            8 => {
                for &byte in data.iter().take(count) {
                    out.push(byte as u16);
                }
            }
            _ => {
                for chunk in data.chunks_exact(2).take(count) {
                    let v = match self.byte_order {
                        ByteOrder::LittleEndian => u16::from_le_bytes([chunk[0], chunk[1]]),
                        ByteOrder::BigEndian => u16::from_be_bytes([chunk[0], chunk[1]]),
                    };
                    out.push(v);
                }
            }
        }
        while out.len() < count {
            out.push(0);
        }
        out
    }
}

struct ColorTransform {
    rgb_cam: Option<[[f32; 3]; 3]>,
    pre_mul: [f32; 3],
}

const KODAK_DCS_COLOR_TABLE: &[(&str, u16, [i16; 9])] = &[
    ("NC2000", 0, [13891, -6055, -803, -465, 9919, 642, 2121, 82, 1291]),
    ("DCS315C", 8, [17523, -4827, -2510, 756, 8546, -137, 6113, 1649, 2250]),
    ("DCS330C", 8, [20620, -7572, -2801, -103, 10073, -396, 3551, -233, 2220]),
    ("DCS420", 0, [10868, -1852, -644, -1537, 11083, 484, 2343, 628, 2216]),
    ("DCS46", 0, [10592, -2206, -967, -1944, 11685, 230, 2206, 670, 1273]),
    ("DCS520C", 178, [24542, -10860, -3401, -1490, 11370, -297, 2858, -605, 3225]),
    ("DCS560C", 177, [20482, -7172, -3125, -1033, 10410, -285, 2542, 226, 3136]),
    ("DCS620C", 177, [23617, -10175, -3149, -2054, 11749, -272, 2586, -489, 3453]),
    ("DCS660C", 173, [18244, -6351, -2739, -791, 11193, -521, 3711, -129, 2802]),
    ("DCS760C", 0, [16623, -6309, -1411, -4344, 13923, 323, 2285, 274, 2926]),
    ("EOSDCS1", 0, [10592, -2206, -967, -1944, 11685, 230, 2206, 670, 1273]),
    ("EOSDCS3", 0, [9898, -2700, -940, -2478, 12219, 206, 1985, 634, 1031]),
];

const XYZ_RGB: [[f32; 3]; 3] =
    [[0.4124564, 0.3575761, 0.1804375], [0.2126729, 0.7151522, 0.0721750], [0.0193339, 0.119192, 0.9503041]];

const HISTOGRAM_SIZE: usize = 0x2000;
const MAX_BLACK_LUTS: usize = 16;
const MAX_LUT_ROWS: usize = 64;
const AUTO_BRIGHT_THRESHOLD: f64 = 0.01;
const PARALLEL_CHUNK: usize = 0x10000;
const DEVELOP_BAND_SAMPLES: usize = 0x20000;
#[cfg(feature = "rayon")]
const PREFAULT_MIN_BYTES: usize = 8 << 20;

#[cfg(feature = "rayon")]
fn merge_histograms(mut a: Vec<[u32; HISTOGRAM_SIZE]>, b: Vec<[u32; HISTOGRAM_SIZE]>) -> Vec<[u32; HISTOGRAM_SIZE]> {
    for (ha, hb) in a.iter_mut().zip(&b) {
        for (va, &vb) in ha.iter_mut().zip(hb.iter()) {
            *va += vb;
        }
    }
    a
}

fn apply_cam_matrix_planes(m: &[[f32; 3]; 3], r: &mut [u16], g: &mut [u16], b: &mut [u16]) {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return unsafe { apply_cam_matrix_planes_avx2(m, r, g, b) };
    }
    apply_cam_matrix_planes_scalar(m, r, g, b);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn apply_cam_matrix_planes_avx2(m: &[[f32; 3]; 3], r: &mut [u16], g: &mut [u16], b: &mut [u16]) {
    apply_cam_matrix_planes_scalar(m, r, g, b);
}

#[allow(clippy::manual_clamp)]
#[inline(always)]
fn cam_channel(row: &[f32; 3], r: f32, g: f32, b: f32) -> u16 {
    let v = (row[0] * r + row[1] * g + row[2] * b).max(0.0).min(65535.0);
    unsafe { v.to_int_unchecked::<i32>() as u16 }
}

#[inline(always)]
fn apply_cam_matrix_planes_scalar(m: &[[f32; 3]; 3], r: &mut [u16], g: &mut [u16], b: &mut [u16]) {
    let n = r.len().min(g.len()).min(b.len());
    let (r, g, b) = (&mut r[..n], &mut g[..n], &mut b[..n]);
    for i in 0..n {
        let (rf, gf, bf) = (r[i] as f32, g[i] as f32, b[i] as f32);
        r[i] = cam_channel(&m[0], rf, gf, bf);
        g[i] = cam_channel(&m[1], rf, gf, bf);
        b[i] = cam_channel(&m[2], rf, gf, bf);
    }
}

#[derive(Clone, Copy)]
struct Orientation {
    code: u16,
    w: usize,
    h: usize,
}

impl Orientation {
    fn new(code: u16, w: usize, h: usize) -> Self {
        Self { code: if (2..=8).contains(&code) { code } else { 1 }, w, h }
    }

    fn transposed(&self) -> bool {
        self.code >= 5
    }

    fn dest_dims(&self) -> (usize, usize) {
        if self.transposed() {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        }
    }

    fn band_range(&self, y0: usize, y1: usize) -> std::ops::Range<usize> {
        match self.code {
            3 | 4 | 6 | 7 => self.h - y1..self.h - y0,
            _ => y0..y1,
        }
    }

    fn band_windows<'a>(&self, rgb: &'a mut [u16], band_rows: usize) -> Vec<RasterWindow<'a, u16>> {
        let (dw, dh) = self.dest_dims();
        let bands = self.h.div_ceil(band_rows.max(1));
        let mut order: Vec<(std::ops::Range<usize>, usize)> =
            (0..bands).map(|b| (self.band_range(b * band_rows, ((b + 1) * band_rows).min(self.h)), b)).collect();
        order.sort_by_key(|(range, _)| range.start);

        let mut slots: Vec<Option<RasterWindow<'a, u16>>> = (0..bands).map(|_| None).collect();
        let mut rest = RasterWindow::new(rgb, dw * 3, dw * 3, dh);
        let mut consumed = 0;
        for (range, band) in order {
            let len = range.end - range.start;
            let window = if self.transposed() {
                let (head, tail) = rest.split_columns(len * 3);
                rest = tail;
                head
            } else {
                let (head, tail) = rest.split_rows(len);
                rest = tail;
                head
            };
            debug_assert_eq!(range.start, consumed);
            consumed += len;
            slots[band] = Some(window);
        }
        slots.into_iter().map(|w| w.expect("every band has a window")).collect()
    }

    fn write_row(
        &self,
        r: &[u16],
        g: &[u16],
        b: &[u16],
        y: usize,
        y0: usize,
        y1: usize,
        out: &mut RasterWindow<'_, u16>,
    ) {
        let w = self.w;
        let (r, g, b) = (&r[..w], &g[..w], &b[..w]);
        if !self.transposed() {
            let row = match self.code {
                3 | 4 => y1 - 1 - y,
                _ => y - y0,
            };
            let row = &mut out.row_mut(row)[..w * 3];
            if matches!(self.code, 2 | 3) {
                for (x, px) in row.chunks_exact_mut(3).rev().enumerate() {
                    px.copy_from_slice(&[r[x], g[x], b[x]]);
                }
            } else {
                for (x, px) in row.chunks_exact_mut(3).enumerate() {
                    px.copy_from_slice(&[r[x], g[x], b[x]]);
                }
            }
            return;
        }
        let col = match self.code {
            6 | 7 => y1 - 1 - y,
            _ => y - y0,
        };
        let stride = out.stride();
        assert!(out.height() >= w && out.width() >= (col + 1) * 3);
        let base = unsafe { out.as_mut_ptr().add(col * 3) };
        let flipped = matches!(self.code, 7 | 8);
        for x in 0..w {
            let row = if flipped { w - 1 - x } else { x };
            unsafe {
                let px = base.add(row * stride);
                px.write(r[x]);
                px.add(1).write(g[x]);
                px.add(2).write(b[x]);
            }
        }
    }
}

fn accumulate_histogram(r: &[u16], g: &[u16], b: &[u16], hist: &mut [[u32; HISTOGRAM_SIZE]]) {
    let [hr, hg, hb] = hist else {
        return;
    };
    for ((&r, &g), &b) in r.iter().zip(g).zip(b) {
        hr[(r >> 3) as usize] += 1;
        hg[(g >> 3) as usize] += 1;
        hb[(b >> 3) as usize] += 1;
    }
}

fn interleave_with_histogram(r: &[u16], g: &[u16], b: &[u16], out: &mut [u16], hist: &mut [[u32; HISTOGRAM_SIZE]]) {
    let [hr, hg, hb] = hist else {
        return;
    };
    for (((px, &r), &g), &b) in out.chunks_exact_mut(3).zip(r).zip(g).zip(b) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        hr[(r >> 3) as usize] += 1;
        hg[(g >> 3) as usize] += 1;
        hb[(b >> 3) as usize] += 1;
    }
}

fn apply_cam_matrix_and_histogram(rgb: &mut [u16], m: Option<&[[f32; 3]; 3]>) -> Vec<[u32; HISTOGRAM_SIZE]> {
    let accumulate = |mut hist: Vec<[u32; HISTOGRAM_SIZE]>, chunk: &mut [u16]| {
        match m {
            Some(m) => {
                for d in chunk.chunks_exact_mut(3) {
                    let (r, g, b) = (d[0] as f32, d[1] as f32, d[2] as f32);
                    for c in 0..3 {
                        let v = (m[c][0] * r + m[c][1] * g + m[c][2] * b).clamp(0.0, 65535.0) as u16;
                        d[c] = v;
                        hist[c][(v >> 3) as usize] += 1;
                    }
                }
            }
            None => {
                for d in chunk.chunks_exact(3) {
                    for c in 0..3 {
                        hist[c][(d[c] >> 3) as usize] += 1;
                    }
                }
            }
        }
        hist
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        rgb.par_chunks_mut(PARALLEL_CHUNK * 3).fold(|| vec![[0u32; HISTOGRAM_SIZE]; 3], accumulate).reduce(
            || vec![[0u32; HISTOGRAM_SIZE]; 3],
            |mut a, b| {
                for (ha, hb) in a.iter_mut().zip(&b) {
                    for (va, &vb) in ha.iter_mut().zip(hb.iter()) {
                        *va += vb;
                    }
                }
                a
            },
        )
    }
    #[cfg(not(feature = "rayon"))]
    accumulate(vec![[0u32; HISTOGRAM_SIZE]; 3], rgb)
}

fn map_through_curve(values: &mut [u16], curve: &[u16]) {
    let map = |chunk: &mut [u16]| {
        for v in chunk {
            *v = curve[*v as usize];
        }
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        values.par_chunks_mut(PARALLEL_CHUNK).for_each(map);
    }
    #[cfg(not(feature = "rayon"))]
    map(values);
}

fn auto_bright_white(histogram: &[[u32; HISTOGRAM_SIZE]], pixel_count: usize, channels: usize) -> i32 {
    let perc = (pixel_count as f64 * AUTO_BRIGHT_THRESHOLD) as u32;
    let mut t_white = 0i32;
    for hist in histogram.iter().take(channels) {
        let mut total = 0u32;
        let mut val = HISTOGRAM_SIZE as i32;
        loop {
            val -= 1;
            if val <= 32 {
                break;
            }
            total += hist[val as usize];
            if total > perc {
                break;
            }
        }
        if t_white < val {
            t_white = val;
        }
    }
    t_white
}

fn gamma_curve_bt709(imax: i32) -> Vec<u16> {
    let (pwr, ts) = (0.45f64, 4.5f64);
    let mut bnd = [0.0f64; 2];
    bnd[(ts >= 1.0) as usize] = 1.0;

    let mut g2 = 0.0f64;
    if ts != 0.0 && (ts - 1.0) * (pwr - 1.0) <= 0.0 {
        for _ in 0..48 {
            g2 = (bnd[0] + bnd[1]) / 2.0;
            let over = ((g2 / ts).powf(-pwr) - 1.0) / pwr - 1.0 / g2 > -1.0;
            bnd[over as usize] = g2;
        }
    }
    let g3 = g2 / ts;
    let g4 = g2 * (1.0 / pwr - 1.0);

    let imax = imax.max(1) as f64;
    let mut curve = vec![0xffffu16; 0x10000];
    for (i, entry) in curve.iter_mut().enumerate() {
        let r = i as f64 / imax;
        if r < 1.0 {
            let v = 65536.0 * if r < g3 { r * ts } else { r.powf(pwr) * (1.0 + g4) - g4 };
            *entry = v.clamp(0.0, 65535.0) as u16;
        }
    }
    curve
}

fn lcm(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 {
        return a.max(b).max(1);
    }
    let (mut x, mut y) = (a, b);
    while y != 0 {
        let t = x % y;
        x = y;
        y = t;
    }
    a / x * b
}

fn invert_3x3(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);

    if det.abs() < 1e-9 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }

    let inv_det = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ]
}

fn remap_oriented<D: Copy + Default + Send>(
    src: &[u16],
    w: usize,
    h: usize,
    channels: usize,
    orientation: u16,
    map: impl Fn(u16) -> D + Sync,
) -> Vec<D> {
    const TILE: usize = 64;
    let (dw, dh) = if orientation >= 5 { (h, w) } else { (w, h) };
    let mut out = vec![D::default(); dw * dh * channels];
    if out.is_empty() {
        return out;
    }

    let fill_band = |(band, rows): (usize, &mut [D])| {
        let dy0 = band * TILE;
        let band_rows = rows.len() / (dw * channels);
        for dx0 in (0..dw).step_by(TILE) {
            let dx1 = (dx0 + TILE).min(dw);
            for r in 0..band_rows {
                let dy = dy0 + r;
                let row = &mut rows[r * dw * channels..(r + 1) * dw * channels];
                for dx in dx0..dx1 {
                    let (sx, sy) = match orientation {
                        2 => (w - 1 - dx, dy),
                        3 => (w - 1 - dx, h - 1 - dy),
                        4 => (dx, h - 1 - dy),
                        5 => (dy, dx),
                        6 => (dy, h - 1 - dx),
                        7 => (w - 1 - dy, h - 1 - dx),
                        8 => (w - 1 - dy, dx),
                        _ => (dx, dy),
                    };
                    let s = (sy * w + sx) * channels;
                    for c in 0..channels {
                        row[dx * channels + c] = map(src[s + c]);
                    }
                }
            }
        }
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        out.par_chunks_mut(dw * channels * TILE).enumerate().for_each(fill_band);
    }
    #[cfg(not(feature = "rayon"))]
    out.chunks_mut(dw * channels * TILE).enumerate().for_each(fill_band);

    out
}
