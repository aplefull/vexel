use std::io::{Read, Seek};

use super::mask::BitMask;
use super::types::{LercDataType, LERC1_MAGIC, LERC2_MAGIC};
use super::{lerc1, lerc2};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::PixelData;
use crate::utils::info::LercInfo;
use crate::{log_warn, Image, Limits};

pub struct LercRaster {
    pub width: u32,
    pub height: u32,
    pub n_depth: u32,
    pub bands: usize,
    pub data_type: LercDataType,
    pub values: Vec<f64>,
    pub mask: Option<BitMask>,
    pub max_z_error: f64,
    pub version: i32,
}

impl LercRaster {
    pub fn channels(&self) -> usize {
        self.bands * self.n_depth as usize
    }

    pub fn interleaved(&self) -> Vec<f64> {
        interleave(self)
    }
}

fn band_mask(masks: Vec<BitMask>) -> Option<BitMask> {
    let mask = masks.into_iter().next()?;

    if mask.is_all_valid() {
        None
    } else {
        Some(mask)
    }
}

pub fn decode_blob(data: &[u8]) -> VexelResult<LercRaster> {
    if data.len() >= LERC2_MAGIC.len() && &data[..LERC2_MAGIC.len()] == LERC2_MAGIC {
        let bands = lerc2::decode_bands(data)?;
        let first = &bands[0].header;

        let width = first.n_cols as u32;
        let height = first.n_rows as u32;
        let n_depth = first.n_depth as u32;
        let data_type = first.data_type()?;
        let max_z_error = first.max_z_error;
        let version = first.version;
        let band_count = bands.len();

        let mut values = Vec::with_capacity(width as usize * height as usize * n_depth as usize * band_count);
        let mut masks = Vec::with_capacity(band_count);

        for band in bands {
            values.extend_from_slice(&band.values);
            masks.push(band.mask);
        }

        return Ok(LercRaster {
            width,
            height,
            n_depth,
            bands: band_count,
            data_type,
            values,
            mask: band_mask(masks),
            max_z_error,
            version,
        });
    }

    if data.len() >= LERC1_MAGIC.len() && &data[..LERC1_MAGIC.len()] == LERC1_MAGIC {
        let bands = lerc1::decode_bands(data)?;
        let first = &bands[0].header;

        let width = first.width as u32;
        let height = first.height as u32;
        let max_z_error = first.max_z_error;
        let band_count = bands.len();

        let mut values = Vec::with_capacity(width as usize * height as usize * band_count);
        let mut masks = Vec::with_capacity(band_count);

        for band in bands {
            values.extend_from_slice(&band.values);
            masks.push(band.mask);
        }

        return Ok(LercRaster {
            width,
            height,
            n_depth: 1,
            bands: band_count,
            data_type: LercDataType::Float,
            values,
            mask: band_mask(masks),
            max_z_error,
            version: 1,
        });
    }

    Err(VexelError::Custom("Not a LERC blob".to_string()))
}

fn interleave(raster: &LercRaster) -> Vec<f64> {
    let pixels = raster.width as usize * raster.height as usize;
    let n_depth = raster.n_depth as usize;
    let channels = raster.channels();

    if raster.bands == 1 {
        return raster.values.clone();
    }

    let mut out = vec![0.0; pixels * channels];

    for band in 0..raster.bands {
        let base = band * pixels * n_depth;

        for pixel in 0..pixels {
            for d in 0..n_depth {
                let src = base + pixel * n_depth + d;
                let dst = pixel * channels + band * n_depth + d;
                out[dst] = raster.values.get(src).copied().unwrap_or(0.0);
            }
        }
    }

    out
}

fn normalized_alpha(raster: &LercRaster) -> Option<Vec<bool>> {
    let mask = raster.mask.as_ref()?;
    let pixels = raster.width as usize * raster.height as usize;
    Some((0..pixels).map(|k| mask.is_valid(k)).collect())
}

