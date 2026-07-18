use crate::decoders::dicom::types::{Photometric, VoiFunction};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::PixelData;

#[derive(Debug, Clone)]
pub struct PixelParams {
    pub rows: u32,
    pub cols: u32,
    pub bits_allocated: u16,
    pub bits_stored: u16,
    pub high_bit: u16,
    pub signed: bool,
    pub samples_per_pixel: u16,
    pub planar: bool,
    pub photometric: Photometric,
}

impl PixelParams {
    pub fn pixels_per_frame(&self) -> usize {
        self.rows as usize * self.cols as usize
    }

    pub fn frame_size_bytes(&self) -> usize {
        let pixels = self.pixels_per_frame();
        if self.bits_allocated == 1 {
            return pixels.div_ceil(8);
        }
        let bytes_per_sample = (self.bits_allocated as usize).div_ceil(8);
        if self.photometric == Photometric::YbrFull422 && self.samples_per_pixel == 3 {
            return pixels * 2 * bytes_per_sample;
        }
        pixels * self.samples_per_pixel as usize * bytes_per_sample
    }
}

#[derive(Debug, Clone)]
pub struct Lut {
    pub first_mapped: i32,
    pub bits: u16,
    pub entries: Vec<u16>,
}

impl Lut {
    pub fn lookup(&self, value: i64) -> u16 {
        if self.entries.is_empty() {
            return 0;
        }
        let index = value - self.first_mapped as i64;
        let index = index.clamp(0, self.entries.len() as i64 - 1) as usize;
        self.entries[index]
    }
}

#[derive(Debug, Clone, Default)]
pub struct MonoTransform {
    pub rescale: Option<(f64, f64)>,
    pub modality_lut: Option<Lut>,
    pub voi_lut: Option<Lut>,
    pub window: Option<(f64, f64)>,
    pub voi_function: Option<VoiFunction>,
    pub invert: bool,
}

