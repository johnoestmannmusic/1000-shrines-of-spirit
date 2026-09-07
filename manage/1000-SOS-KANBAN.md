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
**Board Last Updated:** 2026-09-07 18:36 by Codex A
**Source of Truth:** This project section replaces the retired
`../../../0006-rust/NEXTSTEPS.md`. Update the relevant card and these handoff
notes after every feature, bug fix, or material verification result.

**Next-agent takeover:** No card is currently assigned. Start with
`0006-PLAN-006`: upload
`../../../0006-rust/release/lantern-player-0006-b27e01e.zip` over the current
`/1000-SOS/0006/` deployment, hard-refresh or purge that Cloudflare route if
the old hashed JavaScript remains, and smoke-test resolved bugs 004–007 plus
both audio modes on the live site. Then address `0006-ASGN-001` by asking the
user whether the deployed Spectral Fusion sound is accepted; move it to
Completed if yes, or record the exact audible mismatch as a new Bugs card
before changing DSP.

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
  WAV, original CHIP-mix, and sample ZIP exports, the six-slot source library, tracker/piano/noise
  views, row/cell/instrument audition, mixer meters, main-table instrument
  editing, animated cover art, reference sidebar, hover Explainer, and
  responsive stacking are implemented. Pattern/page scrolling is separated;
  dynamic sidebar and keyboard regions are size-stable; Source Sample and
  Sampler waveforms show instrument-coloured, numbered playheads with an orange
  raw-preview fallback. The Assigned and Planned buckets below
  are the authoritative remaining-work list.
- **Verification baseline:** `cargo test --workspace` passes 30 tests;
  `cargo check -p lantern-app --target wasm32-unknown-unknown` and
  `trunk build --release` pass. Release-browser checks cover transport,
  sampler and Fusion editing, import/export, tracker audition, live Web Audio
  meters, quick instrument edits, the desktop/sidebar composition, and narrow
  stacking, mode-dependent WAV exports, stable dynamic panels, tracker scrolling,
  live waveform playheads, and a release bundle served below a nested URL. Cover tests verify
  the 1600×1600 RGBA PNG and real tracker-note scan generation; folder tests
  verify valid layout assembly, one-based CHIP-only stem folders, and actionable
  missing-asset errors. `tests/golden-battletrain` browser-loads as a 16-order,
  94.4-second song and plays with no browser warnings/errors; its computed
  94.420-second effect timeline matches the 94.416-second Furnace stems within
  4 ms. Existing
  `lantern-fur` unused-code warnings are known.
  Numerical Fusion parity is recorded in `PARITY.md`;
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
  or samples rather than live Game Boy synthesis. The legacy pre-v240 `INFO`
  layout is supported for the modern old-format range that uses `INS2` and
  compressed `PATN`; `golden-battletrain` validates Furnace format 181.
- **Audio invariants:** Web stem looping deliberately uses Web Audio's native
  `AudioBufferSourceNode.loop` for sample-accurate looping. Sampler playback,
  preview, audition, and offline export share trim, transpose, ADSR, loop,
  ping-pong, pan, polyphony, and fused-source semantics. Loop buffers use 5 ms
  edge fades; mono/channel stealing uses an independent 8 ms cutoff. Fusion
  DSP must remain in `fusion-worker.js`: synchronous WASM FFT work previously
  caused multi-second UI hangs. Generation tokens reject stale worker results.
  Furnace `01xx`/`02xx` effects schedule continuous pitch-rate ramps in web,
  native, and offline sampler paths. `F0xx`, speed, tick-rate, and virtual-tempo
  effects feed one variable row clock shared by transport, seeking, tracker
  position, audition, scheduling, and export.
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
- **Card Completion Note:** In progress in Rust-port commits `af4fdde`, `017bd9b`, `2e42c85`, `f3b7100`, `db94ea4`, and `b27e01e`; folder loading, active-song downloads, nested-route preparation, effect-aware playback, real alternate-song acceptance, both WAV export modes, reproducible release packaging, and the first live upload are complete. The verified `b27e01e` bug-fix archive is ready for the next website transfer.
- **Process Comments:** 2026-09-06 07:19 — Runtime parser exists; user-facing selection and production deployment remain. 2026-09-07 06:56 — Moved from Planned Features to Assigned after completing visual chrome. Audited existing file-input and loader abstractions before adding folder replacement and nested-route release verification. 2026-09-07 07:02 — Added native recursive folder selection and web `webkitdirectory`, validation for exactly one supported `.fur` plus four stems and three source samples, optional Project JSON with a safe generated fallback, and atomic live-state replacement. `Trunk.toml` now emits relative JS/WASM URLs; the optimized bundle loaded successfully from `/dist/` with decoded audio and no browser warnings/errors. All 26 tests and the WASM/release builds pass. A second distinct Game Boy fixture with matching rendered assets is not present in the workspace, so that acceptance check remains open rather than being simulated with the bundled song. 2026-09-07 07:05 — The loader now retains the active `.fur` and optional MIDI, and the release toolbar exposes both downloads; the bundled reference MIDI is packaged. Nested-route browser loading remained clean after the change. The original 38 MB CHIP-mode WAV is still excluded from the optimized bundle and remains a documented parity gap. 2026-09-07 07:37 — Accepted the user-supplied `golden-battletrain` folder directly in the release browser. The loader now supports Furnace v181 `INFO`, one-based root WAV stems, and missing sampler sources for CHIP-only projects; its 16-order, 94.4-second transport played and tracked rows without browser warnings/errors. Added a shared effect-aware row clock plus continuous `01xx`/`02xx` pitch ramps across web, native, and offline sampler playback. All 30 tests, WASM check, and optimized build pass; live publishing remains the final deployment step. 2026-09-07 17:38 — Added the supplied full Furnace mix as a copied release asset without inflating the WASM module, retained named full mixes from runtime-loaded folders, and made `Save .WAV` follow the active engine: direct supplied-mix download in CHIP mode and offline render in SAMPLER mode. Both paths were exercised in the release browser; CHIP reported `Saved flight_school_night_shift.wav`, SAMPLER reported `Sampler WAV rendered`, and browser diagnostics remained clean. All 30 tests, the WASM check, and the release build pass. 2026-09-07 17:48 — Added a reproducible optimized release packager and host-neutral deployment checklist. `release/lantern-player-0006-db94ea4.zip` is a 30 MB archive containing the 47 MB static site; its archive checksum, every per-file checksum, and ZIP integrity all pass. The public route is LiteSpeed behind Cloudflare, but no hosting credentials or deployment connection exist in this workspace, so transfer to `/1000-SOS/0006/` is the sole remaining step on this card. 2026-09-07 18:36 — Released by Codex A for takeover after the user's first live upload succeeded. Upload and live-smoke-test the newer `b27e01e` archive, which contains resolved bugs 004–007.

#### 0006-ASGN-001 — Spectral Fusion numerical and listening validation

- **Card Title:** Spectral Fusion numerical and listening validation
- **Description:** Compare worker and native Rust outputs with controlled original-JavaScript renders for all six algorithms. Resolve material numerical differences and then collect user listening feedback on the bundled default settings. Record accepted limits for Smear randomness and platform-specific convolution behaviour.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** In progress; implementation, responsiveness, and golden numerical comparison are complete, but audible approval remains.
- **Process Comments:** 2026-09-06 07:19 — Browser integration, generation cancellation, preview updates, and clean completion have been verified. 2026-09-06 12:51 — Production-worker fingerprints on the real bundled sources exactly match original Freeze, Cross-Synth, Ring Mod, and Frequency Shift; Convolve passes at 1.397% within its 2% engine tolerance, randomized Smear passes at 20.089% within its 40% statistical bound, and native real-source regression bounds pass. No discrepancy met the threshold for a Bugs card. 2026-09-07 18:36 — Released by Codex A for takeover; next agent should collect the user's by-ear verdict against the deployed build rather than repeat the completed numerical work.

### Assigned

_None currently assigned._

### Completed

#### 0006-PLAN-005 — Cover art and remaining visual chrome

- **Card Title:** Cover art and remaining visual chrome
- **Description:** Recreate the animated dithered CD, Matrix field, trigger arcs, sidebar panels, and compact two-column page composition. Add the 1600×1600 cover export and responsive stacking used by the reference. Preserve the coarse pixel structure and both documented palettes.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-06 07:19
- **Card Completion Note:** Complete in Rust-port commit `6ea1f89`; the app now has the animated pixel cover, reference sidebar, live Explainer, responsive composition, and cover PNG export.
- **Process Comments:** 2026-09-06 07:19 — Font, palettes, compact controls, and movable windows established the visual foundation. 2026-09-06 20:54 — Moved from Planned Features to Assigned and re-read the reference renderer and sidebar implementation. 2026-09-07 06:54 — Implemented the deterministic 32×32 Matrix field, four-second spinning steel CD, instrument-coloured tracker-note scans, nearest-neighbour 1600×1600 RGBA PNG export, comments/timing/chips/license cards, sidebar mixer, and hover explanations for reference cards, tracker channels/rows/cells, and instrument names. The 300 px desktop sidebar and under-900 px stacked layout were browser-verified; all 24 tests, the WASM check, and release Trunk build pass.

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

