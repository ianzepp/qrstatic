# Slide 8: Temporal — Correlation-Based Decode with Balanced Chip Schedules

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "8. TEMPORAL — CORRELATION-BASED DECODE" centered, with a double underline. Smaller subtitle: "balanced chip schedules, per-frame spatial permutation, detector scoring, naive-proof."

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Far left: A pinned note card titled "BALANCED CHIP SCHEDULE" containing a diagram. A vertical list of cells (cell 0, cell 1, cell 2, ...) on the left. For each cell, a horizontal row of N small boxes representing frames. Each box contains either "+1" or "-1" as a chip value. The boxes are shaded lightly to distinguish polarity. A key annotation: "exactly N/2 chips are +1, exactly N/2 are -1" with "balanced" double-underlined. Below: "sum of chips per cell = 0 always". A Fisher-Yates shuffle arrow with label "seeded from: qrstatic:temporal:v1:l1:{master_key}:cell:{cell_idx}" indicating random permutation of the balanced chips. The annotation "different random order per cell, deterministic from key" appears below.

Center: A large encoding flow diagram. At left, a QR grid labeled "QR target" feeds into a box labeled "build_l1_signal_map" which produces a grid where white modules = +1.0 and black modules = -1.0, centered in frame with zero padding. An arrow from this signal map and from the chip schedule feeds into the per-frame encoding step. The per-frame formula is written large: "frame[physical_idx] += l1_amplitude * signal * chip" with each term annotated: "physical_idx" has arrow to "from per-frame permutation", "l1_amplitude" has arrow to "constant signal strength", "signal" has arrow to "+1/-1 from QR map", "chip" has arrow to "+1/-1 from schedule". The base of each frame is a noise grid labeled "noise_frame from PRNG" with annotation "seeded per frame index".

Right side: A detailed diagram titled "PER-FRAME SPATIAL PERMUTATION" showing two grids connected by crossing arrows. Left grid labeled "logical layout (cell positions match QR)" with cells numbered 0,1,2,3... in order. Right grid labeled "physical layout (scrambled)" with the same numbers in shuffled positions. Three different frame indices shown side by side (frame 0, frame 1, frame 2) each with DIFFERENT crossing arrow patterns, emphasizing "unique permutation per frame — unlike binary's fixed shuffle". Annotation: "seeded from: qrstatic:temporal:v1:spatial:{master_key}:frame:{frame_index}". A bold note: "every frame looks different spatially. no static spatial pattern to detect."

Top margin annotation: "Grid<f32>, n_frames >= 4 (must be even), noise_amplitude >= 0, l1_amplitude > 0"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A flow diagram showing the correlation decode. A stack of N frame sketches flows into a processing block. For each frame: first an arrow to "unpermute_grid" using the frame's unique permutation (restores logical layout), then the unpermuted frame is multiplied element-wise by the chip schedule for that frame. The products accumulate into a single grid labeled "CORRELATION FIELD". The key formula is written: "field[cell] += sample[logical] * chip[frame][cell]" with annotation: "multiply-accumulate: chips that matched the encoding amplify the signal, chips that opposed it cancel noise."

Center: A critical comparison panel titled "WHY CORRELATION BEATS NAIVE ACCUMULATION". Two parallel paths drawn side by side. Left path labeled "NAIVE SUM (no key)" shows frames being simply added. Because chip schedule is balanced (+N/2 and -N/2), the signal contributions cancel: "+1*signal" in some frames and "-1*signal" in others, summing to zero. Result: a flat noisy grid labeled "sum ≈ 0 — signal self-cancels" with a large X through it. Right path labeled "CORRELATION (with key)" shows frames being multiplied by the matching chip schedule before summing. Now "+1*signal*+1 = +signal" and "-1*signal*-1 = +signal" — every term adds constructively. Result: a clear QR pattern labeled "signal reinforced N times" with a checkmark. A bold annotation below: "without the master key, the balanced schedule makes naive accumulation useless."

Right side: A sketched card titled "DETECTOR SCORE" showing the formula: "detector_score = mean(|field[cell]|)" with annotation "average absolute correlation across all cells". Below, a number line from 0 to some maximum, with a moveable threshold marker labeled "min_detector_score (from TemporalDecodePolicy)". Left of threshold: "REJECT — insufficient confidence" shaded. Right of threshold: "ACCEPT — decode QR from sign(field)" unshaded. Below: "partial windows (correlate_prefix): score grows as more frames arrive, enabling progressive lock-on."

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "THE CULMINATION" with smaller text: "every previous idea converges here. analog's float signal model. layered's additive encoding on noise carrier. sliding's progressive lock-on (via correlate_prefix). audio's real-signal philosophy. PLUS: balanced chips that provably defeat naive accumulation. per-frame permutation that defeats spatial analysis. detector scoring that gates decode confidence."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": correlation-based decode, balanced chip schedules (sum=0), per-frame spatial permutation, detector score gating, progressive prefix correlation, naive-accumulation resistance, formal key hierarchy. Right column "COMPLETE": every vulnerability from codecs 1-7 addressed — no statistical leakage, no fixed spatial patterns, no boundary artifacts, no naive-sum recovery.

BOTTOM RIGHT: Torn-edge paper with arrow pointing left (back toward all previous slides): "THE JOURNEY: XOR → signed → binary → analog → layered → sliding → audio → temporal. Eight experiments. Each failure taught us what to protect against. This is the codec that learned from all the others." with "learned" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
