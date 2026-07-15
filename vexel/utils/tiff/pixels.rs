//! Quick overview of how we convert raw TIFF sample data into [`PixelData`].
//! Because TIFF is a madness, man.
//!
//! - unsigned integers, 1 bit (grayscale, no alpha) -> L1
//! - integers, 2..=8 bits   -> u8,  full range: v * 255 / (2^bits - 1)
//! - integers, 9..=16 bits  -> u16, full range: v * 65535 / (2^bits - 1)
//! - integers, 17..=32 bits -> f32, normalized: v / (2^bits - 1)
//! - integers, 33..=64 bits -> f64, normalized: v / (2^bits - 1)
//! - floats: 16/24/32 bits -> f32, 64 bits -> f64, values kept as-is.
//! - other widths are decoded as IEEE-like floats
//!
//! Signed integers are first scaled into the unsigned range by flipping the
//! sign bit (v + 2^(bits-1)), so the minimum value maps to 0 at every depth.
//! Palette indices are the only samples that are never rescaled.
//!
//! Channels may declare different bit depths and sample formats, so each channel
//! is read at its own width, and the widest channel decides the output type.

use super::color::{
    cmyk_to_rgb_f32, float24_to_f32, float_bits_to_f64, half_to_f32, lab_to_xyz, logluv32_to_rgb, u16_from_bytes,
    xyz_to_srgb_f32, ycbcr_to_rgb_f32, YCbCrTables, D50_WHITE, D65_WHITE,
};
use super::types::{
    Compression, ExtraSampleType, PhotometricInterpretation, PlanarConfiguration, SampleFormat, TiffHeader,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::types::ByteOrder;
use crate::{log_warn, PixelData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OutClass {
    U8,
    U16,
    F32,
    F64,
}

fn sample_class(bps: u16, fmt: SampleFormat) -> OutClass {
    match fmt {
        SampleFormat::Float => {
            if bps > 32 {
                OutClass::F64
            } else {
                OutClass::F32
            }
        }
        _ => match bps {
            0..=8 => OutClass::U8,
            9..=16 => OutClass::U16,
            17..=32 => OutClass::F32,
            _ => OutClass::F64,
        },
    }
}

pub struct PixelReader {
    pub byte_order: ByteOrder,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
struct Channel {
    bps: u16,
    fmt: SampleFormat,
    bit_offset: usize,
}

struct PixelSource<'a> {
    data: &'a [u8],
    width: usize,
    height: usize,
    channels: Vec<Channel>,
    pixel_bits: usize,
    byte_order: ByteOrder,
    row_bytes: usize,
}

impl<'a> PixelSource<'a> {
    fn new(data: &'a [u8], header: &TiffHeader, width: u32, height: u32, byte_order: ByteOrder) -> Self {
        let spp = header.samples_per_pixel.max(1) as usize;

        let mut channels = Vec::with_capacity(spp);
        let mut bit_offset = 0usize;
        for ch in 0..spp {
            let mut bps = PixelReader::bits_for(header, ch);
            if bps == 0 || bps > 64 {
                log_warn!("Invalid bits per sample {} for channel {}, assuming 8", bps, ch);
                bps = 8;
            }
            channels.push(Channel { bps, fmt: PixelReader::sample_format_for(header, ch), bit_offset });
            bit_offset += bps as usize;
        }

        Self::with_channels(data, width as usize, height as usize, channels, byte_order)
    }

    fn uniform(
        data: &'a [u8],
        width: usize,
        height: usize,
        spp: usize,
        bps: u16,
        fmt: SampleFormat,
        byte_order: ByteOrder,
    ) -> Self {
        let bps = if bps == 0 || bps > 64 { 8 } else { bps };
        let channels = (0..spp).map(|ch| Channel { bps, fmt, bit_offset: ch * bps as usize }).collect();
        Self::with_channels(data, width, height, channels, byte_order)
    }

    fn with_channels(
        data: &'a [u8],
        width: usize,
        height: usize,
        channels: Vec<Channel>,
        byte_order: ByteOrder,
    ) -> Self {
        let pixel_bits: usize = channels.iter().map(|c| c.bps as usize).sum();
        let row_bytes = (width * pixel_bits).div_ceil(8);

        PixelSource { data, width, height, channels, pixel_bits, byte_order, row_bytes }
    }

    fn channel(&self, ch: usize) -> Channel {
        self.channels.get(ch).copied().unwrap_or(Channel { bps: 8, fmt: SampleFormat::UnsignedInt, bit_offset: 0 })
    }

    fn out_class(&self, channels: &[usize]) -> OutClass {
        channels
            .iter()
            .map(|&ch| {
                let c = self.channel(ch);
                sample_class(c.bps, c.fmt)
            })
            .max()
            .unwrap_or(OutClass::U8)
    }

    fn row(&self, y: usize) -> &'a [u8] {
        let start = y * self.row_bytes;
        let end = (start + self.row_bytes).min(self.data.len());
        self.data.get(start..end).unwrap_or(&[])
    }

    fn raw(&self, row: &[u8], px: usize, ch: usize) -> u64 {
        let c = self.channel(ch);
        let bit_pos = px * self.pixel_bits + c.bit_offset;

        if c.bps.is_multiple_of(8) && bit_pos.is_multiple_of(8) {
            let n = c.bps as usize / 8;
            let start = bit_pos / 8;
            let mut value = 0u64;
            match self.byte_order {
                ByteOrder::BigEndian => {
                    for i in 0..n {
                        value = (value << 8) | row.get(start + i).copied().unwrap_or(0) as u64;
                    }
                }
                ByteOrder::LittleEndian => {
                    for i in (0..n).rev() {
                        value = (value << 8) | row.get(start + i).copied().unwrap_or(0) as u64;
                    }
                }
            }
            value
        } else {
            read_msb_bits64(row, bit_pos, c.bps)
        }
    }

    fn max_value(&self, ch: usize) -> u64 {
        let bps = self.channel(ch).bps;
        if bps >= 64 {
            u64::MAX
        } else {
            (1u64 << bps) - 1
        }
    }

    fn biased(&self, ch: usize, raw: u64) -> u64 {
        let c = self.channel(ch);
        if c.fmt == SampleFormat::SignedInt {
            (raw ^ (1u64 << (c.bps - 1))) & self.max_value(ch)
        } else {
            raw & self.max_value(ch)
        }
    }

    fn to_u8(&self, ch: usize, raw: u64) -> u8 {
        let max = self.max_value(ch);
        ((self.biased(ch, raw) * 255 + max / 2) / max) as u8
    }

    fn to_u16(&self, ch: usize, raw: u64) -> u16 {
        let max = self.max_value(ch);
        ((self.biased(ch, raw).min(max) as u128 * 65535 + max as u128 / 2) / max as u128) as u16
    }

    fn to_f32(&self, ch: usize, raw: u64) -> f32 {
        let c = self.channel(ch);
        match c.fmt {
            SampleFormat::Float => match c.bps {
                16 => half_to_f32(raw as u16),
                24 => float24_to_f32(&[raw as u8, (raw >> 8) as u8, (raw >> 16) as u8], ByteOrder::LittleEndian),
                32 => f32::from_bits(raw as u32),
                64 => f64::from_bits(raw) as f32,
                bps => float_bits_to_f64(raw, bps) as f32,
            },
            _ => (self.biased(ch, raw) as f64 / self.max_value(ch) as f64) as f32,
        }
    }

    fn to_f64(&self, ch: usize, raw: u64) -> f64 {
        let c = self.channel(ch);
        match c.fmt {
            SampleFormat::Float => match c.bps {
                16 => half_to_f32(raw as u16) as f64,
                24 => float24_to_f32(&[raw as u8, (raw >> 8) as u8, (raw >> 16) as u8], ByteOrder::LittleEndian) as f64,
                32 => f32::from_bits(raw as u32) as f64,
                64 => f64::from_bits(raw),
                bps => float_bits_to_f64(raw, bps),
            },
            _ => self.biased(ch, raw) as f64 / self.max_value(ch) as f64,
        }
    }

    fn gather<T>(&self, channels: &[usize], mut conv: impl FnMut(usize, u64) -> T) -> Vec<T> {
        let mut out = Vec::with_capacity(self.width * self.height * channels.len());
        for y in 0..self.height {
            let row = self.row(y);
            for px in 0..self.width {
                for &ch in channels {
                    out.push(conv(ch, self.raw(row, px, ch)));
                }
            }
        }
        out
    }
}

impl PixelReader {
    pub fn sample_format_for(header: &TiffHeader, channel: usize) -> SampleFormat {
        header
            .sample_format
            .get(channel)
            .or_else(|| header.sample_format.first())
            .copied()
            .unwrap_or(SampleFormat::UnsignedInt)
    }

    pub fn bits_for(header: &TiffHeader, channel: usize) -> u16 {
        header.bits_per_sample.get(channel).or_else(|| header.bits_per_sample.first()).copied().unwrap_or(8)
    }

    fn alpha_channel(header: &TiffHeader, color_channels: usize) -> Option<usize> {
        let spp = header.samples_per_pixel.max(1) as usize;

        if let Some(pos) = header.extra_samples.iter().position(|&s| {
            s == ExtraSampleType::AssociatedAlpha as u16 || s == ExtraSampleType::UnassociatedAlpha as u16
        }) {
            let ch = color_channels + pos;
            return if ch < spp { Some(ch) } else { None };
        }

        if header.extra_samples.is_empty() && spp == color_channels + 1 {
            log_warn!(
                "Image has {} samples per pixel but no ExtraSamples tag, assuming the extra sample is alpha",
                spp
            );
            return Some(color_channels);
        }

        None
    }

    pub fn read_grayscale(&self, data: &[u8], header: &TiffHeader, invert: bool) -> VexelResult<PixelData> {
        let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
        let alpha = Self::alpha_channel(header, 1);
        let c0 = src.channel(0);

        if c0.bps == 1 && c0.fmt != SampleFormat::Float && alpha.is_none() {
            let pixels = src.gather(&[0], |_, raw| {
                let bit = raw as u8 & 1;
                if invert {
                    1 - bit
                } else {
                    bit
                }
            });
            return Ok(PixelData::L1(pixels));
        }

        let channels: Vec<usize> = match alpha {
            Some(a) => vec![0, a],
            None => vec![0],
        };

        match (src.out_class(&channels), alpha) {
            (OutClass::U8, None) => Ok(PixelData::L8(src.gather(&channels, |ch, raw| {
                let v = src.to_u8(ch, raw);
                if invert {
                    255 - v
                } else {
                    v
                }
            }))),
            (OutClass::U8, Some(_)) => Ok(PixelData::LA8(src.gather(&channels, |ch, raw| {
                let v = src.to_u8(ch, raw);
                if invert && ch == 0 {
                    255 - v
                } else {
                    v
                }
            }))),
            (OutClass::U16, None) => Ok(PixelData::L16(src.gather(&channels, |ch, raw| {
                let v = src.to_u16(ch, raw);
                if invert {
                    u16::MAX - v
                } else {
                    v
                }
            }))),
            (OutClass::U16, Some(_)) => Ok(PixelData::LA16(src.gather(&channels, |ch, raw| {
                let v = src.to_u16(ch, raw);
                if invert && ch == 0 {
                    u16::MAX - v
                } else {
                    v
                }
            }))),
            (OutClass::F32, None) => Ok(PixelData::L32F(src.gather(&channels, |ch, raw| {
                let v = src.to_f32(ch, raw);
                if invert {
                    1.0 - v
                } else {
                    v
                }
            }))),
            (OutClass::F32, Some(_)) => Ok(PixelData::LA32F(src.gather(&channels, |ch, raw| {
                let v = src.to_f32(ch, raw);
                if invert && ch == 0 {
                    1.0 - v
                } else {
                    v
                }
            }))),
            (OutClass::F64, None) => Ok(PixelData::L64F(src.gather(&channels, |ch, raw| {
                let v = src.to_f64(ch, raw);
                if invert {
                    1.0 - v
                } else {
                    v
                }
            }))),
            (OutClass::F64, Some(_)) => Ok(PixelData::LA64F(src.gather(&channels, |ch, raw| {
                let v = src.to_f64(ch, raw);
                if invert && ch == 0 {
                    1.0 - v
                } else {
                    v
                }
            }))),
        }
    }

    pub fn read_rgb(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        if (header.samples_per_pixel.max(1) as usize) < 3 {
            log_warn!("RGB image with only {} samples per pixel, reading as grayscale", header.samples_per_pixel);
            return self.read_grayscale(data, header, false);
        }

        let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
        let alpha = Self::alpha_channel(header, 3);
        let channels: Vec<usize> = match alpha {
            Some(a) => vec![0, 1, 2, a],
            None => vec![0, 1, 2],
        };

        match (src.out_class(&channels), alpha.is_some()) {
            (OutClass::U8, false) => Ok(PixelData::RGB8(src.gather(&channels, |ch, raw| src.to_u8(ch, raw)))),
            (OutClass::U8, true) => Ok(PixelData::RGBA8(src.gather(&channels, |ch, raw| src.to_u8(ch, raw)))),
            (OutClass::U16, false) => Ok(PixelData::RGB16(src.gather(&channels, |ch, raw| src.to_u16(ch, raw)))),
            (OutClass::U16, true) => Ok(PixelData::RGBA16(src.gather(&channels, |ch, raw| src.to_u16(ch, raw)))),
            (OutClass::F32, false) => Ok(PixelData::RGB32F(src.gather(&channels, |ch, raw| src.to_f32(ch, raw)))),
            (OutClass::F32, true) => Ok(PixelData::RGBA32F(src.gather(&channels, |ch, raw| src.to_f32(ch, raw)))),
            (OutClass::F64, false) => Ok(PixelData::RGB64F(src.gather(&channels, |ch, raw| src.to_f64(ch, raw)))),
            (OutClass::F64, true) => Ok(PixelData::RGBA64F(src.gather(&channels, |ch, raw| src.to_f64(ch, raw)))),
        }
    }

    pub fn read_palette(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
        let color_map = &header.color_map;

        if color_map.is_empty() {
            return Err(VexelError::Custom("Missing ColorMap for Palette image".to_string()));
        }

        let index_bits = src.channel(0).bps;
        let n_colors = if index_bits <= 16 { 1usize << index_bits } else { (color_map.len() / 3).max(1) };

        let is_16bit = color_map.iter().take(n_colors * 3).any(|&v| v >= 256);
        if !is_16bit {
            log_warn!("Palette color map has only 8-bit values, treating them as 8-bit");
        }

        let alpha = Self::alpha_channel(header, 1);
        let index_of = |raw: u64| raw.min(n_colors as u64 - 1) as usize;
        let lookup = |idx: usize, plane: usize| color_map.get(idx + plane * n_colors).copied().unwrap_or(0);

        if is_16bit {
            let mut pixels: Vec<u16> = Vec::with_capacity(src.width * src.height * if alpha.is_some() { 4 } else { 3 });
            for y in 0..src.height {
                let row = src.row(y);
                for px in 0..src.width {
                    let idx = index_of(src.raw(row, px, 0));
                    pixels.push(lookup(idx, 0));
                    pixels.push(lookup(idx, 1));
                    pixels.push(lookup(idx, 2));
                    if let Some(a) = alpha {
                        pixels.push(src.to_u16(a, src.raw(row, px, a)));
                    }
                }
            }
            return Ok(if alpha.is_some() { PixelData::RGBA16(pixels) } else { PixelData::RGB16(pixels) });
        }

        let mut pixels: Vec<u8> = Vec::with_capacity(src.width * src.height * if alpha.is_some() { 4 } else { 3 });
        for y in 0..src.height {
            let row = src.row(y);
            for px in 0..src.width {
                let idx = index_of(src.raw(row, px, 0));
                pixels.push(lookup(idx, 0) as u8);
                pixels.push(lookup(idx, 1) as u8);
                pixels.push(lookup(idx, 2) as u8);
                if let Some(a) = alpha {
                    pixels.push(src.to_u8(a, src.raw(row, px, a)));
                }
            }
        }
        Ok(if alpha.is_some() { PixelData::RGBA8(pixels) } else { PixelData::RGB8(pixels) })
    }

    pub fn read_cmyk(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        if (header.samples_per_pixel.max(1) as usize) < 4 {
            log_warn!("CMYK image with only {} samples per pixel, reading as grayscale", header.samples_per_pixel);
            return self.read_grayscale(data, header, false);
        }

        let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
        let alpha = Self::alpha_channel(header, 4);
        let out_channels = if alpha.is_some() { 4 } else { 3 };
        let class_channels: Vec<usize> = match alpha {
            Some(a) => vec![0, 1, 2, 3, a],
            None => vec![0, 1, 2, 3],
        };

        macro_rules! collect_cmyk {
            ($ty:ty, $conv:expr) => {{
                let conv = $conv;
                let mut pixels: Vec<$ty> = Vec::with_capacity(src.width * src.height * out_channels);
                for y in 0..src.height {
                    let row = src.row(y);
                    for px in 0..src.width {
                        let c = src.to_f32(0, src.raw(row, px, 0));
                        let m = src.to_f32(1, src.raw(row, px, 1));
                        let ye = src.to_f32(2, src.raw(row, px, 2));
                        let k = src.to_f32(3, src.raw(row, px, 3));
                        let (r, g, b) = cmyk_to_rgb_f32(c, m, ye, k);
                        pixels.push(conv(r));
                        pixels.push(conv(g));
                        pixels.push(conv(b));
                        if let Some(a) = alpha {
                            pixels.push(conv(src.to_f32(a, src.raw(row, px, a))));
                        }
                    }
                }
                pixels
            }};
        }

        match (src.out_class(&class_channels), alpha.is_some()) {
            (OutClass::U8, false) => Ok(PixelData::RGB8(collect_cmyk!(u8, |v: f32| (v * 255.0).round() as u8))),
            (OutClass::U8, true) => Ok(PixelData::RGBA8(collect_cmyk!(u8, |v: f32| (v * 255.0).round() as u8))),
            (OutClass::U16, false) => Ok(PixelData::RGB16(collect_cmyk!(u16, |v: f32| (v * 65535.0).round() as u16))),
            (OutClass::U16, true) => Ok(PixelData::RGBA16(collect_cmyk!(u16, |v: f32| (v * 65535.0).round() as u16))),
            (OutClass::F32, false) => Ok(PixelData::RGB32F(collect_cmyk!(f32, |v: f32| v))),
            (OutClass::F32, true) => Ok(PixelData::RGBA32F(collect_cmyk!(f32, |v: f32| v))),
            (OutClass::F64, false) => Ok(PixelData::RGB64F(collect_cmyk!(f64, |v: f32| v as f64))),
            (OutClass::F64, true) => Ok(PixelData::RGBA64F(collect_cmyk!(f64, |v: f32| v as f64))),
        }
    }

    pub fn read_ycbcr(&self, data: &[u8], header: &TiffHeader) -> VexelResult<PixelData> {
        if (header.samples_per_pixel.max(1) as usize) < 3 {
            log_warn!("YCbCr image with only {} samples per pixel, reading as grayscale", header.samples_per_pixel);
            return self.read_grayscale(data, header, false);
        }

        let bps = Self::bits_for(header, 0);
        let fmt = Self::sample_format_for(header, 0);

        let [h_sub, v_sub] = header.ycbcr_sub_sampling;
        let h_sub = (h_sub.max(1) as usize).min(4);
        let v_sub = (v_sub.max(1) as usize).min(4);
        let is_planar = header.planar_configuration == PlanarConfiguration::Planar && header.samples_per_pixel > 1;

        if bps != 8 || fmt == SampleFormat::Float {
            return self.read_ycbcr_deep(data, header, h_sub, v_sub, is_planar);
        }

        let tables = YCbCrTables::new(header.ycbcr_coefficients, header.reference_black_white);

        let width = self.width as usize;
        let height = self.height as usize;

        if is_planar {
            return Ok(self.read_ycbcr_planar(data, &tables, h_sub, v_sub));
        }

        if h_sub == 1 && v_sub == 1 {
            let spp = header.samples_per_pixel.max(1) as usize;
            let pixels: Vec<u8> = data
                .chunks_exact(spp)
                .flat_map(|chunk| {
                    let (r, g, b) = tables.convert(chunk[0], chunk[1], chunk[2]);
                    [r, g, b]
                })
                .collect();
            return Ok(PixelData::RGB8(pixels));
        }

        let luma_per_block = h_sub * v_sub;
        let bytes_per_block = luma_per_block + 2;

        let blocks_x = width.div_ceil(h_sub);
        let blocks_y = height.div_ceil(v_sub);

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

                for row in 0..v_sub {
                    for col in 0..h_sub {
                        let px = bx * h_sub + col;
                        let py = by * v_sub + row;
                        if px >= width || py >= height {
                            continue;
                        }
                        let (r, g, b) = tables.convert(block[row * h_sub + col], cb, cr);
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

    fn read_ycbcr_deep(
        &self,
        data: &[u8],
        header: &TiffHeader,
        h_sub: usize,
        v_sub: usize,
        is_planar: bool,
    ) -> VexelResult<PixelData> {
        let width = self.width as usize;
        let height = self.height as usize;
        let bps = Self::bits_for(header, 0).clamp(1, 64);
        let fmt = Self::sample_format_for(header, 0);
        let luma = header.ycbcr_coefficients;

        let code_max = (2f64.powi(bps as i32) - 1.0) as f32;
        let rbw = header.reference_black_white;
        let rbw_max = rbw.iter().fold(0f32, |m, &v| m.max(v));
        let ref_scale = if (bps > 8 && rbw_max <= 255.0) || (bps < 8 && rbw_max > code_max) { 255.0 } else { code_max };

        let (yb, yw) = (rbw[0] / ref_scale, rbw[1] / ref_scale);
        let (cb_b, cb_w) = (rbw[2] / ref_scale, rbw[3] / ref_scale);
        let (cr_b, cr_w) = (rbw[4] / ref_scale, rbw[5] / ref_scale);

        let expand_y = |v: f32| if (yw - yb).abs() < f32::EPSILON { v } else { (v - yb) / (yw - yb) };
        let expand_c =
            |v: f32, b: f32, w: f32| if (w - b).abs() < f32::EPSILON { v - 0.5 } else { (v - b) * 0.5 / (w - b) };
        let convert = |ny: f32, ncb: f32, ncr: f32| {
            ycbcr_to_rgb_f32(expand_y(ny), expand_c(ncb, cb_b, cb_w), expand_c(ncr, cr_b, cr_w), luma)
        };

        let mut rgb = vec![0f32; width * height * 3];

        if is_planar {
            let chroma_w = width.div_ceil(h_sub);
            let chroma_h = height.div_ceil(v_sub);
            let y_bytes = (width * bps as usize).div_ceil(8) * height;
            let c_bytes = (chroma_w * bps as usize).div_ceil(8) * chroma_h;

            let plane = |start: usize| data.get(start.min(data.len())..).unwrap_or(&[]);
            let ysrc = PixelSource::uniform(plane(0), width, height, 1, bps, fmt, self.byte_order);
            let cbsrc = PixelSource::uniform(plane(y_bytes), chroma_w, chroma_h, 1, bps, fmt, self.byte_order);
            let crsrc =
                PixelSource::uniform(plane(y_bytes + c_bytes), chroma_w, chroma_h, 1, bps, fmt, self.byte_order);

            for py in 0..height {
                let y_row = ysrc.row(py);
                let cb_row = cbsrc.row(py / v_sub);
                let cr_row = crsrc.row(py / v_sub);
                for px in 0..width {
                    let ny = ysrc.to_f32(0, ysrc.raw(y_row, px, 0));
                    let ncb = cbsrc.to_f32(0, cbsrc.raw(cb_row, px / h_sub, 0));
                    let ncr = crsrc.to_f32(0, crsrc.raw(cr_row, px / h_sub, 0));
                    let (r, g, b) = convert(ny, ncb, ncr);
                    let idx = (py * width + px) * 3;
                    rgb[idx] = r;
                    rgb[idx + 1] = g;
                    rgb[idx + 2] = b;
                }
            }
        } else if h_sub == 1 && v_sub == 1 {
            let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
            for py in 0..height {
                let row = src.row(py);
                for px in 0..width {
                    let ny = src.to_f32(0, src.raw(row, px, 0));
                    let ncb = src.to_f32(1, src.raw(row, px, 1));
                    let ncr = src.to_f32(2, src.raw(row, px, 2));
                    let (r, g, b) = convert(ny, ncb, ncr);
                    let idx = (py * width + px) * 3;
                    rgb[idx] = r;
                    rgb[idx + 1] = g;
                    rgb[idx + 2] = b;
                }
            }
        } else {
            let luma_per_block = h_sub * v_sub;
            let blocks_x = width.div_ceil(h_sub);
            let blocks_y = height.div_ceil(v_sub);
            let src = PixelSource::uniform(data, blocks_x, blocks_y, luma_per_block + 2, bps, fmt, self.byte_order);

            for by in 0..blocks_y {
                let row = src.row(by);
                for bx in 0..blocks_x {
                    let ncb = src.to_f32(luma_per_block, src.raw(row, bx, luma_per_block));
                    let ncr = src.to_f32(luma_per_block + 1, src.raw(row, bx, luma_per_block + 1));
                    for sub_y in 0..v_sub {
                        for sub_x in 0..h_sub {
                            let px = bx * h_sub + sub_x;
                            let py = by * v_sub + sub_y;
                            if px >= width || py >= height {
                                continue;
                            }
                            let ch = sub_y * h_sub + sub_x;
                            let ny = src.to_f32(ch, src.raw(row, bx, ch));
                            let (r, g, b) = convert(ny, ncb, ncr);
                            let idx = (py * width + px) * 3;
                            rgb[idx] = r;
                            rgb[idx + 1] = g;
                            rgb[idx + 2] = b;
                        }
                    }
                }
            }
        }

        Ok(match sample_class(bps, fmt) {
            OutClass::U8 => PixelData::RGB8(rgb.iter().map(|&v| (v * 255.0).round() as u8).collect()),
            OutClass::U16 => PixelData::RGB16(rgb.iter().map(|&v| (v * 65535.0).round() as u16).collect()),
            _ => PixelData::RGB32F(rgb),
        })
    }

    fn read_ycbcr_planar(&self, data: &[u8], tables: &YCbCrTables, h_sub: usize, v_sub: usize) -> PixelData {
        let width = self.width as usize;
        let height = self.height as usize;
        let chroma_w = width.div_ceil(h_sub);
        let chroma_h = height.div_ceil(v_sub);

        let y_size = width * height;
        let chroma_size = chroma_w * chroma_h;

        let sample = |offset: usize| data.get(offset).copied().unwrap_or(128);

        let mut pixels = Vec::with_capacity(width * height * 3);
        for py in 0..height {
            for px in 0..width {
                let y = data.get(py * width + px).copied().unwrap_or(0);
                let chroma_idx = (py / v_sub) * chroma_w + px / h_sub;
                let cb = sample(y_size + chroma_idx);
                let cr = sample(y_size + chroma_size + chroma_idx);
                let (r, g, b) = tables.convert(y, cb, cr);
                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
            }
        }

        PixelData::RGB8(pixels)
    }

    fn read_lab(&self, data: &[u8], header: &TiffHeader, white: [f32; 3], signed_ab: bool) -> VexelResult<PixelData> {
        if (header.samples_per_pixel.max(1) as usize) < 3 {
            log_warn!("Lab image with only {} samples per pixel, reading as grayscale", header.samples_per_pixel);
            return self.read_grayscale(data, header, false);
        }

        let src = PixelSource::new(data, header, self.width, self.height, self.byte_order);
        let l_max = src.max_value(0) as f32;

        let ab_scale = |bps: u16| if bps >= 8 { (1u64 << (bps - 8)) as f32 } else { 1.0 / (1u64 << (8 - bps)) as f32 };

        let lab_of = |src: &PixelSource, row: &[u8], px: usize| -> (f32, f32, f32) {
            if src.channel(0).fmt == SampleFormat::Float {
                (
                    src.to_f32(0, src.raw(row, px, 0)),
                    src.to_f32(1, src.raw(row, px, 1)),
                    src.to_f32(2, src.raw(row, px, 2)),
                )
            } else {
                let l = src.raw(row, px, 0) as f32 * 100.0 / l_max;
                let a_raw = src.raw(row, px, 1);
                let b_raw = src.raw(row, px, 2);
                let (a_bps, b_bps) = (src.channel(1).bps, src.channel(2).bps);
                if signed_ab {
                    (
                        l,
                        sign_extend(a_raw, a_bps) as f32 / ab_scale(a_bps),
                        sign_extend(b_raw, b_bps) as f32 / ab_scale(b_bps),
                    )
                } else {
                    (
                        l,
                        a_raw as f32 * 255.0 / src.max_value(1) as f32 - 128.0,
                        b_raw as f32 * 255.0 / src.max_value(2) as f32 - 128.0,
                    )
                }
            }
        };

        let to_srgb = |l: f32, a: f32, b: f32| -> (f32, f32, f32) {
            let (x, y, z) = lab_to_xyz(l, a, b, white);
            xyz_to_srgb_f32(x, y, z)
        };

        match src.out_class(&[0, 1, 2]) {
            OutClass::U8 => {
                let mut pixels = Vec::with_capacity(src.width * src.height * 3);
                for y in 0..src.height {
                    let row = src.row(y);
                    for px in 0..src.width {
                        let (l, a, b) = lab_of(&src, row, px);
                        let (r, g, b) = to_srgb(l, a, b);
                        pixels.push((r * 255.0).round() as u8);
                        pixels.push((g * 255.0).round() as u8);
                        pixels.push((b * 255.0).round() as u8);
                    }
                }
                Ok(PixelData::RGB8(pixels))
            }
            OutClass::U16 => {
                let mut pixels = Vec::with_capacity(src.width * src.height * 3);
                for y in 0..src.height {
                    let row = src.row(y);
                    for px in 0..src.width {
                        let (l, a, b) = lab_of(&src, row, px);
                        let (r, g, b) = to_srgb(l, a, b);
                        pixels.push((r * 65535.0).round() as u16);
                        pixels.push((g * 65535.0).round() as u16);
                        pixels.push((b * 65535.0).round() as u16);
                    }
                }
                Ok(PixelData::RGB16(pixels))
            }
            _ => {
                let mut pixels = Vec::with_capacity(src.width * src.height * 3);
                for y in 0..src.height {
                    let row = src.row(y);
                    for px in 0..src.width {
                        let (l, a, b) = lab_of(&src, row, px);
                        let (r, g, b) = to_srgb(l, a, b);
                        pixels.push(r);
                        pixels.push(g);
                        pixels.push(b);
                    }
                }
                Ok(PixelData::RGB32F(pixels))
            }
        }
    }

    pub fn convert_planar_to_chunky(&self, data: &[u8], header: &TiffHeader) -> Vec<u8> {
        let spp = header.samples_per_pixel.max(1) as usize;
        let bps = Self::bits_for(header, 0);
        let width = self.width as usize;
        let height = self.height as usize;

        if bps.is_multiple_of(8) {
            let bytes_per_sample = (bps as usize) / 8;
            let pixels_total = width * height;
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

            return chunky;
        }

        let plane_row_bytes = (width * bps as usize).div_ceil(8);
        let plane_size = plane_row_bytes * height;
        let chunky_row_bytes = (width * spp * bps as usize).div_ceil(8);

        let mut chunky = vec![0u8; chunky_row_bytes * height];

        for sample in 0..spp {
            let plane_start = sample * plane_size;
            for row in 0..height {
                let src_row_start = plane_start + row * plane_row_bytes;
                let src_row = data.get(src_row_start..src_row_start + plane_row_bytes).unwrap_or(&[]);
                let dst_row_start = row * chunky_row_bytes;
                let dst_row = &mut chunky[dst_row_start..dst_row_start + chunky_row_bytes];

                for px in 0..width {
                    let value = read_msb_bits64(src_row, px * bps as usize, bps);
                    let dst_bit_pos = (px * spp + sample) * bps as usize;
                    write_msb_bits(dst_row, dst_bit_pos, bps, value);
                }
            }
        }

        chunky
    }

    pub fn convert_to_pixel_data(&self, data: Vec<u8>, header: &TiffHeader) -> VexelResult<PixelData> {
        let is_planar = header.planar_configuration == PlanarConfiguration::Planar && header.samples_per_pixel > 1;
        let non_uniform = header.bits_per_sample.len() > 1 && header.bits_per_sample.windows(2).any(|w| w[0] != w[1]);

        let fixed_header;
        let header = if is_planar && non_uniform {
            log_warn!(
                "Per-channel bit depths {:?} are not supported for planar images, using {} bits for all channels",
                header.bits_per_sample,
                Self::bits_for(header, 0)
            );
            fixed_header = TiffHeader {
                bits_per_sample: vec![Self::bits_for(header, 0); header.samples_per_pixel.max(1) as usize],
                ..header.clone()
            };
            &fixed_header
        } else {
            header
        };

        let data = if is_planar && header.photometric_interpretation != PhotometricInterpretation::YCbCr {
            self.convert_planar_to_chunky(&data, header)
        } else {
            data
        };

        match header.photometric_interpretation {
            PhotometricInterpretation::WhiteIsZero => self.read_grayscale(&data, header, true),
            PhotometricInterpretation::BlackIsZero => self.read_grayscale(&data, header, false),
            PhotometricInterpretation::RGB => self.read_rgb(&data, header),
            PhotometricInterpretation::Palette => self.read_palette(&data, header),
            PhotometricInterpretation::TransparencyMask => self.read_grayscale(&data, header, false),
            PhotometricInterpretation::CMYK => self.read_cmyk(&data, header),
            PhotometricInterpretation::YCbCr => self.read_ycbcr(&data, header),
            PhotometricInterpretation::CIELab => self.read_lab(&data, header, D50_WHITE, true),
            PhotometricInterpretation::ICCLab => self.read_lab(&data, header, D50_WHITE, false),
            PhotometricInterpretation::ITULab => self.read_lab(&data, header, D65_WHITE, false),
            PhotometricInterpretation::LogLuv => {
                let order = if matches!(header.compression, Compression::SGILog | Compression::SGILog24) {
                    ByteOrder::BigEndian
                } else {
                    self.byte_order
                };
                let pixels: Vec<u8> = data
                    .chunks_exact(4)
                    .flat_map(|chunk| {
                        let bits = match order {
                            ByteOrder::BigEndian => u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                            ByteOrder::LittleEndian => u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                        };
                        let (r, g, b) = logluv32_to_rgb(bits);
                        [r, g, b]
                    })
                    .collect();
                Ok(PixelData::RGB8(pixels))
            }
            PhotometricInterpretation::LogL => {
                let order =
                    if header.compression == Compression::SGILog { ByteOrder::BigEndian } else { self.byte_order };
                let pixels: Vec<u8> = data
                    .chunks_exact(2)
                    .map(|chunk| {
                        let p = u16_from_bytes(chunk, order) as u32;
                        let le = p & 0x7fff;
                        if le == 0 {
                            return 0u8;
                        }
                        let y =
                            (std::f64::consts::LN_2 / 256.0 * (le as f64 + 0.5) - std::f64::consts::LN_2 * 64.0).exp();
                        if y <= 0.0 {
                            0
                        } else if y >= 1.0 {
                            255
                        } else {
                            (256.0 * y.sqrt()) as u8
                        }
                    })
                    .collect();
                Ok(PixelData::L8(pixels))
            }
        }
    }
}

fn sign_extend(raw: u64, bps: u16) -> i64 {
    if bps >= 64 {
        return raw as i64;
    }
    let shift = 64 - bps as u32;
    ((raw << shift) as i64) >> shift
}

pub(crate) fn read_msb_bits64(row: &[u8], bit_pos: usize, bps: u16) -> u64 {
    let mut value: u64 = 0;
    for i in 0..bps as usize {
        let bit_idx = bit_pos + i;
        let byte = row.get(bit_idx / 8).copied().unwrap_or(0);
        let bit = (byte >> (7 - (bit_idx % 8))) & 1;
        value = (value << 1) | bit as u64;
    }
    value
}

pub(crate) fn write_msb_bits(row: &mut [u8], bit_pos: usize, bps: u16, value: u64) {
    for i in 0..bps as usize {
        let bit_idx = bit_pos + i;
        let byte_idx = bit_idx / 8;
        if byte_idx >= row.len() {
            break;
        }
        let bit = (value >> (bps as usize - 1 - i)) & 1;
        let shift = 7 - (bit_idx % 8);
        row[byte_idx] = (row[byte_idx] & !(1 << shift)) | ((bit as u8) << shift);
    }
}
