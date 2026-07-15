use std::io::{Read, Seek};

use crate::bitreader::BitReader;
use crate::decoders::bmp::types::{
    BitmapArrayHeader, BitmapCompression, BitmapCoreHeader, BitmapFileHeader, BitmapInfoHeader, BitmapV2InfoHeader,
    BitmapV3InfoHeader, BitmapV4Header, BitmapV5Header, ColorSpace, DibHeader, OS22XBitmapHeader, CIEXYZ,
};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

pub struct HeaderReader;

impl HeaderReader {
    pub fn read_file_header<R: Read + Seek>(
        reader: &mut BitReader<R>,
    ) -> VexelResult<(BitmapFileHeader, Option<BitmapArrayHeader>)> {
        let peeked = reader.peek_bytes(16).unwrap_or_default();
        if peeked.len() >= 16 && Self::looks_like_headerless_dib(&peeked) {
            log_warn!("No BMP file header found; treating file as a headerless DIB");
            return Ok((BitmapFileHeader { file_size: 0, reserved1: 0, reserved2: 0, pixel_offset: 0 }, None));
        }

        let mut vendor_prefix_len = 0u64;
        if peeked.len() >= 2 && &peeked[0..2] != b"BM" {
            if let Some(prefix_len) = Self::find_bmp_after_vendor_prefix(reader) {
                log_warn!("Found a valid-looking BMP {} bytes into the file; skipping vendor prefix", prefix_len);
                reader.seek(std::io::SeekFrom::Current(prefix_len as i64))?;
                vendor_prefix_len = prefix_len;
            }
        }

        let signature = reader.read_u16()?;

        if signature == 0x4142 {
            let ba = BitmapArrayHeader {
                file_size: reader.read_u32()?,
                next_offset: reader.read_u32()?,
                screen_width: reader.read_u16()?,
                screen_height: reader.read_u16()?,
            };
            let (file_header, _) = Self::read_file_header(reader)?;
            return Ok((file_header, Some(ba)));
        }

        match signature {
            0x4D42 => (), // "BM" - Windows bitmap
            0x4943 => (), // "CI" - OS/2 color icon
            0x5043 => (), // "CP" - OS/2 color pointer
            0x4349 => (), // "IC" - OS/2 icon
            0x5450 => (), // "PT" - OS/2 pointer
            _ => {
                log_warn!("Invalid BMP signature: 0x{:X}", signature);
            }
        }

        Ok((
            BitmapFileHeader {
                file_size: reader.read_u32()?.saturating_add(vendor_prefix_len as u32),
                reserved1: reader.read_u16()?,
                reserved2: reader.read_u16()?,
                pixel_offset: reader.read_u32()?.saturating_add(vendor_prefix_len as u32),
            },
            None,
        ))
    }

    fn looks_like_headerless_dib(header: &[u8]) -> bool {
        let dib_header_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if !matches!(dib_header_size, 40 | 52 | 56 | 64 | 108 | 124) {
            return false;
        }

        let width = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let height = i32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let planes = u16::from_le_bytes([header[12], header[13]]);
        let bpp = u16::from_le_bytes([header[14], header[15]]);

        width != 0 && height != 0 && planes == 1 && matches!(bpp, 1 | 2 | 4 | 8 | 16 | 24 | 32 | 64)
    }

    fn find_bmp_after_vendor_prefix<R: Read + Seek>(reader: &mut BitReader<R>) -> Option<u64> {
        const PREFIX_SCAN_LIMIT: usize = 512;

        let bytes_left = reader.bytes_left().ok()?;
        let window_len = (PREFIX_SCAN_LIMIT as u64).min(bytes_left) as usize;
        let window = reader.peek_bytes(window_len).ok()?;

        for offset in 1..window.len().saturating_sub(30) {
            if &window[offset..offset + 2] != b"BM" {
                continue;
            }

            let bytes_remaining = bytes_left - offset as u64;
            if Self::embedded_bmp_is_plausible(&window[offset..], bytes_remaining) {
                return Some(offset as u64);
            }
        }

        None
    }

