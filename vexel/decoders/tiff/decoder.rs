use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::{Image, PixelData};
use std::io::{Read, Seek, SeekFrom};

use super::types::{Compression, ExtraSampleType, PhotometricInterpretation, SampleFormat, TiffHeader};

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
    byte_order: ByteOrder,
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

    let bytes_per_value: u32 = match type_ {
        1 | 2 => 1,
        3 => 2,
        4 => 4,
        5 | 10 => 8,
        _ => 4,
    };
    let total_bytes = bytes_per_value * count;

    if total_bytes <= 4 {
        let val_bytes = match byte_order {
            ByteOrder::LittleEndian => value_offset.to_le_bytes(),
            ByteOrder::BigEndian => value_offset.to_be_bytes(),
        };
        let mut cursor = std::io::Cursor::new(val_bytes);
        let mut inline_reader = BitReader::new(&mut cursor);
        inline_reader.set_endianness(byte_order);
        for _ in 0..count {
            let value = match type_ {
                1 => inline_reader.read_u8()? as u32,
                3 => inline_reader.read_u16()? as u32,
                4 => inline_reader.read_u32()?,
                _ => inline_reader.read_u8()? as u32,
            };
            values.push(T::try_from(value).map_err(|_| VexelError::Custom("Value conversion error".to_string()))?);
        }
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

fn read_multiple_rationals<R: Read + Seek>(
    count: u32,
    value_offset: u32,
    reader: &mut BitReader<R>,
) -> VexelResult<Vec<f32>> {
    reader.seek(SeekFrom::Start(value_offset as u64))?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let numerator = reader.read_u32()?;
        let denominator = reader.read_u32()?;
        if denominator == 0 {
            values.push(0.0);
        } else {
            values.push(numerator as f32 / denominator as f32);
        }
    }
    Ok(values)
}

fn u16_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> u16 {
    match byte_order {
        ByteOrder::LittleEndian => u16::from_le_bytes([bytes[0], bytes[1]]),
        ByteOrder::BigEndian => u16::from_be_bytes([bytes[0], bytes[1]]),
    }
}

fn u32_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> u32 {
    match byte_order {
        ByteOrder::LittleEndian => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        ByteOrder::BigEndian => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}

fn f32_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> f32 {
    let bits = u32_from_bytes(bytes, byte_order);
    f32::from_bits(bits)
}

fn f64_from_bytes(bytes: &[u8], byte_order: ByteOrder) -> f64 {
    let bits = match byte_order {
        ByteOrder::LittleEndian => {
            u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
        }
        ByteOrder::BigEndian => {
            u64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
        }
    };
    f64::from_bits(bits)
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) as u32) << 31;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mantissa = (bits & 0x3FF) as u32;

    let f32_bits = if exp == 0 {
        if mantissa == 0 {
            sign
        } else {
            let mut m = mantissa;
            let mut e = 127u32 - 14;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            sign | (e << 23) | (m << 13)
        }
    } else if exp == 31 {
        sign | 0x7F800000 | (mantissa << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (mantissa << 13)
    };

    f32::from_bits(f32_bits)
}

fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> (u8, u8, u8) {
    let c = c as f32 / 255.0;
    let m = m as f32 / 255.0;
    let y = y as f32 / 255.0;
    let k = k as f32 / 255.0;

    let r = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
    let g = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
    let b = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;

    (r, g, b)
}

struct YCbCrTables {
    y_tab: [i32; 256],
    cr_r_tab: [i32; 256],
    cb_b_tab: [i32; 256],
    cr_g_tab: [i32; 256],
    cb_g_tab: [i32; 256],
}

