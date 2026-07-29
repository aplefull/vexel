use super::types::*;
use super::{
    decode_utf16_be, parse_xyz_number, read_f16_be, read_f32_be, read_s15_fixed16, read_spectral_range, read_u16_be,
    read_u32_be, read_u8, sig_at, sig_to_string,
};
use crate::utils::deflate::ZlibDecoder;

const MAX_RECURSION_DEPTH: u8 = 6;
const MAX_DECOMPRESSED_TEXT: usize = 16 * 1024 * 1024;

fn unknown_tag(data: &[u8]) -> ICCTagData {
    let type_signature = if data.len() >= 4 { sig_to_string(&data[0..4]) } else { String::new() };
    ICCTagData::Unknown(UnknownTagData { type_signature, data: data.to_vec() })
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

fn decode_spot_shape(value: u32) -> String {
    match value {
        0 => "Unknown".to_string(),
        1 => "Printer Default".to_string(),
        2 => "Round".to_string(),
        3 => "Diamond".to_string(),
        4 => "Ellipse".to_string(),
        5 => "Line".to_string(),
        6 => "Square".to_string(),
        7 => "Cross".to_string(),
        v => format!("Unknown({})", v),
    }
}

fn parametric_param_count(function_type: u16) -> usize {
    match function_type {
        0 => 1,
        1 => 3,
        2 => 4,
        3 => 5,
        4 => 7,
        _ => 0,
    }
}

fn formula_param_count(function_type: u16) -> Option<usize> {
    match function_type {
        0 => Some(4),
        1..=4 => Some(5),
        5 | 7 => Some(6),
        6 => Some(7),
        _ => None,
    }
}

fn parse_text_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let text = String::from_utf8_lossy(&data[8..]).trim_end_matches('\0').to_string();
    ICCTagData::Text(text)
}

fn parse_utf8_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let text = String::from_utf8_lossy(&data[8..]).trim_end_matches('\0').to_string();
    ICCTagData::Utf8(text)
}

fn parse_utf16_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    ICCTagData::Utf16(decode_utf16_be(&data[8..]))
}

fn parse_zip_utf8_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let decompressed = ZlibDecoder::from_slice(&data[8..]).decode_with_max(MAX_DECOMPRESSED_TEXT);
    if decompressed.is_empty() {
        return unknown_tag(data);
    }
    let text = String::from_utf8_lossy(&decompressed).trim_end_matches('\0').to_string();
    ICCTagData::ZipUtf8(text)
}

fn parse_zip_xml_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let decompressed = ZlibDecoder::from_slice(&data[8..]).decode_with_max(MAX_DECOMPRESSED_TEXT);
    if decompressed.is_empty() {
        return unknown_tag(data);
    }
    let text = String::from_utf8_lossy(&decompressed).trim_end_matches('\0').to_string();
    ICCTagData::ZipXml(text)
}

fn parse_mluc_strings(data: &[u8]) -> Option<Vec<LocalizedString>> {
    let count = read_u32_be(data, 8)?;
    let record_size = read_u32_be(data, 12)?;
    if record_size < 12 {
        return None;
    }

    let mut strings = Vec::new();
    for i in 0..count as usize {
        let record_offset = 16usize.checked_add(i.checked_mul(record_size as usize)?)?;
        if record_offset + 12 > data.len() {
            break;
        }
        let language = String::from_utf8_lossy(&data[record_offset..record_offset + 2]).to_string();
        let country = String::from_utf8_lossy(&data[record_offset + 2..record_offset + 4]).to_string();

        let Some(str_len) = read_u32_be(data, record_offset + 4) else { continue };
        let Some(str_offset) = read_u32_be(data, record_offset + 8) else { continue };

        let start = str_offset as usize;
        let Some(end) = start.checked_add(str_len as usize) else { continue };
        if end > data.len() {
            continue;
        }

        strings.push(LocalizedString { language, country, text: decode_utf16_be(&data[start..end]) });
    }
    Some(strings)
}

fn parse_mluc_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    match parse_mluc_strings(data) {
        Some(strings) => ICCTagData::MultiLocalizedUnicode(strings),
        None => unknown_tag(data),
    }
}

fn parse_xyz_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
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

fn parse_curve_data(data: &[u8]) -> Option<CurveData> {
    let count = read_u32_be(data, 8)?;
    match count {
        0 => Some(CurveData::Identity),
        1 => {
            let val = read_u16_be(data, 12)?;
            Some(CurveData::Gamma(val as f64 / 256.0))
        }
        n => {
            let mut table = Vec::new();
            for i in 0..n as usize {
                match read_u16_be(data, 12 + i * 2) {
                    Some(v) => table.push(v),
                    None => break,
                }
            }
            Some(CurveData::Table(table))
        }
    }
}

fn parse_curve_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    match parse_curve_data(data) {
        Some(curve) => ICCTagData::Curve(curve),
        None => unknown_tag(data),
    }
}

fn parse_parametric_data(data: &[u8]) -> Option<ParametricCurveData> {
    let function_type = read_u16_be(data, 8)?;
    let param_count = parametric_param_count(function_type);

    let mut params = Vec::with_capacity(param_count);
    for i in 0..param_count {
        match read_s15_fixed16(data, 12 + i * 4) {
            Some(v) => params.push(v),
            None => break,
        }
    }
    Some(ParametricCurveData { function_type, params })
}

fn parse_para_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    match parse_parametric_data(data) {
        Some(curve) => ICCTagData::ParametricCurve(curve),
        None => unknown_tag(data),
    }
}

