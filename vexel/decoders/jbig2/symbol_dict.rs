use std::sync::Arc;

use crate::decoders::jbig2::arith::{ArithDecoder, Cx, IntCtx};
use crate::decoders::jbig2::bitmap::{Bitmap, ComposeOp, WorkBudget};
use crate::decoders::jbig2::generic::{decode_generic_region, GenericParams};
use crate::decoders::jbig2::huffman::{HuffmanReader, HuffmanTable, HuffmanValue};
use crate::decoders::jbig2::refinement::{decode_refinement_region, RefinementParams};
use crate::decoders::jbig2::text::{
    decode_text_region, RefCorner, TextArithContexts, TextCoder, TextRegionHuffman, TextRegionParams,
};
use crate::log_warn;
use crate::utils::tiff::fax;

const MAX_EMPTY_HEIGHT_CLASSES: u32 = 64;
const SYMBOL_COST: u64 = 512;

pub struct SymbolDictHuffman {
    pub dh: HuffmanTable,
    pub dw: HuffmanTable,
    pub bmsize: HuffmanTable,
    pub agginst: HuffmanTable,
}

pub struct SymbolDictParams {
    pub huffman: bool,
    pub refagg: bool,
    pub template: u8,
    pub rtemplate: u8,
    pub at: [(i8, i8); 12],
    pub rat: [(i8, i8); 2],
    pub num_ex_syms: u32,
    pub num_new_syms: u32,
}

