use serde::Serialize;
use tsify::Tsify;

pub const EXR_MAGIC: [u8; 4] = [0x76, 0x2F, 0x31, 0x01];

pub const VERSION_FLAG_TILED: i32 = 0x200;
pub const VERSION_FLAG_DEEP: i32 = 0x800;
pub const VERSION_FLAG_MULTIPART: i32 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExrPixelType {
    Uint,
    Half,
    Float,
}

impl ExrPixelType {
    pub fn from_i32(value: i32) -> Option<ExrPixelType> {
        match value {
            0 => Some(ExrPixelType::Uint),
            1 => Some(ExrPixelType::Half),
            2 => Some(ExrPixelType::Float),
            _ => None,
        }
    }

    pub fn bytes_per_element(&self) -> usize {
        match self {
            ExrPixelType::Uint => 4,
            ExrPixelType::Half => 2,
            ExrPixelType::Float => 4,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ExrPixelType::Uint => "uint",
            ExrPixelType::Half => "half",
            ExrPixelType::Float => "float",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExrCompression {
    None,
    Rle,
    Zips,
    Zip,
    Piz,
    Pxr24,
    B44,
    B44a,
    Dwaa,
    Dwab,
    Htj2k256,
    Htj2k32,
    Unknown(u8),
}

impl ExrCompression {
    pub fn from_u8(value: u8) -> ExrCompression {
        match value {
            0 => ExrCompression::None,
            1 => ExrCompression::Rle,
            2 => ExrCompression::Zips,
            3 => ExrCompression::Zip,
            4 => ExrCompression::Piz,
            5 => ExrCompression::Pxr24,
            6 => ExrCompression::B44,
            7 => ExrCompression::B44a,
            8 => ExrCompression::Dwaa,
            9 => ExrCompression::Dwab,
            10 => ExrCompression::Htj2k256,
            11 => ExrCompression::Htj2k32,
            other => ExrCompression::Unknown(other),
        }
    }

    pub fn name(&self) -> String {
        match self {
            ExrCompression::None => "none".to_string(),
            ExrCompression::Rle => "rle".to_string(),
            ExrCompression::Zips => "zips".to_string(),
            ExrCompression::Zip => "zip".to_string(),
            ExrCompression::Piz => "piz".to_string(),
            ExrCompression::Pxr24 => "pxr24".to_string(),
            ExrCompression::B44 => "b44".to_string(),
            ExrCompression::B44a => "b44a".to_string(),
            ExrCompression::Dwaa => "dwaa".to_string(),
            ExrCompression::Dwab => "dwab".to_string(),
            ExrCompression::Htj2k256 => "htj2k256".to_string(),
            ExrCompression::Htj2k32 => "htj2k32".to_string(),
            ExrCompression::Unknown(v) => format!("unknown({})", v),
        }
    }

    pub fn lines_per_block(&self) -> u32 {
        match self {
            ExrCompression::None | ExrCompression::Rle | ExrCompression::Zips => 1,
            ExrCompression::Zip | ExrCompression::Pxr24 => 16,
            ExrCompression::Piz
            | ExrCompression::B44
            | ExrCompression::B44a
            | ExrCompression::Dwaa
            | ExrCompression::Htj2k32 => 32,
            ExrCompression::Dwab | ExrCompression::Htj2k256 => 256,
            ExrCompression::Unknown(_) => 1,
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            ExrCompression::None
                | ExrCompression::Rle
                | ExrCompression::Zips
                | ExrCompression::Zip
                | ExrCompression::Piz
                | ExrCompression::Pxr24
                | ExrCompression::B44
                | ExrCompression::B44a
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOrder {
    IncreasingY,
    DecreasingY,
    RandomY,
}

impl LineOrder {
    pub fn from_u8(value: u8) -> LineOrder {
        match value {
            1 => LineOrder::DecreasingY,
            2 => LineOrder::RandomY,
            _ => LineOrder::IncreasingY,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            LineOrder::IncreasingY => "increasing y",
            LineOrder::DecreasingY => "decreasing y",
            LineOrder::RandomY => "random y",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelMode {
    OneLevel,
    Mipmap,
    Ripmap,
}

#[derive(Debug, Clone, Copy)]
pub struct TileDesc {
    pub x_size: u32,
    pub y_size: u32,
    pub level_mode: LevelMode,
    pub round_up: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Scanline,
    Tiled,
    DeepScanline,
    DeepTiled,
}

impl PartKind {
    pub fn from_type_string(value: &str) -> Option<PartKind> {
        match value {
            "scanlineimage" => Some(PartKind::Scanline),
            "tiledimage" => Some(PartKind::Tiled),
            "deepscanline" => Some(PartKind::DeepScanline),
            "deeptile" => Some(PartKind::DeepTiled),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            PartKind::Scanline => "scanlineimage",
            PartKind::Tiled => "tiledimage",
            PartKind::DeepScanline => "deepscanline",
            PartKind::DeepTiled => "deeptile",
        }
    }

    pub fn is_deep(&self) -> bool {
        matches!(self, PartKind::DeepScanline | PartKind::DeepTiled)
    }

    pub fn is_tiled(&self) -> bool {
        matches!(self, PartKind::Tiled | PartKind::DeepTiled)
    }
}

#[derive(Debug, Clone)]
pub struct ExrChannel {
    pub name: String,
    pub pixel_type: ExrPixelType,
    pub p_linear: bool,
    pub x_sampling: i32,
    pub y_sampling: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct Box2i {
    pub x_min: i32,
    pub y_min: i32,
    pub x_max: i32,
    pub y_max: i32,
}

impl Box2i {
    pub fn width(&self) -> i64 {
        self.x_max as i64 - self.x_min as i64 + 1
    }

    pub fn height(&self) -> i64 {
        self.y_max as i64 - self.y_min as i64 + 1
    }
}

#[derive(Debug, Clone)]
pub struct PartHeader {
    pub name: Option<String>,
    pub kind: PartKind,
    pub channels: Vec<ExrChannel>,
    pub compression: ExrCompression,
    pub data_window: Box2i,
    pub display_window: Box2i,
    pub line_order: LineOrder,
    pub tiles: Option<TileDesc>,
    pub chunk_count: Option<u32>,
    pub chromaticities: Option<[f32; 8]>,
    pub attributes: Vec<ExrAttributeInfo>,
}

#[derive(Debug, Clone)]
pub struct ChunkChannel {
    pub width: usize,
    pub rows: usize,
    pub bpe: usize,
    pub pixel_type: ExrPixelType,
    pub p_linear: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrAttributeInfo {
    pub name: String,
    pub attr_type: String,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrChannelInfo {
    pub name: String,
    pub pixel_type: String,
    pub x_sampling: i32,
    pub y_sampling: i32,
    pub p_linear: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExrPartInfo {
    pub name: Option<String>,
    pub part_type: String,
    pub compression: String,
    pub width: u32,
    pub height: u32,
    pub data_window: [i32; 4],
    pub display_window: [i32; 4],
    pub line_order: String,
    pub tile_size: Option<[u32; 2]>,
    pub chunk_count: u32,
    pub channels: Vec<ExrChannelInfo>,
    pub attributes: Vec<ExrAttributeInfo>,
}
