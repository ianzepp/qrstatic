# Noise Channel Review Checklist

Use this checklist when reviewing a covert channel or a codebase that tries to hide structure inside random-looking output.

## Cover Model

- Ask what real-world process the output is trying to imitate.
- Check whether the generated noise matches that process statistically, not just visually.
- Check whether the cover model changes with time, scene, device, or environment. Static synthetic noise is easier to spot than adaptive cover.
- Check whether the output uses a PRNG where natural cover would be stochastic in different ways.

## Detection Surface

- Measure entropy globally and locally.
- Measure autocorrelation and cross-frame correlation.
- Measure whether accumulation reveals stable structure faster than chance.
- Check for histogram asymmetry, quantization cliffs, and clipped tails.
- Check for boundary leakage: message start, message end, reset, flush, or final correction.
- Check whether message size, QR version, or frame count changes observable statistics.

## Synchronization Weaknesses

- Fixed frame count per message
- Stable message boundaries
- Fixed stride or overlap
- Deterministic reseeding schedule
- Repeated initialization behavior
- Distinct startup or teardown frames

These weaknesses let an attacker align accumulation windows without knowing the payload.

## Structural Weaknesses

- Hidden symbol has strong geometry such as QR finder or timing patterns.
- Error correction tolerates partial recovery.
- Format bits or headers leak metadata.
- Capacity cliffs reveal approximate payload size.
- Mask or layout choice is deterministic.

These weaknesses let an attacker score candidate transforms cheaply.

## PRNG And Determinism Weaknesses

- Seed derived from predictable text such as `"key:index"`.
- Seed reused across messages or channels.
- PRNG is non-cryptographic when stronger unpredictability is needed.
- Different phases share one generator state or correlated seeds.
- Decoder can reconstruct the expected cover exactly.

If the attacker can regenerate the cover, concealment may collapse into subtraction.

## Decoder-Oriented Weaknesses

- Decoder only targets self-generated output.
- Decoder assumes known dimensions or known symbol family.
- Decoder assumes low noise outside the intended carrier model.
- Decoder accepts ambiguous candidates too easily.
- Decoder success becomes an oracle for transform search.

## Hardening Questions

- Can the cover model be made adaptive to realistic noise?
- Can message boundaries be obscured or removed?
- Can deterministic seeds be replaced or hidden?
- Can structured symbols be replaced with less geometrically obvious encodings?
- Can error-correction strength be tuned to reduce oracle behavior?
- Can parameter selection vary enough to resist one-size-fits-all attacks?