pub fn decode_symbol_dict(
    data: &[u8],
    params: &SymbolDictParams,
    input_symbols: &[Arc<Bitmap>],
    huffman_tables: Option<&SymbolDictHuffman>,
    gb_stats: &mut [Cx],
    gr_stats: &mut [Cx],
    budget: &mut WorkBudget,
) -> Vec<Arc<Bitmap>> {
    let mut new_symbols: Vec<Arc<Bitmap>> = Vec::new();
    let mut refagg_symbols: Vec<Arc<Bitmap>> = if params.refagg { input_symbols.to_vec() } else { Vec::new() };
    let mut empty_height_classes = 0u32;

    let total_symbols = input_symbols.len() as u64 + params.num_new_syms as u64;
    let mut sym_code_len = 0u32;
    while (1u64 << sym_code_len) < total_symbols {
        sym_code_len += 1;
    }
    if params.huffman {
        sym_code_len = sym_code_len.max(1);
    }

    let mut reader = HuffmanReader::new(data);
    let mut arith = ArithDecoder::new(data);

    let mut iadh = IntCtx::new();
    let mut iadw = IntCtx::new();
    let mut iaex = IntCtx::new();
    let mut iaai = IntCtx::new();
    let mut text_contexts = TextArithContexts::new(sym_code_len);

    let refagg_huffman = if params.huffman {
        Some(TextRegionHuffman {
            fs: crate::decoders::jbig2::huffman::table_b6(),
            ds: crate::decoders::jbig2::huffman::table_b8(),
            dt: crate::decoders::jbig2::huffman::table_b11(),
            rdw: crate::decoders::jbig2::huffman::table_b15(),
            rdh: crate::decoders::jbig2::huffman::table_b15(),
            rdx: crate::decoders::jbig2::huffman::table_b15(),
            rdy: crate::decoders::jbig2::huffman::table_b15(),
            rsize: crate::decoders::jbig2::huffman::table_b1(),
            symcodes: crate::decoders::jbig2::huffman::HuffmanTable::new(Vec::new()),
        })
    } else {
        None
    };

    let mut height_class_height = 0i32;
    let mut symbol_widths: Vec<u32> = Vec::new();

    'height_classes: while (new_symbols.len() as u32) < params.num_new_syms {
        if !params.huffman && arith.exhausted() {
            log_warn!("JBIG2 symbol dictionary data exhausted after {} symbols", new_symbols.len());
            break;
        }

        let delta_height = if params.huffman {
            match huffman_tables.and_then(|tables| tables.dh.decode(&mut reader)) {
                Some(HuffmanValue::Value(value)) => value,
                _ => break,
            }
        } else {
            match iadh.decode(&mut arith) {
                Some(value) => value,
                None => break,
            }
        };

        height_class_height = height_class_height.wrapping_add(delta_height);
        if height_class_height < 0 {
            log_warn!("JBIG2 symbol dictionary has negative height class, stopping");
            break;
        }

        let mut symbol_width = 0i32;
        let mut total_width = 0i32;
        let height_class_first = new_symbols.len();

        loop {
            let delta_width = if params.huffman {
                match huffman_tables.and_then(|tables| tables.dw.decode(&mut reader)) {
                    Some(HuffmanValue::Value(value)) => value,
                    Some(HuffmanValue::Oob) => break,
                    None => break,
                }
            } else {
                match iadw.decode(&mut arith) {
                    Some(value) => value,
                    None => break,
                }
            };

            if new_symbols.len() as u32 >= params.num_new_syms {
                break;
            }

            if !params.huffman && arith.exhausted() {
                log_warn!("JBIG2 symbol dictionary data exhausted after {} symbols", new_symbols.len());
                break 'height_classes;
            }

            if !budget.charge(SYMBOL_COST) {
                log_warn!("JBIG2 symbol dictionary exceeds the decode work budget, stopping");
                break 'height_classes;
            }

            symbol_width = symbol_width.wrapping_add(delta_width);
            if symbol_width <= 0 || height_class_height == 0 {
                log_warn!("JBIG2 symbol dictionary produced empty symbol, substituting 1x1");
                let symbol = Arc::new(Bitmap::new(1, 1));
                if params.refagg {
                    refagg_symbols.push(symbol.clone());
                }
                new_symbols.push(symbol);
                symbol_widths.push(1);
                continue;
            }

            if !budget.charge_area(symbol_width as u32, height_class_height as u32) {
                log_warn!("JBIG2 symbol dictionary exceeds the decode work budget, stopping");
                break 'height_classes;
            }
            total_width = total_width.saturating_add(symbol_width);

            if params.huffman && !params.refagg {
                symbol_widths.push(symbol_width as u32);
                new_symbols.push(Arc::new(Bitmap::new(symbol_width as u32, height_class_height as u32)));
                continue;
            }

            let bitmap = if params.refagg {
                let num_instances = if params.huffman {
                    match huffman_tables.and_then(|tables| tables.agginst.decode(&mut reader)) {
                        Some(HuffmanValue::Value(value)) => value,
                        _ => break,
                    }
                } else {
                    match iaai.decode(&mut arith) {
                        Some(value) => value,
                        None => break,
                    }
                };

                if num_instances == 1 {
                    decode_single_refinement(
                        &mut arith,
                        &mut reader,
                        params,
                        &refagg_symbols,
                        sym_code_len,
                        symbol_width as u32,
                        height_class_height as u32,
                        &mut text_contexts,
                        gr_stats,
                        refagg_huffman.as_ref(),
                    )
                } else {
                    let text_params = TextRegionParams {
                        refine: true,
                        log_strips: 0,
                        ref_corner: RefCorner::TopLeft,
                        transposed: false,
                        comb_op: ComposeOp::Or,
                        def_pixel: false,
                        ds_offset: 0,
                        rtemplate: params.rtemplate,
                        num_instances: num_instances.max(0) as u32,
                        rat: params.rat,
                    };

                    let mut target = Bitmap::new(symbol_width as u32, height_class_height as u32);
                    let mut coder = if params.huffman {
                        let Some(tables) = refagg_huffman.as_ref() else {
                            break;
                        };
                        TextCoder::Huffman { reader: &mut reader, tables }
                    } else {
                        TextCoder::Arith { arith: &mut arith, contexts: &mut text_contexts }
                    };
                    decode_text_region(&mut target, &refagg_symbols, &text_params, &mut coder, gr_stats, budget);
                    target
                }
            } else {
                let generic_params = GenericParams {
                    template: params.template,
                    tpgdon: false,
                    ext_template: false,
                    at: params.at,
                    skip: None,
                };
                let mut target = Bitmap::new(symbol_width as u32, height_class_height as u32);
                decode_generic_region(&mut arith, gb_stats, &mut target, &generic_params);
                target
            };

            symbol_widths.push(bitmap.width);
            let symbol = Arc::new(bitmap);
            if params.refagg {
                refagg_symbols.push(symbol.clone());
            }
            new_symbols.push(symbol);
        }

        if new_symbols.len() == height_class_first {
            empty_height_classes += 1;
            if empty_height_classes > MAX_EMPTY_HEIGHT_CLASSES {
                log_warn!("JBIG2 symbol dictionary has too many empty height classes, stopping");
                break;
            }
        } else {
            empty_height_classes = 0;
        }

        if params.huffman && !params.refagg {
            let collective = decode_collective_bitmap(
                &mut reader,
                huffman_tables,
                total_width.max(0) as u32,
                height_class_height as u32,
                budget,
            );

            if let Some(collective) = collective {
                let mut offset = 0i64;
                for slot in new_symbols.iter_mut().skip(height_class_first) {
                    let width = slot.width;
                    let mut symbol = Bitmap::new(width, height_class_height as u32);
                    symbol.compose(&collective, -offset, 0, ComposeOp::Replace);
                    offset += width as i64;
                    *slot = Arc::new(symbol);
                }
            }
        }
    }

    export_symbols(params, input_symbols, &new_symbols, &mut arith, &mut reader, &mut iaex, huffman_tables)
}

