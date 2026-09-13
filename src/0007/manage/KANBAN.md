# 0007-Electron — Lantern Music Player (TypeScript / Electron) — Project Kanban

How to use:
This file is the shared Kanban Markdown coordination board for porting the
Rust egui/eframe Lantern Music Player (`../SourceRepo/1000-shrines-of-spirit/src/0007/`)
to a TypeScript Electron application in this repository.
Agents should only assign themselves to cards not already assigned, move cards
between buckets instead of duplicating them, preserve card IDs, and append dated
process comments after each completed feature or verification.
Buckets: **Ideas**, **Bugs**, **Planned Features**, **Assigned**, **Completed**.
A found bug gets a card in Bugs (symptom, root cause once known, fix approach);
once verified fixed it moves to Completed like any other card, keeping its ID.
Dates include a time (HH:MM).

---

## Project: 0007-Electron — Lantern Music Player (TypeScript / Electron)

**Project Title:** 0007-Electron — Lantern Music Player (TypeScript / Electron)
**Project Description:** Reimplement the Rust egui/eframe Lantern Music Player
as a TypeScript + Electron desktop app. Web Audio (Chromium) replaces the Rust
web audio backend; the Spectral engine's `prism_dsp` phase vocoder is compiled
to WebAssembly and called from TypeScript.
**Source of Truth (Rust):** `../SourceRepo/1000-shrines-of-spirit/src/0007/` at
commit `5948fdc` ("Handover").
**Rust Kanban:** `../SourceRepo/1000-shrines-of-spirit/src/0007/manage/KANBAN.md`
**Parity Checklist:** `../SourceRepo/1000-shrines-of-spirit/src/0007/PARITY.md`
**Original HTML:** `../SourceRepo/1000-shrines-of-spirit/src/0006/index.html`
**Board Last Updated:** 2026-09-13 10:00 by opencode

### Branding (user-confirmed)

- **Font:** the bundled Medodica Regular (`MedodicaRegular.otf`), exposed as the
  `"Medodica"` CSS family. (User wrote "Melodica"; the asset/upstream family is
  Medodica — flag if the file should be renamed.)
- **Light theme is the default** (the `System` option is removed). Light uses
  the brand green `#6cd73c` on `#ffffff` panels.
- **Dark theme** remains available via the header toggle; choice persists in
  `localStorage`.

### How this backlog was produced (2026-09-12 18:10)

A fresh parity audit compared the Electron port against:
1. the Rust `manage/KANBAN.md` "Technical Handoff Notes" (the authoritative
   feature description of the shipped Rust app),
2. every Rust Completed card (`0006-PLAN-001..031`, `0006-BUG-001..008`,
   `0006-DONE-001..007`, `0006-IDEA-001..003`),
3. `PARITY.md`'s functional checklist and "Audio and editor behaviours" list,
4. direct inspection of `lantern-app/src/{app,pattern,editor,cover}.rs`.

Cards below are **features present in Rust but missing or partial in the
Electron port**. Rust's own only-open item — `0006-PLAN-025`'s `.FUR` writer —
is *also* unimplemented in Rust (both apps passthrough the loaded bytes), so it
is captured as an optional card, not a parity gap.

### Technical Handoff Notes

- **Porting map (Rust -> TS):**
  | Rust | TS location |
  | --- | --- |
  | `lantern-fur` | `src/core/fur/` |
  | `lantern-core` | `src/core/{songModel,timing,pitch,sampler,spectral,project,export,midi,dsp,tracker}.ts` |
  | `lantern-audio` (web) | `src/audio/{backend,webSampler,webAudioBackend}.ts` |
  | `lantern-app` | `src/renderer/` (React) |
  | `prism_dsp` | `native/prism-wasm/` -> `src/renderer/vendor/prism/` + `src/wasm/{prism,prism.worker,prismWorkerClient,prismWorkerProtocol}.ts` |
  | bundled-asset I/O | `src/main/` + `src/preload/` (folder hot-loading removed; see `0007E-PLAN-111`) |
- **Dropped by design:** native `rodio` backend, `eframe` storage, `trunk`
  packaging, `System` theme.
- **Byte-format contracts:** WAV PCM16 (LE asymmetric scaling), ZIP STORE
  (DOS date `0x0021`, CRC-32), MIDI (`TICKS_PER_ROW = 24`, big-endian, no
  running status), Project JSON v1 (camelCase, legacy `rootNote` migration,
  Rust serde `NoteValue` encoding), cover PRNG seed `9001`, offline mixdown
  PRNG seed `"LANTERN\x01"`.
- **WASM build:** `npm run build:prism-wasm` uses `wasm-bindgen-cli` after
  `cargo build --target wasm32-unknown-unknown`; output committed under
  `native/prism-wasm/pkg/` and copied to `src/renderer/vendor/prism/`.

### Commands

```
npm run dev              # Vite renderer + Electron (renderer hot reload)
npm run build            # esbuild main/preload + vite renderer -> dist/
npm run typecheck        # renderer + node tsconfigs
npm test                 # Vitest unit tests
npm run test:e2e         # Playwright Electron smoke test (needs dist/)
npm run audit:deps       # dependency-provenance / slopsquatting audit
npm run build:prism-wasm # rebuild prism_dsp WASM (needs wasm-bindgen-cli)
npm run build:web        # static web build -> dist/web (Electron + browser)
npm run serve:web        # serve dist/web for browser testing
npm run test:web         # Playwright browser test against dist/web (uses Chrome)
npm run test:all         # typecheck + unit + audit + build + web + e2e
```

### Verification baseline (2026-09-12 22:20)

- `npm run typecheck` — clean.
- `npx vitest run` — **6 files, 37 tests passing**, including the real
  `prism_dsp` WASM engine (freeze + cross-synth) instantiated in-process.
- `npm run audit:deps` — 40 checks passed, 0 failures; 172 transitive lockfile
  packages registry-resolved with sha512.
- `npm run build` — renderer 326 kB JS + 326 kB WASM (102 kB gzip) + hashed font.
- `npx playwright test` — 7 tests: smoke, EDIT MODE (select/navigate + centred
  scroll), editor modal, project (confirm New Project + hamburger delete),
  features (comments/Base Tempo/no audition error), and JSON Load auto-apply.

