# qrstatic — Build Plan

Zero-dependency Rust crate for steganographic QR codes hidden in accumulated noise frames.
Ported from [qr-static-stream](https://github.com/ianzepp/qr-static-stream) (Python).

## Architecture

```
qrstatic/
  src/
    lib.rs                    # Public re-exports
    error.rs                  # Error enum, Result alias
    grid.rs                   # Grid<T> — 2D container over Vec<T>
    sha256.rs                 # Hand-rolled SHA-256
    prng.rs                   # Xoshiro256** seeded PRNG
    bits.rs                   # Bit pack/unpack, majority voting
    qr/
      mod.rs                  # Re-exports
      gf256.rs                # GF(256) field arithmetic
      reed_solomon.rs         # RS encoder/decoder
      encode.rs               # QR encoder (byte mode, EC-H, v1-6)
      decode.rs               # QR decoder (own programmatic output only)
      mask.rs                 # 8 mask patterns + penalty scoring
      format.rs               # Format/version info encoding
    codec/
      mod.rs                  # Frame enum, shared types, traits
      xor.rs                  # Binary XOR
      analog.rs               # Analog grayscale + payload in magnitude
      binary.rs               # Binary static, probability-biased
      signed.rs               # Signed accumulation + noise reconstruction
      layered.rs              # Two-layer recursive (L1/L2)
      sliding.rs              # Sliding window + L2 overlay
      audio.rs                # Audio steganography
```

## Phases

### Phase 1 — Foundational Primitives

Build the zero-dep infrastructure that everything else depends on.

**Files:**
- `src/error.rs` — `Error` enum with variants for QR, codec, and grid errors. `Result<T>` alias.
- `src/grid.rs` — `Grid<T>` 2D container. Row-major `Vec<T>` with `width`/`height`. Index by `(row, col)`. Methods: `new`, `from_vec`, `get`, `get_mut`, `map`, `zip_with`, `accumulate_from`. Index trait impls.
- `src/sha256.rs` — Single-shot `sha256(input: &[u8]) -> [u8; 32]`. FIPS 180-4. Tested against NIST vectors.
- `src/prng.rs` — `Prng` struct wrapping xoshiro256**. `from_seed([u8; 32])`, `from_key(key, index)` (SHA-256 hashes `"key:index"` to produce seed). Methods: `next_u64`, `next_f32` in [0,1), `next_bool(p)`, `next_range(lo, hi)`.
- `src/bits.rs` — `bytes_to_bits`, `bits_to_bytes`, `majority_vote` (given repeated samples per bit, vote on each bit value).
- `src/lib.rs` — Declare modules, public re-exports.

**Tests:**
- Grid: indexing, out-of-bounds, map, zip_with, accumulate, non-square grids, 1x1 edge case.
- SHA-256: NIST test vectors ("abc", empty string, "abcdbcdecdefdefg..." 56-byte vector).
- PRNG: determinism (same seed → same sequence), `from_key` determinism, `next_f32` range [0,1), `next_bool` distribution over 10k samples.
- Bits: roundtrip pack/unpack, majority vote with clean signal, majority vote with noise.

### Phase 2 — QR Codec

Hand-rolled QR encoder and decoder. Byte mode only, EC level H, versions 1-6.
The decoder only handles our own programmatic output (known grid, no camera/image processing).

**Files:**
- `src/qr/gf256.rs` — GF(256) with primitive polynomial 0x11d. `mul`, `div`, `pow`, `log`/`exp` tables. Generator polynomial computation.
- `src/qr/reed_solomon.rs` — `encode(data, n_ec_codewords) -> ec_bytes`. `decode(data_with_ec, n_ec_codewords) -> corrected_data`. Syndrome computation, error correction via Berlekamp-Massey or Euclidean algorithm, Forney for error values.
- `src/qr/mask.rs` — 8 mask pattern functions `(row, col) -> bool`. `evaluate_penalty(grid) -> u32` implementing all 4 penalty rules. `best_mask(grid) -> u8`.
- `src/qr/format.rs` — Format info: 15-bit BCH code encoding EC level + mask pattern. Encode and decode.
- `src/qr/encode.rs` — `QrCode::encode(data: &str) -> Result<QrCode>`. Byte-mode encoding, version selection (smallest version 1-6 that fits at EC-H), data interleaving, module placement (finders, timing, alignment, format info, data). Output: `Grid<u8>` where 0=white, 1=black.
- `src/qr/decode.rs` — `QrCode::decode(grid: &Grid<u8>) -> Result<String>`. Read format info, determine mask, read data modules, de-interleave, RS decode, extract byte-mode payload. Handles up to 30% module errors via EC-H.
- `src/qr/mod.rs` — Re-exports `QrCode`.

**Tests:**
- GF(256): multiply/divide consistency, exp/log roundtrip, known multiplication results.
- Reed-Solomon: encode then decode with zero errors, with 1 error, with max correctable errors, with too many errors (should fail).
- Mask: each mask pattern produces expected bits at known coordinates.
- Format: encode then decode roundtrip for all 8 mask × 1 EC level combinations.
- QR encode: known strings produce valid QR grids. Version selection is minimal.
- QR decode: roundtrip — encode then decode recovers original string.
- QR decode with noise: flip up to 20% of modules, decode still succeeds.

### Phase 3 — XOR Codec (First End-to-End Validation)

The simplest encoding approach. Validates the entire pipeline: string → QR → frames → decode → QR → string.

**Files:**
- `src/codec/mod.rs` — `Frame` enum (`Binary(Grid<u8>)`, `Signed(Grid<i8>)`, `Analog(Grid<f32>)`). Shared `EncodeConfig` and `DecodeResult` types.
- `src/codec/xor.rs` — `XorEncoder`: generate N frames where XOR of all frames = QR grid. First N-1 are random binary, last frame is computed. `XorDecoder`: XOR all frames to recover grid. `XorStreamEncoder` / `XorStreamDecoder` for frame-by-frame operation.

**Tests:**
- Roundtrip: encode "hello" → N frames → decode → "hello" for N = 2, 8, 64.
- Determinism: same seed produces same frames.
- Stream roundtrip: stream-encode then stream-decode.
- Wrong frame count: partial frames do not produce valid QR.
- Random frames (no encoding): XOR does not produce valid QR.

### Phase 4 — Signed Codec

Signed accumulation with payload encoding via expected noise reconstruction.

**Files:**
- `src/codec/signed.rs` — `SignedEncoder`: N frames of ±1 values. Final frame forces correct QR sign + magnitude bias for payload. `SignedDecoder`: accumulate frames, extract QR from sign, extract payload by subtracting expected noise accumulation and reading residual signs. Streaming wrappers.

**Tests:**
- QR-only roundtrip (no payload).
- QR + payload roundtrip.
- Different signal strengths.
- Minimum frame count (4).
- Payload at maximum capacity.

### Phase 5 — Binary Static Codec

Probability-biased binary frames. Per-frame deterministic RNG via SHA-256.

**Files:**
- `src/codec/binary.rs` — `BinaryEncoder`: each frame is +1/-1 with probability bias toward QR pattern. Payload encoded via bias strength modulation. `BinaryDecoder`: accumulate, threshold at 0, majority vote for payload. Streaming wrappers.

**Tests:**
- QR-only roundtrip.
- QR + payload roundtrip.
- Different bias values.
- Streaming roundtrip.
- 60-frame default window.

### Phase 6 — Analog Codec

Float32 frames with signal accumulating linearly, noise as √N.

**Files:**
- `src/codec/analog.rs` — `AnalogEncoder`: float32 frames with configurable signal strength and noise amplitude. QR in accumulated sign, payload in magnitude deviation. `AnalogDecoder`: accumulate, threshold, majority vote on magnitude. Streaming wrappers.

**Tests:**
- QR-only roundtrip.
- QR + payload roundtrip.
- SNR improves with more frames.
- Different signal/noise parameters.
- Streaming roundtrip.

### Phase 7 — Two-Layer Recursive Codec

N1 frames per L1 output, N2 L1 outputs per L2 output. Hierarchical steganography.

**Files:**
- `src/codec/layered.rs` — `LayeredEncoder`: generates N1×N2 carrier frames. L1 QR in sign, L2 QR + payload in magnitude deviations across L1 outputs. `LayeredDecoder`: group into N1 chunks, accumulate each for L1, then accumulate L1 magnitude deviations for L2. Streaming wrappers.

**Tests:**
- L1-only roundtrip.
- L1 + L2 roundtrip (QR content at both layers).
- L1 + L2 + payload roundtrip.
- Default N1=30, N2=30 (900 total frames).
- Streaming: L1 decodes every N1 frames, L2 decodes every N1×N2 frames.

### Phase 8 — Sliding Window Codec

Overlapping windows, no detectable boundaries. Most sophisticated approach.

**Files:**
- `src/codec/sliding.rs` — `SlidingEncoder`: L1 with overlapping windows (stride < N1), L2 overlay spread across N1×N2 frames. `SlidingDecoder`: can lock on at any offset, decode L1 from any N1 consecutive frames, decode L2 from N2 L1 outputs. Streaming wrappers.

**Tests:**
- L1 roundtrip at offset 0.
- L1 roundtrip at arbitrary offset (proves no boundary detection needed).
- L1 + L2 roundtrip.
- Different stride values.
- Streaming: L1 decoded periodically, L2 decoded after enough L1 outputs.

### Phase 9 — Audio Codec

Maps audio samples to virtual 2D frames via sign biasing.

**Files:**
- `src/codec/audio.rs` — `AudioEncoder`: bias audio sample signs toward QR pattern. Each sample maps to a QR module via `sample_index % frame_size`. `AudioDecoder`: accumulate N frames worth of samples, reshape to 2D, extract QR from accumulated sign. Streaming wrappers.

**Tests:**
- Roundtrip with synthetic samples.
- Different frame sizes (4096 default).
- Streaming: sample-by-sample and chunk-based.
- QR emerges after N frames of samples.

### Phase 10 — Polish

- `src/lib.rs` — Clean public API, crate-level doc comment.
- Final `cargo fmt`, `cargo clippy`, `cargo test`.
- Ensure all public types have doc comments.
- Verify `cargo doc` builds cleanly.
