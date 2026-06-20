use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::PixelData;

use super::color::{
    cielab_to_rgb, cmyk_to_rgb, cmyk_to_rgb_f32, f32_from_bytes, f64_from_bytes, float24_to_f32, half_to_f32,
    icclab_to_rgb, itulab_to_rgb, lab_to_xyz, logluv32_to_rgb, u16_from_bytes, u32_from_bytes,
    xyz_to_srgb_f32, D50_WHITE, D65_WHITE, YCbCrTables,
};
use super::types::{ExtraSampleType, PhotometricInterpretation, PlanarConfiguration, SampleFormat, TiffHeader};

pub struct PixelReader {
    pub byte_order: ByteOrder,
    pub width: u32,
    pub height: u32,
}

impl PixelReader {
    pub fn has_alpha(header: &TiffHeader) -> bool {
        header.extra_samples.iter().any(|&s| {
            s == ExtraSampleType::AssociatedAlpha as u16 || s == ExtraSampleType::UnassociatedAlpha as u16
        })
    }

pub fn sample_format_for(header: &TiffHeader, channel: usize) -> SampleFormat {
        header
            .sample_format
            .get(channel)
            .copied()
            .unwrap_or(SampleFormat::UnsignedInt)
    }

    pub fn bits_for(header: &TiffHeader, channel: usize) -> u16 {
        header.bits_per_sample.get(channel).copied().unwrap_or(8)
    }

    pub fn read_grayscale(&self, data: &[u8], header: &TiffHeader, invert: bool) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let fmt = Self::sample_format_for(header, 0);
        let has_alpha = Self::has_alpha(header);
        let spp = header.samples_per_pixel as usize;

