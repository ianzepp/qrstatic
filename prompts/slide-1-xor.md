# Slide 1: XOR — The Foundation

## Style Brief

- Warm parchment/aged paper background with subtle coffee-stain artifacts and wear marks
- Hand-drawn pencil/ink sketch rendering — not clean vector, not photorealistic
- Bold hand-lettered headings (all-caps, slightly uneven baselines)
- Thin leader lines with small annotations in a lighter hand
- Boxed/framed elements with rough hand-drawn borders (rounded rectangles, pinned note cards)
- Monospace snippets for code/data shown in small "terminal window" sketches with title bars
- Information density: moderate — 4-8 labeled elements per slide, text-heavy but organized spatially
- No color beyond black/dark brown ink on parchment — purely monochromatic sketch aesthetic
- 16:9 aspect ratio

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "1. XOR — THE FOUNDATION" centered, with a double underline. Smaller subtitle: "temporal accumulation recovers a hidden pattern".

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line with labels.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Left side: A hand-drawn 5x5 QR grid labeled "QR target" showing actual 0/1 cell values, with annotation "the secret to hide". Center: a series of three 5x5 grids with random 0/1 values labeled "F1 (random)", "F2 (random)", "..." and then a final grid labeled "FN (computed)" with the formula written along a curved arrow: "FN = QR xor F1 xor F2 xor ... xor F(N-1)". Right side: a pinned note card containing the XOR truth table in a small monospace block, and below it the identity "A xor B xor A = B" double-underlined.

Along the top margin: small hand-written annotation "each frame independently: pure random. P(0)=P(1)=0.5. no detectable bias."

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: a stack of overlapping frame sketches labeled "all N frames received (any order)" with annotation "commutative — shuffle freely". A large hand-drawn arrow flows right labeled "XOR accumulate all". Center: intermediate accumulation shown as a partially-resolved grid with some cells showing final values and others showing "?" marks, suggesting progressive resolution. Right side: the fully resolved QR code with bold radiating lines, labeled "EXACT QR RECOVERY" in bold hand-lettering. A checkmark annotation reads "bit-perfect, deterministic".

BOTTOM LEFT: A large bold hand-lettered callout in a rough sketched box: "N IS THE SECRET" with smaller text below: "an observer who doesn't know N sees only random noise. N frames of random static. nothing to detect."

BOTTOM CENTER: A hand-drawn properties table in a sketched frame with two columns. Left column header "HAS": exact recovery, binary frames, deterministic, commutative. Right column header "LACKS": payload channel, magnitude data, noise model, grayscale.

BOTTOM RIGHT: A torn-edge paper sketch with an arrow pointing right containing hand-lettered text: "LIMITATION: XOR is all-or-nothing. No graceful degradation. No extra data channel. What if accumulation gave us MAGNITUDE too?" with "magnitude" double-underlined as the bridge to the next codec.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on cards. The feel is a thorough lab notebook entry.