    pub(crate) fn embedded_bmp_is_plausible(bytes: &[u8], bytes_remaining: u64) -> bool {
        if bytes.len() < 30 {
            return false;
        }

        let pixel_offset = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);
        let dib_header_size = u32::from_le_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);

        if !matches!(dib_header_size, 40 | 52 | 56 | 64 | 108 | 124) {
            return false;
        }

        let width = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
        let height = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
        let planes = u16::from_le_bytes([bytes[26], bytes[27]]);
        let bpp = u16::from_le_bytes([bytes[28], bytes[29]]);

        if width == 0 || height == 0 || planes != 1 || !matches!(bpp, 1 | 2 | 4 | 8 | 16 | 24 | 32 | 64) {
            return false;
        }

        let min_header_size = 14u64 + dib_header_size as u64;
        if u64::from(pixel_offset) < min_header_size {
            return false;
        }

        let row_size = (u64::from(bpp) * u64::from(width.unsigned_abs())).div_ceil(32) * 4;
        let data_size = row_size * u64::from(height.unsigned_abs());

        u64::from(pixel_offset).saturating_add(data_size) <= bytes_remaining.saturating_add(row_size)
    }

    pub fn read_info_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<(DibHeader, u32, u32)> {
        let header_size = reader.read_u32()?;

        let dib_header = match header_size {
            12 => DibHeader::Core(Self::read_bitmap_core_header(reader)?),
            16 => DibHeader::OS2V2(Self::read_os2_v2_short_header(reader)?),
            64 => DibHeader::OS2V2(Self::read_os2_v2_header(reader)?),
            40 => DibHeader::Info(Self::read_bitmap_info_header(reader)?),
            52 => DibHeader::V2(Self::read_v2_header(reader)?),
            56 => DibHeader::V3(Self::read_v3_header(reader)?),
            108 => DibHeader::V4(Self::read_v4_header(reader)?),
            124 => DibHeader::V5(Self::read_v5_header(reader)?),
            _ => {
                log_warn!("Invalid DIB header size: {}, assuming 40 bytes. This may cause issues.", header_size);
                DibHeader::Info(Self::read_bitmap_info_header(reader)?)
            }
        };

        let (width, height) = match &dib_header {
            DibHeader::Core(h) => (h.width as u32, h.height as u32),
            DibHeader::OS2V2(h) => (h.width.unsigned_abs(), h.height.unsigned_abs()),
            DibHeader::Info(h) => (h.width.unsigned_abs(), h.height.unsigned_abs()),
            DibHeader::V2(h) => (h.info.width.unsigned_abs(), h.info.height.unsigned_abs()),
            DibHeader::V3(h) => (h.v2.info.width.unsigned_abs(), h.v2.info.height.unsigned_abs()),
            DibHeader::V4(h) => (h.v3.v2.info.width.unsigned_abs(), h.v3.v2.info.height.unsigned_abs()),
            DibHeader::V5(h) => (h.v4.v3.v2.info.width.unsigned_abs(), h.v4.v3.v2.info.height.unsigned_abs()),
        };

        if width == 0 || height == 0 {
            return Err(VexelError::InvalidDimensions { width, height });
        }

        Ok((dib_header, width, height))
    }

    fn read_bitmap_core_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapCoreHeader> {
        Ok(BitmapCoreHeader {
            width: reader.read_u16()?,
            height: reader.read_u16()?,
            planes: reader.read_u16()?,
            bits_per_pixel: reader.read_u16()?,
        })
    }

    fn read_os2_v2_short_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<OS22XBitmapHeader> {
        Ok(OS22XBitmapHeader {
            width: reader.read_u32()? as i32,
            height: reader.read_u32()? as i32,
            planes: reader.read_u16()?,
            bits_per_pixel: reader.read_u16()?,
            compression: BitmapCompression::BiRgb,
            image_size: 0,
            x_pixels_per_meter: 0,
            y_pixels_per_meter: 0,
            colors_used: 0,
            important_colors: 0,
            units: 0,
            reserved: 0,
            recording: 0,
            rendering: 0,
            size1: 0,
            size2: 0,
            color_encoding: 0,
            identifier: 0,
        })
    }

    fn read_os2_v2_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<OS22XBitmapHeader> {
        Ok(OS22XBitmapHeader {
            width: reader.read_u32()? as i32,
            height: reader.read_u32()? as i32,
            planes: reader.read_u16()?,
            bits_per_pixel: reader.read_u16()?,
            compression: BitmapCompression::from_u32_os2(reader.read_u32()?),
            image_size: reader.read_u32()?,
            x_pixels_per_meter: reader.read_u32()? as i32,
            y_pixels_per_meter: reader.read_u32()? as i32,
            colors_used: reader.read_u32()?,
            important_colors: reader.read_u32()?,
            units: reader.read_u16()?,
            reserved: reader.read_u16()?,
            recording: reader.read_u16()?,
            rendering: reader.read_u16()?,
            size1: reader.read_u32()?,
            size2: reader.read_u32()?,
            color_encoding: reader.read_u32()?,
            identifier: reader.read_u32()?,
        })
    }

    pub fn read_bitmap_info_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapInfoHeader> {
        Ok(BitmapInfoHeader {
            width: reader.read_u32()? as i32,
            height: reader.read_u32()? as i32,
            planes: reader.read_u16()?,
            bits_per_pixel: reader.read_u16()?,
            compression: BitmapCompression::from_u32(reader.read_u32()?),
            image_size: reader.read_u32()?,
            x_pixels_per_meter: reader.read_u32()? as i32,
            y_pixels_per_meter: reader.read_u32()? as i32,
            colors_used: reader.read_u32()?,
            important_colors: reader.read_u32()?,
        })
    }

    fn read_v2_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapV2InfoHeader> {
        Ok(BitmapV2InfoHeader {
            info: Self::read_bitmap_info_header(reader)?,
            red_mask: reader.read_u32()?,
            green_mask: reader.read_u32()?,
            blue_mask: reader.read_u32()?,
        })
    }

    fn read_v3_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapV3InfoHeader> {
        Ok(BitmapV3InfoHeader { v2: Self::read_v2_header(reader)?, alpha_mask: reader.read_u32()? })
    }

    fn read_v4_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapV4Header> {
        Ok(BitmapV4Header {
            v3: Self::read_v3_header(reader)?,
            cs_type: reader.read_u32()?,
            endpoints: Self::read_color_space(reader)?,
            gamma_red: reader.read_u32()?,
            gamma_green: reader.read_u32()?,
            gamma_blue: reader.read_u32()?,
        })
    }

    fn read_v5_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapV5Header> {
        Ok(BitmapV5Header {
            v4: Self::read_v4_header(reader)?,
            intent: reader.read_u32()?,
            profile_data: reader.read_u32()?,
            profile_size: reader.read_u32()?,
            reserved: reader.read_u32()?,
        })
    }

    fn read_color_space<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<ColorSpace> {
        Ok(ColorSpace {
            ciexyz_red: Self::read_ciexyz(reader)?,
            ciexyz_green: Self::read_ciexyz(reader)?,
            ciexyz_blue: Self::read_ciexyz(reader)?,
        })
    }

    fn read_ciexyz<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<CIEXYZ> {
        Ok(CIEXYZ { x: reader.read_u32()? as i32, y: reader.read_u32()? as i32, z: reader.read_u32()? as i32 })
    }
}
