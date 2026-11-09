use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::types::*;
use crate::log_warn;
use crate::utils::error::{VexelError, VexelResult};

#[derive(Debug, Clone)]
pub struct ImageHeader {
    pub version: u32,
    pub subversion: u32,
    pub tiling_present: bool,
    pub spatial_transform: u32,
    pub short_header: bool,
    pub long_word: bool,
    pub windowing: bool,
    pub tile_stretch: bool,
    pub premultiplied_alpha: bool,
    pub source_color_bits: u32,
    pub source_depth_bits: u32,
    pub hard_tile_boundaries: bool,
    pub bitstream_format: BitstreamFormat,
    pub index_table_present: bool,
    pub overlap: Overlap,
    pub trim_flexbits_flag: bool,
    pub rb_swapped: bool,
    pub alpha_channel: bool,
    pub source_color_format: ColorFormat,
    pub source_bit_depth: BitDepth,
    pub black_white: bool,
    pub width: u32,
    pub height: u32,
    pub tile_cols: usize,
    pub tile_rows: usize,
    pub tile_x: Vec<u32>,
    pub tile_y: Vec<u32>,
    pub extra_top: u32,
    pub extra_left: u32,
    pub extra_bottom: u32,
    pub extra_right: u32,
}

#[derive(Debug, Clone)]
pub struct PlaneHeader {
    pub internal_color_format: ColorFormat,
    pub scaled_arith: bool,
    pub subband: Subband,
    pub num_channels: usize,
    pub mantissa_len: u8,
    pub exp_bias: i8,
    pub qp_mode: u32,
    pub qp_index_dc: [u8; MAX_CHANNELS],
    pub qp_index_lp: [u8; MAX_CHANNELS],
    pub qp_index_hp: [u8; MAX_CHANNELS],
}

impl PlaneHeader {
    pub fn dc_uniform(&self) -> bool {
        self.qp_mode & 1 == 0
    }

    pub fn lp_uniform(&self) -> bool {
        self.qp_mode & 2 == 0
    }

    pub fn hp_uniform(&self) -> bool {
        self.qp_mode & 4 == 0
    }

    pub fn uses_dc_qp(&self) -> bool {
        self.qp_mode & 0x200 == 0
    }

    pub fn uses_lp_qp(&self) -> bool {
        self.qp_mode & 0x400 == 0
    }

    pub fn ch_mode_dc(&self) -> u8 {
        ((self.qp_mode >> 3) & 3) as u8
    }

    pub fn ch_mode_lp(&self) -> u8 {
        ((self.qp_mode >> 5) & 3) as u8
    }

    pub fn ch_mode_hp(&self) -> u8 {
        ((self.qp_mode >> 7) & 3) as u8
    }
}

fn read_quantizer_set(reader: &mut XrBitReader, indices: &mut [u8; MAX_CHANNELS], channels: usize) -> u8 {
    if channels >= MAX_CHANNELS {
        return 0;
    }

    let mut ch_mode = 0u8;

    if channels > 1 {
        ch_mode = reader.read(2) as u8;
    }

    indices[0] = reader.read(8) as u8;

    if ch_mode == 1 {
        indices[1] = reader.read(8) as u8;
    } else if ch_mode > 0 {
        for index in indices.iter_mut().take(channels).skip(1) {
            *index = reader.read(8) as u8;
        }
    }

    ch_mode
}

