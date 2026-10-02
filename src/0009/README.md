# 0009
02 OCT 26

An endless Glitch Ambient track built as a software artifact. A frozen, complex chord and a bass drone hold still underneath, while two cycling layers of glitch artifacts loop, mutate and drift out of phase above them.

The piece is a Rust program, and it runs three ways:

- **Live in the terminal**: plays forever, with an animated tour of the engine that doubles as a visualiser and as documentation of how it works.
- **WAV render**: any length of the piece as a 24-bit / 48 kHz file.
- **In a browser**: the same engine compiled to WebAssembly.

All three produce **bit-identical audio**, and are designed to keep doing so in 20 years (see *Determinism*).

## Project layout

| Folder | What it is | Dependencies |
|---|---|---|
| `engine/` | The music itself, plus `render-0009`, a WAV renderer | **none** |
| `src/` | The `shrine-0009` app: live playback, TUI, render, or both | `cpal` (sound card), `ratatui` (terminal) |
| `web/` | Browser player (`index.html`, `worklet.js`, `track.wasm`) | none |

The split is deliberate. Everything that decides the sound lives in `engine/` and uses nothing but the Rust standard library. The app's crates only connect that sound to a sound card and a terminal. If those crates ever stop building, the track, the WAV renderer and the web player are unaffected.

## Run it

```
cargo run --release
```

The screen clears to the **GlitchAmbiToolkit** title, with the version underneath, then asks its questions in a box. Enter accepts the default shown, and Esc goes back a question:
1. **What to do:** play live, render to WAV, play live and record at the same time, or exit.
2. **When playing:** the title shown in the player, up to 4 characters (default `0009`).
3. **The four seeds.** The defaults shown (1000, 9, 1009, 2026, from `DEFAULT_SEEDS` in `engine/src/lib.rs`) are the canonical version of the track. Press Enter to keep each one, or type any number (decimal or `0x` hex) for an alternate rendition.
4. **When rendering or recording:** the file name (`.wav` is added if missing, and it asks before overwriting) and the length (seconds, `m:ss` or `h:mm:ss`, up to 4 hours, the WAV limit at this quality).

Rendering shows a progress bar in the same box (Esc cancels, leaving a valid but shorter file); playing goes straight into the player. When a render finishes or you leave the player, you're back at the menu, with the outcome shown at the top of the box. Choose **Exit**, or press Esc at the menu, to close the app. Ctrl-C quits from anywhere. The version is the build date (`vYYYYMMDD`), stamped by `build.rs` whenever the app's sources change; set `SOURCE_DATE_EPOCH` for a reproducible stamp.

Rendered and recorded files fade out over their last 40 s (over the last quarter for anything shorter than 2:40). Live playback never ends.

For scripts, pass flags and nothing is asked:

```
cargo run --release -- --mode play|render|both --label 0009 --seconds 10:00 --out 0009.wav --seed1 N … --seed4 N
```

The dependency-free renderer works the same way, minus live playback: `cargo run --release -p shrine-0009-engine`.

## The live TUI

Keys:
- **Tab** (or **1** / **2**) switches between the two views.
- **← →** step through the explainers.
- **Space** pauses. When recording, the recording pauses too.
- **q** (or Esc) fades out and returns to the menu. A recording stopped early gets a 1.5 s fade at that point. **Ctrl-C** does the same, then exits the app.

**1 · Pipeline view** (the default). Every chain from the signal-flow diagram, drawn large, with the sound shown as particles travelling through it. Each particle is spawned by a real engine event at the moment you hear it:
- a drone grain (`•`) every time a new FFT frame starts sounding, splitting at the filter into the dry path and the reverb send;
- a lettered particle for every glitch hit (`B T S P C X`), sent into the echo;
- dimmer copies (`○`) leaving the echo at its real delay time, each fainter by the feedback amount;
- a stream out of the reverb that thickens with its level, and the bass going straight to the output, skipping the reverb.

Boxes glow with their part's level and show live values. Their numbers point to the matching panels in the engine view. Underneath, **What's changing** logs pattern mutations and glitch layers thinning out or filling in, as they happen.

**2 · Engine view.** One panel per part of the engine, each a live visualisation that also shows how its part works:

| Panel | What it shows |
|---|---|
| 1 · FFT freeze | The frozen magnitude spectrum on a log-frequency scale. The low-pass sweeps across it, the cutoff is marked, and the bar tops glint each time a new frame re-rolls every bin's phase. |
| 2 · Overlap-add | The four Hann-windowed grains currently overlapping (plus the one being built), scrolling past a "now" line, with Σw² = 1.50 |
| 3 · FM chord | Every chord note with its FM wiring and ratios, as an animated waveform |
| 4 · Glitch layers | Both step sequencers: current step, ratchets, muted steps, flashes on each hit, sounding voices, and the latest mutation |
| 5 · Slow cycles | The seven prime-period LFOs and the six controls they drive |
| 6 · FM bass + sub | FM index, low-pass cutoff, and both waveforms |
| 7 · Ping-pong echo | Echo level over time: left above, right below |
| 8 · Reverb | The 8 delay lines by length and level, next to the Hadamard mixing matrix |
| 9 · Output | Oscilloscope, stereo goniometer and level meters |

