use std::io::{Read, Seek};

use crate::decoders::jpeg::types::{APP14AdobeData, IccProfileSequenceInfo, JFIFData};
use crate::utils::error::VexelResult;
use crate::utils::exif::ExifReader;
use crate::utils::info::JpegLsInfo;
use crate::{Image, PixelData, log_warn};
use crate::bitreader::BitReader;

use super::bitreader::JlsBitReader;
use crate::utils::marker::Marker;
use super::markers::{JPEG_LS_MARKERS, JpegLsMarker};
use super::types::*;

pub struct JpegLsDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    frame: Option<FrameHeader>,
    pending_scan: Option<ScanHeader>,
    scans: Vec<(ScanHeader, Vec<u8>)>,
    color_transform: u8,
    restart_interval: usize,
    sections: Vec<JpegLsSectionInfo>,
}

impl<R: Read + Seek> JpegLsDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BitReader::new(reader),
            frame: None,
            pending_scan: None,
            scans: Vec::new(),
            color_transform: 0,
            restart_interval: 0,
            sections: Vec::new(),
        }
    }

    pub fn get_info(&self) -> JpegLsInfo {
        JpegLsInfo {
            sections: self.sections.clone(),
        }
    }

    fn read_u8(&mut self) -> std::io::Result<u8> {
        Ok(self.reader.read_bits(8)? as u8)
    }

    fn read_u16(&mut self) -> std::io::Result<u16> {
        Ok(self.reader.read_bits(16)? as u16)
    }

    fn read_sof(&mut self) -> std::io::Result<JpegLsSofData> {
        let marker_len = self.read_u16()?;
        let precision = self.read_u8()?;
        let height = self.read_u16()? as u32;
        let width = self.read_u16()? as u32;
        let comp_count = self.read_u8()?;

        let alpha = 1i32 << precision;

        let mut components = Vec::new();
        let mut info_components = Vec::new();
        for _ in 0..comp_count as usize {
            let id = self.read_u8()?;
            let sampling = self.read_u8()?;
            let tq = self.read_u8()?;
            components.push(ComponentInfo {
                id,
                horizontal_sampling: (sampling >> 4) & 0xF,
                vertical_sampling: sampling & 0xF,
            });
            info_components.push(JpegLsComponentInfo {
                id,
                horizontal_sampling: (sampling >> 4) & 0xF,
                vertical_sampling: sampling & 0xF,
                reserved: tq,
            });
        }

        self.frame = Some(FrameHeader {
            precision,
            height,
            width,
            components,
            alpha,
        });

        Ok(JpegLsSofData {
            length: marker_len,
            precision,
            height,
            width,
            component_count: comp_count,
            components: info_components,
        })
    }

    fn read_lse(&mut self) -> std::io::Result<JpegLsLseData> {
        let marker_len = self.read_u16()?;
        let id_type = self.read_u8()?;

        match id_type {
            1 => {
                let maxval = self.read_u16()?;
                let t1 = self.read_u16()?;
                let t2 = self.read_u16()?;
                let t3 = self.read_u16()?;
                let reset = self.read_u16()?;

                let scan = self.pending_scan.get_or_insert_with(ScanHeader::default);
                scan.alpha = maxval as i32 + 1;
                scan.t1 = t1 as i32;
                scan.t2 = t2 as i32;
                scan.t3 = t3 as i32;
                scan.reset = reset as i32;

                Ok(JpegLsLseData::PresetParameters { length: marker_len, maxval, t1, t2, t3, reset })
            }
            2 => {
                let table_id = self.read_u8()?;
                let entry_count = self.read_u16()?;
                let mut entries = Vec::with_capacity(entry_count as usize + 1);
                for _ in 0..=entry_count {
                    entries.push(self.read_u16()?);
                }
                Ok(JpegLsLseData::MappingTable { length: marker_len, table_id, entry_count, entries })
            }
            3 => {
                let remaining = (marker_len as usize).saturating_sub(3);
                let mut entries = Vec::with_capacity(remaining);
                for _ in 0..remaining {
                    entries.push(self.read_u8()?);
                }
                Ok(JpegLsLseData::ExtendedTemplate { length: marker_len, entries })
            }
            _ => {
                for _ in 3..marker_len as usize {
                    self.read_u8()?;
                }
                Ok(JpegLsLseData::Other { length: marker_len, id_type })
            }
        }
    }

    fn read_sos(&mut self) -> std::io::Result<JpegLsSosData> {
        let marker_len = self.read_u16()?;
        let comp_count = self.read_u8()?;

        let mut component_ids = Vec::new();
        let mut info_components = Vec::new();
        for _ in 0..comp_count as usize {
            let cid = self.read_u8()?;
            let tm = self.read_u8()?;
            component_ids.push(cid);
            info_components.push(JpegLsSosComponentInfo { id: cid, mapping_table_selector: tm });
        }

        let near = self.read_u8()?;
        let ilv_byte = self.read_u8()?;
        let pt = self.read_u8()?;

        let interleave_mode = if comp_count == 1 {
            InterleaveMode::None
        } else {
            match ilv_byte {
                0 => InterleaveMode::None,
                1 => InterleaveMode::Line,
                2 => InterleaveMode::Sample,
                _ => InterleaveMode::Line,
            }
        };

        let frame = self.frame.as_ref();
        let frame_precision = frame.map(|f| f.precision).unwrap_or(8);
        let frame_alpha = frame.map(|f| f.alpha).unwrap_or(256);

        let scan = self.pending_scan.get_or_insert_with(ScanHeader::default);
        scan.component_ids = component_ids;
        scan.near = near as i32;
        scan.interleave_mode = interleave_mode;
        scan.point_transform = pt;
        if scan.alpha <= 1 {
            if pt > 0 {
                scan.alpha = 1 << (frame_precision - pt);
            } else {
                scan.alpha = frame_alpha;
            }
        }

        Ok(JpegLsSosData {
            length: marker_len,
            component_count: comp_count,
            components: info_components,
            near,
            interleave_mode: ilv_byte,
            point_transform: pt,
            scan_data_length: 0,
        })
    }

    fn try_read_hp_color_transform(&mut self) -> std::io::Result<JpegLsAppData> {
        let length = self.read_u16()?;
        let payload = self.reader.read_bytes(length.saturating_sub(2) as usize)?;

        let (identifier, color_transform) = if payload.len() >= 5 {
            let id = String::from_utf8(payload[..4].to_vec()).ok();
            let is_mrfx = payload.starts_with(b"mrfx");
            if is_mrfx {
                self.color_transform = payload[4];
            }
            (id, if is_mrfx { Some(payload[4]) } else { None })
        } else {
            (None, None)
        };

        Ok(JpegLsAppData { marker: 0xFFE8, length, identifier, jfif: None, exif: None, icc_profile_sequence: None, adobe: None, color_transform })
    }

    fn read_compressed_data(&mut self) -> std::io::Result<Vec<u8>> {
        use std::io::SeekFrom;
        let mut data = Vec::new();
        loop {
            let byte = match self.reader.read_bits(8) {
                Ok(b) => b as u8,
                Err(_) => break,
            };

            if byte == 0xFF {
                let next = match self.reader.read_bits(8) {
                    Ok(b) => b as u8,
                    Err(_) => break,
                };

                if next & 0x80 == 0 {
                    data.push(0xFF);
                    data.push(next);
                } else if (0xD0..=0xD7).contains(&next) {
                    data.push(0xFF);
                    data.push(next);
                } else {
                    let _ = self.reader.seek(SeekFrom::Current(-2));
                    break;
                }
            } else {
                data.push(byte);
            }
        }
        Ok(data)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        loop {
            let marker = match self.reader.next_marker(&JPEG_LS_MARKERS) {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(_) => break,
            };
            let marker_start = self.reader.stream_position().unwrap_or(2).saturating_sub(2);

            match marker {
                JpegLsMarker::SOI => {
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Soi,
                    });
                }
                JpegLsMarker::SOF55 => {
                    let sof = self.read_sof()?;
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Sof(sof),
                    });
                }
                JpegLsMarker::LSE => {
                    let lse = self.read_lse()?;
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Lse(lse),
                    });
                }
                JpegLsMarker::SOS => {
                    let mut sos = self.read_sos()?;
                    let data = self.read_compressed_data()?;
                    sos.scan_data_length = data.len();
                    let mut scan = self.pending_scan.take().unwrap_or_default();
                    scan.restart_interval = self.restart_interval;
                    self.scans.push((scan, data));
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Sos(sos),
                    });
                }
                JpegLsMarker::EOI => {
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Eoi,
                    });
                    break;
                }
                JpegLsMarker::DRI => {
                    let len = self.read_u16()?;
                    let interval = self.read_u16()?;
                    self.restart_interval = interval as usize;
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Dri(JpegLsDriData {
                            length: len,
                            restart_interval: interval,
                        }),
                    });
                }
                JpegLsMarker::APP8 => {
                    let app = self.try_read_hp_color_transform()?;
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::App(app),
                    });
                }
                JpegLsMarker::APP0 => {
                    let len = self.read_u16()?;
                    let payload = self.reader.read_bytes(len.saturating_sub(2) as usize)?;
                    let mut app = JpegLsAppData { marker: 0xFFE0, length: len, identifier: None, jfif: None, exif: None, icc_profile_sequence: None, adobe: None, color_transform: None };
                    if payload.len() >= 5 {
                        let id = String::from_utf8_lossy(&payload[..5]).to_string();
                        app.identifier = Some(id.trim_end_matches('\0').to_string());
                        if payload.starts_with(b"JFIF\0") && payload.len() >= 9 {
                            let get = |i: usize| payload.get(i).copied().unwrap_or(0);
                            let get_u16 = |i: usize| u16::from_be_bytes([get(i), get(i + 1)]);
                            app.jfif = Some(JFIFData {
                                length: len,
                                identifier: "JFIF".to_string(),
                                version_major: get(5),
                                version_minor: get(6),
                                density_units: get(7),
                                x_density: get_u16(8),
                                y_density: get_u16(10),
                                thumbnail_width: get(12),
                                thumbnail_height: get(13),
                            });
                        }
                    }
                    self.sections.push(JpegLsSectionInfo { start_offset: marker_start, data: JpegLsSectionData::App(app) });
                }
                JpegLsMarker::APP1 => {
                    let len = self.read_u16()?;
                    let payload = self.reader.read_bytes(len.saturating_sub(2) as usize)?;
                    let exif = if payload.starts_with(b"Exif\0\0") {
                        ExifReader::parse(&payload[6..])
                    } else {
                        None
                    };
                    let identifier = if payload.len() >= 4 {
                        let null_pos = payload.iter().position(|&b| b == 0).unwrap_or(payload.len().min(32));
                        String::from_utf8_lossy(&payload[..null_pos]).into_owned().into()
                    } else {
                        None
                    };
                    self.sections.push(JpegLsSectionInfo { start_offset: marker_start, data: JpegLsSectionData::App(JpegLsAppData { marker: 0xFFE1, length: len, identifier, jfif: None, exif, icc_profile_sequence: None, adobe: None, color_transform: None }) });
                }
                JpegLsMarker::APP2 => {
                    let len = self.read_u16()?;
                    let payload = self.reader.read_bytes(len.saturating_sub(2) as usize)?;
                    let null_pos = payload.iter().position(|&b| b == 0).unwrap_or(payload.len());
                    let identifier = String::from_utf8_lossy(&payload[..null_pos]).to_string();
                    let icc = if identifier == "ICC_PROFILE" && payload.len() >= null_pos + 3 {
                        Some(IccProfileSequenceInfo {
                            chunk_sequence: payload[null_pos + 1],
                            total_chunks: payload[null_pos + 2],
                            profile_data_length: payload.len().saturating_sub(null_pos + 3) as u32,
                        })
                    } else {
                        None
                    };
                    self.sections.push(JpegLsSectionInfo { start_offset: marker_start, data: JpegLsSectionData::App(JpegLsAppData { marker: 0xFFE2, length: len, identifier: Some(identifier), jfif: None, exif: None, icc_profile_sequence: icc, adobe: None, color_transform: None }) });
                }
                JpegLsMarker::APP14 => {
                    let len = self.read_u16()?;
                    let payload = self.reader.read_bytes(len.saturating_sub(2) as usize)?;
                    let adobe = if payload.starts_with(b"Adobe") && payload.len() >= 12 {
                        Some(APP14AdobeData {
                            length: len,
                            version: u16::from_be_bytes([payload[5], payload[6]]),
                            flags0: u16::from_be_bytes([payload[7], payload[8]]),
                            flags1: u16::from_be_bytes([payload[9], payload[10]]),
                            color_transform: payload[11],
                        })
                    } else {
                        None
                    };
                    let identifier = if payload.len() >= 5 {
                        let null_pos = payload.iter().position(|&b| b == 0).unwrap_or(payload.len().min(32));
                        Some(String::from_utf8_lossy(&payload[..null_pos]).to_string())
                    } else {
                        None
                    };
                    self.sections.push(JpegLsSectionInfo { start_offset: marker_start, data: JpegLsSectionData::App(JpegLsAppData { marker: 0xFFEE, length: len, identifier, jfif: None, exif: None, icc_profile_sequence: None, adobe, color_transform: None }) });
                }
                JpegLsMarker::APP3
                | JpegLsMarker::APP4
                | JpegLsMarker::APP5
                | JpegLsMarker::APP6
                | JpegLsMarker::APP7
                | JpegLsMarker::APP9
                | JpegLsMarker::APP10
                | JpegLsMarker::APP11
                | JpegLsMarker::APP12
                | JpegLsMarker::APP13
                | JpegLsMarker::APP15 => {
                    let len = self.read_u16()?;
                    let payload = self.reader.read_bytes(len.saturating_sub(2) as usize)?;
                    let marker_u16 = marker.to_u16();
                    let null_pos = payload.iter().position(|&b| b == 0).unwrap_or(payload.len().min(32));
                    let identifier = if null_pos > 0 && payload[..null_pos].iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
                        String::from_utf8(payload[..null_pos].to_vec()).ok()
                    } else {
                        None
                    };
                    self.sections.push(JpegLsSectionInfo { start_offset: marker_start, data: JpegLsSectionData::App(JpegLsAppData { marker: marker_u16, length: len, identifier, jfif: None, exif: None, icc_profile_sequence: None, adobe: None, color_transform: None }) });
                }
                JpegLsMarker::COM => {
                    let len = self.read_u16()?;
                    let payload_len = len.saturating_sub(2) as usize;
                    let mut text_bytes = Vec::with_capacity(payload_len);
                    for _ in 0..payload_len {
                        text_bytes.push(self.read_u8()?);
                    }
                    let text = String::from_utf8_lossy(&text_bytes).into_owned();
                    self.sections.push(JpegLsSectionInfo {
                        start_offset: marker_start,
                        data: JpegLsSectionData::Com(JpegLsComData { length: len, text }),
                    });
                }
                _ => {
                    log_warn!("Unhandled JPEG-LS marker: {:?}", marker);
                }
            }

        }

        let frame = match self.frame.as_ref() {
            Some(f) => f.clone(),
            None => return Err(crate::utils::error::VexelError::Custom("No SOF marker found".into())),
        };

        if self.scans.is_empty() {
            return Err(crate::utils::error::VexelError::Custom("No SOS marker found".into()));
        }

        let width = frame.width as usize;
        let height = frame.height as usize;
        let components = frame.component_count();

        let scans = std::mem::take(&mut self.scans);

        let max_h = frame.components.iter().map(|c| c.horizontal_sampling as usize).max().unwrap_or(1);
        let max_v = frame.components.iter().map(|c| c.vertical_sampling as usize).max().unwrap_or(1);
        let all_unity = frame.components.iter().all(|c| c.horizontal_sampling <= 1 && c.vertical_sampling <= 1);

        let mut scans_alpha: Option<i32> = None;

        let pixel_data: Vec<u16> = if scans.len() > 1 {
            let mut planes: Vec<(Vec<u16>, usize, usize)> = Vec::with_capacity(scans.len());

            for (scan, data) in scans {
                let alpha = if scan.alpha > 1 { scan.alpha } else { frame.alpha };
                scans_alpha.get_or_insert(alpha);
                let near = scan.near;
                let lossy = near > 0;

                let mut bpp = 2i32;
                {
                    while (1 << bpp) < alpha {
                        bpp += 1;
                    }
                }
                if bpp < 2 {
                    bpp = 2;
                }

                let (qbpp, stats_alpha) = if lossy {
                    let quant = 2 * near + 1;
                    let qbeta = (alpha + 2 * near + quant - 1) / quant;
                    let mut qbpp = 2i32;
                    while (1 << qbpp) < qbeta {
                        qbpp += 1;
                    }
                    (qbpp, qbeta)
                } else {
                    (bpp, alpha)
                };

                let limit = if bpp < 8 {
                    2 * (bpp + 8) - qbpp - 1
                } else {
                    4 * bpp - qbpp - 1
                };

                let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };
                let (t1, t2, t3) = compute_thresholds(alpha, near, scan.t1, scan.t2, scan.t3);
                let reset = scan.reset;

                let (plane_w, plane_h) = if all_unity || scan.component_ids.is_empty() {
                    (width, height)
                } else {
                    let comp_id = scan.component_ids[0];
                    if let Some(comp) = frame.components.iter().find(|c| c.id == comp_id) {
                        let pw = width * comp.horizontal_sampling as usize / max_h.max(1);
                        let ph = height * comp.vertical_sampling as usize / max_v.max(1);
                        (pw.max(1), ph.max(1))
                    } else {
                        (width, height)
                    }
                };

                let mut state = DecoderState::new();
                prepare_luts(&mut state, t1, t2, t3, near, lutmax);
                if lossy {
                    prepare_qtables(&mut state, alpha, near);
                }
                init_stats(&mut state, stats_alpha);
                init_run_state(&mut state, 1);

                let plane = decode_single_plane(
                    &mut JlsBitReader::new(data),
                    &mut state,
                    plane_w,
                    plane_h,
                    alpha,
                    near,
                    lossy,
                    limit,
                    qbpp,
                    reset,
                    scan.restart_interval,
                    stats_alpha,
                );
                planes.push((plane, plane_w, plane_h));
            }

            let num_planes = planes.len();
            let mut result = Vec::with_capacity(width * height * num_planes);

            if all_unity {
                for i in 0..(width * height) {
                    for (plane, _, _) in &planes {
                        result.push(plane[i]);
                    }
                }
            } else {
                let comp_samplings: Vec<(usize, usize)> = frame.components.iter()
                    .map(|c| (c.horizontal_sampling as usize, c.vertical_sampling as usize))
                    .collect();

                for row in 0..height {
                    for x in 0..width {
                        for p in 0..num_planes {
                            let (plane, pw, ph) = &planes[p];
                            let (h_samp, v_samp) = if p < comp_samplings.len() {
                                comp_samplings[p]
                            } else {
                                (1, 1)
                            };
                            let src_x = (x * h_samp / max_h).min(pw.saturating_sub(1));
                            let src_y = (row * v_samp / max_v).min(ph.saturating_sub(1));
                            result.push(plane[src_y * pw + src_x]);
                        }
                    }
                }
            }
            result
        } else {
            let (scan, data) = scans.into_iter().next().unwrap();
            let alpha = if scan.alpha > 1 { scan.alpha } else { frame.alpha };
            scans_alpha = Some(alpha);
            let near = scan.near;
            let lossy = near > 0;

            let mut bpp = 2i32;
            {
                while (1 << bpp) < alpha {
                    bpp += 1;
                }
            }
            if bpp < 2 {
                bpp = 2;
            }

            let (qbpp, stats_alpha) = if lossy {
                let quant = 2 * near + 1;
                let qbeta = (alpha + 2 * near + quant - 1) / quant;
                let mut qbpp = 2i32;
                while (1 << qbpp) < qbeta {
                    qbpp += 1;
                }
                (qbpp, qbeta)
            } else {
                (bpp, alpha)
            };

            let limit = if bpp < 8 {
                2 * (bpp + 8) - qbpp - 1
            } else {
                4 * bpp - qbpp - 1
            };

            let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };
            let (t1, t2, t3) = compute_thresholds(alpha, near, scan.t1, scan.t2, scan.t3);
            let reset = scan.reset;

            let mut state = DecoderState::new();
            prepare_luts(&mut state, t1, t2, t3, near, lutmax);
            if lossy {
                prepare_qtables(&mut state, alpha, near);
            }
            init_stats(&mut state, stats_alpha);
            init_run_state(&mut state, components);

            let restart_interval = scan.restart_interval;
            let mut br = JlsBitReader::new(data);

            match scan.interleave_mode {
                InterleaveMode::None => {
                    decode_plane_interleaved(
                        &mut br, &mut state, width, height, components,
                        alpha, near, lossy, limit, qbpp, reset,
                        restart_interval, stats_alpha,
                    )
                }
                InterleaveMode::Line => {
                    let sampling: Vec<(u8, u8)> = frame.components.iter()
                        .map(|c| (c.horizontal_sampling, c.vertical_sampling))
                        .collect();
                    decode_line_interleaved(
                        &mut br, &mut state, width, height, components,
                        alpha, near, lossy, limit, qbpp, reset, &sampling,
                        restart_interval, stats_alpha,
                    )
                }
                InterleaveMode::Sample => {
                    decode_sample_interleaved(
                        &mut br, &mut state, width, height, components,
                        alpha, near, lossy, limit, qbpp, reset,
                        restart_interval, stats_alpha,
                    )
                }
            }
        };

        let scan_alpha = scans_alpha.unwrap_or(frame.alpha);
        let pixel_data = if self.color_transform != 0 && components == 3 {
            apply_hp_color_transform_inverse(&pixel_data, self.color_transform, scan_alpha)
        } else {
            pixel_data
        };

        let image = if frame.precision > 8 {
            if components == 1 {
                Image::from_pixels(frame.width, frame.height, PixelData::L16(pixel_data))
            } else if components == 4 {
                Image::from_pixels(frame.width, frame.height, PixelData::RGBA16(pixel_data))
            } else {
                Image::from_pixels(frame.width, frame.height, PixelData::RGB16(pixel_data))
            }
        } else {
            let pixel_data_u8: Vec<u8> = pixel_data.into_iter().map(|v| (v >> 8) as u8).collect();
            if components == 1 {
                Image::from_pixels(frame.width, frame.height, PixelData::L8(pixel_data_u8))
            } else if components == 4 {
                Image::from_pixels(frame.width, frame.height, PixelData::RGBA8(pixel_data_u8))
            } else {
                Image::from_pixels(frame.width, frame.height, PixelData::RGB8(pixel_data_u8))
            }
        };

        Ok(image)
    }
}

