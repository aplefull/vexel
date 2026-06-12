use crate::bitreader::BitReader;
use crate::decoders::bmp::compression::RleDecoder;
use crate::decoders::bmp::headers::HeaderReader;
use crate::decoders::bmp::pixels::PixelDecoder;
use crate::decoders::bmp::types::{BitmapCompression, BitmapFileHeader, BitmapInfoHeader, ColorEntry, DibHeader};
use crate::decoders::jpeg::decoder::JpegDecoder;
use crate::decoders::png::decoder::PngDecoder;
use crate::utils::error::VexelResult;
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

            for _ in 0..num_colors {
                let blue = self.reader.read_u8()?;
                let green = self.reader.read_u8()?;
                let red = self.reader.read_u8()?;
                let reserved = self.reader.read_u8()?;

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

    fn read_pixel_data(&mut self) -> VexelResult<()> {
        self.reader
            .seek(SeekFrom::Start(self.file_header.pixel_offset as u64))?;

        let row_size = ((self.dib_header.bits_per_pixel() as u32 * self.width + 31) / 32) * 4;
        let data_size = row_size * self.height;

        let bytes_until_eof = self.reader.bytes_left()?;
        let bytes_to_read = std::cmp::min(data_size as u64, bytes_until_eof);
        let data = self.reader.read_bytes(bytes_to_read as usize)?;

        self.data = data;

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
                if let Some((red_mask, green_mask, blue_mask, alpha_mask)) =
                    self.dib_header.color_masks().or(self.extra_masks)
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
                let (red_mask, green_mask, blue_mask, alpha_mask) = self
                    .dib_header
                    .color_masks()
                    .unwrap_or((0x00FF0000, 0x0000FF00, 0x000000FF, 0));
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
