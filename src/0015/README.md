# ShrineGen

A procedural isometric shrine: a single, deterministic software-art artifact.
The visuals and the music are one coupled system — every pixel and every sample
is generated from code, and the whole thing compiles to WebAssembly.

There are no binary art assets, no runtime dependencies, and no CDN: the audio
engine and the renderer are standard-library-only Rust, and the emitted artifact
builds with bare `rustc`.

## Where things live

| Path | What |
|---|---|
| `engine/` | the artifact: zero-dependency isometric framework, shrine/plant/weather generators, the music engine, and the coupled simulation clock |
| `src/` | the host: the setup TUI, previews, `--emit`, the WAV/cover exporters, the dev server, the seed files |
| `web/` | the uploadable page: `index.html` + `shrine.worklet.js` + `shrine.wasm`, plus `shrine.rs` (the exported source, for the code overlay) and `wav/` (an optional bundled render) |

## Run it

```sh
cargo run                 # the setup menu
```

The menu has a live visual preview on the right. Keys: `↑↓` move, `←→` change,
`Shift+←→` bigger steps, `Enter` act/edit, `r` randomise, `q` quit. Choose the
seed, biome, weather, and every part of the music; set a **Track name** (it heads
the page and titles the WAV); toggle **Audition** to hear it live; **Master FX**
opens a screen of end-of-chain glitch effects (Tape, CD artifacts, pitch drift,
radio, crush, dropouts) you can combine freely; the **Glitches** opens a per-layer screen (kind on/off, how often, step rate, level,
pattern length, mutation cadence, retriggers) for the two glitch layers;
**Architecture editor** paints the 16x16 shrine — plan grid beside the live
preview, height/kind/variation per cell, undo, and a key to hand it back to the
seed; **Reset to defaults** starts a fresh project. **Drone rhythm** switches on a
rhythmic amplitude LFO for the drone, shaped by **Seed 7** (off by default).
Cells in the architecture editor run from **-6 to +6**: positive stacks tiles up,
negative stacks the same blocks *downwards* below the ground plane, and a height
can be typed exactly. The drone rhythm can also be applied to the atmosphere
loops (**Loop rhythm**). There are seven **weathers** — Clear, Rain, Wind, Fog,
Storm, Snow and Dusk — each with its own particles, light and sound: motes and
shafts of light, layered rain splashing on the stone, blown leaves and gust
whistles, drifting fog and drips, lightning with its thunder arriving after the
flash, snowfall whitening the tops, and rising embers under a low sun. The
picture and the weather sound play from the same levels and the same events, so
heavier rain is both denser and louder, and every splash you see is a drop you
hear. **Wildlife** (off by default) adds a few creatures chosen by the biome and
seed — frogs and doves in moss, songbirds and crickets in verdant growth,
cicadas, owls and fireflies among the fungi — whose calls thin out in bad
weather. The atmosphere layers offer twenty loop timbres, from Voices and Bells
to Drift, Tide, Ember, Ripple, Ebb, Bronze, Frost, Lantern and Loom.
The shrine itself is a ruin of brutalist concrete poured over weathered
stone: board-formed walls with tie holes and water stains, ribbed pillars, slab
roofs, ashlar footings and flagstones, carved friezes and glowing rune panels,
cracks, chipped edges and creeping moss. Every texture is fixed to its block,
and every weather lights it differently — its own sun, sky light, cast shadows,
occlusion, rim light and altar glow — in stepped, ordered-dither shading.

**Four built futures** join the three wild biomes (Moss, Verdant, Fungal). Each
re-makes the same blocks in its own materials, lays its own ground out around
the shrine, furnishes it with props, and adjusts the weather's light:

- **Near Future** — a bright city block. Pastel tiled buildings with window
  bands, roller-shuttered shops and narrow lit signs, a vending-machine altar;
  roads with centre lines, zebra crossings and manholes, sidewalks with curbs
  and yellow tactile paving; street lamps, utility poles strung with sagging
  wires, traffic signals, bollards and small parked cars.
- **Research Lab** — brushed steel, glass curtain walls and blue-green neon
  columns, a UI-screen altar, a floor with glowing light lines, server racks,
  terminals and glass partitions, under a steel-and-glass ceiling.
- **Ecofuture** — white titanium panels with vents and status lights, living
  walls, green roofs and solar tiles, a glowing bio-reactor altar; mown lawns
  with curving white paths, wind turbines turning with the wind, round trees,
  sensor masts, hydroponic pods and a hovering drone.
