use crate::log_warn;
use crate::utils::types::ByteOrder;

const UV_SQSIZ: f64 = 0.003_500;
const UV_NDIVS: u32 = 16289;
const UV_VSTART: f64 = 0.016_940;
const UV_NVS: usize = 163;
const U_NEU: f64 = 0.210_526_316;
const V_NEU: f64 = 0.473_684_211;
const UVSCALE: f64 = 410.0;

struct UvRow {
    ustart: f64,
    ncum: i16,
}

#[rustfmt::skip]
const UV_ROW: [UvRow; UV_NVS] = [
    UvRow { ustart: 0.247663, ncum: 0 },
    UvRow { ustart: 0.243779, ncum: 4 },
    UvRow { ustart: 0.241684, ncum: 10 },
    UvRow { ustart: 0.237874, ncum: 17 },
    UvRow { ustart: 0.235906, ncum: 26 },
    UvRow { ustart: 0.232153, ncum: 36 },
    UvRow { ustart: 0.228352, ncum: 48 },
    UvRow { ustart: 0.226259, ncum: 62 },
    UvRow { ustart: 0.222371, ncum: 77 },
    UvRow { ustart: 0.220410, ncum: 94 },
    UvRow { ustart: 0.214710, ncum: 112 },
    UvRow { ustart: 0.212714, ncum: 133 },
    UvRow { ustart: 0.210721, ncum: 155 },
    UvRow { ustart: 0.204976, ncum: 178 },
    UvRow { ustart: 0.202986, ncum: 204 },
    UvRow { ustart: 0.199245, ncum: 231 },
    UvRow { ustart: 0.195525, ncum: 260 },
    UvRow { ustart: 0.193560, ncum: 291 },
    UvRow { ustart: 0.189878, ncum: 323 },
    UvRow { ustart: 0.186216, ncum: 357 },
    UvRow { ustart: 0.186216, ncum: 393 },
    UvRow { ustart: 0.182592, ncum: 429 },
    UvRow { ustart: 0.179003, ncum: 467 },
    UvRow { ustart: 0.175466, ncum: 507 },
    UvRow { ustart: 0.172001, ncum: 549 },
    UvRow { ustart: 0.172001, ncum: 593 },
    UvRow { ustart: 0.168612, ncum: 637 },
    UvRow { ustart: 0.168612, ncum: 683 },
    UvRow { ustart: 0.163575, ncum: 729 },
    UvRow { ustart: 0.158642, ncum: 778 },
    UvRow { ustart: 0.158642, ncum: 830 },
    UvRow { ustart: 0.158642, ncum: 882 },
    UvRow { ustart: 0.153815, ncum: 934 },
    UvRow { ustart: 0.153815, ncum: 989 },
    UvRow { ustart: 0.149097, ncum: 1044 },
    UvRow { ustart: 0.149097, ncum: 1102 },
    UvRow { ustart: 0.142746, ncum: 1160 },
    UvRow { ustart: 0.142746, ncum: 1222 },
    UvRow { ustart: 0.142746, ncum: 1284 },
    UvRow { ustart: 0.138270, ncum: 1346 },
    UvRow { ustart: 0.138270, ncum: 1411 },
    UvRow { ustart: 0.138270, ncum: 1476 },
    UvRow { ustart: 0.132166, ncum: 1541 },
    UvRow { ustart: 0.132166, ncum: 1610 },
    UvRow { ustart: 0.126204, ncum: 1679 },
    UvRow { ustart: 0.126204, ncum: 1752 },
    UvRow { ustart: 0.126204, ncum: 1825 },
    UvRow { ustart: 0.120381, ncum: 1898 },
    UvRow { ustart: 0.120381, ncum: 1975 },
    UvRow { ustart: 0.120381, ncum: 2052 },
    UvRow { ustart: 0.120381, ncum: 2129 },
    UvRow { ustart: 0.112962, ncum: 2206 },
    UvRow { ustart: 0.112962, ncum: 2288 },
    UvRow { ustart: 0.112962, ncum: 2370 },
    UvRow { ustart: 0.107450, ncum: 2452 },
    UvRow { ustart: 0.107450, ncum: 2538 },
    UvRow { ustart: 0.107450, ncum: 2624 },
    UvRow { ustart: 0.107450, ncum: 2710 },
    UvRow { ustart: 0.100343, ncum: 2796 },
    UvRow { ustart: 0.100343, ncum: 2887 },
    UvRow { ustart: 0.100343, ncum: 2978 },
    UvRow { ustart: 0.095126, ncum: 3069 },
    UvRow { ustart: 0.095126, ncum: 3164 },
    UvRow { ustart: 0.095126, ncum: 3259 },
    UvRow { ustart: 0.095126, ncum: 3354 },
    UvRow { ustart: 0.088276, ncum: 3449 },
    UvRow { ustart: 0.088276, ncum: 3549 },
    UvRow { ustart: 0.088276, ncum: 3649 },
    UvRow { ustart: 0.088276, ncum: 3749 },
    UvRow { ustart: 0.081523, ncum: 3849 },
    UvRow { ustart: 0.081523, ncum: 3954 },
    UvRow { ustart: 0.081523, ncum: 4059 },
    UvRow { ustart: 0.081523, ncum: 4164 },
    UvRow { ustart: 0.074861, ncum: 4269 },
    UvRow { ustart: 0.074861, ncum: 4379 },
    UvRow { ustart: 0.074861, ncum: 4489 },
    UvRow { ustart: 0.074861, ncum: 4599 },
    UvRow { ustart: 0.068290, ncum: 4709 },
    UvRow { ustart: 0.068290, ncum: 4824 },
    UvRow { ustart: 0.068290, ncum: 4939 },
    UvRow { ustart: 0.068290, ncum: 5054 },
    UvRow { ustart: 0.063573, ncum: 5169 },
    UvRow { ustart: 0.063573, ncum: 5288 },
    UvRow { ustart: 0.063573, ncum: 5407 },
    UvRow { ustart: 0.063573, ncum: 5526 },
    UvRow { ustart: 0.057219, ncum: 5645 },
    UvRow { ustart: 0.057219, ncum: 5769 },
    UvRow { ustart: 0.057219, ncum: 5893 },
    UvRow { ustart: 0.057219, ncum: 6017 },
    UvRow { ustart: 0.050985, ncum: 6141 },
    UvRow { ustart: 0.050985, ncum: 6270 },
    UvRow { ustart: 0.050985, ncum: 6399 },
    UvRow { ustart: 0.050985, ncum: 6528 },
    UvRow { ustart: 0.050985, ncum: 6657 },
    UvRow { ustart: 0.044859, ncum: 6786 },
    UvRow { ustart: 0.044859, ncum: 6920 },
    UvRow { ustart: 0.044859, ncum: 7054 },
    UvRow { ustart: 0.044859, ncum: 7188 },
    UvRow { ustart: 0.040571, ncum: 7322 },
    UvRow { ustart: 0.040571, ncum: 7460 },
    UvRow { ustart: 0.040571, ncum: 7598 },
    UvRow { ustart: 0.040571, ncum: 7736 },
    UvRow { ustart: 0.036339, ncum: 7874 },
    UvRow { ustart: 0.036339, ncum: 8016 },
    UvRow { ustart: 0.036339, ncum: 8158 },
    UvRow { ustart: 0.036339, ncum: 8300 },
    UvRow { ustart: 0.032139, ncum: 8442 },
    UvRow { ustart: 0.032139, ncum: 8588 },
    UvRow { ustart: 0.032139, ncum: 8734 },
    UvRow { ustart: 0.032139, ncum: 8880 },
    UvRow { ustart: 0.027947, ncum: 9026 },
    UvRow { ustart: 0.027947, ncum: 9176 },
    UvRow { ustart: 0.027947, ncum: 9326 },
    UvRow { ustart: 0.023739, ncum: 9476 },
    UvRow { ustart: 0.023739, ncum: 9630 },
    UvRow { ustart: 0.023739, ncum: 9784 },
    UvRow { ustart: 0.023739, ncum: 9938 },
    UvRow { ustart: 0.019504, ncum: 10092 },
    UvRow { ustart: 0.019504, ncum: 10250 },
    UvRow { ustart: 0.019504, ncum: 10408 },
    UvRow { ustart: 0.016976, ncum: 10566 },
    UvRow { ustart: 0.016976, ncum: 10727 },
    UvRow { ustart: 0.016976, ncum: 10888 },
    UvRow { ustart: 0.016976, ncum: 11049 },
    UvRow { ustart: 0.012639, ncum: 11210 },
    UvRow { ustart: 0.012639, ncum: 11375 },
    UvRow { ustart: 0.012639, ncum: 11540 },
    UvRow { ustart: 0.009991, ncum: 11705 },
    UvRow { ustart: 0.009991, ncum: 11873 },
    UvRow { ustart: 0.009991, ncum: 12041 },
    UvRow { ustart: 0.009016, ncum: 12209 },
    UvRow { ustart: 0.009016, ncum: 12379 },
    UvRow { ustart: 0.009016, ncum: 12549 },
    UvRow { ustart: 0.006217, ncum: 12719 },
    UvRow { ustart: 0.006217, ncum: 12892 },
    UvRow { ustart: 0.005097, ncum: 13065 },
    UvRow { ustart: 0.005097, ncum: 13240 },
    UvRow { ustart: 0.005097, ncum: 13415 },
    UvRow { ustart: 0.003909, ncum: 13590 },
    UvRow { ustart: 0.003909, ncum: 13767 },
    UvRow { ustart: 0.002340, ncum: 13944 },
    UvRow { ustart: 0.002389, ncum: 14121 },
    UvRow { ustart: 0.001068, ncum: 14291 },
    UvRow { ustart: 0.001653, ncum: 14455 },
    UvRow { ustart: 0.000717, ncum: 14612 },
    UvRow { ustart: 0.001614, ncum: 14762 },
    UvRow { ustart: 0.000270, ncum: 14905 },
    UvRow { ustart: 0.000484, ncum: 15041 },
    UvRow { ustart: 0.001103, ncum: 15170 },
    UvRow { ustart: 0.001242, ncum: 15293 },
    UvRow { ustart: 0.001188, ncum: 15408 },
    UvRow { ustart: 0.001011, ncum: 15517 },
    UvRow { ustart: 0.000709, ncum: 15620 },
    UvRow { ustart: 0.000301, ncum: 15717 },
    UvRow { ustart: 0.002416, ncum: 15806 },
    UvRow { ustart: 0.003251, ncum: 15888 },
    UvRow { ustart: 0.003246, ncum: 15964 },
    UvRow { ustart: 0.004141, ncum: 16033 },
    UvRow { ustart: 0.005963, ncum: 16095 },
    UvRow { ustart: 0.008839, ncum: 16150 },
    UvRow { ustart: 0.010490, ncum: 16197 },
    UvRow { ustart: 0.016994, ncum: 16237 },
    UvRow { ustart: 0.023659, ncum: 16268 },
];

