use std::io::{Read, Seek, SeekFrom};

use super::info::{FarbfeldHeader, FarbfeldInfo, FarbfeldPixelData, FarbfeldSection};
use super::simd::{decode_be_u16, drop_alpha_u16, is_opaque_u16};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::PixelData;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary};
use crate::{log_warn, Image, Limits};

pub const MAGIC: &[u8; 8] = b"farbfeld";

const CHANNELS: usize = 4;
const BYTES_PER_PIXEL: usize = CHANNELS * 2;

pub struct FarbfeldDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    reader: BitReader<R>,
    info: Option<FarbfeldInfo>,
}

impl<R: Read + Seek> FarbfeldDecoder<R> {
    pub fn new(reader: R) -> Self {
        FarbfeldDecoder { width: 0, height: 0, limits: Limits::default(), reader: BitReader::new(reader), info: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    fn scan(&mut self) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        let mut info = FarbfeldInfo::default();

        let mut magic = [0u8; 8];
        let magic_read = self.reader.read(&mut magic)?;
        let magic_valid = magic_read == magic.len() && &magic == MAGIC;
        if !magic_valid {
            log_warn!("Invalid or truncated farbfeld magic value: {:?}, continuing anyway", magic);
        }

        let mut dims = [0u8; 8];
        let dims_read = self.reader.read(&mut dims)?;

        if dims_read != dims.len() {
            log_warn!("Truncated farbfeld header: got {} of 8 dimension bytes", dims_read);
        }

        self.width = u32::from_be_bytes([dims[0], dims[1], dims[2], dims[3]]);
        self.height = u32::from_be_bytes([dims[4], dims[5], dims[6], dims[7]]);

        let header_length = (magic_read + dims_read) as u64;
        info.push(
            0,
            Some(header_length),
            FarbfeldSection::Header(FarbfeldHeader { magic_valid, width: self.width, height: self.height }),
        );

        let expected = self.expected_bytes();
        let available = self.reader.bytes_left().unwrap_or(0);
        let actual = expected.min(available);
        info.push(
            header_length,
            Some(actual),
            FarbfeldSection::PixelData(FarbfeldPixelData { expected_length: expected }),
        );

        let mut summary = ImageSummary::new(self.width, self.height, ColorModel::Rgba, CHANNELS as u16, 16);
        summary.has_alpha = true;
        summary.data = Some(DataExtent { expected, actual });
        info.summary = Some(summary);

        self.info = Some(info);

        self.reader.seek(SeekFrom::Start(header_length))?;

        Ok(())
    }

    fn read_header(&mut self) -> VexelResult<()> {
        self.scan()?;

        if self.width == 0 || self.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.width, height: self.height });
        }

        self.limits.check_dimensions(self.width, self.height)?;

        Ok(())
    }

    fn expected_bytes(&self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height)).saturating_mul(BYTES_PER_PIXEL as u64)
    }

    fn expected_samples(&self) -> usize {
        let pixels = u64::from(self.width).saturating_mul(u64::from(self.height));
        let samples = pixels.saturating_mul(CHANNELS as u64);

        usize::try_from(samples).unwrap_or(usize::MAX)
    }

    fn read_pixel_data(&mut self) -> VexelResult<Vec<u8>> {
        let expected_bytes = self.expected_bytes();

        let available = self.reader.bytes_left().unwrap_or(0);
        let to_read = usize::try_from(expected_bytes.min(available)).unwrap_or(usize::MAX);

        self.limits.reserve_usize(to_read)?;

        let data = self.reader.read_bytes(to_read)?;

        if (data.len() as u64) < expected_bytes {
            log_warn!("Truncated farbfeld pixel data: expected {} bytes, got {}", expected_bytes, data.len());
        }

        Ok(data)
    }

    fn build_pixels(&mut self, raw: &[u8]) -> VexelResult<PixelData> {
        let total_samples = self.expected_samples();

        self.limits.reserve_usize(total_samples.saturating_mul(2))?;

        let mut samples = vec![0u16; total_samples];
        decode_be_u16(raw, &mut samples);

        if is_opaque_u16(&samples) {
            self.limits.free_usize(total_samples.saturating_mul(2));

            return Ok(self.build_rgb16(&samples));
        }

        Ok(PixelData::RGBA16(samples))
    }

    fn build_rgb16(&self, samples: &[u16]) -> PixelData {
        let pixels = samples.len() / CHANNELS;
        let mut rgb = vec![0u16; pixels * 3];

        drop_alpha_u16(samples, &mut rgb);

        PixelData::RGB16(rgb)
    }

    pub fn get_info(&mut self) -> FarbfeldInfo {
        if self.info.is_none() {
            if let Err(e) = self.scan() {
                log_warn!("Failed to read farbfeld header: {}", e);
            }
        }

        self.info.clone().unwrap_or_default()
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_header()?;

        let raw = self.read_pixel_data()?;

        let pixels = self.build_pixels(&raw)?;

        Ok(Image::from_pixels(self.width, self.height, pixels))
    }
}
