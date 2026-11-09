use super::info::{
    CompressedTextChunk, GamaChunk, IccpChunk, IhdrChunk, InternationalTextChunk, PhysChunk, SrgbChunk, TextChunk,
};
use super::types::*;
use crate::log_warn;
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::VexelResult;
use crate::utils::icc::ICCProfile;
use crate::utils::limits::Limits;

const MAX_DECOMPRESSED_ICC: usize = 64 * 1024 * 1024;
const MAX_DECOMPRESSED_TEXT: usize = 16 * 1024 * 1024;
const MAX_KEYWORD_LENGTH: usize = 79;

struct ByteCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        ByteCursor { data, pos: 0 }
    }

    fn u8(&mut self) -> u8 {
        let value = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        value
    }

    fn u16(&mut self) -> u16 {
        u16::from_be_bytes([self.u8(), self.u8()])
    }

    fn u32(&mut self) -> u32 {
        u32::from_be_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    fn null_terminated(&mut self) -> &'a [u8] {
        let rest = self.rest();
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        self.pos += end + 1;
        &rest[..end]
    }
}

fn decompress_ceiling(compressed_len: usize, absolute_max: usize, limits: &Limits) -> usize {
    let ceiling = compressed_len.saturating_mul(1024).max(64 * 1024).min(absolute_max);
    match limits.max_alloc {
        Some(budget) => ceiling.min(budget.min(usize::MAX as u64) as usize),
        None => ceiling,
    }
}

fn sanitized_keyword(bytes: &[u8], chunk_name: &str) -> String {
    if bytes.len() > MAX_KEYWORD_LENGTH {
        log_warn!("{} keyword too long ({} bytes)", chunk_name, bytes.len());
    }

    let sanitized: Vec<u8> = bytes
        .iter()
        .take(MAX_KEYWORD_LENGTH)
        .map(|&byte| {
            if (32..=126).contains(&byte) || byte >= 161 {
                byte
            } else {
                log_warn!("Invalid character in {} keyword: {}, replacing with space", chunk_name, byte);
                b' '
            }
        })
        .collect();

    sanitized.iter().map(|&b| b as char).collect()
}

fn decompress_text(compressed: &[u8], limits: &mut Limits) -> VexelResult<String> {
    let max_decompress = decompress_ceiling(compressed.len(), MAX_DECOMPRESSED_TEXT, limits);
    let text_bytes = ZlibDecoder::from_slice(compressed).decode_with_max(max_decompress);
    if let Err(e) = limits.reserve_usize(text_bytes.len().saturating_mul(3)) {
        log_warn!("Skipping compressed text: {}", e);
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&text_bytes).to_string())
}

pub fn parse_ihdr(data: &[u8]) -> IhdrChunk {
    if data.len() < 13 {
        log_warn!("IHDR chunk too short: {} bytes", data.len());
    }

    let mut cursor = ByteCursor::new(data);
    IhdrChunk {
        width: cursor.u32(),
        height: cursor.u32(),
        bit_depth: cursor.u8(),
        color_type: cursor.u8(),
        compression_method: cursor.u8(),
        filter_method: cursor.u8(),
        interlace_method: cursor.u8(),
    }
}

pub fn parse_plte(data: &[u8]) -> Vec<[u8; 3]> {
    if !data.len().is_multiple_of(3) {
        log_warn!("PLTE chunk length is not a multiple of 3");
    }

    data.chunks_exact(3).map(|rgb| [rgb[0], rgb[1], rgb[2]]).collect()
}

pub fn parse_gama(data: &[u8]) -> GamaChunk {
    let gamma_raw = ByteCursor::new(data).u32();
    GamaChunk { gamma_raw, gamma: gamma_raw as f32 / 100000.0 }
}

