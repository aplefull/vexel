use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::ImageFrame;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary, SampleFormat as SummarySampleFormat};
use crate::utils::tiff::compression::{
    apply_predictor_float, apply_predictor_horizontal, decompress_deflate_with_capacity, decompress_jpeg2000,
    decompress_jpeg_xr, decompress_lerc, decompress_lzma_with_capacity, decompress_lzw_with_capacity, decompress_next,
    decompress_packbits_with_capacity, decompress_png, decompress_sgilog, decompress_sgilog24, decompress_sgilog_l16,
    decompress_thunderscan, decompress_zstd_with_capacity, predictor_x_factor, ChunkLayout,
};
use crate::utils::tiff::fax::decompress_ccitt;
use crate::utils::tiff::ifd::{self, Ifd, IfdCycleGuard};
use crate::utils::tiff::info::{compression_name, walk_structure, IfdKind, TiffImageIfd, TiffInfo};
use crate::utils::tiff::jpeg::{jpeg_pixel_data, jpeg_rgb_bytes, splice_jpeg_tables};
use crate::utils::tiff::pixels::PixelReader;
use crate::utils::tiff::types::{
    Compression, LercAdditionalCompression, PhotometricInterpretation, PlanarConfiguration, Predictor, SampleFormat,
    TiffHeader, TiffTags,
};
use crate::utils::types::ByteOrder;
use crate::{log_warn, Image, Limits, PixelData};

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    byte_order: ByteOrder,
    header: TiffHeader,
    reader: BitReader<R>,
    info: Option<TiffInfo>,
    subfile_type: u32,
    reserved_output: u64,
}

struct ChunkAssembler {
    width: usize,
    height: usize,
    depth: usize,
    spp: usize,
    bps: usize,
    planes: usize,
    chunk_w: usize,
    chunk_h: usize,
    chunk_d: usize,
    chunks_x: usize,
    chunks_y: usize,
    chunks_z: usize,
    row_bytes: usize,
    plane_bytes: usize,
    chunk_row_bytes: usize,
    total_chunks: usize,
    slices: Vec<Vec<u8>>,
}

const MAX_IMAGE_DEPTH: usize = 65536;
const MAX_OJPEG_TABLES: usize = 4;
const MAX_QUEUED_IFDS: usize = 4096;

impl ChunkAssembler {
    fn new(
        width: usize,
        height: usize,
        depth: usize,
        spp: usize,
        bps: usize,
        planes: usize,
        chunk_w: usize,
        chunk_h: usize,
        chunk_d: usize,
        limits: &mut Limits,
    ) -> VexelResult<Self> {
        let chunk_w = chunk_w.max(1);
        let chunk_h = chunk_h.max(1);
        let chunk_d = chunk_d.max(1);
        let overflow =
            || VexelError::LimitExceeded(format!("TIFF chunk layout {}x{}x{} overflows", width, height, depth));

        let group_bits = spp.checked_mul(bps).ok_or_else(overflow)?;
        let row_bytes = width.checked_mul(group_bits).ok_or_else(overflow)?.div_ceil(8);
        let plane_bytes = row_bytes.checked_mul(height).ok_or_else(overflow)?;
        let slice_size = plane_bytes.checked_mul(planes).ok_or_else(overflow)?;
        let chunk_row_bytes = chunk_w.checked_mul(group_bits).ok_or_else(overflow)?.div_ceil(8);
        let chunks_x = width.div_ceil(chunk_w);
        let chunks_y = height.div_ceil(chunk_h);
        let chunks_z = depth.div_ceil(chunk_d);
        let total_chunks = chunks_x
            .checked_mul(chunks_y)
            .and_then(|n| n.checked_mul(chunks_z))
            .and_then(|n| n.checked_mul(planes))
            .ok_or_else(overflow)?;
        limits.reserve_usize(slice_size.saturating_mul(depth))?;

        Ok(ChunkAssembler {
            width,
            height,
            depth,
            spp,
            bps,
            planes,
            chunk_w,
            chunk_h,
            chunk_d,
            chunks_x,
            chunks_y,
            chunks_z,
            row_bytes,
            plane_bytes,
            chunk_row_bytes,
            total_chunks,
            slices: vec![vec![0u8; slice_size]; depth],
        })
    }

    fn chunks_per_plane(&self) -> usize {
        self.chunks_x * self.chunks_y * self.chunks_z
    }

    fn total_chunks(&self) -> usize {
        self.total_chunks
    }

    fn chunk_row_bytes(&self) -> usize {
        self.chunk_row_bytes
    }

    fn rows_in_chunk(&self, chunk_idx: usize) -> usize {
        let within = if self.chunks_per_plane() > 0 { chunk_idx % self.chunks_per_plane() } else { 0 };
        let cy = (within % (self.chunks_x * self.chunks_y)) / self.chunks_x;
        self.chunk_h.min(self.height.saturating_sub(cy.saturating_mul(self.chunk_h)))
    }

    fn place(&mut self, chunk_idx: usize, data: &[u8]) {
        let per_plane = self.chunks_per_plane();
        if per_plane == 0 {
            return;
        }
        let plane = chunk_idx / per_plane;
        if plane >= self.planes {
            return;
        }
        let within = chunk_idx % per_plane;
        let cz = within / (self.chunks_x * self.chunks_y);
        let rest = within % (self.chunks_x * self.chunks_y);
        let cy = rest / self.chunks_x;
        let cx = rest % self.chunks_x;

        let x0 = cx * self.chunk_w;
        let y0 = cy * self.chunk_h;
        let z0 = cz * self.chunk_d;

        let chunk_row_bytes = self.chunk_row_bytes();
        let group_bits = self.bps * self.spp;

        for dz in 0..self.chunk_d {
            let z = z0 + dz;
            if z >= self.depth {
                break;
            }
            let slice = &mut self.slices[z];

            for row in 0..self.chunk_h {
                let y = y0 + row;
                if y >= self.height {
                    break;
                }
                let Some(src_off) = dz
                    .checked_mul(self.chunk_h)
                    .and_then(|r| r.checked_add(row))
                    .and_then(|r| r.checked_mul(chunk_row_bytes))
                else {
                    break;
                };
                let src_end = src_off.saturating_add(chunk_row_bytes).min(data.len());
                if src_off >= src_end {
                    continue;
                }
                let src_row = &data[src_off..src_end];
                let dst_base = plane * self.plane_bytes + y * self.row_bytes;

                if x0 == 0 && self.chunk_w >= self.width {
                    let n = src_row.len().min(self.row_bytes);
                    slice[dst_base..dst_base + n].copy_from_slice(&src_row[..n]);
                } else if self.bps.is_multiple_of(8) {
                    let group_bytes = group_bits / 8;
                    let copy_w = self.chunk_w.min(self.width - x0);
                    let n = (copy_w * group_bytes).min(src_row.len());
                    let dst = dst_base + x0 * group_bytes;
                    slice[dst..dst + n].copy_from_slice(&src_row[..n]);
                } else {
                    let copy_w = self.chunk_w.min(self.width - x0);
                    let dst_row = &mut slice[dst_base..dst_base + self.row_bytes];
                    copy_bits(src_row, dst_row, x0 * group_bits, copy_w * group_bits);
                }
            }
        }
    }
}