fn parse_segmented_curve(data: &[u8]) -> Option<SegmentedCurveData> {
    if data.len() < 12 || &data[0..4] != b"curf" {
        return None;
    }
    let segment_count = read_u16_be(data, 8)? as usize;

    let mut break_points = Vec::new();
    let mut pos = 12;
    for _ in 0..segment_count.saturating_sub(1) {
        break_points.push(read_f32_be(data, pos)?);
        pos += 4;
    }

    let mut segments = Vec::new();
    for _ in 0..segment_count {
        if pos + 8 > data.len() {
            break;
        }
        match &data[pos..pos + 4] {
            b"parf" => {
                let function_type = read_u16_be(data, pos + 8)?;
                let count = formula_param_count(function_type)?;
                let mut params = Vec::with_capacity(count);
                for i in 0..count {
                    params.push(read_f32_be(data, pos + 12 + i * 4)?);
                }
                segments.push(CurveSegment::Formula(FormulaSegment { function_type, params }));
                pos += 12 + count * 4;
            }
            b"samf" => {
                let count = read_u32_be(data, pos + 8)? as usize;
                let mut values = Vec::new();
                for i in 0..count {
                    match read_f32_be(data, pos + 12 + i * 4) {
                        Some(v) => values.push(v),
                        None => break,
                    }
                }
                let consumed = values.len();
                segments.push(CurveSegment::Sampled(values));
                pos += 12 + consumed * 4;
            }
            other => {
                segments.push(CurveSegment::Unknown(sig_to_string(other)));
                break;
            }
        }
    }

    Some(SegmentedCurveData { break_points, segments })
}

fn parse_curf_tag(data: &[u8]) -> ICCTagData {
    match parse_segmented_curve(data) {
        Some(curve) => ICCTagData::SegmentedCurve(curve),
        None => unknown_tag(data),
    }
}

fn read_matrix_3x3(data: &[u8], offset: usize) -> Option<[f64; 9]> {
    let mut matrix = [0.0; 9];
    for (i, value) in matrix.iter_mut().enumerate() {
        *value = read_s15_fixed16(data, offset + i * 4)?;
    }
    Some(matrix)
}

fn clut_entry_count(grid_points: u8, input_channels: u8, output_channels: u8) -> usize {
    let count = (grid_points as u64)
        .checked_pow(input_channels as u32)
        .and_then(|v| v.checked_mul(output_channels as u64))
        .unwrap_or(0);
    count.min(usize::MAX as u64) as usize
}

fn parse_lut8_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 48 {
        return unknown_tag(data);
    }
    let input_channels = data[8];
    let output_channels = data[9];
    let clut_grid_points = data[10];
    let Some(matrix) = read_matrix_3x3(data, 12) else {
        return unknown_tag(data);
    };

    let mut pos = 48;
    let mut input_tables = Vec::with_capacity(input_channels as usize);
    for _ in 0..input_channels {
        let end = (pos + 256).min(data.len());
        input_tables.push(data[pos.min(data.len())..end].to_vec());
        pos += 256;
    }

    let clut_len = clut_entry_count(clut_grid_points, input_channels, output_channels);
    let clut_end = pos.saturating_add(clut_len).min(data.len());
    let clut = data[pos.min(data.len())..clut_end].to_vec();
    pos = pos.saturating_add(clut_len);

    let mut output_tables = Vec::with_capacity(output_channels as usize);
    for _ in 0..output_channels {
        let end = (pos + 256).min(data.len());
        output_tables.push(data[pos.min(data.len())..end].to_vec());
        pos += 256;
    }

    ICCTagData::Lut8(Lut8Data {
        input_channels,
        output_channels,
        clut_grid_points,
        matrix,
        input_tables,
        clut,
        output_tables,
    })
}

fn read_u16_table(data: &[u8], offset: usize, count: usize) -> Vec<u16> {
    let mut table = Vec::with_capacity(count.min((data.len().saturating_sub(offset)) / 2));
    for i in 0..count {
        match read_u16_be(data, offset + i * 2) {
            Some(v) => table.push(v),
            None => break,
        }
    }
    table
}

fn parse_lut16_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 52 {
        return unknown_tag(data);
    }
    let input_channels = data[8];
    let output_channels = data[9];
    let clut_grid_points = data[10];
    let Some(matrix) = read_matrix_3x3(data, 12) else {
        return unknown_tag(data);
    };
    let input_entries = read_u16_be(data, 48).unwrap_or(0) as usize;
    let output_entries = read_u16_be(data, 50).unwrap_or(0) as usize;

    let mut pos = 52;
    let mut input_tables = Vec::with_capacity(input_channels as usize);
    for _ in 0..input_channels {
        input_tables.push(read_u16_table(data, pos, input_entries));
        pos += input_entries * 2;
    }

    let clut_len = clut_entry_count(clut_grid_points, input_channels, output_channels);
    let clut = read_u16_table(data, pos, clut_len);
    pos += clut_len * 2;

    let mut output_tables = Vec::with_capacity(output_channels as usize);
    for _ in 0..output_channels {
        output_tables.push(read_u16_table(data, pos, output_entries));
        pos += output_entries * 2;
    }

    ICCTagData::Lut16(Lut16Data {
        input_channels,
        output_channels,
        clut_grid_points,
        matrix,
        input_tables,
        clut,
        output_tables,
    })
}

fn embedded_curve_size(data: &[u8], pos: usize) -> Option<usize> {
    match &data[pos..pos + 4] {
        b"curv" => {
            let count = read_u32_be(data, pos + 8)? as usize;
            Some(12 + count * 2)
        }
        b"para" => {
            let function_type = read_u16_be(data, pos + 8)?;
            Some(12 + parametric_param_count(function_type) * 4)
        }
        _ => None,
    }
}

