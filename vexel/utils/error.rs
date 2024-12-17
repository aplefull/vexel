use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum VexelError {
    IoError(io::Error),
    UnsupportedFormat(String),
    InvalidDimensions { width: u32, height: u32 },
    Custom(String),
}

impl Error for VexelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            VexelError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl Display for VexelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            VexelError::IoError(err) => write!(f, "I/O error: {}", err),
            VexelError::UnsupportedFormat(format) => write!(f, "Unsupported image format: {}", format),
            VexelError::InvalidDimensions { width, height } => {
                write!(f, "Invalid image dimensions: {}x{}", width, height)
            }
            VexelError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<io::Error> for VexelError {
    fn from(error: io::Error) -> Self {
        VexelError::IoError(error)
    }
}

impl From<String> for VexelError {
    fn from(error: String) -> Self {
        VexelError::Custom(error)
    }
}

impl From<&str> for VexelError {
    fn from(error: &str) -> Self {
        VexelError::Custom(error.to_string())
    }
}

impl From<ParseIntError> for VexelError {
    fn from(error: ParseIntError) -> Self {
        VexelError::Custom(error.to_string())
    }
}

pub type VexelResult<T> = Result<T, VexelError>;
