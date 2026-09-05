# 1000 Shrines of SPIRIT — Project Kanban

How to use:
This file is the shared Kanban Markdown coordination board for active subprojects.
When starting on a project, Agents should familiarize themselves with the state of the project as described in that section in this document.
Agents should only assign themselves to cards that are not already assigned to another agent. They should move cards between buckets instead of duplicating them, preserve
card IDs, and append dated process comments after each completed feature.

---

## Project: 0006 Lantern Music Player — Rust egui/eframe Port

**Project Title:** 0006 Lantern Music Player — Rust egui/eframe Port  
**Project Description:** Port the original single-file browser experience at
`src/0006/index.html` to Rust using egui/eframe, with web/WASM as the primary
deployment target. Preserve the original music-player behaviour, Spectral
Fusion processing, compact visual style, and project/export workflows while
using suitable egui improvements such as movable editor windows. Native builds
are a development convenience; browser behaviour and audio fidelity take
priority.  
**Implementation Repository:** `../../../0006-rust/`  
**Primary Reference:** `../src/0006/index.html`  
**Detailed Handoff:** `../../../0006-rust/NEXTSTEPS.md`  
**Parity Checklist:** `../../../0006-rust/PARITY.md`  
**Board Last Updated:** 2026-09-06 07:19 by Codex A

### Ideas

#### 0006-IDEA-001 — Additional Furnace chip-system support

- **Card Title:** Additional Furnace chip-system support
- **Description:** Extend runtime song loading beyond the currently supported Game Boy `.fur` subset. Add parsers and playback mappings only for systems backed by real fixture files and documented acceptance tests. Keep this separate from completing parity for the bundled 0006 experience.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 07:19 — Recorded as a possible extension after the Game Boy port is complete.

#### 0006-IDEA-002 — Persistent Fusion render cache

- **Card Title:** Persistent Fusion render cache
- **Description:** Cache rendered Spectral Fusion results by source and settings fingerprint across browser sessions. This could reduce startup preparation after the project and source import formats become stable. Measure storage cost and invalidation behaviour before scheduling it.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 07:19 — Added after moving live Fusion rendering into a browser worker.

#### 0006-IDEA-003 — Mobile-focused compact layout

- **Card Title:** Mobile-focused compact layout
- **Description:** Add a compact arrangement for narrow screens while retaining the original visual language. Keep transport and active editors usable without hiding essential audio state. Validate touch targets and movable-window behaviour on real mobile browsers.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 07:19 — Original CSS stacks below 900 pixels, providing the starting reference.

### Planned Features

#### 0006-PLAN-001 — Complete project JSON and audio exports

- **Card Title:** Complete project JSON and audio exports
- **Description:** Implement the original complete project schema rather than extending the current local-settings subset. Add compatible JSON import/export plus sampler WAV and numbered-sample ZIP generation. Preserve legacy field names, null source slots, metadata, and referenced audio filenames.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending; scheduled after final Spectral Fusion validation.
- **Process Comments:** 2026-09-06 07:19 — Identified in `NEXTSTEPS.md` as the next implementation milestone.

#### 0006-PLAN-002 — Source sample library and import workflow

- **Card Title:** Source sample library and import workflow
- **Description:** Build the six-slot source panel with waveforms, duration, playback, loading, editing, packaging, and clearing. Ensure imported samples update affected instruments and invalidate Fusion and loop caches safely. Match the original source metadata and comments workflow.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Three bundled sources currently load without the complete library UI.

#### 0006-PLAN-003 — Pattern, piano, and audition panels

- **Card Title:** Pattern, piano, and audition panels
- **Description:** Add the pattern order view, tracker cells, follow-playhead behaviour, piano state, and noise display. Implement row, cell, and instrument audition without disturbing transport state. Use the colours, boundary lines, held values, and hover explanations documented in `PARITY.md`.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Furnace parsing and the shared song model already provide the required note data.

#### 0006-PLAN-004 — Mixer meters and instrument quick editing

