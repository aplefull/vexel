use std::io::{Read, Seek, SeekFrom};

use super::info::{
    TgaColorMap, TgaDeveloperDirectory, TgaDeveloperTag, TgaImageId, TgaInfo, TgaPixelData, TgaPostageStamp,
    TgaScanLineTable, TgaSection,
};
use super::types::{
    TgaExtensionAreaData, TgaFooterData, TgaHeader, ATTR_TYPE_ALPHA, ATTR_TYPE_PREMULTIPLIED_ALPHA,
    FLAG_ALPHA_SIZE_MASK, FLAG_ORIGIN_RIGHT, FLAG_ORIGIN_TOP, IMAGE_TYPE_FLAG_RLE, IMAGE_TYPE_MASK,
    IMAGE_TYPE_MONOCHROME, IMAGE_TYPE_PALETTED, TGA_FOOTER_SIZE, TGA_SIGNATURE,
};
use crate::utils::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::info::{ColorModel, DataExtent, ImageSummary};
use crate::{log_warn, Image, Limits, PixelData};

const HEADER_SIZE: u64 = 18;
const DEVELOPER_TAG_SIZE: u64 = 10;
const COLOR_CORRECTION_TABLE_SIZE: u64 = 256 * 4 * 2;

struct TgaStructure {
    header: TgaHeader,
    ext_area: Option<TgaExtensionAreaData>,
}

pub struct TgaDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    limits: Limits,
    info: Option<TgaInfo>,
}

impl<R: Read + Seek> TgaDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader: BitReader::with_le(reader), limits: Limits::default(), info: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> TgaInfo {
        if self.info.is_none() {
            match self.scan() {
                Ok(TgaStructure { header, .. }) => self.measure_rle_pixel_data(&header),
                Err(e) => log_warn!("Failed to read TGA structure: {}", e),
            }
        }