fn apply_hp_color_transform_inverse(data: &[u16], transform: u8, alpha: i32) -> Vec<u16> {
    let scale = 65535 / (alpha - 1);
    let bias = alpha / 2;

    let mut result = Vec::with_capacity(data.len());

    for chunk in data.chunks_exact(3) {
        let v1 = (chunk[0] as i32) / scale;
        let v2 = (chunk[1] as i32) / scale;
        let v3 = (chunk[2] as i32) / scale;

        let (r, g, b) = match transform {
            1 => {
                let g = v2;
                let r = (v1 + g - bias).rem_euclid(alpha);
                let b = (v3 + g - bias).rem_euclid(alpha);
                (r, g, b)
            }
            2 => {
                let g = v2;
                let r = (v1 + g - bias).rem_euclid(alpha);
                let b = (v3 + ((r + g) >> 1) - bias).rem_euclid(alpha);
                (r, g, b)
            }
            3 => {
                let g = (v1 - ((v3 + v2) >> 2) + alpha / 4).rem_euclid(alpha);
                let r = (v3 + g - bias).rem_euclid(alpha);
                let b = (v2 + g - bias).rem_euclid(alpha);
                (r, g, b)
            }
            _ => (v1, v2, v3),
        };

        result.push((r * scale) as u16);
        result.push((g * scale) as u16);
        result.push((b * scale) as u16);
    }

    result
}