- **Greenhouse** — a glass atrium on minimal steel scaffolding: glazing with
  plants pressed against it, a pool altar, terracotta tiles, gravel and
  planting beds, tree ferns, misting heads and hanging baskets, sun shafts
  slanting through the pitched glass roof.

Lab and Greenhouse are **roofed**: rain splashes land on the glass (the same
splashes the sound plays), beads run down the panes, snow settles on the roof
as frost, nothing inside gets wet, and the weather is heard muffled through
glass. Lights and screens are lit from within, so they stay bright in a storm.
Seeds chosen on the command line still land on a wild biome; the futures are
picked by name (`--biome near-future`, `research-lab`, `ecofuture`, `greenhouse`).

**Mode** sets how the music moves: *half-time* and *jungle* (the chopped
break), **2-step garage** — melodic, optimistic, swung — or **microsound**. In garage:

- the drums play a 2-step loop at the track's own tempo: a kick on the one and
  a second that skips past beat three, claps on two and four, rimshots on the
  pushes, shuffled hats with open hats on the off-beats; sections move between
  hats, a kick-less breakdown, the full 2-step and 2-step with glitch edits
  (hat stutters, reversed claps, dropped kicks), all swung by **Swing**
  (50–66 %);
- the chords follow a **Progression** — the seed's pick from the scale's
  family, or one chosen by name — with chords borrowed from outside the key
  where it asks (secondary dominants, a borrowed iv, ♭VII): *Royal road*
  (IVmaj7 V7 iii7 vi7), *Royal road, bright* (… III7 …), *Soft turn*,
  *City lights*, *Sunrise*, *Menu loop*, *Wistful*, *Two-five-one*,
  *Pop loop*, *Daydream*, and in minor *Night drive*, *Neon*, *Climb*,
  *Rain city* and *Descent*. They change every two bars on the downbeat;
  **Key lift** (*every 4* or *every 8 loops*) steps the whole piece up a
  whole tone for as long, then home. The drone holds an open fifth on the
  tonic beneath, and steps back under any chord that rubs against it;
- **Bass** *rhythmic* plays a syncopated, swung bassline on the chord roots,
  octaves and fifths that walks into the top of the loop (*drone* keeps the
  sustained bass);
- **Stabs** (*off / sparse / busy*) play voice-led seventh chords on the pushes
  in a call and answer, in a *dark* electric-piano or *bright* bell-organ
  **Stab tone**, through a dotted-eighth echo;
- the drone steps back and the atmosphere loops come forward, voiced as open
  shells (root, fifth, tenth) above the stabs; bells and glitch pings keep to
  whole-number overtones, the glitch layers pitch only to the root and fifth, and at each change the stab echo clears and the reverb send dips, so
  no chord rings over the next.

Choosing garage moves a default tempo to 132 BPM, Lydian to Ionian and three
chords to four; anything set by hand is kept. Two bright Japanese scales join
the Scale list for every mode: the major pentatonic (*yonanuki*) and *Ryukyu*.

**Microsound** works at the smallest scale of sound — clicks a few samples
long, grains of a few milliseconds, short sine pips — on a faint, exact
pulse (no swing, no human timing):

- **Pulse** *sub* puts a sub-sine kick on every beat and a sub pulse on the
  chord root on the off-beats; *sub + click* adds a click to each kick and a
  few on the off-beats; *off* leaves only the small sounds. Ticks and grains
  fall on the sixteenths, panned anywhere across the field, and skip the
  sampler colour, so each stays as sharp as it was made. Sections move
  between clicks alone, the pulse, and the pulse failing more often;
- **Dropouts** (*off / some / many*) are its edits: a step drops out, skips
  back to repeat the one before, or ratchets four times in a sixteenth;
- **Grains** (*sparse / medium / dense*) sets how many ticks and grains the
  loop holds, and how thick the cloud the bed becomes: the atmosphere loops
  and the drone are listened to and scattered back as 20–80 ms grains;
- **Pips** (*off / sparse / busy*) play chord tones as pure sine pips of
  5–40 ms, three or four octaves up, through a small ping-pong echo;
- the drone is frozen from pure sines (no FM), thin and well back; the glitch
  layers come forward, there is less reverb, and the whole sits about 3 dB
  lower.

