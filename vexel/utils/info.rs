use crate::decoders::bmp::{BitmapFileHeader, ColorEntry, DibHeader};
use crate::decoders::gif::{ApplicationExtension, GifFrameInfo, PlainTextExtension};
use crate::decoders::hdr::HdrFormat;
use crate::decoders::jpeg::{
    ArithmeticCodingTable, ColorComponentInfo, ExifHeader, JFIFHeader, JpegCodingMethod, JpegMode, QuantizationTable,
    ScanInfo,
};
use crate::decoders::netpbm::{NetpbmFormat, TupleType};
use crate::decoders::png::{
    ActlChunk, BackgroundData, Chromaticities, ColorType, CompressionMethod, ImageTime, PhysicalDimensions, PngFrame,
    PngText, RenderingIntent, SignificantBits, SuggestedPalette, TransparencyData,
};
use crate::utils::icc::ICCProfile;
use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::decoders::avif::{AvifColorInfo, AvifFrameInfo, AvifProperties};
use crate::decoders::webp::{AlphaChunkInfo, WebpAnimationInfo, WebpCompressionType, WebpExtendedInfo, WebpFrame};

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
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: ColorType,
    pub compression_method: CompressionMethod,
    pub has_filters: bool,
    pub interlace: bool,
    pub palette: Option<Vec<[u8; 3]>>,
    pub gamma: Option<f32>,
    pub icc_profile: Option<(String, ICCProfile)>,
    pub transparency: Option<TransparencyData>,
    pub background: Option<BackgroundData>,
    pub rendering_intent: Option<RenderingIntent>,
    pub chromaticities: Option<Chromaticities>,
    pub suggested_palettes: Vec<SuggestedPalette>,
    pub physical_dimensions: Option<PhysicalDimensions>,
    pub significant_bits: Option<SignificantBits>,
    pub histogram: Option<Vec<u16>>,
    pub modification_time: Option<ImageTime>,
    pub text_chunks: Vec<PngText>,
    pub frames: Vec<PngFrame>,
    pub actl_info: Option<ActlChunk>,
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