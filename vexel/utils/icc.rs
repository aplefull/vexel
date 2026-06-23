use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Read, Result};
use tsify::Tsify;

fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn read_u16_be(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_u32_be(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]))
}

fn read_s15_fixed16(data: &[u8], offset: usize) -> Option<f64> {
    let raw = read_u32_be(data, offset)? as i32;
    Some(raw as f64 / 65536.0)
}

fn sig_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn decode_utf16_be(data: &[u8]) -> String {
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&words)
        .trim_end_matches('\0')
        .to_string()
}

#[derive(Debug, Clone, Serialize, Tsify)]
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
    MultiLocalizedUnicode(Vec<LocalizedString>),
    XYZ(Vec<XYZNumber>),
    Curve(CurveData),
    ParametricCurve(ParametricCurveData),
    Measurement(MeasurementData),
    DateTime(DateTimeNumber),
    Signature(String),
    Data(DataTagData),
    Chromaticity(ChromaticityData),
    ColorantOrder(Vec<u8>),
    ColorantTable(Vec<ColorantEntry>),
    ViewingConditions(ViewingConditionsData),
    S15Fixed16Array(Vec<f64>),
    U16Fixed16Array(Vec<f64>),
    UInt8Array(Vec<u8>),
    UInt16Array(Vec<u16>),
    UInt32Array(Vec<u32>),
    UInt64Array(Vec<[u32; 2]>),
    Float32Array(Vec<f32>),
    Float64Array(Vec<f64>),
    Cicp(CicpData),
    TextDescription(TextDescriptionData),
    Unknown(Vec<u8>),
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

