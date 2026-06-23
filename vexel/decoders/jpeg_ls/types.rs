use crate::decoders::jpeg::types::{APP14AdobeData, JFIFData};
use crate::utils::exif::ExifData;
use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsSectionInfo {
    pub start_offset: u64,
    pub data: JpegLsSectionData,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum JpegLsSectionData {
    Soi,
    Eoi,
    Sof(JpegLsSofData),
    Sos(JpegLsSosData),
    Lse(JpegLsLseData),
    Dri(JpegLsDriData),
    Dnl(JpegLsDnlData),
    App(JpegLsAppData),
    Com(JpegLsComData),
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsDnlData {
    pub length: u16,
    pub number_of_lines: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsSofData {
    pub length: u16,
    pub precision: u8,
    pub height: u32,
    pub width: u32,
    pub component_count: u8,
    pub components: Vec<JpegLsComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsComponentInfo {
    pub id: u8,
    pub horizontal_sampling: u8,
    pub vertical_sampling: u8,
    pub reserved: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsSosData {
    pub length: u16,
    pub component_count: u8,
    pub components: Vec<JpegLsSosComponentInfo>,
    pub near: u8,
    pub interleave_mode: u8,
    pub point_transform: u8,
    pub scan_data_length: usize,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsSosComponentInfo {
    pub id: u8,
    pub mapping_table_selector: u8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum JpegLsLseData {
    PresetParameters {
        length: u16,
        maxval: u16,
        t1: u16,
        t2: u16,
        t3: u16,
        reset: u16,
    },
    MappingTable {
        length: u16,
        table_id: u8,
        entry_count: u16,
        entries: Vec<u16>,
    },
    ExtendedTemplate {
        length: u16,
        entries: Vec<u8>,
    },
    Other {
        length: u16,
        id_type: u8,
    },
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsDriData {
    pub length: u16,
    pub restart_interval: u16,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsAppData {
    pub marker: u16,
    pub length: u16,
    pub identifier: Option<String>,
    pub jfif: Option<JFIFData>,
    pub exif: Option<ExifData>,
    pub icc_profile_sequence: Option<crate::decoders::jpeg::types::IccProfileSequenceInfo>,
    pub adobe: Option<APP14AdobeData>,
    pub color_transform: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JpegLsComData {
    pub length: u16,
    pub text: String,
}

pub const MAX_COMPONENTS: usize = 6;
pub const CREGIONS: usize = 9;
pub const CONTEXTS1: usize = CREGIONS * CREGIONS * CREGIONS;
pub const CONTEXTS: usize = (CONTEXTS1 + 1) / 2;
pub const EOR_CONTEXTS: usize = 2;
pub const TOT_CONTEXTS: usize = CONTEXTS + EOR_CONTEXTS;
pub const EOR_0: usize = CONTEXTS;

pub const BASIC_T1: i32 = 3;
pub const BASIC_T2: i32 = 7;
pub const BASIC_T3: i32 = 21;

pub const INITNSTAT: i32 = 1;
pub const MIN_INITABSTAT: i32 = 2;
pub const INITABSLACK: i32 = 6;

pub const DEFAULT_RESET: i32 = 64;

pub const MAX_C: i32 = 127;
pub const MIN_C: i32 = -128;

pub const MELC_STATES: usize = 32;

pub static J_TABLE: [i32; MELC_STATES] = [
    0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

pub const LUTMAX8: usize = 256;
pub const LUTMAX16: usize = 4501;

#[derive(Debug, Clone, PartialEq)]
pub enum InterleaveMode {
    None,
    Line,
    Sample,
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub id: u8,
    pub horizontal_sampling: u8,
    pub vertical_sampling: u8,
}

#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub precision: u8,
    pub height: u32,
    pub width: u32,
    pub components: Vec<ComponentInfo>,
    pub alpha: i32,
}

impl FrameHeader {
    pub fn component_count(&self) -> usize {
        self.components.len()
    }
}

#[derive(Debug, Clone)]
pub struct ScanHeader {
    pub component_ids: Vec<u8>,
    pub near: i32,
    pub interleave_mode: InterleaveMode,
    pub point_transform: u8,
    pub t1: i32,
    pub t2: i32,
    pub t3: i32,
    pub reset: i32,
    pub alpha: i32,
    pub restart_interval: usize,
}

impl Default for ScanHeader {
    fn default() -> Self {
        Self {
            component_ids: Vec::new(),
            near: 0,
            interleave_mode: InterleaveMode::None,
            point_transform: 0,
            t1: 0,
            t2: 0,
            t3: 0,
            reset: DEFAULT_RESET,
            alpha: 0,
            restart_interval: 0,
        }
    }
}

pub struct DecoderState {
    pub n: Vec<i32>,
    pub a: Vec<i32>,
    pub b: Vec<i32>,
    pub c: Vec<i32>,
    pub vlut: Vec<Vec<i32>>,
    pub classmap: Vec<i32>,
    pub melc_state: Vec<usize>,
    pub melc_len: Vec<i32>,
    pub melc_order: Vec<i32>,
    pub limit_reduce: i32,
    pub qmul: Vec<i32>,
    pub qmul_offset: i32,
    pub neg_near: i32,
    pub alpha1eps: i32,
    pub beta: i32,
}

impl DecoderState {
    pub fn new() -> Self {
        Self {
            n: vec![0; TOT_CONTEXTS],
            a: vec![0; TOT_CONTEXTS],
            b: vec![0; TOT_CONTEXTS],
            c: vec![0; TOT_CONTEXTS],
            vlut: vec![vec![0; 2 * LUTMAX16]; 3],
            classmap: vec![0; CONTEXTS1],
            melc_state: vec![0; MAX_COMPONENTS],
            melc_len: vec![0; MAX_COMPONENTS],
            melc_order: vec![0; MAX_COMPONENTS],
            limit_reduce: 0,
            qmul: Vec::new(),
            qmul_offset: 0,
            neg_near: 0,
            alpha1eps: 255,
            beta: 256,
        }
    }

    pub fn qmul(&self, qdiff: i32) -> i32 {
        if self.qmul.is_empty() {
            return 0;
        }
        let max_diff = self.qmul_offset;
        let clamped = qdiff.clamp(-max_diff, max_diff);
        let idx = (clamped + self.qmul_offset) as usize;
        self.qmul[idx]
    }
}

pub fn prepare_qtables(state: &mut DecoderState, alpha: i32, near: i32) {
    let quant = 2 * near + 1;
    let qbeta = (alpha + 2 * near + quant - 1) / quant;
    let beta = quant * qbeta;

    state.neg_near = -near;
    state.alpha1eps = alpha - 1 + near;
    state.beta = beta;

    let size = 2 * beta - 1;
    state.qmul = vec![0i32; size as usize];
    state.qmul_offset = beta - 1;

    for qdiff in -(beta - 1)..beta {
        let diff = quant * qdiff;
        let idx = (qdiff + state.qmul_offset) as usize;
        state.qmul[idx] = diff;
    }
}

pub fn compute_thresholds(alpha: i32, near: i32, t1_in: i32, t2_in: i32, t3_in: i32) -> (i32, i32, i32) {
    let lambda = if alpha < 4096 { (alpha + 127) / 256 } else { (4096 + 127) / 256 };

    let mut t1 = t1_in;
    let mut t2 = t2_in;
    let mut t3 = t3_in;

    if t1 <= 0 {
        t1 = if lambda != 0 {
            lambda * (BASIC_T1 - 2) + 2
        } else {
            let ilambda = 256 / alpha;
            let v = BASIC_T1 / ilambda;
            if v < 2 { 2 } else { v }
        };
        t1 += 3 * near;
        if t1 < near + 1 || t1 > alpha - 1 {
            t1 = near + 1;
        }
    }

    if t2 <= 0 {
        t2 = if lambda != 0 {
            lambda * (BASIC_T2 - 3) + 3
        } else {
            let ilambda = 256 / alpha;
            let v = BASIC_T2 / ilambda;
            if v < 3 { 3 } else { v }
        };
        t2 += 5 * near;
        if t2 < t1 || t2 > alpha - 1 {
            t2 = t1;
        }
    }

    if t3 <= 0 {
        t3 = if lambda != 0 {
            lambda * (BASIC_T3 - 4) + 4
        } else {
            let ilambda = 256 / alpha;
            let v = BASIC_T3 / ilambda;
            if v < 4 { 4 } else { v }
        };
        t3 += 7 * near;
        if t3 < t2 || t3 > alpha - 1 {
            t3 = t2;
        }
    }

    (t1, t2, t3)
}

pub fn init_stats(state: &mut DecoderState, alpha: i32) {
    let slack = 1 << INITABSLACK;
    let initabstat = {
        let v = (alpha + slack / 2) / slack;
        if v < MIN_INITABSTAT { MIN_INITABSTAT } else { v }
    };

    for i in 0..TOT_CONTEXTS {
        state.c[i] = 0;
        state.b[i] = 0;
        state.n[i] = INITNSTAT;
        state.a[i] = initabstat;
    }
}

pub fn prepare_luts(state: &mut DecoderState, t1: i32, t2: i32, t3: i32, near: i32, lutmax: usize) {
    let lmax = lutmax as i32;

    for i in (-(lmax - 1))..lmax {
        let idx = if i <= -t3 {
            7
        } else if i <= -t2 {
            5
        } else if i <= -t1 {
            3
        } else if i <= -(near + 1) {
            1
        } else if i <= near {
            0
        } else if i < t1 {
            2
        } else if i < t2 {
            4
        } else if i < t3 {
            6
        } else {
            8
        };

        let offset = (i + lutmax as i32) as usize;
        state.vlut[0][offset] = (CREGIONS * CREGIONS) as i32 * idx;
        state.vlut[1][offset] = CREGIONS as i32 * idx;
        state.vlut[2][offset] = idx;
    }

    state.classmap[0] = 0;
    let mut j = 0i32;
    for i in 1..CONTEXTS1 {
        if state.classmap[i] != 0 {
            continue;
        }

        let q1 = i / (CREGIONS * CREGIONS);
        let q2 = (i / CREGIONS) % CREGIONS;
        let q3 = i % CREGIONS;

        let sgn = if (q1 % 2 != 0) || (q1 == 0 && q2 % 2 != 0) || (q1 == 0 && q2 == 0 && q3 % 2 != 0) {
            -1i32
        } else {
            1i32
        };

        let n1 = if q1 != 0 { if q1 % 2 != 0 { q1 + 1 } else { q1 - 1 } } else { 0 };
        let n2 = if q2 != 0 { if q2 % 2 != 0 { q2 + 1 } else { q2 - 1 } } else { 0 };
        let n3 = if q3 != 0 { if q3 % 2 != 0 { q3 + 1 } else { q3 - 1 } } else { 0 };

        let ineg = (n1 * CREGIONS + n2) * CREGIONS + n3;
        j += 1;
        state.classmap[i] = sgn * j;
        state.classmap[ineg] = -sgn * j;
    }
}

pub fn init_run_state(state: &mut DecoderState, components: usize) {
    for n_c in 0..components {
        state.melc_state[n_c] = 0;
        state.melc_len[n_c] = J_TABLE[0];
        state.melc_order[n_c] = 1 << J_TABLE[0];
    }
}
