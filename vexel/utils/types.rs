use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}