fn decode_rendering_intent(value: u32) -> String {
    match value & 0xFFFF {
        0 => "Perceptual".to_string(),
        1 => "Relative Colorimetric".to_string(),
        2 => "Saturation".to_string(),
        3 => "Absolute Colorimetric".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn decode_profile_class(sig: &str) -> String {
    let trimmed = sig.trim_end_matches('\0').trim();
    match trimmed {
        "scnr" => "Input Device Profile".to_string(),
        "mntr" => "Display Device Profile".to_string(),
        "prtr" => "Output Device Profile".to_string(),
        "link" => "DeviceLink Profile".to_string(),
        "abst" => "Abstract Profile".to_string(),
        "spac" => "ColorSpace Profile".to_string(),
        "nmcl" => "Named Color Profile".to_string(),
        "cenc" => "Color Encoding Profile".to_string(),
        _ => sig.to_string(),
    }
}

fn decode_color_space(sig: &str) -> String {
    let trimmed = sig.trim_end_matches('\0').trim();
    match trimmed {
        "XYZ " => "XYZ".to_string(),
        "Lab " => "CIE L*a*b*".to_string(),
        "RGB " => "RGB".to_string(),
        "GRAY" => "Grayscale".to_string(),
        "CMYK" => "CMYK".to_string(),
        "CMY " => "CMY".to_string(),
        "HSV " => "HSV".to_string(),
        "HLS " => "HLS".to_string(),
        "YCbr" => "YCbCr".to_string(),
        "Yxy " => "CIE Yxy".to_string(),
        "Luv " => "CIE L*u*v*".to_string(),
        _ => sig.to_string(),
    }
}

fn decode_platform(sig: &str) -> String {
    let trimmed = sig.trim_end_matches('\0').trim();
    match trimmed {
        "APPL" => "Macintosh".to_string(),
        "MSFT" => "Microsoft".to_string(),
        "SUNW" => "Solaris".to_string(),
        "SGI " => "SGI".to_string(),
        "TGNT" => "Taligent".to_string(),
        "\0\0\0\0" | "" => "Unspecified".to_string(),
        _ => sig.to_string(),
    }
}

fn decode_standard_observer(value: u32) -> String {
    match value {
        0 => "Unknown".to_string(),
        1 => "CIE 1931 2°".to_string(),
        2 => "CIE 1964 10°".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn decode_measurement_geometry(value: u32) -> String {
    match value {
        0 => "Unknown".to_string(),
        1 => "0°/45° or 45°/0°".to_string(),
        2 => "0°/d or d/0°".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn decode_illuminant(value: u32) -> String {
    match value {
        0 => "Unknown".to_string(),
        1 => "D50".to_string(),
        2 => "D65".to_string(),
        3 => "D93".to_string(),
        4 => "F2".to_string(),
        5 => "D55".to_string(),
        6 => "A".to_string(),
        7 => "Equi-Power E".to_string(),
        8 => "F8".to_string(),
        9 => "Black Body".to_string(),
        10 => "Daylight".to_string(),
        11 => "B".to_string(),
        12 => "C".to_string(),
        13 => "F1".to_string(),
        14 => "F3".to_string(),
        15 => "F4".to_string(),
        16 => "F5".to_string(),
        17 => "F6".to_string(),
        18 => "F7".to_string(),
        19 => "F9".to_string(),
        20 => "F10".to_string(),
        21 => "F11".to_string(),
        22 => "F12".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn decode_colorant_type(value: u16) -> String {
    match value {
        0x0000 => "Unknown".to_string(),
        0x0001 => "ITU-R BT.709".to_string(),
        0x0002 => "SMPTE RP145-1994".to_string(),
        0x0003 => "EBU Tech.3213-E".to_string(),
        0x0004 => "P22".to_string(),
        v => format!("Unknown(0x{:04X})", v),
    }
}

fn parse_xyz_number(data: &[u8], offset: usize) -> Option<XYZNumber> {
    Some(XYZNumber {
        x: read_s15_fixed16(data, offset)?,
        y: read_s15_fixed16(data, offset + 4)?,
        z: read_s15_fixed16(data, offset + 8)?,
    })
}

fn parse_text_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let text_bytes = &data[8..];
    let text = String::from_utf8_lossy(text_bytes)
        .trim_end_matches('\0')
        .to_string();
    ICCTagData::Text(text)
}

fn parse_mluc_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(count) = read_u32_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };
    let Some(record_size) = read_u32_be(data, 12) else {
        return ICCTagData::Unknown(data.to_vec());
    };
    if record_size < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }

    let mut strings = Vec::new();
    for i in 0..count as usize {
        let record_offset = 16 + i * record_size as usize;
        if record_offset + 12 > data.len() {
            break;
        }
        let lang_bytes = &data[record_offset..record_offset + 2];
        let country_bytes = &data[record_offset + 2..record_offset + 4];
        let language = String::from_utf8_lossy(lang_bytes).to_string();
        let country = String::from_utf8_lossy(country_bytes).to_string();

        let Some(str_len) = read_u32_be(data, record_offset + 4) else { continue };
        let Some(str_offset) = read_u32_be(data, record_offset + 8) else { continue };

        let start = str_offset as usize;
        let end = start + str_len as usize;
        if end > data.len() {
            continue;
        }

        let text = decode_utf16_be(&data[start..end]);
        strings.push(LocalizedString { language, country, text });
    }
    ICCTagData::MultiLocalizedUnicode(strings)
}

fn parse_xyz_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 12;
    let mut xyzs = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(xyz) = parse_xyz_number(payload, i * 12) {
            xyzs.push(xyz);
        }
    }
    ICCTagData::XYZ(xyzs)
}

fn parse_curve_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(count) = read_u32_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };

    let curve = match count {
        0 => CurveData::Identity,
        1 => {
            if data.len() < 14 {
                return ICCTagData::Unknown(data.to_vec());
            }
            let val = read_u16_be(data, 12).unwrap_or(0);
            CurveData::Gamma(val as f64 / 256.0)
        }
        n => {
            let mut table = Vec::with_capacity(n as usize);
            for i in 0..n as usize {
                let offset = 12 + i * 2;
                if let Some(v) = read_u16_be(data, offset) {
                    table.push(v);
                }
            }
            CurveData::Table(table)
        }
    };
    ICCTagData::Curve(curve)
}

fn parse_para_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(func_type) = read_u16_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };

    let param_count = match func_type {
        0 => 1,
        1 => 3,
        2 => 4,
        3 => 5,
        4 => 7,
        _ => 0,
    };

    let mut params = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let offset = 12 + i * 4;
        if let Some(v) = read_s15_fixed16(data, offset) {
            params.push(v);
        }
    }

    ICCTagData::ParametricCurve(ParametricCurveData { function_type: func_type, params })
}

