use std::io::{Read, Seek, SeekFrom};

use super::bitreader::XsBitReader;
use super::ids::Ids;
use super::info::{scan, JpegXsInfo};
use super::markers::{parse_header, parse_slice_header};
use super::precinct::Precinct;
use super::simd;
use super::transform::{dwt_inverse, mct_inverse_rct, mct_inverse_tetrix, nlt_inverse};
use super::types::*;
use super::unpack::{sign_magnitude_to_value, Unpacker};
use crate::bitreader::BitReader;
use crate::utils::error::{VexelError, VexelResult};
use crate::utils::image::PixelData;
use crate::{log_warn, Image, Limits};

pub struct JpegXsDecoder<R: Read + Seek> {
    reader: BitReader<R>,
    limits: Limits,
    config: Config,
    image: ImageParams,
    info: Option<JpegXsInfo>,
    truncated: bool,
}

impl<R: Read + Seek> JpegXsDecoder<R> {
    pub fn new(reader: R) -> Self {
        JpegXsDecoder {
            reader: BitReader::new(reader),
            limits: Limits::default(),
            config: Config::default(),
            image: ImageParams::default(),
            info: None,
            truncated: false,
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    pub fn get_info(&mut self) -> JpegXsInfo {
        if self.info.is_none() {
            let saved_pos = self.reader.stream_position().unwrap_or(0);
            let data = self.reader.seek(SeekFrom::Start(0)).and_then(|_| self.reader.read_to_end());
            let _ = self.reader.seek(SeekFrom::Start(saved_pos));
            match data {
                Ok(data) => self.info = Some(scan(&data)),
                Err(e) => log_warn!("Failed to read JPEG XS data: {}", e),
            }
        }

        self.info.clone().unwrap_or_default()
    }

    fn read_all(&mut self) -> VexelResult<Vec<u8>> {
        let available = self.reader.bytes_left().unwrap_or(0);
        let to_read = usize::try_from(available).unwrap_or(usize::MAX);

        self.limits.reserve_usize(to_read)?;

        Ok(self.reader.read_bytes(to_read)?)
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        let data = self.read_all()?;

        if data.is_empty() {
            return Err(VexelError::Custom("empty JPEG XS codestream".to_string()));
        }

        let mut reader = XsBitReader::new(&data);

        let header = parse_header(&mut reader);

        self.config = header.config;
        self.image = header.image;

        if !header.saw_pih {
            return Err(VexelError::Custom("JPEG XS codestream has no picture header".to_string()));
        }

        if self.image.width == 0 || self.image.height == 0 {
            return Err(VexelError::InvalidDimensions { width: self.image.width, height: self.image.height });
        }

        if self.image.ncomps == 0 {
            return Err(VexelError::Custom("JPEG XS codestream declares no components".to_string()));
        }

        if !header.saw_cdt {
            log_warn!("JPEG XS: no component table, assuming {}-bit non-subsampled components", self.image.depth);
        }

        self.limits.check_dimensions(self.image.width, self.image.height)?;

        self.ensure_weights();

        let Some(ids) = Ids::construct(
            &self.image,
            i32::from(self.config.nlx),
            i32::from(self.config.nly),
            i32::from(self.config.sd),
            i32::from(self.config.cw),
            self.config.lh,
        ) else {
            return Err(VexelError::Custom("JPEG XS codestream has an unusable decomposition structure".to_string()));
        };

        let planes = self.decode_planes(&mut reader, &ids)?;

        self.build_image(planes, &ids)
    }

    fn ensure_weights(&mut self) {
        if self.config.lvl_gains.is_empty() {
            log_warn!("JPEG XS: no weights table, using zero gains and priorities");
        }

        if self.config.lvl_gains.len() < MAX_NBANDS {
            self.config.lvl_gains.resize(MAX_NBANDS, 0);
        }

        if self.config.lvl_priorities.len() < MAX_NBANDS {
            self.config.lvl_priorities.resize(MAX_NBANDS, 0);
        }

        if self.config.n_g == 0 {
            log_warn!("JPEG XS: invalid group size 0, using 4");
            self.config.n_g = 4;
        }

        if self.config.s_s == 0 {
            log_warn!("JPEG XS: invalid significance group size 0, using 8");
            self.config.s_s = 8;
        }

        if self.config.slice_height == 0 {
            log_warn!("JPEG XS: invalid slice height 0, using precinct height");
            self.config.slice_height = 1 << self.config.nly.min(16);
        }
    }

    fn decode_planes(&mut self, reader: &mut XsBitReader, ids: &Ids) -> VexelResult<Vec<Vec<i32>>> {
        let mut total = 0u64;
        for comp in 0..ids.ncomps {
            let w = ids.comp_w[comp].max(0) as u64;
            let h = ids.comp_h[comp].max(0) as u64;
            total = total.saturating_add(w.saturating_mul(h));
        }

        self.limits.reserve_usize(usize::try_from(total.saturating_mul(4)).unwrap_or(usize::MAX))?;

        let mut planes: Vec<Vec<i32>> = Vec::with_capacity(ids.ncomps);
        for comp in 0..ids.ncomps {
            let len = (ids.comp_w[comp].max(0) as usize).saturating_mul(ids.comp_h[comp].max(0) as usize);
            planes.push(vec![0i32; len]);
        }

        let group_size = self.config.n_g.max(1) as usize;

        let mut current: Vec<Precinct> = (0..ids.npx).map(|c| Precinct::new(ids, group_size, c)).collect();
        let mut previous: Vec<Precinct> = (0..ids.npx).map(|c| Precinct::new(ids, group_size, c)).collect();
        let mut gtli_top: Vec<Vec<i32>> = vec![vec![0i32; ids.nbands]; ids.npx];
        let mut has_previous = vec![false; ids.npx];

        let mut unpacker = Unpacker::new(&self.config, ids);

        let slice_height = self.config.slice_height.max(1) as i32;
        let mut line_idx = 0i32;

        while line_idx < ids.h {
            let prec_y_idx = (line_idx / ids.ph.max(1)) as usize;

            if prec_y_idx >= ids.npy {
                break;
            }

            for column in 0..ids.npx {
                let first_of_slice = (prec_y_idx as i32 * ids.ph) % slice_height == 0;

                if first_of_slice && column == 0 && parse_slice_header(reader).is_none() && !reader.exhausted() {
                    log_warn!("JPEG XS: expected slice header at line {}", line_idx);
                }

                if reader.exhausted() {
                    self.truncated = true;
                    log_warn!("JPEG XS: codestream ended at line {}, keeping decoded portion", line_idx);
                    self.finish_planes(&mut planes, ids);
                    return Ok(planes);
                }

                current[column].y_idx = prec_y_idx;

                let top = if first_of_slice || !has_previous[column] { None } else { Some(&previous[column]) };

                let result =
                    unpacker.unpack_precinct(reader, ids, &mut current[column], top, &gtli_top[column], &self.config);

                if result.truncated {
                    self.truncated = true;
                }

                self.precinct_to_planes(&current[column], &mut planes, ids, column, prec_y_idx);

                gtli_top[column] = result.gtli_gcli;
                has_previous[column] = true;

                std::mem::swap(&mut previous[column], &mut current[column]);
            }

            line_idx += ids.ph.max(1);
        }

        if self.truncated {
            log_warn!("JPEG XS: codestream was truncated or corrupted, output may be incomplete");
        }

        self.finish_planes(&mut planes, ids);

        Ok(planes)
    }

    fn finish_planes(&mut self, planes: &mut [Vec<i32>], ids: &Ids) {
        dwt_inverse(ids, planes);

        match self.config.color_transform {
            ColorTransform::Rct => {
                mct_inverse_rct(planes, ids.comp_w[0].max(0) as usize, ids.comp_h[0].max(0) as usize);
            }
            ColorTransform::Tetrix => {
                mct_inverse_tetrix(
                    planes,
                    ids.comp_w[0].max(0) as usize,
                    ids.comp_h[0].max(0) as usize,
                    self.config.cfa_pattern,
                    self.config.tetrix,
                );
            }
            ColorTransform::None => {}
        }

        nlt_inverse(planes, &self.config, &self.image, ids);
    }

    fn precinct_to_planes(
        &self,
        precinct: &Precinct,
        planes: &mut [Vec<i32>],
        ids: &Ids,
        column: usize,
        prec_y_idx: usize,
    ) {
        let fq = self.config.fq.min(24) as u32;
        let is_last_y = prec_y_idx + 1 >= ids.npy;

        for band in 0..ids.nbands {
            let height = ids.band_height_in_precinct(band, is_last_y);

            for ypos in 0..height.max(0) as usize {
                let Some(idx) = precinct.index_of(band, ypos) else {
                    continue;
                };

                let Some(src) = precinct.data.get(idx) else {
                    continue;
                };

                let Some((comp, start, x_inc, len)) =
                    self.line_placement(ids, precinct, band, ypos, column, prec_y_idx)
                else {
                    continue;
                };

                let Some(plane) = planes.get_mut(comp) else {
                    continue;
                };

                let count = len.min(src.len()).min(plane.len().saturating_sub(start).div_ceil(x_inc));

                if x_inc == 1 {
                    simd::sign_magnitude_row(&src[..count], &mut plane[start..start + count], fq);
                    continue;
                }

                for i in 0..count {
                    plane[start + i * x_inc] = sign_magnitude_to_value(src[i]).wrapping_shl(fq);
                }
            }
        }
    }

    fn line_placement(
        &self,
        ids: &Ids,
        precinct: &Precinct,
        band: usize,
        ypos: usize,
        column: usize,
        prec_y_idx: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        let comp = *ids.band_to_comp.get(band)?;
        let filter = *ids.band_to_filter.get(band)?;

        let comp_w = ids.comp_w.get(comp).copied()? as usize;
        let comp_h = ids.comp_h.get(comp).copied()? as usize;

        if comp_w == 0 || comp_h == 0 {
            return None;
        }

        let dx = ids.band_d[comp][filter].x;
        let dy = ids.band_d[comp][filter].y;

        if dx < 0 || dy < 0 {
            return None;
        }

        let mut offset = 0usize;

        if ids.band_is_high[filter].y != 0 {
            offset = offset.checked_add((1usize << (dy - 1).clamp(0, 30)).checked_mul(comp_w)?)?;
        }

        if ids.band_is_high[filter].x != 0 {
            offset = offset.checked_add(1usize << (dx - 1).clamp(0, 30))?;
        }

        let sy = self.image.sy.get(comp).copied().unwrap_or(1).max(1) as usize;
        let sx = self.image.sx.get(comp).copied().unwrap_or(1).max(1) as usize;

        let lines_per_precinct = (ids.ph.max(0) as usize) >> (sy - 1);
        offset = offset.checked_add(comp_w.checked_mul(lines_per_precinct)?.checked_mul(prec_y_idx)?)?;

        offset = offset.checked_add(comp_w.checked_mul(ypos)?.checked_mul(1usize << dy.clamp(0, 30))?)?;

        offset = offset.checked_add(((ids.pw[0].max(0) as usize) >> (sx - 1)).checked_mul(column)?)?;

        if offset >= comp_w * comp_h {
            return None;
        }

        let x_inc = 1usize << dx.clamp(0, 30);
        let len = ids.band_width_in_precinct(band, precinct.is_last_column).max(0) as usize;

        Some((comp, offset, x_inc, len))
    }

    fn build_image(&mut self, planes: Vec<Vec<i32>>, ids: &Ids) -> VexelResult<Image> {
        let width = self.image.width;
        let height = self.image.height;
        let depth = self.image.depth.clamp(1, 16);

        let subsampled = (0..self.image.ncomps).any(|c| self.image.sx[c] != 1 || self.image.sy[c] != 1);

        let is_bayer = self.image.ncomps == 4 && self.config.color_transform == ColorTransform::Tetrix;

        let (width, height) =
            if is_bayer { (width.saturating_mul(2), height.saturating_mul(2)) } else { (width, height) };

        let channels = match self.image.ncomps {
            3 => 3,
            4 if !is_bayer => 4,
            _ => 1,
        };
        let sample_bytes = if depth <= 8 { 1 } else { 2 };
        self.limits.reserve_buffer(width, height, channels * sample_bytes)?;

        let pixels = match self.image.ncomps {
            1 => self.build_gray(&planes, ids, depth),
            3 => self.build_rgb(&planes, ids, depth, subsampled),
            4 => {
                if is_bayer {
                    self.build_bayer(&planes, ids, depth)
                } else {
                    self.build_rgba(&planes, ids, depth, subsampled)
                }
            }
            2 => self.build_gray(&planes, ids, depth),
            other => {
                log_warn!("JPEG XS: wrong component count {}, using first component", other);
                self.build_gray(&planes, ids, depth)
            }
        };

        Ok(Image::from_pixels(width, height, pixels))
    }

    fn sample_at(&self, planes: &[Vec<i32>], ids: &Ids, comp: usize, x: usize, y: usize) -> i32 {
        let Some(plane) = planes.get(comp) else {
            return 0;
        };

        let sx = self.image.sx.get(comp).copied().unwrap_or(1).max(1) as usize;
        let sy = self.image.sy.get(comp).copied().unwrap_or(1).max(1) as usize;

        let comp_w = ids.comp_w.get(comp).copied().unwrap_or(0).max(0) as usize;
        let comp_h = ids.comp_h.get(comp).copied().unwrap_or(0).max(0) as usize;

        if comp_w == 0 || comp_h == 0 {
            return 0;
        }

        let cx = (x / sx).min(comp_w - 1);
        let cy = (y / sy).min(comp_h - 1);

        plane.get(cy * comp_w + cx).copied().unwrap_or(0)
    }

    fn direct_plane<'p>(&self, planes: &'p [Vec<i32>], ids: &Ids, comp: usize) -> Option<&'p [i32]> {
        let width = self.image.width as usize;
        let height = self.image.height as usize;

        if self.image.sx.get(comp).copied().unwrap_or(1) != 1 || self.image.sy.get(comp).copied().unwrap_or(1) != 1 {
            return None;
        }

        if ids.comp_w.get(comp).copied().unwrap_or(0).max(0) as usize != width {
            return None;
        }

        let plane = planes.get(comp)?;

        if plane.len() < width * height || ids.comp_h.get(comp).copied().unwrap_or(0).max(0) as usize != height {
            return None;
        }

        Some(&plane[..width * height])
    }

