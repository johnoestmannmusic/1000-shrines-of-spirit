# SpectralFreeze plugin — status & next steps

Rust/nih-plug port of the "Freeze" algorithm from `src/0006/index.html`'s Spectral Fusion feature: load a sample, freeze a spectral snapshot into a sustained pad/drone, play it via MIDI. Full design rationale lives in the plan doc at `~/.claude/plans/i-really-like-the-partitioned-papert.md` (same machine) — this file is the short version for picking work back up.

## Where things stand: Phase B complete

`freeze_dsp`, `freeze_cli`, `freeze_plugin`, and `xtask` all exist. The plugin builds as a real VST3/CLAP bundle and standalone binary against nih-plug pinned at commit `de421011f41a6d10fc8c7a6084e4f4dee0143683` (recorded 2026-09-06, was `HEAD` of the default branch at the time — check for a newer rev before assuming this is still current).

```
src/plugins/SpectralFreeze/
├── Cargo.toml              # workspace; workspace.dependencies pins nih_plug/nih_plug_xtask git rev
├── .cargo/config.toml       # `cargo xtask` / `cargo xtask-debug` aliases
├── bundler.toml             # names the freeze_plugin bundle "SpectralFreeze"
├── .gitignore               # /target
├── xtask/                   # nih_plug_xtask wrapper — `cargo xtask bundle freeze_plugin --release`
└── crates/
    ├── freeze_dsp/          # pure Rust, only dep = realfft. 39 tests, all passing.
    ├── freeze_cli/          # binary: --input a.wav --out b.wav --freeze-point --formant-shift --stereo-width --note --seconds
    └── freeze_plugin/       # nih-plug wrapper. No MIDI/params/GUI yet (Phase C/D/E) — see below.
```

**Not yet committed to git** — the user hasn't asked for a commit.

### Phase B exit gate — verified 2026-09-06

`cargo xtask bundle freeze_plugin --release` produces `target/bundled/SpectralFreeze.vst3` and `SpectralFreeze.clap`. The standalone binary (`target/release/freeze_plugin_standalone`) was run directly on the dev machine and confirmed audible by the user on both `--backend alsa` (stuttery — ALSA device contention with PulseAudio on this machine) and `--backend jack` (clean — jackdbus auto-starts on demand). **Prefer `--backend jack` for manual listening tests on this machine** so other running audio programs don't clash with the plugin's device access.