#### 0006-BUG-003 — Legacy Furnace songs ignored effect timing and pitch slides

- **Card Title:** Legacy Furnace songs ignored effect timing and pitch slides
- **Description:** Load the user-supplied `golden-battletrain` Furnace 0.6 module and use it as the second real-song acceptance fixture. Preserve its global tempo automation in every row/time conversion and its continuous pitch slides in sampler playback. Accept its one-based root WAV stems without requiring unrelated source-sample assets.
- **Symptom:** The format-181 `.fur` initially failed at the newer `INF2` check, and the existing sequence assumed one fixed row duration while ignoring pitch effects. The folder validator also required zero-based stems under `ASSETS` plus three source samples, so the supplied valid CHIP-only export could not load.
- **Root cause:** The parser and loader had been scoped to the newer bundled-song layout, and row timing was represented by a single scalar. Parsed effect cells existed for display but were not compiled into transport or sampler scheduling.
- **Fix approach:** Parse the legacy `INFO` container for the `INS2`/`PATN` era, build an absolute variable-row timeline from Furnace timing commands, and emit continuous pitch-rate ramps for `01xx`/`02xx`. Share that data across browser/native scheduling, transport, seek/audition, tracker position, and offline export.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-07 07:37
- **Card Completion Note:** Resolved in Rust-port commit `2e42c85` and browser-verified with the real folder; the song loads as 16 orders and 94.4 seconds, plays with row tracking, and produces no warnings/errors.
- **Process Comments:** 2026-09-07 07:37 — The binary parser reports Furnace v181, six instruments, two wavetables, and real effect cells. The computed 94.420-second timeline matches all four 94.416-second Furnace stem exports within 4 ms; 30 workspace tests, WASM check, and release build pass.

#### 0006-BUG-004 — Pattern scrollbar overlapped page scrolling

- **Card Title:** Pattern scrollbar overlapped page scrolling
- **Symptom:** The Pattern grid's vertical scrollbar occupied the same far-right edge as the main page scrollbar. This made it easy for the tracker to capture the wheel when the user intended to move down the page. The competing scroll regions were especially awkward on a wide desktop viewport.
- **Root cause:** The Pattern `ScrollArea` expanded to the full central-panel width even though its four channel columns used substantially less space.
- **Fix approach:** Cap the tracker viewport at 680 px, just beyond the row index and four 150 px channel columns, while allowing narrower responsive layouts to use their available width.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-07 18:27
- **Card Completion Note:** Resolved in Rust-port commit `b27e01e`; the tracker scrollbar now sits beside the channel grid and the main scrollbar remains at the window edge.
- **Process Comments:** 2026-09-07 18:27 — Browser-verified independent wheel scrolling with the tracker ending around x=680 and the page scrollbar remaining at the central panel's far edge.

#### 0006-BUG-005 — Noise readout changed keyboard width

- **Card Title:** Noise readout changed keyboard width
- **Symptom:** Showing a Noise note and instrument name widened the readout beyond its empty-state size. The keyboard and surrounding content shifted horizontally as Noise notes started and stopped. Longer instrument names made the movement more noticeable.
- **Root cause:** The readout inherited the surrounding horizontal layout and specified only a minimum width, so its labels expanded the frame.
- **Fix approach:** Put the Noise content in a vertical child layout with an exact 110 px content width and fixed 58 px content height.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-07 18:27
- **Card Completion Note:** Resolved in Rust-port commit `b27e01e`; empty and active Noise states retain the same dimensions.
- **Process Comments:** 2026-09-07 18:27 — Browser-compared the empty readout with `E-5 / Perc Long` and `C-4 / Perc 1`; the keyboard and frame edges remained fixed.

#### 0006-BUG-006 — Explainer descriptions shifted the sidebar

- **Card Title:** Explainer descriptions shifted the sidebar
- **Symptom:** Hover descriptions of different lengths changed the Explainer card's height. Cards below it moved whenever the pointer crossed between tracker cells, panels, or cover art. The shifting made the reference sidebar visually unstable.
- **Root cause:** The card specified only a minimum height, leaving longer wrapped text free to enlarge it.
- **Fix approach:** Give the card a fixed 150 px body and place description text in its own bounded vertical scroll region.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-07 18:27
- **Card Completion Note:** Resolved in Rust-port commit `b27e01e`; browser checks across short and long descriptions keep the same outer frame height.
- **Process Comments:** 2026-09-07 18:27 — Verified cover/default and tracker-effect descriptions without movement in the cards below.

#### 0006-BUG-007 — Source and Sampler waveforms lacked readable playheads

- **Card Title:** Source and Sampler waveforms lacked readable playheads
- **Symptom:** Source Sample strips showed no cursor during raw preview or sampler voices, while the Sampler editor used a low-contrast white cursor. Multiple instruments sharing a source could not be identified. This differed from the original HTML's coloured, numbered playheads.
- **Root cause:** The audio trait exposed only one instrument-preview position and no visual state for live transport voices, pattern auditions, or raw Source Sample previews.
- **Fix approach:** Expose visual-only playheads with source, instrument, position, Fusion timebase, and level; draw every matching raw voice in its instrument colour with a number left of the line, and use orange for an unassigned raw preview.
- **Assigned Agent:** Codex A
- **Card Creation Date:** 2026-09-07 18:27
- **Card Completion Note:** Resolved in Rust-port commit `b27e01e`; Source Sample, Sampler, and Fusion result waveforms share the new playhead renderer.
- **Process Comments:** 2026-09-07 18:27 — Browser-verified a moving orange raw-source cursor and an instrument-0 cursor in its red colour with `0` at the line's upper-left; browser diagnostics remained empty. All 30 tests, WASM check, release build, new archive checksums, and ZIP integrity pass.

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

## Project: SpectralPrism Plugin — Rust/nih-plug VST3/CLAP Port

**Project Title:** SpectralPrism Plugin — Rust/nih-plug VST3/CLAP Port  
**Renamed from "SpectralFreeze" on 2026-09-06** — plugin display name, `CLAP_ID` (`com.johnoestmannmusic.spectral-prism`), `VST3_CLASS_ID` (`SpectralPrism001`, deliberately a *new* ID, not a relabel - see `FREEZE-PLAN-012`), bundle name, install-path symlink, project/crate/directory names (`freeze_dsp`/`freeze_plugin`/`freeze_cli` → `prism_dsp`/`prism_plugin`/`prism_cli`, `src/plugins/SpectralFreeze/` → `src/plugins/SpectralPrism/`), and this board's project title all updated. The DSP *algorithm* concept borrowed from 0006 is still called "Freeze" throughout the code (`FreezeFft`, `render_frozen_loop`, the "Freeze Point" param, etc.) - that names what the algorithm does (freezes a spectral snapshot), not the product, matching 0006's own terminology. Existing card IDs below keep their historical `FREEZE-` prefix per this board's "preserve card IDs" rule; read that prefix as "this plugin's" rather than literally "SpectralFreeze's" from here on.  
**Project Description:** Port the "Freeze" algorithm from `src/0006/index.html`'s Spectral Fusion feature into a standalone Rust/nih-plug instrument: load a sample, freeze a spectral snapshot into a sustained pad/drone, and play it polyphonically via MIDI as a VST3/CLAP plugin (and standalone binary). Built bottom-up as a pure-Rust, unit-tested DSP core (`prism_dsp`, no nih-plug/hardware dependency) first, then a thin nih-plug wrapper (`prism_plugin`) around it, so DSP correctness and plugin-build-pipeline correctness could be verified independently. `prism_cli` is a WAV-in/WAV-out test harness for fast by-ear DSP iteration without a host. This board (not a separate NEXT_STEPS.md, which has been removed) is the authoritative status/handoff doc for this project going forward.  
**Implementation Repository:** `../src/plugins/SpectralPrism/`  
**Primary Reference:** `../src/0006/index.html` (Spectral Fusion "Freeze" feature, and its dark palette/font for `FREEZE-PLAN-011`); full original design rationale in the plan doc at `~/.claude/plans/i-really-like-the-partitioned-papert.md` (same machine, not in this repo)  
**Architecture note:** Freeze Point / Formant Shift / Stereo Width are all *render-time* operations (they call `render::render_frozen_loop` again to take effect), not cheap per-sample transforms — relevant to FREEZE-PLAN-003 below and to anything that touches how they're triggered.  
**Board Last Updated:** 2026-09-07 02:40 by Reason A  
**Host-scaling caveat, worth knowing before chasing GUI-size reports again:** confirmed directly by the user across three hosts - Bespoke Synth doesn't scale our reported editor size at all, Carla scales it up dramatically (elements "massive"), and **Reaper matches what we actually intend**. Since this is `open_parented`-embedded (the surrounding window/frame belongs to the host, not this plugin - see `nih_plug_egui`'s `EguiWindow::open_parented`), and the exact `baseview` revision pinned here sets no X11 size hints at all, this is host-side interpretation of our reported size, not something fixable from plugin code. **Reaper is the reliable reference host for any future GUI-sizing verification** - a report of "too big" or "too small" from Bespoke Synth or Carla alone isn't actionable without also checking Reaper.  
**Session status 2026-09-07 02:40** — User confirmed `FREEZE-BUG-007`/`FREEZE-PLAN-018/019/020` (below) "looks and sounds great" live - all four moved from "not yet confirmed" to fully verified. One follow-up from that same check: Pitch Bend Range/Pan Center/Pan Width's three new rows pushed the right column past the visible height at the default size, requiring a scroll - `BASE_EDITOR_HEIGHT` bumped 460->600 to fit everything by default again (`FREEZE-PLAN-021`). Built and bundled; `cargo test --workspace` passes 65 tests unchanged (pure sizing constant, no logic change). **Not yet confirmed live** - needs a look to confirm all controls are now visible without scrolling at the default size, and that the window doesn't feel oversized either.

