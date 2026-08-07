use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::headers::{ImageHeader, PlaneHeader};
use crate::decoders::jpeg_xr::types::*;
use crate::utils::error::{VexelError, VexelResult};

pub const MAX_QP_SETS: usize = 16;

#[derive(Debug, Clone)]
pub struct TileQuantizers {
    pub dc: [Quantizer; MAX_CHANNELS],
    pub lp: [[Quantizer; MAX_CHANNELS]; MAX_QP_SETS],
    pub hp: [[Quantizer; MAX_CHANNELS]; MAX_QP_SETS],
    pub num_qp_lp: u8,
    pub num_qp_hp: u8,
    pub bits_lp: u8,
    pub bits_hp: u8,
    pub use_dc: bool,
    pub use_lp: bool,
}

impl Default for TileQuantizers {
    fn default() -> Self {
        TileQuantizers {
            dc: [Quantizer::default(); MAX_CHANNELS],
            lp: [[Quantizer::default(); MAX_CHANNELS]; MAX_QP_SETS],
            hp: [[Quantizer::default(); MAX_CHANNELS]; MAX_QP_SETS],
            num_qp_lp: 1,
            num_qp_hp: 1,
            bits_lp: 0,
            bits_hp: 0,
            use_dc: false,
            use_lp: false,
        }
    }
}

pub fn dquant_bits(num_qp: u8) -> u8 {
    if num_qp < 2 {
        0
    } else if num_qp < 4 {
        1
    } else if num_qp < 6 {
        2
    } else if num_qp < 10 {
        3
    } else {
        4
    }
}

fn read_quantizer(reader: &mut XrBitReader, indices: &mut [Quantizer; MAX_CHANNELS], channels: usize) -> u8 {
    let mut ch_mode = 0u8;

    if channels > 1 {
        ch_mode = reader.read(2) as u8;
    }

    indices[0].index = reader.read(8) as u8;

    if ch_mode == 1 {
        if channels > 1 {
            indices[1].index = reader.read(8) as u8;
        }
    } else if ch_mode > 0 {
        for quantizer in indices.iter_mut().take(channels.min(MAX_CHANNELS)).skip(1) {
            quantizer.index = reader.read(8) as u8;
        }
    }

    ch_mode
}

fn format_quantizer(
    set: &mut [Quantizer; MAX_CHANNELS],
    ch_mode: u8,
    channels: usize,
    shifted_uv: bool,
    scaled_arith: bool,
) {
    for ch in 0..channels.min(MAX_CHANNELS) {
        if ch > 0 {
            if ch_mode == 0 {
                set[ch] = set[0];
            } else if ch_mode == 1 {
                set[ch] = set[1];
            }
        }

        let shift = if ch > 0 && shifted_uv { SHIFTZERO - 1 } else { SHIFTZERO };
        set[ch] = Quantizer::from_index(set[ch].index, shift, scaled_arith);
    }
}

pub fn read_tile_header_dc(tile: &mut TileQuantizers, reader: &mut XrBitReader, plane: &PlaneHeader) {
    if plane.dc_uniform() {
        return;
    }

    let ch_mode = read_quantizer(reader, &mut tile.dc, plane.num_channels);
    format_quantizer(&mut tile.dc, ch_mode, plane.num_channels, true, plane.scaled_arith);
}

pub fn read_tile_header_lp(
    tile: &mut TileQuantizers,
    reader: &mut XrBitReader,
    plane: &PlaneHeader,
) -> VexelResult<()> {
    if plane.subband == Subband::DcOnly || plane.lp_uniform() {
        return Ok(());
    }

    tile.use_dc = reader.read_bool();
    tile.bits_lp = 0;
    tile.num_qp_lp = 1;

    if tile.use_dc {
        tile.lp[0] = tile.dc;
        return Ok(());
    }

    tile.num_qp_lp = reader.read(4) as u8 + 1;
    tile.bits_lp = dquant_bits(tile.num_qp_lp);

    if usize::from(tile.num_qp_lp) > MAX_QP_SETS {
        return Err(VexelError::Custom(format!("JPEG XR: too many LP quantizer sets ({})", tile.num_qp_lp)));
    }

    for i in 0..usize::from(tile.num_qp_lp) {
        let ch_mode = read_quantizer(reader, &mut tile.lp[i], plane.num_channels);
        format_quantizer(&mut tile.lp[i], ch_mode, plane.num_channels, true, plane.scaled_arith);
    }

    Ok(())
}

