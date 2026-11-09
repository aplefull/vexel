use std::borrow::Cow;

use serde::Serialize;
use tsify::Tsify;

use super::types::*;
use crate::utils::info::{ColorModel, FormatInfo, ImageSummary, InfoSection, InfoWriter};

const BIH_SIZE: usize = 20;
const DPTABLE_SIZE: usize = 1728;
const MAX_STRIPE_SECTIONS: usize = 256;
const MAX_MARKER_SECTIONS: usize = 1024;
const UNKNOWN_HEIGHT: u32 = 0xFFFF_FFFF;

const ORDER_HITOLO: u8 = 0x08;
const ORDER_SEQ: u8 = 0x04;
const ORDER_ILEAVE: u8 = 0x02;
const ORDER_SMID: u8 = 0x01;

#[tsify::declare]
pub type Jbig1Info = FormatInfo<Jbig1Section>;

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1Bih {
    pub dl: u8,
    pub d: u8,
    pub planes: u8,
    pub reserved: u8,
    pub xd: u32,
    pub yd: u32,
    pub l0: u32,
    pub mx: u8,
    pub my: u8,
    pub order: u8,
    pub options: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum Jbig1StripeEnd {
    Normal,
    Reset,
    Unterminated,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1Stripe {
    pub plane: u8,
    pub layer: u8,
    pub stripe: u32,
    pub data_length: u64,
    pub end: Jbig1StripeEnd,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1MoreStripes {
    pub count: u32,
    pub data_length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1MoreMarkers {
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1AtMove {
    pub line: u32,
    pub tx: i8,
    pub ty: i8,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1Newlen {
    pub new_height: u32,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct Jbig1Comment {
    pub declared_length: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub enum Jbig1Section {
    Bih(Jbig1Bih),
    Dptable,
    Stripe(Jbig1Stripe),
    MoreStripes(Jbig1MoreStripes),
    MoreMarkers(Jbig1MoreMarkers),
    AtMove(Jbig1AtMove),
    Newlen(Jbig1Newlen),
    Comment(Jbig1Comment),
    Abort,
    Unknown(u8),
}

fn flag_list(value: u8, flags: &[(u8, &str)]) -> String {
    let names: Vec<&str> = flags.iter().filter(|(bit, _)| value & bit != 0).map(|(_, name)| *name).collect();
    let reserved = flags.iter().fold(value, |rest, (bit, _)| rest & !bit);
    let mut text = if names.is_empty() { "none".to_string() } else { names.join(", ") };
    if reserved != 0 {
        text.push_str(&format!(", reserved bits 0x{:02X}", reserved));
    }
    format!("{} (0x{:02X})", text, value)
}

fn prediction_name(options: u8) -> &'static str {
    match (options & OPT_DPON != 0, options & OPT_DPPRIV != 0, options & OPT_DPLAST != 0) {
        (false, _, _) => "off",
        (true, false, _) => "default table",
        (true, true, false) => "private table",
        (true, true, true) => "private table from previous image",
    }
}

impl InfoSection for Jbig1Section {
    fn title(&self) -> Cow<'_, str> {
        match self {
            Jbig1Section::Bih(_) => Cow::Borrowed("Binary Image Header"),
            Jbig1Section::Dptable => Cow::Borrowed("DPTABLE (private prediction table)"),
            Jbig1Section::Stripe(stripe) => match stripe.end {
                Jbig1StripeEnd::Normal => Cow::Borrowed("Stripe (SDNORM)"),
                Jbig1StripeEnd::Reset => Cow::Borrowed("Stripe (SDRST)"),
                Jbig1StripeEnd::Unterminated => Cow::Borrowed("Stripe (unterminated)"),
            },
            Jbig1Section::MoreStripes(_) => Cow::Borrowed("More Stripes"),
            Jbig1Section::MoreMarkers(_) => Cow::Borrowed("More Markers"),
            Jbig1Section::AtMove(_) => Cow::Borrowed("ATMOVE"),
            Jbig1Section::Newlen(_) => Cow::Borrowed("NEWLEN"),
            Jbig1Section::Comment(_) => Cow::Borrowed("Comment"),
            Jbig1Section::Abort => Cow::Borrowed("ABORT"),
            Jbig1Section::Unknown(marker) => Cow::Owned(format!("Unknown Marker (0xFF {:02X})", marker)),
        }
    }

    fn describe(&self, out: &mut InfoWriter) {
        match self {
            Jbig1Section::Bih(bih) => {
                out.field("Lowest resolution layer (DL)", bih.dl);
                out.field("Highest resolution layer (D)", bih.d);
                out.field("Resolution layers", (bih.d as i16 - bih.dl as i16 + 1).max(1));
                out.field("Planes (P)", bih.planes);
                if bih.reserved != 0 {
                    out.field("Reserved byte", format!("0x{:02X}", bih.reserved));
                }
                out.field("Width (XD)", bih.xd);
                if bih.yd == UNKNOWN_HEIGHT {
                    out.field("Height (YD)", format!("unknown ({}), set by NEWLEN", bih.yd));
                } else {
                    out.field("Height (YD)", bih.yd);
                }
                out.field("Lines per stripe (L0)", bih.l0);
                out.field("Max ATMOVE x (MX)", bih.mx);
                out.field("Max ATMOVE y (MY)", bih.my);
                out.field(
                    "Order",
                    flag_list(
                        bih.order,
                        &[(ORDER_HITOLO, "HITOLO"), (ORDER_SEQ, "SEQ"), (ORDER_ILEAVE, "ILEAVE"), (ORDER_SMID, "SMID")],
                    ),
                );
                out.field(
                    "Options",
                    flag_list(
                        bih.options,
                        &[
                            (OPT_LRLTWO, "LRLTWO"),
                            (OPT_VLENGTH, "VLENGTH"),
                            (OPT_TPDON, "TPDON"),
                            (OPT_TPBON, "TPBON"),
                            (OPT_DPON, "DPON"),
                            (OPT_DPPRIV, "DPPRIV"),
                            (OPT_DPLAST, "DPLAST"),
                        ],
                    ),
                );
                out.field("Template", if bih.options & OPT_LRLTWO != 0 { "two-line" } else { "three-line" });
                out.field("Deterministic prediction", prediction_name(bih.options));
            }
            Jbig1Section::Dptable => {}
            Jbig1Section::Stripe(stripe) => {
                out.field("Plane", stripe.plane);
                out.field("Layer", stripe.layer);
                out.field("Stripe", stripe.stripe);
                out.field("Coded data", format!("{} bytes", stripe.data_length));
            }
            Jbig1Section::MoreStripes(more) => {
                out.field("Stripes", more.count);
                out.field("Coded data", format!("{} bytes", more.data_length));
            }
            Jbig1Section::MoreMarkers(more) => {
                out.field("Markers", more.count);
            }
            Jbig1Section::AtMove(at_move) => {
                out.field("Line", at_move.line);
                out.field("TX", at_move.tx);
                out.field("TY", at_move.ty);
            }
            Jbig1Section::Newlen(newlen) => {
                out.field("New height (YD)", newlen.new_height);
                if !newlen.applied {
                    out.field("Applied", "no, VLENGTH not set");
                }
            }
            Jbig1Section::Comment(comment) => {
                out.field("Declared length", format!("{} bytes", comment.declared_length));
                out.text("Text", &comment.text);
            }
            Jbig1Section::Abort | Jbig1Section::Unknown(_) => {}
        }
    }
}

fn ceil_half(x: u32, n: u32) -> u32 {
    if n >= 32 {
        return u32::from(x > 0);
    }
    ((x as u64 + (1u64 << n) - 1) >> n) as u32
}

fn iindex_for_order(order: u8) -> [usize; 3] {
    match order & 0x07 {
        0 => [2, 1, 0],
        2 => [2, 0, 1],
        3 => [1, 0, 2],
        4 => [0, 2, 1],
        5 => [1, 2, 0],
        6 => [0, 1, 2],
        _ => [1, 0, 2],
    }
}

struct StripeOrder {
    dl: u8,
    d: u8,
    planes: u8,
    l0: u32,
    yd: u32,
    iindex: [usize; 3],
    ii: [u32; 3],
}

impl StripeOrder {
    fn new(bih: &Jbig1Bih) -> Self {
        let d = bih.d;
        let dl = bih.dl.min(d);
        let planes = bih.planes.max(1);
        let mut l0 = bih.l0;
        if l0 == 0 {
            l0 = ceil_half(bih.yd, d as u32).max(1);
        }
        let iindex = iindex_for_order(bih.order);
        let mut ii = [0u32; 3];
        ii[iindex[1]] = dl as u32;
        StripeOrder { dl, d, planes, l0, yd: bih.yd, iindex, ii }
    }

    fn stripes(&self) -> u32 {
        let hy = ceil_half(self.yd, (self.d - self.dl) as u32) as u64;
        let hl = (self.l0 as u64).checked_shl(self.dl as u32).unwrap_or(u64::MAX).max(1);
        hy.div_ceil(hl).min(u32::MAX as u64) as u32
    }

    fn current(&self) -> (u8, u8, u32) {
        (self.ii[self.iindex[2]] as u8, self.ii[self.iindex[1]] as u8, self.ii[self.iindex[0]])
    }

    fn advance(&mut self) {
        let mut start = [0u32; 3];
        let mut end = [0u32; 3];
        end[self.iindex[0]] = self.stripes().saturating_sub(1);
        start[self.iindex[1]] = self.dl as u32;
        end[self.iindex[1]] = self.d as u32;
        end[self.iindex[2]] = self.planes as u32 - 1;

        for i in (0..3).rev() {
            self.ii[i] += 1;
            if self.ii[i] <= end[i] {
                return;
            }
            self.ii[i] = start[i];
        }
    }
}

struct Scanner {
    info: Jbig1Info,
    stripe_sections: usize,
    more: Option<(u64, Jbig1MoreStripes)>,
    marker_sections: usize,
    more_markers: Option<(u64, Jbig1MoreMarkers)>,
}

impl Scanner {
    fn push_stripe(&mut self, offset: u64, length: u64, stripe: Jbig1Stripe) {
        if self.stripe_sections < MAX_STRIPE_SECTIONS {
            self.stripe_sections += 1;
            self.info.push(offset, Some(length), Jbig1Section::Stripe(stripe));
            return;
        }
        let (_, more) = self.more.get_or_insert((offset, Jbig1MoreStripes { count: 0, data_length: 0 }));
        more.count += 1;
        more.data_length += stripe.data_length;
    }

    fn push_marker(&mut self, offset: u64, length: Option<u64>, section: Jbig1Section) {
        if self.marker_sections < MAX_MARKER_SECTIONS {
            self.marker_sections += 1;
            self.info.push(offset, length, section);
            return;
        }
        let (_, more) = self.more_markers.get_or_insert((offset, Jbig1MoreMarkers { count: 0 }));
        more.count = more.count.saturating_add(1);
    }
}

fn be_u32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

pub fn scan(data: &[u8]) -> Jbig1Info {
    let mut scanner =
        Scanner { info: Jbig1Info::default(), stripe_sections: 0, more: None, marker_sections: 0, more_markers: None };
    if data.len() < BIH_SIZE {
        return scanner.info;
    }

    let bih = Jbig1Bih {
        dl: data[0],
        d: data[1],
        planes: data[2],
        reserved: data[3],
        xd: be_u32(data, 4),
        yd: be_u32(data, 8),
        l0: be_u32(data, 12),
        mx: data[16],
        my: data[17],
        order: data[18],
        options: data[19],
    };
    scanner.info.push(0, Some(BIH_SIZE as u64), Jbig1Section::Bih(bih.clone()));

    let mut start = BIH_SIZE;
    if bih.options & OPT_DPPRIV != 0 && bih.options & OPT_DPLAST == 0 && data.len() >= BIH_SIZE + DPTABLE_SIZE {
        scanner.info.push(start as u64, Some(DPTABLE_SIZE as u64), Jbig1Section::Dptable);
        start += DPTABLE_SIZE;
    }

    let mut order = StripeOrder::new(&bih);
    let mut height = bih.yd;
    let mut pos = start;
    let mut sde_start = start;
    let mut aborted = false;

    while pos < data.len() {
        if data[pos] != MARKER_ESC {
            pos += 1;
            continue;
        }
        if pos + 1 >= data.len() {
            break;
        }

        let marker = data[pos + 1];
        let offset = pos as u64;
        match marker {
            MARKER_STUFF => {
                pos += 2;
                continue;
            }
            MARKER_SDNORM | MARKER_SDRST => {
                let (plane, layer, stripe) = order.current();
                let end = if marker == MARKER_SDRST { Jbig1StripeEnd::Reset } else { Jbig1StripeEnd::Normal };
                let data_length = (pos - sde_start) as u64;
                let stripe = Jbig1Stripe { plane, layer, stripe, data_length, end };
                scanner.push_stripe(sde_start as u64, data_length + 2, stripe);
                order.advance();
                pos += 2;
            }
            MARKER_ATMOVE => {
                if pos + 7 >= data.len() {
                    break;
                }
                let at_move =
                    Jbig1AtMove { line: be_u32(data, pos + 2), tx: data[pos + 6] as i8, ty: data[pos + 7] as i8 };
                scanner.push_marker(offset, Some(8), Jbig1Section::AtMove(at_move));
                pos += 8;
            }
            MARKER_NEWLEN => {
                if pos + 5 >= data.len() {
                    break;
                }
                let new_height = be_u32(data, pos + 2);
                let applied = bih.options & OPT_VLENGTH != 0;
                if applied {
                    height = new_height;
                    order.yd = new_height;
                }
                scanner.push_marker(offset, Some(6), Jbig1Section::Newlen(Jbig1Newlen { new_height, applied }));
                pos += 6;
            }
            MARKER_COMMENT => {
                if pos + 5 >= data.len() {
                    break;
                }
                let declared_length = be_u32(data, pos + 2);
                let text_end = (pos + 6).saturating_add(declared_length as usize).min(data.len());
                let text = String::from_utf8_lossy(&data[pos + 6..text_end]).into_owned();
                scanner.push_marker(
                    offset,
                    Some((text_end - pos) as u64),
                    Jbig1Section::Comment(Jbig1Comment { declared_length, text }),
                );
                pos = text_end;
            }
            MARKER_ABORT => {
                scanner.info.push(offset, Some(2), Jbig1Section::Abort);
                aborted = true;
                break;
            }
            other => {
                scanner.push_marker(offset, Some(2), Jbig1Section::Unknown(other));
                pos += 2;
            }
        }
        sde_start = pos;
    }

    if !aborted && sde_start < data.len() {
        let (plane, layer, stripe) = order.current();
        let data_length = (data.len() - sde_start) as u64;
        let stripe = Jbig1Stripe { plane, layer, stripe, data_length, end: Jbig1StripeEnd::Unterminated };
        scanner.push_stripe(sde_start as u64, data_length, stripe);
    }

    if let Some((offset, more)) = scanner.more.take() {
        scanner.info.push(offset, None, Jbig1Section::MoreStripes(more));
    }
    if let Some((offset, more)) = scanner.more_markers.take() {
        scanner.info.push(offset, None, Jbig1Section::MoreMarkers(more));
    }
    scanner.info.sections.sort_by_key(|section| section.offset);

    scanner.info.summary = build_summary(&bih, height);
    scanner.info
}

fn build_summary(bih: &Jbig1Bih, height: u32) -> Option<ImageSummary> {
    if bih.xd == 0 || height == 0 {
        return None;
    }
    let planes = bih.planes.max(1);
    let mut summary = if (2..=8).contains(&planes) {
        ImageSummary::new(bih.xd, height, ColorModel::Gray, 1, planes)
    } else {
        ImageSummary::new(bih.xd, height, ColorModel::Bilevel, 1, 1)
    };
    summary.compression = Some("JBIG (T.82)".to_string());
    Some(summary)
}