fn decode_single_plane(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let buf_size = width + 3;
    let mut prev_line = vec![0i32; buf_size];
    let mut curr_line = vec![0i32; buf_size];
    let mut result = Vec::with_capacity(width * height);

    let max_val = alpha - 1;
    let scale = 65535 / max_val;
    let mut rows_in_interval = 0usize;

    for _row in 0..height {
        if restart_interval > 0 && rows_in_interval == restart_interval {
            br.consume_restart_marker();
            prev_line.iter_mut().for_each(|v| *v = 0);
            curr_line.iter_mut().for_each(|v| *v = 0);
            init_stats(state, stats_alpha);
            init_run_state(state, 1);
            rows_in_interval = 0;
        }

        let first_prev_pixel = prev_line[2];
        curr_line[0] = first_prev_pixel;
        curr_line[1] = first_prev_pixel;

        undoscanline(br, state, &prev_line, &mut curr_line, width, 0, alpha, near, lossy, limit, qbpp, reset);

        curr_line[width + 2] = curr_line[width + 1];

        for x in 2..=width + 1 {
            result.push((curr_line[x].clamp(0, max_val) * scale) as u16);
        }

        std::mem::swap(&mut prev_line, &mut curr_line);
        rows_in_interval += 1;
    }

    result
}

fn decode_plane_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let mut planes: Vec<Vec<u16>> = (0..components)
        .map(|_| Vec::with_capacity(width * height))
        .collect();

    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    for comp in 0..components {
        init_stats(state, stats_alpha);
        init_run_state(state, 1);

        let buf_size = width + 3;
        let mut prev_line = vec![0i32; buf_size];
        let mut curr_line = vec![0i32; buf_size];
        let mut rows_in_interval = 0usize;

        for _row in 0..height {
            if restart_interval > 0 && rows_in_interval == restart_interval {
                br.consume_restart_marker();
                prev_line.iter_mut().for_each(|v| *v = 0);
                curr_line.iter_mut().for_each(|v| *v = 0);
                init_stats(state, stats_alpha);
                init_run_state(state, 1);
                rows_in_interval = 0;
            }

            let first_prev_pixel = prev_line[2];
            curr_line[0] = first_prev_pixel;
            curr_line[1] = first_prev_pixel;

            undoscanline(br, state, &prev_line, &mut curr_line, width, comp, alpha, near, lossy, limit, qbpp, reset);

            curr_line[width + 2] = curr_line[width + 1];

            for x in 2..=width + 1 {
                planes[comp].push((curr_line[x].clamp(0, max_val) * scale) as u16);
            }

            std::mem::swap(&mut prev_line, &mut curr_line);
            rows_in_interval += 1;
        }
    }

    let mut result = Vec::with_capacity(width * height * components);
    for i in 0..(width * height) {
        for comp in 0..components {
            result.push(planes[comp][i]);
        }
    }

    result
}

