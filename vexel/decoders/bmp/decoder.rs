use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::bmp::compression::RleDecoder;
use crate::decoders::bmp::headers::HeaderReader;
use crate::decoders::bmp::info::{
    compression_name, BmpIccProfile, BmpInfo, BmpLinkedProfile, BmpPixelData, BmpSection, CS_TYPE_LINKED,
};
use crate::decoders::bmp::pixels::{mask_alpha_is_used, PixelDecoder};
use crate::decoders::bmp::types::{BitmapCompression, BitmapFileHeader, ColorEntry, ColorMasks, DibHeader, DibKind};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, DataExtent, ImageSummary};
use crate::{log_error, log_warn, Image, Limits};

const BA_HEADER_SIZE: u64 = 14;
const FILE_HEADER_SIZE: u64 = 14;

#[derive(Clone, Copy, PartialEq)]
enum ScanMode {
    InfoOnly,
    Decode,
}

struct PixelExtent {
    offset: u64,
    length: u64,
    expected: Option<u64>,
}

pub struct BmpDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    limits: Limits,
    file_header: BitmapFileHeader,
    dib_header: DibHeader,
    dib_offset: u64,
    extra_masks: Option<(u32, u32, u32, u32)>,
    color_table: Vec<ColorEntry>,
    icc_profile: Option<ICCProfile>,
    data: Vec<u8>,
    rle_decoded: bool,
    fallback_pixel_offset: u64,
    collected: BmpInfo,
    info: Option<BmpInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> BmpDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            limits: Limits::default(),
            file_header: BitmapFileHeader::default(),
            dib_header: DibHeader::empty(),
            dib_offset: 0,
            extra_masks: None,
            color_table: Vec::new(),
            icc_profile: None,
            data: Vec::new(),
            rle_decoded: false,
            fallback_pixel_offset: 0,
            collected: BmpInfo::default(),
            info: None,
            reader: BitReader::with_le(reader),
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> BmpInfo {
        if self.info.is_none() {
            let saved_limits = self.limits.clone();
            if let Err(e) = self.scan(ScanMode::InfoOnly) {
                log_warn!("Failed to read BMP structure: {}", e);
                self.finish_info();
            }
            self.limits = saved_limits;
        }

        self.info.clone().unwrap_or_default()
    }

    fn reset_state(&mut self) {
        self.width = 0;
        self.height = 0;
        self.file_header = BitmapFileHeader::default();
        self.dib_header = DibHeader::empty();
        self.dib_offset = 0;
        self.extra_masks = None;
        self.color_table.clear();
        self.icc_profile = None;
        self.data.clear();
        self.rle_decoded = false;
        self.fallback_pixel_offset = 0;
        self.collected = BmpInfo::default();
    }

    fn read_file_header(&mut self) -> VexelResult<()> {
        let start_offset = self.reader.stream_position().unwrap_or(0);
        let headers = HeaderReader::read_file_header(&mut self.reader)?;
        let mut offset = start_offset + headers.vendor_prefix_len;

        if let Some(ba) = headers.array_header {
            self.collected.push(offset, Some(BA_HEADER_SIZE), BmpSection::BitmapArrayHeader(ba));
            offset += BA_HEADER_SIZE;
        }

        if let Some(file_header) = headers.file_header {
            self.collected.push(offset, Some(FILE_HEADER_SIZE), BmpSection::FileHeader(file_header.clone()));
            self.file_header = file_header;
        }

        Ok(())
    }

    fn read_info_header(&mut self, mode: ScanMode) -> VexelResult<()> {
        let start_offset = self.reader.stream_position().unwrap_or(0);
        self.dib_offset = start_offset;
        let (dib_header, width, height) = HeaderReader::read_info_header(&mut self.reader)?;
        self.dib_header = dib_header;
        self.width = width;
        self.height = height;
        self.collected.push(
            start_offset,
            Some(u64::from(self.dib_header.header_size)),
            BmpSection::DibHeader(self.dib_header.clone()),
        );
        if mode == ScanMode::Decode {
            self.limits.reserve_buffer(self.width, self.height, 4)?;
        }
        Ok(())
    }

    fn read_extra_masks(&mut self) -> VexelResult<()> {
        if self.dib_header.kind == DibKind::Info {
            let compression = self.dib_header.compression;
            if matches!(compression, BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields) {
                let start_offset = self.reader.stream_position().unwrap_or(0);
                let red = self.reader.read_u32()?;
                let green = self.reader.read_u32()?;
                let blue = self.reader.read_u32()?;
                let alpha = if matches!(compression, BitmapCompression::BiAlphaBitfields) {
                    Some(self.reader.read_u32()?)
                } else {
                    None
                };
                self.extra_masks = Some((red, green, blue, alpha.unwrap_or(0)));
                let length = if alpha.is_some() { 16 } else { 12 };
                self.collected.push(
                    start_offset,
                    Some(length),
                    BmpSection::ExtraMasks(ColorMasks { red, green, blue, alpha }),
                );
            }
        }
        Ok(())
    }

    fn read_color_table(&mut self) -> VexelResult<()> {
        if self.dib_header.bits_per_pixel <= 8 {
            let max_colors = 1u32 << self.dib_header.bits_per_pixel;
            let mut num_colors = if self.dib_header.colors_used > 0 { self.dib_header.colors_used } else { max_colors };

            if num_colors > max_colors {
                log_warn!(
                    "Color table claims {} entries for {}-bit image, clamping to {}",
                    num_colors,
                    self.dib_header.bits_per_pixel,
                    max_colors
                );
                num_colors = max_colors;
            }

            let is_core = self.dib_header.kind == DibKind::Core;
            let bytes_per_entry = if is_core { 3usize } else { 4usize };
            let table_bytes = u64::from(num_colors).saturating_mul(bytes_per_entry as u64);
            self.limits.reserve(table_bytes)?;

            let start_offset = self.reader.stream_position().unwrap_or(0);

            for i in 0..num_colors {
                let entry = match self.reader.read_bytes(bytes_per_entry) {
                    Ok(entry) => entry,
                    Err(e) => {
                        log_warn!("Color table truncated at entry {} of {}: {}", i, num_colors, e);
                        break;
                    }
                };

                self.color_table.push(ColorEntry {
                    blue: entry[0],
                    green: entry[1],
                    red: entry[2],
                    reserved: if is_core { 0 } else { entry[3] },
                });
            }

            let length = self.color_table.len() as u64 * bytes_per_entry as u64;
            self.collected.push(start_offset, Some(length), BmpSection::ColorTable(self.color_table.clone()));
        }

        self.fallback_pixel_offset = self.reader.stream_position().unwrap_or(0);

        Ok(())
    }

    fn read_icc_profile(&mut self) -> VexelResult<()> {
        const ICC_MIN_PROFILE_SIZE: u64 = 128;

        let (profile_data_offset, declared_size) = match &self.dib_header.v5 {
            Some(v5) if v5.profile_size > 0 => (v5.profile_data, v5.profile_size),
            _ => return Ok(()),
        };

        let file_offset = self.dib_offset + profile_data_offset as u64;
        let stream_len = self.reader.seek(SeekFrom::End(0)).unwrap_or(0);
        if file_offset >= stream_len {
            log_warn!("BMP ICC profile offset {} is past end of file, skipping", file_offset);
            return Ok(());
        }

        let available = stream_len - file_offset;
        let profile_size = u64::from(declared_size).min(available);
        let is_linked = self.dib_header.color_space.as_ref().is_some_and(|cs| cs.cs_type == CS_TYPE_LINKED);

        if is_linked {
            self.reader.seek(SeekFrom::Start(file_offset))?;
            let bytes = self.reader.read_bytes(profile_size.min(4096) as usize)?;
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            let path = bytes[..end].iter().map(|&b| b as char).collect();
            self.collected.push(file_offset, Some(profile_size), BmpSection::LinkedProfile(BmpLinkedProfile { path }));
            return Ok(());
        }

        if profile_size < ICC_MIN_PROFILE_SIZE {
            log_warn!("BMP ICC profile is truncated to {} bytes, skipping", profile_size);
            self.collected.push(
                file_offset,
                Some(profile_size),
                BmpSection::IccProfile(BmpIccProfile { declared_size, parsed: false }),
            );
            return Ok(());
        }

        self.limits.reserve(profile_size)?;

        self.reader.seek(SeekFrom::Start(file_offset))?;
        let data = self.reader.read_bytes(profile_size as usize)?;

        let profile = ICCProfile::new(&data).ok();
        self.collected.push(
            file_offset,
            Some(profile_size),
            BmpSection::IccProfile(BmpIccProfile { declared_size, parsed: profile.is_some() }),
        );
        self.icc_profile = profile;

        Ok(())
    }

    fn is_compressed(&self) -> bool {
        matches!(
            self.dib_header.compression,
            BitmapCompression::BiRle4
                | BitmapCompression::BiRle8
                | BitmapCompression::BiCMYKRle4
                | BitmapCompression::BiCMYKRle8
                | BitmapCompression::Os2Huffman1D
                | BitmapCompression::Os2Rle24
        )
    }

    fn pixel_extent(&mut self) -> VexelResult<PixelExtent> {
        const MIN_PIXEL_OFFSET: u64 = 26;

        let mut offset = self.file_header.pixel_offset as u64;
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut bytes_until_eof = self.reader.bytes_left()?;

        if (offset < MIN_PIXEL_OFFSET || bytes_until_eof == 0)
            && self.fallback_pixel_offset >= MIN_PIXEL_OFFSET
            && self.fallback_pixel_offset != offset
        {
            log_warn!(
                "Pixel data offset {} looks invalid, using offset {} right after the headers instead",
                offset,
                self.fallback_pixel_offset
            );
            offset = self.fallback_pixel_offset;
            self.reader.seek(SeekFrom::Start(offset))?;
            bytes_until_eof = self.reader.bytes_left()?;
        }

        let embedded = matches!(self.dib_header.compression, BitmapCompression::BiJpeg | BitmapCompression::BiPng);

        let (length, expected) = if embedded {
            let image_size = u64::from(self.dib_header.image_size);
            let length = if image_size > 0 { image_size.min(bytes_until_eof) } else { bytes_until_eof };
            (length, (image_size > 0).then_some(image_size))
        } else if self.is_compressed() {
            (bytes_until_eof, None)
        } else {
            let row_size = (u64::from(self.dib_header.bits_per_pixel) * u64::from(self.width)).div_ceil(32) * 4;
            let data_size = row_size.saturating_mul(u64::from(self.height));
            (data_size.min(bytes_until_eof), Some(data_size))
        };

        Ok(PixelExtent { offset, length, expected })
    }

    fn record_pixel_extent(&mut self) -> VexelResult<PixelExtent> {
        let extent = self.pixel_extent()?;
        self.collected.push(
            extent.offset,
            Some(extent.length),
            BmpSection::PixelData(BmpPixelData {
                compression: self.dib_header.compression,
                expected_length: extent.expected,
            }),
        );
        Ok(extent)
    }

    fn read_pixel_data(&mut self, extent: &PixelExtent) -> VexelResult<()> {
        self.limits.reserve(extent.length)?;
        self.reader.seek(SeekFrom::Start(extent.offset))?;
        self.data = self.reader.read_bytes(extent.length as usize)?;

        Ok(())
    }

    fn build_summary(&self, extent: Option<&PixelExtent>) -> ImageSummary {
        let header = &self.dib_header;
        let bpp = header.bits_per_pixel;
        let alpha_mask = header.color_masks().or(self.extra_masks).map(|(_, _, _, a)| a).unwrap_or(0);
        let uses_masks =
            matches!(header.compression, BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields);
        let has_alpha = (uses_masks && alpha_mask != 0) || bpp == 64;

        let (color_model, channels, bits) = match (header.compression, bpp) {
            (BitmapCompression::BiCmyk, 32) => (ColorModel::Cmyk, 4, 8),
            (_, 0..=8) => (ColorModel::Indexed, 1, bpp as u8),
            (_, 16) => (if has_alpha { ColorModel::Rgba } else { ColorModel::Rgb }, if has_alpha { 4 } else { 3 }, 5),
            (_, 64) => (ColorModel::Rgba, 4, 16),
            _ => (if has_alpha { ColorModel::Rgba } else { ColorModel::Rgb }, if has_alpha { 4 } else { 3 }, 8),
        };

        let mut summary = ImageSummary::new(self.width, self.height, color_model, channels, bits);
        summary.has_alpha = has_alpha;
        if header.compression != BitmapCompression::BiRgb {
            summary.compression = Some(compression_name(header.compression).to_string());
        }
        summary.icc_profile = self.icc_profile.clone();
        if let Some(extent) = extent {
            if let (Some(expected), false) = (extent.expected, self.is_compressed()) {
                summary.data = Some(DataExtent { expected, actual: extent.length });
            }
        }
        summary
    }

    fn finish_info(&mut self) {
        let mut info = std::mem::take(&mut self.collected);
        let has_dib = info.sections.iter().any(|section| matches!(section.data, BmpSection::DibHeader(_)));
        if has_dib {
            let extent = info.sections.iter().find_map(|section| match &section.data {
                BmpSection::PixelData(pixel_data) => Some(PixelExtent {
                    offset: section.offset,
                    length: section.length.unwrap_or(0),
                    expected: pixel_data.expected_length,
                }),
                _ => None,
            });
            info.summary = Some(self.build_summary(extent.as_ref()));
        }
        info.sections.sort_by_key(|section| section.offset);
        self.info = Some(info);
    }

    fn scan(&mut self, mode: ScanMode) -> VexelResult<Option<PixelExtent>> {
        self.reset_state();
        self.reader.seek(SeekFrom::Start(0))?;

        if let Err(e) = self.read_file_header() {
            log_error!("Error reading file header. This might be critical! Error: {}", e);
        };

        match self.read_info_header(mode) {
            Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
            Err(e) => {
                log_error!("Error reading info header. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        if let Err(e) = self.read_extra_masks() {
            log_error!("Error reading extra masks. This might be critical! Error: {}", e);
        };

        match self.read_color_table() {
            Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
            Err(e) => {
                log_error!("Error reading color table. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_icc_profile() {
            Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
            Err(e) => {
                log_warn!("Error reading ICC profile: {}", e);
            }
            Ok(_) => (),
        };

        let extent = match self.record_pixel_extent() {
            Ok(extent) => Some(extent),
            Err(e) => {
                log_error!("Error locating pixel data. This might be critical! Error: {}", e);
                None
            }
        };

        self.finish_info();

        Ok(extent)
    }

    fn convert_cmyk_color_table(&mut self) {
        for entry in &mut self.color_table {
            let (r, g, b) = PixelDecoder::cmyk_to_rgb(entry.reserved, entry.red, entry.green, entry.blue);
            entry.red = r;
            entry.green = g;
            entry.blue = b;
            entry.reserved = 0;
        }
    }

    fn decode_jpeg(&mut self) -> VexelResult<Image> {
        self.reader.seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;
        let image_size = self.dib_header.image_size;
        let jpeg_bytes = if image_size > 0 {
            self.limits.reserve(u64::from(image_size))?;
            self.reader.read_bytes(image_size as usize)?
        } else {
            let bytes_left = self.reader.bytes_left().unwrap_or(0);
            self.limits.reserve(bytes_left)?;
            self.reader.read_to_end()?
        };
        let mut jpeg_decoder = JpegDecoder::new(Cursor::new(jpeg_bytes));
        jpeg_decoder.decode()
    }

    fn decode_png(&mut self) -> VexelResult<Image> {
        self.reader.seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;
        let image_size = self.dib_header.image_size;
        let png_bytes = if image_size > 0 {
            self.limits.reserve(u64::from(image_size))?;
            self.reader.read_bytes(image_size as usize)?
        } else {
            let bytes_left = self.reader.bytes_left().unwrap_or(0);
            self.limits.reserve(bytes_left)?;
            self.reader.read_to_end()?
        };
        let mut png_decoder = PngDecoder::new(Cursor::new(png_bytes));
        png_decoder.decode()
    }

    pub fn decode_ico_bmp(data: &[u8], dir_width: u32, dir_height: u32) -> VexelResult<(Image, Vec<u8>, u16)> {
        if data.len() < 4 {
            return Err(VexelError::Custom("ICO BMP data too short".to_string()));
        }

        let header_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        let (bpp, colors_used, compression_val, is_core) = if header_size == 12 && data.len() >= 12 {
            let bpp = u16::from_le_bytes([data[10], data[11]]);
            (bpp, 0u32, 0u32, true)
        } else if data.len() >= 36 {
            let bpp = u16::from_le_bytes([data[14], data[15]]);
            let compression = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
            let colors_used = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);
            (bpp, colors_used, compression, false)
        } else {
            return Err(VexelError::Custom("ICO BMP header too short".to_string()));
        };

        let bytes_per_color = if is_core { 3 } else { 4 };
        let num_colors = if bpp <= 8 {
            if colors_used > 0 {
                colors_used
            } else {
                1u32 << bpp
            }
        } else {
            0
        };

        let extra_mask_bytes = match compression_val {
            3 => 12usize,
            6 => 16usize,
            _ => 0,
        };

        let declared_header_end = (header_size as u64)
            .saturating_add(extra_mask_bytes as u64)
            .saturating_add(u64::from(num_colors).saturating_mul(bytes_per_color as u64));
        if declared_header_end > data.len() as u64 {
            log_warn!("ICO BMP header and palette extend past the image data, truncating");
        }
        let header_end = declared_header_end.min(data.len() as u64) as usize;
        let pixel_offset = 14 + header_end;

        let to_usize = |value: u64| usize::try_from(value).unwrap_or(usize::MAX);
        let xor_size = {
            let row_size = (u64::from(bpp) * u64::from(dir_width)).div_ceil(32) * 4;
            to_usize(row_size.saturating_mul(u64::from(dir_height)))
        };
        let and_start = header_end.saturating_add(xor_size);
        let and_mask_row_size = u64::from(dir_width.div_ceil(32)) * 4;
        let and_size = to_usize(and_mask_row_size.saturating_mul(u64::from(dir_height)));
        let and_mask = if and_start < data.len() {
            let end = and_start.saturating_add(and_size).min(data.len());
            data[and_start..end].to_vec()
        } else {
            Vec::new()
        };

        let inject_alpha_masks = bpp == 32 && compression_val == 0 && !is_core;
        let adjusted_pixel_offset = if inject_alpha_masks { pixel_offset + 16 } else { pixel_offset };

        let xor_end = header_end.saturating_add(xor_size).min(data.len());
        let pre_pixel_data = &data[..header_end];
        let pixel_data = &data[header_end..xor_end];

        let total_size =
            (14 + pre_pixel_data.len() + if inject_alpha_masks { 16 } else { 0 } + pixel_data.len()) as u32;
        let mut full_bmp = Vec::with_capacity(total_size as usize);
        full_bmp.extend_from_slice(b"BM");
        full_bmp.extend_from_slice(&total_size.to_le_bytes());
        full_bmp.extend_from_slice(&0u16.to_le_bytes());
        full_bmp.extend_from_slice(&0u16.to_le_bytes());
        full_bmp.extend_from_slice(&(adjusted_pixel_offset as u32).to_le_bytes());
        full_bmp.extend_from_slice(pre_pixel_data);

        if inject_alpha_masks {
            if let Some(compression) = full_bmp.get_mut(30) {
                *compression = 6;
            }
            for mask in [0x00FF0000u32, 0x0000FF00, 0x000000FF, 0xFF000000] {
                full_bmp.extend_from_slice(&mask.to_le_bytes());
            }
        }

        full_bmp.extend_from_slice(pixel_data);

        let h_offset = if is_core { 20usize } else { 22usize };
        if is_core && full_bmp.len() >= h_offset + 2 {
            let bytes = (dir_height as u16).to_le_bytes();
            full_bmp[h_offset] = bytes[0];
            full_bmp[h_offset + 1] = bytes[1];
        } else if !is_core && full_bmp.len() >= h_offset + 4 {
            let bytes = (dir_height as i32).to_le_bytes();
            full_bmp[h_offset] = bytes[0];
            full_bmp[h_offset + 1] = bytes[1];
            full_bmp[h_offset + 2] = bytes[2];
            full_bmp[h_offset + 3] = bytes[3];
        }

        let mut decoder = BmpDecoder::new(Cursor::new(full_bmp));
        let image = decoder.decode()?;

        Ok((image, and_mask, bpp))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.info = None;
        let extent = self.scan(ScanMode::Decode)?;

        let embedded = matches!(self.dib_header.compression, BitmapCompression::BiJpeg | BitmapCompression::BiPng);
        if let Some(extent) = extent.filter(|_| !embedded) {
            match self.read_pixel_data(&extent) {
                Err(e @ VexelError::LimitExceeded(_)) => return Err(e),
                Err(e) => {
                    log_error!("Error reading pixel data. This might be critical! Error: {}", e);
                }
                Ok(_) => (),
            };
        }

        let bottom_up = self.dib_header.height > 0;
        let compression = self.dib_header.compression;

        match compression {
            BitmapCompression::BiRgb => (),
            BitmapCompression::BiRle8 | BitmapCompression::BiCMYKRle8 => {
                if self.dib_header.bits_per_pixel != 8 {
                    log_warn!("Invalid bit depth for RLE8 compression: {}", self.dib_header.bits_per_pixel);
                }

                let raw_size = self.data.len();
                let decoded_size = u64::from(self.width).saturating_mul(u64::from(self.height));
                self.limits.reserve(decoded_size)?;
                self.data = RleDecoder::decode_rle8(&self.data, self.width, self.height)?;
                self.limits.free_usize(raw_size);
                self.rle_decoded = true;

                if compression == BitmapCompression::BiCMYKRle8 {
                    self.convert_cmyk_color_table();
                }
            }
            BitmapCompression::BiRle4 | BitmapCompression::BiCMYKRle4 => {
                if self.dib_header.bits_per_pixel != 4 {
                    log_warn!("Invalid bit depth for RLE4 compression: {}", self.dib_header.bits_per_pixel);
                }

                let raw_size = self.data.len();
                let decoded_size = u64::from(self.width).saturating_mul(u64::from(self.height));
                self.limits.reserve(decoded_size)?;
                self.data = RleDecoder::decode_rle4(&self.data, self.width, self.height)?;
                self.limits.free_usize(raw_size);
                self.rle_decoded = true;

                if compression == BitmapCompression::BiCMYKRle4 {
                    self.convert_cmyk_color_table();
                }
            }
            BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields => (),
            BitmapCompression::BiJpeg => {
                return self.decode_jpeg();
            }
            BitmapCompression::BiPng => {
                return self.decode_png();
            }
            BitmapCompression::Os2Huffman1D => {
                if self.dib_header.bits_per_pixel != 1 {
                    log_warn!(
                        "Invalid bit depth for Huffman 1D compression: {}, decoding as 1-bit",
                        self.dib_header.bits_per_pixel
                    );
                }

                let raw_size = self.data.len();
                let src_stride = u64::from(self.width.div_ceil(8));
                let dst_stride = (src_stride + 3) & !3;
                let decoded_size = (src_stride + dst_stride).saturating_mul(u64::from(self.height));
                self.limits.reserve(decoded_size)?;
                self.data = RleDecoder::decode_huffman1d(&self.data, self.width, self.height);
                self.limits.free(src_stride.saturating_mul(u64::from(self.height)));
                self.limits.free_usize(raw_size);

                return Ok(PixelDecoder::decode_1bit_image(
                    &self.data,
                    self.width,
                    self.height,
                    bottom_up,
                    &self.color_table,
                ));
            }
            BitmapCompression::Os2Rle24 => {
                if self.dib_header.bits_per_pixel != 24 {
                    log_warn!(
                        "Invalid bit depth for RLE24 compression: {}, decoding as 24-bit",
                        self.dib_header.bits_per_pixel
                    );
                }

                let raw_size = self.data.len();
                let stride = (u64::from(self.width) * 3 + 3) & !3;
                let decoded_size = stride.saturating_mul(u64::from(self.height));
                self.limits.reserve(decoded_size)?;
                self.data = RleDecoder::decode_rle24(&self.data, self.width, self.height);
                self.limits.free_usize(raw_size);

                return Ok(PixelDecoder::decode_24bit_image(&self.data, self.width, self.height, bottom_up));
            }
            BitmapCompression::BiCmyk => match self.dib_header.bits_per_pixel {
                32 => {
                    return Ok(PixelDecoder::decode_cmyk32_image(&self.data, self.width, self.height, bottom_up));
                }
                bpp if bpp <= 8 => self.convert_cmyk_color_table(),
                bpp => {
                    log_warn!("CMYK bitmap cannot have bit depth {}, decoding as RGB", bpp);
                }
            },
        }

        let image = match self.dib_header.bits_per_pixel {
            1 => PixelDecoder::decode_1bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table),
            2 => PixelDecoder::decode_2bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table),
            4 => {
                if self.rle_decoded {
                    RleDecoder::decode_rle4_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                } else {
                    PixelDecoder::decode_4bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                }
            }
            8 => {
                if self.rle_decoded {
                    RleDecoder::decode_rle8_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                } else {
                    PixelDecoder::decode_8bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                }
            }
            16 => {
                let use_masks = matches!(
                    self.dib_header.compression,
                    BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields
                );
                if let Some((red_mask, green_mask, blue_mask, alpha_mask)) =
                    use_masks.then(|| self.dib_header.color_masks().or(self.extra_masks)).flatten()
                {
                    PixelDecoder::decode_16bit_image_masked(
                        &self.data,
                        self.width,
                        self.height,
                        bottom_up,
                        red_mask,
                        green_mask,
                        blue_mask,
                        alpha_mask,
                    )
                } else {
                    PixelDecoder::decode_16bit_image(&self.data, self.width, self.height, bottom_up)
                }
            }
            24 => PixelDecoder::decode_24bit_image(&self.data, self.width, self.height, bottom_up),
            32 => {
                let use_masks = matches!(
                    self.dib_header.compression,
                    BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields
                );
                let (red_mask, green_mask, blue_mask, alpha_mask) = use_masks
                    .then(|| self.dib_header.color_masks().or(self.extra_masks))
                    .flatten()
                    .unwrap_or_else(|| {
                        let has_alpha = mask_alpha_is_used(&self.data, 0xFF000000, 4);
                        let a_mask = if has_alpha { 0xFF000000 } else { 0 };
                        (0x00FF0000, 0x0000FF00, 0x000000FF, a_mask)
                    });
                PixelDecoder::decode_32bit_image(
                    &self.data,
                    self.width,
                    self.height,
                    bottom_up,
                    red_mask,
                    green_mask,
                    blue_mask,
                    alpha_mask,
                )
            }
            64 => PixelDecoder::decode_64bit_image(&self.data, self.width, self.height, bottom_up),
            _ => {
                log_warn!("Invalid bit depth: {}. Attempting to decode as 24bit.", self.dib_header.bits_per_pixel);
                PixelDecoder::decode_24bit_image(&self.data, self.width, self.height, bottom_up)
            }
        };

        Ok(image)
    }
}
