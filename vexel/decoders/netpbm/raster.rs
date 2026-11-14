use crate::PixelData;

const PARALLEL_MIN_SAMPLES: usize = 1 << 20;

#[cfg(feature = "rayon")]
const CHUNK_SAMPLES: usize = 1 << 16;

pub enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl Samples {
    pub fn zeroed(count: usize, wide: bool) -> Self {
        if wide {
            Samples::U16(vec![0u16; count])
        } else {
            Samples::U8(vec![0u8; count])
        }
    }

    pub fn into_pixel_data(self, channels: usize) -> PixelData {
        match (self, channels) {
            (Samples::U8(data), 1) => PixelData::L8(data),
            (Samples::U8(data), 2) => PixelData::LA8(data),
            (Samples::U8(data), 4) => PixelData::RGBA8(data),
            (Samples::U8(data), _) => PixelData::RGB8(data),
            (Samples::U16(data), 1) => PixelData::L16(data),
            (Samples::U16(data), 2) => PixelData::LA16(data),
            (Samples::U16(data), 4) => PixelData::RGBA16(data),
            (Samples::U16(data), _) => PixelData::RGB16(data),
        }
    }
}

pub fn scale_value(value: u32, max_value: u32, target: u32) -> u32 {
    let value = value.min(max_value);
    ((u64::from(value) * u64::from(target) + u64::from(max_value / 2)) / u64::from(max_value)) as u32
}

pub fn lut_u8(max_value: u32) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (value, entry) in lut.iter_mut().enumerate() {
        *entry = scale_value(value as u32, max_value, 255) as u8;
    }
    lut
}

pub fn lut_u16(max_value: u32) -> Box<[u16; 65536]> {
    let mut lut = Box::new([0u16; 65536]);
    for (value, entry) in lut.iter_mut().enumerate() {
        *entry = scale_value(value as u32, max_value, 65535) as u16;
    }
    lut
}

pub fn first_channels<T: Copy + Default>(data: &[T], depth: usize, keep: usize, pixels: usize) -> Vec<T> {
    let mut out = Vec::with_capacity(pixels * keep);
    for px in data.chunks_exact(depth).take(pixels) {
        out.extend_from_slice(&px[..keep]);
    }
    out.resize(pixels * keep, T::default());
    out
}

pub fn u16_bytes_mut(data: &mut [u16]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), std::mem::size_of_val(data)) }
}

pub fn f32_bytes_mut(data: &mut [f32]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), std::mem::size_of_val(data)) }
}

pub fn for_each_chunk<T: Send>(data: &mut [T], f: impl Fn(&mut [T]) + Sync + Send) {
    #[cfg(feature = "rayon")]
    if data.len() >= PARALLEL_MIN_SAMPLES {
        use rayon::prelude::*;
        data.par_chunks_mut(CHUNK_SAMPLES).for_each(f);
        return;
    }

    f(data);
}

pub fn finish_u8(data: &mut [u8], max_value: u32) {
    if max_value == 255 {
        return;
    }
    let lut = lut_u8(max_value);
    for_each_chunk(data, |chunk| chunk.iter_mut().for_each(|v| *v = lut[*v as usize]));
}

pub fn finish_u16_be(data: &mut [u16], max_value: u32) {
    if max_value == 65535 {
        for_each_chunk(data, |chunk| chunk.iter_mut().for_each(|v| *v = u16::from_be(*v)));
        return;
    }
    let lut = lut_u16(max_value);
    for_each_chunk(data, |chunk| chunk.iter_mut().for_each(|v| *v = lut[u16::from_be(*v) as usize]));
}

pub fn finish_f32(data: &mut [f32], little_endian: bool) {
    if little_endian == cfg!(target_endian = "little") {
        return;
    }
    for_each_chunk(data, |chunk| chunk.iter_mut().for_each(|v| *v = f32::from_bits(v.to_bits().swap_bytes())));
}

