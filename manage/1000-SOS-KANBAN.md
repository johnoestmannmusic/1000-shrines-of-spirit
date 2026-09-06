# 1000 Shrines of SPIRIT — Project Kanban

How to use:
This file is the shared Kanban Markdown coordination board for active subprojects.
When starting on a project, Agents should familiarize themselves with the state of the project as described in that section in this document.
Agents should only assign themselves to cards that are not already assigned to another agent. They should move cards between buckets instead of duplicating them, preserve
card IDs, and append dated process comments after each completed feature.
Each project's buckets: Ideas, Bugs, Planned Features, Assigned, Completed. A found bug gets a card in Bugs (symptom, root cause once known, fix approach); once verified fixed it moves to Completed like any other card, keeping its ID.
Dates throughout this file include a time (HH:MM), not just a date, since multiple agents may work on the same project on the same day.

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
**Technical Handoff:** See the source-of-truth notes below.
**Parity Checklist:** `../../../0006-rust/PARITY.md`  
**Board Last Updated:** 2026-09-06 20:54 by Codex A
**Source of Truth:** This project section replaces the retired
`../../../0006-rust/NEXTSTEPS.md`. Update the relevant card and these handoff
notes after every feature, bug fix, or material verification result.

### Technical Handoff Notes

- **Scope and priority:** This is a Rust/egui port of the complete 0006 browser
  experience. Web/WASM is the production target; native playback is a
  development convenience with accepted lower timing and audio fidelity. The
  extension beyond the original is runtime loading of another Game Boy `.fur`
  song with matching assets; support for other chip systems is outside the
  current parity scope.
- **Current implementation:** CHIP and SAMPLER transport, the Game Boy Furnace
  parser, full sampler voice/editor behaviour, all six Spectral Fusion
  algorithms, non-blocking browser rendering, complete Project JSON, sampler
  WAV and sample ZIP exports, the six-slot source library, tracker/piano/noise
  views, row/cell/instrument audition, mixer meters, and main-table instrument
  editing are implemented. The Assigned and Planned buckets below are the
  authoritative remaining-work list.
- **Verification baseline:** `cargo test --workspace` passes 22 tests;
  `cargo check -p lantern-app --target wasm32-unknown-unknown` and
  `trunk build --release` pass. Release-browser checks cover transport,
  sampler and Fusion editing, import/export, tracker audition, live Web Audio
  meters, and quick instrument edits. Existing `lantern-fur` unused-code
  warnings are known. Numerical Fusion parity is recorded in `PARITY.md`;
  musical listening approval remains on `0006-ASGN-001`.
- **Reference workflow:** Read `../../../0006-rust/PARITY.md` and inspect
  `../src/0006/index.html` through a web server before changing appearance or
  behaviour. The original is the authority, with movable egui windows an
  accepted adaptation. A release preview can be built in `lantern-app` with
  `trunk build --release` and served from `lantern-app/dist`; browser sessions
  and local servers may not survive a handoff.
- **Parser facts:** Furnace format 251 `PATN` channel indices are one byte even
  though the prose documentation says 16-bit for format 240 and later. Trust
  fixture bytes and the validated parser comments. On-disk note sentinels are
  180=off, 181=release, 182=macro release, and 183=raw frequency override; do
  not replace them with the original HTML converter's 253 note-off sentinel.
  `INS2` macros are intentionally skipped because playback uses rendered stems
  or samples rather than live Game Boy synthesis.
- **Audio invariants:** Web stem looping deliberately uses Web Audio's native
  `AudioBufferSourceNode.loop` for sample-accurate looping. Sampler playback,
  preview, audition, and offline export share trim, transpose, ADSR, loop,
  ping-pong, pan, polyphony, and fused-source semantics. Loop buffers use 5 ms
  edge fades; mono/channel stealing uses an independent 8 ms cutoff. Fusion
  DSP must remain in `fusion-worker.js`: synchronous WASM FFT work previously
  caused multi-second UI hangs. Generation tokens reject stale worker results.
- **Fusion facts:** The STFT is 2048 points with a 1024 hop and sine analysis /
  synthesis windows. Saved Cross-Synth uses the legacy key
  `spectral-blend`. The production worker matches the reference fingerprints
  exactly for Freeze, Cross-Synth, Ring Modulate, and Frequency Shift;
  Convolve differs by 1.397% within its browser-engine tolerance, and randomized
  Smear passes its statistical bound. The repeatable browser comparison is
  `../../../0006-rust/tests/fusion_web_parity.html`.
