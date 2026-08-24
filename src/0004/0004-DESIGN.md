# DESIGN.md — Shrine 0004

**1000 - shrines - /of/ - SPIRIT**
An audio-visual artifact where the sound and the image are the same array of bytes.

---

## 1. Premise

A single HTML file that generates 90 seconds of 8-bit audio from four one-line
functions, and renders that same byte stream as a scrolling 240×240 image.

The image is not a visualisation of the audio. At 8 bits, a pixel value and a
sample value are the same number. The canvas and the audio buffer read from one
array.

Every visual artifact must arise from the data. No CSS filters, no decorative
RGB shift, no scanline overlay, no reverb. If a viewer asks why the image does
something, the answer is a line of code.

---

## 2. Guidelines (the constraints that define this shrine)

These are compositional rules, not technical limits. They exist so the piece can
be finished.

1. **No `Math.sin`, no floats in the generators.** Integer and bitwise
   operations only: `+ - * % & | ^ << >> ~`. Floats are permitted only in the
   final conversion to Web Audio's `Float32Array`.
2. **One line per layer.** Four layers, four expressions. Chaining is allowed.
   If this genuinely fails, it can be relaxed — but try hard first.
3. **The preset is hardcoded in the source.** No seed, no query string, no
   randomness. The four lines in the file *are* the score. View-source is the
   sheet music.
4. **The loop must close.** No crossfade. See §5.

---

## 3. Core numbers

| Constant | Value | Note |
|---|---|---|
| `SAMPLE_RATE` | 48000 | |
| `FRAME_W` | 240 | canvas width, samples per row |
| `FRAME_H` | 240 | canvas height, rows visible at once |
| `ROW_SAMPLES` | 240 | = `FRAME_W` |
| `ROW_MS` | 5 | 240 / 48000 |
| `WINDOW_S` | 1.2 | 240 rows visible |
| `DURATION_S` | 90 | |
| `TOTAL_SAMPLES` | 4320000 | 90 × 48000 |
| `TOTAL_ROWS` | 18000 | 4320000 / 240 |
| `TABLE_SIZE` | 256 | wavetable entries |
| `SCROLL_RATE` | 200 rows/sec | 1 row per 5 ms |

`TOTAL_SAMPLES = 2^8 × 3^3 × 5^4` — highly composite. This matters for §5.

---

## 4. Architecture — four layers

Each layer owns one time scale. This is the whole design.

| # | Layer | Time scale | Channel | Produces |
|---|---|---|---|---|
| 1 | Wavetable | none (a shape) | — | `Uint8Array(256)` |
| 2 | Index over time | per-sample | R | `Uint8Array(TOTAL_SAMPLES)` |
| 3 | Transform | per-window | G | `Uint8Array(TOTAL_SAMPLES)` |
| 4 | Stereo (right channel) | whole piece | B | `Uint8Array(TOTAL_SAMPLES)` |

### Layer 1 — Wavetable

A pure function of table position. No time, no state. 256 values, 0–255.

```js
wavetable = i => (i ^ (i >> 3)) & 0xFF
```

This is the hand-designed layer. It should be small enough to write, hear, and
rewrite in seconds. It is rendered as **row 0** of the image — the top row of
the canvas is literally the instrument, and everything scrolling below it is
what the instrument did.

### Layer 2 — Index over time (microsound)

Integer phase accumulator with deliberate overflow and feedback. Reads the
wavetable by truncation — no interpolation. The truncation error is part of the
sound.

Signature: `(t, table, state) => value`, where `state` is a small mutable object
persisting across samples.

```js
index = (t, T, s) => (s.p = (s.p + s.inc) % PHASE_MOD, s.inc = (s.inc + (T[(s.p >> 8) & 0xFF] >> 5)) % INC_MOD, T[(s.p >> 8) & 0xFF])
```

