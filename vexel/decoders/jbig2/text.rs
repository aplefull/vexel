use std::sync::Arc;

use crate::decoders::jbig2::arith::{ArithDecoder, IaidCtx, IntCtx};
use crate::decoders::jbig2::bitmap::{Bitmap, ComposeOp, WorkBudget};
use crate::decoders::jbig2::huffman::{HuffmanReader, HuffmanTable, HuffmanValue};
use crate::decoders::jbig2::refinement::{decode_refinement_region, RefinementParams};
use crate::log_warn;

const PLACEMENT_COST: u64 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefCorner {
    BottomLeft,
    TopLeft,
    BottomRight,
    TopRight,
}

impl RefCorner {
    pub fn from_bits(bits: u8) -> RefCorner {
        match bits & 3 {
            0 => RefCorner::BottomLeft,
            1 => RefCorner::TopLeft,
            2 => RefCorner::BottomRight,
            _ => RefCorner::TopRight,
        }
    }

    fn is_bottom(self) -> bool {
        matches!(self, RefCorner::BottomLeft | RefCorner::BottomRight)
    }

    fn is_right(self) -> bool {
        matches!(self, RefCorner::BottomRight | RefCorner::TopRight)
    }
}

pub struct TextRegionHuffman {
    pub fs: HuffmanTable,
    pub ds: HuffmanTable,
    pub dt: HuffmanTable,
    pub rdw: HuffmanTable,
    pub rdh: HuffmanTable,
    pub rdx: HuffmanTable,
    pub rdy: HuffmanTable,
    pub rsize: HuffmanTable,
    pub symcodes: HuffmanTable,
}

pub struct TextRegionParams {
    pub refine: bool,
    pub log_strips: u32,
    pub ref_corner: RefCorner,
    pub transposed: bool,
    pub comb_op: ComposeOp,
    pub def_pixel: bool,
    pub ds_offset: i32,
    pub rtemplate: u8,
    pub num_instances: u32,
    pub rat: [(i8, i8); 2],
}

pub struct TextArithContexts {
    pub iadt: IntCtx,
    pub iafs: IntCtx,
    pub iads: IntCtx,
    pub iait: IntCtx,
    pub iari: IntCtx,
    pub iardw: IntCtx,
    pub iardh: IntCtx,
    pub iardx: IntCtx,
    pub iardy: IntCtx,
    pub iaid: IaidCtx,
}

impl TextArithContexts {
    pub fn new(sym_code_len: u32) -> TextArithContexts {
        TextArithContexts {
            iadt: IntCtx::new(),
            iafs: IntCtx::new(),
            iads: IntCtx::new(),
            iait: IntCtx::new(),
            iari: IntCtx::new(),
            iardw: IntCtx::new(),
            iardh: IntCtx::new(),
            iardx: IntCtx::new(),
            iardy: IntCtx::new(),
            iaid: IaidCtx::new(sym_code_len),
        }
    }
}

