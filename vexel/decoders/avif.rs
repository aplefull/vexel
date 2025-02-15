use crate::utils::bitreader::BitReader;
use crate::utils::error::VexelResult;
use crate::utils::info::AvifInfo;
use crate::{log_debug, log_warn, Image, VexelError};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxType {
    Ftyp, // File Type Box
    Meta, // Meta Box
    Iprp, // Item Properties Box
    Ipco, // Item Property Container
    Ispe, // Image Spatial Extents Property
    Colr, // Colour Information Box
    Pixi, // Pixel Information Property
    Av1C, // AV1 Configuration Box
    Mdat, // Media Data Box
    Hdlr, // Handler Reference Box
    Iinf, // Item Information Box
    Iloc, // Item Location Box
    Pitm, // Primary Item Box
    Iref, // Item Reference Box
    Idat, // Item Data Box
    Infe, // Item Info Entry Box
    Ipma, // Item Property Association Box
    Moov, // Movie Box
    Mvhd, // Movie Header Box
    Trak, // Track Box
    Mdia, // Media Box
    Minf, // Media Information Box
    Stbl, // Sample Table Box
    Tkhd, // Track Header Box
    Stsd, // Sample Description Box
    Udta, // User Data Box
    Avis, // AVIF Sequence (for animated AVIF)
    Unknown,
}

impl BoxType {
    fn from_bytes(bytes: &[u8; 4]) -> Self {
        match bytes {
            b"ftyp" => BoxType::Ftyp,
            b"meta" => BoxType::Meta,
            b"iprp" => BoxType::Iprp,
            b"ipco" => BoxType::Ipco,
            b"ispe" => BoxType::Ispe,
            b"colr" => BoxType::Colr,
            b"pixi" => BoxType::Pixi,
            b"av1C" => BoxType::Av1C,
            b"mdat" => BoxType::Mdat,
            b"hdlr" => BoxType::Hdlr,
            b"iinf" => BoxType::Iinf,
            b"iloc" => BoxType::Iloc,
            b"pitm" => BoxType::Pitm,
            b"iref" => BoxType::Iref,
            b"idat" => BoxType::Idat,
            b"infe" => BoxType::Infe,
            b"ipma" => BoxType::Ipma,
            b"moov" => BoxType::Moov,
            b"mvhd" => BoxType::Mvhd,
            b"trak" => BoxType::Trak,
            b"mdia" => BoxType::Mdia,
            b"minf" => BoxType::Minf,
            b"stbl" => BoxType::Stbl,
            b"tkhd" => BoxType::Tkhd,
            b"stsd" => BoxType::Stsd,
            b"udta" => BoxType::Udta,
            b"avis" => BoxType::Avis,
            _ => {
                log_debug!("Unknown box type: {}", String::from_utf8_lossy(bytes));
                BoxType::Unknown
            }
        }
    }
}