fn to_pixel_data(raster: &LercRaster) -> PixelData {
    let pixels = raster.width as usize * raster.height as usize;
    let channels = raster.channels();
    let values = interleave(raster);
    let alpha = normalized_alpha(raster);

    let usable = match channels {
        1..=4 => channels,
        other => {
            log_warn!(
                "LERC image has {} channels, which has no direct pixel format, using the first channel as grayscale",
                other
            );
            1
        }
    };

    let mut selected = Vec::with_capacity(pixels * usable);
    for pixel in 0..pixels {
        for c in 0..usable {
            selected.push(values.get(pixel * channels + c).copied().unwrap_or(0.0));
        }
    }

    let with_alpha = matches!((usable, alpha.is_some()), (1, true) | (3, true));

    let final_channels = if with_alpha { usable + 1 } else { usable };

    let dt = raster.data_type;

    match dt {
        LercDataType::Byte | LercDataType::Char => {
            let max = if dt == LercDataType::Char { i8::MAX as f64 } else { u8::MAX as f64 };
            let bias = if dt == LercDataType::Char { 128.0 } else { 0.0 };

            let mut out = Vec::with_capacity(pixels * final_channels);

            for pixel in 0..pixels {
                for c in 0..usable {
                    let v = selected[pixel * usable + c] + bias;
                    out.push(v.clamp(0.0, if bias > 0.0 { 255.0 } else { max }) as u8);
                }

                if with_alpha {
                    let valid = alpha.as_ref().map(|a| a[pixel]).unwrap_or(true);
                    out.push(if valid { 255 } else { 0 });
                }
            }

            match final_channels {
                1 => PixelData::L8(out),
                2 => PixelData::LA8(out),
                3 => PixelData::RGB8(out),
                _ => PixelData::RGBA8(out),
            }
        }
        LercDataType::UShort | LercDataType::Short => {
            let bias = if dt == LercDataType::Short { 32768.0 } else { 0.0 };
            let mut out = Vec::with_capacity(pixels * final_channels);

            for pixel in 0..pixels {
                for c in 0..usable {
                    let v = selected[pixel * usable + c] + bias;
                    out.push(v.clamp(0.0, u16::MAX as f64) as u16);
                }

                if with_alpha {
                    let valid = alpha.as_ref().map(|a| a[pixel]).unwrap_or(true);
                    out.push(if valid { u16::MAX } else { 0 });
                }
            }

            match final_channels {
                1 => PixelData::L16(out),
                2 => PixelData::LA16(out),
                3 => PixelData::RGB16(out),
                _ => PixelData::RGBA16(out),
            }
        }
        LercDataType::Double => {
            let mut out = Vec::with_capacity(pixels * final_channels);

            for pixel in 0..pixels {
                for c in 0..usable {
                    out.push(selected[pixel * usable + c]);
                }

                if with_alpha {
                    let valid = alpha.as_ref().map(|a| a[pixel]).unwrap_or(true);
                    out.push(if valid { 1.0 } else { 0.0 });
                }
            }

            match final_channels {
                1 => PixelData::L64F(out),
                2 => PixelData::LA64F(out),
                3 => PixelData::RGB64F(out),
                _ => PixelData::RGBA64F(out),
            }
        }
        _ => {
            let mut out = Vec::with_capacity(pixels * final_channels);

            for pixel in 0..pixels {
                for c in 0..usable {
                    out.push(selected[pixel * usable + c] as f32);
                }

                if with_alpha {
                    let valid = alpha.as_ref().map(|a| a[pixel]).unwrap_or(true);
                    out.push(if valid { 1.0 } else { 0.0 });
                }
            }

            match final_channels {
                1 => PixelData::L32F(out),
                2 => PixelData::LA32F(out),
                3 => PixelData::RGB32F(out),
                _ => PixelData::RGBA32F(out),
            }
        }
    }
}

pub struct LercDecoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    info: Option<LercInfo>,
}

impl<R: Read + Seek> LercDecoder<R> {
    pub fn new(reader: R) -> Self {
        LercDecoder { reader, limits: Limits::default(), info: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    fn read_blob(&mut self) -> VexelResult<Vec<u8>> {
        let mut data = Vec::new();
        self.reader.read_to_end(&mut data)?;
        Ok(data)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let data = self.read_blob()?;
        let raster = decode_blob(&data)?;

        self.limits.check_dimensions(raster.width, raster.height)?;

        self.info = Some(LercInfo {
            width: raster.width,
            height: raster.height,
            version: raster.version,
            bands: raster.bands as u32,
            depth: raster.n_depth,
            data_type: format!("{:?}", raster.data_type),
            max_z_error: raster.max_z_error,
            has_mask: raster.mask.is_some(),
        });

        let pixels = to_pixel_data(&raster);

        Ok(Image::from_pixels(raster.width, raster.height, pixels))
    }

    pub fn get_info(&mut self) -> LercInfo {
        if let Some(info) = &self.info {
            return info.clone();
        }

        match self.decode() {
            Ok(_) => self.info.clone().unwrap_or_default(),
            Err(_) => LercInfo::default(),
        }
    }
}