**Explainer ticker.** Along the bottom, 55 explainers about the software appear one at a time. Each types on quickly, then holds still long enough to read, timed to its length. Whichever part the current explainer describes is marked with a ◆ in both views.

The TUI needs a terminal of at least 80×22 (94×30 for the pipeline view) and looks best at about 140×46 with true colour. `cargo run --release -- --frame 75` prints both views as plain text, as they would look 75 s in, without needing a sound card.

## Play in a browser

```
./build-web.sh          # builds web/track.wasm
cd web && python3 -m http.server
```

Then open http://localhost:8000 and press Play. Alternate renditions take URL parameters: `?s1=1&s2=2&s3=3&s4=4`. The player has no build step and no JS libraries.

## Signal flow

```
seed 1 → 3-op FM chord → random-phase FFT freeze → cyclic long filter mod ─┐
seed 2 → glitch artifacts 1 → cyclic repeats 1 ─┐                              │
seed 3 → glitch artifacts 2 → cyclic repeats 2 ─┴→ echo / delay ───────────────┼→ long reverb
seed 4 → FM bass + sub-bass → low-pass ────────────────────────────────────────┘
```

| Stage | File (in `engine/src/`) |
|---|---|
| FM chord, spectral freeze (random-phase resynthesis, 32k FFT, ¼ hop) | `drone.rs`, `fm.rs`, `fft.rs` |
| Slow cyclic modulation (LFO periods of 41–307 s, all prime) | `modulate.rs` |
| Glitch voices: crushed FM blips, noise ticks, drone stutters, bell pings, square dropouts, crushed noise | `glitch.rs` |
| Cyclic repeats: 7-step and 11-step loops that repeat, then mutate one step | `pattern.rs` |
| Ping-pong echo | `delay.rs` |
| FM bass + sine sub through a low-pass | `bass.rs` |
| 8-line FDN reverb, ~18 s tail | `reverb.rs` |
| Mix and master | `lib.rs` |
| Read-only views for the TUI | `telemetry.rs` |
| Prompts, fade and WAV writer shared by both native programs | `cli.rs` |

The musical constants (chord voicings, tempo, layer configs, mix levels) are named `const`s at the top of each file.

## Licences

The project is MIT-licensed, and nothing it uses prevents that:
- **The engine** (`engine/`) has no dependencies at all.
- **The app's crates** are all under permissive licences: MIT, Apache-2.0, Zlib, BSL-1.0 or the Unicode licence. There is nothing GPL or LGPL among them.
- **On Linux**, audio goes through the system's ALSA library (LGPL). It is linked dynamically, the normal way, which places no conditions on this project's licence.
- **If you distribute compiled binaries**, include the licence notices of the bundled crates; Apache-2.0 and MIT both ask for this. A tool such as `cargo about` can generate the list.

## Determinism

The same seeds always give the same music, sample for sample. This holds natively and in WASM, at any block size, and whether the track is played live, recorded, or rendered:

- **No crates in the engine.** All the maths, the RNG (SplitMix64), the FFT and every DSP block are in `engine/`.
- **No platform maths.** `sin`, `exp` and `tanh` come from the platform's maths library, which differs between systems. `math.rs` rebuilds them from `+ − × ÷`, `floor` and `sqrt`, which IEEE-754 guarantees exactly on every CPU and in WASM.
- **Block-size independence.** All timing runs off a single integer sample clock. Even the drone's FFT work is scheduled by sample position, spread across each hop.
- **Observation only.** The TUI reads the engine's state but never feeds anything back into the sound.
- **Same file, three ways.** A live play + record session produces a WAV byte-identical to a render with the same seeds and length, from either program. Even the 24-bit dither comes from a fixed seed.
- **Locked by tests.** `GOLDEN_HASH` in `engine/tests/determinism.rs` is a fingerprint of the first 30 s. `node web/verify.mjs` computes the same fingerprint from the WASM build.
- **Pinned toolchain.** Rust 1.98.1 is pinned in `rust-toolchain.toml`.

Live playback has two exceptions, and neither changes the music itself. On a sound card that can't run at 48 kHz, what you hear is resampled (recordings are not affected). And if the computer stalls, you hear a gap of silence, after which the music continues exactly where it left off.

```
cargo test --workspace                  # unit + determinism tests
cargo test --release -- --ignored       # 2-hour stability test (from engine/)
./build-web.sh && node web/verify.mjs   # WASM hash must equal GOLDEN_HASH
```

If the composition is changed on purpose, the golden hash changes with it. Update the constant, because that is a new version of the piece.
