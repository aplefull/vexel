use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum PgxByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PgxHeaderData {
    pub width: u32,
    pub height: u32,
    pub precision: u8,
    pub signed: bool,
    pub byte_order: PgxByteOrder,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PgxPixelDataInfo {
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum PgxSectionData {
    Header(PgxHeaderData),
    PixelData(PgxPixelDataInfo),
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PgxSectionInfo {
    pub start_offset: u64,
    pub data: PgxSectionData,
}