impl MonoTransform {
    pub fn identity() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub enum StoredFrame {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

pub fn extract_native_frame(data: &[u8], params: &PixelParams) -> StoredFrame {
    let pixels = params.pixels_per_frame();
    let samples = params.samples_per_pixel.max(1) as usize;

    if params.bits_allocated == 1 {
        let mut out = Vec::with_capacity(pixels);
        for i in 0..pixels {
            let byte = data.get(i / 8).copied().unwrap_or(0);
            out.push((byte >> (i % 8)) & 1);
        }
        return StoredFrame::U8(out);
    }

    let bytes_per_sample = (params.bits_allocated as usize).div_ceil(8);

    if params.photometric == Photometric::YbrFull422 && samples == 3 && bytes_per_sample == 1 {
        return expand_ybr_422(data, params);
    }

    let total_samples = pixels * samples;

    match bytes_per_sample {
        1 => {
            let mut out = vec![0u8; total_samples];
            let count = total_samples.min(data.len());
            out[..count].copy_from_slice(&data[..count]);
            if params.planar && samples > 1 {
                return StoredFrame::U8(planar_to_interleaved(&out, pixels, samples));
            }
            StoredFrame::U8(out)
        }
        2 => {
            let mut out = vec![0u16; total_samples];
            for (i, v) in out.iter_mut().enumerate() {
                let p = i * 2;
                if p + 1 < data.len() {
                    *v = u16::from_le_bytes([data[p], data[p + 1]]);
                }
            }
            if params.planar && samples > 1 {
                return StoredFrame::U16(planar_to_interleaved(&out, pixels, samples));
            }
            StoredFrame::U16(out)
        }
        _ => {
            let mut out = vec![0u32; total_samples];
            for (i, v) in out.iter_mut().enumerate() {
                let p = i * bytes_per_sample;
                let mut value = 0u32;
                for b in 0..bytes_per_sample.min(4) {
                    value |= (data.get(p + b).copied().unwrap_or(0) as u32) << (8 * b);
                }
                *v = value;
            }
            if params.planar && samples > 1 {
                return StoredFrame::U32(planar_to_interleaved(&out, pixels, samples));
            }
            StoredFrame::U32(out)
        }
    }
}

fn planar_to_interleaved<T: Copy + Default>(data: &[T], pixels: usize, samples: usize) -> Vec<T> {
    let mut out = vec![T::default(); pixels * samples];
    for s in 0..samples {
        let plane_start = s * pixels;
        for p in 0..pixels {
            if let Some(&v) = data.get(plane_start + p) {
                out[p * samples + s] = v;
            }
        }
    }
    out
}

fn expand_ybr_422(data: &[u8], params: &PixelParams) -> StoredFrame {
    let pixels = params.pixels_per_frame();
    let mut out = vec![0u8; pixels * 3];

    for pair in 0..pixels / 2 {
        let src = pair * 4;
        let y1 = data.get(src).copied().unwrap_or(0);
        let y2 = data.get(src + 1).copied().unwrap_or(0);
        let cb = data.get(src + 2).copied().unwrap_or(128);
        let cr = data.get(src + 3).copied().unwrap_or(128);

        let dst = pair * 6;
        out[dst] = y1;
        out[dst + 1] = cb;
        out[dst + 2] = cr;
        out[dst + 3] = y2;
        out[dst + 4] = cb;
        out[dst + 5] = cr;
    }

    StoredFrame::U8(out)
}

struct SampleMask {
    shift: u32,
    mask: u32,
    sign_bit: u32,
    sign_extension: u32,
}

impl SampleMask {
    fn new(params: &PixelParams) -> SampleMask {
        let bits_allocated = params.bits_allocated.clamp(1, 32) as u32;
        let mut bits_stored = params.bits_stored.clamp(1, 32) as u32;
        if bits_stored > bits_allocated {
            bits_stored = bits_allocated;
        }
        let mut high_bit = params.high_bit as u32;
        if high_bit < bits_stored - 1 || high_bit >= bits_allocated {
            high_bit = bits_stored - 1;
        }
        let shift = high_bit + 1 - bits_stored;
        let mask = if bits_stored >= 32 { u32::MAX } else { (1u32 << bits_stored) - 1 };
        let sign_bit = if params.signed { 1u32 << (bits_stored - 1) } else { 0 };
        let sign_extension = !mask;

        SampleMask { shift, mask, sign_bit, sign_extension }
    }

    #[inline(always)]
    fn apply(&self, container: u32) -> i64 {
        let v = (container >> self.shift) & self.mask;
        if self.sign_bit != 0 && (v & self.sign_bit) != 0 {
            (v | self.sign_extension) as i32 as i64
        } else {
            v as i64
        }
    }
}

fn apply_modality(value: i64, transform: &MonoTransform) -> f64 {
    if let Some(lut) = &transform.modality_lut {
        return lut.lookup(value) as f64;
    }
    if let Some((slope, intercept)) = transform.rescale {
        return (value as f64 * slope + intercept).trunc();
    }
    value as f64
}

fn stored_abs_range(params: &PixelParams, transform: &MonoTransform) -> (f64, f64) {
    if let Some(lut) = &transform.modality_lut {
        let bits = lut.bits.clamp(1, 16) as u32;
        return (0.0, ((1u64 << bits) - 1) as f64);
    }

    let bits = params.bits_stored.clamp(1, 32) as u32;
    let (raw_min, raw_max) = if params.signed {
        (-((1i64 << (bits - 1)) as f64), ((1i64 << (bits - 1)) - 1) as f64)
    } else {
        (0.0, ((1u64 << bits) - 1) as f64)
    };

    match transform.rescale {
        Some((slope, intercept)) if slope < 0.0 => (raw_max * slope + intercept, raw_min * slope + intercept),
        Some((slope, intercept)) => (raw_min * slope + intercept, raw_max * slope + intercept),
        None => (raw_min, raw_max),
    }
}

enum MapMode {
    WindowLinear { left: f64, right: f64, offset: f64, gradient: f64 },
    WindowLinearExact { center: f64, width: f64 },
    WindowSigmoid { center: f64, width: f64 },
    VoiLut { lut: Lut, gradient: f64 },
    Scale { abs_min: f64, gradient: f64 },
}

struct DisplayMapper {
    low: f64,
    high: f64,
    out_max: f64,
    mode: MapMode,
}

impl DisplayMapper {
    fn new(transform: &MonoTransform, abs_min: f64, abs_max: f64, out_max: f64) -> DisplayMapper {
        let (low, high) = if transform.invert { (out_max, 0.0) } else { (0.0, out_max) };
        let outrange = high - low;
        let function = transform.voi_function.unwrap_or(VoiFunction::Linear);

        let valid_window = transform.window.filter(|&(_, w)| match function {
            VoiFunction::LinearExact => w > 0.0,
            _ => w >= 1.0,
        });

        let mode = if let Some(lut) = transform.voi_lut.as_ref().filter(|l| !l.entries.is_empty()) {
            let bits = lut.bits.clamp(1, 16) as u32;
            let gradient = outrange / (1u64 << bits) as f64;
            MapMode::VoiLut { lut: lut.clone(), gradient }
        } else if let Some((center, width)) = valid_window {
            match function {
                VoiFunction::LinearExact => MapMode::WindowLinearExact { center, width },
                VoiFunction::Sigmoid => MapMode::WindowSigmoid { center, width },
                VoiFunction::Linear => {
                    let w1 = width - 1.0;
                    let left = center - 0.5 - w1 / 2.0;
                    let right = center - 0.5 + w1 / 2.0;
                    let (offset, gradient) = if w1 == 0.0 {
                        (0.0, 0.0)
                    } else {
                        (high - ((center - 0.5) / w1 + 0.5) * outrange, outrange / w1)
                    };
                    MapMode::WindowLinear { left, right, offset, gradient }
                }
            }
        } else {
            let denom = abs_max - abs_min;
            let gradient = if denom > 0.0 { outrange / denom } else { 0.0 };
            MapMode::Scale { abs_min, gradient }
        };

        DisplayMapper { low, high, out_max, mode }
    }

    #[inline]
    fn map(&self, value: f64) -> f64 {
        let y = match &self.mode {
            MapMode::WindowLinear { left, right, offset, gradient } => {
                if value <= *left {
                    self.low
                } else if value > *right {
                    self.high
                } else {
                    offset + value * gradient
                }
            }
            MapMode::WindowLinearExact { center, width } => {
                self.low + ((value - center) / width + 0.5) * (self.high - self.low)
            }
            MapMode::WindowSigmoid { center, width } => {
                self.low + (self.high - self.low) / (1.0 + (-4.0 * (value - center) / width).exp())
            }
            MapMode::VoiLut { lut, gradient } => self.low + lut.lookup(value as i64) as f64 * gradient,
            MapMode::Scale { abs_min, gradient } => self.low + (value - abs_min) * gradient,
        };

        y.clamp(0.0, self.out_max)
    }
}

pub fn mono_frames_to_pixels(
    frames: &[StoredFrame],
    params: &PixelParams,
    transform: &MonoTransform,
) -> Vec<PixelData> {
    let mask = SampleMask::new(params);
    let deep = params.bits_stored > 8;
    let out_max = if deep { 65535.0 } else { 255.0 };

    let (abs_min, abs_max) = stored_abs_range(params, transform);
    let mapper = DisplayMapper::new(transform, abs_min, abs_max, out_max);
    let frac_offset = abs_min - abs_min.trunc();

    let table: Option<Vec<u16>> = if params.bits_allocated <= 16 {
        let size = 1usize << params.bits_allocated.clamp(1, 16);
        let mut table = vec![0u16; size];
        for (container, out) in table.iter_mut().enumerate() {
            let value = apply_modality(mask.apply(container as u32), transform);
            *out = mapper.map(value + frac_offset) as u16;
        }
        Some(table)
    } else {
        None
    };

    frames
        .iter()
        .map(|frame| {
            let map_value = |container: u32| -> u16 {
                match &table {
                    Some(t) => t[(container as usize).min(t.len() - 1)],
                    None => mapper.map(apply_modality(mask.apply(container), transform) + frac_offset) as u16,
                }
            };

            let out16: Vec<u16> = match frame {
                StoredFrame::U8(v) => v.iter().map(|&s| map_value(s as u32)).collect(),
                StoredFrame::U16(v) => v.iter().map(|&s| map_value(s as u32)).collect(),
                StoredFrame::U32(v) => v.iter().map(|&s| map_value(s)).collect(),
                StoredFrame::F32(_) | StoredFrame::F64(_) => Vec::new(),
            };

            if deep {
                PixelData::L16(out16)
            } else {
                PixelData::L8(out16.into_iter().map(|v| v as u8).collect())
            }
        })
        .collect()
}

pub fn float_frames_to_pixels(frames: &[StoredFrame], transform: &MonoTransform) -> Vec<PixelData> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;

    for frame in frames {
        match frame {
            StoredFrame::F32(v) => {
                for &s in v {
                    if s.is_finite() {
                        min = min.min(s as f64);
                        max = max.max(s as f64);
                    }
                }
            }
            StoredFrame::F64(v) => {
                for &s in v {
                    if s.is_finite() {
                        min = min.min(s);
                        max = max.max(s);
                    }
                }
            }
            _ => {}
        }
    }

    if !min.is_finite() || !max.is_finite() {
        min = 0.0;
        max = 1.0;
    }

    let mapper = DisplayMapper::new(transform, min, max, 1.0);

    frames
        .iter()
        .map(|frame| match frame {
            StoredFrame::F32(v) => PixelData::L32F(v.iter().map(|&s| mapper.map(s as f64) as f32).collect()),
            StoredFrame::F64(v) => PixelData::L64F(v.iter().map(|&s| mapper.map(s)).collect()),
            _ => PixelData::L32F(Vec::new()),
        })
        .collect()
}

fn sample_scale_table(params: &PixelParams, deep: bool) -> Vec<u16> {
    let mask = SampleMask::new(params);
    let bits = params.bits_stored.clamp(1, 16) as u32;
    let in_max = ((1u64 << bits) - 1) as f64;
    let out_max = if deep { 65535.0 } else { 255.0 };
    let size = 1usize << params.bits_allocated.clamp(1, 16);

    (0..size)
        .map(|container| {
            let v = mask.apply(container as u32).max(0) as f64;
            ((v / in_max) * out_max).round().min(out_max) as u16
        })
        .collect()
}

fn ybr_to_rgb(y: f64, cb: f64, cr: f64, max: f64, partial: bool) -> (u16, u16, u16) {
    let (r, g, b) = if partial {
        (
            1.1631 * y + 1.5969 * cr - 0.8713 * max,
            1.1631 * y - 0.3913 * cb - 0.8121 * cr + 0.5290 * max,
            1.1631 * y + 2.0177 * cb - 1.0820 * max,
        )
    } else {
        (y + 1.4020 * cr - 0.7010 * max, y - 0.3441 * cb - 0.7141 * cr + 0.5291 * max, y + 1.7720 * cb - 0.8859 * max)
    };

    (r.clamp(0.0, max) as u16, g.clamp(0.0, max) as u16, b.clamp(0.0, max) as u16)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64, max: f64) -> (u16, u16, u16) {
    let h6 = h / (max + 1.0) * 6.0;
    let sector = (h6.floor() as i32).rem_euclid(6);
    let f = h6 - h6.floor();
    let s = s / max;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));

    let (r, g, b) = match sector {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };

    (r.clamp(0.0, max) as u16, g.clamp(0.0, max) as u16, b.clamp(0.0, max) as u16)
}