    fn build_gray(&self, planes: &[Vec<i32>], ids: &Ids, depth: u8) -> PixelData {
        let width = self.image.width as usize;
        let height = self.image.height as usize;
        let max_val = ((1u32 << depth) - 1) as i32;

        let direct = self.direct_plane(planes, ids, 0);

        if depth <= 8 {
            let mut out = vec![0u8; width * height];

            if let Some(src) = direct {
                simd::pack_clamped_u8(src, &mut out, max_val);
                return PixelData::L8(out);
            }

            for y in 0..height {
                for x in 0..width {
                    let v = self.sample_at(planes, ids, 0, x, y).clamp(0, max_val);
                    out[y * width + x] = v as u8;
                }
            }
            PixelData::L8(out)
        } else {
            let mut out = vec![0u16; width * height];

            if let Some(src) = direct {
                simd::pack_clamped_u16(src, &mut out, max_val);
                return PixelData::L16(out);
            }

            for y in 0..height {
                for x in 0..width {
                    let v = self.sample_at(planes, ids, 0, x, y).clamp(0, max_val);
                    out[y * width + x] = v as u16;
                }
            }
            PixelData::L16(out)
        }
    }

    fn build_bayer(&self, planes: &[Vec<i32>], ids: &Ids, depth: u8) -> PixelData {
        let width = (self.image.width as usize).saturating_mul(2);
        let height = (self.image.height as usize).saturating_mul(2);
        let max_val = ((1u32 << depth) - 1) as i32;

        let comp_w = ids.comp_w.first().copied().unwrap_or(0).max(0) as usize;
        let comp_h = ids.comp_h.first().copied().unwrap_or(0).max(0) as usize;

        let order: [usize; 4] = match self.config.cfa_pattern {
            CfaPattern::Rggb | CfaPattern::Bggr => [0, 1, 2, 3],
            CfaPattern::Grbg | CfaPattern::Gbrg => [1, 0, 3, 2],
        };

        let fetch = |comp: usize, x: usize, y: usize| -> i32 {
            planes
                .get(comp)
                .and_then(|p| p.get(y.min(comp_h.saturating_sub(1)) * comp_w + x.min(comp_w.saturating_sub(1))))
                .copied()
                .unwrap_or(0)
                .clamp(0, max_val)
        };

        if depth <= 8 {
            let mut out = vec![0u8; width * height];
            for y in 0..comp_h {
                for x in 0..comp_w {
                    let quad = [
                        (2 * x, 2 * y, order[0]),
                        (2 * x + 1, 2 * y, order[1]),
                        (2 * x, 2 * y + 1, order[2]),
                        (2 * x + 1, 2 * y + 1, order[3]),
                    ];
                    for (px, py, comp) in quad {
                        if px < width && py < height {
                            out[py * width + px] = fetch(comp, x, y) as u8;
                        }
                    }
                }
            }
            PixelData::L8(out)
        } else {
            let mut out = vec![0u16; width * height];
            for y in 0..comp_h {
                for x in 0..comp_w {
                    let quad = [
                        (2 * x, 2 * y, order[0]),
                        (2 * x + 1, 2 * y, order[1]),
                        (2 * x, 2 * y + 1, order[2]),
                        (2 * x + 1, 2 * y + 1, order[3]),
                    ];
                    for (px, py, comp) in quad {
                        if px < width && py < height {
                            out[py * width + px] = fetch(comp, x, y) as u16;
                        }
                    }
                }
            }
            PixelData::L16(out)
        }
    }

