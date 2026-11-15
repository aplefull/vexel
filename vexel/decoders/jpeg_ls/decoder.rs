use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use super::bitreader::JlsBitReader;
use super::info::{
    JpegLsExtension, JpegLsFrame, JpegLsFrameComponent, JpegLsInfo, JpegLsMappingTable, JpegLsOversizeDimensions,
    JpegLsPresetParameters, JpegLsScan, JpegLsScanComponent, JpegLsSection,
};
use super::markers::{JpegLsMarker, JPEG_LS_MARKERS};
use super::types::*;
use crate::bitreader::BitReader;
use crate::decoders::jpeg::app::{parse_app, parse_comment, AppMetadata, AppSection, AppSegment};
use crate::decoders::jpeg::info::subsampling_name;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, ImageSummary};
use crate::utils::marker::Marker;
use crate::{log_warn, Image, Limits, PixelData};

enum MappedSamples {
    Palette { data: Vec<u8>, channels: usize },
    Values { data: Vec<u16>, wide: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WalkMode {
    InfoOnly,
    Decode,
}

pub struct JpegLsDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    limits: Limits,
    frame: Option<FrameHeader>,
    pending_scan: Option<ScanHeader>,
    scans: Vec<(ScanHeader, Vec<u8>)>,
    color_transform: u8,
    restart_interval: usize,
    oversize_dimensions: Option<(u32, u32)>,
    mapping_tables: HashMap<u8, MappingTable>,
    info: Option<JpegLsInfo>,
    walk_mode: WalkMode,
    app_metadata: AppMetadata,
}

impl<R: Read + Seek> JpegLsDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BitReader::new(reader),
            limits: Limits::default(),
            frame: None,
            pending_scan: None,
            scans: Vec::new(),
            color_transform: 0,
            restart_interval: 0,
            oversize_dimensions: None,
            mapping_tables: HashMap::new(),
            info: None,
            walk_mode: WalkMode::Decode,
            app_metadata: AppMetadata::default(),
        }
    }

    pub fn precision(&self) -> u8 {
        self.frame.as_ref().map(|f| f.precision).unwrap_or(0)
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> JpegLsInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk(WalkMode::InfoOnly) {
                log_warn!("Failed to read JPEG-LS structure: {}", e);
            }
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        crate::decoders::jpeg::thumbnails::scan_jpeg_thumbnails(&mut self.reader, &self.limits)
    }

    fn record(&mut self, offset: u64, length: Option<u64>, section: JpegLsSection) {
        if let Some(info) = &mut self.info {
            info.push(offset, length, section);
        }
    }

    fn read_u8(&mut self) -> std::io::Result<u8> {
        Ok(self.reader.read_bits(8)? as u8)
    }

    fn read_u16(&mut self) -> std::io::Result<u16> {
        Ok(self.reader.read_bits(16)? as u16)
    }

    fn read_be_uint(&mut self, bytes: usize) -> std::io::Result<u32> {
        let mut value = 0u32;
        for _ in 0..bytes {
            value = (value << 8) | u32::from(self.read_u8()?);
        }
        Ok(value)
    }

    fn skip_bytes(&mut self, count: usize) -> std::io::Result<()> {
        for _ in 0..count {
            self.read_u8()?;
        }
        Ok(())
    }

    fn read_sized_value(&mut self, length: u16, what: &str) -> std::io::Result<u32> {
        let size = usize::from(length.saturating_sub(2));
        if (2..=4).contains(&size) {
            return self.read_be_uint(size);
        }
        log_warn!("JPEG-LS: {} segment has invalid length {}, reading a 16-bit value", what, length);
        let value = self.read_be_uint(2)?;
        self.skip_bytes(size.saturating_sub(2))?;
        Ok(value)
    }

    fn read_segment_payload(&mut self) -> VexelResult<(u16, Vec<u8>)> {
        let length = self.read_u16()?;
        let payload_len = usize::from(length.saturating_sub(2));
        let mut payload = Vec::with_capacity(payload_len);
        for _ in 0..payload_len {
            match self.read_u8() {
                Ok(byte) => payload.push(byte),
                Err(_) => break,
            }
        }
        Ok((length, payload))
    }

    fn read_sof(&mut self, marker_start: u64) -> VexelResult<()> {
        let marker_len = self.read_u16()?;
        let precision = self.read_u8()?;
        let raw_height = self.read_u16()?;
        let raw_width = self.read_u16()?;
        let comp_count = self.read_u8()?;

        let effective_precision = precision.clamp(2, 16);
        if effective_precision != precision {
            log_warn!("JPEG-LS: invalid sample precision {}, using {}", precision, effective_precision);
        }
        let alpha = 1i32 << effective_precision;

        let mut components = Vec::new();
        let mut info_components = Vec::new();
        let mut failure = None;
        for _ in 0..comp_count as usize {
            let entry = self.read_u8().and_then(|id| Ok((id, self.read_u8()?, self.read_u8()?)));
            let (id, sampling, tq) = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };
            let horizontal_sampling = (sampling >> 4) & 0xF;
            let vertical_sampling = sampling & 0xF;
            if horizontal_sampling == 0 || vertical_sampling == 0 {
                log_warn!(
                    "JPEG-LS: component {} has invalid sampling {}x{}, using 1",
                    id,
                    horizontal_sampling,
                    vertical_sampling
                );
            }
            components.push(ComponentInfo {
                id,
                horizontal_sampling: horizontal_sampling.max(1),
                vertical_sampling: vertical_sampling.max(1),
            });
            info_components.push(JpegLsFrameComponent {
                id,
                horizontal_sampling: (sampling >> 4) & 0xF,
                vertical_sampling: sampling & 0xF,
                reserved: tq,
            });
        }

        self.record(
            marker_start,
            Some(2 + u64::from(marker_len)),
            JpegLsSection::Frame(JpegLsFrame {
                precision,
                height: raw_height,
                width: raw_width,
                component_count: comp_count,
                components: info_components,
            }),
        );

        if let Some(e) = failure {
            return Err(e.into());
        }

        let (mut width, mut height) = (u32::from(raw_width), u32::from(raw_height));
        if let Some((oversize_height, oversize_width)) = self.oversize_dimensions {
            if height == 0 {
                height = oversize_height;
            }
            if width == 0 {
                width = oversize_width;
            }
        }

        self.frame = Some(FrameHeader { precision: effective_precision, height, width, components, alpha });

        Ok(())
    }

    fn read_lse(&mut self, marker_start: u64) -> VexelResult<()> {
        let marker_len = self.read_u16()?;
        let id_type = self.read_u8()?;
        let length = Some(2 + u64::from(marker_len));

        match id_type {
            1 => {
                let maxval = self.read_u16()?;
                let t1 = self.read_u16()?;
                let t2 = self.read_u16()?;
                let t3 = self.read_u16()?;
                let reset = self.read_u16()?;

                let scan = self.pending_scan.get_or_insert_with(ScanHeader::default);
                scan.alpha = maxval as i32 + 1;
                scan.t1 = t1 as i32;
                scan.t2 = t2 as i32;
                scan.t3 = t3 as i32;
                scan.reset = if reset == 0 { DEFAULT_RESET } else { reset as i32 };

                self.record(
                    marker_start,
                    length,
                    JpegLsSection::PresetParameters(JpegLsPresetParameters { maxval, t1, t2, t3, reset }),
                );
            }
            2 | 3 => {
                let continuation = id_type == 3;
                let table_id = self.read_u8()?;
                let entry_width = self.read_u8()?;
                let table_len = usize::from(marker_len).saturating_sub(5);
                if self.walk_mode == WalkMode::Decode {
                    self.limits.reserve_usize(table_len)?;
                }
                let mut table = Vec::with_capacity(table_len);
                for _ in 0..table_len {
                    match self.read_u8() {
                        Ok(byte) => table.push(byte),
                        Err(_) => break,
                    }
                }
                let width = usize::from(entry_width);
                let entries: Vec<u32> = if (1..=4).contains(&width) {
                    table
                        .chunks_exact(width)
                        .map(|entry| entry.iter().fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte)))
                        .collect()
                } else {
                    log_warn!("JPEG-LS: mapping table {} has entry width {}, showing no entries", table_id, width);
                    Vec::new()
                };
                let entry_count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
                self.record(
                    marker_start,
                    length,
                    JpegLsSection::MappingTable(JpegLsMappingTable {
                        table_id,
                        entry_width,
                        continuation,
                        entry_count,
                        entries,
                    }),
                );
                if self.walk_mode == WalkMode::Decode {
                    self.store_mapping_table(continuation, table_id, width, table);
                }
            }
            4 => {
                let size = self.read_u8()?;
                let value_size = if (2..=4).contains(&size) {
                    usize::from(size)
                } else {
                    log_warn!("JPEG-LS: invalid oversize dimension size {}, using 4 bytes", size);
                    4
                };
                let height = self.read_be_uint(value_size)?;
                let width = self.read_be_uint(value_size)?;
                self.skip_bytes(usize::from(marker_len).saturating_sub(4 + 2 * value_size))?;

                self.record(
                    marker_start,
                    length,
                    JpegLsSection::OversizeDimensions(JpegLsOversizeDimensions { size, height, width }),
                );

                self.oversize_dimensions = Some((height, width));
                if let Some(frame) = &mut self.frame {
                    if frame.height == 0 {
                        frame.height = height;
                    }
                    if frame.width == 0 {
                        frame.width = width;
                    }
                }
            }
            _ => {
                for _ in 3..marker_len as usize {
                    self.read_u8()?;
                }
                self.record(
                    marker_start,
                    length,
                    JpegLsSection::Extension(JpegLsExtension {
                        id: id_type,
                        data_length: u32::from(marker_len.saturating_sub(3)),
                    }),
                );
            }
        }

        Ok(())
    }

    fn store_mapping_table(&mut self, continuation: bool, table_id: u8, entry_width: usize, data: Vec<u8>) {
        if table_id == 0 {
            log_warn!("JPEG-LS: mapping table with reserved ID 0, ignoring");
            return;
        }
        if continuation {
            match self.mapping_tables.get_mut(&table_id) {
                Some(table) if table.entry_width == entry_width => {
                    table.data.extend_from_slice(&data);
                    return;
                }
                Some(table) => {
                    log_warn!(
                        "JPEG-LS: continuation of mapping table {} has entry width {} instead of {}, ignoring it",
                        table_id,
                        entry_width,
                        table.entry_width
                    );
                    return;
                }
                None => {
                    log_warn!("JPEG-LS: continuation of undefined mapping table {}, using it as a new table", table_id);
                }
            }
        }
        self.mapping_tables.insert(table_id, MappingTable { entry_width, data });
    }

    fn read_dnl(&mut self, marker_start: u64) -> std::io::Result<()> {
        let length = self.read_u16()?;
        let number_of_lines = self.read_sized_value(length, "DNL")?;

        self.record(marker_start, Some(2 + u64::from(length)), JpegLsSection::NumberOfLines(number_of_lines));

        if let Some(ref mut frame) = self.frame {
            if number_of_lines > 0 {
                frame.height = number_of_lines;
            } else {
                log_warn!("DNL specifies 0 lines, ignoring");
            }
        } else {
            log_warn!("DNL marker encountered before SOF, ignoring");
        }

        Ok(())
    }

    fn read_sos(&mut self, marker_start: u64) -> std::io::Result<()> {
        let marker_len = self.read_u16()?;
        let comp_count = self.read_u8()?;

        let mut component_ids = Vec::new();
        let mut mapping_table_ids = Vec::new();
        let mut info_components = Vec::new();
        for _ in 0..comp_count as usize {
            let cid = self.read_u8()?;
            let tm = self.read_u8()?;
            component_ids.push(cid);
            mapping_table_ids.push(tm);
            info_components.push(JpegLsScanComponent { id: cid, mapping_table: tm });
        }

        let near = self.read_u8()?;
        let ilv_byte = self.read_u8()?;
        let pt = self.read_u8()?;

        let interleave_mode = if comp_count == 1 {
            InterleaveMode::None
        } else {
            match ilv_byte {
                0 => InterleaveMode::None,
                1 => InterleaveMode::Line,
                2 => InterleaveMode::Sample,
                _ => InterleaveMode::Line,
            }
        };

        let frame = self.frame.as_ref();
        let frame_precision = frame.map(|f| f.precision).unwrap_or(8);
        let frame_alpha = frame.map(|f| f.alpha).unwrap_or(256);

        let scan = self.pending_scan.get_or_insert_with(ScanHeader::default);
        scan.component_ids = component_ids;
        scan.mapping_table_ids = mapping_table_ids;
        scan.near = near as i32;
        scan.interleave_mode = interleave_mode;
        scan.point_transform = pt;
        if scan.alpha <= 1 {
            if pt > 0 {
                if pt >= frame_precision {
                    log_warn!("Point transform {} exceeds frame precision {}", pt, frame_precision);
                }
                scan.alpha =
                    1i32.checked_shl(u32::from(frame_precision.saturating_sub(pt))).filter(|a| *a > 1).unwrap_or(0);
            } else {
                scan.alpha = frame_alpha;
            }
        }

        let collect = self.walk_mode == WalkMode::Decode;
        let data_start = self.reader.stream_position().unwrap_or(marker_start);
        let data = self.read_compressed_data(collect)?;
        let data_end = self.reader.stream_position().unwrap_or(data_start);

        let mut scan = self.pending_scan.take().unwrap_or_default();
        scan.restart_interval = self.restart_interval;

        let alpha = if scan.alpha > 1 { scan.alpha } else { frame_alpha };
        let (t1, t2, t3) = compute_thresholds(alpha, scan.near, scan.t1, scan.t2, scan.t3);

        self.record(
            marker_start,
            Some(data_end.saturating_sub(marker_start)),
            JpegLsSection::Scan(JpegLsScan {
                header_length: marker_len,
                components: info_components,
                near,
                interleave_mode: ilv_byte,
                point_transform: pt,
                maxval: alpha - 1,
                thresholds: [t1, t2, t3],
                reset: scan.reset,
                restart_interval: u32::try_from(self.restart_interval).unwrap_or(u32::MAX),
                entropy_coded_length: data_end.saturating_sub(data_start),
            }),
        );

        if collect {
            self.scans.push((scan, data));
        }

        Ok(())
    }

    fn read_app(&mut self, index: u8, marker_start: u64) -> VexelResult<()> {
        let (length, payload) = self.read_segment_payload()?;
        let segment = parse_app(index, &payload, marker_start + 4, &mut self.app_metadata);
        if let AppSegment::HpTransform(hp) = &segment {
            self.color_transform = hp.transform;
        }
        self.record(marker_start, Some(2 + u64::from(length)), JpegLsSection::App(AppSection { index, segment }));
        Ok(())
    }

    fn read_compressed_data(&mut self, collect: bool) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        while let Ok(b) = self.reader.read_bits(8) {
            let byte = b as u8;

            if byte == 0xFF {
                let next = match self.reader.read_bits(8) {
                    Ok(b) => b as u8,
                    Err(_) => break,
                };

                if next & 0x80 == 0 || (0xD0..=0xD7).contains(&next) {
                    if collect {
                        data.push(0xFF);
                        data.push(next);
                    }
                } else {
                    let _ = self.reader.seek(SeekFrom::Current(-2));
                    break;
                }
            } else if collect {
                data.push(byte);
            }
        }
        Ok(data)
    }

    fn reset_state(&mut self) {
        self.frame = None;
        self.pending_scan = None;
        self.scans.clear();
        self.color_transform = 0;
        self.restart_interval = 0;
        self.oversize_dimensions = None;
        self.mapping_tables.clear();
        self.app_metadata.clear();
    }

    fn walk(&mut self, mode: WalkMode) -> VexelResult<()> {
        self.reset_state();
        self.walk_mode = mode;
        self.info = Some(JpegLsInfo::default());
        self.reader.seek(SeekFrom::Start(0))?;

        let result = self.walk_markers();
        self.finish_info();
        result
    }

    fn walk_markers(&mut self) -> VexelResult<()> {
        loop {
            let marker = match self.reader.next_marker(&JPEG_LS_MARKERS) {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(_) => break,
            };
            let marker_start = self.reader.stream_position().unwrap_or(2).saturating_sub(2);

            match marker {
                JpegLsMarker::SOI => self.record(marker_start, Some(2), JpegLsSection::StartOfImage),
                JpegLsMarker::SOF55 => self.read_sof(marker_start)?,
                JpegLsMarker::LSE => self.read_lse(marker_start)?,
                JpegLsMarker::SOS => self.read_sos(marker_start)?,
                JpegLsMarker::EOI => {
                    self.record(marker_start, Some(2), JpegLsSection::EndOfImage);
                    let remaining = self.reader.bytes_left().unwrap_or(0);
                    if remaining > 0 {
                        self.record(marker_start + 2, Some(remaining), JpegLsSection::TrailingData);
                    }
                    break;
                }
                JpegLsMarker::DRI => {
                    let len = self.read_u16()?;
                    let interval = self.read_sized_value(len, "DRI")?;
                    self.restart_interval = interval as usize;
                    self.record(marker_start, Some(2 + u64::from(len)), JpegLsSection::RestartInterval(interval));
                }
                JpegLsMarker::DNL => {
                    if let Err(e) = self.read_dnl(marker_start) {
                        log_warn!("Failed to read DNL segment: {}", e);
                    }
                }
                JpegLsMarker::APP0
                | JpegLsMarker::APP1
                | JpegLsMarker::APP2
                | JpegLsMarker::APP3
                | JpegLsMarker::APP4
                | JpegLsMarker::APP5
                | JpegLsMarker::APP6
                | JpegLsMarker::APP7
                | JpegLsMarker::APP8
                | JpegLsMarker::APP9
                | JpegLsMarker::APP10
                | JpegLsMarker::APP11
                | JpegLsMarker::APP12
                | JpegLsMarker::APP13
                | JpegLsMarker::APP14
                | JpegLsMarker::APP15 => self.read_app((marker.to_u16() & 0x0F) as u8, marker_start)?,
                JpegLsMarker::COM => {
                    let (length, payload) = self.read_segment_payload()?;
                    self.record(
                        marker_start,
                        Some(2 + u64::from(length)),
                        JpegLsSection::Comment(parse_comment(&payload)),
                    );
                }
                _ => {
                    log_warn!("Unhandled JPEG-LS marker: {:?}", marker);
                }
            }
        }

        Ok(())
    }

    fn finish_info(&mut self) {
        let summary = self.build_summary();
        if let Some(info) = &mut self.info {
            info.summary = summary;
        }
    }

    fn build_summary(&self) -> Option<ImageSummary> {
        let info = self.info.as_ref()?;
        let frame = info.sections.iter().find_map(|section| match &section.data {
            JpegLsSection::Frame(frame) => Some(frame),
            _ => None,
        })?;

        let width = self.frame.as_ref().map(|f| f.width).unwrap_or(u32::from(frame.width));
        let height = self.frame.as_ref().map(|f| f.height).unwrap_or(u32::from(frame.height));
        let components = frame.components.len();
        let color_model = match components {
            1 => ColorModel::Gray,
            3 => ColorModel::Rgb,
            4 => ColorModel::Rgba,
            _ => ColorModel::Multichannel,
        };

        let mut summary = ImageSummary::new(width, height, color_model, components as u16, frame.precision);
        summary.has_alpha = color_model == ColorModel::Rgba;

        let scans: Vec<&JpegLsScan> = info
            .sections
            .iter()
            .filter_map(|section| match &section.data {
                JpegLsSection::Scan(scan) => Some(scan),
                _ => None,
            })
            .collect();
        let near = scans.iter().map(|scan| scan.near).max().unwrap_or(0);
        let mut parts = vec![if near == 0 {
            "JPEG-LS lossless".to_string()
        } else {
            format!("JPEG-LS near-lossless (NEAR {})", near)
        }];
        if let Some(scan) = scans.first() {
            if components > 1 && scan.components.len() > 1 {
                let interleave = match scan.interleave_mode {
                    0 => "non-interleaved",
                    2 => "sample-interleaved",
                    _ => "line-interleaved",
                };
                parts.push(interleave.to_string());
            } else if components > 1 {
                parts.push("one scan per component".to_string());
            }
        }
        if self.color_transform != 0 {
            parts.push(format!("HP{} color transform", self.color_transform));
        }

        let table_width = |id: u8| {
            info.sections.iter().rev().find_map(|section| match &section.data {
                JpegLsSection::MappingTable(table) if table.table_id == id && !table.continuation => {
                    Some(table.entry_width)
                }
                _ => None,
            })
        };
        let mapped_widths: Vec<u8> = scans
            .iter()
            .flat_map(|scan| scan.components.iter())
            .filter(|component| component.mapping_table != 0)
            .filter_map(|component| table_width(component.mapping_table))
            .collect();
        if !mapped_widths.is_empty() {
            parts.push("mapping table".to_string());
            if components == 1 {
                match mapped_widths[0] {
                    3 => {
                        summary.color_model = ColorModel::Rgb;
                        summary.channels = 3;
                        summary.bits_per_sample = 8;
                    }
                    4 => {
                        summary.color_model = ColorModel::Rgba;
                        summary.channels = 4;
                        summary.bits_per_sample = 8;
                        summary.has_alpha = true;
                    }
                    2 => summary.bits_per_sample = 16,
                    1 => summary.bits_per_sample = 8,
                    _ => {}
                }
            } else if mapped_widths.iter().all(|&width| width <= 2) {
                if mapped_widths.contains(&2) {
                    summary.bits_per_sample = 16;
                } else if frame.precision <= 8 || mapped_widths.len() >= components {
                    summary.bits_per_sample = 8;
                }
            }
        }
        summary.compression = Some(parts.join(", "));

        let factors: Vec<(u8, u8)> =
            frame.components.iter().map(|c| (c.horizontal_sampling, c.vertical_sampling)).collect();
        summary.subsampling = subsampling_name(&factors);
        self.app_metadata.apply(&mut summary);
        Some(summary)
    }

    fn apply_mapping_tables(
        &self,
        samples: &[u16],
        channel_tables: &[(u8, i32)],
        precision: u8,
    ) -> Option<MappedSamples> {
        let channels = channel_tables.len();
        if channels == 0 || channel_tables.iter().all(|(id, _)| *id == 0) {
            return None;
        }

        let tables: Vec<Option<(&MappingTable, i32)>> = channel_tables
            .iter()
            .map(|&(id, alpha)| {
                if id == 0 {
                    return None;
                }
                match self.mapping_tables.get(&id) {
                    Some(table) if table.entry_count() > 0 => Some((table, (65535 / (alpha - 1).max(1)).max(1))),
                    Some(_) => {
                        log_warn!("JPEG-LS: mapping table {} has no usable entries, ignoring it", id);
                        None
                    }
                    None => {
                        log_warn!("JPEG-LS: mapping table {} is referenced but never defined, ignoring it", id);
                        None
                    }
                }
            })
            .collect();

        if tables.iter().all(Option::is_none) {
            return None;
        }

        let mut clamped = false;
        let mut lookup = |table: &MappingTable, scale: i32, sample: u16| -> usize {
            let index = (i32::from(sample) / scale) as usize;
            if index >= table.entry_count() {
                clamped = true;
            }
            index
        };

        let result = if channels == 1 {
            let (table, scale) = tables[0]?;
            match table.entry_width {
                1 => {
                    let data =
                        samples.iter().map(|&s| u16::from(table.entry(lookup(table, scale, s))[0]) * 257).collect();
                    Some(MappedSamples::Values { data, wide: false })
                }
                2 => {
                    let data = samples
                        .iter()
                        .map(|&s| {
                            let entry = table.entry(lookup(table, scale, s));
                            u16::from_be_bytes([entry[0], entry[1]])
                        })
                        .collect();
                    Some(MappedSamples::Values { data, wide: true })
                }
                width @ (3 | 4) => {
                    let mut data = Vec::with_capacity(samples.len() * width);
                    for &s in samples {
                        data.extend_from_slice(table.entry(lookup(table, scale, s)));
                    }
                    Some(MappedSamples::Palette { data, channels: width })
                }
                width => {
                    log_warn!(
                        "JPEG-LS: mapping table entries of {} bytes are not supported, ignoring the table",
                        width
                    );
                    None
                }
            }
        } else {
            let unsupported = tables.iter().flatten().find(|(table, _)| table.entry_width > 2);
            if let Some((table, _)) = unsupported {
                log_warn!(
                    "JPEG-LS: {}-byte mapping table entries in a {}-component image are not supported, ignoring the tables",
                    table.entry_width,
                    channels
                );
                return None;
            }
            let wide = tables.iter().flatten().any(|(table, _)| table.entry_width == 2)
                || (precision > 8 && tables.iter().any(Option::is_none));
            let data = samples
                .iter()
                .enumerate()
                .map(|(i, &s)| match tables[i % channels] {
                    Some((table, scale)) => {
                        let entry = table.entry(lookup(table, scale, s));
                        if table.entry_width == 2 {
                            u16::from_be_bytes([entry[0], entry[1]])
                        } else {
                            u16::from(entry[0]) * 257
                        }
                    }
                    None => s,
                })
                .collect();
            Some(MappedSamples::Values { data, wide })
        };

        if clamped {
            log_warn!("JPEG-LS: sample values exceed the mapping table size, using the last entry");
        }
        result
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        if let Err(e) = self.walk(WalkMode::Decode) {
            self.info = None;
            return Err(e);
        }

        let frame = match self.frame.as_ref() {
            Some(f) => f.clone(),
            None => return Err(VexelError::Custom("No SOF marker found".into())),
        };

        if self.scans.is_empty() {
            return Err(VexelError::Custom("No SOS marker found".into()));
        }

        if frame.width == 0 || frame.height == 0 {
            return Err(VexelError::InvalidDimensions { width: frame.width, height: frame.height });
        }

        self.limits.reserve_buffer(frame.width, frame.height, 4)?;
        self.limits.check_dimensions(frame.width, frame.height)?;

        let width = frame.width as usize;
        let height = frame.height as usize;
        let components = frame.component_count().max(1);

        let scans = std::mem::take(&mut self.scans);

        let scan_alpha = |scan: &ScanHeader| if scan.alpha > 1 { scan.alpha } else { frame.alpha };
        let channel_tables: Vec<(u8, i32)> = if scans.len() > 1 {
            scans
                .iter()
                .map(|(scan, _)| (scan.mapping_table_ids.first().copied().unwrap_or(0), scan_alpha(scan)))
                .collect()
        } else {
            let scan = &scans[0].0;
            (0..components).map(|i| (scan.mapping_table_ids.get(i).copied().unwrap_or(0), scan_alpha(scan))).collect()
        };

        let max_h = frame.components.iter().map(|c| c.horizontal_sampling as usize).max().unwrap_or(1);
        let max_v = frame.components.iter().map(|c| c.vertical_sampling as usize).max().unwrap_or(1);
        let all_unity = frame.components.iter().all(|c| c.horizontal_sampling <= 1 && c.vertical_sampling <= 1);

        let mut scans_alpha: Option<i32> = None;

        let pixel_data: Vec<u16> = if scans.len() > 1 {
            let mut planes: Vec<(Vec<u16>, usize, usize)> = Vec::with_capacity(scans.len());

            for (scan, data) in scans {
                let alpha = if scan.alpha > 1 { scan.alpha } else { frame.alpha };
                scans_alpha.get_or_insert(alpha);
                let near = scan.near;
                let lossy = near > 0;

                let mut bpp = 2i32;
                {
                    while (1 << bpp) < alpha {
                        bpp += 1;
                    }
                }
                if bpp < 2 {
                    bpp = 2;
                }

                let (qbpp, stats_alpha) = if lossy {
                    let quant = 2 * near + 1;
                    let qbeta = (alpha + 2 * near + quant - 1) / quant;
                    let mut qbpp = 2i32;
                    while (1 << qbpp) < qbeta {
                        qbpp += 1;
                    }
                    (qbpp, qbeta)
                } else {
                    (bpp, alpha)
                };

                let limit = if bpp < 8 { 2 * (bpp + 8) - qbpp - 1 } else { 4 * bpp - qbpp - 1 };

                let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };
                let (t1, t2, t3) = compute_thresholds(alpha, near, scan.t1, scan.t2, scan.t3);
                let reset = scan.reset;

                let (plane_w, plane_h) = if all_unity || scan.component_ids.is_empty() {
                    (width, height)
                } else {
                    let comp_id = scan.component_ids[0];
                    if let Some(comp) = frame.components.iter().find(|c| c.id == comp_id) {
                        let pw = width * comp.horizontal_sampling as usize / max_h.max(1);
                        let ph = height * comp.vertical_sampling as usize / max_v.max(1);
                        (pw.max(1), ph.max(1))
                    } else {
                        (width, height)
                    }
                };

                let mut state = DecoderState::new(1);
                prepare_luts(&mut state, t1, t2, t3, near, lutmax);
                if lossy {
                    let quant = 2 * near + 1;
                    let qbeta = (alpha + 2 * near + quant - 1) / quant;
                    let beta = quant * qbeta;
                    let qtable_bytes = ((2 * beta - 1) as usize).saturating_mul(4);
                    self.limits.reserve_usize(qtable_bytes)?;
                    prepare_qtables(&mut state, alpha, near);
                }
                init_stats(&mut state, stats_alpha);
                init_run_state(&mut state, 1);

                self.limits.reserve_usize(plane_w.saturating_mul(plane_h).saturating_mul(2))?;

                let plane = decode_single_plane(
                    &mut JlsBitReader::new(data),
                    &mut state,
                    plane_w,
                    plane_h,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                    scan.restart_interval,
                    stats_alpha,
                );
                planes.push((plane, plane_w, plane_h));
            }

            let num_planes = planes.len();
            self.limits.reserve_usize(width.saturating_mul(height).saturating_mul(num_planes).saturating_mul(2))?;
            let mut result = Vec::with_capacity(width * height * num_planes);

            if all_unity {
                for i in 0..(width * height) {
                    for (plane, _, _) in &planes {
                        result.push(plane[i]);
                    }
                }
            } else {
                let comp_samplings: Vec<(usize, usize)> = frame
                    .components
                    .iter()
                    .map(|c| (c.horizontal_sampling as usize, c.vertical_sampling as usize))
                    .collect();

                for row in 0..height {
                    for x in 0..width {
                        for p in 0..num_planes {
                            let (plane, pw, ph) = &planes[p];
                            let (h_samp, v_samp) = if p < comp_samplings.len() { comp_samplings[p] } else { (1, 1) };
                            let src_x = (x * h_samp / max_h).min(pw.saturating_sub(1));
                            let src_y = (row * v_samp / max_v).min(ph.saturating_sub(1));
                            result.push(plane[src_y * pw + src_x]);
                        }
                    }
                }
            }

            for (plane, pw, ph) in planes {
                self.limits.free_usize(pw.saturating_mul(ph).saturating_mul(2));
                drop(plane);
            }

            result
        } else {
            let (scan, data) = scans.into_iter().next().unwrap();
            let alpha = if scan.alpha > 1 { scan.alpha } else { frame.alpha };
            scans_alpha = Some(alpha);
            let near = scan.near;
            let lossy = near > 0;

            let mut bpp = 2i32;
            {
                while (1 << bpp) < alpha {
                    bpp += 1;
                }
            }
            if bpp < 2 {
                bpp = 2;
            }

            let (qbpp, stats_alpha) = if lossy {
                let quant = 2 * near + 1;
                let qbeta = (alpha + 2 * near + quant - 1) / quant;
                let mut qbpp = 2i32;
                while (1 << qbpp) < qbeta {
                    qbpp += 1;
                }
                (qbpp, qbeta)
            } else {
                (bpp, alpha)
            };

            let limit = if bpp < 8 { 2 * (bpp + 8) - qbpp - 1 } else { 4 * bpp - qbpp - 1 };

            let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };
            let (t1, t2, t3) = compute_thresholds(alpha, near, scan.t1, scan.t2, scan.t3);
            let reset = scan.reset;

            let mut state = DecoderState::new(components);
            prepare_luts(&mut state, t1, t2, t3, near, lutmax);
            if lossy {
                let quant = 2 * near + 1;
                let qbeta = (alpha + 2 * near + quant - 1) / quant;
                let beta = quant * qbeta;
                let qtable_bytes = ((2 * beta - 1) as usize).saturating_mul(4);
                self.limits.reserve_usize(qtable_bytes)?;
                prepare_qtables(&mut state, alpha, near);
            }
            init_stats(&mut state, stats_alpha);
            init_run_state(&mut state, components);

            let restart_interval = scan.restart_interval;
            let mut br = JlsBitReader::new(data);

            self.limits.reserve_usize(width.saturating_mul(height).saturating_mul(components).saturating_mul(2))?;

            match scan.interleave_mode {
                InterleaveMode::None => decode_plane_interleaved(
                    &mut br,
                    &mut state,
                    width,
                    height,
                    components,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                    restart_interval,
                    stats_alpha,
                ),
                InterleaveMode::Line => {
                    let sampling: Vec<(u8, u8)> =
                        frame.components.iter().map(|c| (c.horizontal_sampling, c.vertical_sampling)).collect();
                    decode_line_interleaved(
                        &mut br,
                        &mut state,
                        width,
                        height,
                        components,
                        alpha,
                        near,
                        lossy,
                        limit,
                        qbpp,
                        reset,
                        &sampling,
                        restart_interval,
                        stats_alpha,
                    )
                }
                InterleaveMode::Sample => decode_sample_interleaved(
                    &mut br,
                    &mut state,
                    width,
                    height,
                    components,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                    restart_interval,
                    stats_alpha,
                ),
            }
        };

        let (pixel_data, wide_output) = match self.apply_mapping_tables(&pixel_data, &channel_tables, frame.precision) {
            Some(MappedSamples::Palette { data, channels }) => {
                let pixels = if channels == 4 { PixelData::RGBA8(data) } else { PixelData::RGB8(data) };
                return Ok(Image::from_pixels(frame.width, frame.height, pixels));
            }
            Some(MappedSamples::Values { data, wide }) => {
                if self.color_transform != 0 {
                    log_warn!("JPEG-LS: mapping tables are in use, ignoring the HP color transform");
                }
                (data, wide)
            }
            None => {
                let scan_alpha = scans_alpha.unwrap_or(frame.alpha);
                let data = if self.color_transform != 0 && components == 3 {
                    apply_hp_color_transform_inverse(&pixel_data, self.color_transform, scan_alpha)
                } else {
                    pixel_data
                };
                (data, frame.precision > 8)
            }
        };

        let image = if wide_output {
            if components == 1 {
                Image::from_pixels(frame.width, frame.height, PixelData::L16(pixel_data))
            } else if components == 4 {
                Image::from_pixels(frame.width, frame.height, PixelData::RGBA16(pixel_data))
            } else {
                Image::from_pixels(frame.width, frame.height, PixelData::RGB16(pixel_data))
            }
        } else {
            let pixel_data_u8: Vec<u8> = pixel_data.into_iter().map(|v| (v >> 8) as u8).collect();
            if components == 1 {
                Image::from_pixels(frame.width, frame.height, PixelData::L8(pixel_data_u8))
            } else if components == 4 {
                Image::from_pixels(frame.width, frame.height, PixelData::RGBA8(pixel_data_u8))
            } else {
                Image::from_pixels(frame.width, frame.height, PixelData::RGB8(pixel_data_u8))
            }
        };

        Ok(image)
    }
}