- **Card Title:** Mixer meters and instrument quick editing
- **Description:** Complete editable dB controls, live peak and clipping meters, and default-project mixer restoration. Add instrument colour, name, transpose, volume, source, preview, and shortcut editing to the main table. Keep edits synchronized with movable Sampler and Spectral windows.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Linear gains, channel mute, master volume, preview, and editor shortcuts already exist.

#### 0006-PLAN-005 — Cover art and remaining visual chrome

- **Card Title:** Cover art and remaining visual chrome
- **Description:** Recreate the animated dithered CD, Matrix field, trigger arcs, sidebar panels, and compact two-column page composition. Add the 1600×1600 cover export and responsive stacking used by the reference. Preserve the coarse pixel structure and both documented palettes.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Font, palettes, compact controls, and movable windows establish the current visual foundation.

#### 0006-PLAN-006 — Alternate-song loading and website deployment

- **Card Title:** Alternate-song loading and website deployment
- **Description:** Add the folder-based flow for a different Game Boy `.fur` file with matching assets. Validate browser error handling, cache paths, and hosting from a nested website route. Produce and test the final optimized WASM bundle before publishing.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Runtime parser exists; user-facing selection and production deployment remain.

### Assigned

#### 0006-ASGN-001 — Spectral Fusion numerical and listening validation

- **Card Title:** Spectral Fusion numerical and listening validation
- **Description:** Compare worker and native Rust outputs with controlled original-JavaScript renders for all six algorithms. Resolve material numerical differences and then collect user listening feedback on the bundled default settings. Record accepted limits for Smear randomness and platform-specific convolution behaviour.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** In progress; implementation and responsiveness are complete, but golden comparison and audible approval remain.
- **Process Comments:** 2026-09-06 07:19 — Browser integration, generation cancellation, preview updates, and clean completion have been verified.

### Completed

#### 0006-DONE-001 — Rust workspace and Furnace Game Boy parser

- **Card Title:** Rust workspace and Furnace Game Boy parser
- **Description:** Created the multi-crate Rust workspace and parsed the bundled Furnace song directly from its binary format. Validated real PATN bytes where the prose format documentation was inaccurate. Built typed song data used by both audio modes and future inspection panels.
- **Assigned Agent:** Previous project agent
- **Card Creation Date:** Before 2026-09-05
- **Card Completion Note:** Complete; parser fixture tests pass for the bundled Game Boy song.
- **Process Comments:** 2026-09-05 — Existing implementation was audited and retained when Codex A assumed the project.

#### 0006-DONE-002 — CHIP mode transport and four-stem mixer

- **Card Title:** CHIP mode transport and four-stem mixer
- **Description:** Implemented browser and native playback for the four rendered chip stems. Added shared play, pause, stop, seek, looping, channel gains and mutes, and master volume. Kept Web Audio timing anchored to the audio clock.
- **Assigned Agent:** Previous project agent
- **Card Creation Date:** Before 2026-09-05
- **Card Completion Note:** Complete; native and browser behaviour was user-verified.
- **Process Comments:** 2026-09-05 — Preserved as the stable baseline while sampler features were added.

#### 0006-DONE-003 — Basic sampler engine and scheduling

- **Card Title:** Basic sampler engine and scheduling
- **Description:** Compiled tracker notes into sampler events and scheduled them with bounded lookahead. Added source assignment, trims, transpose, volume, mute, note-off, seek, and loop-safe playback. Implemented a dedicated web timer and an approximate native worker.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-05
- **Card Completion Note:** Complete in commit `ac4c355`; browser smoke checks and core timing tests pass.
- **Process Comments:** 2026-09-05 — Web/WASM remains the timing-fidelity target.

#### 0006-DONE-004 — Original HTML parity audit

- **Card Title:** Original HTML parity audit
- **Description:** Read the complete original `index.html` and served it locally for visual and behavioural inspection. Recorded palettes, layout, controls, algorithms, export semantics, and remaining gaps in `PARITY.md`. Converted the source audit into implementation checkpoints that future agents can follow.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-05
- **Card Completion Note:** Complete in commit `6a24f34`.
- **Process Comments:** 2026-09-05 — The original remains the authority for appearance and behaviour.