---

## Ideas

- **0007E-IDEA-001** — Spectrogram/FFT inspector panel using prism_dsp frames.
- **0007E-IDEA-002** — `electron-builder` packaging; compress/generate the
  ~38 MB CHIP mix WAV. (Rust: `0006-PLAN-006` was scrapped; user deploys.)
- **0007E-IDEA-004** — Additional Furnace chip-system support beyond Game Boy.
  (Rust `0006-IDEA-001`; out of current parity scope.)
- **0007E-IDEA-005** — Persistent Fusion render cache keyed by source + settings
  fingerprint. (Rust `0006-IDEA-002`.)
- **0007E-IDEA-006** — Mobile-focused compact layout validated on real mobile
  browsers. (Rust `0006-IDEA-003`.)

## Bugs

_(none open — see 0007E-BUG-001…017 in Completed)_

## Planned Features — Remaining Rust parity backlog

All tracked Rust-parity cards except the optional Rust-deferred `.FUR` writer
have now landed.

- **0007E-PLAN-051** — *Optional (mirrors Rust's own open item):* **Save .FUR
  writer from live editor state** (splice SNG2/PATN, recompute the block-pointer
  table, re-wrap in zlib). Rust deliberately deferred this because it could not
  validate against a real Furnace install; both apps currently pass through the
  loaded bytes. Verify against a real Furnace build before trusting it.

## Assigned

_(none currently)_

## Completed

- **0007E-PLAN-000** — Architecture analysis. *(2026-09-12 17:10)* Mapped all
  four Rust crates + `prism_dsp`; confirmed Electron/Chromium → port the web
  backend, drop `rodio`.
- **0007E-PLAN-001** — Scaffold. *TS 7 removed `baseUrl`; paths are `./src/*`.*
- **0007E-PLAN-002/003/004** — Core ports: `.fur` parser (INF2/SNG2/ADIR/INS2/
  WAVE/PATN/legacy INFO), song model + timelines, timing, pitch, sampler
  settings + look-ahead `Scheduler`, project v1 schema, WAV/ZIP, MIDI writer,
  offline `renderSamplerMix`. Verified against the v251 and v181 fixtures.
- **0007E-PLAN-005** — Unit tests (parser, model, scheduler, envelopes,
  ping-pong, project, WAV/ZIP, MIDI, mixdown, tracker helpers, WASM).
- **0007E-PLAN-006/007/008** — Web Audio backend + React vertical slice +
  Electron main/preload; verified by E2E.
- **0007E-PLAN-009/010** — Slopsquatting dependency audit + Playwright E2E.
- **0007E-PLAN-011b** — prism_dsp WASM build + Spectral wiring (8 modes).
- **0007E-PLAN-012** — Cover art, piano visualiser, Light/Dark theming.
- **0007E-PLAN-013** — Tracker EDIT MODE (selection/keybinds/copy/paste/
  interpolate/Pattern Manager/undo-redo).
- **0007E-PLAN-014** — Sampler + Spectral editor windows.
- **0007E-PLAN-015** — Folder-load / song-swap.
- **0007E-PLAN-016** — Exports/IO (FUR/MIDI/JSON/WAV/ZIP).
- **0007E-PLAN-017** — Transport/audition/sample waveforms.
- **0007E-PLAN-018** — Source Sample windows.
- **0007E-BUG-001** — 2-byte `tagEquals` bug. Fixed; regression-tested.
- **0007E-PLAN-019/021/022/023/024/025 — Sidebar chrome.** *(2026-09-12 18:15)*
  Hover Explainer card (shared React context, wired to tracker cells/rows/
  channels and mixer/instrument hovers), Song Comments, editable-in-EDIT-MODE
  Timing card (BPM + tick rate/speed/virtual tempo/highlights applied live and
  retimed), Chips card, Licenses + page links, and editable Title/Artist/Album.
- **0007E-PLAN-027/028/029/030 — Quick edits.** *(2026-09-12 18:15)* Editable
  channel/master dB text fields (commit on Enter/blur, `-∞` support), editable
  instrument colour swatch, name, and dB volume.
- **0007E-PLAN-033/034/035/036 — Tracker improvements.** *(2026-09-12 18:15)*
  X clears an active range; plain Space starts from the viewed pattern's row 0
  with Ctrl+Space resuming; "Order" renamed to "Pattern" with a current-pattern
  colour swatch; beat/bar guide lines now follow `highlight_a`/`highlight_b`.
- **0007E-PLAN-041/042/043/044/045 — Editor completeness.** *(2026-09-12 18:15)*
  Freeze-point waveform drag (A and B); shared Spectral ENVELOPE section;
  Spectral Result waveform with status and playheads; fusion lifecycle (render
  then re-trim, `setSpectralEnabled` on tab/pill switch, Project JSON import
  reconstructs renders preserving saved trim); tab↔pill engine sync.
- **0007E-BUG-002 — Tracker selection invisible.** *(2026-09-12 18:10)* Cell
  `className` had dropped the `tracker-cell` class, so `.tracker-cell.selected`
  / `.tracker-cell.in-range` never matched and EDIT MODE looked non-functional.
  Fixed by prefixing every tracker cell with `tracker-cell`; regression test
  `tests/e2e/edit.spec.ts` verifies select/navigate/edit.
- **0007E-BUG-003 — Sampler/Spectral close button dead.** *(2026-09-12 18:10)*
  The title-bar drag handler called `setPointerCapture` on pointerdown, which
  retargeted the subsequent `click` away from the ✕ button. Fixed by ignoring
  pointerdowns originating on a button and stopping propagation on the close
  button; regression test `tests/e2e/editor.spec.ts`.
- **0007E-BUG-004 — "Unmute" UI hang.** *(2026-09-12 18:25)* Mitigated: per-panel
  frame updates throttled, `songPositionAt` no longer allocates a `rowTimes`
  slice per call, and Spectral re-render is guarded to genuine Spectral
  param/source changes. Feel should be re-confirmed on the user's machine.
- **0007E-BUG-005 — Scroll-while-playing hang.** *(2026-09-12 18:25)* Mitigated
  by the same throttling: Mixer/Transport/PatternGrid at 20 fps,
  Piano/SourceSamples at 12 fps, Piano skips idle frames, SourceSamples
  de-dupes duration updates.
- **0007E-BUG-006 — Spectral Sample A source picker missing.** *(2026-09-12
  18:25)* Added a Sample A Source selector to the Spectral tab.
- **0007E-PLAN-026 — Persistent local editor state.** *(2026-09-12 18:25)*
  Mutes, instrument display names/colours and Ref Pitch persist to
  `localStorage` (`lantern-local-state-v1`) and restore over the project on load.
- **0007E-PLAN-031 — Transpose preview.** *(2026-09-12 18:25)* Table −/+ now
  clamp and preview via `onTransposeAdjust`.
- **0007E-PLAN-032 — CHIP-mode instrument preview.** *(2026-09-12 18:25)* In CHIP
  mode, instrument Preview finds a pattern occurrence (`findInstrumentSpot`)
  and auditions the stem slice.
- **0007E-PLAN-037 — Tracker note-trigger flashes.** *(2026-09-12 18:25)* Note
  starts briefly outline the cell in the accent colour.
- **0007E-PLAN-039 — Per-column interpolate menu.** *(2026-09-12 18:25)* One
  Interpolate entry per selected column.
- **0007E-PLAN-046 — Clear Samples resets editor state.** *(2026-09-12 18:25)*
  Also resets ADSR to the defaults.
- **0007E-PLAN-050 — Theme-following waveform colours.** *(2026-09-12 18:25)*
  Waveforms read the `--wave` theme variable.
- **0007E-PLAN-020 — Cross-channel range selection.** *(2026-09-12 18:40)*
  `tracker.ts` now exposes a global flat-column space (`flatColumns`,
  `globalColumnIndex`, `columnInRect`) and `selectionRect` spans channels.
  Shift-select, Ctrl+C/V + flood paste, X-range-clear, interpolate and Ctrl/Cmd+A
  all operate across channels; unit test asserts a CH0→CH1 rectangle and
  `inSelection` membership.
- **0007E-PLAN-038 — Frozen multi-row tracker headers.** *(2026-09-12 18:40)*
  Both header rows use explicit non-overlapping sticky offsets (0 / 28px) and
  the channel header `colSpan` now tracks the real effect-column count.
- **0007E-PLAN-040 — Draggable ADSR graph.** *(2026-09-12 18:40)* New
  `AdsrGraph` renders the envelope with three draggable handles (attack,
  decay/sustain, release), alongside the numeric sliders; E2E asserts the
  handles render.
- **0007E-PLAN-047 — Per-panel audio errors.** *(2026-09-12 18:40)* New
  `AudioError` component polls the backend and shows a red inline label under
  the player; no longer only the global banner.
- **0007E-PLAN-048 — Legacy Project JSON compatibility.** *(2026-09-12 18:40)*
  Import now falls back to the legacy `spectralFusion` object, aliases the old
  `spectral-blend` mode to `cross-synth`, and maps unknown modes to `off`
  instead of producing an invalid value; covered by a unit test.
- **0007E-PLAN-049 — Export filename parity.** *(2026-09-12 18:40)* Package
  Samples, sampler WAV and Project JSON filenames now match the Rust fallbacks
  (`project.songTitle`, else the Rust default string) exactly.
- **0007E-BUG-007 — `Z` key ("enter last value") was dead code.** *(2026-09-12
  19:15)* Found during a fresh parity pass: `PatternGrid.tsx`'s `z` handler
  read the current cell's value and discarded it, writing back an unchanged
  cell, even though the Keyboard Help modal advertised "enter last value /
  repeat." Fixed by porting Rust's per-column-type memory
  (`lantern-app/src/pattern.rs`'s `last_note`/`last_ins`/`last_vol`/`last_fx`,
  defaulting to C-4/0/15/`{0,0}`) into `src/core/tracker.ts` as
  `defaultLastValues`/`recordLastValue`/`applyLastValue`, hooked into
  `PatternGrid.tsx`'s shared `commit()` (used by both keybind edits and the
  right-click menu, matching Rust's `commit_edit`) so paste/interpolate still
  do not affect it. Covered by new cases in `tests/unit/tracker.test.ts`.
- **0007E-PLAN-052 — Sidebar width parity.** *(2026-09-12 19:15)* `.sidebar`
  was 320px; the Rust/original spec is 300px. One-line CSS fix.
- **0007E-PLAN-053 — Spectral Worker (was 0007E-IDEA-003).** *(2026-09-12
  19:15)* `prism_dsp` Spectral renders now run in a dedicated Web Worker
  (`src/wasm/prism.worker.ts` + `prismWorkerClient.ts`, request/response
  protocol in `prismWorkerProtocol.ts`) instead of blocking the main thread;
  `initPrismWasm` tries the Worker first and falls back to the previous
  main-thread WASM path if the Worker can't start. This finally gives real
  meaning to `AudioBackend`'s `fusionRendering`/`takeFusionCompleted`, which
  every implementation (Rust and TS) had hardcoded to `false`/no-op since
  rendering was always synchronous — `SamplerEngine.renderSpectral` now
  tracks per-instrument in-flight/completed state and discards stale results
  when a newer render supersedes an older one for the same instrument.
  `SamplerEditor`'s Spectral tab shows "Rendering…" and disables Render while
  a render is in flight. Verified end-to-end in the built app (Playwright):
  Render correctly shows "Rendering…" then "Rendered result is ready.", no
  console errors, no Worker-init fallback warning logged. New tests:
  `tests/unit/prism-worker-client.test.ts` (protocol, injected fake
  transport), `tests/unit/webSampler.test.ts` (stale-render discard).

- **0007E-BUG-008 — Tracker did not follow the playhead or the selected EDIT
  cell.** *(2026-09-12 19:20)* The React grid rendered a plain scroll container
  with no scroll-to logic, unlike Rust's `TableBuilder::scroll_to_row`. Added a
  `trackerRef` plus per-row refs and a post-render effect that keeps the playhead
  row or the selected EDIT cell **centred** in the scroll area (below the sticky
  header);
  regression test `tests/e2e/edit.spec.ts` asserts the selection stays inside the
  viewport after 45 ArrowDowns.

- **0007E-PLAN-054 — New Project button.** *(2026-09-12 19:45)* Toolbar button
  clears every pattern (one empty pattern per channel), resets to a single
  instrument with default sampler settings and a default name/colour, and sets
  Title/Author/Album to "New Song"/"Unknown Artist"/"New Album". Loaded Source
  Samples are retained from the folder; Project JSON overrides are refreshed.
- **0007E-PLAN-055 — Add / Delete instruments.** *(2026-09-12 19:45)*
  "+ Add Instrument" appends a blank Game Boy instrument with default settings
  and a golden-angle colour. Each row gains a Delete button (disabled on the
  last remaining instrument); deleting remaps every pattern INS cell via
  `remapInstrumentsAfterDelete` (references to it cleared, higher indices
  shifted down), rebuilds timelines/timing, re-syncs the backend sequence and
  settings, and closes or shifts any open editor. Covered by E2E
  `tests/e2e/project.spec.ts` and a unit test for the remap.

- **0007E-PLAN-056 — Follow Playhead stays on in EDIT MODE.** *(2026-09-12 20:20)*
  Default `true`; removed the automatic `setFollow(false)` from cell selection,
  vertical navigation and the pattern dropdown. The checkbox remains the only
  way to turn it off.
- **0007E-PLAN-057 — New Project resets volume levels.** *(2026-09-12 20:20)*
  Channel volumes -> 1.0, mutes -> off, master -> 1.0, pushed to the backend
  and written to the Project JSON fields.
- **0007E-PLAN-058 — Editable Base Tempo (BPM).** *(2026-09-12 20:20)* New
  EDIT-MODE field in the Timing card; editing back-solves
  `tickRate = bpm × highlightA × speed / 60` (clamped 1..1000), reusing the
  existing retime/sequence/`tickRateOverride` path.
- **0007E-PLAN-059 — ADSR attack fade-in.** *(2026-09-12 20:20)* The graph now
  draws a zero-volume baseline and start marker and enforces a minimum
  on-screen attack ramp so short attacks still read as a fade-in (display only;
  audio unchanged).
- **0007E-PLAN-060 — Ref Pitch removed from the header.** *(2026-09-12 20:20)*
  Kept in the instrument editor only; persistence unchanged.
- **0007E-PLAN-061 — Project JSON Load.** *(2026-09-12 20:20)* Hidden
  `<input type="file" accept=".json,application/json">`; reads the file into
  the JSON textarea. Works in all target browsers (no grey-out needed).
- **0007E-PLAN-062 — New Project confirmation.** *(2026-09-12 20:20)* Toolbar
  button opens a confirm modal before resetting.
- **0007E-PLAN-063 — Instrument delete behind a hamburger + confirm.**
  *(2026-09-12 20:20)* Per-row `☰` menu with "Delete instrument…" and a
  confirmation modal; disabled on the last instrument.
- **0007E-PLAN-064 — Song Comments always present + EDIT-MODE editable.**
  *(2026-09-12 20:20)* Panel always renders (collapsible); read-only with
  `song.meta.comment` fallback, `<textarea>` bound to `project.comments` in
  EDIT MODE.
- **0007E-PLAN-065 — Spurious "Cannot Audition" fixed.** *(2026-09-12 20:20)*
  `previewPattern` silently skips not-ready/not-assigned/empty-trim instead of
  setting a persistent `webError`; `onAudition` skips source-less and
  not-yet-rendered Spectral instruments; `AudioError` is now dismissible.
  New E2E `tests/e2e/features.spec.ts`.

- **0007E-PLAN-066 — Spectral Render button removed.** *(2026-09-12 20:35)*
  Spectral edits already re-render automatically (`onUpdateSetting` /
  `applyEngine`), so the manual Render button was redundant; the tab now shows
  the rendering/ready status only.
- **0007E-PLAN-067 — Project JSON Load auto-applies.** *(2026-09-12 20:35)*
  `applyProjectText` now takes the text directly; `loadProjectFile` reads the
  chosen file and applies it immediately, closing the modal. New E2E
  `features.spec.ts` verifies title + modal close.
- **0007E-PLAN-068 — Ctrl+Space plays from the selected EDIT cell.**
  *(2026-09-12 20:35)* `PatternGrid` reports the selection to App; in EDIT MODE
  Ctrl+Space seeks/plays from that cell's row instead of the current time.

- **0007E-PLAN-069 — Instrument volume dB entry fixed (0 dB max).**
  *(2026-09-12 21:10)* New shared `DbInput` keeps a local text buffer and
  commits on Enter/blur, so typing "-2" is no longer reformatted mid-edit; the
  instrument rack caps at 0 dB (1.0 linear).
- **0007E-PLAN-070 — Base Tempo field typing fixed.** *(2026-09-12 21:10)* The
  BPM box now buffers its text and commits on Enter/blur, so large values like
  150 are not clamped to 2.5 while typing.
- **0007E-PLAN-071 — ADSR ranges up to 5s.** *(2026-09-12 21:10)* Attack/decay/
  release sliders, graph drag clamps, `envelopeAt`, `buildVoice`, voice release
  and offline mixdown clamps all raised to 5 s; graph x-domain widened to 15.3 s.
- **0007E-PLAN-072 — ADSR graph zero-origin fixed.** *(2026-09-12 21:10)* The
  first vertex used raw `0` (top of the inverted y axis) instead of `py(0)`, so
  the attack looked flat at max volume. Now it starts at the bottom and ramps to
  max, with the zero baseline and minimum display ramp.
- **0007E-PLAN-073 — Error banner dismiss button.** *(2026-09-12 21:10)* The
  red error strip now has an explicit ✕ close button (e.g. after a bad JSON
  load); previously only the whole strip was clickable.
- **0007E-PLAN-074 — Sticky right-hand sidebar.** *(2026-09-12 21:10)* Matches
  the original (`position: sticky; top: 12px`) on wide screens, reverting to
  static in the narrow breakpoint.
- **0007E-PLAN-075 — Richer hover explainer text.** *(2026-09-12 21:10)* Ported
  the descriptive copy from the original HTML's `EXPLAIN` registry into
  `src/renderer/explainerContent.ts` and wired it to the tracker (rows, cells,
  channels, patterns), mixer, instruments (name/transpose/dB), source samples,
  cover art, piano, timing, chips, comments, toolbar and the Spectral tab.
- **0007E-PLAN-076 — Reduced sample popping.** *(2026-09-12 21:10)* One-shot
  voices get a 5 ms end fade (an envelope-accurate ramp to 0) and raw source
  previews fade out before stopping, removing end-of-sample clicks. Should be
  re-confirmed by ear; loop seams already had 5 ms fades.

- **0007E-PLAN-077 — Instrument row flash on trigger.** *(2026-09-12 22:20)*
  Instrument rows briefly flash in the accent colour when a note using them
  starts. *(item 1)*
- **0007E-PLAN-078 — Sidebar text +4px.** *(2026-09-12 22:20)* Right-hand panel
  text (Explainer, Comments, Timing, Chips, Mixer) increased ~4px. *(item 2)*
- **0007E-PLAN-079 — Sidebar/scrollbar gap.** *(2026-09-12 22:20)* Added
  `padding-right: 10px` so panel content doesn't touch the scrollbar. *(item 3)*
- **0007E-PLAN-080 — ADSR recreated from the original 0006 logic.**
  *(2026-09-12 22:20)* Canvas graph: fixed x-axis (stage max + nominal hold),
  4px padding, filled amber curve, three draggable points; removed the earlier
  min-width distortion. *(item 4)*
- **0007E-PLAN-081 — Explainer colour-coding + code blocks.** *(2026-09-12
  22:20)* Explainer bodies auto-highlight numbers/identifiers/keywords and
  render formula lines as code blocks, matching the original's `tk-*` style.
  *(item 5)*
- **0007E-PLAN-082 — All modals draggable.** *(2026-09-12 22:20)* New
  `DraggableModal` used by Project JSON, Sample Info, Clear Samples, New
  Project, Delete Instrument, Pattern Manager, Keyboard Help and Clear Patterns.
  *(item 6)*
- **0007E-PLAN-083 — Delete instrument asks remove vs re-assign.** *(2026-09-12
  22:20)* Prompt with "remove all notes" or "re-assign to <instrument>";
  `reassignInstrument` re-targets INS cells. Also clears loop caches via
  `replaceSettings`, fixing post-delete "Cannot play sample" errors. *(item 7)*
- **0007E-PLAN-084 — Explainer fixed height, no scroll.** *(2026-09-12 22:20)*
  Fixed panel height sized to the longest authored body (~246 chars) with a
  320-char cap enforced at render. *(item 8)*
- **0007E-PLAN-085 — Theme fade transition.** *(2026-09-12 22:20)*
  0.3s background/colour/border transitions across panels and controls. *(item 9)*
- **0007E-PLAN-086 — Mixer mute buttons.** *(2026-09-12 22:20)* Channel
  checkboxes replaced with CH{n} Mute buttons in the instrument style. *(item 10)*
- **0007E-PLAN-087 — Muted instrument rows semi-transparent.** *(2026-09-12
  22:20)* *(item 11)*
- **0007E-PLAN-088 — JSON load adopts its instrument count.** *(2026-09-12
  22:20)* Grows with blank instruments or shrinks (clearing out-of-range INS
  cells) to match the file instead of erroring. *(item 12)*
- **0007E-PLAN-089 — Package Samples removed from the toolbar.** *(2026-09-12
  22:20)* Still available in the Source Samples panel. *(item 13)*
- **0007E-PLAN-090 — Song panel engine label.** *(2026-09-12 22:20)* Shows
  `SAMPLER`, `SAMPLER / SPECTRALPRISM` (any instrument in Spectral), or
  `Game Boy` in CHIP mode. *(item 14)*
- **0007E-PLAN-091 — Title build number.** *(2026-09-12 22:20)* Heading is now
  `Lantern Music Player vYYYYMMDD`. *(item 15)*
- **0007E-PLAN-092 — Spectral modal title trimmed.** *(2026-09-12 22:20)* No
  date; just "SpectralPrism". *(item 16)*
- **0007E-PLAN-093 — Instrument previews at C5.** *(2026-09-12 22:20)* Preview
  voice rate/reference tone shifted from A4 to C5. *(item 17)*
- **0007E-PLAN-094 — CHIP MODE disabled style + tooltip.** *(2026-09-12 22:20)*
  When stems are missing the button is dimmed with a hover tooltip; New Project
  now marks stems unavailable. *(item 18)*
- **0007E-PLAN-095 — EDIT MODE blue accent.** *(2026-09-12 22:20)*
  `data-mode="edit"` switches the accent to `#3ea3e3`; CHIP/SAMPLER stay green.
  *(item 19)*
- **0007E-PLAN-096 — Click-drag cell selection.** *(2026-09-12 22:20)*
  Mouse-drag across cells extends the selection like Shift. *(item 20)*
- **0007E-PLAN-097 — Ctrl+X cut.** *(2026-09-12 22:20)* Copies the selection to
  the clipboard, then clears it. *(item 21)*
- **0007E-PLAN-098 — Visible/editable pattern number.** *(2026-09-12 22:20)*
  Pattern Manager shows a `PAT xx` badge per position and an editable number
  input (channel 0's pattern index, stable across reordering). *(item 22)*
- **0007E-PLAN-099 — Lighter Project JSON.** *(2026-09-12 22:20)* Pattern
  snapshots now store only non-empty cells as sparse `[row, cell]` pairs (still
  reads the dense legacy form); empty metadata/mutes/source samples omitted.
  *(item 23)*
- **0007E-PLAN-100 — Toolbar reordered/trimmed.** *(2026-09-12 22:20)* Removed
  Load Song Folder and Package Samples; order is Save .FUR, Save .MIDI,
  Save .WAV, Project JSON, New Project. *(items 24, 25, 26)*

- **0007E-PLAN-101 — Project JSON modal opens instantly.** *(2026-09-12 22:35)*
  The modal now paints immediately; the (sometimes large) serialization runs on
  the next tick and fills the textarea after. *(item 1)*
- **0007E-PLAN-102 — Instrument Pan + Random Pan Width.** *(2026-09-12 22:35)*
  `SamplerSettings` gains a `pan` centre (-1..1), and `panRandomRange` is now
  the random width around that centre; wired through the live Web Audio voice,
  the offline mixdown, Project JSON, and the instrument rows / Sampler editor.
  *(item 2)*

- **0007E-PLAN-103 — Q/A/W/S edit every highlighted cell.** *(2026-09-12 22:55)*
  In EDIT MODE, Q/A/W/S now adjust all cells in the active selection range
  (Shift or drag), not just the focus cell. *(item 1)*
- **0007E-PLAN-104 — Instrument rows fit without horizontal scroll.**
  *(2026-09-12 22:55)* Rows wrap instead of scrolling horizontally; the name
  input is narrower. *(item 2)*
- **0007E-PLAN-105 — Web build.** *(2026-09-12 22:55)* New browser platform
  layer (`src/renderer/platform.ts`) so the same renderer runs in Electron or
  the browser: bundled assets are fetched from `./assets/`, `.fur` zlib is
  inflated with `DecompressionStream`, saves become Blob downloads and file
  picks use `<input type="file">`. `npm run build:web` emits a static
  `dist/web/` (assets copied, minus the large CHIP mix WAV) and
  `npm run test:web` loads it in a real browser. *(item 3)*

- **0007E-BUG-009 — Clicking a tracker cell turned off Follow Playhead.**
  *(2026-09-12 23:05)* The drag-select `beginDrag` (fired on mouse-down for
  every cell click) still called `setFollow(false)`, left over from before the
  "follow stays on while editing" change. Removed it, so Follow Playhead is now
  only ever toggled by its checkbox. Regression assertion added to
  `tests/e2e/edit.spec.ts` (click a cell, then expect the checkbox checked).

- **0007E-PLAN-106 — Project files use the `.lampjson` extension.**
  *(2026-09-12 23:20)* Downloaded project files are now `*.lampjson`; the
  Project JSON **Load** picker accepts `.lampjson` (plus legacy `.json` for
  backward compatibility), and the Electron save dialog filters by the
  suggested extension (a "Lantern Project" filter for `.lampjson`). Folder
  assembly accepts `lmp-default-proj.lampjson` or `.json`. The bundled default
  asset keeps its `.json` name since it is an internal asset / folder
  convention, not a user-saved project.

- **0007E-PLAN-107 — Strict `.lampjson` only.** *(2026-09-12 23:35)* The Load
  picker now accepts only `.lampjson` and rejects anything else with an inline
  error (even if the OS picker allows "all files"); the `.json` save filter was
  dropped; folder assembly requires `lmp-default-proj.lampjson`; and the bundled
  default asset was renamed to `lmp-default-proj.lampjson`. Regression test:
  loading a `.json` file shows ".lampjson" in the error banner and keeps the
  modal open.

- **0007E-PLAN-108 — New cover art: laboratory plant vat.** *(2026-09-12 23:55)*
  Same 240×240 / 32×32 / 4×4 Bayer pipeline, new scene: a glass vat with a
  translucent green nutrient liquid and a vibrant green stem that grows in and
  sways, jiggling on note triggers, with instrument-coloured auras and rim
  flashes, set against dull-blue humming monitor screens. Export PNG unchanged.

- **0007E-PLAN-109 — Build number stamped at build time.** *(2026-09-13 00:10)*
  The title's `vYYYYMMDD` is now injected by Vite (`__BUILD_DATE__`, local
  date) at build time rather than computed when the app runs, so each build
  regenerates it. Applies to both the Electron and web builds; the smoke test
  asserts the `v\d{8}` format.

- **0007E-PLAN-110 — Project-only songs (no `.fur` / CHIP MODE).**
  *(2026-09-13 00:35)* The bundled song now ships as just
  `lmp-default-proj.lampjson` + `SourceSamples/`. The loader no longer requires
  a `.fur`: when absent, `buildSongModelFromProject` rebuilds the `SongModel`
  from the project's pattern snapshot (channels/orders/rows), timing overrides,
  and instrument count, so the app loads and plays in SAMPLER mode with CHIP
  MODE disabled. `LoadedSong.raw`/`furBytes` are now optional and "Save .FUR"
  is disabled for project-only songs. Both Electron and web loaders updated;
  E2E fixtures switched from the old 10-instrument song to "Aquavats" (1
  instrument).

- **0007E-PLAN-111 — Folder hot-loading removed.** *(2026-09-13 07:45)*
  Deleted `src/main/folder.ts` and all `loadSongFolder`/`assembleSongFolder`/
  `SongFolderFile` plumbing across main, preload, shared IPC types, the
  renderer platform layer and App. A Furnace `.fur` + matching files are now
  bundled with the program rather than chosen at runtime. Supersedes the
  folder-assembly parts of `0007E-PLAN-015`/`0007E-PLAN-107`; the removed
  "Load Song Folder" button is gone for good.
- **0007E-PLAN-112 — 3D vat in the cover art.** *(2026-09-13 07:45)* The glass
  tube now reads as a cylinder: rounded top/bottom caps (row insets), a
  left-of-centre specular band with darkened rim edges, depth-shaded liquid
  with a lit meniscus, a front-glass highlight and right-edge sheen, a back rim
  seen through the liquid, and a base plate. Same 32×32/Bayer pipeline.

- **0007E-BUG-010 — Q/A/W/S retuned non-note columns in an all-columns
  selection.** *(2026-09-13 08:20)* When a selection spans multiple columns
  (e.g. Ctrl+A "all columns"), semitone/octave keys now only adjust note
  columns; a single selected column still adjusts its own value (INS index,
  VOL, FX value).
- **0007E-BUG-011 — Beat/bar row markers invisible.** *(2026-09-13 08:20)* The
  markers were cell backgrounds (hidden by instrument tint) and used a
  dark-only colour. Now theme-aware `--beat-bg`/`--bar-bg` applied to the row
  so the translucent instrument tint composites over them.
- **0007E-BUG-012 — Only 3 of 6 Source Samples loaded.** *(2026-09-13 08:20)*
  Both loaders hardcoded slots 0–2; now read all six
  (`SourceSamples/0..5.ogg`), matching the new bundled folder.
- **0007E-BUG-013 — Z always inserted C4.** *(2026-09-13 08:20)* The Q/A/W/S
  selection refactor bypassed `recordLastValue`, so the "last entered value"
  never updated. Those edits now record the focus value (and the adjusted note
  on an all-columns selection), so Z repeats the last note/value as designed.
  New E2E assertions: Z-repeat and a `6 / 6` Source Samples count.
  *Follow-up (2026-09-13):* the Q/A/W/S branch recorded the last value by
  re-applying the adjustment after `mutate` had already applied it, storing a
  value one step off (Z then inserted one note above/below). It now records the
  resulting cell directly; the Z E2E asserts D-4 then Q -> Z yields D#4, not
  E-4.

- **0007E-PLAN-113 — Preview when editing a cell's note or volume.**
  *(2026-09-13 08:30)* Changing a cell with Q/A/W/S now auditions it the same
  way clicking the cell does — fired for note (semitone/octave) and volume
  columns, on the focused channel/row, after the edit is applied.

- **0007E-BUG-014 — Notes after a note-off played silently.** *(2026-09-13 08:40)*
  `buildInstrumentTimeline` cleared the channel's held instrument on a Note Off
  (copied from the original JS/Rust), so any note after an OFF with a blank
  instrument column (e.g. the staccato runs in Aquavats) resolved to no
  instrument and never sounded. Furnace keeps the instrument across note-offs,
  so the clearing was removed; only a new instrument value changes it. A note
  after an OFF now plays with the held instrument, while the held *note*
  timeline is still cleared by the OFF. Unit-tested; deliberate divergence from
  the reference implementation, justified by real Furnace semantics and the
  composer's song.
  *Follow-up (2026-09-13):* the tracker's instrument colour-coding now keys off
  a **held note** (`noteTimeline`) rather than the persistent instrument, so a
  note-off clears the row tint (and mute dimming) as it should, while playback
  still resolves the held instrument for later notes.

- **0007E-PLAN-114 — Instrument names persist in the project JSON.**
  *(2026-09-13 09:00)* Added `instrumentNames` to the Project schema; export
  writes the current names and load (bundled or via Load) applies them, so
  edited instrument names round-trip. Unit-tested.
- **0007E-PLAN-115 — Hold-to-drag cell selection (250 ms).** *(2026-09-13 09:00)*
  Range selection now only begins after the pointer has been held on a cell for
  250 ms; a quick click (which may drift across a cell or two) stays a
  single-cell selection. E2E covers both the quick-click and hold-drag cases.

- **0007E-PLAN-116 — Numbered channel headings outside CHIP MODE.**
  *(2026-09-13 09:10)* The tracker channel headers show just `CH0…CH3` in
  SAMPLER/EDIT mode; the Game Boy roles (`PULSE 1`, `PULSE 2`, `WAVE`, `NOISE`)
  only appear in CHIP MODE. `/tmp` smoke E2E asserts this.

- **0007E-BUG-015 — Spectral instruments ended early (e.g. Aquavats Instrument 7).**
  *(2026-09-13 09:15)* A Spectral instrument's saved trim (a ~0.44 s slice
  of Sample A) was left in place after the fused loop rendered, so it cut the
  rendered multi-second loop off early. `SamplerEngine.renderSpectral` now
  resets `startSec`/`endSec` to the full fused result and sets `looping = true`
  on completion (Spectral instruments loop by default). `setSpectralEnabled`
  saves/restores the sampler's loop flag so turning Spectral off restores it.
  Unit-tested.
