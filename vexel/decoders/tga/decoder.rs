use crate::utils::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::TgaInfo;
use crate::{log_warn, Image, PixelData};
use std::io::{Read, Seek, SeekFrom};

use super::types::{
    ATTR_TYPE_ALPHA, ATTR_TYPE_PREMULTIPLIED_ALPHA, EXT_AREA_ATTR_TYPE_OFFSET, FLAG_ALPHA_SIZE_MASK,
    FLAG_ORIGIN_RIGHT, FLAG_ORIGIN_TOP, IMAGE_TYPE_FLAG_RLE, IMAGE_TYPE_MASK, IMAGE_TYPE_MONOCHROME,
    IMAGE_TYPE_PALETTED, TGA_FOOTER_SIZE, TGA_SIGNATURE,
};
use super::types::{
    ExtAlphaType, TgaColorMapData, TgaExtensionAreaData, TgaFooterData, TgaHeader, TgaHeaderData,
    TgaImageIdData, TgaPixelData, TgaSectionData, TgaSectionInfo,
};

pub struct TgaDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    sections: Vec<TgaSectionInfo>,
}

impl<R: Read + Seek> TgaDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BitReader::with_le(reader),
            sections: Vec::new(),
        }
    }

    pub fn get_info(&self) -> TgaInfo {
        TgaInfo {
            sections: self.sections.clone(),
        }
    }

    fn read_header(&mut self) -> VexelResult<TgaHeader> {
        Ok(TgaHeader {
            id_length: self.reader.read_u8()?,
            palette_type: self.reader.read_u8()?,
            image_type_raw: self.reader.read_u8()?,
            palette_first: self.reader.read_u16()?,
            palette_length: self.reader.read_u16()?,
            palette_bpp: self.reader.read_u8()?,
            x_origin: self.reader.read_u16()?,
            y_origin: self.reader.read_u16()?,
            width: self.reader.read_u16()?,
            height: self.reader.read_u16()?,
            bpp: self.reader.read_u8()?,
            flags: self.reader.read_u8()?,
        })
    }

    fn read_footer(&mut self) -> Option<(TgaFooterData, u64)> {
        self.reader.seek(SeekFrom::End(-TGA_FOOTER_SIZE)).ok()?;
        let footer_offset = self.reader.stream_position().ok()?;

        let mut footer = [0u8; TGA_FOOTER_SIZE as usize];
        self.reader.read_exact(&mut footer).ok()?;

        if &footer[8..26] != TGA_SIGNATURE {
            return None;
        }

        let extension_area_offset = u32::from_le_bytes([footer[0], footer[1], footer[2], footer[3]]);
        let developer_dir_offset = u32::from_le_bytes([footer[4], footer[5], footer[6], footer[7]]);

        Some((TgaFooterData { extension_area_offset, developer_dir_offset }, footer_offset))
    }

    fn read_extension_area(&mut self, ext_offset: u64) -> Option<TgaExtensionAreaData> {
        self.reader.seek(SeekFrom::Start(ext_offset)).ok()?;

        let mut buf = [0u8; 495];
        self.reader.read_exact(&mut buf).ok()?;

        let get_u16 = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]);
        let get_u32 = |i: usize| u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);

        let read_str = |start: usize, len: usize| -> String {
            let slice = &buf[start..start + len];
            let end = slice.iter().position(|&b| b == 0).unwrap_or(len);
            String::from_utf8_lossy(&slice[..end]).into_owned()
        };

        Some(TgaExtensionAreaData {
            extension_size: get_u16(0),
            author_name: read_str(2, 41),
            author_comments: read_str(43, 324),
            date_month: get_u16(367),
            date_day: get_u16(369),
            date_year: get_u16(371),
            time_hour: get_u16(373),
            time_minute: get_u16(375),
            time_second: get_u16(377),
            job_name: read_str(379, 41),
            job_hours: get_u16(420),
            job_minutes: get_u16(422),
            job_seconds: get_u16(424),
            software_id: read_str(426, 41),
            software_version_number: get_u16(467),
            software_version_letter: buf[469],
            key_color_a: buf[470],
            key_color_r: buf[471],
            key_color_g: buf[472],
            key_color_b: buf[473],
            pixel_aspect_ratio_numerator: get_u16(474),
            pixel_aspect_ratio_denominator: get_u16(476),
            gamma_value_numerator: get_u16(478),
            gamma_value_denominator: get_u16(480),
            color_correction_offset: get_u32(482),
            postage_stamp_offset: get_u32(486),
            scan_line_offset: get_u32(490),
            attributes_type: buf[494],
        })
    }

    fn expand_5bit(ch: u8) -> u8 {
        ((ch as u16 * 255 + 15) / 31) as u8
    }

    fn rgb_from_word(word: u16) -> [u8; 3] {
        [
            Self::expand_5bit(((word >> 10) & 0x1f) as u8),
            Self::expand_5bit(((word >> 5) & 0x1f) as u8),
            Self::expand_5bit((word & 0x1f) as u8),
        ]
    }

    fn read_palette_entry(&mut self, palette_bpp: u8, has_alpha: bool) -> [u8; 4] {
        match palette_bpp {
            15 | 16 => {
                let lo = self.reader.read_u8().unwrap_or(0);
                let hi = self.reader.read_u8().unwrap_or(0);
                let word = lo as u16 | ((hi as u16) << 8);
                let [r, g, b] = Self::rgb_from_word(word);
                [r, g, b, 255]
            }
            24 => {
                let b = self.reader.read_u8().unwrap_or(0);
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                [r, g, b, 255]
            }
            32 => {
                let b = self.reader.read_u8().unwrap_or(0);
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                let a = self.reader.read_u8().unwrap_or(255);
                [r, g, b, if has_alpha { a } else { 255 }]
            }
            bpp => {
                log_warn!("Unsupported TGA palette BPP: {}", bpp);
                [0, 0, 0, 255]
            }
        }
    }

    fn read_palette(&mut self, header: &TgaHeader) -> Vec<[u8; 4]> {
        let bytes_per_entry = ((header.palette_bpp as usize + 7) / 8).max(1);
        let skip_count = header.palette_first as usize;
        let read_count = (header.palette_length as usize).saturating_sub(skip_count);
        let has_alpha = header.palette_bpp == 32;

        if skip_count > 0 {
            let _ = self.reader.seek(SeekFrom::Current((skip_count * bytes_per_entry) as i64));
        }

        (0..read_count)
            .map(|_| self.read_palette_entry(header.palette_bpp, has_alpha))
            .collect()
    }

    fn skip_palette(&mut self, header: &TgaHeader) {
        if header.palette_type != 1 || header.palette_length == 0 {
            return;
        }
        let bytes_per_entry = ((header.palette_bpp as usize + 7) / 8).max(1);
        let total_bytes = header.palette_length as usize * bytes_per_entry;
        let _ = self.reader.seek(SeekFrom::Current(total_bytes as i64));
    }

    fn lookup_palette(palette: &[[u8; 4]], index: usize) -> [u8; 4] {
        if palette.is_empty() {
            return [0, 0, 0, 255];
        }
        palette.get(index).copied().unwrap_or_else(|| *palette.last().unwrap())
    }

    fn read_pixel(&mut self, bpp: u8, image_type: u8, has_alpha: bool) -> [u8; 4] {
        match bpp {
            8 => {
                let v = self.reader.read_u8().unwrap_or(0);
                [v, v, v, 255]
            }
            16 if image_type == IMAGE_TYPE_MONOCHROME => {
                let gray = self.reader.read_u8().unwrap_or(0);
                let a = self.reader.read_u8().unwrap_or(255);
                [gray, gray, gray, if has_alpha { a } else { 255 }]
            }
            15 | 16 => {
                let lo = self.reader.read_u8().unwrap_or(0);
                let hi = self.reader.read_u8().unwrap_or(0);
                let word = lo as u16 | ((hi as u16) << 8);
                let [r, g, b] = Self::rgb_from_word(word);
                let a = if has_alpha && (word & 0x8000) == 0 { 0 } else { 255 };
                [r, g, b, a]
            }
            24 => {
                let b = self.reader.read_u8().unwrap_or(0);
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                [r, g, b, 255]
            }
            32 => {
                let b = self.reader.read_u8().unwrap_or(0);
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                let a = self.reader.read_u8().unwrap_or(255);
                [r, g, b, if has_alpha { a } else { 255 }]
            }
            bpp => {
                log_warn!("Unsupported TGA BPP: {}", bpp);
                [0, 0, 0, 255]
            }
        }
    }

    fn decode_image_data(
        &mut self,
        header: &TgaHeader,
        palette: &[[u8; 4]],
        has_alpha: bool,
        is_rle: bool,
    ) -> Vec<u8> {
        let width = header.width as usize;
        let height = header.height as usize;
        let total_pixels = width * height;
        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        let is_paletted = image_type == IMAGE_TYPE_PALETTED;
        let bpp = header.bpp;

        let mut pixels = vec![0u8; total_pixels * 4];
        let mut written = 0usize;

        macro_rules! write_pixel {
            ($px:expr) => {
                if written < total_pixels {
                    let off = written * 4;
                    let px = $px;
                    pixels[off] = px[0];
                    pixels[off + 1] = px[1];
                    pixels[off + 2] = px[2];
                    pixels[off + 3] = px[3];
                    written += 1;
                }
            };
        }

        if is_rle {
            'rle: while written < total_pixels {
                let packet = match self.reader.read_u8() {
                    Ok(b) => b,
                    Err(_) => break,
                };
                let count = (packet & 0x7f) as usize + 1;

                if packet & 0x80 != 0 {
                    let px = if is_paletted {
                        let idx = self.reader.read_u8().unwrap_or(0) as usize;
                        Self::lookup_palette(palette, idx)
                    } else {
                        self.read_pixel(bpp, image_type, has_alpha)
                    };
                    for _ in 0..count {
                        write_pixel!(px);
                    }
                } else {
                    for _ in 0..count {
                        let px = if is_paletted {
                            match self.reader.read_u8() {
                                Ok(idx) => Self::lookup_palette(palette, idx as usize),
                                Err(_) => break 'rle,
                            }
                        } else {
                            self.read_pixel(bpp, image_type, has_alpha)
                        };
                        write_pixel!(px);
                    }
                }
            }
        } else {
            'raw: for _ in 0..total_pixels {
                let px = if is_paletted {
                    match self.reader.read_u8() {
                        Ok(idx) => Self::lookup_palette(palette, idx as usize),
                        Err(_) => break 'raw,
                    }
                } else {
                    self.read_pixel(bpp, image_type, has_alpha)
                };
                write_pixel!(px);
            }
        }

        pixels
    }

    fn apply_orientation(pixels: &mut [u8], width: usize, height: usize, flags: u8) {
        let flip_h = (flags & FLAG_ORIGIN_RIGHT) != 0;
        let flip_v = (flags & FLAG_ORIGIN_TOP) == 0;

        if flip_h {
            let row_bytes = width * 4;
            for y in 0..height {
                let start = y * row_bytes;
                let row = &mut pixels[start..start + row_bytes];
                let mut l = 0;
                let mut r = width.saturating_sub(1);
                while l < r {
                    row.swap(l * 4, r * 4);
                    row.swap(l * 4 + 1, r * 4 + 1);
                    row.swap(l * 4 + 2, r * 4 + 2);
                    row.swap(l * 4 + 3, r * 4 + 3);
                    l += 1;
                    r -= 1;
                }
            }
        }

        if flip_v {
            let stride = width * 4;
            let mut top = 0;
            let mut bot = height.saturating_sub(1);
            while top < bot {
                for i in 0..stride {
                    pixels.swap(top * stride + i, bot * stride + i);
                }
                top += 1;
                bot -= 1;
            }
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.sections.clear();

        let header = self.read_header()?;

        self.sections.push(TgaSectionInfo {
            start_offset: 0,
            data: TgaSectionData::Header(TgaHeaderData {
                id_length: header.id_length,
                palette_type: header.palette_type,
                image_type_raw: header.image_type_raw,
                palette_first: header.palette_first,
                palette_length: header.palette_length,
                palette_bpp: header.palette_bpp,
                x_origin: header.x_origin,
                y_origin: header.y_origin,
                width: header.width,
                height: header.height,
                bpp: header.bpp,
                flags: header.flags,
            }),
        });

        if header.id_length > 0 {
            let id_offset: u64 = 18;
            let mut id_bytes = vec![0u8; header.id_length as usize];
            self.reader.read_exact(&mut id_bytes).unwrap_or(());
            let text = String::from_utf8_lossy(&id_bytes).into_owned();
            self.sections.push(TgaSectionInfo {
                start_offset: id_offset,
                data: TgaSectionData::ImageId(TgaImageIdData {
                    length: header.id_length,
                    text,
                }),
            });
        }

        let footer_info = self.read_footer();
        let ext_alpha = footer_info.as_ref().and_then(|(footer_data, _)| {
            let ext_offset = footer_data.extension_area_offset as u64;
            if ext_offset == 0 {
                return None;
            }
            self.reader.seek(SeekFrom::Start(ext_offset + EXT_AREA_ATTR_TYPE_OFFSET)).ok()?;
            let mut attr = [0u8; 1];
            self.reader.read_exact(&mut attr).ok()?;
            match attr[0] {
                ATTR_TYPE_ALPHA => Some(ExtAlphaType::Alpha),
                ATTR_TYPE_PREMULTIPLIED_ALPHA => Some(ExtAlphaType::PremultipliedAlpha),
                _ => Some(ExtAlphaType::NoAlpha),
            }
        });

        self.reader.seek(SeekFrom::Start(18 + header.id_length as u64))?;

        if header.width == 0 || header.height == 0 {
            return Err(VexelError::InvalidDimensions {
                width: header.width as u32,
                height: header.height as u32,
            });
        }

        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        let is_rle = (header.image_type_raw & IMAGE_TYPE_FLAG_RLE) != 0;
        let alpha_bits = header.flags & FLAG_ALPHA_SIZE_MASK;

        let has_alpha = match ext_alpha {
            Some(ExtAlphaType::Alpha | ExtAlphaType::PremultipliedAlpha) => true,
            Some(ExtAlphaType::NoAlpha) => false,
            None => {
                alpha_bits != 0
                    || header.bpp == 32
                    || (image_type == IMAGE_TYPE_MONOCHROME && header.bpp == 16)
                    || (image_type == IMAGE_TYPE_PALETTED && header.palette_bpp == 32)
            }
        };

        if image_type == IMAGE_TYPE_PALETTED && header.palette_type == 1 && header.palette_length > 0 {
            let color_map_offset = self.reader.stream_position().unwrap_or(0);
            let bytes_per_entry = ((header.palette_bpp as usize + 7) / 8).max(1);
            let data_length = header.palette_length as usize * bytes_per_entry;
            self.sections.push(TgaSectionInfo {
                start_offset: color_map_offset,
                data: TgaSectionData::ColorMap(TgaColorMapData {
                    first_entry_index: header.palette_first,
                    entry_count: header.palette_length,
                    entry_size: header.palette_bpp,
                    data_length,
                }),
            });
        }

        let palette = if image_type == IMAGE_TYPE_PALETTED {
            self.read_palette(&header)
        } else {
            self.skip_palette(&header);
            Vec::new()
        };

        let pixel_data_offset = self.reader.stream_position().unwrap_or(0);
        let mut pixels = self.decode_image_data(&header, &palette, has_alpha, is_rle);
        let pixel_data_end = self.reader.stream_position().unwrap_or(pixel_data_offset);

        self.sections.push(TgaSectionInfo {
            start_offset: pixel_data_offset,
            data: TgaSectionData::PixelData(TgaPixelData {
                length: (pixel_data_end - pixel_data_offset) as usize,
            }),
        });

        if let Some((footer_data, footer_offset)) = footer_info {
            if footer_data.extension_area_offset != 0 {
                let ext_offset = footer_data.extension_area_offset as u64;
                if let Some(ext_area) = self.read_extension_area(ext_offset) {
                    self.sections.push(TgaSectionInfo {
                        start_offset: ext_offset,
                        data: TgaSectionData::ExtensionArea(ext_area),
                    });
                }
            }
            self.sections.push(TgaSectionInfo {
                start_offset: footer_offset,
                data: TgaSectionData::Footer(footer_data),
            });
        }

        Self::apply_orientation(&mut pixels, header.width as usize, header.height as usize, header.flags);

        let mut pixel_data = PixelData::RGBA8(pixels);
        pixel_data.correct_pixels(header.width as u32, header.height as u32);

        Ok(Image::from_pixels(header.width as u32, header.height as u32, pixel_data))
    }
}
