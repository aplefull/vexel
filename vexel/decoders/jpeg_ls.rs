use crate::bitreader::BitReader;
use crate::utils::error::VexelResult;
use crate::utils::marker::Marker;
use crate::{Image, PixelData};
use std::fmt;
use std::fmt::Debug;
use std::io::{Cursor, Error, Read, Seek};

#[derive(Debug, Clone, PartialEq)]
pub enum JpegLsMarker {
    SOI,   // Start of Image
    EOI,   // End of Image
    SOF55, // Start of Frame (JPEG-LS)
    SOS,   // Start of Scan
    LSE,   // JPEG-LS preset parameters
    DNL,   // Define Number of Lines
    DRI,   // Define Restart Interval

    // Restart interval termination
    RST0,
    RST1,
    RST2,
    RST3,
    RST4,
    RST5,
    RST6,
    RST7,

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

    COM, // Comment
}

impl Marker for JpegLsMarker {
    fn from_u16(value: u16) -> Option<Self> {
        match value {
            0xFFD8 => Some(JpegLsMarker::SOI),
            0xFFD9 => Some(JpegLsMarker::EOI),
            0xFFF7 => Some(JpegLsMarker::SOF55),
            0xFFDA => Some(JpegLsMarker::SOS),
            0xFFF8 => Some(JpegLsMarker::LSE),
            0xFFDC => Some(JpegLsMarker::DNL),
            0xFFDD => Some(JpegLsMarker::DRI),
            0xFFD0 => Some(JpegLsMarker::RST0),
            0xFFD1 => Some(JpegLsMarker::RST1),
            0xFFD2 => Some(JpegLsMarker::RST2),
            0xFFD3 => Some(JpegLsMarker::RST3),
            0xFFD4 => Some(JpegLsMarker::RST4),
            0xFFD5 => Some(JpegLsMarker::RST5),
            0xFFD6 => Some(JpegLsMarker::RST6),
            0xFFD7 => Some(JpegLsMarker::RST7),
            0xFFE0 => Some(JpegLsMarker::APP0),
            0xFFE1 => Some(JpegLsMarker::APP1),
            0xFFE2 => Some(JpegLsMarker::APP2),
            0xFFE3 => Some(JpegLsMarker::APP3),
            0xFFE4 => Some(JpegLsMarker::APP4),
            0xFFE5 => Some(JpegLsMarker::APP5),
            0xFFE6 => Some(JpegLsMarker::APP6),
            0xFFE7 => Some(JpegLsMarker::APP7),
            0xFFE8 => Some(JpegLsMarker::APP8),
            0xFFE9 => Some(JpegLsMarker::APP9),
            0xFFEA => Some(JpegLsMarker::APP10),
            0xFFEB => Some(JpegLsMarker::APP11),
            0xFFEC => Some(JpegLsMarker::APP12),
            0xFFED => Some(JpegLsMarker::APP13),
            0xFFEE => Some(JpegLsMarker::APP14),
            0xFFEF => Some(JpegLsMarker::APP15),
            0xFFFE => Some(JpegLsMarker::COM),
            _ => None,
        }
    }

    fn to_u16(&self) -> u16 {
        match self {
            JpegLsMarker::SOI => 0xFFD8,
            JpegLsMarker::EOI => 0xFFD9,
            JpegLsMarker::SOF55 => 0xFFF7,
            JpegLsMarker::SOS => 0xFFDA,
            JpegLsMarker::LSE => 0xFFF8,
            JpegLsMarker::DNL => 0xFFDC,
            JpegLsMarker::DRI => 0xFFDD,
            JpegLsMarker::RST0 => 0xFFD0,
            JpegLsMarker::RST1 => 0xFFD1,
            JpegLsMarker::RST2 => 0xFFD2,
            JpegLsMarker::RST3 => 0xFFD3,
            JpegLsMarker::RST4 => 0xFFD4,
            JpegLsMarker::RST5 => 0xFFD5,
            JpegLsMarker::RST6 => 0xFFD6,
            JpegLsMarker::RST7 => 0xFFD7,
            JpegLsMarker::APP0 => 0xFFE0,
            JpegLsMarker::APP1 => 0xFFE1,
            JpegLsMarker::APP2 => 0xFFE2,
            JpegLsMarker::APP3 => 0xFFE3,
            JpegLsMarker::APP4 => 0xFFE4,
            JpegLsMarker::APP5 => 0xFFE5,
            JpegLsMarker::APP6 => 0xFFE6,
            JpegLsMarker::APP7 => 0xFFE7,
            JpegLsMarker::APP8 => 0xFFE8,
            JpegLsMarker::APP9 => 0xFFE9,
            JpegLsMarker::APP10 => 0xFFEA,
            JpegLsMarker::APP11 => 0xFFEB,
            JpegLsMarker::APP12 => 0xFFEC,
            JpegLsMarker::APP13 => 0xFFED,
            JpegLsMarker::APP14 => 0xFFEE,
            JpegLsMarker::APP15 => 0xFFEF,
            JpegLsMarker::COM => 0xFFFE,
        }
    }
}

