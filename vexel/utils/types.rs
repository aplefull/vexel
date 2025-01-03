use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}
