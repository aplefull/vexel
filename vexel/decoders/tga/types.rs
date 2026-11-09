use serde::Serialize;
use tsify::Tsify;

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

#[derive(Debug, Clone, Default, Serialize, Tsify)]
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

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaFooterData {
    pub extension_area_offset: u32,
    pub developer_dir_offset: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TgaExtensionAreaData {
    pub extension_size: u16,
    pub author_name: String,
    pub author_comments: String,
    pub date_month: u16,
    pub date_day: u16,
    pub date_year: u16,
    pub time_hour: u16,
    pub time_minute: u16,
    pub time_second: u16,
    pub job_name: String,
    pub job_hours: u16,
    pub job_minutes: u16,
    pub job_seconds: u16,
    pub software_id: String,
    pub software_version_number: u16,
    pub software_version_letter: u8,
    pub key_color_a: u8,
    pub key_color_r: u8,
    pub key_color_g: u8,
    pub key_color_b: u8,
    pub pixel_aspect_ratio_numerator: u16,
    pub pixel_aspect_ratio_denominator: u16,
    pub gamma_value_numerator: u16,
    pub gamma_value_denominator: u16,
    pub color_correction_offset: u32,
    pub postage_stamp_offset: u32,
    pub scan_line_offset: u32,
    pub attributes_type: u8,
}
