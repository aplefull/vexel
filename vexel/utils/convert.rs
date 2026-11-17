use crate::utils::channel_simd;
use crate::utils::image::{PixelData, PixelFormat};
use crate::utils::sample::sealed::Sealed;
use crate::utils::sample::{Sample, SampleType};

const BAND_PIXELS: usize = 1 << 14;

#[cfg(feature = "rayon")]
const PARALLEL_MIN_PIXELS: usize = 1 << 18;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channels {
    L,
    LA,
    Rgb,
    Rgba,
}

impl Channels {
    pub(crate) fn has_alpha(self) -> bool {
        matches!(self, Channels::LA | Channels::Rgba)
    }

    pub(crate) fn count(self) -> usize {
        match self {
            Channels::L => 1,
            Channels::LA => 2,
            Channels::Rgb => 3,
            Channels::Rgba => 4,
        }
    }
}

type Kernel<T> = fn(&[T], &mut [u8]);

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub channels: Channels,
    pub premultiply: bool,
}

pub(crate) fn write_8bit(pixels: &PixelData, dst: &mut [u8], to: Target) {
    match pixels {
        PixelData::RGB8(p) => convert_u8(p, Channels::Rgb, dst, to),
        PixelData::RGBA8(p) => convert_u8(p, Channels::Rgba, dst, to),
        PixelData::L8(p) => convert_u8(p, Channels::L, dst, to),
        PixelData::LA8(p) => convert_u8(p, Channels::LA, dst, to),
        PixelData::RGB16(p) => convert_scaled(p, Channels::Rgb, dst, to, channel_simd::scale_u16_to_u8),
        PixelData::RGBA16(p) => convert_scaled(p, Channels::Rgba, dst, to, channel_simd::scale_u16_to_u8),
        PixelData::L16(p) => convert_scaled(p, Channels::L, dst, to, channel_simd::scale_u16_to_u8),
        PixelData::LA16(p) => convert_scaled(p, Channels::LA, dst, to, channel_simd::scale_u16_to_u8),
        PixelData::RGB32F(p) => convert_scaled(p, Channels::Rgb, dst, to, channel_simd::scale_f32_to_u8),
        PixelData::RGBA32F(p) => convert_scaled(p, Channels::Rgba, dst, to, channel_simd::scale_f32_to_u8),
        PixelData::L32F(p) => convert_scaled(p, Channels::L, dst, to, channel_simd::scale_f32_to_u8),
        PixelData::LA32F(p) => convert_scaled(p, Channels::LA, dst, to, channel_simd::scale_f32_to_u8),
        PixelData::RGB64F(p) => convert_scaled(p, Channels::Rgb, dst, to, scale_f64_to_u8),
        PixelData::RGBA64F(p) => convert_scaled(p, Channels::Rgba, dst, to, scale_f64_to_u8),
        PixelData::L64F(p) => convert_scaled(p, Channels::L, dst, to, scale_f64_to_u8),
        PixelData::LA64F(p) => convert_scaled(p, Channels::LA, dst, to, scale_f64_to_u8),
    }
}

pub(crate) fn write<T: Sample>(pixels: &PixelData, to: Channels, dst: &mut [T], premultiply: bool) {
    if matches!(to, Channels::Rgb | Channels::Rgba) {
        if let Some(bytes) = T::as_u8_mut(dst) {
            write_8bit(pixels, bytes, Target { channels: to, premultiply });
            return;
        }
    }

    match pixels {
        PixelData::RGB8(p) => convert_generic(p, Channels::Rgb, dst, to, premultiply),
        PixelData::RGBA8(p) => convert_generic(p, Channels::Rgba, dst, to, premultiply),
        PixelData::L8(p) => convert_generic(p, Channels::L, dst, to, premultiply),
        PixelData::LA8(p) => convert_generic(p, Channels::LA, dst, to, premultiply),
        PixelData::RGB16(p) => convert_generic(p, Channels::Rgb, dst, to, premultiply),
        PixelData::RGBA16(p) => convert_generic(p, Channels::Rgba, dst, to, premultiply),
        PixelData::L16(p) => convert_generic(p, Channels::L, dst, to, premultiply),
        PixelData::LA16(p) => convert_generic(p, Channels::LA, dst, to, premultiply),
        PixelData::RGB32F(p) => convert_generic(p, Channels::Rgb, dst, to, premultiply),
        PixelData::RGBA32F(p) => convert_generic(p, Channels::Rgba, dst, to, premultiply),
        PixelData::L32F(p) => convert_generic(p, Channels::L, dst, to, premultiply),
        PixelData::LA32F(p) => convert_generic(p, Channels::LA, dst, to, premultiply),
        PixelData::RGB64F(p) => convert_generic(p, Channels::Rgb, dst, to, premultiply),
        PixelData::RGBA64F(p) => convert_generic(p, Channels::Rgba, dst, to, premultiply),
        PixelData::L64F(p) => convert_generic(p, Channels::L, dst, to, premultiply),
        PixelData::LA64F(p) => convert_generic(p, Channels::LA, dst, to, premultiply),
    }
}