        self.info.clone().unwrap_or_default()
    }

    fn measure_rle_pixel_data(&mut self, header: &TgaHeader) {
        if header.image_type_raw & IMAGE_TYPE_FLAG_RLE == 0 || header.width == 0 || header.height == 0 {
            return;
        }

        let Some(offset) = self.info.as_ref().and_then(|info| {
            info.sections.iter().find(|section| matches!(section.data, TgaSection::PixelData(_))).map(|s| s.offset)
        }) else {
            return;
        };

        let total_pixels = u64::from(header.width) * u64::from(header.height);
        let bytes_per_pixel = u64::from(Self::normalized_bpp(header.bpp)).div_ceil(8);
        let length = self.rle_data_length(offset, total_pixels, bytes_per_pixel);

        if let Some(info) = self.info.as_mut() {
            for section in &mut info.sections {
                if matches!(section.data, TgaSection::PixelData(_)) {
                    section.length = Some(length);
                }
            }
        }
    }

    fn rle_data_length(&mut self, offset: u64, total_pixels: u64, bytes_per_pixel: u64) -> u64 {
        if self.reader.seek(SeekFrom::Start(offset)).is_err() {
            return 0;
        }

        let mut scratch = [0u8; 128 * 4];
        let mut written = 0u64;
        while written < total_pixels {
            let Ok(packet) = self.reader.read_u8() else {
                break;
            };
            let count = (u64::from(packet & 0x7f) + 1).min(total_pixels - written);
            let stored_pixels = if packet & 0x80 != 0 { 1 } else { count };
            let bytes = (stored_pixels * bytes_per_pixel) as usize;
            if self.reader.read_exact(&mut scratch[..bytes]).is_err() {
                break;
            }
            written += count;
        }

        self.reader.stream_position().map_or(0, |end| end.saturating_sub(offset))
    }

    fn normalized_bpp(bpp: u8) -> u8 {
        match bpp {
            8 | 15 | 16 | 24 | 32 => bpp,
            0..=11 => 8,
            12..=19 => 16,
            20..=27 => 24,
            _ => 32,
        }
    }

    fn normalized_palette_bpp(bpp: u8) -> u8 {
        match bpp {
            15 | 16 | 24 | 32 => bpp,
            0..=19 => 16,
            20..=27 => 24,
            _ => 32,
        }
    }

    fn detect_alpha(header: &TgaHeader, ext_attr: Option<u8>) -> bool {
        let image_type = header.image_type_raw & IMAGE_TYPE_MASK;
        match ext_attr {
            Some(ATTR_TYPE_ALPHA | ATTR_TYPE_PREMULTIPLIED_ALPHA) => true,
            Some(_) => false,
            None => {
                (header.flags & FLAG_ALPHA_SIZE_MASK) != 0
                    || header.bpp == 32
                    || (image_type == IMAGE_TYPE_MONOCHROME && header.bpp == 16)
                    || (image_type == IMAGE_TYPE_PALETTED && header.palette_bpp == 32)
            }
        }
    }

    fn has_color_map(header: &TgaHeader) -> bool {
        header.palette_length > 0
            && ((header.image_type_raw & IMAGE_TYPE_MASK) == IMAGE_TYPE_PALETTED || header.palette_type != 0)
    }

    fn color_map_length(header: &TgaHeader) -> u64 {
        let bytes_per_entry = (Self::normalized_palette_bpp(header.palette_bpp) as u64).div_ceil(8).max(1);
        u64::from(header.palette_length) * bytes_per_entry
    }

    fn build_summary(
        header: &TgaHeader,
        ext_attr: Option<u8>,
        pixel_data: &TgaPixelData,
        actual: Option<u64>,
    ) -> ImageSummary {
        let bpp = Self::normalized_bpp(header.bpp);
        let mut normalized = header.clone();
        normalized.bpp = bpp;
        if normalized.palette_length > 0 {
            normalized.palette_bpp = Self::normalized_palette_bpp(normalized.palette_bpp);
        }
        let has_alpha = Self::detect_alpha(&normalized, ext_attr);
        let (color_model, channels, bits) = match header.image_type_raw & IMAGE_TYPE_MASK {
            IMAGE_TYPE_PALETTED => (ColorModel::Indexed, 1, bpp),
            2 => {
                let bits = if bpp <= 16 { 5 } else { 8 };
                if has_alpha {
                    (ColorModel::Rgba, 4, bits)
                } else {
                    (ColorModel::Rgb, 3, bits)
                }
            }
            IMAGE_TYPE_MONOCHROME if has_alpha => (ColorModel::GrayAlpha, 2, 8),
            IMAGE_TYPE_MONOCHROME => (ColorModel::Gray, 1, 8),
            _ => (ColorModel::Unknown, 0, bpp),
        };

        let mut summary =
            ImageSummary::new(u32::from(header.width), u32::from(header.height), color_model, channels, bits);
        summary.has_alpha = has_alpha;
        summary.compression = pixel_data.rle.then(|| "RLE".to_string());
        if let (Some(expected), Some(actual)) = (pixel_data.expected_length, actual) {
            summary.data = Some(DataExtent { expected, actual });
        }
        summary
    }

    fn scan(&mut self) -> VexelResult<TgaStructure> {
        self.reader.seek(SeekFrom::Start(0))?;
        let file_size = self.reader.bytes_left().unwrap_or(0);
        let mut info = TgaInfo::default();

        let header = match self.read_header() {
            Ok(header) => header,
            Err(e) => {
                self.info = Some(info);
                return Err(e);
            }
        };
        info.push(0, Some(HEADER_SIZE), TgaSection::Header(header.clone()));

        let mut offset = HEADER_SIZE;
        if header.id_length > 0 {
            let mut id_bytes = vec![0u8; header.id_length as usize];
            self.reader.read_exact(&mut id_bytes).unwrap_or(());
            let text = String::from_utf8_lossy(&id_bytes).into_owned();
            info.push(offset, Some(u64::from(header.id_length)), TgaSection::ImageId(TgaImageId { text }));
            offset += u64::from(header.id_length);
        }

        if Self::has_color_map(&header) {
            let length = Self::color_map_length(&header);
            info.push(
                offset,
                Some(length),
                TgaSection::ColorMap(TgaColorMap {
                    first_entry_index: header.palette_first,
                    entry_count: header.palette_length,
                    entry_size: header.palette_bpp,
                    used: (header.image_type_raw & IMAGE_TYPE_MASK) == IMAGE_TYPE_PALETTED,
                }),
            );
            offset += length;
        }

        let rle = (header.image_type_raw & IMAGE_TYPE_FLAG_RLE) != 0;
        let bytes_per_pixel = u64::from(Self::normalized_bpp(header.bpp)).div_ceil(8);
        let expected_length = (!rle).then(|| u64::from(header.width) * u64::from(header.height) * bytes_per_pixel);
        let actual = expected_length.map(|expected| expected.min(file_size.saturating_sub(offset)));
        let pixel_data = TgaPixelData { rle, expected_length };

        let footer_info = self.read_footer();
        let ext_area = footer_info.as_ref().and_then(|(footer, _)| match footer.extension_area_offset {
            0 => None,
            ext_offset => self.read_extension_area(ext_offset as u64),
        });

        info.summary =
            Some(Self::build_summary(&header, ext_area.as_ref().map(|ext| ext.attributes_type), &pixel_data, actual));
        info.push(offset, actual, TgaSection::PixelData(pixel_data));

        if let Some((footer, footer_offset)) = footer_info {
            if let Some(ext) = &ext_area {
                info.push(
                    footer.extension_area_offset as u64,
                    Some(u64::from(ext.extension_size)),
                    TgaSection::ExtensionArea(ext.clone()),
                );
                self.push_extension_tables(&mut info, &header, ext);
            }
            if footer.developer_dir_offset != 0 {
                self.push_developer_directory(&mut info, footer.developer_dir_offset as u64);
            }
            info.push(footer_offset, Some(TGA_FOOTER_SIZE as u64), TgaSection::Footer(footer));
        }

        info.sections.sort_by_key(|section| section.offset);
        self.info = Some(info);

        Ok(TgaStructure { header, ext_area })
    }

    fn push_extension_tables(&mut self, info: &mut TgaInfo, header: &TgaHeader, ext: &TgaExtensionAreaData) {
        if ext.color_correction_offset != 0 {
            info.push(
                ext.color_correction_offset as u64,
                Some(COLOR_CORRECTION_TABLE_SIZE),
                TgaSection::ColorCorrectionTable,
            );
        }

        if ext.scan_line_offset != 0 {
            let entries = u32::from(header.height);
            info.push(
                ext.scan_line_offset as u64,
                Some(u64::from(entries) * 4),
                TgaSection::ScanLineTable(TgaScanLineTable { entries }),
            );
        }

        if ext.postage_stamp_offset != 0 && self.reader.seek(SeekFrom::Start(ext.postage_stamp_offset as u64)).is_ok() {
            if let (Ok(width), Ok(height)) = (self.reader.read_u8(), self.reader.read_u8()) {
                let bytes_per_pixel = u64::from(Self::normalized_bpp(header.bpp)).div_ceil(8);
                let length = 2 + u64::from(width) * u64::from(height) * bytes_per_pixel;
                info.push(
                    ext.postage_stamp_offset as u64,
                    Some(length),
                    TgaSection::PostageStamp(TgaPostageStamp { width, height }),
                );
            }
        }
    }

    fn push_developer_directory(&mut self, info: &mut TgaInfo, offset: u64) {
        if self.reader.seek(SeekFrom::Start(offset)).is_err() {
            return;
        }
        let Ok(declared_count) = self.reader.read_u16() else {
            return;
        };

        let available = self.reader.bytes_left().unwrap_or(0) / DEVELOPER_TAG_SIZE;
        let count = u64::from(declared_count).min(available);
        if count < u64::from(declared_count) {
            log_warn!("TGA developer directory declares {} entries, only {} fit", declared_count, count);
        }

        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (Ok(tag), Ok(tag_offset), Ok(size)) =
                (self.reader.read_u16(), self.reader.read_u32(), self.reader.read_u32())
            else {
                break;
            };
            entries.push(TgaDeveloperTag { tag, offset: tag_offset, size });
        }

        let length = 2 + entries.len() as u64 * DEVELOPER_TAG_SIZE;
        info.push(
            offset,
            Some(length),
            TgaSection::DeveloperDirectory(TgaDeveloperDirectory { declared_count, entries }),
        );
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
        let bpp = Self::normalized_bpp(header.bpp);
        if bpp != header.bpp {
            log_warn!("Invalid TGA pixel depth {}, treating it as {}", header.bpp, bpp);
            header.bpp = bpp;
        }

        if header.palette_length > 0 {
            let palette_bpp = Self::normalized_palette_bpp(header.palette_bpp);
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
            author_comments: (0..4)
                .map(|line| read_str(43 + line * 81, 81))
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
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
        let TgaStructure { mut header, ext_area } = self.scan()?;
        let ext_attr = ext_area.as_ref().map(|ext| ext.attributes_type);

        self.reader.seek(SeekFrom::Start(HEADER_SIZE + header.id_length as u64))?;

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
        let has_alpha = Self::detect_alpha(&header, ext_attr);
        let premultiplied = ext_attr == Some(ATTR_TYPE_PREMULTIPLIED_ALPHA);

        let mut palette = Vec::new();
        if Self::has_color_map(&header) {
            if is_paletted {
                palette = self.read_palette(&header, has_alpha);
            } else {
                let _ = self.reader.seek(SeekFrom::Current(Self::color_map_length(&header) as i64));
            }
        }

        let pixel_data_offset = self.reader.stream_position().unwrap_or(0);
        let pixels = self.decode_image_data(&header, &palette, has_alpha, is_rle);
        let pixel_data_end = self.reader.stream_position().unwrap_or(pixel_data_offset);

        if let Some(info) = self.info.as_mut() {
            for section in &mut info.sections {
                if matches!(section.data, TgaSection::PixelData(_)) {
                    section.length = Some(pixel_data_end.saturating_sub(pixel_data_offset));
                }
            }
        }

        Ok(Self::build_image(pixels, header.width as u32, header.height as u32, header.flags, premultiplied))
    }
}