- **Build facts:** The Trunk Rust link requires
  `data-target-name="lantern_app"` because the crate emits both a native binary
  and a `cdylib`. Web assets are fetched at runtime. Generated downloads must
  retain their Blob URL briefly after clicking; immediate revocation was the
  cause of resolved bug `0006-BUG-001`.
- **Persistence boundary:** Full Project JSON stores the reference version-1
  schema and audio filenames. egui storage also preserves local editor state,
  mutes, theme, instrument display names, and colours for the bundled session.
  Project files do not embed decoded audio bytes; source packaging remains the
  companion workflow.

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

### Bugs

_None currently open - see Completed for resolved bugs._

### Planned Features

#### 0006-PLAN-006 — Alternate-song loading and website deployment

- **Card Title:** Alternate-song loading and website deployment
- **Description:** Add the folder-based flow for a different Game Boy `.fur` file with matching assets. Validate browser error handling, cache paths, and hosting from a nested website route. Produce and test the final optimized WASM bundle before publishing.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 07:19 — Runtime parser exists; user-facing selection and production deployment remain.

### Assigned

#### 0006-PLAN-005 — Cover art and remaining visual chrome

- **Card Title:** Cover art and remaining visual chrome
- **Description:** Recreate the animated dithered CD, Matrix field, trigger arcs, sidebar panels, and compact two-column page composition. Add the 1600×1600 cover export and responsive stacking used by the reference. Preserve the coarse pixel structure and both documented palettes.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** In progress.
- **Process Comments:** 2026-09-06 07:19 — Font, palettes, compact controls, and movable windows establish the current visual foundation. 2026-09-06 20:54 — Moved from Planned Features to Assigned. Re-reading the reference cover renderer and sidebar cards before implementation; browser/WASM remains the acceptance target.

#### 0006-ASGN-001 — Spectral Fusion numerical and listening validation

- **Card Title:** Spectral Fusion numerical and listening validation
- **Description:** Compare worker and native Rust outputs with controlled original-JavaScript renders for all six algorithms. Resolve material numerical differences and then collect user listening feedback on the bundled default settings. Record accepted limits for Smear randomness and platform-specific convolution behaviour.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** In progress; implementation, responsiveness, and golden numerical comparison are complete, but audible approval remains.
- **Process Comments:** 2026-09-06 07:19 — Browser integration, generation cancellation, preview updates, and clean completion have been verified. 2026-09-06 12:51 — Production-worker fingerprints on the real bundled sources exactly match original Freeze, Cross-Synth, Ring Mod, and Frequency Shift; Convolve passes at 1.397% within its 2% engine tolerance, randomized Smear passes at 20.089% within its 40% statistical bound, and native real-source regression bounds pass. No discrepancy met the threshold for a Bugs card.

### Completed

#### 0006-PLAN-004 — Mixer meters and instrument quick editing

- **Card Title:** Mixer meters and instrument quick editing
- **Description:** Complete editable dB controls, live peak and clipping meters, and default-project mixer restoration. Add instrument colour, name, transpose, volume, source, preview, and shortcut editing to the main table. Keep edits synchronized with movable Sampler and Spectral windows.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete in Rust-port commit `03e5d47`; the web mixer now exposes synchronized linear and dB controls with post-gain channel/master peak meters, while the always-visible instrument table provides every reference quick edit and both movable-editor shortcuts.
- **Process Comments:** 2026-09-06 07:19 — Linear gains, channel mute, master volume, preview, and editor shortcuts already existed. 2026-09-06 20:49 — Browser-verified live meters, clipping colour, direct dB entry, editable names/colours, transpose audition, volume/source changes, and Sampler/Spectral buttons. CHIP mode table audition uses a real matching tracker slice; SAMPLER mode uses the processed sample voice. Instrument names and colours persist through egui storage, all 22 workspace tests pass, the WASM check and release Trunk build pass, and `PARITY.md` now records completion.

#### 0006-PLAN-003 — Pattern, piano, and audition panels