#### 0006-DONE-005 — Full movable sampler editor and voice controls

- **Card Title:** Full movable sampler editor and voice controls
- **Description:** Added waveform trimming, source selection, fractional transpose, loop and ping-pong, ADSR, volume, random pan, polyphony, and voice caps. Added independent preview with reference pitch and local settings persistence. Styled the movable egui editor with the bundled font and light, dark, and system themes.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-05
- **Card Completion Note:** Complete in commit `ac7ccc6`; browser interaction checks and tests pass.
- **Process Comments:** 2026-09-06 07:19 — User confirmed the implemented features work and that the appearance is progressing well.

#### 0006-DONE-006 — Spectral Fusion DSP, editor, and playback integration

- **Card Title:** Spectral Fusion DSP, editor, and playback integration
- **Description:** Implemented Freeze, Cross-Synth, Convolve, Ring Modulate, Frequency Shift, Smear, and formant shifting. Added A, B, and Result waveforms with source, amount, status, and enable controls in the movable editor. Routed the effective fused source through preview, song playback, waveform display, and loop preparation.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete in commit `d48d603`; invariant tests and native/WASM builds pass.
- **Process Comments:** 2026-09-06 07:19 — Exact golden audio comparison remains tracked separately in Assigned.

#### 0006-DONE-007 — Non-blocking web Spectral Fusion rendering

- **Card Title:** Non-blocking web Spectral Fusion rendering
- **Description:** Moved browser FFT and convolution work from egui's UI event handler into a dedicated web worker. Added transferable audio buffers, rendering status, generation tokens, stale-result rejection, and completion-driven preview and trim updates. Preserved the previous playable result while a replacement is calculated.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete in Rust-port commit `f8c9033`; automated browser checks measured algorithm clicks at approximately 90–160 ms while rendering continued independently.
- **Process Comments:** 2026-09-06 07:19 — Rapid Cross-Synth-to-Smear changes and Convolve completion were verified with updated result waveforms and no console warnings or errors.

---

## Project: SpectralFreeze Plugin — Rust/nih-plug VST3/CLAP Port

**Project Title:** SpectralFreeze Plugin — Rust/nih-plug VST3/CLAP Port  
**Project Description:** Port the "Freeze" algorithm from `src/0006/index.html`'s Spectral Fusion feature into a standalone Rust/nih-plug instrument: load a sample, freeze a spectral snapshot into a sustained pad/drone, and play it polyphonically via MIDI as a VST3/CLAP plugin (and standalone binary). Built bottom-up as a pure-Rust, unit-tested DSP core (`freeze_dsp`, no nih-plug/hardware dependency) first, then a thin nih-plug wrapper (`freeze_plugin`) around it, so DSP correctness and plugin-build-pipeline correctness could be verified independently. `freeze_cli` is a WAV-in/WAV-out test harness for fast by-ear DSP iteration without a host. This board (not a separate NEXT_STEPS.md, which has been removed) is the authoritative status/handoff doc for this project going forward.  
**Implementation Repository:** `../src/plugins/SpectralFreeze/`  
**Primary Reference:** `../src/0006/index.html` (Spectral Fusion "Freeze" feature); full original design rationale in the plan doc at `~/.claude/plans/i-really-like-the-partitioned-papert.md` (same machine, not in this repo)  
**Architecture note:** Freeze Point / Formant Shift / Stereo Width are all *render-time* operations (they call `render::render_frozen_loop` again to take effect), not cheap per-sample transforms — relevant to FREEZE-PLAN-003 below and to anything that touches how they're triggered.  
**Board Last Updated:** 2026-09-06 08:51 by Reason A

### Ideas

#### FREEZE-IDEA-001 — Master soft-saturation limiter as alternative/companion to gain compensation

