use crate::bitreader::BitReader;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::utils::tiff::compression::{
    apply_predictor_float, apply_predictor_horizontal, apply_predictor_horizontal_be, decompress_deflate,
    decompress_lzw, decompress_packbits, predictor_x_factor,
};
use crate::utils::tiff::ifd::{self, Ifd};
use crate::utils::tiff::color::{f32_from_bytes, float24_to_f32, half_to_f32, u16_from_bytes};
use crate::utils::tiff::jpeg::splice_jpeg_tables;
use crate::utils::tiff::types::{
    Compression, PhotometricInterpretation, Predictor, SampleFormat, TiffHeader, TiffTags,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{ImageFrame, PixelData};
use crate::utils::types::ByteOrder;
use crate::{Image, Limits, log_warn};
use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{Cursor, Read, Seek, SeekFrom};

const DNG_PHOTOMETRIC_CFA: u32 = 32803;
const DNG_PHOTOMETRIC_LINEAR_RAW: u32 = 34892;

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
    Spatial {
        values: Vec<f32>,
        repeat_rows: u32,
        repeat_cols: u32,
    },
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

struct IfdSummary {
    ifd: Ifd,
    width: u32,
    height: u32,
    is_raw: bool,
    has_data: bool,
    sub_ifd_offsets: Vec<u32>,
}

pub struct DngDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    limits: Limits,
    byte_order: ByteOrder,
    header: TiffHeader,
    dng: DngMetadata,
    width: u32,
    height: u32,
    cfa_phase: (u32, u32),
    lossy_curves: Option<Box<[[u16; 256]; 3]>>,
    float_scale: Option<f32>,
}

