use crate::utils::bitreader::BitReader;
use crate::utils::error::VexelResult;
use crate::utils::info::WebpInfo;
use crate::{log_debug, log_warn, Image};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebpChunk {
    // Core chunks
    VP8,  // Simple lossy
    VP8L, // Simple lossless
    VP8X, // Extended features

    // Extended feature chunks
    ALPH, // Alpha
    ANIM, // Animation control
    ANMF, // Animation frame

    // Metadata chunks
    EXIF, // EXIF metadata
    XMP,  // XMP metadata
    ICCP, // ICC color profile

    // Processing chunks
    FRGM, // Fragment
    TILE, // Tile information

    // Additional chunks
    CHRM, // Chrome-specific
    META, // Additional metadata
    GDTA, // Google data
    NDAT, // Named data

    // Unknown chunk
    Unknown,
}

impl WebpChunk {
    fn from_bytes(bytes: [u8; 4]) -> Self {
        match &bytes {
            b"VP8 " => WebpChunk::VP8,
            b"VP8L" => WebpChunk::VP8L,
            b"VP8X" => WebpChunk::VP8X,
            b"ALPH" => WebpChunk::ALPH,
            b"ANIM" => WebpChunk::ANIM,
            b"ANMF" => WebpChunk::ANMF,
            b"EXIF" => WebpChunk::EXIF,
            b"XMP " => WebpChunk::XMP,
            b"ICCP" => WebpChunk::ICCP,
            b"FRGM" => WebpChunk::FRGM,
            b"TILE" => WebpChunk::TILE,
            b"CHRM" => WebpChunk::CHRM,
            b"META" => WebpChunk::META,
            b"GDTA" => WebpChunk::GDTA,
            b"NDAT" => WebpChunk::NDAT,
            _ => WebpChunk::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum WebpCompressionType {
    Lossy,
    Lossless,
    Extended,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpAnimationInfo {
    pub frame_count: u32,
    pub loop_count: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpFrame {
    pub width: u32,
    pub height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub duration: u32,
    pub blend: bool,
    pub dispose: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpExtendedInfo {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub features: WebpFeatures,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct WebpFeatures {
    pub has_alpha: bool,
    pub has_animation: bool,
    pub has_icc: bool,
    pub has_exif: bool,
    pub has_xmp: bool,
    pub has_tiling: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AlphaChunkInfo {
    pub compression_method: u8,
    pub filter: u8,
    pub preprocessing: u8,
}

pub struct WebpDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    compression_type: WebpCompressionType,
    has_alpha: bool,
    has_animation: bool,
    animation_info: Option<WebpAnimationInfo>,
    extended_info: Option<WebpExtendedInfo>,
    alpha_info: Option<AlphaChunkInfo>,
    background_color: Option<[u8; 4]>,
    frames: Vec<WebpFrame>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> WebpDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            compression_type: WebpCompressionType::Lossy,
            has_alpha: false,
            has_animation: false,
            animation_info: None,
            extended_info: None,
            alpha_info: None,
            background_color: None,
            frames: Vec::new(),
            reader: BitReader::with_le(reader),
        }
    }

    pub fn get_info(&self) -> WebpInfo {
        WebpInfo {
            width: self.width,
            height: self.height,
            compression_type: self.compression_type,
            has_alpha: self.has_alpha,
            has_animation: self.has_animation,
            animation_info: self.animation_info.clone(),
            background_color: self.background_color,
            frames: self.frames.clone(),
            extended_info: self.extended_info.clone(),
            alpha_info: self.alpha_info.clone(),
        }
    }

    fn read_riff_header(&mut self) -> VexelResult<()> {
        let header = self.reader.read_bytes(12)?;

        if &header[0..4] != b"RIFF" {
            log_warn!("Invalid RIFF signature");
        }

        if &header[8..12] != b"WEBP" {
            log_warn!("Invalid WEBP signature");
        }

        Ok(())
    }

    fn read_lossy_header(&mut self) -> VexelResult<()> {
        self.compression_type = WebpCompressionType::Lossy;
        self.reader.read_bytes(8)?;
        
        let frame_type = self.reader.read_bit()?;
        let version_number = self.reader.read_bits(3)?;
        let show_frame = self.reader.read_bit()?;
        let first_partition_size = self.reader.read_bits(19)?;
        
        // If it's a key frame:
        let bytes = self.reader.read_bytes(3)?;
        
        if bytes.get(0) != Some(&0x9d) || bytes.get(1) != Some(&0x01) || bytes.get(2) != Some(&0x2a) {
            log_warn!("Invalid lossy signature");
        }

        self.width = self.reader.read_bits(16)?;
        self.height = self.reader.read_bits(16)?;

        Ok(())
    }

    fn read_lossless_header(&mut self) -> VexelResult<()> {
        self.compression_type = WebpCompressionType::Lossless;
        self.reader.read_bytes(8)?;
        
        let signature = self.reader.read_u8()?;
        
        if signature != 0x2f {
            log_warn!("Invalid lossless signature");
        }
        
        self.width = self.reader.read_bits(14)?;
        self.height = self.reader.read_bits(14)?;
        self.has_alpha = self.reader.read_bit()?;

        Ok(())
    }

    fn read_extended_header(&mut self) -> VexelResult<()> {
        self.compression_type = WebpCompressionType::Extended;
        self.reader.read_bytes(8)?;

        let flags = self.reader.read_u8()?;
        let features = WebpFeatures {
            has_alpha: (flags & 0x10) != 0,
            has_animation: (flags & 0x02) != 0,
            has_icc: (flags & 0x20) != 0,
            has_xmp: (flags & 0x04) != 0,
            has_exif: (flags & 0x08) != 0,
            has_tiling: (flags & 0x40) != 0,
        };

        self.has_alpha = features.has_alpha;
        self.has_animation = features.has_animation;

        let canvas_width = self.reader.read_u24()? + 1;
        let canvas_height = self.reader.read_u24()? + 1;

        self.extended_info = Some(WebpExtendedInfo {
            canvas_width,
            canvas_height,
            features,
        });

        Ok(())
    }

    fn read_alpha_chunk(&mut self) -> VexelResult<()> {
        self.has_alpha = true;

        let header = self.reader.read_u8()?;

        let compression_method = (header >> 2) & 0x03;
        let filter = (header >> 4) & 0x03;
        let preprocessing = (header >> 6) & 0x03;

        self.alpha_info = Some(AlphaChunkInfo {
            compression_method,
            filter,
            preprocessing,
        });

        Ok(())
    }

    fn read_animation_chunk(&mut self) -> VexelResult<()> {
        self.has_animation = true;
        self.reader.read_bytes(8)?;

        let bg_color_bytes = self.reader.read_bytes(4)?;
        self.background_color = Some([
            *bg_color_bytes.get(0).unwrap_or(&0),
            *bg_color_bytes.get(1).unwrap_or(&0),
            *bg_color_bytes.get(2).unwrap_or(&0),
            *bg_color_bytes.get(3).unwrap_or(&0),
        ]);

        let loop_count = self.reader.read_u16()?;

        self.animation_info = Some(WebpAnimationInfo {
            frame_count: 0,
            loop_count: loop_count as u32,
        });

        Ok(())
    }

    fn read_frame_chunk(&mut self) -> VexelResult<()> {
        let chunk_size = self.read_chunk_size()?;
        self.reader.read_bytes(8)?;

        let mut frame = WebpFrame {
            x_offset: self.reader.read_u24()?,
            y_offset: self.reader.read_u24()?,
            width: self.reader.read_u24()? + 1,
            height: self.reader.read_u24()? + 1,
            duration: self.reader.read_u24()?,
            blend: false,
            dispose: false,
        };

        let byte = self.reader.read_u8()?;
        frame.blend = (byte & 2) != 0;
        frame.dispose = (byte & 1) != 0;

        self.frames.push(frame);

        if let Some(ref mut info) = self.animation_info {
            info.frame_count = self.frames.len() as u32;
        }
        
        self.reader.seek(SeekFrom::Current(chunk_size as i64 - 16))?;

        Ok(())
    }

    fn read_icc_chunk(&mut self, size: u32) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Current(size as i64))?;
        Ok(())
    }

    fn read_exif_chunk(&mut self, size: u32) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Current(size as i64))?;
        Ok(())
    }

    fn read_xmp_chunk(&mut self, size: u32) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Current(size as i64))?;
        Ok(())
    }

    fn read_tile_chunk(&mut self, size: u32) -> VexelResult<()> {
        self.reader.seek(SeekFrom::Current(size as i64))?;
        Ok(())
    }

    fn read_chunk_size(&mut self) -> VexelResult<u32> {
        // Skip chunk tag
        self.reader.read_u32()?;

        // Read chunk size
        let size = self.reader.read_u32()?;

        // Seek back to start of chunk
        self.reader.seek(SeekFrom::Current(-8))?;

        Ok(size)
    }

    fn find_next_chunk(&mut self) -> VexelResult<Option<WebpChunk>> {
        let mut buffer = [0u8; 8];
        let mut temp = [0u8; 1];

        while self.reader.read_exact(&mut temp).is_ok() {
            buffer.copy_within(1.., 0);
            buffer[7] = temp[0];

            let potential_chunk = WebpChunk::from_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);

            if potential_chunk != WebpChunk::Unknown {
                self.reader.seek(SeekFrom::Current(-8))?;

                return Ok(Some(potential_chunk));
            }
        }

        // End of file
        Ok(None)
    }

    fn read_chunks(&mut self) -> VexelResult<()> {
        while let Some(chunk) = self.find_next_chunk()? {
            log_debug!("Reading chunk: {:?}", chunk);

            let result = match chunk {
                WebpChunk::VP8 => {
                    self.compression_type = WebpCompressionType::Lossy;
                    self.read_lossy_header()
                }
                WebpChunk::VP8L => {
                    self.compression_type = WebpCompressionType::Lossless;
                    self.read_lossless_header()
                }
                WebpChunk::VP8X => {
                    self.compression_type = WebpCompressionType::Extended;
                    self.read_extended_header()
                }
                WebpChunk::ALPH => self.read_alpha_chunk(),
                WebpChunk::ANIM => self.read_animation_chunk(),
                WebpChunk::ANMF => self.read_frame_chunk(),
                WebpChunk::ICCP => self.read_icc_chunk(0),
                WebpChunk::EXIF => self.read_exif_chunk(0),
                WebpChunk::XMP => self.read_xmp_chunk(0),
                WebpChunk::TILE => self.read_tile_chunk(0),
                WebpChunk::Unknown => {
                    log_warn!("Skipping unknown chunk");

                    Ok(())
                }
                _ => {
                    log_warn!("Unhandled chunk: {:?}", chunk);

                    Ok(())
                }
            };

            if let Err(e) = result {
                log_warn!("Error reading chunk {:?}: {:?}", chunk, e);
            }
        }

        Ok(())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        match self.read_riff_header() {
            Ok(_) => (),
            Err(e) => {
                log_warn!("Error reading RIFF header: {:?}", e);
            }
        };

        match self.read_chunks() {
            Ok(_) => (),
            Err(e) => {
                log_warn!("Error reading WebP chunks: {:?}", e);
            }
        };

        println!("{:?}", self.get_info());

        Ok(Image::default())
    }
}