fn decode_single_refinement(
    arith: &mut ArithDecoder,
    reader: &mut HuffmanReader,
    params: &SymbolDictParams,
    all_symbols: &[Arc<Bitmap>],
    sym_code_len: u32,
    width: u32,
    height: u32,
    contexts: &mut TextArithContexts,
    gr_stats: &mut [Cx],
    huffman: Option<&TextRegionHuffman>,
) -> Bitmap {
    let mut target = Bitmap::new(width, height);

    let (id, rdx, rdy, bmsize) = if params.huffman {
        let Some(tables) = huffman else {
            return target;
        };
        let id = reader.read_bits(sym_code_len).unwrap_or(0);
        let rdx = match tables.rdx.decode(reader) {
            Some(HuffmanValue::Value(value)) => value,
            _ => 0,
        };
        let rdy = match tables.rdy.decode(reader) {
            Some(HuffmanValue::Value(value)) => value,
            _ => 0,
        };
        let bmsize = match tables.rsize.decode(reader) {
            Some(HuffmanValue::Value(value)) => value,
            _ => 0,
        };
        reader.align();
        (id, rdx, rdy, bmsize)
    } else {
        let id = contexts.iaid.decode(arith);
        let rdx = contexts.iardx.decode(arith).unwrap_or(0);
        let rdy = contexts.iardy.decode(arith).unwrap_or(0);
        (id, rdx, rdy, 0)
    };

    let Some(reference) = all_symbols.get(id as usize) else {
        log_warn!("JBIG2 symbol dictionary refinement references unknown symbol {}", id);
        return target;
    };

    let refine_params = RefinementParams {
        template: params.rtemplate,
        reference: reference.as_ref(),
        dx: rdx,
        dy: rdy,
        tpgron: false,
        at: params.rat,
    };

    if params.huffman {
        let start = reader.byte_offset().min(reader.data().len());
        let mut sub_arith = ArithDecoder::new(&reader.data()[start..]);
        decode_refinement_region(&mut sub_arith, gr_stats, &mut target, &refine_params);
        let advance = if bmsize > 0 { bmsize as usize } else { target.stride * target.height as usize };
        reader.advance_bytes(advance);
    } else {
        decode_refinement_region(arith, gr_stats, &mut target, &refine_params);
    }

    target
}

