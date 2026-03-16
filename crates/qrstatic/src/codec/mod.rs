//! Codec families for hiding QR payloads inside deterministic noise frames.
//!
//! Each codec module defines its own encoder/decoder pair and any
//! codec-specific configuration or decode output types.

use crate::Grid;

pub mod analog;
pub mod audio;
pub mod binary;
pub(crate) mod common;
pub mod layered;
pub mod signed;
pub mod sliding;
pub mod temporal;
pub mod temporal_packet;
pub mod xor;

/// A carrier frame for one of the supported codec families.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// Binary frame with modules in `{0, 1}`.
    Binary(Grid<u8>),
    /// Signed frame with carrier cells in `{-1, 1}`.
    Signed(Grid<i8>),
    /// Floating-point frame carrying analog signal plus noise.
    Analog(Grid<f32>),
}

/// Shared encoding configuration used by codec implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeConfig {
    /// Number of carrier frames in one encode/decode window.
    pub n_frames: usize,
    /// Deterministic seed used to derive per-frame randomness.
    pub seed: String,
}

impl EncodeConfig {
    /// Construct a shared encoding configuration.
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
    /// Recovered QR grid.
    pub qr: Grid<u8>,
    /// Decoded QR message when QR decoding succeeds.
    pub message: Option<String>,
}