fn parse_curve_sequence(data: &[u8], start: usize, count: usize) -> Vec<EmbeddedCurve> {
    let mut curves = Vec::new();
    let mut pos = start;
    for _ in 0..count {
        if pos + 12 > data.len() {
            break;
        }
        let curve = match &data[pos..pos + 4] {
            b"curv" => parse_curve_data(&data[pos..]).map(EmbeddedCurve::Curve),
            b"para" => parse_parametric_data(&data[pos..]).map(EmbeddedCurve::Parametric),
            other => {
                curves.push(EmbeddedCurve::Unknown(sig_to_string(other)));
                break;
            }
        };
        let Some(curve) = curve else { break };
        let Some(size) = embedded_curve_size(data, pos) else { break };
        curves.push(curve);
        pos += (size + 3) & !3;
    }
    curves
}

fn parse_lut_ab_clut(data: &[u8], offset: usize, input_channels: u8, output_channels: u8) -> Option<ClutData> {
    if offset + 20 > data.len() {
        return None;
    }
    let grid_points = data[offset..offset + 16].to_vec();
    let precision = data[offset + 16];

    let mut expected: u64 = output_channels as u64;
    for &points in grid_points.iter().take(input_channels as usize) {
        expected = expected.checked_mul(points.max(1) as u64)?;
    }

    let mut values = Vec::new();
    let value_start = offset + 20;
    for i in 0..expected.min(usize::MAX as u64) as usize {
        let value = match precision {
            1 => read_u8(data, value_start + i).map(|v| v as f64 / 255.0),
            2 => read_u16_be(data, value_start + i * 2).map(|v| v as f64 / 65535.0),
            _ => None,
        };
        match value {
            Some(v) => values.push(v),
            None => break,
        }
    }

    Some(ClutData { grid_points, precision, values })
}

fn parse_lut_ab_tag(data: &[u8], is_a_to_b: bool) -> ICCTagData {
    if data.len() < 32 {
        return unknown_tag(data);
    }
    let input_channels = data[8];
    let output_channels = data[9];
    let offset_b = read_u32_be(data, 12).unwrap_or(0) as usize;
    let offset_matrix = read_u32_be(data, 16).unwrap_or(0) as usize;
    let offset_m = read_u32_be(data, 20).unwrap_or(0) as usize;
    let offset_clut = read_u32_be(data, 24).unwrap_or(0) as usize;
    let offset_a = read_u32_be(data, 28).unwrap_or(0) as usize;

    let (a_count, m_count, b_count) = if is_a_to_b {
        (input_channels, output_channels, output_channels)
    } else {
        (output_channels, input_channels, input_channels)
    };

    let a_curves = if offset_a > 0 { parse_curve_sequence(data, offset_a, a_count as usize) } else { Vec::new() };
    let m_curves = if offset_m > 0 { parse_curve_sequence(data, offset_m, m_count as usize) } else { Vec::new() };
    let b_curves = if offset_b > 0 { parse_curve_sequence(data, offset_b, b_count as usize) } else { Vec::new() };

    let matrix = if offset_matrix > 0 {
        let mut values = Vec::with_capacity(12);
        for i in 0..12 {
            match read_s15_fixed16(data, offset_matrix + i * 4) {
                Some(v) => values.push(v),
                None => break,
            }
        }
        if values.len() == 12 {
            Some(values)
        } else {
            None
        }
    } else {
        None
    };

    let clut =
        if offset_clut > 0 { parse_lut_ab_clut(data, offset_clut, input_channels, output_channels) } else { None };

    let lut = LutABData { input_channels, output_channels, a_curves, m_curves, b_curves, matrix, clut };

    if is_a_to_b {
        ICCTagData::LutAToB(lut)
    } else {
        ICCTagData::LutBToA(lut)
    }
}

fn parse_ncl2_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 84 {
        return unknown_tag(data);
    }
    let vendor_flag = read_u32_be(data, 8).unwrap_or(0);
    let count = read_u32_be(data, 12).unwrap_or(0) as usize;
    let device_coord_count = read_u32_be(data, 16).unwrap_or(0);
    let prefix = String::from_utf8_lossy(&data[20..52]).trim_end_matches('\0').to_string();
    let suffix = String::from_utf8_lossy(&data[52..84]).trim_end_matches('\0').to_string();

    let entry_size = 38 + device_coord_count as usize * 2;
    let mut colors = Vec::new();
    let mut pos = 84;
    for _ in 0..count {
        if pos + entry_size > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[pos..pos + 32]).trim_end_matches('\0').to_string();
        let pcs = [
            read_u16_be(data, pos + 32).unwrap_or(0),
            read_u16_be(data, pos + 34).unwrap_or(0),
            read_u16_be(data, pos + 36).unwrap_or(0),
        ];
        let mut device = Vec::with_capacity(device_coord_count as usize);
        for i in 0..device_coord_count as usize {
            device.push(read_u16_be(data, pos + 38 + i * 2).unwrap_or(0));
        }
        colors.push(NamedColorEntry { name, pcs, device });
        pos += entry_size;
    }

    ICCTagData::NamedColors(NamedColorsData { vendor_flag, prefix, suffix, device_coord_count, colors })
}

fn parse_embedded_text(data: &[u8], pos: usize) -> Option<(String, usize)> {
    if pos + 12 > data.len() {
        return None;
    }
    match &data[pos..pos + 4] {
        b"desc" => {
            let ascii_len = read_u32_be(data, pos + 8)? as usize;
            let ascii_end = pos.checked_add(12)?.checked_add(ascii_len)?;
            if ascii_end > data.len() {
                return None;
            }
            let text = String::from_utf8_lossy(&data[pos + 12..ascii_end]).trim_end_matches('\0').to_string();
            let unicode_count = read_u32_be(data, ascii_end + 4).unwrap_or(0) as usize;
            let consumed = 12 + ascii_len + 8 + unicode_count * 2 + 3 + 67;
            Some((text, consumed))
        }
        b"mluc" => {
            let sub = &data[pos..];
            let count = read_u32_be(sub, 8)? as usize;
            let record_size = read_u32_be(sub, 12)? as usize;
            if record_size < 12 {
                return None;
            }
            let mut end = 16usize.checked_add(count.checked_mul(record_size)?)?;
            for i in 0..count {
                let record_offset = 16 + i * record_size;
                let Some(len) = read_u32_be(sub, record_offset + 4) else { continue };
                let Some(off) = read_u32_be(sub, record_offset + 8) else { continue };
                if let Some(record_end) = (off as usize).checked_add(len as usize) {
                    end = end.max(record_end);
                }
            }
            let text =
                parse_mluc_strings(sub).and_then(|strings| strings.first().map(|s| s.text.clone())).unwrap_or_default();
            Some((text, end))
        }
        _ => None,
    }
}