fn decode_line_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    sampling: &[(u8, u8)],
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    let all_unity = sampling.iter().all(|&(h, v)| h <= 1 && v <= 1);

    if all_unity || sampling.len() != components {
        let buf_size = width + 3;
        let mut prev_lines: Vec<Vec<i32>> = (0..components).map(|_| vec![0i32; buf_size]).collect();
        let mut curr_lines: Vec<Vec<i32>> = (0..components).map(|_| vec![0i32; buf_size]).collect();
        let mut result = Vec::with_capacity(width * height * components);
        let mut rows_in_interval = 0usize;

        for _row in 0..height {
            if restart_interval > 0 && rows_in_interval == restart_interval {
                br.consume_restart_marker();
                for comp in 0..components {
                    prev_lines[comp].iter_mut().for_each(|v| *v = 0);
                    curr_lines[comp].iter_mut().for_each(|v| *v = 0);
                }
                init_stats(state, stats_alpha);
                init_run_state(state, components);
                rows_in_interval = 0;
            }

            for comp in 0..components {
                let first_prev_pixel = prev_lines[comp][2];
                curr_lines[comp][0] = first_prev_pixel;
                curr_lines[comp][1] = first_prev_pixel;

                let prev_clone = prev_lines[comp].clone();
                undoscanline(br, state, &prev_clone, &mut curr_lines[comp], width, comp, alpha, near, lossy, limit, qbpp, reset);

                curr_lines[comp][width + 2] = curr_lines[comp][width + 1];
            }

            for x in 2..=width + 1 {
                for comp in 0..components {
                    result.push((curr_lines[comp][x].clamp(0, max_val) * scale) as u16);
                }
            }

            for comp in 0..components {
                std::mem::swap(&mut prev_lines[comp], &mut curr_lines[comp]);
            }

            rows_in_interval += 1;
        }

        return result;
    }

    let max_h = sampling.iter().map(|&(h, _)| h as usize).max().unwrap_or(1);
    let max_v = sampling.iter().map(|&(_, v)| v as usize).max().unwrap_or(1);

    let c_widths: Vec<usize> = sampling.iter().map(|&(h, _)| width * h as usize / max_h).collect();
    let c_heights: Vec<usize> = sampling.iter().map(|&(_, v)| height * v as usize / max_v).collect();

    let mut comp_data: Vec<Vec<i32>> = (0..components)
        .map(|c| Vec::with_capacity(c_widths[c] * c_heights[c]))
        .collect();

    let mut prev_lines: Vec<Vec<i32>> = (0..components).map(|c| vec![0i32; c_widths[c] + 3]).collect();
    let mut curr_lines: Vec<Vec<i32>> = (0..components).map(|c| vec![0i32; c_widths[c] + 3]).collect();

    let num_groups = c_heights.iter().map(|&h| h / sampling[0].1 as usize).max().unwrap_or(height);
    let mut groups_in_interval = 0usize;
    let interval_in_groups = if restart_interval > 0 {
        let v_samp_max = sampling.iter().map(|&(_, v)| v as usize).max().unwrap_or(1);
        restart_interval / v_samp_max
    } else {
        0
    };

    for _ in 0..num_groups {
        if interval_in_groups > 0 && groups_in_interval == interval_in_groups {
            br.consume_restart_marker();
            for comp in 0..components {
                prev_lines[comp].iter_mut().for_each(|v| *v = 0);
                curr_lines[comp].iter_mut().for_each(|v| *v = 0);
            }
            init_stats(state, stats_alpha);
            init_run_state(state, components);
            groups_in_interval = 0;
        }

        for comp in 0..components {
            let v_samp = sampling[comp].1 as usize;
            let cw = c_widths[comp];

            for _ in 0..v_samp {
                let first_prev_pixel = prev_lines[comp][2];
                curr_lines[comp][0] = first_prev_pixel;
                curr_lines[comp][1] = first_prev_pixel;

                let prev_clone = prev_lines[comp].clone();
                undoscanline(br, state, &prev_clone, &mut curr_lines[comp], cw, comp, alpha, near, lossy, limit, qbpp, reset);

                curr_lines[comp][cw + 2] = curr_lines[comp][cw + 1];

                for x in 2..=cw + 1 {
                    comp_data[comp].push(curr_lines[comp][x]);
                }

                std::mem::swap(&mut prev_lines[comp], &mut curr_lines[comp]);
            }
        }

        groups_in_interval += 1;
    }

    let mut result = Vec::with_capacity(width * height * components);

    for row in 0..height {
        for x in 0..width {
            for comp in 0..components {
                let h_samp = sampling[comp].0 as usize;
                let v_samp = sampling[comp].1 as usize;
                let cw = c_widths[comp];

                let src_x = (x * h_samp / max_h).min(cw.saturating_sub(1));
                let src_y = (row * v_samp / max_v).min(c_heights[comp].saturating_sub(1));

                let val = comp_data[comp][src_y * cw + src_x];
                result.push((val.clamp(0, max_val) * scale) as u16);
            }
        }
    }

    result
}

