use std::io::{Read, Seek, SeekFrom};

use super::info::{PgxByteOrder, PgxHeader, PgxInfo, PgxPixelData, PgxSection};
use super::simd::{unpack_u16, unpack_u8};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::PixelData;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary, SampleFormat};
use crate::{log_warn, Image, Limits};

pub const MAGIC: &[u8; 2] = b"PG";

const MAX_HEADER_SIZE: usize = 64;

pub struct PgxDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    precision: u8,
    signed: bool,
    byte_order: PgxByteOrder,
    limits: Limits,
    reader: BitReader<R>,
    info: Option<PgxInfo>,
}

impl<R: Read + Seek> PgxDecoder<R> {
    pub fn new(reader: R) -> Self {
        PgxDecoder {
            width: 0,
            height: 0,
            precision: 8,
            signed: false,
            byte_order: PgxByteOrder::BigEndian,
            limits: Limits::default(),
            reader: BitReader::new(reader),
            info: None,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    fn scan(&mut self) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;

        let available = self.reader.bytes_left().unwrap_or(0);
        let peek_len = usize::try_from(available).unwrap_or(usize::MAX).min(MAX_HEADER_SIZE);
        let peeked = self.reader.peek_bytes(peek_len)?;

        let Some(header) = parse_header(&peeked) else {
            return Err(VexelError::Custom("invalid PGX header".to_string()));
        };

        self.width = header.width;
        self.height = header.height;
        self.signed = header.signed;
        self.byte_order = header.byte_order;
        self.precision = header.precision.clamp(1, 31) as u8;
        if u32::from(self.precision) != header.precision {
            log_warn!("Invalid PGX precision {}, clamping to {}", header.precision, self.precision);
        }

        let mut info = PgxInfo::default();
        let header_length = header.data_offset as u64;
        info.push(
            0,
            Some(header_length),
            PgxSection::Header(PgxHeader {
                byte_order: header.byte_order,
                signed: header.signed,
                precision: header.precision,
                width: header.width,
                height: header.height,
            }),
        );

        let expected = self.expected_bytes();
        let actual = expected.min(available.saturating_sub(header_length));
        info.push(
            header_length,
            Some(actual),
            PgxSection::PixelData(PgxPixelData {
                bytes_per_sample: self.bytes_per_sample() as u8,
                expected_length: expected,
            }),
        );

        let mut summary = ImageSummary::new(self.width, self.height, ColorModel::Gray, 1, self.precision);
        summary.sample_format = if self.signed { SampleFormat::Signed } else { SampleFormat::Unsigned };
        summary.data = Some(DataExtent { expected, actual });
        info.summary = Some(summary);
        self.info = Some(info);

        self.reader.seek(SeekFrom::Start(header_length))?;

        Ok(())
    }

    fn read_header(&mut self) -> VexelResult<()> {
        self.scan()?;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        self.limits.check_dimensions(self.width, self.height)?;

        Ok(())
    }

    fn expected_bytes(&self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height)).saturating_mul(self.bytes_per_sample() as u64)
    }

    fn bytes_per_sample(&self) -> usize {
        if self.precision > 16 {
            4
        } else if self.precision > 8 {
            2
        } else {
            1
        }
    }

    fn read_samples(&mut self) -> VexelResult<Vec<u8>> {
        let expected = self.expected_bytes();

        let available = self.reader.bytes_left().unwrap_or(0);
        let to_read = usize::try_from(expected.min(available)).unwrap_or(usize::MAX);

        self.limits.reserve_usize(to_read)?;

        let data = self.reader.read_bytes(to_read)?;

        if (data.len() as u64) < expected {
            log_warn!("Truncated PGX pixel data: expected {} bytes, got {}", expected, data.len());
        }

        Ok(data)
    }

    fn build_pixels(&mut self, raw: &[u8]) -> VexelResult<PixelData> {
        let pixels =
            usize::try_from(u64::from(self.width).saturating_mul(u64::from(self.height))).unwrap_or(usize::MAX);

        let big_endian = self.byte_order == PgxByteOrder::BigEndian;

        if self.bytes_per_sample() == 1 {
            self.limits.reserve_usize(pixels)?;

            let mut out = vec![0u8; pixels];
            unpack_u8(raw, &mut out, self.signed, 8 - self.precision.min(8));

            return Ok(PixelData::L8(out));
        }

        self.limits.reserve_usize(pixels.saturating_mul(2))?;

        let mut out = vec![0u16; pixels];

        if self.bytes_per_sample() == 2 {
            unpack_u16(raw, &mut out, self.signed, big_endian, 16 - self.precision.min(16));
        } else {
            self.unpack_u32_to_u16(raw, &mut out, big_endian);
        }

        Ok(PixelData::L16(out))
    }

    fn unpack_u32_to_u16(&self, raw: &[u8], out: &mut [u16], big_endian: bool) {
        let bias = if self.signed { 1u32 << (self.precision - 1) } else { 0 };
        let down = self.precision.saturating_sub(16);

        for (d, chunk) in out.iter_mut().zip(raw.chunks_exact(4)) {
            let raw = if big_endian {
                u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            } else {
                u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            };

            *d = (raw.wrapping_add(bias) >> down).min(u16::MAX as u32) as u16;
        }
    }

    pub fn get_info(&mut self) -> PgxInfo {
        if self.info.is_none() {
            if let Err(e) = self.scan() {
                log_warn!("Failed to read PGX header: {}", e);
            }
        }

        self.info.clone().unwrap_or_default()
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let raw = self.read_samples()?;

        let pixels = self.build_pixels(&raw)?;

        Ok(Image::from_pixels(self.width, self.height, pixels))
    }
}

struct ParsedHeader {
    width: u32,
    height: u32,
    precision: u32,
    signed: bool,
    byte_order: PgxByteOrder,
    data_offset: usize,
}

fn parse_header(data: &[u8]) -> Option<ParsedHeader> {
    if !data.starts_with(MAGIC) {
        return None;
    }

    let mut pos = MAGIC.len();

    skip_blanks(data, &mut pos);

    let byte_order = match (data.get(pos), data.get(pos + 1)) {
        (Some(b'M'), Some(b'L')) => PgxByteOrder::BigEndian,
        (Some(b'L'), Some(b'M')) => PgxByteOrder::LittleEndian,
        _ => return None,
    };
    pos += 2;

    let mut signed = false;
    while let Some(&b) = data.get(pos) {
        match b {
            b' ' | b'\t' | b'+' => pos += 1,
            b'-' => {
                signed = true;
                pos += 1;
            }
            _ => break,
        }
    }

    let precision = read_number(data, &mut pos)?;
    skip_blanks(data, &mut pos);
    let width = read_number(data, &mut pos)?;
    skip_blanks(data, &mut pos);
    let height = read_number(data, &mut pos)?;

    while let Some(&b) = data.get(pos) {
        if b == b'\r' || b == b'\n' {
            pos += 1;
        } else {
            break;
        }
    }

    Some(ParsedHeader { width, height, precision, signed, byte_order, data_offset: pos })
}

fn skip_blanks(data: &[u8], pos: &mut usize) {
    while matches!(data.get(*pos), Some(b' ') | Some(b'\t')) {
        *pos += 1;
    }
}

fn read_number(data: &[u8], pos: &mut usize) -> Option<u32> {
    let start = *pos;
    let mut value: u32 = 0;

    while let Some(&b) = data.get(*pos) {
        if !b.is_ascii_digit() {
            break;
        }

        value = value.saturating_mul(10).saturating_add(u32::from(b - b'0'));
        *pos += 1;
    }

    if *pos == start {
        return None;
    }

    Some(value)
}
