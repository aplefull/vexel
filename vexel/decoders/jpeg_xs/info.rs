use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::bitreader::XsBitReader;
use super::ids::Ids;
use super::markers::parse_header;
use super::types::*;
use crate::log_warn;
use crate::utils::info::{ColorModel, DataExtent, FormatInfo, ImageSummary, InfoSection, InfoWriter, Section};

const MAX_SLICE_SECTIONS: usize = 256;
const MAX_TEXT_BYTES: usize = 4096;

#[tsify::declare]
pub type JpegXsInfo = FormatInfo<JpegXsSection>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsPictureHeader {
    pub codestream_length: u32,
    pub profile: u16,
    pub level: u8,
    pub sublevel: u8,
    pub width: u16,
    pub height: u16,
    pub column_width: u16,
    pub slice_height: u16,
    pub components: u8,
    pub group_size: u8,
    pub significance_group_size: u8,
    pub bits_per_raw_gcli: u8,
    pub fractional_bits: u8,
    pub raw_bits: u8,
    pub slice_coding: u8,
    pub progression: u8,
    pub color_transform: u8,
    pub horizontal_decompositions: u8,
    pub vertical_decompositions: u8,
    pub long_header: u8,
    pub raw_mode: u8,
    pub quantization: u8,
    pub sign_handling: u8,
    pub run_mode: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsComponent {
    pub bit_depth: u8,
    pub sx: u8,
    pub sy: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsWeight {
    pub gain: u8,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsComment {
    pub comment_type: u16,
    pub text: String,
    pub data_length: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JxsNonlinearity {
    Quadratic { sigma: u8, alpha: u16 },
    Extended { t1: u32, t2: u32, e: u8 },
    Unknown(u8),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsColorTransformSpec {
    pub mode: u8,
    pub e1: u8,
    pub e2: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsSlice {
    pub index: u16,
    pub precincts: u32,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct JxsSliceRemainder {
    pub slices: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum JpegXsSection {
    StartOfCodestream,
    Capabilities(Vec<u8>),
    PictureHeader(JxsPictureHeader),
    ComponentTable(Vec<JxsComponent>),
    Weights(Vec<JxsWeight>),
    Comment(JxsComment),
    Nonlinearity(JxsNonlinearity),
    ComponentDependentWavelet(u8),
    ColorTransformSpec(JxsColorTransformSpec),
    ComponentRegistration(Vec<[u16; 2]>),
    Unknown(u16),
    Slice(JxsSlice),
    MoreSlices(JxsSliceRemainder),
    EndOfCodestream,
}

struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    fn u16(&self, at: usize) -> u16 {
        u16::from_be_bytes([self.u8(at), self.u8(at + 1)])
    }

    fn u24(&self, at: usize) -> u32 {
        u32::from_be_bytes([0, self.u8(at), self.u8(at + 1), self.u8(at + 2)])
    }

    fn u32(&self, at: usize) -> u32 {
        u32::from_be_bytes([self.u8(at), self.u8(at + 1), self.u8(at + 2), self.u8(at + 3)])
    }
}

fn parse_picture_header(body: &[u8]) -> JxsPictureHeader {
    let b = Bytes(body);
    let plev = b.u16(6);
    let packed = b.u8(20);
    let packed2 = b.u8(21);
    let packed3 = b.u8(22);
    let packed4 = b.u8(23);
    JxsPictureHeader {
        codestream_length: b.u32(0),
        profile: b.u16(4),
        level: (plev >> 8) as u8,
        sublevel: plev as u8,
        width: b.u16(8),
        height: b.u16(10),
        column_width: b.u16(12),
        slice_height: b.u16(14),
        components: b.u8(16),
        group_size: b.u8(17),
        significance_group_size: b.u8(18),
        bits_per_raw_gcli: b.u8(19),
        fractional_bits: packed >> 4,
        raw_bits: packed & 0x0F,
        slice_coding: packed2 >> 7,
        progression: (packed2 >> 4) & 0x07,
        color_transform: packed2 & 0x0F,
        horizontal_decompositions: packed3 >> 4,
        vertical_decompositions: packed3 & 0x0F,
        long_header: packed4 >> 7,
        raw_mode: (packed4 >> 6) & 1,
        quantization: (packed4 >> 4) & 3,
        sign_handling: (packed4 >> 2) & 3,
        run_mode: packed4 & 3,
    }
}

fn parse_marker(code: u16, body: &[u8]) -> JpegXsSection {
    let b = Bytes(body);
    match code {
        MARKER_CAP => JpegXsSection::Capabilities(body.to_vec()),
        MARKER_PIH => JpegXsSection::PictureHeader(parse_picture_header(body)),
        MARKER_CDT => JpegXsSection::ComponentTable(
            body.chunks_exact(2).map(|c| JxsComponent { bit_depth: c[0], sx: c[1] >> 4, sy: c[1] & 0x0F }).collect(),
        ),
        MARKER_WGT => {
            JpegXsSection::Weights(body.chunks_exact(2).map(|c| JxsWeight { gain: c[0], priority: c[1] }).collect())
        }
        MARKER_COM => {
            let payload = body.get(2..).unwrap_or(&[]);
            JpegXsSection::Comment(JxsComment {
                comment_type: b.u16(0),
                text: String::from_utf8_lossy(&payload[..payload.len().min(MAX_TEXT_BYTES)])
                    .trim_end_matches('\0')
                    .to_string(),
                data_length: payload.len() as u32,
            })
        }
        MARKER_NLT => JpegXsSection::Nonlinearity(match b.u8(0) {
            1 => JxsNonlinearity::Quadratic { sigma: b.u8(1) >> 7, alpha: b.u16(1) & 0x7FFF },
            2 => JxsNonlinearity::Extended { t1: b.u32(1), t2: b.u32(5), e: b.u8(9) },
            other => JxsNonlinearity::Unknown(other),
        }),
        MARKER_CWD => JpegXsSection::ComponentDependentWavelet(b.u8(0)),
        MARKER_CTS => JpegXsSection::ColorTransformSpec(JxsColorTransformSpec {
            mode: b.u8(0) & 0x0F,
            e1: b.u8(1) >> 4,
            e2: b.u8(1) & 0x0F,
        }),
        MARKER_CRG => JpegXsSection::ComponentRegistration(
            body.chunks_exact(4)
                .map(|c| [u16::from_be_bytes([c[0], c[1]]), u16::from_be_bytes([c[2], c[3]])])
                .collect(),
        ),
        other => JpegXsSection::Unknown(other),
    }
}

struct Scanner<'a> {
    data: &'a [u8],
    sections: Vec<Section<JpegXsSection>>,
}

impl Scanner<'_> {
    fn push(&mut self, offset: usize, length: usize, section: JpegXsSection) {
        self.sections.push(Section::new(offset as u64, Some(length as u64), section));
    }

    fn walk_headers(&mut self) -> usize {
        let b = Bytes(self.data);
        let mut pos = 0usize;
        if b.u16(0) == MARKER_SOC {
            self.push(0, 2, JpegXsSection::StartOfCodestream);
            pos = 2;
        }

        while pos + 4 <= self.data.len() {
            let code = b.u16(pos);
            if code == MARKER_SLH || code == MARKER_EOC || code & 0xFF00 != 0xFF00 {
                break;
            }
            let length = usize::from(b.u16(pos + 2));
            if length < 2 {
                log_warn!("JPEG XS: marker 0x{:04X} has invalid length {}", code, length);
                break;
            }
            let end = (pos + 2 + length).min(self.data.len());
            let section = parse_marker(code, &self.data[pos + 4..end]);
            self.push(pos, end - pos, section);
            pos = end;
        }
        pos
    }

    fn walk_slices(&mut self, start: usize, config: &Config, ids: &Ids) -> usize {
        let b = Bytes(self.data);
        let header_bytes = (24 + 8 + 8 + 2 * ids.nbands).div_ceil(8);
        let slice_height = config.slice_height.max(1) as i32;
        let mut pos = start;
        let mut detailed = 0usize;
        let mut remainder: Option<(usize, JxsSliceRemainder)> = None;
        let mut current: Option<(usize, u16, u32)> = None;

        let mut finish = |scanner: &mut Self, current: Option<(usize, u16, u32)>, end: usize| {
            let Some((offset, index, precincts)) = current else { return };
            let span = end.saturating_sub(offset);
            if detailed < MAX_SLICE_SECTIONS {
                detailed += 1;
                scanner.push(
                    offset,
                    span,
                    JpegXsSection::Slice(JxsSlice { index, precincts, data_length: span.saturating_sub(6) as u64 }),
                );
            } else {
                let entry = remainder.get_or_insert((offset, JxsSliceRemainder { slices: 0, bytes: 0 }));
                entry.1.slices += 1;
                entry.1.bytes += span as u64;
            }
        };

        'rows: for row in 0..ids.npy {
            if (row as i32 * ids.ph) % slice_height == 0 {
                finish(self, current.take(), pos);
                if b.u16(pos) != MARKER_SLH {
                    log_warn!("JPEG XS: expected slice header at offset {}, stopping slice walk", pos);
                    break 'rows;
                }
                current = Some((pos, b.u16(pos + 4), 0));
                pos += 6;
            }
            for _ in 0..ids.npx {
                if pos + header_bytes > self.data.len() {
                    break 'rows;
                }
                let precinct_length = b.u24(pos) as usize;
                pos = pos.saturating_add(header_bytes).saturating_add(precinct_length);
                if let Some((_, _, precincts)) = current.as_mut() {
                    *precincts += 1;
                }
            }
        }
        let end = pos.min(self.data.len());
        finish(self, current.take(), end);

        if let Some((offset, remainder)) = remainder {
            let bytes = remainder.bytes as usize;
            self.push(offset, bytes, JpegXsSection::MoreSlices(remainder));
        }
        end
    }
}

pub fn scan(data: &[u8]) -> JpegXsInfo {
    let mut scanner = Scanner { data, sections: Vec::new() };
    let header_end = scanner.walk_headers();

    let mut reader = XsBitReader::new(data);
    let header = parse_header(&mut reader);
    let mut config = header.config;
    let image = header.image;
    if config.slice_height == 0 {
        config.slice_height = 1 << config.nly.min(16);
    }

    let ids = (header.saw_pih && image.ncomps > 0).then(|| {
        Ids::construct(
            &image,
            i32::from(config.nlx),
            i32::from(config.nly),
            i32::from(config.sd),
            i32::from(config.cw),
            config.lh,
        )
    });

    let mut end = header_end;
    if let Some(Some(ids)) = &ids {
        end = scanner.walk_slices(header_end, &config, ids);
    }

    let tail = data.len().saturating_sub(2);
    let eoc_at = if Bytes(data).u16(end) == MARKER_EOC {
        Some(end)
    } else if data.len() >= 2 && Bytes(data).u16(tail) == MARKER_EOC {
        Some(tail)
    } else {
        None
    };
    if let Some(at) = eoc_at {
        scanner.push(at, 2, JpegXsSection::EndOfCodestream);
    }

    let summary = header.saw_pih.then(|| build_summary(&config, &image, data.len()));
    let mut sections = scanner.sections;
    sections.sort_by_key(|section| section.offset);
    JpegXsInfo { summary, sections }
}

fn build_summary(config: &Config, image: &ImageParams, file_length: usize) -> ImageSummary {
    let is_bayer = image.ncomps == 4 && config.color_transform == ColorTransform::Tetrix;
    let subsampled = (0..image.ncomps.min(MAX_NCOMPS)).any(|c| image.sx[c] != 1 || image.sy[c] != 1);
    let color_model = match image.ncomps {
        _ if is_bayer => ColorModel::Cfa,
        1 => ColorModel::Gray,
        2 => ColorModel::GrayAlpha,
        3 if subsampled => ColorModel::YCbCr,
        3 => ColorModel::Rgb,
        4 => ColorModel::Rgba,
        _ => ColorModel::Multichannel,
    };
    let (width, height) = if is_bayer {
        (image.width.saturating_mul(2), image.height.saturating_mul(2))
    } else {
        (image.width, image.height)
    };

    let mut summary = ImageSummary::new(width, height, color_model, image.ncomps as u16, image.depth);
    summary.has_alpha = matches!(color_model, ColorModel::GrayAlpha | ColorModel::Rgba);

    let mut parts = vec![format!("JPEG XS, {}", profile_name(config.profile))];
    parts.push(format!("{}x{} decompositions", config.nlx, config.nly));
    match config.color_transform {
        ColorTransform::Rct => parts.push("RCT".to_string()),
        ColorTransform::Tetrix => parts.push("Star-Tetrix".to_string()),
        ColorTransform::None => {}
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > 0 && config.bitstream_size > 0 {
        parts.push(format!("{:.2} bpp", config.bitstream_size as f64 * 8.0 / pixels as f64));
    }
    summary.compression = Some(parts.join(", "));

    if image.ncomps > 1 && subsampled {
        let factors: Vec<(u8, u8)> = (0..image.ncomps.min(MAX_NCOMPS))
            .map(|c| (2 / image.sx[c].clamp(1, 2), 2 / image.sy[c].clamp(1, 2)))
            .collect();
        summary.subsampling = crate::decoders::jpeg::info::subsampling_name(&factors);
    }

    if config.bitstream_size > 0 {
        summary.data = Some(DataExtent { expected: config.bitstream_size, actual: file_length as u64 });
    }

    summary
}

fn profile_name(profile: u16) -> String {
    let name = match profile {
        0x0000 => "Unrestricted",
        0x1500 => "Light 422.10",
        0x1A00 => "Light 444.12",
        0x2500 => "Light-Subline 422.10",
        0x3240 => "Main 420.12",
        0x3540 => "Main 422.10",
        0x3A40 => "Main 444.12",
        0x3E40 => "Main 4444.12",
        0x4240 => "High 420.12",
        0x4A40 => "High 444.12",
        0x4E40 => "High 4444.12",
        0x6EC0 => "MLS.12",
        0x9300 => "Light Bayer",
        0xB340 => "Main Bayer",
        0xC340 => "High Bayer",
        _ => "Unknown profile",
    };
    format!("{} (0x{:04X})", name, profile)
}

fn sublevel_name(sublevel: u8) -> String {
    let name = match sublevel {
        0x00 => "Unrestricted",
        0x80 => "Full",
        0x10 => "12 bpp",
        0x0C => "9 bpp",
        0x08 => "6 bpp",
        0x06 => "4 bpp",
        0x04 => "3 bpp",
        0x03 => "2 bpp",
        _ => "Unknown",
    };
    format!("{} (0x{:02X})", name, sublevel)
}

fn describe_picture_header(pih: &JxsPictureHeader, out: &mut InfoWriter) {
    out.field("Codestream length", format!("{} bytes", pih.codestream_length));
    out.field("Profile", profile_name(pih.profile));
    out.field("Level", format!("0x{:02X}", pih.level));
    out.field("Sublevel", sublevel_name(pih.sublevel));
    out.field("Dimensions", format!("{}x{}", pih.width, pih.height));
    if pih.column_width == 0 {
        out.field("Column width", "0 (full width)");
    } else {
        out.field("Column width", format!("{} (raw)", pih.column_width));
    }
    out.field("Slice height", format!("{} precinct rows", pih.slice_height));
    out.field("Components", pih.components);
    out.field("Coefficient group size", pih.group_size);
    out.field("Significance group size", pih.significance_group_size);
    out.field("Bits per raw GCLI", pih.bits_per_raw_gcli);
    out.field("Fractional bits", pih.fractional_bits);
    out.field("Raw bits per GCLI", pih.raw_bits);
    out.field("Slice coding mode", pih.slice_coding);
    out.field("Progression mode", pih.progression);
    let transform = match pih.color_transform {
        0 => "None (0)".to_string(),
        1 => "Reversible color transform (1)".to_string(),
        3 => "Star-Tetrix (3)".to_string(),
        other => format!("Reserved ({})", other),
    };
    out.field("Color transform", transform);
    out.field(
        "Decompositions",
        format!("{} horizontal, {} vertical", pih.horizontal_decompositions, pih.vertical_decompositions),
    );
    out.field("Long precinct headers", if pih.long_header != 0 { "yes" } else { "no" });
    out.field("Raw mode switch", if pih.raw_mode != 0 { "per packet" } else { "off" });
    let quantization = match pih.quantization {
        0 => "Deadzone (0)".to_string(),
        1 => "Uniform (1)".to_string(),
        other => format!("Reserved ({})", other),
    };
    out.field("Quantization", quantization);
    let sign = match pih.sign_handling {
        0 => "Jointly with data (0)".to_string(),
        1 => "Separate sign packets (1)".to_string(),
        other => format!("Reserved ({})", other),
    };
    out.field("Sign handling", sign);
    let run = match pih.run_mode {
        0 => "Zero prediction residuals (0)".to_string(),
        1 => "Zero coefficients (1)".to_string(),
        other => format!("Reserved ({})", other),
    };
    out.field("Run mode", run);
}

impl InfoSection for JpegXsSection {
    fn title(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            JpegXsSection::StartOfCodestream => "SOC (Start of Codestream)",
            JpegXsSection::Capabilities(_) => "CAP (Capabilities)",
            JpegXsSection::PictureHeader(_) => "PIH (Picture Header)",
            JpegXsSection::ComponentTable(_) => "CDT (Component Table)",
            JpegXsSection::Weights(_) => "WGT (Weights Table)",
            JpegXsSection::Comment(_) => "COM (Extension / Comment)",
            JpegXsSection::Nonlinearity(_) => "NLT (Nonlinearity)",
            JpegXsSection::ComponentDependentWavelet(_) => "CWD (Component-dependent Wavelet Decomposition)",
            JpegXsSection::ColorTransformSpec(_) => "CTS (Color Transformation Specification)",
            JpegXsSection::ComponentRegistration(_) => "CRG (Component Registration)",
            JpegXsSection::Unknown(_) => "Unknown Marker",
            JpegXsSection::Slice(_) => "SLH (Slice)",
            JpegXsSection::MoreSlices(_) => "Further Slices",
            JpegXsSection::EndOfCodestream => "EOC (End of Codestream)",
        })
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            JpegXsSection::Capabilities(bits) => {
                let hex = bits.iter().map(|b| format!("{:02X}", b)).collect::<String>();
                out.field("Capability bits", if hex.is_empty() { "none".to_string() } else { format!("0x{}", hex) });
            }
            JpegXsSection::PictureHeader(pih) => describe_picture_header(pih, out),
            JpegXsSection::ComponentTable(components) => {
                for (index, component) in components.iter().enumerate() {
                    out.field(
                        &format!("Component {}", index),
                        format!("{} bits, sampling {}x{}", component.bit_depth, component.sx, component.sy),
                    );
                }
            }
            JpegXsSection::Weights(weights) => {
                out.field("Bands", weights.len());
                let text = weights.iter().map(|w| format!("{}/{}", w.gain, w.priority)).collect::<Vec<_>>().join(", ");
                out.text("Gain/priority", &text);
            }
            JpegXsSection::Comment(comment) => {
                out.field("Type", format!("0x{:04X}", comment.comment_type));
                out.field("Data", format!("{} bytes", comment.data_length));
                out.text("Text", &comment.text);
            }
            JpegXsSection::Nonlinearity(nlt) => match nlt {
                JxsNonlinearity::Quadratic { sigma, alpha } => {
                    out.field("Type", "Quadratic (1)");
                    out.field("Sigma", sigma);
                    out.field("Alpha", alpha);
                }
                JxsNonlinearity::Extended { t1, t2, e } => {
                    out.field("Type", "Extended (2)");
                    out.field("T1", t1);
                    out.field("T2", t2);
                    out.field("E", e);
                }
                JxsNonlinearity::Unknown(kind) => {
                    out.field("Type", format!("Unknown ({}), treated as linear", kind));
                }
            },
            JpegXsSection::ComponentDependentWavelet(sd) => {
                out.field("Components without vertical decomposition", sd);
            }
            JpegXsSection::ColorTransformSpec(cts) => {
                let mode = match cts.mode {
                    0 => "Full (0)".to_string(),
                    3 => "In-line (3)".to_string(),
                    other => format!("Unknown ({}), treated as full", other),
                };
                out.field("Transform mode", mode);
                out.field("Exponents", format!("e1 {}, e2 {}", cts.e1, cts.e2));
            }
            JpegXsSection::ComponentRegistration(offsets) => {
                for (index, [x, y]) in offsets.iter().enumerate() {
                    out.field(&format!("Component {}", index), format!("offset {}/65536, {}/65536", x, y));
                }
            }
            JpegXsSection::Unknown(code) => {
                out.field("Marker", format!("0x{:04X}", code));
            }
            JpegXsSection::Slice(slice) => {
                out.field("Slice index", slice.index);
                out.field("Precincts", slice.precincts);
                out.field("Data", format!("{} bytes", slice.data_length));
            }
            JpegXsSection::MoreSlices(remainder) => {
                out.field("Slices", remainder.slices);
                out.field("Data", format!("{} bytes", remainder.bytes));
            }
            JpegXsSection::StartOfCodestream | JpegXsSection::EndOfCodestream => {}
        }
    }
}
