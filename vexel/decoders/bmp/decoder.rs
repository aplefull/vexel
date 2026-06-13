use crate::bitreader::BitReader;
use crate::decoders::bmp::compression::RleDecoder;
use crate::decoders::bmp::headers::HeaderReader;
use crate::decoders::bmp::pixels::PixelDecoder;
use crate::decoders::bmp::types::{BitmapCompression, BitmapFileHeader, BitmapInfoHeader, ColorEntry, DibHeader};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::icc::ICCProfile;
use crate::utils::info::BmpInfo;
use crate::{log_error, log_warn, Image};
use std::io::{Cursor, Read, Seek, SeekFrom};

pub struct BmpDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    file_header: BitmapFileHeader,
    dib_header: DibHeader,
    extra_masks: Option<(u32, u32, u32, u32)>,
    color_table: Vec<ColorEntry>,
    icc_profile: Option<ICCProfile>,
    data: Vec<u8>,
    rle_decoded: bool,
    reader: BitReader<R>,
}

impl<R: Read + Seek> BmpDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            file_header: BitmapFileHeader {
                file_size: 0,
                reserved1: 0,
                reserved2: 0,
                pixel_offset: 0,
            },
            dib_header: DibHeader::Info(BitmapInfoHeader {
                width: 0,
                height: 0,
                planes: 0,
                bits_per_pixel: 0,
                compression: BitmapCompression::BiRgb,
                image_size: 0,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                important_colors: 0,
            }),
            extra_masks: None,
            color_table: Vec::new(),
            icc_profile: None,
            data: Vec::new(),
            rle_decoded: false,
            reader: BitReader::with_le(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> BmpInfo {
        BmpInfo {
            width: self.width,
            height: self.height,
            file_header: self.file_header.clone(),
            dib_header: self.dib_header.clone(),
            color_table: self.color_table.clone(),
            icc_profile: self.icc_profile.clone(),
        }
    }

    fn read_file_header(&mut self) -> VexelResult<()> {
        self.file_header = HeaderReader::read_file_header(&mut self.reader)?;
        Ok(())
    }

    fn read_info_header(&mut self) -> VexelResult<()> {
        let (dib_header, width, height) = HeaderReader::read_info_header(&mut self.reader)?;
        self.dib_header = dib_header;
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn read_extra_masks(&mut self) -> VexelResult<()> {
        if matches!(self.dib_header, DibHeader::Info(_)) {
            let compression = self.dib_header.compression();
            if matches!(compression, BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields) {
                let r = self.reader.read_u32()?;
                let g = self.reader.read_u32()?;
                let b = self.reader.read_u32()?;
                let a = if matches!(compression, BitmapCompression::BiAlphaBitfields) {
                    self.reader.read_u32()?
                } else {
                    0
                };
                self.extra_masks = Some((r, g, b, a));
            }
        }
        Ok(())
    }

    fn read_color_table(&mut self) -> VexelResult<()> {
        if self.dib_header.bits_per_pixel() <= 8 {
            let num_colors = if self.dib_header.colors_used() > 0 {
                self.dib_header.colors_used()
            } else {
                1u32 << self.dib_header.bits_per_pixel()
            };

            let is_core = matches!(self.dib_header, DibHeader::Core(_));

            for _ in 0..num_colors {
                let blue = self.reader.read_u8()?;
                let green = self.reader.read_u8()?;
                let red = self.reader.read_u8()?;
                let reserved = if is_core { 0 } else { self.reader.read_u8()? };

                self.color_table.push(ColorEntry {
                    blue,
                    green,
                    red,
                    reserved,
                });
            }
        }

        Ok(())
    }

    fn read_icc_profile(&mut self) -> VexelResult<()> {
        let (profile_data_offset, profile_size) = match &self.dib_header {
            DibHeader::V5(h) if h.profile_size > 0 => (h.profile_data, h.profile_size),
            _ => return Ok(()),
        };

        let file_offset = 14u64 + profile_data_offset as u64;
        self.reader.seek(SeekFrom::Start(file_offset))?;
        let data = self.reader.read_bytes(profile_size as usize)?;

        if let Ok(profile) = ICCProfile::new(&data) {
            self.icc_profile = Some(profile);
        }

        Ok(())
    }

    fn read_pixel_data(&mut self) -> VexelResult<()> {
        self.reader
            .seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;

        let is_rle = matches!(
            self.dib_header.compression(),
            BitmapCompression::BiRle4 | BitmapCompression::BiRle8
        );

        let bytes_until_eof = self.reader.bytes_left()?;
        let bytes_to_read = if is_rle {
            bytes_until_eof
        } else {
            let row_size = ((self.dib_header.bits_per_pixel() as u32 * self.width + 31) / 32) * 4;
            let data_size = row_size * self.height;
            std::cmp::min(data_size as u64, bytes_until_eof)
        };

        self.data = self.reader.read_bytes(bytes_to_read as usize)?;

        Ok(())
    }

    fn decode_jpeg(&mut self) -> VexelResult<Image> {
        self.reader.seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;
        let image_size = self.dib_header.image_size();
        let jpeg_bytes = if image_size > 0 {
            self.reader.read_bytes(image_size as usize)?
        } else {
            self.reader.read_to_end()?
        };
        let mut jpeg_decoder = JpegDecoder::new(Cursor::new(jpeg_bytes));
        jpeg_decoder.decode()
    }

    fn decode_png(&mut self) -> VexelResult<Image> {
        self.reader.seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;
        let image_size = self.dib_header.image_size();
        let png_bytes = if image_size > 0 {
            self.reader.read_bytes(image_size as usize)?
        } else {
            self.reader.read_to_end()?
        };
        let mut png_decoder = PngDecoder::new(Cursor::new(png_bytes));
        png_decoder.decode()
    }

    pub fn decode_ico_bmp(data: &[u8], dir_width: u32, dir_height: u32) -> VexelResult<(Image, Vec<u8>)> {
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
            if colors_used > 0 { colors_used } else { 1u32 << bpp }
        } else {
            0
        };

        let extra_mask_bytes = match compression_val {
            3 => 12usize,
            6 => 16usize,
            _ => 0,
        };

        let pixel_offset = 14 + header_size + extra_mask_bytes + (num_colors as usize * bytes_per_color);

        let xor_size = {
            let row_size = (((bpp as u32 * dir_width) + 31) / 32) * 4;
            (row_size * dir_height) as usize
        };
        let and_start = pixel_offset - 14 + xor_size;
        let and_mask_row_size = ((dir_width + 31) / 32) * 4;
        let and_size = (and_mask_row_size * dir_height) as usize;
        let and_mask = if and_start < data.len() {
            let end = (and_start + and_size).min(data.len());
            data[and_start..end].to_vec()
        } else {
            Vec::new()
        };

        let inject_alpha_masks = bpp == 32 && compression_val == 0 && !is_core;
        let adjusted_pixel_offset = if inject_alpha_masks { pixel_offset + 16 } else { pixel_offset };

        let xor_end = (pixel_offset - 14 + xor_size).min(data.len());
        let pre_pixel_data = &data[..pixel_offset - 14];
        let pixel_data = &data[pixel_offset - 14..xor_end];

        let total_size = (14 + pre_pixel_data.len() + if inject_alpha_masks { 16 } else { 0 } + pixel_data.len()) as u32;
        let mut full_bmp = Vec::with_capacity(total_size as usize);
        full_bmp.extend_from_slice(b"BM");
        full_bmp.extend_from_slice(&total_size.to_le_bytes());
        full_bmp.extend_from_slice(&0u16.to_le_bytes());
        full_bmp.extend_from_slice(&0u16.to_le_bytes());
        full_bmp.extend_from_slice(&(adjusted_pixel_offset as u32).to_le_bytes());
        full_bmp.extend_from_slice(pre_pixel_data);

        if inject_alpha_masks {
            full_bmp[30] = 6;
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

        Ok((image, and_mask))
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.read_file_header() {
            Err(e) => {
                log_error!("Error reading file header. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_info_header() {
            Err(e) => {
                log_error!("Error reading info header. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_extra_masks() {
            Err(e) => {
                log_error!("Error reading extra masks. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_color_table() {
            Err(e) => {
                log_error!("Error reading color table. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_icc_profile() {
            Err(e) => {
                log_warn!("Error reading ICC profile: {}", e);
            }
            Ok(_) => (),
        };

        match self.read_pixel_data() {
            Err(e) => {
                log_error!("Error reading pixel data. This might be critical! Error: {}", e);
            }
            Ok(_) => (),
        };

        match self.dib_header.compression() {
            BitmapCompression::BiRgb => (),
            BitmapCompression::BiRle8 => {
                if self.dib_header.bits_per_pixel() != 8 {
                    log_warn!(
                        "Invalid bit depth for RLE8 compression: {}",
                        self.dib_header.bits_per_pixel()
                    );
                }

                self.data = RleDecoder::decode_rle8(&self.data, self.width, self.height)?;
                self.rle_decoded = true;
            }
            BitmapCompression::BiRle4 => {
                if self.dib_header.bits_per_pixel() != 4 {
                    log_warn!(
                        "Invalid bit depth for RLE4 compression: {}",
                        self.dib_header.bits_per_pixel()
                    );
                }

                self.data = RleDecoder::decode_rle4(&self.data, self.width, self.height)?;
                self.rle_decoded = true;
            }
            BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields => (),
            BitmapCompression::BiJpeg => {
                return self.decode_jpeg();
            }
            BitmapCompression::BiPng => {
                return self.decode_png();
            }
            _ => {
                // TODO: Implement other compression types
                log_warn!("Unsupported compression type: {:?}", self.dib_header.compression());
            }
        }

        let bottom_up = self.dib_header.height() > 0;

        let image = match self.dib_header.bits_per_pixel() {
            1 => PixelDecoder::decode_1bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table),
            2 => PixelDecoder::decode_2bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table),
            4 => {
                if self.rle_decoded {
                    RleDecoder::decode_rle4_image(&self.data, self.width, self.height, &self.color_table)
                } else {
                    PixelDecoder::decode_4bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                }
            }
            8 => {
                if self.rle_decoded {
                    RleDecoder::decode_rle8_image(&self.data, self.width, self.height, &self.color_table)
                } else {
                    PixelDecoder::decode_8bit_image(&self.data, self.width, self.height, bottom_up, &self.color_table)
                }
            }
            16 => {
                let use_masks = matches!(
                    self.dib_header.compression(),
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
                    self.dib_header.compression(),
                    BitmapCompression::BiBitfields | BitmapCompression::BiAlphaBitfields
                );
                let (red_mask, green_mask, blue_mask, alpha_mask) = use_masks
                    .then(|| self.dib_header.color_masks().or(self.extra_masks))
                    .flatten()
                    .unwrap_or_else(|| {
                        let has_alpha = self.data.chunks_exact(4).any(|px| px[3] != 0);
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
                log_warn!(
                    "Invalid bit depth: {}. Attempting to decode as 24bit.",
                    self.dib_header.bits_per_pixel()
                );
                PixelDecoder::decode_24bit_image(&self.data, self.width, self.height, bottom_up)
            }
        };

        Ok(image)
    }
}