pub fn logl16_to_y(p16: i32) -> f64 {
    let le = p16 & 0x7fff;
    if le == 0 {
        return 0.0;
    }
    let y = (std::f64::consts::LN_2 / 256.0 * (le as f64 + 0.5) - std::f64::consts::LN_2 * 64.0).exp();
    if p16 & 0x8000 == 0 {
        y
    } else {
        -y
    }
}

pub fn logl10_to_y(p10: u32) -> f64 {
    if p10 == 0 {
        return 0.0;
    }
    (std::f64::consts::LN_2 / 64.0 * (p10 as f64 + 0.5) - std::f64::consts::LN_2 * 12.0).exp()
}

fn uv_decode(c: u32) -> Option<(f64, f64)> {
    if c >= UV_NDIVS {
        return None;
    }

    let c = c as i16;
    let mut lower = 0usize;
    let mut upper = UV_NVS;

    while upper - lower > 1 {
        let vi = (lower + upper) >> 1;
        let ui = c - UV_ROW[vi].ncum;
        match ui.cmp(&0) {
            std::cmp::Ordering::Greater => lower = vi,
            std::cmp::Ordering::Less => upper = vi,
            std::cmp::Ordering::Equal => {
                lower = vi;
                break;
            }
        }
    }

    let vi = lower;
    let ui = (c - UV_ROW[vi].ncum) as f64;
    let u = UV_ROW[vi].ustart + (ui + 0.5) * UV_SQSIZ;
    let v = UV_VSTART + (vi as f64 + 0.5) * UV_SQSIZ;

    Some((u, v))
}

