use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::{Image, PixelData};
use std::io::{Read, Seek, SeekFrom};

use super::types::{Compression, PhotometricInterpretation, TiffHeader};

fn read_single_value<T, R: Read + Seek>(type_: u16, value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<T>
where
    T: TryFrom<u32>,
{
    let value = match type_ {
        1 => value_offset & 0xFF,
        3 => value_offset & 0xFFFF,
        4 => value_offset,
        _ => {
            reader.seek(SeekFrom::Start(value_offset as u64))?;
            reader.read_u32()?
        }
    };

    T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))
}

fn read_multiple_values<T, R: Read + Seek>(
    type_: u16,
    count: u32,
    value_offset: u32,
    reader: &mut BitReader<R>,
) -> VexelResult<Vec<T>>
where
    T: TryFrom<u32>,
{
    let mut values = Vec::with_capacity(count as usize);

    if count == 1 {
        values.push(read_single_value(type_, value_offset, reader)?);
        return Ok(values);
    }

    reader.seek(SeekFrom::Start(value_offset as u64))?;

    for _ in 0..count {
        let value = match type_ {
            1 => reader.read_u8()? as u32,
            3 => reader.read_u16()? as u32,
            4 => reader.read_u32()?,
            _ => return Err(VexelError::Custom("Unsupported type".to_string())),
        };

        values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
    }

    Ok(values)
}

fn read_rational<R: Read + Seek>(value_offset: u32, reader: &mut BitReader<R>) -> VexelResult<f32> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let numerator = reader.read_u32()?;
    let denominator = reader.read_u32()?;

    if denominator == 0 {
        return Err(VexelError::Custom("Division by zero in rational".to_string()));
    }

    Ok(numerator as f32 / denominator as f32)
}

pub struct TiffDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    header: TiffHeader,
    reader: BitReader<R>,
}