    fn build_rgb(&self, planes: &[Vec<i32>], ids: &Ids, depth: u8, subsampled: bool) -> PixelData {
        let width = self.image.width as usize;
        let height = self.image.height as usize;
        let max_val = ((1u32 << depth) - 1) as i32;

        let convert = subsampled;

        let direct = if convert {
            None
        } else {
            match (
                self.direct_plane(planes, ids, 0),
                self.direct_plane(planes, ids, 1),
                self.direct_plane(planes, ids, 2),
            ) {
                (Some(r), Some(g), Some(b)) => Some((r, g, b)),
                _ => None,
            }
        };

        if depth <= 8 {
            let mut out = vec![0u8; width * height * 3];

            if let Some((r, g, b)) = direct {
                simd::interleave_rgb_u8(r, g, b, &mut out, max_val);
                return PixelData::RGB8(out);
            }

            for y in 0..height {
                for x in 0..width {
                    let (r, g, b) = self.pixel_rgb(planes, ids, x, y, convert, depth, max_val);
                    let i = (y * width + x) * 3;
                    out[i] = r as u8;
                    out[i + 1] = g as u8;
                    out[i + 2] = b as u8;
                }
            }
            PixelData::RGB8(out)
        } else {
            let mut out = vec![0u16; width * height * 3];

            if let Some((r, g, b)) = direct {
                simd::interleave_rgb_u16(r, g, b, &mut out, max_val);
                return PixelData::RGB16(out);
            }

            for y in 0..height {
                for x in 0..width {
                    let (r, g, b) = self.pixel_rgb(planes, ids, x, y, convert, depth, max_val);
                    let i = (y * width + x) * 3;
                    out[i] = r as u16;
                    out[i + 1] = g as u16;
                    out[i + 2] = b as u16;
                }
            }
            PixelData::RGB16(out)
        }
    }