- **Card Title:** Master soft-saturation limiter as alternative/companion to gain compensation
- **Description:** Current polyphony fix (`1/active_count` gain compensation in `VoiceManager::process_block`) guarantees no clipping but trades in a real, audible volume dip at 3+ held voices — acceptable to the user for now but flagged to revisit once there's a fuller instrument (params, real samples) to judge it against. A tanh-style master limiter, alone or layered with a gentler compensation curve, would keep single-note volume closer to constant at the cost of coloring loud chords. Needs a listening comparison once Phase D/E params exist to actually play with.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Pending; not scheduled.
- **Process Comments:** 2026-09-06 07:44 — User confirmed the current dip isn't a blocker; explicitly deferred to a later full-instrument listening pass.

### Planned Features

#### FREEZE-PLAN-003 — Decouple Formant Shift from full re-analysis (Phase F)

- **Card Title:** Decouple Formant Shift from full re-analysis
- **Description:** Formant Shift currently requires the same full source-sample re-analysis as Freeze Point on every change. Decouple it so it only reprocesses the already-frozen magnitude spectrum (cheap), making live Formant Shift automation snappier than Freeze Point/Stereo Width, which genuinely need the expensive path.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Pending; final planned phase.
- **Process Comments:** 2026-09-06 07:45 — Pure optimization, deferred until the three params are actually wired and automatable (Phase D).

### Assigned

#### FREEZE-PLAN-004 — ADSR envelope parameters with a draggable visual graph

