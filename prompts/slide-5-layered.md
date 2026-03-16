# Slide 5: Layered — Recursive Two-Layer Steganography

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "5. LAYERED — RECURSIVE TWO-LAYER STEGANOGRAPHY" centered, with a double underline. Smaller subtitle: "the output of one accumulation becomes the input to the next".

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Left side: A large recursive structure diagram dominating the upper half. At the top, a row of small frame sketches representing N1=30 carrier frames, drawn as thin stacked rectangles with wavy lines suggesting float noise. A bracket groups them with label "N1 frames". A large downward arrow labeled "accumulate" leads to a single larger rectangle labeled "L1 output 1" with a faint QR pattern sketched inside. This entire block (N1 frames to L1 output) is repeated three times side by side with "..." between them, showing N2 such blocks. Labels read "L1 output 1", "L1 output 2", "...", "L1 output N2". A bracket groups all N2 outputs with label "N2 L1 outputs". A second large downward arrow labeled "accumulate residuals" leads to a final large rectangle labeled "L2 FIELD" with a different QR pattern plus magnitude annotation inside.

Center-right: A pinned note card titled "HOW L2 HIDES INSIDE L1" showing a vertical bar chart metaphor. Three bars representing three L1 outputs, all approximately the same height labeled "expected L1 magnitude". But each bar has a tiny deviation above or below the expected line — one bar slightly taller labeled "+L2 deviation", one slightly shorter labeled "-L2 deviation". Annotation: "L2 signal = small perturbation of L1 magnitude. invisible unless you subtract the L1 baseline." Below: "deviation per output = QR2_sign * (L2_signal + payload_bias) / N2".

Right margin: A sketched table titled "FOUR SECRETS REQUIRED" in a rough bordered box with a pushpin: "N1 = carrier frames per L1 output" then "N2 = L1 outputs per L2 decode" then "QR1 = validates L1 structure" then "QR2 = key to decode L2 payload". Below: "total frames = N1 * N2 = 900" with "900" circled for emphasis.

Top margin annotation: "Grid<f32>, n1 >= 2, n2 >= 2, independent noise per layer, defaults: L1 signal=5.0, L2 signal=2.0"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A flow diagram showing the L1 decode path. A stack of N1 frame sketches flows into an arrow labeled "accumulate N1" leading to "raw L1 output". Then a minus sign and a dashed-border grid labeled "expected L1 noise (reconstructed)" produces "cleaned L1 output". An arrow labeled "sign threshold" leads to a small QR grid labeled "QR1 recovered". A checkmark annotation reads "first secret unlocked".

Center: A second flow diagram showing the L2 decode path, positioned below and to the right of L1. Multiple cleaned L1 output rectangles (N2 of them) feed into a processing step. For each L1 output, a large annotated step shows: "actual L1 magnitude" minus "expected L1 signal (from QR1)" equals "L2 residual". The N2 residuals accumulate into a single "L2 FIELD" grid. From this grid, two forking arrows: upper labeled "SIGN" leads to "QR2 recovered", lower labeled "MAGNITUDE" leads to "payload decoded".

Right side: A sketched detail card showing the layer separation visually. A tall vertical number line. At the top, a large bold region labeled "L1 SIGNAL (dominant)" spanning most of the height. Within the L1 region, a much thinner band labeled "L2 deviation (subtle)" shown as a small oscillation around the L1 baseline. Below that, an even thinner band labeled "noise". Annotation: "L2 is ~40x weaker than L1. only visible after L1 subtraction."

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "RECURSION AS SECURITY" with smaller text: "an observer who finds QR1 still sees nothing unusual. the L2 signal looks like normal L1 variance. you need QR1 as a key to even begin looking for L2. each layer requires the previous layer's secret."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": recursive two-layer structure, two independent QR codes, four-secret hierarchy, payload in L2 magnitude, independent noise per layer. Right column "LACKS": overlapping windows (fixed N1 blocks), boundary-free carrier, spatial scrambling, graceful lock-on.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: fixed N1-frame blocks create DETECTABLE BOUNDARIES. What if windows overlapped? Decode from ANY offset..." with "any offset" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
