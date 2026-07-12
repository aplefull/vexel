use std::collections::HashMap;

use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct ICCProfile {
    pub header: ProfileHeader,
    pub tag_table: TagTable,
    pub tags: HashMap<String, ICCTagData>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ProfileHeader {
    pub size: u32,
    pub preferred_cmm_type: String,
    pub version: String,
    pub profile_class: String,
    pub color_space: String,
    pub pcs: String,
    pub creation_date: DateTimeNumber,
    pub signature: String,
    pub platform: String,
    pub flags: ProfileFlags,
    pub manufacturer: String,
    pub model: String,
    pub attributes: DeviceAttributes,
    pub rendering_intent: String,
    pub illuminant: XYZNumber,
    pub creator: String,
    pub profile_id: [u8; 16],
    pub spectral_pcs: Option<String>,
    pub spectral_range: Option<SpectralRange>,
    pub bi_spectral_range: Option<SpectralRange>,
    pub mcs: Option<String>,
    pub device_sub_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SpectralRange {
    pub start: f32,
    pub end: f32,
    pub steps: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DateTimeNumber {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hours: u16,
    pub minutes: u16,
    pub seconds: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ProfileFlags {
    pub raw: u32,
    pub embedded_profile: bool,
    pub use_with_embedded_data_only: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DeviceAttributes {
    pub raw: [u8; 8],
    pub reflective: bool,
    pub glossy: bool,
    pub positive: bool,
    pub color: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct XYZNumber {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagTable {
    pub tag_count: u32,
    pub entries: Vec<TagEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagEntry {
    pub signature: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(tag = "type", content = "data")]
pub enum ICCTagData {
    Text(String),
    Utf8(String),
    Utf16(String),
    ZipUtf8(String),
    ZipXml(String),
    MultiLocalizedUnicode(Vec<LocalizedString>),
    XYZ(Vec<XYZNumber>),
    Curve(CurveData),
    ParametricCurve(ParametricCurveData),
    SegmentedCurve(SegmentedCurveData),
    Lut8(Lut8Data),
    Lut16(Lut16Data),
    LutAToB(LutABData),
    LutBToA(LutABData),
    NamedColors(NamedColorsData),
    ProfileSequence(Vec<ProfileDescription>),
    ProfileSequenceId(Vec<ProfileIdDescription>),
    ResponseCurveSet(ResponseCurveSetData),
    Dictionary(Vec<DictEntry>),
    MultiProcessElements(MultiProcessElementsData),
    SpectralViewingConditions(Box<SpectralViewingConditionsData>),
    SpectralDataInfo(SpectralDataInfoData),
    TagStruct(TagStructData),
    TagArray(TagArrayData),
    Measurement(MeasurementData),
    DateTime(DateTimeNumber),
    Signature(String),
    Data(DataTagData),
    Chromaticity(ChromaticityData),
    ColorantOrder(Vec<u8>),
    ColorantTable(Vec<ColorantEntry>),
    ViewingConditions(ViewingConditionsData),
    Screening(ScreeningData),
    UcrBg(UcrBgData),
    CrdInfo(CrdInfoData),
    S15Fixed16Array(Vec<f64>),
    U16Fixed16Array(Vec<f64>),
    UInt8Array(Vec<u8>),
    UInt16Array(Vec<u16>),
    UInt32Array(Vec<u32>),
    UInt64Array(Vec<[u32; 2]>),
    Float16Array(Vec<f32>),
    Float32Array(Vec<f32>),
    Float64Array(Vec<f64>),
    Cicp(CicpData),
    TextDescription(TextDescriptionData),
    GamutBoundary(GamutBoundaryData),
    SparseMatrixArray(SparseMatrixArrayData),
    EmbeddedProfile(Box<ICCProfile>),
    EmbeddedImage(EmbeddedImageData),
    Unknown(UnknownTagData),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct GamutBoundaryData {
    pub pcs_channels: u16,
    pub device_channels: u16,
    pub triangles: Vec<[u32; 3]>,
    pub pcs_values: Vec<f32>,
    pub device_values: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SparseMatrixArrayData {
    pub channels: u16,
    pub matrix_type: u16,
    pub matrices: Vec<SparseMatrix>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SparseMatrix {
    pub rows: u16,
    pub cols: u16,
    pub row_starts: Vec<u16>,
    pub column_indices: Vec<u16>,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct EmbeddedImageData {
    pub is_height_map: bool,
    pub seamless: bool,
    pub encoding: String,
    pub height_range_meters: Option<[f32; 2]>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct UnknownTagData {
    pub type_signature: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct LocalizedString {
    pub language: String,
    pub country: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(tag = "kind", content = "value")]
pub enum CurveData {
    Identity,
    Gamma(f64),
    Table(Vec<u16>),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ParametricCurveData {
    pub function_type: u16,
    pub params: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SegmentedCurveData {
    pub break_points: Vec<f32>,
    pub segments: Vec<CurveSegment>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(tag = "kind", content = "value")]
pub enum CurveSegment {
    Formula(FormulaSegment),
    Sampled(Vec<f32>),
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct FormulaSegment {
    pub function_type: u16,
    pub params: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Lut8Data {
    pub input_channels: u8,
    pub output_channels: u8,
    pub clut_grid_points: u8,
    pub matrix: [f64; 9],
    pub input_tables: Vec<Vec<u8>>,
    pub clut: Vec<u8>,
    pub output_tables: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Lut16Data {
    pub input_channels: u8,
    pub output_channels: u8,
    pub clut_grid_points: u8,
    pub matrix: [f64; 9],
    pub input_tables: Vec<Vec<u16>>,
    pub clut: Vec<u16>,
    pub output_tables: Vec<Vec<u16>>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct LutABData {
    pub input_channels: u8,
    pub output_channels: u8,
    pub a_curves: Vec<EmbeddedCurve>,
    pub m_curves: Vec<EmbeddedCurve>,
    pub b_curves: Vec<EmbeddedCurve>,
    pub matrix: Option<Vec<f64>>,
    pub clut: Option<ClutData>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(tag = "kind", content = "value")]
pub enum EmbeddedCurve {
    Curve(CurveData),
    Parametric(ParametricCurveData),
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ClutData {
    pub grid_points: Vec<u8>,
    pub precision: u8,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NamedColorsData {
    pub vendor_flag: u32,
    pub prefix: String,
    pub suffix: String,
    pub device_coord_count: u32,
    pub colors: Vec<NamedColorEntry>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct NamedColorEntry {
    pub name: String,
    pub pcs: [u16; 3],
    pub device: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ProfileDescription {
    pub device_manufacturer: String,
    pub device_model: String,
    pub attributes: DeviceAttributes,
    pub technology: String,
    pub manufacturer_description: String,
    pub model_description: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ProfileIdDescription {
    pub profile_id: Vec<u8>,
    pub description: Vec<LocalizedString>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ResponseCurveSetData {
    pub channels: u16,
    pub measurements: Vec<ResponseCurveMeasurement>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ResponseCurveMeasurement {
    pub unit: String,
    pub max_colorants: Vec<XYZNumber>,
    pub channel_responses: Vec<Vec<ResponseNumber>>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ResponseNumber {
    pub device_code: u16,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DictEntry {
    pub name: String,
    pub value: String,
    pub display_name: Vec<LocalizedString>,
    pub display_value: Vec<LocalizedString>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct MultiProcessElementsData {
    pub input_channels: u16,
    pub output_channels: u16,
    pub elements: Vec<ProcessingElement>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(tag = "kind", content = "value")]
pub enum ProcessingElement {
    CurveSet(CurveSetElement),
    Matrix(MatrixElement),
    Clut(ClutElement),
    Unknown(UnknownElement),
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CurveSetElement {
    pub input_channels: u16,
    pub output_channels: u16,
    pub curves: Vec<SegmentedCurveData>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct MatrixElement {
    pub input_channels: u16,
    pub output_channels: u16,
    pub matrix: Vec<f32>,
    pub constants: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ClutElement {
    pub input_channels: u16,
    pub output_channels: u16,
    pub grid_points: Vec<u8>,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct UnknownElement {
    pub signature: String,
    pub input_channels: u16,
    pub output_channels: u16,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SpectralViewingConditionsData {
    pub standard_observer: String,
    pub observer_range: SpectralRange,
    pub observer_data: Vec<f32>,
    pub standard_illuminant: String,
    pub color_temperature: f32,
    pub illuminant_range: SpectralRange,
    pub illuminant_data: Vec<f32>,
    pub illuminant_xyz: XYZNumber,
    pub surround_xyz: XYZNumber,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct SpectralDataInfoData {
    pub color_signature: String,
    pub spectral_range: SpectralRange,
    pub bi_spectral_range: SpectralRange,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagStructData {
    pub struct_type: String,
    pub members: Vec<TagStructMember>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagStructMember {
    pub signature: String,
    pub data: ICCTagData,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TagArrayData {
    pub array_type: String,
    pub elements: Vec<ICCTagData>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct MeasurementData {
    pub standard_observer: String,
    pub backing_xyz: XYZNumber,
    pub geometry: String,
    pub flare: f64,
    pub illuminant: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct DataTagData {
    pub flag: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ChromaticityData {
    pub colorant_type: String,
    pub channels: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ColorantEntry {
    pub name: String,
    pub pcs: [u16; 3],
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ViewingConditionsData {
    pub illuminant_xyz: XYZNumber,
    pub surround_xyz: XYZNumber,
    pub illuminant_type: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ScreeningData {
    pub flag: u32,
    pub channels: Vec<ScreeningChannel>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ScreeningChannel {
    pub frequency: f64,
    pub angle: f64,
    pub spot_shape: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct UcrBgData {
    pub ucr: Vec<u16>,
    pub bg: Vec<u16>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CrdInfoData {
    pub postscript_product_name: String,
    pub crd_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct CicpData {
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub video_full_range_flag: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TextDescriptionData {
    pub ascii: String,
    pub unicode_language_code: u32,
    pub unicode: String,
    pub script_code: u16,
    pub script: Vec<u8>,
}