- **0007E-PLAN-117 — Per-instrument Vibrato.** *(2026-09-13 09:15)* Added
  `vibratoSpeed` (Hz) and `vibratoDepth` (semitones, default 0) to
  `SamplerSettings`. Live voices run a sine LFO into `AudioBufferSourceNode.detune`
  (layering on pitch ramps); the offline mixdown applies the same modulation;
  controls appear in the Sampler editor and compactly in the instrument rows;
  Project JSON round-trips both fields.
  *Follow-up (2026-09-13):* the instrument-row numeric fields (and the Timing
  card values / voice cap) now use a buffered `NumberInput` that commits on
  Enter/blur, so values can be backspaced and retyped (a small E2E clears and
  retypes the Depth field).

- **0007E-BUG-016 — Instrument names weren't exported/applied in Project JSON.**
  *(2026-09-13 09:35)* The `live` export object omitted `instrumentNames` and
  `applyProjectText` never applied them, so names never round-tripped. Both
  paths added; E2E covers save and load.
- **0007E-PLAN-118 — Numeric-input sizing, editable ADSR, centred editor.**
  *(2026-09-13 09:35)* Buffered `NumberInput`s now carry a compact
  `.number-input` style (they had reverted to default size when switched from
  `type="number"`); the Sampler/Spectral envelope values are editable directly
  (each slider now shows a `NumberInput`); and the editor window opens high and
  horizontally centred.