pub fn read_tile_header_hp(
    tile: &mut TileQuantizers,
    reader: &mut XrBitReader,
    plane: &PlaneHeader,
) -> VexelResult<()> {
    if plane.subband == Subband::DcOnly || plane.subband == Subband::NoHighpass || plane.hp_uniform() {
        return Ok(());
    }

    tile.use_lp = reader.read_bool();
    tile.bits_hp = 0;
    tile.num_qp_hp = 1;

    if tile.use_lp {
        tile.num_qp_hp = tile.num_qp_lp;
        for i in 0..usize::from(tile.num_qp_hp).min(MAX_QP_SETS) {
            tile.hp[i] = tile.lp[i];
        }
        return Ok(());
    }

    tile.num_qp_hp = reader.read(4) as u8 + 1;
    tile.bits_hp = dquant_bits(tile.num_qp_hp);

    if usize::from(tile.num_qp_hp) > MAX_QP_SETS {
        return Err(VexelError::Custom(format!("JPEG XR: too many HP quantizer sets ({})", tile.num_qp_hp)));
    }

    for i in 0..usize::from(tile.num_qp_hp) {
        let ch_mode = read_quantizer(reader, &mut tile.hp[i], plane.num_channels);
        format_quantizer(&mut tile.hp[i], ch_mode, plane.num_channels, false, plane.scaled_arith);
    }

    Ok(())
}

fn load_indices(set: &mut [Quantizer; MAX_CHANNELS], indices: &[u8; MAX_CHANNELS], channels: usize) {
    for ch in 0..channels.min(MAX_CHANNELS) {
        set[ch].index = indices[ch];
    }
}

pub fn init_uniform_quantizers(plane: &PlaneHeader) -> TileQuantizers {
    let mut tile = TileQuantizers::default();
    let channels = plane.num_channels;

    if plane.dc_uniform() {
        load_indices(&mut tile.dc, &plane.qp_index_dc, channels);
        format_quantizer(&mut tile.dc, plane.ch_mode_dc(), channels, true, plane.scaled_arith);
    }

    if plane.subband == Subband::DcOnly {
        return tile;
    }

    if plane.lp_uniform() {
        if plane.uses_dc_qp() {
            tile.lp[0] = tile.dc;
        } else {
            load_indices(&mut tile.lp[0], &plane.qp_index_lp, channels);
            format_quantizer(&mut tile.lp[0], plane.ch_mode_lp(), channels, true, plane.scaled_arith);
        }
    }

    if plane.subband == Subband::NoHighpass {
        return tile;
    }

    if plane.hp_uniform() {
        if plane.uses_lp_qp() {
            tile.hp[0] = tile.lp[0];
        } else {
            load_indices(&mut tile.hp[0], &plane.qp_index_hp, channels);
            format_quantizer(&mut tile.hp[0], plane.ch_mode_hp(), channels, false, plane.scaled_arith);
        }
    }

    tile
}

#[derive(Debug, Clone)]
pub struct TileLayout {
    pub mb_width: usize,
    pub mb_height: usize,
    pub col_starts: Vec<usize>,
    pub row_starts: Vec<usize>,
}

impl TileLayout {
    pub fn new(image: &ImageHeader) -> Self {
        let mb_width = ((image.width + image.extra_left + image.extra_right) as usize).div_ceil(MB_SIZE);
        let mb_height = ((image.height + image.extra_top + image.extra_bottom) as usize).div_ceil(MB_SIZE);

        let mut col_starts: Vec<usize> = image.tile_x.iter().map(|&v| v as usize).collect();
        let mut row_starts: Vec<usize> = image.tile_y.iter().map(|&v| v as usize).collect();

        col_starts.retain(|&v| v <= mb_width);
        row_starts.retain(|&v| v <= mb_height);

        if col_starts.is_empty() || col_starts[0] != 0 {
            col_starts.insert(0, 0);
        }
        if row_starts.is_empty() || row_starts[0] != 0 {
            row_starts.insert(0, 0);
        }

        col_starts.push(mb_width);
        row_starts.push(mb_height);

        TileLayout { mb_width, mb_height, col_starts, row_starts }
    }

    pub fn cols(&self) -> usize {
        self.col_starts.len() - 1
    }

    pub fn rows(&self) -> usize {
        self.row_starts.len() - 1
    }
}
