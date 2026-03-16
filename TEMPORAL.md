# TEMPORAL

Production codec design for visually static steganographic transport.

## Status

This document defines the intended production codec for `qrstatic`.

Current repository status:

- `temporal` Stage 1 Layer 1 exists in code
- it is still a fixed-window bootstrap implementation, not the full production stack
- packet/FEC Layer 2 is not implemented yet
- the debug viewer and eval tooling now target `temporal`, not the old reference codecs

All existing codecs in this repository should be treated as experimental or reference designs:

- `xor`
- `signed`
- `binary`
- `analog`
- `layered`
- `sliding`
- `audio`

They informed this design, but they do not satisfy the production visual concealment requirement.

## Problem

The production goal is not "support many codecs." The production goal is:

- individual frames should look like ordinary static
- naive frame inspection should not reveal a QR code or payload structure
- naive accumulation should not be the intended decode path
- the correct keyed temporal reconstruction should reveal Layer 1 reliably
- Layer 2 should carry useful payload data above Layer 1
- large structured payloads should not force a custom media transport protocol inside the codec

The key design correction is this:

- the signal must live in keyed temporal correlation, not in the visible per-frame mean image

## Non-Goals

`temporal` is not trying to be:

- a new media container format
- a new file transfer protocol
- a general-purpose video codec
- a cryptographically complete steganography scheme by itself
- a peer-to-peer transport

The codec is the steganographic physical layer. Framing, packetization, FEC, and payload container concerns should reuse standard well-known designs whenever possible.

## Research Basis

This design is now grounded in three established bodies of work rather than only in-house codec experiments:

- direct-sequence spread spectrum (DSSS) acquisition and detection
- spread-spectrum watermarking in host media
- standard Reed-Solomon packet/block error correction

The design consequences are specific:

- Layer 1 should be implemented as a DSSS-like matched-filter problem, not as plain frame summation
- the frame field should be treated as host media carrying many weak hidden contributions, as in spread-spectrum watermarking
- Stage 2 should start from standard Reed-Solomon packet groups rather than bespoke parity logic

This document cites the following source material:

- Milica Stojanovic et al., "Code acquisition for DS/SS communications over time-varying multipath channels"
- Ingemar Cox et al., "Secure spread spectrum watermarking for multimedia"
- NASA JPL, "Tutorial on Reed-Solomon error correction coding"

## Production Invariants

The codec is only acceptable if all of the following hold:

1. Any individual frame is visually plausible as static.
2. The QR is not obviously visible in any individual frame.
3. Naive accumulation is not the intended recovery path and should not cleanly reveal Layer 1.
4. The correct keyed temporal correlator should recover Layer 1 reliably.
5. Layer 2 should not be meaningfully recoverable before Layer 1 reconstruction and subtraction.
6. Wrong key material or wrong accumulation window should fail closed.
7. The system should support large payloads without inventing a custom application-layer transport.

## Layering

`temporal` is a 3-layer design.

### Layer A: Stego Physical Layer

This is the actual codec.

Responsibilities:

- synthesize static-looking frames
- embed a hidden signal using keyed temporal coding
- recover a hidden signal using keyed temporal correlation
- produce a detector output whose correct-key response is measurably separated from wrong-key responses

This layer should not know:

- video semantics
- file structure semantics
- stream dependency graphs
- application-specific chunk meaning

### Layer B: Packet and FEC Layer

This is a packetization and recovery layer carried inside Layer 2.

Responsibilities:

- packet identity
- out-of-order reassembly
- integrity checks
- erasure correction or parity grouping

This layer should reuse standard packet framing and standard FEC concepts rather than inventing bespoke protocol semantics.

### Layer C: Payload Container Layer

This is the application-facing format carried over the packet layer.

Examples:

- small opaque payloads: framed records such as CBOR or protobuf messages
- large media payloads: fragmented or independently decodable media/container units

This layer owns application semantics. `temporal` does not.

## High-Level Model

Each frame is generated as:

```text
frame_t = noise_t + l1_t + l2_t
```

Where:

- `noise_t` is visually static background noise
- `l1_t` is the Layer 1 contribution
- `l2_t` is the Layer 2 contribution

The critical rule is:

- `l1_t` and `l2_t` must be temporally balanced so they do not show up strongly in any individual frame

