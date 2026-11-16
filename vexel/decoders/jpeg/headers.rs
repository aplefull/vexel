use std::io::{Read, Seek, SeekFrom};

use super::decoder::{JpegDecoder, WalkMode, MAX_SCANS, MAX_SCAN_COMPONENTS, SCAN_BLOCK_SIZE};
use crate::decoders::jpeg::app::{parse_app, parse_comment, AppSection, AppSegment};
use crate::decoders::jpeg::info::{
    JpegArithmeticConditioning, JpegExpand, JpegFrame, JpegFrameComponent, JpegHuffmanTable, JpegHuffmanTables,
    JpegQuantizationTable, JpegQuantizationTables, JpegScan, JpegScanComponent, JpegSection,
};
use crate::decoders::jpeg::markers::JpegMarker;
use crate::decoders::jpeg::types::{
    ArithmeticCodingTable, ArithmeticCodingValue, ColorComponentInfo, HuffmanTable, JpegCodingMethod, JpegMode,
    QuantizationTable, ScanComponent, ScanData, ZIGZAG_MAP,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn};

pub(super) struct RawHuffmanTable {
    pub(super) id: u8,
    pub(super) class: u8,
    pub(super) counts: Vec<u8>,
    pub(super) offsets: Vec<u32>,
    pub(super) symbols: Vec<u8>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    pub(super) fn read_segment_payload(&mut self) -> VexelResult<(u16, Vec<u8>)> {
        let length = self.reader.read_u16()?;
        let payload_len = usize::from(length.saturating_sub(2));
        let mut payload = vec![0u8; payload_len];
        let mut filled = 0;
        while filled < payload_len {
            match self.reader.read(&mut payload[filled..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        payload.truncate(filled);
        Ok((length, payload))
    }

    pub(super) fn skip_unknown_marker_segment(&mut self, marker: u16, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::Unknown(marker));

        for _ in 0..length.saturating_sub(2) {
            self.reader.read_u8()?;
        }

        Ok(())
    }

    pub(super) fn read_app(&mut self, index: u8, segment_start: u64) -> VexelResult<()> {
        let (length, payload) = self.read_segment_payload()?;
        let segment = parse_app(index, &payload, segment_start + 4, &mut self.app_metadata);

        match &segment {
            AppSegment::Jfif(_) => self.has_jfif = true,
            AppSegment::Adobe(adobe) => {
                self.adobe_color_transform = Some(adobe.color_transform);
                log_debug!("Adobe APP14: color_transform={}", adobe.color_transform);
            }
            AppSegment::Generic(_) if index == 0 => {
                log_warn!("APP0 segment is neither JFIF nor JFXX");
            }
            _ => {}
        }

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::App(AppSection { index, segment }));

        Ok(())
    }

    pub(super) fn read_com(&mut self, segment_start: u64) -> VexelResult<()> {
        let (length, payload) = self.read_segment_payload()?;
        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::Comment(parse_comment(&payload)));
        Ok(())
    }