pub fn unpack_bits(packed: &[u8], dst: &mut [u8], width: usize) {
    if width == 0 {
        return;
    }
    let bytes_per_row = width.div_ceil(8);
    let expand = expand_table();

    let rows_per_chunk = (PARALLEL_MIN_SAMPLES / 16 / width).max(1);
    let unpack_rows = |first_row: usize, rows: &mut [u8]| {
        for (y, row) in rows.chunks_exact_mut(width).enumerate() {
            let start = (first_row + y) * bytes_per_row;
            let src = packed.get(start..).map_or(&[][..], |rest| &rest[..rest.len().min(bytes_per_row)]);
            unpack_row(src, row, &expand);
        }
    };

    #[cfg(feature = "rayon")]
    if dst.len() >= PARALLEL_MIN_SAMPLES {
        use rayon::prelude::*;
        dst.par_chunks_mut(rows_per_chunk * width)
            .enumerate()
            .for_each(|(i, rows)| unpack_rows(i * rows_per_chunk, rows));
        return;
    }

    let _ = rows_per_chunk;
    unpack_rows(0, dst);
}

fn expand_table() -> [[u8; 8]; 256] {
    let mut table = [[0u8; 8]; 256];
    for (byte, entry) in table.iter_mut().enumerate() {
        for (bit, out) in entry.iter_mut().enumerate() {
            *out = u8::from(byte & (0x80 >> bit) == 0);
        }
    }
    table
}

fn unpack_row(src: &[u8], row: &mut [u8], expand: &[[u8; 8]; 256]) {
    let mut groups = row.chunks_exact_mut(8);
    let mut bytes = src.iter();
    for group in groups.by_ref() {
        group.copy_from_slice(&expand[*bytes.next().unwrap_or(&0) as usize]);
    }
    let tail = groups.into_remainder();
    if !tail.is_empty() {
        let bits = &expand[*bytes.next().unwrap_or(&0) as usize];
        tail.copy_from_slice(&bits[..tail.len()]);
    }
}

pub fn cmyk_to_rgb_u8(src: &[u8], max_value: u32, alpha: bool) -> Vec<u8> {
    let channels = if alpha { 5 } else { 4 };
    let out_channels = if alpha { 4 } else { 3 };
    let pixels = src.len() / channels;
    let mv = max_value as f32;
    let alpha_lut = lut_u8(max_value);
    let mut out = vec![0u8; pixels * out_channels];
    for (px, dst) in src.chunks_exact(channels).zip(out.chunks_exact_mut(out_channels)) {
        let k = 1.0 - px[3] as f32 / mv;
        dst[0] = ((1.0 - px[0] as f32 / mv) * k * 255.0).round() as u8;
        dst[1] = ((1.0 - px[1] as f32 / mv) * k * 255.0).round() as u8;
        dst[2] = ((1.0 - px[2] as f32 / mv) * k * 255.0).round() as u8;
        if alpha {
            dst[3] = alpha_lut[px[4] as usize];
        }
    }
    out
}

pub fn cmyk_to_rgb_u16(src: &[u16], max_value: u32, alpha: bool) -> Vec<u16> {
    let channels = if alpha { 5 } else { 4 };
    let out_channels = if alpha { 4 } else { 3 };
    let pixels = src.len() / channels;
    let mv = max_value as f32;
    let mut out = vec![0u16; pixels * out_channels];
    for (px, dst) in src.chunks_exact(channels).zip(out.chunks_exact_mut(out_channels)) {
        let k = 1.0 - px[3] as f32 / mv;
        dst[0] = ((1.0 - px[0] as f32 / mv) * k * 65535.0).round() as u16;
        dst[1] = ((1.0 - px[1] as f32 / mv) * k * 65535.0).round() as u16;
        dst[2] = ((1.0 - px[2] as f32 / mv) * k * 65535.0).round() as u16;
        if alpha {
            dst[3] = scale_value(u32::from(px[4]), max_value, 65535) as u16;
        }
    }
    out
}
