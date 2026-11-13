use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom};

use super::color::JpegColorspace;
use super::hierarchical::HierarchicalFrame;
use super::thumbnails::scan_jpeg_thumbnails;
use crate::bitreader::BitReader;
use crate::decoders::jpeg::app::IccAssembler;
use crate::decoders::jpeg::info::{coding_name, process_name, subsampling_name, JpegInfo, JpegSection};
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::decoders::jpeg::types::{
    ArithmeticCodingTable, ColorComponentInfo, HuffmanTable, JpegCodingMethod, JpegMode, QuantizationTable, ScanData,
    DEFAULT_QUANTIZATION_TABLE,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, ImageSummary};
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn, Image, Limits};

#[rustfmt::skip]
pub(super) const SCAN_BLOCK_SIZE: usize = 64 * 1024;

pub(super) const MAX_SCANS: usize = 500;
pub(super) const MAX_SCAN_COMPONENTS: usize = 4;
pub(super) const MAX_HIERARCHICAL_FRAMES: usize = 32;
pub(super) const PROGRESSIVE_PASS_BUDGET: u64 = 128;
pub(super) const LOSSLESS_PASS_BUDGET: u64 = 4;
pub(super) const HIERARCHICAL_PIXEL_BUDGET: u64 = 4;

#[cfg(feature = "rayon")]
pub(super) const PARALLEL_LOSSLESS_MIN_SAMPLES: usize = 256 * 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum WalkMode {
    InfoOnly,
    Decode,
}