Recovery uses keyed matched filtering, not plain summation. This follows the DSSS and spread-spectrum watermarking result that many weak contributions can be concentrated into a strong detector response only when the receiver knows the correct schedule and test sequence.

## Current Stage 1 Implementation

The current implemented Stage 1 in the repository does the following:

- uses `Grid<f32>` carrier frames
- uses fixed even-length decode windows
- uses balanced pseudorandom `+1/-1` temporal schedules
- uses per-frame keyed spatial permutation before emission so raw frames do not carry a stable centered QR layout
- uses keyed matched filtering to reconstruct a logical correlation field
- computes a detector score from the correlation field
- gates QR recovery on an explicit detector threshold

What it does not do yet:

- Layer 2 payload packets
- packet FEC
- blind synchronization or sliding acquisition
- threshold calibration from large empirical sweeps
- richer detector statistics than the current scalar score

This matters because the design is no longer only a proposal. Stage 1 behavior should now be described in terms that match the implemented contract.

## Layer 1

Layer 1 carries a QR code.

The QR payload is not intended to carry the entire user payload. It acts as a bootstrap and locator for Layer 2.

Recommended Layer 1 content:

- codec version
- key/session locator
- payload container type
- packet layer profile ID
- accumulation window profile or hint
- optional short manifest digest or locator

Layer 1 should be recoverable through keyed temporal correlation across the correct window.

### Layer 1 Signal Model

Let:

- `q(x, y)` be the logical QR symbol at position `(x, y)`, mapped to `{+1, -1}`
- `c1_t(x, y)` be the Layer 1 temporal code for that position and frame
- `a1` be the Layer 1 amplitude

Then:

```text
l1_t(x, y) = a1 * q(x, y) * c1_t(x, y)
```

Constraints:

- `c1_t(x, y)` is deterministic from key material
- `c1_t(x, y)` is balanced over the decode window
- per-frame visible expectation should remain near zero

Recovery:

```text
L1(x, y) = sum over t of frame_t(x, y) * c1_t(x, y)
```

After correlation, threshold or normalize `L1` into a QR grid and decode it.

### Stage 1 Contract

The current Stage 1 contract should be treated as:

1. encode a fixed window using a temporal key and Layer 1 QR payload
2. correlate a candidate frame window using that same temporal key
3. compute a detector score from the correlation field
4. reject the window if the detector score does not meet policy threshold
5. only then attempt QR extraction and QR decoding

This is an intentionally smaller and sharper contract than "always try QR decode and see what happens."

The codec primitive is the correlation field plus detector score. QR recovery is a consumer of that primitive.

## Layer 2

Layer 2 carries payload packets, not arbitrary application semantics.

Layer 2 is decoded only after:

1. Layer 1 recovery
2. expected Layer 1 reconstruction
3. Layer 1 subtraction or residual estimation

### Layer 2 Signal Model

Let:

- `b_k` be a packet-layer symbol mapped to `{+1, -1}` or a soft analog symbol
- `c2_t(x, y, k)` be the Layer 2 temporal code
- `a2` be the Layer 2 amplitude, with `a2 < a1`

Then:

```text
l2_t(x, y) = sum over selected payload symbols of a2 * b_k * c2_t(x, y, k)
```

Layer 2 should use:

- whitening or encryption before embedding
- spreading across many cells and frames
- interleaving
- packet-level integrity checks
- packet-block FEC

The codec should not carry large user payloads as a single monolithic hidden byte string.

## Temporal Codes

Temporal codes are the heart of the codec.

Requirements:

- deterministic from key material
- balanced over the intended decode window
- low visible single-frame bias
- low accidental correlation under wrong keys or wrong windows

Candidate code families:

- pseudorandom balanced `+1/-1` sequences
- Walsh/Hadamard-like orthogonal code families if constrained appropriately
- Gold-code-like families if correlation behavior is desirable and implementation cost is acceptable

### Initial Production Choice

The first production implementation should use pseudorandom balanced `+1/-1` temporal sequences derived from key material.

Reasons:

- DSSS acquisition and detection literature assumes correlation against a known spreading sequence and thresholding on detector output, which maps cleanly to fixed-window Layer 1 recovery
- spread-spectrum watermarking favors many weak, host-distributed contributions that only become strong under the correct detector
- pseudorandom balanced sequences are the cheapest way to get low single-frame bias and low accidental wrong-key correlation without forcing highly visible regular structure into the carrier

