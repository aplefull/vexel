use serde::Serialize;
use tsify::Tsify;

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
pub enum HdrFormat {
    RGBE,
    XYZE,
}