pub enum TextCoder<'a, 'b> {
    Arith { arith: &'a mut ArithDecoder<'b>, contexts: &'a mut TextArithContexts },
    Huffman { reader: &'a mut HuffmanReader<'b>, tables: &'a TextRegionHuffman },
}

pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub bitmap: Bitmap,
}

pub fn decode_text_region(
    bitmap: &mut Bitmap,
    symbols: &[Arc<Bitmap>],
    params: &TextRegionParams,
    coder: &mut TextCoder,
    refine_stats: &mut [crate::decoders::jbig2::arith::Cx],
    budget: &mut WorkBudget,
) {
    decode_text_region_tracked(bitmap, symbols, params, coder, refine_stats, None, budget);
}

pub fn decode_text_region_tracked(
    bitmap: &mut Bitmap,
    symbols: &[Arc<Bitmap>],
    params: &TextRegionParams,
    coder: &mut TextCoder,
    refine_stats: &mut [crate::decoders::jbig2::arith::Cx],
    mut placements: Option<&mut Vec<Placement>>,
    budget: &mut WorkBudget,
) {
    bitmap.fill(params.def_pixel);

    let strips = 1i32 << params.log_strips;

    let mut strip_t = match read_dt(coder) {
        Some(value) => value.wrapping_mul(-strips),
        None => return,
    };

    let mut first_s = 0i32;
    let mut instances = 0u32;

    while instances < params.num_instances {
        let Some(dt) = read_dt(coder) else {
            return;
        };
        strip_t = strip_t.wrapping_add(dt.wrapping_mul(strips));

        let Some(dfs) = read_dfs(coder) else {
            return;
        };
        first_s = first_s.wrapping_add(dfs);
        let mut cur_s = first_s;
        let mut first_symbol = true;

        loop {
            if !first_symbol {
                match read_ids(coder) {
                    Some(Some(ids)) => {
                        cur_s = cur_s.wrapping_add(ids).wrapping_add(params.ds_offset);
                    }
                    Some(None) => break,
                    None => return,
                }
            }
            first_symbol = false;

            if instances >= params.num_instances {
                break;
            }

            if let TextCoder::Arith { arith, .. } = coder {
                if arith.exhausted() {
                    log_warn!("JBIG2 text region data exhausted after {} instances", instances);
                    return;
                }
            }

            if !budget.charge(if placements.is_some() { PLACEMENT_COST } else { 1 }) {
                log_warn!("JBIG2 text region exceeds the decode work budget after {} instances", instances);
                return;
            }

            let cur_t = if strips == 1 {
                0
            } else {
                match read_curt(coder, params.log_strips) {
                    Some(value) => value,
                    None => return,
                }
            };
            let t = strip_t.wrapping_add(cur_t);

            let Some(id) = read_id(coder) else {
                return;
            };

            let symbol = symbols.get(id as usize).map(|symbol| symbol.as_ref());
            if symbol.is_none() {
                log_warn!("JBIG2 text region references out of range symbol ID {}", id);
            }

            let refined = if params.refine {
                match read_ri(coder) {
                    Some(ri) => ri != 0,
                    None => return,
                }
            } else {
                false
            };

            let mut refined_bitmap = None;
            if refined {
                let Some((rdw, rdh, rdx, rdy, bmsize)) = read_refinement_deltas(coder) else {
                    return;
                };

                if let Some(reference) = symbol {
                    let new_width = (reference.width as i32).saturating_add(rdw);
                    let new_height = (reference.height as i32).saturating_add(rdh);
                    if new_width > 0 && new_height > 0 && !budget.charge_area(new_width as u32, new_height as u32) {
                        log_warn!("JBIG2 text region refinement exceeds the decode work budget");
                        return;
                    }
                    if new_width > 0 && new_height > 0 {
                        let mut target = Bitmap::new(new_width as u32, new_height as u32);
                        let refine_params = RefinementParams {
                            template: params.rtemplate,
                            reference,
                            dx: (rdw >> 1).wrapping_add(rdx),
                            dy: (rdh >> 1).wrapping_add(rdy),
                            tpgron: false,
                            at: params.rat,
                        };

                        match coder {
                            TextCoder::Arith { arith, .. } => {
                                decode_refinement_region(arith, refine_stats, &mut target, &refine_params);
                            }
                            TextCoder::Huffman { reader, .. } => {
                                reader.align();
                                let start = reader.byte_offset();
                                let slice = &reader.data()[start.min(reader.data().len())..];
                                let mut arith = ArithDecoder::new(slice);
                                decode_refinement_region(&mut arith, refine_stats, &mut target, &refine_params);
                                reader.advance_bytes(bmsize as usize);
                            }
                        }
                        refined_bitmap = Some(target);
                    }
                } else if let TextCoder::Huffman { reader, .. } = coder {
                    reader.advance_bytes(bmsize as usize);
                }
            }

            let instance = refined_bitmap.as_ref().or(symbol);
            let (sym_width, sym_height) = match instance {
                Some(bitmap) => (bitmap.width as i32, bitmap.height as i32),
                None => (0, 0),
            };

            if !params.transposed && params.ref_corner.is_right() {
                cur_s = cur_s.wrapping_add(sym_width.saturating_sub(1));
            } else if params.transposed && params.ref_corner.is_bottom() {
                cur_s = cur_s.wrapping_add(sym_height.saturating_sub(1));
            }

            let s = cur_s;
            let (x, y) = if !params.transposed {
                let x = if params.ref_corner.is_right() { s.wrapping_sub(sym_width).wrapping_add(1) } else { s };
                let y = if params.ref_corner.is_bottom() { t.wrapping_sub(sym_height).wrapping_add(1) } else { t };
                (x, y)
            } else {
                let x = if params.ref_corner.is_right() { t.wrapping_sub(sym_width).wrapping_add(1) } else { t };
                let y = if params.ref_corner.is_bottom() { s.wrapping_sub(sym_height).wrapping_add(1) } else { s };
                (x, y)
            };

            if let Some(instance) = instance {
                let copies = if placements.is_some() { 2 } else { 1 };
                let area = u64::from(instance.width).saturating_mul(u64::from(instance.height));
                if !budget.charge(area.saturating_mul(copies)) {
                    log_warn!("JBIG2 text region exceeds the decode work budget after {} instances", instances);
                    return;
                }
                if let Some(list) = placements.as_deref_mut() {
                    list.push(Placement {
                        x,
                        y,
                        width: instance.width,
                        height: instance.height,
                        bitmap: instance.clone(),
                    });
                }
                bitmap.compose(instance, x as i64, y as i64, params.comb_op);
            }

            if !params.transposed && !params.ref_corner.is_right() {
                cur_s = cur_s.wrapping_add(sym_width.saturating_sub(1));
            } else if params.transposed && !params.ref_corner.is_bottom() {
                cur_s = cur_s.wrapping_add(sym_height.saturating_sub(1));
            }

            instances += 1;
        }
    }
}

fn read_dt(coder: &mut TextCoder) -> Option<i32> {
    match coder {
        TextCoder::Arith { arith, contexts } => contexts.iadt.decode(arith),
        TextCoder::Huffman { reader, tables, .. } => match tables.dt.decode(reader)? {
            HuffmanValue::Value(value) => Some(value),
            HuffmanValue::Oob => None,
        },
    }
}

fn read_dfs(coder: &mut TextCoder) -> Option<i32> {
    match coder {
        TextCoder::Arith { arith, contexts } => contexts.iafs.decode(arith),
        TextCoder::Huffman { reader, tables, .. } => match tables.fs.decode(reader)? {
            HuffmanValue::Value(value) => Some(value),
            HuffmanValue::Oob => None,
        },
    }
}

fn read_ids(coder: &mut TextCoder) -> Option<Option<i32>> {
    match coder {
        TextCoder::Arith { arith, contexts } => match contexts.iads.decode(arith) {
            Some(value) => Some(Some(value)),
            None => Some(None),
        },
        TextCoder::Huffman { reader, tables, .. } => match tables.ds.decode(reader)? {
            HuffmanValue::Value(value) => Some(Some(value)),
            HuffmanValue::Oob => Some(None),
        },
    }
}

fn read_curt(coder: &mut TextCoder, log_strips: u32) -> Option<i32> {
    match coder {
        TextCoder::Arith { arith, contexts } => contexts.iait.decode(arith),
        TextCoder::Huffman { reader, .. } => reader.read_bits(log_strips).map(|value| value as i32),
    }
}

fn read_id(coder: &mut TextCoder) -> Option<u32> {
    match coder {
        TextCoder::Arith { arith, contexts } => Some(contexts.iaid.decode(arith)),
        TextCoder::Huffman { reader, tables, .. } => match tables.symcodes.decode(reader)? {
            HuffmanValue::Value(value) => Some(value.max(0) as u32),
            HuffmanValue::Oob => None,
        },
    }
}

fn read_ri(coder: &mut TextCoder) -> Option<i32> {
    match coder {
        TextCoder::Arith { arith, contexts } => contexts.iari.decode(arith),
        TextCoder::Huffman { reader, .. } => reader.read_bits(1).map(|value| value as i32),
    }
}

fn read_refinement_deltas(coder: &mut TextCoder) -> Option<(i32, i32, i32, i32, i32)> {
    match coder {
        TextCoder::Arith { arith, contexts } => {
            let rdw = contexts.iardw.decode(arith)?;
            let rdh = contexts.iardh.decode(arith)?;
            let rdx = contexts.iardx.decode(arith)?;
            let rdy = contexts.iardy.decode(arith)?;
            Some((rdw, rdh, rdx, rdy, 0))
        }
        TextCoder::Huffman { reader, tables, .. } => {
            let rdw = expect_value(tables.rdw.decode(reader)?)?;
            let rdh = expect_value(tables.rdh.decode(reader)?)?;
            let rdx = expect_value(tables.rdx.decode(reader)?)?;
            let rdy = expect_value(tables.rdy.decode(reader)?)?;
            let bmsize = expect_value(tables.rsize.decode(reader)?)?;
            reader.align();
            Some((rdw, rdh, rdx, rdy, bmsize))
        }
    }
}

fn expect_value(value: HuffmanValue) -> Option<i32> {
    match value {
        HuffmanValue::Value(value) => Some(value),
        HuffmanValue::Oob => None,
    }
}

pub fn decode_symbol_id_huffman_table(reader: &mut HuffmanReader, num_syms: u32) -> Option<HuffmanTable> {
    let mut runcode_lengths = [0u32; 35];
    for length in runcode_lengths.iter_mut() {
        *length = reader.read_bits(4)?;
    }
    let runcode_table = crate::decoders::jbig2::huffman::build_runcode_table(&runcode_lengths);

    let mut lengths: Vec<u32> = Vec::with_capacity(num_syms as usize);

    while (lengths.len() as u32) < num_syms {
        let code = match runcode_table.decode(reader)? {
            HuffmanValue::Value(value) => value,
            HuffmanValue::Oob => return None,
        };

        if !(0..35).contains(&code) {
            return None;
        }

        let (length, range) = match code {
            0..=31 => (code as u32, 1u32),
            32 => {
                let previous = *lengths.last()?;
                (previous, reader.read_bits(2)? + 3)
            }
            33 => (0, reader.read_bits(3)? + 3),
            _ => (0, reader.read_bits(7)? + 11),
        };

        for _ in 0..range {
            if lengths.len() as u32 >= num_syms {
                break;
            }
            lengths.push(length);
        }
    }

    reader.align();

    let lines = lengths
        .iter()
        .enumerate()
        .map(|(index, &pref_len)| crate::decoders::jbig2::huffman::HuffmanLine {
            pref_len,
            range_len: 0,
            range_low: index as i32,
            is_lower: false,
            is_oob: false,
        })
        .collect();

    Some(HuffmanTable::new(lines))
}
