use std::borrow::Cow;
use std::io::Cursor;

use serde::Serialize;
use tsify::Tsify;

use super::bitreader::XrBitReader;
use super::container::{self, Container};
use super::decoder::locate_codestream;
use super::headers::{parse_image_header, parse_plane_header, ImageHeader, PlaneHeader};
use super::types::*;
use crate::bitreader::BitReader;
use crate::log_warn;
use crate::utils::icc::ICCProfile;
use crate::utils::info::{ColorModel, FormatInfo, ImageSummary, InfoSection, InfoWriter, SampleFormat, Section};
use crate::utils::limits::Limits;
use crate::utils::tiff::info::{walk_structure, TiffSection};

#[tsify::declare]
pub type JpegXrInfo = FormatInfo<JpegXrSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxrImageHeader {
    pub alpha_stream: bool,
    pub version: u32,
    pub subversion: u32,
    pub tiling: bool,
    pub frequency_order: bool,
    pub spatial_transform: u32,
    pub index_table: bool,
    pub overlap: u8,
    pub short_header: bool,
    pub long_word: bool,
    pub windowing: bool,
    pub trim_flexbits: bool,
    pub tile_stretch: bool,
    pub red_blue_swapped: bool,
    pub premultiplied_alpha: bool,
    pub alpha_plane: bool,
    pub source_color_format: u32,
    pub source_bit_depth: u32,
    pub width: u32,
    pub height: u32,
    pub tile_columns: u32,
    pub tile_rows: u32,
    pub window: [u32; 4],
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxrPlaneHeader {
    pub alpha_plane: bool,
    pub internal_color_format: u32,
    pub scaled_arithmetic: bool,
    pub subband: u8,
    pub channels: u32,
    pub mantissa_length: u8,
    pub exponent_bias: i8,
    pub qp_mode: u32,
    pub dc_qp: Vec<u8>,
    pub lp_qp: Vec<u8>,
    pub hp_qp: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JpegXrSection {
    Tiff(TiffSection),
    ImageHeader(JxrImageHeader),
    PlaneHeader(JxrPlaneHeader),
}

fn color_format_code(format: ColorFormat) -> u32 {
    match format {
        ColorFormat::YOnly => 0,
        ColorFormat::Yuv420 => 1,
        ColorFormat::Yuv422 => 2,
        ColorFormat::Yuv444 => 3,
        ColorFormat::Cmyk => 4,
        ColorFormat::NComponent => 6,
        ColorFormat::Rgb => 7,
        ColorFormat::Rgbe => 8,
    }
}

fn color_format_name(code: u32) -> String {
    let name = match code {
        0 => "Y only",
        1 => "YUV 4:2:0",
        2 => "YUV 4:2:2",
        3 => "YUV 4:4:4",
        4 => "CMYK",
        5 => "CMYK direct",
        6 => "N-component",
        7 => "RGB",
        8 => "RGBE",
        _ => "Unknown",
    };
    format!("{} ({})", name, code)
}

fn bit_depth_name(code: u32) -> String {
    let name = match code {
        0 => "1-bit, white is one",
        1 => "8-bit",
        2 => "16-bit",
        3 => "16-bit signed",
        4 => "16-bit float",
        5 => "32-bit",
        6 => "32-bit signed",
        7 => "32-bit float",
        8 => "5-bit",
        9 => "10-bit",
        10 => "5-6-5",
        15 => "1-bit, black is one",
        _ => "Unknown",
    };
    format!("{} ({})", name, code)
}

fn subband_code(subband: Subband) -> u8 {
    match subband {
        Subband::All => 0,
        Subband::NoFlexbits => 1,
        Subband::NoHighpass => 2,
        Subband::DcOnly => 3,
        Subband::Isolated => 4,
    }
}

fn image_header_info(header: &ImageHeader, alpha_stream: bool) -> JxrImageHeader {
    JxrImageHeader {
        alpha_stream,
        version: header.version,
        subversion: header.subversion,
        tiling: header.tiling_present,
        frequency_order: header.bitstream_format == BitstreamFormat::Frequency,
        spatial_transform: header.spatial_transform,
        index_table: header.index_table_present,
        overlap: match header.overlap {
            Overlap::None => 0,
            Overlap::One => 1,
            Overlap::Two => 2,
        },
        short_header: header.short_header,
        long_word: header.long_word,
        windowing: header.windowing,
        trim_flexbits: header.trim_flexbits_flag,
        tile_stretch: header.tile_stretch,
        red_blue_swapped: header.rb_swapped,
        premultiplied_alpha: header.premultiplied_alpha,
        alpha_plane: header.alpha_channel,
        source_color_format: header.source_color_bits,
        source_bit_depth: header.source_depth_bits,
        width: header.width,
        height: header.height,
        tile_columns: header.tile_cols as u32 + 1,
        tile_rows: header.tile_rows as u32 + 1,
        window: [header.extra_top, header.extra_left, header.extra_bottom, header.extra_right],
    }
}

fn plane_header_info(plane: &PlaneHeader, alpha_plane: bool) -> JxrPlaneHeader {
    let channels = plane.num_channels.min(MAX_CHANNELS);
    JxrPlaneHeader {
        alpha_plane,
        internal_color_format: color_format_code(plane.internal_color_format),
        scaled_arithmetic: plane.scaled_arith,
        subband: subband_code(plane.subband),
        channels: plane.num_channels as u32,
        mantissa_length: plane.mantissa_len,
        exponent_bias: plane.exp_bias,
        qp_mode: plane.qp_mode,
        dc_qp: plane.qp_index_dc[..channels].to_vec(),
        lp_qp: plane.qp_index_lp[..channels].to_vec(),
        hp_qp: plane.qp_index_hp[..channels].to_vec(),
    }
}

struct Headers {
    image: ImageHeader,
    plane: PlaneHeader,
}

fn read_headers(
    data: &[u8],
    offset: usize,
    alpha_stream: bool,
    sections: &mut Vec<Section<JpegXrSection>>,
) -> Option<Headers> {
    let stream = data.get(offset..)?;
    let mut reader = XrBitReader::new(stream);
    let image = match parse_image_header(&mut reader) {
        Ok(image) => image,
        Err(e) => {
            log_warn!("JPEG XR: failed to parse image header at {}: {}", offset, e);
            return None;
        }
    };
    sections.push(Section::new(
        offset as u64,
        None,
        JpegXrSection::ImageHeader(image_header_info(&image, alpha_stream)),
    ));

    let plane = match parse_plane_header(&mut reader, &image) {
        Ok(plane) => plane,
        Err(e) => {
            log_warn!("JPEG XR: failed to parse plane header: {}", e);
            return None;
        }
    };
    sections.push(Section::new(offset as u64, None, JpegXrSection::PlaneHeader(plane_header_info(&plane, false))));

    if image.alpha_channel {
        match parse_plane_header(&mut reader, &image) {
            Ok(alpha) => sections.push(Section::new(
                offset as u64,
                None,
                JpegXrSection::PlaneHeader(plane_header_info(&alpha, true)),
            )),
            Err(e) => log_warn!("JPEG XR: failed to parse alpha plane header: {}", e),
        }
    }

    Some(Headers { image, plane })
}

fn bits_and_format(depth: u32) -> (u8, SampleFormat) {
    match depth {
        0 | 15 => (1, SampleFormat::Unsigned),
        1 => (8, SampleFormat::Unsigned),
        2 => (16, SampleFormat::Unsigned),
        3 => (16, SampleFormat::Signed),
        4 => (16, SampleFormat::Float),
        5 => (32, SampleFormat::Unsigned),
        6 => (32, SampleFormat::Signed),
        7 => (32, SampleFormat::Float),
        8 => (5, SampleFormat::Unsigned),
        9 => (10, SampleFormat::Unsigned),
        10 => (6, SampleFormat::Unsigned),
        _ => (8, SampleFormat::Unsigned),
    }
}

fn color_model(pixel_format: Option<PixelFormatKind>, headers: &Headers, has_alpha: bool) -> ColorModel {
    use PixelFormatKind::*;
    match pixel_format {
        Some(BlackWhite1) => ColorModel::Bilevel,
        Some(Gray8 | Gray16 | Gray16Fixed | Gray16Half | Gray32Fixed | Gray32Float) => ColorModel::Gray,
        Some(Bgra32 | Rgba64 | Rgba64Fixed | Rgba64Half | Rgba128Fixed | Rgba128Float) => ColorModel::Rgba,
        Some(
            Rgb24 | Bgr24 | Bgr32 | Rgb48 | Rgb48Fixed | Rgb48Half | Rgb96Fixed | Rgb128Float | Rgbe32 | Bgr555
            | Bgr565 | Bgr101010,
        ) => ColorModel::Rgb,
        Some(Cmyk32 | Cmyk64) => ColorModel::Cmyk,
        Some(Cmyka40 | Cmyka80) => ColorModel::Cmyka,
        Some(NChannel) => ColorModel::Multichannel,
        Some(Unknown) | None => match headers.image.source_color_format {
            ColorFormat::YOnly => ColorModel::Gray,
            ColorFormat::Cmyk if has_alpha => ColorModel::Cmyka,
            ColorFormat::Cmyk => ColorModel::Cmyk,
            ColorFormat::NComponent => ColorModel::Multichannel,
            _ if has_alpha => ColorModel::Rgba,
            _ => ColorModel::Rgb,
        },
    }
}

fn build_summary(container: Option<&Container>, headers: &Headers, alpha_stream: bool) -> ImageSummary {
    let image = &headers.image;
    let has_alpha = image.alpha_channel || alpha_stream;
    let orientation = container.and_then(|c| c.transformation).map(Orientation::from_bits).unwrap_or(Orientation::None);
    let (width, height) = if orientation.rotates() { (image.height, image.width) } else { (image.width, image.height) };

    let (bits, sample_format) = bits_and_format(image.source_depth_bits);
    let model = color_model(container.and_then(|c| c.pixel_format), headers, has_alpha);
    let channels = headers.plane.num_channels as u16 + u16::from(has_alpha);
    let mut summary = ImageSummary::new(width, height, model, channels, bits);
    summary.sample_format = sample_format;
    summary.has_alpha = has_alpha;

    let order = if image.bitstream_format == BitstreamFormat::Frequency { "frequency order" } else { "spatial order" };
    let overlap = match image.overlap {
        Overlap::None => "no overlap",
        Overlap::One => "one-level overlap",
        Overlap::Two => "two-level overlap",
    };
    let tiles = (image.tile_cols + 1) * (image.tile_rows + 1);
    let mut parts = vec!["JPEG XR".to_string(), order.to_string(), overlap.to_string()];
    if headers.plane.subband != Subband::All {
        parts.push(format!("subbands {:?}", headers.plane.subband).to_lowercase());
    }
    if tiles > 1 {
        parts.push(format!("{} tiles", tiles));
    }
    summary.compression = Some(parts.join(", "));

    summary.subsampling = match headers.plane.internal_color_format {
        ColorFormat::Yuv420 => Some("4:2:0".to_string()),
        ColorFormat::Yuv422 => Some("4:2:2".to_string()),
        _ => None,
    };

    if let Some(icc) = container.and_then(|c| c.icc_profile.as_ref()) {
        match ICCProfile::new(icc) {
            Ok(profile) => summary.icc_profile = Some(profile),
            Err(e) => log_warn!("JPEG XR: failed to parse ICC profile: {}", e),
        }
    }

    summary
}

pub fn scan(data: &[u8]) -> JpegXrInfo {
    let mut sections = Vec::new();

    if container::looks_like_container(data) {
        let mut reader = BitReader::new(Cursor::new(data));
        let mut limits = Limits::default();
        match walk_structure(&mut reader, &mut limits) {
            Ok(structure) => {
                for section in structure.sections {
                    sections.push(Section::new(section.offset, section.length, JpegXrSection::Tiff(section.data)));
                }
            }
            Err(e) => log_warn!("JPEG XR: failed to walk container: {}", e),
        }
    }

    let (container, offset) = match locate_codestream(data) {
        Ok(found) => found,
        Err(e) => {
            log_warn!("{}", e);
            return JpegXrInfo { summary: None, sections };
        }
    };

    let primary = read_headers(data, offset, false, &mut sections);
    let alpha_offset =
        container.as_ref().filter(|c| c.alpha_offset != 0 && c.alpha_byte_count != 0).map(|c| c.alpha_offset);
    let alpha = alpha_offset.and_then(|at| read_headers(data, at, true, &mut sections));

    let summary = primary.as_ref().map(|headers| build_summary(container.as_ref(), headers, alpha.is_some()));
    sections.sort_by_key(|section| section.offset);
    JpegXrInfo { summary, sections }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn qp_text(values: &[u8]) -> String {
    values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
}

impl InfoSection for JpegXrSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            JpegXrSection::Tiff(section) => section.title(),
            JpegXrSection::ImageHeader(header) => {
                Cow::Borrowed(if header.alpha_stream { "Image Header (alpha codestream)" } else { "Image Header" })
            }
            JpegXrSection::PlaneHeader(plane) => {
                Cow::Borrowed(if plane.alpha_plane { "Plane Header (alpha)" } else { "Plane Header" })
            }
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            JpegXrSection::Tiff(section) => section.describe(out),
            JpegXrSection::ImageHeader(header) => {
                out.field("Version", format!("{}.{}", header.version, header.subversion));
                out.field("Hard tiling", yes_no(header.subversion & 0x8 != 0));
                out.field("Bitstream order", if header.frequency_order { "Frequency" } else { "Spatial" });
                out.field("Spatial transform", header.spatial_transform);
                out.field("Index table", yes_no(header.index_table));
                out.field("Overlap", header.overlap);
                out.field("Short header", yes_no(header.short_header));
                out.field("Long word", yes_no(header.long_word));
                out.field("Trim flexbits", yes_no(header.trim_flexbits));
                out.field("Red/blue swapped", yes_no(header.red_blue_swapped));
                out.field("Premultiplied alpha", yes_no(header.premultiplied_alpha));
                out.field("Alpha plane", yes_no(header.alpha_plane));
                out.field("Source color format", color_format_name(header.source_color_format));
                out.field("Source bit depth", bit_depth_name(header.source_bit_depth));
                out.field("Dimensions", format!("{}x{}", header.width, header.height));
                if header.tiling {
                    out.field("Tiles", format!("{}x{}", header.tile_columns, header.tile_rows));
                    out.field("Tile stretch", yes_no(header.tile_stretch));
                }
                if header.windowing {
                    let [top, left, bottom, right] = header.window;
                    out.field("Window", format!("top {}, left {}, bottom {}, right {}", top, left, bottom, right));
                }
            }
            JpegXrSection::PlaneHeader(plane) => {
                out.field("Internal color format", color_format_name(plane.internal_color_format));
                out.field("Scaled arithmetic", yes_no(plane.scaled_arithmetic));
                let subband = match plane.subband {
                    0 => "All (0)",
                    1 => "No flexbits (1)",
                    2 => "No highpass (2)",
                    3 => "DC only (3)",
                    _ => "Isolated (4)",
                };
                out.field("Subbands", subband);
                out.field("Channels", plane.channels);
                if plane.mantissa_length != 0 || plane.exponent_bias != 0 {
                    out.field("Mantissa length", plane.mantissa_length);
                    out.field("Exponent bias", plane.exponent_bias);
                }
                out.field("QP mode", format!("0x{:03X}", plane.qp_mode));
                out.field("DC QP", qp_text(&plane.dc_qp));
                if plane.subband < 3 {
                    out.field("LP QP", qp_text(&plane.lp_qp));
                }
                if plane.subband < 2 {
                    out.field("HP QP", qp_text(&plane.hp_qp));
                }
            }
        }
    }
}
