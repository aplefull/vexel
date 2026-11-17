use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Orientation {
    pub value: u8,
    pub applied: bool,
}

impl Orientation {
    pub(crate) fn new(value: u32, applied: bool) -> Option<Self> {
        match u8::try_from(value) {
            Ok(value @ 1..=8) => Some(Orientation { value, applied }),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self.value {
            1 => "Normal",
            2 => "Mirror horizontal",
            3 => "Rotate 180",
            4 => "Mirror vertical",
            5 => "Mirror horizontal and rotate 270 CW",
            6 => "Rotate 90 CW",
            7 => "Mirror horizontal and rotate 90 CW",
            8 => "Rotate 270 CW",
            _ => "Invalid",
        }
    }

    pub fn swaps_dimensions(&self) -> bool {
        self.value >= 5
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name(), self.value)?;
        if self.applied && self.value != 1 {
            f.write_str(", applied to decoded pixels")?;
        }
        Ok(())
    }
}