fn decode_collective_bitmap(
    reader: &mut HuffmanReader,
    huffman_tables: Option<&SymbolDictHuffman>,
    width: u32,
    height: u32,
    budget: &mut WorkBudget,
) -> Option<Bitmap> {
    let bmsize = match huffman_tables.and_then(|tables| tables.bmsize.decode(reader)) {
        Some(HuffmanValue::Value(value)) => value.max(0) as usize,
        _ => return None,
    };

    reader.align();
    let start = reader.byte_offset();
    let data = reader.data();
    if start >= data.len() || width == 0 || height == 0 {
        return None;
    }
    if !budget.charge_area(width, height) {
        log_warn!("JBIG2 symbol dictionary collective bitmap exceeds the decode work budget");
        return None;
    }

    let mut bitmap = Bitmap::new(width, height);

    if bmsize == 0 {
        let stride = (width as usize).div_ceil(8);
        let needed = stride.saturating_mul(height as usize);
        let available = data.len() - start;
        if available < needed {
            log_warn!("JBIG2 collective bitmap truncated, filling what is available");
        }
        for row in 0..height as usize {
            let src = start + row * stride;
            let dst = row * bitmap.stride;
            let count = stride.min(data.len().saturating_sub(src));
            if count == 0 {
                break;
            }
            bitmap.data[dst..dst + count].copy_from_slice(&data[src..src + count]);
        }
        reader.advance_bytes(needed);
    } else {
        let end = (start + bmsize).min(data.len());
        let (decoded, _) = fax::decode_mmr(&data[start..end], width, height as usize);
        let stride = (width as usize).div_ceil(8);
        for row in 0..height as usize {
            let src = row * stride;
            let dst = row * bitmap.stride;
            if src + stride > decoded.len() {
                break;
            }
            bitmap.data[dst..dst + stride].copy_from_slice(&decoded[src..src + stride]);
        }
        reader.advance_bytes(bmsize);
    }

    Some(bitmap)
}

fn export_symbols(
    params: &SymbolDictParams,
    input_symbols: &[Arc<Bitmap>],
    new_symbols: &[Arc<Bitmap>],
    arith: &mut ArithDecoder,
    reader: &mut HuffmanReader,
    iaex: &mut IntCtx,
    huffman_tables: Option<&SymbolDictHuffman>,
) -> Vec<Arc<Bitmap>> {
    let total = input_symbols.len() + new_symbols.len();
    let mut exported = Vec::new();
    let mut index = 0usize;
    let mut export_flag = false;
    let mut empty_runs = 0u32;

    let table_b1 = crate::decoders::jbig2::huffman::table_b1();

    while index < total {
        let run_length = if params.huffman {
            let table = huffman_tables.map(|_| &table_b1).unwrap_or(&table_b1);
            match table.decode(reader) {
                Some(HuffmanValue::Value(value)) => value,
                _ => break,
            }
        } else {
            match iaex.decode(arith) {
                Some(value) => value,
                None => break,
            }
        };

        if run_length < 0 {
            break;
        }
        if run_length == 0 {
            empty_runs += 1;
            if empty_runs > 1000 {
                log_warn!("JBIG2 symbol dictionary export table has too many empty runs, stopping");
                break;
            }
        } else {
            empty_runs = 0;
        }

        let run_length = (run_length as usize).min(total - index);

        if export_flag {
            for offset in 0..run_length {
                let position = index + offset;
                let symbol = if position < input_symbols.len() {
                    input_symbols[position].clone()
                } else {
                    new_symbols[position - input_symbols.len()].clone()
                };
                exported.push(symbol);
            }
        }

        index += run_length;
        export_flag = !export_flag;
    }

    if exported.is_empty() && !new_symbols.is_empty() {
        log_warn!("JBIG2 symbol dictionary exported no symbols, falling back to all new symbols");
        return new_symbols.to_vec();
    }

    if exported.len() > params.num_ex_syms as usize {
        exported.truncate(params.num_ex_syms as usize);
    }

    exported
}