- **0007E-PLAN-119 — Master FX (Delay + Reverb).** *(2026-09-13 09:35)* New
  master-output effect chain in `WebAudioBackend` (sum bus feeding delay with
  feedback + low-pass tone, and a convolution reverb from a generated
  noise-decay impulse). A **Master FX** button in the Mixer opens a draggable
  modal with per-effect enable toggles (off by default), Delay
  time/feedback/tone/mix, Reverb decay/mix, and a **PS1 Echo** preset. Settings
  persist in Project JSON (`masterFx`). Unit + E2E tested.
  *Follow-up (2026-09-13):* the export `live` object (and its callback deps)
  omitted `masterFx`, so toggles never saved; fixed, and New Project now resets
  Master FX to defaults. E2E verifies both save and load.

- **0007E-BUG-017 — Dragging pan during playback hung the UI.** *(2026-09-13 10:00)*
  Continuous control drags (pan/volume/vibrato) update app state on every
  pointer tick, which re-rendered the whole tree — the tracker rebuilds
  thousands of per-cell handlers each pass, so a drag became a re-render storm.
  Memoized the heavy panels (`PatternGrid`, `Piano`, `CoverArt`, `Mixer`,
  `SourceSamples`) and stabilized their props (stable muted-flags list, stable
  callbacks) so a pan drag only re-renders the instrument row. E2E stresses 60
  pan steps during playback and asserts the UI still responds.

