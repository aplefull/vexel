use std::io::{Read, Seek, SeekFrom};

use super::types::{
    TgaColorMapData, TgaExtensionAreaData, TgaFooterData, TgaHeader, TgaHeaderData, TgaImageIdData, TgaPixelData,
    TgaSectionData, TgaSectionInfo, ATTR_TYPE_ALPHA, ATTR_TYPE_PREMULTIPLIED_ALPHA, FLAG_ALPHA_SIZE_MASK,
    FLAG_ORIGIN_RIGHT, FLAG_ORIGIN_TOP, IMAGE_TYPE_FLAG_RLE, IMAGE_TYPE_MASK, IMAGE_TYPE_MONOCHROME,
    IMAGE_TYPE_PALETTED, TGA_FOOTER_SIZE, TGA_SIGNATURE,
};
use crate::utils::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::TgaInfo;
use crate::{log_warn, Image, Limits, PixelData};

pub struct TgaDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    limits: Limits,
    sections: Vec<TgaSectionInfo>,
}

impl<R: Read + Seek> TgaDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader: BitReader::with_le(reader), limits: Limits::default(), sections: Vec::new() }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> TgaInfo {
        TgaInfo { sections: self.sections.clone() }
    }

    pub fn decode_thumbnails(&mut self) -> VexelResult<Vec<Image>> {
        let saved_pos = self.reader.stream_position().unwrap_or(0);
        let result = self.decode_postage_stamp();
        let _ = self.reader.seek(SeekFrom::Start(saved_pos));
        Ok(result.into_iter().collect())
    }

    fn decode_postage_stamp(&mut self) -> Option<Image> {
        self.reader.seek(SeekFrom::Start(0)).ok()?;
        let mut header = self.read_header().ok()?;
        Self::normalize_header(&mut header);

        let (footer, _) = self.read_footer()?;
        if footer.extension_area_offset == 0 {
            return None;
        }

        let ext = self.read_extension_area(footer.extension_area_offset as u64)?;
        if ext.postage_stamp_offset == 0 {
            return None;
        }

        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        let has_alpha = matches!(ext.attributes_type, ATTR_TYPE_ALPHA | ATTR_TYPE_PREMULTIPLIED_ALPHA);
        let premultiplied = ext.attributes_type == ATTR_TYPE_PREMULTIPLIED_ALPHA;

        let palette = if image_type == IMAGE_TYPE_PALETTED && header.palette_type == 1 && header.palette_length > 0 {
            self.reader.seek(SeekFrom::Start(18 + header.id_length as u64)).ok()?;
            self.read_palette(&header, has_alpha)
        } else {
            Vec::new()
        };

        self.reader.seek(SeekFrom::Start(ext.postage_stamp_offset as u64)).ok()?;
        let width = self.reader.read_u8().ok()?;
        let height = self.reader.read_u8().ok()?;
        if width == 0 || height == 0 {
            return None;
        }

        self.limits.reserve_buffer(width as u32, height as u32, 4).ok()?;

        let mut stamp_header = header.clone();
        stamp_header.width = width as u16;
        stamp_header.height = height as u16;

        let pixels = self.decode_image_data(&stamp_header, &palette, has_alpha, false);
        Some(Self::build_image(pixels, width as u32, height as u32, header.flags, premultiplied))
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

    fn normalize_header(header: &mut TgaHeader) {
        let bpp = match header.bpp {
            8 | 15 | 16 | 24 | 32 => header.bpp,
            0..=11 => 8,
            12..=19 => 16,
            20..=27 => 24,
            _ => 32,
        };
        if bpp != header.bpp {
            log_warn!("Invalid TGA pixel depth {}, treating it as {}", header.bpp, bpp);
            header.bpp = bpp;
        }

        if header.palette_length > 0 {
            let palette_bpp = match header.palette_bpp {
                15 | 16 | 24 | 32 => header.palette_bpp,
                0..=19 => 16,
                20..=27 => 24,
                _ => 32,
            };
            if palette_bpp != header.palette_bpp {
                log_warn!("Invalid TGA palette entry size {}, treating it as {}", header.palette_bpp, palette_bpp);
                header.palette_bpp = palette_bpp;
            }
        }
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
                let [r, g, b] = Self::rgb_from_word(lo as u16 | ((hi as u16) << 8));
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
            _ => [0, 0, 0, 255],
        }
    }

    fn read_palette(&mut self, header: &TgaHeader, has_alpha: bool) -> Vec<[u8; 4]> {
        let has_alpha = has_alpha && header.palette_bpp == 32;
        (0..header.palette_length).map(|_| self.read_palette_entry(header.palette_bpp, has_alpha)).collect()
    }

    fn lookup_palette(palette: &[[u8; 4]], index: usize) -> [u8; 4] {
        palette.get(index).or(palette.last()).copied().unwrap_or([0, 0, 0, 255])
    }

    fn read_pixel(&mut self, bpp: u8, image_type: u8, has_alpha: bool) -> Option<[u8; 4]> {
        let b0 = self.reader.read_u8().ok()?;
        Some(match bpp {
            8 => [b0, b0, b0, 255],
            16 if image_type == IMAGE_TYPE_MONOCHROME => {
                let a = self.reader.read_u8().unwrap_or(255);
                [b0, b0, b0, if has_alpha { a } else { 255 }]
            }
            15 | 16 => {
                let word = b0 as u16 | ((self.reader.read_u8().unwrap_or(0) as u16) << 8);
                let [r, g, b] = Self::rgb_from_word(word);
                let a = if has_alpha && (word & 0x8000) == 0 { 0 } else { 255 };
                [r, g, b, a]
            }
            24 => {
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                [r, g, b0, 255]
            }
            32 => {
                let g = self.reader.read_u8().unwrap_or(0);
                let r = self.reader.read_u8().unwrap_or(0);
                let a = self.reader.read_u8().unwrap_or(255);
                [r, g, b0, if has_alpha { a } else { 255 }]
            }
            _ => [b0, b0, b0, 255],
        })
    }

    fn read_source_pixel(
        &mut self,
        bpp: u8,
        image_type: u8,
        has_alpha: bool,
        palette: &[[u8; 4]],
        palette_first: usize,
    ) -> Option<[u8; 4]> {
        if image_type != IMAGE_TYPE_PALETTED {
            return self.read_pixel(bpp, image_type, has_alpha);
        }

        let index_bytes = (bpp as usize).div_ceil(8).clamp(1, 4);
        let mut index = self.reader.read_u8().ok()? as usize;
        for i in 1..index_bytes {
            index |= (self.reader.read_u8().unwrap_or(0) as usize) << (8 * i);
        }

        Some(Self::lookup_palette(palette, index.saturating_sub(palette_first)))
    }

    fn decode_image_data(&mut self, header: &TgaHeader, palette: &[[u8; 4]], has_alpha: bool, is_rle: bool) -> Vec<u8> {
        let total_pixels = header.width as usize * header.height as usize;
        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        let palette_first = header.palette_first as usize;
        let bpp = header.bpp;

        let mut pixels = vec![0u8; total_pixels * 4];
        let mut written = 0usize;

        if is_rle {
            'packets: while written < total_pixels {
                let Ok(packet) = self.reader.read_u8() else {
                    break;
                };
                let count = ((packet & 0x7f) as usize + 1).min(total_pixels - written);

                if packet & 0x80 != 0 {
                    let Some(px) = self.read_source_pixel(bpp, image_type, has_alpha, palette, palette_first) else {
                        break;
                    };
                    for chunk in pixels[written * 4..(written + count) * 4].chunks_exact_mut(4) {
                        chunk.copy_from_slice(&px);
                    }
                    written += count;
                } else {
                    for _ in 0..count {
                        let Some(px) = self.read_source_pixel(bpp, image_type, has_alpha, palette, palette_first)
                        else {
                            break 'packets;
                        };
                        pixels[written * 4..written * 4 + 4].copy_from_slice(&px);
                        written += 1;
                    }
                }
            }
        } else {
            while written < total_pixels {
                let Some(px) = self.read_source_pixel(bpp, image_type, has_alpha, palette, palette_first) else {
                    break;
                };
                pixels[written * 4..written * 4 + 4].copy_from_slice(&px);
                written += 1;
            }
        }

        if written < total_pixels {
            log_warn!("TGA image data truncated, expected {} pixels but decoded {}", total_pixels, written);
            for chunk in pixels[written * 4..].chunks_exact_mut(4) {
                chunk[3] = 255;
            }
        }

        pixels
    }

    fn unpremultiply(pixels: &mut [u8]) {
        let (chunks, _) = pixels.as_chunks_mut::<4>();
        for px in chunks {
            let a = px[3] as u16;
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u16 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
    }

    fn apply_orientation(pixels: &mut [u8], width: usize, height: usize, flags: u8) {
        let stride = width * 4;
        if stride == 0 || height == 0 {
            return;
        }

        if (flags & FLAG_ORIGIN_RIGHT) != 0 {
            let (rows, _) = pixels.as_chunks_mut::<4>();
            for row in rows.chunks_exact_mut(width) {
                row.reverse();
            }
        }

        if (flags & FLAG_ORIGIN_TOP) == 0 {
            let (mut top, mut bottom) = (0, height - 1);
            while top < bottom {
                let (upper, lower) = pixels.split_at_mut(bottom * stride);
                upper[top * stride..(top + 1) * stride].swap_with_slice(&mut lower[..stride]);
                top += 1;
                bottom -= 1;
            }
        }
    }

    fn build_image(mut pixels: Vec<u8>, width: u32, height: u32, flags: u8, premultiplied: bool) -> Image {
        if premultiplied {
            Self::unpremultiply(&mut pixels);
        }
        Self::apply_orientation(&mut pixels, width as usize, height as usize, flags);
        Image::from_pixels(width, height, PixelData::RGBA8(pixels))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.sections.clear();

        let mut header = self.read_header()?;
        self.sections
            .push(TgaSectionInfo { start_offset: 0, data: TgaSectionData::Header(TgaHeaderData::from(&header)) });

        if header.id_length > 0 {
            let mut id_bytes = vec![0u8; header.id_length as usize];
            self.reader.read_exact(&mut id_bytes).unwrap_or(());
            let text = String::from_utf8_lossy(&id_bytes).into_owned();
            self.sections.push(TgaSectionInfo {
                start_offset: 18,
                data: TgaSectionData::ImageId(TgaImageIdData { length: header.id_length, text }),
            });
        }

        let footer_info = self.read_footer();
        let ext_area = footer_info.as_ref().and_then(|(footer_data, _)| match footer_data.extension_area_offset {
            0 => None,
            offset => self.read_extension_area(offset as u64),
        });
        let ext_attr = ext_area.as_ref().map(|ext| ext.attributes_type);

        self.reader.seek(SeekFrom::Start(18 + header.id_length as u64))?;

        if header.width == 0 || header.height == 0 {
            return Err(VexelError::InvalidDimensions { width: header.width as u32, height: header.height as u32 });
        }

        self.limits.reserve_buffer(header.width as u32, header.height as u32, 4)?;

        Self::normalize_header(&mut header);

        if header.image_type_raw == 0 {
            log_warn!("TGA image type is 0 (no image data), attempting to decode as true-color anyway");
        }

        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        let is_paletted = image_type == IMAGE_TYPE_PALETTED;
        let is_rle = (header.image_type_raw & IMAGE_TYPE_FLAG_RLE) != 0;
        let alpha_bits = header.flags & FLAG_ALPHA_SIZE_MASK;

        let has_alpha = match ext_attr {
            Some(ATTR_TYPE_ALPHA | ATTR_TYPE_PREMULTIPLIED_ALPHA) => true,
            Some(_) => false,
            None => {
                alpha_bits != 0
                    || header.bpp == 32
                    || (image_type == IMAGE_TYPE_MONOCHROME && header.bpp == 16)
                    || (is_paletted && header.palette_bpp == 32)
            }
        };
        let premultiplied = ext_attr == Some(ATTR_TYPE_PREMULTIPLIED_ALPHA);

        let mut palette = Vec::new();
        if header.palette_length > 0 && (is_paletted || header.palette_type != 0) {
            let bytes_per_entry = (header.palette_bpp as usize).div_ceil(8).max(1);
            self.sections.push(TgaSectionInfo {
                start_offset: self.reader.stream_position().unwrap_or(0),
                data: TgaSectionData::ColorMap(TgaColorMapData {
                    first_entry_index: header.palette_first,
                    entry_count: header.palette_length,
                    entry_size: header.palette_bpp,
                    data_length: header.palette_length as usize * bytes_per_entry,
                }),
            });

            if is_paletted {
                palette = self.read_palette(&header, has_alpha);
            } else {
                let _ = self.reader.seek(SeekFrom::Current((header.palette_length as usize * bytes_per_entry) as i64));
            }
        }

        let pixel_data_offset = self.reader.stream_position().unwrap_or(0);
        let pixels = self.decode_image_data(&header, &palette, has_alpha, is_rle);
        let pixel_data_end = self.reader.stream_position().unwrap_or(pixel_data_offset);

        self.sections.push(TgaSectionInfo {
            start_offset: pixel_data_offset,
            data: TgaSectionData::PixelData(TgaPixelData {
                length: pixel_data_end.saturating_sub(pixel_data_offset) as usize,
            }),
        });

        if let Some((footer_data, footer_offset)) = footer_info {
            if let Some(ext_area) = ext_area {
                self.sections.push(TgaSectionInfo {
                    start_offset: footer_data.extension_area_offset as u64,
                    data: TgaSectionData::ExtensionArea(ext_area),
                });
            }
            self.sections
                .push(TgaSectionInfo { start_offset: footer_offset, data: TgaSectionData::Footer(footer_data) });
        }

        Ok(Self::build_image(pixels, header.width as u32, header.height as u32, header.flags, premultiplied))
    }
}
