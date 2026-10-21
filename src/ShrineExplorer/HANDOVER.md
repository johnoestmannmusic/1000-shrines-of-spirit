# ShrineExplorer — handover (2026-10-21)

Copied from `src/0016`. The approved plan is in `~/.claude/plans/okay-please-read-and-zesty-lighthouse.md`, sections 0–5. Read `manage/CONSTRAINTS.md` first:
- zero deps;
- zero WASM imports;
- a self-contained page;
- no third-party names;
- never commit (John commits).

## Feature
On the web page:
- Hovering near any of the shrine's 4 sides shows an arrow that turns with the shrine.
- Clicking it travels to the neighbouring shrine on a deterministic map. Every spot `(x, y)` is a fixed, slightly mutated shrine around the home `(0, 0)`.
- Coordinates typed into the setup reproduce a spot.

The mutations cover the Randomize groups except Master FX and Camera; every map shrine spins at −2°/s.

## Status: steps 0–5 done (2026-10-22), all green
At hand-off, `cargo test --workspace --release`, `./build-web.sh && node web/verify.mjs`, and `scripts/check-zero-deps.sh` all pass.

### Done
- **0. Copy and rename.** The crates are `shrine-explorer`, `shrine-explorer-engine` and lib `shrine_explorer`. The config is `~/.config/shrine-explorer/explorer.cfg`, copied from 0016's. The album is "ShrineExplorer".
- **1. Randomize moved into the engine** (`engine/src/randomize.rs`).
  - Master FX is removed, and `Look` became `CodeOverlay` (no camera), leaving 15 groups.
  - The setup's Randomize screen ticks all groups by default (`RandomForm::new` in `src/tui.rs`).
- **2. The map: `engine/src/explore.rs`.**
  - `at(home, x, y)`, `resolve()`, `changed_groups()`, `MAP_REGION_SCALE`, `MAP_CAMERA_SPEED = -2`.
  - Each group changes per square region of its own size (`region_size`), so a step changes about 2 groups plus the architecture. The home's region keeps the home's values.
  - Away from home, `start_s = 0` (so a step never fast-forwards) and the master FX are always the home's.
  - `Settings::map: (i32, i32)`, saved in recipe `VERSION 19` (two words, biased by 2^31).
  - `Sim::new` and `Sim::new_visual` call `explore::resolve`, so every consumer plays the explored shrine.
  - Tests: `engine/tests/explore.rs`, plus `abi.rs` updated (v18 → map at home; length 170).
  - The parity fixture gained "map 3,-2" and "map -11,7" (`FUTURE_GOLDEN`, `web/parity-futures.json`); `verify.mjs` replays them.
- **3. Exports**, in both `engine/src/wasm.rs` and the emitted ABI in `src/emit.rs`:
  - `shrine_new_at(x, y, mode)`;
  - `shrine_map_x()` / `shrine_map_y()`;
  - `shrine_side_point(player, side, clock)`, returning `[x, y, dx, dy]` in logical px. Backed by `Scene::side_point` and `Player::side_point`.
- **Bug fixed on the way, in this folder and in `src/0016`:**
  - The problem: a glitch layer with ratchet 1 crashed `pattern.rs::random_event`, because it asked `rng.int(2, 1)`. Randomize can roll ratchet 1.
  - The fix: it never ratchets when `max_ratchet < 2`. The draw order is unchanged, so all goldens are unchanged.
  - Regression test: `engine/tests/glitch_ratchet.rs`, in both folders.

### Also done (2026-10-22)
- **4. Page:** arrows, travel, the `⌖` readout, `#at=` links, and the worklet's `{travel}` message. Learn and CyberPal follow the new shrine. Test hooks are on `window.shrineExplore` under `?orbs=test`.
- **5. Setup:** "Map position" and "Make this shrine home", seed lines, the info card, `--at`.
- README "Exploring the map" and KANBAN FEAT-55.
- `web/shrine.wasm` is the real artifact for the stored settings.
- Gotcha: `scripts/check-wasm.sh` overwrites `web/shrine.wasm` with the dev engine build, so check an artifact's imports directly instead.

### UX pass (2026-10-22)
- No camera turn.
- Twice the variation.
- The music changes at biome borders.
- Arrows further out.
- The island-to-island transition (`transDraw`, timing constants `T_*`).
- The minimap (`drawMinimap`, `shrine_map_glance`).
- The arrival banner (`arrive()`).
- The picture's clock carries across arrival via `clockShift`.

### Second pass (2026-10-22)
- **Map:** small biomes; the lattice for biome, mode and scale; no architecture mutation; map shrines start 24 s in.
- **Recipe card:** moved to `engine/src/card.rs`; the page refreshes it on every move.
- **Audio architecture changed:** `web/shrine.worklet.js` now only plays streams. Each shrine's music runs in an inline Blob worker (`AUDIO_WORKER` in `index.html`, ~0.6 s ahead, acked by the worklet every 2048 frames). Travel crossfades streams; solo and Learn's live state go to the current worker (`musicSend`).

### Open ideas
- The code overlay's look on the page comes from the home's baked exports, not each map shrine's own (mutated) overlay settings.
- Orbs and arrows are drawn over everything, even when behind tall columns.

## Verification ideas
Use headless Chrome over the DevTools protocol: `google-chrome --headless=new --remote-debugging-port=…`, driven by Node's built-in WebSocket. `?orbs=test` exposes test hooks.
- Hover each side and screenshot.
- Click an arrow and check the coordinates and hash change and the audio resumes.
- Go there and back, and check the frame hash is identical.
- Load `#at=3,-2` and check it matches travelling there.

Kill test servers with `pgrep -f "[h]ttp.server PORT"`, never `pkill -f`: that pattern matches its own shell and kills it.