The feedback shift (`>> 5` above) is the single most expressive parameter: large
shifts give vibrato, small shifts give chaos. This is the tonal↔glitch rider.

### Layer 3 — Transform (macro)

Address manipulation over the buffer produced by layer 2. Operates on windows,
not samples. Reordering and re-addressing, not filtering.

Signature: `(buf, i) => value` — returns the value at output index `i`, free to
read any input index.

Candidate techniques (pick one, chain if needed):
- stride read: `buf[(i * k) % len]`
- XOR addressing: `buf[i ^ mask]`
- bit-reversed addressing
- run-length stretch/smear
- value sorting within a threshold band (pixel-sort / glissando)
- delta encode → corrupt → decode

### Layer 4 — Stereo (right channel)

**Revised from the original residue-echo design.** The residue/echo approach
below shipped first but turned out to be nearly inaudible in practice: it only
activates during layer 3's glitch half, and Track mode's cells are typically
built from exactly the clean half — so the echo was silent in the mode where
this shrine actually got used for composing. Removed rather than patched
around, in favor of turning layer 4 into the shrine's stereo width instead:
left channel = layer 3's output directly; right channel = layer 4, a plain
modulo-addressed read of layer 3's own buffer at a fixed sample offset
(`buf[(i + STEREO_OFFSET) % buf.length]`). Small offsets (~5-20ms) widen;
larger ones become an audible slap between speakers. No residue capture, no
mixing step — the two channels are exactly what plays and exactly what's in
the raw PNG/WAV export, with nothing in between.

Signature: `(buf, i) => value`, same shape as layer 3.

<details>
<summary>Original residue-echo design (superseded, kept for reference)</summary>

**Critical: this is not a reverb send.** Layer 4 receives the *discard buffer* —
the difference between the pre- and post-quantisation signal, plus anything
layer 3 truncated or dropped. It is made of what the process threw away.

Signature: `(residue, i) => value`.

The bed is always causally related to the main material because it is literally
made of the same bytes. It changes automatically when layers 2 or 3 change.

Implementation note: the render pipeline must explicitly capture and pass the
residue. If layer 4 is wired to receive the *output* buffer, the design has been
lost.
</details>

### Modulators (dev mode)

Layers 2, 3, and 4 each get one cycling LFO (Min/Max/Freq-in-ms, triangle
wave) applied to their one modulatable parameter — Feedback, Stride, and
Stereo Offset respectively — swapped inline into that layer's compiled
formula as a `triangleLFO(min, max, freqSamples, G_POS)` call instead of a
bare literal.

**Layer 1 deliberately has none.** The wavetable is a 256-entry table computed
*once*, with no time axis — a modulator measured in milliseconds can't move
meaningfully across only 256 fixed table slots (verified: at any allowed Freq,
the value never leaves the neighborhood of Min across the whole table). A
modulator box was built for it and removed once this was confirmed empirically
rather than left in a non-functional state.