pub fn luv_to_xyz(l: f64, u: f64, v: f64) -> [f32; 3] {
    let s = 1.0 / (6.0 * u - 16.0 * v + 12.0);
    let x = 9.0 * u * s;
    let y = 4.0 * v * s;

    [(x / y * l) as f32, l as f32, ((1.0 - x - y) / y * l) as f32]
}

pub fn logluv32_to_xyz(p: u32) -> [f32; 3] {
    let l = logl16_to_y((p >> 16) as i32);
    if l <= 0.0 {
        return [0.0; 3];
    }

    let u = ((p >> 8 & 0xff) as f64 + 0.5) / UVSCALE;
    let v = ((p & 0xff) as f64 + 0.5) / UVSCALE;

    luv_to_xyz(l, u, v)
}

pub fn logluv24_to_xyz(p: u32) -> [f32; 3] {
    let l = logl10_to_y(p >> 14 & 0x3ff);
    if l <= 0.0 {
        return [0.0; 3];
    }

    let (u, v) = uv_decode(p & 0x3fff).unwrap_or((U_NEU, V_NEU));

    luv_to_xyz(l, u, v)
}

pub fn xyz_to_rgb(xyz: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = [xyz[0] as f64, xyz[1] as f64, xyz[2] as f64];

    [
        (2.690 * x - 1.276 * y - 0.414 * z) as f32,
        (-1.022 * x + 1.978 * y + 0.044 * z) as f32,
        (0.061 * x - 0.224 * y + 1.163 * z) as f32,
    ]
}

