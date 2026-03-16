use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Data too large for the maximum supported QR version.
    DataTooLarge { len: usize, max: usize },
    /// Grid dimensions do not match the expected size.
    GridMismatch { expected: usize, actual: usize },
    /// QR decoding failed (too many errors, invalid structure, etc.).
    QrDecode(String),
    /// Codec error (wrong frame count, invalid parameters, etc.).
    Codec(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DataTooLarge { len, max } => {
                write!(f, "data length {len} exceeds maximum {max}")
            }
            Error::GridMismatch { expected, actual } => {
                write!(f, "grid size mismatch: expected {expected}, got {actual}")
            }
            Error::QrDecode(msg) => write!(f, "QR decode error: {msg}"),
            Error::Codec(msg) => write!(f, "codec error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
