use crate::utils::convert::Channels;
use crate::utils::image::PixelData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    U8,
    U16,
    F32,
    F64,
}

impl SampleType {
    pub(crate) fn size(self) -> usize {
        match self {
            SampleType::U8 => 1,
            SampleType::U16 => 2,
            SampleType::F32 => 4,
            SampleType::F64 => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alpha {
    Straight,
    Premultiplied,
}

pub trait Sample: sealed::Sealed {}

impl Sample for u8 {}
impl Sample for u16 {}
impl Sample for f32 {}
impl Sample for f64 {}

#[allow(private_interfaces)]
pub(crate) mod sealed {
    use super::*;

    pub trait Sealed: Copy + Default + PartialEq + Send + Sync + 'static {
        const TYPE: SampleType;
        const OPAQUE: Self;

        fn from_u8(v: u8) -> Self;
        fn from_u16(v: u16) -> Self;
        fn from_f32(v: f32) -> Self;
        fn from_f64(v: f64) -> Self;
        fn cast<T: Sealed>(self) -> T;
        fn unit(self) -> f64;
        fn premultiply(self, alpha: Self) -> Self;

        fn view(pixels: &PixelData) -> Option<&[Self]>;
        fn take(pixels: PixelData) -> Result<Vec<Self>, PixelData>;
        fn wrap(layout: Channels, samples: Vec<Self>) -> PixelData;
        fn as_u8_mut(samples: &mut [Self]) -> Option<&mut [u8]>;
    }

    impl Sealed for u8 {
        const TYPE: SampleType = SampleType::U8;
        const OPAQUE: Self = u8::MAX;

        fn from_u8(v: u8) -> Self {
            v
        }

        fn from_u16(v: u16) -> Self {
            (v as f32 * 255.0 / 65535.0).round() as u8
        }

        fn from_f32(v: f32) -> Self {
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        }

        fn from_f64(v: f64) -> Self {
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        }

        fn cast<T: Sealed>(self) -> T {
            T::from_u8(self)
        }

        fn unit(self) -> f64 {
            self as f64 / 255.0
        }

        fn premultiply(self, alpha: Self) -> Self {
            let t = u16::from(self) * u16::from(alpha) + 128;
            ((t + (t >> 8)) >> 8) as u8
        }

        fn view(pixels: &PixelData) -> Option<&[Self]> {
            match pixels {
                PixelData::RGB8(v) | PixelData::RGBA8(v) | PixelData::L8(v) | PixelData::LA8(v) => Some(v),
                _ => None,
            }
        }

        fn take(pixels: PixelData) -> Result<Vec<Self>, PixelData> {
            match pixels {
                PixelData::RGB8(v) | PixelData::RGBA8(v) | PixelData::L8(v) | PixelData::LA8(v) => Ok(v),
                other => Err(other),
            }
        }

        fn wrap(layout: Channels, samples: Vec<Self>) -> PixelData {
            match layout {
                Channels::L => PixelData::L8(samples),
                Channels::LA => PixelData::LA8(samples),
                Channels::Rgb => PixelData::RGB8(samples),
                Channels::Rgba => PixelData::RGBA8(samples),
            }
        }

        fn as_u8_mut(samples: &mut [Self]) -> Option<&mut [u8]> {
            Some(samples)
        }
    }

    impl Sealed for u16 {
        const TYPE: SampleType = SampleType::U16;
        const OPAQUE: Self = u16::MAX;

        fn from_u8(v: u8) -> Self {
            u16::from(v) * 257
        }

        fn from_u16(v: u16) -> Self {
            v
        }

        fn from_f32(v: f32) -> Self {
            (v.clamp(0.0, 1.0) * 65535.0).round() as u16
        }

        fn from_f64(v: f64) -> Self {
            (v.clamp(0.0, 1.0) * 65535.0).round() as u16
        }

        fn cast<T: Sealed>(self) -> T {
            T::from_u16(self)
        }

        fn unit(self) -> f64 {
            self as f64 / 65535.0
        }

        fn premultiply(self, alpha: Self) -> Self {
            let t = u32::from(self) * u32::from(alpha) + 32768;
            ((t + (t >> 16)) >> 16) as u16
        }

        fn view(pixels: &PixelData) -> Option<&[Self]> {
            match pixels {
                PixelData::RGB16(v) | PixelData::RGBA16(v) | PixelData::L16(v) | PixelData::LA16(v) => Some(v),
                _ => None,
            }
        }

        fn take(pixels: PixelData) -> Result<Vec<Self>, PixelData> {
            match pixels {
                PixelData::RGB16(v) | PixelData::RGBA16(v) | PixelData::L16(v) | PixelData::LA16(v) => Ok(v),
                other => Err(other),
            }
        }

        fn wrap(layout: Channels, samples: Vec<Self>) -> PixelData {
            match layout {
                Channels::L => PixelData::L16(samples),
                Channels::LA => PixelData::LA16(samples),
                Channels::Rgb => PixelData::RGB16(samples),
                Channels::Rgba => PixelData::RGBA16(samples),
            }
        }

        fn as_u8_mut(_: &mut [Self]) -> Option<&mut [u8]> {
            None
        }
    }

    impl Sealed for f32 {
        const TYPE: SampleType = SampleType::F32;
        const OPAQUE: Self = 1.0;

        fn from_u8(v: u8) -> Self {
            v as f32 / 255.0
        }

        fn from_u16(v: u16) -> Self {
            v as f32 / 65535.0
        }

        fn from_f32(v: f32) -> Self {
            v
        }

        fn from_f64(v: f64) -> Self {
            v as f32
        }

        fn cast<T: Sealed>(self) -> T {
            T::from_f32(self)
        }

        fn unit(self) -> f64 {
            self as f64
        }

        fn premultiply(self, alpha: Self) -> Self {
            self * alpha
        }

        fn view(pixels: &PixelData) -> Option<&[Self]> {
            match pixels {
                PixelData::RGB32F(v) | PixelData::RGBA32F(v) | PixelData::L32F(v) | PixelData::LA32F(v) => Some(v),
                _ => None,
            }
        }

        fn take(pixels: PixelData) -> Result<Vec<Self>, PixelData> {
            match pixels {
                PixelData::RGB32F(v) | PixelData::RGBA32F(v) | PixelData::L32F(v) | PixelData::LA32F(v) => Ok(v),
                other => Err(other),
            }
        }

        fn wrap(layout: Channels, samples: Vec<Self>) -> PixelData {
            match layout {
                Channels::L => PixelData::L32F(samples),
                Channels::LA => PixelData::LA32F(samples),
                Channels::Rgb => PixelData::RGB32F(samples),
                Channels::Rgba => PixelData::RGBA32F(samples),
            }
        }

        fn as_u8_mut(_: &mut [Self]) -> Option<&mut [u8]> {
            None
        }
    }

    impl Sealed for f64 {
        const TYPE: SampleType = SampleType::F64;
        const OPAQUE: Self = 1.0;

        fn from_u8(v: u8) -> Self {
            v as f64 / 255.0
        }

        fn from_u16(v: u16) -> Self {
            v as f64 / 65535.0
        }

        fn from_f32(v: f32) -> Self {
            v as f64
        }

        fn from_f64(v: f64) -> Self {
            v
        }

        fn cast<T: Sealed>(self) -> T {
            T::from_f64(self)
        }

        fn unit(self) -> f64 {
            self
        }

        fn premultiply(self, alpha: Self) -> Self {
            self * alpha
        }

        fn view(pixels: &PixelData) -> Option<&[Self]> {
            match pixels {
                PixelData::RGB64F(v) | PixelData::RGBA64F(v) | PixelData::L64F(v) | PixelData::LA64F(v) => Some(v),
                _ => None,
            }
        }

        fn take(pixels: PixelData) -> Result<Vec<Self>, PixelData> {
            match pixels {
                PixelData::RGB64F(v) | PixelData::RGBA64F(v) | PixelData::L64F(v) | PixelData::LA64F(v) => Ok(v),
                other => Err(other),
            }
        }

        fn wrap(layout: Channels, samples: Vec<Self>) -> PixelData {
            match layout {
                Channels::L => PixelData::L64F(samples),
                Channels::LA => PixelData::LA64F(samples),
                Channels::Rgb => PixelData::RGB64F(samples),
                Channels::Rgba => PixelData::RGBA64F(samples),
            }
        }

        fn as_u8_mut(_: &mut [Self]) -> Option<&mut [u8]> {
            None
        }
    }
}
