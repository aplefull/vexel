pub const DEFAULT_PALETTE: [[u8; 3]; 32] = [
    [0, 0, 0],
    [128, 128, 128],
    [192, 192, 192],
    [255, 255, 255],
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
    [255, 0, 255],
    [128, 0, 0],
    [0, 128, 0],
    [0, 0, 128],
    [128, 128, 0],
    [0, 128, 128],
    [128, 0, 128],
    [255, 165, 0],
    [204, 204, 0],
    [153, 0, 0],
    [0, 204, 0],
    [0, 153, 0],
    [204, 204, 0],
    [153, 153, 0],
    [102, 0, 0],
    [0, 0, 204],
    [0, 0, 153],
    [204, 0, 204],
    [153, 0, 153],
    [0, 204, 204],
    [0, 153, 153],
    [102, 102, 102],
    [153, 153, 153],
];

pub fn parse_palette_segment(data: &[u8]) -> Vec<[u8; 3]> {
    if data.len() < 3 {
        return Vec::new();
    }

    let components = data[0] as usize;
    let component_length = data[1] as usize;
    let count = data[2] as usize;

    if components == 0 || !matches!(component_length, 1 | 2 | 4) {
        return Vec::new();
    }

    let mut colors = Vec::with_capacity(count);
    let mut offset = 3usize;

    for _ in 0..count {
        let mut channels = [0u8; 3];
        for channel in channels.iter_mut().take(components.min(3)) {
            let Some(bytes) = data.get(offset..offset + component_length) else {
                return colors;
            };
            *channel = bytes[0];
            offset += component_length;
        }
        if components > 3 {
            offset += (components - 3) * component_length;
        }
        if components == 1 {
            channels[1] = channels[0];
            channels[2] = channels[0];
        }
        colors.push(channels);
    }

    colors
}

pub enum ColorValues {
    Rgb(Vec<[u8; 3]>),
    PaletteIds(Vec<u32>),
}

pub fn decode_t45(data: &[u8], expected: usize) -> Option<ColorValues> {
    if data.len() < 6 {
        return None;
    }

    let components = data[0] as usize;
    let component_length = data[1] as usize;
    let count = u32::from_be_bytes([data[2], data[3], data[4], data[5]]) as usize;

    if components == 0 || !matches!(component_length, 1 | 2 | 4) {
        return None;
    }

    let count = if count == 0 { expected } else { count };
    let mut rgb = vec![[0u8; 3]; count];
    let mut ids = vec![0u32; count];
    let mut offset = 6usize;
    let mut index = 0usize;

    while index < count {
        let first = *data.get(offset)?;
        offset += 1;

        let run_length = if first == 0 {
            let high = *data.get(offset)? as usize;
            let low = *data.get(offset + 1)? as usize;
            offset += 2;
            (high << 8) | low
        } else {
            first as usize
        };

        if run_length == 0 {
            break;
        }

        let mut channels = [0u8; 3];
        let mut id = 0u32;
        for component in 0..components {
            let bytes = data.get(offset..offset + component_length)?;
            offset += component_length;
            if component == 0 {
                id = bytes.iter().fold(0u32, |acc, &b| (acc << 8) | b as u32);
            }
            if let Some(channel) = channels.get_mut(component) {
                *channel = *bytes.last().unwrap_or(&0);
            }
        }
        if components == 1 {
            channels[1] = channels[0];
            channels[2] = channels[0];
        }

        for slot in rgb.iter_mut().skip(index).take(run_length) {
            *slot = channels;
        }
        for slot in ids.iter_mut().skip(index).take(run_length) {
            *slot = id;
        }
        index += run_length;
    }

    if components >= 3 {
        Some(ColorValues::Rgb(rgb))
    } else {
        Some(ColorValues::PaletteIds(ids))
    }
}