#[derive(Debug)]
struct Box {
    box_type: BoxType,
    size: u64,
    offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum PixelFormat {
    YUV444,
    YUV422,
    YUV420,
    Monochrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
pub enum ColorPrimaries {
    BT709,
    BT2020,
    SRGB,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifMetadata {
    pub major_brand: String,
    pub minor_version: u32,
    pub compatible_brands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifHandlerBox {
    pub version: u8,
    pub flags: u32,
    pub handler_type: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifColorBox {
    pub colour_type: String,
    pub primaries: ColorPrimaries,
    pub transfer_characteristics: u16,
    pub matrix_coefficients: u16,
    pub full_range: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifAv1Config {
    pub profile: u8,
    pub level: u8,
    pub depth: u8,
    pub monochrome: bool,
    pub chroma_subsampling_x: u8,
    pub chroma_subsampling_y: u8,
    pub pixel_format: PixelFormat,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IlocItem {
    pub item_id: u16,
    pub construction_method: u16,
    pub data_reference_index: u16,
    pub base_offset: u64,
    pub extent_offsets: Vec<u64>,
    pub extent_lengths: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifColorInfo {
    pub depth: u8,
    pub pixel_format: PixelFormat,
    pub primaries: ColorPrimaries,
    pub full_range: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifFrameInfo {
    pub duration: u32,
    pub timescale: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifItemLocation {
    pub item_id: u16,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct InfeBox {
    pub version: u8,
    pub item_id: u32,
    pub item_protection_index: u16,
    pub item_type: String,
    pub content_type: String,
    pub content_encoding: Option<String>,
    pub item_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct IinfBox {
    pub version: u8,
    pub flags: u32,
    pub entry_count: u32,
    pub item_infos: Vec<InfeBox>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct HdlrBox {
    pub version: u8,
    pub flags: u32,
    pub pre_defined: u32,
    pub handler_type: String,
    pub handler_name: String,
    pub reserved: [u32; 3],
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct MvhdBox {
    pub version: u8,
    pub creation_time: u64,
    pub modification_time: u64,
    pub timescale: u32,
    pub duration: u64,
    pub rate: f32,        // Fixed point 16.16
    pub volume: f32,      // Fixed point 8.8
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct TkhdBox {
    pub version: u8,
    pub flags: u32,
    pub creation_time: u64,
    pub modification_time: u64,
    pub track_id: u32,
    pub duration: u64,
    pub width: f32,       // Fixed point 16.16
    pub height: f32,      // Fixed point 16.16
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct AvifProperties {
    pub metadata: AvifMetadata,
    pub handler: HdlrBox,
    pub primary_item: u16,
    pub items: Vec<AvifItemLocation>,
    pub color: AvifColorBox,
    pub av1_config: AvifAv1Config,
    pub iloc_items: Vec<IlocItem>,
    pub iinf: Option<IinfBox>,
    pub bits_per_channel: Vec<u8>,
}

pub struct AvifDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    color_info: AvifColorInfo,
    is_animated: bool,
    loop_count: u32,
    frames: Vec<AvifFrameInfo>,
    properties: AvifProperties,
    reader: BitReader<R>,
}

impl<R: Read + Seek> AvifDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            color_info: AvifColorInfo {
                depth: 8,
                pixel_format: PixelFormat::YUV420,
                primaries: ColorPrimaries::Unknown,
                full_range: true,
            },
            is_animated: false,
            loop_count: 0,
            properties: AvifProperties {
                metadata: AvifMetadata {
                    major_brand: String::new(),
                    minor_version: 0,
                    compatible_brands: Vec::new(),
                },
                bits_per_channel: Vec::new(),
                handler: HdlrBox {
                    version: 0,
                    flags: 0,
                    pre_defined: 0,
                    handler_type: String::new(),
                    handler_name: String::new(),
                    reserved: [0, 0, 0],
                },
                primary_item: 0,
                items: Vec::new(),
                color: AvifColorBox {
                    colour_type: String::new(),
                    primaries: ColorPrimaries::Unknown,
                    transfer_characteristics: 0,
                    matrix_coefficients: 0,
                    full_range: false,
                },
                av1_config: AvifAv1Config {
                    profile: 0,
                    level: 0,
                    depth: 8,
                    monochrome: false,
                    chroma_subsampling_x: 0,
                    chroma_subsampling_y: 0,
                    pixel_format: PixelFormat::YUV420,
                },
                iloc_items: Vec::new(),
                iinf: None,
            },
            frames: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn get_info(&self) -> AvifInfo {
        AvifInfo {
            width: self.width,
            height: self.height,
            color_info: self.color_info.clone(),
            is_animated: self.is_animated,
            loop_count: self.loop_count,
            properties: self.properties.clone(),
            frames: self.frames.clone(),
        }
    }

    fn read_box_header(&mut self) -> VexelResult<Option<Box>> {
        let mut size_buf = [0u8; 4];
        if self.reader.read_exact(&mut size_buf).is_err() {
            return Ok(None);
        }

        let mut size = u32::from_be_bytes(size_buf) as u64;

        let mut type_buf = [0u8; 4];
        self.reader.read_exact(&mut type_buf)?;
        let box_type = BoxType::from_bytes(&type_buf);

        if size == 1 {
            let mut large_size_buf = [0u8; 8];
            self.reader.read_exact(&mut large_size_buf)?;
            size = u64::from_be_bytes(large_size_buf);
        }

        let offset = self.reader.stream_position()?;

        Ok(Some(Box { box_type, size, offset }))
    }

    fn read_mvhd_box(&mut self) -> VexelResult<()> {
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];
        let flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        let (creation_time, modification_time, timescale, duration) = if version == 1 {
            // 64-bit values
            let mut time_buf = [0u8; 8];

            self.reader.read_exact(&mut time_buf)?;
            let creation_time = u64::from_be_bytes(time_buf);

            self.reader.read_exact(&mut time_buf)?;
            let modification_time = u64::from_be_bytes(time_buf);

            let mut scale_buf = [0u8; 4];
            self.reader.read_exact(&mut scale_buf)?;
            let timescale = u32::from_be_bytes(scale_buf);

            self.reader.read_exact(&mut time_buf)?;
            let duration = u64::from_be_bytes(time_buf);

            (creation_time, modification_time, timescale, duration)
        } else {
            // 32-bit values
            let mut time_buf = [0u8; 4];

            self.reader.read_exact(&mut time_buf)?;
            let creation_time = u32::from_be_bytes(time_buf) as u64;

            self.reader.read_exact(&mut time_buf)?;
            let modification_time = u32::from_be_bytes(time_buf) as u64;

            self.reader.read_exact(&mut time_buf)?;
            let timescale = u32::from_be_bytes(time_buf);

            self.reader.read_exact(&mut time_buf)?;
            let duration = u32::from_be_bytes(time_buf) as u64;

            (creation_time, modification_time, timescale, duration)
        };

        // Read rate (fixed point 16.16)
        let mut rate_buf = [0u8; 4];
        self.reader.read_exact(&mut rate_buf)?;
        let rate = f32::from_be_bytes(rate_buf) / 65536.0;

        // Read volume (fixed point 8.8)
        let mut volume_buf = [0u8; 2];
        self.reader.read_exact(&mut volume_buf)?;
        let volume = (u16::from_be_bytes(volume_buf) as f32) / 256.0;

        let mvhd = MvhdBox {
            version,
            creation_time,
            modification_time,
            timescale,
            duration,
            rate,
            volume,
        };

        // Skip reserved bytes
        self.reader.seek(SeekFrom::Current(70))?;

        // Calculate frame info
        if timescale > 0 {
            let duration_seconds = duration as f64 / timescale as f64;
            self.frames.push(AvifFrameInfo {
                duration: duration as u32,
                timescale,
            });
            self.is_animated = true;
        }

        log_debug!(
            "MVHD: version={}, timescale={}, duration={}, rate={}",
            version,
            timescale,
            duration,
            rate
        );

        Ok(())
    }

    fn read_loop_count(&mut self) -> VexelResult<()> {
        // Look for a track user data box with loop count
        // The box hierarchy is: moov/trak/udta/loop

        let mut loop_count_buf = [0u8; 4];
        if self.reader.read_exact(&mut loop_count_buf).is_ok() {
            self.loop_count = u32::from_be_bytes(loop_count_buf);
            log_debug!("Found loop count: {}", self.loop_count);
        }

        Ok(())
    }

    fn read_tkhd_box(&mut self) -> VexelResult<()> {
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];
        let flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        let (creation_time, modification_time, track_id, _, duration) = if version == 1 {
            // 64-bit values for times
            let mut time_buf = [0u8; 8];

            self.reader.read_exact(&mut time_buf)?;
            let creation_time = u64::from_be_bytes(time_buf);

            self.reader.read_exact(&mut time_buf)?;
            let modification_time = u64::from_be_bytes(time_buf);

            let mut id_buf = [0u8; 4];
            self.reader.read_exact(&mut id_buf)?;
            let track_id = u32::from_be_bytes(id_buf);

            // Skip reserved
            self.reader.seek(SeekFrom::Current(4))?;

            self.reader.read_exact(&mut time_buf)?;
            let duration = u64::from_be_bytes(time_buf);

            (creation_time, modification_time, track_id, 0u32, duration)
        } else {
            // 32-bit values
            let mut time_buf = [0u8; 4];

            self.reader.read_exact(&mut time_buf)?;
            let creation_time = u32::from_be_bytes(time_buf) as u64;

            self.reader.read_exact(&mut time_buf)?;
            let modification_time = u32::from_be_bytes(time_buf) as u64;

            self.reader.read_exact(&mut time_buf)?;
            let track_id = u32::from_be_bytes(time_buf);

            // Skip reserved
            self.reader.seek(SeekFrom::Current(4))?;

            self.reader.read_exact(&mut time_buf)?;
            let duration = u32::from_be_bytes(time_buf) as u64;

            (creation_time, modification_time, track_id, 0u32, duration)
        };

        // Skip the rest of the transformation matrix and layer/alternate_group
        self.reader.seek(SeekFrom::Current(52))?;

        // Read width and height (fixed point 16.16)
        let mut dim_buf = [0u8; 4];
        self.reader.read_exact(&mut dim_buf)?;
        let width = f32::from_be_bytes(dim_buf) / 65536.0;

        self.reader.read_exact(&mut dim_buf)?;
        let height = f32::from_be_bytes(dim_buf) / 65536.0;

        let tkhd = TkhdBox {
            version,
            flags,
            creation_time,
            modification_time,
            track_id,
            duration,
            width,
            height,
        };

        log_debug!(
            "TKHD: version={}, track_id={}, duration={}, width={}, height={}",
            version,
            track_id,
            duration,
            width,
            height
        );

        Ok(())
    }

    fn read_ftyp_box(&mut self, size: u64) -> VexelResult<()> {
        let mut brand_buf = [0u8; 4];
        self.reader.read_exact(&mut brand_buf)?;
        let major_brand = String::from_utf8_lossy(&brand_buf).into_owned();

        let mut minor_version = [0u8; 4];
        self.reader.read_exact(&mut minor_version)?;
        let minor_version = u32::from_be_bytes(minor_version);

        let remaining = size - 8;
        let num_brands = remaining / 4;
        let mut compatible_brands = Vec::with_capacity(num_brands as usize);
        let mut is_avif = false;

        for _ in 0..num_brands {
            self.reader.read_exact(&mut brand_buf)?;
            let brand = String::from_utf8_lossy(&brand_buf).into_owned();
            if brand == "avif" || brand == "avis" {
                is_avif = true;
            }
            compatible_brands.push(brand);
        }

        self.properties.metadata = AvifMetadata {
            major_brand,
            minor_version,
            compatible_brands,
        };

        if !is_avif {
            return Err(VexelError::UnsupportedFormat("Not an AVIF file".to_string()));
        }
        Ok(())
    }

    fn read_ispe_box(&mut self) -> VexelResult<()> {
        // Skip version and flags
        self.reader.seek(SeekFrom::Current(4))?;

        let mut buf = [0u8; 4];

        // Read width
        self.reader.read_exact(&mut buf)?;
        self.width = u32::from_be_bytes(buf);

        // Read height
        self.reader.read_exact(&mut buf)?;
        self.height = u32::from_be_bytes(buf);

        Ok(())
    }

    fn read_colr_box(&mut self) -> VexelResult<()> {
        let mut colour_type = [0u8; 4];
        self.reader.read_exact(&mut colour_type)?;
        let colour_type = String::from_utf8_lossy(&colour_type).into_owned();

        let mut color_box = AvifColorBox {
            colour_type: colour_type.clone(),
            primaries: ColorPrimaries::Unknown,
            transfer_characteristics: 0,
            matrix_coefficients: 0,
            full_range: false,
        };

        if colour_type == "nclx" {
            let mut buf = [0u8; 2];
            self.reader.read_exact(&mut buf)?;
            let primaries = u16::from_be_bytes(buf);

            color_box.primaries = match primaries {
                1 => ColorPrimaries::BT709,
                9 => ColorPrimaries::BT2020,
                13 => ColorPrimaries::SRGB,
                _ => ColorPrimaries::Unknown,
            };

            // Read transfer characteristics
            self.reader.read_exact(&mut buf)?;
            color_box.transfer_characteristics = u16::from_be_bytes(buf);

            // Read matrix coefficients
            self.reader.read_exact(&mut buf)?;
            color_box.matrix_coefficients = u16::from_be_bytes(buf);

            // Read full range flag
            let mut range_buf = [0u8; 1];
            self.reader.read_exact(&mut range_buf)?;
            color_box.full_range = (range_buf[0] & 0x80) != 0;
        }

        self.properties.color = color_box;
        self.color_info.primaries = self.properties.color.primaries;
        self.color_info.full_range = self.properties.color.full_range;

        Ok(())
    }

    fn read_av1c_box(&mut self) -> VexelResult<()> {
        let mut marker = [0u8; 1];
        self.reader.read_exact(&mut marker)?;

        let mut profile_level = [0u8; 1];
        self.reader.read_exact(&mut profile_level)?;
        let profile = (profile_level[0] >> 5) & 0x07;
        let level = profile_level[0] & 0x1F;

        let mut flags = [0u8; 1];
        self.reader.read_exact(&mut flags)?;
        let monochrome = (flags[0] & 0x08) != 0;
        let chroma_subsampling_x = (flags[0] >> 6) & 0x01;
        let chroma_subsampling_y = (flags[0] >> 5) & 0x01;

        let depth = match (flags[0] >> 1) & 0x07 {
            0 => 8,
            1 => 10,
            2 => 12,
            _ => 8,
        };

        let pixel_format = if monochrome {
            PixelFormat::Monochrome
        } else {
            match (chroma_subsampling_x, chroma_subsampling_y) {
                (0, 0) => PixelFormat::YUV444,
                (1, 0) => PixelFormat::YUV422,
                (1, 1) => PixelFormat::YUV420,
                _ => PixelFormat::YUV420,
            }
        };

        let config = AvifAv1Config {
            profile,
            level,
            depth,
            monochrome,
            chroma_subsampling_x,
            chroma_subsampling_y,
            pixel_format,
        };

        self.properties.av1_config = config;
        self.color_info.depth = depth;
        self.color_info.pixel_format = pixel_format;

        Ok(())
    }

    fn read_hdlr_box(&mut self) -> VexelResult<()> {
        // Version and flags
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];
        let flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        // Pre-defined value
        let mut pre_defined = [0u8; 4];
        self.reader.read_exact(&mut pre_defined)?;
        let pre_defined = u32::from_be_bytes(pre_defined);

        // Handler type (4 bytes)
        let mut handler_type = [0u8; 4];
        self.reader.read_exact(&mut handler_type)?;
        let handler_type = String::from_utf8_lossy(&handler_type).into_owned();

        // Reserved bytes (12 bytes total, three u32s)
        let mut reserved = [0u32; 3];
        for i in 0..3 {
            let mut reserved_bytes = [0u8; 4];
            self.reader.read_exact(&mut reserved_bytes)?;
            reserved[i] = u32::from_be_bytes(reserved_bytes);
        }

        // Handler name (null-terminated string that fills the remainder of the box)
        let mut handler_name = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            match self.reader.read_exact(&mut byte) {
                Ok(_) => {
                    if byte[0] == 0 {
                        break;
                    }
                    handler_name.push(byte[0]);
                }
                Err(_) => break,
            }
        }

        let handler_name = String::from_utf8_lossy(&handler_name).into_owned();

        self.properties.handler = HdlrBox {
            version,
            flags,
            pre_defined,
            handler_type: handler_type.clone(),
            handler_name,
            reserved,
        };

        Ok(())
    }

    fn read_pitm_box(&mut self) -> VexelResult<()> {
        // Skip version and flags
        self.reader.seek(SeekFrom::Current(4))?;

        let mut buf = [0u8; 2];
        self.reader.read_exact(&mut buf)?;
        let item_id = u16::from_be_bytes(buf);

        log_debug!("Primary item ID: {}", item_id);

        Ok(())
    }

    fn read_iloc_box(&mut self) -> VexelResult<()> {
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];

        let mut size_fields = [0u8; 2];
        self.reader.read_exact(&mut size_fields)?;

        let offset_size = (size_fields[0] >> 4) & 0xF;
        let length_size = size_fields[0] & 0xF;

        let base_offset_size = (size_fields[1] >> 4) & 0xF;
        let index_size = if version == 1 || version == 2 {
            size_fields[1] & 0xF
        } else {
            0
        };

        let item_count = if version < 2 {
            let mut count = [0u8; 2];
            self.reader.read_exact(&mut count)?;
            u16::from_be_bytes(count) as u32
        } else {
            let mut count = [0u8; 4];
            self.reader.read_exact(&mut count)?;
            u32::from_be_bytes(count)
        };

        let mut items = Vec::with_capacity(item_count as usize);

        for item_index in 0..item_count {
            let item_id = if version < 2 {
                let mut id = [0u8; 2];

                if let Err(e) = self.reader.read_exact(&mut id) {
                    log_warn!("Failed to read item ID at index {}: {}", item_index, e);
                    break;
                }

                u16::from_be_bytes(id)
            } else {
                let mut id = [0u8; 4];

                if let Err(e) = self.reader.read_exact(&mut id) {
                    log_warn!("Failed to read item ID at index {}: {}", item_index, e);
                    break;
                }

                u32::from_be_bytes(id) as u16
            };

            let construction_method = if version == 1 || version == 2 {
                let mut method = [0u8; 2];

                if let Err(e) = self.reader.read_exact(&mut method) {
                    log_warn!("Failed to read construction method: {}", e);
                    break;
                }

                u16::from_be_bytes(method)
            } else {
                0
            };

            let mut data_ref = [0u8; 2];
            if let Err(e) = self.reader.read_exact(&mut data_ref) {
                log_warn!("Failed to read data reference index: {}", e);
                break;
            }
            
            let data_reference_index = u16::from_be_bytes(data_ref);

            let base_offset = match self.read_sized_int(base_offset_size as usize) {
                Ok(offset) => offset,
                Err(e) => {
                    log_warn!("Failed to read base offset: {}", e);
                    break;
                }
            };

            let mut extent_count_buf = [0u8; 2];
            if let Err(e) = self.reader.read_exact(&mut extent_count_buf) {
                log_debug!("Failed to read extent count: {}", e);
                break;
            }
            
            let extent_count = u16::from_be_bytes(extent_count_buf);

            let mut extent_offsets = Vec::with_capacity(extent_count as usize);
            let mut extent_lengths = Vec::with_capacity(extent_count as usize);

            let mut valid_extents = true;
            for extent_index in 0..extent_count {
                if index_size > 0 {
                    if let Err(e) = self.read_sized_int(index_size as usize) {
                        log_warn!("Failed to read extent index {}: {}", extent_index, e);
                        valid_extents = false;
                        break;
                    }
                }

                match self.read_sized_int(offset_size as usize) {
                    Ok(offset) => extent_offsets.push(offset),
                    Err(e) => {
                        log_warn!("Failed to read extent offset {}: {}", extent_index, e);
                        valid_extents = false;
                        break;
                    }
                }

                match self.read_sized_int(length_size as usize) {
                    Ok(length) => extent_lengths.push(length),
                    Err(e) => {
                        log_warn!("Failed to recad extent length {}: {}", extent_index, e);
                        valid_extents = false;
                        break;
                    }
                }
            }

            if !valid_extents {
                break;
            }

            items.push(IlocItem {
                item_id,
                construction_method,
                data_reference_index,
                base_offset,
                extent_offsets,
                extent_lengths,
            });
        }

        self.properties.iloc_items = items;

        Ok(())
    }

    fn read_infe_box(&mut self, version: u8) -> VexelResult<InfeBox> {
        // Read version and flags for this box
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let infe_version = version_flags[0];
        let infe_flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        let item_id = if version >= 2 {
            let mut buf = [0u8; 4];
            self.reader.read_exact(&mut buf)?;
            u32::from_be_bytes(buf)
        } else {
            let mut buf = [0u8; 2];
            self.reader.read_exact(&mut buf)?;
            u16::from_be_bytes(buf) as u32
        };

        let mut protection_index = [0u8; 2];
        self.reader.read_exact(&mut protection_index)?;
        let item_protection_index = u16::from_be_bytes(protection_index);

        let mut item_type = [0u8; 4];
        self.reader.read_exact(&mut item_type)?;
        let item_type = String::from_utf8_lossy(&item_type).into_owned();

        // Read null-terminated strings
        let mut content_type = Vec::new();
        let mut byte = [0u8; 1];

        while self.reader.read_exact(&mut byte).is_ok() && byte[0] != 0 {
            content_type.push(byte[0]);
        }

        let content_type = String::from_utf8_lossy(&content_type).into_owned();

        // Optional content encoding (null-terminated string)
        let mut content_encoding = None;
        let mut encoding_bytes = Vec::new();

        while self.reader.read_exact(&mut byte).is_ok() && byte[0] != 0 {
            encoding_bytes.push(byte[0]);
        }

        if !encoding_bytes.is_empty() {
            content_encoding = Some(String::from_utf8_lossy(&encoding_bytes).into_owned());
        }

        // Optional item name (null-terminated string)
        let mut item_name = None;
        let mut name_bytes = Vec::new();

        while self.reader.read_exact(&mut byte).is_ok() && byte[0] != 0 {
            name_bytes.push(byte[0]);
        }

        if !name_bytes.is_empty() {
            item_name = Some(String::from_utf8_lossy(&name_bytes).into_owned());
        }

        Ok(InfeBox {
            version: infe_version,
            item_id,
            item_protection_index,
            item_type,
            content_type,
            content_encoding,
            item_name,
        })
    }

    fn read_iinf_box(&mut self) -> VexelResult<()> {
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];
        let flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        let entry_count = if version == 0 {
            let mut count = [0u8; 2];
            self.reader.read_exact(&mut count)?;
            u16::from_be_bytes(count) as u32
        } else {
            let mut count = [0u8; 4];
            self.reader.read_exact(&mut count)?;
            u32::from_be_bytes(count)
        };

        let mut item_infos = Vec::with_capacity(entry_count as usize);

        // Read each item information entry
        for _ in 0..entry_count {
            let mut size = [0u8; 4];
            if self.reader.read_exact(&mut size).is_ok() {
                let size = u32::from_be_bytes(size);
                let infe = self.read_infe_box(version)?;
                item_infos.push(infe);
            }
        }

        self.properties.iinf = Some(IinfBox {
            version,
            flags,
            entry_count,
            item_infos,
        });

        Ok(())
    }

    fn read_pixi_box(&mut self) -> VexelResult<()> {
        // Read version and flags
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];
        let flags = u32::from_be_bytes(version_flags) & 0x00FFFFFF;

        // Read number of channels
        let num_channels = self.reader.read_u8()?;

        // Read bits per channel
        let mut bits_per_channel = vec![0u8; num_channels as usize];
        self.reader.read_exact(&mut bits_per_channel)?;

        log_debug!(
            "PIXI: version={}, flags={}, num_channels={}, bits_per_channel={:?}",
            version,
            flags,
            num_channels,
            bits_per_channel
        );

        // Store the bits per channel information
        self.properties.bits_per_channel = bits_per_channel;

        Ok(())
    }

    fn read_ipma_box(&mut self) -> VexelResult<()> {
        // Skip version and flags
        let mut version_flags = [0u8; 4];
        self.reader.read_exact(&mut version_flags)?;
        let version = version_flags[0];

        let mut entry_count = [0u8; 4];
        self.reader.read_exact(&mut entry_count)?;
        let count = u32::from_be_bytes(entry_count);

        log_debug!("IPMA: version={}, entry_count={}", version, count);

        // Skip associations for now
        Ok(())
    }

    fn read_sized_int(&mut self, size: usize) -> VexelResult<u64> {
        match size {
            0 => Ok(0),
            1 => {
                let mut buf = [0u8; 1];
                self.reader.read_exact(&mut buf)?;
                Ok(buf[0] as u64)
            }
            2 => {
                let mut buf = [0u8; 2];
                self.reader.read_exact(&mut buf)?;
                Ok(u16::from_be_bytes(buf) as u64)
            }
            4 => {
                let mut buf = [0u8; 4];
                self.reader.read_exact(&mut buf)?;
                Ok(u32::from_be_bytes(buf) as u64)
            }
            8 => {
                let mut buf = [0u8; 8];
                self.reader.read_exact(&mut buf)?;
                Ok(u64::from_be_bytes(buf))
            }
            _ => Err(VexelError::Custom(format!("Invalid integer size: {}", size))),
        }
    }

    fn read_boxes(&mut self, end_pos: Option<u64>) -> VexelResult<()> {
        while let Some(box_header) = self.read_box_header()? {
            // Check if we've reached the container's end
            if let Some(end) = end_pos {
                if self.reader.stream_position()? >= end {
                    break;
                }
            }

            let box_end = box_header.offset + box_header.size - 8;

            let result = match box_header.box_type {
                BoxType::Ftyp => self.read_ftyp_box(box_header.size - 8),
                BoxType::Meta => {
                    self.reader.seek(SeekFrom::Current(4));
                    self.read_boxes(Some(box_end))
                }
                BoxType::Moov
                | BoxType::Trak
                | BoxType::Mdia
                | BoxType::Minf
                | BoxType::Stbl
                | BoxType::Iprp
                | BoxType::Ipco => {
                    // All these are container boxes - parse their contents
                    self.read_boxes(Some(box_end))
                }
                BoxType::Mvhd => self.read_mvhd_box(),
                BoxType::Trak => self.read_boxes(Some(box_end)),
                BoxType::Tkhd => self.read_tkhd_box(),
                BoxType::Udta => self.read_loop_count(),
                BoxType::Ispe => self.read_ispe_box(),
                BoxType::Colr => self.read_colr_box(),
                BoxType::Pixi => self.read_pixi_box(),
                BoxType::Av1C => self.read_av1c_box(),
                BoxType::Hdlr => self.read_hdlr_box(),
                BoxType::Iinf => self.read_iinf_box(),
                BoxType::Iloc => self.read_iloc_box(),
                BoxType::Pitm => self.read_pitm_box(),
                BoxType::Ipma => self.read_ipma_box(),
                _ => {
                    log_warn!("Skipping unhandled box: {:?}", box_header);

                    Ok(())
                }
            };

            if let Err(e) = result {
                log_warn!("Error reading box {:?}: {:?}", box_header.box_type, e);
            }

            // Always seek to the end of the box to ensure we're positioned correctly
            self.reader.seek(SeekFrom::Start(box_end))?;
        }

        Ok(())
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        self.read_boxes(None)?;

        println!("{:?}", self.get_info());

        Ok(Image::default())
    }
}
