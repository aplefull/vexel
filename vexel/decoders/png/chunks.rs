use crate::bitreader::BitReader;
use crate::log_warn;
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::VexelResult;
use crate::utils::icc::ICCProfile;
use std::io::{Read, Seek, SeekFrom};
use super::types::*;

pub struct ChunkReader;

impl ChunkReader {
    pub fn read_ihdr<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<(u32, u32, u8, ColorType, CompressionMethod, bool, bool)> {
        let (start_offset, length, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let width = reader.read_u32()?;
        let height = reader.read_u32()?;
        let bit_depth = reader.read_u8()?;
        let color_type_raw = reader.read_u8()?;
        let compression_method_raw = reader.read_u8()?;
        let filter_method = reader.read_u8()?;
        let interlace_method = reader.read_u8()?;

        let bit_depth = match bit_depth {
            1 | 2 | 4 | 8 | 16 => bit_depth,
            _ => {
                log_warn!("Invalid bit depth: {}", bit_depth);
                8
            }
        };

        let color_type = match color_type_raw {
            0 => ColorType::Grayscale,
            2 => ColorType::RGB,
            3 => ColorType::Indexed,
            4 => ColorType::GrayscaleAlpha,
            6 => ColorType::RGBA,
            _ => {
                log_warn!("Invalid color type: {}", color_type_raw);
                ColorType::RGB
            }
        };

        let compression_method = match compression_method_raw {
            0 => CompressionMethod::Deflate,
            1 => CompressionMethod::None,
            _ => {
                log_warn!("Invalid compression method: {}", compression_method_raw);
                CompressionMethod::None
            }
        };

        let has_filters = match filter_method {
            0 => true,
            1 => false,
            _ => {
                log_warn!("Invalid filter method: {}", filter_method);
                true
            }
        };

        let interlace = match interlace_method {
            0 => false,
            1 => true,
            _ => {
                log_warn!("Invalid interlace method: {}", interlace_method);
                false
            }
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length,
            raw_bytes,
            data: PngChunkData::IHDR(IhdrChunkData {
                width,
                height,
                bit_depth,
                color_type,
                compression_method: compression_method_raw,
                filter_method,
                interlace_method,
                crc,
            }),
        });

        Ok((width, height, bit_depth, color_type, compression_method, has_filters, interlace))
    }

    pub fn read_plte<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<Vec<[u8; 3]>> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;

        if length % 3 != 0 {
            log_warn!("PLTE chunk length is not a multiple of 3");
        }

        let entries = length / 3;
        let mut palette = Vec::with_capacity(entries as usize);