    pub(super) fn read_start_of_frame(&mut self, marker: u8, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let raw_precision = self.reader.read_u8()?;
        self.precision = raw_precision;

        if self.mode == JpegMode::Lossless {
            if self.precision < 2 || self.precision > 16 {
                log_warn!("Invalid precision for lossless jpeg mode: {}, clamping", self.precision);
                self.precision = self.precision.clamp(2, 16);
            }
        } else if self.precision < 8 || self.precision > 16 {
            log_warn!("Invalid precision: {}, clamping to 8", self.precision);
            self.precision = 8;
        }

        let raw_height = self.reader.read_u16()?;
        let raw_width = self.reader.read_u16()?;
        self.height = u32::from(raw_height);
        self.width = u32::from(raw_width);

        let raw_component_count = self.reader.read_u8()?;
        let mut component_count = raw_component_count;

        if component_count > 4 || component_count == 0 {
            log_warn!("Invalid number of components in SOF marker: {}, assuming 3", component_count);
            component_count = 3;
        }

        self.components.clear();
        let mut frame_components = Vec::with_capacity(usize::from(component_count));

        for _ in 0..component_count {
            let id = match self.reader.read_u8() {
                Ok(v) => v,
                Err(_) => break,
            };
            let sampling_factors = self.reader.read_u8().unwrap_or(0x11);
            let horizontal_sampling_factor = ((sampling_factors >> 4) & 0xF).max(1);
            let vertical_sampling_factor = (sampling_factors & 0xF).max(1);
            let quantization_table_id = self.reader.read_u8().unwrap_or(0);

            frame_components.push(JpegFrameComponent {
                id,
                horizontal_sampling: sampling_factors >> 4,
                vertical_sampling: sampling_factors & 0xF,
                quantization_table: quantization_table_id,
            });

            self.components.push(ColorComponentInfo {
                id,
                horizontal_sampling_factor,
                vertical_sampling_factor,
                quantization_table_id,
                dc_table_selector: 0,
                ac_table_selector: 0,
            });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::Frame(JpegFrame {
                marker,
                precision: raw_precision,
                height: raw_height,
                width: raw_width,
                component_count: raw_component_count,
                components: frame_components,
            }),
        );

        if length != 8 + 3 * component_count as u16 {
            log_warn!("Invalid SOF marker length, expected {}, got {}", 8 + 3 * component_count, length);
        }

        if self.width == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if self.height == 0 {
            log_warn!("SOF height is 0, expecting DNL marker to define number of lines");
        }

        if self.walk_mode == WalkMode::Decode {
            self.limits.reserve_buffer(self.width, self.height, 4)?;
            self.limits.free(u64::from(self.width).saturating_mul(u64::from(self.height)).saturating_mul(4));
        }

        Ok(())
    }

    pub(super) fn read_restart_interval(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.restart_interval = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::RestartInterval(self.restart_interval));