fn parse_device_attributes(bytes: &[u8]) -> DeviceAttributes {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[0..8]);
    let low = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    DeviceAttributes {
        raw,
        reflective: (low & 0x01) == 0,
        glossy: (low & 0x02) == 0,
        positive: (low & 0x04) == 0,
        color: (low & 0x08) == 0,
    }
}

fn parse_pseq_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let count = read_u32_be(data, 8).unwrap_or(0) as usize;

    let mut descriptions = Vec::new();
    let mut pos = 12;
    for _ in 0..count {
        if pos + 20 > data.len() {
            break;
        }
        let device_manufacturer = sig_at(data, pos);
        let device_model = sig_at(data, pos + 4);
        let attributes = parse_device_attributes(&data[pos + 8..pos + 16]);
        let technology = sig_at(data, pos + 16);
        pos += 20;

        let Some((manufacturer_description, consumed)) = parse_embedded_text(data, pos) else { break };
        pos += consumed;
        let Some((model_description, consumed)) = parse_embedded_text(data, pos) else { break };
        pos += consumed;

        descriptions.push(ProfileDescription {
            device_manufacturer,
            device_model,
            attributes,
            technology,
            manufacturer_description,
            model_description,
        });
    }

    ICCTagData::ProfileSequence(descriptions)
}

fn parse_psid_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let count = read_u32_be(data, 8).unwrap_or(0) as usize;

    let mut entries = Vec::new();
    for i in 0..count {
        let table_offset = 12 + i * 8;
        let Some(offset) = read_u32_be(data, table_offset) else { break };
        let Some(size) = read_u32_be(data, table_offset + 4) else { break };
        let start = offset as usize;
        let Some(end) = start.checked_add(size as usize) else { continue };
        if end > data.len() || start + 16 > data.len() {
            continue;
        }
        let profile_id = data[start..start + 16].to_vec();
        let description = if data.get(start + 16..start + 20) == Some(b"mluc") {
            parse_mluc_strings(&data[start + 16..end]).unwrap_or_default()
        } else {
            Vec::new()
        };
        entries.push(ProfileIdDescription { profile_id, description });
    }

    ICCTagData::ProfileSequenceId(entries)
}

fn parse_rcs2_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let channels = read_u16_be(data, 8).unwrap_or(0);
    let count = read_u16_be(data, 10).unwrap_or(0) as usize;

    let mut measurements = Vec::new();
    for i in 0..count {
        let Some(offset) = read_u32_be(data, 12 + i * 4) else { break };
        let base = offset as usize;
        if base + 4 + channels as usize * 4 > data.len() {
            continue;
        }
        let unit = sig_at(data, base);

        let mut counts = Vec::with_capacity(channels as usize);
        for c in 0..channels as usize {
            counts.push(read_u32_be(data, base + 4 + c * 4).unwrap_or(0) as usize);
        }

        let mut pos = base + 4 + channels as usize * 4;
        let mut max_colorants = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            match parse_xyz_number(data, pos) {
                Some(xyz) => max_colorants.push(xyz),
                None => break,
            }
            pos += 12;
        }

        let mut channel_responses = Vec::with_capacity(channels as usize);
        for &response_count in &counts {
            let mut responses = Vec::new();
            for _ in 0..response_count {
                let Some(device_code) = read_u16_be(data, pos) else { break };
                let Some(value) = read_s15_fixed16(data, pos + 4) else { break };
                responses.push(ResponseNumber { device_code, value });
                pos += 8;
            }
            channel_responses.push(responses);
        }

        measurements.push(ResponseCurveMeasurement { unit, max_colorants, channel_responses });
    }

    ICCTagData::ResponseCurveSet(ResponseCurveSetData { channels, measurements })
}

fn parse_dict_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let count = read_u32_be(data, 8).unwrap_or(0) as usize;
    let record_size = read_u32_be(data, 12).unwrap_or(0) as usize;
    if record_size != 16 && record_size != 24 && record_size != 32 {
        return unknown_tag(data);
    }

    let read_string = |offset: u32, size: u32| -> String {
        let start = offset as usize;
        let end = start.saturating_add(size as usize);
        if offset == 0 || end > data.len() || start >= end {
            return String::new();
        }
        decode_utf16_be(&data[start..end])
    };

    let read_localized = |offset: u32, size: u32| -> Vec<LocalizedString> {
        let start = offset as usize;
        let end = start.saturating_add(size as usize);
        if offset == 0 || end > data.len() || start + 16 > end {
            return Vec::new();
        }
        if &data[start..start + 4] != b"mluc" {
            return Vec::new();
        }
        parse_mluc_strings(&data[start..end]).unwrap_or_default()
    };

    let mut entries = Vec::new();
    for i in 0..count {
        let record = 16 + i * record_size;
        if record + record_size > data.len() {
            break;
        }
        let name_offset = read_u32_be(data, record).unwrap_or(0);
        let name_size = read_u32_be(data, record + 4).unwrap_or(0);
        let value_offset = read_u32_be(data, record + 8).unwrap_or(0);
        let value_size = read_u32_be(data, record + 12).unwrap_or(0);

        let mut display_name = Vec::new();
        let mut display_value = Vec::new();
        if record_size >= 24 {
            let offset = read_u32_be(data, record + 16).unwrap_or(0);
            let size = read_u32_be(data, record + 20).unwrap_or(0);
            display_name = read_localized(offset, size);
        }
        if record_size >= 32 {
            let offset = read_u32_be(data, record + 24).unwrap_or(0);
            let size = read_u32_be(data, record + 28).unwrap_or(0);
            display_value = read_localized(offset, size);
        }

        entries.push(DictEntry {
            name: read_string(name_offset, name_size),
            value: read_string(value_offset, value_size),
            display_name,
            display_value,
        });
    }

    ICCTagData::Dictionary(entries)
}