fn parse_meas_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 36 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let observer = read_u32_be(data, 8).unwrap_or(0);
    let backing_xyz = parse_xyz_number(data, 12).unwrap_or(XYZNumber { x: 0.0, y: 0.0, z: 0.0 });
    let geometry = read_u32_be(data, 24).unwrap_or(0);
    let flare_raw = read_u32_be(data, 28).unwrap_or(0);
    let flare = flare_raw as f64 / 65536.0;
    let illuminant = read_u32_be(data, 32).unwrap_or(0);

    ICCTagData::Measurement(MeasurementData {
        standard_observer: decode_standard_observer(observer),
        backing_xyz,
        geometry: decode_measurement_geometry(geometry),
        flare,
        illuminant: decode_illuminant(illuminant),
    })
}

fn parse_dtim_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 20 {
        return ICCTagData::Unknown(data.to_vec());
    }
    ICCTagData::DateTime(DateTimeNumber {
        year: read_u16_be(data, 8).unwrap_or(0),
        month: read_u16_be(data, 10).unwrap_or(0),
        day: read_u16_be(data, 12).unwrap_or(0),
        hours: read_u16_be(data, 14).unwrap_or(0),
        minutes: read_u16_be(data, 16).unwrap_or(0),
        seconds: read_u16_be(data, 18).unwrap_or(0),
    })
}

fn parse_sig_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let sig = sig_to_string(&data[8..12]);
    ICCTagData::Signature(sig)
}

fn parse_data_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let flag = read_u32_be(data, 8).unwrap_or(0);
    let payload = data[12..].to_vec();
    ICCTagData::Data(DataTagData { flag, data: payload })
}

fn parse_chrm_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(num_channels) = read_u16_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };
    let Some(colorant_type) = read_u16_be(data, 10) else {
        return ICCTagData::Unknown(data.to_vec());
    };

    let mut channels = Vec::with_capacity(num_channels as usize);
    for i in 0..num_channels as usize {
        let offset = 12 + i * 8;
        if offset + 8 > data.len() {
            break;
        }
        let x = read_u32_be(data, offset).unwrap_or(0) as f64 / 65536.0;
        let y = read_u32_be(data, offset + 4).unwrap_or(0) as f64 / 65536.0;
        channels.push([x, y]);
    }

    ICCTagData::Chromaticity(ChromaticityData {
        colorant_type: decode_colorant_type(colorant_type),
        channels,
    })
}

fn parse_clro_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(count) = read_u32_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };
    let order: Vec<u8> = data[12..].iter().take(count as usize).copied().collect();
    ICCTagData::ColorantOrder(order)
}

fn parse_clrt_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(count) = read_u32_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };
    let mut entries = Vec::with_capacity(count as usize);
    let mut offset = 12;
    for _ in 0..count {
        if offset + 38 > data.len() {
            break;
        }
        let name_bytes = &data[offset..offset + 32];
        let name = String::from_utf8_lossy(name_bytes)
            .trim_end_matches('\0')
            .to_string();
        let pcs = [
            read_u16_be(data, offset + 32).unwrap_or(0),
            read_u16_be(data, offset + 34).unwrap_or(0),
            read_u16_be(data, offset + 36).unwrap_or(0),
        ];
        entries.push(ColorantEntry { name, pcs });
        offset += 38;
    }
    ICCTagData::ColorantTable(entries)
}

fn parse_view_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 36 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let illuminant_xyz = parse_xyz_number(data, 8).unwrap_or(XYZNumber { x: 0.0, y: 0.0, z: 0.0 });
    let surround_xyz = parse_xyz_number(data, 20).unwrap_or(XYZNumber { x: 0.0, y: 0.0, z: 0.0 });
    let illuminant_type = read_u32_be(data, 32).unwrap_or(0);
    ICCTagData::ViewingConditions(ViewingConditionsData {
        illuminant_xyz,
        surround_xyz,
        illuminant_type: decode_illuminant(illuminant_type),
    })
}

