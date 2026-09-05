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
**Board Last Updated:** 2026-09-06 by Codex A

### Ideas

#### 0006-IDEA-001 — Additional Furnace chip-system support

- **Card Title:** Additional Furnace chip-system support
- **Description:** Extend runtime song loading beyond the currently supported Game Boy `.fur` subset. Add parsers and playback mappings only for systems backed by real fixture files and documented acceptance tests. Keep this separate from completing parity for the bundled 0006 experience.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 — Recorded as a possible extension after the Game Boy port is complete.

#### 0006-IDEA-002 — Persistent Fusion render cache

- **Card Title:** Persistent Fusion render cache
- **Description:** Cache rendered Spectral Fusion results by source and settings fingerprint across browser sessions. This could reduce startup preparation after the project and source import formats become stable. Measure storage cost and invalidation behaviour before scheduling it.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 — Added after moving live Fusion rendering into a browser worker.

#### 0006-IDEA-003 — Mobile-focused compact layout

- **Card Title:** Mobile-focused compact layout
- **Description:** Add a compact arrangement for narrow screens while retaining the original visual language. Keep transport and active editors usable without hiding essential audio state. Validate touch targets and movable-window behaviour on real mobile browsers.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending; idea has not been scheduled.
- **Process Comments:** 2026-09-06 — Original CSS stacks below 900 pixels, providing the starting reference.

### Planned Features

#### 0006-PLAN-001 — Complete project JSON and audio exports

- **Card Title:** Complete project JSON and audio exports
- **Description:** Implement the original complete project schema rather than extending the current local-settings subset. Add compatible JSON import/export plus sampler WAV and numbered-sample ZIP generation. Preserve legacy field names, null source slots, metadata, and referenced audio filenames.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending; scheduled after final Spectral Fusion validation.
- **Process Comments:** 2026-09-06 — Identified in `NEXTSTEPS.md` as the next implementation milestone.

#### 0006-PLAN-002 — Source sample library and import workflow

- **Card Title:** Source sample library and import workflow
- **Description:** Build the six-slot source panel with waveforms, duration, playback, loading, editing, packaging, and clearing. Ensure imported samples update affected instruments and invalidate Fusion and loop caches safely. Match the original source metadata and comments workflow.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 — Three bundled sources currently load without the complete library UI.

#### 0006-PLAN-003 — Pattern, piano, and audition panels

- **Card Title:** Pattern, piano, and audition panels
- **Description:** Add the pattern order view, tracker cells, follow-playhead behaviour, piano state, and noise display. Implement row, cell, and instrument audition without disturbing transport state. Use the colours, boundary lines, held values, and hover explanations documented in `PARITY.md`.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 — Furnace parsing and the shared song model already provide the required note data.

#### 0006-PLAN-004 — Mixer meters and instrument quick editing

- **Card Title:** Mixer meters and instrument quick editing
- **Description:** Complete editable dB controls, live peak and clipping meters, and default-project mixer restoration. Add instrument colour, name, transpose, volume, source, preview, and shortcut editing to the main table. Keep edits synchronized with movable Sampler and Spectral windows.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 — Linear gains, channel mute, master volume, preview, and editor shortcuts already exist.

#### 0006-PLAN-005 — Cover art and remaining visual chrome

- **Card Title:** Cover art and remaining visual chrome
- **Description:** Recreate the animated dithered CD, Matrix field, trigger arcs, sidebar panels, and compact two-column page composition. Add the 1600×1600 cover export and responsive stacking used by the reference. Preserve the coarse pixel structure and both documented palettes.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 — Font, palettes, compact controls, and movable windows establish the current visual foundation.

#### 0006-PLAN-006 — Alternate-song loading and website deployment

- **Card Title:** Alternate-song loading and website deployment
- **Description:** Add the folder-based flow for a different Game Boy `.fur` file with matching assets. Validate browser error handling, cache paths, and hosting from a nested website route. Produce and test the final optimized WASM bundle before publishing.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Pending.
- **Process Comments:** 2026-09-06 — Runtime parser exists; user-facing selection and production deployment remain.

### Assigned

#### 0006-ASGN-001 — Spectral Fusion numerical and listening validation

- **Card Title:** Spectral Fusion numerical and listening validation
- **Description:** Compare worker and native Rust outputs with controlled original-JavaScript renders for all six algorithms. Resolve material numerical differences and then collect user listening feedback on the bundled default settings. Record accepted limits for Smear randomness and platform-specific convolution behaviour.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** In progress; implementation and responsiveness are complete, but golden comparison and audible approval remain.
- **Process Comments:** 2026-09-06 — Browser integration, generation cancellation, preview updates, and clean completion have been verified.

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
- **Process Comments:** 2026-09-06 — User confirmed the implemented features work and that the appearance is progressing well.

#### 0006-DONE-006 — Spectral Fusion DSP, editor, and playback integration

- **Card Title:** Spectral Fusion DSP, editor, and playback integration
- **Description:** Implemented Freeze, Cross-Synth, Convolve, Ring Modulate, Frequency Shift, Smear, and formant shifting. Added A, B, and Result waveforms with source, amount, status, and enable controls in the movable editor. Routed the effective fused source through preview, song playback, waveform display, and loop preparation.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Complete in commit `d48d603`; invariant tests and native/WASM builds pass.
- **Process Comments:** 2026-09-06 — Exact golden audio comparison remains tracked separately in Assigned.

#### 0006-DONE-007 — Non-blocking web Spectral Fusion rendering

- **Card Title:** Non-blocking web Spectral Fusion rendering
- **Description:** Moved browser FFT and convolution work from egui's UI event handler into a dedicated web worker. Added transferable audio buffers, rendering status, generation tokens, stale-result rejection, and completion-driven preview and trim updates. Preserved the previous playable result while a replacement is calculated.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06
- **Card Completion Note:** Complete in Rust-port commit `f8c9033`; automated browser checks measured algorithm clicks at approximately 90–160 ms while rendering continued independently.
- **Process Comments:** 2026-09-06 — Rapid Cross-Synth-to-Smear changes and Convolve completion were verified with updated result waveforms and no console warnings or errors.