- **0007E-BUG-018 — WAV export ignored Master FX.** *(2026-09-13 10:20)* `saveWav`
  rendered the sampler mix and encoded it straight to WAV, bypassing the delay
  and reverb entirely. Extracted the live chain into a shared
  `createMasterFxGraph` (`src/audio/masterFxGraph.ts`) used by both playback and
  a new offline renderer (`src/audio/offline.ts`), so exports now sound like the
  transport. Unit + E2E tested.
- **0007E-PLAN-120 — WAV export options modal.** *(2026-09-13 10:20, revised 12:10)*
  **Save .WAV** now opens a draggable modal (`WavExportModal`) with Number of
  Loops (default 0 = single pass), Fade In ms, Fade Out ms (both default 0) and
  Peak Normalize (default on). `finalizeExport` joins `loops + 1` full passes
  with no gap, then appends the fade-out as an EXTRA tail that keeps looping the
  source while ramping to zero over the fade time; the file ends exactly when it
  reaches silence (e.g. Loops=1 + 8000ms = two full passes + an 8s fading 3rd
  loop). The arranged signal is run through the offline Master FX and the
  fade/normalise envelope is applied after it (see 0007E-BUG-019). Unit tests
  cover the loop joins, the appended fade tail, long-fade wrapping, fade-in and
  normalisation.

