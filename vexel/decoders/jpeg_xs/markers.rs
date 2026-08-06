use super::bitreader::XsBitReader;
use super::types::*;
use crate::log_warn;

pub struct HeaderParse {
    pub config: Config,
    pub image: ImageParams,
    pub markers: Vec<JpegXsMarkerInfo>,
    pub saw_pih: bool,
    pub saw_cdt: bool,
}

fn marker_name(marker: u16) -> &'static str {
    match marker {
        MARKER_SOC => "SOC",
        MARKER_EOC => "EOC",
        MARKER_PIH => "PIH",
        MARKER_CDT => "CDT",
        MARKER_WGT => "WGT",
        MARKER_COM => "COM",
        MARKER_NLT => "NLT",
        MARKER_CWD => "CWD",
        MARKER_CTS => "CTS",
        MARKER_CRG => "CRG",
        MARKER_SLH => "SLH",
        MARKER_CAP => "CAP",
        _ => "UNKNOWN",
    }
}

pub fn parse_header(reader: &mut XsBitReader) -> HeaderParse {
    let mut out = HeaderParse {
        config: Config::default(),
        image: ImageParams::default(),
        markers: Vec::new(),
        saw_pih: false,
        saw_cdt: false,
    };

    if reader.peek(16) as u16 == MARKER_SOC {
        out.markers.push(JpegXsMarkerInfo { offset: 0, marker: "SOC".to_string(), length: 2 });
        reader.read(16);
    } else {
        log_warn!("JPEG XS: missing SOC marker, attempting to continue");
    }

    loop {
        if !reader.can_peek(16) {
            log_warn!("JPEG XS: unexpected end of stream while parsing headers");
            break;
        }

        let marker = reader.peek(16) as u16;

        if marker == MARKER_SLH || marker == MARKER_EOC {
            break;
        }

        if marker & 0xff00 != 0xff00 {
            log_warn!("JPEG XS: invalid marker 0x{:04x} in header, stopping header parse", marker);
            break;
        }

        let offset = reader.consumed_bits() / 8;
        reader.read(16);

        let length = if reader.can_peek(16) { reader.peek(16) as u32 } else { 0 };

        out.markers.push(JpegXsMarkerInfo { offset, marker: marker_name(marker).to_string(), length });

        let segment_start = reader.consumed_bits();

        let handled = match marker {
            MARKER_CAP => parse_cap(reader, &mut out.config),
            MARKER_PIH => {
                out.saw_pih = parse_pih(reader, &mut out.config, &mut out.image);
                out.saw_pih
            }
            MARKER_CDT => {
                out.saw_cdt = parse_cdt(reader, &mut out.image);
                out.saw_cdt
            }
            MARKER_WGT => parse_wgt(reader, &mut out.config),
            MARKER_NLT => parse_nlt(reader, &mut out.config),
            MARKER_CWD => parse_cwd(reader, &mut out.config),
            MARKER_CTS => parse_cts(reader, &mut out.config),
            MARKER_CRG => parse_crg(reader, &mut out.config, &out.image),
            _ => false,
        };

        if !handled && marker != MARKER_COM {
            log_warn!("JPEG XS: skipping unhandled or malformed marker {}", marker_name(marker));
        }

        if length < 2 {
            log_warn!("JPEG XS: marker {} has invalid length {}, stopping header parse", marker_name(marker), length);
            break;
        }

        let segment_end = segment_start + u64::from(length) * 8;

        if segment_end < reader.consumed_bits() {
            log_warn!("JPEG XS: marker {} overran its declared length", marker_name(marker));
        }

        reader.seek_bits(segment_end);

        if reader.exhausted() {
            log_warn!("JPEG XS: stream ended during header parse");
            break;
        }
    }

    out
}

fn parse_cap(reader: &mut XsBitReader, config: &mut Config) -> bool {
    let length = reader.read(16);

    if !(2..=4).contains(&length) {
        return false;
    }

    let payload = length - 2;

    config.cap_bits = match payload {
        1 => (reader.read(8) << 8) as u16,
        2 => reader.read(16) as u16,
        _ => 0,
    };

    true
}

fn parse_pih(reader: &mut XsBitReader, config: &mut Config, image: &mut ImageParams) -> bool {
    let length = reader.read(16);

    if length != 26 {
        log_warn!("JPEG XS: PIH length {} is not 26, parsing anyway", length);
    }

    config.bitstream_size = reader.read(32);
    config.profile = reader.read(16) as u16;

    let plev = reader.read(16);
    config.level = ((plev >> 8) & 0xff) as u8;
    config.sublevel = (plev & 0xff) as u8;

    image.width = reader.read(16) as u32;
    image.height = reader.read(16) as u32;

    config.cw = reader.read(16) as u16;
    config.slice_height = reader.read(16) as u32;

    image.ncomps = reader.read(8) as usize;

    config.n_g = reader.read(8) as u8;
    config.s_s = reader.read(8) as u8;
    config.bw = reader.read(8) as u8;
    config.fq = reader.read(4) as u8;
    config.b_r = reader.read(4) as u8;
    config.fslc = reader.read(1) as u8;
    config.ppoc = reader.read(3) as u8;
    config.color_transform = ColorTransform::from_bits(reader.read(4) as u8);
    config.nlx = reader.read(4) as u8;
    config.nly = reader.read(4) as u8;
    config.slice_height = config.slice_height.saturating_mul(1 << config.nly.min(16));
    config.lh = reader.read(1) as u8;
    config.rl = reader.read(1) as u8;
    config.qpih = reader.read(2) as u8;
    config.fs = reader.read(2) as u8;
    config.rm = reader.read(2) as u8;

    if image.ncomps > MAX_NCOMPS {
        log_warn!("JPEG XS: ncomps {} exceeds maximum {}, clamping", image.ncomps, MAX_NCOMPS);
        image.ncomps = MAX_NCOMPS;
    }

    true
}

