use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use crate::decoders::jbig2::arith::{ArithDecoder, Cx};
use crate::decoders::jbig2::bitmap::{Bitmap, ComposeOp};
use crate::decoders::jbig2::color::{self, ColorValues, DEFAULT_PALETTE};
use crate::decoders::jbig2::generic::{
    decode_generic_region, stats_size as generic_stats_size, GenericParams, NOMINAL_AT,
};
use crate::decoders::jbig2::halftone::{
    decode_halftone_region, decode_pattern_dict, HalftoneParams, PatternDict, PatternDictParams,
};
use crate::decoders::jbig2::huffman::{self, HuffmanReader, HuffmanTable};
use crate::decoders::jbig2::info::{scan, Jbig2Info};
use crate::decoders::jbig2::refinement::{decode_refinement_region, stats_size as refine_stats_size, RefinementParams};
use crate::decoders::jbig2::segment::{parse_region_info, parse_segment_header, SegmentHeader, SegmentType};
use crate::decoders::jbig2::symbol_dict::{decode_symbol_dict, SymbolDictHuffman, SymbolDictParams};
use crate::decoders::jbig2::text::{
    decode_symbol_id_huffman_table, decode_text_region_tracked, Placement, RefCorner, TextArithContexts, TextCoder,
    TextRegionHuffman, TextRegionParams,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, PixelData};
use crate::utils::limits::Limits;
use crate::utils::tiff::fax;
use crate::{log_debug, log_warn};

pub(super) const JBIG2_FILE_ID: [u8; 8] = [0x97, 0x4a, 0x42, 0x32, 0x0d, 0x0a, 0x1a, 0x0a];

enum SegmentResult {
    Symbols(Vec<Arc<Bitmap>>),
    Patterns(Arc<PatternDict>),
    Table(Arc<HuffmanTable>),
    Palette(Arc<Vec<[u8; 3]>>),
    Bitmap(Arc<Bitmap>),
}

struct Page {
    number: u32,
    bitmap: Bitmap,
    color: Option<Vec<[u8; 3]>>,
    default_pixel: bool,
    striped: bool,
    declared_height: u32,
    combination_operator_overridden: bool,
    default_op: ComposeOp,
}

pub struct Jbig2Decoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    info: Option<Jbig2Info>,
}