fn decode_sample_interleaved(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    width: usize,
    height: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
    restart_interval: usize,
    stats_alpha: i32,
) -> Vec<u16> {
    let total_samples = width * components;
    let buf_size = total_samples + components * 3;
    let mut result = Vec::with_capacity(width * height * components);

    let max_val = alpha - 1;
    let scale = 65535 / max_val;

    let mut prev_line = vec![0i32; buf_size];
    let mut curr_line = vec![0i32; buf_size];
    let mut rows_in_interval = 0usize;

    for _row in 0..height {
        if restart_interval > 0 && rows_in_interval == restart_interval {
            br.consume_restart_marker();
            prev_line.iter_mut().for_each(|v| *v = 0);
            curr_line.iter_mut().for_each(|v| *v = 0);
            init_stats(state, stats_alpha);
            init_run_state(state, components);
            rows_in_interval = 0;
        }

        for nc in 0..components {
            let first_prev = prev_line[2 * components + nc];
            curr_line[nc] = first_prev;
            curr_line[components + nc] = first_prev;
        }

        let prev_clone = prev_line.clone();
        undoscanline_pixel(br, state, &prev_clone, &mut curr_line, total_samples, components, alpha, near, lossy, limit, qbpp, reset);

        for nc in 0..components {
            curr_line[total_samples + 2 * components + nc] = curr_line[total_samples + components + nc];
        }

        for i in 0..total_samples {
            result.push((curr_line[2 * components + i].clamp(0, max_val) * scale) as u16);
        }

        std::mem::swap(&mut prev_line, &mut curr_line);
        rows_in_interval += 1;
    }

    result
}

