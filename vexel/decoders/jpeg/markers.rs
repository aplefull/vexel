use crate::utils::marker::Marker;

#[derive(Clone, Debug, PartialEq)]
pub enum JpegMarker {
    // Start Of Frame markers, non-differential, Huffman coding
    SOF0, // Baseline DCT
    SOF1, // Extended sequential DCT
    SOF2, // Progressive DCT
    SOF3, // Lossless (sequential)

    // Start Of Frame markers, differential, Huffman coding
    SOF5, // Differential sequential DCT
    SOF6, // Differential progressive DCT
    SOF7, // Differential lossless (sequential)

    // Start Of Frame markers, non-differential, arithmetic coding
    JPG,   // Reserved for JPEG extensions
    SOF9,  // Extended sequential DCT
    SOF10, // Progressive DCT
    SOF11, // Lossless (sequential)

    // Start Of Frame markers, differential, arithmetic coding
    SOF13, // Differential sequential DCT
    SOF14, // Differential progressive DCT
    SOF15, // Differential lossless (sequential)

    // Huffman table specification
    DHT, // Define Huffman table(s)

    // Arithmetic coding conditioning specification
    DAC, // Define arithmetic coding conditioning(s)

    // Restart interval termination
    RST0,
    RST1,
    RST2,
    RST3,
    RST4,
    RST5,
    RST6,
    RST7,

    // Other markers
    SOI, // Start of image
    EOI, // End of image
    SOS, // Start of scan
    DQT, // Define quantization table(s)
    DNL, // Define number of lines
    DRI, // Define restart interval
    DHP, // Define hierarchical progression
    EXP, // Expand reference component(s)

    // Application segments
    APP0,
    APP1,
    APP2,
    APP3,
    APP4,
    APP5,
    APP6,
    APP7,
    APP8,
    APP9,
    APP10,
    APP11,
    APP12,
    APP13,
    APP14,
    APP15,

    // JPEG extensions
    JPG0,
    JPG1,
    JPG2,
    JPG3,
    JPG4,
    JPG5,
    JPG6,
    JPG7,
    JPG8,
    JPG9,
    JPG10,
    JPG11,
    JPG12,
    JPG13,

    COM, // Comment

    // Special markers
    TEM, // For temporary private use in arithmetic coding

    // Reserved marker
    RES(u8),
}