fn apply_hp_color_transform_inverse(data: &[u16], transform: u8, alpha: i32) -> Vec<u16> {
    let scale = 65535 / (alpha - 1);
    let bias = alpha / 2;

    let mut result = Vec::with_capacity(data.len());

    for chunk in data.chunks_exact(3) {
        let v1 = (chunk[0] as i32) / scale;
        let v2 = (chunk[1] as i32) / scale;
        let v3 = (chunk[2] as i32) / scale;

        let (r, g, b) = match transform {
            1 => {
                let g = v2;
                let r = (v1 + g - bias).rem_euclid(alpha);
                let b = (v3 + g - bias).rem_euclid(alpha);
                (r, g, b)
            }
            2 => {
                let g = v2;
                let r = (v1 + g - bias).rem_euclid(alpha);
                let b = (v3 + ((r + g) >> 1) - bias).rem_euclid(alpha);
                (r, g, b)
            }
            3 => {
                let g = (v1 - ((v3 + v2) >> 2) + alpha / 4).rem_euclid(alpha);
                let r = (v3 + g - bias).rem_euclid(alpha);
                let b = (v2 + g - bias).rem_euclid(alpha);
                (r, g, b)
            }
            _ => (v1, v2, v3),
        };

        result.push((r * scale) as u16);
        result.push((g * scale) as u16);
        result.push((b * scale) as u16);
    }

    result
}

