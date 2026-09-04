# SpectralFreeze plugin — status & next steps

Rust/nih-plug port of the "Freeze" algorithm from `src/0006/index.html`'s Spectral Fusion feature: load a sample, freeze a spectral snapshot into a sustained pad/drone, play it via MIDI. Full design rationale lives in the plan doc at `~/.claude/plans/i-really-like-the-partitioned-papert.md` (same machine) — this file is the short version for picking work back up.

## Where things stand: Phase A complete

Only `freeze_dsp` (pure-Rust DSP core, no nih-plug) and `freeze_cli` (WAV-in/WAV-out test harness) exist so far. **No plugin code yet** — `freeze_plugin/`, `xtask/`, and the actual VST3/CLAP wrapper are all still todo (Phase B onward, see below).

```
src/plugins/SpectralFreeze/
├── Cargo.toml              # workspace, members = [freeze_dsp, freeze_cli] only
├── .gitignore               # /target
└── crates/
    ├── freeze_dsp/          # pure Rust, only dep = realfft. 39 tests, all passing.
    └── freeze_cli/          # binary: --input a.wav --out b.wav --freeze-point --formant-shift --stereo-width --note --seconds
```

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

## Next: Phase B — minimal nih-plug wrapper (no MIDI yet)

Goal: prove the VST3/CLAP/standalone build pipeline works, independent of any DSP correctness question (already proven in Phase A).

1. Add `freeze_plugin/` and `xtask/` crates to the workspace `Cargo.toml` members list.
2. Pin `nih_plug`/`nih_plug_egui`/`nih_plug_xtask` to a specific git `rev` (not floating `master`) — check the current HEAD of `https://github.com/robbert-vdh/nih-plug` and record the commit hash used, so the build stays reproducible.
3. `freeze_plugin/src/lib.rs`: implement `Plugin`/`Vst3Plugin`/`ClapPlugin` with a **hardcoded/baked buffer** looped to output — deliberately skip MIDI/params/GUI at this stage. The point is just to prove `cargo xtask bundle spectral_freeze_plugin --release` produces a `.vst3`/`.clap` that a host (or the standalone binary via `nih_export_standalone!`) will actually load and make sound with.
4. Exit gate: hear sound out of the bundled plugin in at least one host, or via the standalone binary on this Linux dev machine.

## After that (Phases C-F, in order)

- **C**: wire `VoiceManager` into `process()` for real MIDI note-on/off + polyphony + playback-rate pitch. Stress-test >16 simultaneous notes for graceful voice stealing.
- **D**: register Freeze Point / Formant Shift / Stereo Width as real automatable `FloatParam`s, backed by a background render thread + `ArcSwap<LoopBufferData>` + a latest-value-wins mailbox (drops superseded requests so a fast automation sweep doesn't back up the worker). All three params go through this same path — none of them are free.
- **E**: sample loading (`rfd` file dialog + `hound` WAV decode, GUI thread only) + minimal `nih_plug_egui` GUI (three sliders, load button, filename label).
- **F**: decouple Formant Shift so it only reprocesses the already-frozen magnitude spectrum (cheap) instead of requiring the full source-sample re-analysis Freeze Point needs (expensive) — makes live Formant Shift automation snappier than Freeze Point/Stereo Width.

Full test-plan checklist and dependency list (crate versions, why `realfft` over a hand-rolled FFT, etc.) are in the plan doc referenced at the top.