pub(crate) fn collect<T: Sample>(pixels: &PixelData, to: Channels) -> Vec<T> {
    let mut out = vec![T::default(); pixels.pixel_count() * to.count()];
    write(pixels, to, &mut out, false);
    out
}

pub(crate) fn into_layout<T: Sample>(pixels: PixelData, to: Channels) -> Vec<T> {
    if pixels.pixel_format() != PixelFormat::from_parts(to, T::TYPE) {
        return collect(&pixels, to);
    }

    match T::take(pixels) {
        Ok(samples) => samples,
        Err(pixels) => collect(&pixels, to),
    }
}

pub(crate) fn build(pixels: &PixelData, format: PixelFormat) -> PixelData {
    let layout = format.layout();
    match format.sample_type() {
        SampleType::U8 => u8::wrap(layout, collect(pixels, layout)),
        SampleType::U16 => u16::wrap(layout, collect(pixels, layout)),
        SampleType::F32 => f32::wrap(layout, collect(pixels, layout)),
        SampleType::F64 => f64::wrap(layout, collect(pixels, layout)),
    }
}

fn convert_generic<S: Sample, T: Sample>(src: &[S], from: Channels, dst: &mut [T], to: Channels, premultiply: bool) {
    let premultiply = premultiply && from.has_alpha() && to.has_alpha();
    let (from_count, to_count) = (from.count(), to.count());
    let pixels = (src.len() / from_count).min(dst.len() / to_count);
    let src = &src[..pixels * from_count];
    let dst = &mut dst[..pixels * to_count];

    let run = |s: &[S], d: &mut [T]| {
        for (sp, dp) in s.chunks_exact(from_count).zip(d.chunks_exact_mut(to_count)) {
            convert_pixel(sp, from, dp, to, premultiply);
        }
    };

    #[cfg(feature = "rayon")]
    if pixels >= PARALLEL_MIN_PIXELS {
        use rayon::prelude::*;

        dst.par_chunks_mut(BAND_PIXELS * to_count)
            .zip(src.par_chunks(BAND_PIXELS * from_count))
            .for_each(|(d, s)| run(s, d));
        return;
    }

    run(src, dst);
}

#[inline(always)]
fn convert_pixel<S: Sample, T: Sample>(s: &[S], from: Channels, d: &mut [T], to: Channels, premultiply: bool) {
    let alpha = match from {
        Channels::LA => s[1].cast::<T>(),
        Channels::Rgba => s[3].cast::<T>(),
        Channels::L | Channels::Rgb => T::OPAQUE,
    };
    let gray = matches!(from, Channels::L | Channels::LA);
    let finish = |v: T| if premultiply { v.premultiply(alpha) } else { v };

    match to {
        Channels::L | Channels::LA => {
            let luma = if gray { s[0].cast::<T>() } else { T::from_f64(luma(s[0].unit(), s[1].unit(), s[2].unit())) };
            d[0] = finish(luma);
            if to == Channels::LA {
                d[1] = alpha;
            }
        }
        Channels::Rgb | Channels::Rgba => {
            let (r, g, b) = if gray { (s[0], s[0], s[0]) } else { (s[0], s[1], s[2]) };
            d[0] = finish(r.cast());
            d[1] = finish(g.cast());
            d[2] = finish(b.cast());
            if to == Channels::Rgba {
                d[3] = alpha;
            }
        }
    }
}

fn luma(r: f64, g: f64, b: f64) -> f64 {
    0.299 * r + 0.587 * g + 0.114 * b
}

