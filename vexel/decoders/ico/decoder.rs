use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::bmp::decoder::BmpDecoder;
use crate::decoders::bmp::headers::HeaderReader;
use crate::decoders::bmp::types::{BitmapCompression, DibKind};
use crate::decoders::ico::info::{
    EmbeddedBmp, EmbeddedImage, IcoDirectory, IcoDirectoryEntry, IcoImageData, IcoInfo, IcoSection,
};
use crate::decoders::ico::types::{IcoImageFormat, IcoType, IconDirEntry};
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, ImageFrame, PixelData};
use crate::utils::info::{ColorModel, ImageSummary};
use crate::{log_error, log_warn, Limits};

const DIRECTORY_SIZE: u64 = 6;
const ENTRY_SIZE: u64 = 16;
const MAX_DETAILED_IMAGES: usize = 256;

pub struct IcoDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    ico_type: IcoType,
    type_raw: u16,
    reserved: u16,
    declared_count: u16,
    entries: Vec<IconDirEntry>,
    entry_offsets: Vec<u64>,
    info: Option<IcoInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> IcoDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            ico_type: IcoType::Ico,
            type_raw: 0,
            reserved: 0,
            declared_count: 0,
            entries: Vec::new(),
            entry_offsets: Vec::new(),
            info: None,
            reader: BitReader::with_le(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> IcoInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.read_directory() {
                log_warn!("Failed to read ICO directory: {}", e);
            }
            self.info = Some(self.build_info());
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    fn build_info(&mut self) -> IcoInfo {
        let mut info = IcoInfo::default();
        if self.type_raw == 0 && self.entries.is_empty() && self.declared_count == 0 {
            return info;
        }

        let ico_type = match self.type_raw {
            1 => Some(IcoType::Ico),
            2 => Some(IcoType::Cur),
            _ => None,
        };
        info.push(
            0,
            Some(DIRECTORY_SIZE),
            IcoSection::Directory(IcoDirectory {
                reserved: self.reserved,
                type_raw: self.type_raw,
                ico_type,
                image_count: self.declared_count,
            }),
        );

        let entries = self.entries.clone();
        let is_cursor = self.ico_type == IcoType::Cur;
        for (index, (entry, &offset)) in entries.iter().zip(self.entry_offsets.clone().iter()).enumerate() {
            info.push(
                offset,
                Some(ENTRY_SIZE),
                IcoSection::DirectoryEntry(IcoDirectoryEntry {
                    index: index as u32,
                    width: entry.width,
                    height: entry.height,
                    color_count: entry.color_count,
                    reserved: entry.reserved,
                    planes: (!is_cursor).then_some(entry.planes),
                    bit_count: (!is_cursor).then_some(entry.bit_count),
                    hotspot: is_cursor.then_some([entry.hotspot_x, entry.hotspot_y]),
                    bytes_in_res: entry.bytes_in_res,
                    image_offset: entry.image_offset,
                }),
            );
        }

        let file_length = self.reader.seek(SeekFrom::End(0)).unwrap_or(0);
        let mut image_offsets: Vec<u64> = entries.iter().map(|entry| u64::from(entry.image_offset)).collect();
        image_offsets.sort_unstable();
        image_offsets.dedup();
        if entries.len() > MAX_DETAILED_IMAGES {
            log_warn!("ICO has {} images, inspecting only the first {}", entries.len(), MAX_DETAILED_IMAGES);
        }

        let mut largest: Option<(u64, ImageSummary)> = None;
        let mut inspected: HashMap<u64, (u32, u64)> = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            if index >= MAX_DETAILED_IMAGES {
                let start = u64::from(entry.image_offset);
                let next = image_offsets.get(image_offsets.partition_point(|&offset| offset <= start)).copied();
                let span = next.unwrap_or(file_length).min(file_length).saturating_sub(start);
                info.push(
                    start,
                    Some(span),
                    IcoSection::ImageData(IcoImageData {
                        index: index as u32,
                        image_format: entry.image_format,
                        declared_length: entry.bytes_in_res,
                        directory_bit_count: (!is_cursor).then_some(entry.bit_count),
                        embedded: None,
                        same_data_as: None,
                    }),
                );
                continue;
            }

            let start = u64::from(entry.image_offset);
            if let Some(&(first_index, length)) = inspected.get(&start) {
                info.push(
                    start,
                    Some(length),
                    IcoSection::ImageData(IcoImageData {
                        index: index as u32,
                        image_format: entry.image_format,
                        declared_length: entry.bytes_in_res,
                        directory_bit_count: (!is_cursor).then_some(entry.bit_count),
                        embedded: None,
                        same_data_as: Some(first_index),
                    }),
                );
                continue;
            }

            let bytes = match self.read_entry_bytes(entry) {
                Ok(bytes) => bytes,
                Err(e) => {
                    log_warn!("Failed to read ICO image {}: {}", index + 1, e);
                    Vec::new()
                }
            };

            let (embedded, summary) = match entry.image_format {
                IcoImageFormat::Png => {
                    let mut png = PngDecoder::new(Cursor::new(bytes.clone()));
                    let mut png_info = png.get_info();
                    png_info.shift_offsets(u64::from(entry.image_offset));
                    let summary = png_info.summary.clone();
                    (Some(EmbeddedImage::Png(png_info)), summary)
                }
                IcoImageFormat::Bmp => {
                    let bmp = Self::inspect_bmp(&bytes);
                    let summary = Self::bmp_summary(&bmp, entry);
                    (Some(EmbeddedImage::Bmp(bmp)), Some(summary))
                }
            };

            let area = u64::from(entry.width) * u64::from(entry.height);
            if let Some(summary) = summary {
                if largest.as_ref().is_none_or(|(best, _)| area > *best) {
                    largest = Some((area, summary));
                }
            }

            inspected.insert(start, (index as u32, bytes.len() as u64));
            info.push(
                start,
                Some(bytes.len() as u64),
                IcoSection::ImageData(IcoImageData {
                    index: index as u32,
                    image_format: entry.image_format,
                    declared_length: entry.bytes_in_res,
                    directory_bit_count: (!is_cursor).then_some(entry.bit_count),
                    embedded,
                    same_data_as: None,
                }),
            );
        }

        if let Some((_, mut summary)) = largest {
            summary.pages = Some(entries.len() as u32);
            info.summary = Some(summary);
        }

        info.sections.sort_by_key(|section| section.offset);
        info
    }

    fn read_entry_bytes(&mut self, entry: &IconDirEntry) -> VexelResult<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(entry.image_offset as u64))?;
        let bytes_available = self.reader.bytes_left().unwrap_or(0);
        let bytes_to_read = self.max_readable_bytes(entry, bytes_available) as usize;
        Ok(self.reader.read_bytes(bytes_to_read)?)
    }

    fn inspect_bmp(bytes: &[u8]) -> EmbeddedBmp {
        let mut reader = BitReader::with_le(Cursor::new(bytes));
        let dib = HeaderReader::read_info_header(&mut reader).ok().map(|(dib, _, _)| dib);

        let Some(header) = &dib else {
            return EmbeddedBmp {
                dib: None,
                color_table_entries: 0,
                xor_length: 0,
                and_mask_length: 0,
                and_mask_present: false,
            };
        };

        let bpp = u64::from(header.bits_per_pixel);
        let color_table_entries = if bpp <= 8 {
            if header.colors_used > 0 {
                header.colors_used.min(1 << bpp)
            } else {
                1 << bpp
            }
        } else {
            0
        };
        let bytes_per_color = if header.kind == DibKind::Core { 3 } else { 4 };
        let mask_bytes = match header.compression {
            BitmapCompression::BiBitfields if header.kind == DibKind::Info => 12,
            BitmapCompression::BiAlphaBitfields if header.kind == DibKind::Info => 16,
            _ => 0,
        };

        let width = u64::from(header.width.unsigned_abs());
        let height = u64::from(header.height.unsigned_abs() / 2);
        let xor_length = (bpp * width).div_ceil(32).saturating_mul(4).saturating_mul(height);
        let and_mask_length = width.div_ceil(32).saturating_mul(4).saturating_mul(height);
        let and_start = (u64::from(header.header_size) + mask_bytes + u64::from(color_table_entries) * bytes_per_color)
            .saturating_add(xor_length);
        let and_mask_present = (bytes.len() as u64) > and_start;

        EmbeddedBmp { dib, color_table_entries, xor_length, and_mask_length, and_mask_present }
    }

    fn bmp_summary(bmp: &EmbeddedBmp, entry: &IconDirEntry) -> ImageSummary {
        let bpp = bmp.dib.as_ref().map(|dib| dib.bits_per_pixel).unwrap_or(entry.bit_count);
        let (color_model, channels, bits) = match bpp {
            0..=8 => (ColorModel::Indexed, 1, bpp as u8),
            16 => (ColorModel::Rgb, 3, 5),
            32 => (ColorModel::Rgba, 4, 8),
            _ => (ColorModel::Rgb, 3, 8),
        };
        let mut summary = ImageSummary::new(entry.width, entry.height, color_model, channels, bits);
        summary.has_alpha = bpp == 32 || bmp.and_mask_present;
        summary
    }

    fn read_header(&mut self) -> VexelResult<u16> {
        let reserved = self.reader.read_u16()?;
        self.reserved = reserved;
        if reserved != 0 {
            log_warn!("ICO reserved field is non-zero: {}", reserved);
        }

        let type_val = self.reader.read_u16()?;
        self.type_raw = type_val;
        self.ico_type = match type_val {
            1 => IcoType::Ico,
            2 => IcoType::Cur,
            _ => {
                log_warn!("Unknown ICO type: {}, treating as ICO", type_val);
                IcoType::Ico
            }
        };

        let count = self.reader.read_u16()?;
        self.declared_count = count;

        Ok(count)
    }

    fn read_entries(&mut self, count: u16) -> VexelResult<()> {
        for _ in 0..count {
            let entry_offset = self.reader.stream_position().unwrap_or(0);

            let width_byte = self.reader.read_u8()?;
            let height_byte = self.reader.read_u8()?;
            let color_count = self.reader.read_u8()?;
            let reserved = self.reader.read_u8()?;
            let planes_or_hotspot_x = self.reader.read_u16()?;
            let bit_count_or_hotspot_y = self.reader.read_u16()?;
            let bytes_in_res = self.reader.read_u32()?;
            let image_offset = self.reader.read_u32()?;

            let width = if width_byte == 0 { 256 } else { width_byte as u32 };
            let height = if height_byte == 0 { 256 } else { height_byte as u32 };

            let (hotspot_x, hotspot_y, planes, bit_count) = match self.ico_type {
                IcoType::Cur => (planes_or_hotspot_x, bit_count_or_hotspot_y, 0, 0),
                IcoType::Ico => (0, 0, planes_or_hotspot_x, bit_count_or_hotspot_y),
            };

            self.entries.push(IconDirEntry {
                reserved,
                width,
                height,
                color_count,
                planes,
                bit_count,
                bytes_in_res,
                image_offset,
                hotspot_x,
                hotspot_y,
                image_format: IcoImageFormat::Bmp,
            });

            self.entry_offsets.push(entry_offset);
        }

        Ok(())
    }

    fn detect_image_formats(&mut self) -> VexelResult<()> {
        let png_sig = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

        let offsets: Vec<(usize, u64)> =
            self.entries.iter().enumerate().map(|(i, e)| (i, e.image_offset as u64)).collect();

        for (idx, offset) in offsets {
            self.reader.seek(SeekFrom::Start(offset))?;
            match self.reader.read_bytes(8) {
                Ok(magic) if magic.len() == 8 => {
                    if magic.as_slice() == png_sig {
                        self.entries[idx].image_format = IcoImageFormat::Png;
                    }
                }
                _ => continue,
            }
        }

        Ok(())
    }

    fn max_readable_bytes(&self, entry: &IconDirEntry, bytes_available: u64) -> u64 {
        let next_entry_offset = self
            .entries
            .iter()
            .map(|e| e.image_offset as u64)
            .filter(|&offset| offset > entry.image_offset as u64)
            .min();

        let bound = match next_entry_offset {
            Some(offset) => offset - entry.image_offset as u64,
            None => bytes_available,
        };

        bound.min(bytes_available)
    }

    fn decode_png_frame(&mut self, entry: &IconDirEntry) -> VexelResult<ImageFrame> {
        let png_bytes = self.read_entry_bytes(entry)?;

        let mut png_decoder = PngDecoder::new(Cursor::new(png_bytes));
        png_decoder.set_limits(self.limits.clone());
        let image = png_decoder.decode()?;

        Ok(ImageFrame::new(image.width(), image.height(), image.into_pixels(), 0))
    }

    fn decode_bmp_frame(&mut self, entry: &IconDirEntry) -> VexelResult<ImageFrame> {
        let bmp_bytes = self.read_entry_bytes(entry)?;

        let (image, and_mask, actual_bpp) =
            BmpDecoder::<Cursor<&[u8]>>::decode_ico_bmp(&bmp_bytes, entry.width, entry.height)?;

        let mut pixels = apply_and_mask(image.into_pixels(), &and_mask, entry.width, entry.height, actual_bpp);
        pixels.correct_pixels(entry.width, entry.height);

        Ok(ImageFrame::new(entry.width, entry.height, pixels, 0))
    }

    fn read_directory(&mut self) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.entries.clear();
        self.entry_offsets.clear();
        self.type_raw = 0;
        self.reserved = 0;
        self.declared_count = 0;

        let count = match self.read_header() {
            Ok(c) => c,
            Err(e) => {
                log_error!("Error reading ICO header: {}", e);
                return Err(e);
            }
        };

        if count == 0 {
            return Err(VexelError::Custom("ICO file contains no images".to_string()));
        }

        match self.read_entries(count) {
            Ok(_) => {}
            Err(e) => {
                log_error!("Error reading ICO entries: {}", e);
            }
        }

        match self.detect_image_formats() {
            Ok(_) => {}
            Err(e) => {
                log_warn!("Error detecting ICO image formats: {}", e);
            }
        }

        if self.entries.is_empty() {
            return Err(VexelError::Custom("ICO file contains no valid entries".to_string()));
        }

        Ok(())
    }

    fn decode_entry(&mut self, entry: &IconDirEntry) -> Option<ImageFrame> {
        if entry.width == 0 || entry.height == 0 {
            log_warn!("ICO entry has zero dimensions ({}x{}), skipping", entry.width, entry.height);
            return None;
        }

        if let Err(e) = self.limits.reserve_buffer(entry.width, entry.height, 4) {
            log_warn!("ICO entry {}x{} exceeds limits, skipping: {}", entry.width, entry.height, e);
            return None;
        }

        match entry.image_format {
            IcoImageFormat::Png => match self.decode_png_frame(entry) {
                Ok(f) => Some(f),
                Err(e) => {
                    log_warn!("Error decoding PNG frame in ICO: {}", e);
                    None
                }
            },
            IcoImageFormat::Bmp => match self.decode_bmp_frame(entry) {
                Ok(f) => Some(f),
                Err(e) => {
                    log_warn!("Error decoding BMP frame in ICO: {}", e);
                    None
                }
            },
        }
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_directory()?;

        let mut candidates = self.entries.clone();
        candidates.sort_by_key(|e| std::cmp::Reverse((e.width * e.height, e.bit_count)));

        for entry in &candidates {
            if let Some(frame) = self.decode_entry(entry) {
                self.width = frame.width();
                self.height = frame.height();
                return Ok(Image::from_frame(frame));
            }
        }

        Err(VexelError::Custom("Failed to decode any images from ICO file".to_string()))
    }

    pub fn decode_pages(&mut self) -> VexelResult<Vec<Image>> {
        self.read_directory()?;

        let entries = self.entries.clone();
        let mut pages = Vec::with_capacity(entries.len());

        for entry in &entries {
            if let Some(frame) = self.decode_entry(entry) {
                pages.push(Image::from_frame(frame));
            }
        }

        if pages.is_empty() {
            return Err(VexelError::Custom("Failed to decode any images from ICO file".to_string()));
        }

        self.width = pages.iter().map(|p| p.width()).max().unwrap_or(0);
        self.height = pages.iter().map(|p| p.height()).max().unwrap_or(0);

        Ok(pages)
    }
}

fn apply_and_mask(pixels: PixelData, and_mask: &[u8], width: u32, height: u32, bpp: u16) -> PixelData {
    if and_mask.is_empty() || bpp == 32 {
        return pixels;
    }

    let and_row_size = width.div_ceil(32) * 4;
    let rgba = pixels.into_rgba8();
    let mut data = match rgba {
        PixelData::RGBA8(v) => v,
        other => return other,
    };

    for y in 0..height as usize {
        let src_row = height as usize - 1 - y;
        let mask_row_offset = src_row * and_row_size as usize;

        for x in 0..width as usize {
            let byte_idx = mask_row_offset + x / 8;
            let bit_idx = 7 - (x % 8);

            if byte_idx < and_mask.len() {
                let masked = (and_mask[byte_idx] >> bit_idx) & 1;
                if masked == 1 {
                    let pixel_offset = (y * width as usize + x) * 4;
                    if pixel_offset + 3 < data.len() {
                        data[pixel_offset + 3] = 0;
                    }
                }
            }
        }
    }

    PixelData::RGBA8(data)
}
