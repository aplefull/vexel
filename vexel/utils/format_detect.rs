use std::io::{Read, Seek, SeekFrom};

use crate::decoders::bmp::headers::HeaderReader;
use crate::utils::error::VexelResult;
use crate::utils::image::ImageFormat;

// Walk the IFD tree and try to figure out if a TIFF file is a camera raw
pub(crate) fn tiff_is_camera_raw<R: Read + Seek>(reader: &mut R, first_ifd_offset: u32, is_le: bool) -> bool {
    const MAX_IFDS: usize = 16;
    const MAX_ENTRIES: usize = 4096;

    let read_u16 = |b: &[u8]| if is_le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let read_u32 = |b: &[u8]| {
        if is_le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    };

    let mut queue: Vec<u32> = vec![first_ifd_offset];
    let mut visited: Vec<u32> = Vec::new();
    let mut make = String::new();
    let mut model = String::new();

    while let Some(offset) = queue.pop() {
        if offset < 8 || visited.contains(&offset) || visited.len() >= MAX_IFDS {
            continue;
        }
        visited.push(offset);

        if reader.seek(SeekFrom::Start(offset as u64)).is_err() {
            continue;
        }
        let mut count_buf = [0u8; 2];
        if reader.read_exact(&mut count_buf).is_err() {
            continue;
        }
        let num_entries = (read_u16(&count_buf) as usize).min(MAX_ENTRIES);

        let mut entries = vec![0u8; num_entries * 12];
        if reader.read_exact(&mut entries).is_err() {
            continue;
        }
        let mut next_buf = [0u8; 4];
        if reader.read_exact(&mut next_buf).is_ok() {
            queue.push(read_u32(&next_buf));
        }

        for entry in entries.chunks_exact(12) {
            let tag = read_u16(&entry[0..2]);
            let field_type = read_u16(&entry[2..4]);
            let count = read_u32(&entry[4..8]);
            let value = &entry[8..12];

            match tag {
                // DNGVersion
                50706 => return true,
                // PhotometricInterpretation: CFA or LinearRaw
                262 => {
                    let v = match field_type {
                        3 => read_u16(&value[..2]) as u32,
                        4 => read_u32(value),
                        _ => continue,
                    };
                    if v == 32803 || v == 34892 {
                        return true;
                    }
                }
                // Make / Model
                271 | 272 => {
                    let len = (count as usize).min(32);
                    let bytes = if len <= 4 {
                        value[..len].to_vec()
                    } else {
                        let mut buf = vec![0u8; len];
                        if reader.seek(SeekFrom::Start(read_u32(value) as u64)).is_err()
                            || reader.read_exact(&mut buf).is_err()
                        {
                            continue;
                        }
                        buf
                    };
                    let text = String::from_utf8_lossy(&bytes).trim_end_matches('\0').trim().to_uppercase();
                    if tag == 271 {
                        make = text
                    } else {
                        model = text
                    }
                }
                // SubIFDs
                330 => {
                    if count == 1 {
                        queue.push(read_u32(value));
                    } else {
                        let sub_count = (count as usize).min(8);
                        let mut buf = vec![0u8; sub_count * 4];
                        if reader.seek(SeekFrom::Start(read_u32(value) as u64)).is_ok()
                            && reader.read_exact(&mut buf).is_ok()
                        {
                            for chunk in buf.chunks_exact(4) {
                                queue.push(read_u32(chunk));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    make.starts_with("KODAK")
        && (model.starts_with("DCS") || model.starts_with("EOSDCS") || model.starts_with("NC2000"))
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

    let mut wide_header = vec![0u8; PREFIX_SCAN_LIMIT.min(file_len as usize)];
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