### Ideas

_None currently open - see Completed for resolved ideas._

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

_None currently assigned._

### Completed

#### FREEZE-PLAN-021 — Expand default window height to fit all controls without scrolling

- **Card Title:** Expand default window height to fit all controls without scrolling
- **Description:** After `FREEZE-PLAN-019`/`FREEZE-PLAN-020` added three more rows (Pitch Bend Range, Pan Center, Pan Width) to the right column, the default window size no longer fit everything, requiring a scroll to reach the new controls.
- **Implementation:** `BASE_EDITOR_HEIGHT` (the 1x-scale reference size the default window opens at `DEFAULT_SCALE` times) raised from 460 to 600. `BASE_EDITOR_WIDTH` (800) unchanged - only height was reported as an issue this time. The `ScrollArea` safety net added back in `FREEZE-PLAN-014` meant nothing was ever actually clipped or lost, just not visible without scrolling - this is a pure sizing-constant change, no layout logic touched.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 02:40
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 65 tests unchanged (pure constant change, no logic affected). Release bundle rebuilt and live at the symlinked install path. **Not yet confirmed live** - like the previous sizing passes, 600 is an estimate rather than a measurement against the real rendered layout; flag if it's still short or now noticeably oversized.
- **Process Comments:** 2026-09-07 02:40 — Requested directly after confirming the previous batch (`FREEZE-BUG-007`/`FREEZE-PLAN-018/019/020`) worked well overall, with this as the one follow-up needed.

#### FREEZE-IDEA-001 — Master soft-saturation limiter as alternative/companion to gain compensation

- **Card Title:** Master soft-saturation limiter as alternative/companion to gain compensation
- **Description:** Current polyphony fix (`1/active_count` gain compensation in `VoiceManager::process_block`) guarantees no clipping but trades in a real, audible volume dip at 3+ held voices — acceptable to the user for now but flagged to revisit once there's a fuller instrument (params, real samples) to judge it against. A tanh-style master limiter, alone or layered with a gentler compensation curve, would keep single-note volume closer to constant at the cost of coloring loud chords. Needs a listening comparison once Phase D/E params exist to actually play with.
- **Assigned Agent:** Unassigned
- **Card Creation Date:** 2026-09-06 07:45
- **Card Completion Note:** Both halves of this idea shipped, in two separate later cards once the "fuller instrument" this was waiting for existed: the tanh soft-saturation limiter itself (`soft_limit`, added in `FREEZE-BUG-002`), and the gentler compensation curve layered with it (`1/sqrt(n)` replacing strict `1/n`, `FREEZE-PLAN-018`, directly prompted by the user finding the old dip "way too extreme").
- **Process Comments:** 2026-09-06 07:44 — User confirmed the current dip isn't a blocker; explicitly deferred to a later full-instrument listening pass. 2026-09-07 02:15 — That listening pass happened; moved to Completed.

#### FREEZE-PLAN-020 — Per-voice pan randomizer (center + width)