fn copy_bits(src: &[u8], dst: &mut [u8], dst_bit: usize, nbits: usize) {
    for i in 0..nbits {
        let byte = src.get(i / 8).copied().unwrap_or(0);
        let bit = (byte >> (7 - i % 8)) & 1;
        let d = dst_bit + i;
        let di = d / 8;
        if di >= dst.len() {
            break;
        }
        let shift = 7 - d % 8;
        dst[di] = (dst[di] & !(1 << shift)) | (bit << shift);
    }
}

impl<R: Read + Seek> TiffDecoder<R> {
    pub fn new(reader: R) -> TiffDecoder<R> {
        TiffDecoder {
            width: 0,
            height: 0,
            limits: Limits::default(),
            byte_order: ByteOrder::LittleEndian,
            header: TiffHeader::default(),
            reader: BitReader::new(reader),
            info: None,
            subfile_type: 0,
            reserved_output: 0,
        }
    }

    pub fn get_info(&mut self) -> TiffInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            let saved_pos = self.reader.stream_position().unwrap_or(0);
            let info = self.scan_structure();
            self.limits = saved_limits;
            let _ = self.reader.seek(SeekFrom::Start(saved_pos));
            match info {
                Ok(info) => self.info = Some(info),
                Err(e) => log_warn!("Failed to read TIFF structure: {}", e),
            }
        }

        self.info.clone().unwrap_or_default()
    }

    fn scan_structure(&mut self) -> VexelResult<TiffInfo> {
        let structure = walk_structure(&mut self.reader, &mut self.limits)?;
        let pages: Vec<&TiffImageIfd> =
            structure.image_ifds.iter().filter(|image| image.kind == IfdKind::Page).collect();
        let summary = self.build_summary(&pages);
        Ok(TiffInfo { summary, sections: structure.sections })
    }

    fn build_summary(&mut self, pages: &[&TiffImageIfd]) -> Option<ImageSummary> {
        let page = *pages
            .iter()
            .find(|page| ifd_subfile_type(&page.ifd, &mut self.reader) & 1 == 0)
            .or_else(|| pages.first())?;

        let byte_order = page.ifd.byte_order;
        self.reader.set_endianness(byte_order);
        let (mut header, photometric_known) = parse_header(&page.ifd, &mut self.reader, &mut self.limits);
        let raw_compression: u32 = page.ifd.value(TiffTags::Compression, &mut self.reader).unwrap_or(1);
        validate_header(&mut header, photometric_known);

        let spp = header.samples_per_pixel;
        let has_alpha = header.extra_samples.iter().any(|&e| e == 1 || e == 2);
        let bits = header.bits_per_sample.first().copied().unwrap_or(1);
        let color_model = match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero | PhotometricInterpretation::BlackIsZero
                if bits == 1 && spp == 1 =>
            {
                ColorModel::Bilevel
            }
            PhotometricInterpretation::WhiteIsZero
            | PhotometricInterpretation::BlackIsZero
            | PhotometricInterpretation::LogL => {
                if has_alpha {
                    ColorModel::GrayAlpha
                } else {
                    ColorModel::Gray
                }
            }
            PhotometricInterpretation::RGB => {
                if has_alpha {
                    ColorModel::Rgba
                } else {
                    ColorModel::Rgb
                }
            }
            PhotometricInterpretation::Palette => ColorModel::Indexed,
            PhotometricInterpretation::TransparencyMask => ColorModel::Bilevel,
            PhotometricInterpretation::CMYK => {
                if has_alpha {
                    ColorModel::Cmyka
                } else {
                    ColorModel::Cmyk
                }
            }
            PhotometricInterpretation::YCbCr => ColorModel::YCbCr,
            PhotometricInterpretation::CIELab
            | PhotometricInterpretation::ICCLab
            | PhotometricInterpretation::ITULab => ColorModel::Lab,
            PhotometricInterpretation::LogLuv => ColorModel::Xyz,
        };

        let mut summary = ImageSummary::new(header.image_width, header.image_length, color_model, spp, bits as u8);
        summary.sample_format = match header.sample_format.first() {
            Some(SampleFormat::SignedInt) => SummarySampleFormat::Signed,
            Some(SampleFormat::Float) => SummarySampleFormat::Float,
            _ => SummarySampleFormat::Unsigned,
        };
        summary.has_alpha = has_alpha;
        summary.compression = Some(format!("{} ({})", compression_name(raw_compression), raw_compression));
        if header.photometric_interpretation == PhotometricInterpretation::YCbCr
            && !matches!(header.compression, Compression::JPEG | Compression::OldJPEG)
        {
            let [h, v] = header.ycbcr_sub_sampling;
            summary.subsampling = crate::decoders::jpeg::info::subsampling_name(&[(h as u8, v as u8), (1, 1), (1, 1)]);
        }

        let page_count =
            pages.iter().filter(|page| ifd_subfile_type(&page.ifd, &mut self.reader) & 1 == 0).count() as u32;
        if page_count > 1 {
            summary.pages = Some(page_count);
        }

        if let Some(icc) = page.ifd.byte_data(TiffTags::InterColorProfile, &mut self.reader, &mut self.limits) {
            match crate::utils::icc::ICCProfile::new(&icc) {
                Ok(profile) => summary.icc_profile = Some(profile),
                Err(e) => log_warn!("Failed to parse TIFF ICC profile: {}", e),
            }
        }

        summary.data = page
            .image_data
            .as_ref()
            .map(|data| DataExtent { expected: data.declared_bytes, actual: data.available_bytes });

        Some(summary)
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        let saved_pos = self.reader.stream_position().unwrap_or(0);
        let saved_header = std::mem::take(&mut self.header);
        let saved_byte_order = self.byte_order;
        let (saved_width, saved_height) = (self.width, self.height);

        let result = self.decode_reduced_resolution_ifds();

        self.header = saved_header;
        self.byte_order = saved_byte_order;
        self.width = saved_width;
        self.height = saved_height;
        let _ = self.reader.seek(SeekFrom::Start(saved_pos));

        result
    }

    fn decode_reduced_resolution_ifds(&mut self) -> VexelResult<Vec<Image>> {
        self.reader.seek(SeekFrom::Start(0))?;
        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;

        let mut thumbnails = Vec::new();
        let mut guard = IfdCycleGuard::default();
        let mut queue = vec![file_header.first_ifd_offset];

        while let Some(offset) = queue.pop() {
            if !guard.enter(offset) {
                continue;
            }

            let ifd = match Ifd::read(&mut self.reader, offset, self.byte_order, &mut self.limits) {
                Ok(ifd) => ifd,
                Err(_) => continue,
            };

            if ifd.next_ifd_offset != 0 {
                queue.push(ifd.next_ifd_offset);
            }
            if let Some(sub_ifds) = ifd.values::<u32, _>(TiffTags::SubIFDs, &mut self.reader, &mut self.limits) {
                let room = MAX_QUEUED_IFDS.saturating_sub(queue.len());
                queue.extend(sub_ifds.into_iter().filter(|&o| o != 0).take(room));
            }

            let subfile_type: u32 = ifd.value(TiffTags::NewSubfileType, &mut self.reader).unwrap_or(0);
            if subfile_type & 1 == 0 {
                continue;
            }

            match self.read_ifd(offset) {
                Ok(_) => match self.decode_current_ifd() {
                    Ok(frames) => {
                        if let Some(frame) = frames.into_iter().next() {
                            thumbnails.push(Image::from_frame(frame));
                        }
                    }
                    Err(e) => {
                        log_warn!("Failed to decode TIFF reduced-resolution IFD: {}", e);
                        self.release_ifd_reservation();
                    }
                },
                Err(e) => log_warn!("Failed to read TIFF reduced-resolution IFD: {}", e),
            }
        }

        Ok(thumbnails)
    }

    fn read_file_header(&mut self) -> VexelResult<u32> {
        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;
        Ok(file_header.first_ifd_offset)
    }

    fn read_ifd(&mut self, ifd_offset: u32) -> VexelResult<u32> {
        let ifd = Ifd::read(&mut self.reader, ifd_offset, self.byte_order, &mut self.limits)?;
        let ifd_end = self.reader.stream_position()?;

        self.subfile_type = ifd_subfile_type(&ifd, &mut self.reader);

        let (header, photometric_known) = parse_header(&ifd, &mut self.reader, &mut self.limits);
        self.header = header;

        validate_header(&mut self.header, photometric_known);

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        let bytes_per_pixel = self.output_bytes_per_pixel();
        self.limits.reserve_buffer(self.width, self.height, bytes_per_pixel)?;
        self.reserved_output =
            u64::from(self.width).saturating_mul(u64::from(self.height)).saturating_mul(u64::from(bytes_per_pixel));

        self.reader.seek(SeekFrom::Start(ifd_end))?;

        Ok(ifd.next_ifd_offset)
    }

    fn output_bytes_per_pixel(&self) -> u8 {
        if matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG) {
            return if self.bits_for(0) > 8 { 6 } else { 3 };
        }

        let spp = self.header.samples_per_pixel.max(1) as usize;
        let sample_bytes = (0..spp)
            .map(|c| match (PixelReader::sample_format_for(&self.header, c), self.bits_for(c)) {
                (SampleFormat::Float, bits) if bits > 32 => 8,
                (SampleFormat::Float, _) => 4,
                (_, 0..=8) => 1,
                (_, 9..=16) => 2,
                (_, 17..=32) => 4,
                _ => 8,
            })
            .max()
            .unwrap_or(1);

        match self.header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero
            | PhotometricInterpretation::BlackIsZero
            | PhotometricInterpretation::TransparencyMask => sample_bytes * if spp > 1 { 2 } else { 1 },
            PhotometricInterpretation::LogL => 4,
            PhotometricInterpretation::LogLuv => 12,
            PhotometricInterpretation::Palette => {
                if spp > 1 {
                    8
                } else {
                    6
                }
            }
            PhotometricInterpretation::YCbCr => {
                let deep =
                    self.bits_for(0) != 8 || PixelReader::sample_format_for(&self.header, 0) == SampleFormat::Float;
                sample_bytes * 3 + if deep { 12 } else { 0 }
            }
            PhotometricInterpretation::RGB
            | PhotometricInterpretation::CIELab
            | PhotometricInterpretation::ICCLab
            | PhotometricInterpretation::ITULab => sample_bytes * if spp > 3 { 4 } else { 3 },
            PhotometricInterpretation::CMYK => sample_bytes * if spp > 4 { 4 } else { 3 },
        }
    }

    fn bits_for(&self, channel: usize) -> u16 {
        self.header.bits_per_sample.get(channel).or_else(|| self.header.bits_per_sample.first()).copied().unwrap_or(8)
    }

    fn chunk_layout(&self, width: usize, rows: usize, samples_per_pixel: usize) -> ChunkLayout {
        ChunkLayout {
            width,
            rows,
            samples_per_pixel,
            bits_per_sample: self.bits_for(0),
            sample_format: self.header.sample_format.first().copied().unwrap_or_default(),
            byte_order: self.byte_order,
        }
    }

    fn stream_len(&mut self) -> u64 {
        let pos = self.reader.stream_position().unwrap_or(0);
        let len = self.reader.seek(SeekFrom::End(0)).unwrap_or(0);
        let _ = self.reader.seek(SeekFrom::Start(pos));
        len
    }

    fn read_chunk_bytes(&mut self, offset: u32, byte_count: usize, stream_len: u64) -> VexelResult<Vec<u8>> {
        let available = stream_len.saturating_sub(offset as u64) as usize;
        let len = byte_count.min(available);
        if len < byte_count {
            log_warn!(
                "Chunk at offset {} claims {} bytes but only {} are available, reading what is there",
                offset,
                byte_count,
                len
            );
        }

        self.reader.seek(SeekFrom::Start(offset as u64))?;
        self.limits.reserve_usize(len)?;
        let mut data = vec![0u8; len];
        let result = self.reader.read_exact(&mut data);
        self.limits.free_usize(len);
        result?;

        if self.header.fill_order == 2 && !matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG) {
            for b in &mut data {
                *b = b.reverse_bits();
            }
        }

        Ok(data)
    }

    fn decompress_chunk(
        &self,
        data: Vec<u8>,
        chunk_width: usize,
        chunk_rows: usize,
        chunk_spp: usize,
        expected_bytes: usize,
    ) -> Vec<u8> {
        match self.header.compression {
            Compression::None => data,
            Compression::LZW => decompress_lzw_with_capacity(&data, expected_bytes),
            Compression::PackBits => decompress_packbits_with_capacity(&data, expected_bytes),
            Compression::AdobeDeflate | Compression::Deflate => decompress_deflate_with_capacity(&data, expected_bytes),
            Compression::LZMA => decompress_lzma_with_capacity(&data, expected_bytes),
            Compression::ZSTD => decompress_zstd_with_capacity(&data, expected_bytes),
            Compression::CCITTRLE | Compression::CCITTRLEWord | Compression::CCITTFax3 | Compression::CCITTFax4 => {
                decompress_ccitt(&data, chunk_width as u32, chunk_rows, self.header.compression, self.header.t4_options)
            }
            Compression::ThunderScan => decompress_thunderscan(&data, chunk_width, chunk_rows),
            Compression::NeXT => decompress_next(&data, chunk_width, chunk_rows),
            Compression::SGILog24 => decompress_sgilog24(&data, chunk_width, chunk_rows),
            Compression::SGILog => {
                if self.header.photometric_interpretation == PhotometricInterpretation::LogL {
                    decompress_sgilog_l16(&data, chunk_width, chunk_rows)
                } else {
                    decompress_sgilog(&data, chunk_width, chunk_rows)
                }
            }
            Compression::PNG => {
                decompress_png(&data, self.chunk_layout(chunk_width, chunk_rows, chunk_spp), &self.limits)
            }
            Compression::JPEG2000 => {
                decompress_jpeg2000(&data, self.chunk_layout(chunk_width, chunk_rows, chunk_spp), &self.limits)
            }
            Compression::JPEGXR => {
                decompress_jpeg_xr(&data, self.chunk_layout(chunk_width, chunk_rows, chunk_spp), &self.limits)
            }
            Compression::LERC => {
                let blob = match self.header.lerc_additional_compression {
                    LercAdditionalCompression::None => data,
                    LercAdditionalCompression::Deflate => decompress_deflate_with_capacity(&data, expected_bytes),
                    LercAdditionalCompression::Zstd => decompress_zstd_with_capacity(&data, expected_bytes),
                };

                decompress_lerc(
                    &blob,
                    chunk_width,
                    chunk_rows,
                    chunk_spp,
                    self.bits_for(0),
                    self.header.sample_format.first().copied().unwrap_or_default(),
                )
            }
            _ => {
                log_warn!(
                    "Unsupported compression method {:?}, using raw data, image will be incorrect",
                    self.header.compression
                );
                data
            }
        }
    }

    fn apply_predictor(&self, data: &mut [u8], chunk_width: usize, spp: u16) {
        let bps = self.bits_for(0);
        match self.header.predictor {
            Predictor::HorizontalDifferencing => {
                apply_predictor_horizontal(data, chunk_width as u32, spp, bps, self.byte_order == ByteOrder::BigEndian);
            }
            Predictor::FloatingPoint | Predictor::FloatingPointX2 | Predictor::FloatingPointX4 => {
                let x = predictor_x_factor(self.header.predictor);
                apply_predictor_float(
                    data,
                    (chunk_width as u32 / x).max(1),
                    spp.saturating_mul(x as u16),
                    bps,
                    self.byte_order == ByteOrder::BigEndian,
                );
            }
            _ => {}
        }
    }

    fn read_image_chunks(&mut self) -> VexelResult<Vec<Vec<u8>>> {
        let width = self.width as usize;
        let height = self.height as usize;
        let spp_total = self.header.samples_per_pixel.max(1) as usize;
        let bps = self.bits_for(0).clamp(1, 64) as usize;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();
        let is_planar = self.header.planar_configuration == PlanarConfiguration::Planar && spp_total > 1;
        let planes = if is_planar { spp_total } else { 1 };
        let chunk_spp = if is_planar { 1 } else { spp_total };

        let pixel_bits: usize = (0..spp_total).map(|c| self.bits_for(c).clamp(1, 64) as usize).sum();
        let uniform_bits = pixel_bits == spp_total * bps;

        let (asm_spp, asm_bps) = match self.header.compression {
            Compression::SGILog if self.header.photometric_interpretation == PhotometricInterpretation::LogL => (1, 16),
            Compression::SGILog | Compression::SGILog24 => (1, 32),
            _ if !uniform_bits && !is_planar => (1, pixel_bits),
            _ => (chunk_spp, bps),
        };

        let [h_sub, v_sub] = self.header.ycbcr_sub_sampling;
        let (h_sub, v_sub) = (h_sub.max(1) as usize, v_sub.max(1) as usize);
        let is_subsampled_ycbcr = self.header.photometric_interpretation == PhotometricInterpretation::YCbCr
            && !is_planar
            && (h_sub > 1 || v_sub > 1)
            && uniform_bits;

        let mut depth = self.header.image_depth.max(1) as usize;
        if depth > 1 && !is_tiled {
            log_warn!("ImageDepth {} is only supported for tiled images, decoding a single slice", depth);
            depth = 1;
        }
        if depth > MAX_IMAGE_DEPTH {
            log_warn!("ImageDepth {} is implausibly large, decoding the first {} slices", depth, MAX_IMAGE_DEPTH);
            depth = MAX_IMAGE_DEPTH;
        }

        let (chunk_w, chunk_h, chunk_d, offsets, byte_counts) = if is_tiled {
            (
                (self.header.tile_width.unwrap_or(self.width) as usize).max(1),
                (self.header.tile_length.unwrap_or(self.height) as usize).max(1),
                self.header.tile_depth.max(1) as usize,
                self.header.tile_offsets.clone(),
                self.header.tile_byte_counts.clone(),
            )
        } else {
            (
                width,
                (self.header.rows_per_strip as usize).min(height).max(1),
                1,
                self.header.strip_offsets.clone(),
                self.header.strip_byte_counts.clone(),
            )
        };

        if offsets.is_empty() {
            log_warn!("TIFF IFD has no strip or tile offsets, producing an empty image");
        }

        let (asm_w, asm_h, asm_chunk_w, asm_chunk_h, asm_spp) = if is_subsampled_ycbcr {
            (
                width.div_ceil(h_sub),
                height.div_ceil(v_sub),
                chunk_w.div_ceil(h_sub),
                chunk_h.div_ceil(v_sub),
                h_sub * v_sub + 2,
            )
        } else {
            (width, height, chunk_w, chunk_h, asm_spp)
        };

        let mut asm = ChunkAssembler::new(
            asm_w,
            asm_h,
            depth,
            asm_spp,
            asm_bps,
            planes,
            asm_chunk_w,
            asm_chunk_h,
            chunk_d,
            &mut self.limits,
        )?;

        if !offsets.is_empty() && offsets.len() != asm.total_chunks() {
            log_warn!(
                "Expected {} strips/tiles but the IFD lists {}, decoding what is available",
                asm.total_chunks(),
                offsets.len()
            );
        }
        let chunk_count = offsets.len().min(asm.total_chunks());
        if byte_counts.len() < offsets.len() {
            log_warn!(
                "StripByteCounts/TileByteCounts has {} entries for {} chunks, estimating the missing sizes",
                byte_counts.len(),
                offsets.len()
            );
        }

        let tile_slices = chunk_d.min(depth);
        let (codec_chunk_h, asm_rows) = if tile_slices > 1 {
            (chunk_h.saturating_mul(tile_slices), asm.chunk_h.saturating_mul(tile_slices))
        } else {
            (chunk_h.min(height), asm.chunk_h.min(asm.height))
        };

        let stream_len = self.stream_len();
        let expected_bytes = asm.chunk_row_bytes().saturating_mul(asm_rows);
        let is_sgilog = matches!(self.header.compression, Compression::SGILog | Compression::SGILog24);
        let has_predictor = self.header.predictor != Predictor::None && !is_sgilog;

        let is_embedded_codec = matches!(
            self.header.compression,
            Compression::PNG | Compression::JPEG2000 | Compression::JPEGXR | Compression::LERC
        );
        let codec_output_bytes = if is_embedded_codec {
            chunk_w
                .saturating_mul(codec_chunk_h)
                .saturating_mul(chunk_spp)
                .saturating_mul((self.bits_for(0) as usize).div_ceil(8).max(1))
        } else {
            0
        };
        let gate_bytes = expected_bytes.max(codec_output_bytes);

        let slices_bytes = asm.slices.iter().map(|s| s.len()).sum::<usize>();
        if let Err(e) = self.limits.reserve_usize(gate_bytes) {
            self.limits.free_usize(slices_bytes);
            return Err(e);
        }
        self.limits.free_usize(gate_bytes);
        let mut read_budget =
            stream_len.saturating_add((expected_bytes as u64).saturating_mul(chunk_count as u64).saturating_mul(2));

        for (chunk_idx, &offset) in offsets.iter().enumerate().take(chunk_count) {
            let byte_count = match byte_counts.get(chunk_idx) {
                Some(&c) if c > 0 => c as usize,
                _ => {
                    if self.header.compression == Compression::None {
                        expected_bytes
                    } else {
                        stream_len.saturating_sub(offset as u64) as usize
                    }
                }
            };

            if (byte_count as u64).min(stream_len.saturating_sub(offset as u64)) > read_budget {
                log_warn!("Strips/tiles read far more data than the file holds, stopping at chunk {}", chunk_idx);
                break;
            }

            let raw = match self.read_chunk_bytes(offset, byte_count, stream_len) {
                Ok(raw) => raw,
                Err(e) => {
                    log_warn!("Failed to read chunk {} at offset {}: {}", chunk_idx, offset, e);
                    continue;
                }
            };
            read_budget = read_budget.saturating_sub(raw.len() as u64);

            let chunk_rows = if is_tiled { codec_chunk_h } else { asm.rows_in_chunk(chunk_idx) };

            let mut data = self.decompress_chunk(raw, chunk_w, chunk_rows, chunk_spp, expected_bytes);
            let reserved = data.len();
            if let Err(e) = self.limits.reserve_usize(reserved) {
                log_warn!("Skipping chunk {}: {}", chunk_idx, e);
                continue;
            }

            if has_predictor {
                data.truncate(expected_bytes);
                self.apply_predictor(&mut data, chunk_w, chunk_spp as u16);
            }

            asm.place(chunk_idx, &data);
            self.limits.free_usize(reserved);
        }

        Ok(asm.slices)
    }

    fn decode_jpeg_bytes(&self, jpeg_data: Vec<u8>, wide: bool) -> Option<Vec<u8>> {
        let cursor = Cursor::new(jpeg_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);
        jpeg_decoder.set_limits(self.limits.clone());
        match jpeg_decoder.decode() {
            Ok(image) => Some(jpeg_rgb_bytes(image.into_pixels(), wide, self.byte_order)),
            Err(e) => {
                log_warn!("Failed to decode JPEG-compressed TIFF chunk: {}", e);
                None
            }
        }
    }

    fn read_image_jpeg(&mut self) -> VexelResult<PixelData> {
        let width = self.width as usize;
        let height = self.height as usize;
        let is_ojpeg = self.header.compression == Compression::OldJPEG;
        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();

        if self.header.planar_configuration == PlanarConfiguration::Planar && self.header.samples_per_pixel > 1 {
            log_warn!("Planar JPEG-compressed TIFF is not supported, image will be incorrect");
        }
        if self.header.photometric_interpretation == PhotometricInterpretation::CMYK {
            log_warn!("JPEG-compressed CMYK TIFF is decoded as RGB, colors may be incorrect");
        }

        let wide = self.bits_for(0) > 8;
        let bytes_per_sample = if wide { 2 } else { 1 };
        let image_len =
            width.checked_mul(height).and_then(|n| n.checked_mul(3 * bytes_per_sample)).ok_or_else(|| {
                VexelError::LimitExceeded(format!("TIFF image {}x{} is too large to decode", width, height))
            })?;
        self.limits.reserve_usize(image_len)?;
        let stream_len = self.stream_len();

        if is_ojpeg && self.header.jpeg_interchange_format != 0 {
            let offset = self.header.jpeg_interchange_format;
            let declared = self.header.jpeg_interchange_format_length as usize;
            let available = stream_len.saturating_sub(offset as u64) as usize;
            let len = if declared == 0 { available } else { declared.min(available) };
            let span_end = offset as u64 + len as u64;

            let first_data_offset = self.header.strip_offsets.first().or(self.header.tile_offsets.first()).copied();
            let data_inside_span = first_data_offset.is_none_or(|o| o >= offset && (o as u64) < span_end);

            if data_inside_span {
                if let Ok(raw) = self.read_chunk_bytes(offset, len, stream_len) {
                    let has_scan = raw.windows(2).any(|w| w == [0xFF, 0xDA]);
                    if raw.starts_with(&[0xFF, 0xD8]) && has_scan {
                        if let Some(pixels) = self.decode_jpeg_bytes(raw, wide) {
                            let mut image_data = vec![0u8; image_len];
                            let n = pixels.len().min(image_len);
                            image_data[..n].copy_from_slice(&pixels[..n]);
                            self.limits.free_usize(image_len);
                            return Ok(jpeg_pixel_data(image_data, wide, self.byte_order));
                        }
                    } else {
                        log_warn!(
                            "JPEGInterchangeFormat does not contain a full JPEG stream, falling back to strip data"
                        );
                    }
                }
            }
        }

        let (chunk_w, chunk_h, offsets, byte_counts) = if is_tiled {
            (
                self.header.tile_width.unwrap_or(self.width) as usize,
                self.header.tile_length.unwrap_or(self.height) as usize,
                self.header.tile_offsets.clone(),
                self.header.tile_byte_counts.clone(),
            )
        } else {
            (
                width,
                (self.header.rows_per_strip as usize).min(height).max(1),
                self.header.strip_offsets.clone(),
                self.header.strip_byte_counts.clone(),
            )
        };

        if is_ojpeg && !is_tiled && offsets.len() > 1 {
            let [h_sub, v_sub] = self.header.ycbcr_sub_sampling;
            let mcu_width = 8 * h_sub.max(1) as usize;
            let mcu_height = 8 * v_sub.max(1) as usize;
            let mcus_per_row = width.div_ceil(mcu_width);
            let mcu_rows_per_strip = chunk_h.div_ceil(mcu_height);
            let restart_interval = mcus_per_row.saturating_mul(mcu_rows_per_strip).min(u16::MAX as usize) as u16;

            let mut scan_data = Vec::new();
            let mut read_budget = stream_len.saturating_mul(2);
            for (strip_idx, &offset) in offsets.iter().enumerate().take(height.div_ceil(chunk_h)) {
                let byte_count = byte_counts.get(strip_idx).copied().unwrap_or(0) as usize;
                if (byte_count as u64).min(stream_len.saturating_sub(offset as u64)) > read_budget {
                    log_warn!("JPEG strips read far more data than the file holds, stopping at strip {}", strip_idx);
                    break;
                }
                let Ok(strip_data) = self.read_chunk_bytes(offset, byte_count, stream_len) else {
                    continue;
                };
                read_budget = read_budget.saturating_sub(strip_data.len() as u64);
                if strip_idx > 0 {
                    scan_data.push(0xFF);
                    scan_data.push(0xD0 + ((strip_idx - 1) % 8) as u8);
                }
                scan_data.extend_from_slice(&strip_data);
            }

            let jpeg_data = self.build_ojpeg_stream(&scan_data, self.width, self.height, restart_interval);
            let mut image_data = vec![0u8; image_len];
            if let Some(pixels) = self.decode_jpeg_bytes(jpeg_data, wide) {
                let n = pixels.len().min(image_len);
                image_data[..n].copy_from_slice(&pixels[..n]);
            }
            self.limits.free_usize(image_len);
            return Ok(jpeg_pixel_data(image_data, wide, self.byte_order));
        }

        let mut asm = match ChunkAssembler::new(
            width,
            height,
            1,
            3,
            bytes_per_sample * 8,
            1,
            chunk_w,
            chunk_h,
            1,
            &mut self.limits,
        ) {
            Ok(asm) => asm,
            Err(e) => {
                self.limits.free_usize(image_len);
                return Err(e);
            }
        };
        let jpeg_tables = self.header.jpeg_tables.clone();
        let mut read_budget = stream_len.saturating_add((image_len as u64).saturating_mul(2));

        for (chunk_idx, &offset) in offsets.iter().enumerate().take(asm.total_chunks()) {
            let byte_count = match byte_counts.get(chunk_idx) {
                Some(&c) if c > 0 => c as usize,
                _ => stream_len.saturating_sub(offset as u64) as usize,
            };
            if (byte_count as u64).min(stream_len.saturating_sub(offset as u64)) > read_budget {
                log_warn!("JPEG strips/tiles read far more data than the file holds, stopping at chunk {}", chunk_idx);
                break;
            }
            let Ok(raw) = self.read_chunk_bytes(offset, byte_count, stream_len) else {
                continue;
            };
            read_budget = read_budget.saturating_sub(raw.len() as u64);

            let chunk_rows = if is_tiled { chunk_h } else { asm.rows_in_chunk(chunk_idx) };
            let jpeg_data = if is_ojpeg {
                self.build_ojpeg_stream(&raw, chunk_w as u32, chunk_rows as u32, 0)
            } else if !jpeg_tables.is_empty() {
                splice_jpeg_tables(&jpeg_tables, &raw)
            } else {
                raw
            };

            if let Some(pixels) = self.decode_jpeg_bytes(jpeg_data, wide) {
                asm.place(chunk_idx, &pixels);
            }
        }

        let total: usize = asm.slices.iter().map(|s| s.len()).sum();
        self.limits.free_usize(total);
        self.limits.free_usize(image_len);

        Ok(jpeg_pixel_data(asm.slices.into_iter().next().unwrap_or_default(), wide, self.byte_order))
    }

    fn read_ojpeg_dqt_segments(&mut self) -> (Vec<u8>, Vec<u8>) {
        let mut out = Vec::new();
        let mut table_ids = Vec::with_capacity(self.header.jpeg_q_tables.len().min(MAX_OJPEG_TABLES));
        let mut last_offset = 0u32;
        let mut last_id = 0u8;

        for (idx, &offset) in self.header.jpeg_q_tables.clone().iter().enumerate().take(MAX_OJPEG_TABLES) {
            if offset == 0 || (idx > 0 && offset == last_offset) {
                table_ids.push(last_id);
                continue;
            }

            let mut table = [0u8; 64];
            if self.reader.seek(SeekFrom::Start(offset as u64)).is_err() || self.reader.read_exact(&mut table).is_err()
            {
                table_ids.push(last_id);
                continue;
            }

            out.push(0xFF);
            out.push(0xDB);
            out.extend_from_slice(&67u16.to_be_bytes());
            out.push(idx as u8);
            out.extend_from_slice(&table);

            last_offset = offset;
            last_id = idx as u8;
            table_ids.push(last_id);
        }

        (out, table_ids)
    }

    fn read_ojpeg_dht_segments(&mut self, offsets: &[u32], is_ac: bool) -> (Vec<u8>, Vec<u8>) {
        let mut out = Vec::new();
        let mut table_ids = Vec::with_capacity(offsets.len().min(MAX_OJPEG_TABLES));
        let mut last_offset = 0u32;
        let mut last_id = 0u8;

        for (idx, &offset) in offsets.iter().enumerate().take(MAX_OJPEG_TABLES) {
            if offset == 0 || (idx > 0 && offset == last_offset) {
                table_ids.push(last_id);
                continue;
            }

            let mut counts = [0u8; 16];
            if self.reader.seek(SeekFrom::Start(offset as u64)).is_err() || self.reader.read_exact(&mut counts).is_err()
            {
                table_ids.push(last_id);
                continue;
            }
            let total: usize = counts.iter().map(|&c| c as usize).sum();
            let mut symbols = vec![0u8; total];
            if self.reader.read_exact(&mut symbols).is_err() {
                table_ids.push(last_id);
                continue;
            }

            let table_class_id = if is_ac { 0x10 | idx as u8 } else { idx as u8 };
            let segment_len = 19 + total;

            out.push(0xFF);
            out.push(0xC4);
            out.extend_from_slice(&(segment_len as u16).to_be_bytes());
            out.push(table_class_id);
            out.extend_from_slice(&counts);
            out.extend_from_slice(&symbols);

            last_offset = offset;
            last_id = idx as u8;
            table_ids.push(last_id);
        }

        (out, table_ids)
    }

    fn build_ojpeg_stream(
        &mut self,
        scan_data: &[u8],
        width: u32,
        height: u32,
        restart_interval_override: u16,
    ) -> Vec<u8> {
        let spp = self.header.samples_per_pixel.clamp(1, 255);
        let [h_sub, v_sub] = self.header.ycbcr_sub_sampling;
        let h_sub = h_sub.max(1);
        let v_sub = v_sub.max(1);

        let (dqt, qtable_ids) = self.read_ojpeg_dqt_segments();
        let (dht_dc, dc_table_ids) = self.read_ojpeg_dht_segments(&self.header.jpeg_dc_tables.clone(), false);
        let (dht_ac, ac_table_ids) = self.read_ojpeg_dht_segments(&self.header.jpeg_ac_tables.clone(), true);
        let restart_interval =
            if restart_interval_override != 0 { restart_interval_override } else { self.header.jpeg_restart_interval };

        let tq = |c: usize| qtable_ids.get(c).copied().unwrap_or(0);
        let td = |c: usize| dc_table_ids.get(c).copied().unwrap_or(0);
        let ta = |c: usize| ac_table_ids.get(c).copied().unwrap_or(0);

        let mut out = Vec::new();
        out.push(0xFF);
        out.push(0xD8);
        out.extend_from_slice(&dqt);
        out.extend_from_slice(&dht_dc);
        out.extend_from_slice(&dht_ac);

        if restart_interval != 0 {
            out.push(0xFF);
            out.push(0xDD);
            out.extend_from_slice(&4u16.to_be_bytes());
            out.extend_from_slice(&restart_interval.to_be_bytes());
        }

        out.push(0xFF);
        out.push(0xC0);
        out.extend_from_slice(&(8 + spp * 3).to_be_bytes());
        out.push(8);
        out.extend_from_slice(&(height as u16).to_be_bytes());
        out.extend_from_slice(&(width as u16).to_be_bytes());
        out.push(spp as u8);
        for c in 0..spp as usize {
            out.push(c as u8);
            let hv = if c == 0 { (h_sub as u8) << 4 | v_sub as u8 } else { 0x11 };
            out.push(hv);
            out.push(tq(c));
        }

        if scan_data.starts_with(&[0xFF, 0xDA]) {
            out.extend_from_slice(scan_data);
        } else {
            out.push(0xFF);
            out.push(0xDA);
            out.extend_from_slice(&(6 + spp * 2).to_be_bytes());
            out.push(spp as u8);
            for c in 0..spp as usize {
                out.push(c as u8);
                out.push(td(c) << 4 | ta(c));
            }
            out.push(0);
            out.push(63);
            out.push(0);

            out.extend_from_slice(scan_data);
        }

        out.push(0xFF);
        out.push(0xD9);

        out
    }

    fn decode_current_ifd(&mut self) -> VexelResult<Vec<ImageFrame>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);

        if is_jpeg {
            let mut pixel_data = self.read_image_jpeg()?;
            pixel_data.correct_pixels(self.width, self.height);

            return Ok(vec![ImageFrame::new(self.width, self.height, pixel_data, 0)]);
        }

        let slices = self.read_image_chunks()?;
        let slices_bytes: usize = slices.iter().map(|s| s.len()).sum();
        let result = self.convert_slices(slices);
        self.limits.free_usize(slices_bytes);
        result
    }

    fn convert_slices(&mut self, slices: Vec<Vec<u8>>) -> VexelResult<Vec<ImageFrame>> {
        let pixel_reader = PixelReader { byte_order: self.byte_order, width: self.width, height: self.height };
        let is_planar = self.header.planar_configuration == PlanarConfiguration::Planar
            && self.header.samples_per_pixel > 1
            && self.header.photometric_interpretation != PhotometricInterpretation::YCbCr;

        let mut frames = Vec::with_capacity(slices.len());
        for (index, slice) in slices.into_iter().enumerate() {
            let extra_output = if index > 0 { self.reserved_output } else { 0 };
            let scratch = if is_planar { slice.len() } else { 0 };
            let converted = self.convert_slice(&pixel_reader, slice, extra_output, scratch);

            match converted {
                Ok(mut pixel_data) => {
                    pixel_data.correct_pixels(self.width, self.height);
                    frames.push(ImageFrame::new(self.width, self.height, pixel_data, 0));
                }
                Err(e) if index == 0 => return Err(e),
                Err(e) => {
                    log_warn!("Failed to decode TIFF slice {}: {}, keeping {} slices", index, e, index);
                    break;
                }
            }
        }

        Ok(frames)
    }

    fn convert_slice(
        &mut self,
        pixel_reader: &PixelReader,
        slice: Vec<u8>,
        extra_output: u64,
        scratch: usize,
    ) -> VexelResult<PixelData> {
        self.limits.reserve(extra_output)?;
        if let Err(e) = self.limits.reserve_usize(scratch) {
            self.limits.free(extra_output);
            return Err(e);
        }
        let converted = pixel_reader.convert_to_pixel_data(slice, &self.header);
        self.limits.free_usize(scratch);
        if converted.is_err() {
            self.limits.free(extra_output);
        }
        converted
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut pages = self.decode_ifd_chain(true)?;
        Ok(pages.remove(0))
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        self.decode_ifd_chain(false)
    }

    fn decode_ifd_chain(&mut self, first_only: bool) -> VexelResult<Vec<Image>> {
        self.reader.seek(SeekFrom::Start(0))?;

        let first_ifd_offset = self.read_file_header()?;
        let reduced_only = !self.chain_has_full_res(first_ifd_offset);

        let mut pages: Vec<Image> = Vec::new();
        let mut first_error: Option<VexelError> = None;
        let mut next_ifd_offset = first_ifd_offset;
        let mut guard = IfdCycleGuard::default();

        while next_ifd_offset != 0 {
            if first_only && !pages.is_empty() {
                break;
            }
            if !guard.enter(next_ifd_offset) {
                log_warn!("Cyclic or excessively long IFD chain detected at offset {}, stopping", next_ifd_offset);
                break;
            }

            next_ifd_offset = match self.read_ifd(next_ifd_offset) {
                Ok(offset) => offset,
                Err(e) => {
                    log_warn!("Failed to read TIFF IFD: {}", e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    break;
                }
            };

            let is_reduced_resolution = self.subfile_type & 1 != 0;
            if is_reduced_resolution && !reduced_only {
                self.release_ifd_reservation();
                continue;
            }

            match self.decode_current_ifd() {
                // TODO: an IFD with ImageDepth > 1 produces several slices, which are
                // flattened into separate pages here.
                Ok(ifd_frames) => {
                    pages.extend(ifd_frames.into_iter().map(Image::from_frame));
                }
                Err(e) => {
                    log_warn!("Failed to decode TIFF page: {}", e);
                    self.release_ifd_reservation();
                    if !pages.is_empty() {
                        break;
                    }
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if pages.is_empty() {
            return Err(first_error.unwrap_or_else(|| VexelError::Custom("No pages decoded from TIFF".to_string())));
        }

        Ok(pages)
    }

    fn release_ifd_reservation(&mut self) {
        self.limits.free(std::mem::take(&mut self.reserved_output));
    }

    fn chain_has_full_res(&mut self, first_ifd_offset: u32) -> bool {
        let mut guard = IfdCycleGuard::default();
        let mut offset = first_ifd_offset;

        while offset != 0 {
            if !guard.enter(offset) {
                break;
            }

            let Ok(ifd) = Ifd::read(&mut self.reader, offset, self.byte_order, &mut self.limits) else {
                break;
            };

            let subfile_type: u32 = ifd.value(TiffTags::NewSubfileType, &mut self.reader).unwrap_or(0);
            if subfile_type & 1 == 0 {
                return true;
            }

            offset = ifd.next_ifd_offset;
        }

        false
    }
}

fn ifd_subfile_type<R: Read + Seek>(ifd: &Ifd, reader: &mut BitReader<R>) -> u32 {
    ifd.value(TiffTags::NewSubfileType, reader).unwrap_or(0)
}

fn parse_header<R: Read + Seek>(ifd: &Ifd, reader: &mut BitReader<R>, limits: &mut Limits) -> (TiffHeader, bool) {
    let mut header = TiffHeader::default();
    if let Some(v) = ifd.value(TiffTags::ImageWidth, reader) {
        header.image_width = v;
    }
    if let Some(v) = ifd.value(TiffTags::ImageLength, reader) {
        header.image_length = v;
    }
    if let Some(v) = ifd.values(TiffTags::BitsPerSample, reader, limits) {
        header.bits_per_sample = v;
    }
    if let Some(v) = ifd.value::<u32, _>(TiffTags::Compression, reader) {
        match Compression::try_from(v as u16) {
            Ok(c) => header.compression = c,
            Err(_) => log_warn!("Unknown compression method {}, assuming uncompressed data", v),
        }
    }
    let mut photometric_known = false;
    if let Some(v) = ifd.value::<u32, _>(TiffTags::PhotometricInterpretation, reader) {
        match PhotometricInterpretation::try_from(v) {
            Ok(p) => {
                header.photometric_interpretation = p;
                photometric_known = true;
            }
            Err(_) => log_warn!("Unknown photometric interpretation {}, guessing from sample count", v),
        }
    }
    if let Some(v) = ifd.values(TiffTags::StripOffsets, reader, limits) {
        header.strip_offsets = v;
    }
    if let Some(v) = ifd.value(TiffTags::SamplesPerPixel, reader) {
        header.samples_per_pixel = v;
    }
    if let Some(v) = ifd.value(TiffTags::RowsPerStrip, reader) {
        header.rows_per_strip = v;
    }
    if let Some(v) = ifd.values(TiffTags::StripByteCounts, reader, limits) {
        header.strip_byte_counts = v;
    }
    if let Some(v) = ifd.rational(TiffTags::XResolution, reader) {
        header.x_resolution = v;
    }
    if let Some(v) = ifd.rational(TiffTags::YResolution, reader) {
        header.y_resolution = v;
    }
    if let Some(v) = ifd.value(TiffTags::PlanarConfiguration, reader) {
        header.planar_configuration = v;
    }
    if let Some(v) = ifd.value(TiffTags::ResolutionUnit, reader) {
        header.resolution_unit = v;
    }
    if let Some(v) = ifd.value::<u32, _>(TiffTags::Predictor, reader) {
        match Predictor::try_from(v) {
            Ok(p) => header.predictor = p,
            Err(_) => log_warn!("Unknown predictor {}, ignoring it", v),
        }
    }
    if let Some(v) = ifd.values(TiffTags::ColorMap, reader, limits) {
        header.color_map = v;
    }
    if let Some(v) = ifd.value(TiffTags::TileWidth, reader) {
        header.tile_width = Some(v);
    }
    if let Some(v) = ifd.value(TiffTags::TileLength, reader) {
        header.tile_length = Some(v);
    }
    if let Some(v) = ifd.values(TiffTags::TileOffsets, reader, limits) {
        header.tile_offsets = v;
    }
    if let Some(v) = ifd.values(TiffTags::TileByteCounts, reader, limits) {
        header.tile_byte_counts = v;
    }
    if header.tile_width.is_some() && header.tile_offsets.is_empty() && !header.strip_offsets.is_empty() {
        log_warn!("TIFF has TileWidth/TileLength but no TileOffsets, using StripOffsets as tile offsets");
        header.tile_offsets = std::mem::take(&mut header.strip_offsets);
        header.tile_byte_counts = std::mem::take(&mut header.strip_byte_counts);
    }
    if let Some(v) = ifd.value(TiffTags::ImageDepth, reader) {
        header.image_depth = v;
    }
    if let Some(v) = ifd.value(TiffTags::TileDepth, reader) {
        header.tile_depth = v;
    }
    if let Some(v) = ifd.values(TiffTags::ExtraSamples, reader, limits) {
        header.extra_samples = v;
    }
    if let Some(v) = ifd.values::<u32, _>(TiffTags::SampleFormat, reader, limits) {
        header.sample_format = v
            .into_iter()
            .map(|f| {
                SampleFormat::try_from(f).unwrap_or_else(|_| {
                    log_warn!("Unknown sample format {}, assuming unsigned integers", f);
                    SampleFormat::UnsignedInt
                })
            })
            .collect();
    }
    if let Some(v) = ifd.values::<u32, _>(TiffTags::LercParameters, reader, limits) {
        if let Some(&code) = v.get(1) {
            header.lerc_additional_compression = LercAdditionalCompression::from_code(code).unwrap_or_else(|| {
                log_warn!("Unknown LERC additional compression {}, assuming none", code);
                LercAdditionalCompression::None
            });
        }
    }
    if let Some(v) = ifd.value::<u32, _>(TiffTags::FillOrder, reader) {
        header.fill_order = v as u16;
    }
    if let Some(v) = ifd.value::<u32, _>(TiffTags::Orientation, reader) {
        header.orientation = v as u16;
    }
    if let Some(v) = ifd.value(TiffTags::T4Options, reader) {
        header.t4_options = v;
    }
    if let Some(v) = ifd.value(TiffTags::T6Options, reader) {
        header.t6_options = v;
    }
    if let Some(v) = ifd.byte_data(TiffTags::JPEGTables, reader, limits) {
        header.jpeg_tables = v;
    }
    if let Some(v) = ifd.values(TiffTags::JPEGQTables, reader, limits) {
        header.jpeg_q_tables = v;
    }
    if let Some(v) = ifd.values(TiffTags::JPEGDCTables, reader, limits) {
        header.jpeg_dc_tables = v;
    }
    if let Some(v) = ifd.values(TiffTags::JPEGACTables, reader, limits) {
        header.jpeg_ac_tables = v;
    }
    if let Some(v) = ifd.value(TiffTags::JPEGInterchangeFormat, reader) {
        header.jpeg_interchange_format = v;
    }
    if let Some(v) = ifd.value(TiffTags::JPEGInterchangeFormatLength, reader) {
        header.jpeg_interchange_format_length = v;
    }
    if let Some(v) = ifd.value(TiffTags::JPEGRestartInterval, reader) {
        header.jpeg_restart_interval = v;
    }
    if let Some(v) = ifd.rationals(TiffTags::YCbCrCoefficients, reader, limits) {
        if v.len() >= 3 {
            header.ycbcr_coefficients = [v[0], v[1], v[2]];
        }
    }
    if let Some(v) = ifd.values::<u32, _>(TiffTags::YCbCrSubSampling, reader, limits) {
        if v.len() >= 2 {
            header.ycbcr_sub_sampling = [v[0] as u16, v[1] as u16];
        }
    }
    if let Some(v) = ifd.rationals(TiffTags::ReferenceBlackWhite, reader, limits) {
        if v.len() >= 6 {
            header.reference_black_white = [v[0], v[1], v[2], v[3], v[4], v[5]];
        }
    }

    (header, photometric_known)
}

fn validate_header(header: &mut TiffHeader, photometric_known: bool) {
    if header.samples_per_pixel == 0 {
        log_warn!("SamplesPerPixel is 0, assuming 1");
        header.samples_per_pixel = 1;
    }

    if header.rows_per_strip == 0 {
        log_warn!("RowsPerStrip is 0, assuming a single strip");
        header.rows_per_strip = u32::MAX;
    }

    if !photometric_known && header.samples_per_pixel >= 3 {
        log_warn!("PhotometricInterpretation is missing or unknown, assuming RGB from sample count");
        header.photometric_interpretation = PhotometricInterpretation::RGB;
    }

    let bps0 = header.bits_per_sample.first().copied().unwrap_or(1);
    let forced_bps = match header.compression {
        Compression::CCITTRLE | Compression::CCITTRLEWord | Compression::CCITTFax3 | Compression::CCITTFax4
            if bps0 != 1 =>
        {
            Some(1)
        }
        Compression::ThunderScan if bps0 != 4 => Some(4),
        Compression::NeXT if bps0 != 2 => Some(2),
        _ => None,
    };
    if let Some(bps) = forced_bps {
        log_warn!(
            "{:?} compression requires {}-bit samples but BitsPerSample is {}, forcing {}",
            header.compression,
            bps,
            bps0,
            bps
        );
        header.bits_per_sample = vec![bps];
    }

    let [h_sub, v_sub] = header.ycbcr_sub_sampling;
    if !matches!(h_sub, 1 | 2 | 4) || !matches!(v_sub, 1 | 2 | 4) {
        log_warn!("Invalid YCbCr subsampling {}x{}, assuming 2x2", h_sub, v_sub);
        header.ycbcr_sub_sampling = [2, 2];
    }

    if header.fill_order != 1 && header.fill_order != 2 {
        log_warn!("Invalid FillOrder {}, assuming MSB-first", header.fill_order);
        header.fill_order = 1;
    }

    if header.orientation > 1 {
        log_warn!("Orientation {} is not applied, image will be shown as stored", header.orientation);
    }

    if header.predictor != Predictor::None {
        let non_uniform = header.bits_per_sample.len() > 1 && header.bits_per_sample.windows(2).any(|w| w[0] != w[1]);
        let float_predictor = matches!(
            header.predictor,
            Predictor::FloatingPoint | Predictor::FloatingPointX2 | Predictor::FloatingPointX4
        );
        if non_uniform {
            log_warn!("Predictor is not supported with per-channel bit depths, ignoring it");
            header.predictor = Predictor::None;
        } else if float_predictor && bps0 % 8 != 0 {
            log_warn!(
                "Floating-point predictor requires byte-aligned samples but BitsPerSample is {}, ignoring it",
                bps0
            );
            header.predictor = Predictor::None;
        }
    }
}
