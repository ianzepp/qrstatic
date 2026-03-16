# Slide 8b: Research Basis — Standing on the Shoulders of DSSS, Watermarking, and Reed-Solomon

## Style Brief

(Same as slide 1 — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Three-Column Prior Art with Central Convergence

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like a dense page from a researcher's lab notebook — specifically the page where the researcher connects their experimental work to established theory.

TOP: Bold hand-lettered title "8b. RESEARCH BASIS — THREE PILLARS" centered, with a double underline. Smaller subtitle: "temporal is not invented from scratch. it is grounded in three established bodies of work."

The main area is organized as three vertical columns, each representing one research pillar, converging at the bottom into the temporal codec design.

LEFT COLUMN titled "DSSS ACQUISITION & DETECTION" in a sketched tab with a pushpin:

A hand-drawn diagram of the direct-sequence spread spectrum concept. At top, a signal waveform labeled "data signal" — a slow square wave alternating between +1 and -1. Below it, a much faster oscillating sequence labeled "spreading code (PN sequence)" — rapid +1/-1 chips. A multiplication symbol between them produces a third waveform labeled "spread signal" that looks like modulated noise. Annotation: "the data is hidden by multiplying with a fast pseudorandom chip sequence."

Below, a receiver diagram. The spread signal enters from the left. It is multiplied by the SAME spreading code (labeled "matched filter / correlator"). The output is the recovered data signal. A bold annotation: "only the correct code concentrates the energy. wrong codes produce noise." A small graph shows a correlation peak — a tall spike labeled "correct code" surrounded by low flat responses labeled "wrong codes."

A pinned citation card at the bottom of the column: "Stojanovic et al., 'Code acquisition for DS/SS communications' — acquisition is a correlation-and-threshold problem. fixed-window detection is the correct narrow first stage." The text "correlation-and-threshold" is underlined.

A dashed arrow from this column downward labeled "temporal borrows: balanced +1/-1 chip schedules, matched-filter correlation, detector thresholding."

CENTER COLUMN titled "SPREAD-SPECTRUM WATERMARKING" in a sketched tab with a pushpin:

A hand-drawn diagram showing the watermarking concept. At top, a large rectangle labeled "host media (image, audio, video)" with a sketch of a mountain landscape inside. Below, many tiny arrows pointing into different regions of the media, each arrow labeled with a tiny "+w" or "-w" representing weak watermark contributions scattered across the host. Annotation: "many weak, widely distributed contributions — invisible in the host signal."

Below, a detection diagram. The watermarked media enters a "detector" box alongside the "known watermark pattern." The detector outputs a score on a number line. A clear separation is shown between a cluster of low scores labeled "unwatermarked / wrong key" and a single high score labeled "correct watermark detected." The gap is cross-hatched and labeled "detector separation." Annotation: "judge success by detector separation between correct key and incorrect keys — not by visual inspection."

A pinned citation card: "Cox et al., 'Secure spread spectrum watermarking for multimedia' — treat the carrier as host media. spread many weak contributions widely. judge success by detector separation." The text "detector separation" is underlined.

A dashed arrow from this column downward labeled "temporal borrows: host-distributed signal, invisible per-frame contributions, detector-score-based acceptance."

RIGHT COLUMN titled "REED-SOLOMON ERROR CORRECTION" in a sketched tab with a pushpin:

A hand-drawn diagram of the RS coding concept. At top, a row of data blocks labeled "k data symbols" feeding into an encoder box that produces a longer row labeled "n coded symbols (n > k)." Some of the coded symbols are crossed out with X marks, labeled "erasures (lost packets)." An arrow leads to a decoder box that receives the surviving symbols and outputs the original "k data symbols recovered." Annotation: "can recover from up to n-k erasures per block."

Below, a small diagram showing a packet grid — rows represent FEC blocks, columns represent packet positions within each block. Some cells are shaded (received) and some are empty (lost). RS decoding arrows span across each row, recovering the missing cells from the surviving ones. Label: "packet-block erasure correction — standard, not bespoke."

A pinned citation card: "Geisel / NASA JPL, 'Tutorial on Reed-Solomon error correction coding' — reuse standard RS coding at the packet layer with erasure-oriented decoding instead of inventing codec-specific parity logic." The text "reuse standard" is underlined.

A dashed arrow from this column downward labeled "temporal borrows: packet-block FEC for Layer 2, erasure-first recovery, standard framing."

BOTTOM CENTER — CONVERGENCE: The three dashed arrows converge into a large rounded box labeled "TEMPORAL CODEC" drawn with a double border. Inside, a diagram shows the three contributions merging:

Left input arrow: "DSSS" feeds into "Layer 1: balanced chip correlation" — a row of +1/-1 chips multiplied by frame samples.
Center input arrow: "WATERMARKING" feeds into "host-distributed embedding" — many tiny signal contributions scattered across a noise carrier grid.
Right input arrow: "REED-SOLOMON" feeds into "Layer 2: packet FEC" — a packet grid with RS parity blocks (shown as future/planned with dashed lines).

Below the convergence box, a bold hand-lettered annotation: "invent ONE new thing: the stego physical layer. reuse EVERYTHING else." with "one" and "everything" double-underlined.

BOTTOM LEFT: A sketched note card titled "WHAT WE TOOK" listing: "from DSSS: correlation decode, balanced codes, threshold detection" then "from watermarking: invisible host embedding, detector separation metric" then "from RS/FEC: packet erasure recovery, standard framing (Stage 2+)".

BOTTOM RIGHT: A sketched note card titled "WHAT WE DID NOT TAKE" listing: "from DSSS: blind synchronization search (Stage 1 uses fixed windows)" then "from watermarking: frequency-domain embedding (we stay in spatial/temporal)" then "from RS/FEC: full implementation (Stage 2 future work)" with annotation "each pillar contributed its simplest applicable idea first."

Scattered margin annotations: "Walsh/Hadamard and Gold codes are follow-on experiments, not required for Stage 1", "acquisition literature says: test, threshold, declare — that is exactly correlate_prefix + detector_score + policy", "the carrier IS the host media — Cox's insight applied to synthetic noise."

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards, worn page edges.
