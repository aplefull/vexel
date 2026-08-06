use super::ids::Ids;
use super::simd;
use super::types::MAX_GCLI;

pub const SIGN_BIT_MASK: u32 = 1 << 31;

pub struct Precinct {
    pub data: Vec<Vec<u32>>,
    pub gclis: Vec<Vec<i8>>,
    pub idx_from_band_line: Vec<i32>,
    pub band_count: usize,
    pub precinct_height: usize,
    pub y_idx: usize,
    pub is_last_column: bool,
    group_size: usize,
}

impl Precinct {
    pub fn new(ids: &Ids, group_size: usize, column: usize) -> Precinct {
        let is_last_column = column + 1 == ids.npx;
        let group_size = group_size.max(1);

        let precinct_height = ids.pi.iter().map(|p| (p.y - ids.l0[p.band]) as usize + 1).max().unwrap_or(1);

        let mut idx_from_band_line = vec![-1i32; precinct_height * ids.nbands];
        let mut data = Vec::with_capacity(ids.pi.len());
        let mut gclis = Vec::with_capacity(ids.pi.len());

        for (idx, packet) in ids.pi.iter().enumerate() {
            let band = packet.band;
            let y = (packet.y - ids.l0[band]).max(0) as usize;

            if y < precinct_height && band < ids.nbands {
                idx_from_band_line[y * ids.nbands + band] = idx as i32;
            }

            let width = ids.band_width_in_precinct(band, is_last_column).max(0) as usize;
            let n_cg = width.div_ceil(group_size);
            let coefficients = n_cg.saturating_mul(group_size);

            data.push(vec![0u32; coefficients]);
            gclis.push(vec![0i8; n_cg]);
        }

        Precinct {
            data,
            gclis,
            idx_from_band_line,
            band_count: ids.nbands,
            precinct_height,
            y_idx: 0,
            is_last_column,
            group_size,
        }
    }

    pub fn index_of(&self, band: usize, ypos: usize) -> Option<usize> {
        if band >= self.band_count || ypos >= self.precinct_height {
            return None;
        }

        match self.idx_from_band_line.get(ypos * self.band_count + band) {
            Some(&idx) if idx >= 0 => Some(idx as usize),
            _ => None,
        }
    }

    pub fn line_width(&self, band: usize) -> usize {
        self.index_of(band, 0).and_then(|idx| self.data.get(idx)).map(|line| line.len()).unwrap_or(0)
    }

    pub fn gcli_width(&self, band: usize) -> usize {
        self.index_of(band, 0).and_then(|idx| self.gclis.get(idx)).map(|line| line.len()).unwrap_or(0)
    }

    pub fn group_size(&self) -> usize {
        self.group_size
    }

    pub fn clear(&mut self) {
        for line in self.data.iter_mut() {
            line.fill(0);
        }
        for line in self.gclis.iter_mut() {
            line.fill(0);
        }
    }

    pub fn dequantize(&mut self, gtli_table: &[i32], dq_type: u8) {
        for band in 0..self.band_count {
            for ypos in 0..self.precinct_height {
                let Some(idx) = self.index_of(band, ypos) else {
                    continue;
                };

                let gtli = gtli_table.get(band).copied().unwrap_or(0);

                if gtli <= 0 {
                    continue;
                }

                let group_size = self.group_size;

                let (Some(data), Some(gclis)) = (self.data.get_mut(idx), self.gclis.get(idx)) else {
                    continue;
                };

                if dq_type != 1 {
                    let mut group = 0usize;

                    while group < gclis.len() {
                        if i32::from(gclis[group]) <= gtli {
                            group += 1;
                            continue;
                        }

                        let run_start = group;
                        while group < gclis.len() && i32::from(gclis[group]) > gtli {
                            group += 1;
                        }

                        let start = run_start * group_size;
                        let end = (group * group_size).min(data.len());

                        if start < end {
                            simd::deadzone_dequant(&mut data[start..end], gtli);
                        }
                    }

                    continue;
                }

                for (group, &gcli) in gclis.iter().enumerate() {
                    if i32::from(gcli) <= gtli {
                        continue;
                    }

                    let start = group * group_size;
                    let end = (start + group_size).min(data.len());

                    for value in &mut data[start..end] {
                        *value = dequant_value(dq_type, *value, i32::from(gcli), gtli);
                    }
                }
            }
        }
    }
}

fn dequant_value(dq_type: u8, value: u32, gcli: i32, gtli: i32) -> u32 {
    match dq_type {
        1 => uniform_dq_inverse(value, gcli, gtli),
        _ => deadzone_dq_inverse(value, gtli),
    }
}

fn deadzone_dq_inverse(value: u32, gtli: i32) -> u32 {
    if gtli > 0 && (value & !SIGN_BIT_MASK) != 0 {
        value | (1u32 << (gtli - 1).clamp(0, 31))
    } else {
        value
    }
}

fn uniform_dq_inverse(value: u32, gcli: i32, gtli: i32) -> u32 {
    let sign = value & SIGN_BIT_MASK;
    let mut phi = value & !SIGN_BIT_MASK;
    let zeta = (gcli - gtli + 1).clamp(1, 31) as u32;

    let mut rho: u32 = 0;
    while phi > 0 {
        rho = rho.wrapping_add(phi);
        phi >>= zeta;
    }

    sign | (rho & !SIGN_BIT_MASK)
}

pub fn compute_gtli(scenario: i32, gain: i32, add_1bp: bool) -> i32 {
    let mut gtli = scenario - gain;
    if add_1bp {
        gtli -= 1;
    }
    gtli.clamp(0, MAX_GCLI)
}

pub fn compute_gtli_tables(
    quantization: i32,
    refinement: i32,
    n_bands: usize,
    gains: &[u8],
    priorities: &[u8],
    gtli_data: &mut Vec<i32>,
    gtli_gcli: &mut Vec<i32>,
) {
    gtli_data.clear();
    gtli_gcli.clear();

    for band in 0..n_bands {
        let gain = gains.get(band).copied().unwrap_or(0) as i32;
        let priority = priorities.get(band).copied().unwrap_or(0) as i32;
        let gtli = compute_gtli(quantization, gain, priority < refinement);

        gtli_data.push(gtli);
        gtli_gcli.push(gtli);
    }
}