Choosing microsound moves a default tempo to 120 BPM, the scale to the minor
pentatonic and the chords to two (and back again when you leave), and keeps
the Kit to the light kits.
**Camera · Rotation speed** orbits
the whole viewport around the shrine, in degrees per second (default 0); the
**Code plane** setting puts the scrolling source on a *static* plane, a *flat*
one that slides with the camera, or a *sphere* that wraps around the viewer so
panning reveals it on both sides. Every numeric row can be
nudged with the arrows *or* typed exactly with Enter. The
**Overlay** settings drift the
exported source (`shrine.rs`) up through the sky behind the shrine — *Code
text size* scales it (100% = normal, 200% = double), *Code angle* tilts the
drift (positive leans left) so it travels along that tilted axis, and *Code
vertical* shifts the whole drift up or down to re-centre it. The **Architecture**
controls turn the shrine about x, y and z (any angle, so it can be tipped onto
its side or pushed through the ground — the ground plateau hides whatever goes
below it, though it still shows past the plateau's edge) and offset it in x, y and z; the default
is centred and upright, and the *model* stays the centred 16x16 footprint
whatever the transform says. The engine can
leave the sky unfilled and reports the lit sky colours it would have used; the
page paints that sky plus the scrolling source into the canvas itself, then
draws the frame over
it, so the shrine occludes the code and nothing smears or ghosts when the camera
moves; **Cover PNG** saves a 1600×1600 still; **Export WAV** opens a screen that
writes a WAV with metadata (the track name as its title) and an embedded
1600×1600 cover.

## The workflow (menu → artifact → browser)

```sh
cargo run                       # choose settings (remembered between runs)
# inside the menu: Compile WASM  → emits the artifact and writes web/shrine.wasm
cargo run -- --serve            # open http://127.0.0.1:8080/
```

**Compile WASM** writes `artifacts/ShrineGen-<seed>.rs` (the whole piece, as one
readable file), `artifacts/ShrineGen-<seed>.shrineseed` (the settings), compiles
it to `web/shrine.wasm`, and leaves the page ready.

## Releasing

```sh
scripts/build-dist.sh [SEED]     # → dist/ (page + worklet + wasm + source + wav)
```

Drop a rendered track at `web/wav/track.wav` before releasing and the page shows
a **Download WAV** button beside the recipe; without it the button stays hidden.
The recipe card also ends with the licence lines (music CC0 / public domain,
code MIT), baked into the artifact's info card.

Upload the files in `dist/` to any static host (`shrine.rs` is only needed for
the code overlay; the page works without it). `SOURCE_DATE_EPOCH` is honoured,
so two builds of the same settings are byte-identical.

## The recipe

Every shrine is fully described by its settings. They live in
`~/.config/shrine0015/0015.cfg` (as compact words) and can be written out as a
human-readable `.shrineseed` file (menu: **Export seed** / **Import seed**; CLI:
`--export` / `--import`). Anyone with the file can rebuild the exact shrine.

```sh
cargo run -- --export shrine.shrineseed
cargo run -- --import shrine.shrineseed
```

## Determinism

Identical settings produce bit-identical audio and frames, however the audio is
cut into blocks (the browser's 128 frames or the exporters' 4096). That is what
makes **Start at** work: a start offset (0–600 s) is the very same piece joined
part-way. The audio is fast-forwarded exactly at startup — about a second or
two per minute of offset in the browser — then fades in over 1.5 s; the visuals,
weather and wildlife simply begin at that time. The offset is part of the
recipe, so the page, the audition, previews and WAV exports all start there.
This is locked by tests:

```sh
cargo test --workspace          # includes the golden hashes
node web/verify.mjs             # native <-> WASM parity (after ./build-web.sh)
```

`engine/tests/audio_golden.rs` fixes the music, `engine/tests/parity_golden.rs`
fixes the coupled audio and the rendered frames — including each built future
playing 2-step garage, whose recipes and hashes it also writes to
`web/parity-futures.json` — and `web/verify.mjs` proves the browser matches
Rust exactly.

## Commands

```
cargo run                                   interactive setup + live preview
cargo run -- --preview [SEED] [--biome] [--weather] [--out FILE]
cargo run -- --cover  [SEED] …              1600×1600 PNG
cargo run -- --wav    [SEED] [SECONDS] … [--mode garage|micro] [--bpm N]   plain WAV
cargo run -- --emit   [--seed N] [--biome] [--weather] [--out FILE]
cargo run -- --export / --import FILE.shrineseed
cargo run -- --serve  [PORT]                serve web/ locally
```

## Constraints

See `CONSTRAINTS.md` — the artifact must have zero external dependencies, a
300×300 logical buffer, a 1200×1200 canvas, no third-party names, and no
`wasm-bindgen`. `scripts/naming-audit.sh`, `scripts/check-zero-deps.sh` and
`scripts/check-wasm.sh` enforce these.
