use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::JpegInfo;
use crate::utils::marker::Marker;
use crate::{log_debug, log_warn, Image, ImageFrame, PixelData, PixelFormat};
use std::f32::consts::PI;
use std::fmt::Debug;
use std::io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom};
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::decoders::jpeg::types::{ArithmeticCodingTable, ArithmeticCodingValue, ColorComponentInfo, DACData, DHTData, DQTData, ExifHeader, HuffmanTable, JFIFData, JFIFHeader, JpegCodingMethod, JpegMode, JpegSegmentData, JpegSegmentInfo, Predictor, QuantizationTable, SOFData, SOSData, ScanComponent, ScanData, DEFAULT_QUANTIZATION_TABLE, ZIGZAG_MAP};

#[derive(Debug, Clone)]
struct UpsampledPlane {
    data: Vec<i32>,
    width: u32,
    height: u32,
}

impl UpsampledPlane {
    fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![0; (width * height) as usize],
            width,
            height,
        }
    }

    fn get_pixel(&self, x: u32, y: u32) -> Option<i32> {
        if x < self.width && y < self.height {
            Some(self.data[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, value: i32) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = value;
        }
    }
}

#[derive(Debug, Clone)]
struct ComponentPlane {
    data: Vec<i32>,
    width: u32,
    height: u32,
    blocks_per_line: u32,
}

impl ComponentPlane {
    fn new(width: u32, height: u32) -> Self {
        let blocks_per_line = (width + 7) / 8;
        let block_lines = (height + 7) / 8;

        Self {
            width,
            height,
            blocks_per_line,
            data: vec![0; (blocks_per_line * block_lines * 64) as usize],
        }
    }

    fn get_block_mut(&mut self, block_x: u32, block_y: u32) -> Option<&mut [i32]> {
        let block_idx = block_y * self.blocks_per_line + block_x;
        let start = (block_idx * 64) as usize;
        if start + 64 <= self.data.len() {
            Some(&mut self.data[start..start + 64])
        } else {
            None
        }
    }

    fn upsample(&self, target_width: u32, target_height: u32) -> UpsampledPlane {
        let mut upsampled = UpsampledPlane::new(target_width, target_height);

        let mut source_pixels = vec![0i32; (self.width * self.height) as usize];

        let blocks_per_line = (self.width + 7) / 8;
        for by in 0..((self.height + 7) / 8) {
            for bx in 0..blocks_per_line {
                let block_idx = (by * blocks_per_line + bx) as usize * 64;

                for py in 0..8 {
                    let y = by * 8 + py;
                    if y >= self.height {
                        continue;
                    }

                    for px in 0..8 {
                        let x = bx * 8 + px;
                        if x >= self.width {
                            continue;
                        }

                        let pixel_idx = (y * self.width + x) as usize;
                        let block_pixel_idx = block_idx + (py * 8 + px) as usize;

                        if block_pixel_idx < self.data.len() {
                            source_pixels[pixel_idx] = self.data[block_pixel_idx];
                        }
                    }
                }
            }
        }

        for y in 0..target_height {
            for x in 0..target_width {
                let src_x = (x * self.width / target_width) as usize;
                let src_y = (y * self.height / target_height) as usize;

                let src_idx = src_y * self.width as usize + src_x;
                if src_idx < source_pixels.len() {
                    upsampled.set_pixel(x, y, source_pixels[src_idx]);
                }
            }
        }

        upsampled
    }
}

