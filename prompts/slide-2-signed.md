# Slide 2: Signed — Two Channels from One Accumulation

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "2. SIGNED — TWO CHANNELS FROM ONE ACCUMULATION" centered, with a double underline. Smaller subtitle: "sign carries QR, magnitude carries payload".

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Far left: a hand-drawn QR grid labeled "QR target" with cells marked as +1 (white modules) and -1 (black modules). A thin leader line reads "white=+1, black=-1". Below it, a small data block labeled "payload bits" showing a row of 1s and 0s: "1 0 1 1 0 1 0 0".

Center-left: A large pinned note card titled "PER-CELL ASSIGNMENT" showing the core mechanism. A single cell is blown up with N=8 frame slots drawn as a horizontal row of boxes. Five boxes contain "+1" and three contain "-1", shuffled in random order. Annotations: "desired sum = sign * magnitude", "n_positive = (N + desired_sum) / 2", "n_negative = N - n_positive". A small note reads "shuffle order via PRNG — each frame looks random".

Center-right: A diagram showing how the desired magnitude is computed. A vertical number line from -N to +N with tick marks. A baseline magnitude is marked with a horizontal dashed line labeled "base magnitude (signal_strength)". Two offset arrows show "+delta" for payload bit=1 and "-delta" for payload bit=0. Bold label: "payload bit shifts HOW FAR from zero". A small annotation: "sign determines WHICH SIDE of zero".

Right margin: A small sketched cell grid showing one column of N=8 frames for a single cell position, with +1 and -1 values written in each row, and the sum written at the bottom with an arrow pointing to it. Label: "sum = +2 (positive = white QR module, magnitude 2 = payload bit)".

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: a stack of overlapping Grid<i8> frames labeled "N signed frames (+1/-1)" flowing into a large arrow labeled "accumulate (sum all frames)".

Center: A hand-drawn accumulated grid showing integer values like "+6", "-4", "+8", "-6", "+2", "-8" in cells. Two extraction arrows fork from this grid. Upper arrow labeled "SIGN" points to a QR pattern grid where positive cells are white and negative cells are black, with label "threshold at 0 yields QR". Lower arrow labeled "MAGNITUDE" points to a grid showing absolute values like "6", "4", "8", "6", "2", "8" with annotation "compare to expected baseline".

Center-right: A sketched detail box labeled "PAYLOAD RECOVERY" showing the comparison: "actual magnitude vs expected magnitude" with "residual = actual - expected". Below: "residual > 0 votes bit=1" and "residual < 0 votes bit=0". A small majority-vote tally sketch shows tick marks under "1" and "0" columns, with "majority wins" circled.

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "TWO DIMENSIONS, ONE STREAM" with smaller text: "sign = QR key (which side of zero). magnitude = payload (how far from zero). same accumulated data, two independent channels."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": payload channel, exact integer arithmetic, deterministic noise cancellation, per-cell magnitude control. Right column "LACKS": grayscale appearance (still binary per frame), continuous values, spatial scrambling, probability model.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: what if we drop exact assignment and use PROBABILITY BIAS instead? Each frame becomes pure random static..." with "probability" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards. Along the top margin, a small annotation reads "Grid<i8>, min 4 frames, signal_strength controls baseline magnitude".