fn parse_sf32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 4;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(v) = read_s15_fixed16(payload, i * 4) {
            values.push(v);
        }
    }
    ICCTagData::S15Fixed16Array(values)
}

fn parse_uf32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 4;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(raw) = read_u32_be(payload, i * 4) {
            values.push(raw as f64 / 65536.0);
        }
    }
    ICCTagData::U16Fixed16Array(values)
}

fn parse_ui08_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    ICCTagData::UInt8Array(data[8..].to_vec())
}

fn parse_ui16_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 2;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(v) = read_u16_be(payload, i * 2) {
            values.push(v);
        }
    }
    ICCTagData::UInt16Array(values)
}

fn parse_ui32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 4;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(v) = read_u32_be(payload, i * 4) {
            values.push(v);
        }
    }
    ICCTagData::UInt32Array(values)
}

fn parse_ui64_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 8;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 8;
        if offset + 8 > payload.len() {
            break;
        }
        let hi = read_u32_be(payload, offset).unwrap_or(0);
        let lo = read_u32_be(payload, offset + 4).unwrap_or(0);
        values.push([hi, lo]);
    }
    ICCTagData::UInt64Array(values)
}

fn parse_fl32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 4;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 4;
        if offset + 4 > payload.len() {
            break;
        }
        let bits = read_u32_be(payload, offset).unwrap_or(0);
        values.push(f32::from_bits(bits));
    }
    ICCTagData::Float32Array(values)
}

fn parse_fl64_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let payload = &data[8..];
    let count = payload.len() / 8;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 8;
        if offset + 8 > payload.len() {
            break;
        }
        let hi = read_u32_be(payload, offset).unwrap_or(0) as u64;
        let lo = read_u32_be(payload, offset + 4).unwrap_or(0) as u64;
        let bits = (hi << 32) | lo;
        values.push(f64::from_bits(bits));
    }
    ICCTagData::Float64Array(values)
}

fn parse_cicp_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    ICCTagData::Cicp(CicpData {
        color_primaries: read_u8(data, 8).unwrap_or(0),
        transfer_characteristics: read_u8(data, 9).unwrap_or(0),
        matrix_coefficients: read_u8(data, 10).unwrap_or(0),
        video_full_range_flag: read_u8(data, 11).unwrap_or(0),
    })
}

fn parse_desc_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return ICCTagData::Unknown(data.to_vec());
    }
    let Some(ascii_len) = read_u32_be(data, 8) else {
        return ICCTagData::Unknown(data.to_vec());
    };

    let ascii_end = 12 + ascii_len as usize;
    if ascii_end > data.len() {
        return ICCTagData::Unknown(data.to_vec());
    }
    let ascii = String::from_utf8_lossy(&data[12..ascii_end])
        .trim_end_matches('\0')
        .to_string();

    let mut unicode_language_code = 0u32;
    let mut unicode = String::new();
    let mut script_code = 0u16;
    let mut script = Vec::new();

    if ascii_end + 6 <= data.len() {
        unicode_language_code = read_u32_be(data, ascii_end).unwrap_or(0);
        let unicode_count = read_u32_be(data, ascii_end + 4).unwrap_or(0);
        let unicode_start = ascii_end + 8;
        let unicode_byte_len = unicode_count as usize * 2;
        let unicode_end = unicode_start + unicode_byte_len;
        if unicode_end <= data.len() {
            unicode = decode_utf16_be(&data[unicode_start..unicode_end]);

            if unicode_end + 3 <= data.len() {
                script_code = read_u16_be(data, unicode_end).unwrap_or(0);
                let script_len = read_u8(data, unicode_end + 2).unwrap_or(0) as usize;
                let script_start = unicode_end + 3;
                let script_end = (script_start + script_len).min(data.len());
                script = data[script_start..script_end].to_vec();
            }
        }
    }

    ICCTagData::TextDescription(TextDescriptionData {
        ascii,
        unicode_language_code,
        unicode,
        script_code,
        script,
    })
}

