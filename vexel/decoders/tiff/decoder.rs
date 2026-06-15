use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::Image;
use std::io::{Read, Seek, SeekFrom};

use super::pixels::PixelReader;
use super::reader::{read_multiple_rationals, read_multiple_values, read_rational, read_single_value};
use super::types::{Compression, SampleFormat, TiffHeader};

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
                    self.header.compression = match read_single_value(type_, value_offset, &mut self.reader)? {
                        1u32 => Compression::None,
                        2 => Compression::CCITT1D,
                        3 => Compression::Group3Fax,
                        4 => Compression::Group4Fax,
                        5 => Compression::LZW,
                        6 => Compression::JPEG,
                        32773 => Compression::PackBits,
                        _ => Compression::None,
                    }
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

    fn read_strip_data(&mut self) -> VexelResult<Vec<u8>> {
        let mut bytes = Vec::new();

        for (offset, byte_count) in self
            .header
            .strip_offsets
            .clone()
            .iter()
            .zip(self.header.strip_byte_counts.clone().iter())
        {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;

            let mut strip_data = vec![0u8; *byte_count as usize];
            self.reader.read_exact(&mut strip_data)?;

            let decompressed = match self.header.compression {
                Compression::None => strip_data,
                _ => return Err(VexelError::Custom("Unsupported compression".to_string())),
            };

            bytes.extend_from_slice(&decompressed);
        }

        Ok(bytes)
    }

    fn read_tile_data(&mut self) -> VexelResult<Vec<u8>> {
        let tile_width = self.header.tile_width.unwrap_or(self.width) as usize;
        let tile_height = self.header.tile_length.unwrap_or(self.height) as usize;
        let image_width = self.width as usize;
        let image_height = self.height as usize;
        let spp = self.header.samples_per_pixel as usize;
        let bps = self.bits_for(0);
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        let tiles_x = image_width.div_ceil(tile_width);

        let mut image_data = vec![0u8; image_width * image_height * bytes_per_pixel];

        for tile_idx in 0..self.header.tile_offsets.len() {
            let offset = self.header.tile_offsets[tile_idx];
            let byte_count = self.header.tile_byte_counts.get(tile_idx).copied().unwrap_or(0);

            self.reader.seek(SeekFrom::Start(offset as u64))?;
            let mut tile_data = vec![0u8; byte_count as usize];
            self.reader.read_exact(&mut tile_data)?;

            let tile_data = match self.header.compression {
                Compression::None => tile_data,
                _ => return Err(VexelError::Custom("Unsupported compression in tiled image".to_string())),
            };

            let tile_col = tile_idx % tiles_x;
            let tile_row = tile_idx / tiles_x;

            let img_x = tile_col * tile_width;
            let img_y = tile_row * tile_height;

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

        Ok(image_data)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let is_tiled = self.header.tile_width.is_some() && !self.header.tile_offsets.is_empty();

        let bytes = if is_tiled {
            self.read_tile_data()?
        } else {
            self.read_strip_data()?
        };

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