**`G_POS`, not the formula's own `i`/`t`.** In continuous mode these are the
same thing. In Track mode they are not: each cell's own `i`/`t` resets to 0
every retrigger (that's what makes a repeated cell deterministic), but a
modulator's Freq is usually longer than one cell — reading the local index
made the modulator visibly "reset early" every cell instead of sweeping
smoothly. `G_POS` is a separate counter, set by the render loop immediately
before each per-sample formula call, that advances continuously across the
whole track regardless of cell boundaries. Consequence: once a modulator on
layers 2/3/4 is active, repeated cells at the same transposition are no longer
guaranteed byte-identical (their modulated parameter now depends on where in
the track they fall) — the cell cache keys on `semitones@trackOffset` instead
of `semitones` alone whenever any of the three modulators is enabled, and on
`semitones` alone (the cheaper, fully-shared path) otherwise.

---

## 5. Seamless looping

The loop closes by construction, not by fade.

**Rule:** every period in the system must divide `TOTAL_SAMPLES` (4,320,000).

Safe modulus set — divisors of 2^8 × 3^3 × 5^4. Useful values:
`240, 480, 960, 1200, 2400, 6912, 57600, 160000, 172800`.

Choose `PHASE_MOD`, `INC_MOD`, window lengths, and dither matrix size from this
set. `PHASE_MOD = 172800` is a good default (57,600 = exactly one frame is also
attractive).

**Seam check (build this).** After rendering, compare the first 1024 samples
against the continuation of the final state. Display the RMS seam error in the
UI as a small number. Do not auto-correct it. The shrine reports its own
discontinuity rather than concealing it — and it gives you a target to tune
parameters against.

---

## 6. Visual mapping

### Channels

- R ← layer 2 buffer
- G ← layer 3 buffer
- B ← layer 4 buffer
- Row 0 ← wavetable (256 entries mapped to 240 px)

Layers of different length or offset produce channel misalignment. This is
desirable and must not be corrected.

### Waterfall

The canvas is a **scrolling window**, not a frame-stepper. One new row enters
every 5 ms; the buffer scrolls upward. 240 rows visible = 1.2 s of history.

Implement as a circular buffer in an `ImageData`, not by redrawing 57,600 pixels
per frame.

Cover art = a snapshot of the window at a chosen moment.

### Palette LUT

A 256-entry lookup table sits between the byte value and the rendered colour.
The LUT is an *interpretation* of the data, not decoration — indexed colour is
native to this medium (GIF, VGA, Amiga).

The target aesthetic is **high-key and mostly empty**: a pale bone/cream field
with narrow bands of saturated chroma, not a smooth two-colour ramp. Reference:
the existing 0003 cover art.

LUT construction:
```js
{ base: '#FBF7E4', accent: '#D9418C',
  bands: [ {from: 92,  to: 104, color: '#3B3FD4'},
           {from: 168, to: 176, color: '#E8D24A'},
           {from: 232, to: 240, color: '#7BC48A'} ] }
```

Most of the range renders as `base`. Three or four narrow windows spike to
accent colours. The flatness produces the emptiness; the narrowness makes chroma
read as confetti rather than gradient.

Two main colours, plus small accents. Hardcoded per shrine.

### Dither — 4×4 Bayer

Ordered dithering, applied in scan order using `x = i % FRAME_W`,
`y = floor(i / FRAME_W)`.

**Ordered dither is periodic, so it adds pitch, not noise.** A 4×4 matrix
repeats every 4 samples and every 4 rows:

- horizontal period 4 samples → 12,000 Hz
- vertical period 960 samples → 50 Hz

Matrix size is therefore a tuning control:

| Matrix | Horizontal component |
|---|---|
| 4×4 | 12 kHz |
| 8×8 | 6 kHz |
| 16×16 | 3 kHz |

The dither is an oscillator that also makes the image look right. Expose matrix
size as a first-class parameter. Default 4×4.

---

## 7. UI

Total footprint **540 × 320**, 20 px outer margin, 20 px gutter.

```
┌─ 20px margin ────────────────────────────────────┐
│  ┌────────────────┐ 20 ┌────────────────┐        │
│  │                │    │ 1 wavetable    │ 40px   │
│  │   Visualizer   │    ├────────────────┤ 8px    │
│  │    240×240     │    │ 2 index    (R) │ 40px   │
│  │   waterfall    │    ├────────────────┤ 8px    │
│  │                │    │ 3 transform(G) │ 40px   │
│  │                │    ├────────────────┤ 8px    │
│  │                │    │ 4 stereo   (B) │ 40px   │
│  │                │    ├────────────────┤ 8px    │
│  │                │    │ palette/dither │ 56px   │
│  └────────────────┘    └────────────────┘        │
│               20px gap                            │
│  ┌────────┬──┬──────────────────┬────┬────┐      │
│  │ Render │▶ │    scrubber      │WAV │PNG │ 40px │
│  └────────┴──┴──────────────────┴────┴────┘      │
└──────────────────────────────────────────────────┘
```

Control row widths (total 500): Render 90, gap 10, Play 40, gap 10,
scrubber 250, gap 10, WAV 40, gap 10, PNG 40.

### Code fields

Four single-line inputs. Field border colour indicates channel (layer 1 amber,
2 red, 3 green, 4 blue) so the mapping is stated by the interface without a
legend.

### Render behaviour

- **No re-render while dragging.** Render happens on clicking Render, or
  automatically once a knob is released (native `change`, not `input`) — the
  knob-based UI (added after the initial code-field build) made this feel
  natural without violating the spirit of the rule: nothing re-renders mid-drag.
- While rendering, the button label changes to `Rendering` and animates
  (subtle pulse or progress fill). It is disabled during render.
- On syntax or runtime error in a field: mark that field's border as errored,
  show a one-line message, keep the last good buffer playing.
- Render off the main thread (Web Worker) if 4.3M samples blocks noticeably.

### Tooltip readout

On hover over the visualizer, show a small readout near the cursor:
`x 100 · y 110` / `val 58 · 0x3A`. Should also report which channel is under the
cursor if practical.

### Scrubber

Shares state directly with the scanline position — one clock, not two.

---

## 8. Playback

- Render offline into `Uint8Array` buffers, then convert to `Float32Array` and
  play via an `AudioBufferSourceNode` with `loop = true`.
- Offline rendering is deliberate: it means the whole piece exists as an
  addressable array that can be inspected, sliced, exported and drawn.
- Audio requires a user gesture to start. Make the Play button part of the
  design; the shrine is entered deliberately.

---

## 9. Export

- **WAV** — 8-bit stereo (revised from the original mono spec once layer 4
  became a real stereo channel rather than a mono echo bed), 48 kHz,
  interleaved, the exact bytes. No dithering to 16-bit.
- **PNG** — the current window, 240×240, indexed via the LUT.

Stretch goal, worth doing: an option to export the *raw* PNG (channel bytes
unmapped, no LUT). At 8 bits that PNG losslessly contains the audio — someone
can drag it back in and hear it. A single CC0 file that is simultaneously the
artwork, the data, and the source.

---

## 10. Constraints on the build

- **One HTML file.** No build step, no bundler, no `npm install`. Vanilla JS.
  If it needs a toolchain to run, it stops being forkable by a stranger and
  archivable by me.
- Must run from a static host and from its own standalone URL that can be used in an iFrame.
- Must still open in a browser in twenty years.
- CC0. Link the source explicitly on the page — do not rely on view-source
  being discoverable.

---

## 11. Non-goals

- Not a framework. Shrine 0004 is one instance. If an engine emerges, it will be
  *extracted* from three or four working shrines, not predicted now.
- No preset browser, no save/load, no sharing UI.
- No reverb, no filters, no continuous-domain DSP anywhere.
- No user seed. No randomness. This software and final music artifact are bound.

---

## 12. Open questions for the build

1. Does 4.3M samples render fast enough on the main thread, or is a Worker
   required from the start?
2. Does one line per layer actually hold for layers 3 and 4, or do those fields
   need to scroll?
3. Should the palette/dither strip be editable in the page, or fixed per shrine
   and only editable in source?
4. Does the wavetable-as-row-0 read clearly, or does it need to be a separate
   strip above the canvas?

---

## 13. Reference notes

- Ryan Maguire, *The Ghost in the MP3* — residue as material. Layer 4 is this.
- Oval / Markus Popp — Glitch aesthetic, and per-release arbitrary constraint as a finishing device.
  §2 is that.
- Bytebeat / demoscene — integer expressions as sound sources.
- General Aesthetic of Glitch, Digital, Eco-Futurism, Near Future, Structural Expressonism, Digital Biophilia
