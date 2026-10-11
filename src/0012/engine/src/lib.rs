//! # shrine0012 — the artifact
//!
//! A procedural, deterministic isometric shrine: 16x16 grid squares of stacked
//! 2.5D tile layers, centred in a 24x24 isometric grid, surrounded by code-grown
//! plant life that moves with the weather, and paired with a music engine built
//! from the same simulation clock.
//!
//! Everything here is **standard library only** (`HC001`). No crates, no
//! generated JS bindings, no build script. The browser loads the compiled
//! `.wasm` through the plain C ABI in [`abi`] with the standard WebAssembly API.
//!
//! ## Output contract (`HC003`)
//!
//! * The logical render buffer is exactly [`LOGICAL_W`] x [`LOGICAL_H`] (300x300).
//! * The on-screen canvas is exactly [`CANVAS_W`] x [`CANVAS_H`] (1200x1200).
//! * The only upscale is nearest-neighbour, by [`ART_PX`] (4) — so one logical
//!   pixel becomes a crisp 4x4 block of screen pixels.
//!
//! ## Composition (`HC005`)
//!
//! The grid is [`GRID_W`] x [`GRID_H`] (24x24) cells across the canvas. The
//! shrine occupies [`SHRINE_W`] x [`SHRINE_H`] (16x16) cells, centred.
//!
//! > Note: 1200 screen pixels / 24 cells = 50 px per cell, and 50 / 4 = 12.5
//! > logical pixels. The grid therefore maps to logical space at a half-pixel
//! > boundary; the rasteriser works in fixed point and rounds fill edges to
//! > whole logical pixels. See `iso.rs` when it lands (FEAT-4).

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod audio;
pub mod budget;
pub mod framebuffer;
pub mod grid;
pub mod iso;
pub mod math;
pub mod name;
pub mod overlay;
pub mod palette;
pub mod plants;
pub mod player;
pub mod raster;
pub mod recipe;
pub mod rng;
pub mod scene;
pub mod shrine;
pub mod sim;
pub mod wasm;
pub mod weather;

// The music engine is organised under `audio`, but its modules are re-exported
// at the crate root so they share the same paths the ported DSP expects
// (`crate::filter`, `crate::texture`, …) and the artifact reads uniformly.
pub use crate::audio::*;

pub const SAMPLE_RATE: u32 = 48_000;

/// Logical render buffer: one byte-scaled art pixel here is `ART_PX` on screen.
pub const LOGICAL_W: u32 = 300;
pub const LOGICAL_H: u32 = 300;

/// The canvas the web page presents. The logical buffer is nearest-upscaled.
pub const CANVAS_W: u32 = 1200;
pub const CANVAS_H: u32 = 1200;

/// Screen pixels per logical (art) pixel.
pub const ART_PX: u32 = CANVAS_W / LOGICAL_W; // 4

/// The isometric grid, in cells.
pub const GRID_W: u32 = 24;
pub const GRID_H: u32 = 24;

/// The shrine footprint, in cells.
pub const SHRINE_W: u32 = 16;
pub const SHRINE_H: u32 = 16;

/// ABI version. Bump only on a breaking change to the exported functions.
pub const ABI_VERSION: u32 = 1;

/// Plain C ABI exports. Kept in one place so the contract is easy to audit.
pub mod abi {
    use super::ABI_VERSION;

    /// Version of the C ABI, so the page can refuse an incompatible module.
    #[no_mangle]
    pub extern "C" fn shrine_abi_version() -> u32 {
        ABI_VERSION
    }

    /// Packed logical size as `(w << 16) | h`, so JS can assert the contract.
    #[no_mangle]
    pub extern "C" fn shrine_logical_size() -> u32 {
        (super::LOGICAL_W << 16) | super::LOGICAL_H
    }
}