One system dependency was needed and is now installed: `libx11-xcb-dev` (pulled in transitively by nih-plug's `standalone` feature even for this audio-only, no-GUI plugin).

`FreezePlugin` (`crates/freeze_plugin/src/lib.rs`) bakes a real frozen loop via `freeze_dsp::render::render_frozen_loop` from a synthetic harmonic tone (not a loaded sample — that's Phase E) at `initialize()` time, then just loops it to output in `process()` regardless of MIDI/transport — deliberately dumb, per the Phase B goal of proving the build pipeline independent of DSP correctness (already proven in Phase A) or real playback logic (Phase C).

Run `cargo test -p freeze_dsp` from `src/plugins/SpectralFreeze/` to verify (should be 39 passed, 0 failed). Run the CLI to listen to changes — it's the fastest way to sanity-check a DSP change before writing a plugin around it:

```
cargo run -p freeze_cli -- --input <some.wav> --out /tmp/out.wav --freeze-point 30 --formant-shift 0 --stereo-width 40 --note 60 --seconds 6
```

**Not yet committed to git** — the user hasn't asked for a commit. `src/plugins/` shows as untracked in `git status`.

## Three parameters, all render-time (not live post-processes)

All three require calling `render::render_frozen_loop` again to take effect — none are cheap per-sample operations:

- **Freeze Point** (0-100%): which moment in the source sample gets frozen.
- **Formant Shift** (-12..12 semitones): cepstral envelope shift, currently applied once per render (see Phase F below for why this should change).
- **Stereo Width** (0-100%, default 0%): see "Key learnings" below — this one has a non-obvious implementation and is worth reading before touching.

## Key learnings from Phase A (read before changing the DSP core)

1. **`realfft` requires DC/Nyquist bins to be purely real.** The original JS used a hand-rolled complex FFT with no such constraint. `resynth.rs::FreezeResynth::next_frame` explicitly forces bins `0` and `N/2` to real values (tracking sign via `cos()`, zeroing the imaginary part) rather than letting accumulated phase drift them off the real axis — without this, `realfft`'s inverse transform panics ("Imaginary part of last value was non-zero").

2. **Phase wrapping is fragile at exactly ±π with a naive `%`-based implementation.** `phase_advance.rs::principal_value` uses a floored-modulo formula (`PI - floored_mod(PI - x, 2*PI)`) instead, specifically because float rounding can land a value like `-3π` on the wrong side of a naive `<=`/`>` branch check.

3. **Stereo Width cannot be a post-hoc mid-side transform.** First implementation applied `mid ± width*side` to the final mixed stereo output — this does nothing when the source's L/R channels are correlated or identical, which is exactly the case for this project's own test asset (`src/0006/ASSETS/flight_school_night_shift.wav` has bit-identical L/R, verified numerically). A mid-side transform can only rescale existing difference, never create it. **Current (correct) design**: baked into `render_frozen_loop` (`stereo.rs`) — at width=0% the right channel is forced to exactly the left channel's own frozen spectrum (guaranteed centering regardless of source); as width increases, the right channel's magnitude blends toward its own independent analysis AND a deterministic per-bin phase offset (`stereo::decorrelation_spread`, golden-ratio-stepped) is added, which is what actually guarantees audible width even on a fully mono source. If you're tempted to "simplify" Stereo Width back to a live per-block transform for automation-smoothness reasons — don't, it was tried and doesn't work.

4. `render_frozen_loop` **always outputs exactly 2 channels** now, regardless of source channel count (mono sources get duplicated before freezing). `VoiceManager::process_block` does no width/channel logic of its own — it just reads both (already width-shaped) channels in lockstep per voice via `PlaybackReader::read_stereo_and_advance`.

## Next: Phase C — real MIDI wiring via VoiceManager

Goal: wire `VoiceManager` (already built and tested in Phase A, `crates/freeze_dsp/src/voice.rs`) into `freeze_plugin`'s `process()` for real MIDI note-on/off + polyphony + playback-rate pitch, replacing the Phase B baked-loop-ignoring-MIDI placeholder.

1. Set `FreezePlugin::MIDI_INPUT = MidiConfig::Basic` (currently `None`).
2. In `process()`, read `context.next_event()` each block and call `VoiceManager::note_on`/`note_off` on `NoteEvent::NoteOn`/`NoteOff` (see the `sine` example in nih-plug's own repo for the polling pattern — `next_event = context.next_event()` in a loop keyed off `event.timing()` vs. `sample_id`).
3. Replace the hand-rolled `play_pos` loop in `process()` with `VoiceManager::process_block(&loop_buffer, out_left, out_right)`.
4. The synthetic baked loop from Phase B stays as-is for now (Phase E adds real sample loading) — this phase is purely about proving MIDI → voices → audio, independent of what's frozen.
5. Stress-test >16 simultaneous notes for graceful voice stealing (`VoiceManager` already has this logic from Phase A — confirm it holds up driven by real MIDI rather than test harness calls).
6. Exit gate: play notes on a MIDI keyboard/host into the standalone binary (`--backend jack` preferred on this machine, see Phase B notes above) or a DAW, hear pitched frozen pads with clean polyphony.

## After that (Phases D-F, in order)
- **D**: register Freeze Point / Formant Shift / Stereo Width as real automatable `FloatParam`s, backed by a background render thread + `ArcSwap<LoopBufferData>` + a latest-value-wins mailbox (drops superseded requests so a fast automation sweep doesn't back up the worker). All three params go through this same path — none of them are free.
- **E**: sample loading (`rfd` file dialog + `hound` WAV decode, GUI thread only) + minimal `nih_plug_egui` GUI (three sliders, load button, filename label).
- **F**: decouple Formant Shift so it only reprocesses the already-frozen magnitude spectrum (cheap) instead of requiring the full source-sample re-analysis Freeze Point needs (expensive) — makes live Formant Shift automation snappier than Freeze Point/Stereo Width.

Full test-plan checklist and dependency list (crate versions, why `realfft` over a hand-rolled FFT, etc.) are in the plan doc referenced at the top.
