# QRstatic Notes

Use this note when the target resembles `qrstatic`: a stream designed to look like static while hiding QR-like structure through accumulation, deterministic noise generation, or layered codecs.

## Why QRstatic-Like Designs Leak

- QR codes are highly structured symbols with finder, timing, alignment, format, and payload layout regularities.
- Accumulation-based recovery creates a natural attack surface: attackers can try the same accumulation families as the decoder.
- Deterministic per-frame noise generation can let an attacker model or cancel the cover if seed derivation is predictable.
- Strong error correction can turn weak partial recovery into a reliable oracle.

## Immediate Attack Plan

1. Test accumulation over many window sizes and offsets.
2. Search for finder-like square motifs and timing-line periodicity.
3. Score candidate grids with QR format and mask consistency checks.
4. Use QR decode success as an oracle while controlling for false positives.
5. If code is available, reconstruct the PRNG and expected cover stream.

## Code Review Priorities

- Seed derivation and whether it is predictable
- Frame grouping assumptions
- Fixed or narrow QR version support
- Deterministic mask or layout behavior
- "Last frame fixes everything" logic
- Any decoder assumptions that mirror encoder choices too closely

## Likely Weakness Classes

- Simple XOR or additive codecs expose themselves under accumulation.
- Fixed message boundaries make window search easy.
- Narrow supported QR versions reduce the search space.
- Own-output-only decoders usually imply unrealistic assumptions that also help attackers.
- Validation based only on happy-path roundtrips misses hostile-input and false-positive behavior.
