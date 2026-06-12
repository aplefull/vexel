use crate::decoders::bmp::types::{
    BitmapCoreHeader, BitmapFileHeader, BitmapInfoHeader, BitmapV2InfoHeader, BitmapV3InfoHeader,
    BitmapV4Header, BitmapV5Header, BitmapCompression, CIEXYZ, ColorSpace, DibHeader, OS22XBitmapHeader,
};
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn};
use crate::bitreader::BitReader;
use std::io::{Read, Seek};

pub struct HeaderReader;

impl HeaderReader {
    pub fn read_file_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<BitmapFileHeader> {
        let signature = reader.read_u16()?;

        if signature == 0x4142 {
            let _file_size = reader.read_u32()?;
            let _next_offset = reader.read_u32()?;
            let _screen_width = reader.read_u16()?;
            let _screen_height = reader.read_u16()?;
            return Self::read_file_header(reader);
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

        Ok(BitmapFileHeader {
            file_size: reader.read_u32()?,
            reserved1: reader.read_u16()?,
            reserved2: reader.read_u16()?,
            pixel_offset: reader.read_u32()?,
        })
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
                log_warn!(
                    "Invalid DIB header size: {}, assuming 40 bytes. This may cause issues.",
                    header_size
                );
                DibHeader::Info(Self::read_bitmap_info_header(reader)?)
            }
        };

        let (width, height) = match &dib_header {
            DibHeader::Core(h) => (h.width as u32, h.height as u32),
            DibHeader::OS2V2(h) => (h.width.abs() as u32, h.height.abs() as u32),
            DibHeader::Info(h) => (h.width.abs() as u32, h.height.abs() as u32),
            DibHeader::V2(h) => (h.info.width.abs() as u32, h.info.height.abs() as u32),
            DibHeader::V3(h) => (h.v2.info.width.abs() as u32, h.v2.info.height.abs() as u32),
            DibHeader::V4(h) => (h.v3.v2.info.width.abs() as u32, h.v3.v2.info.height.abs() as u32),
            DibHeader::V5(h) => (h.v4.v3.v2.info.width.abs() as u32, h.v4.v3.v2.info.height.abs() as u32),
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
            compression: BitmapCompression::from_u32(reader.read_u32()?),
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
        Ok(BitmapV3InfoHeader {
            v2: Self::read_v2_header(reader)?,
            alpha_mask: reader.read_u32()?,
        })
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
        Ok(CIEXYZ {
            x: reader.read_u32()? as i32,
            y: reader.read_u32()? as i32,
            z: reader.read_u32()? as i32,
        })
    }
}
