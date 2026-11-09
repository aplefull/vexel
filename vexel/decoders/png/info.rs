use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::{
    ActlChunk, BackgroundData, Chromaticities, ColorType, FctlChunk, ImageTime, SignificantBits, SuggestedPalette,
    TransparencyData,
};
use crate::utils::info::{FormatInfo, InfoSection, InfoWriter};

#[tsify::declare]
pub type PngInfo = FormatInfo<PngSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum PngSection {
    Signature(PngSignature),
    Chunk(PngChunk),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PngSignature {
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct PngChunk {
    pub chunk_type: String,
    pub data_length: u32,
    pub data_read: u64,
    pub crc: Option<u32>,
    pub crc_valid: Option<bool>,
    pub body: PngChunkBody,
}

impl PngChunk {
    pub fn is_truncated(&self) -> bool {
        self.data_read < u64::from(self.data_length) || self.crc.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IhdrChunk {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub compression_method: u8,
    pub filter_method: u8,
    pub interlace_method: u8,
}

impl IhdrChunk {
    pub fn color_type(&self) -> Option<ColorType> {
        match self.color_type {
            0 => Some(ColorType::Grayscale),
            2 => Some(ColorType::RGB),
            3 => Some(ColorType::Indexed),
            4 => Some(ColorType::GrayscaleAlpha),
            6 => Some(ColorType::RGBA),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GamaChunk {
    pub gamma_raw: u32,
    pub gamma: f32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct IccpChunk {
    pub profile_name: String,
    pub compression_method: u8,
    pub compressed_length: u64,
    pub profile_length: Option<u64>,
    pub profile_parsed: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SrgbChunk {
    pub rendering_intent: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct PhysChunk {
    pub pixels_per_unit_x: u32,
    pub pixels_per_unit_y: u32,
    pub unit: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TextChunk {
    pub keyword: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CompressedTextChunk {
    pub keyword: String,
    pub compression_method: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct InternationalTextChunk {
    pub keyword: String,
    pub compressed: bool,
    pub compression_method: u8,
    pub language_tag: String,
    pub translated_keyword: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FdatChunk {
    pub sequence_number: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub enum PngChunkBody {
    Ihdr(IhdrChunk),
    Plte(Vec<[u8; 3]>),
    Idat,
    Iend,
    Trns(Option<TransparencyData>),
    Chrm(Chromaticities),
    Gama(GamaChunk),
    Iccp(IccpChunk),
    Sbit(SignificantBits),
    Srgb(SrgbChunk),
    Text(TextChunk),
    Ztxt(CompressedTextChunk),
    Itxt(InternationalTextChunk),
    Bkgd(Option<BackgroundData>),
    Phys(PhysChunk),
    Time(ImageTime),
    Splt(SuggestedPalette),
    Hist(Vec<u16>),
    Actl(ActlChunk),
    Fctl(FctlChunk),
    Fdat(FdatChunk),
    Unknown,
}

fn color_type_name(raw: u8) -> String {
    match raw {
        0 => "Grayscale (0)".to_string(),
        2 => "RGB (2)".to_string(),
        3 => "Indexed (3)".to_string(),
        4 => "Grayscale + alpha (4)".to_string(),
        6 => "RGBA (6)".to_string(),
        other => format!("Invalid ({})", other),
    }
}

fn method_name(raw: u8, valid_name: &str) -> String {
    if raw == 0 {
        format!("{} (0)", valid_name)
    } else {
        format!("Invalid ({})", raw)
    }
}

fn hex_colors(entries: &[[u8; 3]]) -> String {
    entries.iter().map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b)).collect::<Vec<_>>().join(", ")
}

fn describe_ihdr(ihdr: &IhdrChunk, out: &mut InfoWriter) {
    out.field("Width", ihdr.width);
    out.field("Height", ihdr.height);
    out.field("Bit depth", ihdr.bit_depth);
    out.field("Color type", color_type_name(ihdr.color_type));
    out.field("Compression", method_name(ihdr.compression_method, "Deflate"));
    out.field("Filter method", method_name(ihdr.filter_method, "Adaptive"));
    let interlace = match ihdr.interlace_method {
        0 => "None (0)".to_string(),
        1 => "Adam7 (1)".to_string(),
        other => format!("Invalid ({})", other),
    };
    out.field("Interlace", interlace);
}

fn describe_fctl(fctl: &FctlChunk, out: &mut InfoWriter) {
    out.field("Sequence", fctl.sequence_number);
    out.field("Dimensions", format!("{}x{}", fctl.width, fctl.height));
    out.field("Offset", format!("({}, {})", fctl.x_offset, fctl.y_offset));
    let denominator = if fctl.delay_den == 0 { 100 } else { fctl.delay_den };
    out.field(
        "Delay",
        format!(
            "{}/{} s ({:.1} ms)",
            fctl.delay_num,
            denominator,
            f64::from(fctl.delay_num) * 1000.0 / f64::from(denominator)
        ),
    );
    let dispose = match fctl.dispose_op {
        0 => "None (0)".to_string(),
        1 => "Background (1)".to_string(),
        2 => "Previous (2)".to_string(),
        other => format!("Invalid ({})", other),
    };
    out.field("Dispose", dispose);
    let blend = match fctl.blend_op {
        0 => "Source (0)".to_string(),
        1 => "Over (1)".to_string(),
        other => format!("Invalid ({})", other),
    };
    out.field("Blend", blend);
}

fn describe_body(body: &PngChunkBody, out: &mut InfoWriter) {
    match body {
        PngChunkBody::Ihdr(ihdr) => describe_ihdr(ihdr, out),
        PngChunkBody::Plte(entries) => {
            out.field("Entries", entries.len());
            out.text("Colors", &hex_colors(entries));
        }
        PngChunkBody::Idat | PngChunkBody::Iend | PngChunkBody::Unknown => {}
        PngChunkBody::Trns(None) => {
            out.field("Transparency", "not allowed for this color type");
        }
        PngChunkBody::Trns(Some(TransparencyData::Grayscale(value))) => {
            out.field("Transparent gray", value);
        }
        PngChunkBody::Trns(Some(TransparencyData::RGB(r, g, b))) => {
            out.field("Transparent color", format!("RGB({}, {}, {})", r, g, b));
        }
        PngChunkBody::Trns(Some(TransparencyData::Palette(alpha))) => {
            out.field("Palette alpha entries", alpha.len());
        }
        PngChunkBody::Chrm(c) => {
            out.field("White point", format!("({}, {})", c.white_point_x, c.white_point_y));
            out.field("Red", format!("({}, {})", c.red_x, c.red_y));
            out.field("Green", format!("({}, {})", c.green_x, c.green_y));
            out.field("Blue", format!("({}, {})", c.blue_x, c.blue_y));
        }
        PngChunkBody::Gama(gama) => {
            out.field("Gamma", format!("{} (raw {})", gama.gamma, gama.gamma_raw));
        }
        PngChunkBody::Iccp(iccp) => {
            out.text("Profile name", &iccp.profile_name);
            out.field("Compression", method_name(iccp.compression_method, "Deflate"));
            out.field("Compressed size", format!("{} bytes", iccp.compressed_length));
            out.opt_field("Profile size", iccp.profile_length.map(|n| format!("{} bytes", n)));
            out.field("Profile parsed", if iccp.profile_parsed { "yes" } else { "no" });
        }
        PngChunkBody::Sbit(bits) => {
            let text = match bits {
                SignificantBits::Grayscale { gray } => format!("gray {}", gray),
                SignificantBits::RGB { red, green, blue } | SignificantBits::Indexed { red, green, blue } => {
                    format!("red {}, green {}, blue {}", red, green, blue)
                }
                SignificantBits::GrayscaleAlpha { gray, alpha } => format!("gray {}, alpha {}", gray, alpha),
                SignificantBits::RGBA { red, green, blue, alpha } => {
                    format!("red {}, green {}, blue {}, alpha {}", red, green, blue, alpha)
                }
            };
            out.field("Significant bits", text);
        }
        PngChunkBody::Srgb(srgb) => {
            let intent = match srgb.rendering_intent {
                0 => "Perceptual (0)".to_string(),
                1 => "Relative colorimetric (1)".to_string(),
                2 => "Saturation (2)".to_string(),
                3 => "Absolute colorimetric (3)".to_string(),
                other => format!("Invalid ({})", other),
            };
            out.field("Rendering intent", intent);
        }
        PngChunkBody::Text(text) => {
            out.text("Keyword", &text.keyword);
            out.text("Text", &text.text);
        }
        PngChunkBody::Ztxt(text) => {
            out.text("Keyword", &text.keyword);
            out.field("Compression", method_name(text.compression_method, "Deflate"));
            out.text("Text", &text.text);
        }
        PngChunkBody::Itxt(text) => {
            out.text("Keyword", &text.keyword);
            if text.compressed {
                out.field("Compression", method_name(text.compression_method, "Deflate"));
            }
            out.text("Language", &text.language_tag);
            out.text("Translated keyword", &text.translated_keyword);
            out.text("Text", &text.text);
        }
        PngChunkBody::Bkgd(None) => {
            out.field("Background", "missing data");
        }
        PngChunkBody::Bkgd(Some(BackgroundData::Grayscale(value))) => {
            out.field("Background gray", value);
        }
        PngChunkBody::Bkgd(Some(BackgroundData::RGB(r, g, b))) => {
            out.field("Background color", format!("RGB({}, {}, {})", r, g, b));
        }
        PngChunkBody::Bkgd(Some(BackgroundData::PaletteIndex(index))) => {
            out.field("Background palette index", index);
        }
        PngChunkBody::Phys(phys) => {
            out.field("Pixels per unit X", phys.pixels_per_unit_x);
            out.field("Pixels per unit Y", phys.pixels_per_unit_y);
            let unit = match phys.unit {
                0 => "Unknown (aspect ratio only)".to_string(),
                1 => {
                    let dpi_x = f64::from(phys.pixels_per_unit_x) * 0.0254;
                    let dpi_y = f64::from(phys.pixels_per_unit_y) * 0.0254;
                    format!("Meter ({:.1}x{:.1} DPI)", dpi_x, dpi_y)
                }
                other => format!("Invalid ({})", other),
            };
            out.field("Unit", unit);
        }
        PngChunkBody::Time(t) => {
            out.field(
                "Last modified",
                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second),
            );
        }
        PngChunkBody::Splt(palette) => {
            out.text("Palette name", &palette.name);
            out.field("Sample depth", format!("{} bits", palette.sample_depth));
            out.field("Entries", palette.samples.len());
        }
        PngChunkBody::Hist(frequencies) => {
            out.field("Entries", frequencies.len());
            let joined = frequencies.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
            out.text("Frequencies", &joined);
        }
        PngChunkBody::Actl(actl) => {
            out.field("Frames", actl.num_frames);
            if actl.num_plays == 0 {
                out.field("Plays", "infinite (0)");
            } else {
                out.field("Plays", actl.num_plays);
            }
        }
        PngChunkBody::Fctl(fctl) => describe_fctl(fctl, out),
        PngChunkBody::Fdat(fdat) => {
            out.field("Sequence", fdat.sequence_number);
        }
    }
}

impl InfoSection for PngSection {
    fn title(&self) -> Cow<'_, str> {
        match self {
            PngSection::Signature(_) => Cow::Borrowed("Signature"),
            PngSection::Chunk(chunk) => Cow::Borrowed(chunk.chunk_type.as_str()),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            PngSection::Signature(signature) => {
                out.field("Valid", if signature.valid { "yes" } else { "no" });
            }
            PngSection::Chunk(chunk) => {
                out.field("Data length", format!("{} bytes", chunk.data_length));
                if chunk.data_read < u64::from(chunk.data_length) {
                    out.field("Data present", format!("{} bytes (truncated)", chunk.data_read));
                }
                let crc = match (chunk.crc, chunk.crc_valid) {
                    (Some(crc), Some(true)) => format!("0x{:08X} (valid)", crc),
                    (Some(crc), Some(false)) => format!("0x{:08X} (mismatch)", crc),
                    (Some(crc), None) => format!("0x{:08X}", crc),
                    (None, _) => "missing".to_string(),
                };
                out.field("CRC", crc);
                describe_body(&chunk.body, out);
            }
        }
    }
}