- **0007E-BUG-019 — Silence between exported loops (per-loop Master FX tail).**
  *(2026-09-13 12:25)* `runWavExport` applied the offline Master FX to a single
  pass and then looped the result, so every repeat carried its own reverb/delay
  decay tail — audible as a multi-second "gap" before the next loop (confirmed
  in a real export: silent regions at 42.4–44.9s and 87.3–89.8s). Export now
  arranges `loops + 1` passes + fade tail first (`arrangeExport`), runs the FX
  chain once over that continuous signal, trims back to the arrangement length,
  then applies the fade/normalise envelope (`applyExportEnvelope`). Re-exporting
  the same project produced no silent regions and the expected length
  (2×40.678s + 8.000s). Unit tests cover the arrange/envelope split.

- **0007E-PLAN-121 — Linkified, wrapping Comments/Licenses text.** *(2026-09-13 12:40)*
  Song Comments and the Licenses card now render through `LinkifiedText`:
  `https://` words become anchors that open in the system browser (desktop,
  via a new `shell:open-external` IPC + preload method and `setWindowOpenHandler`)
  or a new tab (web), and a `.linkified` style (`overflow-wrap: anywhere`) wraps
  long unbroken words/URLs instead of overflowing the panel. E2E checks a
  Licenses link and the wrapping rule.