pub fn parse_image_header(reader: &mut XrBitReader) -> VexelResult<ImageHeader> {
    let mut signature = [0u8; 8];
    for byte in signature.iter_mut() {
        *byte = reader.read(8) as u8;
    }

    if &signature != CODESTREAM_MAGIC {
        return Err(VexelError::Custom(format!("JPEG XR: bad codestream signature {:?}", signature)));
    }

    let version = reader.read(4);
    let subversion = reader.read(4);

    if version != CODEC_VERSION {
        log_warn!("JPEG XR: unexpected codec version {}", version);
    }

    if subversion != CODEC_SUBVERSION
        && subversion != CODEC_SUBVERSION_NEWSCALING_SOFT_TILES
        && subversion != CODEC_SUBVERSION_NEWSCALING_HARD_TILES
    {
        log_warn!("JPEG XR: unexpected codec subversion {}", subversion);
    }

    let hard_tile_boundaries = subversion == CODEC_SUBVERSION_NEWSCALING_HARD_TILES;

    let tiling_present = reader.read_bool();
    let bitstream_format = if reader.read_bool() { BitstreamFormat::Frequency } else { BitstreamFormat::Spatial };
    let spatial_transform = reader.read(3);
    let index_table_present = reader.read_bool();

    let overlap_bits = reader.read(2);
    let overlap = Overlap::from_bits(overlap_bits).unwrap_or_else(|| {
        log_warn!("JPEG XR: invalid overlap mode {}, using none", overlap_bits);
        Overlap::None
    });

    let short_header = reader.read_bool();
    let long_word = reader.read_bool();
    let windowing = reader.read_bool();
    let trim_flexbits_flag = reader.read_bool();
    let tile_stretch = reader.read_bool();
    let rb_swapped = reader.read_bool();
    let premultiplied_alpha = reader.read_bool();
    let alpha_channel = reader.read_bool();

    let color_bits = reader.read(4);
    let source_color_format = ColorFormat::from_bits(color_bits).unwrap_or_else(|| {
        log_warn!("JPEG XR: unknown source color format {}, assuming YUV444", color_bits);
        ColorFormat::Yuv444
    });

    let depth_bits = reader.read(4);
    let mut source_bit_depth = BitDepth::from_bits(depth_bits).unwrap_or_else(|| {
        log_warn!("JPEG XR: unknown source bit depth {}, assuming 8", depth_bits);
        BitDepth::Bd8
    });

    let mut black_white = false;
    if source_bit_depth == BitDepth::Bd1Black {
        source_bit_depth = BitDepth::Bd1White;
        black_white = true;
    }

    let size_bits = if short_header { 16 } else { 32 };
    let mut width = reader.read(size_bits).wrapping_add(1);
    let mut height = reader.read(size_bits).wrapping_add(1);

    let mut extra_top = 0;
    let mut extra_left = 0;
    let mut extra_bottom = 0;
    let mut extra_right = 0;

    if !windowing {
        if width & 0xf != 0 {
            extra_right = 0x10 - (width & 0xf);
        }
        if height & 0xf != 0 {
            extra_bottom = 0x10 - (height & 0xf);
        }
    }

    let mut tile_cols = 0usize;
    let mut tile_rows = 0usize;

    if tiling_present {
        tile_cols = reader.read(LOG_MAX_TILES) as usize;
        tile_rows = reader.read(LOG_MAX_TILES) as usize;
    }

    if !index_table_present && (bitstream_format == BitstreamFormat::Frequency || tile_cols + tile_rows > 0) {
        return Err(VexelError::Custom(
            "JPEG XR: index table required for frequency order or tiled images".to_string(),
        ));
    }

    let tile_bits = if short_header { 8 } else { 16 };

    let mut tile_x = vec![0u32; tile_cols + 1];
    for i in 0..tile_cols {
        tile_x[i + 1] = reader.read(tile_bits).wrapping_add(tile_x[i]);
    }

    let mut tile_y = vec![0u32; tile_rows + 1];
    for i in 0..tile_rows {
        tile_y[i + 1] = reader.read(tile_bits).wrapping_add(tile_y[i]);
    }

    if tile_stretch {
        reader.skip(8 * ((tile_cols + 1) * (tile_rows + 1)) as u64);
    }

    if windowing {
        extra_top = reader.read(6);
        extra_left = reader.read(6);
        extra_bottom = reader.read(6);
        extra_right = reader.read(6);
    }

    if (width.wrapping_add(extra_left).wrapping_add(extra_right) & 0xf)
        + (height.wrapping_add(extra_top).wrapping_add(extra_bottom) & 0xf)
        != 0
    {
        if (width & 0xf) + (height & 0xf) + extra_left + extra_top != 0 {
            return Err(VexelError::Custom("JPEG XR: inconsistent windowing parameters".to_string()));
        }
        if width <= extra_right || height <= extra_bottom {
            return Err(VexelError::Custom("JPEG XR: windowing exceeds image size".to_string()));
        }
        width -= extra_right;
        height -= extra_bottom;
    }

    reader.align_byte();

    Ok(ImageHeader {
        version,
        subversion,
        tiling_present,
        spatial_transform,
        short_header,
        long_word,
        windowing,
        tile_stretch,
        premultiplied_alpha,
        source_color_bits: color_bits,
        source_depth_bits: depth_bits,
        hard_tile_boundaries,
        bitstream_format,
        index_table_present,
        overlap,
        trim_flexbits_flag,
        rb_swapped,
        alpha_channel,
        source_color_format,
        source_bit_depth,
        black_white,
        width,
        height,
        tile_cols,
        tile_rows,
        tile_x,
        tile_y,
        extra_top,
        extra_left,
        extra_bottom,
        extra_right,
    })
}