        Ok(())
    }

    pub(super) fn read_dnl(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        if length != 4 {
            log_warn!("Invalid DNL segment length: {}, expected 4", length);
        }

        let number_of_lines = self.reader.read_u16()?;

        self.record(segment_start, Some(2 + u64::from(length)), JpegSection::NumberOfLines(number_of_lines));

        if number_of_lines == 0 {
            log_warn!("DNL specifies 0 lines, ignoring");
        } else {
            if self.walk_mode == WalkMode::Decode {
                self.limits.reserve_buffer(self.width, u32::from(number_of_lines), 4)?;
                self.limits.free(u64::from(self.width).saturating_mul(u64::from(number_of_lines)).saturating_mul(4));
            }
            self.height = number_of_lines as u32;
            log_debug!("DNL: updated height to {}", self.height);
        }

        Ok(())
    }

    pub(super) fn read_quantization_entry(&mut self) -> std::io::Result<(u8, u8, Vec<u16>)> {
        let table_spec = self.reader.read_u8()?;
        let id = table_spec & 0x0F;
        let precision = (table_spec >> 4) & 0x0F;

        let mut table = Vec::with_capacity(64);
        if precision == 0 {
            for _ in 0..64 {
                table.push(self.reader.read_u8()? as u16);
            }
        } else {
            for _ in 0..64 {
                table.push(self.reader.read_u16()?);
            }
        }

        Ok((id, precision, table))
    }

    pub(super) fn read_quantization_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut table_length = (segment_length as i16).saturating_sub(2);

        let mut info_tables = Vec::new();
        let mut failure = None;

        while table_length > 0 {
            let (id, precision, table) = match self.read_quantization_entry() {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };

            table_length = table_length.saturating_sub(1);
            table_length = table_length.saturating_sub(if precision == 0 { 64 } else { 128 });

            let table_natural = Self::unzigzag_block(&table).to_vec();
            info_tables.push(JpegQuantizationTable { id, precision, values: table_natural.clone() });
            self.quantization_tables.push(QuantizationTable { id, table_natural });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(segment_length)),
            JpegSection::QuantizationTables(JpegQuantizationTables {
                tables: info_tables,
                truncated: failure.is_some(),
            }),
        );

        match failure {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    pub(super) fn unzigzag_block(block: &[u16]) -> [u16; 64] {
        let mut unzigzagged = [0u16; 64];

        for i in 0..64 {
            unzigzagged[ZIGZAG_MAP[i] as usize] = block.get(i).copied().unwrap_or(0);
        }

        unzigzagged
    }

    pub(super) fn read_huffman_entry(&mut self) -> std::io::Result<RawHuffmanTable> {
        let table_spec = self.reader.read_u8()?;
        let id = table_spec & 0x0F;
        let class = table_spec >> 4;

        let mut counts = vec![0u8; 16];
        self.reader.read_exact(&mut counts)?;

        let mut offsets = Vec::with_capacity(17);
        let mut total_symbols: u32 = 0;
        offsets.push(0);
        for &count in &counts {
            total_symbols += count as u32;
            offsets.push(total_symbols);
        }

        if total_symbols > 256 {
            log_warn!("Too many symbols in Huffman table: {}", total_symbols);
            total_symbols = 256;
            for offset in offsets.iter_mut() {
                *offset = (*offset).min(total_symbols);
            }
        }

        let mut symbols = vec![0u8; total_symbols as usize];
        self.reader.read_exact(&mut symbols)?;

        Ok(RawHuffmanTable { id, class, counts, offsets, symbols })
    }

    pub(super) fn read_huffman_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let total_length = self.reader.read_u16()?;
        let mut segment_length = (total_length as i16).saturating_sub(2);

        let mut info_tables = Vec::new();
        let mut failure = None;

        while segment_length > 0 {
            let RawHuffmanTable { id, class, counts, offsets, symbols } = match self.read_huffman_entry() {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };

            let total_symbols = symbols.len() as u32;
            segment_length -= 1 + 16 + total_symbols as i16;

            let mut huffman_table = HuffmanTable {
                id,
                symbols: symbols.clone(),
                offsets,
                codes: vec![0; total_symbols as usize],
                first_code: vec![u32::MAX; 16],
                fast_lookup: Vec::new(),
            };

            let mut code = 0;
            for i in 0..16 {
                if huffman_table.offsets.len() <= i + 1 {
                    log_warn!("Offset index {} is out of bounds in Huffman table", i);
                    break;
                }

                for k in huffman_table.offsets[i]..huffman_table.offsets[i + 1] {
                    if huffman_table.codes.len() <= k as usize {
                        log_warn!("Code index {} is out of bounds in Huffman table", k);
                        break;
                    }

                    huffman_table.codes[k as usize] = code;
                    code += 1;
                }

                code <<= 1;
            }

            for i in 0..16 {
                if huffman_table.offsets.len() > i + 1 {
                    let start = huffman_table.offsets[i] as usize;
                    let end = huffman_table.offsets[i + 1] as usize;
                    if start < end && start < huffman_table.codes.len() {
                        huffman_table.first_code[i] = huffman_table.codes[start];
                    }
                }
            }

            huffman_table.build_fast_lookup();
            info_tables.push(JpegHuffmanTable { class, id, counts, symbols });

            match class {
                0 => {
                    if let Some(existing_table) = self.dc_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.dc_huffman_tables.push(huffman_table);
                    }
                }
                1 => {
                    if let Some(existing_table) = self.ac_huffman_tables.iter_mut().find(|t| t.id == id) {
                        *existing_table = huffman_table;
                    } else {
                        self.ac_huffman_tables.push(huffman_table);
                    }
                }
                _ => {
                    log_warn!("Invalid Huffman table class: {}, ignoring the table", class);
                }
            }
        }

        self.record(
            segment_start,
            Some(2 + u64::from(total_length)),
            JpegSection::HuffmanTables(JpegHuffmanTables { tables: info_tables, truncated: failure.is_some() }),
        );

        match failure {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    pub(super) fn read_dac(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut data_length = segment_length.saturating_sub(2);

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();
        let mut info_tables = Vec::new();
        let mut failure = None;

        while data_length >= 2 {
            let entry = self.reader.read_u8().and_then(|info| Ok((info, self.reader.read_u8()?)));
            let (table_info, raw_value) = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            };
            let table_class = (table_info >> 4) & 0x0F;
            let identifier = table_info & 0x0F;

            info_tables.push(JpegArithmeticConditioning { class: table_class, id: identifier, value: raw_value });

            let (value, length) =
                if table_class == 0 { ((raw_value >> 4) & 0x0F, raw_value & 0x0F) } else { (raw_value, 0) };

            let ac_value = ArithmeticCodingValue { value, length };

            let table = ArithmeticCodingTable { identifier, values: Vec::from([ac_value]) };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => {
                    log_warn!("Invalid arithmetic coding table class: {}, ignoring the table", table_class);
                }
            }

            data_length -= 2;
        }

        self.record(
            segment_start,
            Some(2 + u64::from(segment_length)),
            JpegSection::ArithmeticConditioning(info_tables),
        );

        if let Some(e) = failure {
            return Err(e.into());
        }

        self.ac_arithmetic_tables = ac_tables;
        self.dc_arithmetic_tables = dc_tables;

        Ok(())
    }

    pub(super) fn read_start_of_scan(&mut self, segment_start: u64) -> VexelResult<()> {
        if self.coding_method == JpegCodingMethod::Huffman {
            self.ensure_default_huffman_tables();
        }

        let collect = self.walk_mode == WalkMode::Decode && self.collected_scans < MAX_SCANS;
        if self.walk_mode == WalkMode::Decode && !collect {
            log_warn!("More than {} scans in the image, ignoring the rest", MAX_SCANS);
        }
        if self.width > 0 && !self.components.is_empty() {
            self.frame_has_scan = true;
        }

        let length = self.reader.read_u16()?;

        let scan_component_count = self.reader.read_u8()?;

        let mut scan_components = Vec::new();
        for _ in 0..scan_component_count {
            let component_selector = self.reader.read_u8()?;
            let table_selectors = self.reader.read_u8()?;

            scan_components.push(ScanComponent {
                component_id: component_selector,
                dc_table_selector: (table_selectors >> 4) & 0x0F,
                ac_table_selector: table_selectors & 0x0F,
            });

            if let Some(color_component) = self.components.iter_mut().find(|c| c.id == component_selector) {
                color_component.dc_table_selector = (table_selectors >> 4) & 0x0F;
                color_component.ac_table_selector = table_selectors & 0x0F;
            }
        }

        let raw_start_spectral = self.reader.read_u8()?;
        let raw_end_spectral = self.reader.read_u8()?;
        let start_spectral = raw_start_spectral.min(63);
        let end_spectral = raw_end_spectral.min(63);
        let successive_approx = self.reader.read_u8()?;
        let successive_high = (successive_approx >> 4) & 0x0F;
        let successive_low = successive_approx & 0x0F;

        if length != 6 + (2 * scan_component_count as u16) {
            log_warn!("Invalid SOS marker length, expected {}, got {}", 6 + (2 * scan_component_count as u16), length);
        }

        let scan_start = self.reader.stream_position()?;
        let defer = collect && self.defer_entropy;
        let collect = collect && !defer;
        let scan_data_limit = if collect {
            (self.width as usize).saturating_mul(self.height as usize).saturating_mul(4).max(65536)
        } else {
            usize::MAX
        };
        if collect {
            self.limits.reserve_usize(scan_data_limit)?;
        }
        let remaining = if collect { self.reader.bytes_left().unwrap_or(0) as usize } else { 0 };
        let base = self.entropy.len();
        if collect {
            self.entropy.reserve(remaining.min(scan_data_limit));
        }
        let scan_data = &mut self.entropy;
        let is_arithmetic = self.coding_method == JpegCodingMethod::Arithmetic;

        let mut restart_offsets: Vec<usize> = vec![0];
        let mut restart_markers = 0u32;
        let mut block = vec![0u8; if defer { 0 } else { SCAN_BLOCK_SIZE }];
        let mut filled = 0usize;
        let mut cursor = 0usize;
        let mut block_origin = 0u64;
        let mut truncated = false;
        let mut end_marker: Option<u64> = None;

        loop {
            if defer {
                break;
            }
            if filled - cursor < 2 {
                block.copy_within(cursor..filled, 0);
                block_origin += cursor as u64;
                filled -= cursor;
                cursor = 0;

                match self.reader.read(&mut block[filled..]) {
                    Ok(0) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                    Ok(n) => filled += n,
                    Err(_) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                }
                continue;
            }

            let chunk = &block[cursor..filled];
            if collect {
                let space = scan_data_limit.saturating_sub(scan_data.len() - base);
                let consumed = destuff_entropy(&chunk[..chunk.len().min(space)], scan_data, is_arithmetic);
                if consumed > 0 {
                    cursor += consumed;
                    continue;
                }
            } else {
                let run = find_ff(chunk);
                if run > 0 {
                    cursor += run;
                    continue;
                }
            }

            if collect && scan_data.len() - base >= scan_data_limit {
                truncated = true;
                break;
            }

            match block[cursor + 1] {
                0x00 => {
                    if collect {
                        scan_data.push(0xFF);
                        if is_arithmetic {
                            scan_data.push(0x00);
                        }
                    }
                    cursor += 2;
                }
                0xFF => {
                    cursor += 1;
                }
                b if b >= (JpegMarker::RST0.to_u16() & 0xFF) as u8 && b <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 => {
                    restart_markers += 1;
                    if collect {
                        scan_data.push(0xFF);
                        scan_data.push(b);
                        restart_offsets.push(scan_data.len() - base);
                    }
                    cursor += 2;
                }
                b if matches!(JpegMarker::from_u16(0xFF00 | b as u16), None | Some(JpegMarker::RES(_))) => {
                    log_warn!("Skipping invalid marker 0xFF{:02X} inside scan data", b);
                    cursor += 2;
                }
                _ => {
                    end_marker = Some(block_origin + cursor as u64);
                    break;
                }
            }
        }

        if truncated {
            log_warn!("Scan data exceeded limit of {} bytes, truncating", scan_data_limit);
        }

        let stop = end_marker.unwrap_or(block_origin + cursor as u64);
        if defer {
            self.deferred_scan_start = Some(scan_start);
        } else {
            self.reader.seek(SeekFrom::Start(scan_start + stop))?;
        }

        let lossless = matches!(self.mode, JpegMode::Lossless | JpegMode::DifferentialLossless);
        let header_span = scan_start.saturating_sub(segment_start);
        self.record(
            segment_start,
            Some(header_span + stop),
            JpegSection::Scan(JpegScan {
                header_length: length,
                components: scan_components
                    .iter()
                    .map(|c| JpegScanComponent {
                        id: c.component_id,
                        dc_table: c.dc_table_selector,
                        ac_table: c.ac_table_selector,
                    })
                    .collect(),
                spectral_start: raw_start_spectral,
                spectral_end: raw_end_spectral,
                approximation_high: successive_high,
                approximation_low: successive_low,
                lossless,
                restart_interval: self.restart_interval,
                entropy_coded_length: stop,
                restart_markers,
            }),
        );

        if !collect && !defer {
            return Ok(());
        }

        if collect {
            let actual_scan_size = self.entropy.len() - base;
            self.limits.free_usize(scan_data_limit.saturating_sub(actual_scan_size));
        }

        if scan_components.len() > MAX_SCAN_COMPONENTS {
            log_warn!("Scan lists {} components, using the first {}", scan_components.len(), MAX_SCAN_COMPONENTS);
            scan_components.truncate(MAX_SCAN_COMPONENTS);
        }

        let dc_tables = self
            .dc_huffman_tables
            .iter()
            .filter(|t| scan_components.iter().any(|c| c.dc_table_selector == t.id))
            .cloned()
            .collect();
        let ac_tables = self
            .ac_huffman_tables
            .iter()
            .filter(|t| scan_components.iter().any(|c| c.ac_table_selector == t.id))
            .cloned()
            .collect();

        let scan = ScanData {
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            components: scan_components,
            dc_tables,
            ac_tables,
            arith_dc_tables: self.dc_arithmetic_tables.clone(),
            arith_ac_tables: self.ac_arithmetic_tables.clone(),
            data: base..self.entropy.len(),
            restart_offsets,
            restart_interval: self.restart_interval,
        };

        self.scans.push(scan);
        self.collected_scans += 1;

        Ok(())
    }

    pub(super) fn read_dhp(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let precision = self.reader.read_u8()?;
        let height = self.reader.read_u16()?;
        let width = self.reader.read_u16()?;
        let component_count = self.reader.read_u8()?;

        let mut components = Vec::with_capacity(usize::from(component_count));
        for _ in 0..component_count {
            let id = self.reader.read_u8()?;
            let sampling = self.reader.read_u8()?;
            let quantization_table = self.reader.read_u8()?;
            components.push(JpegFrameComponent {
                id,
                horizontal_sampling: sampling >> 4,
                vertical_sampling: sampling & 0x0F,
                quantization_table,
            });
        }

        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::HierarchicalProgression(JpegFrame {
                marker: 0xDE,
                precision,
                height,
                width,
                component_count,
                components,
            }),
        );

        self.is_hierarchical = true;
        self.dhp_width = u32::from(width);
        self.dhp_height = u32::from(height);

        if self.walk_mode == WalkMode::Decode {
            self.limits.reserve_buffer(self.dhp_width, self.dhp_height, 4)?;
        }

        Ok(())
    }

    pub(super) fn read_exp(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;
        let expand = self.reader.read_u8()?;
        let expand_horizontal = (expand >> 4) & 1 != 0;
        let expand_vertical = expand & 1 != 0;
        self.pending_expand_h = expand_horizontal;
        self.pending_expand_v = expand_vertical;
        self.record(
            segment_start,
            Some(2 + u64::from(length)),
            JpegSection::Expand(JpegExpand { horizontal: expand_horizontal, vertical: expand_vertical }),
        );
        Ok(())
    }
}