fn parse_mpe_curve_set(data: &[u8]) -> CurveSetElement {
    let input_channels = read_u16_be(data, 8).unwrap_or(0);
    let output_channels = read_u16_be(data, 10).unwrap_or(0);

    let mut curves = Vec::new();
    for i in 0..input_channels as usize {
        let Some(offset) = read_u32_be(data, 12 + i * 8) else { break };
        let Some(size) = read_u32_be(data, 12 + i * 8 + 4) else { break };
        let start = offset as usize;
        let Some(end) = start.checked_add(size as usize) else { continue };
        if end > data.len() || start >= end {
            continue;
        }
        if let Some(curve) = parse_segmented_curve(&data[start..end]) {
            curves.push(curve);
        }
    }

    CurveSetElement { input_channels, output_channels, curves }
}

fn parse_mpe_matrix(data: &[u8]) -> MatrixElement {
    let input_channels = read_u16_be(data, 8).unwrap_or(0);
    let output_channels = read_u16_be(data, 10).unwrap_or(0);

    let matrix_count = input_channels as usize * output_channels as usize;
    let mut matrix = Vec::with_capacity(matrix_count.min(data.len() / 4));
    for i in 0..matrix_count {
        match read_f32_be(data, 12 + i * 4) {
            Some(v) => matrix.push(v),
            None => break,
        }
    }

    let constants_start = 12 + matrix_count * 4;
    let mut constants = Vec::with_capacity(output_channels as usize);
    for i in 0..output_channels as usize {
        match read_f32_be(data, constants_start + i * 4) {
            Some(v) => constants.push(v),
            None => break,
        }
    }

    MatrixElement { input_channels, output_channels, matrix, constants }
}

fn parse_mpe_clut(data: &[u8]) -> Option<ClutElement> {
    let input_channels = read_u16_be(data, 8)?;
    let output_channels = read_u16_be(data, 10)?;
    if data.len() < 28 {
        return None;
    }
    let grid_points = data[12..28].to_vec();

    let mut expected: u64 = output_channels as u64;
    for &points in grid_points.iter().take(input_channels.min(16) as usize) {
        expected = expected.checked_mul(points.max(1) as u64)?;
    }

    let mut values = Vec::new();
    for i in 0..expected.min(usize::MAX as u64) as usize {
        match read_f32_be(data, 28 + i * 4) {
            Some(v) => values.push(v),
            None => break,
        }
    }

    Some(ClutElement { input_channels, output_channels, grid_points, values })
}

fn parse_mpe_element(data: &[u8]) -> Option<ProcessingElement> {
    if data.len() < 12 {
        return None;
    }
    let element = match &data[0..4] {
        b"cvst" => ProcessingElement::CurveSet(parse_mpe_curve_set(data)),
        b"matf" => ProcessingElement::Matrix(parse_mpe_matrix(data)),
        b"clut" => match parse_mpe_clut(data) {
            Some(clut) => ProcessingElement::Clut(clut),
            None => ProcessingElement::Unknown(UnknownElement {
                signature: sig_to_string(&data[0..4]),
                input_channels: read_u16_be(data, 8).unwrap_or(0),
                output_channels: read_u16_be(data, 10).unwrap_or(0),
                size: data.len() as u32,
            }),
        },
        other => ProcessingElement::Unknown(UnknownElement {
            signature: sig_to_string(other),
            input_channels: read_u16_be(data, 8).unwrap_or(0),
            output_channels: read_u16_be(data, 10).unwrap_or(0),
            size: data.len() as u32,
        }),
    };
    Some(element)
}

fn parse_mpet_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let input_channels = read_u16_be(data, 8).unwrap_or(0);
    let output_channels = read_u16_be(data, 10).unwrap_or(0);
    let count = read_u32_be(data, 12).unwrap_or(0) as usize;

    let mut elements = Vec::new();
    for i in 0..count {
        let table_offset = 16 + i * 8;
        let Some(offset) = read_u32_be(data, table_offset) else { break };
        let Some(size) = read_u32_be(data, table_offset + 4) else { break };
        let start = offset as usize;
        let Some(end) = start.checked_add(size as usize) else { continue };
        if end > data.len() || start >= end {
            continue;
        }
        if let Some(element) = parse_mpe_element(&data[start..end]) {
            elements.push(element);
        }
    }

    ICCTagData::MultiProcessElements(MultiProcessElementsData { input_channels, output_channels, elements })
}