- **Card Title:** ADSR envelope parameters with a draggable visual graph
- **Description:** Replace the current fixed, unexposed two-stage envelope (`freeze_dsp::envelope::ArEnvelope`, hardcoded `ATTACK_MS = 10.0` / `RELEASE_MS = 150.0` constants in `voice.rs`) with a real Attack/Decay/Sustain/Release envelope: 4 automatable `FloatParam`s (attack/decay/release time, sustain level), plus a custom-drawn envelope-shape graph in the editor with grabbable points (attack-end point drags horizontally only - it always tops out at 1.0; decay-end/sustain-level point drags both axes; release-end point drags horizontally only, always returns to 0) instead of plain sliders. Unlike Freeze Point/Formant Shift/Stereo Width, ADSR is cheap (per-sample, not render-time) - no `RenderWorker`/crossfade/throttle needed. Values should be read at note-trigger time in `VoiceManager::note_on` (like most synths - changing Attack doesn't reshape a note already mid-decay), so `note_on`'s signature needs to grow to accept the 4 current values (or a small `AdsrSettings` struct) read from the params where `process()` currently handles `NoteEvent::NoteOn`. `ArEnvelope`'s decay math (currently release-only: `level *= release_coeff` each sample, decaying toward 0) generalizes to decay by decaying toward `sustain_level` instead of 0, using the same `exp(-9.2103 / samples)` coefficient shape.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 08:40
- **Card Completion Note:** In progress.
- **Process Comments:** 2026-09-06 08:41 — User confirmed full ADSR (not just exposing Attack/Release) plus a visual graph with grabbable points, not plain sliders. 2026-09-06 08:52 — Assigned to Reason A; `FREEZE-PLAN-005`'s waveform widget (same "custom-drawn, draggable egui widget over normalized param value" pattern) completed first and used as the template for this one's handle-dragging code.

### Completed

#### FREEZE-PLAN-005 — Freeze Point waveform display with position marker

- **Card Title:** Freeze Point waveform display with position marker
- **Description:** Added `draw_freeze_point_waveform()` to the editor: draws the loaded source's waveform (min/max per pixel column) with a vertical marker at Freeze Point's current position, directly click/draggable via `ParamSetter::set_parameter_normalized` (bracketed by `begin_set_parameter`/`end_set_parameter` on `drag_started`/`drag_stopped` so host automation recording sees a proper gesture, not a value jump) rather than only adjustable through the numeric `ParamSlider` below it, which stays for precision. Uses `unmodulated_normalized_value()` to read the marker position and `set_parameter_normalized()` to write it, so the widget doesn't need to know anything about Freeze Point's underlying range/skew - reusable as the template for `FREEZE-PLAN-004`'s handles.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 08:41
- **Card Completion Note:** Complete in commit `1a6d3f2`; 46 tests still passing (no DSP changes, GUI-only). User confirmed the waveform renders, the marker tracks Freeze Point, and dragging directly on it both moves the marker and audibly changes the frozen content.
- **Process Comments:** 2026-09-06 08:41 — Requested by the user immediately after scoping `FREEZE-PLAN-004`; done first since it's the simpler of the two (one draggable line vs. three-handle curve editing).

#### FREEZE-PLAN-002 — Sample loading and minimal GUI (Phase E)

- **Card Title:** Sample loading and minimal GUI
- **Description:** Added a `nih_plug_egui` editor (`FreezePlugin::editor()`): three `widgets::ParamSlider`s bound directly to the Phase D `FloatParam`s, a "Load Sample..." button (`rfd::FileDialog::pick_file`), and a filename/error label. `load_wav_channels` (`freeze_plugin/src/lib.rs`) decodes WAV via `hound` (int and float formats, normalized to `[-1.0, 1.0]`) on the GUI thread, matching `freeze_cli`'s existing decode logic but kept as a separate small copy rather than sharing code with it, to keep `hound`/file-I/O out of `freeze_dsp`'s pure-DSP dependency surface.

  **Architecture change needed to make this possible**: `RenderWorker`'s source was a fixed `Arc<Vec<Vec<f32>>>` captured at spawn time (Phase D) - fine when the source never changed, but loading a new sample needs the worker to pick up new audio after it's already running. Source is now `Arc<ArcSwap<Vec<Vec<f32>>>>`; the worker reads whatever is *current* at the moment each render actually runs. Also split a `RenderTrigger` (cheap-to-clone, just the request-a-render capability) out of `RenderWorker` (owns the thread, stops it on drop) so the GUI thread can trigger a re-render after loading without being able to affect the worker's lifetime. Test: `render_worker::tests::swapping_the_source_and_re_requesting_uses_the_new_source`.

  **New DSP utility**: `freeze_dsp::resample::resample_linear` - naive linear-interpolation whole-buffer resample (no anti-aliasing filter, so a large downsample factor can alias) used to bring a loaded file to the plugin's operating sample rate when they differ. Without this, a loaded 44.1kHz file in a 48kHz host session (or vice versa) would play back pitch/speed-shifted, since `VoiceManager` reads the frozen loop assuming it's already at the plugin's rate. The plugin reads its own operating rate live from `loop_buffer.load().sample_rate` at load time rather than a value captured once when the editor was created, in case the editor is ever opened unusually early.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Complete in commit `2a89d5e`; 46 tests passing (43 `freeze_dsp` + 3 `freeze_plugin`); user confirmed the GUI opens in Carla with working sliders, loaded a real WAV file via the dialog, and heard the loaded sample's frozen content play back correctly.
- **Process Comments:** 2026-09-06 08:40 — User has requested ADSR envelope parameters next (see `FREEZE-PLAN-004`); current envelope is a fixed attack/release only (`freeze_dsp::envelope::ArEnvelope`, `ATTACK_MS`/`RELEASE_MS` constants in `voice.rs`), not yet exposed as params at all.

#### FREEZE-PLAN-001 — Automatable Freeze Point / Formant Shift / Stereo Width parameters (Phase D)

- **Card Title:** Automatable Freeze Point / Formant Shift / Stereo Width parameters
- **Description:** Registered all three as real `FloatParam`s (`FreezePluginParams` in `freeze_plugin/src/lib.rs`). Each is checked once per host block; a detected change is handed to `render_worker::RenderWorker` (new file, `freeze_plugin/src/render_worker.rs`) — a background thread that renders on demand via `render_frozen_loop` and publishes into `loop_buffer: Arc<ArcSwap<LoopBufferData>>`, which `process()` reads lock-free. Tested via Carla (installed on this machine, gives generic parameter sliders for any VST3 without needing our own GUI) with real MIDI routed in through the `aplaymidi`/Midi-Bridge technique from `FREEZE-DONE-003`.

  **Two mechanisms were needed together to make live param changes sound clean — neither alone was enough:**
  1. **Crossfade** (`voice.rs`, `BUFFER_CROSSFADE_MS = 15.0`): `VoiceManager::process_block` now takes `buffer: &Arc<LoopBufferData>` (was `&LoopBufferData`) specifically so it can detect a *new* buffer via `Arc::ptr_eq` and crossfade from the outgoing one instead of hard-cutting — two different frozen spectra are, in general, unrelated at any given playhead sample, so an instant swap is a real waveform discontinuity (a click). `resample.rs::sample_stereo_at` was factored out of `PlaybackReader::read_stereo_and_advance` so the crossfade can sample the *outgoing* buffer at the exact position the reader is using for the incoming one. Test: `voice::tests::buffer_swap_crossfades_instead_of_clicking`.
  2. **Throttle** (`freeze_plugin/src/lib.rs`, `RENDER_THROTTLE_MS = 100.0`): the crossfade alone wasn't enough — a host automating (or a human dragging) a param smoothly triggers a fresh render on nearly every block, each new swap landing mid-crossfade from the previous one, so crossfades pile up into continuous stuttering instead of ever finishing. `process()` now only actually sends a render request once every `RENDER_THROTTLE_MS`, always using the *latest* observed value at that moment (`pending_request: Option<RenderRequest>`, overwritten not queued) — keeping `RENDER_THROTTLE_MS ≳ BUFFER_CROSSFADE_MS` so swaps can't overlap. A fast drag now produces occasional smooth morphs instead of constant noise; a very fast drag still feels stepped rather than perfectly continuous — an inherent tradeoff of re-rendering full spectral snapshots rather than interpolating them in the frequency domain, not a bug to chase further right now.

  **Two debugging gotchas surfaced while chasing "still popping" reports, both worth knowing about for future host-testing sessions:**
  - **A stale test source can hide the very thing you're testing.** Freeze Point kept sounding bad while Formant Shift sounded clean through the *same* crossfade+throttle code path. The original placeholder `synthetic_source()` was a **constant** tone (fixed partial mix for the whole second). Different Freeze Point values on a non-evolving source analyze nearly-identical magnitude spectra differing mainly in essentially arbitrary starting phase — measured directly: two renders at freeze_point 10 vs 90 had RMS nearly identical (0.394 vs 0.393) but time-domain sample-for-sample difference (0.69) *larger than either signal's own RMS* — the signature of same-loudness-different-phase content, which doesn't crossfade smoothly no matter how well-tuned the crossfade is. `synthetic_source()` now sweeps brightness (partial balance) over its duration so different Freeze Points capture genuinely different moments (confirmed: RMS 0.358 vs 0.525 at the same two freeze points afterward) — also just a better demo of what Freeze Point is *for*. If a future param/DSP change seems to only affect *some* values through otherwise-shared code, check whether the test material actually varies enough to exercise the difference before assuming the code is at fault.
  - **A host can keep running a stale/deleted binary after "remove and re-add."** Rebuilding the plugin and using Carla's "remove and re-add plugin" did **not** guarantee the host reloaded the shared library — `/proc/<carla_pid>/maps` showed `SpectralFreeze.so (deleted)`, the old `.so` inode still mapped even though `cargo xtask bundle` had overwritten the file on disk with a new inode. Several rounds of "still broken" were against code from several rebuilds ago. **After rebuilding, if a host's behavior doesn't match a code change you're sure about, check `grep -i <plugin>.so /proc/<host_pid>/maps` for `(deleted)` before debugging further — a full host restart forces a real reload.** Same category of trap as `FREEZE-DONE-003`'s JACK-client-name-collision gotcha: a test harness silently running stale code.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Complete in commit `0b2a29c`; 43 tests passing (41 `freeze_dsp` + 2 `freeze_plugin`); user confirmed clean automation of all three params via Carla/VST3 with no stuttering.
- **Process Comments:** 2026-09-06 08:23 — This board (and NEXT_STEPS.md's removal) folds in what was previously tracked in `NEXT_STEPS.md`, which is no longer maintained for this project.

#### FREEZE-DONE-001 — Pure-Rust DSP core and CLI test harness (Phase A)

- **Card Title:** Pure-Rust DSP core and CLI test harness
- **Description:** Built `freeze_dsp` (phase-vocoder freeze + cepstral formant shift, ported from `src/0006/index.html`'s Spectral Fusion Freeze) with no nih-plug/hardware dependency, plus `freeze_cli` as a WAV-in/WAV-out test harness for fast by-ear iteration:
  ```
  cargo run -p freeze_cli -- --input <some.wav> --out /tmp/out.wav --freeze-point 30 --formant-shift 0 --stereo-width 40 --note 60 --seconds 6
  ```
  Four non-obvious DSP correctness issues resolved, all worth knowing before touching this code again:
  1. **`realfft` requires DC/Nyquist bins to be purely real.** The original JS used a hand-rolled complex FFT with no such constraint. `resynth.rs::FreezeResynth::next_frame` explicitly forces bins `0` and `N/2` to real values (tracking sign via `cos()`, zeroing the imaginary part) rather than letting accumulated phase drift them off the real axis — without this, `realfft`'s inverse transform panics ("Imaginary part of last value was non-zero").
  2. **Phase wrapping is fragile at exactly ±π with a naive `%`-based implementation.** `phase_advance.rs::principal_value` uses a floored-modulo formula (`PI - floored_mod(PI - x, 2*PI)`) instead, specifically because float rounding can land a value like `-3π` on the wrong side of a naive `<=`/`>` branch check.
  3. **Stereo Width cannot be a post-hoc mid-side transform.** First implementation applied `mid ± width*side` to the final mixed stereo output — this does nothing when the source's L/R channels are correlated or identical, which is exactly the case for this project's own test asset (`src/0006/ASSETS/flight_school_night_shift.wav` has bit-identical L/R, verified numerically). A mid-side transform can only rescale existing difference, never create it. Current (correct) design: baked into `render_frozen_loop` (`stereo.rs`) — at width=0% the right channel is forced to exactly the left channel's own frozen spectrum (guaranteed centering regardless of source); as width increases, the right channel's magnitude blends toward its own independent analysis AND a deterministic per-bin phase offset (`stereo::decorrelation_spread`, golden-ratio-stepped) is added, which is what actually guarantees audible width even on a fully mono source. Don't "simplify" this back to a live per-block transform for automation-smoothness reasons — it was tried and doesn't work.
  4. `render_frozen_loop` **always outputs exactly 2 channels**, regardless of source channel count (mono sources get duplicated before freezing). `VoiceManager::process_block` does no width/channel logic of its own — it just reads both (already width-shaped) channels in lockstep per voice via `PlaybackReader::read_stereo_and_advance`.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-05 09:02
- **Card Completion Note:** Complete in commit `7d91249`; 39 tests passing.
- **Process Comments:** 2026-09-06 07:45 — Full rationale for each DSP decision recorded above for future agents touching this code.

#### FREEZE-DONE-002 — Minimal nih-plug wrapper and build pipeline (Phase B)

- **Card Title:** Minimal nih-plug wrapper and build pipeline
- **Description:** Added `freeze_plugin`/`xtask` to the workspace (`Cargo.toml`'s `workspace.dependencies` pins `nih_plug`/`nih_plug_xtask` to git commit `de421011f41a6d10fc8c7a6084e4f4dee0143683`, which was the default branch's `HEAD` as of 2026-09-06 — check for a newer rev before assuming this is still current), plus `.cargo/config.toml` (`cargo xtask` / `cargo xtask-debug` aliases) and `bundler.toml` (names the bundle "SpectralFreeze"). Implemented a minimal `Plugin`/`ClapPlugin`/`Vst3Plugin` with a baked/looped buffer (no MIDI/params/GUI) to prove the VST3/CLAP/standalone build pipeline independent of DSP correctness. Required installing the `libx11-xcb-dev` system package (pulled in transitively by nih-plug's `standalone` feature even for this audio-only, no-GUI plugin).
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 06:58
- **Card Completion Note:** Complete in commit `fe5767c`; `cargo xtask bundle freeze_plugin --release` produced working `target/bundled/SpectralFreeze.vst3`/`.clap`, and the standalone binary (`target/release/freeze_plugin_standalone`) was confirmed audible by the user via both `--backend alsa` and `--backend jack`.
- **Process Comments:** 2026-09-06 07:00 — **Prefer `--backend jack` for manual listening tests on this machine**: ALSA stutters from PulseAudio device contention, JACK (jackdbus auto-starts on demand) is clean.

#### FREEZE-DONE-003 — Real MIDI wiring, polyphony, and chord-clipping fix (Phase C)

- **Card Title:** Real MIDI wiring, polyphony, and chord-clipping fix
- **Description:** Set `FreezePlugin::MIDI_INPUT = MidiConfig::Basic`; `process()` drains `context.next_event()` once per host buffer (block-level, not sample-accurate — a fast chord/arpeggio attack might reveal a need to split at event boundaries later, but wasn't audible in testing) and calls `VoiceManager::note_on`/`note_off`/`choke_all` (`freeze_dsp`'s already-tested voice pool from Phase A), then renders the whole block in one `VoiceManager::process_block` call via `buffer.as_slice().split_at_mut(1)`.

  **Real chord-clipping bug found and fixed**: `VoiceManager::process_block` summed voices with no headroom, so a held chord clipped. The standard `1/sqrt(active_count)` polysynth approach (tuned for uncorrelated signals) still let a real chord clip — a single frozen note can already sit close to full scale, and different-pitched voices reading the *same* frozen spectrum can align closely enough in phase to behave more like correlated signals than sqrt(n) assumes. Switched to strict **`1/active_count`**, which guarantees the sum can never exceed a single voice's own peak (triangle inequality: `|Σx_i| ≤ Σ|x_i| ≤ N·(A/N) = A`) regardless of correlation, at the cost of a real, user-accepted volume dip as more notes are held (noticeable around 3 and 5 voices — see `FREEZE-IDEA-001`). Test: `voice::tests::gain_compensation_scales_down_with_more_active_voices`.

  **Testing-process gotcha, separate from the DSP bug above**: several "kill old process, rebuild, relaunch" cycles didn't actually work — a stale PID reference meant the *original* `freeze_plugin_standalone` process was never killed, so every later launch attempt failed outright (JACK already had a client registered as `spectralfreeze`) and exited almost immediately with no visible error. The user kept hearing the first build the entire time despite several real code fixes landing. **Before trusting a "does it sound right now" test after a rebuild, confirm there's exactly one relevant process alive** (`pgrep -af freeze_plugin_standalone`) and that `jack_lsp | grep spectralfreeze` shows freshly-connected ports.

  **Reusable deterministic-testing technique** (no physical MIDI keyboard needed): the ALSA "Midi Through" port (find via `aplaymidi -l`, typically `14:0`) is bridged into JACK automatically on this machine as `Midi-Bridge:Midi Through:...` — `jack_connect` that to the plugin's MIDI input, then `aplaymidi -p 14:0 some.mid` sends a hand-built chord (a minimal Standard MIDI File is easy to construct by hand — see git history for the exact byte-level approach used here). Combine with `jack_capture --port <plugin>:output_1 --port <plugin>:output_2 -d <secs> -fn out.wav` to objectively measure peak/clipping instead of relying on ear alone — this combination is what caught that "still distorting" reports were against a stale process, not the actual fix.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 07:38
- **Card Completion Note:** Complete in commit `e9e2dae`; 40 tests passing; user confirmed clean chords up to 5 simultaneous notes with only the expected/accepted gain-compensation volume dip.
- **Process Comments:** 2026-09-06 07:44 — User accepted the current volume-dip tradeoff and deferred further tuning to a later full-instrument listening pass (see `FREEZE-IDEA-001`).
