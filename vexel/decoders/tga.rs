use crate::utils::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::{log_warn, Image, PixelData};
use std::io::{Read, Seek, SeekFrom};

const FLAG_ORIGIN_RIGHT: u8 = 1 << 4;
const FLAG_ORIGIN_TOP: u8 = 1 << 5;
const FLAG_ALPHA_SIZE_MASK: u8 = 0x0f;

const IMAGE_TYPE_PALETTED: u8 = 1;
const IMAGE_TYPE_MONOCHROME: u8 = 3;
const IMAGE_TYPE_MASK: u8 = 3;
const IMAGE_TYPE_FLAG_RLE: u8 = 1 << 3;

const ATTR_TYPE_ALPHA: u8 = 3;
const ATTR_TYPE_PREMULTIPLIED_ALPHA: u8 = 4;

const TGA_FOOTER_SIZE: i64 = 26;
const TGA_SIGNATURE: &[u8] = b"TRUEVISION-XFILE.\x00";
const EXT_AREA_ATTR_TYPE_OFFSET: u64 = 0x1ee;

#[derive(Debug, Default)]
#[allow(dead_code)]
struct TgaHeader {
    id_length: u8,
    palette_type: u8,
    image_type_raw: u8,
    palette_first: u16,
    palette_length: u16,
    palette_bpp: u8,
    x_origin: u16,
    y_origin: u16,
    width: u16,
    height: u16,
    bpp: u8,
    flags: u8,
}

#[derive(Debug, Clone, Copy)]
enum ExtAlphaType {
    Alpha,
    PremultipliedAlpha,
    NoAlpha,
}

pub struct TgaDecoder<R: Read + Seek> {
    reader: BitReader<R>,
}

impl<R: Read + Seek> TgaDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BitReader::with_le(reader),
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

    fn read_ext_alpha_type(&mut self) -> Option<ExtAlphaType> {
        self.reader.seek(SeekFrom::End(-TGA_FOOTER_SIZE)).ok()?;

        let mut footer = [0u8; TGA_FOOTER_SIZE as usize];
        self.reader.read_exact(&mut footer).ok()?;

        if &footer[8..26] != TGA_SIGNATURE {
            return None;
        }

        let ext_offset = u32::from_le_bytes([footer[0], footer[1], footer[2], footer[3]]) as u64;
        if ext_offset == 0 {
            return None;
        }

        self.reader
            .seek(SeekFrom::Start(ext_offset + EXT_AREA_ATTR_TYPE_OFFSET))
            .ok()?;

        let mut attr = [0u8; 1];
        self.reader.read_exact(&mut attr).ok()?;

        match attr[0] {
            ATTR_TYPE_ALPHA => Some(ExtAlphaType::Alpha),
            ATTR_TYPE_PREMULTIPLIED_ALPHA => Some(ExtAlphaType::PremultipliedAlpha),
            _ => Some(ExtAlphaType::NoAlpha),
        }
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
        let header = self.read_header()?;
        let ext_alpha = self.read_ext_alpha_type();
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

        let palette = if image_type == IMAGE_TYPE_PALETTED {
            self.read_palette(&header)
        } else {
            self.skip_palette(&header);
            Vec::new()
        };

        let mut pixels = self.decode_image_data(&header, &palette, has_alpha, is_rle);

        Self::apply_orientation(&mut pixels, header.width as usize, header.height as usize, header.flags);

        let mut pixel_data = PixelData::RGBA8(pixels);
        pixel_data.correct_pixels(header.width as u32, header.height as u32);

        Ok(Image::from_pixels(header.width as u32, header.height as u32, pixel_data))
    }
}