- **Card Title:** Pattern, piano, and audition panels
- **Description:** Add the pattern order view, tracker cells, follow-playhead behaviour, piano state, and noise display. Implement row, cell, and instrument audition without disturbing transport state. Use the colours, boundary lines, held values, and hover explanations documented in `PARITY.md`.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete; the four-channel tracker, order/row navigation, live piano and Noise state, trigger flashes, and independent row/cell audition now work in both playback modes.
- **Process Comments:** 2026-09-06 07:19 — Furnace parsing and the shared song model already provide the required note data. 2026-09-06 17:40 — Moved from Planned Features to Assigned. The release browser renders the bundled 13-order × 64-row NOTE/INS/VOL/FX grid with held-instrument tints and no console warnings or errors; all 22 workspace tests and the release WASM build pass. 2026-09-06 17:55 — Live playback verified three coloured tonal keys plus the separate Noise readout. Paused cell audition left transport at 24.5 seconds, row audition sought precisely to order 3 row 8, CHIP and SAMPLER audition produced no console warnings/errors, and all 22 tests plus the release build pass.

#### 0006-PLAN-002 — Source sample library and import workflow

- **Card Title:** Source sample library and import workflow
- **Description:** Build the six-slot source panel with waveforms, duration, playback, loading, editing, packaging, and clearing. Ensure imported samples update affected instruments and invalidate Fusion and loop caches safely. Match the original source metadata and comments workflow.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete; six sparse source slots now support waveform and duration display, full-source preview, WAV/Ogg/MP3 loading, editable metadata, packaging, and a confirmed clear/reset workflow on native and web backends.
- **Process Comments:** 2026-09-06 07:19 — Three bundled sources currently load without the complete library UI. 2026-09-06 14:28 — Moved from Planned Features to Assigned after the project/export milestone was completed in Rust commit `deb9e5e`. 2026-09-06 14:56 — Browser verification loaded an Ogg into empty slot 3 without shifting existing indices, saved a renamed slot, played a full source, and opened/cancelled the clear confirmation with no console warnings or errors; workspace tests, WASM check, and release build pass.

#### 0006-PLAN-001 — Complete project JSON and audio exports

- **Card Title:** Complete project JSON and audio exports
- **Description:** Implement the original complete project schema rather than extending the current local-settings subset. Add compatible JSON import/export plus sampler WAV and numbered-sample ZIP generation. Preserve legacy field names, null source slots, metadata, and referenced audio filenames.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete; full Project JSON Copy/Apply, deterministic sampler WAV rendering, and numbered PCM-WAV sample ZIP packaging work in native and web builds.
- **Process Comments:** 2026-09-06 07:19 — Identified in the previous handoff as the next implementation milestone. 2026-09-06 12:54 — Moved from Planned Features to Assigned after commit `e743c5d` completed the numerical portion of Fusion validation. 2026-09-06 13:06 — The original version-1 fixture round-trips through the shared serde schema; browser testing generated a three-sample ZIP and a 105.8-second sampler WAV in about 1.2 seconds with visible success feedback and no console warnings/errors. Default mode, mixer, mutes, metadata, sampler state, and Fusion reconstruction are now applied rather than partially ignored.

#### 0006-BUG-001 — Browser download URL revoked before consumption

- **Card Title:** Browser download URL revoked before consumption
- **Symptom:** The initial browser export implementation could report a successful sample package while the in-app browser did not accept a corresponding download event. This affected generated ZIP and WAV files that use temporary Blob URLs. Static assets and native save dialogs were unaffected.
- **Root cause:** The Rust download helper revoked its object URL in the same JavaScript task immediately after clicking the temporary anchor. A browser may consume the click asynchronously, by which point the URL is already invalid.
- **Fix approach:** Keep the Blob URL alive for one second after the click, matching the delayed cleanup in the original HTML, then revoke it through a one-shot callback.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 13:01
- **Card Completion Note:** Resolved and browser-verified; package and sampler-WAV actions complete with visible success feedback and no console error.
- **Process Comments:** 2026-09-06 13:02 — Found during release-browser verification of `0006-PLAN-001`; fixed immediately before milestone sign-off.

#### 0006-BUG-002 — Project JSON controls clipped below viewport