impl YCbCrTables {
    fn new(luma: [f32; 3], ref_black_white: [f32; 6]) -> Self {
        const SHIFT: u32 = 16;
        const ONE_HALF: i32 = 1 << (SHIFT - 1);

        fn fix(x: f32) -> i32 {
            (x as f64 * (1i64 << SHIFT) as f64 + 0.5) as i32
        }

        fn code2v(c: f32, rb: f32, rw: f32, cr: f32) -> f32 {
            let range = rw - rb;
            if range.abs() < f32::EPSILON {
                0.0
            } else {
                (c - rb) * cr / range
            }
        }

        let luma_red = luma[0];
        let luma_green = luma[1];
        let luma_blue = luma[2];

        let f1 = 2.0 - 2.0 * luma_red;
        let d1 = fix(f1.clamp(0.0, 2.0));
        let f2 = luma_red * f1 / luma_green;
        let d2 = -fix(f2.clamp(0.0, 2.0));
        let f3 = 2.0 - 2.0 * luma_blue;
        let d3 = fix(f3.clamp(0.0, 2.0));
        let f4 = luma_blue * f3 / luma_green;
        let d4 = -fix(f4.clamp(0.0, 2.0));

        let mut y_tab = [0i32; 256];
        let mut cr_r_tab = [0i32; 256];
        let mut cb_b_tab = [0i32; 256];
        let mut cr_g_tab = [0i32; 256];
        let mut cb_g_tab = [0i32; 256];

        for i in 0..256i32 {
            let x = i - 128;

            let cr = code2v(
                x as f32,
                ref_black_white[4] - 128.0,
                ref_black_white[5] - 128.0,
                127.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0) as i32;

            let cb = code2v(
                x as f32,
                ref_black_white[2] - 128.0,
                ref_black_white[3] - 128.0,
                127.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0) as i32;

            cr_r_tab[i as usize] = (d1 * cr + ONE_HALF) >> SHIFT;
            cb_b_tab[i as usize] = (d3 * cb + ONE_HALF) >> SHIFT;
            cr_g_tab[i as usize] = d2 * cr;
            cb_g_tab[i as usize] = d4 * cb + ONE_HALF;

            let y_val = code2v(
                (x + 128) as f32,
                ref_black_white[0],
                ref_black_white[1],
                255.0,
            )
            .clamp(-128.0 * 32.0, 128.0 * 32.0);
            y_tab[i as usize] = y_val as i32;
        }

        Self {
            y_tab,
            cr_r_tab,
            cb_b_tab,
            cr_g_tab,
            cb_g_tab,
        }
    }

    fn convert(&self, y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
        let y_idx = y as usize;
        let cb_idx = cb as usize;
        let cr_idx = cr as usize;

        let r = self.y_tab[y_idx] + self.cr_r_tab[cr_idx];
        let g = self.y_tab[y_idx] + ((self.cb_g_tab[cb_idx] + self.cr_g_tab[cr_idx]) >> 16);
        let b = self.y_tab[y_idx] + self.cb_b_tab[cb_idx];

        (clamp_u8(r), clamp_u8(g), clamp_u8(b))
    }
}

const SRGB_MATRIX: [[f32; 3]; 3] = [
    [3.2410, -1.5374, -0.4986],
    [-0.9692, 1.8760, 0.0416],
    [0.0556, -0.2040, 1.0570],
];

const D50_WHITE: [f32; 3] = [0.9642, 1.0000, 0.8251];
const D65_WHITE: [f32; 3] = [0.9505, 1.0000, 1.0890];

fn lab_to_xyz(l: f32, a: f32, b: f32, white: [f32; 3]) -> (f32, f32, f32) {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;

    const DELTA: f32 = 6.0 / 29.0;
    const DELTA2: f32 = DELTA * DELTA;

    let x = if fx > DELTA { fx * fx * fx } else { 3.0 * DELTA2 * (fx - 4.0 / 29.0) };
    let y = if fy > DELTA { fy * fy * fy } else { 3.0 * DELTA2 * (fy - 4.0 / 29.0) };
    let z = if fz > DELTA { fz * fz * fz } else { 3.0 * DELTA2 * (fz - 4.0 / 29.0) };

    (x * white[0], y * white[1], z * white[2])
}

