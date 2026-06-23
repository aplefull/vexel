use crate::bitreader::BitReader;
use crate::utils::deflate::ZlibDecoder;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::icc::ICCProfile;
use crate::utils::info::PngInfo;
use crate::{log_debug, log_warn, Image, PixelData, PixelFormat};
use std::io::{Read, Seek, SeekFrom};

use super::animation::AnimationDecoder;
use super::chunks::{self, ChunkReader};
use super::pixels::PixelDecoder;
use super::types::*;

pub struct PngDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: ColorType,
    compression_method: CompressionMethod,
    has_filters: bool,
    interlace: bool,
    palette: Option<Vec<[u8; 3]>>,
    idat_data: Vec<u8>,
    gamma: Option<f32>,
    icc_profile: Option<(String, ICCProfile)>,
    transparency: Option<TransparencyData>,
    background: Option<BackgroundData>,
    rendering_intent: Option<RenderingIntent>,
    chromaticities: Option<Chromaticities>,
    suggested_palettes: Vec<SuggestedPalette>,
    physical_dimensions: Option<PhysicalDimensions>,
    significant_bits: Option<SignificantBits>,
    histogram: Option<Vec<u16>>,
    modification_time: Option<ImageTime>,
    text_chunks: Vec<PngText>,
    frames: Vec<PngFrame>,
    actl_info: Option<ActlChunk>,
    chunks: Vec<PngChunkInfo>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> PngDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            bit_depth: 0,
            color_type: ColorType::RGB,
            compression_method: CompressionMethod::None,
            has_filters: true,
            interlace: false,
            palette: None,
            idat_data: Vec::new(),
            gamma: None,
            icc_profile: None,
            transparency: None,
            background: None,
            rendering_intent: None,
            chromaticities: None,
            suggested_palettes: Vec::new(),
            physical_dimensions: None,
            significant_bits: None,
            histogram: None,
            modification_time: None,
            text_chunks: Vec::new(),
            frames: Vec::new(),
            actl_info: None,
            chunks: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get_info(&self) -> PngInfo {
        PngInfo {
            sections: self.chunks.clone(),
        }
    }

    fn decode_pixels(&mut self) -> VexelResult<PixelData> {
        if self.compression_method == CompressionMethod::Deflate {
            let bits_per_pixel = match self.color_type {
                ColorType::Grayscale => self.bit_depth as u32,
                ColorType::RGB => self.bit_depth as u32 * 3,
                ColorType::Indexed => self.bit_depth as u32,
                ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
                ColorType::RGBA => self.bit_depth as u32 * 4,
            };
            let bytes_per_row = ((bits_per_pixel * self.width + 7) / 8 + 1) as usize;
            let capacity = bytes_per_row * self.height as usize;
            self.idat_data = ZlibDecoder::from_bytes(std::mem::take(&mut self.idat_data))
                .decode_with_capacity(capacity);
        }

        let bits_per_pixel = match self.color_type {
            ColorType::Grayscale => self.bit_depth as u32,
            ColorType::RGB => self.bit_depth as u32 * 3,
            ColorType::Indexed => self.bit_depth as u32,
            ColorType::GrayscaleAlpha => self.bit_depth as u32 * 2,
            ColorType::RGBA => self.bit_depth as u32 * 4,
        };
        let bytes_per_row = (bits_per_pixel * self.width + 7) / 8 + 1;

        if !self.interlace && bytes_per_row > 0 && self.idat_data.len() > 0 {
            let actual_rows = (self.idat_data.len() as u32) / bytes_per_row;
            if actual_rows < self.height {
                log_warn!(
                    "Decompressed data covers only {} rows, but IHDR declares {}. Adjusting height.",
                    actual_rows,
                    self.height
                );
                self.height = actual_rows;
            }
        }

        if self.idat_data.is_empty() {
            log_warn!(
                "No decompressed pixel data for {}x{} image, filling with zeros",
                self.width,
                self.height
            );
            return Ok(PixelData::RGBA8(vec![0u8; (self.width * self.height * 4) as usize]));
        }

        let pixel_decoder = PixelDecoder::new(
            self.bit_depth,
            self.color_type,
            self.width,
            self.interlace,
            self.palette.clone(),
            self.transparency.clone(),
        );

        let data = pixel_decoder.deinterlace_scan_lines(&self.idat_data, self.width, self.height)?;

        let mut pixels = match self.color_type {
            ColorType::Indexed => pixel_decoder.decode_indexed(data)?,
            ColorType::RGB => pixel_decoder.decode_rgb(data)?,
            ColorType::RGBA => pixel_decoder.decode_rgba(data)?,
            ColorType::Grayscale => pixel_decoder.decode_grayscale(data)?,
            ColorType::GrayscaleAlpha => pixel_decoder.decode_grayscale_alpha(data)?,
        };

        pixels.correct_pixels(self.width, self.height);

        Ok(pixels)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let mut signature = vec![0u8; 8];
        self.reader.seek(SeekFrom::Start(0))?;
        self.reader.read_exact(&mut signature)?;

        self.chunks.push(PngChunkInfo {
            start_offset: 0,
            chunk_type: "Signature".to_string(),
            length: 8,
            data: PngChunkData::Signature,
        });

        let mut window = [0u8; 4];

        let _first_length = self.reader.read_u32()?;

        for i in 0..4 {
            window[i] = self.reader.read_u8()?;
        }

        loop {
            match get_chunk(&window) {
                Some(chunk) => {
                    log_debug!("Found chunk: {:?}", chunk);

                    let chunk_data_start = self.reader.stream_position()?;
                    let chunk_length = {
                        self.reader.seek(SeekFrom::Start(chunk_data_start - 8))?;
                        let len = self.reader.read_u32().unwrap_or(0);
                        self.reader.seek(SeekFrom::Start(chunk_data_start))?;
                        len
                    };
                    let next_chunk_start = chunk_data_start + chunk_length as u64 + 4;

                    let result = match chunk {
                        PngChunk::IHDR => {
                            match ChunkReader::read_ihdr(&mut self.reader, &mut self.chunks) {
                                Ok((width, height, bit_depth, color_type, compression_method, has_filters, interlace)) => {
                                    self.width = width;
                                    self.height = height;
                                    self.bit_depth = bit_depth;
                                    self.color_type = color_type;
                                    self.compression_method = compression_method;
                                    self.has_filters = has_filters;
                                    self.interlace = interlace;

                                    if self.width == 0 || self.height == 0 {
                                        return Err(VexelError::InvalidDimensions {
                                            width: self.width,
                                            height: self.height,
                                        });
                                    }

                                    Ok(())
                                }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::PLTE => {
                            match ChunkReader::read_plte(&mut self.reader, &mut self.chunks) {
                                Ok(palette) => { self.palette = Some(palette); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::IDAT => {
                            ChunkReader::read_idat(
                                &mut self.reader,
                                &mut self.chunks,
                                &mut self.idat_data,
                                &mut self.frames,
                            )
                        }
                        PngChunk::GAMA => {
                            match ChunkReader::read_gama(&mut self.reader, &mut self.chunks) {
                                Ok(gamma) => { self.gamma = Some(gamma); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::CHRM => {
                            match ChunkReader::read_chrm(&mut self.reader, &mut self.chunks) {
                                Ok(chrm) => { self.chromaticities = Some(chrm); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::TEXT => {
                            match ChunkReader::read_text(&mut self.reader, &mut self.chunks) {
                                Ok(text) => { self.text_chunks.push(text); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::ZTXT => {
                            match ChunkReader::read_ztxt(&mut self.reader, &mut self.chunks) {
                                Ok(text) => { self.text_chunks.push(text); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::ITXT => {
                            match ChunkReader::read_itxt(&mut self.reader, &mut self.chunks) {
                                Ok(text) => { self.text_chunks.push(text); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::SRGB => {
                            match ChunkReader::read_srgb(&mut self.reader, &mut self.chunks) {
                                Ok(intent) => { self.rendering_intent = Some(intent); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::TRNS => {
                            match ChunkReader::read_trns(&mut self.reader, &mut self.chunks, self.color_type, self.palette.as_ref()) {
                                Ok(trns) => { self.transparency = Some(trns); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::BKGD => {
                            match ChunkReader::read_bkgd(&mut self.reader, &mut self.chunks, self.color_type, self.palette.as_ref()) {
                                Ok(bkgd) => { self.background = Some(bkgd); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::PHYS => {
                            match ChunkReader::read_phys(&mut self.reader, &mut self.chunks) {
                                Ok(phys) => { self.physical_dimensions = Some(phys); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::SBIT => {
                            match ChunkReader::read_sbit(&mut self.reader, &mut self.chunks, self.color_type) {
                                Ok(sbit) => { self.significant_bits = Some(sbit); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::HIST => {
                            match ChunkReader::read_hist(&mut self.reader, &mut self.chunks, self.palette.as_ref()) {
                                Ok(hist) => { self.histogram = Some(hist); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::TIME => {
                            match ChunkReader::read_time(&mut self.reader, &mut self.chunks) {
                                Ok(time) => { self.modification_time = Some(time); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::SPLT => {
                            match ChunkReader::read_splt(&mut self.reader, &mut self.chunks) {
                                Ok(splt) => { self.suggested_palettes.push(splt); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::ACTL => {
                            match ChunkReader::read_actl(&mut self.reader, &mut self.chunks) {
                                Ok(actl) => { self.actl_info = Some(actl); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::FCTL => {
                            match ChunkReader::read_fctl(&mut self.reader, &mut self.chunks, self.width, self.height) {
                                Ok(fctl) => {
                                    self.frames.push(PngFrame {
                                        fctl_info: fctl,
                                        fdat: Vec::new(),
                                    });
                                    Ok(())
                                }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::FDAT => ChunkReader::read_fdat(&mut self.reader, &mut self.chunks, &mut self.frames),
                        PngChunk::ICCP => {
                            match ChunkReader::read_iccp(&mut self.reader, &mut self.chunks) {
                                Ok(icc) => { self.icc_profile = Some(icc); Ok(()) }
                                Err(e) => Err(e),
                            }
                        }
                        PngChunk::IEND => ChunkReader::read_iend(&mut self.reader, &mut self.chunks),
                    };

                    if let Err(e) = result {
                        log_warn!("Error reading chunk {:?}: {:?}", chunk, e);
                    }

                    if self.reader.seek(SeekFrom::Start(next_chunk_start)).is_err() {
                        break;
                    }

                    let _next_length = match self.reader.read_u32() {
                        Ok(len) => len,
                        Err(_) => break,
                    };

                    for i in 0..4 {
                        window[i] = match self.reader.read_u8() {
                            Ok(b) => b,
                            Err(_) => break,
                        };
                    }
                }
                None => {
                    if let Ok((start_offset, length_u32, chunk_type_str, crc)) =
                        chunks::capture_chunk_info(&mut self.reader)
                    {
                        log_debug!("Unknown chunk: {}", chunk_type_str);

                        self.chunks.push(PngChunkInfo {
                            start_offset,
                            chunk_type: chunk_type_str.clone(),
                            length: length_u32,
                            data: PngChunkData::Unknown {
                                chunk_type: chunk_type_str,
                                length: length_u32,
                                crc,
                            },
                        });
                        
                        let current_pos = self.reader.stream_position()?;
                        
                        self.reader.seek(SeekFrom::Start(current_pos + length_u32 as u64 + 4))?;
                        
                        let _next_length = match self.reader.read_u32() {
                            Ok(len) => len,
                            Err(_) => break,
                        };
                        
                        for i in 0..4 {
                            window[i] = match self.reader.read_u8() {
                                Ok(b) => b,
                                Err(_) => break,
                            };
                        }
                    } else {
                        let byte = match self.reader.read_u8() {
                            Ok(b) => b,
                            Err(_) => break,
                        };
                        window[0] = window[1];
                        window[1] = window[2];
                        window[2] = window[3];
                        window[3] = byte;
                    }
                }
            }
        }

        if self.actl_info.is_some() {
            let mut anim_decoder = AnimationDecoder::new(self.width, self.height);
            let result = anim_decoder.decode_apng_frames(
                &self.frames,
                self.bit_depth,
                self.color_type,
                self.interlace,
                self.palette.clone(),
                self.transparency.clone(),
            );

            if let Ok(image_frames) = result {
                return Ok(Image::new(self.width, self.height, PixelFormat::RGBA8, image_frames));
            } else {
                log_warn!("Error decoding APNG frames: {:?}", result);
            }
        }

        let pixel_data = self.decode_pixels()?;

        Ok(Image::from_pixels(self.width, self.height, pixel_data))
    }
}
