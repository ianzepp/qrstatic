# EVALS

Eval notes for the temporal codec family.

This document is not a generic testing philosophy writeup. It is a concrete description of:

- which harnesses exist in this repository
- what each harness actually measures
- which sweeps have been run so far
- what those results imply for current defaults and acceptable operating ranges

The canonical design document is [`/Users/ianzepp/github/ianzepp/qrstatic/TEMPORAL.md`](/Users/ianzepp/github/ianzepp/qrstatic/TEMPORAL.md). This file is the measurement companion to it.

## Scope

Two eval harnesses currently matter:

- Stage 1 single-channel temporal eval:
  [`/Users/ianzepp/github/ianzepp/qrstatic/crates/qrstatic-cli/src/temporal_eval.rs`](/Users/ianzepp/github/ianzepp/qrstatic/crates/qrstatic-cli/src/temporal_eval.rs)
- Experimental tiled temporal eval:
  [`/Users/ianzepp/github/ianzepp/qrstatic/crates/qrstatic-cli/src/temporal_tiled_eval.rs`](/Users/ianzepp/github/ianzepp/qrstatic/crates/qrstatic-cli/src/temporal_tiled_eval.rs)

Their result tables are:

- [`/Users/ianzepp/github/ianzepp/qrstatic/temporal_results.tsv`](/Users/ianzepp/github/ianzepp/qrstatic/temporal_results.tsv)
- [`/Users/ianzepp/github/ianzepp/qrstatic/temporal_tiled_results.tsv`](/Users/ianzepp/github/ianzepp/qrstatic/temporal_tiled_results.tsv)

## Eval Principles

These evals are intended to answer four concrete questions:

1. Does the correct keyed decode succeed reliably?
2. Do wrong-key and wrong-window decodes fail closed?
3. Is the profile stronger than necessary, meaning it reveals too early or perturbs the carrier too much?
4. Where is the first plausible operating band, not just the strongest possible one?

That means the harnesses deliberately evaluate both success and over-strength. A profile that decodes perfectly but is too visually aggressive is not accepted as the default.

## Harness 1: `qrstatic-temporal-eval`

Purpose:

- evaluate the fixed-window Stage 1 Layer 1 temporal bootstrap channel
- measure detector separation between correct-key and false paths
- measure late-reveal acquisition timing with prefix-length sweeps

Primary command surface:

```bash
cargo run -p qrstatic-cli --bin qrstatic-temporal-eval -- --help
```

Important inputs:

- `--frames`
- `--noise-amplitude`
- `--l1-amplitude`
- `--threshold`
- `--trials`
- `--prefix-step`
- `--results-tsv`

What each trial does:

- encodes one QR payload with one master key
- decodes it with the correct key
- attempts decode with a wrong key
- attempts decode with a wrong accumulation window
- checks whether naive accumulation reveals a QR
- records detector scores for all four paths
- if `--prefix-step` is set, repeats the score/decode check on partial prefixes of the window

Core outputs:

- correct decode rate
- wrong-key decode rate
- wrong-window decode rate
- naive decode rate
- mean detector scores for each path
- mean margins between correct and false paths
- optional prefix acquisition table with `k50` and `k95`

### Stage 1 profiles swept

Representative profiles currently in [`/Users/ianzepp/github/ianzepp/qrstatic/temporal_results.tsv`](/Users/ianzepp/github/ianzepp/qrstatic/temporal_results.tsv):

- `baseline-64`
- `low-snr-64`
- `tough-64`
- `baseline-96`
- `low-snr-96`
- `baseline-128`
- `middle-64-a`

All of the earlier high-strength profiles were effectively perfect in the sampled threshold band:

- correct decode `100%`
- wrong-key `0%`
- wrong-window `0%`
- naive `0%`

That told us the early Stage 1 profiles were too strong to be useful for concealment tuning.

### Goldilocks acquisition sweep

The key Stage 1 decision was not made from the broad threshold table. It was made from the prefix acquisition sweep for `middle-64-a`.

Profile:

- `frames=64`
- `noise_amplitude=0.42`
- `l1_amplitude=0.22`
- `threshold=6.0`

Measured behavior:

- `0%` correct decode through `44/64`
- weak emergence around `48/64`
- `100%` correct decode by `52/64`
- `k50 = 52`
- `k95 = 52`

This profile became the Stage 1 default because it was the first one that landed in the intended middle ground:

- not visibly eager
- not fragile at full window
- still fails closed on wrong-key and wrong-window paths

### Stage 1 default and acceptable range

Current Stage 1 default:

- profile: `middle-64-a`
- `frames=64`
- `noise_amplitude=0.42`
- `l1_amplitude=0.22`
- `threshold=6.0`

