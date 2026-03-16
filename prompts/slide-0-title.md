# Title Slide: qrstatic — Hiding QR Codes in Noise

## Style Brief

(Same as all content slides — warm parchment, pencil sketch, dense lab notebook feel.)

## Layout: Central Title with Evolution Timeline

## Prompt

Technical pencil sketch on warm aged parchment paper with subtle coffee stain marks, worn edges, and faint ruled lines showing through like notebook paper. Dense hand-drawn diagram in dark brown ink with slightly uneven hand-lettered labels. 16:9 aspect ratio. The composition should feel like the cover page of a researcher's lab notebook — the page you'd see when you first open the journal.

CENTER: A large, bold, hand-lettered title "QRSTATIC" in tall uppercase letters, drawn with careful but slightly uneven strokes as if inked with a technical pen. The letters have faint construction lines visible above and below, like hand-drafted lettering. Below the title, a thinner hand-lettered subtitle: "hiding QR codes in noise — eight experiments in steganographic accumulation". Below that, a thin horizontal rule.

UPPER HALF — EVOLUTION TIMELINE: A long horizontal timeline arrow spanning the full width of the page, drawn with a slightly wavy hand-drawn line. Eight numbered waypoints are evenly spaced along the timeline, each marked by a small circle. At each waypoint, a miniature iconic sketch represents the codec:

1. "XOR" — a tiny 4x4 grid with alternating black/white cells and an XOR symbol (circled plus)
2. "SIGNED" — a tiny grid with "+1" and "-1" labels and a small sum arrow
3. "BINARY" — a tiny square of dense dots suggesting TV static, with a small biased coin sketch
4. "ANALOG" — a tiny ascending curve (signal line) pulling away from a flatter curve (noise), suggesting the SNR graph
5. "LAYERED" — two tiny stacked rectangles, the upper containing a faint QR and the lower a different QR, connected by a recursive arrow
6. "SLIDING" — three tiny overlapping brackets along a mini timeline, suggesting sliding windows
7. "AUDIO" — a tiny waveform oscillating above and below a zero line, with one sample marked as flipped
8. "TEMPORAL" — a tiny grid of "+1/-1" chips with crossing permutation arrows converging into a correlation symbol

Each miniature is connected to its waypoint by a thin leader line. Below each waypoint, the codec name is hand-lettered in small capitals. The timeline has a subtle gradient of complexity — the early waypoints are simpler sketches, the later ones progressively denser, visually suggesting the increasing sophistication of the codecs.

LOWER HALF — THE ACCUMULATION METAPHOR: A large central illustration showing the core concept that unifies all eight codecs. On the left, a tall stack of overlapping frame rectangles drawn with slightly offset edges, suggesting a pile of noise frames. Each frame has tiny random marks suggesting noise. A large bold arrow labeled "accumulate" points rightward from the stack toward a single large rectangle on the right. Inside this rectangle, a clear QR code pattern emerges — drawn as a crisp grid of filled and empty squares. The transition from noise stack to clear QR is the visual anchor of the slide. Between the stack and the QR, faint intermediate states are sketched: after a few frames, a very faint hint of pattern; after many frames, a strong pattern. Three ghost rectangles at increasing clarity, labeled "N=1", "N=16", "N=64" with the final QR at full clarity.

BOTTOM LEFT: Hand-lettered in a slightly smaller but still prominent style: "a zero-dependency Rust crate" with a small hand-drawn Rust gear logo (the cog shape, not the word). Below: "for embedding recoverable QR codes in synthetic noise carriers and real-world signals."

BOTTOM CENTER: A small sketched properties summary in a rough box: "8 codecs | 3 frame types (u8, i8, f32) | payload channels | spatial permutation | correlation decode | detector scoring | streaming decode"

BOTTOM RIGHT: A torn-edge note pinned with a pushpin containing: "the journey: from XOR — the simplest possible accumulation — to temporal — correlation-based decode that defeats every known attack on the carrier." The word "every" is double-underlined.

Scattered across the margins, faint hand-written annotations as if the researcher jotted notes while planning the notebook: "signal grows as N, noise as sqrt(N)", "balanced chips: sum=0", "per-frame permutation defeats spatial analysis", "the boundary IS the vulnerability". These are placed at slight angles as margin scribbles, adding to the dense notebook authenticity.

No people, no faces, no color beyond dark brown ink on warm parchment. Dense margin annotations, thin leader lines, pushpins on note cards, worn page edges.