        match (bps, fmt, has_alpha) {
            (1, SampleFormat::UnsignedInt, false) => {
                let width = self.width as usize;
                let row_bytes = if spp > 1 {
                    (width * spp).div_ceil(8)
                } else {
                    width.div_ceil(8)
                };
                let mut pixels = Vec::with_capacity(width * self.height as usize);
                if spp > 1 {
                    for row in data.chunks(row_bytes) {
                        for col in 0..width {
                            let bit_idx = col * spp;
                            let byte = row.get(bit_idx / 8).copied().unwrap_or(0);
                            let bit = (byte >> (7 - (bit_idx % 8))) & 1;
                            pixels.push(if invert { 1 - bit } else { bit });
                        }
                    }
                } else {
                    for row in data.chunks(row_bytes) {
                        for col in 0..width {
                            let byte = row.get(col / 8).copied().unwrap_or(0);
                            let bit = (byte >> (7 - (col % 8))) & 1;
                            pixels.push(if invert { 1 - bit } else { bit });
                        }
                    }
                }
                Ok(PixelData::L1(pixels))
            }

            (2, SampleFormat::UnsignedInt, false) if spp == 1 => {
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

            (2, SampleFormat::UnsignedInt, false) => {
                let bytes_per_pixel = (spp * 2).div_ceil(8);
                let pixels: Vec<u8> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let val = (chunk[0] >> 6) & 0x3;
                        let expanded = val * 85;
                        if invert { 255 - expanded } else { expanded }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (4, SampleFormat::UnsignedInt, false) if spp == 1 => {
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

            (4, SampleFormat::UnsignedInt, false) => {
                let bytes_per_pixel = (spp * 4).div_ceil(8);
                let pixels: Vec<u8> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let hi = (chunk[0] >> 4) & 0xF;
                        let out = if invert { 255 - hi * 17 } else { hi * 17 };
                        out
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (8, SampleFormat::UnsignedInt, false) => {
                let pixels: Vec<u8> = data
                    .chunks(spp)
                    .map(|chunk| {
                        let b = chunk[0];
                        if invert { 255 - b } else { b }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (16, SampleFormat::UnsignedInt, false) => {
                let bytes_per_pixel = spp * 2;
                let pixels: Vec<u16> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = u16_from_bytes(&chunk[0..2], self.byte_order);
                        if invert { u16::MAX - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L16(pixels))
            }

            (32, SampleFormat::UnsignedInt, false) => {
                let bytes_per_pixel = spp * 4;
                let pixels: Vec<f32> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = u32_from_bytes(&chunk[0..4], self.byte_order) as f64 / u32::MAX as f64;
                        let v = if invert { 1.0 - v } else { v };
                        v as f32
                    })
                    .collect();
                Ok(PixelData::L32F(pixels))
            }

            (8, SampleFormat::SignedInt, false) => {
                let pixels: Vec<u8> = data
                    .chunks(spp)
                    .map(|chunk| {
                        let b = chunk[0];
                        if invert { 255 - b } else { b }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }

            (16, SampleFormat::SignedInt, false) => {
                let bytes_per_pixel = spp * 2;
                let pixels: Vec<u16> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = u16_from_bytes(&chunk[0..2], self.byte_order);
                        if invert { u16::MAX - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L16(pixels))
            }

            (32, SampleFormat::SignedInt, false) => {
                let bytes_per_pixel = spp * 4;
                let pixels: Vec<f32> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let raw = u32_from_bytes(&chunk[0..4], self.byte_order);
                        let v = i32::from_ne_bytes(raw.to_ne_bytes());
                        let normalized = (v as f64 - i32::MIN as f64) / u32::MAX as f64;
                        let normalized = if invert { 1.0 - normalized } else { normalized };
                        normalized as f32
                    })
                    .collect();
                Ok(PixelData::L32F(pixels))
            }

            (64, SampleFormat::UnsignedInt, false) => {
                let bytes_per_pixel = spp * 8;
                let pixels: Vec<f64> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = match self.byte_order {
                            ByteOrder::LittleEndian => u64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => u64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                        };
                        let normalized = v as f64 / u64::MAX as f64;
                        if invert { 1.0 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L64F(pixels))
            }

            (64, SampleFormat::SignedInt, false) => {
                let bytes_per_pixel = spp * 8;
                let pixels: Vec<f64> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = match self.byte_order {
                            ByteOrder::LittleEndian => i64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => i64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                        };
                        let normalized = (v as f64 - i64::MIN as f64) / u64::MAX as f64;
                        if invert { 1.0 - normalized } else { normalized }
                    })
                    .collect();
                Ok(PixelData::L64F(pixels))
            }

            (16, SampleFormat::Float, false) => {
                let bytes_per_pixel = spp * 2;
                let pixels: Vec<f32> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order));
                        if invert { 1.0 - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L32F(pixels))
            }

            (24, SampleFormat::Float, false) => {
                let bytes_per_pixel = spp * 3;
                let pixels: Vec<f32> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = float24_to_f32(&chunk[0..3], self.byte_order);
                        if invert { 1.0 - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L32F(pixels))
            }

            (32, SampleFormat::Float, false) => {
                let bytes_per_pixel = spp * 4;
                let pixels: Vec<f32> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = f32_from_bytes(&chunk[0..4], self.byte_order);
                        if invert { 1.0 - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L32F(pixels))
            }

            (64, SampleFormat::Float, false) => {
                let bytes_per_pixel = spp * 8;
                let pixels: Vec<f64> = data
                    .chunks(bytes_per_pixel)
                    .map(|chunk| {
                        let v = f64_from_bytes(&chunk[0..8], self.byte_order);
                        if invert { 1.0 - v } else { v }
                    })
                    .collect();
                Ok(PixelData::L64F(pixels))
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

            (16, SampleFormat::SignedInt, true) if spp >= 2 => {
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
                let pixels: Vec<f32> = data
                    .chunks_exact(spp * 2)
                    .flat_map(|chunk| {
                        let gray = half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order));
                        let alpha = half_to_f32(u16_from_bytes(&chunk[2..4], self.byte_order));
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA32F(pixels))
            }

            (32, SampleFormat::UnsignedInt, true) if spp >= 2 => {
                let pixels: Vec<f32> = data
                    .chunks_exact(spp * 4)
                    .flat_map(|chunk| {
                        let gray = u32_from_bytes(&chunk[0..4], self.byte_order) as f64 / u32::MAX as f64;
                        let alpha = u32_from_bytes(&chunk[4..8], self.byte_order) as f64 / u32::MAX as f64;
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out as f32, alpha as f32]
                    })
                    .collect();
                Ok(PixelData::LA32F(pixels))
            }

            (32, SampleFormat::SignedInt, true) if spp >= 2 => {
                let pixels: Vec<f32> = data
                    .chunks_exact(spp * 4)
                    .flat_map(|chunk| {
                        let raw_g = u32_from_bytes(&chunk[0..4], self.byte_order);
                        let raw_a = u32_from_bytes(&chunk[4..8], self.byte_order);
                        let gray = (i32::from_ne_bytes(raw_g.to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                        let alpha = (i32::from_ne_bytes(raw_a.to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out as f32, alpha as f32]
                    })
                    .collect();
                Ok(PixelData::LA32F(pixels))
            }

            (32, SampleFormat::Float, true) if spp >= 2 => {
                let pixels: Vec<f32> = data
                    .chunks_exact(spp * 4)
                    .flat_map(|chunk| {
                        let gray = f32_from_bytes(&chunk[0..4], self.byte_order);
                        let alpha = f32_from_bytes(&chunk[4..8], self.byte_order);
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA32F(pixels))
            }

            (64, SampleFormat::UnsignedInt, true) if spp >= 2 => {
                let pixels: Vec<f64> = data
                    .chunks_exact(spp * 8)
                    .flat_map(|chunk| {
                        let gray = match self.byte_order {
                            ByteOrder::LittleEndian => u64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => u64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                        } as f64 / u64::MAX as f64;
                        let alpha = match self.byte_order {
                            ByteOrder::LittleEndian => u64::from_le_bytes(chunk[8..16].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => u64::from_be_bytes(chunk[8..16].try_into().unwrap_or_default()),
                        } as f64 / u64::MAX as f64;
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA64F(pixels))
            }

            (64, SampleFormat::SignedInt, true) if spp >= 2 => {
                let pixels: Vec<f64> = data
                    .chunks_exact(spp * 8)
                    .flat_map(|chunk| {
                        let gray = match self.byte_order {
                            ByteOrder::LittleEndian => i64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => i64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                        };
                        let alpha = match self.byte_order {
                            ByteOrder::LittleEndian => i64::from_le_bytes(chunk[8..16].try_into().unwrap_or_default()),
                            ByteOrder::BigEndian => i64::from_be_bytes(chunk[8..16].try_into().unwrap_or_default()),
                        };
                        let gray = (gray as f64 - i64::MIN as f64) / u64::MAX as f64;
                        let alpha = (alpha as f64 - i64::MIN as f64) / u64::MAX as f64;
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA64F(pixels))
            }

            (64, SampleFormat::Float, true) if spp >= 2 => {
                let pixels: Vec<f64> = data
                    .chunks_exact(spp * 8)
                    .flat_map(|chunk| {
                        let gray = f64_from_bytes(&chunk[0..8], self.byte_order);
                        let alpha = f64_from_bytes(&chunk[8..16], self.byte_order);
                        let gray_out = if invert { 1.0 - gray } else { gray };
                        [gray_out, alpha]
                    })
                    .collect();
                Ok(PixelData::LA64F(pixels))
            }

            _ => Ok(PixelData::L8(
                data.iter().map(|&b| if invert { 255 - b } else { b }).collect(),
            )),
        }
    }

    pub fn read_rgb(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let fmt = Self::sample_format_for(header, 0);
        let spp = header.samples_per_pixel as usize;
        let has_alpha = spp >= 4 && Self::has_alpha(header);

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
                if has_alpha {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = u32_from_bytes(&chunk[0..4], self.byte_order) as f64 / u32::MAX as f64;
                            let g = u32_from_bytes(&chunk[4..8], self.byte_order) as f64 / u32::MAX as f64;
                            let b = u32_from_bytes(&chunk[8..12], self.byte_order) as f64 / u32::MAX as f64;
                            let a = if chunk.len() >= 16 {
                                u32_from_bytes(&chunk[12..16], self.byte_order) as f64 / u32::MAX as f64
                            } else {
                                1.0
                            };
                            [r as f32, g as f32, b as f32, a as f32]
                        })
                        .collect();
                    Ok(PixelData::RGBA32F(pixels))
                } else {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = u32_from_bytes(&chunk[0..4], self.byte_order) as f64 / u32::MAX as f64;
                            let g = u32_from_bytes(&chunk[4..8], self.byte_order) as f64 / u32::MAX as f64;
                            let b = u32_from_bytes(&chunk[8..12], self.byte_order) as f64 / u32::MAX as f64;
                            [r as f32, g as f32, b as f32]
                        })
                        .collect();
                    Ok(PixelData::RGB32F(pixels))
                }
            }

            (8, SampleFormat::SignedInt) => {
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

            (16, SampleFormat::SignedInt) => {
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

            (32, SampleFormat::SignedInt) => {
                if has_alpha {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = (i32::from_ne_bytes(u32_from_bytes(&chunk[0..4], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            let g = (i32::from_ne_bytes(u32_from_bytes(&chunk[4..8], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            let b = (i32::from_ne_bytes(u32_from_bytes(&chunk[8..12], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            let a = if chunk.len() >= 16 {
                                (i32::from_ne_bytes(u32_from_bytes(&chunk[12..16], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64
                            } else {
                                1.0
                            };
                            [r as f32, g as f32, b as f32, a as f32]
                        })
                        .collect();
                    Ok(PixelData::RGBA32F(pixels))
                } else {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = (i32::from_ne_bytes(u32_from_bytes(&chunk[0..4], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            let g = (i32::from_ne_bytes(u32_from_bytes(&chunk[4..8], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            let b = (i32::from_ne_bytes(u32_from_bytes(&chunk[8..12], self.byte_order).to_ne_bytes()) as f64 - i32::MIN as f64) / u32::MAX as f64;
                            [r as f32, g as f32, b as f32]
                        })
                        .collect();
                    Ok(PixelData::RGB32F(pixels))
                }
            }

            (64, SampleFormat::UnsignedInt) => {
                if has_alpha {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            let g = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[8..16].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[8..16].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            let b = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[16..24].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[16..24].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            let a = if chunk.len() >= 32 {
                                (match self.byte_order {
                                    ByteOrder::LittleEndian => u64::from_le_bytes(chunk[24..32].try_into().unwrap_or_default()),
                                    ByteOrder::BigEndian => u64::from_be_bytes(chunk[24..32].try_into().unwrap_or_default()),
                                }) as f64 / u64::MAX as f64
                            } else {
                                1.0
                            };
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA64F(pixels))
                } else {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[0..8].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[0..8].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            let g = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[8..16].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[8..16].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            let b = match self.byte_order {
                                ByteOrder::LittleEndian => u64::from_le_bytes(chunk[16..24].try_into().unwrap_or_default()),
                                ByteOrder::BigEndian => u64::from_be_bytes(chunk[16..24].try_into().unwrap_or_default()),
                            } as f64 / u64::MAX as f64;
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB64F(pixels))
                }
            }

            (64, SampleFormat::SignedInt) => {
                let read_i64 = |b: &[u8]| -> i64 {
                    match self.byte_order {
                        ByteOrder::LittleEndian => i64::from_le_bytes(b.try_into().unwrap_or_default()),
                        ByteOrder::BigEndian => i64::from_be_bytes(b.try_into().unwrap_or_default()),
                    }
                };
                let norm_i64 = |v: i64| -> f64 { (v as f64 - i64::MIN as f64) / u64::MAX as f64 };

                if has_alpha {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = norm_i64(read_i64(&chunk[0..8]));
                            let g = norm_i64(read_i64(&chunk[8..16]));
                            let b = norm_i64(read_i64(&chunk[16..24]));
                            let a = if chunk.len() >= 32 { norm_i64(read_i64(&chunk[24..32])) } else { 1.0 };
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA64F(pixels))
                } else {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = norm_i64(read_i64(&chunk[0..8]));
                            let g = norm_i64(read_i64(&chunk[8..16]));
                            let b = norm_i64(read_i64(&chunk[16..24]));
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB64F(pixels))
                }
            }

            (16, SampleFormat::Float) => {
                if has_alpha {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order));
                            let g = half_to_f32(u16_from_bytes(&chunk[2..4], self.byte_order));
                            let b = half_to_f32(u16_from_bytes(&chunk[4..6], self.byte_order));
                            let a = if chunk.len() >= 8 { half_to_f32(u16_from_bytes(&chunk[6..8], self.byte_order)) } else { 1.0 };
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA32F(pixels))
                } else {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = half_to_f32(u16_from_bytes(&chunk[0..2], self.byte_order));
                            let g = half_to_f32(u16_from_bytes(&chunk[2..4], self.byte_order));
                            let b = half_to_f32(u16_from_bytes(&chunk[4..6], self.byte_order));
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB32F(pixels))
                }
            }

            (32, SampleFormat::Float) => {
                if has_alpha {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = f32_from_bytes(&chunk[0..4], self.byte_order);
                            let g = f32_from_bytes(&chunk[4..8], self.byte_order);
                            let b = f32_from_bytes(&chunk[8..12], self.byte_order);
                            let a = if chunk.len() >= 16 { f32_from_bytes(&chunk[12..16], self.byte_order) } else { 1.0 };
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA32F(pixels))
                } else {
                    let pixels: Vec<f32> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = f32_from_bytes(&chunk[0..4], self.byte_order);
                            let g = f32_from_bytes(&chunk[4..8], self.byte_order);
                            let b = f32_from_bytes(&chunk[8..12], self.byte_order);
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB32F(pixels))
                }
            }

            (64, SampleFormat::Float) => {
                if has_alpha {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = f64_from_bytes(&chunk[0..8], self.byte_order);
                            let g = f64_from_bytes(&chunk[8..16], self.byte_order);
                            let b = f64_from_bytes(&chunk[16..24], self.byte_order);
                            let a = if chunk.len() >= 32 { f64_from_bytes(&chunk[24..32], self.byte_order) } else { 1.0 };
                            [r, g, b, a]
                        })
                        .collect();
                    Ok(PixelData::RGBA64F(pixels))
                } else {
                    let pixels: Vec<f64> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let r = f64_from_bytes(&chunk[0..8], self.byte_order);
                            let g = f64_from_bytes(&chunk[8..16], self.byte_order);
                            let b = f64_from_bytes(&chunk[16..24], self.byte_order);
                            [r, g, b]
                        })
                        .collect();
                    Ok(PixelData::RGB64F(pixels))
                }
            }

            _ => Err(VexelError::Custom(format!(
                "Unsupported RGB format: {} bps, {:?} format",
                bps, fmt
            ))),
        }
    }

    pub fn read_palette(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let color_map = &header.color_map;

        if color_map.is_empty() {
            return Err(VexelError::Custom("Missing ColorMap for Palette image".to_string()));
        }

        let n_colors = 1usize << bps;
        let is_16bit = color_map.iter().take(n_colors * 3).any(|&v| v >= 256);

        let get_rgb16 = |idx: usize| -> (u16, u16, u16) {
            if idx >= n_colors {
                return (0, 0, 0);
            }
            (
                color_map.get(idx).copied().unwrap_or(0),
                color_map.get(idx + n_colors).copied().unwrap_or(0),
                color_map.get(idx + 2 * n_colors).copied().unwrap_or(0),
            )
        };

        let get_rgb8_direct = |idx: usize| -> (u8, u8, u8) {
            if idx >= n_colors {
                return (0, 0, 0);
            }
            let r_raw = color_map.get(idx).copied().unwrap_or(0);
            let g_raw = color_map.get(idx + n_colors).copied().unwrap_or(0);
            let b_raw = color_map.get(idx + 2 * n_colors).copied().unwrap_or(0);
            (r_raw as u8, g_raw as u8, b_raw as u8)
        };

        if is_16bit {
            let pixels: Vec<u16> = match bps {
                1 => data
                    .iter()
                    .flat_map(|&byte| {
                        (0..8u8).flat_map(move |i| {
                            let idx = ((byte >> (7 - i)) & 1) as usize;
                            let (r, g, b) = get_rgb16(idx);
                            [r, g, b]
                        })
                    })
                    .collect(),
                2 => data
                    .iter()
                    .flat_map(|&byte| {
                        (0..4u8).flat_map(move |i| {
                            let idx = ((byte >> (6 - i * 2)) & 0x3) as usize;
                            let (r, g, b) = get_rgb16(idx);
                            [r, g, b]
                        })
                    })
                    .collect(),
                4 => data
                    .iter()
                    .flat_map(|&byte| {
                        let hi = (byte >> 4) as usize;
                        let lo = (byte & 0xF) as usize;
                        let (r1, g1, b1) = get_rgb16(hi);
                        let (r2, g2, b2) = get_rgb16(lo);
                        [r1, g1, b1, r2, g2, b2]
                    })
                    .collect(),
                8 => data
                    .iter()
                    .flat_map(|&byte| {
                        let (r, g, b) = get_rgb16(byte as usize);
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
            return Ok(PixelData::RGB16(pixels));
        }

        let pixels: Vec<u8> = match bps {
            1 => data
                .iter()
                .flat_map(|&byte| {
                    (0..8u8).flat_map(move |i| {
                        let idx = ((byte >> (7 - i)) & 1) as usize;
                        let (r, g, b) = get_rgb8_direct(idx);
                        [r, g, b]
                    })
                })
                .collect(),
            2 => data
                .iter()
                .flat_map(|&byte| {
                    (0..4u8).flat_map(move |i| {
                        let idx = ((byte >> (6 - i * 2)) & 0x3) as usize;
                        let (r, g, b) = get_rgb8_direct(idx);
                        [r, g, b]
                    })
                })
                .collect(),
            4 => data
                .iter()
                .flat_map(|&byte| {
                    let hi = (byte >> 4) as usize;
                    let lo = (byte & 0xF) as usize;
                    let (r1, g1, b1) = get_rgb8_direct(hi);
                    let (r2, g2, b2) = get_rgb8_direct(lo);
                    [r1, g1, b1, r2, g2, b2]
                })
                .collect(),
            8 => data
                .iter()
                .flat_map(|&byte| {
                    let (r, g, b) = get_rgb8_direct(byte as usize);
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

    pub fn read_cmyk(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let spp = header.samples_per_pixel as usize;
        let has_alpha = spp >= 5 && Self::has_alpha(header);
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
                if has_alpha {
                    let pixels: Vec<u16> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let c = u16_from_bytes(&chunk[0..2], self.byte_order) as f32 / 65535.0;
                            let m = u16_from_bytes(&chunk[2..4], self.byte_order) as f32 / 65535.0;
                            let y = u16_from_bytes(&chunk[4..6], self.byte_order) as f32 / 65535.0;
                            let k = u16_from_bytes(&chunk[6..8], self.byte_order) as f32 / 65535.0;
                            let (r, g, b) = cmyk_to_rgb_f32(c, m, y, k);
                            let a = if chunk.len() >= 10 {
                                u16_from_bytes(&chunk[8..10], self.byte_order) as f32 / 65535.0
                            } else {
                                1.0
                            };
                            [(r * 65535.0).round() as u16, (g * 65535.0).round() as u16, (b * 65535.0).round() as u16, (a * 65535.0).round() as u16]
                        })
                        .collect();
                    Ok(PixelData::RGBA16(pixels))
                } else {
                    let pixels: Vec<u16> = data
                        .chunks_exact(bytes_per_pixel)
                        .flat_map(|chunk| {
                            let c = u16_from_bytes(&chunk[0..2], self.byte_order) as f32 / 65535.0;
                            let m = u16_from_bytes(&chunk[2..4], self.byte_order) as f32 / 65535.0;
                            let y = u16_from_bytes(&chunk[4..6], self.byte_order) as f32 / 65535.0;
                            let k = u16_from_bytes(&chunk[6..8], self.byte_order) as f32 / 65535.0;
                            let (r, g, b) = cmyk_to_rgb_f32(c, m, y, k);
                            [(r * 65535.0).round() as u16, (g * 65535.0).round() as u16, (b * 65535.0).round() as u16]
                        })
                        .collect();
                    Ok(PixelData::RGB16(pixels))
                }
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported CMYK bit depth: {}",
                bps
            ))),
        }
    }

    pub fn read_ycbcr(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        if bps != 8 {
            return Err(VexelError::Custom(format!(
                "Unsupported YCbCr bit depth: {} (only 8-bit supported)",
                bps
            )));
        }

        let tables = YCbCrTables::new(header.ycbcr_coefficients, header.reference_black_white);
        let [h_sub, v_sub] = header.ycbcr_sub_sampling;
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

    pub fn read_cielab(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let spp = header.samples_per_pixel as usize;
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
                let pixels: Vec<u16> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order) as i16;
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order) as i16;

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = a_raw as f32 / 256.0;
                        let b = b_raw as f32 / 256.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
                        let (r, g, b_out) = xyz_to_srgb_f32(x, y, z);
                        [(r * 65535.0).round() as u16, (g * 65535.0).round() as u16, (b_out * 65535.0).round() as u16]
                    })
                    .collect();
                Ok(PixelData::RGB16(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported CIELab bit depth: {}",
                bps
            ))),
        }
    }

    pub fn read_icclab(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let spp = header.samples_per_pixel as usize;
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
                let pixels: Vec<u16> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order);

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = (a_raw as f32 / 257.0) - 128.0;
                        let b = (b_raw as f32 / 257.0) - 128.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D50_WHITE);
                        let (r, g, b_out) = xyz_to_srgb_f32(x, y, z);
                        [(r * 65535.0).round() as u16, (g * 65535.0).round() as u16, (b_out * 65535.0).round() as u16]
                    })
                    .collect();
                Ok(PixelData::RGB16(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported ICCLab bit depth: {}",
                bps
            ))),
        }
    }

    pub fn read_itulab(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let bps = Self::bits_for(header, 0);
        let spp = header.samples_per_pixel as usize;
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
                let pixels: Vec<u16> = data
                    .chunks_exact(bytes_per_pixel)
                    .flat_map(|chunk| {
                        let l_raw = u16_from_bytes(&chunk[0..2], self.byte_order);
                        let a_raw = u16_from_bytes(&chunk[2..4], self.byte_order);
                        let b_raw = u16_from_bytes(&chunk[4..6], self.byte_order);

                        let l = l_raw as f32 * 100.0 / 65535.0;
                        let a = (a_raw as f32 / 257.0) - 128.0;
                        let b = (b_raw as f32 / 257.0) - 128.0;

                        let (x, y, z) = lab_to_xyz(l, a, b, D65_WHITE);
                        let (r, g, b_out) = xyz_to_srgb_f32(x, y, z);
                        [(r * 65535.0).round() as u16, (g * 65535.0).round() as u16, (b_out * 65535.0).round() as u16]
                    })
                    .collect();
                Ok(PixelData::RGB16(pixels))
            }
            _ => Err(VexelError::Custom(format!(
                "Unsupported ITULab bit depth: {}",
                bps
            ))),
        }
    }

    pub fn convert_planar_to_chunky(&self, data: &[u8], header: &TiffHeader) -> Vec<u8> {
        let spp = header.samples_per_pixel as usize;
        let bps = Self::bits_for(header, 0);
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

    pub fn convert_to_pixel_data(&self, data: Vec<u8>, header: &TiffHeader) -> VexelResult<PixelData> {
        let data = if header.planar_configuration == PlanarConfiguration::Planar && header.samples_per_pixel > 1 {
            self.convert_planar_to_chunky(&data, header)
        } else {
            data
        };

        match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero => self.read_grayscale(&data, header, true),
            PhotometricInterpretation::BlackIsZero => self.read_grayscale(&data, header, false),
            PhotometricInterpretation::RGB => self.read_rgb(&data, header),
            PhotometricInterpretation::Palette => self.read_palette(&data, header),
            PhotometricInterpretation::TransparencyMask => Ok(PixelData::L1(data)),
            PhotometricInterpretation::CMYK => self.read_cmyk(&data, header),
            PhotometricInterpretation::YCbCr => self.read_ycbcr(&data, header),
            PhotometricInterpretation::CIELab => self.read_cielab(&data, header),
            PhotometricInterpretation::ICCLab => self.read_icclab(&data, header),
            PhotometricInterpretation::ITULab => self.read_itulab(&data, header),
            PhotometricInterpretation::LogLuv => {
                let pixels: Vec<u8> = data
                    .chunks_exact(4)
                    .flat_map(|chunk| {
                        let p = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        let (r, g, b) = logluv32_to_rgb(p);
                        [r, g, b]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            PhotometricInterpretation::LogL => {
                let pixels: Vec<u8> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let p = u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
                        let le = p & 0x7fff;
                        if le == 0 {
                            return 0u8;
                        }
                        let y = (std::f64::consts::LN_2 / 256.0 * (le as f64 + 0.5)
                            - std::f64::consts::LN_2 * 64.0)
                            .exp();
                        if y <= 0.0 { 0 } else if y >= 1.0 { 255 } else { (256.0 * y.sqrt()) as u8 }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }
        }
    }
}
