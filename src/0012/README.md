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
radio, crush, dropouts) you can combine freely; the **Overlay** settings drift the
exported source (`shrine.rs`) up through the sky behind the shrine — *Code
text size* scales it (100% = normal, 200% = double), *Code angle* tilts the
drift (positive leans left) so it travels along that tilted axis, and *Code
vertical* shifts the whole drift up or down to re-centre it. The engine can
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
`~/.config/shrine0012/0012.cfg` (as compact words) and can be written out as a
human-readable `.shrineseed` file (menu: **Export seed** / **Import seed**; CLI:
`--export` / `--import`). Anyone with the file can rebuild the exact shrine.

```sh
cargo run -- --export shrine.shrineseed
cargo run -- --import shrine.shrineseed
```

## Determinism

Identical settings produce bit-identical audio and frames. This is locked by
tests:

```sh
cargo test --workspace          # includes the golden hashes
node web/verify.mjs             # native <-> WASM parity (after ./build-web.sh)
```

`engine/tests/audio_golden.rs` fixes the music, `engine/tests/parity_golden.rs`
fixes the coupled audio and the rendered frames, and `web/verify.mjs` proves the
browser matches Rust exactly.

## Commands

```
cargo run                                   interactive setup + live preview
cargo run -- --preview [SEED] [--biome] [--weather] [--out FILE]
cargo run -- --cover  [SEED] …              1600×1600 PNG
cargo run -- --wav    [SEED] [SECONDS] …    plain WAV
cargo run -- --emit   [--seed N] [--biome] [--weather] [--out FILE]
cargo run -- --export / --import FILE.shrineseed
cargo run -- --serve  [PORT]                serve web/ locally
```

## Constraints

See `CONSTRAINTS.md` — the artifact must have zero external dependencies, a
300×300 logical buffer, a 1200×1200 canvas, no third-party names, and no
`wasm-bindgen`. `scripts/naming-audit.sh`, `scripts/check-zero-deps.sh` and
`scripts/check-wasm.sh` enforce these.