fn decode_single_plane(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let buf_size = width + 3;
    let mut prev_line = vec![0i32; buf_size];
    let mut curr_line = vec![0i32; buf_size];
    let mut result = Vec::with_capacity(width * height);

    let max_val = alpha - 1;
    let scale = 65535 / max_val;
    let mut rows_in_interval = 0usize;

    for _row in 0..height {
        if restart_interval > 0 && rows_in_interval == restart_interval {
            br.consume_restart_marker();
            prev_line.iter_mut().for_each(|v| *v = 0);
            curr_line.iter_mut().for_each(|v| *v = 0);
            init_stats(state, stats_alpha);
            init_run_state(state, 1);
            rows_in_interval = 0;
        }

        let first_prev_pixel = prev_line[2];
        curr_line[0] = first_prev_pixel;
        curr_line[1] = first_prev_pixel;

        undoscanline(br, state, &prev_line, &mut curr_line, width, 0, alpha, near, lossy, limit, qbpp, reset);

        curr_line[width + 2] = curr_line[width + 1];

        for &v in curr_line.iter().take(width + 2).skip(2) {
            result.push((v.clamp(0, max_val) * scale) as u16);
        }

        std::mem::swap(&mut prev_line, &mut curr_line);
        rows_in_interval += 1;
    }

    result
}

