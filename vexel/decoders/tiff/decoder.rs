use crate::bitreader::BitReader;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::ImageFrame;
use crate::utils::info::TiffInfo;
use crate::utils::types::ByteOrder;
use crate::{Image, Limits, log_warn};
use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::utils::tiff::compression::{
    apply_predictor_float, apply_predictor_horizontal, apply_predictor_horizontal_be, decompress_deflate,
    decompress_lzw, decompress_packbits, decompress_sgilog, decompress_sgilog24, predictor_x_factor,
};
use crate::utils::tiff::ifd::{self, Ifd};
use crate::utils::tiff::jpeg::splice_jpeg_tables;
use crate::utils::tiff::pixels::PixelReader;
use crate::utils::tiff::types::{
    Compression, Predictor, SampleFormat, TiffFileHeaderData, TiffHeader, TiffIfdData, TiffPixelData,
    TiffSectionData, TiffSectionInfo, TiffTags,
};

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    byte_order: ByteOrder,
    header: TiffHeader,
    reader: BitReader<R>,
    sections: Vec<TiffSectionInfo>,
    ifd_count: usize,
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
            sections: Vec::new(),
            ifd_count: 0,
        }
    }

    pub fn get_info(&self) -> TiffInfo {
        TiffInfo {
            sections: self.sections.clone(),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }


    fn read_file_header(&mut self) -> VexelResult<u32> {
        let header_start = self.reader.stream_position().unwrap_or(0);

        let file_header = ifd::read_file_header(&mut self.reader)?;
        self.byte_order = file_header.byte_order;

        self.sections.push(TiffSectionInfo {
            start_offset: header_start,
            data: TiffSectionData::FileHeader(TiffFileHeaderData {
                byte_order: if self.byte_order == ByteOrder::LittleEndian {
                    "LittleEndian".to_string()
                } else {
                    "BigEndian".to_string()
                },
                magic: file_header.magic,
                first_ifd_offset: file_header.first_ifd_offset,
            }),
        });

        Ok(file_header.first_ifd_offset)
    }

    fn read_ifd(&mut self, ifd_offset: u32) -> VexelResult<u32> {
        self.header = TiffHeader::default();
        let ifd_start = ifd_offset as u64;

        let ifd = Ifd::read(&mut self.reader, ifd_offset, self.byte_order, &mut self.limits)?;
        let ifd_end = self.reader.stream_position()?;

        if let Some(v) = ifd.value(TiffTags::ImageWidth, &mut self.reader) {
            self.header.image_width = v;
        }
        if let Some(v) = ifd.value(TiffTags::ImageLength, &mut self.reader) {
            self.header.image_length = v;
        }
        if let Some(v) = ifd.values(TiffTags::BitsPerSample, &mut self.reader, &mut self.limits) {
            self.header.bits_per_sample = v;
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::Compression, &mut self.reader) {
            self.header.compression = Compression::try_from(v as u16).unwrap_or(Compression::None);
        }
        if let Some(v) = ifd.value(TiffTags::PhotometricInterpretation, &mut self.reader) {
            self.header.photometric_interpretation = v;
        }
        if let Some(v) = ifd.values(TiffTags::StripOffsets, &mut self.reader, &mut self.limits) {
            self.header.strip_offsets = v;
        }
        if let Some(v) = ifd.value(TiffTags::SamplesPerPixel, &mut self.reader) {
            self.header.samples_per_pixel = v;
        }
        if let Some(v) = ifd.value(TiffTags::RowsPerStrip, &mut self.reader) {
            self.header.rows_per_strip = v;
        }
        if let Some(v) = ifd.values(TiffTags::StripByteCounts, &mut self.reader, &mut self.limits) {
            self.header.strip_byte_counts = v;
        }
        if let Some(v) = ifd.rational(TiffTags::XResolution, &mut self.reader) {
            self.header.x_resolution = v;
        }
        if let Some(v) = ifd.rational(TiffTags::YResolution, &mut self.reader) {
            self.header.y_resolution = v;
        }
        if let Some(v) = ifd.value(TiffTags::PlanarConfiguration, &mut self.reader) {
            self.header.planar_configuration = v;
        }
        if let Some(v) = ifd.value(TiffTags::ResolutionUnit, &mut self.reader) {
            self.header.resolution_unit = v;
        }
        if let Some(v) = ifd.value::<u32, _>(TiffTags::Predictor, &mut self.reader) {
            self.header.predictor = Predictor::try_from(v).unwrap_or(Predictor::None);
        }
        if let Some(v) = ifd.values(TiffTags::ColorMap, &mut self.reader, &mut self.limits) {
            self.header.color_map = v;
        }
        if let Some(v) = ifd.value(TiffTags::TileWidth, &mut self.reader) {
            self.header.tile_width = Some(v);
        }
        if let Some(v) = ifd.value(TiffTags::TileLength, &mut self.reader) {
            self.header.tile_length = Some(v);
        }
        if let Some(v) = ifd.values(TiffTags::TileOffsets, &mut self.reader, &mut self.limits) {
            self.header.tile_offsets = v;
        }
        if let Some(v) = ifd.values(TiffTags::TileByteCounts, &mut self.reader, &mut self.limits) {
            self.header.tile_byte_counts = v;
        }
        if let Some(v) = ifd.value(TiffTags::ImageDepth, &mut self.reader) {
            self.header.image_depth = v;
        }
        if let Some(v) = ifd.value(TiffTags::TileDepth, &mut self.reader) {
            self.header.tile_depth = v;
        }
        if let Some(v) = ifd.values(TiffTags::ExtraSamples, &mut self.reader, &mut self.limits) {
            self.header.extra_samples = v;
        }
        if let Some(v) = ifd.values::<u32, _>(TiffTags::SampleFormat, &mut self.reader, &mut self.limits) {
            self.header.sample_format = v
                .into_iter()
                .map(|f| SampleFormat::try_from(f).unwrap_or(SampleFormat::UnsignedInt))
                .collect();
        }
        if let Some(v) = ifd.byte_data(TiffTags::JPEGTables, &mut self.reader, &mut self.limits) {
            self.header.jpeg_tables = v;
        }
        if let Some(v) = ifd.rationals(TiffTags::YCbCrCoefficients, &mut self.reader, &mut self.limits) {
            if v.len() >= 3 {
                self.header.ycbcr_coefficients = [v[0], v[1], v[2]];
            }
        }
        if let Some(v) = ifd.values::<u32, _>(TiffTags::YCbCrSubSampling, &mut self.reader, &mut self.limits) {
            if v.len() >= 2 {
                self.header.ycbcr_sub_sampling = [v[0] as u16, v[1] as u16];
            }
        }
        if let Some(v) = ifd.rationals(TiffTags::ReferenceBlackWhite, &mut self.reader, &mut self.limits) {
            if v.len() >= 6 {
                self.header.reference_black_white = [v[0], v[1], v[2], v[3], v[4], v[5]];
            }
        }

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        self.limits.reserve_buffer(self.width, self.height, 4)?;

        self.reader.seek(SeekFrom::Start(ifd_end))?;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();
        let is_volumetric = is_tiled && self.header.image_depth.max(1) > 1;
        let ifd_index = self.ifd_count;
        self.ifd_count += 1;

        self.sections.push(TiffSectionInfo {
            start_offset: ifd_start,
            data: TiffSectionData::Ifd(TiffIfdData {
                ifd_index,
                image_width: self.header.image_width,
                image_length: self.header.image_length,
                bits_per_sample: self.header.bits_per_sample.clone(),
                compression: self.header.compression,
                photometric_interpretation: self.header.photometric_interpretation,
                samples_per_pixel: self.header.samples_per_pixel,
                rows_per_strip: self.header.rows_per_strip,
                strip_count: self.header.strip_offsets.len(),
                strip_offsets: self.header.strip_offsets.clone(),
                strip_byte_counts: self.header.strip_byte_counts.clone(),
                x_resolution: self.header.x_resolution,
                y_resolution: self.header.y_resolution,
                resolution_unit: self.header.resolution_unit,
                planar_configuration: self.header.planar_configuration,
                predictor: self.header.predictor,
                sample_format: self.header.sample_format.clone(),
                extra_samples: self.header.extra_samples.clone(),
                color_map_entries: self.header.color_map.len(),
                ycbcr_coefficients: self.header.ycbcr_coefficients,
                ycbcr_sub_sampling: self.header.ycbcr_sub_sampling,
                reference_black_white: self.header.reference_black_white,
                tile_width: self.header.tile_width,
                tile_length: self.header.tile_length,
                tile_count: self.header.tile_offsets.len(),
                tile_offsets: self.header.tile_offsets.clone(),
                tile_byte_counts: self.header.tile_byte_counts.clone(),
                jpeg_tables_length: self.header.jpeg_tables.len(),
                image_depth: self.header.image_depth,
                tile_depth: self.header.tile_depth,
                is_tiled,
                is_volumetric,
            }),
        });

        Ok(ifd.next_ifd_offset)
    }

    fn bits_for(&self, channel: usize) -> u16 {
        self.header.bits_per_sample.get(channel).copied().unwrap_or(8)
    }

    fn decompress_chunk(&self, data: Vec<u8>) -> Vec<u8> {
        match self.header.compression {
            Compression::None => data,
            Compression::LZW => decompress_lzw(&data),
            Compression::PackBits => decompress_packbits(&data),
            Compression::AdobeDeflate | Compression::Deflate => decompress_deflate(&data),
            Compression::JPEG => self.decompress_jpeg_strip(data),
            Compression::OldJPEG => self.decompress_jpeg_strip(data),
            Compression::PNG => self.decompress_png_strip(data),
            _ => {
                log_warn!("Unsupported compression method {:?}, using raw data, image will be incorrect", self.header.compression);
                data
            },
        }
    }

    fn decompress_png_strip(&self, strip_data: Vec<u8>) -> Vec<u8> {
        let cursor = Cursor::new(strip_data);
        let mut png_decoder = PngDecoder::new(cursor);

        match png_decoder.decode() {
            Ok(image) => image.into_pixels().into_rgb8_bytes(),
            Err(_) => Vec::new(),
        }
    }

    fn decompress_jpeg_strip(&self, strip_data: Vec<u8>) -> Vec<u8> {
        let jpeg_data = if !self.header.jpeg_tables.is_empty() {
            splice_jpeg_tables(&self.header.jpeg_tables, &strip_data)
        } else {
            strip_data
        };

        let cursor = Cursor::new(jpeg_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);

        match jpeg_decoder.decode() {
            Ok(image) => image.into_pixels().into_rgb8_bytes(),
            Err(_) => Vec::new(),
        }
    }

    fn apply_predictor(&self, data: &mut [u8], strip_width: u32) {
        let bps = self.bits_for(0);
        let spp = self.header.samples_per_pixel;
        match self.header.predictor {
            Predictor::HorizontalDifferencing => {
                if self.byte_order == ByteOrder::BigEndian {
                    apply_predictor_horizontal_be(data, strip_width, spp, bps);
                } else {
                    apply_predictor_horizontal(data, strip_width, spp, bps);
                }
            }
            Predictor::FloatingPoint | Predictor::FloatingPointX2 | Predictor::FloatingPointX4 => {
                let x = predictor_x_factor(self.header.predictor);
                apply_predictor_float(data, (strip_width / x).max(1), spp * x as u16, bps, self.byte_order == ByteOrder::BigEndian);
            }
            _ => {}
        }
    }

    fn apply_predictor_planar(&self, data: &mut [u8], strip_width: u32) {
        let bps = self.bits_for(0);
        match self.header.predictor {
            Predictor::HorizontalDifferencing => {
                if self.byte_order == ByteOrder::BigEndian {
                    apply_predictor_horizontal_be(data, strip_width, 1, bps);
                } else {
                    apply_predictor_horizontal(data, strip_width, 1, bps);
                }
            }
            Predictor::FloatingPoint | Predictor::FloatingPointX2 | Predictor::FloatingPointX4 => {
                let x = predictor_x_factor(self.header.predictor);
                apply_predictor_float(data, (strip_width / x).max(1), x as u16, bps, self.byte_order == ByteOrder::BigEndian);
            }
            _ => {}
        }
    }

    fn read_strip_data(&mut self) -> VexelResult<Vec<u8>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);

        if is_jpeg {
            return self.read_strip_data_jpeg();
        }

        let is_sgilog = matches!(self.header.compression, Compression::SGILog | Compression::SGILog24);

        let rows_per_strip = self.header.rows_per_strip;
        let image_width = self.width;
        let is_planar = self.header.planar_configuration == crate::utils::tiff::types::PlanarConfiguration::Planar
            && self.header.samples_per_pixel > 1;
        let mut bytes = Vec::new();

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();
        let total_strips = offsets.len();
        let strips_per_plane = if is_planar && self.header.samples_per_pixel > 0 {
            total_strips / self.header.samples_per_pixel as usize
        } else {
            total_strips
        };

        for (strip_idx, (offset, byte_count)) in offsets.iter().zip(byte_counts.iter()).enumerate() {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let strip_len = *byte_count as usize;
            self.limits.reserve_usize(strip_len)?;
            let mut strip_data = vec![0u8; strip_len];
            self.reader.read_exact(&mut strip_data)?;

            let strip_within_plane = if is_planar && strips_per_plane > 0 {
                strip_idx % strips_per_plane
            } else {
                strip_idx
            };
            let strip_row_start = (strip_within_plane as u32).saturating_mul(rows_per_strip);
            let strip_row_end = strip_row_start.saturating_add(rows_per_strip).min(self.height);
            let strip_rows = strip_row_end.saturating_sub(strip_row_start) as usize;

            let mut decompressed = if is_sgilog {
                if matches!(self.header.compression, Compression::SGILog24) {
                    decompress_sgilog24(&strip_data, image_width as usize, strip_rows)
                } else {
                    decompress_sgilog(&strip_data, image_width as usize, strip_rows)
                }
            } else {
                self.decompress_chunk(strip_data)
            };
            self.limits.free_usize(strip_len);

            self.limits.reserve_usize(decompressed.len())?;

            if self.header.predictor != Predictor::None && !is_sgilog {
                let bps = self.bits_for(0);
                if bps >= 8 {
                    let stride = if is_planar { 1u16 } else { self.header.samples_per_pixel };
                    let row_bytes = image_width as usize * stride as usize * (bps as usize / 8);
                    let expected_len = strip_rows * row_bytes;
                    decompressed.truncate(expected_len);
                    if is_planar {
                        self.apply_predictor_planar(&mut decompressed, image_width);
                    } else {
                        self.apply_predictor(&mut decompressed, image_width);
                    }
                }
            }

            let decompressed_len = decompressed.len();
            bytes.extend_from_slice(&decompressed);
            self.limits.free_usize(decompressed_len);
        }

        Ok(bytes)
    }

    fn read_strip_data_jpeg(&mut self) -> VexelResult<Vec<u8>> {
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let rows_per_strip = self.header.rows_per_strip as usize;
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_pixel = spp.max(3);

        let image_data_len = image_width * image_height * bytes_per_pixel;
        self.limits.reserve_usize(image_data_len)?;
        let mut image_data = vec![0u8; image_data_len];

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();
        let jpeg_tables = self.header.jpeg_tables.clone();

        for (strip_idx, (offset, byte_count)) in offsets.iter().zip(byte_counts.iter()).enumerate() {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let strip_len = *byte_count as usize;
            self.limits.reserve_usize(strip_len)?;
            let mut strip_data = vec![0u8; strip_len];
            self.reader.read_exact(&mut strip_data)?;

            let jpeg_data = if !jpeg_tables.is_empty() {
                splice_jpeg_tables(&jpeg_tables, &strip_data)
            } else {
                strip_data
            };
            self.limits.free_usize(strip_len);

            let cursor = Cursor::new(jpeg_data);
            let mut jpeg_decoder = JpegDecoder::new(cursor);

            let strip_pixels = match jpeg_decoder.decode() {
                Ok(image) => image.into_pixels().into_rgb8_bytes(),
                Err(_) => continue,
            };

            let strip_row_start = strip_idx * rows_per_strip;
            let strip_row_end = (strip_row_start + rows_per_strip).min(image_height);
            let decoded_rows = strip_row_end - strip_row_start;
            let row_bytes = image_width * bytes_per_pixel;

            for row in 0..decoded_rows {
                let src_start = row * row_bytes;
                let dst_start = (strip_row_start + row) * row_bytes;

                if src_start + row_bytes <= strip_pixels.len() && dst_start + row_bytes <= image_data.len() {
                    image_data[dst_start..dst_start + row_bytes]
                        .copy_from_slice(&strip_pixels[src_start..src_start + row_bytes]);
                }
            }
        }

        self.limits.free_usize(image_data_len);
        Ok(image_data)
    }

    fn read_tile_data_volumetric(&mut self) -> VexelResult<Vec<Vec<u8>>> {
        let tile_width = self.header.tile_width.unwrap_or(self.width) as usize;
        let tile_height = self.header.tile_length.unwrap_or(self.height) as usize;
        let tile_depth = self.header.tile_depth.max(1) as usize;
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let image_depth = self.header.image_depth.max(1) as usize;
        let bps = self.bits_for(0);
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_pixel = bytes_per_sample * spp;
        let is_sub_byte = bps < 8;

        let tiles_x = image_width.div_ceil(tile_width);
        let tiles_y = image_height.div_ceil(tile_height);
        let tiles_z = image_depth.div_ceil(tile_depth);

        let slice_size = if is_sub_byte {
            image_width.div_ceil(8) * image_height
        } else {
            image_width * image_height * bytes_per_pixel
        };

        let total_slices_size = slice_size.saturating_mul(image_depth);
        self.limits.reserve_usize(total_slices_size)?;
        let mut slices = vec![vec![0u8; slice_size]; image_depth];

        let tile_offsets = self.header.tile_offsets.clone();
        let tile_byte_counts = self.header.tile_byte_counts.clone();

        for tz in 0..tiles_z {
            for ty in 0..tiles_y {
                for tx in 0..tiles_x {
                    let tile_idx = tz * (tiles_x * tiles_y) + ty * tiles_x + tx;
                    let offset = match tile_offsets.get(tile_idx) {
                        Some(&o) => o,
                        None => continue,
                    };
                    let byte_count = tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

                    self.reader.seek(SeekFrom::Start(offset as u64))?;
                    let raw_tile_len = byte_count as usize;
                    self.limits.reserve_usize(raw_tile_len)?;
                    let mut raw_tile = vec![0u8; raw_tile_len];
                    self.reader.read_exact(&mut raw_tile)?;

                    let mut tile_data = self.decompress_chunk(raw_tile);
                    self.limits.free_usize(raw_tile_len);
                    self.limits.reserve_usize(tile_data.len())?;
                    if self.header.predictor != Predictor::None && bps >= 8 {
                        self.apply_predictor(&mut tile_data, tile_width as u32);
                    }

                    let img_x = tx * tile_width;
                    let img_y = ty * tile_height;
                    let img_z_start = tz * tile_depth;

                    for dz in 0..tile_depth {
                        let slice_z = img_z_start + dz;
                        if slice_z >= image_depth {
                            break;
                        }
                        let slice = &mut slices[slice_z];

                        if is_sub_byte {
                            let tile_row_bytes = tile_width.div_ceil(8);
                            let img_row_bytes = image_width.div_ceil(8);
                            for row in 0..tile_height {
                                let py = img_y + row;
                                if py >= image_height {
                                    break;
                                }
                                for col in 0..tile_width {
                                    let px = img_x + col;
                                    if px >= image_width {
                                        continue;
                                    }
                                    let src_byte_idx = dz * tile_height * tile_row_bytes + row * tile_row_bytes + col / 8;
                                    let src_byte = tile_data.get(src_byte_idx).copied().unwrap_or(0);
                                    let bit = (src_byte >> (7 - (col % 8))) & 1;
                                    let dst_byte_idx = py * img_row_bytes + px / 8;
                                    let dst_bit_pos = 7 - (px % 8);
                                    if dst_byte_idx < slice.len() {
                                        slice[dst_byte_idx] = (slice[dst_byte_idx] & !(1 << dst_bit_pos)) | (bit << dst_bit_pos);
                                    }
                                }
                            }
                        } else {
                            let tile_slice_pixels = tile_width * tile_height;
                            for row in 0..tile_height {
                                let py = img_y + row;
                                if py >= image_height {
                                    break;
                                }
                                for col in 0..tile_width {
                                    let px = img_x + col;
                                    if px >= image_width {
                                        continue;
                                    }
                                    let src = (dz * tile_slice_pixels + row * tile_width + col) * bytes_per_pixel;
                                    let dst = (py * image_width + px) * bytes_per_pixel;
                                    if src + bytes_per_pixel <= tile_data.len() && dst + bytes_per_pixel <= slice.len() {
                                        slice[dst..dst + bytes_per_pixel].copy_from_slice(&tile_data[src..src + bytes_per_pixel]);
                                    }
                                }
                            }
                        }
                    }
                    self.limits.free_usize(tile_data.len());
                }
            }
        }

        self.limits.free_usize(total_slices_size);
        Ok(slices)
    }

    fn read_tile_data(&mut self) -> VexelResult<Vec<u8>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);
        let is_planar = self.header.planar_configuration == crate::utils::tiff::types::PlanarConfiguration::Planar
            && self.header.samples_per_pixel > 1;

        let tile_width = self.header.tile_width.unwrap_or(self.width) as usize;
        let tile_height = self.header.tile_length.unwrap_or(self.height) as usize;
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let spp = self.header.samples_per_pixel as usize;
        let bps = self.bits_for(0);
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = if is_jpeg { spp.max(3) } else { bytes_per_sample * spp };
        let bytes_per_plane_sample = bytes_per_sample;
        let is_sub_byte = bps < 8 && !is_jpeg;

        let tiles_x = image_width.div_ceil(tile_width);
        let tiles_y = image_height.div_ceil(tile_height);
        let tiles_per_plane = tiles_x * tiles_y;

        let img_data_size = if is_sub_byte {
            image_width.div_ceil(8) * image_height
        } else {
            image_width * image_height * bytes_per_pixel
        };

        let tile_offsets = self.header.tile_offsets.clone();
        let tile_byte_counts = self.header.tile_byte_counts.clone();
        let jpeg_tables = self.header.jpeg_tables.clone();

        if is_planar && !is_jpeg && !is_sub_byte {
            let plane_size = image_width * image_height * bytes_per_plane_sample;
            let planar_data_len = plane_size.saturating_mul(spp);
            self.limits.reserve_usize(planar_data_len)?;
            let mut planar_data = vec![0u8; planar_data_len];

            for (tile_idx, &offset) in tile_offsets.iter().enumerate() {
                let byte_count = tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

                self.reader.seek(SeekFrom::Start(offset as u64))?;
                let raw_tile_len = byte_count as usize;
                self.limits.reserve_usize(raw_tile_len)?;
                let mut raw_tile = vec![0u8; raw_tile_len];
                self.reader.read_exact(&mut raw_tile)?;

                let mut tile_data = self.decompress_chunk(raw_tile);
                self.limits.free_usize(raw_tile_len);
                self.limits.reserve_usize(tile_data.len())?;
                if self.header.predictor != Predictor::None && bps >= 8 {
                    self.apply_predictor_planar(&mut tile_data, tile_width as u32);
                }

                let sample = tile_idx / tiles_per_plane;
                let within_plane = tile_idx % tiles_per_plane;
                let tile_col = within_plane % tiles_x;
                let tile_row = within_plane / tiles_x;

                let img_x = tile_col * tile_width;
                let img_y = tile_row * tile_height;
                let plane_offset = sample * plane_size;

                let expected_tile_bytes = tile_width * tile_height * bytes_per_plane_sample;
                if tile_data.len() < expected_tile_bytes {
                    tile_data.resize(expected_tile_bytes, 0);
                }

                for row in 0..tile_height {
                    let py = img_y + row;
                    if py >= image_height {
                        break;
                    }
                    for col in 0..tile_width {
                        let px = img_x + col;
                        if px >= image_width {
                            continue;
                        }
                        let src = (row * tile_width + col) * bytes_per_plane_sample;
                        let dst = plane_offset + (py * image_width + px) * bytes_per_plane_sample;
                        if src + bytes_per_plane_sample <= tile_data.len()
                            && dst + bytes_per_plane_sample <= planar_data.len()
                        {
                            planar_data[dst..dst + bytes_per_plane_sample]
                                .copy_from_slice(&tile_data[src..src + bytes_per_plane_sample]);
                        }
                    }
                }
                self.limits.free_usize(tile_data.len());
            }

            self.limits.free_usize(planar_data_len);
            return Ok(planar_data);
        }

        self.limits.reserve_usize(img_data_size)?;
        let mut image_data = vec![0u8; img_data_size];

        for (tile_idx, &offset) in tile_offsets.iter().enumerate() {
            let byte_count = tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

            self.reader.seek(SeekFrom::Start(offset as u64))?;
            let raw_tile_len = byte_count as usize;
            self.limits.reserve_usize(raw_tile_len)?;
            let mut raw_tile = vec![0u8; raw_tile_len];
            self.reader.read_exact(&mut raw_tile)?;

            let tile_data = if is_jpeg {
                let jpeg_data = if !jpeg_tables.is_empty() {
                    splice_jpeg_tables(&jpeg_tables, &raw_tile)
                } else {
                    raw_tile
                };
                self.limits.free_usize(raw_tile_len);

                let cursor = Cursor::new(jpeg_data);
                let mut jpeg_decoder = JpegDecoder::new(cursor);
                match jpeg_decoder.decode() {
                    Ok(image) => image.into_pixels().into_rgb8_bytes(),
                    Err(_) => vec![0u8; tile_width * tile_height * bytes_per_pixel],
                }
            } else {
                let mut d = self.decompress_chunk(raw_tile);
                self.limits.free_usize(raw_tile_len);
                if self.header.predictor != Predictor::None && self.bits_for(0) >= 8 {
                    self.apply_predictor(&mut d, tile_width as u32);
                }
                d
            };
            self.limits.reserve_usize(tile_data.len())?;

            let tile_col = tile_idx % tiles_x;
            let tile_row = tile_idx / tiles_x;

            let img_x = tile_col * tile_width;
            let img_y = tile_row * tile_height;

            if is_sub_byte {
                let tile_row_bytes = tile_width.div_ceil(8);
                let img_row_bytes = image_width.div_ceil(8);
                for row in 0..tile_height {
                    let py = img_y + row;
                    if py >= image_height {
                        break;
                    }
                    let tile_row_start = row * tile_row_bytes;
                    let img_row_start = py * img_row_bytes;
                    for col in 0..tile_width {
                        let px = img_x + col;
                        if px >= image_width {
                            continue;
                        }
                        let src_byte_idx = tile_row_start + col / 8;
                        let src_byte = tile_data.get(src_byte_idx).copied().unwrap_or(0);
                        let bit = (src_byte >> (7 - (col % 8))) & 1;
                        let dst_byte_idx = img_row_start + px / 8;
                        let dst_bit_pos = 7 - (px % 8);
                        if dst_byte_idx < image_data.len() {
                            image_data[dst_byte_idx] = (image_data[dst_byte_idx] & !(1 << dst_bit_pos)) | (bit << dst_bit_pos);
                        }
                    }
                }
                self.limits.free_usize(tile_data.len());
            } else {
                let expected_tile_bytes = tile_width * tile_height * bytes_per_pixel;
                let mut tile_data = tile_data;
                if tile_data.len() < expected_tile_bytes {
                    tile_data.resize(expected_tile_bytes, 0);
                }

                for row in 0..tile_height {
                    let py = img_y + row;
                    if py >= image_height {
                        break;
                    }

                    for col in 0..tile_width {
                        let px = img_x + col;
                        if px >= image_width {
                            continue;
                        }

                        let tile_pixel_offset = (row * tile_width + col) * bytes_per_pixel;
                        let img_pixel_offset = (py * image_width + px) * bytes_per_pixel;

                        if tile_pixel_offset + bytes_per_pixel <= tile_data.len()
                            && img_pixel_offset + bytes_per_pixel <= image_data.len()
                        {
                            image_data[img_pixel_offset..img_pixel_offset + bytes_per_pixel]
                                .copy_from_slice(&tile_data[tile_pixel_offset..tile_pixel_offset + bytes_per_pixel]);
                        }
                    }
                }
                self.limits.free_usize(tile_data.len());
            }
        }

        self.limits.free_usize(img_data_size);
        Ok(image_data)
    }

    fn decode_current_ifd(&mut self) -> VexelResult<Vec<ImageFrame>> {
        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);
        let image_depth = self.header.image_depth.max(1);
        let is_volumetric = is_tiled && image_depth > 1;

        let make_header = |h: &TiffHeader| TiffHeader {
            image_width: h.image_width,
            image_length: h.image_length,
            bits_per_sample: h.bits_per_sample.clone(),
            compression: h.compression,
            photometric_interpretation: h.photometric_interpretation,
            strip_offsets: Vec::new(),
            samples_per_pixel: h.samples_per_pixel,
            rows_per_strip: h.rows_per_strip,
            strip_byte_counts: Vec::new(),
            x_resolution: h.x_resolution,
            y_resolution: h.y_resolution,
            planar_configuration: h.planar_configuration,
            resolution_unit: h.resolution_unit,
            sample_format: h.sample_format.clone(),
            extra_samples: h.extra_samples.clone(),
            color_map: h.color_map.clone(),
            ycbcr_coefficients: h.ycbcr_coefficients,
            ycbcr_sub_sampling: h.ycbcr_sub_sampling,
            reference_black_white: h.reference_black_white,
            tile_width: h.tile_width,
            tile_length: h.tile_length,
            tile_offsets: Vec::new(),
            tile_byte_counts: Vec::new(),
            predictor: h.predictor,
            jpeg_tables: h.jpeg_tables.clone(),
            image_depth: h.image_depth,
            tile_depth: h.tile_depth,
        };

        if is_volumetric {
            let vol_offset = self.reader.stream_position().unwrap_or(0);
            let slices = self.read_tile_data_volumetric()?;
            let total_vol_len: usize = slices.iter().map(|s| s.len()).sum();
            let header = make_header(&self.header);
            let pixel_reader = PixelReader {
                byte_order: self.byte_order,
                width: self.width,
                height: self.height,
            };

            let mut frames = Vec::with_capacity(slices.len());
            for slice_data in slices {
                let mut pd = pixel_reader.convert_to_pixel_data(slice_data, &header)?;
                pd.correct_pixels(self.width, self.height);
                frames.push(ImageFrame::new(self.width, self.height, pd, 0));
            }

            self.sections.push(TiffSectionInfo {
                start_offset: vol_offset,
                data: TiffSectionData::PixelData(TiffPixelData { length: total_vol_len }),
            });

            return Ok(frames);
        }

        let pixel_data_offset = self.reader.stream_position().unwrap_or(0);

        let bytes = if is_tiled {
            self.read_tile_data()?
        } else {
            self.read_strip_data()?
        };

        let pixel_data_len = bytes.len();

        let pixel_data = if is_jpeg {
            let spp = self.header.samples_per_pixel.max(3);
            if spp >= 4 {
                crate::utils::image::PixelData::RGBA8(bytes)
            } else {
                crate::utils::image::PixelData::RGB8(bytes)
            }
        } else {
            let header = make_header(&self.header);
            let pixel_reader = PixelReader {
                byte_order: self.byte_order,
                width: self.width,
                height: self.height,
            };

            let mut pd = pixel_reader.convert_to_pixel_data(bytes, &header)?;
            pd.correct_pixels(self.width, self.height);
            pd
        };

        self.sections.push(TiffSectionInfo {
            start_offset: pixel_data_offset,
            data: TiffSectionData::PixelData(TiffPixelData { length: pixel_data_len }),
        });

        Ok(vec![ImageFrame::new(self.width, self.height, pixel_data, 0)])
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.sections.clear();
        self.ifd_count = 0;

        let first_ifd_offset = self.read_file_header()?;

        let mut frames: Vec<ImageFrame> = Vec::new();
        let mut next_ifd_offset = first_ifd_offset;

        while next_ifd_offset != 0 {
            next_ifd_offset = self.read_ifd(next_ifd_offset)?;

            match self.decode_current_ifd() {
                Ok(ifd_frames) => frames.extend(ifd_frames),
                Err(e) => {
                    log_warn!("Failed to decode TIFF frame: {}", e);
                    if frames.is_empty() {
                        return Err(e);
                    }
                    break;
                }
            }
        }

        if frames.is_empty() {
            return Err(VexelError::Custom("No frames decoded from TIFF".to_string()));
        }

        let width = frames[0].width();
        let height = frames[0].height();
        let pixel_format = frames[0].pixel_format();

        Ok(Image::new(width, height, pixel_format, frames))
    }
}