pub fn parse_chrm(data: &[u8]) -> Chromaticities {
    if data.len() < 32 {
        log_warn!("cHRM chunk too short: {} bytes", data.len());
    }

    let mut cursor = ByteCursor::new(data);
    let mut next = || cursor.u32() as f32 / 100000.0;
    Chromaticities {
        white_point_x: next(),
        white_point_y: next(),
        red_x: next(),
        red_y: next(),
        green_x: next(),
        green_y: next(),
        blue_x: next(),
        blue_y: next(),
    }
}

pub fn parse_trns(data: &[u8], color_type: ColorType, has_palette: bool) -> Option<TransparencyData> {
    let mut cursor = ByteCursor::new(data);
    match color_type {
        ColorType::Grayscale => {
            if data.len() != 2 {
                log_warn!("Invalid tRNS length for grayscale");
            }
            Some(TransparencyData::Grayscale(cursor.u16()))
        }
        ColorType::RGB => {
            if data.len() != 6 {
                log_warn!("Invalid tRNS length for RGB");
            }
            Some(TransparencyData::RGB(cursor.u16(), cursor.u16(), cursor.u16()))
        }
        ColorType::Indexed => {
            if !has_palette {
                log_warn!("tRNS chunk before PLTE chunk");
            }
            if data.len() > 256 {
                log_warn!("tRNS chunk length {} exceeds palette limit, clamping to 256", data.len());
            }
            Some(TransparencyData::Palette(data[..data.len().min(256)].to_vec()))
        }
        _ => {
            log_warn!("tRNS chunk not allowed for color type {:?}", color_type);
            None
        }
    }
}

pub fn parse_bkgd(data: &[u8], color_type: ColorType, has_palette: bool) -> Option<BackgroundData> {
    let mut cursor = ByteCursor::new(data);
    let (expected, background) = match color_type {
        ColorType::Grayscale | ColorType::GrayscaleAlpha => (2, BackgroundData::Grayscale(cursor.u16())),
        ColorType::RGB | ColorType::RGBA => (6, BackgroundData::RGB(cursor.u16(), cursor.u16(), cursor.u16())),
        ColorType::Indexed => {
            if !has_palette {
                log_warn!("bKGD chunk before PLTE chunk");
            }
            (1, BackgroundData::PaletteIndex(cursor.u8()))
        }
    };

    if data.len() != expected {
        log_warn!("Invalid bKGD length {} for color type {:?}", data.len(), color_type);
    }

    (!data.is_empty()).then_some(background)
}

pub fn parse_phys(data: &[u8]) -> PhysChunk {
    let mut cursor = ByteCursor::new(data);
    PhysChunk { pixels_per_unit_x: cursor.u32(), pixels_per_unit_y: cursor.u32(), unit: cursor.u8() }
}

pub fn parse_sbit(data: &[u8], color_type: ColorType) -> SignificantBits {
    let expected = match color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::RGB | ColorType::Indexed => 3,
        ColorType::RGBA => 4,
    };
    if data.len() != expected {
        log_warn!("Invalid sBIT length {} for color type {:?}", data.len(), color_type);
    }

    let mut cursor = ByteCursor::new(data);
    match color_type {
        ColorType::Grayscale => SignificantBits::Grayscale { gray: cursor.u8() },
        ColorType::RGB => SignificantBits::RGB { red: cursor.u8(), green: cursor.u8(), blue: cursor.u8() },
        ColorType::Indexed => SignificantBits::Indexed { red: cursor.u8(), green: cursor.u8(), blue: cursor.u8() },
        ColorType::GrayscaleAlpha => SignificantBits::GrayscaleAlpha { gray: cursor.u8(), alpha: cursor.u8() },
        ColorType::RGBA => {
            SignificantBits::RGBA { red: cursor.u8(), green: cursor.u8(), blue: cursor.u8(), alpha: cursor.u8() }
        }
    }
}

pub fn parse_hist(data: &[u8], palette_len: Option<usize>) -> Vec<u16> {
    match palette_len {
        None => log_warn!("Encountered hIST chunk before PLTE chunk"),
        Some(len) if data.len() != len * 2 => {
            log_warn!("Invalid hIST length: {}, expected {}", data.len(), len * 2);
        }
        Some(_) => {}
    }

    data.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect()
}

