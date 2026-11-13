use std::io::{Read, Seek, SeekFrom};

use super::decoder::JpegDecoder;
use crate::bitreader::BitReader;
use crate::decoders::jpeg::markers::{JpegMarker, JPEG_MARKERS};
use crate::utils::error::VexelResult;
use crate::{log_warn, Image, Limits, PixelData};

pub(crate) fn scan_jpeg_thumbnails<R: Read + Seek>(
    reader: &mut BitReader<R>,
    limits: &Limits,
) -> VexelResult<Vec<Image>> {
    let saved_pos = reader.stream_position().unwrap_or(0);
    let result = scan_thumbnail_segments(reader, limits);
    let _ = reader.seek(SeekFrom::Start(saved_pos));
    result
}

pub(super) fn scan_thumbnail_segments<R: Read + Seek>(
    reader: &mut BitReader<R>,
    limits: &Limits,
) -> VexelResult<Vec<Image>> {
    reader.seek(SeekFrom::Start(0))?;

    let mut thumbnails = Vec::new();
    let mut budget = limits.clone();

    while let Ok(Some(marker)) = reader.next_marker(&JPEG_MARKERS) {
        let decoded_before = thumbnails.len();
        match marker {
            JpegMarker::SOI
            | JpegMarker::TEM
            | JpegMarker::RST0
            | JpegMarker::RST1
            | JpegMarker::RST2
            | JpegMarker::RST3
            | JpegMarker::RST4
            | JpegMarker::RST5
            | JpegMarker::RST6
            | JpegMarker::RST7 => continue,
            JpegMarker::EOI | JpegMarker::SOS => break,
            JpegMarker::APP0 | JpegMarker::APP1 => {
                let length = match reader.read_u16() {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let payload_len = (length as usize).saturating_sub(2);
                let payload = match reader.read_bytes(payload_len) {
                    Ok(p) => p,
                    Err(_) => break,
                };

                if marker == JpegMarker::APP0 {
                    extract_app0_thumbnail(&payload, &budget, &mut thumbnails);
                } else if payload.starts_with(b"Exif\0\0") {
                    if let Some(bytes) = crate::utils::exif::exif_thumbnail(&payload[6..]) {
                        if let Some(image) = decode_embedded_jpeg(bytes, &budget) {
                            thumbnails.push(image);
                        }
                    }
                }

                let new_bytes: usize = thumbnails[decoded_before..].iter().map(|t| t.pixels().as_bytes().len()).sum();
                if budget.reserve_usize(new_bytes).is_err() {
                    log_warn!("Embedded thumbnails exceed the allocation limit, ignoring the rest");
                    thumbnails.truncate(decoded_before);
                    break;
                }
            }
            _ => {
                let length = match reader.read_u16() {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let skip = (length as i64) - 2;
                if skip > 0 && reader.seek(SeekFrom::Current(skip)).is_err() {
                    break;
                }
            }
        }
    }

    Ok(thumbnails)
}

pub(super) fn extract_app0_thumbnail(payload: &[u8], limits: &Limits, thumbnails: &mut Vec<Image>) {
    if payload.len() < 6 {
        return;
    }

    match &payload[..5] {
        b"JFIF\0" => {
            let width = payload.get(12).copied().unwrap_or(0) as u32;
            let height = payload.get(13).copied().unwrap_or(0) as u32;
            if width == 0 || height == 0 {
                return;
            }

            let size = (width * height * 3) as usize;
            let data = payload.get(14..).unwrap_or(&[]);
            let rgb = data.get(..size).unwrap_or(data).to_vec();

            let mut pixels = PixelData::RGB8(rgb);
            pixels.correct_pixels(width, height);
            thumbnails.push(Image::from_pixels(width, height, pixels));
        }
        b"JFXX\0" => match payload[5] {
            0x10 => {
                if let Some(image) = decode_embedded_jpeg(&payload[6..], limits) {
                    thumbnails.push(image);
                }
            }
            0x11 => {
                let width = payload.get(6).copied().unwrap_or(0) as u32;
                let height = payload.get(7).copied().unwrap_or(0) as u32;
                if width == 0 || height == 0 {
                    return;
                }

                let palette = payload.get(8..8 + 768).unwrap_or(&[]);
                let indices = payload.get(8 + 768..).unwrap_or(&[]);

                let mut rgb = Vec::with_capacity((width * height * 3) as usize);
                for &index in indices.iter().take((width * height) as usize) {
                    let base = index as usize * 3;
                    let entry = palette.get(base..base + 3).unwrap_or(&[0, 0, 0]);
                    rgb.extend_from_slice(entry);
                }

                let mut pixels = PixelData::RGB8(rgb);
                pixels.correct_pixels(width, height);
                thumbnails.push(Image::from_pixels(width, height, pixels));
            }
            0x13 => {
                let width = payload.get(6).copied().unwrap_or(0) as u32;
                let height = payload.get(7).copied().unwrap_or(0) as u32;
                if width == 0 || height == 0 {
                    return;
                }

                let size = (width * height * 3) as usize;
                let data = payload.get(8..).unwrap_or(&[]);
                let rgb = data.get(..size).unwrap_or(data).to_vec();

                let mut pixels = PixelData::RGB8(rgb);
                pixels.correct_pixels(width, height);
                thumbnails.push(Image::from_pixels(width, height, pixels));
            }
            code => {
                log_warn!("Unknown JFXX extension code: {:#04x}", code);
            }
        },
        _ => {}
    }
}

pub(super) fn decode_embedded_jpeg(bytes: &[u8], limits: &Limits) -> Option<Image> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        log_warn!("Embedded thumbnail does not start with a JPEG SOI marker, skipping");
        return None;
    }

    let mut decoder = JpegDecoder::new(std::io::Cursor::new(bytes.to_vec()));
    decoder.set_limits(limits.clone());

    match decoder.decode() {
        Ok(image) => Some(image),
        Err(e) => {
            log_warn!("Failed to decode embedded thumbnail: {}", e);
            None
        }
    }
}