impl<R: Read + Seek> DngDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BitReader::new(reader),
            limits: Limits::default(),
            byte_order: ByteOrder::LittleEndian,
            header: TiffHeader::default(),
            dng: DngMetadata::default(),
            width: 0,
            height: 0,
            cfa_phase: (0, 0),
            lossy_curves: None,
            float_scale: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
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
            .values::<u32, _>(TiffTags::SubIFDs, &mut self.reader, &mut self.limits)
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
                PhotometricInterpretation::try_from(v).unwrap_or(PhotometricInterpretation::BlackIsZero);
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
            self.header.predictor = Predictor::try_from(v).unwrap_or(Predictor::None);
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
                    if let Some(table) = ifd.values::<u16, _>(TiffTags::GrayResponseCurve, &mut self.reader, &mut self.limits) {
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
                self.dng.color_matrix1 = Some([
                    [vals[0], vals[1], vals[2]],
                    [vals[3], vals[4], vals[5]],
                    [vals[6], vals[7], vals[8]],
                ]);
            }
        }
        if let Some(vals) = ifd.signed_rationals(TiffTags::ColorMatrix2, &mut self.reader, &mut self.limits) {
            if self.dng.color_matrix2.is_none() && vals.len() >= 9 {
                self.dng.color_matrix2 = Some([
                    [vals[0], vals[1], vals[2]],
                    [vals[3], vals[4], vals[5]],
                    [vals[6], vals[7], vals[8]],
                ]);
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
                    .map(|f| SampleFormat::try_from(f).unwrap_or(SampleFormat::UnsignedInt))
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
            if let Some(table) = ifd.values::<u16, _>(TiffTags::LinearizationTable, &mut self.reader, &mut self.limits) {
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
                let repeat_rows = self.dng.black_level_repeat_rows.max(1);
                let repeat_cols = self.dng.black_level_repeat_cols.max(1);
                let spatial_count = repeat_rows * repeat_cols;

                let values: Vec<f32> = if entry.field_type == 5 {
                    ifd.rationals(TiffTags::BlackLevel, &mut self.reader, &mut self.limits)
                        .unwrap_or_default()
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

    fn read_opcode_list(&mut self, count: u32, value_offset: u32) {
        let len = (count as usize).min(1 << 20);
        if len < 4 || self.reader.seek(SeekFrom::Start(value_offset as u64)).is_err() {
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

            let (Some(plane), Some(planes), Some(degree)) = (
                u32_at(params_start + 16),
                u32_at(params_start + 20),
                u32_at(params_start + 32),
            ) else {
                continue;
            };
            if degree > 8 {
                continue;
            }

            let mut coefficients = Vec::with_capacity(degree as usize + 1);
            for i in 0..=degree as usize {
                let coeff_pos = params_start + 36 + i * 8;
                match data.get(coeff_pos..coeff_pos + 8) {
                    Some(b) => coefficients.push(f64::from_be_bytes([
                        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                    ])),
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
            let vals = ifd
                .values::<u32, _>(TiffTags::CFAPattern, &mut self.reader, &mut self.limits)
                .unwrap_or_default();
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

    fn strip_context(&self) -> StripContext<'_> {
        StripContext {
            header: &self.header,
            dng: &self.dng,
            byte_order: self.byte_order,
            lossy_curves: self.lossy_curves.as_deref(),
            float_scale: self.float_scale,
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
            let curves = self.strip_context().build_lossy_curves();
            self.lossy_curves = Some(curves);
        }
    }

    fn decompress_all(&mut self, pieces: Vec<(Vec<u8>, usize)>, width: usize, bps: u16) -> VexelResult<Vec<Vec<u16>>> {
        let ctx = self.strip_context();

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            pieces
                .into_par_iter()
                .map(|(data, rows)| ctx.decompress_strip(data, rows, width, bps))
                .collect()
        }
        #[cfg(not(feature = "rayon"))]
        pieces
            .into_iter()
            .map(|(data, rows)| ctx.decompress_strip(data, rows, width, bps))
            .collect()
    }

    fn read_raw_strip_data(&mut self) -> VexelResult<Vec<u16>> {
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let bps = self.header.bits_per_sample.first().copied().unwrap_or(16);
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = image_width * image_height * spp;
        let mut raw = vec![0u16; sample_count];

        self.prepare_lossy_curves();

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();

        let num_strips = offsets.len().max(1);
        let mut rows_per_strip = (self.header.rows_per_strip as usize).max(1).min(image_height.max(1));
        if image_height.div_ceil(rows_per_strip) != num_strips {
            rows_per_strip = image_height.div_ceil(num_strips).max(1);
        }

        let used_strips = offsets
            .len()
            .min(byte_counts.len())
            .min(image_height.div_ceil(rows_per_strip));

        let batch = batch_len(rows_per_strip * image_width * spp * 2);
        let mut strip_idx = 0;
        while strip_idx < used_strips {
            let batch_end = (strip_idx + batch).min(used_strips);

            let mut strips_data: Vec<(Vec<u8>, usize)> = Vec::with_capacity(batch_end - strip_idx);
            for i in strip_idx..batch_end {
                self.reader.seek(SeekFrom::Start(offsets[i] as u64))?;
                let mut strip_data = vec![0u8; byte_counts[i] as usize];
                self.reader.read_exact(&mut strip_data)?;

                let strip_rows = rows_per_strip.min(image_height - i * rows_per_strip);
                strips_data.push((strip_data, strip_rows));
            }

            let decompressed = self.decompress_all(strips_data, image_width, bps)?;

            for (k, strip) in decompressed.iter().enumerate() {
                let base = (strip_idx + k) * rows_per_strip * image_width * spp;
                if base >= raw.len() {
                    break;
                }
                let len = strip.len().min(raw.len() - base);
                raw[base..base + len].copy_from_slice(&strip[..len]);
            }

            strip_idx = batch_end;
        }

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

        let mut raw = vec![0u16; image_width * image_height * spp];

        self.prepare_lossy_curves();

        let offsets = self.header.tile_offsets.clone();
        let byte_counts = self.header.tile_byte_counts.clone();

        let num_tiles = offsets.len().min(tiles_x.saturating_mul(tiles_y));

        let batch = batch_len(tile_width * tile_height * spp * 2);
        let mut tile_idx = 0;
        while tile_idx < num_tiles {
            let batch_end = (tile_idx + batch).min(num_tiles);

            let mut tiles_data: Vec<(Vec<u8>, usize)> = Vec::with_capacity(batch_end - tile_idx);
            for i in tile_idx..batch_end {
                let byte_count = byte_counts.get(i).copied().unwrap_or(0);
                self.reader.seek(SeekFrom::Start(offsets[i] as u64))?;
                let mut tile_data = vec![0u8; byte_count as usize];
                self.reader.read_exact(&mut tile_data)?;
                tiles_data.push((tile_data, tile_height));
            }

            let decompressed = self.decompress_all(tiles_data, tile_width, bps)?;

            for (k, tile) in decompressed.iter().enumerate() {
                let img_x = ((tile_idx + k) % tiles_x) * tile_width;
                let img_y = ((tile_idx + k) / tiles_x) * tile_height;
                let cols = tile_width.min(image_width - img_x);

                for row in 0..tile_height {
                    let py = img_y + row;
                    if py >= image_height {
                        break;
                    }
                    let src = row * tile_width * spp;
                    let dst = (py * image_width + img_x) * spp;
                    let len = (cols * spp)
                        .min(tile.len().saturating_sub(src))
                        .min(raw.len() - dst);
                    raw[dst..dst + len].copy_from_slice(&tile[src..src + len]);
                }
            }

            tile_idx = batch_end;
        }

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

        let mut cropped = Vec::with_capacity(new_width * new_height * spp);
        for y in top as usize..bottom as usize {
            let row_start = (y * full_width + left as usize) * spp;
            let row_end = row_start + new_width * spp;
            if row_end > raw.len() {
                break;
            }
            cropped.extend_from_slice(&raw[row_start..row_end]);
        }
        cropped.resize(new_width * new_height * spp, 0);

        *raw = cropped;
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
            } else if self.width == 6080 && model.contains("K-3") && !model.contains("K-3 II") && !model.contains("K-3 III") {
                self.crop_raw(raw, spp, 0, 4, self.height, 4 + 6040);
            } else if self.width == 7424 && model.contains("645D") {
                self.crop_raw(raw, spp, 29, 48, 29 + 5502, 48 + 7328);
            }
        }

        if is_ricoh && self.width == 4096 && self.height == 3014 {
            self.crop_raw(raw, spp, 0, 0, self.height, 4014);
        }
    }

    fn apply_orientation<T: Copy + Default>(&mut self, data: Vec<T>, channels: usize) -> Vec<T> {
        let orientation = self.dng.orientation;
        let w = self.width as usize;
        let h = self.height as usize;
        if !(2..=8).contains(&orientation) || data.len() != w * h * channels {
            return data;
        }

        let (dw, dh) = if orientation >= 5 { (h, w) } else { (w, h) };
        let mut out = vec![T::default(); data.len()];
        for dy in 0..dh {
            for dx in 0..dw {
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
                let src = (sy * w + sx) * channels;
                let dst = (dy * dw + dx) * channels;
                out[dst..dst + channels].copy_from_slice(&data[src..src + channels]);
            }
        }

        self.width = dw as u32;
        self.height = dh as u32;
        out
    }

    fn demosaic_bilinear(&self, raw: &[u16]) -> Vec<u16> {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut rgb = vec![0u16; w * h * 3];

        for y in 0..h {
            for x in 0..w {
                let color = self.cfa_color(x as u32, y as u32);
                let (r, g, b) = self.interpolate_pixel(raw, x, y, w, h, color);
                let base = (y * w + x) * 3;
                rgb[base] = r;
                rgb[base + 1] = g;
                rgb[base + 2] = b;
            }
        }
        rgb
    }

    fn sample(&self, raw: &[u16], x: i32, y: i32, w: usize, h: usize) -> u16 {
        let cx = x.clamp(0, w as i32 - 1) as usize;
        let cy = y.clamp(0, h as i32 - 1) as usize;
        raw[cy * w + cx]
    }

    fn avg_in_bounds(&self, raw: &[u16], x: i32, y: i32, w: usize, h: usize, offsets: &[(i32, i32)]) -> u16 {
        let mut sum = 0u32;
        let mut count = 0u32;
        for &(dx, dy) in offsets {
            let nx = x + dx;
            let ny = y + dy;
            if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                sum += raw[ny as usize * w + nx as usize] as u32;
                count += 1;
            }
        }
        if count == 0 { return 0; }
        (sum / count) as u16
    }

    fn interpolate_pixel(&self, raw: &[u16], x: usize, y: usize, w: usize, h: usize, color: usize) -> (u16, u16, u16) {
        let xi = x as i32;
        let yi = y as i32;
        let s = |dx: i32, dy: i32| self.sample(raw, xi + dx, yi + dy, w, h);
        let a = |offsets: &[(i32, i32)]| self.avg_in_bounds(raw, xi, yi, w, h, offsets);
        let pattern = self.dng.cfa_pattern.unwrap_or(CfaPattern::Rggb);
        let row_parity = ((y as u32 & 1) ^ self.cfa_phase.0) as usize;
        let col_parity = ((x as u32 & 1) ^ self.cfa_phase.1) as usize;
        let _ = color;

        let (r, g, b) = match pattern {
            CfaPattern::Rggb => match (row_parity, col_parity) {
                (0, 0) => (s(0,0), a(&[(-1,0),(1,0),(0,-1),(0,1)]), a(&[(-1,-1),(1,-1),(-1,1),(1,1)])),
                (0, 1) => (a(&[(-1,0),(1,0)]), s(0,0), a(&[(0,-1),(0,1)])),
                (1, 0) => (a(&[(0,-1),(0,1)]), s(0,0), a(&[(-1,0),(1,0)])),
                (1, 1) => (a(&[(-1,-1),(1,-1),(-1,1),(1,1)]), a(&[(-1,0),(1,0),(0,-1),(0,1)]), s(0,0)),
                _ => (s(0,0), s(0,0), s(0,0)),
            },
            CfaPattern::Bggr => match (row_parity, col_parity) {
                (0, 0) => (a(&[(-1,-1),(1,-1),(-1,1),(1,1)]), a(&[(-1,0),(1,0),(0,-1),(0,1)]), s(0,0)),
                (0, 1) => (a(&[(0,-1),(0,1)]), s(0,0), a(&[(-1,0),(1,0)])),
                (1, 0) => (a(&[(-1,0),(1,0)]), s(0,0), a(&[(0,-1),(0,1)])),
                (1, 1) => (s(0,0), a(&[(-1,0),(1,0),(0,-1),(0,1)]), a(&[(-1,-1),(1,-1),(-1,1),(1,1)])),
                _ => (s(0,0), s(0,0), s(0,0)),
            },
            CfaPattern::Grbg => match (row_parity, col_parity) {
                (0, 0) => (a(&[(-1,0),(1,0)]), s(0,0), a(&[(0,-1),(0,1)])),
                (0, 1) => (s(0,0), a(&[(-1,0),(1,0),(0,-1),(0,1)]), a(&[(-1,-1),(1,-1),(-1,1),(1,1)])),
                (1, 0) => (a(&[(-1,-1),(1,-1),(-1,1),(1,1)]), a(&[(-1,0),(1,0),(0,-1),(0,1)]), s(0,0)),
                (1, 1) => (a(&[(0,-1),(0,1)]), s(0,0), a(&[(-1,0),(1,0)])),
                _ => (s(0,0), s(0,0), s(0,0)),
            },
            CfaPattern::Gbrg => match (row_parity, col_parity) {
                (0, 0) => (a(&[(0,-1),(0,1)]), s(0,0), a(&[(-1,0),(1,0)])),
                (0, 1) => (a(&[(-1,-1),(1,-1),(-1,1),(1,1)]), a(&[(-1,0),(1,0),(0,-1),(0,1)]), s(0,0)),
                (1, 0) => (s(0,0), a(&[(-1,0),(1,0),(0,-1),(0,1)]), a(&[(-1,-1),(1,-1),(-1,1),(1,1)])),
                (1, 1) => (a(&[(-1,0),(1,0)]), s(0,0), a(&[(0,-1),(0,1)])),
                _ => (s(0,0), s(0,0), s(0,0)),
            },
            CfaPattern::Unknown => { let v = s(0,0); (v, v, v) }
        };
        (r, g, b)
    }

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

            ColorTransform {
                rgb_cam: Some(invert_3x3(&cam_rgb)),
                pre_mul: cam_mul.unwrap_or(matrix_pre_mul),
            }
        } else {
            ColorTransform {
                rgb_cam: None,
                pre_mul: cam_mul.unwrap_or([1.0; 3]),
            }
        }
    }

    fn scale_colors(&self, raw: &[u16], pre_mul: [f32; 3], is_cfa: bool) -> Vec<u16> {
        let w = self.width as usize;
        let spp = if is_cfa { 1 } else { (self.header.samples_per_pixel as usize).max(1) };
        let row_samples = w * spp;
        if raw.is_empty() || row_samples == 0 {
            return Vec::new();
        }

        let channel_rows: Vec<Vec<usize>> = if is_cfa {
            (0..2)
                .map(|y| (0..row_samples).map(|x| self.cfa_color(x as u32, y)).collect())
                .collect()
        } else {
            vec![(0..row_samples).map(|i| (i % spp).min(2)).collect()]
        };

        let black_row_count = match &self.dng.black_level {
            None | Some(BlackLevel::Uniform(_)) => 1,
            Some(BlackLevel::PerChannel(_)) => channel_rows.len(),
            Some(BlackLevel::Spatial { repeat_rows, .. }) => (*repeat_rows as usize).max(1),
        };
        let black_rows: Vec<Vec<f32>> = (0..black_row_count)
            .map(|y| {
                (0..row_samples)
                    .map(|s| self.get_black((s / spp) as u32, y as u32, channel_rows[y % channel_rows.len()][s]))
                    .collect()
            })
            .collect();

        let lin_table = self.dng.linearization_table.as_deref().filter(|t| !t.is_empty());

        let mut vals = vec![0.0f32; raw.len()];
        let mut data_max = 0.0f32;

        for (y, (raw_row, val_row)) in raw.chunks(row_samples).zip(vals.chunks_mut(row_samples)).enumerate() {
            let black_row = &black_rows[y % black_rows.len()];
            match lin_table {
                Some(table) => {
                    let last = table.len() - 1;
                    for ((v, &r), &b) in val_row.iter_mut().zip(raw_row).zip(black_row) {
                        *v = (table[(r as usize).min(last)] as f32 - b).max(0.0);
                    }
                }
                None => {
                    for ((v, &r), &b) in val_row.iter_mut().zip(raw_row).zip(black_row) {
                        *v = (r as f32 - b).max(0.0);
                    }
                }
            }
            data_max = val_row.iter().copied().fold(data_max, f32::max);
        }

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

        let scale_rows: Vec<Vec<f32>> = channel_rows
            .iter()
            .map(|row| row.iter().map(|&ch| scale_mul[ch]).collect())
            .collect();

        let mut out = vec![0u16; raw.len()];
        for (y, (val_row, out_row)) in vals.chunks(row_samples).zip(out.chunks_mut(row_samples)).enumerate() {
            let scale_row = &scale_rows[y % scale_rows.len()];
            for ((o, &v), &s) in out_row.iter_mut().zip(val_row).zip(scale_row) {
                *o = (v * s).clamp(0.0, 65535.0) as u16;
            }
        }

        out
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;
        let first_ifd_offset = file_header.first_ifd_offset;

        let mut queue: Vec<u32> = vec![first_ifd_offset];
        let mut visited: HashSet<u32> = HashSet::new();
        let mut all_ifds: Vec<IfdSummary> = Vec::new();

        while let Some(ifd_offset) = queue.first().cloned() {
            queue.remove(0);
            if ifd_offset == 0 || !visited.insert(ifd_offset) {
                continue;
            }

            let Ok(parsed) = Ifd::read(&mut self.reader, ifd_offset, self.byte_order, &mut self.limits) else {
                continue;
            };

            let summary = self.summarize_ifd(parsed);

            for &sub in &summary.sub_ifd_offsets {
                queue.push(sub);
            }
            if summary.ifd.next_ifd_offset != 0 {
                queue.push(summary.ifd.next_ifd_offset);
            }
            all_ifds.push(summary);
        }

        let best = all_ifds
            .iter()
            .filter(|s| s.is_raw && s.has_data)
            .max_by_key(|s| s.width as u64 * s.height as u64)
            .or_else(|| {
                all_ifds
                    .iter()
                    .filter(|s| s.has_data)
                    .max_by_key(|s| s.width as u64 * s.height as u64)
            })
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
                parsed_best = Ifd::read(&mut self.reader, best_offset, self.byte_order, &mut self.limits)?;
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

        self.limits.reserve_buffer(self.width, self.height, 4)?;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();

        let mut raw_pixels = if is_tiled {
            self.read_raw_tile_data()?
        } else {
            self.read_raw_strip_data()?
        };

        let spp = (self.header.samples_per_pixel as usize).max(1);
        self.crop_to_active_area(&mut raw_pixels, spp);
        self.apply_camera_quirks(&mut raw_pixels, spp);

        let w = self.width as usize;
        let h = self.height as usize;
        let is_cfa = self.dng.photometric_cfa && spp == 1 && self.dng.cfa_pattern.is_some();

        let transform = self.build_color_transform();

        let deep = self.header.bits_per_sample.first().copied().unwrap_or(16) > 8;

        let pixel_data = if is_cfa {
            let scaled = self.scale_colors(&raw_pixels, transform.pre_mul, true);
            let demosaiced = self.demosaic_bilinear(&scaled);
            let rgb = convert_to_rgb16(&demosaiced, w * h, transform.rgb_cam);
            let rgb = self.apply_orientation(rgb, 3);
            if deep {
                PixelData::RGB16(rgb)
            } else {
                PixelData::RGB8(downscale_to_u8(&rgb))
            }
        } else if spp >= 3 {
            let scaled = self.scale_colors(&raw_pixels, transform.pre_mul, false);
            let pixel_count = w * h;
            let mut rgb = vec![0u16; pixel_count * 3];
            for i in 0..pixel_count {
                let src = i * spp;
                for c in 0..3 {
                    rgb[i * 3 + c] = scaled.get(src + c).copied().unwrap_or(0);
                }
            }
            let rgb = convert_to_rgb16(&rgb, pixel_count, transform.rgb_cam);
            let rgb = self.apply_orientation(rgb, 3);
            if deep {
                PixelData::RGB16(rgb)
            } else {
                PixelData::RGB8(downscale_to_u8(&rgb))
            }
        } else {
            let scaled = self.scale_colors(&raw_pixels, [1.0, 1.0, 1.0], false);
            let gray = convert_to_gray16(&scaled);
            let gray = self.apply_orientation(gray, 1);
            if deep {
                PixelData::L16(gray)
            } else {
                PixelData::L8(downscale_to_u8(&gray))
            }
        };

        let frame = ImageFrame::new(self.width, self.height, pixel_data, 0);
        Ok(Image::new(self.width, self.height, frame.pixel_format(), vec![frame]))
    }
}