fn parse_tag(type_sig: &str, data: &[u8]) -> ICCTagData {
    match type_sig.trim_end_matches('\0') {
        "text" => parse_text_tag(data),
        "mluc" => parse_mluc_tag(data),
        "XYZ " => parse_xyz_tag(data),
        "curv" => parse_curve_tag(data),
        "para" => parse_para_tag(data),
        "meas" => parse_meas_tag(data),
        "dtim" => parse_dtim_tag(data),
        "sig " => parse_sig_tag(data),
        "data" => parse_data_tag(data),
        "chrm" => parse_chrm_tag(data),
        "clro" => parse_clro_tag(data),
        "clrt" => parse_clrt_tag(data),
        "view" => parse_view_tag(data),
        "sf32" => parse_sf32_tag(data),
        "uf32" => parse_uf32_tag(data),
        "ui08" => parse_ui08_tag(data),
        "ui16" => parse_ui16_tag(data),
        "ui32" => parse_ui32_tag(data),
        "ui64" => parse_ui64_tag(data),
        "fl32" | "fct " => parse_fl32_tag(data),
        "fl64" => parse_fl64_tag(data),
        "cicp" => parse_cicp_tag(data),
        "desc" => parse_desc_tag(data),
        _ => ICCTagData::Unknown(data.to_vec()),
    }
}

impl ICCProfile {
    pub fn new(data: &[u8]) -> Result<ICCProfile> {
        let mut reader = Cursor::new(data);
        let header = ICCProfile::read_header(&mut reader)?;
        let tag_table = ICCProfile::read_tag_table(&mut reader)?;
        let tags = ICCProfile::parse_tags(data, &tag_table);

        Ok(ICCProfile { header, tag_table, tags })
    }

    fn read_header<R: Read>(reader: &mut R) -> Result<ProfileHeader> {
        let mut buffer = [0u8; 128];
        reader.read_exact(&mut buffer)?;

        let size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        let preferred_cmm_type = sig_to_string(&buffer[4..8]);
        let raw_version = u32::from_be_bytes([buffer[8], buffer[9], buffer[10], buffer[11]]);
        let major = (raw_version >> 24) & 0xFF;
        let minor = (raw_version >> 20) & 0x0F;
        let patch = (raw_version >> 16) & 0x0F;
        let version = format!("{}.{}.{}", major, minor, patch);

        let profile_class_sig = sig_to_string(&buffer[12..16]);
        let profile_class = decode_profile_class(&profile_class_sig);

        let color_space_sig = sig_to_string(&buffer[16..20]);
        let color_space = decode_color_space(&color_space_sig);

        let pcs_sig = sig_to_string(&buffer[20..24]);
        let pcs = decode_color_space(&pcs_sig);

        let creation_date = DateTimeNumber {
            year: u16::from_be_bytes([buffer[24], buffer[25]]),
            month: u16::from_be_bytes([buffer[26], buffer[27]]),
            day: u16::from_be_bytes([buffer[28], buffer[29]]),
            hours: u16::from_be_bytes([buffer[30], buffer[31]]),
            minutes: u16::from_be_bytes([buffer[32], buffer[33]]),
            seconds: u16::from_be_bytes([buffer[34], buffer[35]]),
        };

        let signature = sig_to_string(&buffer[36..40]);
        let platform_sig = sig_to_string(&buffer[40..44]);
        let platform = decode_platform(&platform_sig);

        let raw_flags = u32::from_be_bytes([buffer[44], buffer[45], buffer[46], buffer[47]]);
        let flags = ProfileFlags {
            raw: raw_flags,
            embedded_profile: (raw_flags & 0x01) != 0,
            use_with_embedded_data_only: (raw_flags & 0x02) != 0,
        };

        let manufacturer = sig_to_string(&buffer[48..52]);
        let model = sig_to_string(&buffer[52..56]);

        let attr_low = u32::from_be_bytes([buffer[60], buffer[61], buffer[62], buffer[63]]);
        let mut attributes_raw = [0u8; 8];
        attributes_raw.copy_from_slice(&buffer[56..64]);
        let attributes = DeviceAttributes {
            raw: attributes_raw,
            reflective: (attr_low & 0x01) == 0,
            glossy: (attr_low & 0x02) == 0,
            positive: (attr_low & 0x04) == 0,
            color: (attr_low & 0x08) == 0,
        };

        let raw_rendering_intent = u32::from_be_bytes([buffer[64], buffer[65], buffer[66], buffer[67]]);
        let rendering_intent = decode_rendering_intent(raw_rendering_intent);

        let illuminant = XYZNumber {
            x: i32::from_be_bytes([buffer[68], buffer[69], buffer[70], buffer[71]]) as f64 / 65536.0,
            y: i32::from_be_bytes([buffer[72], buffer[73], buffer[74], buffer[75]]) as f64 / 65536.0,
            z: i32::from_be_bytes([buffer[76], buffer[77], buffer[78], buffer[79]]) as f64 / 65536.0,
        };

        let creator = sig_to_string(&buffer[80..84]);

        let mut profile_id = [0u8; 16];
        profile_id.copy_from_slice(&buffer[84..100]);

        Ok(ProfileHeader {
            size,
            preferred_cmm_type,
            version,
            profile_class,
            color_space,
            pcs,
            creation_date,
            signature,
            platform,
            flags,
            manufacturer,
            model,
            attributes,
            rendering_intent,
            illuminant,
            creator,
            profile_id,
        })
    }