fn parse_cdt(reader: &mut XsBitReader, image: &mut ImageParams) -> bool {
    let length = reader.read(16);
    let declared = length.saturating_sub(2) / 2;
    let count = image.ncomps.min(declared as usize).min(MAX_NCOMPS);

    if count == 0 {
        return false;
    }

    for c in 0..count {
        image.depth = reader.read(8) as u8;

        let sx = reader.read(4) as u8;
        let sy = reader.read(4) as u8;

        image.sx[c] = if sx == 1 || sx == 2 {
            sx
        } else {
            log_warn!("JPEG XS: invalid Sx {} for component {}, using 1", sx, c);
            1
        };

        image.sy[c] = if sy == 1 || sy == 2 {
            sy
        } else {
            log_warn!("JPEG XS: invalid Sy {} for component {}, using 1", sy, c);
            1
        };
    }

    if image.depth == 0 || image.depth > 31 {
        log_warn!("JPEG XS: invalid bit depth {}, using 8", image.depth);
        image.depth = 8;
    }

    true
}

fn parse_wgt(reader: &mut XsBitReader, config: &mut Config) -> bool {
    let length = reader.read(16);

    if length < 2 {
        return false;
    }

    let payload = length - 2;

    if !payload.is_multiple_of(2) {
        log_warn!("JPEG XS: WGT payload {} is not even", payload);
    }

    let count = (payload / 2) as usize;
    let count = count.min(MAX_NBANDS);

    config.lvl_gains.clear();
    config.lvl_priorities.clear();

    for _ in 0..count {
        config.lvl_gains.push(reader.read(8) as u8);
        config.lvl_priorities.push(reader.read(8) as u8);
    }

    true
}

fn parse_nlt(reader: &mut XsBitReader, config: &mut Config) -> bool {
    let length = reader.read(16);

    if length != 5 && length != 12 {
        log_warn!("JPEG XS: unexpected NLT length {}", length);
    }

    match reader.read(8) {
        1 => {
            config.tnlt = NltType::Quadratic;
            config.quadratic.sigma = reader.read(1) as u16;
            config.quadratic.alpha = reader.read(15) as u16;
            true
        }
        2 => {
            config.tnlt = NltType::Extended;
            config.extended.t1 = reader.read(32) as u32;
            config.extended.t2 = reader.read(32) as u32;
            config.extended.e = reader.read(8) as u8;
            true
        }
        other => {
            log_warn!("JPEG XS: unknown NLT type {}, treating as linear", other);
            config.tnlt = NltType::None;
            false
        }
    }
}

fn parse_cwd(reader: &mut XsBitReader, config: &mut Config) -> bool {
    let length = reader.read(16);

    if length != 3 {
        log_warn!("JPEG XS: unexpected CWD length {}", length);
    }

    config.sd = reader.read(8) as u8;
    true
}

fn parse_cts(reader: &mut XsBitReader, config: &mut Config) -> bool {
    let length = reader.read(16);

    if length != 4 {
        log_warn!("JPEG XS: unexpected CTS length {}", length);
    }

    reader.read(4);

    config.tetrix.cf = match reader.read(4) {
        0 => TetrixMode::Full,
        3 => TetrixMode::Inline,
        other => {
            log_warn!("JPEG XS: unknown TETRIX mode {}, using full", other);
            TetrixMode::Full
        }
    };

    config.tetrix.e1 = reader.read(4) as u8;
    config.tetrix.e2 = reader.read(4) as u8;

    true
}

const CRG_PATTERNS: [(CfaPattern, [u16; 4], [u16; 4]); 4] = [
    (CfaPattern::Rggb, [0, 32768, 0, 32768], [0, 0, 32768, 32768]),
    (CfaPattern::Bggr, [32768, 32768, 0, 0], [32768, 0, 32768, 0]),
    (CfaPattern::Grbg, [32768, 0, 32768, 0], [0, 0, 32768, 32768]),
    (CfaPattern::Gbrg, [0, 0, 32768, 32768], [32768, 0, 32768, 0]),
];

fn parse_crg(reader: &mut XsBitReader, config: &mut Config, image: &ImageParams) -> bool {
    let length = reader.read(16);
    let declared = length.saturating_sub(2) / 4;
    let count = declared.min(MAX_NCOMPS as u64) as usize;

    let mut xcrg = [0u16; MAX_NCOMPS];
    let mut ycrg = [0u16; MAX_NCOMPS];

    for c in 0..count {
        xcrg[c] = reader.read(16) as u16;
        ycrg[c] = reader.read(16) as u16;
    }

    if image.ncomps != 4 || count < 4 {
        return false;
    }

    for (pattern, xs, ys) in CRG_PATTERNS.iter() {
        if xcrg[..4] == *xs && ycrg[..4] == *ys {
            config.cfa_pattern = *pattern;
            return true;
        }
    }

    log_warn!("JPEG XS: unrecognized CRG values, defaulting to RGGB");
    false
}

pub fn parse_slice_header(reader: &mut XsBitReader) -> Option<u32> {
    if !reader.can_peek(16) {
        return None;
    }

    if reader.peek(16) as u16 != MARKER_SLH {
        return None;
    }

    reader.read(16);

    let length = reader.read(16);
    if length != 4 {
        log_warn!("JPEG XS: unexpected SLH length {}", length);
    }

    Some(reader.read(16) as u32)
}