pub fn parse_time(data: &[u8]) -> ImageTime {
    let mut cursor = ByteCursor::new(data);
    let time = ImageTime {
        year: cursor.u16(),
        month: cursor.u8(),
        day: cursor.u8(),
        hour: cursor.u8(),
        minute: cursor.u8(),
        second: cursor.u8(),
    };

    if !(1..=12).contains(&time.month) {
        log_warn!("Invalid month in tIME chunk: {}", time.month);
    }
    if !(1..=31).contains(&time.day) {
        log_warn!("Invalid day in tIME chunk: {}", time.day);
    }
    if time.hour > 23 {
        log_warn!("Invalid hour in tIME chunk: {}", time.hour);
    }
    if time.minute > 59 {
        log_warn!("Invalid minute in tIME chunk: {}", time.minute);
    }
    if time.second > 60 {
        log_warn!("Invalid second in tIME chunk: {}", time.second);
    }

    time
}

pub fn parse_splt(data: &[u8]) -> SuggestedPalette {
    let mut cursor = ByteCursor::new(data);
    let name = sanitized_keyword(cursor.null_terminated(), "sPLT");

    let mut sample_depth = cursor.u8();
    if sample_depth != 8 && sample_depth != 16 {
        log_warn!("Invalid sPLT sample depth: {}, assuming 8", sample_depth);
        sample_depth = 8;
    }

    let entry_size = if sample_depth == 8 { 6 } else { 10 };
    let entries = cursor.rest();
    if !entries.len().is_multiple_of(entry_size) {
        log_warn!("Invalid sPLT chunk length");
    }

    let samples = entries
        .chunks_exact(entry_size)
        .map(|entry| {
            let mut cursor = ByteCursor::new(entry);
            let (red, green, blue, alpha) = if sample_depth == 8 {
                (cursor.u8() as u16, cursor.u8() as u16, cursor.u8() as u16, cursor.u8() as u16)
            } else {
                (cursor.u16(), cursor.u16(), cursor.u16(), cursor.u16())
            };
            SuggestedPaletteSample { red, green, blue, alpha, frequency: cursor.u16() }
        })
        .collect();

    SuggestedPalette { name, sample_depth, samples }
}

pub fn parse_srgb(data: &[u8]) -> SrgbChunk {
    let rendering_intent = ByteCursor::new(data).u8();
    if rendering_intent > 3 {
        log_warn!("Invalid sRGB rendering intent: {}", rendering_intent);
    }
    SrgbChunk { rendering_intent }
}

pub fn parse_text(data: &[u8]) -> TextChunk {
    let mut cursor = ByteCursor::new(data);
    let keyword = String::from_utf8_lossy(cursor.null_terminated()).to_string();
    let text = String::from_utf8_lossy(cursor.rest()).to_string();
    TextChunk { keyword, text }
}

pub fn parse_ztxt(data: &[u8], limits: &mut Limits) -> VexelResult<CompressedTextChunk> {
    let mut cursor = ByteCursor::new(data);
    let keyword = String::from_utf8_lossy(cursor.null_terminated()).to_string();
    let compression_method = cursor.u8();

    if compression_method != 0 {
        log_warn!("Unknown compression method in zTXt chunk: {}", compression_method);
        return Ok(CompressedTextChunk { keyword, compression_method, text: String::new() });
    }

    let text = decompress_text(cursor.rest(), limits)?;
    Ok(CompressedTextChunk { keyword, compression_method, text })
}

pub fn parse_itxt(data: &[u8], limits: &mut Limits) -> VexelResult<InternationalTextChunk> {
    let mut cursor = ByteCursor::new(data);
    let keyword = String::from_utf8_lossy(cursor.null_terminated()).to_string();
    let compressed = cursor.u8() == 1;
    let compression_method = cursor.u8();
    let language_tag = String::from_utf8_lossy(cursor.null_terminated()).to_string();
    let translated_keyword = String::from_utf8_lossy(cursor.null_terminated()).to_string();

    let text = if compressed {
        if compression_method != 0 {
            log_warn!("Invalid compression method in iTXt chunk: {}", compression_method);
        }
        decompress_text(cursor.rest(), limits)?
    } else {
        String::from_utf8_lossy(cursor.rest()).to_string()
    };

    Ok(InternationalTextChunk { keyword, compressed, compression_method, language_tag, translated_keyword, text })
}