fn predict(ra: i32, rb: i32, rc: i32) -> i32 {
    let minx = ra.min(rb);
    let maxx = ra.max(rb);
    if rc >= maxx {
        minx
    } else if rc <= minx {
        maxx
    } else {
        ra + rb - rc
    }
}

fn lossless_regular_mode(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    q: usize,
    sign: i32,
    px: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let nt = state.n[q];
    let at = state.a[q];

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let abs_errval = {
        let unary = br.count_leading_zeros();
        if unary < limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let errval = if abs_errval & 1 != 0 {
        -(abs_errval + 1) / 2
    } else {
        abs_errval / 2
    };

    let bt = state.b[q];

    let errval = if k == 0 && 2 * bt <= -nt {
        -(errval + 1)
    } else {
        errval
    };

    let abs_errval = errval.abs();

    let current = if sign == -1 {
        let px_adj = clip(px - state.c[q], alpha);
        (px_adj - errval) & (alpha - 1)
    } else {
        let px_adj = clip(px + state.c[q], alpha);
        (px_adj + errval) & (alpha - 1)
    };

    state.b[q] += errval;
    state.a[q] = state.a[q].saturating_add(abs_errval);

    if nt == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    let bt = state.b[q];
    if bt <= -state.n[q] {
        if state.c[q] > MIN_C {
            state.c[q] -= 1;
        }
        state.b[q] += state.n[q];
        if state.b[q] <= -state.n[q] {
            state.b[q] = -state.n[q] + 1;
        }
    } else if bt > 0 {
        if state.c[q] < MAX_C {
            state.c[q] += 1;
        }
        state.b[q] -= state.n[q];
        if state.b[q] > 0 {
            state.b[q] = 0;
        }
    }

    current
}

fn lossless_end_of_run(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    ra: i32,
    rb: i32,
    ri_type: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let q = EOR_0 + ri_type as usize;
    let nt = state.n[q];
    let at = if ri_type != 0 {
        state.a[q].saturating_add(nt / 2)
    } else {
        state.a[q]
    };

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let eor_limit = limit - state.limit_reduce;

    let merrval = {
        let unary = br.count_leading_zeros();
        if unary < eor_limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let oldmap = if k == 0 && (ri_type != 0 || merrval != 0) && 2 * state.b[q] < nt {
        1i32
    } else {
        0i32
    };

    let merrval = merrval + ri_type + oldmap;

    let (errval, abs_errval) = if merrval & 1 != 0 {
        let e = oldmap - (merrval + 1) / 2;
        let ae = -e - ri_type;
        state.b[q] += 1;
        (e, ae)
    } else {
        let e = merrval / 2;
        let ae = e - ri_type;
        (e, ae)
    };

    let ix = if rb < ra {
        (rb - errval) & (alpha - 1)
    } else {
        (rb + errval) & (alpha - 1)
    };

    state.a[q] = state.a[q].saturating_add(abs_errval);

    if state.n[q] == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    ix
}

fn process_run_dec(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    line_left: i32,
    color: usize,
) -> i32 {
    let mut runlen = 0i32;

    loop {
        let top_byte = (!(br.reg >> 24)) as u8;
        let temp = br.zero_lut[top_byte as usize] as i32;

        for hits in 1..=temp {
            runlen += state.melc_order[color];
            if runlen >= line_left {
                if runlen == line_left && state.melc_state[color] < MELC_STATES - 1 {
                    state.melc_state[color] += 1;
                    state.melc_len[color] = J_TABLE[state.melc_state[color]];
                    state.melc_order[color] = 1 << state.melc_len[color];
                }
                br.fill_buffer(hits);
                return line_left;
            }
            if state.melc_state[color] < MELC_STATES - 1 {
                state.melc_state[color] += 1;
                state.melc_len[color] = J_TABLE[state.melc_state[color]];
                state.melc_order[color] = 1 << state.melc_len[color];
            }
        }

        if temp != 8 {
            br.fill_buffer(temp + 1);
            break;
        }
        br.fill_buffer(8);
    }

    if state.melc_len[color] > 0 {
        let extra = br.get_bits(state.melc_len[color]) as i32;
        runlen += extra;
    }
    state.limit_reduce = state.melc_len[color] + 1;

    if state.melc_state[color] > 0 {
        state.melc_state[color] -= 1;
        state.melc_len[color] = J_TABLE[state.melc_state[color]];
        state.melc_order[color] = 1 << state.melc_len[color];
    }

    runlen
}

fn clip(x: i32, alpha: i32) -> i32 {
    if x < 0 {
        0
    } else if x >= alpha {
        alpha - 1
    } else {
        x
    }
}

fn lossy_regular_mode(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    q: usize,
    sign: i32,
    px: i32,
    alpha: i32,
    near: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let nt = state.n[q];
    let at = state.a[q];

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let abs_errval = {
        let unary = br.count_leading_zeros();
        if unary < limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let errval = if abs_errval & 1 != 0 {
        -(abs_errval + 1) / 2
    } else {
        abs_errval / 2
    };

    let bt = state.b[q];
    let errval = if k == 0 && near == 0 && 2 * bt <= -nt {
        -(errval + 1)
    } else {
        errval
    };

    let abs_errval = errval.abs();
    let errval = state.qmul(errval);

    let current = if sign == -1 {
        let px_adj = clip(px - state.c[q], alpha);
        px_adj - errval
    } else {
        let px_adj = clip(px + state.c[q], alpha);
        px_adj + errval
    };

    let current = if current < state.neg_near {
        current + state.beta
    } else if current > state.alpha1eps {
        current - state.beta
    } else {
        current
    };
    let current = clip(current, alpha);

    state.b[q] += errval;
    state.a[q] = state.a[q].saturating_add(abs_errval);

    if nt == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    let bt = state.b[q];
    if bt <= -state.n[q] {
        if state.c[q] > MIN_C {
            state.c[q] -= 1;
        }
        state.b[q] += state.n[q];
        if state.b[q] <= -state.n[q] {
            state.b[q] = -state.n[q] + 1;
        }
    } else if bt > 0 {
        if state.c[q] < MAX_C {
            state.c[q] += 1;
        }
        state.b[q] -= state.n[q];
        if state.b[q] > 0 {
            state.b[q] = 0;
        }
    }

    current
}

fn lossy_end_of_run(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    ra: i32,
    rb: i32,
    ri_type: i32,
    alpha: i32,
    limit: i32,
    qbpp: i32,
    reset: i32,
) -> i32 {
    let q = EOR_0 + ri_type as usize;
    let nt = state.n[q];
    let at = if ri_type != 0 {
        state.a[q].saturating_add(nt / 2)
    } else {
        state.a[q]
    };

    let mut k = 0i32;
    {
        let mut nst = nt;
        while nst < at {
            nst = nst.saturating_mul(2);
            k += 1;
        }
    }

    let eor_limit = limit - state.limit_reduce;

    let merrval = {
        let unary = br.count_leading_zeros();
        if unary < eor_limit {
            if k > 0 {
                let low = br.get_bits(k) as i32;
                (unary << k) + low
            } else {
                unary
            }
        } else {
            let v = br.get_bits(qbpp) as i32;
            v + 1
        }
    };

    let oldmap = if k == 0 && (ri_type != 0 || merrval != 0) && 2 * state.b[q] < nt {
        1i32
    } else {
        0i32
    };

    let merrval = merrval + ri_type + oldmap;

    let (errval, abs_errval) = if merrval & 1 != 0 {
        let e = oldmap - (merrval + 1) / 2;
        let ae = -e - ri_type;
        state.b[q] += 1;
        (e, ae)
    } else {
        let e = merrval / 2;
        let ae = e - ri_type;
        (e, ae)
    };

    let errval = state.qmul(errval);

    let ix = if ri_type != 0 {
        ra + errval
    } else if rb < ra {
        rb - errval
    } else {
        rb + errval
    };

    let ix = if ix < state.neg_near {
        ix + state.beta
    } else if ix > state.alpha1eps {
        ix - state.beta
    } else {
        ix
    };
    let ix = clip(ix, alpha);

    state.a[q] = state.a[q].saturating_add(abs_errval);

    if state.n[q] == reset {
        state.n[q] >>= 1;
        state.a[q] >>= 1;
        state.b[q] >>= 1;
    }

    state.n[q] += 1;

    ix
}

fn undoscanline(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    psl: &[i32],
    sl: &mut Vec<i32>,
    no: usize,
    color: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
) {
    let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };

    let mut rc = psl[1];
    let mut rb = psl[2];
    let mut ra = sl[1];

    let mut i = 2usize;
    while i <= no + 1 {
        let rd = if i + 1 <= no + 1 { psl[i + 1] } else { psl[no + 1] };

        let drd = (rd - rb).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);
        let drb = (rb - rc).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);
        let drc = (rc - ra).clamp(-(lutmax as i32 - 1), lutmax as i32 - 1);

        let cont = state.vlut[0][(drd + lutmax as i32) as usize]
            + state.vlut[1][(drb + lutmax as i32) as usize]
            + state.vlut[2][(drc + lutmax as i32) as usize];

        if cont == 0 {
            let line_left = (no + 1) as i32 - i as i32 + 1;
            let m = process_run_dec(br, state, line_left, color);
            let n = m;

            if n > 0 {
                for j in 0..n as usize {
                    let idx = i + j;
                    if idx <= no + 1 {
                        sl[idx] = ra;
                    }
                }
                i += n as usize;

                if i > no + 1 {
                    return;
                }

                rb = if i <= no + 1 { psl[i] } else { psl[no + 1] };
            }

            if lossy {
                let ri_type = if (rb - ra).abs() <= near { 1 } else { 0 };
                ra = lossy_end_of_run(br, state, ra, rb, ri_type, alpha, limit, qbpp, reset);
            } else {
                ra = lossless_end_of_run(br, state, ra, rb, (ra == rb) as i32, alpha, limit, qbpp, reset);
            }
        } else {
            let px = predict(ra, rb, rc);

            let cont_mapped = state.classmap[cont as usize];
            let (sign, q) = if cont_mapped < 0 {
                (-1i32, (-cont_mapped) as usize)
            } else {
                (1i32, cont_mapped as usize)
            };

            if lossy {
                ra = lossy_regular_mode(br, state, q, sign, px, alpha, near, limit, qbpp, reset);
            } else {
                ra = lossless_regular_mode(br, state, q, sign, px, alpha, limit, qbpp, reset);
            }
        }

        sl[i] = ra;
        rc = rb;
        rb = if i + 1 <= no + 1 { psl[i + 1] } else { psl[no + 1] };
        i += 1;
    }
}