fn parse_svcn_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 20 {
        return unknown_tag(data);
    }
    let standard_observer = decode_standard_observer(read_u32_be(data, 8).unwrap_or(0));
    let Some(observer_range) = read_spectral_range(data, 12) else {
        return unknown_tag(data);
    };

    let mut pos = 20;
    let observer_count = observer_range.steps as usize * 3;
    let mut observer_data = Vec::with_capacity(observer_count);
    for i in 0..observer_count {
        match read_f32_be(data, pos + i * 4) {
            Some(v) => observer_data.push(v),
            None => break,
        }
    }
    pos += observer_count * 4;

    let standard_illuminant = decode_illuminant(read_u32_be(data, pos).unwrap_or(0));
    let color_temperature = read_f32_be(data, pos + 4).unwrap_or(0.0);
    let Some(illuminant_range) = read_spectral_range(data, pos + 8) else {
        return unknown_tag(data);
    };
    pos += 16;

    let illuminant_count = illuminant_range.steps as usize;
    let mut illuminant_data = Vec::with_capacity(illuminant_count);
    for i in 0..illuminant_count {
        match read_f32_be(data, pos + i * 4) {
            Some(v) => illuminant_data.push(v),
            None => break,
        }
    }
    pos += illuminant_count * 4;

    let read_f32_xyz = |offset: usize| -> XYZNumber {
        XYZNumber {
            x: read_f32_be(data, offset).unwrap_or(0.0) as f64,
            y: read_f32_be(data, offset + 4).unwrap_or(0.0) as f64,
            z: read_f32_be(data, offset + 8).unwrap_or(0.0) as f64,
        }
    };

    let illuminant_xyz = read_f32_xyz(pos);
    let surround_xyz = read_f32_xyz(pos + 12);

    ICCTagData::SpectralViewingConditions(Box::new(SpectralViewingConditionsData {
        standard_observer,
        observer_range,
        observer_data,
        standard_illuminant,
        color_temperature,
        illuminant_range,
        illuminant_data,
        illuminant_xyz,
        surround_xyz,
    }))
}

fn parse_sdin_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 24 {
        return unknown_tag(data);
    }
    let color_signature = sig_at(data, 8);
    let Some(spectral_range) = read_spectral_range(data, 12) else {
        return unknown_tag(data);
    };
    let Some(bi_spectral_range) = read_spectral_range(data, 18) else {
        return unknown_tag(data);
    };
    ICCTagData::SpectralDataInfo(SpectralDataInfoData { color_signature, spectral_range, bi_spectral_range })
}

fn parse_tstr_tag(data: &[u8], depth: u8) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let struct_type = sig_at(data, 8);
    let count = read_u32_be(data, 12).unwrap_or(0) as usize;

    let mut members = Vec::new();
    for i in 0..count {
        let entry_offset = 16 + i * 12;
        let Some(offset) = read_u32_be(data, entry_offset + 4) else { break };
        let Some(size) = read_u32_be(data, entry_offset + 8) else { break };
        if entry_offset + 12 > data.len() {
            break;
        }
        let signature = sig_at(data, entry_offset);
        let start = offset as usize;
        let Some(end) = start.checked_add(size as usize) else { continue };
        if end > data.len() || start + 4 > end {
            continue;
        }
        let member_data = &data[start..end];
        let member_type = sig_to_string(&member_data[0..4]);
        members.push(TagStructMember { signature, data: parse_tag(&member_type, member_data, depth + 1) });
    }

    ICCTagData::TagStruct(TagStructData { struct_type, members })
}

fn parse_tary_tag(data: &[u8], depth: u8) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let array_type = sig_at(data, 8);
    let count = read_u32_be(data, 12).unwrap_or(0) as usize;

    let mut elements = Vec::new();
    for i in 0..count {
        let entry_offset = 16 + i * 8;
        let Some(offset) = read_u32_be(data, entry_offset) else { break };
        let Some(size) = read_u32_be(data, entry_offset + 4) else { break };
        let start = offset as usize;
        let Some(end) = start.checked_add(size as usize) else { continue };
        if end > data.len() || start + 4 > end {
            continue;
        }
        let element_data = &data[start..end];
        let element_type = sig_to_string(&element_data[0..4]);
        elements.push(parse_tag(&element_type, element_data, depth + 1));
    }

    ICCTagData::TagArray(TagArrayData { array_type, elements })
}

fn parse_meas_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 36 {
        return unknown_tag(data);
    }
    let observer = read_u32_be(data, 8).unwrap_or(0);
    let backing_xyz = parse_xyz_number(data, 12).unwrap_or(XYZNumber { x: 0.0, y: 0.0, z: 0.0 });
    let geometry = read_u32_be(data, 24).unwrap_or(0);
    let flare = read_u32_be(data, 28).unwrap_or(0) as f64 / 65536.0;
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
        return unknown_tag(data);
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
        return unknown_tag(data);
    }
    ICCTagData::Signature(sig_at(data, 8))
}

fn parse_data_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let flag = read_u32_be(data, 8).unwrap_or(0);
    ICCTagData::Data(DataTagData { flag, data: data[12..].to_vec() })
}

fn parse_chrm_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let Some(num_channels) = read_u16_be(data, 8) else {
        return unknown_tag(data);
    };
    let Some(colorant_type) = read_u16_be(data, 10) else {
        return unknown_tag(data);
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

    ICCTagData::Chromaticity(ChromaticityData { colorant_type: decode_colorant_type(colorant_type), channels })
}

fn parse_clro_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let Some(count) = read_u32_be(data, 8) else {
        return unknown_tag(data);
    };
    let order: Vec<u8> = data[12..].iter().take(count as usize).copied().collect();
    ICCTagData::ColorantOrder(order)
}

fn parse_clrt_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }
    let Some(count) = read_u32_be(data, 8) else {
        return unknown_tag(data);
    };
    let mut entries = Vec::new();
    let mut offset = 12;
    for _ in 0..count {
        if offset + 38 > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[offset..offset + 32]).trim_end_matches('\0').to_string();
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
        return unknown_tag(data);
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

fn parse_scrn_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let flag = read_u32_be(data, 8).unwrap_or(0);
    let count = read_u32_be(data, 12).unwrap_or(0) as usize;

    let mut channels = Vec::new();
    for i in 0..count {
        let offset = 16 + i * 12;
        if offset + 12 > data.len() {
            break;
        }
        channels.push(ScreeningChannel {
            frequency: read_s15_fixed16(data, offset).unwrap_or(0.0),
            angle: read_s15_fixed16(data, offset + 4).unwrap_or(0.0),
            spot_shape: decode_spot_shape(read_u32_be(data, offset + 8).unwrap_or(0)),
        });
    }

    ICCTagData::Screening(ScreeningData { flag, channels })
}