fn find_ff(data: &[u8]) -> usize {
    let mut blocks = data.chunks_exact(32);
    let mut offset = 0;
    for block in &mut blocks {
        if block.iter().fold(0u8, |acc, &b| acc | u8::from(b == 0xFF)) != 0 {
            break;
        }
        offset += 32;
    }
    offset + data[offset..].iter().position(|&b| b == 0xFF).unwrap_or(data.len() - offset)
}

fn destuff_entropy(input: &[u8], out: &mut Vec<u8>, keep_zero: bool) -> usize {
    const LOW: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;

    let n = input.len();
    out.reserve(n);
    let start = out.len();
    let dst = out.spare_capacity_mut().as_mut_ptr() as *mut u8;
    let mut i = 0;
    let mut w = 0;
    loop {
        while i + 32 <= n {
            let words: [u64; 4] =
                std::array::from_fn(|k| u64::from_le_bytes(input[i + 8 * k..i + 8 * k + 8].try_into().unwrap()));
            let any_ff = words.iter().fold(0u64, |acc, &word| acc | ((!word).wrapping_sub(LOW) & word & HIGH));
            if any_ff != 0 {
                break;
            }
            for (k, &word) in words.iter().enumerate() {
                unsafe { (dst.add(w + 8 * k) as *mut [u8; 8]).write_unaligned(word.to_le_bytes()) };
            }
            i += 32;
            w += 32;
        }
        while i + 8 <= n {
            let word = u64::from_le_bytes(input[i..i + 8].try_into().unwrap());
            unsafe { (dst.add(w) as *mut [u8; 8]).write_unaligned(word.to_le_bytes()) };
            let inverted = !word;
            let ff = inverted.wrapping_sub(LOW) & !inverted & HIGH;
            if ff == 0 {
                i += 8;
                w += 8;
                continue;
            }
            let skip = (ff.trailing_zeros() / 8) as usize;
            i += skip;
            w += skip;
            break;
        }
        while i < n && input[i] != 0xFF {
            unsafe { dst.add(w).write(input[i]) };
            i += 1;
            w += 1;
        }
        if i + 1 >= n {
            break;
        }
        match input[i + 1] {
            0x00 => {
                unsafe { dst.add(w).write(0xFF) };
                w += 1;
                if keep_zero {
                    unsafe { dst.add(w).write(0x00) };
                    w += 1;
                }
                i += 2;
            }
            0xFF => i += 1,
            _ => break,
        }
    }
    unsafe { out.set_len(start + w) };
    i
}