fn undoscanline_pixel(
    br: &mut JlsBitReader,
    state: &mut DecoderState,
    psl: &[i32],
    sl: &mut Vec<i32>,
    no: usize,
    components: usize,
    alpha: i32,
    near: i32,
    lossy: bool,
    limit: i32,
    qbpp: i32,
    reset: i32,
) {
    let lutmax = if alpha > 256 { LUTMAX16 } else { LUTMAX8 };

    let comps = components;
    let mut c_aa = vec![0i32; comps];
    let mut c_bb = vec![0i32; comps];
    let mut c_cc = vec![0i32; comps];
    let mut c_dd = vec![0i32; comps];
    let mut c_cont = vec![0i32; comps];

    for nc in 0..comps {
        c_cc[nc] = psl[comps + nc];
        c_bb[nc] = psl[2 * comps + nc];
        c_aa[nc] = sl[comps + nc];
    }

    let mut i = 2 * comps;
    let mut color = comps - 1;
    let mut was_in_run = false;
    let total = no + comps;

    while i < total + comps {
        if !was_in_run {
            color = (color + 1) % comps;
        } else {
            color = 0;
        }

        if color == 0 {
            for nc in 0..comps {
                c_dd[nc] = psl[i + comps + nc];

                let d0 = c_dd[nc] - c_bb[nc];
                let d1 = c_bb[nc] - c_cc[nc];
                let d2 = c_cc[nc] - c_aa[nc];

                let bound = (lutmax - 1) as i32;
                let d0c = d0.clamp(-bound, bound);
                let d1c = d1.clamp(-bound, bound);
                let d2c = d2.clamp(-bound, bound);

                c_cont[nc] = state.vlut[0][(d0c + lutmax as i32) as usize]
                    + state.vlut[1][(d1c + lutmax as i32) as usize]
                    + state.vlut[2][(d2c + lutmax as i32) as usize];
            }
        }

        let ra = c_aa[color];
        let rb = c_bb[color];
        let rc = c_cc[color];
        let cont = c_cont[color];

        was_in_run = false;

        let test_run = color == 0 && c_cont.iter().all(|&v| v == 0);

        if test_run {
            was_in_run = true;

            let samples_left = ((total + comps - i) / comps) as i32;
            let m = process_run_dec(br, state, samples_left, 0);
            let n = m;

            if n > 0 {
                for j in 0..n as usize {
                    for nc in 0..comps {
                        let idx = i + j * comps + nc;
                        if idx < sl.len() {
                            sl[idx] = c_aa[nc];
                        }
                    }
                }
                i += n as usize * comps;

                if i >= total + comps {
                    return;
                }

                for nc in 0..comps {
                    c_bb[nc] = psl[i + nc];
                    c_dd[nc] = psl[i + comps + nc];
                }
            }

            for nc in 0..comps {
                let ra_nc = c_aa[nc];
                let rb_nc = c_bb[nc];
                c_aa[nc] = if lossy {
                    lossy_end_of_run(br, state, ra_nc, rb_nc, 0, alpha, limit, qbpp, reset)
                } else {
                    lossless_end_of_run(br, state, ra_nc, rb_nc, 0, alpha, limit, qbpp, reset)
                };
            }
        } else {
            let px = predict(ra, rb, rc);
            let cont_mapped = state.classmap[cont as usize];
            let (sign, q) = if cont_mapped < 0 {
                (-1i32, (-cont_mapped) as usize)
            } else {
                (1i32, cont_mapped as usize)
            };

            c_aa[color] = if lossy {
                lossy_regular_mode(br, state, q, sign, px, alpha, near, limit, qbpp, reset)
            } else {
                lossless_regular_mode(br, state, q, sign, px, alpha, limit, qbpp, reset)
            };
        }

        if !was_in_run {
            sl[i] = c_aa[color];
            c_cc[color] = rb;
            c_bb[color] = c_dd[color];
            i += 1;
        } else {
            for nc in 0..comps {
                sl[i + nc] = c_aa[nc];
                c_cc[nc] = c_bb[nc];
                c_bb[nc] = c_dd[nc];
            }
            i += comps;
        }
    }
}
