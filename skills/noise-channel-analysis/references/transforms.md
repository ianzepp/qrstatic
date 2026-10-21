# Transform Arsenal

Use these transforms to search for latent structure in a random-looking stream. Apply them systematically and record which assumptions each transform introduces.

## Baseline Pass

- Raw visualization
- Histogram and run-length plots
- Frame differencing
- Rolling mean and variance
- Autocorrelation
- Cross-correlation between adjacent frames

This pass answers whether any simple non-random behavior is already visible.

## Accumulation Pass

- XOR accumulation
- Integer summation
- Sign of cumulative sum
- Majority vote over binary interpretations
- Rolling-window accumulation with multiple widths
- Prefix-sum and suffix-sum scans

Use this pass when the hidden channel may bias many weak observations toward one latent symbol.

## Threshold And Quantization Pass

- Sweep thresholds instead of choosing one threshold early.
- Try bipolar interpretations such as `{0,1}` and `{-1,+1}`.
- Try clipping weak magnitudes before thresholding.
- Try absolute-value maps and magnitude-only maps.

Many covert channels only appear after the right interpretation of the carrier alphabet.

## Geometry Recovery Pass

- Search candidate grid sizes.
- Search row-major vs column-major layouts.
- Search transpose, flip, rotation, and offset.
- Search channel interleavings if the source is RGB, stereo, or multiplexed.
- Search overlap and stride if the encoder may use sliding windows.

If the hidden object is a structured 2D symbol, geometry recovery often matters more than payload decoding.

## Oracle Pass

- Attempt format parsing or checksum validation.
- Attempt Reed-Solomon or BCH recovery.
- Attempt QR finder/timing/alignment scoring.
- Attempt constrained payload decoding such as ASCII or URL prefixes.

Use oracle success as evidence, not proof. Random data occasionally looks valid by accident.

## Control Pass

- Run the same transforms on synthetic random controls.
- Run the same transforms on adjacent windows expected to be empty.
- Perturb the best parameters slightly and confirm the signal persists.

If the result disappears immediately under small perturbations, confidence should stay low.