impl Marker for JpegMarker {
    fn from_u16(value: u16) -> Option<JpegMarker> {
        match value {
            0xFFC0 => Some(JpegMarker::SOF0),
            0xFFC1 => Some(JpegMarker::SOF1),
            0xFFC2 => Some(JpegMarker::SOF2),
            0xFFC3 => Some(JpegMarker::SOF3),
            0xFFC5 => Some(JpegMarker::SOF5),
            0xFFC6 => Some(JpegMarker::SOF6),
            0xFFC7 => Some(JpegMarker::SOF7),
            0xFFC8 => Some(JpegMarker::JPG),
            0xFFC9 => Some(JpegMarker::SOF9),
            0xFFCA => Some(JpegMarker::SOF10),
            0xFFCB => Some(JpegMarker::SOF11),
            0xFFCD => Some(JpegMarker::SOF13),
            0xFFCE => Some(JpegMarker::SOF14),
            0xFFCF => Some(JpegMarker::SOF15),
            0xFFC4 => Some(JpegMarker::DHT),
            0xFFCC => Some(JpegMarker::DAC),
            0xFFD0 => Some(JpegMarker::RST0),
            0xFFD1 => Some(JpegMarker::RST1),
            0xFFD2 => Some(JpegMarker::RST2),
            0xFFD3 => Some(JpegMarker::RST3),
            0xFFD4 => Some(JpegMarker::RST4),
            0xFFD5 => Some(JpegMarker::RST5),
            0xFFD6 => Some(JpegMarker::RST6),
            0xFFD7 => Some(JpegMarker::RST7),
            0xFFD8 => Some(JpegMarker::SOI),
            0xFFD9 => Some(JpegMarker::EOI),
            0xFFDA => Some(JpegMarker::SOS),
            0xFFDB => Some(JpegMarker::DQT),
            0xFFDC => Some(JpegMarker::DNL),
            0xFFDD => Some(JpegMarker::DRI),
            0xFFDE => Some(JpegMarker::DHP),
            0xFFDF => Some(JpegMarker::EXP),
            0xFFE0 => Some(JpegMarker::APP0),
            0xFFE1 => Some(JpegMarker::APP1),
            0xFFE2 => Some(JpegMarker::APP2),
            0xFFE3 => Some(JpegMarker::APP3),
            0xFFE4 => Some(JpegMarker::APP4),
            0xFFE5 => Some(JpegMarker::APP5),
            0xFFE6 => Some(JpegMarker::APP6),
            0xFFE7 => Some(JpegMarker::APP7),
            0xFFE8 => Some(JpegMarker::APP8),
            0xFFE9 => Some(JpegMarker::APP9),
            0xFFEA => Some(JpegMarker::APP10),
            0xFFEB => Some(JpegMarker::APP11),
            0xFFEC => Some(JpegMarker::APP12),
            0xFFED => Some(JpegMarker::APP13),
            0xFFEE => Some(JpegMarker::APP14),
            0xFFEF => Some(JpegMarker::APP15),
            0xFFF0 => Some(JpegMarker::JPG0),
            0xFFF1 => Some(JpegMarker::JPG1),
            0xFFF2 => Some(JpegMarker::JPG2),
            0xFFF3 => Some(JpegMarker::JPG3),
            0xFFF4 => Some(JpegMarker::JPG4),
            0xFFF5 => Some(JpegMarker::JPG5),
            0xFFF6 => Some(JpegMarker::JPG6),
            0xFFF7 => Some(JpegMarker::JPG7),
            0xFFF8 => Some(JpegMarker::JPG8),
            0xFFF9 => Some(JpegMarker::JPG9),
            0xFFFA => Some(JpegMarker::JPG10),
            0xFFFB => Some(JpegMarker::JPG11),
            0xFFFC => Some(JpegMarker::JPG12),
            0xFFFD => Some(JpegMarker::JPG13),
            0xFFFE => Some(JpegMarker::COM),
            0xFF01 => Some(JpegMarker::TEM),
            0xFF02..=0xFFBF => Some(JpegMarker::RES((value & 0xFF) as u8)),
            _ => None,
        }
    }