pub fn parse_iccp(data: &[u8], limits: &mut Limits) -> VexelResult<(IccpChunk, Option<ICCProfile>)> {
    let mut cursor = ByteCursor::new(data);
    let profile_name = sanitized_keyword(cursor.null_terminated(), "iCCP");
    let compression_method = cursor.u8();

    if compression_method != 0 {
        log_warn!("Invalid compression method in iCCP chunk: {}", compression_method);
    }

    let compressed = cursor.rest();
    let max_decompress = decompress_ceiling(compressed.len(), MAX_DECOMPRESSED_ICC, limits);
    let profile_data = ZlibDecoder::from_slice(compressed).decode_with_max(max_decompress);

    let profile = match limits.reserve_usize(profile_data.len()) {
        Err(e) => {
            log_warn!("Skipping iCCP profile: {}", e);
            None
        }
        Ok(()) => match ICCProfile::new(&profile_data) {
            Ok(profile) => Some(profile),
            Err(e) => {
                log_warn!("Failed to parse iCCP profile: {}", e);
                None
            }
        },
    };

    let chunk = IccpChunk {
        profile_name,
        compression_method,
        compressed_length: compressed.len() as u64,
        profile_length: (!profile_data.is_empty()).then_some(profile_data.len() as u64),
        profile_parsed: profile.is_some(),
    };

    Ok((chunk, profile))
}

pub fn parse_actl(data: &[u8]) -> ActlChunk {
    let mut cursor = ByteCursor::new(data);
    let actl = ActlChunk { num_frames: cursor.u32(), num_plays: cursor.u32() };
    if actl.num_frames == 0 {
        log_warn!("acTL chunk with zero frames");
    }
    actl
}

pub fn parse_fctl(data: &[u8]) -> FctlChunk {
    let mut cursor = ByteCursor::new(data);
    FctlChunk {
        sequence_number: cursor.u32(),
        width: cursor.u32(),
        height: cursor.u32(),
        x_offset: cursor.u32(),
        y_offset: cursor.u32(),
        delay_num: cursor.u16(),
        delay_den: cursor.u16(),
        dispose_op: cursor.u8(),
        blend_op: cursor.u8(),
    }
}

pub fn sanitize_fctl(fctl: &FctlChunk, width: u32, height: u32) -> FctlChunk {
    let mut sanitized = fctl.clone();

    if fctl.x_offset.saturating_add(fctl.width) > width {
        log_warn!(format!(
            "fcTL width would overflow actual image width, clamping: x_offset={}, width={}, image_width={}",
            fctl.x_offset, fctl.width, width
        ));
        sanitized.width = width.saturating_sub(fctl.x_offset);
    }

    if fctl.y_offset.saturating_add(fctl.height) > height {
        log_warn!(format!(
            "fcTL height would overflow actual image height, clamping: y_offset={}, height={}, image_height={}",
            fctl.y_offset, fctl.height, height
        ));
        sanitized.height = height.saturating_sub(fctl.y_offset);
    }

    if sanitized.width == 0 || sanitized.height == 0 {
        log_warn!(format!("Invalid fcTL parameters: width={}, height={}", sanitized.width, sanitized.height));
    }

    if fctl.dispose_op > 2 {
        log_warn!(format!("Invalid fcTL dispose_op: {}", fctl.dispose_op));
        sanitized.dispose_op = 0;
    }

    if fctl.blend_op > 1 {
        log_warn!(format!("Invalid fcTL blend_op: {}", fctl.blend_op));
        sanitized.blend_op = 0;
    }

    sanitized
}
