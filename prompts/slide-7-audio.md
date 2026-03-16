# Slide 7: Audio — QR Hidden in Sound

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Encode/Decode Split with Margin Notes

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook.

TOP: Bold hand-lettered title "7. AUDIO — QR HIDDEN IN SOUND" centered, with a double underline. Smaller subtitle: "1D audio samples mapped to a virtual 2D grid. imperceptible modification."

The main area is divided into upper ENCODE and lower DECODE sections by a hand-drawn horizontal dashed line.

UPPER SECTION labeled "ENCODING" in a sketched tab:

Far left: A hand-drawn audio waveform sketch showing a typical audio signal — a wavy line oscillating above and below a horizontal zero axis, labeled "COVER AUDIO" with a small speaker icon. Below it, a second nearly identical waveform labeled "ENCODED AUDIO" with annotation "sounds identical — only sign distribution shifted". Between them, a magnified detail bubble showing a few samples: one sample labeled "+0.3" with an arrow flipping it to "-0.3" and text "sign flipped (P=0.4)", another sample labeled "-0.7" staying as "-0.7" with text "sign already matches — kept".

Center: A large domain-mapping diagram titled "1D to 2D MAPPING". A horizontal strip of numbered audio samples (0, 1, 2, ... 4095, 4096, 4097, ...) at the top. Curved arrows map samples into cells of a square grid below. The grid is labeled "virtual 64x64 frame" with annotation "frame_size = 64*64 = 4096 samples". The mapping rule is written as: "sample_index % frame_size gives virtual cell position". Samples 0-4095 fill the first virtual frame, samples 4096-8191 fill the second, and so on. A bracket groups N virtual frames with label "accumulate N virtual frames".

Right side: A pinned note card titled "SIGN-FLIP RULE" containing the decision logic as a small flowchart. Top: "for each sample". Branch: "does sample sign match desired QR polarity at this virtual cell?" Yes branch: "keep sample unchanged". No branch: "flip sign with probability P" then "P = flip_probability (default 0.4)". Below: "desired polarity from QR: white module = positive, black module = negative".

Top margin annotation: "Vec<f32> raw samples, frame_size must be perfect square, flip_probability in [0.0, 1.0], n_frames >= 1"

LOWER SECTION labeled "DECODING" in a sketched tab:

Left side: A stream of audio samples flowing left to right as a waveform. Below, each sample is assigned to a virtual grid position via the same modular mapping. A counter shows "sample_count" incrementing, and when "sample_count % frame_size == 0" a tick mark shows "virtual frame complete". A second counter "frame_count" increments toward N.

Center: The accumulation diagram. N virtual frames worth of samples have been summed into a single 2D grid. Each cell shows an accumulated float value. The grid has a visible pattern — some cells strongly positive, some strongly negative, corresponding to QR modules. Labels: "after N frames: signal >> noise in each cell". A sign-threshold operation produces the recovered QR grid below, with label "threshold at 0 yields QR". Radiating emphasis lines around the QR.

Right side: A sketched comparison panel titled "WHY IMPERCEPTIBLE?" showing three points. First: a tiny waveform snippet with one sample flipped, labeled "single flip: amplitude preserved, phase shift inaudible in complex audio". Second: "flip_probability = 0.4 means only 40% of mismatched samples change". Third: "the ear integrates over thousands of samples — individual sign changes vanish in the mix". Below: "no payload channel — QR only. the cover audio IS the carrier."

BOTTOM LEFT: Bold hand-lettered callout in a rough box: "DOMAIN SHIFT" with smaller text: "all previous codecs generated synthetic 2D frames. audio works with real-world 1D signals — music, speech, noise. the cover content IS the carrier. modification is imperceptible. but we lose the payload channel and multi-layer structure."

BOTTOM CENTER: Properties table in a sketched frame. Left column "HAS": real-world cover signal, imperceptible modification, 1D-to-2D virtual mapping, sample-by-sample streaming, works on any audio. Right column "LACKS": payload channel (QR only), multi-layer structure, spatial scrambling, synthetic carrier control, correlation-based decode.

BOTTOM RIGHT: Torn-edge paper with arrow pointing right: "NEXT: what if we combined the best ideas? Correlation-based decode with balanced chip schedules, per-frame spatial permutation, detector scoring, and naive-accumulation resistance — all in one codec..." with "temporal" double-underlined.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards.
