use crate::utils::image::PixelData;
use crate::utils::types::ByteOrder;

pub fn jpeg_rgb_bytes(pixels: PixelData, wide: bool, byte_order: ByteOrder) -> Vec<u8> {
    if !wide {
        return pixels.into_rgb8_bytes();
    }

    let samples = match pixels {
        PixelData::RGB16(samples) => samples,
        PixelData::RGBA16(samples) => samples.chunks_exact(4).flat_map(|p| p[..3].to_vec()).collect(),
        PixelData::L16(samples) => samples.iter().flat_map(|&v| [v, v, v]).collect(),
        PixelData::LA16(samples) => samples.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        other => other.into_rgb8_bytes().into_iter().map(|v| u16::from(v) * 257).collect(),
    };

    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        match byte_order {
            ByteOrder::BigEndian => out.extend_from_slice(&sample.to_be_bytes()),
            ByteOrder::LittleEndian => out.extend_from_slice(&sample.to_le_bytes()),
        }
    }
    out
}

pub fn jpeg_pixel_data(data: Vec<u8>, wide: bool, byte_order: ByteOrder) -> PixelData {
    if !wide {
        return PixelData::RGB8(data);
    }

    let samples = data
        .chunks_exact(2)
        .map(|b| match byte_order {
            ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
            ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
        })
        .collect();

    PixelData::RGB16(samples)
}

pub fn splice_jpeg_tables(tables: &[u8], strip: &[u8]) -> Vec<u8> {
    if tables.len() < 4 || strip.len() < 2 {
        return strip.to_vec();
    }

    let tables_body = if tables.starts_with(&[0xFF, 0xD8]) && tables.ends_with(&[0xFF, 0xD9]) {
        &tables[2..tables.len() - 2]
    } else {
        tables
    };

    if strip.starts_with(&[0xFF, 0xD8]) {
        let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len() - 2);
        result.extend_from_slice(&[0xFF, 0xD8]);
        result.extend_from_slice(tables_body);
        result.extend_from_slice(&strip[2..]);
        result
    } else {
        let mut result = Vec::with_capacity(2 + tables_body.len() + strip.len());
        result.extend_from_slice(&[0xFF, 0xD8]);
        result.extend_from_slice(tables_body);
        result.extend_from_slice(strip);
        result
    }
}
