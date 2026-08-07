use std::io::{Read, Seek};

use crate::decoders::jpeg_xr::bitreader::XrBitReader;
use crate::decoders::jpeg_xr::coeffs::{decode_coefficients, DecodeConfig, PlaneCoefficients, COEFFS_PER_PLANE};
use crate::decoders::jpeg_xr::container::{self, Container};
use crate::decoders::jpeg_xr::headers::{parse_image_header, parse_plane_header, ImageHeader, PlaneHeader};
use crate::decoders::jpeg_xr::pixels::{build_image_data, OutputParams};
use crate::decoders::jpeg_xr::tile::TileLayout;
use crate::decoders::jpeg_xr::transform::{
    inverse_transform_macroblock, CornerPrediction, MacroblockCursor, MacroblockPosition, Plane, TransformParams,
};
use crate::decoders::jpeg_xr::types::*;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::{Image, PixelData};
use crate::utils::info::JpegXrInfo;
use crate::utils::limits::Limits;

pub struct JpegXrDecoder<R: Read + Seek> {
    reader: R,
    limits: Limits,
    info: Option<JpegXrInfo>,
}

impl<R: Read + Seek> JpegXrDecoder<R> {
    pub fn new(reader: R) -> Self {
        JpegXrDecoder { reader, limits: Limits::default(), info: None }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    fn read_all(&mut self) -> VexelResult<Vec<u8>> {
        let mut data = Vec::new();
        self.reader.rewind()?;
        self.reader.read_to_end(&mut data)?;
        Ok(data)
    }

    pub fn get_info(&mut self) -> JpegXrInfo {
        if self.info.is_none() {
            let _ = self.decode();
        }

        self.info.clone().unwrap_or_else(empty_info)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let data = self.read_all()?;
        let (container, codestream_offset) = locate_codestream(&data)?;

        let primary = decode_plane_group(&data, codestream_offset, &self.limits, None)?;

        let alpha = container.as_ref().and_then(|c| {
            if c.alpha_byte_count == 0 || c.alpha_offset == 0 {
                return None;
            }
            decode_plane_group(&data, c.alpha_offset, &self.limits, None).ok()
        });

        let image_header = &primary.image;
        let plane_header = &primary.plane;

        let pixel_format = container.as_ref().and_then(|c| c.pixel_format).unwrap_or(PixelFormatKind::Unknown);

        let has_alpha = alpha.is_some() || primary.alpha_planes.is_some();

        let orientation =
            container.as_ref().and_then(|c| c.transformation).map(Orientation::from_bits).unwrap_or(Orientation::None);

        let params = OutputParams {
            width: image_header.width as usize,
            height: image_header.height as usize,
            source_color_format: image_header.source_color_format,
            internal_color_format: plane_header.internal_color_format,
            bit_depth: image_header.source_bit_depth,
            channels: plane_header.num_channels,
            scaled_arith: plane_header.scaled_arith,
            mantissa_len: plane_header.mantissa_len,
            exp_bias: plane_header.exp_bias,
            rb_swapped: image_header.rb_swapped,
            black_white: image_header.black_white,
            has_alpha,
            window_top: image_header.extra_top as usize,
            window_left: image_header.extra_left as usize,
        };

        let alpha_planes = primary.alpha_planes.as_deref().or(alpha.as_ref().map(|group| group.planes.as_slice()));

        let pixels = build_image_data(&primary.planes, alpha_planes, &params, primary.mb_width, primary.mb_height);
        let pixels = apply_orientation(pixels, params.width, params.height, orientation);

        let (width, height) = if orientation.rotates() {
            (image_header.height, image_header.width)
        } else {
            (image_header.width, image_header.height)
        };

        self.info = Some(JpegXrInfo {
            width,
            height,
            bitstream_format: format!("{:?}", image_header.bitstream_format),
            source_color_format: format!("{:?}", image_header.source_color_format),
            internal_color_format: format!("{:?}", plane_header.internal_color_format),
            source_bit_depth: format!("{:?}", image_header.source_bit_depth),
            pixel_format: format!("{:?}", pixel_format),
            num_channels: plane_header.num_channels as u32,
            overlap: format!("{:?}", image_header.overlap),
            subband: format!("{:?}", plane_header.subband),
            orientation: format!("{:?}", orientation),
            tile_columns: image_header.tile_cols as u32 + 1,
            tile_rows: image_header.tile_rows as u32 + 1,
            alpha_channel: has_alpha,
            hard_tile_boundaries: image_header.hard_tile_boundaries,
            scaled_arithmetic: plane_header.scaled_arith,
            trim_flexbits: image_header.trim_flexbits_flag,
        });

        Ok(Image::from_pixels(width, height, pixels))
    }
}

struct PlaneGroup {
    image: ImageHeader,
    plane: PlaneHeader,
    planes: Vec<Vec<i32>>,
    alpha_planes: Option<Vec<Vec<i32>>>,
    mb_width: usize,
    mb_height: usize,
}

fn decode_plane_group(data: &[u8], offset: usize, limits: &Limits, _reserved: Option<()>) -> VexelResult<PlaneGroup> {
    let stream =
        data.get(offset..).ok_or_else(|| VexelError::Custom("JPEG XR: codestream offset out of range".to_string()))?;

    let mut reader = XrBitReader::new(stream);
    let image = parse_image_header(&mut reader)?;
    let plane = parse_plane_header(&mut reader, &image)?;

    let alpha_plane = if image.alpha_channel { Some(parse_plane_header(&mut reader, &image)?) } else { None };

    limits.check_dimensions(image.width, image.height)?;

    let layout = TileLayout::new(&image);
    let header_end = reader.byte_pos();

    let (coeffs, alpha_coeffs) = decode_coefficients(
        stream,
        header_end,
        &image,
        &plane,
        alpha_plane.as_ref(),
        &layout,
        &DecodeConfig::default(),
    )?;

    let planes = reconstruct_planes(&coeffs, &image, &plane, &layout);

    let alpha_planes = match (alpha_coeffs, alpha_plane.as_ref()) {
        (Some(coeffs), Some(header)) => Some(reconstruct_planes(&coeffs, &image, header, &layout)),
        _ => None,
    };

    Ok(PlaneGroup { image, plane, planes, alpha_planes, mb_width: layout.mb_width, mb_height: layout.mb_height })
}

fn reconstruct_planes(
    coeffs: &PlaneCoefficients,
    image: &ImageHeader,
    plane: &PlaneHeader,
    layout: &TileLayout,
) -> Vec<Vec<i32>> {
    let mb_width = layout.mb_width;
    let mb_height = layout.mb_height;
    let channels = coeffs.planes.len();

    let params = TransformParams {
        overlap: image.overlap,
        color_format: plane.internal_color_format,
        channels: plane.num_channels,
        scaled_arith: plane.scaled_arith,
        altered_operators: image.subversion != CODEC_SUBVERSION,
        hp_absent: !plane.subband.has_highpass(),
    };

    let strides: Vec<usize> =
        (0..channels).map(|channel| macroblock_stride(plane.internal_color_format, channel)).collect();

    let mut output: Vec<Vec<i32>> = strides.iter().map(|&stride| vec![0i32; mb_height * mb_width * stride]).collect();

    let mut planes: Vec<Plane> = strides
        .iter()
        .map(|&stride| {
            let row_len = (mb_width + 1) * stride;
            Plane { data: vec![0i32; row_len * 2], stride, row_len }
        })
        .collect();

    let mut corner = CornerPrediction::default();
    let mut boundaries = TileBoundaries::default();

    let mut previous_row = 0usize;
    let mut current_row = 1usize;

    for mb_y in 0..=mb_height {
        for (channel, target) in planes.iter_mut().enumerate() {
            let source = &coeffs.planes[channel];
            let stride = target.stride;
            let current_base = current_row * target.row_len;

            for mb_x in 0..=mb_width {
                let dst = current_base + mb_x * stride;
                let Some(to) = target.data.get_mut(dst..dst + stride) else { continue };

                if mb_y < mb_height && mb_x < mb_width {
                    let src = coeffs.macroblock_offset(mb_x, mb_y);
                    match source.get(src..src + stride) {
                        Some(from) => to.copy_from_slice(from),
                        None => to.fill(0),
                    }
                } else {
                    to.fill(0);
                }
            }
        }

        for mb_x in 0..=mb_width {
            let mut position = MacroblockPosition {
                left: mb_x == 0,
                right: mb_x == mb_width,
                top: mb_y == 0,
                bottom: mb_y == mb_height,
                left_adjacent: mb_x == 1,
                right_adjacent: mb_width > 0 && mb_x + 1 == mb_width,
                ..Default::default()
            };

            if image.hard_tile_boundaries {
                boundaries.update(&mut position, layout, mb_x, mb_y);
            }

            let cursor = MacroblockCursor { previous_row, current_row, mb_x };

            let last_y = mb_y.min(mb_height.saturating_sub(1));
            let last_x = mb_x.min(mb_width.saturating_sub(1));
            let hp_qp = coeffs.hp_qp.get(last_y * mb_width + last_x).copied().unwrap_or([255i32; MAX_CHANNELS]);

            inverse_transform_macroblock(&mut planes, cursor, &position, &params, &hp_qp, &mut corner);
        }

        if mb_y > 0 {
            for (channel, plane_data) in planes.iter().enumerate() {
                let row_len = mb_width * plane_data.stride;
                let source_base = previous_row * plane_data.row_len;
                let target_base = (mb_y - 1) * row_len;

                let Some(from) = plane_data.data.get(source_base..source_base + row_len) else {
                    continue;
                };
                if let Some(to) = output[channel].get_mut(target_base..target_base + row_len) {
                    to.copy_from_slice(from);
                }
            }
        }

        std::mem::swap(&mut previous_row, &mut current_row);
    }

    output
}

#[derive(Default)]
struct TileBoundaries {
    tile_col: usize,
    tile_row: usize,
    vertical: bool,
    horizontal: bool,
    previous_mb_y: Option<usize>,
}

impl TileBoundaries {
    fn update(&mut self, position: &mut MacroblockPosition, layout: &TileLayout, mb_x: usize, mb_y: usize) {
        let last_col = layout.cols() - 1;
        let last_row = layout.rows() - 1;

        if mb_x == 0 {
            self.vertical = false;
            self.tile_col = 0;
        }

        let mut one_mb_left = false;
        let mut one_mb_right = false;

        if self.tile_col > 0
            && self.tile_col <= last_col
            && mb_x.checked_sub(1) == Some(layout.col_starts[self.tile_col])
        {
            one_mb_right = true;
        }

        if self.tile_col < last_col && mb_x == layout.col_starts[self.tile_col + 1] {
            self.vertical = true;
            self.tile_col += 1;
        } else {
            self.vertical = false;
        }

        if self.tile_col < last_col && mb_x + 1 == layout.col_starts[self.tile_col + 1] {
            one_mb_left = true;
        }

        if mb_y == 0 {
            self.horizontal = false;
            self.tile_row = 0;
        } else if self.previous_mb_y != Some(mb_y) {
            if self.tile_row < last_row && mb_y == layout.row_starts[self.tile_row + 1] {
                self.horizontal = true;
                self.tile_row += 1;
            } else {
                self.horizontal = false;
            }
        }

        self.previous_mb_y = Some(mb_y);

        position.vertical_tile_boundary = self.vertical;
        position.horizontal_tile_boundary = self.horizontal;
        position.one_mb_left_vertical_tb = one_mb_left;
        position.one_mb_right_vertical_tb = one_mb_right;
    }
}

fn macroblock_stride(color_format: ColorFormat, channel: usize) -> usize {
    if channel == 0 {
        return COEFFS_PER_PLANE;
    }

    match color_format {
        ColorFormat::Yuv420 => 64,
        ColorFormat::Yuv422 => 128,
        _ => COEFFS_PER_PLANE,
    }
}

fn apply_orientation(pixels: PixelData, width: usize, height: usize, orientation: Orientation) -> PixelData {
    if orientation == Orientation::None {
        return pixels;
    }

    macro_rules! reorient {
        ($data:expr, $channels:expr, $variant:path) => {{
            let data = $data;
            let channels = $channels;
            let (out_width, out_height) = if orientation.rotates() { (height, width) } else { (width, height) };

            let mut out = vec![Default::default(); out_width * out_height * channels];

            for y in 0..height {
                for x in 0..width {
                    let (mut tx, mut ty) = if orientation.rotates() { (height - 1 - y, x) } else { (x, y) };

                    if orientation.flips_h() {
                        tx = out_width - 1 - tx;
                    }
                    if orientation.flips_v() {
                        ty = out_height - 1 - ty;
                    }

                    let src = (y * width + x) * channels;
                    let dst = (ty * out_width + tx) * channels;

                    for c in 0..channels {
                        if let (Some(&value), Some(slot)) = (data.get(src + c), out.get_mut(dst + c)) {
                            *slot = value;
                        }
                    }
                }
            }

            $variant(out)
        }};
    }

    match pixels {
        PixelData::L1(data) => reorient!(data, 1, PixelData::L1),
        PixelData::L8(data) => reorient!(data, 1, PixelData::L8),
        PixelData::L16(data) => reorient!(data, 1, PixelData::L16),
        PixelData::L32F(data) => reorient!(data, 1, PixelData::L32F),
        PixelData::RGB8(data) => reorient!(data, 3, PixelData::RGB8),
        PixelData::RGBA8(data) => reorient!(data, 4, PixelData::RGBA8),
        PixelData::RGB16(data) => reorient!(data, 3, PixelData::RGB16),
        PixelData::RGBA16(data) => reorient!(data, 4, PixelData::RGBA16),
        PixelData::RGB32F(data) => reorient!(data, 3, PixelData::RGB32F),
        PixelData::RGBA32F(data) => reorient!(data, 4, PixelData::RGBA32F),
        other => other,
    }
}

fn locate_codestream(data: &[u8]) -> VexelResult<(Option<Container>, usize)> {
    if container::looks_like_codestream(data) {
        return Ok((None, 0));
    }

    if container::looks_like_container(data) {
        if let Some(parsed) = container::parse_container(data) {
            let offset = parsed.image_offset;
            return Ok((Some(parsed), offset));
        }
    }

    if let Some(parsed) = container::parse_jpx_boxes(data) {
        let offset = parsed.image_offset;
        return Ok((Some(parsed), offset));
    }

    if let Some(offset) = container::find_codestream(data) {
        return Ok((None, offset));
    }

    Err(VexelError::Custom("JPEG XR: no codestream found".to_string()))
}

fn empty_info() -> JpegXrInfo {
    JpegXrInfo {
        width: 0,
        height: 0,
        bitstream_format: String::new(),
        source_color_format: String::new(),
        internal_color_format: String::new(),
        source_bit_depth: String::new(),
        pixel_format: String::new(),
        num_channels: 0,
        overlap: String::new(),
        subband: String::new(),
        orientation: String::new(),
        tile_columns: 0,
        tile_rows: 0,
        alpha_channel: false,
        hard_tile_boundaries: false,
        scaled_arithmetic: false,
        trim_flexbits: false,
    }
}