fn parse_bfd_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }

    let mut pos = 8;
    let read_curve = |pos: &mut usize| -> Vec<u16> {
        let Some(count) = read_u32_be(data, *pos) else { return Vec::new() };
        *pos += 4;
        let mut curve = Vec::new();
        for _ in 0..count {
            match read_u16_be(data, *pos) {
                Some(v) => {
                    curve.push(v);
                    *pos += 2;
                }
                None => break,
            }
        }
        curve
    };

    let ucr = read_curve(&mut pos);
    let bg = read_curve(&mut pos);
    let description = if pos < data.len() {
        String::from_utf8_lossy(&data[pos..]).trim_end_matches('\0').to_string()
    } else {
        String::new()
    };

    ICCTagData::UcrBg(UcrBgData { ucr, bg, description })
}

fn parse_crdi_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
    }

    let mut pos = 8;
    let mut strings = Vec::new();
    for _ in 0..5 {
        let Some(count) = read_u32_be(data, pos) else { break };
        pos += 4;
        let end = pos.saturating_add(count as usize).min(data.len());
        strings.push(String::from_utf8_lossy(&data[pos.min(data.len())..end]).trim_end_matches('\0').to_string());
        pos += count as usize;
    }

    let postscript_product_name = strings.first().cloned().unwrap_or_default();
    let crd_names = strings.into_iter().skip(1).collect();

    ICCTagData::CrdInfo(CrdInfoData { postscript_product_name, crd_names })
}

fn parse_sf32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
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
        return unknown_tag(data);
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
        return unknown_tag(data);
    }
    ICCTagData::UInt8Array(data[8..].to_vec())
}

fn parse_ui16_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
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
        return unknown_tag(data);
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
        return unknown_tag(data);
    }
    let payload = &data[8..];
    let count = payload.len() / 8;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 8;
        let hi = read_u32_be(payload, offset).unwrap_or(0);
        let lo = read_u32_be(payload, offset + 4).unwrap_or(0);
        values.push([hi, lo]);
    }
    ICCTagData::UInt64Array(values)
}

fn parse_fl16_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let payload = &data[8..];
    let count = payload.len() / 2;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(v) = read_f16_be(payload, i * 2) {
            values.push(v);
        }
    }
    ICCTagData::Float16Array(values)
}

fn parse_fl32_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let payload = &data[8..];
    let count = payload.len() / 4;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        if let Some(v) = read_f32_be(payload, i * 4) {
            values.push(v);
        }
    }
    ICCTagData::Float32Array(values)
}

fn parse_fl64_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 8 {
        return unknown_tag(data);
    }
    let payload = &data[8..];
    let count = payload.len() / 8;
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 8;
        let hi = read_u32_be(payload, offset).unwrap_or(0) as u64;
        let lo = read_u32_be(payload, offset + 4).unwrap_or(0) as u64;
        values.push(f64::from_bits((hi << 32) | lo));
    }
    ICCTagData::Float64Array(values)
}

fn parse_cicp_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 12 {
        return unknown_tag(data);
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
        return unknown_tag(data);
    }
    let Some(ascii_len) = read_u32_be(data, 8) else {
        return unknown_tag(data);
    };

    let ascii_end = 12usize.saturating_add(ascii_len as usize);
    if ascii_end > data.len() {
        return unknown_tag(data);
    }
    let ascii = String::from_utf8_lossy(&data[12..ascii_end]).trim_end_matches('\0').to_string();

    let mut unicode_language_code = 0u32;
    let mut unicode = String::new();
    let mut script_code = 0u16;
    let mut script = Vec::new();

    if ascii_end + 8 <= data.len() {
        unicode_language_code = read_u32_be(data, ascii_end).unwrap_or(0);
        let unicode_count = read_u32_be(data, ascii_end + 4).unwrap_or(0);
        let unicode_start = ascii_end + 8;
        let unicode_end = unicode_start.saturating_add(unicode_count as usize * 2);
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

    ICCTagData::TextDescription(TextDescriptionData { ascii, unicode_language_code, unicode, script_code, script })
}

fn parse_gbd_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 20 {
        return unknown_tag(data);
    }
    let pcs_channels = read_u16_be(data, 8).unwrap_or(0);
    let device_channels = read_u16_be(data, 10).unwrap_or(0);
    let vertex_count = read_u32_be(data, 12).unwrap_or(0) as usize;
    let triangle_count = read_u32_be(data, 16).unwrap_or(0) as usize;

    let mut pos = 20;
    let mut triangles = Vec::new();
    for _ in 0..triangle_count {
        if pos + 12 > data.len() {
            break;
        }
        triangles.push([
            read_u32_be(data, pos).unwrap_or(0),
            read_u32_be(data, pos + 4).unwrap_or(0),
            read_u32_be(data, pos + 8).unwrap_or(0),
        ]);
        pos += 12;
    }

    let mut read_f32_values = |count: usize| -> Vec<f32> {
        let mut values = Vec::new();
        for _ in 0..count {
            match read_f32_be(data, pos) {
                Some(v) => {
                    values.push(v);
                    pos += 4;
                }
                None => break,
            }
        }
        values
    };

    let pcs_values = read_f32_values(vertex_count.saturating_mul(pcs_channels as usize));
    let device_values = read_f32_values(vertex_count.saturating_mul(device_channels as usize));

    ICCTagData::GamutBoundary(GamutBoundaryData { pcs_channels, device_channels, triangles, pcs_values, device_values })
}