fn decode_plane_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let mut planes: Vec<Vec<u16>> = (0..components).map(|_| Vec::with_capacity(width * height)).collect();

    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    for (comp, plane) in planes.iter_mut().enumerate() {
        init_stats(state, stats_alpha);
        init_run_state(state, 1);

        let buf_size = width + 3;
        let mut prev_line = vec![0i32; buf_size];
        let mut curr_line = vec![0i32; buf_size];
        let mut rows_in_interval = 0usize;

        for _row in 0..height {
            if restart_interval > 0 && rows_in_interval == restart_interval {
                br.consume_restart_marker();
                prev_line.iter_mut().for_each(|v| *v = 0);
                curr_line.iter_mut().for_each(|v| *v = 0);
                init_stats(state, stats_alpha);
                init_run_state(state, 1);
                rows_in_interval = 0;
            }

            let first_prev_pixel = prev_line[2];
            curr_line[0] = first_prev_pixel;
            curr_line[1] = first_prev_pixel;

            undoscanline(br, state, &prev_line, &mut curr_line, width, comp, alpha, near, lossy, limit, qbpp, reset);

            curr_line[width + 2] = curr_line[width + 1];

            for &v in curr_line.iter().take(width + 2).skip(2) {
                plane.push((v.clamp(0, max_val) * scale) as u16);
            }

            std::mem::swap(&mut prev_line, &mut curr_line);
            rows_in_interval += 1;
        }
    }

    let mut result = Vec::with_capacity(width * height * components);
    for i in 0..(width * height) {
        for plane in planes.iter() {
            result.push(plane[i]);
        }
    }

    result
}