pub fn parse_plane_header(reader: &mut XrBitReader, image: &ImageHeader) -> VexelResult<PlaneHeader> {
    let color_bits = reader.read(3);
    let internal_color_format = ColorFormat::from_bits(color_bits)
        .ok_or_else(|| VexelError::Custom(format!("JPEG XR: unsupported internal color format {}", color_bits)))?;

    if matches!(internal_color_format, ColorFormat::Rgb | ColorFormat::Rgbe) {
        return Err(VexelError::Custom("JPEG XR: internal color format out of range".to_string()));
    }

    let scaled_arith = reader.read_bool();
    let subband = Subband::from_bits(reader.read(4));

    let num_channels = match internal_color_format {
        ColorFormat::YOnly => 1,
        ColorFormat::Yuv420 | ColorFormat::Yuv422 | ColorFormat::Yuv444 => {
            reader.skip(8);
            3
        }
        ColorFormat::NComponent => {
            let channels = reader.read(4) as usize + 1;
            reader.skip(4);
            channels
        }
        ColorFormat::Cmyk => 4,
        _ => 3,
    };

    if num_channels > MAX_CHANNELS {
        return Err(VexelError::Custom(format!("JPEG XR: too many channels ({})", num_channels)));
    }

    let mut mantissa_len = 0u8;
    let mut exp_bias = 0i8;

    match image.source_bit_depth {
        BitDepth::Bd16 | BitDepth::Bd16S | BitDepth::Bd32 | BitDepth::Bd32S => {
            mantissa_len = reader.read(8) as u8;
        }
        BitDepth::Bd32F => {
            mantissa_len = reader.read(8) as u8;
            exp_bias = reader.read(8) as i8;
        }
        _ => {}
    }

    let mut qp_mode = 0u32;
    let mut qp_index_dc = [0u8; MAX_CHANNELS];
    let mut qp_index_lp = [0u8; MAX_CHANNELS];
    let mut qp_index_hp = [0u8; MAX_CHANNELS];

    if reader.read_bool() {
        let ch_mode = read_quantizer_set(reader, &mut qp_index_dc, num_channels);
        qp_mode += u32::from(ch_mode) << 3;
    } else {
        qp_mode += 1;
    }

    if subband != Subband::DcOnly {
        if !reader.read_bool() {
            qp_mode += 0x200;
            if reader.read_bool() {
                let ch_mode = read_quantizer_set(reader, &mut qp_index_lp, num_channels);
                qp_mode += u32::from(ch_mode) << 5;
            } else {
                qp_mode += 2;
            }
        } else {
            qp_mode += ((qp_mode & 1) << 1) + ((qp_mode & 0x18) << 2);
        }

        if subband != Subband::NoHighpass {
            if !reader.read_bool() {
                qp_mode += 0x400;
                if reader.read_bool() {
                    let ch_mode = read_quantizer_set(reader, &mut qp_index_hp, num_channels);
                    qp_mode += u32::from(ch_mode) << 7;
                } else {
                    qp_mode += 4;
                }
            } else {
                qp_mode += ((qp_mode & 2) << 1) + ((qp_mode & 0x60) << 2);
            }
        }
    }

    if subband == Subband::DcOnly {
        qp_mode |= 0x200;
    } else if subband == Subband::NoHighpass {
        qp_mode |= 0x400;
    }

    if qp_mode & 0x600 == 0 {
        return Err(VexelError::Custom("JPEG XR: frame level QPs must be specified independently".to_string()));
    }

    reader.align_byte();

    Ok(PlaneHeader {
        internal_color_format,
        scaled_arith,
        subband,
        num_channels,
        mantissa_len,
        exp_bias,
        qp_mode,
        qp_index_dc,
        qp_index_lp,
        qp_index_hp,
    })
}
