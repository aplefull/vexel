use std::io::{Read, Seek, SeekFrom};

use super::animation::AnimationDecoder;
use super::chunks;
use super::crc_simd::update_crc;
use super::info::{FdatChunk, IhdrChunk, PngChunk, PngChunkBody, PngInfo, PngSection, PngSignature};
use super::pixels::PixelDecoder;
use super::types::*;
use crate::bitreader::BitReader;
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, ImageSummary};
use crate::{log_debug, log_warn, Image, ImageFrame, Limits, PixelData};

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const STREAM_BLOCK_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy, PartialEq)]
enum WalkMode {
    InfoOnly,
    Decode,
}

enum Payload {
    Collected(Vec<u8>),
    Idat,
    Fdat { sequence_number: Option<u32>, data: Vec<u8> },
    Streamed { sequence_number: Option<u32> },
}

struct ChunkRead {
    payload: Payload,
    data_read: u64,
    crc: Option<u32>,
    crc_valid: Option<bool>,
}

impl ChunkRead {
    fn complete(&self, data_length: u32) -> bool {
        self.data_read == u64::from(data_length) && self.crc.is_some()
    }
}

pub struct PngDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    bit_depth: u8,
    color_type: ColorType,
    compression_method: CompressionMethod,
    interlace: bool,
    palette: Option<Vec<[u8; 3]>>,
    idat_data: Vec<u8>,
    transparency: Option<TransparencyData>,
    frames: Vec<PngFrame>,
    actl_info: Option<ActlChunk>,
    icc_profile: Option<ICCProfile>,
    collected: PngInfo,
    info: Option<PngInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> PngDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            bit_depth: 0,
            color_type: ColorType::RGB,
            compression_method: CompressionMethod::None,
            interlace: false,
            palette: None,
            idat_data: Vec::new(),
            transparency: None,
            frames: Vec::new(),
            actl_info: None,
            icc_profile: None,
            collected: PngInfo::default(),
            info: None,
            reader: BitReader::new(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> PngInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk_chunks(WalkMode::InfoOnly) {
                log_warn!("Failed to read PNG structure: {}", e);
            }
            self.limits = saved_limits;
            self.finish_info();
        }

        self.info.clone().unwrap_or_default()
    }

    fn reset_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.bit_depth = 0;
        self.color_type = ColorType::RGB;
        self.compression_method = CompressionMethod::None;
        self.interlace = false;
        self.palette = None;
        self.idat_data.clear();
        self.transparency = None;
        self.frames.clear();
        self.actl_info = None;
        self.icc_profile = None;
        self.collected = PngInfo::default();
    }

    fn finish_info(&mut self) {
        let mut info = std::mem::take(&mut self.collected);

        let ihdr = info.sections.iter().find_map(|section| match &section.data {
            PngSection::Chunk(PngChunk { body: PngChunkBody::Ihdr(ihdr), .. }) => Some(ihdr.clone()),
            _ => None,
        });

        if let Some(ihdr) = ihdr {
            let has_trns = info.sections.iter().any(|section| {
                matches!(&section.data, PngSection::Chunk(PngChunk { body: PngChunkBody::Trns(Some(_)), .. }))
            });

            let (color_model, channels) = match ihdr.color_type() {
                Some(ColorType::Grayscale) => (ColorModel::Gray, 1),
                Some(ColorType::GrayscaleAlpha) => (ColorModel::GrayAlpha, 2),
                Some(ColorType::RGB) => (ColorModel::Rgb, 3),
                Some(ColorType::RGBA) => (ColorModel::Rgba, 4),
                Some(ColorType::Indexed) => (ColorModel::Indexed, 1),
                None => (ColorModel::Unknown, 0),
            };

            let mut summary = ImageSummary::new(ihdr.width, ihdr.height, color_model, channels, ihdr.bit_depth);
            summary.has_alpha = matches!(color_model, ColorModel::GrayAlpha | ColorModel::Rgba) || has_trns;
            summary.compression = Some(if ihdr.compression_method == 0 {
                "Deflate".to_string()
            } else {
                format!("Unknown ({})", ihdr.compression_method)
            });
            summary.frames = self.actl_info.as_ref().map(|actl| actl.num_frames);
            summary.loop_count = self.actl_info.as_ref().map(|actl| actl.num_plays);
            if self.actl_info.is_some() {
                let total_ms: f64 = info
                    .sections
                    .iter()
                    .filter_map(|section| match &section.data {
                        PngSection::Chunk(PngChunk { body: PngChunkBody::Fctl(fctl), .. }) => {
                            let denominator = if fctl.delay_den == 0 { 100 } else { fctl.delay_den };
                            Some(f64::from(fctl.delay_num) * 1000.0 / f64::from(denominator))
                        }
                        _ => None,
                    })
                    .sum();
                summary.duration_ms = Some(total_ms.round() as u64);
            }
            summary.icc_profile = self.icc_profile.clone();
            info.summary = Some(summary);
        }

        self.info = Some(info);
    }

    fn push_section(&mut self, offset: u64, length: u64, data: PngSection) {
        self.collected.push(offset, Some(length), data);
    }

    fn stream_payload(&mut self, length: u32, mut crc: u32, mut sink: impl FnMut(&[u8])) -> VexelResult<(u64, u32)> {
        let mut buffer = vec![0u8; STREAM_BLOCK_SIZE.min(length as usize)];
        let mut remaining = length as usize;
        let mut total = 0u64;

        while remaining > 0 {
            let want = remaining.min(buffer.len());
            let read = match self.reader.read(&mut buffer[..want]) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            crc = update_crc(crc, &buffer[..read]);
            sink(&buffer[..read]);
            remaining -= read;
            total += read as u64;
        }

        Ok((total, crc))
    }

    fn read_chunk(
        &mut self,
        kind: Option<ChunkKind>,
        chunk_type: [u8; 4],
        length: u32,
        mode: WalkMode,
    ) -> VexelResult<ChunkRead> {
        let crc_seed = update_crc(0xffffffff, &chunk_type);
        let available = self.reader.bytes_left().unwrap_or(0);
        let reservable = u64::from(length).min(available);

        let (payload, data_read, computed) = match (kind, mode) {
            (Some(ChunkKind::IDAT), WalkMode::Decode) => {
                self.limits.reserve(reservable)?;
                let start = self.idat_data.len();
                let mut idat = std::mem::take(&mut self.idat_data);
                let (read, crc) = self.stream_payload(length, crc_seed, |bytes| idat.extend_from_slice(bytes))?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.fdat.extend_from_slice(&idat[start..]);
                }
                self.idat_data = idat;
                (Payload::Idat, read, crc)
            }
            (Some(ChunkKind::FDAT), WalkMode::Decode) => {
                self.limits.reserve(reservable)?;
                let mut data = Vec::new();
                let (read, crc) = self.stream_payload(length, crc_seed, |bytes| data.extend_from_slice(bytes))?;
                let sequence_number =
                    (data.len() >= 4).then(|| u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
                (Payload::Fdat { sequence_number, data }, read, crc)
            }
            (Some(ChunkKind::IDAT) | Some(ChunkKind::FDAT) | None, _) => {
                let mut prefix = Vec::with_capacity(4);
                let (read, crc) = self.stream_payload(length, crc_seed, |bytes| {
                    let needed = 4usize.saturating_sub(prefix.len());
                    prefix.extend_from_slice(&bytes[..needed.min(bytes.len())]);
                })?;
                let sequence_number = (prefix.len() == 4 && kind == Some(ChunkKind::FDAT))
                    .then(|| u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]));
                (Payload::Streamed { sequence_number }, read, crc)
            }
            (Some(_), _) => {
                self.limits.reserve(reservable)?;
                let mut data = Vec::with_capacity(reservable as usize);
                let (read, crc) = self.stream_payload(length, crc_seed, |bytes| data.extend_from_slice(bytes))?;
                (Payload::Collected(data), read, crc)
            }
        };

        let stored = self.reader.read_u32().ok();
        let computed = computed ^ 0xffffffff;
        let crc_valid = match stored {
            Some(stored) if data_read == u64::from(length) => Some(stored == computed),
            _ => None,
        };

        let chunk_name = String::from_utf8_lossy(&chunk_type);
        if data_read < u64::from(length) {
            log_warn!("Truncated {} chunk: read {} of {} bytes", chunk_name, data_read, length);
        }
        if crc_valid == Some(false) {
            log_warn!(
                "CRC mismatch for chunk {}: expected 0x{:08x}, calculated 0x{:08x}",
                chunk_name,
                stored.unwrap_or(0),
                computed
            );
        }

        Ok(ChunkRead { payload, data_read, crc: stored, crc_valid })
    }

    fn parse_body(
        &mut self,
        kind: ChunkKind,
        data: &[u8],
        complete: bool,
        mode: WalkMode,
    ) -> VexelResult<PngChunkBody> {
        let body = match kind {
            ChunkKind::IHDR => {
                let ihdr = chunks::parse_ihdr(data);
                if complete {
                    self.apply_ihdr(&ihdr, mode)?;
                }
                PngChunkBody::Ihdr(ihdr)
            }
            ChunkKind::PLTE => {
                let palette = chunks::parse_plte(data);
                if complete {
                    self.palette = Some(palette.clone());
                }
                PngChunkBody::Plte(palette)
            }
            ChunkKind::TRNS => {
                let trns = chunks::parse_trns(data, self.color_type, self.palette.is_some());
                if complete {
                    self.transparency = trns.clone();
                }
                PngChunkBody::Trns(trns)
            }
            ChunkKind::BKGD => PngChunkBody::Bkgd(chunks::parse_bkgd(data, self.color_type, self.palette.is_some())),
            ChunkKind::CHRM => PngChunkBody::Chrm(chunks::parse_chrm(data)),
            ChunkKind::GAMA => PngChunkBody::Gama(chunks::parse_gama(data)),
            ChunkKind::ICCP => {
                let (iccp, profile) = chunks::parse_iccp(data, &mut self.limits)?;
                if profile.is_some() {
                    self.icc_profile = profile;
                }
                PngChunkBody::Iccp(iccp)
            }
            ChunkKind::SBIT => PngChunkBody::Sbit(chunks::parse_sbit(data, self.color_type)),
            ChunkKind::SRGB => PngChunkBody::Srgb(chunks::parse_srgb(data)),
            ChunkKind::TEXT => PngChunkBody::Text(chunks::parse_text(data)),
            ChunkKind::ZTXT => PngChunkBody::Ztxt(chunks::parse_ztxt(data, &mut self.limits)?),
            ChunkKind::ITXT => PngChunkBody::Itxt(chunks::parse_itxt(data, &mut self.limits)?),
            ChunkKind::PHYS => PngChunkBody::Phys(chunks::parse_phys(data)),
            ChunkKind::TIME => PngChunkBody::Time(chunks::parse_time(data)),
            ChunkKind::SPLT => PngChunkBody::Splt(chunks::parse_splt(data)),
            ChunkKind::HIST => PngChunkBody::Hist(chunks::parse_hist(data, self.palette.as_ref().map(Vec::len))),
            ChunkKind::ACTL => {
                let actl = chunks::parse_actl(data);
                if complete {
                    self.actl_info = Some(actl.clone());
                }
                PngChunkBody::Actl(actl)
            }
            ChunkKind::FCTL => {
                let fctl = chunks::parse_fctl(data);
                if complete {
                    let sanitized = chunks::sanitize_fctl(&fctl, self.width, self.height);
                    if mode == WalkMode::Decode {
                        self.limits.reserve_buffer(sanitized.width, sanitized.height, 4)?;
                    }
                    self.frames.push(PngFrame { fctl_info: sanitized, fdat: Vec::new() });
                }
                PngChunkBody::Fctl(fctl)
            }
            ChunkKind::IEND => PngChunkBody::Iend,
            ChunkKind::IDAT | ChunkKind::FDAT => PngChunkBody::Unknown,
        };

        Ok(body)
    }

    fn apply_ihdr(&mut self, ihdr: &IhdrChunk, mode: WalkMode) -> VexelResult<()> {
        self.width = ihdr.width;
        self.height = ihdr.height;

        self.bit_depth = match ihdr.bit_depth {
            1 | 2 | 4 | 8 | 16 => ihdr.bit_depth,
            other => {
                log_warn!("Invalid bit depth: {}", other);
                8
            }
        };

        self.color_type = ihdr.color_type().unwrap_or_else(|| {
            log_warn!("Invalid color type: {}", ihdr.color_type);
            ColorType::RGB
        });

        self.compression_method = match ihdr.compression_method {
            0 => CompressionMethod::Deflate,
            1 => CompressionMethod::None,
            other => {
                log_warn!("Invalid compression method: {}", other);
                CompressionMethod::None
            }
        };

        if ihdr.filter_method > 1 {
            log_warn!("Invalid filter method: {}", ihdr.filter_method);
        }

        self.interlace = match ihdr.interlace_method {
            0 => false,
            1 => true,
            other => {
                log_warn!("Invalid interlace method: {}", other);
                false
            }
        };

        if mode == WalkMode::Decode {
            if self.width == 0 || self.height == 0 {
                return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
            }

            self.limits.reserve_buffer(self.width, self.height, 4)?;
        }

        Ok(())
    }

    fn process_chunk(&mut self, chunk_type: [u8; 4], length: u32, mode: WalkMode) -> VexelResult<()> {
        let offset = self.reader.stream_position()?.saturating_sub(8);
        let kind = chunk_kind(&chunk_type);
        match kind {
            Some(kind) => log_debug!("Found chunk: {:?}", kind),
            None => log_debug!("Unknown chunk: {}", String::from_utf8_lossy(&chunk_type)),
        }

        let read = self.read_chunk(kind, chunk_type, length, mode)?;
        let complete = read.complete(length);

        let body = match (read.payload, kind) {
            (Payload::Collected(data), Some(kind)) => {
                let result = self.parse_body(kind, &data, complete, mode);
                self.limits.free_usize(data.capacity());
                result?
            }
            (Payload::Idat, _) => PngChunkBody::Idat,
            (Payload::Fdat { sequence_number, data }, _) => {
                if self.frames.is_empty() {
                    log_warn!("fdAT chunk without preceding fcTL chunk");
                    self.limits.free_usize(data.capacity());
                } else if let Some(frame) = self.frames.last_mut() {
                    frame.fdat.extend_from_slice(data.get(4..).unwrap_or(&[]));
                }
                PngChunkBody::Fdat(FdatChunk { sequence_number: sequence_number.unwrap_or(0) })
            }
            (Payload::Streamed { sequence_number }, Some(ChunkKind::FDAT)) => {
                PngChunkBody::Fdat(FdatChunk { sequence_number: sequence_number.unwrap_or(0) })
            }
            (Payload::Streamed { .. }, Some(ChunkKind::IDAT)) => PngChunkBody::Idat,
            _ => PngChunkBody::Unknown,
        };

        let chunk = PngChunk {
            chunk_type: String::from_utf8_lossy(&chunk_type).to_string(),
            data_length: length,
            data_read: read.data_read,
            crc: read.crc,
            crc_valid: read.crc_valid,
            body,
        };
        let on_disk = 8 + read.data_read + if read.crc.is_some() { 4 } else { 0 };
        self.push_section(offset, on_disk, PngSection::Chunk(chunk));

        Ok(())
    }

    fn walk_chunks(&mut self, mode: WalkMode) -> VexelResult<()> {
        self.reset_state();
        self.reader.seek(SeekFrom::Start(0))?;

        let mut signature = [0u8; 8];
        self.reader.read_exact(&mut signature)?;
        let valid = signature == PNG_SIGNATURE;
        if !valid {
            log_warn!("Invalid PNG signature: {:02X?}", signature);
        }
        self.push_section(0, 8, PngSection::Signature(PngSignature { valid }));

        loop {
            let chunk_start = self.reader.stream_position()?;

            let length = match self.reader.read_u32() {
                Ok(length) => length,
                Err(_) => break,
            };

            let mut chunk_type = [0u8; 4];
            if self.reader.read_exact(&mut chunk_type).is_err() {
                break;
            }

            match self.process_chunk(chunk_type, length, mode) {
                Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
                Err(e @ VexelError::InvalidDimensions { .. }) => return Err(e),
                Err(e) => log_warn!("Error reading chunk {}: {:?}", String::from_utf8_lossy(&chunk_type), e),
                Ok(()) => {}
            }

            let next_chunk_start = chunk_start + 12 + u64::from(length);
            let at_next = self.reader.stream_position().map(|p| p == next_chunk_start).unwrap_or(false);
            if !at_next && self.reader.seek(SeekFrom::Start(next_chunk_start)).is_err() {
                break;
            }
        }

        Ok(())
    }

    fn decode_pixels(&mut self) -> VexelResult<PixelData> {
        if self.compression_method == CompressionMethod::Deflate {
            let bits_per_pixel = match self.color_type {
                ColorType::Grayscale => self.bit_depth as u32,
                ColorType::RGB => self.bit_depth as u32 * 3,
                ColorType::Indexed => self.bit_depth as u32,
                ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
                ColorType::RGBA => self.bit_depth as u32 * 4,
            };
            let bytes_per_row = (u64::from(bits_per_pixel) * u64::from(self.width)).div_ceil(8) + 1;
            let capacity =
                usize::try_from(bytes_per_row.saturating_mul(u64::from(self.height))).unwrap_or(usize::MAX / 8);
            let compressed = std::mem::take(&mut self.idat_data);
            let compressed_len = compressed.len();
            self.idat_data = ZlibDecoder::from_slice(&compressed).decode_with_capacity(capacity);
            drop(compressed);
            self.limits.free_usize(compressed_len);
        }

        let bits_per_pixel = match self.color_type {
            ColorType::Grayscale => self.bit_depth as u32,
            ColorType::RGB => self.bit_depth as u32 * 3,
            ColorType::Indexed => self.bit_depth as u32,
            ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
            ColorType::RGBA => self.bit_depth as u32 * 4,
        };
        let bytes_per_row = (u64::from(bits_per_pixel) * u64::from(self.width)).div_ceil(8) + 1;

        if !self.interlace && !self.idat_data.is_empty() {
            let expected = usize::try_from(bytes_per_row.saturating_mul(u64::from(self.height))).unwrap_or(usize::MAX);
            if self.idat_data.len() < expected {
                log_warn!(
                    "Decompressed data covers only {} of {} rows, filling the rest with zeros",
                    self.idat_data.len() as u64 / bytes_per_row,
                    self.height
                );
                self.idat_data.resize(expected, 0);
            }
        }

        if self.idat_data.is_empty() {
            log_warn!("No decompressed pixel data for {}x{} image, filling with zeros", self.width, self.height);
            let length = (self.width as usize).saturating_mul(self.height as usize).saturating_mul(4);
            return Ok(PixelData::RGBA8(vec![0u8; length]));
        }

        let pixel_decoder = PixelDecoder::new(
            self.bit_depth,
            self.color_type,
            self.width,
            self.interlace,
            self.palette.clone(),
            self.transparency.clone(),
        );

        let data =
            pixel_decoder.deinterlace_scan_lines_owned(std::mem::take(&mut self.idat_data), self.width, self.height)?;

        let mut pixels = match self.color_type {
            ColorType::Indexed => pixel_decoder.decode_indexed(data)?,
            ColorType::RGB => pixel_decoder.decode_rgb(data)?,
            ColorType::RGBA => pixel_decoder.decode_rgba(data)?,
            ColorType::Grayscale => pixel_decoder.decode_grayscale(data)?,
            ColorType::GrayscaleAlpha => pixel_decoder.decode_grayscale_alpha(data)?,
        };

        pixels.correct_pixels(self.width, self.height);

        Ok(pixels)
    }

    fn decode_inner(&mut self) -> VexelResult<DecodedPng> {
        self.info = None;
        self.walk_chunks(WalkMode::Decode)?;
        self.finish_info();

        if self.actl_info.is_some() {
            let mut anim_decoder = AnimationDecoder::new(self.width, self.height);
            let result = anim_decoder.decode_apng_frames(
                &self.frames,
                self.bit_depth,
                self.color_type,
                self.interlace,
                self.palette.clone(),
                self.transparency.clone(),
            );

            match result {
                Ok(image_frames) => return Ok(DecodedPng::Animation(image_frames)),
                Err(e) => log_warn!("Error decoding APNG frames: {:?}", e),
            }
        }

        let pixel_data = self.decode_pixels()?;

        Ok(DecodedPng::Still(Image::from_pixels(self.width, self.height, pixel_data)))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.decode_inner()? {
            DecodedPng::Still(image) => Ok(image),
            DecodedPng::Animation(frames) => {
                let first = frames
                    .into_iter()
                    .next()
                    .ok_or(VexelError::Custom("PNG: animation contains no frames".to_string()))?;

                Ok(Image::from_frame(first))
            }
        }
    }

    pub(crate) fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        match self.decode_inner()? {
            DecodedPng::Still(image) => {
                let (width, height) = (image.width(), image.height());
                Ok(vec![ImageFrame::new(width, height, image.into_pixels(), 0)])
            }
            DecodedPng::Animation(frames) => Ok(frames),
        }
    }
}

enum DecodedPng {
    Still(Image),
    Animation(Vec<ImageFrame>),
}
