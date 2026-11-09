use std::io::{Read, Seek};

use crate::bitreader::BitReader;
use crate::decoders::bmp::types::{
    BitmapArrayHeader, BitmapCompression, BitmapFileHeader, ColorMasks, ColorSpace, DibColorSpace, DibHeader, DibKind,
    DibV5Fields, Os2Fields, CIEXYZ,
};
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

pub struct FileHeaders {
    pub file_header: Option<BitmapFileHeader>,
    pub array_header: Option<BitmapArrayHeader>,
    pub vendor_prefix_len: u64,
}

pub struct HeaderReader;

const MAX_BITMAP_ARRAY_DEPTH: u32 = 16;

impl HeaderReader {
    pub fn read_file_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<FileHeaders> {
        Self::read_file_header_at_depth(reader, 0)
    }

    fn read_file_header_at_depth<R: Read + Seek>(reader: &mut BitReader<R>, depth: u32) -> VexelResult<FileHeaders> {
        let peeked = reader.peek_bytes(16).unwrap_or_default();
        if peeked.len() >= 16 && Self::looks_like_headerless_dib(&peeked) {
            log_warn!("No BMP file header found; treating file as a headerless DIB");
            return Ok(FileHeaders { file_header: None, array_header: None, vendor_prefix_len: 0 });
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
            if depth >= MAX_BITMAP_ARRAY_DEPTH {
                log_warn!("BMP bitmap array headers nested deeper than {}, ignoring the rest", MAX_BITMAP_ARRAY_DEPTH);
                return Ok(FileHeaders { file_header: None, array_header: Some(ba), vendor_prefix_len });
            }
            let inner = Self::read_file_header_at_depth(reader, depth + 1)?;
            return Ok(FileHeaders {
                file_header: inner.file_header,
                array_header: Some(ba),
                vendor_prefix_len: vendor_prefix_len + inner.vendor_prefix_len,
            });
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

        Ok(FileHeaders {
            file_header: Some(BitmapFileHeader {
                signature,
                file_size: reader.read_u32()?.saturating_add(vendor_prefix_len as u32),
                reserved1: reader.read_u16()?,
                reserved2: reader.read_u16()?,
                pixel_offset: reader.read_u32()?.saturating_add(vendor_prefix_len as u32),
            }),
            array_header: None,
            vendor_prefix_len,
        })
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
        let data_size = row_size.saturating_mul(u64::from(height.unsigned_abs()));

        u64::from(pixel_offset).saturating_add(data_size) <= bytes_remaining.saturating_add(row_size)
    }

    pub fn read_info_header<R: Read + Seek>(reader: &mut BitReader<R>) -> VexelResult<(DibHeader, u32, u32)> {
        let header_size = reader.read_u32()?;
        let mut header = DibHeader::empty();
        header.header_size = header_size;

        match header_size {
            12 => {
                header.kind = DibKind::Core;
                header.width = i32::from(reader.read_u16()?);
                header.height = i32::from(reader.read_u16()?);
                header.planes = reader.read_u16()?;
                header.bits_per_pixel = reader.read_u16()?;
            }
            16 => {
                header.kind = DibKind::Os2V2Short;
                header.width = reader.read_u32()? as i32;
                header.height = reader.read_u32()? as i32;
                header.planes = reader.read_u16()?;
                header.bits_per_pixel = reader.read_u16()?;
            }
            64 => {
                header.kind = DibKind::Os2V2;
                Self::read_info_fields(reader, &mut header, true)?;
                header.os2 = Some(Os2Fields {
                    units: reader.read_u16()?,
                    reserved: reader.read_u16()?,
                    recording: reader.read_u16()?,
                    rendering: reader.read_u16()?,
                    size1: reader.read_u32()?,
                    size2: reader.read_u32()?,
                    color_encoding: reader.read_u32()?,
                    identifier: reader.read_u32()?,
                });
            }
            40 | 52 | 56 | 108 | 124 => {
                header.kind = match header_size {
                    40 => DibKind::Info,
                    52 => DibKind::V2,
                    56 => DibKind::V3,
                    108 => DibKind::V4,
                    _ => DibKind::V5,
                };
                Self::read_info_fields(reader, &mut header, false)?;
                if header_size >= 52 {
                    let red = reader.read_u32()?;
                    let green = reader.read_u32()?;
                    let blue = reader.read_u32()?;
                    let alpha = if header_size >= 56 { Some(reader.read_u32()?) } else { None };
                    header.masks = Some(ColorMasks { red, green, blue, alpha });
                }
                if header_size >= 108 {
                    header.color_space = Some(DibColorSpace {
                        cs_type: reader.read_u32()?,
                        endpoints: Self::read_color_space(reader)?,
                        gamma_red: reader.read_u32()?,
                        gamma_green: reader.read_u32()?,
                        gamma_blue: reader.read_u32()?,
                    });
                }
                if header_size >= 124 {
                    header.v5 = Some(DibV5Fields {
                        intent: reader.read_u32()?,
                        profile_data: reader.read_u32()?,
                        profile_size: reader.read_u32()?,
                        reserved: reader.read_u32()?,
                    });
                }
            }
            _ => {
                log_warn!("Invalid DIB header size: {}, assuming 40 bytes. This may cause issues.", header_size);
                header.kind = DibKind::Info;
                Self::read_info_fields(reader, &mut header, false)?;
            }
        }

        let (width, height) = (header.width.unsigned_abs(), header.height.unsigned_abs());

        if width == 0 || height == 0 {
            return Err(VexelError::InvalidDimensions { width, height });
        }

        Ok((header, width, height))
    }

    fn read_info_fields<R: Read + Seek>(
        reader: &mut BitReader<R>,
        header: &mut DibHeader,
        os2: bool,
    ) -> VexelResult<()> {
        header.width = reader.read_u32()? as i32;
        header.height = reader.read_u32()? as i32;
        header.planes = reader.read_u16()?;
        header.bits_per_pixel = reader.read_u16()?;
        header.compression_raw = reader.read_u32()?;
        header.compression = if os2 {
            BitmapCompression::from_u32_os2(header.compression_raw)
        } else {
            BitmapCompression::from_u32(header.compression_raw)
        };
        header.image_size = reader.read_u32()?;
        header.x_pixels_per_meter = reader.read_u32()? as i32;
        header.y_pixels_per_meter = reader.read_u32()? as i32;
        header.colors_used = reader.read_u32()?;
        header.important_colors = reader.read_u32()?;
        Ok(())
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
