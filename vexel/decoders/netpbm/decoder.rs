use std::io::{Read, Seek};

use super::info::{NetpbmComment, NetpbmEncoding, NetpbmHeader, NetpbmInfo, NetpbmPixelData, NetpbmSection};
use super::input::Input;
use super::raster::{self, Samples};
use super::types::{NetpbmFormat, TupleType};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::format_detect::is_netpbm_space;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary, SampleFormat};
use crate::{log_warn, Image, Limits, PixelData};

#[derive(Clone, Copy, PartialEq)]
enum WalkMode {
    InfoOnly,
    DecodeFirst,
    DecodeAll,
}

const SKIP_CHUNK: usize = 1 << 14;
const MAX_SCALE_TOKEN: usize = 64;

pub struct NetPbmDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    max_value: u32,
    depth: u32,
    format: NetpbmFormat,
    tuple_type: Option<TupleType>,
    tuple_type_raw: Option<String>,
    raw_max_value: Option<u32>,
    raw_depth: Option<u32>,
    scale: Option<f64>,
    delimiter: Option<u8>,
    input: Input<R>,
    image_index: u32,
    collected: NetpbmInfo,
    info: Option<NetpbmInfo>,
}

const DIGIT: u8 = 0;
const SPACE: u8 = 1;
const HASH: u8 = 2;
const OTHER: u8 = 3;

const BYTE_CLASS: [u8; 256] = {
    let mut table = [OTHER; 256];
    let mut byte = 0;
    while byte < 256 {
        table[byte] = match byte as u8 {
            b'0'..=b'9' => DIGIT,
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => SPACE,
            b'#' => HASH,
            _ => OTHER,
        };
        byte += 1;
    }
    table
};

#[inline(always)]
fn parse_digits(chunk: [u8; 8]) -> (u32, usize) {
    let word = u64::from_le_bytes(chunk);
    let digits = word ^ 0x3030_3030_3030_3030;
    let non_digit = (digits.wrapping_add(0x7676_7676_7676_7676) | digits) & 0x8080_8080_8080_8080;
    let count = (non_digit.trailing_zeros() / 8) as usize;
    if count == 0 || count == 8 {
        return (0, count);
    }

    let aligned = digits << (64 - 8 * count);
    let pairs = aligned.wrapping_mul(10).wrapping_add(aligned >> 8);
    let mask = 0x0000_00ff_0000_00ff;
    let value = ((pairs & mask)
        .wrapping_mul(0x000f_4240_0000_0064)
        .wrapping_add(((pairs >> 16) & mask).wrapping_mul(0x0000_2710_0000_0001)))
        >> 32;
    (value as u32, count)
}

