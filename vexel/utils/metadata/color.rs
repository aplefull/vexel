use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Cicp {
    pub color_primaries: u16,
    pub transfer_characteristics: u16,
    pub matrix_coefficients: u16,
    pub full_range: bool,
}

impl Cicp {
    pub fn primaries_name(&self) -> &'static str {
        match self.color_primaries {
            1 => "BT.709",
            2 => "Unspecified",
            4 => "BT.470 System M",
            5 => "BT.470 System B/G",
            6 => "BT.601",
            7 => "SMPTE 240M",
            8 => "Generic film",
            9 => "BT.2020",
            10 => "SMPTE ST 428-1 (CIE XYZ)",
            11 => "SMPTE RP 431-2 (DCI-P3)",
            12 => "SMPTE EG 432-1 (Display P3)",
            22 => "EBU Tech 3213-E",
            _ => "Reserved",
        }
    }

    pub fn transfer_name(&self) -> &'static str {
        match self.transfer_characteristics {
            1 => "BT.709",
            2 => "Unspecified",
            4 => "Gamma 2.2 (BT.470 System M)",
            5 => "Gamma 2.8 (BT.470 System B/G)",
            6 => "BT.601",
            7 => "SMPTE 240M",
            8 => "Linear",
            9 => "Logarithmic 100:1",
            10 => "Logarithmic 316:1",
            11 => "IEC 61966-2-4 (xvYCC)",
            12 => "BT.1361",
            13 => "sRGB (IEC 61966-2-1)",
            14 => "BT.2020 10-bit",
            15 => "BT.2020 12-bit",
            16 => "PQ (SMPTE ST 2084)",
            17 => "SMPTE ST 428-1",
            18 => "HLG (ARIB STD-B67)",
            _ => "Reserved",
        }
    }

    pub fn matrix_name(&self) -> &'static str {
        match self.matrix_coefficients {
            0 => "Identity (RGB)",
            1 => "BT.709",
            2 => "Unspecified",
            4 => "FCC",
            5 => "BT.470 System B/G",
            6 => "BT.601",
            7 => "SMPTE 240M",
            8 => "YCgCo",
            9 => "BT.2020 non-constant luminance",
            10 => "BT.2020 constant luminance",
            11 => "SMPTE ST 2085",
            12 => "Chromaticity-derived non-constant luminance",
            13 => "Chromaticity-derived constant luminance",
            14 => "ICtCp",
            15 => "IPT-C2",
            16 => "YCgCo-Re",
            17 => "YCgCo-Ro",
            _ => "Reserved",
        }
    }

    pub fn range_name(&self) -> &'static str {
        if self.full_range {
            "Full range"
        } else {
            "Narrow range"
        }
    }
}

impl fmt::Display for Cicp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} primaries ({}), {} transfer ({}), {} matrix ({}), {}",
            self.primaries_name(),
            self.color_primaries,
            self.transfer_name(),
            self.transfer_characteristics,
            self.matrix_name(),
            self.matrix_coefficients,
            self.range_name()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct MasteringDisplay {
    pub red: [f64; 2],
    pub green: [f64; 2],
    pub blue: [f64; 2],
    pub white_point: [f64; 2],
    pub max_luminance: f64,
    pub min_luminance: f64,
}

fn chromaticity(xy: [f64; 2]) -> String {
    format!("({:.4}, {:.4})", xy[0], xy[1])
}

impl MasteringDisplay {
    pub fn primaries_text(&self) -> String {
        format!(
            "R {}, G {}, B {}, white {}",
            chromaticity(self.red),
            chromaticity(self.green),
            chromaticity(self.blue),
            chromaticity(self.white_point)
        )
    }

    pub fn luminance_text(&self) -> String {
        format!("{} to {} cd/m²", self.min_luminance, self.max_luminance)
    }
}

impl fmt::Display for MasteringDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, luminance {}", self.primaries_text(), self.luminance_text())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct ContentLightLevel {
    pub max_content_light_level: f64,
    pub max_frame_average_light_level: f64,
}

impl fmt::Display for ContentLightLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MaxCLL {} cd/m², MaxFALL {} cd/m²", self.max_content_light_level, self.max_frame_average_light_level)
    }
}
