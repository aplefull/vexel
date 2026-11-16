use std::io::{Read, Seek, SeekFrom};

use super::animation::{PngData, PngFrames};
use super::chunks;
use super::crc_simd::update_crc;
use super::info::{ExifChunk, FdatChunk, IhdrChunk, PngChunk, PngChunkBody, PngInfo, PngSection, PngSignature};
use super::pixels::PngFormat;
use super::types::*;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::exif::ExifReader;
use crate::utils::frames::{FrameExtent, FrameSource, Frames};
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, ImageSummary};
use crate::utils::metadata::{strip_exif_header, MetadataCollector};
use crate::utils::stream::StreamReader;
use crate::{log_debug, log_warn, Image, Limits};

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
    Fdat { sequence_number: Option<u32> },
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
    metadata: MetadataCollector,
    collected: PngInfo,
    info: Option<PngInfo>,
    reader: StreamReader<R>,
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
            metadata: MetadataCollector::default(),
            collected: PngInfo::default(),
            info: None,
            reader: StreamReader::new(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> PngInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.walk_chunks(WalkMode::InfoOnly, FrameExtent::All) {
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
        self.metadata = MetadataCollector::default();
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
            self.metadata.apply(&mut summary);
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
                    frame.append_idat(&idat, start..idat.len());
                }
                self.idat_data = idat;
                (Payload::Idat, read, crc)
            }
            (Some(ChunkKind::FDAT), WalkMode::Decode) => {
                self.limits.reserve(reservable)?;
                let idat = &self.idat_data;
                let mut fdat = self.frames.last_mut().map(|frame| frame.take_fdat(idat));
                let mut prefix = [0u8; 4];
                let mut prefix_len = 0;
                let (read, crc) = self.stream_payload(length, crc_seed, |bytes| {
                    let needed = (prefix.len() - prefix_len).min(bytes.len());
                    prefix[prefix_len..prefix_len + needed].copy_from_slice(&bytes[..needed]);
                    prefix_len += needed;
                    if let Some(fdat) = fdat.as_mut() {
                        fdat.extend_from_slice(&bytes[needed..]);
                    }
                })?;
                match (self.frames.last_mut(), fdat) {
                    (Some(frame), Some(fdat)) => frame.fdat = fdat,
                    _ => self.limits.free(reservable),
                }
                let sequence_number = (prefix_len == prefix.len()).then(|| u32::from_be_bytes(prefix));
                (Payload::Fdat { sequence_number }, read, crc)
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
                if complete && self.bit_depth != 0 {
                    log_warn!("Duplicate IHDR chunk, ignoring");
                } else if complete {
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
            ChunkKind::TEXT => {
                let text = chunks::parse_text(data);
                chunks::collect_text_metadata(&text.keyword, &text.text, &mut self.metadata);
                PngChunkBody::Text(text)
            }
            ChunkKind::ZTXT => {
                let text = chunks::parse_ztxt(data, &mut self.limits)?;
                chunks::collect_text_metadata(&text.keyword, &text.text, &mut self.metadata);
                PngChunkBody::Ztxt(text)
            }
            ChunkKind::ITXT => {
                let text = chunks::parse_itxt(data, &mut self.limits)?;
                chunks::collect_text_metadata(&text.keyword, &text.text, &mut self.metadata);
                PngChunkBody::Itxt(text)
            }
            ChunkKind::EXIF => {
                let exif = ExifReader::parse(strip_exif_header(data));
                match &exif {
                    Some(exif) => self.metadata.add_exif_data(exif),
                    None => log_warn!("Failed to parse eXIf chunk"),
                }
                PngChunkBody::Exif(ExifChunk { exif })
            }
            ChunkKind::CICP => match chunks::parse_cicp(data) {
                Some(cicp) => {
                    self.metadata.set_cicp(cicp);
                    PngChunkBody::Cicp(cicp)
                }
                None => PngChunkBody::Unknown,
            },
            ChunkKind::MDCV => match chunks::parse_mdcv(data) {
                Some(display) => {
                    self.metadata.set_mastering_display(display);
                    PngChunkBody::Mdcv(display)
                }
                None => PngChunkBody::Unknown,
            },
            ChunkKind::CLLI => match chunks::parse_clli(data) {
                Some(level) => {
                    self.metadata.set_content_light_level(level);
                    PngChunkBody::Clli(level)
                }
                None => PngChunkBody::Unknown,
            },
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
                    self.frames.push(PngFrame::new(sanitized));
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

            self.limits.reserve_buffer(self.width, self.height, self.output_bytes_per_pixel())?;
        }

        Ok(())
    }

    fn output_bytes_per_pixel(&self) -> u8 {
        if self.bit_depth == 16 {
            8
        } else {
            4
        }
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
            (Payload::Fdat { sequence_number }, _) => {
                if self.frames.is_empty() {
                    log_warn!("fdAT chunk without preceding fcTL chunk");
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

    fn first_frame_ready(&self) -> bool {
        self.actl_info.is_some() && !self.idat_data.is_empty() && self.frames.iter().any(PngFrame::is_drawable)
    }

    fn walk_chunks(&mut self, mode: WalkMode, extent: FrameExtent) -> VexelResult<bool> {
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

            if extent == FrameExtent::First
                && chunk_kind(&chunk_type) == Some(ChunkKind::FCTL)
                && self.first_frame_ready()
            {
                return Ok(false);
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

        Ok(true)
    }

    fn prepare(&mut self, extent: FrameExtent) -> VexelResult<bool> {
        self.info = None;
        let complete = self.walk_chunks(WalkMode::Decode, extent)?;
        if complete {
            self.finish_info();
        } else {
            self.collected = PngInfo::default();
        }

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        Ok(complete)
    }

    fn frame_source(&mut self, lookahead: bool, max_alloc: Option<u64>) -> PngFrames {
        let data = PngData {
            format: PngFormat {
                width: self.width,
                height: self.height,
                bit_depth: self.bit_depth,
                color_type: self.color_type,
                compression_method: self.compression_method,
                interlace: self.interlace,
                palette: self.palette.clone(),
                transparency: self.transparency.clone(),
            },
            frames: std::mem::take(&mut self.frames),
            idat: std::mem::take(&mut self.idat_data),
        };

        PngFrames::new(data, self.actl_info.is_some(), lookahead, &mut self.limits, max_alloc)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let limits = self.limits.clone();
        let mut extent = FrameExtent::First;

        loop {
            let complete = self.prepare(extent)?;
            let mut frames = self.frame_source(false, limits.max_alloc);

            if let Some(frame) = frames.next_animation_frame() {
                return Ok(Image::from_frame(frame));
            }

            if complete {
                return frames
                    .next_frame()?
                    .map(Image::from_frame)
                    .ok_or_else(|| VexelError::Custom("PNG: image contains no frames".to_string()));
            }

            self.limits = limits.clone();
            extent = FrameExtent::All;
        }
    }

    pub fn decode_frames(&mut self) -> VexelResult<Frames> {
        let max_alloc = self.limits.max_alloc;
        self.prepare(FrameExtent::All)?;
        Ok(Frames::new(self.frame_source(true, max_alloc)))
    }
}