fn parse_smat_tag(data: &[u8]) -> ICCTagData {
    if data.len() < 16 {
        return unknown_tag(data);
    }
    let channels = read_u16_be(data, 8).unwrap_or(0);
    let matrix_type = read_u16_be(data, 10).unwrap_or(0);
    let count = read_u32_be(data, 12).unwrap_or(0);

    let mut matrices = Vec::new();
    let mut pos = 16;
    'outer: for _ in 0..count {
        let (Some(rows), Some(cols)) = (read_u16_be(data, pos), read_u16_be(data, pos + 2)) else {
            break;
        };
        pos += 4;

        let mut row_starts = Vec::with_capacity(rows as usize + 1);
        for i in 0..=rows as usize {
            match read_u16_be(data, pos + i * 2) {
                Some(v) => row_starts.push(v),
                None => break 'outer,
            }
        }
        pos += (rows as usize + 1) * 2;

        let entry_count = row_starts.last().copied().unwrap_or(0) as usize;

        let mut column_indices = Vec::with_capacity(entry_count);
        for i in 0..entry_count {
            match read_u16_be(data, pos + i * 2) {
                Some(v) => column_indices.push(v),
                None => break 'outer,
            }
        }
        pos += entry_count * 2;
        pos = (pos + 3) & !3;

        let mut values = Vec::with_capacity(entry_count);
        for i in 0..entry_count {
            match read_f32_be(data, pos + i * 4) {
                Some(v) => values.push(v),
                None => break 'outer,
            }
        }
        pos += entry_count * 4;

        matrices.push(SparseMatrix { rows, cols, row_starts, column_indices, values });
    }

    ICCTagData::SparseMatrixArray(SparseMatrixArrayData { channels, matrix_type, matrices })
}

fn parse_embedded_profile_tag(data: &[u8], depth: u8) -> ICCTagData {
    if data.len() < 8 + 128 {
        return unknown_tag(data);
    }
    match super::ICCProfile::parse_with_depth(&data[8..], depth + 1) {
        Ok(profile) => ICCTagData::EmbeddedProfile(Box::new(profile)),
        Err(_) => unknown_tag(data),
    }
}

fn parse_embedded_image_tag(data: &[u8], is_height_map: bool) -> ICCTagData {
    let header_size = if is_height_map { 24 } else { 16 };
    if data.len() < header_size {
        return unknown_tag(data);
    }
    let seamless = read_u32_be(data, 8).unwrap_or(0) != 0;
    let encoding = match read_u32_be(data, 12).unwrap_or(u32::MAX) {
        0 => "PNG".to_string(),
        1 => "TIFF".to_string(),
        v => format!("Unknown({})", v),
    };
    let height_range_meters = if is_height_map {
        Some([read_f32_be(data, 16).unwrap_or(0.0), read_f32_be(data, 20).unwrap_or(0.0)])
    } else {
        None
    };

    ICCTagData::EmbeddedImage(EmbeddedImageData {
        is_height_map,
        seamless,
        encoding,
        height_range_meters,
        data: data[header_size..].to_vec(),
    })
}

pub(super) fn parse_tag(type_sig: &str, data: &[u8], depth: u8) -> ICCTagData {
    if depth > MAX_RECURSION_DEPTH {
        return unknown_tag(data);
    }
    match type_sig.trim_end_matches('\0') {
        "text" => parse_text_tag(data),
        "utf8" => parse_utf8_tag(data),
        "ut16" => parse_utf16_tag(data),
        "zut8" => parse_zip_utf8_tag(data),
        "zxml" | "ZXML" => parse_zip_xml_tag(data),
        "mluc" => parse_mluc_tag(data),
        "XYZ " => parse_xyz_tag(data),
        "curv" => parse_curve_tag(data),
        "para" => parse_para_tag(data),
        "curf" => parse_curf_tag(data),
        "mft1" => parse_lut8_tag(data),
        "mft2" => parse_lut16_tag(data),
        "mAB " => parse_lut_ab_tag(data, true),
        "mBA " => parse_lut_ab_tag(data, false),
        "ncl2" => parse_ncl2_tag(data),
        "pseq" => parse_pseq_tag(data),
        "psid" => parse_psid_tag(data),
        "rcs2" => parse_rcs2_tag(data),
        "dict" => parse_dict_tag(data),
        "mpet" => parse_mpet_tag(data),
        "svcn" => parse_svcn_tag(data),
        "sdin" => parse_sdin_tag(data),
        "tstr" => parse_tstr_tag(data, depth),
        "tary" => parse_tary_tag(data, depth),
        "meas" => parse_meas_tag(data),
        "dtim" => parse_dtim_tag(data),
        "sig " => parse_sig_tag(data),
        "data" => parse_data_tag(data),
        "chrm" => parse_chrm_tag(data),
        "clro" => parse_clro_tag(data),
        "clrt" => parse_clrt_tag(data),
        "view" => parse_view_tag(data),
        "scrn" => parse_scrn_tag(data),
        "bfd" | "bfd " => parse_bfd_tag(data),
        "crdi" => parse_crdi_tag(data),
        "sf32" => parse_sf32_tag(data),
        "uf32" => parse_uf32_tag(data),
        "ui08" => parse_ui08_tag(data),
        "ui16" => parse_ui16_tag(data),
        "ui32" => parse_ui32_tag(data),
        "ui64" => parse_ui64_tag(data),
        "fl16" => parse_fl16_tag(data),
        "fl32" | "fct " => parse_fl32_tag(data),
        "fl64" => parse_fl64_tag(data),
        "cicp" => parse_cicp_tag(data),
        "desc" => parse_desc_tag(data),
        "gbd" | "gbd " => parse_gbd_tag(data),
        "smat" => parse_smat_tag(data),
        "ICCp" => parse_embedded_profile_tag(data, depth),
        "ehim" => parse_embedded_image_tag(data, true),
        "enim" => parse_embedded_image_tag(data, false),
        _ => unknown_tag(data),
    }
}
