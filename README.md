# qrstatic

Zero-dependency Rust crate for steganographic QR codes hidden in accumulated noise frames.

This project ports the ideas from [`qr-static-stream`](https://github.com/ianzepp/qr-static-stream) into a standalone Rust crate with no external runtime dependencies. It includes a hand-rolled QR encoder/decoder, deterministic noise generation, several carrier schemes, and streaming decoders that recover messages only after enough frames or samples have accumulated.

## What It Does

`qrstatic` takes a visible payload such as `"hello"` or `"https://example.com"`, encodes it as a QR grid, and then distributes that signal across sequences of frames that individually look like noise or ordinary carrier data. The message becomes recoverable only when the correct number of frames are accumulated or decoded with the matching algorithm.

The crate currently includes:

- A QR encoder/decoder for byte mode, EC level H, versions 1-6
- Deterministic SHA-256-seeded PRNG utilities
- Binary, signed, analog, layered, sliding-window, XOR, and audio codecs
- Batch and streaming encode/decode paths
- A hygiene ratchet and full automated test coverage for the implemented phases

## Codec Families

The crate implements several different hiding strategies:

- `xor`: simplest end-to-end validation path; XOR all frames to recover the QR grid
- `signed`: uses `{-1, 1}` carrier frames and recovers payload from expected-noise reconstruction
- `binary`: probability-biased binary static with deterministic per-frame randomness
- `analog`: floating-point signal plus noise where useful signal grows linearly with frame count
- `layered`: two-layer recursive scheme with an outer visible layer and a deeper hidden layer
- `sliding`: overlapping windows so there is no obvious hard message boundary
- `audio`: maps audio samples into virtual 2D frames and recovers QR from accumulated sign bias

## Project Status

The implementation plan in [PLAN.md](/Users/ianzepp/github/ianzepp/qrstatic/PLAN.md) is complete through Phase 10.

Validated locally on March 15, 2026:

- `cargo doc --no-deps`
- `cargo build`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

Current test counts at that validation point:

- 186 passing functional tests
- 6 passing hygiene tests

## Installation

Add the crate as a dependency once it is published, or depend on the repository directly during development.

```toml
[dependencies]
qrstatic = { git = "https://github.com/ianzepp/qrstatic" }
```

Clone and validate locally:

```bash
git clone git@github.com:ianzepp/qrstatic.git
cd qrstatic
cargo test
```

## Quick Start

### Direct QR Encoding

If you only want the QR layer:

```rust
use qrstatic::qr;

let grid = qr::encode::encode("hello from qrstatic")?;
let decoded = qr::decode::decode(&grid)?;
assert_eq!(decoded, "hello from qrstatic");
# Ok::<(), qrstatic::Error>(())
```

### Binary Static Codec

Batch encode a QR message plus payload into biased binary static, then decode it again:

```rust
use qrstatic::codec::binary::{BinaryDecoder, BinaryEncoder};

let encoder = BinaryEncoder::new(60, (41, 41), "carrier-seed", 0.8, 0.1)?;
let frames = encoder.encode_message("visible-key", b"hidden payload")?;

let decoded = BinaryDecoder::new("hidden payload".len(), 0.8)?.decode_message(&frames)?;
assert_eq!(decoded.message.as_deref(), Some("visible-key"));
assert_eq!(decoded.payload.as_deref(), Some(&b"hidden payload"[..]));
# Ok::<(), qrstatic::Error>(())
```

### Audio Codec

Bias audio sample signs toward a QR pattern and recover the message after enough samples:

```rust
use qrstatic::codec::audio::{AudioConfig, AudioDecoder, AudioEncoder};

let config = AudioConfig::new(60, 64 * 64, 0.4, "audio-seed");
let mut rng = qrstatic::Prng::from_str_seed("cover");
let cover: Vec<f32> = (0..config.n_frames * config.frame_size)
    .map(|_| rng.next_range(-1.0, 1.0))
    .collect();

let encoded = AudioEncoder::new(config.clone())?.encode_samples(&cover, "audio-key")?;
let decoded = AudioDecoder::new(config)?.decode_samples(&encoded)?;
assert_eq!(decoded.message.as_deref(), Some("audio-key"));
# Ok::<(), qrstatic::Error>(())
```

## Design Constraints

This crate is intentionally narrow:

- No camera/image-processing pipeline
- No dependency on OpenCV, ffmpeg, or external QR libraries
- QR decoding is optimized for grids produced by this project, not arbitrary photographed QR codes
- The focus is deterministic encode/decode behavior and reproducible steganographic experiments

## Repository Layout

```text
src/
  lib.rs
  error.rs
  grid.rs
  sha256.rs
  prng.rs
  bits.rs
  qr/
    encode.rs
    decode.rs
    gf256.rs
    reed_solomon.rs
    mask.rs
    format.rs
  codec/
    xor.rs
    signed.rs
    binary.rs
    analog.rs
    layered.rs
    sliding.rs
    audio.rs
tests/
  codec_*.rs
  hygiene.rs
```

## Notes

- The Rust crate extends the upstream concept with a broader codec surface, including the audio path.
- The public API is documented well enough for `cargo doc --no-deps` to build cleanly.
- The current repository is best treated as a library and reference implementation rather than an end-user application.

## Origin

This repository is based on the ideas and structure of [`qr-static-stream`](https://github.com/ianzepp/qr-static-stream), but the implementation here is Rust-specific and rewritten for this codebase.
