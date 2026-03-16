//! QR encoding and decoding primitives used by the steganographic codecs.
//!
//! This module exposes low-level pieces such as finite-field arithmetic,
//! Reed-Solomon coding, masking, and format handling in addition to the
//! high-level [`encode`] and [`decode`] entrypoints.

pub mod decode;
pub mod encode;
pub mod format;
pub mod gf256;
pub mod mask;
pub mod reed_solomon;