        for _ in 0..entries {
            let r = reader.read_u8()?;
            let g = reader.read_u8()?;
            let b = reader.read_u8()?;
            palette.push([r, g, b]);
        }

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::PLTE(PlteChunkData {
                entries: palette.clone(),
                crc,
            }),
        });

        Ok(palette)
    }

    pub fn read_idat<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        idat_data: &mut Vec<u8>,
        frames: &mut Vec<PngFrame>,
    ) -> VexelResult<()> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;
        let mut chunk_data = vec![0; length as usize];
        reader.read_exact(&mut chunk_data)?;

        if !frames.is_empty() {
            let fctl_info = frames.last_mut().unwrap();
            fctl_info.fdat.extend(chunk_data.clone());
        }

        idat_data.extend(chunk_data);

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::IDAT(IdatChunkData {
                data_length: length,
                crc,
            }),
        });

        Ok(())
    }

    pub fn read_iccp<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<(String, ICCProfile)> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;
        let mut num_read = 0;

        let mut profile_name_bytes = Vec::new();
        loop {
            let byte = reader.read_u8()?;
            num_read += 1;

            if byte == 0 {
                break;
            }

            if !((byte >= 32 && byte <= 126) || byte >= 161) {
                log_warn!("Invalid character in iCCP profile name: {}, replacing with space", byte);
                profile_name_bytes.push(32);
            } else {
                profile_name_bytes.push(byte);
            }

            if profile_name_bytes.len() >= 79 {
                log_warn!("iCCP profile name too long");
                break;
            }
        }

        let compression_method = reader.read_u8()?;
        num_read += 1;

        if compression_method != 0 {
            log_warn!("Invalid compression method in iCCP chunk: {}", compression_method);
        }

        let mut compressed_profile = Vec::new();
        while num_read < length {
            compressed_profile.push(reader.read_u8()?);
            num_read += 1;
        }

        let profile_data = ZlibDecoder::from_bytes(compressed_profile).decode();

        let icc = ICCProfile::new(&*profile_data)?;
        let profile_name = String::from_utf8_lossy(&profile_name_bytes).to_string();

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::ICCP(IccpChunkData {
                profile_name: profile_name.clone(),
                profile: icc.clone(),
                crc,
            }),
        });

        Ok((profile_name, icc))
    }

    pub fn read_iend<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<()> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::IEND { crc },
        });

        Ok(())
    }

    pub fn read_splt<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<SuggestedPalette> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;

        let mut name = Vec::new();
        loop {
            let byte = reader.read_u8()?;
            if byte == 0 {
                break;
            }

            if !((byte >= 32 && byte <= 126) || byte >= 161) {
                log_warn!("Invalid character in sPLT name: {}, replacing with space", byte);
                name.push(32);
            }

            name.push(byte);

            if name.len() >= 79 {
                log_warn!("sPLT name too long");
                break;
            }
        }

        let mut sample_depth = reader.read_u8()?;

        if sample_depth != 8 && sample_depth != 16 {
            log_warn!("Invalid sPLT sample depth: {}, assuming 8", sample_depth);
            sample_depth = 8;
        }

        let entry_size = if sample_depth == 8 { 6 } else { 10 };
        let remaining_bytes = length - (name.len() as u32 + 2);

        if remaining_bytes % entry_size as u32 != 0 {
            log_warn!("Invalid sPLT chunk length");
        }

        let num_entries = remaining_bytes / entry_size as u32;

        let mut entries = Vec::new();
        for _ in 0..num_entries {
            let (red, green, blue, alpha) = if sample_depth == 8 {
                (
                    reader.read_u8()? as u16,
                    reader.read_u8()? as u16,
                    reader.read_u8()? as u16,
                    reader.read_u8()? as u16,
                )
            } else {
                (
                    reader.read_u16()?,
                    reader.read_u16()?,
                    reader.read_u16()?,
                    reader.read_u16()?,
                )
            };
            let frequency = reader.read_u16()?;

            entries.push(SuggestedPaletteSample {
                red,
                green,
                blue,
                alpha,
                frequency,
            });
        }

        let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
        let palette = SuggestedPalette {
            name: name_str,
            sample_depth,
            samples: entries,
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::SPLT(SpltChunkData {
                palette: palette.clone(),
                crc,
            }),
        });

        Ok(palette)
    }

    pub fn read_srgb<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<RenderingIntent> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let intent = match reader.read_u8()? {
            0 => RenderingIntent::Perceptual,
            1 => RenderingIntent::RelativeColorimetric,
            2 => RenderingIntent::Saturation,
            3 => RenderingIntent::AbsoluteColorimetric,
            n => {
                log_warn!("Invalid sRGB rendering intent: {}", n);
                RenderingIntent::Perceptual
            }
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::SRGB(SrgbChunkData {
                rendering_intent: intent,
                crc,
            }),
        });

        Ok(intent)
    }

    pub fn read_gama<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<f32> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let gamma_int = reader.read_u32()?;
        let gamma = gamma_int as f32 / 100000.0;

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::GAMA(GamaChunkData {
                gamma,
                gamma_raw: gamma_int,
                crc,
            }),
        });

        Ok(gamma)
    }

    pub fn read_chrm<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<Chromaticities> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let mut buf = vec![0u8; length_u32 as usize];
        reader.read_exact(&mut buf)?;

        let read_u32_be = |buf: &[u8], offset: usize| -> u32 {
            if offset + 4 <= buf.len() {
                u32::from_be_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
            } else {
                0
            }
        };

        let chromaticities = Chromaticities {
            white_point_x: read_u32_be(&buf, 0) as f32 / 100000.0,
            white_point_y: read_u32_be(&buf, 4) as f32 / 100000.0,
            red_x: read_u32_be(&buf, 8) as f32 / 100000.0,
            red_y: read_u32_be(&buf, 12) as f32 / 100000.0,
            green_x: read_u32_be(&buf, 16) as f32 / 100000.0,
            green_y: read_u32_be(&buf, 20) as f32 / 100000.0,
            blue_x: read_u32_be(&buf, 24) as f32 / 100000.0,
            blue_y: read_u32_be(&buf, 28) as f32 / 100000.0,
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::CHRM(ChrmChunkData {
                chromaticities,
                crc,
            }),
        });

        Ok(chromaticities)
    }

    pub fn read_trns<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        color_type: ColorType,
        palette: Option<&Vec<[u8; 3]>>,
    ) -> VexelResult<TransparencyData> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;

        let trns_data = match color_type {
            ColorType::Grayscale => {
                if length != 2 {
                    log_warn!("Invalid tRNS length for grayscale");
                }

                let value = reader.read_u16()?;
                TransparencyData::Grayscale(value)
            }
            ColorType::RGB => {
                if length != 6 {
                    log_warn!("Invalid tRNS length for RGB");
                }

                let r = reader.read_u16()?;
                let g = reader.read_u16()?;
                let b = reader.read_u16()?;

                TransparencyData::RGB(r, g, b)
            }
            ColorType::Indexed => {
                if palette.is_none() {
                    log_warn!("tRNS chunk before PLTE chunk");
                }

                let mut value = vec![0; length as usize];
                reader.read_exact(&mut value)?;

                TransparencyData::Palette(value)
            }
            _ => {
                log_warn!("tRNS chunk not allowed for color type {:?}", color_type);
                TransparencyData::Grayscale(0)
            }
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::TRNS(TrnsChunkData {
                transparency: trns_data.clone(),
                crc,
            }),
        });

        Ok(trns_data)
    }

    pub fn read_bkgd<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        color_type: ColorType,
        palette: Option<&Vec<[u8; 3]>>,
    ) -> VexelResult<BackgroundData> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;

        let background = match color_type {
            ColorType::Grayscale | ColorType::GrayscaleAlpha => {
                if length != 2 {
                    log_warn!("Invalid bKGD length for grayscale");
                }

                let value = reader.read_u16()?;
                BackgroundData::Grayscale(value)
            }
            ColorType::RGB | ColorType::RGBA => {
                if length != 6 {
                    log_warn!("Invalid bKGD length for RGB");
                }

                let r = reader.read_u16()?;
                let g = reader.read_u16()?;
                let b = reader.read_u16()?;

                BackgroundData::RGB(r, g, b)
            }
            ColorType::Indexed => {
                if length != 1 {
                    log_warn!("Invalid bKGD length for indexed color");
                }

                if palette.is_none() {
                    log_warn!("bKGD chunk before PLTE chunk");
                }

                let value = reader.read_u8()?;
                BackgroundData::PaletteIndex(value)
            }
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::BKGD(BkgdChunkData {
                background: background.clone(),
                crc,
            }),
        });

        Ok(background)
    }

    pub fn read_phys<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<PhysicalDimensions> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let pixels_per_unit_x = reader.read_u32()?;
        let pixels_per_unit_y = reader.read_u32()?;
        let unit_specifier = reader.read_u8()?;

        let unit = match unit_specifier {
            0 => PhysicalUnit::Unknown,
            1 => PhysicalUnit::Meter,
            _ => PhysicalUnit::Unknown,
        };

        let dimensions = PhysicalDimensions {
            pixels_per_unit_x,
            pixels_per_unit_y,
            unit,
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::PHYS(PhysChunkData {
                physical_dimensions: dimensions.clone(),
                crc,
            }),
        });

        Ok(dimensions)
    }

    pub fn read_sbit<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        color_type: ColorType,
    ) -> VexelResult<SignificantBits> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;

        let mut chunk_data = vec![0; length as usize];
        reader.read_exact(&mut chunk_data)?;

        let sbit_data = match color_type {
            ColorType::Grayscale => {
                if length != 1 {
                    log_warn!("Invalid sBIT length for grayscale: {}", length);
                }

                SignificantBits::Grayscale {
                    gray: *chunk_data.first().unwrap_or(&0),
                }
            }
            ColorType::RGB => {
                if length != 3 {
                    log_warn!("Invalid sBIT length for RGB: {}", length);
                }

                SignificantBits::RGB {
                    red: *chunk_data.first().unwrap_or(&0),
                    green: *chunk_data.get(1).unwrap_or(&0),
                    blue: *chunk_data.get(2).unwrap_or(&0),
                }
            }
            ColorType::Indexed => {
                if length != 3 {
                    log_warn!("Invalid sBIT length for indexed color: {}", length);
                }

                SignificantBits::Indexed {
                    red: *chunk_data.first().unwrap_or(&0),
                    green: *chunk_data.get(1).unwrap_or(&0),
                    blue: *chunk_data.get(2).unwrap_or(&0),
                }
            }
            ColorType::GrayscaleAlpha => {
                if length != 2 {
                    log_warn!("Invalid sBIT length for grayscale alpha: {}", length);
                }

                SignificantBits::GrayscaleAlpha {
                    gray: *chunk_data.first().unwrap_or(&0),
                    alpha: *chunk_data.get(1).unwrap_or(&0),
                }
            }
            ColorType::RGBA => {
                if length != 4 {
                    log_warn!("Invalid sBIT length for RGBA: {}", length);
                }

                SignificantBits::RGBA {
                    red: *chunk_data.first().unwrap_or(&0),
                    green: *chunk_data.get(1).unwrap_or(&0),
                    blue: *chunk_data.get(2).unwrap_or(&0),
                    alpha: *chunk_data.get(3).unwrap_or(&0),
                }
            }
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::SBIT(SbitChunkData {
                significant_bits: sbit_data.clone(),
                crc,
            }),
        });

        Ok(sbit_data)
    }

    pub fn read_hist<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        palette: Option<&Vec<[u8; 3]>>,
    ) -> VexelResult<Vec<u16>> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        if palette.is_none() {
            log_warn!("Encountered hIST chunk before PLTE chunk");
            return Ok(Vec::new());
        }

        let palette_len = palette.unwrap().len();
        let length = length_u32;

        if length as usize != palette_len * 2 {
            log_warn!("Invalid hIST length: {}, expected {}", length, palette_len * 2);
        }

        let entry_count = (length / 2) as usize;
        let mut frequencies = Vec::new();

        for _ in 0..entry_count {
            frequencies.push(reader.read_u16()?);
        }

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::HIST(HistChunkData {
                frequencies: frequencies.clone(),
                crc,
            }),
        });

        Ok(frequencies)
    }

    pub fn read_time<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<ImageTime> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let year = reader.read_u16()?;
        let month = reader.read_u8()?;
        let day = reader.read_u8()?;
        let hour = reader.read_u8()?;
        let minute = reader.read_u8()?;
        let second = reader.read_u8()?;

        if month < 1 || month > 12 {
            log_warn!("Invalid month in tIME chunk: {}", month);
        }

        if day < 1 || day > 31 {
            log_warn!("Invalid day in tIME chunk: {}", day);
        }

        if hour > 23 {
            log_warn!("Invalid hour in tIME chunk: {}", hour);
        }

        if minute > 59 {
            log_warn!("Invalid minute in tIME chunk: {}", minute);
        }

        if second > 60 {
            log_warn!("Invalid second in tIME chunk: {}", second);
        }

        let time = ImageTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::TIME(TimeChunkData {
                time: time.clone(),
                crc,
            }),
        });

        Ok(time)
    }

    pub fn read_text<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<PngText> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32;
        let mut chunk_data = vec![0u8; length as usize];
        reader.read_exact(&mut chunk_data)?;

        let null_pos = chunk_data.iter().position(|&b| b == 0).unwrap_or(chunk_data.len());
        let keyword = String::from_utf8_lossy(&chunk_data[..null_pos]).to_string();
        let text_bytes = if null_pos + 1 < chunk_data.len() { &chunk_data[null_pos + 1..] } else { &[] };
        let text = String::from_utf8_lossy(text_bytes).to_string();

        let png_text = PngText::Basic {
            keyword: keyword.clone(),
            text: text.clone(),
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::TEXT(TextChunkData {
                text: png_text.clone(),
                crc,
            }),
        });

        Ok(png_text)
    }

    pub fn read_ztxt<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<PngText> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let mut chunk_data = vec![0u8; length_u32 as usize];
        reader.read_exact(&mut chunk_data)?;

        let null_pos = chunk_data.iter().position(|&b| b == 0).unwrap_or(chunk_data.len());
        let keyword = String::from_utf8_lossy(&chunk_data[..null_pos]).to_string();

        let after_null = null_pos + 1;
        let compression_method = chunk_data.get(after_null).copied().unwrap_or(0);

        if compression_method != 0 {
            log_warn!("Unknown compression method in zTXt chunk: {}", compression_method);
            let png_text = PngText::Compressed { keyword, text: String::new() };
            chunks.push(PngChunkInfo {
                start_offset,
                chunk_type: chunk_type_str,
                length: length_u32,
                raw_bytes,
                data: PngChunkData::ZTXT(TextChunkData {
                    text: png_text.clone(),
                    crc,
                }),
            });
            return Ok(png_text);
        }

        let compressed_text = if after_null + 1 < chunk_data.len() {
            chunk_data[after_null + 1..].to_vec()
        } else {
            Vec::new()
        };

        let text_bytes = ZlibDecoder::from_bytes(compressed_text).decode();

        let text = String::from_utf8_lossy(&text_bytes).to_string();

        let png_text = PngText::Compressed {
            keyword: keyword.clone(),
            text: text.clone(),
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::ZTXT(TextChunkData {
                text: png_text.clone(),
                crc,
            }),
        });

        Ok(png_text)
    }

    pub fn read_itxt<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<PngText> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let mut chunk_data = vec![0u8; length_u32 as usize];
        reader.read_exact(&mut chunk_data)?;

        let mut pos = 0;

        let kw_end = chunk_data[pos..].iter().position(|&b| b == 0).unwrap_or(chunk_data.len() - pos);
        let keyword = String::from_utf8_lossy(&chunk_data[pos..pos + kw_end]).to_string();
        pos += kw_end + 1;

        let compression_flag = chunk_data.get(pos).copied().unwrap_or(0);
        pos += 1;
        let compression_method = chunk_data.get(pos).copied().unwrap_or(0);
        pos += 1;

        let lang_end = chunk_data[pos.min(chunk_data.len())..].iter().position(|&b| b == 0).unwrap_or(chunk_data.len().saturating_sub(pos));
        let language_tag = String::from_utf8_lossy(&chunk_data[pos..pos + lang_end]).to_string();
        pos += lang_end + 1;

        let trans_end = chunk_data[pos.min(chunk_data.len())..].iter().position(|&b| b == 0).unwrap_or(chunk_data.len().saturating_sub(pos));
        let translated_keyword = String::from_utf8_lossy(&chunk_data[pos..pos + trans_end]).to_string();
        pos += trans_end + 1;

        let text_bytes = if pos < chunk_data.len() { chunk_data[pos..].to_vec() } else { Vec::new() };

        let text = if compression_flag == 1 {
            if compression_method != 0 {
                log_warn!("Invalid compression method in iTXt chunk: {}", compression_method);
            }

            let decompressed = ZlibDecoder::from_bytes(text_bytes).decode();
            String::from_utf8_lossy(&decompressed).to_string()
        } else {
            String::from_utf8_lossy(&text_bytes).to_string()
        };

        let png_text = PngText::International {
            keyword: keyword.clone(),
            language_tag: language_tag.clone(),
            translated_keyword: translated_keyword.clone(),
            text: text.clone(),
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::ITXT(TextChunkData {
                text: png_text.clone(),
                crc,
            }),
        });

        Ok(png_text)
    }

    pub fn read_actl<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
    ) -> VexelResult<ActlChunk> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let num_frames = reader.read_u32()?;
        let num_plays = reader.read_u32()?;

        if num_frames == 0 {
            log_warn!("acTL chunk with zero frames");
        }

        let actl = ActlChunk { num_frames, num_plays };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::ACTL(ActlChunkData {
                actl: actl.clone(),
                crc,
            }),
        });

        Ok(actl)
    }

    pub fn read_fctl<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        width: u32,
        height: u32,
    ) -> VexelResult<FctlChunk> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let sequence_number = reader.read_u32()?;
        let mut width_frame = reader.read_u32()?;
        let mut height_frame = reader.read_u32()?;
        let x_offset = reader.read_u32()?;
        let y_offset = reader.read_u32()?;
        let delay_num = reader.read_u16()?;
        let delay_den = reader.read_u16()?;
        let mut dispose_op = reader.read_u8()?;
        let mut blend_op = reader.read_u8()?;

        if x_offset + width_frame > width {
            log_warn!(format!(
                "fcTL width would overflow actual image width, clamping: x_offset={}, width={}, image_width={}",
                x_offset, width_frame, width
            ));
            width_frame = width.saturating_sub(x_offset);
        }

        if y_offset + height_frame > height {
            log_warn!(format!(
                "fcTL height would overflow actual image height, clamping: y_offset={}, height={}, image_height={}",
                y_offset, height_frame, height
            ));
            height_frame = height.saturating_sub(y_offset);
        }

        if width_frame == 0 || height_frame == 0 {
            log_warn!(format!("Invalid fcTL parameters: width={}, height={}", width_frame, height_frame));
        }

        if dispose_op > 2 {
            log_warn!(format!("Invalid fcTL dispose_op: {}", dispose_op));
            dispose_op = 0;
        }

        if blend_op > 1 {
            log_warn!(format!("Invalid fcTL blend_op: {}", blend_op));
            blend_op = 0;
        }

        let fctl = FctlChunk {
            sequence_number,
            width: width_frame,
            height: height_frame,
            x_offset,
            y_offset,
            delay_num,
            delay_den,
            dispose_op,
            blend_op,
        };

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::FCTL(FctlChunkData {
                fctl: fctl.clone(),
                crc,
            }),
        });

        Ok(fctl)
    }

    pub fn read_fdat<R: Read + Seek>(
        reader: &mut BitReader<R>,
        chunks: &mut Vec<PngChunkInfo>,
        frames: &mut Vec<PngFrame>,
    ) -> VexelResult<()> {
        let (start_offset, length_u32, raw_bytes, chunk_type_str, crc) = capture_chunk_info(reader)?;

        let length = length_u32 - 4;

        let sequence_number = reader.read_u32()?;

        if frames.is_empty() {
            log_warn!("fdAT chunk without preceding fcTL chunk");
            return Ok(());
        }

        let mut frame_data = vec![0; length as usize];
        reader.read_exact(&mut frame_data)?;

        if let Some(frame) = frames.last_mut() {
            frame.fdat.extend(frame_data);
        }

        chunks.push(PngChunkInfo {
            start_offset,
            chunk_type: chunk_type_str,
            length: length_u32,
            raw_bytes,
            data: PngChunkData::FDAT(FdatChunkData {
                sequence_number,
                data_length: length,
                crc,
            }),
        });

        Ok(())
    }
}

