use std::io::{Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::bmp::headers::HeaderReader;
use crate::utils::error::VexelResult;
use crate::utils::image::ImageFormat;
use crate::utils::limits::Limits;
use crate::utils::read::read_vec_up_to;
use crate::utils::tiff::ifd::{read_file_header, Ifd, IfdCycleGuard};
use crate::utils::tiff::types::TiffTags;

// Walk the IFD tree and try to figure out if a TIFF file is a camera raw
pub(crate) fn tiff_is_camera_raw<R: Read + Seek>(reader: &mut R) -> bool {
    const MAX_IFDS: usize = 16;
    const MAX_SUB_IFDS: usize = 8;
    const PHOTOMETRIC_CFA: u32 = 32803;
    const PHOTOMETRIC_LINEAR_RAW: u32 = 34892;

    let mut reader = BitReader::new(reader);
    let Ok(header) = read_file_header(&mut reader) else {
        return false;
    };

    let mut limits = Limits { max_alloc: Some(1 << 20), ..Limits::no_limits() };
    let mut guard = IfdCycleGuard::new(MAX_IFDS);
    let mut queue = vec![header.first_ifd_offset];
    let mut make = String::new();
    let mut model = String::new();

    while let Some(offset) = queue.pop() {
        if offset < header.format.header_length() || !guard.enter(offset) {
            continue;
        }
        let Ok(ifd) = Ifd::read(&mut reader, offset, header.format, &mut limits) else {
            continue;
        };

        if ifd.has(TiffTags::DNGVersion) {
            return true;
        }
        if matches!(
            ifd.value::<u32, _>(TiffTags::PhotometricInterpretation, &mut reader),
            Some(PHOTOMETRIC_CFA | PHOTOMETRIC_LINEAR_RAW)
        ) {
            return true;
        }
        if let Some(text) = ifd.string(TiffTags::Make, &mut reader) {
            make = text.to_uppercase();
        }
        if let Some(text) = ifd.string(TiffTags::Model, &mut reader) {
            model = text.to_uppercase();
        }

        queue.push(ifd.next_ifd_offset);
        if let Some(sub_ifds) = ifd.values::<u64, _>(TiffTags::SubIFDs, &mut reader, &mut limits) {
            queue.extend(sub_ifds.into_iter().take(MAX_SUB_IFDS));
        }
    }

    make.starts_with("KODAK")
        && (model.starts_with("DCS") || model.starts_with("EOSDCS") || model.starts_with("NC2000"))
}

pub(crate) fn jpeg_marker_chain_is_jpeg_ls<R: Read + Seek>(reader: &mut R) -> bool {
    const MAX_SEGMENTS: usize = 256;

    let mut byte = [0u8; 1];
    let mut length = [0u8; 2];
    if reader.seek(SeekFrom::Start(2)).is_err() {
        return false;
    }

    for _ in 0..MAX_SEGMENTS {
        if reader.read_exact(&mut byte).is_err() || byte[0] != 0xFF {
            return false;
        }
        let mut marker = 0xFF;
        while marker == 0xFF {
            if reader.read_exact(&mut byte).is_err() {
                return false;
            }
            marker = byte[0];
        }

        match marker {
            0xF7 | 0xF8 => return true,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xD9 | 0xDA => return false,
            0x01 | 0xD0..=0xD7 => continue,
            _ => {
                if reader.read_exact(&mut length).is_err() {
                    return false;
                }
                let skip = i64::from(u16::from_be_bytes(length)) - 2;
                if skip < 0 || reader.seek(SeekFrom::Current(skip)).is_err() {
                    return false;
                }
            }
        }
    }

    false
}

/// Checks whether the first 4 bytes at the current header window look like a headerless DIB
pub(crate) fn looks_like_headerless_dib(header: &[u8]) -> bool {
    if header.len() < 16 {
        return false;
    }

    let dib_header_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if !matches!(dib_header_size, 40 | 52 | 56 | 64 | 108 | 124) {
        return false;
    }

    let width = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let height = i32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    let planes = u16::from_le_bytes([header[12], header[13]]);
    let bpp = u16::from_le_bytes([header[14], header[15]]);

    width != 0 && height != 0 && planes == 1 && matches!(bpp, 1 | 2 | 4 | 8 | 16 | 24 | 32 | 64)
}

pub(crate) fn looks_like_pgx(header: &[u8]) -> bool {
    if !header.starts_with(b"PG") {
        return false;
    }

    let mut pos = 2;

    while matches!(header.get(pos), Some(b' ') | Some(b'\t')) {
        pos += 1;
    }

    if pos == 2 {
        return false;
    }

    match (header.get(pos), header.get(pos + 1)) {
        (Some(b'M'), Some(b'L')) | (Some(b'L'), Some(b'M')) => pos += 2,
        _ => return false,
    }

    while matches!(header.get(pos), Some(b' ') | Some(b'\t') | Some(b'+') | Some(b'-')) {
        pos += 1;
    }

    let mut digits = 0;
    let mut precision: u32 = 0;

    while let Some(&b) = header.get(pos) {
        if !b.is_ascii_digit() {
            break;
        }

        precision = precision.saturating_mul(10).saturating_add(u32::from(b - b'0'));
        digits += 1;
        pos += 1;
    }

    digits > 0 && (1..=31).contains(&precision) && matches!(header.get(pos), Some(b' ') | Some(b'\t'))
}

pub(crate) fn try_guess_format_harder<R: Read + Seek>(reader: &mut R) -> VexelResult<ImageFormat> {
    const HEADER_SIZE: usize = 48;
    const FOOTER_SIZE: usize = 12;
    let mut header = [0u8; HEADER_SIZE];
    let mut read_pos = 0;

    while read_pos < HEADER_SIZE {
        match reader.read(&mut header[read_pos..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => read_pos += n,
        }
    }

    let mut footer = [0u8; FOOTER_SIZE];
    if reader.seek(SeekFrom::End(-(FOOTER_SIZE as i64))).is_ok() {
        let _ = reader.read_exact(&mut footer);
    }

    reader.seek(SeekFrom::Start(0))?;

    // PNG
    let header_str = String::from_utf8_lossy(&header).to_lowercase();
    let chunks = ["png", "ihdr", "idat", "iend"];

    if chunks.iter().any(|chunk| header_str.contains(chunk)) {
        return Ok(ImageFormat::Png);
    }

    // JPEG
    if footer.ends_with(&[0xFF, 0xD9]) {
        return Ok(ImageFormat::Jpeg);
    }

    // GIF
    for i in 0..4 {
        if header[i..].starts_with(b"GIF87a") || header[i..].starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }
        if header[i..].starts_with(b"IF87a") || header[i..].starts_with(b"IF89a") {
            return Ok(ImageFormat::Gif);
        }
    }

    // BMP
    const PREFIX_SCAN_LIMIT: usize = 512;
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;

    let mut wide_header = vec![0u8; file_len.min(PREFIX_SCAN_LIMIT as u64) as usize];
    let mut read_pos = 0;
    while read_pos < wide_header.len() {
        match reader.read(&mut wide_header[read_pos..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => read_pos += n,
        }
    }
    reader.seek(SeekFrom::Start(0))?;

    for offset in 1..wide_header.len().saturating_sub(30) {
        if &wide_header[offset..offset + 2] != b"BM" {
            continue;
        }

        if HeaderReader::embedded_bmp_is_plausible(&wide_header[offset..], file_len - offset as u64) {
            return Ok(ImageFormat::Bmp);
        }
    }

    // TODO other formats

    // We tried
    Ok(ImageFormat::Unknown)
}

pub fn looks_like_jpeg_xs(header: &[u8]) -> bool {
    if header.len() < 4 {
        return false;
    }

    if header[0] != 0xff || header[1] != 0x10 {
        return false;
    }

    let next = u16::from_be_bytes([header[2], header[3]]);

    matches!(next, 0xff50 | 0xff12 | 0xff15 | 0xff13 | 0xff14 | 0xff16 | 0xff17 | 0xff18 | 0xff19)
}

pub fn looks_like_jpeg_xr(header: &[u8]) -> bool {
    if header.len() >= 8 && &header[..8] == b"WMPHOTO\0" {
        return true;
    }

    header.len() >= 4 && header[..4] == [0x49, 0x49, 0xbc, 0x01]
}

pub(crate) fn jp2_family_is_jpeg_xr<R: Read + Seek>(reader: &mut R) -> bool {
    const SIGNATURE_BOX_LEN: u64 = 12;
    const MAX_FTYP_LEN: usize = 1024;

    if reader.seek(SeekFrom::Start(SIGNATURE_BOX_LEN)).is_err() {
        return false;
    }
    let Ok(ftyp) = read_vec_up_to(reader, MAX_FTYP_LEN) else {
        return false;
    };
    let _ = reader.seek(SeekFrom::Start(0));

    if ftyp.len() < 16 || &ftyp[4..8] != b"ftyp" {
        return false;
    }
    let box_len = (u32::from_be_bytes([ftyp[0], ftyp[1], ftyp[2], ftyp[3]]) as usize).clamp(16, ftyp.len());
    let brand = &ftyp[8..12];
    let compatible = ftyp[16..box_len].chunks_exact(4);

    brand == b"jxrc" || compatible.into_iter().any(|entry| entry == b"jxrc")
}

pub fn looks_like_radiance_hdr(header: &[u8]) -> bool {
    let Some(rest) = header.strip_prefix(b"#?") else {
        return false;
    };
    let name_len = rest.iter().take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-').count();
    name_len > 0 && matches!(rest.get(name_len), Some(b'\n' | b'\r' | b' ') | None)
}

pub fn is_netpbm_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}
