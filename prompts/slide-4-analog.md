# Slide 4: Analog — Continuous Signal + Noise Model

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "4. ANALOG — CONTINUOUS SIGNAL + NOISE MODEL" centered, with a double underline. Smaller subtitle: "signal grows linearly, noise grows as square root of N".

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Far left: A pinned note card titled "PER-FRAME FORMULA" containing a large hand-lettered equation: "frame[cell] = sign * (S + payload_bias) + noise" with annotations below each term: "sign" has arrow to "+1 or -1 from QR", "S" has arrow to "signal_strength", "payload_bias" has arrow to "+delta or -delta from payload bit", "noise" has arrow to "uniform random from PRNG". The card has a pushpin and slightly curled corner.

Center: A large hand-drawn graph taking up significant space, titled "SNR IMPROVEMENT" in bold lettering. The x-axis is labeled "N frames" with tick marks at 1, 4, 16, 64, 256. The y-axis is labeled "accumulated value". Two hand-drawn curves: a steep straight line rising steeply labeled "Signal = N * S (linear)" drawn in bold strokes, and a much flatter curve labeled "Noise = sqrt(N) * amplitude" drawn in thinner strokes. The growing gap between them is cross-hatched and labeled "SNR = sqrt(N)". Below the graph, a small table shows: "N=1: SNR=1" then "N=4: SNR=2" then "N=16: SNR=4" then "N=64: SNR=8".

Right side: A hand-drawn 3D perspective sketch of a height field or terrain map, showing peaks and valleys arranged in a QR-like pattern. Tall peaks are labeled "+N*S (white module)" and deep valleys labeled "-N*S (black module)". A horizontal plane cuts through at zero labeled "threshold plane: sign = QR". The varying heights of peaks above the threshold are annotated "taller peak = payload bit 1" and "shorter peak = payload bit 0". Title above: "ACCUMULATED HEIGHT FIELD".

Top margin annotation: "Grid<f32>, min 2 frames, signal_strength must exceed noise_amplitude"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A stack of overlapping frame sketches labeled "N float frames" with wavy lines suggesting continuous values, flowing into a large arrow labeled "sum all frames" leading to an accumulated grid with float values like "+4.8 -3.2 +5.1 -4.7".

Center: A critical step shown as a large diagram. The accumulated grid sits at left. A second grid labeled "expected noise sum" sits below it, drawn with dashed borders and annotation "reconstructed from shared PRNG seed — same key, same noise". A large minus sign between them leads to a third grid labeled "CLEANED FIELD" with cleaner values. Annotation: "noise is deterministic, so we subtract it exactly". This is shown as the key insight with double-underline on "subtract it exactly".

Center-right: From the cleaned field, two forking arrows. Upper arrow labeled "SIGN (threshold at 0)" leads to a QR grid with label "recovered QR". Lower arrow labeled "MAGNITUDE (distance from baseline)" leads to a comparison diagram showing actual magnitude versus expected baseline "N * signal_strength", with residuals marked as bit=1 (above) and bit=0 (below). A small majority-vote tally for multi-cell positions.

Right margin: A small sketched note card titled "DETERMINISTIC NOISE" containing: "encoder seeds: Prng::from_key(qr_key, frame_index)" and "decoder reconstructs identical sequence" and "therefore: noise cancels PERFECTLY" with "perfectly" underlined.

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "FROM DISCRETE TO CONTINUOUS" with smaller text: "binary flipped biased coins. analog adds real-valued signal plus noise. now we have a formal noise model: SNR improves as sqrt(N). and deterministic noise means perfect cancellation on decode."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": continuous float values, formal SNR model, deterministic noise cancellation, payload in magnitude, grayscale appearance. Right column "LACKS": multi-layer structure, overlapping windows, spatial permutation, boundary-free carrier.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: what if L1 accumulated outputs THEMSELVES became frames for a second layer? Recursive steganography — a QR hidden inside a QR..." with "recursive" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
