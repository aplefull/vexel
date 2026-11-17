use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}