fn decode_line_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    sampling: &[(u8, u8)],
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    let all_unity = sampling.iter().all(|&(h, v)| h <= 1 && v <= 1);

    if all_unity || sampling.len() != components {
        let buf_size = width + 3;
        let mut prev_lines: Vec<Vec<i32>> = (0..components).map(|_| vec![0i32; buf_size]).collect();
        let mut curr_lines: Vec<Vec<i32>> = (0..components).map(|_| vec![0i32; buf_size]).collect();
        let mut result = Vec::with_capacity(width * height * components);
        let mut rows_in_interval = 0usize;

        for _row in 0..height {
            if restart_interval > 0 && rows_in_interval == restart_interval {
                br.consume_restart_marker();
                for comp in 0..components {
                    prev_lines[comp].iter_mut().for_each(|v| *v = 0);
                    curr_lines[comp].iter_mut().for_each(|v| *v = 0);
                }
                init_stats(state, stats_alpha);
                init_run_state(state, components);
                rows_in_interval = 0;
            }

            for comp in 0..components {
                let first_prev_pixel = prev_lines[comp][2];
                curr_lines[comp][0] = first_prev_pixel;
                curr_lines[comp][1] = first_prev_pixel;

                let prev_clone = prev_lines[comp].clone();
                undoscanline(
                    br,
                    state,
                    &prev_clone,
                    &mut curr_lines[comp],
                    width,
                    comp,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                );

                curr_lines[comp][width + 2] = curr_lines[comp][width + 1];
            }

            for x in 2..=width + 1 {
                for curr_line in curr_lines.iter() {
                    result.push((curr_line[x].clamp(0, max_val) * scale) as u16);
                }
            }

            for comp in 0..components {
                std::mem::swap(&mut prev_lines[comp], &mut curr_lines[comp]);
            }

            rows_in_interval += 1;
        }

        return result;
    }

    let max_h = sampling.iter().map(|&(h, _)| h as usize).max().unwrap_or(1);
    let max_v = sampling.iter().map(|&(_, v)| v as usize).max().unwrap_or(1);

    let c_widths: Vec<usize> = sampling.iter().map(|&(h, _)| (width * h as usize / max_h).max(1)).collect();
    let c_heights: Vec<usize> = sampling.iter().map(|&(_, v)| (height * v as usize / max_v).max(1)).collect();

    let mut comp_data: Vec<Vec<i32>> =
        (0..components).map(|c| Vec::with_capacity(c_widths[c] * c_heights[c])).collect();

    let mut prev_lines: Vec<Vec<i32>> = (0..components).map(|c| vec![0i32; c_widths[c] + 3]).collect();
    let mut curr_lines: Vec<Vec<i32>> = (0..components).map(|c| vec![0i32; c_widths[c] + 3]).collect();

    let num_groups = height.div_ceil(max_v.max(1));
    let mut groups_in_interval = 0usize;
    let interval_in_groups = if restart_interval > 0 {
        let v_samp_max = sampling.iter().map(|&(_, v)| v as usize).max().unwrap_or(1);
        restart_interval / v_samp_max
    } else {
        0
    };

    for _ in 0..num_groups {
        if interval_in_groups > 0 && groups_in_interval == interval_in_groups {
            br.consume_restart_marker();
            for comp in 0..components {
                prev_lines[comp].iter_mut().for_each(|v| *v = 0);
                curr_lines[comp].iter_mut().for_each(|v| *v = 0);
            }
            init_stats(state, stats_alpha);
            init_run_state(state, components);
            groups_in_interval = 0;
        }

        for comp in 0..components {
            let v_samp = sampling[comp].1 as usize;
            let cw = c_widths[comp];

            for _ in 0..v_samp {
                let first_prev_pixel = prev_lines[comp][2];
                curr_lines[comp][0] = first_prev_pixel;
                curr_lines[comp][1] = first_prev_pixel;

                let prev_clone = prev_lines[comp].clone();
                undoscanline(
                    br,
                    state,
                    &prev_clone,
                    &mut curr_lines[comp],
                    cw,
                    comp,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                );

                curr_lines[comp][cw + 2] = curr_lines[comp][cw + 1];

                let room = (cw * c_heights[comp]).saturating_sub(comp_data[comp].len());
                comp_data[comp].extend(curr_lines[comp].iter().take(cw + 2).skip(2).take(room));

                std::mem::swap(&mut prev_lines[comp], &mut curr_lines[comp]);
            }
        }

        groups_in_interval += 1;
    }

    for (comp, data) in comp_data.iter_mut().enumerate() {
        data.resize(c_widths[comp] * c_heights[comp], 0);
    }

    let mut result = Vec::with_capacity(width * height * components);

    for row in 0..height {
        for x in 0..width {
            for comp in 0..components {
                let h_samp = sampling[comp].0 as usize;
                let v_samp = sampling[comp].1 as usize;
                let cw = c_widths[comp];

                let src_x = (x * h_samp / max_h).min(cw.saturating_sub(1));
                let src_y = (row * v_samp / max_v).min(c_heights[comp].saturating_sub(1));

                let val = comp_data[comp][src_y * cw + src_x];
                result.push((val.clamp(0, max_val) * scale) as u16);
            }
        }
    }

    result
}

