use crate::decoders::exr::types::ExrPixelType;
use crate::utils::error::{VexelError, VexelResult};

const MAX_SUFFIX_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Unknown,
    LossyDct,
    Rle,
}

impl Scheme {
    fn from_bits(value: u8) -> Option<Scheme> {
        match value {
            0 => Some(Scheme::Unknown),
            1 => Some(Scheme::LossyDct),
            2 => Some(Scheme::Rle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    suffix: String,
    scheme: Scheme,
    pixel_type: ExrPixelType,
    csc_index: Option<usize>,
    case_insensitive: bool,
}

impl Rule {
    pub fn matches(&self, suffix: &str, pixel_type: ExrPixelType) -> bool {
        if self.pixel_type != pixel_type {
            return false;
        }

        if self.case_insensitive {
            self.suffix.eq_ignore_ascii_case(suffix)
        } else {
            self.suffix == suffix
        }
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn csc_index(&self) -> Option<usize> {
        self.csc_index
    }
}

pub struct RuleSpec {
    suffix: &'static str,
    scheme: Scheme,
    pixel_type: ExrPixelType,
    csc_index: Option<usize>,
    case_insensitive: bool,
}

impl RuleSpec {
    const fn new(
        suffix: &'static str,
        scheme: Scheme,
        pixel_type: ExrPixelType,
        csc_index: Option<usize>,
        case_insensitive: bool,
    ) -> Self {
        Self { suffix, scheme, pixel_type, csc_index, case_insensitive }
    }

    fn build(&self) -> Rule {
        Rule {
            suffix: self.suffix.to_string(),
            scheme: self.scheme,
            pixel_type: self.pixel_type,
            csc_index: self.csc_index,
            case_insensitive: self.case_insensitive,
        }
    }
}

const DEFAULT_RULES: [RuleSpec; 15] = [
    RuleSpec::new("R", Scheme::LossyDct, ExrPixelType::Half, Some(0), false),
    RuleSpec::new("R", Scheme::LossyDct, ExrPixelType::Float, Some(0), false),
    RuleSpec::new("G", Scheme::LossyDct, ExrPixelType::Half, Some(1), false),
    RuleSpec::new("G", Scheme::LossyDct, ExrPixelType::Float, Some(1), false),
    RuleSpec::new("B", Scheme::LossyDct, ExrPixelType::Half, Some(2), false),
    RuleSpec::new("B", Scheme::LossyDct, ExrPixelType::Float, Some(2), false),
    RuleSpec::new("Y", Scheme::LossyDct, ExrPixelType::Half, None, false),
    RuleSpec::new("Y", Scheme::LossyDct, ExrPixelType::Float, None, false),
    RuleSpec::new("BY", Scheme::LossyDct, ExrPixelType::Half, None, false),
    RuleSpec::new("BY", Scheme::LossyDct, ExrPixelType::Float, None, false),
    RuleSpec::new("RY", Scheme::LossyDct, ExrPixelType::Half, None, false),
    RuleSpec::new("RY", Scheme::LossyDct, ExrPixelType::Float, None, false),
    RuleSpec::new("A", Scheme::Rle, ExrPixelType::Uint, None, false),
    RuleSpec::new("A", Scheme::Rle, ExrPixelType::Half, None, false),
    RuleSpec::new("A", Scheme::Rle, ExrPixelType::Float, None, false),
];

const LEGACY_RULES: [RuleSpec; 25] = [
    RuleSpec::new("r", Scheme::LossyDct, ExrPixelType::Half, Some(0), true),
    RuleSpec::new("r", Scheme::LossyDct, ExrPixelType::Float, Some(0), true),
    RuleSpec::new("red", Scheme::LossyDct, ExrPixelType::Half, Some(0), true),
    RuleSpec::new("red", Scheme::LossyDct, ExrPixelType::Float, Some(0), true),
    RuleSpec::new("g", Scheme::LossyDct, ExrPixelType::Half, Some(1), true),
    RuleSpec::new("g", Scheme::LossyDct, ExrPixelType::Float, Some(1), true),
    RuleSpec::new("grn", Scheme::LossyDct, ExrPixelType::Half, Some(1), true),
    RuleSpec::new("grn", Scheme::LossyDct, ExrPixelType::Float, Some(1), true),
    RuleSpec::new("green", Scheme::LossyDct, ExrPixelType::Half, Some(1), true),
    RuleSpec::new("green", Scheme::LossyDct, ExrPixelType::Float, Some(1), true),
    RuleSpec::new("b", Scheme::LossyDct, ExrPixelType::Half, Some(2), true),
    RuleSpec::new("b", Scheme::LossyDct, ExrPixelType::Float, Some(2), true),
    RuleSpec::new("blu", Scheme::LossyDct, ExrPixelType::Half, Some(2), true),
    RuleSpec::new("blu", Scheme::LossyDct, ExrPixelType::Float, Some(2), true),
    RuleSpec::new("blue", Scheme::LossyDct, ExrPixelType::Half, Some(2), true),
    RuleSpec::new("blue", Scheme::LossyDct, ExrPixelType::Float, Some(2), true),
    RuleSpec::new("y", Scheme::LossyDct, ExrPixelType::Half, None, true),
    RuleSpec::new("y", Scheme::LossyDct, ExrPixelType::Float, None, true),
    RuleSpec::new("by", Scheme::LossyDct, ExrPixelType::Half, None, true),
    RuleSpec::new("by", Scheme::LossyDct, ExrPixelType::Float, None, true),
    RuleSpec::new("ry", Scheme::LossyDct, ExrPixelType::Half, None, true),
    RuleSpec::new("ry", Scheme::LossyDct, ExrPixelType::Float, None, true),
    RuleSpec::new("a", Scheme::Rle, ExrPixelType::Uint, None, true),
    RuleSpec::new("a", Scheme::Rle, ExrPixelType::Half, None, true),
    RuleSpec::new("a", Scheme::Rle, ExrPixelType::Float, None, true),
];

pub fn default_rules() -> Vec<Rule> {
    DEFAULT_RULES.iter().map(RuleSpec::build).collect()
}

pub fn legacy_rules() -> Vec<Rule> {
    LEGACY_RULES.iter().map(RuleSpec::build).collect()
}

pub fn channel_suffix(name: &str) -> &str {
    match name.rfind('.') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

pub fn channel_prefix(name: &str) -> &str {
    match name.rfind('.') {
        Some(pos) => &name[..=pos],
        None => "",
    }
}

pub fn read_rules(data: &[u8]) -> VexelResult<(Vec<Rule>, usize)> {
    if data.len() < 2 {
        return Err(VexelError::Custom("EXR: DWA channel rule block truncated".to_string()));
    }

    let rule_size = u16::from_le_bytes([data[0], data[1]]) as usize;
    if rule_size < 2 || rule_size > data.len() {
        return Err(VexelError::Custom(format!("EXR: DWA channel rule block has invalid size {}", rule_size)));
    }

    let mut rules = Vec::new();
    let mut cursor = &data[2..rule_size];

    while !cursor.is_empty() {
        let (rule, consumed) = read_rule(cursor)?;
        rules.push(rule);
        cursor = &cursor[consumed..];
    }

    Ok((rules, rule_size))
}

fn read_rule(data: &[u8]) -> VexelResult<(Rule, usize)> {
    let terminator = data
        .iter()
        .take(MAX_SUFFIX_LENGTH + 1)
        .position(|&b| b == 0)
        .ok_or_else(|| VexelError::Custom("EXR: DWA channel rule suffix is unterminated".to_string()))?;

    if data.len() < terminator + 3 {
        return Err(VexelError::Custom("EXR: DWA channel rule truncated".to_string()));
    }

    let suffix = String::from_utf8_lossy(&data[..terminator]).into_owned();
    let value = data[terminator + 1];
    let pixel_type_raw = data[terminator + 2];

    let csc_index = match value >> 4 {
        0 => None,
        n if n <= 3 => Some(n as usize - 1),
        n => return Err(VexelError::Custom(format!("EXR: DWA channel rule has invalid CSC index {}", n))),
    };

    let scheme = Scheme::from_bits((value >> 2) & 3)
        .ok_or_else(|| VexelError::Custom("EXR: DWA channel rule has invalid scheme".to_string()))?;

    let pixel_type = ExrPixelType::from_i32(pixel_type_raw as i32)
        .ok_or_else(|| VexelError::Custom(format!("EXR: DWA channel rule has invalid type {}", pixel_type_raw)))?;

    let rule = Rule { suffix, scheme, pixel_type, csc_index, case_insensitive: value & 1 != 0 };

    Ok((rule, terminator + 3))
}