- **0007E-PLAN-122 — WAV metadata tags, cover art, dated filename.** *(2026-09-13 12:40)*
  `wavPcm16` accepts `WavTags` and appends a `LIST`/`INFO` block (INAM/IART/IPRD)
  and an `id3 ` chunk with an ID3v2.3 tag (TIT2/TPE1/TALB + `APIC` front cover).
  Exports embed the animated cover rendered at 1600×1600 at export time (via a
  `CoverArtHandle` ref) and are named `YYMMDD- Title.wav`. Verified on a real
  export: 1600×1600 PNG APIC and correct tags. Unit test covers the chunks.
- **0007E-PLAN-123 — "Exporting WAV…" progress bar.** *(2026-09-13 12:40)*
  The export modal swaps to a percentage progress bar while rendering;
  `applyMasterFxOffline` reports progress using `OfflineAudioContext.suspend`
  checkpoints, and the flow yields once so the bar paints before the synchronous
  sampler mixdown.

- **0007E-PLAN-124 — Remove the CHIPS panel.** *(2026-09-13 12:50)* The unused
  right-hand Chips panel was removed (`ChipsCard` + its hover explainer and the
  now-dead `chipsExplain` helper); the sidebar now runs Cover Art → Explainer →
  Comments → Timing → Mixer → Licenses.