Walsh/Hadamard-like and Gold/Kasami-like families remain valid follow-on experiments, but they are not required to build Stage 1. They should only replace the initial pseudorandom design if empirical testing shows materially better correct-key separation without increasing visible structure or synchronization complexity.

## Key Derivation

All schedules should derive from one master secret plus explicit profile metadata.

Derived keys:

- `K1`: Layer 1 temporal schedule
- `K2`: Layer 2 temporal schedule
- `Kn`: noise schedule
- `Ks`: spatial permutation or spread schedule

Requirements:

- deterministic
- versioned
- domain-separated
- wrong key must fail cleanly

This layer should be treated as schedule derivation, not vague "seed reuse."

## Spatial Strategy

The signal should not occupy a stable, obvious physical layout frame-to-frame.

Recommended techniques:

- keyed spatial permutation
- keyed block shuffle
- optional spreading of logical symbols across multiple nearby carrier cells

This is a secondary concealment mechanism. The primary concealment mechanism is temporal balancing.

## Carrier Domain

Internal representation should be analog or soft-valued.

Recommended internal type:

- `Grid<f32>`

Why:

- precise low-amplitude perturbations are easier to control
- soft decoding is easier
- visual plausibility tuning is easier

Display or storage output may later be quantized to `u8` grayscale or another compact format, but the production signal model should not start from hard `±1` carriers.

## Decode Window

The first production version should use fixed decode windows.

Recommended initial range:

- `N = 64` to `N = 256`

Longer windows improve stealth at the cost of:

- latency
- synchronization burden
- more expensive recovery

Sliding or overlapping windows can be added later, but they should not complicate the initial production design.

## Acquisition and Synchronization

The first production version should avoid a general synchronization problem.

Stage 1 assumptions:

- one known fixed decode window
- no out-of-band preamble design
- no overlapping-window search in the production path

The decoder should therefore start as a fixed-window correlator over a candidate frame block. This is the simplest case supported by DSSS acquisition literature: test the received block against the expected spreading schedule and declare success only when detector response exceeds a conservative threshold.

If later stages require blind start-offset discovery, the next step should be a serial or parallel search over candidate offsets using the same matched-filter test, not a redesign of the physical layer.

### Threshold Policy

The current implementation now reflects an explicit thresholded decode policy instead of an implicit "decode succeeded if QR extraction happened to work" policy.

That is the correct direction.

However, the current threshold should still be treated as profile-specific policy, not a timeless constant. The important invariant is not the specific numeric threshold value; the important invariant is that decode acceptance is controlled by a detector policy grounded in measured wrong-key and wrong-window response distributions.

Current Stage 1 calibration status:

- the working baseline profile is `middle-64-a`
- baseline parameters are `frames = 64`, `noise_amplitude = 0.42`, `l1_amplitude = 0.22`
- the current working threshold is `6.0`
- this profile was chosen to reduce early visual emergence while preserving strong full-window decode reliability

Measured on a `128`-trial prefix sweep with `--prefix-step 4`:

- full-window decode was `100%`
- wrong-key, wrong-window, and naive-path decode were all `0%`
- no correct decodes occurred through `44/64` frames
- correct decode reached `12.5%` at `48/64`
- correct decode reached `100%` at `52/64`
- `k50 = 52`
- `k95 = 52`

That is the current "Goldilocks" Stage 1 target:

- mostly latent through the first two thirds of the window
- weak emergence late in the window
- reliable keyed decode before the final few frames

## Packet and FEC Layer

This layer exists above the stego codec and below application payloads.

It should not be bespoke unless forced by hard constraints.

Responsibilities:

- packet identity
- packet framing
- out-of-order delivery tolerance
- integrity verification
- erasure recovery

### Packet Model

Each Layer 2 packet should be a bounded unit with:

- packet ID
- stream or object ID if needed
- payload bytes
- integrity checksum
- optional FEC block membership

This is enough for reassembly. The codec should not define richer application semantics than necessary.

### FEC

Use standard packet-block erasure correction ideas.

Acceptable approaches:

- Reed-Solomon over bounded packet groups
- fountain-code-style ideas if implementation practicality supports them