- **Card Title:** Per-voice pan randomizer (center + width)
- **Description:** Add a per-voice pan randomizer with an adjustable center point (default 0, middle) and a pan width % (how far either side of center a voice's pan can randomly land).
- **Implementation:** Two new `FloatParam`s, `pan_center_pct` (-100 to 100%, default 0) and `pan_width_pct` (0-100%, default 0 - randomization off by default). `VoiceManager` gained `pan_center`/`pan_width` (normalized -1..1/0..1, set via `set_pan_settings()`, applied like `AdsrSettings` - only affects voices triggered after it's set) and a small dependency-free `SimpleRng` (xorshift32, deterministic given the same call sequence so it stays testable - not a real `rand`-crate dependency, since nothing here needs cryptographic or rigorous statistical quality, just a perceptually-varied spread). Each `note_on` draws `pan = (pan_center + pan_width * rng.next_bipolar()).clamp(-1.0, 1.0)`, stored on the new `Voice.pan` field, fixed for that voice's lifetime like gain/rate. Applied in `process_block` as a **linear** (not constant-power) per-channel gain: `left_gain = 1.0 - pan.max(0.0)`, `right_gain = 1.0 + pan.min(0.0)` - deliberately chosen over the more common constant-power law because it's *exact identity* at `pan == 0.0` (both gains exactly 1.0, i.e. the original unpanned behavior), which matters here because Stereo Width already bakes a real L/R difference into the source at render time - a constant-power law's mono-collapse-at-center would have quietly undone that whenever the pan feature is left at its default (every voice pans to exactly 0.0).
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 02:15
- **Card Completion Note:** Complete; `cargo test --workspace` passes 65 tests (55 `prism_dsp`, including new tests proving zero width/center is a no-op, hard pan silences the opposite channel, and pan values both stay within the configured width and actually vary across notes). Included in saved presets (`FREEZE-PLAN-010`), with a `#[serde(default)]` fallback of 0/0 (no panning) for presets saved before this existed. User confirmed live ("looks and sounds great").
- **Process Comments:** 2026-09-07 02:15 — Requested in the same batch as `FREEZE-BUG-007`/`FREEZE-PLAN-018`/`FREEZE-PLAN-019`.

#### FREEZE-PLAN-019 — MIDI pitch bend with adjustable range

- **Card Title:** MIDI pitch bend with adjustable range
- **Description:** Add MIDI pitch wheel support, with an adjustable semitone amount for how far the wheel bends.
- **Implementation:** `MIDI_INPUT` bumped from `MidiConfig::Basic` to `MidiConfig::MidiCCs` - required for `NoteEvent::MidiPitchBend` to actually be delivered at all (silently never arrives under `Basic`, confirmed by reading nih-plug's own doc comment on the variant). New `FloatParam` `pitch_bend_range_semitones` (0-24 semitones, default 2 - the MIDI/GM standard). The wheel's own raw position isn't a param (it's a continuous performance control, not a settable value) - `PrismPlugin` tracks it in a plain `pitch_bend_normalized: f32` field (nih-plug's own convention: normalized `[0, 1]`, `0.5` = no bend), updated from `NoteEvent::MidiPitchBend` in `process()`'s existing MIDI loop, then converted to semitones (`(normalized - 0.5) * 2.0 * range`) and pushed into `VoiceManager::set_pitch_bend_semitones()` once per block - *after* the MIDI loop, so a same-block wheel movement still takes effect for that block rather than being delayed to the next one. Unlike gain/pan/ADSR (decided once at `note_on`, only affecting new notes), pitch bend is applied to *every* currently-active voice's playback rate every block (`voice.rate * 2^(semitones/12)`, computed once per block, not per-sample) - matching real MIDI pitch bend, which bends whatever is already sounding on the channel.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 02:15
- **Card Completion Note:** Complete; `cargo test --workspace` passes 65 tests (55 `prism_dsp`, including new tests proving a 12-semitone bend exactly doubles the effective playback rate and that zero bend is exact identity). Included in saved presets, with a `#[serde(default)]` fallback of 0 (bend range disabled) for presets saved before this existed - an old preset recalled won't bend until the range is manually set again. User confirmed live ("looks and sounds great").
- **Process Comments:** 2026-09-07 02:15 — Requested in the same batch as `FREEZE-BUG-007`/`FREEZE-PLAN-018`/`FREEZE-PLAN-020`.

#### FREEZE-BUG-007 — Selected preset name doesn't survive a saved host project

- **Card Title:** Selected preset name doesn't survive a saved host project
- **Symptom:** Reported directly: "the plugin state does get saved with the project... but the chosen preset is not saved. It does load the preset correctly when selecting it from the Preset viewer though!"
- **Root cause:** The exact same class of gap `FREEZE-PLAN-010` already fixed once for the loaded *sample* - `selected_preset: Option<String>` lived on `PrismEditorState` (GUI-thread-only, recreated fresh every `editor()` call, never part of nih-plug's own state save/restore), not on `PrismPluginParams`. The underlying `FloatParam` values a preset applies genuinely do persist correctly on their own (confirmed by the user: "does get saved... that's great"), so the *sound* was never actually broken - only the editor's own display of *which preset produced it* reset to "(no preset)" after every reload, even though selecting that same preset again from the browser still worked exactly as expected (unsurprising, since applying a preset only ever touches the real persisted `FloatParam`s).
- **Fix:** Moved `selected_preset` from `PrismEditorState` to a new `#[persist = "selected-preset"] selected_preset: Mutex<Option<String>>` field on `PrismPluginParams`, mirroring `sample_path`'s existing pattern exactly. The combo box/Prev/Next code now clones it out of the lock briefly each frame for display rather than holding a lock across the frame, and writes to it (through the lock) on both preset load and preset save.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 02:15
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 65 tests (GUI/persistence-only change, no new DSP behavior to unit-test - the persistence mechanism itself is exactly what `FREEZE-PLAN-010`'s existing tests for `sample_path` already cover the shape of). Release bundle rebuilt and live at the symlinked install path. User confirmed live ("looks and sounds great").
- **Process Comments:** 2026-09-07 02:15 — Requested in the same batch as `FREEZE-PLAN-018`/`FREEZE-PLAN-019`/`FREEZE-PLAN-020`.

#### FREEZE-PLAN-018 — Gentler polyphony gain compensation (1/sqrt(n), not 1/n)

- **Card Title:** Gentler polyphony gain compensation (1/sqrt(n), not 1/n)
- **Description:** Reported directly: "our voice-volume scaling is way too extreme. When 1 voice is active, it is way louder than 2, which is way louder than 3."
- **Root cause / design history:** Not a bug so much as a tradeoff coming due - `FREEZE-DONE-003` chose strict `1/n` specifically because the gentler, standard `1/sqrt(n)` "still let 4 voices clip in practice" *at the time*, with no other safety net. Since then, `FREEZE-BUG-002` added `soft_limit` (a tanh soft-knee limiter) as a safety net for exactly this kind of correlated-signal overshoot - which `1/n`'s strict algebraic guarantee made partially redundant, at the cost of a very audible ~6dB-per-voice step (1→2 voices) that the user is now correctly identifying as too aggressive.
- **Fix:** `VoiceManager::note_on`'s ratchet (`active_compensation`, see `FREEZE-BUG-005`) now targets `1.0 / sqrt(new_total)` instead of `1.0 / new_total`. This no longer *algebraically* guarantees the mixed sum can never exceed a single voice's own peak in the fully-correlated worst case the way strict `1/n` did - `soft_limit` is now genuinely relied on as the safety net for that rarer case, rather than being redundant with an already-conservative-enough compensation curve. The ratchet mechanism itself (ratchets down when a new voice needs stricter compensation, never back up just because a sibling finished, resets to 1.0 only once fully silent) is unchanged - only the target formula changed.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 02:15
- **Card Completion Note:** Complete; `cargo test --workspace` passes 65 tests. `gain_compensation_scales_down_with_more_active_voices` renamed to `gain_compensation_uses_inverse_sqrt_of_voice_count` and rewritten to verify the new ratio directly (4 fully-correlated voices now sum to `sqrt(4)x` a single voice, not the same level `1/n` guaranteed); `gain_compensation_never_recovers_or_jumps_when_a_voice_finishes`'s expected 2-voice-compensated level updated from `0.25` (`1/2`) to `~0.354` (`1/sqrt(2)`); `chord_attack_at_zero_velocity_sensitivity_never_exceeds_unity` (the `soft_limit` safety-net test) re-verified passing unchanged, confirming the limiter still catches the worst case under the gentler curve. User confirmed live ("looks and sounds great").
- **Process Comments:** 2026-09-07 02:15 — Requested in the same batch as `FREEZE-BUG-007`/`FREEZE-PLAN-019`/`FREEZE-PLAN-020`. Directly resolves the tradeoff `FREEZE-IDEA-001` flagged and deferred ("acceptable to the user for now but flagged to revisit once there's a fuller instrument... to judge it against") - that fuller instrument now exists.

#### FREEZE-PLAN-017 — 30px outer margin around the editor content

- **Card Title:** 30px outer margin around the editor content
- **Description:** After confirming in Reaper (see this project's host-scaling caveat above) that the window size/scale itself is now correct, content was touching the window edges with no breathing room.
- **Implementation:** `ResizableWindow::show`'s content closure is now wrapped in an `egui::Frame::default().inner_margin(egui::Margin::same((EDITOR_MARGIN * scale) as i8))` before the existing `ScrollArea`. New constant `EDITOR_MARGIN = 16.0` (a 1x-scale value, multiplied by the live `scale` factor like everything else) - chosen so that `16.0 * DEFAULT_SCALE` (1.875) is exactly 30px at the default scale, matching what was asked for. `egui::Margin`'s fields are `i8` (an epaint size-optimization), hence the cast - not a concern at any realistic scale value.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 01:05
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 59 tests (GUI-only change). Release bundle rebuilt and live at the symlinked install path. **Not yet visually confirmed** - needs a look in Reaper specifically (the reliable reference host for GUI sizing right now) to confirm 30px reads as intended at the default scale, and that it still looks reasonable if the window is resized larger or smaller.
- **Process Comments:** 2026-09-07 01:05 — Requested directly after the user confirmed the sizing/scaling itself was correct in Reaper, with edge-touching content as the one remaining issue.

#### FREEZE-BUG-006 — Loading a sample doesn't take effect until Freeze Point is nudged

- **Card Title:** Loading a sample doesn't take effect until Freeze Point is nudged
- **Symptom:** Reported directly: "when a sample is loaded, we need it to activate the Freeze Position. Currently if you don't move Freeze Position after loading, it still keeps the previous 'freeze' in buffer and played."
- **Root cause:** Not the render pipeline itself - `render_worker.rs`'s worker thread has no deduplication logic and always re-renders using whatever `source` is *current* when it wakes, confirmed by reading it directly and by the existing `swapping_the_source_and_re_requesting_uses_the_new_source` test, which already covered exactly this scenario. The actual bug was in how the editor obtained its `RenderTrigger` at all: `editor()` computed `self.worker.as_ref().map(|worker| worker.trigger())` **once**, at the moment the host created the editor - and `self.worker` is `None` until `initialize()` runs and spawns it. nih-plug hosts aren't guaranteed to call `initialize()` before creating the editor (Bespoke Synth apparently doesn't, or not reliably), so an editor created early could permanently capture `trigger = None` for its entire session. Every subsequent "Load Sample" click would then correctly swap `source` and persist `sample_path`, but the `if let Some(trigger) = trigger { trigger.request_render(...) }` guard would silently no-op forever - the loaded audio just sat there unrendered until *something else* (nudging Freeze Point, which goes through `process()`'s own always-current `self.worker` on the audio thread, a completely separate code path) triggered a render that happened to pick up the already-swapped source.
- **Fix:** Decoupled `RenderTrigger`'s mailbox from `RenderWorker`'s lifecycle. `RenderTrigger::new()` now constructs a standalone mailbox with no worker required; `PrismPlugin` holds one (`trigger: RenderTrigger`) created in `Default::default()`, before anything else exists. `RenderWorker::spawn()` now takes a `RenderTrigger` as a parameter and reuses *its* mailbox rather than creating a fresh one internally, so a request sent before the thread even exists just waits in the mailbox as the first thing the loop processes once it does start. `editor()` now clones `self.trigger` directly - always valid regardless of whether `initialize()` has run yet - eliminating the `Option<RenderTrigger>` entirely from `load_sample_from_path`/`apply_preset`'s signatures. `process()` similarly calls `self.trigger.request_render(...)` directly instead of going through `self.worker`. `RenderWorker::trigger()`/`request_render()` (now unused by production code, since `PrismPlugin` holds its own trigger from construction) were removed rather than left as dead code; the tests that used them now hold their own `RenderTrigger` clone from the start instead, matching the corrected real usage pattern.
- **Test:** `render_worker::tests::request_sent_before_spawn_is_still_processed` reproduces the exact ordering bug directly: creates a `RenderTrigger`, calls `request_render` on it *before* `RenderWorker::spawn` is ever called, then spawns and confirms the pre-spawn request still gets processed.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 00:10
- **Card Completion Note:** Complete; `cargo test --workspace` passes 59 tests (50 `prism_dsp`, 9 `prism_plugin` including the new regression test), `cargo build --workspace` clean with no warnings (removing the two now-dead `RenderWorker` methods avoided a `never used` warning rather than suppressing it). **Not yet confirmed live** - needs a check in Bespoke Synth specifically: load a sample fresh (new plugin instance, editor open from the start) and confirm it's audibly playing without touching Freeze Point at all.
- **Process Comments:** 2026-09-07 00:10 — Found by reading nih-plug's actual `editor()`/`initialize()` call-ordering contract (or lack thereof) rather than guessing at the render pipeline, since the render worker itself already had a passing test proving the swap-and-rerender path works correctly in isolation - the bug had to be somewhere in *how* the trigger reached the editor, not in what the trigger did once called.

#### FREEZE-PLAN-016 — ADSR graph height matches the waveform graph

- **Card Title:** ADSR graph height matches the waveform graph
- **Description:** The Freeze Point waveform box and the ADSR envelope graph, side by side in the new two-column layout (`FREEZE-PLAN-014`), used different heights (70pt vs. 90pt) and visibly didn't line up.
- **Implementation:** Introduced a single `GRAPH_HEIGHT = 70.0` constant, now used by both `draw_freeze_point_waveform` and `draw_adsr_graph` for their `desired_size` height (each still multiplied by the live `scale` factor, so they stay matched at any window size, not just the default one). The ADSR graph's internal margin/handle sizing already scaled proportionally, so shrinking the box doesn't clip the curve or handles - it's simply a more compact envelope shape, matching the waveform strip's proportions.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 00:10
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 59 tests (GUI-only change). Release bundle rebuilt and live at the symlinked install path.
- **Process Comments:** 2026-09-07 00:10 — Requested directly in the same message as the scale/window-size follow-up.

#### FREEZE-PLAN-015 — Further GUI scale and window-size correction

- **Card Title:** Further GUI scale and window-size correction
- **Description:** `FREEZE-PLAN-014`'s 150% default and 620×560 base size weren't enough after trying it live - requested a further +25% on top of the current size, and a window shape that actually matches: "Currently it is a bit narrow (and elements are being cut off), but still very tall."
- **Implementation:** `DEFAULT_SCALE` raised from `1.5` to `1.875` (150% × 1.25, i.e. +25% on top of where it already was, per the literal request - not a fresh 125%). `BASE_EDITOR_WIDTH`/`BASE_EDITOR_HEIGHT` (the 1x-scale reference size both `apply_gui_scale` and `ResizableWindow`'s `min_size` use) changed from 620×560 to 800×460 - wider to stop the horizontal clipping, and shorter since the two-column layout doesn't need nearly as much vertical room as the previous guess assumed.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-07 00:10
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 59 tests (GUI-only change). Release bundle rebuilt and live at the symlinked install path. **Still an estimate, not a measurement** - like `FREEZE-PLAN-014` before it, these exact numbers (800×460, 1.875x) weren't verified against the real rendered layout; flag anything still cramped or still oversized so the next pass can adjust with an actual data point instead of another guess.
- **Process Comments:** 2026-09-07 00:10 — Requested directly after the user tried `FREEZE-PLAN-014`'s sizing live and found it still not quite right in both directions (scale too small, aspect ratio wrong).

#### FREEZE-BUG-005 — A held voice's volume climbs on its own when a sibling voice finishes

- **Card Title:** A held voice's volume climbs on its own when a sibling voice finishes
- **Symptom:** The user identified the actual mechanism behind the loudness-jump family of complaints (`FREEZE-BUG-001`'s original fix only addressed the *abruptness* of the jump, not its existence): "when old voices die, the volume scaling kicks in and makes the current voices even louder (while they are playing)."
- **Root cause:** `VoiceManager::process_block` recomputed its polyphony gain-compensation *target* fresh every block as `1.0 / active_voice_count()` (live count), then smoothed the *transition* toward that target over `GAIN_COMPENSATION_SMOOTHING_MS` (30ms) - `FREEZE-BUG-001` fixed the smoothing but never questioned whether the target itself should be allowed to recompute this way. Since the target depends on the *current* count, it necessarily goes back *up* (less attenuation) the instant any voice - even one totally unrelated to whichever voices are still sounding - finishes and frees its slot. The 30ms smoothing made this gradual rather than a hard step, but it never stopped it from happening: a still-playing, already-triggered voice's volume would climb on its own with no new note to justify it, exactly matching the user's description. This was the actual, intended behavior of the original design (`FREEZE-DONE-003`'s "chords get quieter, then louder again as notes release" polyphony compensation) - just not what a musician expects from a note they're already holding.
- **Fix:** Replaced the live-recomputed target with `VoiceManager::active_compensation`, a ratchet that only ever moves in one direction while anything is sounding: `note_on()` computes what compensation the *new* total voice count requires and, if that's stricter (lower) than the current ratchet level, lowers it immediately (affecting all currently-active voices, guaranteeing the sum-never-exceeds-a-single-voice's-peak safety property `FREEZE-DONE-003` established still holds) - but nothing ever raises it back up while voices remain active, so an already-playing voice's volume can only ever dip (still smoothed, for a new chord note) or hold steady, never climb, as siblings die. The ratchet only resets to 1.0 once the *last* voice finishes and everything goes silent (checked at the start of `note_on`, hard reset since nothing sounding can audibly jump) - so the next phrase starts fresh rather than being permanently stuck at some earlier chord's attenuation. `process_block`'s per-block work is now just smoothing toward whatever `active_compensation` currently is, with no live-count computation left in it at all. Correctly accounts for voice-stealing (overwriting a slot doesn't actually increase the total count, unlike claiming a genuinely free slot) so the ratchet target isn't overestimated when all `MAX_VOICES` slots are already full.
- **Test:** `voice::tests::gain_compensation_never_recovers_or_jumps_when_a_voice_finishes` (renamed and rewritten from the `FREEZE-BUG-001` regression test it grew out of) now proves three things together: no instant jump at the exact sample a voice frees its slot (the original check), the surviving voice's level staying flat at its 2-voice-compensated level indefinitely afterward rather than climbing back toward full (the actual bug), and that a fresh note played after complete silence correctly resets to full, uncompensated level (proving the design doesn't get permanently stuck attenuated).
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 23:15
- **Card Completion Note:** Complete; `cargo test --workspace` passes 58 tests (50 `prism_dsp`, `cargo test -p prism_dsp gain_compensation` specifically green). **Not yet confirmed live** - needs the exact scenario the user described: hold an overlapping chord, release one note, and confirm by ear the remaining note(s) do not get louder as the released note fades out and finishes.
- **Process Comments:** 2026-09-06 23:15 — User found this by ear after `FREEZE-BUG-001`'s fix (which addressed a different, earlier-discovered instance of the same underlying design gap) - correctly identified that the *existence* of the swell, not just its abruptness, was the actual problem.

#### FREEZE-PLAN-014 — Two-column layout and 150% default GUI scale

- **Card Title:** Two-column layout and 150% default GUI scale
- **Description:** Even with `FREEZE-PLAN-008`'s scaling mechanism available, the Medodica font (`FREEZE-PLAN-011`) made the default view too small to read - requested a two-column layout (stacking the two main sections horizontally instead of vertically) plus a 150% default scale, rather than relying on the user manually dragging the corner every time.
- **Implementation:** `BASE_EDITOR_WIDTH`/`BASE_EDITOR_HEIGHT` (the 1x-scale reference size `apply_gui_scale` and `ResizableWindow`'s `min_size` both use) changed from a tall single-column shape (420×700) to a wider, shorter two-column shape (620×560). A new `DEFAULT_SCALE = 1.5` constant multiplies those for the *initial* `EguiState::from_size()` call - since `apply_gui_scale` already derives its scale factor from `actual window size / BASE size` every frame, opening 1.5x bigger than the base automatically yields a 1.5x scale factor from the very first frame with no other change needed, while staying freely resizable (down to 1x, or further up) afterward via the existing corner-drag. The Freeze Point/Formant Shift/Stereo Width column and the Envelope/Velocity Sensitivity column - previously stacked top-to-bottom - now render side by side via `ui.columns(2, |columns| {...})`, each column's widgets (including the custom waveform/ADSR graphs, via `ui.available_width()`) sizing themselves to half the window automatically. The whole editor body is now wrapped in `egui::ScrollArea::vertical()` as a safety net (matching the precedent set by `0006-BUG-002`) in case the new base height estimate needs a follow-up tweak once seen live - content simply scrolls rather than getting clipped if so.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 23:15
- **Card Completion Note:** Complete; `cargo build --workspace` clean with no warnings, `cargo test --workspace` passes 58 tests (GUI-only change). Release bundle rebuilt and live at the symlinked install path. **Not yet visually confirmed** - the exact base dimensions (620×560) are an estimate, not measured against the real rendered layout; needs a look in Bespoke Synth to confirm nothing feels cramped or has excess dead space at the default 150% size, and that dragging back down to 100% still looks reasonable.
- **Process Comments:** 2026-09-06 23:15 — Requested directly after the user reported everything was too small to read with the new Medodica font in place.

#### FREEZE-PLAN-013 — Light theme with green accent (revises FREEZE-PLAN-011)

- **Card Title:** Light theme with green accent
- **Description:** After trying `FREEZE-PLAN-011`'s dark/amber theme live, the user asked for a white background with `#6cd73c` green as the primary/graph color instead.
- **Implementation:** All `COLOR_*` constants in `apply_theme()` switched from 0006's *dark* palette values to its *light* palette values (`--bg`/`--panel`/`--edge`/`--edge-hover`/`--ink`/`--dim`/`--surface-deep`), with two deliberate deviations from a literal light-palette port: `COLOR_BG`/`COLOR_PANEL` are pure white (`#ffffff`) rather than 0006's own slightly-off-white light `--bg` (`#fafafa`), and the accent (renamed `COLOR_AMBER` → `COLOR_ACCENT` throughout, since it's no longer amber-colored) is `#6cd73c` exactly as specified - not 0006's own light-mode `--amber` value (`#3f9e0e`), which is a different green. `Visuals::dark_mode` flipped to `false`, and the "active" widget state's text color (buttons/handles while being interacted with) changed from white to `COLOR_INK` (dark), since white text on the bright `#6cd73c` fill read as washed-out. The custom-drawn Freeze Point waveform and ADSR graph widgets already read their colors from these same `COLOR_*` constants (`FREEZE-PLAN-011`), so they picked up the new palette with no further changes needed beyond the constant values themselves.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 23:15
- **Card Completion Note:** Complete; `cargo build --workspace` clean with no warnings, `cargo test --workspace` passes 58 tests (GUI-only change). Release bundle rebuilt and live at the symlinked install path. **Not yet visually confirmed** - needs a look in Bespoke Synth to confirm the white background and green accent read as intended, and that text stays legible against both the white body background and the light `--surface-deep` graph/waveform backgrounds.
- **Process Comments:** 2026-09-06 23:15 — Requested directly after the user tried `FREEZE-PLAN-011`'s dark/amber theme live; confirmed the exact green (`#6cd73c`, which 0006 itself uses elsewhere as a hardcoded "on" state color, not its own light-theme `--amber` variable) before implementing.

#### FREEZE-PLAN-012 — Rename SpectralFreeze to SpectralPrism

- **Card Title:** Rename SpectralFreeze to SpectralPrism
- **Description:** Full rename of the plugin's product identity and internal scaffolding, requested by the user directly (not scoped in advance like the two cards above) - not just a display-name tweak, extended on request to cover "freeze" in filenames/crate names too.
- **What changed:** `Plugin::NAME` ("SpectralPrism"), `CLAP_ID` (`com.johnoestmannmusic.spectral-prism`), `VST3_CLASS_ID` (`SpectralPrism001` - a genuinely *new* 16-byte ID, not a relabel of the old one; see the note below), `bundler.toml`'s bundle name, the editor heading, the preset config directory (`~/.config/SpectralPrism/presets`), internal egui widget ID strings, and this board's project title. Structurally: `src/plugins/SpectralFreeze/` → `src/plugins/SpectralPrism/`, and all three crates renamed with `git mv` (preserving history) - `crates/freeze_dsp` → `crates/prism_dsp`, `crates/freeze_plugin` → `crates/prism_plugin`, `crates/freeze_cli` → `crates/prism_cli` - along with their `Cargo.toml` package names, the workspace member list, inter-crate path dependencies, the standalone binary name (`freeze_plugin_standalone` → `prism_plugin_standalone`), and every `use freeze_dsp::...` import. The plugin's own Rust types followed too: `FreezePlugin` → `PrismPlugin`, `FreezePluginParams` → `PrismPluginParams`, `FreezeEditorState` → `PrismEditorState`.
- **Deliberately left alone**: the DSP *algorithm* concept borrowed from 0006 is still called "Freeze" everywhere it appears as a technical term rather than product branding - `FreezeFft`, `FreezeResynth`, `render_frozen_loop`, the "Freeze Point" parameter/label, and this board's `FREEZE-` card-ID prefix (kept per this board's own "preserve card IDs" rule). This reads as "SpectralPrism's Freeze algorithm," matching how 0006 itself names the feature it was ported from - judgment call, flagged clearly in the project header above in case the user wants that swept too.
- **Real consequence worth knowing, not just a formality**: because `VST3_CLASS_ID` changed, any host (including the live Bespoke Synth/Carla sessions used for testing earlier this same day) sees this as a genuinely different plugin, not an updated version of the old one - a previously-saved project or host preset referencing the old ID will show it as missing, and the renamed plugin needs to be re-added as a fresh instance. The old `SpectralFreeze.vst3` symlink at the DAW's install path (`00_JOM-Common/Plugins/VST3/`) was removed (it pointed at a now-nonexistent build path after the directory rename) and replaced with a `SpectralPrism.vst3` symlink to the freshly built bundle.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 22:00
- **Card Completion Note:** Complete; `cargo build --workspace` and `cargo test --workspace` both clean across all three renamed crates (58 tests: 50 `prism_dsp` + 8 `prism_plugin`), release bundle rebuilt as `SpectralPrism.vst3`/`.clap` and live at the re-pointed symlink. Done as the last step of this session (after `FREEZE-PLAN-010`/`011`, committed separately) specifically so the rename is a clean, purely-mechanical diff on top of already-working code, not tangled up with feature changes. **Not yet confirmed live** - the renamed instance needs to be freshly added in Bespoke Synth (see the class-ID consequence above) and its saved parameter/preset behavior re-verified.
- **Process Comments:** 2026-09-06 22:00 — Requested by the user mid-session, in three follow-up messages that progressively confirmed scope: first "the plugin (and references in code)," then explicitly "this includes changing 'freeze' in filenames, etc, to 'prism'," then a reminder to also update the Kanban board's Project Title. Proceeded on the "surface identity + all scaffolding, but not the borrowed DSP-algorithm terminology" reading described above without stopping to ask, since a sensible default existed and asking would have interrupted a fast-moving session over a well-flagged judgment call.

#### FREEZE-PLAN-010 — Presets

- **Card Title:** Presets
- **Description:** Add an in-plugin preset browser (name/prev/next/save/save-as, independent of whatever the host's own project/preset system does) for the instrument's own parameters: Freeze Point, Formant Shift, Stereo Width, Attack/Decay/Sustain/Release, Velocity Sensitivity.
- **Prerequisite found while scoping this (fixed first, as planned)**: the loaded sample itself was **not persisted** anywhere nih-plug's own state save/restore covered - `source`/`loaded_filename` were plain fields on the plugin struct, outside the `#[derive(Params)]` struct (only `editor_state` there was `#[persist]`). Fixed by adding `sample_path: Mutex<Option<PathBuf>>` with `#[persist = "sample-path"]` (`Arc<std::sync::Mutex<T>>`... actually a bare `Mutex<T>` directly, since nih-plug's `PersistentField` is implemented for `std::sync::Mutex<T>` itself, no `Arc` wrapper needed - the whole params struct is already behind one `Arc`) alongside the existing `FloatParam`s. `PrismPlugin::initialize()` now checks it (state restore happens before `initialize()` runs, same as every param) and re-decodes the file from disk if present, falling back to the built-in placeholder tone (and logging via `nih_log!`) if the file has since moved or been deleted - an honest, accepted tradeoff for a personal tool rather than embedding raw audio bytes in every saved project/preset.
- **Implementation:** `load_and_prepare_sample()`/`load_sample_from_path()` factor the existing decode+resample+swap+persist+re-render logic out of the interactive "Load Sample..." dialog so `initialize()` and preset recall can reuse it exactly. `Preset` (serde, stored as pretty-printed JSON) captures all 8 DSP params plus the optional `sample_path` - `None` if the preset was saved on the placeholder tone, in which case recalling it leaves whatever's currently loaded untouched rather than resetting it. Presets live under `~/.config/SpectralPrism/presets/<name>.json` (a per-user directory, not next to the plugin bundle, which may not be writable) - `presets_dir()` returns `None` if `$HOME` isn't set, disabling the feature gracefully rather than panicking. Editor UI: a Prev (`◀`)/`ComboBox`/Next (`▶`) row for browsing (instant-apply on selection, like scrubbing through patches on a real synth) plus a name field + Save button, added right below the heading. Recalling a preset sets every param through the same `begin_set_parameter`/`set_parameter`/`end_set_parameter` pattern the custom graph widgets already use, so host automation recording sees a proper set rather than a silent jump.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:45
- **Card Completion Note:** Complete; `cargo test --workspace` passes 58 tests (50 `prism_dsp` + 8 new `prism_plugin` preset/round-trip tests), release bundle rebuilt and live at the symlinked install path. **Not yet confirmed live** - needs a quick check in Bespoke Synth: save a preset with a loaded sample, load the placeholder tone or a different sample, then recall the preset and confirm both the params and the sample come back; also confirm a saved host project restores its sample after a full close/reopen.
- **Process Comments:** 2026-09-06 11:45 — Requested by the user directly after confirming the plugin working well in Bespoke Synth. 2026-09-06 22:00 — Implemented, including the sample-persistence prerequisite exactly as recommended when this card was scoped.

#### FREEZE-PLAN-011 — Match 0006's visual aesthetic (colors, font, chrome)

- **Card Title:** Match 0006's visual aesthetic (colors, font, chrome)
- **Description:** Restyle the editor to match `../src/0006/index.html`'s look (the project's established visual language) rather than egui's stock dark theme.
- **Implementation:** `apply_theme()` (called once, from the editor's `build` callback - the palette/font never change at runtime, unlike `apply_gui_scale`'s per-frame sizing) installs the bundled Medodica font (`crates/prism_plugin/assets/MedodicaRegular.otf`, copied locally per the user's instruction rather than referenced across projects by relative path, so this plugin doesn't depend on the rest of the repo layout to build) as the first choice for both the proportional and monospace `egui::FontFamily`s, then recolors `egui::Visuals` (window/panel fill, all five `WidgetVisuals` states, selection, hyperlink, error/warn colors) to 0006's exact dark-palette hex values (`--bg` `--panel` `--edge` `--edge-hover` `--ink` `--dim` `--amber` `--surface-deep`, all as named `COLOR_*` constants) via `ctx.all_styles_mut()`. The custom-drawn Freeze Point waveform and ADSR graph widgets got the same treatment directly (surface-deep backgrounds with an `--edge` border stroke - a detail the originals didn't have - amber markers/handles/curves, an ink-tinted translucent waveform trace) since they paint themselves rather than reading `Visuals`. 0006's amber accent snapped the editor's pre-existing amber-ish marker/handle color (`rgb(240,180,60)`, an approximation) to the exact `--amber` value (`#c9973a` = `rgb(201,151,58)`). 0006's light palette (used only when the OS prefers light and no explicit choice was made) was deliberately not added - this editor has no theme toggle, so only the dark/default look applies.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:50
- **Card Completion Note:** Complete; `cargo build --workspace` clean with no warnings, `cargo test --workspace` passes 58 tests (GUI/asset-only change, no new DSP behavior to test). Release bundle rebuilt with the font asset bundled in and live at the symlinked install path. **Not yet visually confirmed** - needs a quick look in Bespoke Synth to confirm Medodica actually renders (falls back silently to the system default if the font file failed to load for any reason) and the palette reads as intended.
- **Process Comments:** 2026-09-06 11:50 — Requested by the user directly after `FREEZE-PLAN-010` (Presets) was queued, in the same session. 2026-09-06 22:00 — Implemented first (before Presets) since the user asked for aesthetics before presets specifically; font copied locally per explicit instruction ("feel free to copy the font into the plugin folder so it is locally referenced").

#### FREEZE-PLAN-007 — Velocity Sensitivity parameter

- **Card Title:** Velocity Sensitivity parameter
- **Description:** Add a "Velocity Sensitivity" control (0-100%) so notes can optionally ignore MIDI velocity entirely - requested because some controllers/playing styles don't want velocity scaling the frozen pad's volume.
- **Implementation:** New automatable `FloatParam` `velocity_sensitivity` (0-100%, default 100% - preserves the plugin's original gain-equals-velocity behavior exactly). Follows the same "cheap, per-block setter feeding note_on" pattern as `AdsrSettings`/`set_adsr()` (`FREEZE-PLAN-004`) rather than threading it through `note_on()`'s signature: `VoiceManager` gained a `velocity_sensitivity: f32` field and `set_velocity_sensitivity()` setter, called once per block in `process()` alongside `set_adsr()`. `note_on()` now computes `gain = 1.0 - velocity_sensitivity * (1.0 - velocity)` (a lerp between a fixed full gain at 0% and the original gain-equals-velocity at 100%) instead of using `velocity` directly as gain. A newly triggered voice picks up whatever sensitivity was most recently set; already-playing voices are unaffected, matching the ADSR precedent. UI: a `ParamSlider` placed right after the ADSR sliders in the editor.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Complete. The 0% case distorted on real chords - see `FREEZE-BUG-002` for the root cause and fix (a safety soft-limiter, unrelated to the gain formula itself, which was already correct). User confirmed 0% and 100% behavior after the fix.
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth. 2026-09-06 11:15 — User reported distortion at 0%; see `FREEZE-BUG-002`. 2026-09-06 11:45 — Fixed and moved to Completed.

#### FREEZE-PLAN-008 — GUI scaling via drag-to-resize

- **Card Title:** GUI scaling via drag-to-resize
- **Description:** The editor window (420×700 logical points) was reported as very small on the user's screen. Add a way to make the whole GUI bigger - not just a bigger window with more blank margin, but actually larger text/sliders/graphs.
- **Implementation:** `nih_plug_egui::resizable_window::ResizableWindow` (public API already in the pinned nih-plug rev, previously unused) wraps the editor's content, adding a draggable bottom-right corner with `min_size` locked to the base `420×700` (`BASE_EDITOR_WIDTH`/`BASE_EDITOR_HEIGHT` constants) so it can only be dragged bigger, never smaller/more cramped than the original design. Actual point-size scaling (not `set_zoom_factor` - see `FREEZE-BUG-003`) is applied via `apply_gui_scale()`: each frame, `scale = average(current_width/BASE_WIDTH, current_height/BASE_HEIGHT)` (from `EguiState::size()`, clamped to never go below 1.0) is used to set every `egui::Style` text size and spacing field (`item_spacing`, `button_padding`, `interact_size`, `slider_width`) to `default_value * scale` via `ctx.all_styles_mut()`, plus the custom-drawn waveform/ADSR graphs' own fixed pixel dimensions (heights, handle radius, margins) are multiplied by the same `scale`. Rebuilt from `egui::Style::default()`'s reference values every call (not compounded onto the previous frame's style) so it stays correct as the window is dragged to any size, in either direction. Window size (and therefore the user's chosen scale) is already persisted via the existing `#[persist = "editor-state"]` field, so it's remembered across sessions once set.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Complete. The original `set_zoom_factor`-based approach didn't actually reach built-in widgets (buttons/sliders stayed the same size while the width-stretching custom graphs appeared to grow) - see `FREEZE-BUG-003` for the root cause and the point-size-scaling fix above, which reaches every widget uniformly.
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth ("the GUI is very small on my screen"). 2026-09-06 11:15 — User reported "the graphs scale, but the buttons don't"; see `FREEZE-BUG-003`. 2026-09-06 11:45 — Fixed and moved to Completed.

#### FREEZE-PLAN-009 — Clickable "[ Load Sample ]" sign replacing the empty waveform placeholder

- **Card Title:** Clickable "[ Load Sample ]" sign replacing the empty waveform placeholder
- **Description:** Before a sample is loaded, the Freeze Point waveform area (`FREEZE-PLAN-005`) rendered as an empty box with a static "No sample loaded" caption - visually looked like a broken/empty waveform display rather than a call to action. Replace it with a clickable "[ Load Sample ]" sign that opens the same file dialog as the "Load Sample..." button.
- **Implementation:** `draw_freeze_point_waveform()` now takes an explicit `has_loaded_sample: bool` (see `FREEZE-BUG-004` for why it can't just check whether `source` is empty) and branches on that *before* choosing its `egui::Sense`: with nothing loaded, the area senses `click()` only, shows "[ Load Sample ]" text that brightens (and changes the cursor to a pointing hand) on hover, and the function returns `true` on the frame it's clicked - it deliberately does **not** know how to open a file dialog itself (that logic already existed inline in the "Load Sample..." button's click handler). Instead, the editor's update closure defines `open_sample_dialog` once (a local closure over the existing `source`/`loop_buffer`/`trigger`/`params` captures) and calls it from both the waveform sign's click and the existing button's click, so the two entry points share one code path with no duplicated dialog/decode/re-render logic. When a source *is* loaded, behavior is unchanged from `FREEZE-PLAN-005` (waveform drawing, freeze-point marker, click/drag-to-set).
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:10
- **Card Completion Note:** Complete. The sign never appeared because `initialize()` always pre-loads a built-in placeholder tone into `source` before the editor can even open, so "is `source` empty" was never true in practice - see `FREEZE-BUG-004` for the root cause and fix (gate on a real-file-loaded signal instead).
- **Process Comments:** 2026-09-06 11:10 — Requested directly after the user confirmed the plugin working well in Bespoke Synth. 2026-09-06 11:15 — User reported the sign never appeared; see `FREEZE-BUG-004`. 2026-09-06 11:45 — Fixed and moved to Completed.

#### FREEZE-BUG-002 — Distortion at 0% Velocity Sensitivity on chord attacks

- **Card Title:** Distortion at 0% Velocity Sensitivity on chord attacks
- **Symptom:** Reported by the user testing live in Bespoke Synth: "Velocity sensitivity works, but distorts at 0."
- **Root cause:** Not a bug in the gain formula itself (verified: `gain = 1.0 - sensitivity*(1.0-velocity)` never exceeds 1.0 for any input in range). The real cause is a pre-existing, previously-accepted tradeoff in `VoiceManager::process_block`'s polyphony gain compensation: the compensation *target* (`1/active_count`) is deliberately smoothed over `GAIN_COMPENSATION_SMOOTHING_MS` (30ms) rather than applied instantly (see `FREEZE-BUG-001`), so several voices attacked together (a real chord) genuinely sum well past unity for the first ~30-50ms while the smoothed multiplier is still chasing its new, lower target down - a numeric simulation using the project's own bundled-sample render peak (~0.64) and a 5-note chord found the transient reaching roughly 2.5x full scale. This was previously masked because real MIDI velocity sits below 1.0 most of the time, quietly providing headroom; Velocity Sensitivity at 0% forces every voice to gain 1.0 regardless of how hard a key is struck, removing that incidental headroom and making the pre-existing transient audible as real clipping for the first time.
- **Fix:** Added `freeze_dsp::voice::soft_limit()`, a soft-knee limiter (identity below `SOFT_LIMIT_THRESHOLD = 0.9`, tanh-shaped asymptotic approach to a 1.0 ceiling above it, C1-continuous at the threshold so there's no audible kink) applied as the final stage on the mixed output in `process_block`, after gain compensation. This is the master-limiter idea already on this board (`FREEZE-IDEA-001`), scoped down to a safety net rather than a tone-shaping feature. New regression test `voice::tests::chord_attack_at_zero_velocity_sensitivity_never_exceeds_unity` reproduces the exact reported scenario (5 notes, varied real-world velocities, sensitivity 0.0, realistic buffer amplitude) sample-by-sample and asserts peak output never exceeds unity. Three existing tests (`distinct_channels_produce_distinct_output`, `gain_compensation_ramps_smoothly_when_a_voice_finishes`, `buffer_swap_crossfades_instead_of_clicking`) used full-scale (1.0) buffer amplitudes for reasons unrelated to peak level (channel fidelity, compensation-jump smoothness, crossfade smoothness respectively) and needed their fixture amplitudes lowered below the new threshold so the limiter doesn't confound what they're actually testing.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:15
- **Card Completion Note:** Complete; `cargo test -p freeze_dsp` passes 50 tests (49 + 1 new regression test, 3 adjusted). Numerically verified via a Python simulation of the exact transient (see root cause) before implementing the fix, and the new regression test fails without `soft_limit` applied and passes with it.
- **Process Comments:** 2026-09-06 11:15 — Found investigating the user's report; root-caused via first-principles simulation of the gain-compensation smoothing window rather than guesswork, since the gain formula itself was already confirmed correct.

#### FREEZE-BUG-003 — GUI scaling only affected custom-drawn graphs, not built-in widgets

- **Card Title:** GUI scaling only affected custom-drawn graphs, not built-in widgets
- **Symptom:** Reported by the user testing live in Bespoke Synth: "The graphs scale, but the buttons don't."
- **Root cause:** `FREEZE-PLAN-008`'s original implementation called `egui::Context::set_zoom_factor()`, but this project's pinned `nih_plug_egui`/`egui_baseview` revision never applies that at render time on Linux: `egui_baseview::window.rs` tracks its own `pixels_per_point` field, recomputed on every host resize event strictly from the fixed `WindowScalePolicy` nih_plug_egui chose at window-creation time (`Some(1.0)` on Linux, per `create_egui_editor`'s `#[cfg(not(target_os = "macos"))]` branch) - it never reads back whatever `Context::pixels_per_point()`/zoom_factor was set to internally, and the actual GPU render call (`self.renderer.render(..., self.pixels_per_point, ...)`) uses that same fixed-and-reset field, not the context's. So dragging the corner bigger only ever grew the real window in "points" (since points==pixels at a permanently-1.0 `pixels_per_point`) - our custom `draw_freeze_point_waveform`/`draw_adsr_graph` widgets, which explicitly size themselves to `ui.available_width()`, genuinely got wider and looked "scaled"; built-in widgets like `ui.button()` and nih_plug_egui's `ParamSlider` don't stretch to available width (they auto-size from font/content), so they stayed visually the same absolute size no matter how big the window got.
- **Fix:** Replaced the `set_zoom_factor()` call with `apply_gui_scale()`, which instead scales the actual "points" sizes everything is laid out in - font sizes for every `egui::TextStyle` and spacing fields (`item_spacing`, `button_padding`, `interact_size`, `slider_width`) via `ctx.all_styles_mut()`, computed fresh from `egui::Style::default()`'s reference values each frame (not compounded) so the exact size is correct regardless of how the window was last resized. `ParamSlider`'s height derives from `ui.text_style_height(&TextStyle::Body)` and its width from `ui.spacing().slider_width`, and `ui.button()`'s size derives from its text's `TextStyle::Button` size plus `button_padding` - so scaling those two categories of style field reaches every built-in widget uniformly, the same way it already reached the custom graphs. The custom graphs' own fixed pixel constants (heights, ADSR handle radius, margins) are now also explicitly multiplied by the same `scale` value (passed as a new parameter) rather than relying on width-stretching alone.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:15
- **Card Completion Note:** Complete; `cargo build --workspace` clean with no warnings, `cargo test --workspace` passes 50 `freeze_dsp` + 3 `freeze_plugin` tests (GUI-only change, no new DSP tests needed). Root-caused by reading `egui_baseview`'s actual window-loop source rather than assuming `set_zoom_factor` would work, which is what caught the fixed-`pixels_per_point`-at-render-time behavior.
- **Process Comments:** 2026-09-06 11:15 — Found investigating the user's report; the fix generalizes better than the original approach anyway, since it doesn't depend on whatever a given `nih_plug_egui` backend does with `set_zoom_factor()`.

#### FREEZE-BUG-004 — "[ Load Sample ]" sign never appeared

- **Card Title:** "[ Load Sample ]" sign never appeared
- **Symptom:** Reported by the user testing live in Bespoke Synth: "The Load Sample clickable sign hasn't seemed to appear."
- **Root cause:** `FREEZE-PLAN-009`'s original implementation gated the sign on whether `source.load()` was empty. But `FreezePlugin::initialize()` *always* populates `source` with a built-in synthetic placeholder tone before the editor can ever be opened (a host calls `initialize()` before creating the editor) - so `source` is never actually empty in practice, and the sign's branch could never trigger. A related bug found while fixing this: the originally-planned gate (`FreezeEditorState::filename`) also wouldn't have survived closing and reopening the editor window within the same plugin instance, since `FreezeEditorState::default()` is recreated fresh on every `editor()` call - even after fixing the emptiness check, a real loaded file's name (and therefore "has something really been loaded") would have appeared to reset every time the GUI was closed and reopened.
- **Fix:** Added `loaded_filename: Arc<ArcSwapOption<String>>` directly on `FreezePlugin` (alongside `source`, not in the per-editor-session `FreezeEditorState`) - `None` until the user has loaded a real file via `open_sample_dialog`, and read every frame to gate `draw_freeze_point_waveform`'s sign *and* the "Loaded: <name>" / "Using built-in placeholder tone" label, both of which now correctly survive the editor being closed and reopened.
- **Assigned Agent:** Reason A
- **Card Creation Date:** 2026-09-06 11:15
- **Card Completion Note:** Complete; `cargo build --workspace` clean, `cargo test --workspace` passes 50 `freeze_dsp` + 3 `freeze_plugin` tests. The editor-reopen persistence issue was caught by re-reasoning through the fix before shipping it, not by a test (GUI editor lifecycle isn't exercised by the automated test suite) - worth a manual check in Bespoke Synth: load a sample, close the editor window, reopen it, and confirm it still shows the loaded waveform and filename rather than reverting to the sign.
- **Process Comments:** 2026-09-06 11:15 — Found investigating the user's report; the editor-reopen issue was a self-caught regression in the first draft of this same fix, not a second user report.

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