pub fn capture_chunk_info<R: Read + Seek>(
    reader: &mut BitReader<R>,
) -> VexelResult<(u64, u32, Vec<u8>, String, u32)> {
    let pos_before = reader.stream_position()?;
    
    // Reader is positioned right after chunk type (which was read into the window)
    // Chunk structure: [length:4][type:4][data:length][crc:4]
    // We're at position after type, so: start_offset = current - 4 (type) - 4 (length) = current - 8
    let start_offset = pos_before - 8;
    
    reader.seek(SeekFrom::Start(start_offset))?;

    let length_u32 = reader.read_u32()?;
    
    let mut chunk_type = vec![0; 4];
    reader.read_exact(&mut chunk_type)?;
    let chunk_type_str = String::from_utf8_lossy(&chunk_type).to_string();

    let mut chunk_data = vec![0; length_u32 as usize];
    reader.read_exact(&mut chunk_data)?;

    let crc = reader.read_u32()?;

    let calculator = CrcCalculator::new();
    let calculated_crc = calculator.calculate_crc_two_parts(&chunk_type, &chunk_data);
    
    if calculated_crc != crc {
        log_warn!(
            "CRC mismatch for chunk {}: expected 0x{:08x}, calculated 0x{:08x}",
            chunk_type_str,
            crc,
            calculated_crc
        );
    }

    let total_size = 4 + 4 + length_u32 as usize + 4;
    let mut raw_bytes = Vec::with_capacity(total_size);
    raw_bytes.extend_from_slice(&length_u32.to_be_bytes());
    raw_bytes.extend_from_slice(&chunk_type);
    raw_bytes.extend_from_slice(&chunk_data);
    raw_bytes.extend_from_slice(&crc.to_be_bytes());

    reader.seek(SeekFrom::Start(start_offset + 8))?;

    Ok((start_offset, length_u32, raw_bytes, chunk_type_str, crc))
}