#[cfg(feature = "rayon")]
fn batch_len(piece_bytes: usize) -> usize {
    const TARGET_BATCH_BYTES: usize = 32 * 1024 * 1024;
    (TARGET_BATCH_BYTES / piece_bytes.max(1)).max(rayon::current_num_threads()).max(1)
}

#[cfg(not(feature = "rayon"))]
fn batch_len(_piece_bytes: usize) -> usize {
    1
}

struct StripContext<'a> {
    header: &'a TiffHeader,
    dng: &'a DngMetadata,
    byte_order: ByteOrder,
    lossy_curves: Option<&'a [[u16; 256]; 3]>,
    float_scale: Option<f32>,
}

impl StripContext<'_> {
    fn build_lossy_curves(&self) -> Box<[[u16; 256]; 3]> {
        let mut curves = Box::new([[0u16; 256]; 3]);

        let mut filled = [false; 3];
        for poly in &self.dng.map_polynomials {
            let start = poly.plane as usize;
            let end = (poly.plane + poly.planes.max(1)) as usize;
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
        let decompressed_bytes = match self.header.compression {
            Compression::None => data,
            Compression::LZW => {
                let mut d = decompress_lzw(&data);
                self.apply_row_predictor(&mut d, width, bps);
                d
            }
            Compression::PackBits => decompress_packbits(&data),
            Compression::AdobeDeflate | Compression::Deflate => {
                let mut d = decompress_deflate(&data);
                self.apply_row_predictor(&mut d, width, bps);
                d
            }
            Compression::JPEG | Compression::OldJPEG => {
                return self.decompress_jpeg_lossless(data, rows, width);
            }
            Compression::LossyJPEG => {
                return self.decompress_jpeg_lossy(data, rows, width);
            }
            _ => {
                log_warn!("Unsupported DNG compression {:?}, using raw bytes", self.header.compression);
                data
            }
        };

        let spp = (self.header.samples_per_pixel as usize).max(1);
        let count = rows * width * spp;
        match self.float_scale {
            Some(scale) => Ok(self.unpack_float_pixels(&decompressed_bytes, count, bps, scale)),
            None => Ok(self.unpack_pixels(&decompressed_bytes, count, bps)),
        }
    }

    fn apply_row_predictor(&self, data: &mut Vec<u8>, width: usize, bps: u16) {
        let spp = self.header.samples_per_pixel.max(1);
        let big_endian = self.byte_order == ByteOrder::BigEndian;
        match self.header.predictor {
            Predictor::None => {}
            Predictor::HorizontalDifferencing => {
                if big_endian {
                    apply_predictor_horizontal_be(data, width as u32, spp, bps);
                } else {
                    apply_predictor_horizontal(data, width as u32, spp, bps);
                }
            }
            Predictor::FloatingPoint | Predictor::FloatingPointX2 | Predictor::FloatingPointX4 => {
                let x = predictor_x_factor(self.header.predictor);
                apply_predictor_float(data, (width as u32 / x).max(1), spp * x as u16, bps, big_endian);
            }
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

    fn decompress_jpeg_lossless(&self, strip_data: Vec<u8>, rows: usize, width: usize) -> VexelResult<Vec<u16>> {
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = rows * width * spp;

        let jpeg_data = if !self.header.jpeg_tables.is_empty() {
            splice_jpeg_tables(&self.header.jpeg_tables, &strip_data)
        } else {
            strip_data
        };

        let cursor = Cursor::new(jpeg_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);
        jpeg_decoder.set_lossless_ssss16_is_min(self.dng.dng_version == 0 || self.dng.dng_version >= 0x01010000);
        let image = match jpeg_decoder.decode() {
            Ok(img) => img,
            Err(e) => {
                log_warn!("DNG JPEG lossless decode failed: {}", e);
                return Ok(vec![0u16; sample_count]);
            }
        };

        let precision = jpeg_decoder.precision() as u16;

        let (raw_samples, stored_bits): (Vec<u16>, u16) = match image.pixels() {
            crate::utils::image::PixelData::L16(v) => (v, 16),
            crate::utils::image::PixelData::LA16(v) => (v, 16),
            crate::utils::image::PixelData::RGB16(v) => (v, 16),
            crate::utils::image::PixelData::RGBA16(v) => (v, 16),
            crate::utils::image::PixelData::L8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            crate::utils::image::PixelData::LA8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            crate::utils::image::PixelData::RGB8(v) => (v.iter().map(|&b| b as u16).collect(), 8),
            _ => return Ok(vec![0u16; sample_count]),
        };

        let precision_shift = stored_bits.saturating_sub(precision.clamp(1, stored_bits));

        let mut out = Vec::with_capacity(sample_count);
        for &s in raw_samples.iter().take(sample_count) {
            out.push(s >> precision_shift);
        }
        while out.len() < sample_count {
            out.push(0);
        }
        Ok(out)
    }

    fn decompress_jpeg_lossy(&self, tile_data: Vec<u8>, rows: usize, width: usize) -> VexelResult<Vec<u16>> {
        let spp = (self.header.samples_per_pixel as usize).max(1);
        let sample_count = rows * width * spp;

        let cursor = Cursor::new(tile_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);
        let image = match jpeg_decoder.decode() {
            Ok(img) => img,
            Err(e) => {
                log_warn!("DNG lossy JPEG decode failed: {}", e);
                return Ok(vec![0u16; sample_count]);
            }
        };

        let bytes: Vec<u8> = match image.pixels() {
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
                    if out.len() >= count { break; }
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
        while out.len() < count { out.push(0); }
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

const XYZ_RGB: [[f32; 3]; 3] = [
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041],
];

const HISTOGRAM_SIZE: usize = 0x2000;
const AUTO_BRIGHT_THRESHOLD: f64 = 0.01;
const PARALLEL_CHUNK: usize = 0x10000;

fn convert_to_rgb16(rgb: &[u16], pixel_count: usize, rgb_cam: Option<[[f32; 3]; 3]>) -> Vec<u16> {
    let converted: Cow<'_, [u16]> = match rgb_cam {
        Some(m) => Cow::Owned(apply_cam_matrix(rgb, &m)),
        None => Cow::Borrowed(rgb),
    };

    let histogram = build_rgb_histogram(&converted);
    let t_white = auto_bright_white(&histogram, pixel_count, 3);
    let curve = gamma_curve_bt709(t_white << 3);
    map_through_curve(&converted, &curve)
}

fn convert_to_gray16(gray: &[u16]) -> Vec<u16> {
    let mut histogram = vec![[0u32; HISTOGRAM_SIZE]; 1];
    for &v in gray {
        histogram[0][(v >> 3) as usize] += 1;
    }
    let t_white = auto_bright_white(&histogram, gray.len(), 1);
    let curve = gamma_curve_bt709(t_white << 3);
    map_through_curve(gray, &curve)
}

fn downscale_to_u8(values: &[u16]) -> Vec<u8> {
    values.iter().map(|&v| (v >> 8) as u8).collect()
}

fn apply_cam_matrix(rgb: &[u16], m: &[[f32; 3]; 3]) -> Vec<u16> {
    let mut out = vec![0u16; rgb.len()];

    let transform = |dst: &mut [u16], src: &[u16]| {
        for (d, s) in dst.chunks_exact_mut(3).zip(src.chunks_exact(3)) {
            let (r, g, b) = (s[0] as f32, s[1] as f32, s[2] as f32);
            for c in 0..3 {
                d[c] = (m[c][0] * r + m[c][1] * g + m[c][2] * b).clamp(0.0, 65535.0) as u16;
            }
        }
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        out.par_chunks_mut(PARALLEL_CHUNK * 3)
            .zip(rgb.par_chunks(PARALLEL_CHUNK * 3))
            .for_each(|(dst, src)| transform(dst, src));
    }
    #[cfg(not(feature = "rayon"))]
    transform(&mut out, rgb);

    out
}

fn build_rgb_histogram(rgb: &[u16]) -> Vec<[u32; HISTOGRAM_SIZE]> {
    let accumulate = |mut hist: Vec<[u32; HISTOGRAM_SIZE]>, chunk: &[u16]| {
        for px in chunk.chunks_exact(3) {
            hist[0][(px[0] >> 3) as usize] += 1;
            hist[1][(px[1] >> 3) as usize] += 1;
            hist[2][(px[2] >> 3) as usize] += 1;
        }
        hist
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        rgb.par_chunks(PARALLEL_CHUNK * 3)
            .fold(|| vec![[0u32; HISTOGRAM_SIZE]; 3], accumulate)
            .reduce(
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

fn map_through_curve(values: &[u16], curve: &[u16]) -> Vec<u16> {
    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        values.par_iter().map(|&v| curve[v as usize]).collect()
    }
    #[cfg(not(feature = "rayon"))]
    values.iter().map(|&v| curve[v as usize]).collect()
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

fn invert_3x3(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
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
