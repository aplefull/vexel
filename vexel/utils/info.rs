use crate::decoders::avif::{AvifColorInfo, AvifFrameInfo, AvifProperties};
use crate::decoders::bmp::{BitmapFileHeader, ColorEntry, DibHeader};
use crate::decoders::gif::{ApplicationExtension, GifFrameInfo, PlainTextExtension};
use crate::decoders::hdr::HdrFormat;
use crate::decoders::jpeg::types::{
    ArithmeticCodingTable, ColorComponentInfo, ExifHeader, JFIFHeader, JpegCodingMethod, JpegMode, QuantizationTable,
    ScanInfo,
};
use crate::decoders::netpbm::{NetpbmFormat, TupleType};
use crate::decoders::png::PngChunkInfo;
use crate::decoders::webp::{AlphaChunkInfo, WebpAnimationInfo, WebpCompressionType, WebpExtendedInfo, WebpFrame};
use serde::Serialize;
use std::fmt;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum ImageInfo {
    Jpeg(JpegInfo),
    Png(PngInfo),
    Bmp(BmpInfo),
    Gif(GifInfo),
    Netpbm(NetpbmInfo),
    Hdr(HdrInfo),
    Webp(WebpInfo),
    Avif(AvifInfo),
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
    pub number_of_components: u8,
    pub mode: JpegMode,
    pub coding_method: JpegCodingMethod,
    pub jfif_header: Option<JFIFHeader>,
    pub exif_header: Option<ExifHeader>,
    pub quantization_tables: Vec<QuantizationTable>,
    pub ac_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub dc_arithmetic_tables: Vec<ArithmeticCodingTable>,
    pub scans: Vec<ScanInfo>,
    pub color_components: Vec<ColorComponentInfo>,
    pub spectral_selection_start: u8,
    pub spectral_selection_end: u8,
    pub successive_approximation_high: u8,
    pub successive_approximation_low: u8,
    pub horizontal_sampling_factor: u8,
    pub vertical_sampling_factor: u8,
    pub restart_interval: u16,
    pub comments: Vec<String>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PngInfo {
    pub chunks: Vec<PngChunkInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct BmpInfo {
    pub width: u32,
    pub height: u32,
    pub file_header: BitmapFileHeader,
    pub dib_header: DibHeader,
    pub color_table: Vec<ColorEntry>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct GifInfo {
    pub width: u32,
    pub height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub version: String,
    pub global_color_table_flag: bool,
    pub color_resolution: u8,
    pub sort_flag: bool,
    pub size_of_global_color_table: u8,
    pub background_color_index: u8,
    pub pixel_aspect_ratio: u8,
    pub global_color_table: Vec<u8>,
    pub frames: Vec<GifFrameInfo>,
    pub comments: Vec<String>,
    pub app_extensions: Vec<ApplicationExtension>,
    pub plain_text_extensions: Vec<PlainTextExtension>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct NetpbmInfo {
    pub width: u32,
    pub height: u32,
    pub max_value: u32,
    pub depth: u8,
    pub format: Option<NetpbmFormat>,
    pub tuple_type: Option<TupleType>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct HdrInfo {
    pub width: u32,
    pub height: u32,
    pub gamma: Option<f32>,
    pub exposure: Option<f32>,
    pub pixel_aspect_ratio: Option<f32>,
    pub color_correction: Option<[f32; 3]>,
    pub primaries: Option<[f32; 8]>,
    pub format: HdrFormat,
    pub comments: Vec<String>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct WebpInfo {
    pub width: u32,
    pub height: u32,
    pub compression_type: WebpCompressionType,
    pub has_alpha: bool,
    pub has_animation: bool,
    pub animation_info: Option<WebpAnimationInfo>,
    pub frames: Vec<WebpFrame>,
    pub extended_info: Option<WebpExtendedInfo>,
    pub background_color: Option<[u8; 4]>,
    pub alpha_info: Option<AlphaChunkInfo>,
}

#[derive(Debug, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct AvifInfo {
    pub width: u32,
    pub height: u32,
    pub color_info: AvifColorInfo,
    pub is_animated: bool,
    pub loop_count: u32,
    pub properties: AvifProperties,
    pub frames: Vec<AvifFrameInfo>,
}

impl fmt::Display for ImageInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageInfo::Jpeg(info) => write!(f, "{}", info),
            ImageInfo::Png(info) => write!(f, "{}", info),
            ImageInfo::Bmp(info) => write!(f, "{}", info),
            ImageInfo::Gif(info) => write!(f, "{}", info),
            ImageInfo::Netpbm(info) => write!(f, "{}", info),
            ImageInfo::Hdr(info) => write!(f, "{}", info),
            ImageInfo::Webp(info) => write!(f, "{}", info),
            ImageInfo::Avif(info) => write!(f, "{}", info),
        }
    }
}

impl fmt::Display for PngInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "PNG Info")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Total chunks: {}", self.chunks.len())?;
        writeln!(f)?;

        for (idx, chunk) in self.chunks.iter().enumerate() {
            writeln!(f, "Chunk #{}: {} (offset: 0x{:08X}, length: {} bytes)",
                idx + 1, chunk.chunk_type, chunk.start_offset, chunk.length)?;

            match &chunk.data {
                crate::decoders::png::PngChunkData::IHDR(data) => {
                    writeln!(f, "  Width: {}", data.width)?;
                    writeln!(f, "  Height: {}", data.height)?;
                    writeln!(f, "  Bit Depth: {}", data.bit_depth)?;
                    writeln!(f, "  Color Type: {:?}", data.color_type)?;
                    writeln!(f, "  Compression: {}", data.compression_method)?;
                    writeln!(f, "  Filter: {}", data.filter_method)?;
                    writeln!(f, "  Interlace: {}", data.interlace_method)?;
                }
                crate::decoders::png::PngChunkData::PLTE(data) => {
                    writeln!(f, "  Palette entries: {}", data.entries.len())?;
                }
                crate::decoders::png::PngChunkData::IDAT(data) => {
                    writeln!(f, "  Data length: {} bytes", data.data_length)?;
                }
                crate::decoders::png::PngChunkData::GAMA(data) => {
                    writeln!(f, "  Gamma: {} (raw: {})", data.gamma, data.gamma_raw)?;
                }
                crate::decoders::png::PngChunkData::CHRM(data) => {
                    writeln!(f, "  White point: ({}, {})", data.chromaticities.white_point_x, data.chromaticities.white_point_y)?;
                    writeln!(f, "  Red: ({}, {})", data.chromaticities.red_x, data.chromaticities.red_y)?;
                    writeln!(f, "  Green: ({}, {})", data.chromaticities.green_x, data.chromaticities.green_y)?;
                    writeln!(f, "  Blue: ({}, {})", data.chromaticities.blue_x, data.chromaticities.blue_y)?;
                }
                crate::decoders::png::PngChunkData::TRNS(data) => {
                    match &data.transparency {
                        crate::decoders::png::TransparencyData::Grayscale(v) => {
                            writeln!(f, "  Transparent value: {}", v)?;
                        }
                        crate::decoders::png::TransparencyData::RGB(r, g, b) => {
                            writeln!(f, "  Transparent color: RGB({}, {}, {})", r, g, b)?;
                        }
                        crate::decoders::png::TransparencyData::Palette(vals) => {
                            writeln!(f, "  Palette alpha values: {} entries", vals.len())?;
                        }
                    }
                }
                crate::decoders::png::PngChunkData::BKGD(data) => {
                    match &data.background {
                        crate::decoders::png::BackgroundData::Grayscale(v) => {
                            writeln!(f, "  Background value: {}", v)?;
                        }
                        crate::decoders::png::BackgroundData::RGB(r, g, b) => {
                            writeln!(f, "  Background color: RGB({}, {}, {})", r, g, b)?;
                        }
                        crate::decoders::png::BackgroundData::PaletteIndex(idx) => {
                            writeln!(f, "  Background palette index: {}", idx)?;
                        }
                    }
                }
                crate::decoders::png::PngChunkData::PHYS(data) => {
                    writeln!(f, "  Pixels per unit X: {}", data.physical_dimensions.pixels_per_unit_x)?;
                    writeln!(f, "  Pixels per unit Y: {}", data.physical_dimensions.pixels_per_unit_y)?;
                    writeln!(f, "  Unit: {:?}", data.physical_dimensions.unit)?;
                }
                crate::decoders::png::PngChunkData::SBIT(data) => {
                    writeln!(f, "  Significant bits: {:?}", data.significant_bits)?;
                }
                crate::decoders::png::PngChunkData::TIME(data) => {
                    writeln!(f, "  Last modified: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                        data.time.year, data.time.month, data.time.day,
                        data.time.hour, data.time.minute, data.time.second)?;
                }
                crate::decoders::png::PngChunkData::TEXT(data) |
                crate::decoders::png::PngChunkData::ZTXT(data) |
                crate::decoders::png::PngChunkData::ITXT(data) => {
                    match &data.text {
                        crate::decoders::png::PngText::Basic { keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text: {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text: {}", text)?;
                            }
                        }
                        crate::decoders::png::PngText::Compressed { keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text (compressed): {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text (compressed): {}", text)?;
                            }
                        }
                        crate::decoders::png::PngText::International { keyword, language_tag, translated_keyword, text } => {
                            writeln!(f, "  Keyword: {}", keyword)?;
                            writeln!(f, "  Language: {}", language_tag)?;
                            writeln!(f, "  Translated keyword: {}", translated_keyword)?;
                            if text.len() > 60 {
                                writeln!(f, "  Text: {}... ({} chars)", &text[..60], text.len())?;
                            } else {
                                writeln!(f, "  Text: {}", text)?;
                            }
                        }
                    }
                }
                crate::decoders::png::PngChunkData::SRGB(data) => {
                    writeln!(f, "  Rendering intent: {:?}", data.rendering_intent)?;
                }
                crate::decoders::png::PngChunkData::ICCP(data) => {
                    writeln!(f, "  Profile name: {}", data.profile_name)?;
                    writeln!(f, "  Profile size: {} bytes", data.profile.header.size)?;
                }
                crate::decoders::png::PngChunkData::SPLT(data) => {
                    writeln!(f, "  Palette name: {}", data.palette.name)?;
                    writeln!(f, "  Sample depth: {} bits", data.palette.sample_depth)?;
                    writeln!(f, "  Samples: {}", data.palette.samples.len())?;
                }
                crate::decoders::png::PngChunkData::HIST(data) => {
                    writeln!(f, "  Histogram entries: {}", data.frequencies.len())?;
                }
                crate::decoders::png::PngChunkData::ACTL(data) => {
                    writeln!(f, "  Number of frames: {}", data.actl.num_frames)?;
                    writeln!(f, "  Number of plays: {}", data.actl.num_plays)?;
                }
                crate::decoders::png::PngChunkData::FCTL(data) => {
                    writeln!(f, "  Sequence: {}", data.fctl.sequence_number)?;
                    writeln!(f, "  Dimensions: {}x{}", data.fctl.width, data.fctl.height)?;
                    writeln!(f, "  Offset: ({}, {})", data.fctl.x_offset, data.fctl.y_offset)?;
                    writeln!(f, "  Delay: {}/{} seconds", data.fctl.delay_num, data.fctl.delay_den)?;
                    writeln!(f, "  Dispose: {}, Blend: {}", data.fctl.dispose_op, data.fctl.blend_op)?;
                }
                crate::decoders::png::PngChunkData::FDAT(data) => {
                    writeln!(f, "  Sequence: {}", data.sequence_number)?;
                    writeln!(f, "  Data length: {} bytes", data.data_length)?;
                }
                crate::decoders::png::PngChunkData::IEND { .. } => {
                    writeln!(f, "  End of image")?;
                }
                crate::decoders::png::PngChunkData::Unknown { chunk_type, length, .. } => {
                    writeln!(f, "  Unknown chunk type: {}", chunk_type)?;
                    writeln!(f, "  Length: {} bytes", length)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl fmt::Display for JpegInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "JPEG Info")?;
        writeln!(f, "======================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Color depth: {} bits", self.color_depth)?;
        writeln!(f, "Components: {}", self.number_of_components)?;
        writeln!(f, "Mode: {:?}", self.mode)?;
        writeln!(f, "Coding method: {:?}", self.coding_method)?;

        if let Some(jfif) = &self.jfif_header {
            writeln!(f, "\nJFIF Header:")?;
            writeln!(f, "  Version: {}.{:02}", jfif.version_major, jfif.version_minor)?;
            writeln!(f, "  Density: {}x{} units", jfif.x_density, jfif.y_density)?;
        }

        if !self.quantization_tables.is_empty() {
            writeln!(f, "\nQuantization tables: {}", self.quantization_tables.len())?;
        }

        if !self.comments.is_empty() {
            writeln!(f, "\nComments:")?;
            for comment in &self.comments {
                writeln!(f, "  {}", comment)?;
            }
        }

        Ok(())
    }
}

impl fmt::Display for BmpInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "BMP Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "File size: {} bytes", self.file_header.file_size)?;
        writeln!(f, "Color table entries: {}", self.color_table.len())?;
        Ok(())
    }
}

