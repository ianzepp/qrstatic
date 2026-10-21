---
name: noise-channel-analysis
description: Analyze random-looking image, video, audio, or binary streams for hidden channels, embedded structure, steganographic payloads, and implementation weaknesses that make detection or recovery easier. Use when a stream appears noise-like but may hide a message, when reviewing code that tries to disguise signal as static, or when doing red-team analysis of covert channels, stego pipelines, accumulation-based decoders, deterministic noise generators, or structured symbols hidden in noise.
---

# Noise Channel Analysis

Assume the stream may be hostile, intentionally misleading, and only partially observable. Treat apparent randomness as a claim to test, not a fact to accept.

Work like an analyst trying to answer three questions:
- Is this actually noise?
- If not, what structure leaks through?
- What design or implementation choices make the channel easier to detect, align, decode, or disrupt?

## Workflow

1. Characterize the cover signal.
2. Hunt for leakage and invariants.
3. Apply transforms that would expose latent structure.
4. Search for synchronization and framing assumptions.
5. Exploit redundancy and decoder oracles.
6. Review the implementation for shortcuts that weaken concealment.
7. Separate evidence from speculation.

## Characterize The Cover Signal

Establish what "normal" would look like before trying to decode anything.

- Measure basic distributions: mean, variance, histogram shape, entropy, local entropy, run lengths, autocorrelation, and cross-frame correlation.
- Check whether the stream is binary, signed, quantized, floating-point, audio-like, video-like, or a transformed representation of one of those.
- Compare temporal behavior against true independent noise. Real random cover should not steadily reveal stable spatial structure under simple accumulation unless the channel is biased.
- Look for deterministic artifacts: repeated seeds, repeating blocks, exact duplicate subregions, cadence, periodic resets, or fixed-size windows.
- State clearly whether the "noise" resembles synthetic PRNG output, sensor noise, compression residue, dithering, or a custom simulation.

If a baseline is missing, say so and proceed with conservative claims.

## Hunt For Leakage

Prioritize statistical and structural weaknesses before attempting full recovery.

- Search for frame-to-frame drift that converges instead of canceling.
- Check whether local regions accumulate faster than expected under averaging, majority vote, or signed summation.
- Look for spatial motifs that survive denoising: corners, finder-like squares, timing lines, alignment marks, headers, or packet boundaries.
- Test multiple window sizes and offsets. Hidden channels often leak only when grouped correctly.
- Watch for asymmetric "final correction" behavior where one frame or one phase visibly differs from the rest.
- Treat successful error correction or format parsing as an oracle. If decode success rises above chance under one transform family, you are likely close.

Use the transform families in [references/transforms.md](./references/transforms.md).

## Recover Structure

Try to expose the latent representation before trying to interpret payload semantics.

- Sweep over accumulation methods: XOR, sum, sign of sum, thresholding, rolling average, majority vote, variance maps, differencing, and autocorrelation.
- Sweep over alignment hypotheses: frame count, stride, overlap, offset, transpose, endianness, channel order, and grid dimensions.
- Score each candidate by structured consistency, not by appearance alone.

Good scoring targets:
- Repeated marker patterns
- Error-correcting code consistency
- Expected header or mode fields
- Stable recovery across neighboring parameter choices
- Decodes that survive small perturbations

Do not confuse one lucky decode with a recovered channel. Demand repeatability.

## Exploit Redundancy

Assume the hidden representation is more structured than the cover signal.

- Look for fixed markers, headers, framing bytes, mode bits, checksums, parity, CRCs, Reed-Solomon, BCH, or other integrity structures.
- Use format redundancy as a foothold. Structured symbols such as QR codes leak finder patterns, timing patterns, version fields, and format bits.
- Use constrained payload spaces when available: printable text, URLs, ASCII headers, known prefixes, bounded message lengths, or repeated templates.
- Treat deterministic seeding as a major weakness. If the encoder derives per-frame randomness from predictable inputs, reconstruct the stream generator and subtract the expected cover.

## Review The Code

When source is available, switch from blind analysis to attack-oriented design review.

- Identify the exact cover model and ask whether it matches real noise or only synthetic tests.
- Find every source of determinism: seed derivation, frame numbering, message numbering, static keys, fixed masks, fixed windows, or deterministic shuffles.
- Find every place the encoder "forces" correctness late in the pipeline.
- Find assumptions that make decoding easier than concealment: fixed frame counts, known grid size, stable message boundaries, fixed QR version range, or a decoder that only expects self-generated output.
- Inspect whether stronger error correction accidentally acts as a detection amplifier by making weak partial recovery still decode.
- Check whether the implementation leaks metadata through size, timing, capacity cliffs, or version-selection behavior.

Use the checklist in [references/checklist.md](./references/checklist.md).

## Maintain False-Positive Discipline

Noise analysis is vulnerable to self-deception. Be strict.

- Report which findings are measurements, which are inferences, and which are hypotheses.
- Re-run promising transforms on adjacent windows and perturbed parameters.
- Try to falsify each decoding story with counterexamples and random controls.
- Prefer "evidence of hidden structure under these assumptions" over "message recovered" unless the recovery is stable and repeatable.
- If a decode only works with one brittle parameter setting, say that clearly.

## Produce Outputs That Help Decision-Making

Tailor the deliverable to the task.

For stream analysis, provide:
- strongest evidence the signal is not pure noise
- transform families tried
- best candidate structure and confidence
- synchronization assumptions
- next experiments most likely to reduce uncertainty

For code review, provide:
- weaknesses that materially aid detection or decoding
- why each weakness matters operationally
- likely attacker workflow
- concrete hardening options and tradeoffs

## References

- Use [references/checklist.md](./references/checklist.md) for the red-team review checklist.
- Use [references/transforms.md](./references/transforms.md) for the transform arsenal and search strategy.
- Use [references/qrstatic.md](./references/qrstatic.md) when the target resembles `qrstatic` or another QR-in-noise design.