fn xyz_to_srgb(x: f32, y: f32, z: f32) -> (u8, u8, u8) {
    let m = &SRGB_MATRIX;
    let r_lin = m[0][0] * x + m[0][1] * y + m[0][2] * z;
    let g_lin = m[1][0] * x + m[1][1] * y + m[1][2] * z;
    let b_lin = m[2][0] * x + m[2][1] * y + m[2][2] * z;

    fn linear_to_srgb(v: f32) -> u8 {
        let v = v.clamp(0.0, 1.0);
        let srgb = if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    }

    (linear_to_srgb(r_lin), linear_to_srgb(g_lin), linear_to_srgb(b_lin))
}

fn cielab_to_rgb(l_raw: u8, a_raw: i8, b_raw: i8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32;
    let b = b_raw as f32;
    let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
    xyz_to_srgb(x, y, z)
}

fn icclab_to_rgb(l_raw: u8, a_raw: u8, b_raw: u8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32 - 128.0;
    let b = b_raw as f32 - 128.0;
    let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
    xyz_to_srgb(x, y, z)
}

fn itulab_to_rgb(l_raw: u8, a_raw: u8, b_raw: u8) -> (u8, u8, u8) {
    let l = l_raw as f32 * 100.0 / 255.0;
    let a = a_raw as f32 - 128.0;
    let b = b_raw as f32 - 128.0;
    let (x, y, z) = lab_to_xyz(l, a, b, D65_WHITE);
    xyz_to_srgb(x, y, z)
}

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

    fn has_alpha(&self) -> bool {
        self.header.extra_samples.iter().any(|&s| {
            s == ExtraSampleType::AssociatedAlpha as u16 || s == ExtraSampleType::UnassociatedAlpha as u16
        })
    }

    fn sample_format_for(&self, channel: usize) -> SampleFormat {
        self.header
            .sample_format
            .get(channel)
            .copied()
            .unwrap_or(SampleFormat::UnsignedInt)
    }

    fn bits_for(&self, channel: usize) -> u16 {
        self.header.bits_per_sample.get(channel).copied().unwrap_or(8)
    }

    fn read_grayscale(&self, data: &[u8], invert: bool) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let fmt = self.sample_format_for(0);
        let has_alpha = self.has_alpha();
        let spp = self.header.samples_per_pixel as usize;

        match (bps, fmt, has_alpha) {
            (1, SampleFormat::UnsignedInt, false) => {
                if invert {
                    return Ok(PixelData::L1(data.iter().map(|&b| !b).collect()));
                }
                Ok(PixelData::L1(data.to_vec()))
            }

            (2, SampleFormat::UnsignedInt, false) => {
                let pixels: Vec<u8> = data
                    .iter()
                    .flat_map(|&byte| {
                        (0..4).map(move |i| {
                            let val = (byte >> (6 - i * 2)) & 0x3;
                            let expanded = val * 85;
                            if invert { 255 - expanded } else { expanded }
                        })
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (4, SampleFormat::UnsignedInt, false) => {
                let pixels: Vec<u8> = data
                    .iter()
                    .flat_map(|&byte| {
                        let hi = (byte >> 4) & 0xF;
                        let lo = byte & 0xF;
                        let hi_out = if invert { 255 - hi * 17 } else { hi * 17 };
                        let lo_out = if invert { 255 - lo * 17 } else { lo * 17 };
                        [hi_out, lo_out]
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (8, SampleFormat::UnsignedInt, false) => {
                if invert {
                    return Ok(PixelData::L8(data.iter().map(|&b| 255 - b).collect()));
                }
                Ok(PixelData::L8(data.to_vec()))
            }

            (16, SampleFormat::UnsignedInt, false) => {
                let pixels: Vec<u16> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let v = u16_from_bytes(chunk, self.byte_order);
                        if invert { u16::MAX - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L16(pixels))
            }

            (32, SampleFormat::UnsignedInt, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(4)
                    .map(|chunk| {
                        let v = u32_from_bytes(chunk, self.byte_order);
                        let normalized = (v as f64 / u32::MAX as f64 * 255.0).round() as u8;
                        if invert { 255 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (8, SampleFormat::SignedInt, false) => {
                let pixels: Vec<u8> = data
                    .iter()
                    .map(|&b| {
                        let normalized = (b as i8 as f32 + 128.0).round() as u8;
                        if invert { 255 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (16, SampleFormat::SignedInt, false) => {
                let pixels: Vec<u16> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let v = u16_from_bytes(chunk, self.byte_order) as i16;
                        let normalized = (v as f32 + 32768.0).round() as u16;
                        if invert { u16::MAX - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L16(pixels))
            }

            (32, SampleFormat::SignedInt, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(4)
                    .map(|chunk| {
                        let v = match self.byte_order {
                            ByteOrder::LittleEndian => i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                            ByteOrder::BigEndian => i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                        } as i64;
                        let normalized = ((v + i32::MAX as i64 + 1) as f64 / u32::MAX as f64 * 255.0).round() as u8;
                        if invert { 255 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (64, SampleFormat::SignedInt, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(8)
                    .map(|chunk| {
                        let v = match self.byte_order {
                            ByteOrder::LittleEndian => i64::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]]),
                            ByteOrder::BigEndian => i64::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]]),
                        } as i128;
                        let normalized = ((v + i64::MAX as i128 + 1) as f64 / u64::MAX as f64 * 255.0).round() as u8;
                        if invert { 255 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (16, SampleFormat::Float, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let v = half_to_f32(u16_from_bytes(chunk, self.byte_order));
                        let out = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                        if invert { 255 - out } else { out }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (32, SampleFormat::Float, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(4)
                    .map(|chunk| {
                        let v = f32_from_bytes(chunk, self.byte_order);
                        let out = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                        if invert { 255 - out } else { out }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (64, SampleFormat::Float, false) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(8)
                    .map(|chunk| {
                        let v = f64_from_bytes(chunk, self.byte_order) as f32;
                        let out = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                        if invert { 255 - out } else { out }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (8, _, true) if spp >= 2 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(spp)
                    .flat_map(|chunk| {
                        let gray = if invert { 255 - chunk[0] } else { chunk[0] };
                        [gray, chunk[1]]
                    })
                    .collect();
                Ok(PixelData::LA8(pixels))
            }

            (16, SampleFormat::UnsignedInt, true) if spp >= 2 => {
                let pixels: Vec<u16> = data
                    .chunks_exact(spp * 2)
                    .flat_map(|chunk| {
                        let gray = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let alpha = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let gray_out = if invert { u16::MAX - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA16(pixels))
            }

            (16, SampleFormat::Float, true) if spp >= 2 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(spp * 2)
                    .flat_map(|chunk| {
                        let gray = half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order));
                        let alpha = half_to_f32(u16_from_bytes(&chunk[2..4], self.byte_order));
                        let gray_out = (gray.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let gray_out = if invert { 255 - gray_out } else { gray_out };
                        let alpha_out = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                        [gray_out, alpha_out]
                    })
                    .collect();
                Ok(PixelData::LA8(pixels))
            }

            (32, SampleFormat::Float, true) if spp >= 2 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(spp * 4)
                    .flat_map(|chunk| {
                        let gray = f32_from_bytes(&chunk[0..4], self.byte_order);
                        let alpha = f32_from_bytes(&chunk[4..8], self.byte_order);
                        let gray_out = (gray.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let gray_out = if invert { 255 - gray_out } else { gray_out };
                        let alpha_out = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                        [gray_out, alpha_out]
                    })
                    .collect();
                Ok(PixelData::LA8(pixels))
            }

            (64, SampleFormat::Float, true) if spp >= 2 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(spp * 8)
                    .flat_map(|chunk| {
                        let gray = f64_from_bytes(&chunk[0..8], self.byte_order) as f32;
                        let alpha = f64_from_bytes(&chunk[8..16], self.byte_order) as f32;
                        let gray_out = (gray.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let gray_out = if invert { 255 - gray_out } else { gray_out };
                        let alpha_out = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                        [gray_out, alpha_out]
                    })
                    .collect();
                Ok(PixelData::LA8(pixels))
            }

            (16, SampleFormat::SignedInt, true) if spp >= 2 => {
                let pixels: Vec<u16> = data
                    .chunks_exact(spp * 2)
                    .flat_map(|chunk| {
                        let gray = (u16_from_bytes(&chunk[0..2], self.byte_order) as i16 as f32 + 32768.0).round() as u16;
                        let alpha = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let gray_out = if invert { u16::MAX - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA16(pixels))
            }

            _ => {
                Ok(PixelData::L8(data.iter().map(|&b| if invert { 255 - b } else { b }).collect()))
            }
        }
    }

    fn read_rgb(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let fmt = self.sample_format_for(0);
        let spp = self.header.samples_per_pixel as usize;
        let has_alpha = spp >= 4 && self.has_alpha();

        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        match (bps, fmt) {
            (8, SampleFormat::UnsignedInt) => {
                if has_alpha {
                    let pixels: Vec<u8> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
                        .collect();
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    let pixels: Vec<u8> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| [chunk[0], chunk[1], chunk[2]])
                        .collect();
                    Ok(PixelData::RGB8(pixels))
                }
            }

            (16, SampleFormat::UnsignedInt) => {
                if has_alpha {
                    let pixels: Vec<u16> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            [
                                u16_from_bytes(&chunk[0..2], self.byte_order),
                                u16_from_bytes(&chunk[2..4], self.byte_order),
                                u16_from_bytes(&chunk[4..6], self.byte_order),
                                u16_from_bytes(&chunk[6..8], self.byte_order),
                            ]
                        })
                        .collect();
                    Ok(PixelData::RGBA16(pixels))
                } else {
                    let pixels: Vec<u16> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            [
                                u16_from_bytes(&chunk[0..2], self.byte_order),
                                u16_from_bytes(&chunk[2..4], self.byte_order),
                                u16_from_bytes(&chunk[4..6], self.byte_order),
                            ]
                        })
                        .collect();
                    Ok(PixelData::RGB16(pixels))
                }
            }

            (32, SampleFormat::UnsignedInt) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (u32_from_bytes(&chunk[0..4], self.byte_order) as f64 / u32::MAX as f64 * 255.0).round() as u8;
                        let g = (u32_from_bytes(&chunk[4..8], self.byte_order) as f64 / u32::MAX as f64 * 255.0).round() as u8;
                        let b = (u32_from_bytes(&chunk[8..12], self.byte_order) as f64 / u32::MAX as f64 * 255.0).round() as u8;
                        if has_alpha && chunk.len() >= 16 {
                            let a = (u32_from_bytes(&chunk[12..16], self.byte_order) as f64 / u32::MAX as f64 * 255.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }

            (8, SampleFormat::SignedInt) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (chunk[0] as i8 as f32 + 128.0).round() as u8;
                        let g = (chunk[1] as i8 as f32 + 128.0).round() as u8;
                        let b = (chunk[2] as i8 as f32 + 128.0).round() as u8;
                        if has_alpha && chunk.len() >= 4 {
                            let a = (chunk[3] as i8 as f32 + 128.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }

            (16, SampleFormat::SignedInt) => {
                let pixels: Vec<u16> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (u16_from_bytes(&chunk[0..2], self.byte_order) as i16 as f32 + 32768.0).round() as u16;
                        let g = (u16_from_bytes(&chunk[2..4], self.byte_order) as i16 as f32 + 32768.0).round() as u16;
                        let b = (u16_from_bytes(&chunk[4..6], self.byte_order) as i16 as f32 + 32768.0).round() as u16;
                        if has_alpha && chunk.len() >= 8 {
                            let a = (u16_from_bytes(&chunk[6..8], self.byte_order) as i16 as f32 + 32768.0).round() as u16;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA16(pixels))
                } else {
                    Ok(PixelData::RGB16(pixels))
                }
            }

            (16, SampleFormat::Float) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order)).clamp(0.0, 1.0) * 255.0).round() as u8;
                        let g = (half_to_f32(u16_from_bytes(&chunk[2..4], self.byte_order)).clamp(0.0, 1.0) * 255.0).round() as u8;
                        let b = (half_to_f32(u16_from_bytes(&chunk[4..6], self.byte_order)).clamp(0.0, 1.0) * 255.0).round() as u8;
                        if has_alpha && chunk.len() >= 8 {
                            let a = (half_to_f32(u16_from_bytes(&chunk[6..8], self.byte_order)).clamp(0.0, 1.0) * 255.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }

            (32, SampleFormat::Float) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (f32_from_bytes(&chunk[0..4], self.byte_order).clamp(0.0, 1.0) * 255.0).round() as u8;
                        let g = (f32_from_bytes(&chunk[4..8], self.byte_order).clamp(0.0, 1.0) * 255.0).round() as u8;
                        let b = (f32_from_bytes(&chunk[8..12], self.byte_order).clamp(0.0, 1.0) * 255.0).round() as u8;
                        if has_alpha && chunk.len() >= 16 {
                            let a = (f32_from_bytes(&chunk[12..16], self.byte_order).clamp(0.0, 1.0) * 255.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }

            (64, SampleFormat::Float) => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let r = (f64_from_bytes(&chunk[0..8], self.byte_order).clamp(0.0, 1.0) as f32 * 255.0).round() as u8;
                        let g = (f64_from_bytes(&chunk[8..16], self.byte_order).clamp(0.0, 1.0) as f32 * 255.0).round() as u8;
                        let b = (f64_from_bytes(&chunk[16..24], self.byte_order).clamp(0.0, 1.0) as f32 * 255.0).round() as u8;
                        if has_alpha && chunk.len() >= 32 {
                            let a = (f64_from_bytes(&chunk[24..32], self.byte_order).clamp(0.0, 1.0) as f32 * 255.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }

            _ => Err(VexelError::Custom(format!(
                "Unsupported RGB format: {} bps, {:?} format",
                bps, fmt
            ))),
        }
    }

    fn read_palette(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let color_map = &self.header.color_map;

        if color_map.is_empty() {
            return Err(VexelError::Custom("Missing ColorMap for Palette image".to_string()));
        }

        let n_colors = 1usize << bps;
        let is_16bit = color_map.iter().take(n_colors * 3).any(|&v| v >= 256);

        let get_rgb = |idx: usize| -> (u8, u8, u8) {
            if idx >= n_colors {
                return (0, 0, 0);
            }
            let r_raw = color_map.get(idx).copied().unwrap_or(0);
            let g_raw = color_map.get(idx + n_colors).copied().unwrap_or(0);
            let b_raw = color_map.get(idx + 2 * n_colors).copied().unwrap_or(0);

            if is_16bit {
                ((r_raw >> 8) as u8, (g_raw >> 8) as u8, (b_raw >> 8) as u8)
            } else {
                (r_raw as u8, g_raw as u8, b_raw as u8)
            }
        };

        let pixels: Vec<u8> = match bps {
            1 => data
                .iter()
                .flat_map(|&byte| {
                    (0..8u8).flat_map(move |i| {
                        let idx = ((byte >> (7 - i)) & 1) as usize;
                        let (r, g, b) = get_rgb(idx);
                        [r, g, b]
                    })
                })
                .collect(),
            2 => data
                .iter()
                .flat_map(|&byte| {
                    (0..4u8).flat_map(move |i| {
                        let idx = ((byte >> (6 - i * 2)) & 0x3) as usize;
                        let (r, g, b) = get_rgb(idx);
                        [r, g, b]
                    })
                })
                .collect(),
            4 => data
                .iter()
                .flat_map(|&byte| {
                    let hi = (byte >> 4) as usize;
                    let lo = (byte & 0xF) as usize;
                    let (r1, g1, b1) = get_rgb(hi);
                    let (r2, g2, b2) = get_rgb(lo);
                    [r1, g1, b1, r2, g2, b2]
                })
                .collect(),
            8 => data
                .iter()
                .flat_map(|&byte| {
                    let (r, g, b) = get_rgb(byte as usize);
                    [r, g, b]
                })
                .collect(),
            _ => {
                return Err(VexelError::Custom(format!(
                    "Unsupported bit depth for Palette: {}",
                    bps
                )))
            }
        };

        Ok(PixelData::RGB8(pixels))
    }

    fn read_cmyk(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let spp = self.header.samples_per_pixel as usize;
        let has_alpha = spp >= 5 && self.has_alpha();
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        match bps {
            8 => {
                if has_alpha {
                    let pixels: Vec<u8> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let (r, g, b) = cmyk_to_rgb(chunk[0], chunk[1], chunk[2], chunk[3]);
                            let a = chunk[4];
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    let pixels: Vec<u8> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let (r, g, b) = cmyk_to_rgb(chunk[0], chunk[1], chunk[2], chunk[3]);
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB8(pixels))
                }
            }
            16 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let c = (u16_from_bytes(&chunk[0..2], self.byte_order) as f32 / 65535.0 * 255.0).round() as u8;
                        let m = (u16_from_bytes(&chunk[2..4], self.byte_order) as f32 / 65535.0 * 255.0).round() as u8;
                        let y = (u16_from_bytes(&chunk[4..6], self.byte_order) as f32 / 65535.0 * 255.0).round() as u8;
                        let k = (u16_from_bytes(&chunk[6..8], self.byte_order) as f32 / 65535.0 * 255.0).round() as u8;
                        let (r, g, b) = cmyk_to_rgb(c, m, y, k);
                        if has_alpha && chunk.len() >= 10 {
                            let a = (u16_from_bytes(&chunk[8..10], self.byte_order) as f32 / 65535.0 * 255.0).round() as u8;
                            vec![r, g, b, a]
                        } else {
                            vec![r, g, b]
                        }
                    })
                    .collect();
                if has_alpha {
                    Ok(PixelData::RGBA8(pixels))
                } else {
                    Ok(PixelData::RGB8(pixels))
                }
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported CMYK bit depth: {}",
                bps
            ))),
        }
    }

    fn read_ycbcr(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        if bps != 8 {
            return Err(VexelError::Custom(format!(
                "Unsupported YCbCr bit depth: {} (only 8-bit supported)",
                bps
            )));
        }

        let tables = YCbCrTables::new(self.header.ycbcr_coefficients, self.header.reference_black_white);
        let [h_sub, v_sub] = self.header.ycbcr_sub_sampling;
        let h_sub = h_sub.max(1) as usize;
        let v_sub = v_sub.max(1) as usize;

        let width = self.width as usize;
        let height = self.height as usize;

        if h_sub == 1 && v_sub == 1 {
            let pixels: Vec<u8> = data
                .chunks_exact(3)
                .flat_map(|chunk| {
                    let (r, g, b) = tables.convert(chunk[0], chunk[1], chunk[2]);
                    [r, g, b]
                })
                .collect();
            return Ok(PixelData::RGB8(pixels));
        }

        let block_w = h_sub;
        let block_h = v_sub;
        let luma_per_block = block_w * block_h;
        let bytes_per_block = luma_per_block + 2;

        let blocks_x = width.div_ceil(block_w);
        let blocks_y = height.div_ceil(block_h);

        let mut pixels = vec![0u8; width * height * 3];

        let mut block_offset = 0;
        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                if block_offset + bytes_per_block > data.len() {
                    break;
                }
                let block = &data[block_offset..block_offset + bytes_per_block];
                let cb = block[luma_per_block];
                let cr = block[luma_per_block + 1];

                for row in 0..block_h {
                    for col in 0..block_w {
                        let px = bx * block_w + col;
                        let py = by * block_h + row;
                        if px >= width || py >= height {
                            continue;
                        }
                        let y_idx = row * block_w + col;
                        let y = block[y_idx];
                        let (r, g, b) = tables.convert(y, cb, cr);
                        let out_idx = (py * width + px) * 3;
                        pixels[out_idx] = r;
                        pixels[out_idx + 1] = g;
                        pixels[out_idx + 2] = b;
                    }
                }

                block_offset += bytes_per_block;
            }
        }

        Ok(PixelData::RGB8(pixels))
    }

    fn read_cielab(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        match bps {
            8 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l = chunk[0];
                        let a = chunk[1] as i8;
                        let b = chunk[2] as i8;
                        let (r, g, b_out) = cielab_to_rgb(l, a, b);
                        [r, g, b_out]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            16 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order) as i16;
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order) as i16;

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = a_raw as f32 / 256.0;
                        let b = b_raw as f32 / 256.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
                        let (r, g, b_out) = xyz_to_srgb(x, y, z);
                        [r, g, b_out]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported CIELab bit depth: {}",
                bps
            ))),
        }
    }

    fn read_icclab(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        match bps {
            8 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let (r, g, b) = icclab_to_rgb(chunk[0], chunk[1], chunk[2]);
                        [r, g, b]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            16 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order);

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = (a_raw as f32 / 257.0) - 128.0;
                        let b = (b_raw as f32 / 257.0) - 128.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
                        let (r, g, b_out) = xyz_to_srgb(x, y, z);
                        [r, g, b_out]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported ICCLab bit depth: {}",
                bps
            ))),
        }
    }

    fn read_itulab(&self, data: &[u8]) -> VexelResult<PixelData> {
        let bps = self.bits_for(0);
        let spp = self.header.samples_per_pixel as usize;
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let bytes_per_pixel = bytes_per_sample * spp;

        match bps {
            8 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let (r, g, b) = itulab_to_rgb(chunk[0], chunk[1], chunk[2]);
                        [r, g, b]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            16 => {
                let pixels: Vec<u8> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order);

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = (a_raw as f32 / 257.0) - 128.0;
                        let b = (b_raw as f32 / 257.0) - 128.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D65_WHITE);
                        let (r, g, b_out) = xyz_to_srgb(x, y, z);
                        [r, g, b_out]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported ITULab bit depth: {}",
                bps
            ))),
        }
    }

    fn convert_planar_to_chunky(&self, data: &[u8]) -> Vec<u8> {
        let spp = self.header.samples_per_pixel as usize;
        let bps = self.bits_for(0);
        let bytes_per_sample = (bps as usize).div_ceil(8);
        let pixels_total = (self.width * self.height) as usize;
        let plane_size = pixels_total * bytes_per_sample;

        let mut chunky = vec![0u8; pixels_total * spp * bytes_per_sample];

        for sample in 0..spp {
            let plane_start = sample * plane_size;
            for px in 0..pixels_total {
                let src_start = plane_start + px * bytes_per_sample;
                let dst_start = (px * spp + sample) * bytes_per_sample;
                if src_start + bytes_per_sample <= data.len() && dst_start + bytes_per_sample <= chunky.len() {
                    chunky[dst_start..dst_start + bytes_per_sample]
                        .copy_from_slice(&data[src_start..src_start + bytes_per_sample]);
                }
            }
        }

        chunky
    }

    fn convert_to_pixel_data(&self, data: Vec<u8>, header: &TiffHeader) -> VexelResult<PixelData> {
        use super::types::PlanarConfiguration;

        let data = if header.planar_configuration == PlanarConfiguration::Planar && header.samples_per_pixel > 1 {
            self.convert_planar_to_chunky(&data)
        } else {
            data
        };

        match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero => self.read_grayscale(&data, true),
            PhotometricInterpretation::BlackIsZero => self.read_grayscale(&data, false),
            PhotometricInterpretation::RGB => self.read_rgb(&data),
            PhotometricInterpretation::Palette => self.read_palette(&data),
            PhotometricInterpretation::TransparencyMask => Ok(PixelData::L1(data)),
            PhotometricInterpretation::CMYK => self.read_cmyk(&data),
            PhotometricInterpretation::YCbCr => self.read_ycbcr(&data),
            PhotometricInterpretation::CIELab => self.read_cielab(&data),
            PhotometricInterpretation::ICCLab => self.read_icclab(&data),
            PhotometricInterpretation::ITULab => self.read_itulab(&data),
            PhotometricInterpretation::LogL | PhotometricInterpretation::LogLuv => Err(VexelError::Custom(
                "LogL/LogLuv requires SGI log compression, which is not supported".to_string(),
            )),
        }
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

        let mut pixel_data = self.convert_to_pixel_data(bytes, &header)?;
        pixel_data.correct_pixels(self.width, self.height);

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