    fn pixel_rgb(
        &self,
        planes: &[Vec<i32>],
        ids: &Ids,
        x: usize,
        y: usize,
        convert: bool,
        depth: u8,
        max_val: i32,
    ) -> (i32, i32, i32) {
        let c0 = self.sample_at(planes, ids, 0, x, y);
        let c1 = self.sample_at(planes, ids, 1, x, y);
        let c2 = self.sample_at(planes, ids, 2, x, y);

        if !convert {
            return (c0.clamp(0, max_val), c1.clamp(0, max_val), c2.clamp(0, max_val));
        }

        let half = 1i32 << (depth - 1);
        let yv = c0 as f32;
        let cb = (c1 - half) as f32;
        let cr = (c2 - half) as f32;

        let r = yv + 1.402 * cr;
        let g = yv - 0.344136 * cb - 0.714136 * cr;
        let b = yv + 1.772 * cb;

        (
            (r.round() as i32).clamp(0, max_val),
            (g.round() as i32).clamp(0, max_val),
            (b.round() as i32).clamp(0, max_val),
        )
    }

    fn build_rgba(&self, planes: &[Vec<i32>], ids: &Ids, depth: u8, subsampled: bool) -> PixelData {
        let width = self.image.width as usize;
        let height = self.image.height as usize;
        let max_val = ((1u32 << depth) - 1) as i32;

        if depth <= 8 {
            let mut out = vec![0u8; width * height * 4];
            for y in 0..height {
                for x in 0..width {
                    let (r, g, b) = self.pixel_rgb(planes, ids, x, y, subsampled, depth, max_val);
                    let a = self.sample_at(planes, ids, 3, x, y).clamp(0, max_val);
                    let i = (y * width + x) * 4;
                    out[i] = r as u8;
                    out[i + 1] = g as u8;
                    out[i + 2] = b as u8;
                    out[i + 3] = a as u8;
                }
            }
            PixelData::RGBA8(out)
        } else {
            let mut out = vec![0u16; width * height * 4];
            for y in 0..height {
                for x in 0..width {
                    let (r, g, b) = self.pixel_rgb(planes, ids, x, y, subsampled, depth, max_val);
                    let a = self.sample_at(planes, ids, 3, x, y).clamp(0, max_val);
                    let i = (y * width + x) * 4;
                    out[i] = r as u16;
                    out[i + 1] = g as u16;
                    out[i + 2] = b as u16;
                    out[i + 3] = a as u16;
                }
            }
            PixelData::RGBA16(out)
        }
    }
}