- **0007E-BUG-020 — Web WAV export crashed without `OfflineAudioContext.suspend`.**
  *(2026-09-13 13:40)* On browsers whose `OfflineAudioContext` lacks `suspend`/
  `resume` (Safari/older Firefox), the progress checkpoints threw
  `TypeError: s.suspend is not a function`. Now feature-detected; when missing,
  progress falls back to a timed ramp and the render proceeds. Web regression
  test removes `suspend` and asserts a successful export.
- **0007E-BUG-021 — Mixer sliders/volumes overflowed the sidebar panel.**
  *(2026-09-13 13:40)* The row's fixed widths (66px "dB" label + 64px meter +
  58px input + button + 10px gaps) exceeded the 300px sidebar content width, so
  the range and meter spilled off the right edge. The `dB` label is now auto
  width, the meter is 48px, and `.mixer-row` wraps with tighter gaps. E2E
  measures every row's children against its right edge.
- **0007E-BUG-022 — E2E expected the old bundled song title.**
  *(2026-09-13 13:40)* The bundled default project became the finished track
  ("Completed first track in this"), so hardcoded "Aquavats" waits and the
  Master-FX-disabled / single-instrument assumptions failed. Waits now check for
  the generic "instruments" status, and the Master FX/New Project tests assert
  the modal and the post-reset count instead.

---

_Add new cards at the bottom of their bucket; move them rather than copy._
