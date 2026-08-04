use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::bitreader::BitReader;
use crate::decoders::png::PngDecoder;
use crate::utils::error::VexelResult;
use crate::utils::image::ImageFrame;
use crate::utils::info::PngInfo;
use crate::{log_warn, Image, Limits};

pub const MAGIC: &[u8; 8] = b"RPGMV\0\0\0";

const RPGMV_HEADER_SIZE: usize = 16;
const ENCRYPTED_SIZE: usize = 16;

const PNG_PREFIX: [u8; ENCRYPTED_SIZE] =
    [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52];

pub struct RpgmvpDecoder<R: Read + Seek> {
    limits: Limits,
    reader: BitReader<R>,
}

impl<R: Read + Seek> RpgmvpDecoder<R> {
    pub fn new(reader: R) -> Self {
        RpgmvpDecoder { limits: Limits::default(), reader: BitReader::new(reader) }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    fn restore_png(&mut self) -> VexelResult<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(0))?;

        let mut header = [0u8; RPGMV_HEADER_SIZE];
        let read = self.reader.read(&mut header)?;

        if read != RPGMV_HEADER_SIZE || &header[..MAGIC.len()] != MAGIC {
            log_warn!("Invalid or truncated RPGMV header: {:?}, continuing anyway", &header[..read.min(8)]);
        }

        let mut encrypted = [0u8; ENCRYPTED_SIZE];
        let read = self.reader.read(&mut encrypted)?;

        if read != ENCRYPTED_SIZE {
            log_warn!("Truncated encrypted RPGMV block: got {} of {} bytes", read, ENCRYPTED_SIZE);
        }

        let body = self.reader.read_to_end()?;

        self.limits.reserve_usize(PNG_PREFIX.len().saturating_add(body.len()))?;

        let mut png = Vec::with_capacity(PNG_PREFIX.len() + body.len());
        png.extend_from_slice(&PNG_PREFIX);
        png.extend_from_slice(&body);

        Ok(png)
    }

    fn png_decoder(&mut self) -> VexelResult<PngDecoder<Cursor<Vec<u8>>>> {
        let png = self.restore_png()?;

        let mut decoder = PngDecoder::new(Cursor::new(png));
        decoder.set_limits(self.limits.clone());

        Ok(decoder)
    }

    pub fn get_info(&mut self) -> VexelResult<PngInfo> {
        let mut decoder = self.png_decoder()?;
        let _ = decoder.decode();

        Ok(decoder.get_info())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.png_decoder()?.decode()
    }

    pub fn decode_frames(&mut self) -> VexelResult<Vec<ImageFrame>> {
        self.png_decoder()?.decode_frames()
    }
}