static JPEG_LS_MARKERS: [JpegLsMarker; 32] = [
    JpegLsMarker::SOI,
    JpegLsMarker::EOI,
    JpegLsMarker::SOF55,
    JpegLsMarker::SOS,
    JpegLsMarker::LSE,
    JpegLsMarker::DNL,
    JpegLsMarker::DRI,
    JpegLsMarker::RST0,
    JpegLsMarker::RST1,
    JpegLsMarker::RST2,
    JpegLsMarker::RST3,
    JpegLsMarker::RST4,
    JpegLsMarker::RST5,
    JpegLsMarker::RST6,
    JpegLsMarker::RST7,
    JpegLsMarker::APP0,
    JpegLsMarker::APP1,
    JpegLsMarker::APP2,
    JpegLsMarker::APP3,
    JpegLsMarker::APP4,
    JpegLsMarker::APP5,
    JpegLsMarker::APP6,
    JpegLsMarker::APP7,
    JpegLsMarker::APP8,
    JpegLsMarker::APP9,
    JpegLsMarker::APP10,
    JpegLsMarker::APP11,
    JpegLsMarker::APP12,
    JpegLsMarker::APP13,
    JpegLsMarker::APP14,
    JpegLsMarker::APP15,
    JpegLsMarker::COM,
];

// TODO actually used unused variables if needed, or remove them
#[derive(Debug)]
#[allow(unused)]
pub struct ColorComponentInfo {
    pub id: u8,
    pub horizontal_sampling_factor: u8,
    pub vertical_sampling_factor: u8,
    pub quantization_table_id: u8,
}

#[derive(Debug)]
enum InterleaveMode {
    None,
    Line,
    Sample,
}

pub struct JpegLsDecoder<R: Read + Seek> {
    width: u32,
    height: u32,
    precision: u8,
    max_val: u16,
    near: u8,
    ilv: InterleaveMode,
    range: u16,
    qbpp: u16,
    bpp: u16,
    limit: u16,
    t1: i32,
    t2: i32,
    t3: i32,
    reset: u16,
    min_c: i16,
    max_c: i16,
    color_components: Vec<ColorComponentInfo>,
    n: [i32; 367],
    a: [i32; 367],
    b: [i32; 365],
    c: [i32; 365],
    nn: [i32; 367],
    run_index: i32,
    j: [i32; 32],
    data: Vec<u8>,
    reader: BitReader<R>,
}

impl<R: Read + Seek> Debug for JpegLsDecoder<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JpegLsDecoder")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("precision", &self.precision)
            .field("near", &self.near)
            .field("ilv", &self.ilv)
            .field("range", &self.range)
            .field("qbpp", &self.qbpp)
            .field("bpp", &self.bpp)
            .field("limit", &self.limit)
            .field("t1", &self.t1)
            .field("t2", &self.t2)
            .field("t3", &self.t3)
            .field("reset", &self.reset)
            .field("min_c", &self.min_c)
            .field("max_c", &self.max_c)
            .field("color_components", &self.color_components)
            .field("n", &self.n)
            .field("a", &self.a)
            .field("b", &self.b)
            .field("c", &self.c)
            .field("nn", &self.nn)
            .field("run_index", &self.run_index)
            .field("j", &self.j)
            .field("data", &self.data.len())
            .finish()
    }
}

