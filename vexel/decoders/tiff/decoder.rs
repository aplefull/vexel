use crate::bitreader::BitReader;
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::Image;
use std::io::{Cursor, Read, Seek, SeekFrom};

use super::compression::{
    apply_predictor_float, apply_predictor_horizontal, apply_predictor_horizontal_be, decompress_deflate,
    decompress_lzw, decompress_packbits,
};
use super::pixels::PixelReader;
use super::reader::{read_multiple_rationals, read_multiple_values, read_rational, read_single_value};
use super::types::{Compression, Predictor, SampleFormat, TiffHeader};

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    byte_order: ByteOrder,
    header: TiffHeader,
    reader: BitReader<R>,
}

impl<R: Read + Seek> TiffDecoder<R> {
    pub fn new(reader: R) -> TiffDecoder<R> {
        TiffDecoder {
            width: 0,
            height: 0,
            byte_order: ByteOrder::LittleEndian,
            header: TiffHeader::default(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn read_header(&mut self) -> VexelResult<()> {
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

        self.reader.seek(SeekFrom::Start(ifd_offset as u64))?;

        let num_entries = self.reader.read_u16()?;

        for _ in 0..num_entries {
            let tag = self.reader.read_u16()?;
            let type_ = self.reader.read_u16()?;
            let count = self.reader.read_u32()?;
            let value_offset = self.reader.read_u32()?;

            let current_pos = self.reader.stream_position()?;

            match tag {
                256 => self.header.image_width = read_single_value(type_, value_offset, &mut self.reader)?,
                257 => self.header.image_length = read_single_value(type_, value_offset, &mut self.reader)?,
                258 => {
                    self.header.bits_per_sample =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                259 => {
                    let compression_value: u32 = read_single_value(type_, value_offset, &mut self.reader)?;
                    self.header.compression =
                        Compression::try_from(compression_value as u16).unwrap_or(Compression::None);
                }
                262 => {
                    self.header.photometric_interpretation = read_single_value(type_, value_offset, &mut self.reader)?
                }
                273 => {
                    self.header.strip_offsets =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                277 => self.header.samples_per_pixel = read_single_value(type_, value_offset, &mut self.reader)?,
                278 => self.header.rows_per_strip = read_single_value(type_, value_offset, &mut self.reader)?,
                279 => {
                    self.header.strip_byte_counts =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?
                }
                282 => self.header.x_resolution = read_rational(value_offset, &mut self.reader)?,
                283 => self.header.y_resolution = read_rational(value_offset, &mut self.reader)?,
                284 => self.header.planar_configuration = read_single_value(type_, value_offset, &mut self.reader)?,
                296 => self.header.resolution_unit = read_single_value(type_, value_offset, &mut self.reader)?,
                317 => {
                    let predictor_value: u32 = read_single_value(type_, value_offset, &mut self.reader)?;
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
                    self.header.tile_width = Some(read_single_value(type_, value_offset, &mut self.reader)?);
                }
                323 => {
                    self.header.tile_length = Some(read_single_value(type_, value_offset, &mut self.reader)?);
                }
                324 => {
                    self.header.tile_offsets =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
                }
                325 => {
                    self.header.tile_byte_counts =
                        read_multiple_values(type_, count, value_offset, self.byte_order, &mut self.reader)?;
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

        Ok(())
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
            _ => data,
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

    fn read_strip_data(&mut self) -> VexelResult<Vec<u8>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);

        if is_jpeg {
            return self.read_strip_data_jpeg();
        }

        let rows_per_strip = self.header.rows_per_strip;
        let image_width = self.width;
        let mut bytes = Vec::new();

        let offsets = self.header.strip_offsets.clone();
        let byte_counts = self.header.strip_byte_counts.clone();

        for (strip_idx, (offset, byte_count)) in offsets.iter().zip(byte_counts.iter()).enumerate() {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let mut strip_data = vec![0u8; *byte_count as usize];
            self.reader.read_exact(&mut strip_data)?;

            let mut decompressed = self.decompress_chunk(strip_data);

            let strip_row_start = (strip_idx as u32).saturating_mul(rows_per_strip);
            let strip_row_end = strip_row_start.saturating_add(rows_per_strip).min(self.height);
            let strip_rows = strip_row_end.saturating_sub(strip_row_start);
            let strip_width = if self.header.planar_configuration == super::types::PlanarConfiguration::Planar {
                image_width
            } else {
                image_width
            };

            if self.header.predictor != Predictor::None {
                let bps = self.bits_for(0);
                if bps >= 8 {
                    let spp = self.header.samples_per_pixel;
                    let row_bytes = strip_width as usize * spp as usize * (bps as usize / 8);
                    let expected_len = strip_rows as usize * row_bytes;
                    decompressed.truncate(expected_len);
                    self.apply_predictor(&mut decompressed, strip_width);
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

    fn read_tile_data(&mut self) -> VexelResult<Vec<u8>> {
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);

        let tile_width = self.header.tile_width.unwrap_or(self.width) as usize;
        let tile_height = self.header.tile_length.unwrap_or(self.height) as usize;
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let spp = self.header.samples_per_pixel as usize;
        let bps = self.bits_for(0);
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = if is_jpeg { spp.max(3) } else { bytes_per_sample * spp };
        let is_sub_byte = bps < 8 && !is_jpeg;

        let tiles_x = image_width.div_ceil(tile_width);

        let img_data_size = if is_sub_byte {
            image_width.div_ceil(8) * image_height
        } else {
            image_width * image_height * bytes_per_pixel
        };
        let mut image_data = vec![0u8; img_data_size];

        let tile_offsets = self.header.tile_offsets.clone();
        let tile_byte_counts = self.header.tile_byte_counts.clone();
        let jpeg_tables = self.header.jpeg_tables.clone();

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

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();
        let is_jpeg = matches!(self.header.compression, Compression::JPEG | Compression::OldJPEG);

        let bytes = if is_tiled {
            self.read_tile_data()?
        } else {
            self.read_strip_data()?
        };

        if is_jpeg {
            let spp = self.header.samples_per_pixel.max(3);
            let pixel_data = if spp >= 4 {
                crate::utils::image::PixelData::RGBA8(bytes)
            } else {
                crate::utils::image::PixelData::RGB8(bytes)
            };

            return Ok(Image::from_pixels(self.width, self.height, pixel_data));
        }

        let header = TiffHeader {
            image_width: self.header.image_width,
            image_length: self.header.image_length,
            bits_per_sample: self.header.bits_per_sample.clone(),
            compression: self.header.compression,
            photometric_interpretation: self.header.photometric_interpretation,
            strip_offsets: Vec::new(),
            samples_per_pixel: self.header.samples_per_pixel,
            rows_per_strip: self.header.rows_per_strip,
            strip_byte_counts: Vec::new(),
            x_resolution: self.header.x_resolution,
            y_resolution: self.header.y_resolution,
            planar_configuration: self.header.planar_configuration,
            resolution_unit: self.header.resolution_unit,
            sample_format: self.header.sample_format.clone(),
            extra_samples: self.header.extra_samples.clone(),
            color_map: self.header.color_map.clone(),
            ycbcr_coefficients: self.header.ycbcr_coefficients,
            ycbcr_sub_sampling: self.header.ycbcr_sub_sampling,
            reference_black_white: self.header.reference_black_white,
            tile_width: self.header.tile_width,
            tile_length: self.header.tile_length,
            tile_offsets: Vec::new(),
            tile_byte_counts: Vec::new(),
            predictor: self.header.predictor,
            jpeg_tables: self.header.jpeg_tables.clone(),
        };

        let pixel_reader = PixelReader {
            byte_order: self.byte_order,
            width: self.width,
            height: self.height,
        };

        let mut pixel_data = pixel_reader.convert_to_pixel_data(bytes, &header)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