- **Card Title:** Project JSON controls clipped below viewport
- **Symptom:** The first movable Project JSON window allowed its long multiline editor to grow past the browser viewport, leaving Copy and Apply unavailable below the canvas edge. The JSON itself generated correctly, but the import workflow was not usable at the tested window size. Resizing the outer browser was an avoidable workaround.
- **Root cause:** The editor was placed directly in the window with a large requested row count and no clipping scroll container. egui honored the text editor's content size beyond the available vertical space.
- **Fix approach:** Place the multiline editor inside a height-limited vertical ScrollArea so the JSON scrolls independently and the Copy/Apply row remains inside the movable window.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 13:04
- **Card Completion Note:** Resolved; the editor now has a bounded scrolling region and the action row remains part of the visible window layout.
- **Process Comments:** 2026-09-06 13:05 — Found while visually testing the release WASM build after Project JSON generation passed.

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
**Board Last Updated:** 2026-09-06 11:10 by Reason A  
**Session status 2026-09-06 11:10** — User confirmed the plugin "works great" testing live in **Bespoke Synth** (a new host for this project, alongside Carla/standalone) - `FREEZE-PLAN-004` and `FREEZE-PLAN-006` moved to Completed on that confirmation. Three new features requested in the same message are implemented this session and awaiting the same kind of hands-on confirmation in Bespoke Synth: Velocity Sensitivity (`FREEZE-PLAN-007`), GUI scaling via drag-to-resize (`FREEZE-PLAN-008`), and a clickable "[ Load Sample ]" sign replacing the empty waveform placeholder (`FREEZE-PLAN-009`). All committed, built, and bundled at the symlinked install path.

### Ideas

#### FREEZE-IDEA-001 — Master soft-saturation limiter as alternative/companion to gain compensation

- **Card Title:** Master soft-saturation limiter as alternative/companion to gain compensation
- **Description:** Current polyphony fix (`1/active_count` gain compensation in `VoiceManager::process_block`) guarantees no clipping but trades in a real, audible volume dip at 3+ held voices — acceptable to the user for now but flagged to revisit once there's a fuller instrument (params, real samples) to judge it against. A tanh-style master limiter, alone or layered with a gentler compensation curve, would keep single-note volume closer to constant at the cost of coloring loud chords. Needs a listening comparison once Phase D/E params exist to actually play with.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Pending; not scheduled.
- **Process Comments:** 2026-09-06 07:44 — User confirmed the current dip isn't a blocker; explicitly deferred to a later full-instrument listening pass.

### Bugs

_None currently open - see Completed for resolved bugs._

### Planned Features

#### FREEZE-PLAN-003 — Decouple Formant Shift from full re-analysis (Phase F)

- **Card Title:** Decouple Formant Shift from full re-analysis
- **Description:** Formant Shift currently requires the same full source-sample re-analysis as Freeze Point on every change. Decouple it so it only reprocesses the already-frozen magnitude spectrum (cheap), making live Formant Shift automation snappier than Freeze Point/Stereo Width, which genuinely need the expensive path.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Pending; final planned phase.
- **Process Comments:** 2026-09-06 07:45 — Pure optimization, deferred until the three params are actually wired and automatable (Phase D).

### Assigned

#### FREEZE-PLAN-007 — Velocity Sensitivity parameter