    fn to_u16(&self) -> u16 {
        match self {
            JpegMarker::SOF0 => 0xFFC0,
            JpegMarker::SOF1 => 0xFFC1,
            JpegMarker::SOF2 => 0xFFC2,
            JpegMarker::SOF3 => 0xFFC3,
            JpegMarker::SOF5 => 0xFFC5,
            JpegMarker::SOF6 => 0xFFC6,
            JpegMarker::SOF7 => 0xFFC7,
            JpegMarker::JPG => 0xFFC8,
            JpegMarker::SOF9 => 0xFFC9,
            JpegMarker::SOF10 => 0xFFCA,
            JpegMarker::SOF11 => 0xFFCB,
            JpegMarker::SOF13 => 0xFFCD,
            JpegMarker::SOF14 => 0xFFCE,
            JpegMarker::SOF15 => 0xFFCF,
            JpegMarker::DHT => 0xFFC4,
            JpegMarker::DAC => 0xFFCC,
            JpegMarker::RST0 => 0xFFD0,
            JpegMarker::RST1 => 0xFFD1,
            JpegMarker::RST2 => 0xFFD2,
            JpegMarker::RST3 => 0xFFD3,
            JpegMarker::RST4 => 0xFFD4,
            JpegMarker::RST5 => 0xFFD5,
            JpegMarker::RST6 => 0xFFD6,
            JpegMarker::RST7 => 0xFFD7,
            JpegMarker::SOI => 0xFFD8,
            JpegMarker::EOI => 0xFFD9,
            JpegMarker::SOS => 0xFFDA,
            JpegMarker::DQT => 0xFFDB,
            JpegMarker::DNL => 0xFFDC,
            JpegMarker::DRI => 0xFFDD,
            JpegMarker::DHP => 0xFFDE,
            JpegMarker::EXP => 0xFFDF,
            JpegMarker::APP0 => 0xFFE0,
            JpegMarker::APP1 => 0xFFE1,
            JpegMarker::APP2 => 0xFFE2,
            JpegMarker::APP3 => 0xFFE3,
            JpegMarker::APP4 => 0xFFE4,
            JpegMarker::APP5 => 0xFFE5,
            JpegMarker::APP6 => 0xFFE6,
            JpegMarker::APP7 => 0xFFE7,
            JpegMarker::APP8 => 0xFFE8,
            JpegMarker::APP9 => 0xFFE9,
            JpegMarker::APP10 => 0xFFEA,
            JpegMarker::APP11 => 0xFFEB,
            JpegMarker::APP12 => 0xFFEC,
            JpegMarker::APP13 => 0xFFED,
            JpegMarker::APP14 => 0xFFEE,
            JpegMarker::APP15 => 0xFFEF,
            JpegMarker::JPG0 => 0xFFF0,
            JpegMarker::JPG1 => 0xFFF1,
            JpegMarker::JPG2 => 0xFFF2,
            JpegMarker::JPG3 => 0xFFF3,
            JpegMarker::JPG4 => 0xFFF4,
            JpegMarker::JPG5 => 0xFFF5,
            JpegMarker::JPG6 => 0xFFF6,
            JpegMarker::JPG7 => 0xFFF7,
            JpegMarker::JPG8 => 0xFFF8,
            JpegMarker::JPG9 => 0xFFF9,
            JpegMarker::JPG10 => 0xFFFA,
            JpegMarker::JPG11 => 0xFFFB,
            JpegMarker::JPG12 => 0xFFFC,
            JpegMarker::JPG13 => 0xFFFD,
            JpegMarker::COM => 0xFFFE,
            JpegMarker::TEM => 0xFF01,
            JpegMarker::RES(value) => 0xFF00 | (*value as u16),
        }
    }
}

pub static JPEG_MARKERS: [JpegMarker; 64] = [
    JpegMarker::SOF0,
    JpegMarker::SOF1,
    JpegMarker::SOF2,
    JpegMarker::SOF3,
    JpegMarker::SOF5,
    JpegMarker::SOF6,
    JpegMarker::SOF7,
    JpegMarker::JPG,
    JpegMarker::SOF9,
    JpegMarker::SOF10,
    JpegMarker::SOF11,
    JpegMarker::SOF13,
    JpegMarker::SOF14,
    JpegMarker::SOF15,
    JpegMarker::DHT,
    JpegMarker::DAC,
    JpegMarker::RST0,
    JpegMarker::RST1,
    JpegMarker::RST2,
    JpegMarker::RST3,
    JpegMarker::RST4,
    JpegMarker::RST5,
    JpegMarker::RST6,
    JpegMarker::RST7,
    JpegMarker::SOI,
    JpegMarker::EOI,
    JpegMarker::SOS,
    JpegMarker::DQT,
    JpegMarker::DNL,
    JpegMarker::DRI,
    JpegMarker::DHP,
    JpegMarker::EXP,
    JpegMarker::APP0,
    JpegMarker::APP1,
    JpegMarker::APP2,
    JpegMarker::APP3,
    JpegMarker::APP4,
    JpegMarker::APP5,
    JpegMarker::APP6,
    JpegMarker::APP7,
    JpegMarker::APP8,
    JpegMarker::APP9,
    JpegMarker::APP10,
    JpegMarker::APP11,
    JpegMarker::APP12,
    JpegMarker::APP13,
    JpegMarker::APP14,
    JpegMarker::APP15,
    JpegMarker::JPG0,
    JpegMarker::JPG1,
    JpegMarker::JPG2,
    JpegMarker::JPG3,
    JpegMarker::JPG4,
    JpegMarker::JPG5,
    JpegMarker::JPG6,
    JpegMarker::JPG7,
    JpegMarker::JPG8,
    JpegMarker::JPG9,
    JpegMarker::JPG10,
    JpegMarker::JPG11,
    JpegMarker::JPG12,
    JpegMarker::JPG13,
    JpegMarker::COM,
    JpegMarker::TEM,
];