Stage 2 should begin with Reed-Solomon over bounded packet groups and treat missing or low-confidence packets as erasures first. This matches the simplest reliable reuse of standard coding theory and avoids inventing new parity semantics inside `temporal`.

Do not invent a custom parity theory unless a standard one demonstrably fails the constraints.

## Payload Containers and Reuse

This is the most important simplification in the spec.

`temporal` should not invent a new media or object protocol.

### Small Opaque Payloads

For small structured messages or opaque data, use a standard framed record format above the packet layer.

Good candidates:

- CBOR records
- length-delimited protobuf messages
- MessagePack with explicit framing

### Large Structured Payloads

For large media or complex objects, use independently meaningful container or segment units.

Good candidates:

- fragmented MP4 or CMAF segments
- MPEG-TS packets or segments when graceful degradation matters
- independently decodable image or preview units

Do not feed a large structured file as arbitrary byte slices if graceful degradation matters.

### Consequence

This system is mostly a streaming problem that happens not to be over a network.

That means:

- reuse standard packet framing
- reuse standard FEC ideas
- reuse standard payload containers
- only invent the stego physical layer

## Payload Modes

There are only two valid payload modes.

### Opaque Mode

Use for:

- small payloads
- encrypted blobs
- all-or-nothing use cases

Behavior:

- upper layer may treat payload as arbitrary bytes
- partial recovery may be useless and that is acceptable

### Framed Mode

Use for:

- large payloads
- media payloads
- anything where partial recovery should still be useful

Behavior:

- upper layer supplies framed packets or segments
- partial recovery can still be useful if some packets are missing

If graceful degradation matters, framed mode is mandatory.

## QR Bootstrap

The Layer 1 QR should stay small and stable.

It should not try to describe the entire payload stream in detail.

Recommended QR contents:

- `codec = temporal`
- `version`
- `session locator`
- `container type`
- `packet layer profile`
- optional short digest or capability flags

The detailed packet or manifest state belongs above the QR.

## Minimal Container Requirements

Whatever outer file or stream format carries frames should record:

- magic
- version
- frame width
- frame height
- frame count
- sample format
- quantization or display format

It may also record:

- public profile ID
- non-secret decode hints

It should not store:

- full derived schedules
- secret key material

## Security and Threat Model Notes

Visual plausibility is not the same as strong steganographic security.

This document guarantees only:

- single-frame visual concealment as a production objective
- keyed recovery requirement

It does not automatically guarantee:

- resistance to a powerful analyst with implementation knowledge
- resistance to statistical steganalysis
- cryptographic confidentiality of Layer 2 unless payloads are separately encrypted or whitened

Production use should assume:

- packet payloads are whitened or encrypted before embedding
- schedule derivation is key-dependent and domain-separated
- wrong keys and wrong windows should not degrade gracefully into partial reveals
- detector thresholds are chosen using measured wrong-key and wrong-window response distributions, not intuition

## What Stays Experimental

The existing codecs remain useful for:

- comparative tests
- debugging primitives
- understanding failure modes
- historical documentation

They are not production choices.

The repository should eventually present:

- one production codec: `temporal`
- many experimental codecs, clearly marked as such

## Acceptance Criteria

The production implementation should not be accepted until the following are measured and pass.

### Visual Leakage

- single frames show no obvious QR structure by eye
- single-frame correlation with the true QR stays near null

### Naive Accumulation Resistance

- naive summation should not be the intended decode path
- naive accumulation should not cleanly recover Layer 1

### Correct-Key Recovery

- the correct key and correct window recover Layer 1 reliably
- QR decode success rate is high across representative trials
- correct-key detector response is well separated from the distribution of random wrong-key responses
- decode acceptance is controlled by an explicit detector threshold, not only by whether QR decode happened to succeed
- prefix acquisition should remain subdued early and then rise sharply near the intended window end

Current Stage 1 acquisition target for the working baseline:

- no clean decode through roughly `44/64`
- only weak emergence around `48/64`
- near-certain decode by `52/64`

### Wrong-Key Rejection

- wrong key should fail cleanly
- detector response against many random wrong keys stays below threshold with an acceptably low false-positive rate

### Wrong-Window Rejection

- wrong window should fail cleanly
- detector response against misaligned windows stays below threshold with an acceptably low false-positive rate