- **Card Title:** Velocity Sensitivity parameter
- **Description:** Add a "Velocity Sensitivity" control (0-100%) so notes can optionally ignore MIDI velocity entirely - requested because some controllers/playing styles don't want velocity scaling the frozen pad's volume.
- **Implementation:** New automatable `FloatParam` `velocity_sensitivity` (0-100%, default 100% - preserves the plugin's original gain-equals-velocity behavior exactly). Follows the same "cheap, per-block setter feeding note_on" pattern as `AdsrSettings`/`set_adsr()` (`FREEZE-PLAN-004`) rather than threading it through `note_on()`'s signature: `VoiceManager` gained a `velocity_sensitivity: f32` field and `set_velocity_sensitivity()` setter, called once per block in `process()` alongside `set_adsr()`. `note_on()` now computes `gain = 1.0 - velocity_sensitivity * (1.0 - velocity)` (a lerp between a fixed full gain at 0% and the original gain-equals-velocity at 100%) instead of using `velocity` directly as gain. A newly triggered voice picks up whatever sensitivity was most recently set; already-playing voices are unaffected, matching the ADSR precedent. Two new `freeze_dsp` tests (`voice::tests::zero_velocity_sensitivity_ignores_velocity`, `voice::tests::full_velocity_sensitivity_scales_output_with_velocity`) measure the settled per-sample output level directly rather than inspecting the private `gain` field, proving 0% ignores a low velocity entirely and 100% reproduces the original proportional scaling. UI: a `ParamSlider` placed right after the ADSR sliders in the editor.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Implementation complete; `cargo test --workspace` passes 49 `freeze_dsp` tests (47 + 2 new) + 3 `freeze_plugin` tests, release bundle rebuilt via `cargo xtask bundle freeze_plugin --release` and live at the symlinked install path. **Not yet confirmed by ear**: needs a quick check in Bespoke Synth - set Velocity Sensitivity to 0% and confirm soft/hard hits play at the same volume, then back to 100% and confirm normal velocity response returns.
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth.

#### FREEZE-PLAN-008 — GUI scaling via drag-to-resize

- **Card Title:** GUI scaling via drag-to-resize
- **Description:** The editor window (420×700 logical points) was reported as very small on the user's screen. Add a way to make the whole GUI bigger - not just a bigger window with more blank margin, but actually larger text/sliders/graphs.
- **Implementation:** `nih_plug_egui::resizable_window::ResizableWindow` (public API already in the pinned nih-plug rev, previously unused) now wraps the editor's whole `CentralPanel` content, adding a draggable bottom-right corner with `min_size` locked to the base `420×700` (`BASE_EDITOR_WIDTH`/`BASE_EDITOR_HEIGHT` constants) so it can only be dragged bigger, never smaller/more cramped than the original design. Simply dragging bigger alone would only add blank space, since the layout is coded in fixed logical points - so each frame, before drawing, the editor now reads back the *actual current* window size via `EguiState::size()` (already public) and calls `egui_ctx.set_zoom_factor(scale)` where `scale = average(current_width/BASE_WIDTH, current_height/BASE_HEIGHT)`, clamped to never go below 1.0. Because `ResizableWindow`'s corner-drag and `set_zoom_factor` both operate in the same logical-point coordinate space, this keeps the "points budget" our layout was designed for constant regardless of window size - so dragging the corner bigger makes every point map to more physical screen pixels (real, undistorted scaling of text/sliders/graphs) rather than just revealing empty margin. No changes needed to the private nih_plug_egui internals that normally gate window resizing (`EguiState::set_requested_size` stays crate-private) - `ResizableWindow` already exposes exactly the public hook needed. Window size (and therefore the user's chosen scale) is already persisted via the existing `#[persist = "editor-state"]` field, so it's remembered across sessions once set.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Implementation complete; `cargo build --workspace` clean with no warnings, release bundle rebuilt and live at the symlinked install path. **Not yet visually confirmed**: needs a quick check in Bespoke Synth - drag the bottom-right corner and confirm text/sliders/graphs actually get bigger (not just more blank space), and that nothing overflows or gets clipped at a couple of different drag sizes.
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth ("the GUI is very small on my screen").

#### FREEZE-PLAN-009 — Clickable "[ Load Sample ]" sign replacing the empty waveform placeholder

- **Card Title:** Clickable "[ Load Sample ]" sign replacing the empty waveform placeholder
- **Description:** Before a sample is loaded, the Freeze Point waveform area (`FREEZE-PLAN-005`) rendered as an empty box with a static "No sample loaded" caption - visually looked like a broken/empty waveform display rather than a call to action. Replace it with a clickable "[ Load Sample ]" sign that opens the same file dialog as the "Load Sample..." button.
- **Implementation:** `draw_freeze_point_waveform()` now branches on whether a source is loaded *before* choosing its `egui::Sense` (previously it always sensed `click_and_drag()` and only branched on what to draw): with nothing loaded, the area senses `click()` only, shows "[ Load Sample ]" text that brightens (and changes the cursor to a pointing hand) on hover, and the function returns `true` on the frame it's clicked - it deliberately does **not** know how to open a file dialog itself (that logic already existed inline in the "Load Sample..." button's click handler). Instead, the editor's update closure now defines `open_sample_dialog` once (a local closure over the existing `source`/`loop_buffer`/`trigger`/`params` captures) and calls it from both the waveform sign's click and the existing button's click, so the two entry points share one code path with no duplicated dialog/decode/re-render logic. When a source *is* loaded, behavior is unchanged from `FREEZE-PLAN-005` (waveform drawing, freeze-point marker, click/drag-to-set).
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Implementation complete; `cargo build --workspace` and `cargo test --workspace` both clean (no DSP changes, GUI-only). Release bundle rebuilt and live at the symlinked install path. **Not yet visually confirmed**: needs a quick check in Bespoke Synth with no sample loaded (or after a fresh instance) - confirm the sign reads "[ Load Sample ]", brightens on hover, and clicking it opens the file picker exactly like the button does.
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth.

### Completed

#### FREEZE-PLAN-004 — ADSR envelope parameters with a draggable visual graph

- **Card Title:** ADSR envelope parameters with a draggable visual graph
- **Description:** Implemented as designed, with one deliberate deviation from the original plan below: `note_on()`'s signature was kept **unchanged** (still `note(u8), channel(u8), velocity(f32), id(i32)`) rather than growing to accept ADSR values directly - `VoiceManager` instead gained an `adsr: AdsrSettings` field and a `set_adsr()` setter, called once per block in `process()` before MIDI handling, so `note_on()` just reads `self.adsr` internally. Less invasive (zero changes needed to any existing `note_on()` call site or test) and the effect is identical: a newly triggered voice picks up whatever `set_adsr()` most recently set, and already-playing voices are unaffected by later changes, matching "like most synths, changing Attack doesn't reshape a note already mid-decay."

  Delivered: `AdsrEnvelope` (renamed from `ArEnvelope`) in `freeze_dsp::envelope` adds a Decay stage between Attack and Sustain, exponentially approaching `sustain_level` using the same coefficient shape Release already used for approaching 0 - default `sustain_level = 1.0` makes Decay a no-op and exactly reproduces the old Attack/Release-only sound. Four automatable `FloatParam`s (Attack/Decay/Release in ms, Sustain in %) plus `draw_adsr_graph()` in the editor: a curve with three grabbable handles (attack-end: horizontal-only, always tops out at 1.0; decay-end/sustain-level: both axes; release-end: horizontal-only, always returns to 0), using the same `ParamSetter::set_parameter_normalized` pattern as the Freeze Point waveform widget (`FREEZE-PLAN-005`). Numeric `ParamSlider`s kept below the graph for precision, same as Freeze Point.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 08:40
- **Card Completion Note:** Complete in commit `f476036`; 47 `freeze_dsp` tests + 3 `freeze_plugin` tests passing (including exhaustive numerical verification that the envelope itself is monotonic - no overshoot - across a wide grid of attack/decay/sustain combinations). User confirmed the full instrument working well live in Bespoke Synth, which covers the previously-outstanding by-ear Decay/Sustain shape check (audibly decaying to a lower held sustain level, holding, then releasing) alongside the earlier Carla/Launchkey editor/playback verification already on record.
- **Process Comments:** 2026-09-06 08:41 — User confirmed full ADSR (not just exposing Attack/Release) plus a visual graph with grabbable points, not plain sliders. 2026-09-06 08:52 — Assigned to Reason A; `FREEZE-PLAN-005`'s waveform widget (same "custom-drawn, draggable egui widget over normalized param value" pattern) completed first and used as the template for this one's handle-dragging code. 2026-09-06 09:44 — Session paused here for a break; see Card Completion Note for the exact next step. 2026-09-06 11:10 — User confirmed "it works great" testing live in Bespoke Synth; moved to Completed.

#### FREEZE-PLAN-006 — MIDI activity indicator (note number + active voice count)

- **Card Title:** MIDI activity indicator (note number + active voice count)
- **Description:** Add a small live indicator to the editor showing whether MIDI is currently being received - last note number and current active voice count - so it's visually obvious when notes aren't reaching the plugin (e.g. a dropped JACK/host MIDI connection, see `FREEZE-BUG-001`'s false-alarm follow-up) versus a real DSP issue. Cross-thread state via `Arc<AtomicU8>` fields on `FreezePlugin` (matching the pattern nih-plug's own `gain_gui_egui` example uses for its peak meter), updated in `process()`'s existing MIDI-handling loop and after `VoiceManager::process_block` (voice count must be read *after* that call to reflect voices that just finished this block).

  Delivered as designed: `last_note: Arc<AtomicU8>` (sentinel `NO_NOTE = 255` for "nothing received yet, shown as "--"") and `active_voice_count: Arc<AtomicU8>` on `FreezePlugin`, both `Ordering::Relaxed` (display-only, no synchronization with other state needed). `last_note` is stored in the `NoteEvent::NoteOn` arm of `process()`'s existing MIDI loop; `active_voice_count` is stored once per block *after* `self.voices.process_block(...)` returns, per the Description's ordering requirement. Both Arcs are cloned into the `create_egui_editor` closure alongside the existing `source`/`loop_buffer` clones and read once per frame into a `ui.label` reading e.g. "MIDI: note 60 | 2 voice(s) active", placed right under the heading so it's visible without scrolling. No extra repaint wiring needed - confirmed `nih_plug_egui`'s `create_egui_editor` already calls `egui_ctx.request_repaint()` unconditionally every frame (for meter widgets in general), so this updates live for free.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 09:39
- **Card Completion Note:** Complete; `cargo build --workspace` and `cargo test --workspace` both clean (47 `freeze_dsp` + 3 `freeze_plugin` tests, unchanged count - this is pure display wiring with no new DSP behavior to unit-test), release bundle rebuilt via `cargo xtask bundle freeze_plugin --release` and live at the symlinked install path. User confirmed the full instrument working well live in Bespoke Synth, which covers the previously-outstanding visual confirmation of the "MIDI: note N | N voice(s) active" label.
- **Process Comments:** 2026-09-06 09:39 — Requested by the user directly after the `FREEZE-BUG-001` false-alarm investigation made clear how much faster that would have been ruled out with this visible. 2026-09-06 10:05 — Implemented and built; awaiting the user's visual confirmation pass. 2026-09-06 11:10 — User confirmed "it works great" testing live in Bespoke Synth; moved to Completed.

#### FREEZE-BUG-001 — Gain-compensation volume jump when a released voice finishes

- **Card Title:** Gain-compensation volume jump when a released voice finishes
- **Symptom:** Found by ear while testing ADSR (`FREEZE-PLAN-004`) on a real MIDI controller: playing overlapping notes, a still-sounding voice would suddenly jump in volume (~1.6x measured) at some point after an earlier note was released - not at release-start, at some later moment.
- **Root cause:** `VoiceManager::process_block`'s polyphony gain compensation (`1/active_count`, see `FREEZE-DONE-003`) recomputed its divisor fresh every block with no smoothing. Releasing voices are still "active" (correctly compensated for) right up until their envelope actually crosses the finish threshold and their slot is freed - at that exact instant the divisor steps (e.g. 0.5 -> 1.0 for 2 voices dropping to 1), and every *other* still-sounding voice gets that new multiplier instantly, an audible discontinuity. Confirmed by deterministic measurement: `aplaymidi` + `jack_capture` on two staggered notes showed RMS holding steady at ~0.037 while both played, then jumping to ~0.06 in the exact block where the released voice's slot freed.
- **Fix:** `VoiceManager` now smooths the compensation multiplier itself with a one-pole filter (`GAIN_COMPENSATION_SMOOTHING_MS = 30.0`), applied once per block as a post-sum multiply on the already-mixed output rather than per-voice inside the mixing loop - mathematically identical for a constant multiplier, but it's the only way to *smooth* a value that can change between blocks. The target is read from the voice count *before* that block's voices are processed (a voice finishing partway through a block was still contributing real signal for most of it, so that block must still be compensated as if it were active - using the post-removal count would apply the new divisor retroactively to audio that still included that voice). Test: `voice::tests::gain_compensation_ramps_smoothly_when_a_voice_finishes`, which processes one sample at a time to pinpoint the exact transition sample and isolate it from the envelope's own (legitimate) ongoing decay - verified to fail with `jump=0.5` against the old unsmoothed code and pass against the fix.
- **False-alarm follow-up worth knowing about**: after this fix, the user twice reported "the jump is back" after touching Decay/Sustain, including with just one held note. Exhaustively testing `AdsrEnvelope` itself across attack/decay/sustain combinations found zero non-monotonic behavior (mathematically can't overshoot - decay/release are both simple exponential approaches). The actual cause: unplugging and replugging the Launchkey MIDI controller during testing silently dropped its JACK connection to the plugin's MIDI input (`jack_lsp -c` showed zero connections afterward) - once reconnected, the "bug" was gone. Same category as the JACK-client-collision and stale-binary gotchas already on this board: before trusting a "still broken" report, check the physical/routing state hasn't quietly changed, not just the code.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 09:39
- **Card Completion Note:** Complete in commit `f476036`; regression test passing (47 `freeze_dsp` tests total), user confirmed live on real overlapping notes via a Launchkey Mini MK3 controller with no jump.
- **Process Comments:** 2026-09-06 09:39 — Prompted the user to request a MIDI-in activity indicator (note number + active voice count) in the GUI, both as a nice-to-have and as exactly the kind of diagnostic that would have made the false-alarm follow-up above faster to rule out - see `FREEZE-PLAN-006`.

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
