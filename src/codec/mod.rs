use crate::Grid;

pub mod analog;
pub mod audio;
pub mod binary;
pub(crate) mod common;
pub mod layered;
pub mod signed;
pub mod sliding;
pub mod xor;

/// A carrier frame for one of the supported codec families.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Binary(Grid<u8>),
    Signed(Grid<i8>),
    Analog(Grid<f32>),
}

/// Shared encoding configuration used by codec implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeConfig {
    pub n_frames: usize,
    pub seed: String,
}

impl EncodeConfig {
    pub fn new(n_frames: usize, seed: impl Into<String>) -> Self {
        Self {
            n_frames,
            seed: seed.into(),
        }
    }
}

/// Shared decode output for codecs that recover a QR grid.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeResult {
    pub qr: Grid<u8>,
    pub message: Option<String>,
}