impl<R: Read + Seek> Jbig2Decoder<R> {
    pub fn new(reader: R) -> Self {
        Jbig2Decoder { reader, limits: Limits::default(), info: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> Jbig2Info {
        if self.info.is_none() {
            let saved_pos = self.reader.stream_position().unwrap_or(0);
            let data = self.read_all();
            let _ = self.reader.seek(SeekFrom::Start(saved_pos));
            match data {
                Ok(data) => self.info = Some(scan(&data)),
                Err(e) => log_warn!("Failed to read JBIG2 data: {}", e),
            }
        }

        self.info.clone().unwrap_or_default()
    }

    fn read_all(&mut self) -> VexelResult<Vec<u8>> {
        let mut data = Vec::new();
        self.reader.rewind()?;
        self.reader.read_to_end(&mut data)?;
        Ok(data)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let pages = self.decode_all_pages()?;
        pages
            .into_iter()
            .next()
            .ok_or_else(|| VexelError::Custom("JBIG2 stream contains no decodable page".to_string()))
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        self.decode_all_pages()
    }

    fn decode_all_pages(&mut self) -> VexelResult<Vec<Image>> {
        let data = self.read_all()?;

        let mut context = DecodeContext::new(&self.limits);

        let has_file_header = data.len() >= 8 && data[..8] == JBIG2_FILE_ID;
        let segments = parse_segments(&data, has_file_header);
        context.run(&segments, &data);

        let pages = context.finish();

        if pages.is_empty() {
            return Err(VexelError::Custom("JBIG2 stream contains no page information".to_string()));
        }

        let mut images = Vec::with_capacity(pages.len());
        for (page, color) in &pages {
            images.push(bitmap_to_image(page, color.as_deref()));
        }

        Ok(images)
    }
}

pub(super) struct ParsedSegment {
    pub header: SegmentHeader,
    pub header_offset: usize,
    pub start: usize,
    pub end: usize,
}

pub(super) fn parse_segments(data: &[u8], has_file_header: bool) -> Vec<ParsedSegment> {
    let mut segments = Vec::new();
    let mut offset = 0usize;
    let mut random_access = false;

    if has_file_header {
        if data.len() < 9 {
            return segments;
        }
        let flags = data[8];
        random_access = flags & 1 == 0;
        offset = if flags & 2 != 0 { 9 } else { 13 };
        if offset > data.len() {
            return segments;
        }
    }

    if random_access {
        let mut headers = Vec::new();
        while offset < data.len() {
            let Some(header) = parse_segment_header(&data[offset..]) else {
                break;
            };
            let header_offset = offset;
            offset += header.header_length;
            let is_eof = header.segment_type == SegmentType::EndOfFile;
            headers.push((header, header_offset));
            if is_eof {
                break;
            }
        }

        for (header, header_offset) in headers {
            let length = resolve_length(&header, data, offset);
            let start = offset.min(data.len());
            let end = start.saturating_add(length).min(data.len());
            offset = end;
            segments.push(ParsedSegment { header, header_offset, start, end });
        }

        return segments;
    }

    while offset < data.len() {
        let Some(header) = parse_segment_header(&data[offset..]) else {
            break;
        };
        let header_offset = offset;
        offset += header.header_length;

        let length = resolve_length(&header, data, offset);
        let start = offset.min(data.len());
        let end = start.saturating_add(length).min(data.len());
        offset = end;

        let is_eof = header.segment_type == SegmentType::EndOfFile;
        segments.push(ParsedSegment { header, header_offset, start, end });
        if is_eof {
            break;
        }
    }

    segments
}

fn resolve_length(header: &SegmentHeader, data: &[u8], start: usize) -> usize {
    if header.data_length != u32::MAX {
        return header.data_length as usize;
    }

    if header.segment_type != SegmentType::ImmediateGenericRegion {
        log_warn!("JBIG2 segment {} has unknown length, consuming remaining data", header.number);
        return data.len().saturating_sub(start);
    }

    let Some(region) = data.get(start..) else {
        return 0;
    };
    if region.len() < 18 {
        return region.len();
    }

    let mmr = region[17] & 1 != 0;
    let marker: [u8; 2] = if mmr { [0x00, 0x00] } else { [0xff, 0xac] };

    let mut position = 18usize;
    while position + 2 <= region.len() {
        if region[position] == marker[0] && region[position + 1] == marker[1] {
            return (position + 2 + 4).min(region.len());
        }
        position += 1;
    }

    log_warn!("JBIG2 generic region {} has unknown length and no terminator", header.number);
    region.len()
}

struct DecodeContext<'a> {
    limits: &'a Limits,
    results: HashMap<u32, SegmentResult>,
    symbol_order: Vec<u32>,
    pages: Vec<Page>,
    current_page: Option<usize>,
}

impl<'a> DecodeContext<'a> {
    fn new(limits: &'a Limits) -> DecodeContext<'a> {
        DecodeContext {
            limits,
            results: HashMap::new(),
            symbol_order: Vec::new(),
            pages: Vec::new(),
            current_page: None,
        }
    }

    fn run(&mut self, segments: &[ParsedSegment], data: &[u8]) {
        for segment in segments {
            let body = data.get(segment.start..segment.end).unwrap_or(&[]);
            self.select_page(&segment.header);
            self.process(&segment.header, body);
        }
    }

    fn select_page(&mut self, header: &SegmentHeader) {
        if header.segment_type == SegmentType::PageInfo || header.page_association == 0 {
            return;
        }
        if let Some(index) = self.pages.iter().position(|page| page.number == header.page_association) {
            self.current_page = Some(index);
        }
    }

    fn finish(self) -> Vec<(Bitmap, Option<Vec<[u8; 3]>>)> {
        self.pages.into_iter().map(|page| (page.bitmap, page.color)).collect()
    }

    fn process(&mut self, header: &SegmentHeader, body: &[u8]) {
        log_debug!("JBIG2 segment {} type {:?} length {}", header.number, header.segment_type, body.len());

        match header.segment_type {
            SegmentType::PageInfo => self.handle_page_info(header, body),
            SegmentType::EndOfStripe => self.handle_end_of_stripe(body),
            SegmentType::EndOfPage | SegmentType::EndOfFile | SegmentType::Profiles | SegmentType::Extension => {}
            SegmentType::Tables => {
                if let Some(table) = huffman::parse_custom_table(body) {
                    self.results.insert(header.number, SegmentResult::Table(Arc::new(table)));
                } else {
                    log_warn!("JBIG2 failed to parse custom Huffman table in segment {}", header.number);
                }
            }
            SegmentType::SymbolDictionary => self.handle_symbol_dict(header, body),
            SegmentType::PatternDictionary => self.handle_pattern_dict(header, body),
            SegmentType::IntermediateGenericRegion
            | SegmentType::ImmediateGenericRegion
            | SegmentType::ImmediateLosslessGenericRegion => self.handle_generic_region(header, body),
            SegmentType::IntermediateTextRegion
            | SegmentType::ImmediateTextRegion
            | SegmentType::ImmediateLosslessTextRegion => self.handle_text_region(header, body),
            SegmentType::IntermediateHalftoneRegion
            | SegmentType::ImmediateHalftoneRegion
            | SegmentType::ImmediateLosslessHalftoneRegion => self.handle_halftone_region(header, body),
            SegmentType::IntermediateRefinementRegion
            | SegmentType::ImmediateRefinementRegion
            | SegmentType::ImmediateLosslessRefinementRegion => self.handle_refinement_region(header, body),
            SegmentType::ColorPalette => {
                let colors = color::parse_palette_segment(body);
                if colors.is_empty() {
                    log_warn!("JBIG2 colour palette segment could not be parsed");
                } else {
                    self.results.insert(header.number, SegmentResult::Palette(Arc::new(colors)));
                }
            }
            SegmentType::Unknown(kind) => {
                log_warn!("JBIG2 unknown segment type {}, ignoring", kind);
            }
        }
    }

    fn handle_page_info(&mut self, header: &SegmentHeader, body: &[u8]) {
        if body.len() < 19 {
            log_warn!("JBIG2 page info segment too short");
            return;
        }

        let width = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
        let height = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
        let flags = body[16];
        let striping = u16::from_be_bytes([body[17], body[18]]);

        let striped = striping & 0x8000 != 0;
        let stripe_size = (striping & 0x7fff) as u32;
        let default_pixel = flags & 4 != 0;
        let color_ext = flags & 0x80 != 0;
        let default_op = ComposeOp::from_bits((flags >> 3) & 3);

        let unknown_height = height == u32::MAX;
        let initial_height = if unknown_height {
            if striped && stripe_size > 0 {
                stripe_size
            } else {
                0
            }
        } else {
            height
        };

        if let Err(error) = self.limits.check_dimensions(width, initial_height.max(1)) {
            log_warn!("JBIG2 page exceeds limits: {}", error);
            return;
        }

        let mut bitmap = Bitmap::new(width, initial_height);
        bitmap.fill(default_pixel);

        self.pages.push(Page {
            number: header.page_association,
            bitmap,
            color: if color_ext {
                Some(vec![[255u8; 3]; (width as usize).saturating_mul(initial_height as usize)])
            } else {
                None
            },
            default_pixel,
            striped: striped || unknown_height,
            declared_height: height,
            combination_operator_overridden: flags & 0x40 != 0,
            default_op,
        });
        self.current_page = Some(self.pages.len() - 1);
    }

    fn handle_end_of_stripe(&mut self, body: &[u8]) {
        if body.len() < 4 {
            return;
        }
        let end_row = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
        if let Some(index) = self.current_page {
            let page = &mut self.pages[index];
            if page.declared_height == u32::MAX {
                let needed = end_row.saturating_add(1);
                if page.bitmap.height < needed {
                    page.bitmap.resize_height(needed, page.default_pixel);
                }
            }
        }
    }

    fn compose_to_page(&mut self, bitmap: &Bitmap, x: u32, y: u32, op: ComposeOp) {
        let Some(index) = self.current_page else {
            log_warn!("JBIG2 region segment appeared before any page information, ignoring");
            return;
        };

        let page = &mut self.pages[index];
        let op = if page.combination_operator_overridden { page.default_op } else { op };

        if page.declared_height == u32::MAX || page.striped {
            let needed = y.saturating_add(bitmap.height);
            if page.bitmap.height < needed {
                if self.limits.check_dimensions(page.bitmap.width, needed).is_err() {
                    log_warn!("JBIG2 page growth to {} rows exceeds limits", needed);
                    return;
                }
                page.bitmap.resize_height(needed, page.default_pixel);
                if let Some(plane) = page.color.as_mut() {
                    plane.resize(page.bitmap.width as usize * needed as usize, [255u8; 3]);
                }
            }
        }

        page.bitmap.compose(bitmap, x as i64, y as i64, op);
    }

    fn collect_all_symbols(&self) -> Vec<Arc<Bitmap>> {
        let mut symbols = Vec::new();
        for number in &self.symbol_order {
            if let Some(SegmentResult::Symbols(list)) = self.results.get(number) {
                symbols.extend(list.iter().cloned());
            }
        }
        symbols
    }

    fn collect_symbols(&self, header: &SegmentHeader) -> Vec<Arc<Bitmap>> {
        let mut symbols = Vec::new();
        for reference in &header.referred_to {
            if let Some(SegmentResult::Symbols(list)) = self.results.get(reference) {
                symbols.extend(list.iter().cloned());
            }
        }

        symbols
    }

    fn collect_tables(&self, header: &SegmentHeader) -> Vec<Arc<HuffmanTable>> {
        let mut tables = Vec::new();
        for reference in &header.referred_to {
            if let Some(SegmentResult::Table(table)) = self.results.get(reference) {
                tables.push(table.clone());
            }
        }
        tables
    }

    fn handle_symbol_dict(&mut self, header: &SegmentHeader, body: &[u8]) {
        if body.len() < 2 {
            log_warn!("JBIG2 symbol dictionary segment too short");
            return;
        }

        let flags = u16::from_be_bytes([body[0], body[1]]);
        let huffman = flags & 1 != 0;
        let refagg = (flags >> 1) & 1 != 0;
        let template = ((flags >> 10) & 3) as u8;
        let rtemplate = ((flags >> 12) & 1) as u8;
        let context_used = (flags >> 8) & 1 != 0;

        if context_used {
            log_warn!("JBIG2 symbol dictionary requests reused arithmetic context, decoding with fresh context");
        }

        let mut offset = 2usize;
        let mut at = NOMINAL_AT;
        if !huffman {
            let count = if template == 0 { 4 } else { 1 };
            if body.len() < offset + count * 2 {
                log_warn!("JBIG2 symbol dictionary segment too short for AT pixels");
                return;
            }
            for entry in at.iter_mut().take(count) {
                *entry = (body[offset] as i8, body[offset + 1] as i8);
                offset += 2;
            }
        }

        let mut rat = [(0i8, 0i8); 2];
        if refagg && rtemplate == 0 {
            if body.len() < offset + 4 {
                log_warn!("JBIG2 symbol dictionary segment too short for refinement AT pixels");
                return;
            }
            rat[0] = (body[offset] as i8, body[offset + 1] as i8);
            rat[1] = (body[offset + 2] as i8, body[offset + 3] as i8);
            offset += 4;
        }

        if body.len() < offset + 8 {
            log_warn!("JBIG2 symbol dictionary segment too short for symbol counts");
            return;
        }
        let num_ex_syms = u32::from_be_bytes([body[offset], body[offset + 1], body[offset + 2], body[offset + 3]]);
        let num_new_syms = u32::from_be_bytes([body[offset + 4], body[offset + 5], body[offset + 6], body[offset + 7]]);
        offset += 8;

        if num_new_syms > 100_000 || num_ex_syms > 100_000 {
            log_warn!("JBIG2 symbol dictionary declares implausible symbol counts, ignoring segment");
            return;
        }

        let input_symbols = self.collect_symbols(header);
        let custom_tables = self.collect_tables(header);
        let mut table_index = 0usize;

        let huffman_tables = if huffman {
            let dh = match (flags >> 2) & 3 {
                0 => huffman::table_b4(),
                1 => huffman::table_b5(),
                3 => match take_table(&custom_tables, &mut table_index) {
                    Some(table) => table,
                    None => {
                        log_warn!("JBIG2 symbol dictionary is missing custom DH table");
                        return;
                    }
                },
                _ => {
                    log_warn!("JBIG2 symbol dictionary specifies invalid DH table");
                    return;
                }
            };

            let dw = match (flags >> 4) & 3 {
                0 => huffman::table_b2(),
                1 => huffman::table_b3(),
                3 => match take_table(&custom_tables, &mut table_index) {
                    Some(table) => table,
                    None => {
                        log_warn!("JBIG2 symbol dictionary is missing custom DW table");
                        return;
                    }
                },
                _ => {
                    log_warn!("JBIG2 symbol dictionary specifies invalid DW table");
                    return;
                }
            };

            let bmsize = if (flags >> 6) & 1 != 0 {
                match take_table(&custom_tables, &mut table_index) {
                    Some(table) => table,
                    None => {
                        log_warn!("JBIG2 symbol dictionary is missing custom BMSIZE table");
                        return;
                    }
                }
            } else {
                huffman::table_b1()
            };

            let agginst = if (flags >> 7) & 1 != 0 {
                match take_table(&custom_tables, &mut table_index) {
                    Some(table) => table,
                    None => {
                        log_warn!("JBIG2 symbol dictionary is missing custom AGGINST table");
                        return;
                    }
                }
            } else {
                huffman::table_b1()
            };

            Some(SymbolDictHuffman { dh, dw, bmsize, agginst })
        } else {
            None
        };

        let params = SymbolDictParams { huffman, refagg, template, rtemplate, at, rat, num_ex_syms, num_new_syms };

        let mut gb_stats = vec![Cx::default(); generic_stats_size(template)];
        let mut gr_stats = vec![Cx::default(); refine_stats_size(rtemplate)];

        let symbols = decode_symbol_dict(
            &body[offset..],
            &params,
            &input_symbols,
            huffman_tables.as_ref(),
            &mut gb_stats,
            &mut gr_stats,
        );

        self.symbol_order.push(header.number);
        self.results.insert(header.number, SegmentResult::Symbols(symbols));
    }

    fn handle_pattern_dict(&mut self, header: &SegmentHeader, body: &[u8]) {
        if body.len() < 7 {
            log_warn!("JBIG2 pattern dictionary segment too short");
            return;
        }

        let flags = body[0];
        let params = PatternDictParams {
            mmr: flags & 1 != 0,
            template: (flags >> 1) & 3,
            width: body[1] as u32,
            height: body[2] as u32,
            gray_max: u32::from_be_bytes([body[3], body[4], body[5], body[6]]),
        };

        if params.gray_max > 10_000 {
            log_warn!("JBIG2 pattern dictionary declares implausible GRAYMAX, ignoring segment");
            return;
        }

        let mut gb_stats = vec![Cx::default(); generic_stats_size(params.template)];

        match decode_pattern_dict(&body[7..], &params, &mut gb_stats) {
            Some(dict) => {
                self.results.insert(header.number, SegmentResult::Patterns(Arc::new(dict)));
            }
            None => log_warn!("JBIG2 failed to decode pattern dictionary in segment {}", header.number),
        }
    }

    fn handle_generic_region(&mut self, header: &SegmentHeader, body: &[u8]) {
        let Some(region) = parse_region_info(body) else {
            log_warn!("JBIG2 generic region segment too short");
            return;
        };

        if body.len() < 18 {
            log_warn!("JBIG2 generic region segment too short");
            return;
        }

        let flags = body[17];
        let mmr = flags & 1 != 0;
        let template = (flags >> 1) & 3;
        let tpgdon = (flags >> 3) & 1 != 0;

        let ext_template = (flags >> 4) & 1 != 0;

        let mut offset = 18usize;
        let mut at = NOMINAL_AT;
        if !mmr {
            let count = if ext_template {
                12
            } else if template == 0 {
                4
            } else {
                1
            };
            if body.len() < offset + count * 2 {
                log_warn!("JBIG2 generic region segment too short for AT pixels");
                return;
            }
            for entry in at.iter_mut().take(count) {
                *entry = (body[offset] as i8, body[offset + 1] as i8);
                offset += 2;
            }
        }

        if self.limits.check_dimensions(region.width, region.height).is_err() {
            log_warn!("JBIG2 generic region {}x{} exceeds limits", region.width, region.height);
            return;
        }

        let mut bitmap = Bitmap::new(region.width, region.height);

        if mmr {
            let (decoded, _) = fax::decode_mmr(&body[offset..], region.width, region.height as usize);
            let stride = (region.width as usize).div_ceil(8);
            for row in 0..region.height as usize {
                let src = row * stride;
                let dst = row * bitmap.stride;
                if src + stride > decoded.len() || dst + stride > bitmap.data.len() {
                    break;
                }
                bitmap.data[dst..dst + stride].copy_from_slice(&decoded[src..src + stride]);
            }
        } else {
            let params = GenericParams { template, tpgdon, ext_template, at, skip: None };
            let mut stats = vec![Cx::default(); generic_stats_size(template)];
            let mut arith = ArithDecoder::new(&body[offset..]);
            decode_generic_region(&mut arith, &mut stats, &mut bitmap, &params);
        }

        if header.segment_type.is_intermediate() {
            self.results.insert(header.number, SegmentResult::Bitmap(Arc::new(bitmap)));
        } else {
            self.compose_to_page(&bitmap, region.x, region.y, region.op);
        }
    }

    fn handle_text_region(&mut self, header: &SegmentHeader, body: &[u8]) {
        let Some(region) = parse_region_info(body) else {
            log_warn!("JBIG2 text region segment too short");
            return;
        };

        let color_ext = region.flags & 8 != 0;

        let mut offset = 17usize;
        if body.len() < offset + 2 {
            log_warn!("JBIG2 text region segment too short");
            return;
        }

        let flags = u16::from_be_bytes([body[offset], body[offset + 1]]);
        offset += 2;

        let huffman = flags & 1 != 0;
        let refine = (flags >> 1) & 1 != 0;
        let log_strips = ((flags >> 2) & 3) as u32;
        let ref_corner = RefCorner::from_bits(((flags >> 4) & 3) as u8);
        let transposed = (flags >> 6) & 1 != 0;
        let comb_op = ComposeOp::from_bits(((flags >> 7) & 3) as u8);
        let def_pixel = (flags >> 9) & 1 != 0;
        let mut ds_offset = ((flags >> 10) & 0x1f) as i32;
        if ds_offset > 15 {
            ds_offset -= 32;
        }
        let rtemplate = ((flags >> 15) & 1) as u8;

        let huffman_flags = if huffman {
            if body.len() < offset + 2 {
                log_warn!("JBIG2 text region segment too short for Huffman flags");
                return;
            }
            let value = u16::from_be_bytes([body[offset], body[offset + 1]]);
            offset += 2;
            value
        } else {
            0
        };

        let mut rat = [(0i8, 0i8); 2];
        if refine && rtemplate == 0 {
            if body.len() < offset + 4 {
                log_warn!("JBIG2 text region segment too short for refinement AT pixels");
                return;
            }
            rat[0] = (body[offset] as i8, body[offset + 1] as i8);
            rat[1] = (body[offset + 2] as i8, body[offset + 3] as i8);
            offset += 4;
        }

        if body.len() < offset + 4 {
            log_warn!("JBIG2 text region segment too short for instance count");
            return;
        }
        let num_instances = u32::from_be_bytes([body[offset], body[offset + 1], body[offset + 2], body[offset + 3]]);
        offset += 4;

        let mut symbols = self.collect_symbols(header);
        if symbols.is_empty() {
            symbols = self.collect_all_symbols();
            if symbols.is_empty() {
                log_warn!("JBIG2 text region refers to no symbols");
            } else {
                log_warn!(
                    "JBIG2 text region {} refers to no symbol dictionary, using all {} symbols decoded so far",
                    header.number,
                    symbols.len()
                );
            }
        }

        let symbol_refs: Vec<&Bitmap> = symbols.iter().map(|symbol| symbol.as_ref()).collect();

        let mut sym_code_len = 0u32;
        while (1u64 << sym_code_len) < symbols.len() as u64 {
            sym_code_len += 1;
        }

        if self.limits.check_dimensions(region.width, region.height).is_err() {
            log_warn!("JBIG2 text region {}x{} exceeds limits", region.width, region.height);
            return;
        }

        let params = TextRegionParams {
            refine,
            log_strips,
            ref_corner,
            transposed,
            comb_op,
            def_pixel,
            ds_offset,
            rtemplate,
            num_instances,
            rat,
        };

        let mut bitmap = Bitmap::new(region.width, region.height);
        let mut gr_stats = vec![Cx::default(); refine_stats_size(rtemplate)];

        let mut body_end = body.len();
        let mut instance_colors: Option<ColorValues> = None;
        if color_ext {
            if body.len() >= 4 {
                let section_size = u32::from_be_bytes([
                    body[body.len() - 4],
                    body[body.len() - 3],
                    body[body.len() - 2],
                    body[body.len() - 1],
                ]) as usize;
                if section_size >= 4 && section_size <= body.len() {
                    let section_start = body.len() - section_size;
                    instance_colors = color::decode_t45(&body[section_start..body.len() - 4], num_instances as usize);
                    body_end = section_start;
                } else {
                    log_warn!("JBIG2 text region colour section size {} is implausible", section_size);
                }
            }
            if instance_colors.is_none() {
                log_warn!("JBIG2 text region colour section could not be decoded");
            }
        }

        let data = &body[offset.min(body_end)..body_end];
        let mut placements: Option<Vec<Placement>> = if instance_colors.is_some() { Some(Vec::new()) } else { None };

        if huffman {
            let custom_tables = self.collect_tables(header);
            let mut table_index = 0usize;

            let Some(tables) = build_text_huffman_tables(huffman_flags, &custom_tables, &mut table_index) else {
                log_warn!("JBIG2 text region has invalid Huffman table selection");
                return;
            };

            let mut reader = HuffmanReader::new(data);
            let Some(symcodes) = decode_symbol_id_huffman_table(&mut reader, symbols.len().max(1) as u32) else {
                log_warn!("JBIG2 failed to decode symbol ID Huffman table");
                return;
            };

            let tables = TextRegionHuffman { symcodes, ..tables };
            let mut coder = TextCoder::Huffman { reader: &mut reader, tables: &tables };
            decode_text_region_tracked(
                &mut bitmap,
                &symbol_refs,
                &params,
                &mut coder,
                &mut gr_stats,
                placements.as_mut(),
            );
        } else {
            let mut arith = ArithDecoder::new(data);
            let mut contexts = TextArithContexts::new(sym_code_len);
            let mut coder = TextCoder::Arith { arith: &mut arith, contexts: &mut contexts };
            decode_text_region_tracked(
                &mut bitmap,
                &symbol_refs,
                &params,
                &mut coder,
                &mut gr_stats,
                placements.as_mut(),
            );
        }

        if header.segment_type.is_intermediate() {
            self.results.insert(header.number, SegmentResult::Bitmap(Arc::new(bitmap)));
        } else {
            if let (Some(colors), Some(list)) = (instance_colors.as_ref(), placements.as_ref()) {
                let palette = self.palette_for(header);
                self.paint_colors(list, colors, &palette, region.x, region.y);
            }
            self.compose_to_page(&bitmap, region.x, region.y, region.op);
        }
    }

    fn palette_for(&self, header: &SegmentHeader) -> Vec<[u8; 3]> {
        let mut palette: Vec<[u8; 3]> = DEFAULT_PALETTE.to_vec();
        for reference in &header.referred_to {
            if let Some(SegmentResult::Palette(colors)) = self.results.get(reference) {
                palette.extend(colors.iter().copied());
            }
        }
        palette
    }

    fn paint_colors(
        &mut self,
        placements: &[Placement],
        colors: &ColorValues,
        palette: &[[u8; 3]],
        region_x: u32,
        region_y: u32,
    ) {
        let Some(index) = self.current_page else {
            return;
        };
        let page = &mut self.pages[index];
        let Some(plane) = page.color.as_mut() else {
            return;
        };
        let page_width = page.bitmap.width as i64;
        let page_height = page.bitmap.height as i64;

        for (instance, placement) in placements.iter().enumerate() {
            let rgb = match colors {
                ColorValues::Rgb(list) => list.get(instance).copied().unwrap_or([0, 0, 0]),
                ColorValues::PaletteIds(list) => {
                    let id = list.get(instance).copied().unwrap_or(0) as usize;
                    palette.get(id).copied().unwrap_or([0, 0, 0])
                }
            };

            for row in 0..placement.height as i64 {
                for col in 0..placement.width as i64 {
                    if placement.bitmap.get(col, row) == 0 {
                        continue;
                    }
                    let x = placement.x as i64 + col + region_x as i64;
                    let y = placement.y as i64 + row + region_y as i64;
                    if x < 0 || y < 0 || x >= page_width || y >= page_height {
                        continue;
                    }
                    let slot = (y * page_width + x) as usize;
                    if let Some(pixel) = plane.get_mut(slot) {
                        *pixel = rgb;
                    }
                }
            }
        }
    }

    fn handle_halftone_region(&mut self, header: &SegmentHeader, body: &[u8]) {
        let Some(region) = parse_region_info(body) else {
            log_warn!("JBIG2 halftone region segment too short");
            return;
        };

        if body.len() < 18 {
            log_warn!("JBIG2 halftone region segment too short");
            return;
        }

        let flags = body[17];
        let mut offset = 18usize;

        if body.len() < offset + 20 {
            log_warn!("JBIG2 halftone region segment too short for grid information");
            return;
        }

        let grid_width = u32::from_be_bytes([body[offset], body[offset + 1], body[offset + 2], body[offset + 3]]);
        let grid_height = u32::from_be_bytes([body[offset + 4], body[offset + 5], body[offset + 6], body[offset + 7]]);
        let grid_x = i32::from_be_bytes([body[offset + 8], body[offset + 9], body[offset + 10], body[offset + 11]]);
        let grid_y = i32::from_be_bytes([body[offset + 12], body[offset + 13], body[offset + 14], body[offset + 15]]);
        let vector_x = u16::from_be_bytes([body[offset + 16], body[offset + 17]]);
        let vector_y = u16::from_be_bytes([body[offset + 18], body[offset + 19]]);
        offset += 20;

        let mut patterns = None;
        for reference in &header.referred_to {
            if let Some(SegmentResult::Patterns(dict)) = self.results.get(reference) {
                patterns = Some(dict.clone());
                break;
            }
        }

        let Some(patterns) = patterns else {
            log_warn!("JBIG2 halftone region has no pattern dictionary, skipping");
            return;
        };

        if grid_width > 1 << 20 || grid_height > 1 << 20 {
            log_warn!("JBIG2 halftone region declares implausible grid size, skipping");
            return;
        }

        if self.limits.check_dimensions(region.width, region.height).is_err() {
            log_warn!("JBIG2 halftone region {}x{} exceeds limits", region.width, region.height);
            return;
        }

        let params = HalftoneParams {
            mmr: flags & 1 != 0,
            template: (flags >> 1) & 3,
            enable_skip: (flags >> 3) & 1 != 0,
            comb_op: ComposeOp::from_bits((flags >> 4) & 7),
            def_pixel: (flags >> 7) & 1 != 0,
            grid_width,
            grid_height,
            grid_x,
            grid_y,
            vector_x,
            vector_y,
        };

        let mut bitmap = Bitmap::new(region.width, region.height);
        let mut gb_stats = vec![Cx::default(); generic_stats_size(params.template)];

        decode_halftone_region(&body[offset..], &params, &patterns, &mut bitmap, &mut gb_stats);

        if header.segment_type.is_intermediate() {
            self.results.insert(header.number, SegmentResult::Bitmap(Arc::new(bitmap)));
        } else {
            self.compose_to_page(&bitmap, region.x, region.y, region.op);
        }
    }

    fn handle_refinement_region(&mut self, header: &SegmentHeader, body: &[u8]) {
        let Some(region) = parse_region_info(body) else {
            log_warn!("JBIG2 refinement region segment too short");
            return;
        };

        if body.len() < 18 {
            log_warn!("JBIG2 refinement region segment too short");
            return;
        }

        let flags = body[17];
        let template = flags & 1;
        let tpgron = (flags >> 1) & 1 != 0;
        let mut offset = 18usize;

        let mut at = [(0i8, 0i8); 2];
        if template == 0 {
            if body.len() < offset + 4 {
                log_warn!("JBIG2 refinement region segment too short for AT pixels");
                return;
            }
            at[0] = (body[offset] as i8, body[offset + 1] as i8);
            at[1] = (body[offset + 2] as i8, body[offset + 3] as i8);
            offset += 4;
        }

        if self.limits.check_dimensions(region.width, region.height).is_err() {
            log_warn!("JBIG2 refinement region {}x{} exceeds limits", region.width, region.height);
            return;
        }

        let mut reference = None;
        for referred in &header.referred_to {
            if let Some(SegmentResult::Bitmap(bitmap)) = self.results.get(referred) {
                reference = Some(bitmap.clone());
                break;
            }
        }

        let (reference, dx, dy) = match reference {
            Some(bitmap) => (bitmap, 0, 0),
            None => {
                let Some(index) = self.current_page else {
                    log_warn!("JBIG2 refinement region has no reference bitmap");
                    return;
                };
                let page = &self.pages[index];
                let mut extracted = Bitmap::new(region.width, region.height);
                extracted.compose(&page.bitmap, -(region.x as i64), -(region.y as i64), ComposeOp::Replace);
                (Arc::new(extracted), 0, 0)
            }
        };

        let mut bitmap = Bitmap::new(region.width, region.height);
        let params = RefinementParams { template, reference: &reference, dx, dy, tpgron, at };
        let mut stats = vec![Cx::default(); refine_stats_size(template)];
        let mut arith = ArithDecoder::new(&body[offset.min(body.len())..]);

        decode_refinement_region(&mut arith, &mut stats, &mut bitmap, &params);

        if header.segment_type.is_intermediate() {
            self.results.insert(header.number, SegmentResult::Bitmap(Arc::new(bitmap)));
        } else {
            self.compose_to_page(&bitmap, region.x, region.y, ComposeOp::Replace);
        }
    }
}

fn take_table(tables: &[Arc<HuffmanTable>], index: &mut usize) -> Option<HuffmanTable> {
    let table = tables.get(*index)?;
    *index += 1;
    Some(table.as_ref().clone())
}

fn build_text_huffman_tables(flags: u16, custom: &[Arc<HuffmanTable>], index: &mut usize) -> Option<TextRegionHuffman> {
    let fs = match flags & 3 {
        0 => huffman::table_b6(),
        1 => huffman::table_b7(),
        3 => take_table(custom, index)?,
        _ => return None,
    };

    let ds = match (flags >> 2) & 3 {
        0 => huffman::table_b8(),
        1 => huffman::table_b9(),
        2 => huffman::table_b10(),
        _ => take_table(custom, index)?,
    };

    let dt = match (flags >> 4) & 3 {
        0 => huffman::table_b11(),
        1 => huffman::table_b12(),
        2 => huffman::table_b13(),
        _ => take_table(custom, index)?,
    };

    let rdw = match (flags >> 6) & 3 {
        0 => huffman::table_b14(),
        1 => huffman::table_b15(),
        3 => take_table(custom, index)?,
        _ => return None,
    };

    let rdh = match (flags >> 8) & 3 {
        0 => huffman::table_b14(),
        1 => huffman::table_b15(),
        3 => take_table(custom, index)?,
        _ => return None,
    };

    let rdx = match (flags >> 10) & 3 {
        0 => huffman::table_b14(),
        1 => huffman::table_b15(),
        3 => take_table(custom, index)?,
        _ => return None,
    };

    let rdy = match (flags >> 12) & 3 {
        0 => huffman::table_b14(),
        1 => huffman::table_b15(),
        3 => take_table(custom, index)?,
        _ => return None,
    };

    let rsize = if (flags >> 14) & 1 == 0 { huffman::table_b1() } else { take_table(custom, index)? };

    Some(TextRegionHuffman { fs, ds, dt, rdw, rdh, rdx, rdy, rsize, symcodes: HuffmanTable::new(Vec::new()) })
}

fn bitmap_to_image(bitmap: &Bitmap, color: Option<&[[u8; 3]]>) -> Image {
    if let Some(color) = color {
        let mut pixels = Vec::with_capacity(bitmap.width as usize * bitmap.height as usize * 3);
        for y in 0..bitmap.height as i64 {
            for x in 0..bitmap.width as i64 {
                let slot = (y * bitmap.width as i64 + x) as usize;
                let rgb = match color.get(slot) {
                    Some(rgb) if bitmap.get(x, y) != 0 => *rgb,
                    _ if bitmap.get(x, y) != 0 => [0, 0, 0],
                    _ => [255, 255, 255],
                };
                pixels.extend_from_slice(&rgb);
            }
        }

        let mut pixel_data = PixelData::RGB8(pixels);
        pixel_data.correct_pixels(bitmap.width, bitmap.height);
        return Image::from_pixels(bitmap.width, bitmap.height, pixel_data);
    }

    let mut pixels = Vec::with_capacity(bitmap.width as usize * bitmap.height as usize);
    for y in 0..bitmap.height as i64 {
        for x in 0..bitmap.width as i64 {
            pixels.push(1 - bitmap.get(x, y) as u8);
        }
    }

    let mut pixel_data = PixelData::L1(pixels);
    pixel_data.correct_pixels(bitmap.width, bitmap.height);
    Image::from_pixels(bitmap.width, bitmap.height, pixel_data)
}
