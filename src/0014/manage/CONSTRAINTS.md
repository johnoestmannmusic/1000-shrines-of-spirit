# CONSTRAINTS.md

<!--
  0012's laws. Read by the constraints-validate extension before planning,
  after implementing, and on every write. See NAMING.md for the naming policy
  and the 0011 -> 0012 rename table.
-->

## Goals
G001: Ship 0012 as a single, deterministic, self-contained software-art artifact: a procedural isometric shrine (1200x1200) whose visuals and music are one coupled system, emitted as human-readable Rust and compiled to WASM.

## Hard Constraints
HC001: The engine and the emitted artifact must have zero external Rust dependencies (standard library only).
Why: The artifact must still build and run, unchanged, in twenty years with no network or crate registry.
Run: ./scripts/check-zero-deps.sh

HC002: No third-party product, brand, preset or game name anywhere in source, comments, documentation, UI, recipe codes, or the emitted artifact.
Why: The work must be entirely original and free of trademark or copyright entanglement; describe the synthesis, never the gear.
Files: engine/**, src/**, web/**, README.md, docs/**
Must not contain: /rain\s*world/i
Must not contain: /fantasia/i
Must not contain: /\broland\b/i
Must not contain: /\bkorg\b/i
Must not contain: /\byamaha\b/i
Must not contain: /\bmoog\b/i
Must not contain: /\boberheim\b/i
Must not contain: /\bnovation\b/i
Must not contain: /\bclavia\b/i
Must not contain: /\bnord\b/i
Must not contain: /wavestation/i
Must not contain: /mellotron/i
Must not contain: /fairlight/i
Must not contain: /ensoniq/i
Must not contain: /\bjd-?800\b/i
Must not contain: /\brhodes\b/i
Must not contain: /\bdx-?7\b/i
Must not contain: /\bd-?50\b/i
Must not contain: /\btb-?303\b/i
Must not contain: /\btr-?808\b/i
Must not contain: /\btr-?909\b/i
Must not contain: /prophet[\s_-]?5/i
Must not contain: /juno[\s_-]?106/i
Must not contain: /jupiter[\s_-]?8/i
Must not contain: /sequential\s+circuits/i
Must not contain: /access\s+virus/i
Must not contain: /native\s+instruments/i
Must not contain: /omnisphere/i
Must not contain: /spectrasonics/i
Must not contain: /\bkontakt\b/i
Must not contain: /\barturia\b/i
Must not contain: /\bsylenth\b/i
Must not contain: /\babsynth\b/i
Must not contain: /\breaktor\b/i
Run: ./scripts/naming-audit.sh

HC003: The logical render buffer is exactly 300x300, the on-screen canvas is exactly 1200x1200, and the only upscale is nearest-neighbour.
Why: Crisp 4x4 art pixels and a fixed, predictable output contract.
Run: cargo test -q -p shrine-0014-engine --test contract

HC004: Identical seed and settings produce bit-identical audio samples and visual frames, forever.
Why: The piece must be reproducible and reverse-engineerable; golden-hash tests guard this.

HC005: The shrine occupies exactly 16x16 grid squares, centred in a 24x24 isometric grid.
Why: The brief's composition.
Run: cargo test -q -p shrine-0014-engine --test contract

HC006: No wasm-bindgen, no generated JS glue; the artifact exposes a plain C ABI and loads with the standard WebAssembly API only.
Why: Longevity and zero toolchain lock-in.
Files: engine/**, src/**, web/**
Must not contain: wasm-bindgen

HC007: All art is generated from text/code; no binary art assets are shipped.
Why: The artifact must be fully inspectable as source.
Must not exist: engine/**/*.png, engine/**/*.jpg, engine/**/*.jpeg, engine/**/*.gif, engine/**/*.bmp, engine/**/*.webp

HC008: The engine may reference only the Rust standard library and its own crate; no third-party crate path may appear in its source.
Why: A dependency that changes its behaviour would silently change the piece. The artifact must stay reproducible for decades.
Files: engine/src/**
Must not contain: /^\s*(pub\s+)?use\s+(?!crate\b|std\b|core\b|alloc\b|super\b|self\b)/
Must not contain: /^\s*extern\s+crate\s+(?!std\b|core\b|alloc\b)/

HC009: The shipped WASM must have zero imports and load with the plain WebAssembly API.
Why: No host glue, no WASI, no toolchain version that can alter the experience.
Run: ./scripts/check-wasm.sh

HC010: The shipped web runtime must be self-contained: no external scripts, styles, fonts or CDNs.
Why: The page must work offline, unchanged, without a third party's URL deciding whether it renders. A plain user-navigable hyperlink loads nothing, so it is allowed; anything the browser fetches on its own is not.
Files: web/**
Except: web/**/*.wasm
Must not contain: /<(?:script|link|img|iframe|audio|video|source|embed|object|track)\b[^>]*(?:src|href)\s*=\s*["']?https?:/i
Must not contain: /url\(\s*["']?https?:/i
Must not contain: /@import\s+(?:url\()?["']?https?:/i
Must not contain: /^\s*(?:import|export)\b[^;]*from\s*["']https?:/im
Must not contain: /unpkg|cdnjs|jsdelivr|googleapis/i

## Soft Constraints
SC001: Reuse 0011's DSP building blocks and tuning, silently, without carrying over its names.
SC002: Keep modules small and documented; the isometric and grid framework stays shrine-agnostic and reusable.

## Stretch Goals
SG001: A live TUI preview and audition, then a visual editor that authors the same data model.