fn convert_u8(src: &[u8], from: Channels, dst: &mut [u8], to: Target) {
    match rearrange(from, to.channels) {
        Some(kernel) => for_each_band(src, from, dst, to, |s, d, _| kernel(s, d)),
        None => for_each_band(src, from, dst, to, |s, d, _| d.copy_from_slice(s)),
    }
}

fn convert_scaled<T: Copy + Sync>(src: &[T], from: Channels, dst: &mut [u8], to: Target, scale: Kernel<T>) {
    match rearrange(from, to.channels) {
        Some(kernel) => for_each_band(src, from, dst, to, |s, d, scratch| {
            if scratch.len() < s.len() {
                scratch.resize(s.len(), 0);
            }
            let scaled = &mut scratch[..s.len()];
            scale(s, scaled);
            kernel(scaled, d);
        }),
        None => for_each_band(src, from, dst, to, |s, d, _| scale(s, d)),
    }
}

fn rearrange(from: Channels, to: Channels) -> Option<Kernel<u8>> {
    match (from, to) {
        (Channels::Rgb, Channels::Rgba) => Some(channel_simd::rgb_to_rgba),
        (Channels::Rgba, Channels::Rgb) => Some(channel_simd::rgba_to_rgb),
        (Channels::L, Channels::Rgba) => Some(gray_to_rgba),
        (Channels::L, Channels::Rgb) => Some(gray_to_rgb),
        (Channels::LA, Channels::Rgba) => Some(gray_alpha_to_rgba),
        (Channels::LA, Channels::Rgb) => Some(gray_alpha_to_rgb),
        (from, to) if from == to => None,
        _ => unreachable!("conversion only targets RGB and RGBA"),
    }
}

fn for_each_band<T: Sync>(
    src: &[T],
    from: Channels,
    dst: &mut [u8],
    to: Target,
    kernel: impl Fn(&[T], &mut [u8], &mut Vec<u8>) + Sync,
) {
    let premultiply = to.premultiply && from.has_alpha() && to.channels == Channels::Rgba;
    let kernel = |s: &[T], d: &mut [u8], scratch: &mut Vec<u8>| {
        kernel(s, d, scratch);
        if premultiply {
            channel_simd::premultiply_rgba(d);
        }
    };

    let to = to.channels;
    let pixels = (src.len() / from.count()).min(dst.len() / to.count());
    let src = &src[..pixels * from.count()];
    let dst = &mut dst[..pixels * to.count()];
    let src_band = BAND_PIXELS * from.count();
    let dst_band = BAND_PIXELS * to.count();

    #[cfg(feature = "rayon")]
    if pixels >= PARALLEL_MIN_PIXELS {
        use rayon::prelude::*;

        dst.par_chunks_mut(dst_band)
            .zip(src.par_chunks(src_band))
            .for_each_init(Vec::new, |scratch, (d, s)| kernel(s, d, scratch));
        return;
    }

    let mut scratch = Vec::new();
    for (d, s) in dst.chunks_mut(dst_band).zip(src.chunks(src_band)) {
        kernel(s, d, &mut scratch);
    }
}

fn scale_f64_to_u8(src: &[f64], dst: &mut [u8]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = (s.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

fn gray_to_rgba(src: &[u8], dst: &mut [u8]) {
    for (d, &g) in dst.chunks_exact_mut(4).zip(src) {
        d.copy_from_slice(&((u32::from(g) * 0x0001_0101) | 0xff00_0000).to_le_bytes());
    }
}

fn gray_alpha_to_rgba(src: &[u8], dst: &mut [u8]) {
    for (d, la) in dst.chunks_exact_mut(4).zip(src.chunks_exact(2)) {
        d.copy_from_slice(&((u32::from(la[0]) * 0x0001_0101) | (u32::from(la[1]) << 24)).to_le_bytes());
    }
}

fn gray_to_rgb(src: &[u8], dst: &mut [u8]) {
    for (d, &g) in dst.chunks_exact_mut(3).zip(src) {
        d.fill(g);
    }
}

fn gray_alpha_to_rgb(src: &[u8], dst: &mut [u8]) {
    for (d, la) in dst.chunks_exact_mut(3).zip(src.chunks_exact(2)) {
        d.fill(la[0]);
    }
}