Current acceptable interpretation:

- late reveal is preferred over maximal margin
- `k50` should happen late in the window, not near the midpoint
- full-window correct decode should still be near-certain
- wrong-key and wrong-window decode should remain zero in the sampled set

What would currently be considered too strong:

- profiles like `baseline-64`, `baseline-96`, and `baseline-128`, where the correct-vs-false score margin is much larger than needed for the current concealment goal

What would currently be considered too weak:

- a profile that pushes reveal later but materially harms full-window reliability or causes wrong-window ambiguity

## Harness 2: `qrstatic-temporal-tiled-eval`

Purpose:

- evaluate the experimental tiled temporal transport
- measure capacity versus reliability across frame geometry and QR tile version
- evaluate carrier-overlay perturbation, not just synthetic decode success
- evaluate post-process robustness via coarse quantization

Primary command surface:

```bash
cargo run -p qrstatic-cli --bin qrstatic-temporal-tiled-eval -- --help
```

Important inputs:

- `--width`
- `--height`
- `--qr-version`
- `--frames`
- `--l1-amplitude`
- `--threshold`
- `--data-shards`
- `--parity-shards`
- `--payload-bytes`
- `--carrier-profile`
- `--clip-limit`
- `--quantize-levels`
- `--results-tsv`

What each trial does:

- builds one tiled stream block with block metadata and payload
- encodes it either as synthetic standalone tiled frames or over a carrier frame sequence
- attempts correct decode
- attempts wrong-key decode
- attempts wrong-window decode
- records tile-level and group-level recovery counts
- if a carrier is used, records perturbation metrics against the carrier
- if quantization is enabled, quantizes the transmitted frames before decode and records quantization drift

Core outputs:

- block success rate
- wrong-key block success rate
- wrong-window block success rate
- mean tiles decoded
- mean groups recovered
- mean tile detector score
- layout and capacity metadata
- carrier artifact metrics:
  - mean absolute delta
  - max absolute delta
  - mean PSNR
- quantization drift metrics:
  - mean absolute delta from pre-quantized frames
  - max absolute delta from pre-quantized frames

## Tiled eval phases and results

### Phase A: geometry and capacity surface

Initial geometry sweeps compared:

- `638x464 @ v3`
- `640x480 @ v3`
- `660x495 @ v4`

Representative results:

- current `638x464 @ v3`: `22x16`, `350` active tiles, `22` shard bytes, `4550` max payload bytes
- current `640x480 @ v3`: same active tile count and capacity, but dead region `2x16`
- current `660x495 @ v4`: `20x15`, `300` active tiles, `32` shard bytes, `5660` max payload bytes

For comparison, the pre-refactor tiled transport had materially lower payload density:

- earlier `638x464 @ v3`: `13` shard bytes, `2726` max payload bytes
- earlier `660x495 @ v4`: `20` shard bytes, `3596` max payload bytes

So the geometry story stayed the same, but the payload budget improved substantially.

All three were perfect at the original high-strength overlay settings, so geometry alone was not the decision surface.

Decision impact:

- `638x464 @ v3` was the clean exact-fit comparison baseline
- `660x495 @ v4` became the throughput candidate because it preserved exact fit while materially increasing capacity
- after the transport refactor, that throughput advantage became substantially larger because raw byte QR payloads removed the earlier text-encoding overhead

### Phase B: carrier-overlay artifact sweep

Once the harness moved from synthetic tiled frames to carrier overlay, the earlier `l1=0.22` setting was shown to be too aggressive.

Measured on a motion carrier:

- `638x464 @ v3, l1=0.22`: mean absolute delta `0.221381`, PSNR `18.59 dB`
- `660x495 @ v4, l1=0.22`: mean absolute delta `0.200600`, PSNR `19.02 dB`

Those numbers were too large for a concealment-first operating point.

That led to the follow-up amplitude sweep.

For `638x464 @ v3`:

- earlier `l1=0.06`: `3/4` block success, PSNR `30.63 dB`
- current `l1=0.07`: `4/4` block success, mean tiles decoded `349.00 / 350`, PSNR `28.83 dB`

For `660x495 @ v4`:

- current `l1=0.22`: `4/4` block success, full tile/group recovery, PSNR `19.02 dB`
- current `l1=0.09`: `6/6` block success, full tile/group recovery, PSNR `26.29 dB`

Decision impact:

- `v3 / 0.07` was identified as the gentlest plausible concealment-first option
- `v4 / 0.09` was identified as the better throughput-oriented operating point with still-acceptable artifact pressure

### Phase C: quantization robustness sweep

After choosing `v4 / 0.09` as the working tiled default, quantization was added as a first crude post-process proxy.

Working profile:

- `660x495`
- `qr_version=4`
- `frames=64`
- `l1_amplitude=0.09`
- `threshold=2.5`
- `carrier_profile=motion`
- `data_shards=3`
- `parity_shards=2`
- `payload_bytes=512`

Quantization results:

- earlier `q128`: `8/8` block success, quantization delta `0.003937`, PSNR `26.88 dB`
- earlier `q64`: `8/8` block success, quantization delta `0.007933`, PSNR `26.90 dB`
- earlier `q32`: `4/4` block success, quantization delta `0.016123`, PSNR `26.71 dB`
- current `q16`: `4/4` block success, quantization delta `0.033184`, PSNR `26.44 dB`
- current `q8`: `4/4` block success, slight tile loss (`299.75 / 300`), quantization delta `0.070645`, PSNR `24.46 dB`
- current `q4`: `0/4` block success, tiles decoded `178.50 / 300`, groups recovered `38.00 / 60`, PSNR `19.38 dB`

Decision impact:

- the tiled default is robust through fairly coarse quantization
- `q16` still looks comfortably safe
- `q8` appears to be near the lower practical edge
- `q4` is too destructive and should be treated as beyond the acceptable range

Important caveat:

- quantization is only a crude proxy for video processing
- it is useful because it introduces coarse value collapse
- it is not equivalent to compression artifacts, resampling, block transforms, or platform transcode pipelines

## Current Defaults

### Stage 1 single-channel temporal default

Used for the bootstrap Layer 1 path:

- profile `middle-64-a`
- `41x41`
- `64` frames
- `noise_amplitude=0.42`
- `l1_amplitude=0.22`
- `threshold=6.0`

Reason:

- later reveal timing
- reliable full-window decode
- wrong-key and wrong-window failure remained clean in the sampled set

### Experimental tiled temporal default

Current working tiled default in the eval harness:

- profile `tiled-v4-balanced`
- `660x495`
- `qr_version=4`
- `64` frames
- `noise_amplitude=0.42`
- `l1_amplitude=0.09`
- `threshold=2.5`
- `data_shards=3`
- `parity_shards=2`
- `payload_bytes=512`
- `carrier_profile=motion`

Reason:

- materially higher payload capacity than `v3`
- exact tile fit
- acceptable artifact pressure compared to the earlier `0.22` setting
- clean block recovery on the sampled overlay runs
- robust to quantization down through at least `q16`, and still functional at `q8`
- after the transport refactor, materially higher usable payload density than the earlier tiled implementation while keeping the same measured recovery behavior in spot re-runs
- current measured layout: `300` active tiles, `32` shard bytes, `5660` max payload bytes

## Acceptable Values Right Now

These are current evidence-based working bands, not protocol guarantees.

### Stage 1 single-channel

Good:

- `middle-64-a`

Probably too strong for current concealment target:

- the earlier `baseline-*` family
- high-margin profiles that decode far earlier than intended

### Tiled temporal

Good throughput-oriented working profile:

- `660x495 @ v4`
- `l1_amplitude=0.09`
- `threshold=2.5`
- current measured layout after the transport refactor: `300` active tiles, `32` shard bytes, `5660` max payload bytes

Gentler concealment-oriented fallback:

- `638x464 @ v3`
- `l1_amplitude=0.07`
- `threshold=1.9`
- current measured layout: `350` active tiles, `22` shard bytes, `4550` max payload bytes

Likely too aggressive:

- tiled overlay `l1=0.22`

Likely too weak:

- `638x464 @ v3, l1=0.06`

Quantization floor:

- `q16` looks comfortable
- `q8` still works but is close enough to treat as edge behavior
- `q4` is not acceptable

## What These Evals Do Not Yet Prove

These harnesses are useful, but they do not yet justify stronger claims than the data supports.

Not yet proven:

- behavior under actual H.264 or platform transcode loops
- behavior under downsample and resample pipelines
- robustness to blur, ringing, and block artifacts from real video services
- real synchronization acquisition across long streams
- final production acceptability of the tiled architecture itself

So the current defaults should be read as:

- best measured working defaults inside the current synthetic and carrier-overlay harnesses
- not final production-certified parameters

## Next Eval Work

The next useful eval additions are:

1. Compression proxy evals
- downsample and resample
- blur
- block-style quantization or real transcode loops

2. Occupancy sweeps
- larger payloads closer to tiled capacity
- recovery behavior as shard pressure rises

3. Gutter sweeps
- no-gutter versus small tile gutters
- concealment and recovery tradeoff

4. Long-stream evals
- repeated block runs
- block loss and recovery across longer sequences

The current harnesses are already good enough to drive early parameter decisions. The next phase is to make them more honest about the real carrier pipeline.