pub struct JpegDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    jfif_header: Option<JFIFHeader>,
    exif_header: Option<ExifHeader>,
    comments: Vec<String>,
    mode: JpegMode,
    coding_method: JpegCodingMethod,
    quantization_tables: Vec<QuantizationTable>,
    ac_huffman_tables: Vec<HuffmanTable>,
    dc_huffman_tables: Vec<HuffmanTable>,
    ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    start_of_spectral_selection: u8,
    end_of_spectral_selection: u8,
    successive_approximation_high: u8,
    successive_approximation_low: u8,
    horizontal_sampling_factor: u8,
    vertical_sampling_factor: u8,
    restart_interval: u16,
    mcu_width: u32,
    mcu_height: u32,
    precision: u8,
    component_count: u8,
    components: Vec<ColorComponentInfo>,
    scans: Vec<ScanData>,
    segments: Vec<JpegSegmentInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> JpegDecoder<R> {
    // TODO remove redundant fields, that are duplicated in scans
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            comments: Vec::new(),
            jfif_header: None,
            exif_header: None,
            mode: JpegMode::Baseline,
            coding_method: JpegCodingMethod::Huffman,
            mcu_width: 0,
            mcu_height: 0,
            precision: 0,
            component_count: 0,
            start_of_spectral_selection: 0,
            end_of_spectral_selection: 0,
            successive_approximation_high: 0,
            successive_approximation_low: 0,
            components: Vec::new(),
            quantization_tables: Vec::new(),
            ac_huffman_tables: Vec::new(),
            dc_huffman_tables: Vec::new(),
            ac_arithmetic_tables: Vec::new(),
            dc_arithmetic_tables: Vec::new(),
            horizontal_sampling_factor: 1,
            vertical_sampling_factor: 1,
            restart_interval: 0,
            scans: Vec::new(),
            segments: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> JpegInfo {
        JpegInfo {
            segments: self.segments.clone(),
        }
    }

    fn skip_unknown_marker_segment(&mut self, marker: &str, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        for _ in 0..(length - 2) {
            self.reader.read_u8()?;
        }

        self.record_segment(segment_start, marker, JpegSegmentData::Unknown {
            marker: marker.to_string(),
            length,
        });

        Ok(())
    }

    fn read_com(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let mut comment_bytes = Vec::new();
        for _ in 0..length - 2 {
            comment_bytes.push(self.reader.read_u8()?);
        }

        let text = String::from_utf8_lossy(&comment_bytes).to_string();
        self.comments.push(text.clone());

        self.record_segment(segment_start, "COM", JpegSegmentData::COM { text });

        Ok(())
    }

    fn read_app0_jfif(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        let mut identifier = Vec::new();
        for _ in 0..5 {
            identifier.push(self.reader.read_u8()?);
        }

        let identifier = String::from_utf8_lossy(&identifier).to_string();

        if identifier != "JFIF\0" {
            log_warn!(
                "Invalid JFIF identifier in APP0, might not be a JFIF header: {}",
                identifier
            );
        }

        let version_major = self.reader.read_bits(8)? as u8;
        let version_minor = self.reader.read_bits(8)? as u8;

        let density_units = self.reader.read_bits(8)? as u8;
        let x_density = self.reader.read_bits(16)? as u16;
        let y_density = self.reader.read_bits(16)? as u16;

        let thumbnail_width = self.reader.read_bits(8)? as u8;
        let thumbnail_height = self.reader.read_bits(8)? as u8;

        let thumbnail_size = thumbnail_width * thumbnail_height * 3;
        let mut thumbnail_data = Vec::new();

        if thumbnail_size > 0 {
            for _ in 0..thumbnail_size {
                thumbnail_data.push(self.reader.read_bits(8)? as u8);
            }
        }

        self.jfif_header = Some(JFIFHeader {
            identifier: identifier.clone(),
            version_major,
            version_minor,
            density_units,
            x_density,
            y_density,
            thumbnail_width,
            thumbnail_height,
            thumbnail_data: thumbnail_data.clone(),
        });

        if length != 16 + thumbnail_size as u16 {
            log_warn!(
                "Invalid JFIF segment length, expected {}, got {}",
                16 + thumbnail_size,
                length
            );
        }

        self.record_segment(segment_start, "APP0", JpegSegmentData::APP0(JFIFData {
            length,
            identifier,
            version_major,
            version_minor,
            density_units,
            x_density,
            y_density,
            thumbnail_width,
            thumbnail_height,
            thumbnail_data,
        }));

        Ok(())
    }

    fn read_app1_exif(&mut self, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        for _ in 0..(length - 2) {
            self.reader.read_u8()?;
        }

        // TODO actually implement this
        self.record_segment(segment_start, "APP1", JpegSegmentData::APP1 { length });

        Ok(())
    }

    fn read_start_of_frame(&mut self, sof_marker: &str, segment_start: u64) -> VexelResult<()> {
        let length = self.reader.read_u16()?;

        self.precision = self.reader.read_u8()?;

        if self.mode == JpegMode::Lossless {
            if self.precision < 2 || self.precision > 16 {
                log_warn!("Invalid precision for lossless jpeg mode: {}, clamping", self.precision);
                self.precision = self.precision.clamp(2, 16);
            }
        }

        self.height = self.reader.read_u16()? as u32;
        self.width = self.reader.read_u16()? as u32;

        if self.height == 0 || self.width == 0 {
            return Err(VexelError::from(Error::new(
                ErrorKind::InvalidData,
                "Invalid image dimensions",
            )));
        }

        // TODO rename them, they are not MCU dimensions, but dimensions of the image in MCUs
        self.mcu_width = (self.width + 7) / 8;
        self.mcu_height = (self.height + 7) / 8;

        self.component_count = self.reader.read_u8()?;

        if self.component_count > 4 || self.component_count == 0 {
            log_warn!(
                "Invalid number of components in SOF marker: {}, assuming 3",
                self.component_count
            );
            self.component_count = 3;
        }

        self.components.clear();

        for _ in 0..self.component_count {
            let id = self.reader.read_u8()?;
            let sampling_factors = self.reader.read_u8()?;
            let horizontal_sampling_factor = (sampling_factors >> 4) & 0xF;
            let vertical_sampling_factor = sampling_factors & 0xF;
            let quantization_table_id = self.reader.read_u8()?;

            if id == 1 {
                self.horizontal_sampling_factor = horizontal_sampling_factor;
                self.vertical_sampling_factor = vertical_sampling_factor;
            }

            self.components.push(ColorComponentInfo {
                id,
                horizontal_sampling_factor,
                vertical_sampling_factor,
                quantization_table_id,
                dc_table_selector: 0,
                ac_table_selector: 0,
            });
        }

        if length != 8 + 3 * self.component_count as u16 {
            log_warn!(
                "Invalid SOF marker length, expected {}, got {}",
                8 + 3 * self.component_count,
                length
            );
        }

        self.record_segment(segment_start, sof_marker, JpegSegmentData::SOF(SOFData {
            length,
            marker: sof_marker.to_string(),
            precision: self.precision,
            width: self.width,
            height: self.height,
            component_count: self.component_count,
            components: self.components.clone(),
        }));

        Ok(())
    }

    fn read_restart_interval(&mut self, segment_start: u64) -> VexelResult<()> {
        self.reader.read_u16()?;

        self.restart_interval = self.reader.read_u16()?;

        self.record_segment(segment_start, "DRI", JpegSegmentData::DRI { restart_interval: self.restart_interval });

        Ok(())
    }

    fn read_quantization_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut table_length = (segment_length as i16).saturating_sub(2);

        let mut new_tables = Vec::new();

        while table_length > 0 {
            let mut table = Vec::new();
            let table_spec = self.reader.read_u8()?;
            let id = table_spec & 0x0F;
            let precision = (table_spec >> 4) & 0x0F;

            table_length = table_length.saturating_sub(1);

            if precision == 0 {
                for _ in 0..64 {
                    table.push(self.reader.read_u8()? as u16);
                }
                table_length = table_length.saturating_sub(64);
            } else {
                for _ in 0..64 {
                    table.push(self.reader.read_u16()?);
                }
                table_length = table_length.saturating_sub(128);
            }

            let qt = QuantizationTable {
                id,
                precision,
                length: 0,
                table: Self::unzigzag_block(&table.as_slice()).to_vec(),
            };

            new_tables.push(qt.clone());
            self.quantization_tables.push(qt);
        }

        self.record_segment(segment_start, "DQT", JpegSegmentData::DQT(DQTData {
            length: segment_length,
            tables: new_tables,
        }));

        Ok(())
    }

    fn unzigzag_block(block: &[u16]) -> [u16; 64] {
        let mut unzigzagged = [0u16; 64];

        for i in 0..64 {
            unzigzagged[ZIGZAG_MAP[i] as usize] = block.get(i).copied().unwrap_or(0);
        }

        unzigzagged
    }

    fn read_huffman_table(&mut self, segment_start: u64) -> VexelResult<()> {
        let total_length = self.reader.read_bits(16)? as u16;
        let mut segment_length = (total_length as i16).saturating_sub(2);

        let mut new_tables = Vec::new();

        while segment_length > 0 {
            let table_spec = self.reader.read_bits(8)?;
            let id = (table_spec & 0x0F) as u8;
            let class = ((table_spec >> 4) & 0x0F) as u8;

            let mut offsets = Vec::with_capacity(17);
            let mut total_symbols: u32 = 0;

            offsets.push(0);
            for _ in 1..17 {
                let offset = self.reader.read_bits(8)?;
                total_symbols += offset;
                offsets.push(total_symbols);
            }

            if total_symbols > 162 {
                log_warn!("Too many symbols in Huffman table: {}", total_symbols);
                total_symbols = 162;
            }

            let mut table = Vec::with_capacity(total_symbols as usize);
            for _ in 0..total_symbols {
                table.push(self.reader.read_bits(8)? as u8);
            }

            segment_length -= 2 + 1 + 1 + 16 + total_symbols as i16;

            let mut huffman_table = HuffmanTable {
                id,
                class,
                offsets,
                symbols: table,
                codes: vec![0; 162],
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

            new_tables.push(huffman_table.clone());

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

        self.record_segment(segment_start, "DHT", JpegSegmentData::DHT(DHTData {
            length: total_length,
            tables: new_tables,
        }));

        Ok(())
    }

    fn read_dac(&mut self, segment_start: u64) -> VexelResult<()> {
        let segment_length = self.reader.read_u16()?;
        let mut data_length = segment_length - 2;

        let mut ac_tables = Vec::new();
        let mut dc_tables = Vec::new();

        while data_length > 0 {
            let table_info = self.reader.read_u8()?;
            let table_class = (table_info >> 4) & 0x0F;
            let identifier = table_info & 0x0F;

            let value = self.reader.read_u8()?;

            let (value, length) = if table_class == 0 {
                ((value >> 4) & 0x0F, value & 0x0F)
            } else {
                (value, 0)
            };

            let ac_value = ArithmeticCodingValue { value, length };

            let table = ArithmeticCodingTable {
                table_class,
                identifier,
                values: Vec::from([ac_value]),
            };

            match table_class {
                0 => dc_tables.push(table),
                1 => ac_tables.push(table),
                _ => {
                    log_warn!(
                        "Invalid arithmetic coding table class: {}, ignoring the table",
                        table_class
                    );
                }
            }

            data_length -= 2;
        }

        self.ac_arithmetic_tables = ac_tables.clone();
        self.dc_arithmetic_tables = dc_tables.clone();

        self.record_segment(segment_start, "DAC", JpegSegmentData::DAC(DACData {
            length: segment_length,
            ac_tables,
            dc_tables,
        }));

        Ok(())
    }

    fn read_start_of_scan(&mut self, segment_start: u64) -> VexelResult<()> {
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

        let start_spectral = self.reader.read_u8()?;
        let end_spectral = self.reader.read_u8()?;
        let successive_approx = self.reader.read_u8()?;
        let successive_high = (successive_approx >> 4) & 0x0F;
        let successive_low = successive_approx & 0x0F;

        if length != 6 + (2 * scan_component_count as u16) {
            log_warn!(
                "Invalid SOS marker length, expected {}, got {}",
                6 + (2 * scan_component_count as u16),
                length
            );
        }

        let mut current_byte = self.reader.read_u8().unwrap_or_else(|_| {
            log_warn!("Unexpected EOF while reading first byte of scan data");
            0
        });

        let mut scan_data = Vec::new();

        loop {
            if current_byte != 0xFF {
                // Most common case - regular data byte
                scan_data.push(current_byte);
                current_byte = match self.reader.read_u8() {
                    Ok(byte) => byte,
                    Err(_) => {
                        log_warn!("Unexpected EOF while reading scan data, breaking");
                        break;
                    }
                };

                continue;
            }

            // We have 0xFF byte, read the next one
            let next_byte = match self.reader.read_u8() {
                Ok(byte) => byte,
                Err(_) => {
                    log_warn!("Unexpected EOF while reading scan data, breaking");
                    break;
                }
            };

            match next_byte {
                0x00 => {
                    // Stuffed byte case
                    scan_data.push(current_byte);
                    current_byte = match self.reader.read_u8() {
                        Ok(byte) => byte,
                        Err(_) => {
                            log_warn!("Unexpected EOF while reading scan data, breaking");
                            break;
                        }
                    };
                }
                0xFF => {
                    // Another FF, reprocess it
                    current_byte = next_byte;
                }
                b if b >= (JpegMarker::RST0.to_u16() & 0xFF) as u8
                    && b <= (JpegMarker::RST7.to_u16() & 0xFF) as u8 =>
                    {
                        // Restart marker
                        current_byte = match self.reader.read_u8() {
                            Ok(byte) => byte,
                            Err(_) => {
                                log_warn!("Unexpected EOF while reading scan data, breaking");
                                break;
                            }
                        };
                    }
                b if b == (JpegMarker::EOI.to_u16() & 0xFF) as u8 => {
                    // End of image
                    break;
                }
                _ => {
                    // Any other marker - end of scan
                    self.reader.seek(SeekFrom::Current(-2))?;
                    break;
                }
            }
        }

        let data_length = scan_data.len() as u64;
        let scan = ScanData {
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            components: scan_components.clone(),
            dc_tables: self.dc_huffman_tables.clone(),
            ac_tables: self.ac_huffman_tables.clone(),
            data: scan_data,
        };

        self.scans.push(scan);

        self.record_segment(segment_start, "SOS", JpegSegmentData::SOS(SOSData {
            length,
            component_count: scan_component_count,
            components: scan_components,
            start_spectral,
            end_spectral,
            successive_high,
            successive_low,
            dc_tables: self.dc_huffman_tables.clone(),
            ac_tables: self.ac_huffman_tables.clone(),
            data_length,
        }));

        Ok(())
    }

    #[inline(always)]
    fn get_next_symbol(&self, reader: &mut BitReader<Cursor<Vec<u8>>>, table: &HuffmanTable) -> VexelResult<u8> {
        let mut code = 0;

        for i in 0..16 {
            let bit = reader.read_bit().unwrap_or_else(|_| {
                log_warn!("Failed to read bit from bit reader, replacing with 0");
                false
            }) as u32;

            code = (code << 1) | bit;

            for j in table.offsets[i] as usize..table.offsets[i + 1] as usize {
                if table.codes[j] == code {
                    return Ok(table.symbols[j]);
                }
            }
        }

        log_warn!("Invalid Huffman code: {}, replacing with 0", code);

        Ok(0)
    }

    fn decode_mcu(
        &mut self,
        reader: &mut BitReader<Cursor<Vec<u8>>>,
        mcu_component: &mut [i32],
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        previous_dc: &mut i32,
    ) -> VexelResult<()> {
        let length = self.get_next_symbol(reader, dc_table)?;

        let max_length = if self.precision > 8 { 12 } else { 11 };
        if length > max_length {
            log_warn!("Invalid DC coefficient length (>{}): {}", max_length, length);
            return Ok(());
        }

        let mut coefficient = reader.read_bits(length)? as i32;

        if length != 0 && coefficient < (1 << (length - 1)) {
            coefficient -= (1 << length) - 1;
        }

        mcu_component[0] = coefficient + *previous_dc;
        *previous_dc = mcu_component[0];

        let mut i = 1;
        while i < 64 {
            let symbol = self.get_next_symbol(reader, ac_table).unwrap_or_else(|_| {
                log_warn!("Failed to get next AC symbol during decoding, replacing with 0");
                0
            });

            if symbol == 0 {
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }

                return Ok(());
            }

            let mut zero_count = symbol >> 4;
            let mut coefficient_length = symbol & 0xF;

            if symbol == 0xF0 {
                zero_count = 16;
                coefficient_length = 0;
            }

            if i + zero_count as usize >= 64 {
                log_warn!("Sum of zero count and current index of mcu value exceeds 64");
                for j in i..64 {
                    mcu_component[ZIGZAG_MAP[j] as usize] = 0;
                }
                return Ok(());
            }

            for _ in 0..zero_count {
                mcu_component[ZIGZAG_MAP[i] as usize] = 0;
                i += 1;
            }

            let max_coefficient_length = if self.precision > 8 { 16 } else { 10 };
            if coefficient_length > max_coefficient_length {
                log_warn!("Invalid coefficient length: {}, replacing with 0", coefficient_length);
                coefficient_length = 0;
            }

            if coefficient_length != 0 {
                coefficient = reader.read_bits(coefficient_length)? as i32;

                if coefficient < (1 << (coefficient_length - 1)) {
                    coefficient -= (1 << coefficient_length) - 1;
                }

                if mcu_component.len() <= ZIGZAG_MAP[i] as usize {
                    log_warn!("Invalid zigzag index: {}, skipping", ZIGZAG_MAP[i]);
                    i += 1;
                    continue;
                }

                mcu_component[ZIGZAG_MAP[i] as usize] = coefficient;
                i += 1;
            }
        }

        Ok(())
    }

    fn decode_progressive(&mut self) -> VexelResult<Image> {
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                let comp_width =
                    (self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1) / max_h_samp as u32;
                let comp_height =
                    (self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1) / max_v_samp as u32;

                ComponentPlane::new(
                    comp_width,
                    comp_height,
                )
            })
            .collect();

        self.decode_progressive_scans(&mut component_planes)?;
        self.dequantize_planes(&mut component_planes)?;
        self.inverse_dct_planes(&mut component_planes)?;

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn decode_progressive_scans(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let mut previous_dc = vec![0i32; planes.len()];

        for scan in &self.scans {
            let mut reader = BitReader::new(Cursor::new(scan.data.clone()));
            let mut skips = 0;
            let restart_interval = self.restart_interval;

            let mut max_h_samp = self
                .components
                .iter()
                .map(|c| c.horizontal_sampling_factor)
                .max()
                .unwrap_or(1);
            let mut max_v_samp = self
                .components
                .iter()
                .map(|c| c.vertical_sampling_factor)
                .max()
                .unwrap_or(1);

            let is_luminance_only = scan.components.len() == 1 && scan.components[0].component_id == 1;

            if is_luminance_only {
                max_h_samp = 1;
                max_v_samp = 1;
            }

            let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
            let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

            let mut restart_counter = restart_interval;

            for mcu_y in 0..mcu_height {
                for mcu_x in 0..mcu_width {
                    if restart_interval > 0 {
                        if restart_counter == 0 {
                            previous_dc.fill(0);
                            reader.clear_buffer();
                            restart_counter = restart_interval;
                        }
                        restart_counter = restart_counter.saturating_sub(1);
                    }

                    for (comp_idx, scan_comp) in scan.components.clone().iter().enumerate() {
                        let comp = match self.components.iter().find(|c| c.id == scan_comp.component_id) {
                            Some(c) => c,
                            None => {
                                log_warn!("Component not found: {}", scan_comp.component_id);
                                continue;
                            }
                        };

                        let h_blocks = if is_luminance_only {
                            1
                        } else {
                            comp.horizontal_sampling_factor
                        };
                        let v_blocks = if is_luminance_only {
                            1
                        } else {
                            comp.vertical_sampling_factor
                        };
                        let plane_index = comp.id as usize - 1;

                        if plane_index >= planes.len() {
                            log_warn!("Invalid plane index: {}", plane_index);
                            continue;
                        }

                        for v in 0..v_blocks {
                            for h in 0..h_blocks {
                                let plane_blocks_per_line = planes[plane_index].blocks_per_line;
                                let block_x = if is_luminance_only {
                                    mcu_x + h as u32
                                } else {
                                    (mcu_x * comp.horizontal_sampling_factor as u32 + h as u32)
                                        .min(plane_blocks_per_line - 1)
                                };

                                let block_y = if is_luminance_only {
                                    mcu_y + v as u32
                                } else {
                                    mcu_y * comp.vertical_sampling_factor as u32 + v as u32
                                };

                                if block_x >= plane_blocks_per_line {
                                    continue;
                                }

                                if let Some(component_data) = planes[plane_index].get_block_mut(block_x, block_y) {
                                    let scan_component = scan_comp;

                                    if scan.start_spectral == 0 {
                                        if scan.successive_high == 0 {
                                            // First DC scan
                                            let dc_table =
                                                match scan.dc_tables.get(scan_component.dc_table_selector as usize) {
                                                    Some(table) => table,
                                                    None => {
                                                        log_warn!(
                                                            "DC table not found: {}",
                                                            scan_component.dc_table_selector
                                                        );
                                                        continue;
                                                    }
                                                };

                                            let length = self.get_next_symbol(&mut reader, dc_table)?;

                                            if length > 11 {
                                                log_warn!("Invalid DC coefficient length (>11): {}", length);
                                                continue;
                                            }

                                            let bits = match reader.read_bits(length) {
                                                Ok(bits) => bits,
                                                Err(e) => {
                                                    log_warn!("Failed to read DC coefficient bits: {}", e);
                                                    continue;
                                                }
                                            };

                                            let mut value = bits as i32;

                                            if length != 0 && value < (1 << (length - 1)) {
                                                value -= (1 << length) - 1;
                                            }

                                            value += previous_dc[comp_idx];
                                            previous_dc[comp_idx] = value;
                                            component_data[0] = value << scan.successive_low;
                                        } else {
                                            // Refining DC scan
                                            let bit = match reader.read_bits(1) {
                                                Ok(bit) => bit,
                                                Err(e) => {
                                                    log_warn!("Failed to read DC coefficient bit: {}", e);
                                                    continue;
                                                }
                                            };

                                            component_data[0] |= (bit as i32) << scan.successive_low;
                                        }
                                    }

                                    if scan.end_spectral > 0 {
                                        if scan.successive_high == 0 {
                                            // First AC scan
                                            if skips > 0 {
                                                skips -= 1;
                                                continue;
                                            }

                                            let ac_table =
                                                match scan.ac_tables.get(scan_component.ac_table_selector as usize) {
                                                    Some(table) => table,
                                                    None => {
                                                        log_warn!(
                                                            "AC table not found: {}",
                                                            scan_component.ac_table_selector
                                                        );
                                                        continue;
                                                    }
                                                };

                                            let mut k = scan.start_spectral as usize;
                                            while k <= scan.end_spectral as usize {
                                                let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                    Ok(symbol) => symbol,
                                                    Err(e) => {
                                                        log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                        break;
                                                    }
                                                };

                                                let num_zeros = symbol >> 4;
                                                let length = symbol & 0xF;

                                                if length != 0 {
                                                    if k + num_zeros as usize > 63 {
                                                        log_warn!(
                                                            "Zero run-length exceeded spectral selection: {}",
                                                            k + num_zeros as usize
                                                        );
                                                        break;
                                                    }

                                                    for _ in 0..num_zeros {
                                                        if k > ZIGZAG_MAP.len() {
                                                            log_warn!("k value exceeded zigzag map: {}", k);
                                                            break;
                                                        }

                                                        component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                        k += 1;
                                                    }

                                                    if length > 10 {
                                                        log_warn!("Invalid AC coefficient length (>10): {}", length);
                                                        break;
                                                    }

                                                    let bits = match reader.read_bits(length) {
                                                        Ok(bits) => bits,
                                                        Err(e) => {
                                                            log_warn!("Failed to read AC coefficient bits: {}", e);
                                                            break;
                                                        }
                                                    };

                                                    let mut value = bits as i32;

                                                    if value < (1 << (length - 1)) {
                                                        value -= (1 << length) - 1;
                                                    }

                                                    let zigzag_idx = ZIGZAG_MAP[k] as usize;
                                                    component_data[zigzag_idx] = value << scan.successive_low;
                                                    k += 1;
                                                } else {
                                                    if num_zeros == 15 {
                                                        if k + num_zeros as usize > scan.end_spectral as usize {
                                                            log_warn!(
                                                                "Zero run-length exceeded spectral selection: {}",
                                                                k + num_zeros as usize
                                                            );
                                                            break;
                                                        }

                                                        for _ in 0..num_zeros {
                                                            if k > ZIGZAG_MAP.len() {
                                                                log_warn!("k value exceeded zigzag map: {}", k);
                                                                break;
                                                            }

                                                            component_data[ZIGZAG_MAP[k] as usize] = 0;
                                                            k += 1;
                                                        }
                                                    } else {
                                                        skips = (1 << num_zeros) - 1;
                                                        let extra_skips =
                                                            reader.read_bits(num_zeros).unwrap_or_else(|e| {
                                                                log_warn!("Failed to read extra skips: {}", e);
                                                                0
                                                            });

                                                        skips += extra_skips;
                                                        break;
                                                    }

                                                    k += 1;
                                                }
                                            }
                                        } else {
                                            // Refining AC scan
                                            let positive = 1 << scan.successive_low;
                                            let negative = -1 << scan.successive_low;
                                            let mut k = scan.start_spectral as usize;

                                            if skips == 0 {
                                                let ac_table =
                                                    match scan.ac_tables.get(scan_component.ac_table_selector as usize)
                                                    {
                                                        Some(table) => table,
                                                        None => {
                                                            log_warn!(
                                                                "AC table not found: {}",
                                                                scan_component.ac_table_selector
                                                            );
                                                            continue;
                                                        }
                                                    };

                                                while k <= scan.end_spectral as usize {
                                                    let symbol = match self.get_next_symbol(&mut reader, ac_table) {
                                                        Ok(symbol) => symbol,
                                                        Err(e) => {
                                                            log_warn!("Failed to read AC coefficient symbol: {}", e);
                                                            break;
                                                        }
                                                    };

                                                    let mut num_zeros = symbol >> 4;
                                                    let length = symbol & 0xF;
                                                    let mut coefficient = 0;

                                                    if length != 0 {
                                                        if length != 1 {
                                                            log_warn!(
                                                                "Invalid AC coefficient length (refining): {}",
                                                                length
                                                            );
                                                            break;
                                                        }

                                                        coefficient = match reader.read_bits(1) {
                                                            Ok(bit) => match bit {
                                                                0 => negative,
                                                                1 => positive,
                                                                _ => unreachable!(),
                                                            },
                                                            Err(e) => {
                                                                log_warn!(
                                                                    "Failed to read AC coefficient bit (refining): {}",
                                                                    e
                                                                );
                                                                break;
                                                            }
                                                        };
                                                    } else {
                                                        if num_zeros != 15 {
                                                            skips = 1 << num_zeros;
                                                            let extra_skips =
                                                                reader.read_bits(num_zeros).unwrap_or_else(|e| {
                                                                    log_warn!(
                                                                        "Failed to read extra skips (refining): {}",
                                                                        e
                                                                    );
                                                                    0
                                                                });

                                                            skips += extra_skips;
                                                            break;
                                                        }
                                                    }

                                                    if component_data.len() <= ZIGZAG_MAP[k] as usize {
                                                        log_warn!(
                                                            "Value from a zigzag map exceeds component data length: {}",
                                                            ZIGZAG_MAP[k]
                                                        );
                                                        break;
                                                    }

                                                    loop {
                                                        if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                            match reader.read_bits(1) {
                                                                Ok(bit) => {
                                                                    if bit == 1 {
                                                                        if component_data[ZIGZAG_MAP[k] as usize]
                                                                            & positive
                                                                            == 0
                                                                        {
                                                                            if component_data[ZIGZAG_MAP[k] as usize]
                                                                                >= 0
                                                                            {
                                                                                component_data
                                                                                    [ZIGZAG_MAP[k] as usize] +=
                                                                                    positive;
                                                                            } else {
                                                                                component_data
                                                                                    [ZIGZAG_MAP[k] as usize] +=
                                                                                    negative;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    log_warn!("Failed to read AC coefficient bit (refining): {}", e);
                                                                    break;
                                                                }
                                                            }
                                                        } else {
                                                            if num_zeros == 0 {
                                                                break;
                                                            }
                                                            num_zeros -= 1;
                                                        }

                                                        k += 1;

                                                        if k > scan.end_spectral as usize {
                                                            break;
                                                        }
                                                    }

                                                    if coefficient != 0 && k <= scan.end_spectral as usize {
                                                        component_data[ZIGZAG_MAP[k] as usize] = coefficient;
                                                    }

                                                    k += 1;
                                                }
                                            }

                                            if skips > 0 {
                                                while k <= scan.end_spectral as usize {
                                                    if component_data[ZIGZAG_MAP[k] as usize] != 0 {
                                                        match reader.read_bits(1) {
                                                            Ok(b) => {
                                                                if b == 1 {
                                                                    if component_data[ZIGZAG_MAP[k] as usize] & positive
                                                                        == 0
                                                                    {
                                                                        if component_data[ZIGZAG_MAP[k] as usize] >= 0 {
                                                                            component_data[ZIGZAG_MAP[k] as usize] +=
                                                                                positive;
                                                                        } else {
                                                                            component_data[ZIGZAG_MAP[k] as usize] +=
                                                                                negative;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                log_warn!("Failed to read AC coefficient bit: {}", e);
                                                                break;
                                                            }
                                                        }
                                                    }

                                                    k += 1;
                                                }

                                                skips -= 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn decode_differences(&mut self, scan: &ScanData) -> VexelResult<Vec<Vec<i32>>> {
        let mut reader = BitReader::new(Cursor::new(scan.data.clone()));

        let mut differences: Vec<Vec<i32>> = vec![vec![]; scan.components.len()];

        let width = self.width as usize;
        let height = self.height as usize;

        for diffs in &mut differences {
            diffs.reserve(width * height);
        }

        // TODO handle restarts
        for _ in 0..height {
            for _ in 0..width {
                for (i, scan_component) in scan.components.iter().enumerate() {
                    let dc_table = match scan.dc_tables.get(scan_component.dc_table_selector as usize) {
                        Some(table) => table,
                        None => {
                            log_warn!("No DC table found for component {} during lossless decoding. Using default table which will most likely produce incorrect results.", i);
                            &HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    let bits_to_read = self.get_next_symbol(&mut reader, dc_table)?;

                    let diff = match bits_to_read {
                        0 => 0,
                        1..=15 => {
                            let additional_bits = reader.read_bits(bits_to_read)? as i32;

                            if additional_bits < (1 << (bits_to_read - 1)) {
                                additional_bits + (-1 << bits_to_read) + 1
                            } else {
                                additional_bits
                            }
                        }
                        16 => 32768,
                        _ => {
                            log_warn!("Invalid difference: {}", bits_to_read);
                            0
                        }
                    };

                    differences[i].push(diff);
                }
            }
        }

        Ok(differences)
    }

    fn predict(
        ra: i32,
        rb: i32,
        rc: i32,
        predictor: Predictor,
        point_transform: u8,
        input_precision: u8,
        x: usize,
        y: usize,
    ) -> i32 {
        // TODO handle restarts as well
        if x == 0 && y == 0 {
            if input_precision > point_transform + 1 {
                1 << (input_precision - point_transform - 1)
            } else {
                0
            }
        } else if y == 0 {
            ra
        } else if x == 0 {
            rb
        } else {
            match predictor {
                Predictor::NoPrediction => 0,
                Predictor::Ra => ra,
                Predictor::Rb => rb,
                Predictor::Rc => rc,
                Predictor::RaRbRc1 => ra + rb - rc,
                Predictor::RaRbRc2 => ra + ((rb - rc) >> 1),
                Predictor::RaRbRc3 => rb + ((ra - rc) >> 1),
                Predictor::RaRb => (ra + rb) / 2,
            }
        }
    }

    fn reconstruct_samples(
        &self,
        differences: Vec<Vec<i32>>,
        predictor: Predictor,
        point_transform: u8,
    ) -> VexelResult<Vec<Vec<u16>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = differences.len();

        let mut samples = vec![vec![0u16; width * height]; components_count];

        if predictor == Predictor::Ra {
            for component_index in 0..components_count {
                let default_prediction = 1 << (self.precision - point_transform - 1);

                let first_diff = differences[component_index][0];
                samples[component_index][0] = (((default_prediction + first_diff) & 0xFFFF) as u16) << point_transform;

                for y in 1..height {
                    let diff = differences[component_index][y * width];
                    let rb = samples[component_index][(y - 1) * width] as i32;
                    samples[component_index][y * width] = (((rb + diff) & 0xFFFF) as u16) << point_transform;
                }

                for y in 0..height {
                    for x in 1..width {
                        let index = y * width + x;
                        let diff = differences[component_index][index];
                        let ra = samples[component_index][index - 1] as i32;

                        samples[component_index][index] = (((ra + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    for component_index in 0..components_count {
                        let index = y * width + x;
                        let diff = differences[component_index][index];

                        let ra = if x > 0 {
                            samples[component_index][index - 1] as i32
                        } else {
                            0
                        };
                        let rb = if y > 0 {
                            samples[component_index][(y - 1) * width + x] as i32
                        } else {
                            0
                        };
                        let rc = if x > 0 && y > 0 {
                            samples[component_index][(y - 1) * width + (x - 1)] as i32
                        } else {
                            0
                        };

                        let prediction =
                            Self::predict(ra, rb, rc, predictor.clone(), point_transform, self.precision, x, y);

                        samples[component_index][index] = (((prediction + diff) & 0xFFFF) as u16) << point_transform;
                    }
                }
            }
        }

        Ok(samples)
    }

    fn samples_to_image(&self, samples: Vec<Vec<u16>>) -> VexelResult<Image> {
        let width = self.width as usize;
        let height = self.height as usize;
        let components_count = samples.len();

        let mut output: Vec<u16> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let pixel_pos = y * width + x;

                for component_index in 0..components_count {
                    let sample = samples[component_index][pixel_pos];
                    output.push(sample);
                }
            }
        }

        if components_count == 1 {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::L16(pixels), 0)])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::L8
                } else {
                    PixelFormat::L16
                },
                frames,
            ))
        } else {
            let frames = if self.precision <= 8 {
                let precision_correction = 8 - self.precision;
                let pixels = output.iter().map(|&s| (s as u8) << precision_correction).collect();

                Vec::from([ImageFrame::new(width as u32, height as u32, PixelData::RGB8(pixels), 0)])
            } else {
                let precision_correction = 16 - self.precision;
                let pixels: Vec<u16> = output.iter().map(|&s| s << precision_correction).collect();

                Vec::from([ImageFrame::new(
                    width as u32,
                    height as u32,
                    PixelData::RGB16(pixels),
                    0,
                )])
            };

            Ok(Image::new(
                width as u32,
                height as u32,
                if self.precision <= 8 {
                    PixelFormat::RGB8
                } else {
                    PixelFormat::RGB16
                },
                frames,
            ))
        }
    }

    fn decode_lossless(&mut self) -> VexelResult<Image> {
        // TODO there can be multiple scans in lossless mode somehow
        let scan = match self.scans.first() {
            Some(s) => s.clone(),
            None => {
                return Err(VexelError::from(Error::new(
                    ErrorKind::InvalidData,
                    "No scan data found",
                )))
            }
        };

        let differences = self.decode_differences(&scan)?;

        let point_transform = self.successive_approximation_low;
        let predictor = match scan.start_spectral {
            0 => Predictor::NoPrediction,
            1 => Predictor::Ra,
            2 => Predictor::Rb,
            3 => Predictor::Rc,
            4 => Predictor::RaRbRc1,
            5 => Predictor::RaRbRc2,
            6 => Predictor::RaRbRc3,
            7 => Predictor::RaRb,
            _ => {
                log_warn!("Invalid predictor selection: {}", scan.start_spectral);
                Predictor::NoPrediction
            }
        };

        let samples = self.reconstruct_samples(differences, predictor, point_transform)?;

        self.samples_to_image(samples)
    }

    fn decode_huffman_to_planes(&mut self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        if self.scans.len() < 1 {
            // Well, nothing to do here, how did this even happen?
            log_warn!("No scans found in JPEG data");
            return Ok(());
        }

        let scan = &self.scans[0];
        // TODO uhh, how can we not clone this?
        let mut reader = BitReader::new(Cursor::new(scan.data.clone()));
        let mut previous_dc = vec![0i32; planes.len()];

        let mut max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let mut max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        if max_h_samp == 0 || max_v_samp == 0 {
            log_warn!("Invalid sampling factors: ({}, {})", max_h_samp, max_v_samp);
            max_h_samp = 1;
            max_v_samp = 1;
        }

        let mcu_width = (self.width + 8 * max_h_samp as u32 - 1) / (8 * max_h_samp as u32);
        let mcu_height = (self.height + 8 * max_v_samp as u32 - 1) / (8 * max_v_samp as u32);

        let mut restart_counter = self.restart_interval as u32;

        for mcu_y in 0..mcu_height {
            for mcu_x in 0..mcu_width {
                if self.restart_interval > 0 {
                    if restart_counter == 0 {
                        previous_dc.fill(0);
                        reader.clear_buffer();
                        restart_counter = self.restart_interval as u32;
                    }

                    restart_counter = restart_counter.saturating_sub(1);
                }

                for (comp_idx, comp) in self.components.clone().iter().enumerate() {
                    if self.scans[0].components.len() <= comp_idx {
                        log_warn!(
                            "Component index out of bounds: {} {}",
                            self.scans[0].components.len(),
                            comp_idx
                        );
                        continue;
                    }

                    let dc_selector = self.scans[0].components[comp_idx].dc_table_selector as usize;
                    let ac_selector = self.scans[0].components[comp_idx].ac_table_selector as usize;

                    let dc_table = match self.scans[0].dc_tables.get(dc_selector) {
                        Some(table) => table.clone(),
                        None => {
                            log_warn!("DC table {} not found in baseline mode, substituting default, image will be corrupted.", dc_selector);

                            HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    let ac_table = match self.scans[0].ac_tables.get(ac_selector) {
                        Some(table) => table.clone(),
                        None => {
                            log_warn!("AC table {} not found in baseline mode, substituting default, image will be corrupted.", ac_selector);

                            HuffmanTable {
                                class: 0,
                                id: 0,
                                offsets: vec![0, 0, 0, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7],
                                symbols: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                                codes: vec![
                                    0b000,
                                    0b010,
                                    0b011,
                                    0b100,
                                    0b101,
                                    0b110,
                                    0b1110,
                                    0b11110,
                                    0b111110,
                                    0b1111110,
                                    0b11111110,
                                    0b111111110,
                                ],
                            }
                        }
                    };

                    for v in 0..comp.vertical_sampling_factor {
                        for h in 0..comp.horizontal_sampling_factor {
                            let block_x = mcu_x * comp.horizontal_sampling_factor as u32 + h as u32;
                            let block_y = mcu_y * comp.vertical_sampling_factor as u32 + v as u32;

                            if comp_idx >= previous_dc.len() {
                                log_warn!(
                                    "Component is larger than previous DC buffer: {} {}",
                                    comp_idx,
                                    previous_dc.len()
                                );
                                continue;
                            }

                            if let Some(block) = planes[comp_idx].get_block_mut(block_x, block_y) {
                                match self.decode_mcu(
                                    &mut reader,
                                    block,
                                    &dc_table,
                                    &ac_table,
                                    &mut previous_dc[comp_idx],
                                ) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        log_warn!("Failed to decode MCU: {}", e);
                                    }
                                };
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn dequantize_planes(&self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        for (comp_idx, plane) in planes.iter_mut().enumerate() {
            let default_table = &QuantizationTable {
                id: 0,
                precision: 8,
                length: 64,
                table: DEFAULT_QUANTIZATION_TABLE.to_vec(),
            };

            let quant_table = self
                .components
                .get(comp_idx)
                .and_then(|comp| {
                    self.quantization_tables
                        .iter()
                        .find(|q| q.id == comp.quantization_table_id)
                })
                .map(|q| q)
                .unwrap_or_else(|| {
                    log_warn!("Quantization table not found for component, substituting default one.");
                    default_table
                });

            for block in plane.data.chunks_mut(64) {
                for i in 0..64 {
                    if block.len() <= i || quant_table.table.len() <= i {
                        log_warn!(
                            "Block or quantization table index out of bounds: {} {}",
                            block.len(),
                            quant_table.table.len()
                        );
                        continue;
                    }

                    block[i] *= quant_table.table[i] as i32;
                }
            }
        }

        Ok(())
    }

    fn inverse_dct_planes(&self, planes: &mut [ComponentPlane]) -> VexelResult<()> {
        let m_0 = 2.0 * (1.0 / 16.0 * 2.0 * PI).cos();
        let m_1 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_3 = 2.0 * (2.0 / 16.0 * 2.0 * PI).cos();
        let m_5 = 2.0 * (3.0 / 16.0 * 2.0 * PI).cos();
        let m_2 = m_0 - m_5;
        let m_4 = m_0 + m_5;

        let s_0 = (0.0 / 16.0 * PI).cos() / 8.0_f32.sqrt();
        let s_1 = (1.0 / 16.0 * PI).cos() / 2.0;
        let s_2 = (2.0 / 16.0 * PI).cos() / 2.0;
        let s_3 = (3.0 / 16.0 * PI).cos() / 2.0;
        let s_4 = (4.0 / 16.0 * PI).cos() / 2.0;
        let s_5 = (5.0 / 16.0 * PI).cos() / 2.0;
        let s_6 = (6.0 / 16.0 * PI).cos() / 2.0;
        let s_7 = (7.0 / 16.0 * PI).cos() / 2.0;

        let level_shift = if self.precision <= 8 { 128 } else { 2048 };

        for plane in planes {
            let block_count = (plane.data.len() / 64) as u32;

            for block_idx in 0..block_count {
                let block_start = (block_idx * 64) as usize;

                if block_start + 64 > plane.data.len() {
                    log_warn!("Block index out of bounds: {} {}", block_start, plane.data.len());
                    continue;
                }

                let block = &mut plane.data[block_start..block_start + 64];
                let mut temp = [0.0f32; 64];

                for col in 0..8 {
                    let g_0 = block[0 * 8 + col] as f32 * s_0;
                    let g_1 = block[4 * 8 + col] as f32 * s_4;
                    let g_2 = block[2 * 8 + col] as f32 * s_2;
                    let g_3 = block[6 * 8 + col] as f32 * s_6;
                    let g_4 = block[5 * 8 + col] as f32 * s_5;
                    let g_5 = block[1 * 8 + col] as f32 * s_1;
                    let g_6 = block[7 * 8 + col] as f32 * s_7;
                    let g_7 = block[3 * 8 + col] as f32 * s_3;

                    let f_0 = g_0;
                    let f_1 = g_1;
                    let f_2 = g_2;
                    let f_3 = g_3;
                    let f_4 = g_4 - g_7;
                    let f_5 = g_5 + g_6;
                    let f_6 = g_5 - g_6;
                    let f_7 = g_4 + g_7;

                    let e_0 = f_0;
                    let e_1 = f_1;
                    let e_2 = f_2 - f_3;
                    let e_3 = f_2 + f_3;
                    let e_4 = f_4;
                    let e_5 = f_5 - f_7;
                    let e_6 = f_6;
                    let e_7 = f_5 + f_7;
                    let e_8 = f_4 + f_6;

                    let d_0 = e_0;
                    let d_1 = e_1;
                    let d_2 = e_2 * m_1;
                    let d_3 = e_3;
                    let d_4 = e_4 * m_2;
                    let d_5 = e_5 * m_3;
                    let d_6 = e_6 * m_4;
                    let d_7 = e_7;
                    let d_8 = e_8 * m_5;

                    let c_0 = d_0 + d_1;
                    let c_1 = d_0 - d_1;
                    let c_2 = d_2 - d_3;
                    let c_3 = d_3;
                    let c_4 = d_4 + d_8;
                    let c_5 = d_5 + d_7;
                    let c_6 = d_6 - d_8;
                    let c_7 = d_7;
                    let c_8 = c_5 - c_6;

                    let b_0 = c_0 + c_3;
                    let b_1 = c_1 + c_2;
                    let b_2 = c_1 - c_2;
                    let b_3 = c_0 - c_3;
                    let b_4 = c_4 - c_8;
                    let b_5 = c_8;
                    let b_6 = c_6 - c_7;
                    let b_7 = c_7;

                    temp[0 * 8 + col] = b_0 + b_7;
                    temp[1 * 8 + col] = b_1 + b_6;
                    temp[2 * 8 + col] = b_2 + b_5;
                    temp[3 * 8 + col] = b_3 + b_4;
                    temp[4 * 8 + col] = b_3 - b_4;
                    temp[5 * 8 + col] = b_2 - b_5;
                    temp[6 * 8 + col] = b_1 - b_6;
                    temp[7 * 8 + col] = b_0 - b_7;
                }

                for row in 0..8 {
                    let g_0 = temp[row * 8 + 0] * s_0;
                    let g_1 = temp[row * 8 + 4] * s_4;
                    let g_2 = temp[row * 8 + 2] * s_2;
                    let g_3 = temp[row * 8 + 6] * s_6;
                    let g_4 = temp[row * 8 + 5] * s_5;
                    let g_5 = temp[row * 8 + 1] * s_1;
                    let g_6 = temp[row * 8 + 7] * s_7;
                    let g_7 = temp[row * 8 + 3] * s_3;

                    let f_0 = g_0;
                    let f_1 = g_1;
                    let f_2 = g_2;
                    let f_3 = g_3;
                    let f_4 = g_4 - g_7;
                    let f_5 = g_5 + g_6;
                    let f_6 = g_5 - g_6;
                    let f_7 = g_4 + g_7;

                    let e_0 = f_0;
                    let e_1 = f_1;
                    let e_2 = f_2 - f_3;
                    let e_3 = f_2 + f_3;
                    let e_4 = f_4;
                    let e_5 = f_5 - f_7;
                    let e_6 = f_6;
                    let e_7 = f_5 + f_7;
                    let e_8 = f_4 + f_6;

                    let d_0 = e_0;
                    let d_1 = e_1;
                    let d_2 = e_2 * m_1;
                    let d_3 = e_3;
                    let d_4 = e_4 * m_2;
                    let d_5 = e_5 * m_3;
                    let d_6 = e_6 * m_4;
                    let d_7 = e_7;
                    let d_8 = e_8 * m_5;

                    let c_0 = d_0 + d_1;
                    let c_1 = d_0 - d_1;
                    let c_2 = d_2 - d_3;
                    let c_3 = d_3;
                    let c_4 = d_4 + d_8;
                    let c_5 = d_5 + d_7;
                    let c_6 = d_6 - d_8;
                    let c_7 = d_7;
                    let c_8 = c_5 - c_6;

                    let b_0 = c_0 + c_3;
                    let b_1 = c_1 + c_2;
                    let b_2 = c_1 - c_2;
                    let b_3 = c_0 - c_3;
                    let b_4 = c_4 - c_8;
                    let b_5 = c_8;
                    let b_6 = c_6 - c_7;
                    let b_7 = c_7;

                    block[row * 8 + 0] = ((b_0 + b_7 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 1] = ((b_1 + b_6 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 2] = ((b_2 + b_5 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 3] = ((b_3 + b_4 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 4] = ((b_3 - b_4 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 5] = ((b_2 - b_5 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 6] = ((b_1 - b_6 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                    block[row * 8 + 7] = ((b_0 - b_7 + 0.5) as i32).clamp(-level_shift, level_shift * 2 - 1);
                }
            }
        }

        Ok(())
    }

    fn upsample_planes(&self, planes: &[ComponentPlane]) -> Vec<UpsampledPlane> {
        let mut upsampled_planes = Vec::new();

        for plane in planes.iter() {
            // For Y component (id=1), we keep original dimensions
            // For Cb and Cr (id=2,3), we upsample to full image dimensions
            let target_width = self.width;
            let target_height = self.height;

            let upsampled = plane.upsample(target_width, target_height);
            upsampled_planes.push(upsampled);
        }

        upsampled_planes
    }

    fn convert_colorspace(&self, planes: &[UpsampledPlane]) -> VexelResult<PixelData> {
        let mut pixels = Vec::with_capacity((self.width * self.height * 3) as usize);

        fn get_pixel_from_planes(planes: &[UpsampledPlane], index: usize, x: u32, y: u32) -> f32 {
            match planes.get(index) {
                Some(plane) => plane.get_pixel(x, y).unwrap_or(0) as f32,
                None => 0.0,
            }
        }

        if planes.len() == 1 {
            return if self.precision <= 8 {
                for y in 0..self.height {
                    for x in 0..self.width {
                        let y_val = planes[0].get_pixel(x, y).unwrap_or(0);
                        let gray_val = y_val.clamp(0, 255) as u8;
                        pixels.push(gray_val);
                    }
                }

                Ok(PixelData::L8(pixels))
            } else {
                let mut pixels16 = Vec::with_capacity((self.width * self.height) as usize);

                for y in 0..self.height {
                    for x in 0..self.width {
                        let y_val = planes[0].get_pixel(x, y).unwrap_or(0);
                        let gray_val = y_val.clamp(0, 4095) as u16;
                        pixels16.push(gray_val);
                    }
                }

                Ok(PixelData::L16(pixels16))
            };
        }

        if planes.len() < 3 {
            log_warn!("Invalid number of planes for RGB conversion: {}.", planes.len());
        }

        if self.precision <= 8 {
            for y in 0..self.height {
                for x in 0..self.width {
                    let y_val = get_pixel_from_planes(planes, 0, x, y);
                    let cb_val = get_pixel_from_planes(planes, 1, x, y);
                    let cr_val = get_pixel_from_planes(planes, 2, x, y);

                    let r = (y_val + 1.402 * cr_val + 128.0).clamp(0.0, 255.0) as u8;
                    let g = (y_val - 0.344136 * cb_val - 0.714136 * cr_val + 128.0).clamp(0.0, 255.0) as u8;
                    let b = (y_val + 1.772 * cb_val + 128.0).clamp(0.0, 255.0) as u8;

                    pixels.extend_from_slice(&[r, g, b]);
                }
            }

            Ok(PixelData::RGB8(pixels))
        } else {
            let mut pixels16 = Vec::with_capacity((self.width * self.height * 3) as usize);

            for y in 0..self.height {
                for x in 0..self.width {
                    let y_val = get_pixel_from_planes(planes, 0, x, y);
                    let cb_val = get_pixel_from_planes(planes, 1, x, y);
                    let cr_val = get_pixel_from_planes(planes, 2, x, y);

                    let r = (y_val + 1.402 * cr_val + 2048.0).clamp(0.0, 4095.0) as u16;
                    let g = (y_val - 0.344136 * cb_val - 0.714136 * cr_val + 2048.0).clamp(0.0, 4095.0) as u16;
                    let b = (y_val + 1.772 * cb_val + 2048.0).clamp(0.0, 4095.0) as u16;

                    pixels16.extend_from_slice(&[r, g, b]);
                }
            }

            Ok(PixelData::RGB16(pixels16))
        }
    }
    
    fn decode_baseline(&mut self) -> VexelResult<Image> {
        let max_h_samp = self
            .components
            .iter()
            .map(|c| c.horizontal_sampling_factor)
            .max()
            .unwrap_or(1);
        let max_v_samp = self
            .components
            .iter()
            .map(|c| c.vertical_sampling_factor)
            .max()
            .unwrap_or(1);

        let mut component_planes: Vec<ComponentPlane> = self
            .components
            .iter()
            .map(|comp| {
                let comp_width =
                    (self.width * comp.horizontal_sampling_factor as u32 + max_h_samp as u32 - 1) / max_h_samp as u32;
                let comp_height =
                    (self.height * comp.vertical_sampling_factor as u32 + max_v_samp as u32 - 1) / max_v_samp as u32;

                ComponentPlane::new(
                    comp_width,
                    comp_height,
                )
            })
            .collect();

        match self.coding_method {
            JpegCodingMethod::Huffman => self.decode_huffman_to_planes(&mut component_planes)?,
            JpegCodingMethod::Arithmetic => todo!(),
        }

        self.dequantize_planes(&mut component_planes)?;
        self.inverse_dct_planes(&mut component_planes)?;

        let upsampled_planes = self.upsample_planes(&component_planes);
        let mut pixel_data = self.convert_colorspace(&upsampled_planes)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }

    fn record_segment(&mut self, start_offset: u64, marker: &str, data: JpegSegmentData) {
        self.segments.push(JpegSegmentInfo {
            start_offset,
            marker: marker.to_string(),
            data,
        });
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_MARKERS) {
            match marker {
                Some(marker) => {
                    log_debug!("Found marker: {:?}", marker);

                    let segment_start = self.reader.stream_position().unwrap_or(0).saturating_sub(2);

                    let result = match marker {
                        JpegMarker::SOI => {
                            self.record_segment(segment_start, "SOI", JpegSegmentData::SOI);
                            Ok(())
                        }
                        JpegMarker::EOI => {
                            self.record_segment(segment_start, "EOI", JpegSegmentData::EOI);
                            break;
                        }
                        JpegMarker::COM => self.read_com(segment_start),
                        JpegMarker::APP0 => self.read_app0_jfif(segment_start),
                        JpegMarker::APP1 => self.read_app1_exif(segment_start),
                        JpegMarker::SOF0 => self.read_start_of_frame("SOF0", segment_start),
                        JpegMarker::SOF1 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.read_start_of_frame("SOF1", segment_start)
                        }
                        JpegMarker::SOF2 => {
                            self.mode = JpegMode::Progressive;
                            self.read_start_of_frame("SOF2", segment_start)
                        }
                        JpegMarker::SOF3 => {
                            self.mode = JpegMode::Lossless;
                            self.read_start_of_frame("SOF3", segment_start)
                        }
                        JpegMarker::SOF9 => {
                            self.mode = JpegMode::ExtendedSequential;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF9", segment_start)
                        }
                        JpegMarker::SOF11 => {
                            self.mode = JpegMode::Lossless;
                            self.coding_method = JpegCodingMethod::Arithmetic;
                            self.read_start_of_frame("SOF11", segment_start)
                        }
                        JpegMarker::DRI => self.read_restart_interval(segment_start),
                        JpegMarker::DQT => self.read_quantization_table(segment_start),
                        JpegMarker::DHT => self.read_huffman_table(segment_start),
                        JpegMarker::DAC => self.read_dac(segment_start),
                        JpegMarker::SOS => self.read_start_of_scan(segment_start),
                        _ => {
                            log_warn!("Unhandled marker found: {:?}", marker);
                            self.skip_unknown_marker_segment(&format!("{:?}", marker), segment_start)
                        }
                    };

                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            log_warn!("Failed to process {:?} marker segment: {}", marker, e);
                        }
                    }
                }
                None => {
                    log_debug!("End of file reached");
                    break;
                }
            }
        }

        log_debug!(
            "Dimensions: {}x{}. Number of pixels: {}",
            self.width,
            self.height,
            self.width * self.height
        );
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

        match &self.mode {
            JpegMode::Baseline => {
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::ExtendedSequential => {
                // TODO general decoding process is same as baseline, so this method can be used,
                // but it would be nice to have a different name for this mode
                let image = self.decode_baseline()?;
                Ok(image)
            }
            JpegMode::Progressive => {
                let image = self.decode_progressive()?;
                Ok(image)
            }
            JpegMode::Lossless => {
                let image = self.decode_lossless()?;
                Ok(image)
            }
        }
    }
}