### Layer 2 Recovery

- after Layer 1 subtraction, packet recovery should succeed at the target profile
- packet integrity and FEC behavior should match design expectations

### Structured Payload Behavior

- opaque mode should recover small opaque payloads end-to-end
- framed mode should allow partial useful recovery for structured payloads

## Implementation Plan

The implementation should proceed in narrow stages.

### Stage 1: Layer 1 Only

Build:

- analog internal carrier
- pseudorandom balanced temporal code family
- fixed decode window
- keyed Layer 1 correlation
- explicit detector policy and thresholded QR recovery
- detector score instrumentation
- wrong-key and wrong-window response measurement
- per-frame keyed spatial permutation to avoid stable raw-frame structure

Viewer should show:

- raw frame
- naive accumulation
- keyed Layer 1 correlation field
- detector score for correct and incorrect keys

If Stage 1 fails the visual-static requirement, stop and redesign.

### Stage 1 Current State

Implemented now:

- fixed-window `temporal` Layer 1 codec
- keyed correlation field reconstruction
- detector score reporting
- thresholded decode policy
- debug viewer targeting `temporal`
- CLI eval runner for repeated wrong-key / wrong-window / naive-path measurements
- CLI prefix-acquisition instrumentation via `--prefix-step`
- a tuned working baseline profile, `middle-64-a`, for later-emerging Layer 1 reveal

Not implemented yet:

- packet layer
- Layer 2 embedding
- residual subtraction and Layer 2 decode
- synchronization search beyond fixed known windows

Current working baseline:

- profile: `middle-64-a`
- frame size: `41x41`
- window: `64`
- noise amplitude: `0.42`
- Layer 1 amplitude: `0.22`
- threshold: `6.0`

Current eval workflow:

- append sweep rows to a local untracked TSV such as `temporal_results.tsv`
- use full-window score separation to bound safe thresholds
- use prefix sweeps to tune acquisition timing rather than only terminal decode rate

### Stage 2: Packet Layer

Build:

- bounded packet framing
- integrity checks
- Reed-Solomon packet-block FEC
- opaque payload mode

### Stage 3: Framed Payloads

Build:

- framed mode above the packet layer
- small standard record format support
- support for carrying standard segmented media payloads

### Stage 4: Streaming Refinements

Only after earlier stages succeed:

- overlapping or sliding windows
- more advanced synchronization
- richer profiling and tuning

## API Direction

The eventual public API should center on one production codec:

- `qrstatic::codec::temporal`

Its shape should stay honest:

- correlation field reconstruction is the core primitive
- decode acceptance should be explicit policy, not an accidental side effect of QR decoding
- eval-only helpers should not silently become product-surface semantics

All new docs, viewer work, CLI examples, and product-facing commands should target `temporal`.

Legacy codecs should remain available only as explicitly experimental references.

## Final Position

The correct simplification is:

- invent one new thing: the stego physical layer
- reuse everything else that is already a solved streaming or packetization problem

If the implementation drifts toward inventing a new transport protocol, media container, or application chunk taxonomy inside the codec, it is going off the rails.

## References

- Stojanovic, M., Freitag, L., and Johnson, M. "Code acquisition for DS/SS communications over time-varying multipath channels." IEEE. The practical takeaway used here is that acquisition is a correlation-and-threshold problem and that fixed-window detection is the correct narrow first stage. Source consulted: [MIT-hosted PDF](https://www.mit.edu/~millitsa/resources/pdfs/acq.pdf)
- Cox, I., Kilian, J., Leighton, T., and Shamoon, T. "Secure spread spectrum watermarking for multimedia." The practical takeaway used here is to treat the carrier as host media, spread many weak contributions widely, and judge success by detector separation between the correct key and many incorrect keys. Source consulted: [Columbia-hosted PDF](https://www.ee.columbia.edu/~ywang/MSS/HW2/CoxSpectrumWatermarking.pdf)
- Geisel, W. "Tutorial on Reed-Solomon error correction coding." NASA JPL. The practical takeaway used here is to reuse standard Reed-Solomon coding at the packet layer, especially with erasure-oriented decoding, instead of inventing codec-specific parity logic. Source consulted: [NASA PDF](https://ntrs.nasa.gov/api/citations/19900019023/downloads/19900019023.pdf)