impl fmt::Display for GifInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "GIF Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Canvas: {}x{}", self.canvas_width, self.canvas_height)?;
        writeln!(f, "Frames: {}", self.frames.len())?;
        writeln!(f, "Has global color table: {}", self.global_color_table_flag)?;
        if !self.comments.is_empty() {
            writeln!(f, "\nComments:")?;
            for comment in &self.comments {
                writeln!(f, "  {}", comment)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for NetpbmInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Netpbm Image Information")?;
        writeln!(f, "========================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Max value: {}", self.max_value)?;
        writeln!(f, "Depth: {}", self.depth)?;
        if let Some(format) = &self.format {
            writeln!(f, "Format: {:?}", format)?;
        }
        if let Some(tuple_type) = &self.tuple_type {
            writeln!(f, "Tuple type: {:?}", tuple_type)?;
        }
        Ok(())
    }
}

impl fmt::Display for HdrInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "HDR Image Information")?;
        writeln!(f, "=====================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Format: {:?}", self.format)?;
        if let Some(gamma) = self.gamma {
            writeln!(f, "Gamma: {}", gamma)?;
        }
        if let Some(exposure) = self.exposure {
            writeln!(f, "Exposure: {}", exposure)?;
        }
        if !self.comments.is_empty() {
            writeln!(f, "\nComments:")?;
            for comment in &self.comments {
                writeln!(f, "  {}", comment)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for WebpInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "WebP Image Information")?;
        writeln!(f, "======================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Compression: {:?}", self.compression_type)?;
        writeln!(f, "Has alpha: {}", self.has_alpha)?;
        writeln!(f, "Has animation: {}", self.has_animation)?;
        if self.has_animation {
            writeln!(f, "Frames: {}", self.frames.len())?;
        }
        Ok(())
    }
}

impl fmt::Display for AvifInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "AVIF Image Information")?;
        writeln!(f, "======================")?;
        writeln!(f, "Dimensions: {}x{}", self.width, self.height)?;
        writeln!(f, "Animated: {}", self.is_animated)?;
        if self.is_animated {
            writeln!(f, "Frames: {}", self.frames.len())?;
            writeln!(f, "Loop count: {}", self.loop_count)?;
        }
        Ok(())
    }
}