pub(super) fn destuff_scan_in_place(data: &mut [u8]) -> (usize, Vec<usize>) {
    const LOW: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;

    let n = data.len();
    let ptr = data.as_mut_ptr();
    let mut restart_offsets = vec![0];
    let mut i = 0;
    let mut w = 0;
    loop {
        while i + 32 <= n {
            let words: [u64; 4] =
                std::array::from_fn(|k| unsafe { (ptr.add(i + 8 * k) as *const u64).read_unaligned() });
            let any_ff = words.iter().fold(0u64, |acc, &word| acc | ((!word).wrapping_sub(LOW) & word & HIGH));
            if any_ff != 0 {
                break;
            }
            if i != w {
                for (k, &word) in words.iter().enumerate() {
                    unsafe { (ptr.add(w + 8 * k) as *mut u64).write_unaligned(word) };
                }
            }
            i += 32;
            w += 32;
        }
        while i + 8 <= n {
            let word = unsafe { (ptr.add(i) as *const u64).read_unaligned() };
            let inverted = !word;
            let ff = inverted.wrapping_sub(LOW) & !inverted & HIGH;
            let skip = if ff == 0 { 8 } else { (ff.trailing_zeros() / 8) as usize };
            if i - w >= 8 {
                unsafe { (ptr.add(w) as *mut u64).write_unaligned(word) };
            } else if i != w {
                for k in 0..skip {
                    data[w + k] = data[i + k];
                }
            }
            i += skip;
            w += skip;
            if ff != 0 {
                break;
            }
        }
        while i < n && data[i] != 0xFF {
            data[w] = data[i];
            i += 1;
            w += 1;
        }
        if i + 1 >= n {
            break;
        }
        match data[i + 1] {
            0x00 => {
                data[w] = 0xFF;
                w += 1;
                i += 2;
            }
            0xFF => i += 1,
            b if (0xD0..=0xD7).contains(&b) => {
                data[w] = 0xFF;
                data[w + 1] = b;
                w += 2;
                i += 2;
                restart_offsets.push(w);
            }
            b if matches!(JpegMarker::from_u16(0xFF00 | b as u16), None | Some(JpegMarker::RES(_))) => {
                log_warn!("Skipping invalid marker 0xFF{:02X} inside scan data", b);
                i += 2;
            }
            _ => break,
        }
    }
    (w, restart_offsets)
}