const BLOCK: usize = 64;
const BLOCK_SLACK: usize = BLOCK + 8;

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn classify_block(block: &[u8; BLOCK]) -> (u64, u64) {
    use std::arch::x86_64::*;

    let mut digits = 0u64;
    let mut spaces = 0u64;
    unsafe {
        let below_zero = _mm_set1_epi8(b'0' as i8 - 1);
        let above_nine = _mm_set1_epi8(b'9' as i8 + 1);
        let below_tab = _mm_set1_epi8(b'\t' as i8 - 1);
        let above_cr = _mm_set1_epi8(b'\r' as i8 + 1);
        let space = _mm_set1_epi8(b' ' as i8);
        for lane in 0..BLOCK / 16 {
            let v = _mm_loadu_si128(block.as_ptr().add(lane * 16).cast());
            let digit = _mm_and_si128(_mm_cmpgt_epi8(v, below_zero), _mm_cmpgt_epi8(above_nine, v));
            let control = _mm_and_si128(_mm_cmpgt_epi8(v, below_tab), _mm_cmpgt_epi8(above_cr, v));
            let blank = _mm_or_si128(control, _mm_cmpeq_epi8(v, space));
            digits |= u64::from(_mm_movemask_epi8(digit) as u16) << (lane * 16);
            spaces |= u64::from(_mm_movemask_epi8(blank) as u16) << (lane * 16);
        }
    }
    (digits, spaces)
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn classify_block(block: &[u8; BLOCK]) -> (u64, u64) {
    use std::arch::wasm32::*;

    let mut digits = 0u64;
    let mut spaces = 0u64;
    for lane in 0..BLOCK / 16 {
        let v = unsafe { v128_load(block.as_ptr().add(lane * 16).cast()) };
        let digit = v128_and(u8x16_ge(v, u8x16_splat(b'0')), u8x16_le(v, u8x16_splat(b'9')));
        let control = v128_and(u8x16_ge(v, u8x16_splat(b'\t')), u8x16_le(v, u8x16_splat(b'\r')));
        let blank = v128_or(control, u8x16_eq(v, u8x16_splat(b' ')));
        digits |= u64::from(u8x16_bitmask(digit)) << (lane * 16);
        spaces |= u64::from(u8x16_bitmask(blank)) << (lane * 16);
    }
    (digits, spaces)
}

#[cfg(not(any(target_arch = "x86_64", all(target_arch = "wasm32", target_feature = "simd128"))))]
#[inline(always)]
fn classify_block(block: &[u8; BLOCK]) -> (u64, u64) {
    const LOW: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    const GATHER: u64 = 0x0102_0408_1020_4080;

    let in_range = |word: u64, first: u8, last: u8| {
        let low_bits = word & (LOW * 127);
        let below_end = (LOW * (128 + u64::from(last))).wrapping_sub(low_bits);
        let above_start = low_bits.wrapping_add(LOW * (128 - u64::from(first)));
        below_end & above_start & !word & HIGH
    };
    let compress = |flags: u64| (flags >> 7).wrapping_mul(GATHER) >> 56;

    let mut digits = 0u64;
    let mut spaces = 0u64;
    for (lane, chunk) in block.chunks_exact(8).enumerate() {
        let word = u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
        let digit = in_range(word, b'0', b'9');
        let blank = in_range(word, b'\t', b'\r') | in_range(word, b' ', b' ');
        digits |= compress(digit) << (lane * 8);
        spaces |= compress(blank) << (lane * 8);
    }
    (digits, spaces)
}

#[inline(always)]
fn parse_block<T: Copy>(
    buf: &[u8],
    start: usize,
    out: &mut [T],
    n: &mut usize,
    map: &impl Fn(u32) -> T,
) -> Option<usize> {
    let block: &[u8; BLOCK] = buf.get(start..start + BLOCK)?.try_into().ok()?;
    if buf.len() < start + BLOCK_SLACK {
        return None;
    }
    let (digits, spaces) = classify_block(block);
    if digits | spaces != u64::MAX {
        return None;
    }

    let mut starts = digits & !(digits << 1);
    let mut end = start + BLOCK;
    while starts != 0 {
        let pos = start + starts.trailing_zeros() as usize;
        starts &= starts - 1;
        let chunk: [u8; 8] = buf[pos..pos + 8].try_into().ok()?;
        let (value, count) = parse_digits(chunk);
        if count == 8 {
            return Some(pos);
        }
        out[*n] = map(value);
        *n += 1;
        end = end.max(pos + count);
        if *n == out.len() {
            return Some(pos + count);
        }
    }

    Some(end)
}

fn pfm_little_endian(scale: f64) -> bool {
    let big_endian = scale > 0.0;
    !big_endian
}

impl<R: Read + Seek> NetPbmDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            max_value: 0,
            depth: 0,
            format: NetpbmFormat::P6,
            tuple_type: None,
            tuple_type_raw: None,
            raw_max_value: None,
            raw_depth: None,
            scale: None,
            delimiter: None,
            input: Input::new(reader),
            image_index: 0,
            collected: NetpbmInfo::default(),
            info: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> NetpbmInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk(WalkMode::InfoOnly) {
                log_warn!("Failed to read Netpbm structure: {}", e);
            }
            self.limits = saved_limits;
            self.finish_info();
        }

        self.info.clone().unwrap_or_default()
    }

    fn finish_info(&mut self) {
        let mut info = std::mem::take(&mut self.collected);
        info.sections.sort_by_key(|section| section.offset);

        let image_count = info.sections.iter().filter(|s| matches!(s.data, NetpbmSection::Header(_))).count();

        let first_header = info.sections.iter().find_map(|section| match &section.data {
            NetpbmSection::Header(header) => Some(header.clone()),
            _ => None,
        });
        let first_pixels = info.sections.iter().find_map(|section| match &section.data {
            NetpbmSection::PixelData(pixel_data) if pixel_data.image == 0 => {
                Some((pixel_data.clone(), section.length.unwrap_or(0)))
            }
            _ => None,
        });

        if let Some(header) = first_header {
            info.summary = Some(Self::build_summary(&header, first_pixels, image_count as u32));
        }

        self.info = Some(info);
    }

    fn build_summary(header: &NetpbmHeader, pixels: Option<(NetpbmPixelData, u64)>, image_count: u32) -> ImageSummary {
        let max_value = match header.max_value {
            None => 1,
            Some(0) => 255,
            Some(value) => value.min(65535),
        };
        let bits = (32 - max_value.leading_zeros()) as u8;

        let (color_model, channels) = match header.format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => (ColorModel::Bilevel, 1),
            NetpbmFormat::P2 | NetpbmFormat::P5 | NetpbmFormat::PfmGray => (ColorModel::Gray, 1),
            NetpbmFormat::P3 | NetpbmFormat::P6 | NetpbmFormat::PfmColor => (ColorModel::Rgb, 3),
            NetpbmFormat::P7 => {
                let depth = header.depth.filter(|&d| d > 0).unwrap_or(3);
                let model = match (&header.tuple_type, depth) {
                    (Some(TupleType::BlackAndWhite), _) => ColorModel::Bilevel,
                    (Some(TupleType::Grayscale), _) => ColorModel::Gray,
                    (Some(TupleType::RGB), _) => ColorModel::Rgb,
                    (Some(TupleType::BlackAndWhiteAlpha | TupleType::GrayscaleAlpha), _) => ColorModel::GrayAlpha,
                    (Some(TupleType::RGBAlpha), _) => ColorModel::Rgba,
                    (Some(TupleType::CMYK), _) => ColorModel::Cmyk,
                    (Some(TupleType::CMYKAlpha), _) => ColorModel::Cmyka,
                    (None, 1) => ColorModel::Gray,
                    (None, 2) => ColorModel::GrayAlpha,
                    (None, 3) => ColorModel::Rgb,
                    (None, 4) => ColorModel::Rgba,
                    (None, _) => ColorModel::Multichannel,
                };
                (model, depth.min(u32::from(u16::MAX)) as u16)
            }
        };

        let is_pfm = header.format.is_pfm();
        let mut summary =
            ImageSummary::new(header.width, header.height, color_model, channels, if is_pfm { 32 } else { bits });
        if is_pfm {
            summary.sample_format = SampleFormat::Float;
        }
        summary.has_alpha = matches!(color_model, ColorModel::GrayAlpha | ColorModel::Rgba | ColorModel::Cmyka);
        if image_count > 1 {
            summary.pages = Some(image_count);
        }
        if let Some((pixel_data, actual)) = pixels {
            if let Some(expected) = pixel_data.expected_length {
                summary.data = Some(DataExtent { expected, actual });
            }
        }

        summary
    }

    fn read_comment(&mut self) {
        let offset = self.input.position();
        self.input.consume(1);
        let mut text = String::new();
        while let Some(byte) = self.input.next() {
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            text.push(byte as char);
        }
        let end = self.input.position();
        let image = self.image_index;
        self.collected.push(
            offset,
            Some(end.saturating_sub(offset)),
            NetpbmSection::Comment(NetpbmComment { image, text }),
        );
    }

    fn skip_space(&mut self) -> VexelResult<u8> {
        loop {
            if !self.input.fill() {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
            }
            let buffered = self.input.buffered();
            match buffered.iter().position(|&b| !is_netpbm_space(b)) {
                Some(i) => {
                    let byte = buffered[i];
                    self.input.consume(i);
                    if byte != b'#' {
                        return Ok(byte);
                    }
                    self.read_comment();
                }
                None => {
                    let len = buffered.len();
                    self.input.consume(len);
                }
            }
        }
    }

    fn read_number(&mut self) -> VexelResult<u32> {
        let first = self.skip_space()?;
        self.input.consume(1);
        let mut number = match first {
            b'0'..=b'9' => u32::from(first - b'0'),
            _ => {
                log_warn!("Expected a digit, got byte: {}", first);
                0
            }
        };

        let mut overflowed = false;
        while let Some(byte @ b'0'..=b'9') = self.input.peek() {
            self.input.consume(1);
            match number.checked_mul(10).and_then(|n| n.checked_add(u32::from(byte - b'0'))) {
                Some(n) => number = n,
                None => overflowed = true,
            }
        }
        if overflowed {
            log_warn!("Number is too large, keeping {}", number);
        }

        Ok(number)
    }

    fn read_token(&mut self, max_len: usize) -> VexelResult<String> {
        let first = self.skip_space()?;
        let mut token = String::new();
        token.push(first as char);
        self.input.consume(1);
        while let Some(byte) = self.input.peek() {
            if is_netpbm_space(byte) || token.len() >= max_len {
                break;
            }
            token.push(byte as char);
            self.input.consume(1);
        }
        Ok(token)
    }

    fn read_delimiter(&mut self) {
        self.delimiter = match self.input.peek() {
            Some(byte) if is_netpbm_space(byte) => {
                self.input.consume(1);
                Some(byte)
            }
            _ => None,
        };
    }

    fn reset_frame_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.max_value = 0;
        self.depth = 0;
        self.format = NetpbmFormat::P6;
        self.tuple_type = None;
        self.tuple_type_raw = None;
        self.raw_max_value = None;
        self.raw_depth = None;
        self.scale = None;
        self.delimiter = None;
    }

    fn read_header(&mut self, mode: WalkMode) -> VexelResult<()> {
        let header_start = self.input.position();
        let eof = || VexelError::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));

        let b0 = self.input.next().ok_or_else(eof)?;
        let b1 = self.input.next().ok_or_else(eof)?;

        let mut magic_valid = true;
        let format = match (b0, b1) {
            (b'P', b'1') => NetpbmFormat::P1,
            (b'P', b'2') => NetpbmFormat::P2,
            (b'P', b'3') => NetpbmFormat::P3,
            (b'P', b'4') => NetpbmFormat::P4,
            (b'P', b'5') => NetpbmFormat::P5,
            (b'P', b'6') => NetpbmFormat::P6,
            (b'P', b'7') => NetpbmFormat::P7,
            (b'P', b'F') => NetpbmFormat::PfmColor,
            (b'P', b'f') => NetpbmFormat::PfmGray,
            _ => {
                log_warn!("Invalid magic number: {}", (u16::from(b0) << 8) | u16::from(b1));
                magic_valid = false;
                NetpbmFormat::P6
            }
        };

        self.format = format.clone();

        let parsed = match format {
            NetpbmFormat::P7 => self.read_pam_header(),
            NetpbmFormat::PfmColor | NetpbmFormat::PfmGray => self.read_pfm_header(),
            _ => self.read_standard_header(),
        };

        let header_end = self.input.position();
        self.collected.push(
            header_start,
            Some(header_end.saturating_sub(header_start)),
            NetpbmSection::Header(NetpbmHeader {
                image: self.image_index,
                format,
                magic_valid,
                width: self.width,
                height: self.height,
                max_value: self.raw_max_value,
                depth: self.raw_depth,
                tuple_type: self.tuple_type.clone(),
                tuple_type_raw: self.tuple_type_raw.clone(),
                scale: self.scale,
            }),
        );

        parsed?;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        if mode != WalkMode::InfoOnly {
            self.limits.reserve_buffer(self.width, self.height, self.output_bytes_per_pixel())?;
        }

        Ok(())
    }

    fn output_bytes_per_pixel(&self) -> u8 {
        let bytes_per_sample = if self.max_value > 255 { 2 } else { 1 };
        match self.format {
            NetpbmFormat::P1 | NetpbmFormat::P4 => 1,
            NetpbmFormat::P2 | NetpbmFormat::P5 => bytes_per_sample,
            NetpbmFormat::P3 | NetpbmFormat::P6 => 3 * bytes_per_sample,
            NetpbmFormat::P7 => self.depth.min(4) as u8 * bytes_per_sample,
            NetpbmFormat::PfmColor => 12,
            NetpbmFormat::PfmGray => 4,
        }
    }

    fn read_standard_header(&mut self) -> VexelResult<()> {
        self.width = self.read_number()?;
        self.height = self.read_number()?;

        if matches!(self.format, NetpbmFormat::P1 | NetpbmFormat::P4) {
            self.max_value = 1;
        } else {
            self.max_value = self.read_number()?;
            self.raw_max_value = Some(self.max_value);

            if self.max_value == 0 {
                log_warn!("Invalid MAXVAL value: {}", self.max_value);
                self.max_value = 255;
            }

            if self.max_value > 65535 {
                log_warn!("Invalid MAXVAL value: {}", self.max_value);
                self.max_value = 65535;
            }
        }

        self.read_delimiter();
        Ok(())
    }

    fn read_pfm_header(&mut self) -> VexelResult<()> {
        self.width = self.read_number()?;
        self.height = self.read_number()?;

        let token = self.read_token(MAX_SCALE_TOKEN)?;
        let scale = match token.parse::<f64>() {
            Ok(value) if value != 0.0 && value.is_finite() => value,
            Ok(value) => {
                log_warn!("Invalid PFM scale {}, assuming little-endian samples", value);
                value
            }
            Err(_) => {
                log_warn!("Unreadable PFM scale {:?}, assuming little-endian samples", token);
                self.read_delimiter();
                return Ok(());
            }
        };
        self.scale = Some(scale);

        self.read_delimiter();
        Ok(())
    }

    fn read_pam_line(&mut self) -> (String, String) {
        let mut key = String::new();
        let mut value = String::new();
        let mut reading_key = true;

        while let Some(byte) = self.input.next() {
            match byte {
                b'\n' => break,
                b' ' | b'\t' if reading_key => reading_key = false,
                _ if reading_key => key.push(byte as char),
                _ => value.push(byte as char),
            }
        }

        (key.trim().to_string(), value.trim().to_string())
    }

    fn read_pam_header(&mut self) -> VexelResult<()> {
        loop {
            self.skip_space()?;
            let (key, value) = self.read_pam_line();

            match key.as_str() {
                "ENDHDR" => break,
                "WIDTH" => {
                    self.width = value.parse::<u32>().unwrap_or_else(|_| {
                        log_warn!("Invalid WIDTH value: {}", value);
                        0
                    })
                }
                "HEIGHT" => {
                    self.height = value.parse::<u32>().unwrap_or_else(|_| {
                        log_warn!("Invalid HEIGHT value: {}", value);
                        0
                    })
                }
                "DEPTH" => {
                    self.raw_depth = value.parse::<u32>().ok();
                    self.depth = value.parse::<u8>().map(u32::from).unwrap_or_else(|_| {
                        log_warn!("Invalid DEPTH value: {}", value);
                        3
                    })
                }
                "MAXVAL" => {
                    self.raw_max_value = value.parse::<u32>().ok();
                    self.max_value = value.parse().unwrap_or_else(|_| {
                        log_warn!("Invalid MAXVAL value: {}", value);
                        255
                    })
                }
                "TUPLTYPE" => match &mut self.tuple_type_raw {
                    Some(existing) => {
                        existing.push(' ');
                        existing.push_str(&value);
                    }
                    None => self.tuple_type_raw = Some(value),
                },
                _ => {}
            }
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        };

        if self.depth == 0 {
            self.depth = 3;
        }
        if self.max_value == 0 {
            self.max_value = 255;
        }
        if self.max_value > 65535 {
            log_warn!("Invalid MAXVAL value: {}", self.max_value);
            self.max_value = 65535;
        }

        self.tuple_type = self.tuple_type_raw.as_deref().and_then(|s| match s {
            "BLACKANDWHITE" => Some(TupleType::BlackAndWhite),
            "GRAYSCALE" => Some(TupleType::Grayscale),
            "RGB" => Some(TupleType::RGB),
            "BLACKANDWHITE_ALPHA" => Some(TupleType::BlackAndWhiteAlpha),
            "GRAYSCALE_ALPHA" => Some(TupleType::GrayscaleAlpha),
            "RGB_ALPHA" => Some(TupleType::RGBAlpha),
            "CMYK" => Some(TupleType::CMYK),
            "CMYK_ALPHA" => Some(TupleType::CMYKAlpha),
            _ => {
                log_warn!("Unrecognized TUPLTYPE value: {}", s);
                None
            }
        });

        Ok(())
    }

    fn channels(&self) -> usize {
        match self.format {
            NetpbmFormat::P1 | NetpbmFormat::P2 | NetpbmFormat::P4 | NetpbmFormat::P5 | NetpbmFormat::PfmGray => 1,
            NetpbmFormat::P3 | NetpbmFormat::P6 | NetpbmFormat::PfmColor => 3,
            NetpbmFormat::P7 => self.depth as usize,
        }
    }

    fn pixel_layout(&self) -> NetpbmPixelData {
        let pixels = u64::from(self.width) * u64::from(self.height);
        let samples = pixels.saturating_mul(self.channels() as u64);
        let bytes_per_sample = if self.max_value > 255 { 2u8 } else { 1u8 };

        let (encoding, bytes_per_sample, expected_length) = match self.format {
            NetpbmFormat::P1 | NetpbmFormat::P2 | NetpbmFormat::P3 => (NetpbmEncoding::Plain, None, None),
            NetpbmFormat::P4 => {
                let expected = u64::from(self.width.div_ceil(8)) * u64::from(self.height);
                (NetpbmEncoding::Raw, None, Some(expected))
            }
            NetpbmFormat::P5 | NetpbmFormat::P6 | NetpbmFormat::P7 => {
                (NetpbmEncoding::Raw, Some(bytes_per_sample), Some(samples.saturating_mul(u64::from(bytes_per_sample))))
            }
            NetpbmFormat::PfmColor | NetpbmFormat::PfmGray => {
                (NetpbmEncoding::Raw, Some(4), Some(samples.saturating_mul(4)))
            }
        };

        NetpbmPixelData { image: self.image_index, encoding, samples, bytes_per_sample, expected_length }
    }

    fn skip_crlf_delimiter(&mut self, layout: &NetpbmPixelData) {
        let Some(expected) = layout.expected_length else {
            return;
        };
        if self.delimiter != Some(b'\r') || self.input.peek() != Some(b'\n') {
            return;
        }
        if self.input.remaining() == Some(expected + 1) {
            self.input.consume(1);
        }
    }

    fn skip_frame(&mut self, layout: &NetpbmPixelData) {
        match layout.expected_length {
            Some(expected) => self.input.skip(expected),
            None => {
                let mut scratch = vec![0u8; SKIP_CHUNK];
                let mut remaining = layout.samples;
                while remaining > 0 {
                    let chunk = &mut scratch[..remaining.min(SKIP_CHUNK as u64) as usize];
                    let wanted = chunk.len();
                    let parsed = match self.format {
                        NetpbmFormat::P1 => self.parse_plain_bits(chunk),
                        _ => self.parse_plain(chunk, |_| 0),
                    };
                    if parsed < wanted {
                        break;
                    }
                    remaining -= wanted as u64;
                }
            }
        }
    }

    fn available_count(&mut self, count: usize, bytes_per_sample: usize) -> usize {
        let available = self.input.remaining().unwrap_or(u64::MAX) / bytes_per_sample as u64;
        count.min(usize::try_from(available).unwrap_or(usize::MAX))
    }

    fn warn_truncated(expected: usize, got: usize) {
        log_warn!("Truncated Netpbm pixel data: expected {} bytes, got {}", expected, got);
    }

    fn read_bytes(&mut self, dst: &mut [u8], sample_size: usize) {
        let got = self.input.read_into(dst);
        if got < dst.len() {
            Self::warn_truncated(dst.len(), got);
            let partial = got % sample_size;
            dst[got - partial..got].fill(0);
        }
    }

    fn read_raw_samples(&mut self, count: usize, wide: bool) -> Samples {
        let mut samples = Samples::zeroed(count, wide);
        match &mut samples {
            Samples::U8(data) => self.read_bytes(data, 1),
            Samples::U16(data) => {
                self.read_bytes(raster::u16_bytes_mut(data), 2);
                raster::finish_u16_be(data, 65535);
            }
        }
        samples
    }

    fn read_scaled_samples(&mut self, count: usize) -> Samples {
        let max_value = self.max_value;
        let mut samples = Samples::zeroed(count, max_value > 255);
        match &mut samples {
            Samples::U8(data) => {
                self.read_bytes(data, 1);
                raster::finish_u8(data, max_value);
            }
            Samples::U16(data) => {
                self.read_bytes(raster::u16_bytes_mut(data), 2);
                raster::finish_u16_be(data, max_value);
            }
        }
        samples
    }

    fn parse_plain<T: Copy>(&mut self, out: &mut [T], map: impl Fn(u32) -> T) -> usize {
        let mut n = 0;
        let mut pending: Option<u32> = None;
        let mut warned = false;

        while n < out.len() {
            if !self.input.fill() {
                break;
            }
            let buf = self.input.buffered();
            let len = buf.len();
            let mut i = 0;
            let mut comment = false;

            if let Some(mut value) = pending.take() {
                while i < len {
                    let digit = buf[i].wrapping_sub(b'0');
                    if digit > 9 {
                        break;
                    }
                    value = value.saturating_mul(10).saturating_add(u32::from(digit));
                    i += 1;
                }
                if i == len {
                    pending = Some(value);
                    self.input.consume(i);
                    continue;
                }
                out[n] = map(value);
                n += 1;
            }

            while i < len && n < out.len() {
                if let Some(next) = parse_block(buf, i, out, &mut n, &map) {
                    if next > i {
                        i = next;
                        continue;
                    }
                }
                let byte = buf[i];
                let class = BYTE_CLASS[byte as usize];
                if class == SPACE {
                    i += 1;
                    continue;
                }
                if class == HASH {
                    comment = true;
                    break;
                }
                if class == DIGIT {
                    if let Some(chunk) = buf.get(i..i + 8).and_then(|c| <[u8; 8]>::try_from(c).ok()) {
                        let (value, count) = parse_digits(chunk);
                        if count < 8 {
                            out[n] = map(value);
                            n += 1;
                            i += count;
                            continue;
                        }
                    }
                }

                let mut value = if class == DIGIT {
                    u32::from(byte - b'0')
                } else {
                    if !warned {
                        log_warn!("Unexpected byte {} in plain Netpbm data", byte);
                        warned = true;
                    }
                    0
                };
                i += 1;
                while i < len {
                    let digit = buf[i].wrapping_sub(b'0');
                    if digit > 9 {
                        break;
                    }
                    value = value.saturating_mul(10).saturating_add(u32::from(digit));
                    i += 1;
                }
                if i == len {
                    pending = Some(value);
                    break;
                }
                out[n] = map(value);
                n += 1;
            }

            self.input.consume(i);
            if comment {
                self.read_comment();
            }
        }

        if let Some(value) = pending {
            if n < out.len() {
                out[n] = map(value);
                n += 1;
            }
        }

        n
    }

    fn parse_plain_bits(&mut self, out: &mut [u8]) -> usize {
        let mut n = 0;
        let mut warned = false;

        while n < out.len() {
            if !self.input.fill() {
                break;
            }
            let buf = self.input.buffered();
            let mut i = 0;
            let mut comment = false;

            while i < buf.len() && n < out.len() {
                let byte = buf[i];
                match BYTE_CLASS[byte as usize] {
                    DIGIT => {
                        out[n] = if byte == b'0' { 255 } else { 0 };
                        n += 1;
                    }
                    HASH => {
                        comment = true;
                        break;
                    }
                    SPACE => {}
                    _ => {
                        if !warned {
                            log_warn!("Unexpected byte {} in plain PBM data", byte);
                            warned = true;
                        }
                        out[n] = 255;
                        n += 1;
                    }
                }
                i += 1;
            }

            self.input.consume(i);
            if comment {
                self.read_comment();
            }
        }

        n
    }

    fn warn_plain_truncated(expected: usize, got: usize) {
        if got < expected {
            log_warn!("Truncated plain Netpbm data: expected {} samples, got {}", expected, got);
        }
    }

    fn decode_plain_bitmap(&mut self, pixels: usize) -> PixelData {
        let mut data = vec![255u8; pixels];
        let parsed = self.parse_plain_bits(&mut data);
        Self::warn_plain_truncated(pixels, parsed);
        PixelData::L8(data)
    }

    fn decode_plain(&mut self, count: usize, channels: usize) -> PixelData {
        let max_value = self.max_value;
        let (samples, parsed) = if max_value > 255 {
            let mut data = vec![0u16; count];
            let parsed = if max_value == 65535 {
                self.parse_plain(&mut data, |v| v.min(65535) as u16)
            } else {
                let lut = raster::lut_u16(max_value);
                self.parse_plain(&mut data, |v| lut[v.min(max_value) as u16 as usize])
            };
            (Samples::U16(data), parsed)
        } else {
            let mut data = vec![0u8; count];
            let parsed = if max_value == 255 {
                self.parse_plain(&mut data, |v| v.min(255) as u8)
            } else {
                let lut = raster::lut_u8(max_value);
                self.parse_plain(&mut data, |v| lut[v.min(max_value) as u8 as usize])
            };
            (Samples::U8(data), parsed)
        };
        Self::warn_plain_truncated(count, parsed);
        samples.into_pixel_data(channels)
    }

    fn decode_raw_bitmap(&mut self, pixels: usize) -> PixelData {
        let width = self.width as usize;
        let expected = width.div_ceil(8).saturating_mul(self.height as usize);
        let mut packed = vec![0u8; self.available_count(expected, 1)];
        let got = self.input.read_into(&mut packed);
        if got < expected {
            Self::warn_truncated(expected, got);
        }

        let mut data = vec![0u8; pixels];
        raster::unpack_bits(&packed[..got], &mut data, width);
        PixelData::L8(data)
    }

    fn decode_pfm(&mut self, pixels: usize, channels: usize) -> PixelData {
        let width = self.width as usize;
        let height = self.height as usize;
        let row_bytes = width * channels * 4;
        let mut data = vec![0f32; pixels * channels];

        let bytes = raster::f32_bytes_mut(&mut data);
        for y in (0..height).rev() {
            let row = &mut bytes[y * row_bytes..(y + 1) * row_bytes];
            let got = self.input.read_into(row);
            if got < row_bytes {
                let read = (height - 1 - y) * row_bytes + got;
                Self::warn_truncated(height * row_bytes, read);
                row[got - got % 4..got].fill(0);
                break;
            }
        }

        raster::finish_f32(&mut data, pfm_little_endian(self.scale.unwrap_or(-1.0)));

        match channels {
            1 => PixelData::L32F(data),
            _ => PixelData::RGB32F(data),
        }
    }

    fn decode_pam(&mut self, pixels: usize) -> PixelData {
        let depth = self.depth as usize;
        let max_value = self.max_value;
        let wide = max_value > 255;

        match (&self.tuple_type, depth) {
            (Some(TupleType::BlackAndWhite), 1) => {
                let threshold = max_value / 2;
                let data = match self.read_raw_samples(pixels, wide) {
                    Samples::U8(data) => {
                        data.into_iter().map(|v| if u32::from(v) > threshold { 255 } else { 0 }).collect()
                    }
                    Samples::U16(data) => {
                        data.into_iter().map(|v| if u32::from(v) > threshold { 255 } else { 0 }).collect()
                    }
                };
                PixelData::L8(data)
            }

            (Some(TupleType::BlackAndWhiteAlpha), 2) => {
                let threshold = max_value / 2;
                let alpha_lut = raster::lut_u8(max_value);
                let pair = |value: u32, alpha: u32| {
                    let alpha =
                        if wide { raster::scale_value(alpha, max_value, 255) as u8 } else { alpha_lut[alpha as usize] };
                    [if value > threshold { 255 } else { 0 }, alpha]
                };
                let data = match self.read_raw_samples(pixels * 2, wide) {
                    Samples::U8(data) => {
                        data.chunks_exact(2).flat_map(|p| pair(u32::from(p[0]), u32::from(p[1]))).collect()
                    }
                    Samples::U16(data) => {
                        data.chunks_exact(2).flat_map(|p| pair(u32::from(p[0]), u32::from(p[1]))).collect()
                    }
                };
                PixelData::LA8(data)
            }

            (Some(TupleType::Grayscale), 1)
            | (Some(TupleType::GrayscaleAlpha), 2)
            | (Some(TupleType::RGB), 3)
            | (Some(TupleType::RGBAlpha), 4) => self.read_scaled_samples(pixels * depth).into_pixel_data(depth),

            (Some(TupleType::CMYK), 4) | (Some(TupleType::CMYKAlpha), 5) => {
                let alpha = depth == 5;
                let count = self.available_count(pixels * depth, if wide { 2 } else { 1 });
                let mut data = match self.read_raw_samples(count, wide) {
                    Samples::U8(data) => Samples::U8(raster::cmyk_to_rgb_u8(&data, max_value, alpha)),
                    Samples::U16(data) => Samples::U16(raster::cmyk_to_rgb_u16(&data, max_value, alpha)),
                };
                let out_channels = if alpha { 4 } else { 3 };
                match &mut data {
                    Samples::U8(data) => data.resize(pixels * out_channels, 0),
                    Samples::U16(data) => data.resize(pixels * out_channels, 0),
                }
                data.into_pixel_data(out_channels)
            }

            _ => {
                log_warn!(
                    "Incorrect tuple type / depth combination: {:?}, {}. Inferring layout from depth.",
                    self.tuple_type,
                    depth
                );

                if depth <= 4 {
                    return self.read_scaled_samples(pixels * depth).into_pixel_data(depth);
                }

                let count = self.available_count(pixels * depth, if wide { 2 } else { 1 });
                let samples = match self.read_scaled_samples(count) {
                    Samples::U8(data) => Samples::U8(raster::first_channels(&data, depth, 3, pixels)),
                    Samples::U16(data) => Samples::U16(raster::first_channels(&data, depth, 3, pixels)),
                };
                samples.into_pixel_data(3)
            }
        }
    }

    fn decode_frame(&mut self) -> VexelResult<PixelData> {
        let pixels = (self.width as usize).checked_mul(self.height as usize);
        let bytes = pixels.and_then(|p| p.checked_mul(self.channels().max(1))).and_then(|s| s.checked_mul(4));
        let Some(pixels) = pixels.filter(|_| bytes.is_some_and(|b| b <= isize::MAX as usize)) else {
            return Err(VexelError::LimitExceeded(format!(
                "Netpbm image {}x{} is too large to allocate",
                self.width, self.height
            )));
        };

        Ok(match self.format {
            NetpbmFormat::P1 => self.decode_plain_bitmap(pixels),
            NetpbmFormat::P2 => self.decode_plain(pixels, 1),
            NetpbmFormat::P3 => self.decode_plain(pixels * 3, 3),
            NetpbmFormat::P4 => self.decode_raw_bitmap(pixels),
            NetpbmFormat::P5 => self.read_scaled_samples(pixels).into_pixel_data(1),
            NetpbmFormat::P6 => self.read_scaled_samples(pixels * 3).into_pixel_data(3),
            NetpbmFormat::P7 => self.decode_pam(pixels),
            NetpbmFormat::PfmColor => self.decode_pfm(pixels, 3),
            NetpbmFormat::PfmGray => self.decode_pfm(pixels, 1),
        })
    }

    fn has_more_data(&mut self) -> bool {
        matches!(self.skip_space(), Ok(b'P'))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut pages = self.walk(WalkMode::DecodeFirst)?;
        if pages.is_empty() {
            return Err(VexelError::Custom("No Netpbm image decoded".to_string()));
        }
        Ok(pages.remove(0))
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        let pages = self.walk(WalkMode::DecodeAll)?;
        self.finish_info();
        Ok(pages)
    }

    fn walk(&mut self, mode: WalkMode) -> VexelResult<Vec<Image>> {
        self.input.rewind()?;
        self.collected = NetpbmInfo::default();
        self.image_index = 0;

        let mut pages: Vec<Image> = Vec::new();

        loop {
            self.reset_frame_state();

            match self.read_header(mode) {
                Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
                Err(e) => {
                    if self.image_index == 0 {
                        return Err(e);
                    }
                    log_warn!("Stopping at image {}: failed to read header: {}", self.image_index + 1, e);
                    break;
                }
                Ok(_) => {}
            }

            let layout = self.pixel_layout();
            self.skip_crlf_delimiter(&layout);
            let start = self.input.position();
            let result = match mode {
                WalkMode::InfoOnly => {
                    self.skip_frame(&layout);
                    Ok(None)
                }
                _ => self.decode_frame().map(Some),
            };
            let end = self.input.position();
            self.collected.push(start, Some(end.saturating_sub(start)), NetpbmSection::PixelData(layout));

            match result {
                Ok(Some(mut pixel_data)) => {
                    pixel_data.correct_pixels(self.width, self.height);
                    pages.push(Image::from_pixels(self.width, self.height, pixel_data));
                }
                Ok(None) => {}
                Err(e) => {
                    if self.image_index == 0 {
                        return Err(e);
                    }
                    log_warn!("Stopping at image {}: failed to decode: {}", self.image_index + 1, e);
                    break;
                }
            }

            self.image_index += 1;

            if mode == WalkMode::DecodeFirst || !self.has_more_data() {
                break;
            }
        }

        Ok(pages)
    }
}
