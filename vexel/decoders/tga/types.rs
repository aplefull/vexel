pub const FLAG_ORIGIN_RIGHT: u8 = 1 << 4;
pub const FLAG_ORIGIN_TOP: u8 = 1 << 5;
pub const FLAG_ALPHA_SIZE_MASK: u8 = 0x0f;

pub const IMAGE_TYPE_PALETTED: u8 = 1;
pub const IMAGE_TYPE_MONOCHROME: u8 = 3;
pub const IMAGE_TYPE_MASK: u8 = 3;
pub const IMAGE_TYPE_FLAG_RLE: u8 = 1 << 3;

pub const ATTR_TYPE_ALPHA: u8 = 3;
pub const ATTR_TYPE_PREMULTIPLIED_ALPHA: u8 = 4;

pub const TGA_FOOTER_SIZE: i64 = 26;
pub const TGA_SIGNATURE: &[u8] = b"TRUEVISION-XFILE.\x00";
pub const EXT_AREA_ATTR_TYPE_OFFSET: u64 = 0x1ee;

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct TgaHeader {
    pub id_length: u8,
    pub palette_type: u8,
    pub image_type_raw: u8,
    pub palette_first: u16,
    pub palette_length: u16,
    pub palette_bpp: u8,
    pub x_origin: u16,
    pub y_origin: u16,
    pub width: u16,
    pub height: u16,
    pub bpp: u8,
    pub flags: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum ExtAlphaType {
    Alpha,
    PremultipliedAlpha,
    NoAlpha,
}
