use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::bmp::decoder::BmpDecoder;
use crate::decoders::ico::types::{
    IcoIconDirData, IcoIconDirEntryData, IcoImageDataInfo, IcoImageFormat, IcoSectionData, IcoSectionInfo, IcoType,
    IconDirEntry,
};
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, ImageFrame, PixelData};
use crate::utils::info::IcoInfo;
use crate::{log_error, log_warn, Limits};

pub struct IcoDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    ico_type: IcoType,
    entries: Vec<IconDirEntry>,
    entry_offsets: Vec<u64>,
    sections: Vec<IcoSectionInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> IcoDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            ico_type: IcoType::Ico,
            entries: Vec::new(),
            entry_offsets: Vec::new(),
            sections: Vec::new(),
            reader: BitReader::with_le(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&self) -> IcoInfo {
        IcoInfo { sections: self.sections.clone() }
    }

    fn read_header(&mut self) -> VexelResult<u16> {
        let header_offset = self.reader.stream_position().unwrap_or(0);

        let reserved = self.reader.read_u16()?;
        if reserved != 0 {
            log_warn!("ICO reserved field is non-zero: {}", reserved);
        }

        let type_val = self.reader.read_u16()?;
        self.ico_type = match type_val {
            1 => IcoType::Ico,
            2 => IcoType::Cur,
            _ => {
                log_warn!("Unknown ICO type: {}, treating as ICO", type_val);
                IcoType::Ico
            }
        };

        let count = self.reader.read_u16()?;

        self.sections.push(IcoSectionInfo {
            start_offset: header_offset,
            data: IcoSectionData::IconDir(IcoIconDirData { ico_type: self.ico_type, image_count: count }),
        });

        Ok(count)
    }

    fn read_entries(&mut self, count: u16) -> VexelResult<()> {
        for _ in 0..count {
            let entry_offset = self.reader.stream_position().unwrap_or(0);

            let width_byte = self.reader.read_u8()?;
            let height_byte = self.reader.read_u8()?;
            let color_count = self.reader.read_u8()?;
            // TODO: add a method to skip bytes, so we don't have to read into vars we don't use
            let _reserved = self.reader.read_u8()?;
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

    fn push_entry_and_image_data_sections(&mut self) {
        for (entry, &offset) in self.entries.iter().zip(self.entry_offsets.iter()) {
            self.sections.push(IcoSectionInfo {
                start_offset: offset,
                data: IcoSectionData::IconDirEntry(IcoIconDirEntryData {
                    width: entry.width,
                    height: entry.height,
                    color_count: entry.color_count,
                    planes: entry.planes,
                    bit_count: entry.bit_count,
                    bytes_in_res: entry.bytes_in_res,
                    image_offset: entry.image_offset,
                    hotspot_x: entry.hotspot_x,
                    hotspot_y: entry.hotspot_y,
                    image_format: entry.image_format,
                }),
            });
        }
        for entry in &self.entries {
            self.sections.push(IcoSectionInfo {
                start_offset: entry.image_offset as u64,
                data: IcoSectionData::ImageData(IcoImageDataInfo {
                    length: entry.bytes_in_res,
                    image_format: entry.image_format,
                }),
            });
        }
        self.sections.sort_by_key(|s| s.start_offset);
    }

    fn decode_png_frame(&mut self, entry: &IconDirEntry) -> VexelResult<ImageFrame> {
        self.reader.seek(SeekFrom::Start(entry.image_offset as u64))?;
        let bytes_available = self.reader.bytes_left().unwrap_or(0);
        let bytes_to_read = (entry.bytes_in_res as u64).min(bytes_available) as usize;
        let png_bytes = self.reader.read_bytes(bytes_to_read)?;

        let mut png_decoder = PngDecoder::new(Cursor::new(png_bytes));
        png_decoder.set_limits(self.limits.clone());
        let image = png_decoder.decode()?;

        let frame = image.frames().first().cloned().unwrap_or_else(|| {
            let mut pixels = PixelData::RGBA8(vec![0u8; (entry.width * entry.height * 4) as usize]);
            pixels.correct_pixels(entry.width, entry.height);
            ImageFrame::new(entry.width, entry.height, pixels, 0)
        });

        Ok(frame)
    }

    fn decode_bmp_frame(&mut self, entry: &IconDirEntry) -> VexelResult<ImageFrame> {
        self.reader.seek(SeekFrom::Start(entry.image_offset as u64))?;
        let bytes_available = self.reader.bytes_left().unwrap_or(0);
        let bytes_to_read = (entry.bytes_in_res as u64).min(bytes_available) as usize;
        let bmp_bytes = self.reader.read_bytes(bytes_to_read)?;

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
        self.sections.clear();

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

        self.push_entry_and_image_data_sections();

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