impl<R: Read + Seek> JpegLsDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            width: 0,
            height: 0,
            precision: 0,
            max_val: 0,
            near: 0,
            ilv: InterleaveMode::None,
            range: 0,
            qbpp: 0,
            bpp: 0,
            limit: 0,
            t1: 3,
            t2: 7,
            t3: 21,
            reset: 64,
            min_c: -128,
            max_c: 127,
            color_components: Vec::new(),
            n: [1; 367],
            a: [0; 367],
            b: [0; 365],
            c: [0; 365],
            nn: [0; 367],
            run_index: 0,
            j: [0; 32],
            data: Vec::new(),
            reader: BitReader::new(reader),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn read_sof_info(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        self.reader.read_bits(16)?;

        // Read data precision (1 byte)
        self.precision = self.reader.read_bits(8)? as u8;

        // Read image height and width (2 bytes each)
        // Note: height and width are allowed to be 0.
        // In this case value shall be defined in LSE marker.
        // A non-zero value shall not be changed by LSE marker segments.
        self.height = self.reader.read_bits(16)?;
        self.width = self.reader.read_bits(16)?;

        // Read number of components (1 byte)
        let components_count = self.reader.read_bits(8)?;

        for _ in 0..components_count {
            // Read component ID (1 byte)
            let component_id = self.reader.read_bits(8)?;

            // Read horizontal and vertical sampling factors (1 byte)
            let sampling_factors = self.reader.read_bits(8)?;
            let horizontal_sampling = (sampling_factors >> 4) & 0xF;
            let vertical_sampling = sampling_factors & 0xF;

            // Read quantization table ID (1 byte)
            let quantization_table_selector = self.reader.read_bits(8)?;

            self.color_components.push(ColorComponentInfo {
                id: component_id as u8,
                horizontal_sampling_factor: horizontal_sampling as u8,
                vertical_sampling_factor: vertical_sampling as u8,
                quantization_table_id: quantization_table_selector as u8,
            });
        }

        Ok(())
    }

    fn read_sos_info(&mut self) -> Result<(), Error> {
        // Skip marker length (2 bytes)
        self.reader.read_bits(16)?;

        // Read number of components (1 byte)
        let components_count = self.reader.read_bits(8)?;

        for _ in 0..components_count {
            // Read component ID (1 byte)
            let component_selector = self.reader.read_bits(8)? as u8;

            if !self.color_components.iter().any(|c| c.id == component_selector) {
                return Err(Error::new(std::io::ErrorKind::InvalidData, "Invalid component ID"));
            }

            // Read table selectors (1 byte)
            // TODO save
            let _mapping_table_selector = self.reader.read_bits(8)? as u8;
        }

        // Read near and ilv values (2 bytes)
        self.near = self.reader.read_bits(8)? as u8;
        self.ilv = match self.reader.read_bits(8)? as u8 {
            0 => InterleaveMode::None,
            1 => InterleaveMode::Line,
            2 => InterleaveMode::Sample,
            _ => return Err(Error::new(std::io::ErrorKind::InvalidData, "Invalid ILV value")),
        };

        // TODO Check that near is valid

        // Read point transform (1 byte)
        // TODO this is replaced by something else in JPEG-LS
        let _point_transform = self.reader.read_bits(8)? as u8;

        Ok(())
    }

    fn read_sos_bitstream(&mut self) -> Result<(), Error> {
        let mut current_byte = self.reader.read_bits(8)? as u8;

        loop {
            // This can be either a marker or literal data
            if current_byte == 0xFF {
                let next_byte = self.reader.read_bits(8)? as u8;

                // End of image marker
                if next_byte == (JpegLsMarker::EOI.to_u16() & 0xFF) as u8 {
                    break;
                }

                // This is a marker
                if next_byte != 0x00 {
                    // TODO figure out how to handle markers correctly
                    self.data.push(current_byte);
                    current_byte = self.reader.read_bits(8)? as u8;
                    continue;
                }

                // This is a stuffed byte
                if next_byte == 0x00 {
                    self.data.push(current_byte);

                    current_byte = self.reader.read_bits(8)? as u8;
                    continue;
                }
            } else {
                self.data.push(current_byte);
                current_byte = self.reader.read_bits(8)? as u8;
            }
        }

        Ok(())
    }

    fn init_default_parameters(&mut self) {
        self.max_val = (1 << self.precision) - 1;
        self.range = if self.near == 0 {
            self.max_val + 1
        } else {
            (self.max_val + 2 * self.near as u16) / (2 * self.near as u16 + 1) + 1
        };
        self.qbpp = (self.range as f32).log2() as u16;
        self.bpp = std::cmp::max(2, ((self.max_val + 1) as f64).log2().ceil() as u16);
        self.limit = 2 * (self.bpp + std::cmp::max(8, self.bpp));

        for a in self.a.iter_mut() {
            *a = std::cmp::max(2, (self.range as i32 + 2i32.pow(5)) / 2i32.pow(6));
        }

        self.j = [
            0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        ];
    }

    fn compute_local_gradients(&self, ra: u16, rb: u16, rc: u16, rd: u16) -> (i32, i32, i32) {
        let d1 = rd as i32 - rb as i32;
        let d2 = rb as i32 - rc as i32;
        let d3 = rc as i32 - ra as i32;

        (d1, d2, d3)
    }

    fn correct_predicted_value(&self, val: i32) -> i32 {
        if val > self.max_val as i32 {
            self.max_val as i32
        } else if val < 0 {
            0
        } else {
            val
        }
    }

    fn compute_golomb(&self, n: u32, a: u32) -> u16 {
        let mut k = 0u16;

        while (n << k) < a {
            k += 1;
        }

        k
    }

    fn decode_mapped_error_value(
        &self,
        k: u16,
        g_limit: u16,
        reader: &mut BitReader<Cursor<Vec<u8>>>,
    ) -> Result<u32, Error> {
        let mut value;
        let mut unary_code = 0u32;

        loop {
            let bit = reader.read_bit()?;
            if bit {
                break;
            }
            unary_code += 1;
        }

        let limit = g_limit - self.qbpp - 1;

        let (mut bits_to_read, offset) = if unary_code < limit as u32 {
            value = unary_code;
            (k, 0)
        } else {
            value = 0;
            (self.qbpp, 1)
        };

        while bits_to_read > 0 {
            let bit = reader.read_bit()?;
            value = (value << 1) | bit as u32;

            bits_to_read -= 1;
        }

        value += offset;

        Ok(value)
    }

    fn dequantize_errval(&self, errval: i32) -> i32 {
        if self.near > 0 {
            errval * (2 * self.near as i32 + 1)
        } else {
            errval
        }
    }

    fn decode_run_interruption_value(
        &mut self,
        ix: &mut u16,
        ra: i32,
        rb: i32,
        reader: &mut BitReader<Cursor<Vec<u8>>>,
    ) -> std::io::Result<()> {
        let ri_type = ra == rb || (ra - rb).abs() <= self.near as i32;
        let sign = if !ri_type && ra > rb { -1 } else { 1 };
        let px = if ri_type { ra } else { rb };

        let a = if ri_type {
            self.a[366] + (self.n[366] >> 1)
        } else {
            self.a[365]
        };
        let q = 365 + if ri_type { 1 } else { 0 };

        let k = self.compute_golomb(self.n[q] as u32, a as u32);

        let em_errval =
            self.decode_mapped_error_value(k, self.limit - self.j[self.run_index as usize] as u16 - 1, reader)?;
        let t_em_errval = em_errval + if ri_type { 1 } else { 0 };

        let mut errval = if t_em_errval == 0 {
            0
        } else if k == 0 {
            if 2 * self.nn[q - 365] < self.n[q] {
                if t_em_errval % 2 == 0 {
                    -((t_em_errval >> 1) as i32)
                } else {
                    ((t_em_errval + 1) >> 1) as i32
                }
            } else {
                if t_em_errval % 2 == 0 {
                    (t_em_errval >> 1) as i32
                } else {
                    -(((t_em_errval + 1) >> 1) as i32)
                }
            }
        } else {
            if t_em_errval % 2 == 0 {
                (t_em_errval >> 1) as i32
            } else {
                -(((t_em_errval + 1) >> 1) as i32)
            }
        };

        let update_errval = errval;

        if self.near > 0 {
            errval = self.dequantize_errval(errval);
        }

        if sign < 0 {
            errval = -errval;
        }

        let mut rx = px + errval;

        if rx < -(self.near as i32) {
            rx += self.range as i32 * (2 * self.near as i32 + 1);
        } else if rx > self.max_val as i32 + self.near as i32 {
            rx -= self.range as i32 * (2 * self.near as i32 + 1);
        }

        rx = self.correct_predicted_value(rx);

        *ix = rx as u16;

        if update_errval < 0 {
            self.nn[q - 365] += 1;
        };

        self.a[q] += ((em_errval + 1 - if ri_type { 1 } else { 0 }) >> 1) as i32;

        if self.n[q] == self.reset as i32 {
            self.a[q] = self.a[q] >> 1;
            self.n[q] = self.n[q] >> 1;
            self.nn[q - 365] = self.nn[q - 365] >> 1;
        }

        self.n[q] = self.n[q] + 1;

        Ok(())
    }

    fn quantize_gradient(&mut self, value: i32) -> i16 {
        if value <= -self.t3 {
            return -4;
        }
        if value <= -self.t2 {
            return -3;
        }
        if value <= -self.t1 {
            return -2;
        }
        if value <= -(self.near as i32) {
            return -1;
        }
        if value <= self.near as i32 {
            return 0;
        }
        if value <= self.t1 {
            return 1;
        }
        if value <= self.t2 {
            return 2;
        }
        if value <= self.t3 {
            return 3;
        }

        4
    }

    fn compute_px(&mut self, ra: u16, rb: u16, rc: u16, q: usize, sign: i32) -> i32 {
        let mut px;

        if rc >= std::cmp::max(ra, rb) {
            px = std::cmp::min(ra, rb) as i32
        } else if rc <= std::cmp::min(ra, rb) {
            px = std::cmp::max(ra, rb) as i32
        } else {
            px = (ra + rb - rc) as i32
        };

        px += px + if sign > 0 { self.c[q] } else { -self.c[q] };

        px
    }

    fn inverse_mapped_error_value(&mut self, m_errval: u32, k: u16, q: usize) -> i32 {
        if self.near == 0 && k == 0 && 2 * self.b[q] <= -self.n[q] {
            if m_errval % 2 != 0 {
                ((m_errval - 1) / 2) as i32
            } else {
                -(m_errval as i32) / 2 - 1
            }
        } else {
            if m_errval % 2 == 0 {
                (m_errval / 2) as i32
            } else {
                -((m_errval as i32 + 1) / 2)
            }
        }
    }

    fn update_variables(&mut self, q: usize, errval: i32) {
        self.b[q] = self.b[q] + errval * (2 * self.near + 1) as i32;
        self.a[q] = self.a[q] + errval.abs();

        if self.n[q] == self.reset as i32 {
            self.a[q] = self.a[q] >> 1;
            self.b[q] = self.b[q] >> 1;
            self.n[q] = self.n[q] >> 1;
        }

        self.n[q] = self.n[q] + 1;

        if self.b[q] <= -self.n[q] {
            self.b[q] += self.n[q];

            if self.c[q] > self.min_c as i32 {
                self.c[q] -= 1
            };
            if self.b[q] <= -self.n[q] {
                self.b[q] = -self.n[q] + 1
            };
        } else if self.b[q] > 0 {
            self.b[q] -= self.n[q];

            if self.c[q] < self.max_c as i32 {
                self.c[q] += 1
            };
            if self.b[q] > 0 {
                self.b[q] = 0
            };
        }
    }

    fn compute_q(&self, q1: i16, q2: i16, q3: i16) -> usize {
        (q1 * 81 + q2 * 9 + q3) as usize
    }

    pub fn decode(&mut self) -> VexelResult<Image> {
        while let Ok(marker) = self.reader.next_marker(&JPEG_LS_MARKERS) {
            match marker {
                Some(marker) => match marker {
                    JpegLsMarker::SOI => {
                        println!("Start of image");
                    }
                    JpegLsMarker::SOF55 => self.read_sof_info()?,
                    JpegLsMarker::SOS => {
                        self.read_sos_info()?;
                        self.init_default_parameters();
                        self.read_sos_bitstream()?;
                    }
                    JpegLsMarker::EOI => {
                        println!("End of image marker found");
                        break;
                    }
                    _ => {
                        println!("Unhandled marker found: {:?}", marker);
                    }
                },
                None => {
                    println!("No more markers found");
                    break;
                }
            }
        }

        let mut image = Vec::new();

        let row_a = vec![0u16; self.width as usize];
        let row_b = vec![0u16; self.width as usize];

        let mut this_row = row_a;
        let mut prev_row = row_b;

        for row in 0..self.height {
            let mut prev_ra_0 = 0u16;
            for mut col in 0..self.height {
                // We need to initialize ra, rb, rc, rd
                // Here's how situation looks like:
                //	c b d .
                //	a x . .
                //	. . . .

                let mut rx;

                let mut ra;
                let mut rb;
                let rc;
                let rd;

                if row > 0 {
                    rb = prev_row[col as usize];
                    rc = if col > 0 {
                        prev_row[(col - 1) as usize]
                    } else {
                        prev_ra_0
                    };
                    ra = if col > 0 {
                        this_row[(col - 1) as usize]
                    } else {
                        prev_ra_0 = rb;
                        prev_ra_0
                    };
                    rd = if col + 1 < self.width {
                        prev_row[(col + 1) as usize]
                    } else {
                        rb
                    };
                } else {
                    rb = 0;
                    rc = 0;
                    rd = 0;
                    ra = if col > 0 {
                        this_row[(col - 1) as usize]
                    } else {
                        prev_ra_0 = 0;
                        prev_ra_0
                    };
                }

                let (d1, d2, d3) = self.compute_local_gradients(ra, rb, rc, rd);

                let mut reader = BitReader::new(Cursor::new(self.data.clone()));

                if d1.abs() <= self.near as i32 && d2.abs() <= self.near as i32 && d3.abs() <= self.near as i32 {
                    // Run mode
                    loop {
                        let bit = reader.read_bit()?;

                        if bit {
                            let mut count = self.j[self.run_index as usize];

                            while count > 0 && col < self.width {
                                this_row[col as usize] = ra;
                                col += 1;
                                count -= 1;
                            }

                            if count == -1 && self.run_index < 31 {
                                self.run_index = self.run_index + 1;
                            }

                            if col >= self.width {
                                break;
                            }
                        } else {
                            let mut bits = self.j[self.run_index as usize];
                            let mut count = 0;

                            while bits > 0 {
                                let bit = reader.read_bit()?;

                                if !bit {
                                    break;
                                }

                                count = (count << 1) | bit as i32;
                                bits -= 1;
                            }

                            while count > 0 {
                                this_row[col as usize] = ra;
                                col += 1;
                                count -= 1;
                            }

                            if row > 0 {
                                rb = prev_row[col as usize];
                                ra = if col > 0 { this_row[(col - 1) as usize] } else { rb };
                            } else {
                                rb = 0;
                                ra = if col > 0 { this_row[(col - 1) as usize] } else { 0 };
                            }

                            self.decode_run_interruption_value(
                                &mut this_row[col as usize],
                                ra as i32,
                                rb as i32,
                                &mut reader,
                            )?;

                            if self.run_index > 0 {
                                self.run_index = self.run_index - 1;
                            }

                            break;
                        }
                    }
                } else {
                    // Regular mode
                    let mut q1 = self.quantize_gradient(d1);
                    let mut q2 = self.quantize_gradient(d2);
                    let mut q3 = self.quantize_gradient(d3);

                    let is_component_negative = q1 < 0 || (q1 == 0 && q2 < 0) || (q1 == 0 && q2 == 0 && q3 < 0);
                    let sign = match is_component_negative {
                        true => {
                            q1 = -q1;
                            q2 = -q2;
                            q3 = -q3;

                            -1
                        }
                        false => 1,
                    };

                    let q = self.compute_q(q1, q2, q3);

                    let mut px = self.compute_px(ra, rb, rc, q, sign);
                    px = self.correct_predicted_value(px);

                    let k = self.compute_golomb(self.n[q] as u32, self.a[q] as u32);

                    let m_errval = self.decode_mapped_error_value(k, self.limit, &mut reader)?;
                    let mut errval = self.inverse_mapped_error_value(m_errval, k, q);
                    let errval_copy = errval;

                    errval = self.dequantize_errval(errval);

                    if sign < 0 {
                        errval = -errval;
                    }

                    rx = px + errval;

                    if rx < -(self.near as i32) {
                        rx += self.range as i32 * (2 * self.near + 1) as i32
                    } else if rx > self.max_val as i32 + self.near as i32 {
                        rx -= self.range as i32 * (2 * self.near + 1) as i32
                    };

                    rx = self.correct_predicted_value(rx);

                    this_row[col as usize] = rx as u16;

                    self.update_variables(q, errval_copy);
                }
            }

            // TODO
            // Since we are decoding grayscale only for now, duplicate every pixel 3 times to get RGB
            let mut rgb_row = Vec::new();
            for i in 0..self.width {
                rgb_row.push(this_row[i as usize] as u8);
                rgb_row.push(this_row[i as usize] as u8);
                rgb_row.push(this_row[i as usize] as u8);
            }

            image.extend(rgb_row);

            (this_row, prev_row) = (prev_row, this_row);
        }

        Ok(Image::from_pixels(self.width, self.height, PixelData::RGB8(image)))
    }
}