fn palette_out(lut: &Lut, index: i64, deep: bool) -> u16 {
    let value = lut.lookup(index);
    let bits = lut.bits.clamp(1, 16) as u32;
    let in_max = ((1u64 << bits) - 1) as f64;
    let out_max = if deep { 65535.0 } else { 255.0 };
    ((value as f64 / in_max) * out_max).round().min(out_max) as u16
}

pub fn color_frame_to_pixels(
    frame: &StoredFrame,
    params: &PixelParams,
    palette: Option<&[Lut; 3]>,
) -> VexelResult<PixelData> {
    let pixels = params.pixels_per_frame();
    let mask = SampleMask::new(params);

    let samples_u32: Vec<u32> = match frame {
        StoredFrame::U8(v) => v.iter().map(|&s| s as u32).collect(),
        StoredFrame::U16(v) => v.iter().map(|&s| s as u32).collect(),
        StoredFrame::U32(v) => v.clone(),
        _ => return Err(VexelError::Custom("DICOM: float pixel data cannot be color".to_string())),
    };

    match &params.photometric {
        Photometric::PaletteColor => {
            let palette = palette.ok_or_else(|| {
                VexelError::Custom("DICOM: PALETTE COLOR image without palette lookup tables".to_string())
            })?;
            let deep = palette.iter().any(|l| l.bits > 8);

            let mut out = Vec::with_capacity(pixels * 3);
            for p in 0..pixels {
                let index = mask.apply(samples_u32.get(p).copied().unwrap_or(0));
                out.push(palette_out(&palette[0], index, deep));
                out.push(palette_out(&palette[1], index, deep));
                out.push(palette_out(&palette[2], index, deep));
            }
            Ok(pack_rgb(out, deep))
        }
        Photometric::Argb => {
            let palette_luts = palette;
            let deep = params.bits_stored > 8;
            let out_max = if deep { 65535.0 } else { 255.0 };
            let bits = params.bits_stored.clamp(1, 16) as u32;
            let in_max = ((1u64 << bits) - 1) as f64;

            let mut out = Vec::with_capacity(pixels * 3);
            for p in 0..pixels {
                let a = mask.apply(samples_u32.get(p * 4).copied().unwrap_or(0));
                if let Some(palette) = palette_luts.filter(|_| a > 0) {
                    out.push(palette_out(&palette[0], a, deep));
                    out.push(palette_out(&palette[1], a, deep));
                    out.push(palette_out(&palette[2], a, deep));
                } else {
                    for s in 0..3 {
                        let v = mask.apply(samples_u32.get(p * 4 + 1 + s).copied().unwrap_or(0)).max(0) as f64;
                        out.push(((v / in_max) * out_max).round().min(out_max) as u16);
                    }
                }
            }
            Ok(pack_rgb(out, deep))
        }
        Photometric::Cmyk => {
            let deep = params.bits_stored > 8;
            let bits = params.bits_stored.clamp(1, 16) as u32;
            let max = ((1u64 << bits) - 1) as i64;
            let out_max = if deep { 65535.0 } else { 255.0 };

            let mut out = Vec::with_capacity(pixels * 3);
            for p in 0..pixels {
                let k = mask.apply(samples_u32.get(p * 4 + 3).copied().unwrap_or(0));
                for s in 0..3 {
                    let c = mask.apply(samples_u32.get(p * 4 + s).copied().unwrap_or(0));
                    let v = (max - c - k).clamp(0, max) as f64;
                    out.push(((v / max as f64) * out_max).round().min(out_max) as u16);
                }
            }
            Ok(pack_rgb(out, deep))
        }
        Photometric::Hsv => {
            let deep = params.bits_stored > 8;
            let bits = params.bits_stored.clamp(1, 16) as u32;
            let max = ((1u64 << bits) - 1) as f64;
            let out_max = if deep { 65535.0 } else { 255.0 };

            let mut out = Vec::with_capacity(pixels * 3);
            for p in 0..pixels {
                let h = mask.apply(samples_u32.get(p * 3).copied().unwrap_or(0)).max(0) as f64;
                let s = mask.apply(samples_u32.get(p * 3 + 1).copied().unwrap_or(0)).max(0) as f64;
                let v = mask.apply(samples_u32.get(p * 3 + 2).copied().unwrap_or(0)).max(0) as f64;
                let (r, g, b) = hsv_to_rgb(h, s, v, max);
                out.push(((r as f64 / max) * out_max).round().min(out_max) as u16);
                out.push(((g as f64 / max) * out_max).round().min(out_max) as u16);
                out.push(((b as f64 / max) * out_max).round().min(out_max) as u16);
            }
            Ok(pack_rgb(out, deep))
        }
        photometric if photometric.is_ybr() => {
            let partial = matches!(photometric, Photometric::YbrPartial422 | Photometric::YbrPartial420);
            if *photometric == Photometric::YbrPartial420 {
                log_warn!("DICOM: YBR_PARTIAL_420 native data is not fully supported, interpreting as interleaved");
            }
            let deep = params.bits_stored > 8;
            let bits = params.bits_stored.clamp(1, 16) as u32;
            let max = ((1u64 << bits) - 1) as f64;
            let out_max = if deep { 65535.0 } else { 255.0 };

            let mut out = Vec::with_capacity(pixels * 3);
            for p in 0..pixels {
                let y = mask.apply(samples_u32.get(p * 3).copied().unwrap_or(0)).max(0) as f64;
                let cb = mask.apply(samples_u32.get(p * 3 + 1).copied().unwrap_or(0)).max(0) as f64;
                let cr = mask.apply(samples_u32.get(p * 3 + 2).copied().unwrap_or(0)).max(0) as f64;
                let (r, g, b) = ybr_to_rgb(y, cb, cr, max, partial);
                out.push(((r as f64 / max) * out_max).round().min(out_max) as u16);
                out.push(((g as f64 / max) * out_max).round().min(out_max) as u16);
                out.push(((b as f64 / max) * out_max).round().min(out_max) as u16);
            }
            Ok(pack_rgb(out, deep))
        }
        _ => {
            let samples = params.samples_per_pixel.max(1) as usize;
            let deep = params.bits_stored > 8;
            let table = sample_scale_table(params, deep);
            let has_alpha = samples >= 4;
            let channels = if has_alpha { 4 } else { 3 };

            let mut out = Vec::with_capacity(pixels * channels);
            for p in 0..pixels {
                for s in 0..channels.min(samples) {
                    let container = samples_u32.get(p * samples + s).copied().unwrap_or(0);
                    out.push(table[(container as usize).min(table.len() - 1)]);
                }
                for _ in samples..channels {
                    out.push(if deep { 65535 } else { 255 });
                }
            }

            if has_alpha {
                if deep {
                    Ok(PixelData::RGBA16(out))
                } else {
                    Ok(PixelData::RGBA8(out.into_iter().map(|v| v as u8).collect()))
                }
            } else {
                Ok(pack_rgb(out, deep))
            }
        }
    }
}