fn decode_sample_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let total_samples = width * components;
    let buf_size = total_samples + components * 3;
    let mut result = Vec::with_capacity(width * height * components);

    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    let mut prev_line = vec![0i32; buf_size];
    let mut curr_line = vec![0i32; buf_size];
    let mut rows_in_interval = 0usize;

    for _row in 0..height {
        if restart_interval > 0 && rows_in_interval == restart_interval {
            br.consume_restart_marker();
            prev_line.iter_mut().for_each(|v| *v = 0);
            curr_line.iter_mut().for_each(|v| *v = 0);
            init_stats(state, stats_alpha);
            init_run_state(state, components);
            rows_in_interval = 0;
        }

        for nc in 0..components {
            let first_prev = prev_line[2 * components + nc];
            curr_line[nc] = first_prev;
            curr_line[components + nc] = first_prev;
        }

        let prev_clone = prev_line.clone();
        undoscanline_pixel(
            br,
            state,
            &prev_clone,
            &mut curr_line,
            total_samples,
            components,
            alpha,
            near,
            lossy,
            limit,
            qbpp,
            reset,
        );

        for nc in 0..components {
            curr_line[total_samples + 2 * components + nc] = curr_line[total_samples + components + nc];
        }

        for i in 0..total_samples {
            result.push((curr_line[2 * components + i].clamp(0, max_val) * scale) as u16);
        }

        std::mem::swap(&mut prev_line, &mut curr_line);
        rows_in_interval += 1;
    }

    result
}

