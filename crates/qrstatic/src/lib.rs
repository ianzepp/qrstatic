//! `qrstatic` is a zero-dependency Rust crate for encoding QR content into
//! deterministic noise-like frame sequences and recovering it by accumulation.
//!
//! The crate is split into three layers:
//! - [`qr`] provides the hand-rolled QR encoder/decoder and supporting math.
//! - [`codec`] provides several steganographic carrier schemes built on top of QR grids.
//! - Core utilities such as [`Grid`], [`Prng`], [`sha256`], and [`bits`] support both layers.
//!
//! For direct QR work, use [`qr::encode::encode`] and [`qr::decode::decode`].
//! For end-to-end steganographic experiments, start with the codec modules such as
//! [`codec::xor`] or [`codec::binary`].

pub mod bits;
pub mod codec;
pub mod error;
pub mod grid;
pub mod prng;
pub mod qr;
pub mod sha256;

/// Common analog codec decode output.
pub use codec::analog::AnalogDecodeResult;
/// Common audio codec decode output.
pub use codec::audio::AudioDecodeResult;
/// Common binary codec decode output and default stream window.
pub use codec::binary::{BinaryDecodeResult, DEFAULT_BINARY_WINDOW};
/// Common layered codec decode output.
pub use codec::layered::LayeredDecodeResult;
/// Common signed codec decode output.
pub use codec::signed::SignedDecodeResult;
/// Common sliding codec decode output.
pub use codec::sliding::SlidingDecodeResult;
/// Common temporal codec decode output.
pub use codec::temporal::{TemporalDecodeResult, TemporalLayer2Config, TemporalLayer2DecodeResult};
/// Common temporal packet-layer types.
pub use codec::temporal_packet::{TemporalPacket, TemporalPacketProfile};
/// Common tiled temporal codec types.
pub use codec::temporal_tiled::{
    TiledConfig, TiledDecodeResult, TiledDecoder, TiledEncoder, TiledStreamBlock,
    TiledStreamBlockHeader,
};
/// Shared codec carrier and result types.
pub use codec::{DecodeResult, EncodeConfig, Frame};
/// Error and result aliases used across the crate.
pub use error::{Error, Result};
/// The crate's generic row-major 2D grid container.
pub use grid::Grid;
/// Deterministic pseudo-random number generator used for frame synthesis.
pub use prng::Prng;
