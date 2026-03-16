# Slide 3: Binary — Probability-Biased TV Snow

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "3. BINARY — PROBABILITY-BIASED TV SNOW" centered, with a double underline. Smaller subtitle: "replace exact assignment with biased coin flips".

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Far left: A hand-drawn QR grid labeled "QR target" with cells shaded black or white. Two arrows emerge from it. Upper arrow leads to a pinned note card titled "BIAS MAP" showing the rule: "white module: P(+1) = 0.80" and "black module: P(+1) = 0.20" with a small coin-flip sketch. Lower arrow leads to a small data block labeled "payload bits: 1 0 1 1 0 1" with annotation showing how each bit modulates the bias: "bit=1: bias 0.80 becomes 0.90 (stronger)" and "bit=0: bias 0.80 becomes 0.70 (weaker)".

Center: A large diagram titled "SPATIAL PERMUTATION" showing two grids side by side connected by crossing arrows. Left grid labeled "logical layout (QR structure visible)" shows a small bias-map pattern with clustered high and low values. Right grid labeled "physical layout (scrambled)" shows the same values shuffled into random positions. Annotation: "fixed Fisher-Yates shuffle seeded by frame dimensions. Same permutation every frame. Single frame leaks no spatial structure."

Right side: A sketched frame of TV static — a dense grid of tiny plus and minus signs scattered randomly, labeled "SINGLE FRAME OUTPUT". Below it, a magnified detail showing a few cells with "+1 -1 +1 +1 -1" values. Two annotations with arrows: "looks like pure random noise" and "but each cell was flipped with a biased coin". A small statistical note: "any single frame: no detectable pattern. bias only emerges after accumulation."

Top margin annotation: "Grid<i8>, default N=60 frames, base_bias in [0.5, 1.0), payload_bias_delta in [0.0, 0.45]"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A stack of overlapping frame sketches labeled "N=60 signed frames" with a large arrow labeled "sum all frames" flowing right into an accumulated grid showing integer values like "+38 -32 +41 -36". Label: "accumulated (still permuted)".

Center: An arrow labeled "UNPERMUTE" with crossing lines showing the inverse shuffle, leading to a second accumulated grid labeled "logical layout restored". From this grid, two forking arrows emerge. Upper arrow labeled "SIGN" leads to a QR pattern with annotation "threshold at 0 yields QR". Lower arrow labeled "MAGNITUDE" leads to a number line diagram showing the expected value calculation: "expected = N * (2 * base_bias - 1) = 60 * 0.6 = 36" with actual values scattered around it. Cells above the line are labeled "bit=1 (stronger bias)" and cells below labeled "bit=0 (weaker bias)".

Right side: A sketched detail card titled "EXPECTED VALUE MATH" showing: "N=60, bias=0.8" then "white cell: E[sum] = 60*(0.8 - 0.2) = +36" and "black cell: E[sum] = 60*(0.2 - 0.8) = -36" and "payload bit=1: E = 60*(0.9-0.1) = +48" and "payload bit=0: E = 60*(0.7-0.3) = +24". Below: "magnitude gap = detectable after N frames".

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "AUTHENTIC NOISE" with smaller text: "signed codec dealt exact cards. binary flips biased coins. every frame is genuine random static. 1 bit per pixel per frame. 4x memory savings vs floats."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": authentic TV snow appearance, 1-bit-per-pixel memory, payload in magnitude, spatial permutation scrambling. Right column "LACKS": exact recovery (statistical), continuous signal values, noise cancellation model, multi-layer structure.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: what about CONTINUOUS float values? Signal accumulates linearly, noise as sqrt(N), giving real SNR guarantees..." with "SNR" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