fn pack_rgb(values: Vec<u16>, deep: bool) -> PixelData {
    if deep {
        PixelData::RGB16(values)
    } else {
        PixelData::RGB8(values.into_iter().map(|v| v as u8).collect())
    }
}

pub fn expand_segmented_lut(data: &[u8], entry_bits: u16) -> Vec<u16> {
    let words: Vec<u16> = if entry_bits <= 8 {
        data.iter().map(|&b| b as u16).collect()
    } else {
        data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
    };

    let mut out: Vec<u16> = Vec::new();
    let mut pos = 0usize;

    while pos + 1 < words.len() && out.len() < 1 << 16 {
        let opcode = words[pos];
        let count = words[pos + 1] as usize;
        pos += 2;

        match opcode {
            0 => {
                for i in 0..count {
                    out.push(words.get(pos + i).copied().unwrap_or(0));
                }
                pos += count;
            }
            1 => {
                if pos >= words.len() {
                    break;
                }
                let target = words[pos] as f64;
                pos += 1;
                let start = out.last().copied().unwrap_or(0) as f64;
                if count == 0 {
                    continue;
                }
                for i in 1..=count {
                    let v = start + (target - start) * i as f64 / count as f64;
                    out.push(v.round() as u16);
                }
            }
            2 => {
                log_warn!("DICOM: indirect segmented palette segments are not supported, stopping expansion");
                break;
            }
            _ => {
                log_warn!("DICOM: unknown segmented palette opcode {}, stopping expansion", opcode);
                break;
            }
        }
    }

    out
}