    fn read_tag_table<R: Read>(reader: &mut R) -> Result<TagTable> {
        let mut buffer = [0u8; 4];
        reader.read_exact(&mut buffer)?;
        let tag_count = u32::from_be_bytes(buffer);

        let mut entries = Vec::with_capacity(tag_count as usize);
        for _ in 0..tag_count {
            let mut entry_buffer = [0u8; 12];
            reader.read_exact(&mut entry_buffer)?;

            let signature = sig_to_string(&entry_buffer[0..4]);
            let offset = u32::from_be_bytes([entry_buffer[4], entry_buffer[5], entry_buffer[6], entry_buffer[7]]);
            let size = u32::from_be_bytes([entry_buffer[8], entry_buffer[9], entry_buffer[10], entry_buffer[11]]);

            entries.push(TagEntry { signature, offset, size });
        }

        Ok(TagTable { tag_count, entries })
    }

    fn parse_tags(data: &[u8], tag_table: &TagTable) -> HashMap<String, ICCTagData> {
        let mut tags = HashMap::new();
        let mut seen_offsets: HashMap<u32, String> = HashMap::new();

        for entry in &tag_table.entries {
            let start = entry.offset as usize;
            let end = start.saturating_add(entry.size as usize);

            if start >= data.len() || end > data.len() || end < start {
                tags.insert(entry.signature.clone(), ICCTagData::Unknown(vec![]));
                continue;
            }

            if let Some(existing_sig) = seen_offsets.get(&entry.offset) {
                if let Some(existing) = tags.get(existing_sig).cloned() {
                    tags.insert(entry.signature.clone(), existing);
                    continue;
                }
            }

            let tag_data = &data[start..end];
            if tag_data.len() < 4 {
                tags.insert(entry.signature.clone(), ICCTagData::Unknown(tag_data.to_vec()));
                continue;
            }

            let type_sig = sig_to_string(&tag_data[0..4]);
            let parsed = parse_tag(&type_sig, tag_data);
            seen_offsets.insert(entry.offset, entry.signature.clone());
            tags.insert(entry.signature.clone(), parsed);
        }

        tags
    }

    pub fn log_info(&self) {
        println!("ICC Profile Information:");
        println!("----------------------");
        println!("Profile Size: {} bytes", self.header.size);
        println!("Version: {}", self.header.version);
        println!("Profile Class: {}", self.header.profile_class);
        println!("Color Space: {}", self.header.color_space);
        println!("PCS: {}", self.header.pcs);
        println!("Platform: {}", self.header.platform);
        println!("Manufacturer: {}", self.header.manufacturer);
        println!("Model: {}", self.header.model);
        println!("\nTag Table:");
        println!("Number of Tags: {}", self.tag_table.tag_count);
        for entry in &self.tag_table.entries {
            println!(
                "  {} - Offset: {}, Size: {} bytes",
                entry.signature, entry.offset, entry.size
            );
        }
    }
}