fn predict(ra: i32, rb: i32, rc: i32) -> i32 {
    let minx = ra.min(rb);
    let maxx = ra.max(rb);
    if rc >= maxx {
        minx
    } else if rc <= minx {
        maxx
    } else {
        ra + rb - rc
    }
}

fn lossless_regular_mode(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    q: usize,
    sign: i32,
    px: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let nt = state.n[q];
    let at = state.a[q];

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at && k < MAX_K {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let abs_errval = {
        let unary = br.count_leading_zeros();
        if unary < limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let errval = if abs_errval & 1 != 0 { -(abs_errval + 1) / 2 } else { abs_errval / 2 };

    let bt = state.b[q];

    let errval = if k == 0 && 2 * bt <= -nt { -(errval + 1) } else { errval };

    let abs_errval = errval.abs();

    let current = if sign == -1 {
        let px_adj = clip(px - state.c[q], alpha);
        (px_adj - errval) & (alpha - 1)
    } else {
        let px_adj = clip(px + state.c[q], alpha);
        (px_adj + errval) & (alpha - 1)
    };

    state.b[q] += errval;
    state.a[q] = state.a[q].saturating_add(abs_errval);

    if nt == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    let bt = state.b[q];
    if bt <= -state.n[q] {
        if state.c[q] > MIN_C {
            state.c[q] -= 1;
        }
        state.b[q] += state.n[q];
        if state.b[q] <= -state.n[q] {
            state.b[q] = -state.n[q] + 1;
        }
    } else if bt > 0 {
        if state.c[q] < MAX_C {
            state.c[q] += 1;
        }
        state.b[q] -= state.n[q];
        if state.b[q] > 0 {
            state.b[q] = 0;
        }
    }

    current
}

fn lossless_end_of_run(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    ra: i32,
    rb: i32,
    ri_type: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let q = EOR_0 + ri_type as usize;
    let nt = state.n[q];
    let at = if ri_type != 0 { state.a[q].saturating_add(nt / 2) } else { state.a[q] };

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at && k < MAX_K {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let eor_limit = limit - state.limit_reduce;

    let merrval = {
        let unary = br.count_leading_zeros();
        if unary < eor_limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let oldmap = if k == 0 && (ri_type != 0 || merrval != 0) && 2 * state.b[q] < nt { 1i32 } else { 0i32 };

    let merrval = merrval + ri_type + oldmap;

    let (errval, abs_errval) = if merrval & 1 != 0 {
        let e = oldmap - (merrval + 1) / 2;
        let ae = -e - ri_type;
        state.b[q] += 1;
        (e, ae)
    } else {
        let e = merrval / 2;
        let ae = e - ri_type;
        (e, ae)
    };

    let ix = if rb < ra { (rb - errval) & (alpha - 1) } else { (rb + errval) & (alpha - 1) };

    state.a[q] = state.a[q].saturating_add(abs_errval);

    if state.n[q] == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    ix
}

fn process_run_dec(br: &mut JlsBitReader, state: &mut DecoderState, line_left: i32, color: usize) -> i32 {
    let mut runlen = 0i32;

    loop {
        let top_byte = (!(br.reg >> 24)) as u8;
        let temp = br.zero_lut[top_byte as usize] as i32;

        for hits in 1..=temp {
            runlen += state.melc_order[color];
            if runlen >= line_left {
                if runlen == line_left && state.melc_state[color] < MELC_STATES - 1 {
                    state.melc_state[color] += 1;
                    state.melc_len[color] = J_TABLE[state.melc_state[color]];
                    state.melc_order[color] = 1 << state.melc_len[color];
                }
                br.fill_buffer(hits);
                return line_left;
            }
            if state.melc_state[color] < MELC_STATES - 1 {
                state.melc_state[color] += 1;
                state.melc_len[color] = J_TABLE[state.melc_state[color]];
                state.melc_order[color] = 1 << state.melc_len[color];
            }
        }

        if temp != 8 {
            br.fill_buffer(temp + 1);
            break;
        }
        br.fill_buffer(8);
    }

    if state.melc_len[color] > 0 {
        let extra = br.get_bits(state.melc_len[color]) as i32;
        runlen += extra;
    }
    state.limit_reduce = state.melc_len[color] + 1;

    if state.melc_state[color] > 0 {
        state.melc_state[color] -= 1;
        state.melc_len[color] = J_TABLE[state.melc_state[color]];
        state.melc_order[color] = 1 << state.melc_len[color];
    }

    runlen
}

fn clip(x: i32, alpha: i32) -> i32 {
    if x < 0 {
        0
    } else if x >= alpha {
        alpha - 1
    } else {
        x
    }
}

fn lossy_regular_mode(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    q: usize,
    sign: i32,
    px: i32,
    alpha: i32,
    near: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let nt = state.n[q];
    let at = state.a[q];

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at && k < MAX_K {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let abs_errval = {
        let unary = br.count_leading_zeros();
        if unary < limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let errval = if abs_errval & 1 != 0 { -(abs_errval + 1) / 2 } else { abs_errval / 2 };

    let bt = state.b[q];
    let errval = if k == 0 && near == 0 && 2 * bt <= -nt { -(errval + 1) } else { errval };

    let abs_errval = errval.abs();
    let errval = state.qmul(errval);

    let current = if sign == -1 {
        let px_adj = clip(px - state.c[q], alpha);
        px_adj - errval
    } else {
        let px_adj = clip(px + state.c[q], alpha);
        px_adj + errval
    };

    let current = if current < state.neg_near {
        current + state.beta
    } else if current > state.alpha1eps {
        current - state.beta
    } else {
        current
    };
    let current = clip(current, alpha);

    state.b[q] += errval;
    state.a[q] = state.a[q].saturating_add(abs_errval);

    if nt == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    let bt = state.b[q];
    if bt <= -state.n[q] {
        if state.c[q] > MIN_C {
            state.c[q] -= 1;
        }
        state.b[q] += state.n[q];
        if state.b[q] <= -state.n[q] {
            state.b[q] = -state.n[q] + 1;
        }
    } else if bt > 0 {
        if state.c[q] < MAX_C {
            state.c[q] += 1;
        }
        state.b[q] -= state.n[q];
        if state.b[q] > 0 {
            state.b[q] = 0;
        }
    }

    current
}

fn lossy_end_of_run(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    ra: i32,
    rb: i32,
    ri_type: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let q = EOR_0 + ri_type as usize;
    let nt = state.n[q];
    let at = if ri_type != 0 { state.a[q].saturating_add(nt / 2) } else { state.a[q] };

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at && k < MAX_K {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let eor_limit = limit - state.limit_reduce;

    let merrval = {
        let unary = br.count_leading_zeros();
        if unary < eor_limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let oldmap = if k == 0 && (ri_type != 0 || merrval != 0) && 2 * state.b[q] < nt { 1i32 } else { 0i32 };

    let merrval = merrval + ri_type + oldmap;

    let (errval, abs_errval) = if merrval & 1 != 0 {
        let e = oldmap - (merrval + 1) / 2;
        let ae = -e - ri_type;
        state.b[q] += 1;
        (e, ae)
    } else {
        let e = merrval / 2;
        let ae = e - ri_type;
        (e, ae)
    };

    let errval = state.qmul(errval);

    let ix = if ri_type != 0 {
        ra + errval
    } else if rb < ra {
        rb - errval
    } else {
        rb + errval
    };

    let ix = if ix < state.neg_near {
        ix + state.beta
    } else if ix > state.alpha1eps {
        ix - state.beta
    } else {
        ix
    };
    let ix = clip(ix, alpha);

    state.a[q] = state.a[q].saturating_add(abs_errval);

    if state.n[q] == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    ix
}

fn undoscanline(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    psl: &[i32],
    sl: &mut [i32],
    no: usize,
    color: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
) {
    let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };

    let mut rc = psl[1];
    let mut rb = psl[2];
    let mut ra = sl[1];

    let mut i = 2usize;
    while i <= no + 1 {
        let rd = if i < no + 1 { psl[i + 1] } else { psl[no + 1] };

        let drd = (rd - rb).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);
        let drb = (rb - rc).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);
        let drc = (rc - ra).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);

        let cont = state.vlut[0][(drd + lutmax as i32) as usize]
            + state.vlut[1][(drb + lutmax as i32) as usize]
            + state.vlut[2][(drc + lutmax as i32) as usize];

        if cont == 0 {
            let line_left = (no + 1) as i32 - i as i32 + 1;
            let m = process_run_dec(br, state, line_left, color);
            let n = m;

            if n > 0 {
                for j in 0..n as usize {
                    let idx = i + j;
                    if idx <= no + 1 {
                        sl[idx] = ra;
                    }
                }
                i += n as usize;

                if i > no + 1 {
                    return;
                }

                rb = if i <= no + 1 { psl[i] } else { psl[no + 1] };
            }

            if lossy {
                let ri_type = if (rb - ra).abs() <= near { 1 } else { 0 };
                ra = lossy_end_of_run(br, state, ra, rb, ri_type, alpha, limit, qbpp, reset);
            } else {
                ra = lossless_end_of_run(br, state, ra, rb, (ra == rb) as i32, alpha, limit, qbpp, reset);
            }
        } else {
            let px = predict(ra, rb, rc);

            let cont_mapped = state.classmap[cont as usize];
            let (sign, q) =
                if cont_mapped < 0 { (-1i32, (-cont_mapped) as usize) } else { (1i32, cont_mapped as usize) };

            if lossy {
                ra = lossy_regular_mode(br, state, q, sign, px, alpha, near, limit, qbpp, reset);
            } else {
                ra = lossless_regular_mode(br, state, q, sign, px, alpha, limit, qbpp, reset);
            }
        }

        sl[i] = ra;
        rc = rb;
        rb = if i < no + 1 { psl[i + 1] } else { psl[no + 1] };
        i += 1;
    }
}

fn undoscanline_pixel(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    psl: &[i32],
    sl: &mut [i32],
    no: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
) {
    let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };

    let comps = components;
    let mut c_aa = vec![0i32; comps];
    let mut c_bb = vec![0i32; comps];
    let mut c_cc = vec![0i32; comps];
    let mut c_dd = vec![0i32; comps];
    let mut c_cont = vec![0i32; comps];

    for nc in 0..comps {
        c_cc[nc] = psl[comps + nc];
        c_bb[nc] = psl[2 * comps + nc];
        c_aa[nc] = sl[comps + nc];
    }

    let mut i = 2 * comps;
    let mut color = comps - 1;
    let mut was_in_run = false;
    let total = no + comps;

    while i < total + comps {
        if !was_in_run {
            color = (color + 1) % comps;
        } else {
            color = 0;
        }

        if color == 0 {
            for nc in 0..comps {
                c_dd[nc] = psl[i + comps + nc];

                let d0 = c_dd[nc] - c_bb[nc];
                let d1 = c_bb[nc] - c_cc[nc];
                let d2 = c_cc[nc] - c_aa[nc];

                let bound = (lutmax - 1) as i32;
                let d0c = d0.clamp(-bound, bound);
                let d1c = d1.clamp(-bound, bound);
                let d2c = d2.clamp(-bound, bound);

                c_cont[nc] = state.vlut[0][(d0c + lutmax as i32) as usize]
                    + state.vlut[1][(d1c + lutmax as i32) as usize]
                    + state.vlut[2][(d2c + lutmax as i32) as usize];
            }
        }

        let ra = c_aa[color];
        let rb = c_bb[color];
        let rc = c_cc[color];
        let cont = c_cont[color];

        was_in_run = false;

        let test_run = color == 0 && c_cont.iter().all(|&v| v == 0);

        if test_run {
            was_in_run = true;

            let samples_left = ((total + comps - i) / comps) as i32;
            let m = process_run_dec(br, state, samples_left, 0);
            let n = m;

            if n > 0 {
                for j in 0..n as usize {
                    #[allow(clippy::needless_range_loop)]
                    for nc in 0..comps {
                        let idx = i + j * comps + nc;
                        if idx < sl.len() {
                            sl[idx] = c_aa[nc];
                        }
                    }
                }
                i += n as usize * comps;

                if i >= total + comps {
                    return;
                }

                for nc in 0..comps {
                    c_bb[nc] = psl[i + nc];
                    c_dd[nc] = psl[i + comps + nc];
                }
            }

            for nc in 0..comps {
                let ra_nc = c_aa[nc];
                let rb_nc = c_bb[nc];
                c_aa[nc] = if lossy {
                    lossy_end_of_run(br, state, ra_nc, rb_nc, 0, alpha, limit, qbpp, reset)
                } else {
                    lossless_end_of_run(br, state, ra_nc, rb_nc, 0, alpha, limit, qbpp, reset)
                };
            }
        } else {
            let px = predict(ra, rb, rc);
            let cont_mapped = state.classmap[cont as usize];
            let (sign, q) =
                if cont_mapped < 0 { (-1i32, (-cont_mapped) as usize) } else { (1i32, cont_mapped as usize) };

            c_aa[color] = if lossy {
                lossy_regular_mode(br, state, q, sign, px, alpha, near, limit, qbpp, reset)
            } else {
                lossless_regular_mode(br, state, q, sign, px, alpha, limit, qbpp, reset)
            };
        }

        if !was_in_run {
            sl[i] = c_aa[color];
            c_cc[color] = rb;
            c_bb[color] = c_dd[color];
            i += 1;
        } else {
            sl[i..i + comps].copy_from_slice(&c_aa);
            c_cc.copy_from_slice(&c_bb);
            c_bb.copy_from_slice(&c_dd);
            i += comps;
        }
    }
}
