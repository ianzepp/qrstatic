# Slide 6: Sliding — Overlapping Windows, No Boundaries

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "6. SLIDING — OVERLAPPING WINDOWS, NO BOUNDARIES" centered, with a double underline. Smaller subtitle: "decode from any offset. continuous, uniform carrier."

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Left side: A large timeline diagram spanning most of the upper section width. A horizontal arrow represents time, labeled "frame index" with tick marks at 0, 30, 60, 90, 120. Above the timeline, three overlapping brackets represent sliding windows: Window A spans frames 0-59, Window B spans frames 30-89, Window C spans frames 60-119. Each bracket is labeled with "N1=60 frames". The overlap regions between adjacent windows are cross-hatched and labeled "30 frame overlap (50%)". Annotation along the timeline: "every position is a valid decode window. no fixed boundaries."

Center-right: A side-by-side comparison in two sketched boxes. Left box titled "FIXED BLOCKS (layered)" shows a timeline divided into rigid segments with bold vertical lines at boundaries. An arrow points to one boundary with label "statistical discontinuity here — detectable". Right box titled "SLIDING WINDOWS" shows a smooth continuous timeline with no discontinuities, labeled "uniform carrier — no artifact to detect". A bold annotation below: "the boundary IS the vulnerability. sliding eliminates it."

Right margin: A pinned note card titled "STRIDE PARAMETER" showing: "stride = N1: degenerates to fixed blocks (layered)" then "stride = N1/2: 50% overlap" then "stride = 1: maximum overlap, every frame emits" with "stride controls the tradeoff between overlap density and compute cost" below.

Top margin annotation: "Grid<f32>, n1 >= 2, n2 >= 1, 0 < stride <= n1, defaults: L1 signal=5.0, L2 signal=2.0"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A stream decoder diagram. A continuous stream of frame rectangles flows left to right. A sliding bracket labeled "current window (N1 frames)" sits over the stream, with a dashed arrow showing it advancing by "stride" frames. Below the bracket, an arrow leads to "L1 output" with a faint QR pattern. Annotation: "emit one L1 result every stride frames. lock on at ANY position."

Center: The L2 decode path shown as a flow. N2 L1 output rectangles feed into a processing block. For each L1 output, the step shows: "subtract expected L1 signal (from QR1)" then "subtract expected L1 noise (from shared seed)". The residuals accumulate into "L2 FIELD". From L2 field, two forking arrows: "SIGN yields QR2" and "MAGNITUDE yields payload". A note: "L2 overlay was applied to first N1*N2 frames additively — same mechanism as layered but over a continuous carrier."

Right side: A sketched diagram showing the stream decoder's internal state. A buffer bar fills up frame by frame. At each stride interval, a tick mark shows "emit L1". After N2 emissions, a larger tick mark shows "attempt L2 decode". Labels show the two counters: "last_l1_emit_end" tracking stride distance, and "l1_outputs collected: 0/N2, 1/N2, ... N2/N2 (decode)".

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "LOCK ON ANYWHERE" with smaller text: "the decoder does not need to know frame zero. any contiguous N1-frame window recovers QR1. the carrier looks the same at every offset. an observer cannot determine where encoding started."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": boundary-free continuous carrier, any-offset L1 decode, configurable stride overlap, L1+L2 with payload, stream decoder with progressive lock-on. Right column "LACKS": correlation-based decode (still uses accumulation), per-frame spatial scrambling, balanced chip schedules, key-gated naive resistance.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: can we leave grids entirely? What about hiding QR codes in AUDIO waveforms — 1D samples mapped to virtual 2D frames..." with "audio" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
