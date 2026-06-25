use crate::bitreader::BitReader;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::ImageFrame;
use crate::utils::types::ByteOrder;
use crate::{Image, Limits, log_warn};
use std::io::{Cursor, Read, Seek, SeekFrom};

use super::compression::{
    apply_predictor_float, apply_predictor_horizontal, apply_predictor_horizontal_be, decompress_deflate,
    decompress_lzw, decompress_packbits, decompress_sgilog, decompress_sgilog24,
};
use super::pixels::PixelReader;
use super::reader::{read_multiple_rationals, read_multiple_values, read_rational, read_single_value};
use super::types::{Compression, Predictor, SampleFormat, TiffHeader};

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    byte_order: ByteOrder,
    header: TiffHeader,
    reader: BitReader<R>,
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
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn read_file_header(&mut self) -> VexelResult<u32> {
        let mut byte_order_marker = [0u8; 2];
        self.reader.read_exact(&mut byte_order_marker)?;

        let byte_order = match &byte_order_marker {
            b"II" => ByteOrder::LittleEndian,
            b"MM" => ByteOrder::BigEndian,
            _ => return Err(VexelError::Custom("Invalid byte order marker".to_string())),
        };

        self.byte_order = byte_order;
        self.reader.set_endianness(byte_order);

        let magic = self.reader.read_u16()?;
        if magic != 42 {
            return Err(VexelError::Custom("Not a TIFF file".to_string()));
        }

        let ifd_offset = self.reader.read_u32()?;
        Ok(ifd_offset)
    }

    fn read_ifd(&mut self, ifd_offset: u32) -> VexelResult<u32> {
        self.header = TiffHeader::default();

        self.reader.seek(SeekFrom::Start(ifd_offset as u64))?;

        let num_entries = self.reader.read_u16()?;

        for _ in 0..num_entries {
            let tag = self.reader.read_u16()?;
            let type_ = self.reader.read_u16()?;
            let count = self.reader.read_u32()?;
            let value_offset = self.reader.read_u32()?;

            let current_pos = self.reader.stream_position()?;

            match tag {
                256 => self.header.image_width = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                257 => self.header.image_length = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                258 => {
                    self.header.bits_per_sample =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                259 => {
                    let compression_value: u32 = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?;
                    self.header.compression =
                        Compression::try_from(compression_value as u16).unwrap_or(Compression::None);
                }
                262 => {
                    self.header.photometric_interpretation = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?
                }
                273 => {
                    self.header.strip_offsets =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                277 => self.header.samples_per_pixel = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                278 => self.header.rows_per_strip = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                279 => {
                    self.header.strip_byte_counts =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                282 => self.header.x_resolution = read_rational(value_offset, &mut self.reader)?,
                283 => self.header.y_resolution = read_rational(value_offset, &mut self.reader)?,
                284 => self.header.planar_configuration = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                296 => self.header.resolution_unit = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?,
                317 => {
                    let predictor_value: u32 = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?;
                    self.header.predictor = Predictor::try_from(predictor_value).unwrap_or(Predictor::None);
                }
                320 => {
                    self.reader.seek(SeekFrom::Start(value_offset as u64))?;
                    let mut color_map = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        color_map.push(self.reader.read_u16()?);
                    }
                    self.header.color_map = color_map;
                }
                322 => {
                    self.header.tile_width = Some(read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?);
                }
                323 => {
                    self.header.tile_length = Some(read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?);
                }
                324 => {
                    self.header.tile_offsets =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                }
                325 => {
                    self.header.tile_byte_counts =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                }
                32997 => {
                    self.header.image_depth = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?;
                }
                32998 => {
                    self.header.tile_depth = read_single_value(type_, value_offset, self.byte_order, &mut self.reader)?;
                }
                338 => {
                    self.header.extra_samples =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                }
                339 => {
                    let raw_formats: Vec<u32> =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                    self.header.sample_format = raw_formats
                        .into_iter()
                        .map(|v| SampleFormat::try_from(v).unwrap_or(SampleFormat::UnsignedInt))
                        .collect();
                }
                347 => {
                    self.reader.seek(SeekFrom::Start(value_offset as u64))?;
                    let mut jpeg_tables = vec![0u8; count as usize];
                    self.reader.read_exact(&mut jpeg_tables)?;
                    self.header.jpeg_tables = jpeg_tables;
                }
                529 => {
                    let rationals = read_multiple_rationals(count, value_offset, &mut self.reader)?;
                    if rationals.len() >= 3 {
                        self.header.ycbcr_coefficients = [rationals[0], rationals[1], rationals[2]];
                    }
                }
                530 => {
                    let values: Vec<u32> =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                    if values.len() >= 2 {
                        self.header.ycbcr_sub_sampling = [values[0] as u16, values[1] as u16];
                    }
                }
                532 => {
                    let rationals = read_multiple_rationals(count, value_offset, &mut self.reader)?;
                    if rationals.len() >= 6 {
                        self.header.reference_black_white = [
                            rationals[0],
                            rationals[1],
                            rationals[2],
                            rationals[3],
                            rationals[4],
                            rationals[5],
                        ];
                    }
                }
                _ => {}
            }

            self.reader.seek(SeekFrom::Start(current_pos))?;
        }

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        self.limits.reserve_buffer(self.width, self.height, 4)?;

        let next_ifd_offset = self.reader.read_u32().unwrap_or(0);
        Ok(next_ifd_offset)
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
            Ok(image) => image.as_rgb8(),
            Err(_) => Vec::new(),
        }
    }

    fn decompress_jpeg_strip(&self, strip_data: Vec<u8>) -> Vec<u8> {
        let jpeg_data = if !self.header.jpeg_tables.is_empty() {
            self.splice_jpeg_tables(&self.header.jpeg_tables, &strip_data)
        } else {
            strip_data
        };

        let cursor = Cursor::new(jpeg_data);
        let mut jpeg_decoder = JpegDecoder::new(cursor);

        match jpeg_decoder.decode() {
            Ok(image) => image.as_rgb8(),
            Err(_) => Vec::new(),
        }
    }

    fn splice_jpeg_tables(&self, tables: &[u8], strip: &[u8]) -> Vec<u8> {
        if tables.len() < 4 || strip.len() < 2 {
            return strip.to_vec();
        }

        let tables_body = if tables.starts_with(&[0xFF, 0xD8]) && tables.ends_with(&[0xFF, 0xD9]) {
            &tables[2..tables.len() - 2]
        } else {
            tables
        };

        if strip.starts_with(&[0xFF, 0xD8]) {
            let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len() - 2);
            result.extend_from_slice(&[0xFF, 0xD8]);
            result.extend_from_slice(tables_body);
            result.extend_from_slice(&strip[2..]);
            result
        } else {
            let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len());
            result.extend_from_slice(&[0xFF, 0xD8]);
            result.extend_from_slice(tables_body);
            result.extend_from_slice(strip);
            result
        }
    }

    fn apply_predictor(&self, data: &mut Vec<u8>, strip_width: u32) {
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
            Predictor::FloatingPoint => {
                apply_predictor_float(data, strip_width, spp, bps, self.byte_order == ByteOrder::BigEndian);
            }
            _ => {}
        }
    }

    fn apply_predictor_planar(&self, data: &mut Vec<u8>, strip_width: u32) {
        let bps = self.bits_for(0);
        match self.header.predictor {
            Predictor::HorizontalDifferencing => {
                if self.byte_order == ByteOrder::BigEndian {
                    apply_predictor_horizontal_be(data, strip_width, 1, bps);
                } else {
                    apply_predictor_horizontal(data, strip_width, 1, bps);
                }
            }
            Predictor::FloatingPoint => {
                apply_predictor_float(data, strip_width, 1, bps, self.byte_order == ByteOrder::BigEndian);
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
        let is_planar = self.header.planar_configuration == super::types::PlanarConfiguration::Planar
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

            let mut strip_data = vec![0u8; *byte_count as usize];
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

            bytes.extend_from_slice(&decompressed);
        }

        Ok(bytes)
    }

    fn read_strip_data_jpeg(&mut self) -> VexelResult<Vec<u8>> {
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let rows_per_strip = self.header.rows_per_strip as usize;
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_pixel = spp.max(3);

        let mut image_data = vec![0u8; image_width * image_height * bytes_per_pixel];

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();
        let jpeg_tables = self.header.jpeg_tables.clone();

        for (strip_idx, (offset, byte_count)) in offsets.iter().zip(byte_counts.iter()).enumerate() {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let mut strip_data = vec![0u8; *byte_count as usize];
            self.reader.read_exact(&mut strip_data)?;

            let jpeg_data = if !jpeg_tables.is_empty() {
                self.splice_jpeg_tables(&jpeg_tables, &strip_data)
            } else {
                strip_data
            };

            let cursor = Cursor::new(jpeg_data);
            let mut jpeg_decoder = JpegDecoder::new(cursor);

            let strip_pixels = match jpeg_decoder.decode() {
                Ok(image) => image.as_rgb8(),
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
                    let mut raw_tile = vec![0u8; byte_count as usize];
                    self.reader.read_exact(&mut raw_tile)?;

                    let mut tile_data = self.decompress_chunk(raw_tile);
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
                }
            }
        }

        Ok(slices)
    }

    fn read_tile_data(&mut self) -> VexelResult<Vec<u8>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);
        let is_planar = self.header.planar_configuration == super::types::PlanarConfiguration::Planar
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
            let mut planar_data = vec![0u8; plane_size * spp];

            for tile_idx in 0..tile_offsets.len() {
                let offset = tile_offsets[tile_idx];
                let byte_count = tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

                self.reader.seek(SeekFrom::Start(offset as u64))?;
                let mut raw_tile = vec![0u8; byte_count as usize];
                self.reader.read_exact(&mut raw_tile)?;

                let mut tile_data = self.decompress_chunk(raw_tile);
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
            }

            return Ok(planar_data);
        }

        let mut image_data = vec![0u8; img_data_size];

        for tile_idx in 0..tile_offsets.len() {
            let offset = tile_offsets[tile_idx];
            let byte_count = tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

            self.reader.seek(SeekFrom::Start(offset as u64))?;
            let mut raw_tile = vec![0u8; byte_count as usize];
            self.reader.read_exact(&mut raw_tile)?;

            let tile_data = if is_jpeg {
                let jpeg_data = if !jpeg_tables.is_empty() {
                    self.splice_jpeg_tables(&jpeg_tables, &raw_tile)
                } else {
                    raw_tile
                };

                let cursor = Cursor::new(jpeg_data);
                let mut jpeg_decoder = JpegDecoder::new(cursor);
                match jpeg_decoder.decode() {
                    Ok(image) => image.as_rgb8(),
                    Err(_) => vec![0u8; tile_width * tile_height * bytes_per_pixel],
                }
            } else {
                let mut d = self.decompress_chunk(raw_tile);
                if self.header.predictor != Predictor::None && self.bits_for(0) >= 8 {
                    self.apply_predictor(&mut d, tile_width as u32);
                }
                d
            };

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
            }
        }

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
            let slices = self.read_tile_data_volumetric()?;
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
            return Ok(frames);
        }

        let bytes = if is_tiled {
            self.read_tile_data()?
        } else {
            self.read_strip_data()?
        };

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

        Ok(vec![ImageFrame::new(self.width, self.height, pixel_data, 0)])
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
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