impl<R: Read + Seek> TiffDecoder<R> {
    pub fn new(reader: R) -> TiffDecoder<R> {
        TiffDecoder {
            width: 0,
            height: 0,
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

    #[allow(dead_code)]
    fn read_value<T>(&mut self, type_: u16, count: u32, value_offset: u32) -> VexelResult<Vec<T>>
    where
        T: TryFrom<u32>,
    {
        // Calculate total bytes needed
        let bytes_per_value = match type_ {
            1 | 2 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            _ => return Err(VexelError::Custom(format!("Unsupported type: {}", type_))),
        };

        let total_bytes = bytes_per_value * count;
        let mut values = Vec::with_capacity(count as usize);

        // If total size is 4 bytes or fewer, values are stored in value_offset
        if total_bytes <= 4 {
            match type_ {
                1 => {
                    for i in 0..count {
                        let value = (value_offset >> (i * 8)) & 0xFF;
                        values.push(
                            T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?,
                        );
                    }
                }
                3 => {
                    for i in 0..count {
                        let value = (value_offset >> (i * 16)) & 0xFFFF;
                        values.push(
                            T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?,
                        );
                    }
                }
                4 => {
                    values.push(
                        T::try_from(value_offset)
                            .map_err(|_| VexelError::Custom("Value conversion error".to_string()))?,
                    );
                }
                _ => return Err(VexelError::Custom("Invalid type for inline value".to_string())),
            }
        } else {
            // Values are stored at the offset
            self.reader.seek(SeekFrom::Start(value_offset as u64))?;

            for _ in 0..count {
                let value = match type_ {
                    1 => self.reader.read_u8()? as u32,
                    3 => self.reader.read_u16()? as u32,
                    4 => self.reader.read_u32()?,
                    5 => {
                        let numerator = self.reader.read_u32()?;
                        let denominator = self.reader.read_u32()?;
                        if denominator == 0 {
                            return Err(VexelError::Custom("Division by zero in rational".to_string()));
                        }
                        numerator
                    }
                    _ => return Err(VexelError::Custom("Unsupported type".to_string())),
                };

                values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
            }
        }

        Ok(values)
    }

    fn read_header(&mut self) -> VexelResult<()> {
        let mut byte_order_marker = [0u8; 2];
        self.reader.read_exact(&mut byte_order_marker)?;

        let byte_order = match &byte_order_marker {
            b"II" => ByteOrder::LittleEndian,
            b"MM" => ByteOrder::BigEndian,
            _ => return Err(VexelError::Custom("Invalid byte order marker".to_string())),
        };

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
                    self.header.bits_per_sample = read_multiple_values(type_, count, value_offset, &mut self.reader)?
                }
                259 => {
                    self.header.compression = match read_single_value(type_, value_offset, &mut self.reader)? {
                        1 => Compression::None,
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
                273 => self.header.strip_offsets = read_multiple_values(type_, count, value_offset, &mut self.reader)?,
                277 => self.header.samples_per_pixel = read_single_value(type_, value_offset, &mut self.reader)?,
                278 => self.header.rows_per_strip = read_single_value(type_, value_offset, &mut self.reader)?,
                279 => {
                    self.header.strip_byte_counts = read_multiple_values(type_, count, value_offset, &mut self.reader)?
                }
                282 => self.header.x_resolution = read_rational(value_offset, &mut self.reader)?,
                283 => self.header.y_resolution = read_rational(value_offset, &mut self.reader)?,
                284 => self.header.planar_configuration = read_single_value(type_, value_offset, &mut self.reader)?,
                296 => self.header.resolution_unit = read_single_value(type_, value_offset, &mut self.reader)?,
                _ => {}
            }

            self.reader.seek(SeekFrom::Start(current_pos))?;
        }

        self.width = self.header.image_width;
        self.height = self.header.image_length;

        Ok(())
    }

    fn convert_to_pixel_data(&self, data: Vec<u8>, header: &TiffHeader) -> VexelResult<PixelData> {
        match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero | PhotometricInterpretation::BlackIsZero => {
                // 0 = WhiteIsZero, 1 = BlackIsZero (Grayscale)
                match (
                    header.samples_per_pixel,
                    header.bits_per_sample.get(0).copied().unwrap_or(1),
                ) {
                    (1, 1) => Ok(PixelData::L1(data)),
                    (1, 8) => Ok(PixelData::L8(data)),
                    (1, 16) => {
                        let pixels = data
                            .chunks_exact(2)
                            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                            .collect();
                        Ok(PixelData::L16(pixels))
                    }
                    (2, 8) => Ok(PixelData::LA8(data)), // Grayscale with alpha
                    (2, 16) => {
                        let pixels = data
                            .chunks_exact(4)
                            .map(|chunk| {
                                let gray = u16::from_le_bytes([chunk[0], chunk[1]]);
                                let alpha = u16::from_le_bytes([chunk[2], chunk[3]]);
                                vec![gray, alpha]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::LA16(pixels))
                    }
                    _ => Err(VexelError::Custom(format!(
                        "Unsupported grayscale format: {} samples with {} bits",
                        header.samples_per_pixel,
                        header.bits_per_sample.get(0).unwrap_or(&0)
                    ))),
                }
            }
            PhotometricInterpretation::RGB => {
                // RGB
                match (
                    header.samples_per_pixel,
                    header.bits_per_sample.get(0).copied().unwrap_or(8),
                ) {
                    (3, 8) => Ok(PixelData::RGB8(data)),
                    (4, 8) => Ok(PixelData::RGBA8(data)),
                    (3, 16) => {
                        let pixels = data
                            .chunks_exact(6)
                            .map(|chunk| {
                                vec![
                                    u16::from_le_bytes([chunk[0], chunk[1]]),
                                    u16::from_le_bytes([chunk[2], chunk[3]]),
                                    u16::from_le_bytes([chunk[4], chunk[5]]),
                                ]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::RGB16(pixels))
                    }
                    (4, 16) => {
                        let pixels = data
                            .chunks_exact(8)
                            .map(|chunk| {
                                vec![
                                    u16::from_le_bytes([chunk[0], chunk[1]]),
                                    u16::from_le_bytes([chunk[2], chunk[3]]),
                                    u16::from_le_bytes([chunk[4], chunk[5]]),
                                    u16::from_le_bytes([chunk[6], chunk[7]]),
                                ]
                            })
                            .flatten()
                            .collect();
                        Ok(PixelData::RGBA16(pixels))
                    }
                    _ => Err(VexelError::Custom(format!(
                        "Unsupported RGB format: {} samples with {} bits",
                        header.samples_per_pixel,
                        header.bits_per_sample.get(0).unwrap_or(&0)
                    ))),
                }
            }
            PhotometricInterpretation::Palette => {
                unimplemented!();
            }
            PhotometricInterpretation::TransparencyMask => {
                // Mask (transparency mask)
                Ok(PixelData::L1(data))
            }
            PhotometricInterpretation::CMYK => {
                // Separated (usually CMYK)
                unimplemented!();
            }
            PhotometricInterpretation::YCbCr => {
                // YCbCr
                unimplemented!();
            }
            PhotometricInterpretation::CIELab => {
                // CIELab
                unimplemented!();
            }
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let mut bytes = Vec::new();

        for (offset, byte_count) in self
            .header
            .strip_offsets
            .iter()
            .zip(self.header.strip_byte_counts.iter())
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

        let mut pixel_data = self.convert_to_pixel_data(bytes, &self.header)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