pub struct JpegDecoder<R: Read + Seek> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) limits: Limits,
    pub(super) has_jfif: bool,
    pub(super) mode: JpegMode,
    pub(super) coding_method: JpegCodingMethod,
    pub(super) quantization_tables: Vec<QuantizationTable>,
    pub(super) ac_huffman_tables: Vec<HuffmanTable>,
    pub(super) dc_huffman_tables: Vec<HuffmanTable>,
    pub(super) ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub(super) dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub(super) restart_interval: u16,
    pub(super) precision: u8,
    pub(super) components: Vec<ColorComponentInfo>,
    pub(super) scans: Vec<ScanData>,
    pub(super) info: Option<JpegInfo>,
    pub(super) walk_mode: WalkMode,
    pub(super) reader: BitReader<R>,
    pub(super) adobe_color_transform: Option<u8>,
    pub(super) is_hierarchical: bool,
    pub(super) dhp_width: u32,
    pub(super) dhp_height: u32,
    pub(super) pending_expand_h: bool,
    pub(super) pending_expand_v: bool,
    pub(super) hierarchical_frames: Vec<HierarchicalFrame>,
    pub(super) lossless_ssss16_is_min: bool,
    pub(super) lossless_skip_color_transform: bool,
    pub(super) icc: IccAssembler,
    pub(super) collected_scans: usize,
    pub(super) frame_has_scan: bool,
    pub(super) entropy: Vec<u8>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            has_jfif: false,
            mode: JpegMode::Baseline,
            coding_method: JpegCodingMethod::Huffman,
            precision: 0,
            components: Vec::new(),
            quantization_tables: Vec::new(),
            ac_huffman_tables: Vec::new(),
            dc_huffman_tables: Vec::new(),
            ac_arithmetic_tables: Vec::new(),
            dc_arithmetic_tables: Vec::new(),
            restart_interval: 0,
            scans: Vec::new(),
            info: None,
            walk_mode: WalkMode::Decode,
            reader: BitReader::new(reader),
            adobe_color_transform: None,
            is_hierarchical: false,
            dhp_width: 0,
            dhp_height: 0,
            pending_expand_h: false,
            pending_expand_v: false,
            hierarchical_frames: Vec::new(),
            lossless_ssss16_is_min: true,
            lossless_skip_color_transform: false,
            icc: IccAssembler::default(),
            entropy: Vec::new(),
            collected_scans: 0,
            frame_has_scan: false,
        }
    }

    pub fn set_lossless_ssss16_is_min(&mut self, is_min: bool) {
        self.lossless_ssss16_is_min = is_min;
    }

    pub fn set_lossless_skip_color_transform(&mut self, skip: bool) {
        self.lossless_skip_color_transform = skip;
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> JpegInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk(WalkMode::InfoOnly) {
                log_warn!("Failed to read JPEG structure: {}", e);
            }
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    pub(super) fn reset_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.has_jfif = false;
        self.mode = JpegMode::Baseline;
        self.coding_method = JpegCodingMethod::Huffman;
        self.precision = 0;
        self.components.clear();
        self.quantization_tables.clear();
        self.ac_huffman_tables.clear();
        self.dc_huffman_tables.clear();
        self.ac_arithmetic_tables.clear();
        self.dc_arithmetic_tables.clear();
        self.restart_interval = 0;
        self.scans.clear();
        self.adobe_color_transform = None;
        self.is_hierarchical = false;
        self.dhp_width = 0;
        self.dhp_height = 0;
        self.pending_expand_h = false;
        self.pending_expand_v = false;
        self.hierarchical_frames.clear();
        self.icc.clear();
        self.entropy.clear();
        self.collected_scans = 0;
        self.frame_has_scan = false;
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub(super) fn record(&mut self, offset: u64, length: Option<u64>, section: JpegSection) {
        if let Some(info) = &mut self.info {
            info.push(offset, length, section);
        }
    }

    pub(super) fn quant_table_i32(&self, table_id: u8) -> [i32; 64] {
        let natural: &[u16] = self
            .quantization_tables
            .iter()
            .find(|q| q.id == table_id)
            .map(|t| t.table_natural.as_slice())
            .unwrap_or_else(|| {
                log_warn!("Quantization table not found for component, substituting default one.");
                &DEFAULT_QUANTIZATION_TABLE
            });
        let mut out = [0i32; 64];
        for (o, &q) in out.iter_mut().zip(natural) {
            *o = q as i32;
        }
        out
    }

    pub(super) fn scan_component_index(&self, scan: &ScanData, slot: usize) -> Option<usize> {
        let id = scan.components.get(slot)?.component_id;
        if scan.components.len() == self.components.len() && !self.scan_ids_match(scan) {
            if slot == 0 {
                log_warn!("Scan component ids do not match the frame, pairing components by position");
            }
            return Some(slot);
        }
        self.components.iter().position(|c| c.id == id)
    }

    fn scan_ids_match(&self, scan: &ScanData) -> bool {
        scan.components.iter().enumerate().all(|(i, sc)| {
            self.components.iter().any(|c| c.id == sc.component_id)
                && !scan.components[..i].iter().any(|other| other.component_id == sc.component_id)
        })
    }

    pub(super) fn component_sampling(&self) -> (Vec<(usize, usize)>, usize, usize) {
        let single = self.components.len() == 1;
        let sampling: Vec<(usize, usize)> = self
            .components
            .iter()
            .map(|c| {
                if single {
                    (1, 1)
                } else {
                    (c.horizontal_sampling_factor.max(1) as usize, c.vertical_sampling_factor.max(1) as usize)
                }
            })
            .collect();
        let hmax = sampling.iter().map(|s| s.0).max().unwrap_or(1);
        let vmax = sampling.iter().map(|s| s.1).max().unwrap_or(1);
        (sampling, hmax, vmax)
    }

    pub(super) fn detect_colorspace(&self) -> JpegColorspace {
        let nc = self.components.len();
        let ids: Vec<u8> = self.components.iter().map(|c| c.id).collect();

        match nc {
            1 => JpegColorspace::Grayscale,
            3 => {
                if ids == [0x52, 0x47, 0x42] {
                    return JpegColorspace::Rgb;
                }
                if self.has_jfif {
                    return JpegColorspace::YCbCr;
                }
                if let Some(ct) = self.adobe_color_transform {
                    return match ct {
                        0 => JpegColorspace::Rgb,
                        _ => JpegColorspace::YCbCr,
                    };
                }
                JpegColorspace::YCbCr
            }
            4 => {
                if ids == [0x43, 0x4D, 0x59, 0x4B] {
                    return JpegColorspace::Cmyk;
                }
                if let Some(ct) = self.adobe_color_transform {
                    return match ct {
                        2 => JpegColorspace::Ycck,
                        _ => JpegColorspace::Cmyk,
                    };
                }
                if ids == [0x00, 0x01, 0x02, 0x03] {
                    return JpegColorspace::Rgba;
                }
                JpegColorspace::Cmyk
            }
            _ => JpegColorspace::Unknown,
        }
    }

    pub(super) fn finalize_current_frame(&mut self) {
        if self.scans.is_empty() {
            return;
        }

        if self.hierarchical_frames.len() >= MAX_HIERARCHICAL_FRAMES {
            log_warn!("More than {} hierarchical frames, ignoring the rest", MAX_HIERARCHICAL_FRAMES);
            self.scans.clear();
            return;
        }

        let frame = HierarchicalFrame {
            width: self.width,
            height: self.height,
            mode: self.mode.clone(),
            coding_method: self.coding_method.clone(),
            precision: self.precision,
            components: self.components.clone(),
            quantization_tables: self.quantization_tables.clone(),
            scans: std::mem::take(&mut self.scans),
        };

        self.hierarchical_frames.push(frame);
        self.pending_expand_h = false;
        self.pending_expand_v = false;
    }

    pub(super) fn start_frame(&mut self, marker: u8, segment_start: u64) -> VexelResult<()> {
        if self.is_hierarchical {
            self.finalize_current_frame();
        }
        self.frame_has_scan = false;

        let mode = match marker & 0x07 {
            1 => Some(JpegMode::ExtendedSequential),
            2 => Some(JpegMode::Progressive),
            3 => Some(JpegMode::Lossless),
            5 => Some(JpegMode::DifferentialSequential),
            6 => Some(JpegMode::DifferentialProgressive),
            7 => Some(JpegMode::DifferentialLossless),
            _ => None,
        };
        let arithmetic = marker & 0x08 != 0;

        if let Some(mode) = mode {
            self.mode = mode;
        }
        if arithmetic {
            self.coding_method = JpegCodingMethod::Arithmetic;
        }

        self.read_start_of_frame(marker, segment_start)
    }

    pub(super) fn walk(&mut self, mode: WalkMode) -> VexelResult<()> {
        self.reset_state();
        self.walk_mode = mode;
        self.info = Some(JpegInfo::default());
        self.reader.seek(SeekFrom::Start(0))?;

        let result = self.walk_markers();
        self.finish_info();
        result
    }

    pub(super) fn walk_markers(&mut self) -> VexelResult<()> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            let Some(marker) = marker else {
                log_debug!("End of file reached");
                break;
            };

            log_debug!("Found marker: {:?}", marker);

            let segment_start = self.reader.stream_position().unwrap_or(0).saturating_sub(2);
            let code = marker.to_u16();

            let result = match marker {
                JpegMarker::SOI => {
                    self.record(segment_start, Some(2), JpegSection::StartOfImage);
                    Ok(())
                }
                JpegMarker::EOI => {
                    self.record(segment_start, Some(2), JpegSection::EndOfImage);
                    self.record_trailing_data(segment_start + 2);
                    break;
                }
                JpegMarker::COM => self.read_com(segment_start),
                JpegMarker::APP0
                | JpegMarker::APP1
                | JpegMarker::APP2
                | JpegMarker::APP3
                | JpegMarker::APP4
                | JpegMarker::APP5
                | JpegMarker::APP6
                | JpegMarker::APP7
                | JpegMarker::APP8
                | JpegMarker::APP9
                | JpegMarker::APP10
                | JpegMarker::APP11
                | JpegMarker::APP12
                | JpegMarker::APP13
                | JpegMarker::APP14
                | JpegMarker::APP15 => self.read_app((code & 0x0F) as u8, segment_start),
                JpegMarker::SOF0
                | JpegMarker::SOF1
                | JpegMarker::SOF2
                | JpegMarker::SOF3
                | JpegMarker::SOF5
                | JpegMarker::SOF6
                | JpegMarker::SOF7
                | JpegMarker::SOF9
                | JpegMarker::SOF10
                | JpegMarker::SOF11
                | JpegMarker::SOF13
                | JpegMarker::SOF14
                | JpegMarker::SOF15 => {
                    if !self.is_hierarchical && self.frame_has_scan {
                        log_warn!("Frame header {:?} after a completed scan, ignoring the rest of the file", marker);
                        if self.reader.seek(SeekFrom::Start(segment_start)).is_ok() {
                            self.record_trailing_data(segment_start);
                        }
                        break;
                    }
                    self.start_frame((code & 0xFF) as u8, segment_start)
                }
                JpegMarker::DHP => self.read_dhp(segment_start),
                JpegMarker::EXP => self.read_exp(segment_start),
                JpegMarker::DRI => self.read_restart_interval(segment_start),
                JpegMarker::DNL => self.read_dnl(segment_start),
                JpegMarker::DQT => self.read_quantization_table(segment_start),
                JpegMarker::DHT => self.read_huffman_table(segment_start),
                JpegMarker::DAC => self.read_dac(segment_start),
                JpegMarker::SOS => self.read_start_of_scan(segment_start),
                JpegMarker::TEM
                | JpegMarker::RST0
                | JpegMarker::RST1
                | JpegMarker::RST2
                | JpegMarker::RST3
                | JpegMarker::RST4
                | JpegMarker::RST5
                | JpegMarker::RST6
                | JpegMarker::RST7 => {
                    log_warn!("Standalone marker {:?} between marker segments, skipping", marker);
                    self.record(segment_start, Some(2), JpegSection::StandaloneMarker(code));
                    Ok(())
                }
                _ => {
                    log_warn!("Unhandled marker found: {:?}", marker);
                    self.skip_unknown_marker_segment(code, segment_start)
                }
            };

            match result {
                Ok(_) => {}
                Err(VexelError::LimitExceeded(_)) => return result,
                Err(e) => {
                    log_warn!("Failed to process {:?} marker segment: {}", marker, e);
                }
            }
        }

        Ok(())
    }

    pub(super) fn record_trailing_data(&mut self, offset: u64) {
        let remaining = self.reader.bytes_left().unwrap_or(0);
        if remaining > 0 {
            self.record(offset, Some(remaining), JpegSection::TrailingData);
        }
    }

    pub(super) fn finish_info(&mut self) {
        let summary = self.build_summary();
        if let Some(info) = &mut self.info {
            info.summary = summary;
        }
    }

    pub(super) fn build_summary(&self) -> Option<ImageSummary> {
        let info = self.info.as_ref()?;
        let first_frame = info.sections.iter().find_map(|section| match &section.data {
            JpegSection::Frame(frame) => Some(frame.marker),
            _ => None,
        })?;

        let (width, height) =
            if self.is_hierarchical { (self.dhp_width, self.dhp_height) } else { (self.width, self.height) };

        let color_model = match self.detect_colorspace() {
            JpegColorspace::Grayscale => ColorModel::Gray,
            JpegColorspace::Rgb => ColorModel::Rgb,
            JpegColorspace::YCbCr => ColorModel::YCbCr,
            JpegColorspace::Cmyk => ColorModel::Cmyk,
            JpegColorspace::Ycck => ColorModel::Ycck,
            JpegColorspace::Rgba => ColorModel::Rgba,
            JpegColorspace::Unknown => ColorModel::Unknown,
        };

        let mut summary = ImageSummary::new(width, height, color_model, self.components.len() as u16, self.precision);
        summary.has_alpha = color_model == ColorModel::Rgba;
        let hierarchy = if self.is_hierarchical { "Hierarchical, " } else { "" };
        summary.compression =
            Some(format!("{}{}, {} coding", hierarchy, process_name(first_frame), coding_name(first_frame)));
        let factors: Vec<(u8, u8)> =
            self.components.iter().map(|c| (c.horizontal_sampling_factor, c.vertical_sampling_factor)).collect();
        summary.subsampling = subsampling_name(&factors);
        summary.icc_profile = self.icc.assemble();
        Some(summary)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        if let Err(e) = self.walk(WalkMode::Decode) {
            self.info = None;
            return Err(e);
        }

        log_debug!("Dimensions: {}x{}. Number of pixels: {}", self.width, self.height, self.width * self.height);
        log_debug!("Number of components: {}", self.components.len());
        log_debug!("Number of scans: {}", self.scans.len());
        log_debug!("Mode: {:?}", self.mode);
        log_debug!("Coding method: {:?}", self.coding_method);
        log_debug!("Bit depth: {}", self.precision);
        log_debug!("Restart interval: {}", self.restart_interval);
        log_debug!(
            "Sampling factors: {:?}",
            self.components
                .iter()
                .map(|c| format!("{}/{}", c.horizontal_sampling_factor, c.vertical_sampling_factor))
                .collect::<Vec<String>>()
                .join(", ")
        );

        if self.is_hierarchical {
            self.finalize_current_frame();
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if self.components.is_empty() {
            return Err(VexelError::Custom("Invalid JPEG: no components".into()));
        }

        if self.is_hierarchical {
            return self.decode_hierarchical();
        }

        match &self.mode {
            JpegMode::Baseline | JpegMode::ExtendedSequential => self.decode_sequential(),
            JpegMode::Progressive => self.decode_progressive(),
            JpegMode::Lossless => self.decode_lossless(),
            JpegMode::DifferentialSequential | JpegMode::DifferentialProgressive | JpegMode::DifferentialLossless => {
                log_warn!("Differential JPEG mode outside hierarchical context, treating as sequential");
                self.decode_sequential()
            }
        }
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        scan_jpeg_thumbnails(&mut self.reader, &self.limits)
    }
}
