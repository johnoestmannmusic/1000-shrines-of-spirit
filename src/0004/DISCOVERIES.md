# DISCOVERIES.md — Shrine 0004

Notes from the first build of shrine 0004, kept separate from
[0004-DESIGN.md](0004-DESIGN.md) because they're not the spec — they're what
was *learned* by building against that spec, worth carrying into a restart
rather than rediscovering.

---

## The big one: software and music as one artifact

An HTML file can be the software, the music, *and* the visual — not a player
for a track, and not a visualizer of one. The same byte array drives both the
audio synthesis and the canvas rendering, and the file itself — viewable,
forkable, CC0 — is the thing worth hosting as the main artifact on a track
page, not a demo sitting next to a separately-rendered audio file.

This is the idea the whole *1000 shrines of SPIRIT* project rests on. Shrine
0004 is where it actually clicked into being obviously true, rather than just
a nice line in a design doc.

---

## What's creatively settled (don't re-litigate this in a restart)

- **The glitch aesthetic and the resulting sound are right.** The per-sample integer-generator approach, the general "digital glitch" character — this identity is the point, and a
  restart should protect it rather than treat it as one option among several.
- **Stereo adds a lot.** Layer 4 reading layer 3's own line back at a small
  sample offset (instead of the original residue-echo idea, which turned out
  to be nearly inaudible in practice) gave real width and life. Worth
  building early, not as a late nice-to-have.
- **The Cycle / Cell sequencer (Track mode) was probably the single most
  important addition.** A short cell, repeated rhythmically, with occasional
  transposition, is what turned this from "an interesting generator" into
  "something I'd actually compose with." If a restart only gets to rebuild
  one piece of behavior early, this is the one.
- **Visuals driven directly by the audio's own bytes, laid out with the 240x240px
  canvas beside the layer controls, is a strong UI pattern.** The
  specific layout — canvas on the left, a column of per-layer control boxes
  on the right, colour-coded to match — read clearly. Worth reusing the
  *shape* of that layout even if the specific knobs in each box change.

---

## What's settled about how to build the next one

- **Sliders taught the concepts better than reading the code did.** Turning a
  labeled knob and immediately hearing/seeing what it does built real
  understanding in a way that reading the one-line JS formulas underneath it
  didn't. Any restart should keep exposing meaningful parameters as live,
  interactive controls from early on — not just as a UI nicety, but as the
  actual teaching mechanism.
- **Build it slower next time.** This build stacked a lot of real features
  quickly across one long session — knobs, dev/audience modes, the track
  sequencer, stereo, per-layer LFO modulators — and the pace outran the
  ability to actually absorb what each piece was doing before the next one
  landed. None of the individual features were the problem; the batching and
  speed were. By building in a way that the chain of layers can be disabled to experience the exact effects of one layer at a time is a great way for the user to understand the construction.

---

## Technical gotchas worth not re-discovering

A few sharp edges surfaced during the build that a restart can just avoid:

- **A phase increment that evenly divides a highly-composite modulus can
  freeze forever.** The wavetable is 0 at index 0, so gating pitch-feedback
  updates on the phase's own wrap point (rather than a fixed wall-clock) let
  many increments land on that silent index every cycle and never move again.
  Gate on an independent clock, not on the thing you're trying to update.
- **A "modulator" needs an actual time axis to modulate against.** The
  wavetable layer is a 256-entry table computed once, with no notion of
  playback time — an LFO measured in milliseconds can't move meaningfully
  across 256 fixed slots. Not every parameter is modulatable just because it's
  a knob.
- **Per-cell state resets are great for determinism and terrible for slow
  modulation.** Track mode resets each cell's local sample index to 0 so
  repeats are byte-identical — but a modulator with a longer period than one
  cell would "reset early" every retrigger if it reads that same local index.
  A modulator needs its own clock that ignores cell boundaries, which in turn
  means repeated cells stop being cacheable the moment a modulator is active.
- **Out-of-bounds typed-array reads fail silently, not loudly.** `buf[i]` past
  a `Uint8Array`'s length returns `undefined`, and `undefined & 0xFF` is `0` —
  a plausible-looking number, not a crash. This exact thing quietly wrecked a
  seam-quality measurement for a long stretch of the build before it was
  caught by noticing a result that didn't change when it should have.
- **A UI indicator computing a value independently is not the same as
  verifying the render used that value.** Trust the actual rendered
  buffer/audio, not a second implementation that merely looks plausible next
  to it.