fn decode_rle_plane(data: &[u8], offset: &mut usize, tp: &mut [u32], shift: u32) {
    let npixels = tp.len();
    let mut i = 0;

    while i < npixels && *offset < data.len() {
        let b = data[*offset];

        if b >= 128 {
            if data.len() - *offset < 2 {
                break;
            }

            let rc = b as usize + 2 - 128;
            let value = (data[*offset + 1] as u32) << shift;
            *offset += 2;

            let end = (i + rc).min(npixels);
            for slot in &mut tp[i..end] {
                *slot |= value;
            }
            i = end;
        } else {
            let rc = b as usize;
            *offset += 1;

            let available = rc.min(npixels - i).min(data.len() - *offset);
            for slot in &mut tp[i..i + available] {
                *slot |= (data[*offset] as u32) << shift;
                *offset += 1;
            }
            i += available;

            if available < rc {
                break;
            }
        }
    }

    if i < npixels {
        log_warn!("LogLuv row is short by {} pixels", npixels - i);
    }
}

fn decode_rle_rows(data: &[u8], width: usize, rows: usize, shifts: &[u32]) -> Vec<u32> {
    let mut raw = vec![0u32; width * rows];
    let mut offset = 0;

    for row in 0..rows {
        let tp = &mut raw[row * width..(row + 1) * width];
        for &shift in shifts {
            decode_rle_plane(data, &mut offset, tp, shift);
        }
    }

    raw
}

pub fn decode_sgilog32(data: &[u8], width: usize, rows: usize) -> Vec<u32> {
    decode_rle_rows(data, width, rows, &[24, 16, 8, 0])
}

pub fn decode_sgilog16(data: &[u8], width: usize, rows: usize) -> Vec<u32> {
    decode_rle_rows(data, width, rows, &[8, 0])
}

pub fn decode_sgilog24(data: &[u8], width: usize, rows: usize) -> Vec<u32> {
    let npixels = width * rows;
    let mut raw = vec![0u32; npixels];

    for (i, slot) in raw.iter_mut().enumerate() {
        let Some(bytes) = data.get(i * 3..i * 3 + 3) else {
            log_warn!("LogLuv24 data is short by {} pixels", npixels - i);
            break;
        };

        *slot = (bytes[0] as u32) << 16 | (bytes[1] as u32) << 8 | bytes[2] as u32;
    }

    raw
}

pub fn words_to_bytes(raw: &[u32], order: ByteOrder) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() * 4);

    for &v in raw {
        match order {
            ByteOrder::BigEndian => out.extend_from_slice(&v.to_be_bytes()),
            ByteOrder::LittleEndian => out.extend_from_slice(&v.to_le_bytes()),
        }
    }

    out
}
