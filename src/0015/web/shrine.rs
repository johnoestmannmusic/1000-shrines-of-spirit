// ============================================================================
// ShrineGen — a procedural isometric shrine.
//
// GENERATED ARTIFACT — emitted by `ShrineGen --emit`. Do not edit by hand;
// change the setup and re-emit. This file is the whole piece: the isometric
// framework, the shrine, the plants, the weather, the music engine, and the
// recipe below. It is standard-library only and has no external dependencies.
//
// Build it with nothing but rustc:
//
//   rustc --edition 2021 --target wasm32-unknown-unknown -O \
//         --crate-type cdylib shrine.rs -o shrine.wasm
//
// The shrine it draws and plays:
//   seed 65088 · Verdant · Dusk
//   Phrygian, 3 chord(s), half-time, 156 BPM, root D#, kit Digital crush
//   drone arc 0s up 0s, tone 6000
//
// Contents: crate docs and constants, then engine modules (iso, grid, render,
// audio/…, sim), then the baked recipe and the C ABI. Read top to bottom.
// ============================================================================

//! # shrine0015 — the artifact
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

// ============================================================================
// Engine modules
// ============================================================================

// ---- src/arch.rs ----
pub mod arch {
//! The architecture transform: rotate the shrine about the x, y and z axes and
//! offset it in all three.
//!
//! [`ArchTransform::IDENTITY`] is the default, so the shipped shrine is exactly
//! the centred 16x16 model that `HC005` describes: the transform is an opt-in
//! *presentation* deviation, and the model itself never moves.
//!
//! Rotations are stored as whole degrees so the setup menu and the seed file
//! stay readable, and are converted with [`crate::math::sin_turns`] /
//! [`crate::math::cos_turns`] — never the platform's `f64::sin`, whose results
//! differ between libm versions and would break `HC004`.
//!
//! Offsets are whole grid cells (x, y) and stack levels (z), applied in *world*
//! space: after the rotation, along the grid's own axes.

use crate::math::{cos_turns, sin_turns};
use crate::{GRID_H, GRID_W};

/// The grid's centre, which the camera orbits.
fn centre_x() -> f64 {
    (GRID_W as f64 - 1.0) / 2.0
}

fn centre_y() -> f64 {
    (GRID_H as f64 - 1.0) / 2.0
}

/// 3x3 matrix product, row-major.
fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// Rotate and offset the shrine. Identity by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchTransform {
    /// Degrees about each axis, -180..180.
    pub rot_x: i16,
    pub rot_y: i16,
    pub rot_z: i16,
    /// Whole cells (x, y) and stack levels (z).
    pub off_x: i16,
    pub off_y: i16,
    pub off_z: i16,
}

impl ArchTransform {
    pub const MIN_ROT: i16 = -180;
    pub const MAX_ROT: i16 = 180;
    pub const MIN_OFF: i16 = -24;
    pub const MAX_OFF: i16 = 24;

    /// The normal, centred, unrotated shrine.
    pub const IDENTITY: ArchTransform =
        ArchTransform { rot_x: 0, rot_y: 0, rot_z: 0, off_x: 0, off_y: 0, off_z: 0 };

    /// True when nothing is rotated — an offset alone still uses the exact
    /// axis-aligned render path, because a whole-cell shift is just a moved
    /// grid coordinate.
    pub fn is_unrotated(&self) -> bool {
        self.rot_x == 0 && self.rot_y == 0 && self.rot_z == 0
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// The three rotations in turns, for the deterministic sine and cosine.
    pub fn turns(&self) -> (f64, f64, f64) {
        let t = |deg: i16| (deg.clamp(Self::MIN_ROT, Self::MAX_ROT) as f64) / 360.0;
        (t(self.rot_x), t(self.rot_y), t(self.rot_z))
    }

    /// The rotation as a row-major 3x3 matrix, applied x, then y, then z.
    ///
    /// Built only from `+ - * /` (inside `sin_turns`/`cos_turns`), so it is
    /// bit-identical on every platform.
    pub fn rotation(&self) -> [[f64; 3]; 3] {
        let (tx, ty, tz) = self.turns();
        let (sx, cx) = (sin_turns(tx), cos_turns(tx));
        let (sy, cy) = (sin_turns(ty), cos_turns(ty));
        let (sz, cz) = (sin_turns(tz), cos_turns(tz));
        // Rx * Ry * Rz, expanded so the order is explicit.
        [
            [cz * cy, -sz * cy, sy],
            [sz * cx + cz * sy * sx, cz * cx - sz * sy * sx, -cy * sx],
            [sz * sx - cz * sy * cx, cz * sx + sz * sy * cx, cy * cx],
        ]
    }

    /// The full world transform: the architecture transform, then a camera
    /// orbit of `orbit_turns` about the grid's vertical axis.
    ///
    /// The orbit spins the whole scene around the shrine, so the *world*
    /// vertical stays vertical on screen — plants still stand up and rain still
    /// falls — while the ground turns underneath. Returns the 3x3 rotation and
    /// the world offset, ready for `Iso::project_rotated`.
    pub fn world(&self, orbit_turns: f64) -> ([[f64; 3]; 3], (f64, f64, f64)) {
        let rot = self.rotation();
        let off = (self.off_x as f64, self.off_y as f64, self.off_z as f64);
        if orbit_turns == 0.0 {
            return (rot, off);
        }
        let (sin, cos) = (sin_turns(orbit_turns), cos_turns(orbit_turns));
        let orbit = [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]];
        let m = mul3(&orbit, &rot);
        // world = O * (R * p + off - centre) + centre, so the scene turns about
        // the shrine rather than swinging around the grid's corner.
        let (cx, cy) = (centre_x(), centre_y());
        let (px, py, pz) = (off.0 - cx, off.1 - cy, off.2);
        (m, (cos * px - sin * py + cx, sin * px + cos * py + cy, pz))
    }

    /// Pack the rotations into one recipe word: three 10-bit values biased by
    /// +180.
    pub fn rot_word(self) -> u32 {
        let a = (self.rot_x.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        let b = (self.rot_y.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        let c = (self.rot_z.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        (a & 0x3ff) | ((b & 0x3ff) << 10) | ((c & 0x3ff) << 20)
    }

    /// Pack the offsets into one recipe word: three 6-bit values biased by +32.
    pub fn off_word(self) -> u32 {
        let a = (self.off_x.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        let b = (self.off_y.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        let c = (self.off_z.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        (a & 0x3f) | ((b & 0x3f) << 6) | ((c & 0x3f) << 12)
    }

    /// Read a rotation pair written by [`rot_word`], tolerating corrupt values.
    pub fn rot_from_word(word: u32) -> (i16, i16, i16) {
        let part = |shift: u32| ((word >> shift) & 0x3ff).min(360) as i16 - 180;
        (part(0), part(10), part(20))
    }

    /// Read an offset triple written by [`off_word`].
    pub fn off_from_word(word: u32) -> (i16, i16, i16) {
        let part = |shift: u32| ((word >> shift) & 0x3f).min(63) as i16 - 32;
        (part(0), part(6), part(12))
    }

    /// One line for the recipe card.
    pub fn summary(self) -> String {
        if self.is_identity() {
            return "centred, upright".to_string();
        }
        let mut parts = Vec::new();
        if !self.is_unrotated() {
            parts.push(format!("{}°, {}°, {}°", self.rot_x, self.rot_y, self.rot_z));
        }
        if self.off_x != 0 || self.off_y != 0 || self.off_z != 0 {
            parts.push(format!("offset {:+}, {:+}, {:+}", self.off_x, self.off_y, self.off_z));
        }
        parts.join(" · ")
    }
}

impl Default for ArchTransform {
    fn default() -> Self {
        ArchTransform::IDENTITY
    }
}



}

// ---- src/budget.rs ----
pub mod budget {
//! Frame-time budget: pick an effects quality from how fast frames render.
//!
//! The logical buffer and canvas never change size (`HC003`). Under load the
//! scene instead draws *fewer effects* — every other plant, less rain — so the
//! output contract stays fixed while the cost falls.

use crate::scene::Quality;

/// Observes frame durations and steps `Quality` up or down with hysteresis.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    quality: Quality,
    slow: u32,
    fast: u32,
}

impl Budget {
    pub fn new() -> Self {
        Budget { quality: Quality::High, slow: 0, fast: 0 }
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    /// Feed one frame's duration in milliseconds; returns the quality to use
    /// for the next frame. Slow frames accumulate toward a downgrade, fast ones
    /// toward an upgrade, so brief spikes never thrash the setting.
    pub fn observe(&mut self, frame_ms: f64) -> Quality {
        if frame_ms > 20.0 {
            self.slow += 1;
            self.fast = 0;
        } else if frame_ms < 12.0 {
            self.fast += 1;
            self.slow = 0;
        } else {
            self.slow = self.slow.saturating_sub(1);
            self.fast = self.fast.saturating_sub(1);
        }

        if self.slow >= 8 {
            self.quality = match self.quality {
                Quality::High => Quality::Medium,
                Quality::Medium => Quality::Low,
                Quality::Low => Quality::Low,
            };
            self.slow = 0;
        } else if self.fast >= 240 {
            self.quality = match self.quality {
                Quality::Low => Quality::Medium,
                Quality::Medium => Quality::High,
                Quality::High => Quality::High,
            };
            self.fast = 0;
        }
        self.quality
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

}

// ---- src/framebuffer.rs ----
pub mod framebuffer {
//! The logical framebuffer: exactly 300x300 RGBA8, row-major, top-left origin.
//!
//! The buffer stores bytes in the order `R, G, B, A`, which is directly the
//! layout the browser's `ImageData` expects, so the WASM export can hand back a
//! `*const u8` and the page can wrap it without conversion or endianness
//! assumptions. The canvas nearest-upscales this buffer to 1200x1200 (`HC003`);
//! nothing here ever draws at screen scale.

use crate::LOGICAL_H;
use crate::LOGICAL_W;

/// One colour, in the exact byte order the browser wants.
pub type Rgba = [u8; 4];

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    [r, g, b, a]
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}

/// Scale a colour by `num / den`, keeping its alpha. Used for the lit top /
/// shaded side faces of a column. Integer-only, so it is deterministic.
pub fn shade(c: Rgba, num: u32, den: u32) -> Rgba {
    let f = |v: u8| ((v as u32 * num) / den).min(255) as u8;
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// Mix `a` toward `b` by `num / den` (the weight of `b`). Integer-only.
pub fn blend(a: Rgba, b: Rgba, num: u32, den: u32) -> Rgba {
    let den = den.max(1);
    let num = num.min(den);
    let f = |x: u8, y: u8| ((x as u32 * (den - num) + y as u32 * num) / den) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}

/// A 300x300 RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    /// A new buffer, cleared to transparent black.
    pub fn new(width: u32, height: u32) -> Self {
        Framebuffer {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    /// The canonical logical buffer (`HC003`).
    pub fn logical() -> Self {
        Framebuffer::new(LOGICAL_W, LOGICAL_H)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn clear(&mut self, colour: Rgba) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&colour);
        }
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    #[inline]
    fn offset(&self, x: i32, y: i32) -> usize {
        ((y as u32 * self.width + x as u32) * 4) as usize
    }

    /// Write one pixel. Out-of-bounds writes are ignored (clipping).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, colour: Rgba) {
        if self.in_bounds(x, y) {
            let i = self.offset(x, y);
            self.pixels[i..i + 4].copy_from_slice(&colour);
        }
    }

    /// Read one pixel; transparent black outside the buffer.
    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if !self.in_bounds(x, y) {
            return [0, 0, 0, 0];
        }
        let i = self.offset(x, y);
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    /// Composite `colour` over one pixel with coverage `alpha` (0-255),
    /// respecting the pixel's own alpha: over a transparent pixel (a sky the
    /// page paints itself) the result is `colour` at `alpha`, so the page's
    /// compositing finishes the job. Integer-only, so it is deterministic.
    #[inline]
    pub fn blend_over(&mut self, x: i32, y: i32, colour: Rgba, alpha: u32) {
        let a = alpha.min(255);
        if a == 0 || !self.in_bounds(x, y) {
            return;
        }
        let i = self.offset(x, y);
        let d = [self.pixels[i] as u32, self.pixels[i + 1] as u32, self.pixels[i + 2] as u32, self.pixels[i + 3] as u32];
        // Destination weight, in 255ths of the result.
        let dw = d[3] * (255 - a) / 255;
        let out_a = a + dw;
        if out_a == 0 {
            return;
        }
        for k in 0..3 {
            self.pixels[i + k] = ((colour[k] as u32 * a + d[k] * dw) / out_a) as u8;
        }
        self.pixels[i + 3] = out_a.min(255) as u8;
    }

    /// Fill a horizontal run `x0..=x1` on row `y`, clipped to the buffer.
    pub fn fill_span(&mut self, y: i32, x0: i32, x1: i32, colour: Rgba) {
        if y < 0 || y as u32 >= self.height {
            return;
        }
        let x0 = x0.max(0);
        let x1 = x1.min(self.width as i32 - 1);
        if x0 > x1 {
            return;
        }
        let start = self.offset(x0, y);
        let end = self.offset(x1, y) + 4;
        for chunk in self.pixels[start..end].chunks_exact_mut(4) {
            chunk.copy_from_slice(&colour);
        }
    }

    /// Fill a vertical run `y0..=y1` in column `x`, clipped to the buffer.
    pub fn fill_vertical(&mut self, x: i32, y0: i32, y1: i32, colour: Rgba) {
        if x < 0 || x as u32 >= self.width {
            return;
        }
        let y0 = y0.max(0);
        let y1 = y1.min(self.height as i32 - 1);
        if y0 > y1 {
            return;
        }
        for y in y0..=y1 {
            let i = self.offset(x, y);
            self.pixels[i..i + 4].copy_from_slice(&colour);
        }
    }

    /// Fill an axis-aligned rectangle `x0..=x1`, `y0..=y1`, clipped.
    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, colour: Rgba) {
        for y in y0..=y1 {
            self.fill_span(y, x0, x1, colour);
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.pixels
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Pointer for the WASM ABI. Valid until the buffer is mutated or moved.
    pub fn as_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    /// FNV-1a over the raw bytes: the golden-hash primitive for frame tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.pixels {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

}

// ---- src/future.rs ----
pub mod future {
//! The materials of the four built futures.
//!
//! The same blocks as the ruin — walls, pillars, roofs, the altar — are
//! re-made in each future's own materials, and the ground follows the
//! future's layout (`layout.rs`):
//!
//! - **Near Future**: a bright city block. Tiled facades in pastel colours
//!   with window bands, roller-shuttered shops at street level, tall
//!   vertical signs, a vending machine for the altar; asphalt roads with
//!   centre lines and zebra crossings, sidewalks with curbs and yellow
//!   tactile paving.
//! - **Research Lab**: brushed steel panels, glass curtain walls, blue-green
//!   neon strips, a UI screen for the altar; floor panels with light lines.
//! - **Ecofuture**: white titanium panels with seams, vents and status
//!   lights, living walls and green roofs, solar tiles, a glowing bio
//!   reactor; lawns with mowing stripes and curving white paths.
//! - **Greenhouse**: steel mullions and glass with plants behind, white
//!   columns, a pool for the altar; terracotta tiles, gravel, planting beds.
//!
//! Like the ruin's materials every texel is a pure hash of the seed, the cell,
//! the face band and the texel, so nothing is stored and nothing drifts.

use crate::framebuffer::{blend, Rgba};
use crate::layout::{Ground, Plan, Side, ORIGIN};
use crate::material::{Albedo, Block, Exposure};
use crate::palette::{hsl, Palette};
use crate::plants::Biome;
use crate::raster::{Face, Surface};
use crate::rng::hash3;
use crate::shrine::kind;

/// Texels per face, as for the ruin.
const T: f64 = 6.0;

fn scale(c: Rgba, f: f64) -> Rgba {
    let k = |v: u8| (v as f64 * f).clamp(0.0, 255.0) as u8;
    [k(c[0]), k(c[1]), k(c[2]), c[3]]
}

#[inline]
fn bits(h: u64, shift: u32, n: u32) -> u64 {
    (h >> shift) & ((1 << n) - 1)
}

#[inline]
fn u8f(h: u64, shift: u32) -> f64 {
    bits(h, shift, 8) as f64 / 256.0
}

/// What one future is built from.
#[derive(Clone, Copy, Debug)]
pub struct Stock {
    pub biome: Biome,
    plan: Plan,
    /// Facade colours (the city picks one a building).
    facades: [Rgba; 4],
    /// The main wall material: steel, titanium, tile.
    panel: Rgba,
    /// Trim, mullions, frames.
    trim: Rgba,
    /// A dark base course.
    plinth: Rgba,
    glass: Rgba,
    /// Lights: neon, signage, status lamps, lit windows.
    neon: Rgba,
    warm: Rgba,
    /// The altar's own light (the colour it casts, too).
    pub glow: Rgba,
    screen: Rgba,
    asphalt: Rgba,
    paving: Rgba,
    plaza: Rgba,
    paint: Rgba,
    tactile: Rgba,
    lawn: [Rgba; 2],
    path: Rgba,
    solar: Rgba,
    soil: Rgba,
    tile: Rgba,
    gravel: Rgba,
    floor: Rgba,
    foliage: [Rgba; 3],
    water: Rgba,
}

impl Stock {
    pub fn of(seed: u64, p: &Palette, biome: Biome) -> Self {
        let foliage = [p.foliage_dark, p.foliage, p.foliage_light];
        let base = Stock {
            biome,
            plan: Plan::new(seed, biome),
            facades: [p.stone_light; 4],
            panel: p.stone_light,
            trim: p.stone,
            plinth: p.stone_dark,
            glass: blend(p.sky_bottom, [120, 170, 200, 255], 60, 100),
            neon: p.accent,
            warm: [255, 214, 150, 255],
            glow: p.accent,
            screen: [18, 34, 40, 255],
            asphalt: [74, 76, 82, 255],
            paving: [196, 192, 184, 255],
            plaza: [222, 216, 204, 255],
            paint: [246, 246, 240, 255],
            tactile: [236, 196, 52, 255],
            lawn: [blend(p.foliage, [92, 156, 70, 255], 60, 100), blend(p.foliage_light, [118, 178, 86, 255], 60, 100)],
            path: [234, 232, 224, 255],
            solar: [38, 56, 104, 255],
            soil: [74, 54, 40, 255],
            tile: [184, 104, 72, 255],
            gravel: [190, 186, 176, 255],
            floor: [176, 184, 190, 255],
            foliage,
            water: p.water,
        };
        match biome {
            Biome::NearFuture => Stock {
                // Pastel facades: cream, pale blue, salmon, mint.
                facades: [
                    blend(p.stone_light, [242, 232, 208, 255], 75, 100),
                    blend(p.stone_light, [196, 218, 236, 255], 75, 100),
                    blend(p.stone_light, [240, 198, 182, 255], 75, 100),
                    blend(p.stone_light, [198, 230, 210, 255], 75, 100),
                ],
                trim: [210, 210, 206, 255],
                plinth: [96, 98, 104, 255],
                glass: [110, 160, 196, 255],
                neon: p.accent,
                glow: [250, 246, 226, 255],
                ..base
            },
            Biome::ResearchLab => Stock {
                panel: blend(p.stone, [168, 178, 186, 255], 75, 100),
                trim: [112, 122, 132, 255],
                plinth: [48, 54, 62, 255],
                glass: [96, 150, 168, 255],
                neon: p.accent,
                glow: p.accent,
                floor: blend(p.ground, [150, 160, 168, 255], 70, 100),
                ..base
            },
            Biome::Ecofuture => Stock {
                panel: [236, 238, 240, 255],
                trim: [182, 188, 194, 255],
                plinth: [204, 206, 208, 255],
                glass: [150, 196, 210, 255],
                neon: p.accent,
                glow: blend(p.accent, [255, 200, 110, 255], 50, 100),
                ..base
            },
            Biome::Greenhouse => Stock {
                panel: [226, 230, 228, 255],
                trim: [210, 214, 216, 255],
                plinth: [164, 158, 150, 255],
                glass: blend(p.sky_bottom, [200, 230, 214, 255], 70, 100),
                neon: p.accent,
                glow: [255, 236, 170, 255],
                ..base
            },
            _ => base,
        }
    }

    /// The ground at a plateau point.
    pub fn ground(&self, x: f64, y: f64) -> Ground {
        self.plan.at(x, y)
    }
}

/// The future's palette: the seed's palette, with the roles a future needs
/// set to its own colours (still nudged a little by the seed).
pub fn palette(seed: u64, p: Palette, biome: Biome) -> Palette {
    let mut rng = crate::rng::Rng::stream(seed, 0x4655_5455); // "FUTU"
    let j = rng.range(-0.02, 0.02);
    let pick = rng.below(3);
    match biome {
        Biome::NearFuture => {
            // Bright, clean low-poly-era daylight: a cyan sky fading to near white;
            // signage in orange, magenta or cyan.
            let sign = [0.04, 0.93, 0.52][pick];
            Palette {
                sky_top: hsl(0.56 + j, 0.62, 0.62),
                sky_bottom: hsl(0.54 + j, 0.42, 0.88),
                fog: hsl(0.55 + j, 0.25, 0.86),
                ground: hsl(0.6, 0.05, 0.32),
                ground_dark: hsl(0.6, 0.05, 0.2),
                stone: hsl(0.08 + j, 0.08, 0.66),
                stone_light: hsl(0.08 + j, 0.1, 0.84),
                stone_dark: hsl(0.08 + j, 0.08, 0.42),
                foliage_dark: hsl(0.3 + j, 0.45, 0.24),
                foliage: hsl(0.29 + j, 0.5, 0.38),
                foliage_light: hsl(0.27 + j, 0.55, 0.52),
                water: hsl(0.55, 0.4, 0.6),
                accent: hsl(sign + j, 0.7, 0.62),
            }
        }
        Biome::ResearchLab => Palette {
            sky_top: hsl(0.53 + j, 0.22, 0.12),
            sky_bottom: hsl(0.5 + j, 0.26, 0.26),
            fog: hsl(0.5 + j, 0.2, 0.5),
            ground: hsl(0.56, 0.08, 0.5),
            ground_dark: hsl(0.56, 0.08, 0.3),
            stone: hsl(0.57 + j, 0.08, 0.6),
            stone_light: hsl(0.57 + j, 0.07, 0.76),
            stone_dark: hsl(0.57 + j, 0.1, 0.3),
            foliage_dark: hsl(0.36 + j, 0.5, 0.2),
            foliage: hsl(0.36 + j, 0.55, 0.34),
            foliage_light: hsl(0.4 + j, 0.6, 0.5),
            water: hsl(0.5, 0.5, 0.45),
            // Blue-green neon.
            accent: hsl([0.45, 0.48, 0.42][pick] + j, 0.9, 0.56),
        },
        Biome::Ecofuture => Palette {
            sky_top: hsl(0.57 + j, 0.55, 0.56),
            sky_bottom: hsl(0.52 + j, 0.4, 0.86),
            fog: hsl(0.52 + j, 0.25, 0.84),
            ground: hsl(0.27 + j, 0.45, 0.36),
            ground_dark: hsl(0.27 + j, 0.45, 0.24),
            stone: hsl(0.58, 0.05, 0.74),
            stone_light: hsl(0.58, 0.05, 0.9),
            stone_dark: hsl(0.58, 0.06, 0.5),
            foliage_dark: hsl(0.31 + j, 0.5, 0.22),
            foliage: hsl(0.3 + j, 0.55, 0.36),
            foliage_light: hsl(0.27 + j, 0.6, 0.52),
            water: hsl(0.52, 0.5, 0.55),
            // Solar amber.
            accent: hsl(0.11 + j, 0.88, 0.6),
        },
        Biome::Greenhouse => Palette {
            sky_top: hsl(0.13 + j, 0.45, 0.72),
            sky_bottom: hsl(0.12 + j, 0.55, 0.9),
            fog: hsl(0.2 + j, 0.25, 0.86),
            ground: hsl(0.05, 0.4, 0.48),
            ground_dark: hsl(0.05, 0.4, 0.3),
            stone: hsl(0.6, 0.04, 0.72),
            stone_light: hsl(0.6, 0.04, 0.88),
            stone_dark: hsl(0.6, 0.05, 0.46),
            foliage_dark: hsl(0.32 + j, 0.62, 0.2),
            foliage: hsl(0.3 + j, 0.66, 0.34),
            foliage_light: hsl(0.27 + j, 0.7, 0.5),
            water: hsl(0.5, 0.45, 0.55),
            // Flowers: pink, coral or violet.
            accent: hsl([0.93, 0.02, 0.8][pick] + j, 0.7, 0.66),
        },
        _ => p,
    }
}

/// One pixel's texel and its hashes (as the ruin works them out).
struct Texel {
    side: bool,
    tu: u64,
    tz: u64,
    band: i64,
    /// Band hashes (per face band) and the texel hash.
    hb: u64,
    hb2: u64,
    ht: u64,
    /// A hash of the column alone: a building's colour, a sign's glyph.
    hc: u64,
    speckle: f64,
    below_top: f64,
}

fn texel(seed: u64, b: &Block, s: &Surface) -> Texel {
    let cell = ((b.gx as u64 & 0xffff) << 16) | (b.gy as u64 & 0xffff);
    let key = seed ^ 0x4655_5452 ^ cell.wrapping_mul(0x9E37_79B9);
    let side = !matches!(s.face, Face::Top | Face::Bottom);
    let (u, v) = match s.face {
        Face::PosX | Face::NegX => (s.y - b.gy as f64 + 0.5, 0.0),
        Face::PosY | Face::NegY => (s.x - b.gx as f64 + 0.5, 0.0),
        _ => (s.x - b.gx as f64 + 0.5, s.y - b.gy as f64 + 0.5),
    };
    let tu = ((u * T) as i64).clamp(0, T as i64 - 1) as u64;
    let band = crate::light::floor(s.z - 1e-6) as i64;
    let tz = if side { (((s.z - band as f64) * T) as i64).clamp(0, T as i64 - 1) as u64 } else { ((v * T) as i64).clamp(0, T as i64 - 1) as u64 };
    let band_key = ((s.face as u64) << 16) | (band as u64 & 0xffff);
    let ht = hash3(key, 3, band_key << 8 | tu << 4 | tz);
    Texel {
        side,
        tu,
        tz,
        band,
        hb: hash3(key, 1, band_key),
        hb2: hash3(key, 2, band_key),
        ht,
        hc: hash3(key, 7, 0),
        speckle: 0.96 + 0.08 * u8f(ht, 0),
        below_top: b.top - s.z,
    }
}

/// A pixel of a future.
pub fn albedo(st: &Stock, seed: u64, b: &Block, s: &Surface, ex: Exposure) -> Albedo {
    let t = texel(seed, b, s);
    let mut emissive = 0.0;
    let mut spec = 0.0;
    let mut c = if b.kind == kind::EMPTY {
        ground(st, seed, b, s, &t, &mut emissive, &mut spec)
    } else {
        match st.biome {
            Biome::NearFuture => city(st, b, &t, &mut emissive, &mut spec),
            Biome::ResearchLab => lab(st, b, &t, &mut emissive, &mut spec),
            Biome::Ecofuture => eco(st, b, &t, &mut emissive, &mut spec),
            _ => greenhouse(st, b, &t, &mut emissive, &mut spec),
        }
    };

    // Weather settling, as on the ruin (a roofed future is never exposed).
    let snow_here = if t.side { t.below_top < 1.0 / T || b.kind == kind::EMPTY } else { true };
    if ex.cover > 0.0 && snow_here && u8f(t.ht, 48) < ex.cover * 1.15 {
        c = blend([238, 242, 252, 255], [210, 222, 246, 255], (u8f(t.ht, 56) * 60.0) as u32, 100);
        emissive *= 0.3;
    } else if ex.rain > 0.0 {
        c = scale(c, 1.0 - 0.18 * ex.rain);
        if !t.side && u8f(t.ht, 56) < 0.08 * ex.rain {
            spec = 1.0;
        }
    }
    Albedo { colour: c, emissive, spec }
}

/// The plateau's surface, by the layout.
fn ground(st: &Stock, seed: u64, b: &Block, s: &Surface, t: &Texel, emissive: &mut f64, spec: &mut f64) -> Rgba {
    let (tu, tz) = (t.tu, t.tz);
    let (gx, gy) = (b.gx, b.gy);
    let sp = t.speckle;
    // The texel on the edge facing `side`.
    let on_edge = |side: Side, depth: u64| match side {
        Side::West => tu < depth,
        Side::East => tu >= 6 - depth,
        Side::North => tz < depth,
        Side::South => tz >= 6 - depth,
    };
    match st.ground(s.x, s.y) {
        Ground::Wild => scale(st.soil, sp),
        Ground::Road { along_y, lane, crossing } => {
            let base = scale(st.asphalt, sp * (0.97 + 0.06 * u8f(t.ht, 16)));
            // Along the road (`a`) and across it (`x`), in texels.
            let (a, x) = if along_y { (gy as u64 * 6 + tz, tu) } else { (gx as u64 * 6 + tu, tz) };
            let (g_across, outer_low) = if along_y { (gx * 6 + tu as i32, gx < ORIGIN.0) } else { (gy * 6 + tz as i32, gy < ORIGIN.1) };
            if crossing {
                // Zebra bars along the traffic, side by side across it.
                if g_across / 2 % 2 == 0 && (1..=4).contains(&(a % 6)) {
                    return st.paint;
                }
                return base;
            }
            // The centre line: dashes on the lane cell's outer edge.
            let edge = if outer_low { x == 0 } else { x == 5 };
            if lane && edge && a % 8 < 4 {
                return st.paint;
            }
            // A manhole now and then.
            if crate::layout::cell_hash(seed, gx, gy) % 11 == 0 {
                let (du, dv) = (tu as f64 - 2.5, tz as f64 - 2.5);
                let r = (du * du + dv * dv).sqrt();
                if (1.6..2.4).contains(&r) {
                    return scale(st.asphalt, 0.7);
                }
                if r < 1.6 {
                    return scale(st.asphalt, if (tu + tz) % 2 == 0 { 0.82 } else { 0.92 });
                }
            }
            base
        }
        Ground::Junction => {
            // Stop lines where the roads come in.
            scale(st.asphalt, sp * (0.97 + 0.06 * u8f(t.ht, 16)))
        }
        Ground::Sidewalk { curb } => {
            if let Some(side) = curb {
                if on_edge(side, 1) {
                    return scale(st.paint, 0.92 * sp);
                }
                // Yellow tactile paving, a row of studs in from the curb.
                if on_edge(side, 3) && !on_edge(side, 2) {
                    return scale(st.tactile, if (tu + tz) % 2 == 0 { 1.05 } else { 0.86 });
                }
            }
            let joint = tu % 3 == 0 || tz % 3 == 0;
            let slab = bits(hash3(seed, (gx * 2 + tu as i32 / 3) as u64, (gy * 2 + tz as i32 / 3) as u64), 0, 8) as f64 / 256.0;
            if joint {
                scale(st.paving, 0.84)
            } else {
                scale(st.paving, (0.94 + 0.1 * slab) * sp)
            }
        }
        Ground::Plaza => {
            let joint = tu == 0 || tz == 0;
            scale(st.plaza, if joint { 0.86 } else { sp })
        }
        Ground::Lawn => {
            // Mowing stripes, two cells wide, and the odd tuft.
            let stripe = ((gx + gy) / 2) % 2 == 0;
            let g = st.lawn[stripe as usize];
            if u8f(t.ht, 8) < 0.12 {
                st.foliage[2]
            } else {
                scale(g, sp)
            }
        }
        Ground::Path => scale(st.path, 0.94 + 0.08 * u8f(t.ht, 8)),
        Ground::Solar => {
            if tu == 0 || tz == 0 {
                // The aluminium frame.
                [206, 210, 214, 255]
            } else if tu == 3 || tz == 3 {
                scale(st.solar, 1.35)
            } else {
                if (tu + tz) % 4 == 0 {
                    *spec = 0.5;
                }
                scale(st.solar, sp)
            }
        }
        Ground::Floor { strip } => {
            let ring = crate::layout::ring(gx, gy);
            if strip {
                // The light line runs along the cell, across its middle.
                let along_y = if ring == 1 { gx < ORIGIN.0 || gx >= ORIGIN.0 + crate::SHRINE_W as i32 } else { gx % 6 == 0 };
                let mid = if along_y { tu } else { tz };
                if mid == 2 || mid == 3 {
                    *emissive = 0.9;
                    return st.neon;
                }
            }
            if tu == 0 || tz == 0 {
                scale(st.floor, 0.78)
            } else if (tu == 1 || tu == 5) && (tz == 1 || tz == 5) {
                // Rivets.
                scale(st.floor, 1.12)
            } else {
                scale(st.floor, 0.97 + 0.04 * u8f(t.ht, 8))
            }
        }
        Ground::Bed => {
            // Lab planters have a steel lip; elsewhere, open soil.
            if st.biome == Biome::ResearchLab && (tu == 0 || tz == 0 || tu == 5 || tz == 5) {
                return st.trim;
            }
            if u8f(t.ht, 8) < 0.18 {
                st.foliage[0]
            } else {
                scale(st.soil, sp * (0.9 + 0.2 * u8f(t.ht, 16)))
            }
        }
        Ground::Tile => {
            let grout = tu % 3 == 0 || tz % 3 == 0;
            if grout {
                return [214, 206, 192, 255];
            }
            let tile = hash3(seed ^ 0x54, (gx * 2 + tu as i32 / 3) as u64, (gy * 2 + tz as i32 / 3) as u64);
            if u8f(t.ht, 24) < 0.04 {
                *spec = 0.6;
            }
            scale(st.tile, 0.88 + 0.2 * u8f(tile, 0))
        }
        Ground::Gravel => {
            let r = u8f(t.ht, 16);
            scale(st.gravel, if r < 0.25 { 0.8 } else if r > 0.85 { 1.12 } else { sp })
        }
    }
}

/// An inset glyph: sixteen bits on a 4x4 grid, never empty.
fn glyph(h: u64, tu: u64, tz: u64) -> bool {
    if !(1..=4).contains(&tu) || !(1..=4).contains(&tz) {
        return false;
    }
    let pattern = (bits(h, 8, 16) as u16) | 0x0420;
    pattern >> ((tu - 1) + (tz - 1) * 4) & 1 == 1
}

/// The city: tiled facades with windows, shop shutters, signs, a vending machine.
fn city(st: &Stock, b: &Block, t: &Texel, emissive: &mut f64, spec: &mut f64) -> Rgba {
    let (tu, tz) = (t.tu, t.tz);
    let facade = st.facades[bits(t.hc, 0, 2) as usize];
    if b.kind == kind::ALTAR {
        // A vending machine: white body, a lit display, coloured buttons.
        if !t.side {
            return scale(st.trim, t.speckle);
        }
        if t.below_top < 1.0 && (1..=4).contains(&tu) && (1..=3).contains(&tz) {
            *emissive = 0.85;
            return if tz == 3 { blend(st.neon, st.warm, 40, 100) } else { [[230, 72, 64, 255], [64, 132, 230, 255], [250, 210, 80, 255], [90, 200, 120, 255]][(tu + tz) as usize % 4] };
        }
        return if tz == 0 { scale(st.neon, 0.8) } else { [242, 242, 238, 255] };
    }
    if !t.side {
        return match b.kind {
            kind::FLOOR | kind::STONE => {
                let joint = tu % 3 == 0 || tz % 3 == 0;
                scale(st.plaza, if joint { 0.86 } else { t.speckle })
            }
            kind::RUBBLE => scale(st.gravel, t.speckle * if u8f(t.ht, 24) < 0.3 { 0.8 } else { 1.0 }),
            _ => {
                // Rooftops: pale concrete, a parapet rim, now and then an
                // air-conditioning unit.
                let rim = tu == 0 || tz == 0 || tu == 5 || tz == 5;
                if bits(t.hc, 8, 3) == 0 && (2..=3).contains(&tu) && (2..=4).contains(&tz) {
                    return if tz == 3 { [80, 84, 90, 255] } else { [196, 200, 204, 255] };
                }
                scale(st.paving, if rim { 1.06 } else { 0.94 * t.speckle })
            }
        };
    }
    // Sign colours: the scene's accent now and then, mostly softer lights.
    let sign_colour = |h: u64| [st.neon, [120, 214, 236, 255], [246, 246, 240, 255], [244, 186, 206, 255], [255, 226, 170, 255]][(h % 5) as usize];
    if b.kind == kind::PILLAR {
        // About one pillar in three carries a tall, narrow vertical sign; the
        // rest are slim towers with a strip of windows.
        if bits(t.hc, 12, 2) == 0 {
            if (2..=3).contains(&tu) {
                *emissive = 0.85;
                let c = sign_colour(bits(t.hc, 16, 8));
                return if (tz + tu) % 3 == 0 { st.paint } else { c };
            }
            return st.trim;
        }
        if (2..=3).contains(&tu) && (2..=4).contains(&tz) {
            if u8f(t.hb2, (tz * 8) as u32) < 0.25 {
                *emissive = 0.5;
                return st.warm;
            }
            return scale(st.glass, 0.95 + 0.1 * u8f(t.ht, 8));
        }
        return scale(facade, if tu == 0 || tu == 5 { 0.92 } else { t.speckle });
    }
    if t.band == 0 && b.kind != kind::RUBBLE {
        // Street level: shops behind roller shutters, some open and lit.
        if tz == 5 {
            return scale(st.neon, 0.9);
        }
        if u8f(t.hb, 0) < 0.4 && (1..=4).contains(&tu) && tz <= 3 {
            *emissive = 0.6;
            return scale(st.warm, 0.95 + 0.05 * u8f(t.ht, 8));
        }
        return scale([176, 180, 184, 255], if tz % 2 == 0 { 1.06 } else { 0.9 });
    }
    // Upper floors: small square tiles, a band of windows, a sign now and then.
    if bits(t.hb, 4, 4) == 0 && (1..=4).contains(&tu) && (1..=4).contains(&tz) {
        *emissive = 0.8;
        return if glyph(t.hb2, tu, tz) { st.paint } else { sign_colour(bits(t.hb, 12, 8)) };
    }
    if (2..=4).contains(&tz) && (1..=4).contains(&tu) && b.kind != kind::RUBBLE {
        if tz == 4 && (tu == 1 || tu == 4) {
            return scale(facade, 0.8);
        }
        if u8f(t.hb2, (tu * 8) as u32) < 0.22 {
            *emissive = 0.5;
            return st.warm;
        }
        if (tu + tz) % 4 == 0 {
            *spec = 0.4;
        }
        return scale(st.glass, 0.95 + 0.1 * u8f(t.ht, 8));
    }
    let tile = if (tu + tz * 3) % 2 == 0 { 1.03 } else { 0.97 };
    let top_lip = if t.below_top < 1.0 / T { 1.1 } else { 1.0 };
    scale(facade, tile * top_lip * t.speckle)
}

/// The lab: brushed steel, glass curtain walls, neon strips, screens.
fn lab(st: &Stock, b: &Block, t: &Texel, emissive: &mut f64, spec: &mut f64) -> Rgba {
    let (tu, tz) = (t.tu, t.tz);
    if b.kind == kind::ALTAR {
        if !t.side {
            // A holographic ring over the console.
            let (du, dv) = (tu as f64 - 2.5, tz as f64 - 2.5);
            let r = (du * du + dv * dv).sqrt();
            if (1.4..2.6).contains(&r) {
                *emissive = 1.0;
                return st.neon;
            }
            return st.plinth;
        }
        // A UI screen: a bar graph, lines of readout, a frame.
        if tu == 0 || tu == 5 || tz == 5 {
            return st.trim;
        }
        *emissive = 0.85;
        let bar = bits(t.hb, (tu * 3) as u32, 2) + 1;
        if tz < bar && tu <= 2 {
            return st.neon;
        }
        if tu >= 3 && tz % 2 == 0 && bits(t.ht, 20, 2) != 0 {
            return blend(st.neon, st.paint, 50, 100);
        }
        return st.screen;
    }
    if !t.side {
        // Deck plates with seams and a few small lights.
        let seam = tu == 0 || tz == 0 || tu == 3 || tz == 3;
        if b.kind == kind::PILLAR && (2..=3).contains(&tu) && (2..=3).contains(&tz) {
            *emissive = 0.9;
            return st.neon;
        }
        return scale(st.panel, if seam { 0.84 } else { 0.98 * t.speckle });
    }
    if b.kind == kind::PILLAR {
        // A column with a neon line down its middle.
        if tu == 2 || tu == 3 {
            *emissive = 1.0;
            return st.neon;
        }
        return scale(st.trim, if tu % 2 == 0 { 1.05 } else { 0.95 });
    }
    if t.band == 0 {
        // A dark plinth with a light line along its top.
        if tz == 5 {
            *emissive = 0.8;
            return st.neon;
        }
        return scale(st.plinth, t.speckle);
    }
    let glass = b.kind == kind::ROOF || u8f(t.hb, 0) < 0.45;
    if glass {
        // A curtain wall: mullions, a transom, streaks of reflected sky.
        if tu == 0 || tz == 0 {
            return st.trim;
        }
        if (tu + 6 - tz) % 5 == 0 {
            *spec = 0.5;
            return scale(st.glass, 1.25);
        }
        return scale(st.glass, 0.92 + 0.08 * u8f(t.ht, 8));
    }
    // Brushed steel panels, three texels tall, with a grain along them.
    if tz == 0 || tz == 3 {
        return scale(st.panel, 0.8);
    }
    if bits(t.hb, 8, 3) == 0 && tz == 4 {
        *emissive = 0.8;
        return st.neon;
    }
    let grain = 0.95 + 0.1 * (bits(t.hb2, (tz * 4) as u32, 2) as f64 / 3.0);
    scale(st.panel, grain * if tu % 2 == 0 { 1.02 } else { 0.98 })
}

/// Ecofuture: white titanium, living walls, solar, a bio reactor.
fn eco(st: &Stock, b: &Block, t: &Texel, emissive: &mut f64, spec: &mut f64) -> Rgba {
    let (tu, tz) = (t.tu, t.tz);
    if b.kind == kind::ALTAR {
        if !t.side {
            let rim = tu == 0 || tz == 0 || tu == 5 || tz == 5;
            if rim {
                return st.panel;
            }
            *emissive = 0.7;
            return st.foliage[2];
        }
        // A glass vessel of glowing culture, banded in titanium, bubbling.
        if tz == 0 || tz == 5 {
            return st.trim;
        }
        *emissive = 0.7;
        if bits(t.ht, 8, 4) == 0 {
            return st.paint;
        }
        return blend(st.foliage[2], st.glow, 25, 100);
    }
    if !t.side {
        return match b.kind {
            // Green roofs.
            kind::ROOF => {
                if tu == 0 || tz == 0 {
                    st.panel
                } else {
                    st.foliage[bits(t.ht, 8, 1) as usize + 1]
                }
            }
            kind::FLOOR | kind::STONE if bits(t.hc, 4, 1) == 0 => {
                // Solar on the low roofs.
                if tu == 0 || tz == 0 {
                    st.trim
                } else {
                    if (tu + tz) % 4 == 0 {
                        *spec = 0.5;
                    }
                    scale(st.solar, t.speckle)
                }
            }
            kind::RUBBLE => scale(st.gravel, t.speckle),
            _ => {
                let seam = tu == 0 || tz == 0;
                scale(st.panel, if seam { 0.86 } else { t.speckle })
            }
        };
    }
    if b.kind == kind::PILLAR {
        // A sensor mast: white with grey rings and an amber light on top.
        if t.below_top < 1.0 / T * 2.0 && (2..=3).contains(&tu) {
            *emissive = 0.9;
            return st.glow;
        }
        return if tz == 2 { st.trim } else { scale(st.panel, if tu == 0 { 0.9 } else { 1.0 }) };
    }
    // Living walls on some columns: a vertical garden in a white frame.
    if bits(t.hc, 0, 2) == 0 && t.band > 0 {
        if tu == 0 || tu == 5 {
            return st.panel;
        }
        return st.foliage[bits(t.ht, 8, 2) as usize % 3];
    }
    // Titanium panels: seams, a rounded lit top edge, vents, status lights.
    if tu == 0 || tz == 0 {
        return st.trim;
    }
    if t.below_top < 1.0 / T {
        return scale(st.panel, 1.06);
    }
    if bits(t.hb, 0, 2) == 0 && (tz == 2 || tz == 3) && (1..=4).contains(&tu) {
        // A vent.
        return if tz == 2 { scale(st.trim, 0.6) } else { st.trim };
    }
    if bits(t.hb, 4, 3) == 0 && tz == 4 && tu == 4 {
        *emissive = 0.9;
        return st.glow;
    }
    scale(st.panel, 0.98 + 0.03 * u8f(t.ht, 8))
}

/// The greenhouse: steel and glass with plants behind, white columns, a pool.
fn greenhouse(st: &Stock, b: &Block, t: &Texel, emissive: &mut f64, spec: &mut f64) -> Rgba {
    let (tu, tz) = (t.tu, t.tz);
    if b.kind == kind::ALTAR {
        if !t.side {
            // A still pool, sun glinting on it.
            if tu == 0 || tz == 0 || tu == 5 || tz == 5 {
                return st.plinth;
            }
            if (tu + tz) % 5 == 0 {
                *spec = 0.8;
            }
            *emissive = 0.25;
            return scale(st.water, 0.95 + 0.1 * u8f(t.ht, 8));
        }
        return scale(st.plinth, t.speckle * if tz % 3 == 2 { 0.85 } else { 1.0 });
    }
    if !t.side {
        return match b.kind {
            kind::ROOF => {
                // Glazed roof panes.
                if tu == 0 || tz == 0 {
                    return st.trim;
                }
                if (tu + tz) % 6 == 0 {
                    *spec = 0.6;
                }
                scale(st.glass, 0.96 + 0.06 * u8f(t.ht, 8))
            }
            kind::FLOOR | kind::STONE => {
                // Raised beds.
                if tu == 0 || tz == 0 || tu == 5 || tz == 5 {
                    st.plinth
                } else if u8f(t.ht, 8) < 0.5 {
                    st.foliage[bits(t.ht, 16, 1) as usize]
                } else {
                    scale(st.soil, t.speckle)
                }
            }
            kind::RUBBLE => scale(st.gravel, t.speckle),
            _ => scale(st.panel, if tu == 0 || tz == 0 { 0.88 } else { t.speckle }),
        };
    }
    if b.kind == kind::PILLAR {
        // White steel columns with a bolted collar each level.
        return if tz == 0 || tz == 5 { st.trim } else { scale(st.panel, if tu == 0 || tu == 5 { 0.9 } else { 1.0 }) };
    }
    if t.band == 0 {
        // A low base wall of pale concrete.
        return scale(st.plinth, t.speckle * if tz == 5 { 1.1 } else { 1.0 });
    }
    // Glazing in a steel grid, plants pressed against the glass.
    if tu == 0 || tz == 0 || tu == 3 {
        return st.trim;
    }
    let behind = u8f(t.ht, 16) < 0.35 + 0.25 * u8f(t.hb, 0);
    if behind {
        return blend(st.foliage[bits(t.ht, 8, 1) as usize + 1], st.glass, 35, 100);
    }
    if (tu + tz) % 5 == 0 {
        *spec = 0.5;
    }
    *emissive = 0.0;
    scale(st.glass, 0.95 + 0.08 * u8f(t.ht, 8))
}



}

// ---- src/grid.rs ----
pub mod grid {
//! Stacked 2.5D tile layers — the data model the shrine generator and the
//! future editor both write to.
//!
//! A [`Layers`] value owns a fixed 24x24 grid, repeated for each height level.
//! A level is a flat diamond of tiles at one `z`; the shrine is a stack of
//! them. The module is deliberately shrine-agnostic: it stores tiles and
//! nothing else, so other scenes can reuse it (`SC002`).

use crate::{GRID_H, GRID_W};

/// Cells per level (24x24).
pub const CELLS: usize = (GRID_W * GRID_H) as usize;

/// Upper bound on stack height, so the model is a flat array rather than a
/// growable tree. The shrine is expected to use far fewer levels.
pub const MAX_LEVELS: u32 = 32;

/// One cell of a layer. `kind` 0 means empty; `variant` is cosmetic and
/// deterministic (seeded), never behaviour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    pub kind: u8,
    pub variant: u8,
}

impl Tile {
    pub const EMPTY: Tile = Tile { kind: 0, variant: 0 };

    pub const fn new(kind: u8, variant: u8) -> Self {
        Tile { kind, variant }
    }

    pub const fn is_empty(self) -> bool {
        self.kind == 0
    }
}

/// A stack of tile layers, row-major within each level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers {
    tiles: Vec<Tile>,
    levels: u32,
}

impl Layers {
    /// New empty layers, clamped to [`MAX_LEVELS`].
    pub fn new(levels: u32) -> Self {
        let levels = levels.min(MAX_LEVELS);
        Layers {
            tiles: vec![Tile::EMPTY; levels as usize * CELLS],
            levels,
        }
    }

    pub fn levels(&self) -> u32 {
        self.levels
    }

    pub fn in_bounds(&self, gx: i32, gy: i32, gz: i32) -> bool {
        gx >= 0
            && gy >= 0
            && gz >= 0
            && (gx as u32) < GRID_W
            && (gy as u32) < GRID_H
            && (gz as u32) < self.levels
    }

    fn index(gx: u32, gy: u32, gz: u32) -> usize {
        (gz * CELLS as u32 + gy * GRID_W + gx) as usize
    }

    /// The tile at a cell, or [`Tile::EMPTY`] when out of bounds.
    pub fn get(&self, gx: i32, gy: i32, gz: i32) -> Tile {
        if !self.in_bounds(gx, gy, gz) {
            return Tile::EMPTY;
        }
        self.tiles[Self::index(gx as u32, gy as u32, gz as u32)]
    }

    /// Write a tile. Returns false when the cell is out of bounds.
    pub fn set(&mut self, gx: i32, gy: i32, gz: i32, tile: Tile) -> bool {
        if !self.in_bounds(gx, gy, gz) {
            return false;
        }
        let i = Self::index(gx as u32, gy as u32, gz as u32);
        self.tiles[i] = tile;
        true
    }

    /// Fill an entire level with one tile.
    pub fn fill_level(&mut self, gz: i32, tile: Tile) {
        if gz < 0 || gz as u32 >= self.levels {
            return;
        }
        let start = gz as usize * CELLS;
        for t in &mut self.tiles[start..start + CELLS] {
            *t = tile;
        }
    }

    /// The tiles of one level as a flat, row-major slice.
    pub fn level(&self, gz: u32) -> &[Tile] {
        let start = gz as usize * CELLS;
        &self.tiles[start..start + CELLS]
    }

    pub fn count_non_empty(&self) -> usize {
        self.tiles.iter().filter(|t| !t.is_empty()).count()
    }

    /// A stable FNV-1a hash of every tile, for golden tests and previews.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for t in &self.tiles {
            h ^= t.kind as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
            h ^= t.variant as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// Iterate `(gx, gy, tile)` for one level, row-major.
    pub fn iter_level(&self, gz: u32) -> impl Iterator<Item = (u32, u32, Tile)> + '_ {
        self.level(gz).iter().enumerate().map(|(i, &tile)| {
            let i = i as u32;
            (i % GRID_W, i / GRID_W, tile)
        })
    }
}

}

// ---- src/iso.rs ----
pub mod iso {
//! The isometric framework: a shrine-agnostic projection between a square grid
//! and the fixed-point render plane.
//!
//! This module knows nothing about shrines, plants or weather. It is the
//! reusable layer future scenes (and the editor) build on (`SC002`).
//!
//! # Fixed point
//!
//! The logical buffer is 300x300, but the 24x24 grid does not divide into whole
//! pixels (300 / 24 = 12.5). To keep every edge exact and deterministic, all
//! projection coordinates are integers in *fixed-point units*: [`FP`] units per
//! logical pixel. With `FP = 16`, one cell is 200x100 units (12.5x6.25 logical
//! px) and the whole grid is exactly 4800x2400 units (300x150 logical px), so
//! nothing is rounded until the rasteriser decides a pixel.
//!
//! # Coordinates
//!
//! `gx` runs to the lower-right, `gy` to the lower-left, `gz` up. The centre of
//! cell `(0, 0, 0)` sits at `(origin_x, origin_y)`; the diamond corners of a
//! cell follow from its centre. [`Iso::grid_24`] is the canonical projection
//! that fits a 24x24 grid exactly across the 300-pixel logical width.

use crate::{LOGICAL_H, LOGICAL_W};

/// Fixed-point units per logical pixel.
pub const FP: i32 = 16;

/// A point on the projection plane, in fixed-point units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// The projection. A camera nudge (the mouse tilt) is just a different origin
/// plus a small shear applied before rasterising; the grid maths never changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iso {
    /// Plane position of the centre of cell `(0, 0, 0)`.
    pub origin_x: i32,
    pub origin_y: i32,
    /// Full diamond width and height of one cell, in fixed-point units.
    pub tile_w: i32,
    pub tile_h: i32,
    /// Vertical rise of one stack level, in fixed-point units.
    pub level_h: i32,
    /// Bounded camera nudge in `[-1, 1]`; 0 is the canonical view. Yaw shears
    /// the plane horizontally by depth; pitch squeezes it vertically. This is
    /// the whole "tilt the world a little" effect — no 3D maths, no cost.
    pub cam_yaw: f64,
    pub cam_pitch: f64,
}

impl Iso {
    /// The canonical 24x24 projection.
    ///
    /// One cell is 12.5 logical pixels wide (200 units), so 24 cells span the
    /// full 300-pixel logical width exactly. The grid is placed with its ground
    /// plane in the lower two-thirds of the buffer, leaving room above for the
    /// shrine's height and below for foreground growth.
    pub const fn grid_24() -> Self {
        Iso {
            origin_x: (LOGICAL_W as i32) * FP / 2,
            origin_y: (LOGICAL_H as i32) * FP / 4,
            tile_w: 200,
            tile_h: 100,
            level_h: 100,
            cam_yaw: 0.0,
            cam_pitch: 0.0,
        }
    }

    pub const fn half_w(&self) -> i32 {
        self.tile_w / 2
    }

    pub const fn half_h(&self) -> i32 {
        self.tile_h / 2
    }

    /// The centre of a cell at stack level `gz`, with the camera nudge applied.
    pub fn project(&self, gx: i32, gy: i32, gz: i32) -> Point {
        let depth = (gx + gy) as f64;
        let shear = (self.cam_yaw * depth * 1.4) as i32;
        let y_scale = 1.0 + self.cam_pitch * 0.12;
        Point {
            x: self.origin_x + (gx - gy) * self.half_w() + shear,
            y: self.origin_y + (depth * self.half_h() as f64 * y_scale) as i32 - gz * self.level_h,
        }
    }

    /// The four diamond corners of a cell's top face: top, right, bottom, left.
    pub fn corners(&self, gx: i32, gy: i32, gz: i32) -> [Point; 4] {
        let c = self.project(gx, gy, gz);
        [
            Point { x: c.x, y: c.y - self.half_h() },
            Point { x: c.x + self.half_w(), y: c.y },
            Point { x: c.x, y: c.y + self.half_h() },
            Point { x: c.x - self.half_w(), y: c.y },
        ]
    }

    /// Plane point -> cell at stack level `gz`, rounded to the nearest cell.
    /// Used by the future editor to pick what the mouse is over.
    pub fn unproject(&self, x: i32, y: i32, gz: i32) -> (i32, i32) {
        let a = (x - self.origin_x + self.half_w() / 2).div_euclid(self.half_w());
        let b = (y - self.origin_y + gz * self.level_h + self.half_h() / 2).div_euclid(self.half_h());
        ((a + b).div_euclid(2), (b - a).div_euclid(2))
    }

    /// Painter's key: farther cells sort lower and draw first. The sum of the
    /// two horizontal axes orders the columns; `gz` breaks ties in a column.
    pub const fn depth_key(&self, gx: i32, gy: i32, gz: i32) -> u32 {
        ((gx + gy) as u32) << 8 | (gz as u32 & 0xff)
    }

    /// The plane shift caused by a world-space grid offset. Because the
    /// projection is linear, moving a whole-cell offset into the origin is
    /// exactly equivalent to moving every coordinate — so an unrotated shrine
    /// can be offset without leaving the pixel-exact axis-aligned path.
    pub fn offset_shift(&self, ox: i32, oy: i32, oz: i32) -> Point {
        let depth = (ox + oy) as f64;
        let shear = (self.cam_yaw * depth * 1.4) as i32;
        let y_scale = 1.0 + self.cam_pitch * 0.12;
        Point {
            x: (ox - oy) * self.half_w() + shear,
            y: (depth * self.half_h() as f64 * y_scale) as i32 - oz * self.level_h,
        }
    }

    /// Project a point of the *rotated architecture*: the model point is turned
    /// by `rot` (a row-major 3x3), shifted by `off` in world space, then run
    /// through this same isometric projection. `off` is folded in after the
    /// rotation, so offsets move along the grid's own axes.
    ///
    /// Used only when the architecture is actually rotated; every unrotated
    /// path keeps using [`project`], so their pixels cannot drift.
    pub fn project_rotated(
        &self,
        x: f64,
        y: f64,
        z: f64,
        rot: &[[f64; 3]; 3],
        off: (f64, f64, f64),
    ) -> Point {
        let (wx, wy, wz) = to_world(x, y, z, rot, off);
        self.project_world(wx, wy, wz)
    }

    /// Project a point already in world space (see [`to_world`]). Split out of
    /// [`project_rotated`] so geometry can be clipped against the ground plane
    /// in world space before it is projected; the arithmetic is unchanged.
    pub fn project_world(&self, wx: f64, wy: f64, wz: f64) -> Point {
        let depth = wx + wy;
        let shear = (self.cam_yaw * depth * 1.4) as i32;
        let y_scale = 1.0 + self.cam_pitch * 0.12;
        Point {
            x: self.origin_x + ((wx - wy) * self.half_w() as f64).round() as i32 + shear,
            y: self.origin_y + (depth * self.half_h() as f64 * y_scale).round() as i32
                - (wz * self.level_h as f64).round() as i32,
        }
    }
}

/// A model point turned by `rot` (row-major 3x3) and shifted by `off`: its
/// world position, where `wz = 0` is the ground plane.
pub fn to_world(x: f64, y: f64, z: f64, rot: &[[f64; 3]; 3], off: (f64, f64, f64)) -> (f64, f64, f64) {
    (
        rot[0][0] * x + rot[0][1] * y + rot[0][2] * z + off.0,
        rot[1][0] * x + rot[1][1] * y + rot[1][2] * z + off.1,
        rot[2][0] * x + rot[2][1] * y + rot[2][2] * z + off.2,
    )
}

/// Logical pixels -> fixed-point units.
pub fn to_units(px: i32) -> i32 {
    px * FP
}

/// Fixed-point units -> logical pixels, rounded down.
pub fn to_px(units: i32) -> i32 {
    units.div_euclid(FP)
}

/// Fixed-point units -> logical pixels, rounded to nearest.
pub fn to_px_round(units: i32) -> i32 {
    (units + FP / 2).div_euclid(FP)
}

}

// ---- src/layout.rs ----
pub mod layout {
//! The ground plan of the four built futures.
//!
//! The wild biomes are earth and grass everywhere. The futures lay something
//! out around the shrine instead: a city block with roads and sidewalks, a
//! lab floor with light strips, parkland with curving paths and solar
//! tiles, a greenhouse with planting beds. Each is a pure function of the
//! seed and the point on the plateau, so the materials, the plants and the
//! props all agree on where things are without storing a map.
//!
//! The shrine's 16x16 footprint sits centred on the 24x24 plateau, leaving a
//! ring four cells deep around it; `ring` counts outwards through it (1 is
//! the cells touching the footprint, 4 the plateau's edge).

use crate::math::{cos_turns, sin_turns};
use crate::plants::Biome;
use crate::rng::Rng;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

/// Stream id for the layout's few seeded choices.
const STREAM: u64 = 0x4c41_594f; // "LAYO"

/// Where the footprint starts on the plateau (the same for every shrine).
pub const ORIGIN: (i32, i32) = ((GRID_W as i32 - SHRINE_W as i32) / 2, (GRID_H as i32 - SHRINE_H as i32) / 2);

/// The four sides of the footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Low x (the far left on screen).
    West,
    /// High x.
    East,
    /// Low y (the far right on screen).
    North,
    /// High y.
    South,
}

impl Side {
    const ALL: [Side; 4] = [Side::West, Side::East, Side::North, Side::South];

    /// Whether roads on this side run along the y axis.
    pub fn along_y(self) -> bool {
        matches!(self, Side::West | Side::East)
    }
}

/// What a point of the plateau is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// The wild biomes' earth and grass.
    Wild,
    /// Asphalt. `lane` marks the texel edge carrying the centre line,
    /// `crossing` a zebra crossing across it, `along_y` the direction it runs.
    Road { along_y: bool, lane: bool, crossing: bool },
    /// Where two roads meet: plain asphalt with stop lines.
    Junction,
    /// Paving slabs; `curb` is the side facing the road, if any.
    Sidewalk { curb: Option<Side> },
    /// Pale paving inside the shrine's footprint.
    Plaza,
    /// Mown grass (a city verge, or the park lawn).
    Lawn,
    /// A curving white path through the park.
    Path,
    /// A solar tile.
    Solar,
    /// Lab floor panels; `strip` carries a glowing light line.
    Floor { strip: bool },
    /// A planting bed of dark soil.
    Bed,
    /// Glazed terracotta tiles.
    Tile,
    /// Raked gravel.
    Gravel,
}

impl Ground {
    /// Whether plants may grow here.
    pub fn grows(self) -> bool {
        matches!(self, Ground::Wild | Ground::Lawn | Ground::Bed)
    }

    /// Hard, open floor where a prop can stand.
    pub fn standable(self) -> bool {
        matches!(self, Ground::Sidewalk { .. } | Ground::Plaza | Ground::Floor { .. } | Ground::Tile | Ground::Gravel | Ground::Lawn)
    }
}

/// The ring a cell sits in: 0 inside the footprint, 1..4 outward.
pub fn ring(gx: i32, gy: i32) -> i32 {
    let (ox, oy) = ORIGIN;
    let (ex, ey) = (ox + SHRINE_W as i32 - 1, oy + SHRINE_H as i32 - 1);
    (ox - gx).max(gx - ex).max(oy - gy).max(gy - ey).max(0)
}

/// Which sides of the footprint a cell lies beyond (two at a corner).
fn sides(gx: i32, gy: i32) -> (Option<Side>, Option<Side>) {
    let (ox, oy) = ORIGIN;
    let (ex, ey) = (ox + SHRINE_W as i32 - 1, oy + SHRINE_H as i32 - 1);
    let sx = if gx < ox { Some(Side::West) } else if gx > ex { Some(Side::East) } else { None };
    let sy = if gy < oy { Some(Side::North) } else if gy > ey { Some(Side::South) } else { None };
    (sx, sy)
}

/// The seed's layout choices for a biome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    biome: Biome,
    /// A bit per side (in `Side::ALL` order): the city's roads, or the
    /// greenhouse's planting beds.
    sides: u8,
    /// Where along each side the zebra crossings are (a cell index 0..15).
    crossing: u8,
    /// The park's path: the radius of its loop and the angle of its spur.
    loop_r: f64,
    spur: f64,
    seed: u64,
}

impl Plan {
    pub fn new(seed: u64, biome: Biome) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        // Always at least two sides of road (or bed), one of them in front.
        let mut sides = rng.below(16) as u8 | 0b1000;
        if sides.count_ones() < 2 {
            sides |= 0b0010;
        }
        Plan {
            biome,
            sides,
            crossing: 4 + rng.below(8) as u8,
            loop_r: rng.range(9.6, 10.6),
            spur: rng.range(0.0, 1.0),
            seed,
        }
    }

    fn has(&self, side: Side) -> bool {
        let i = Side::ALL.iter().position(|s| *s == side).unwrap();
        self.sides >> i & 1 == 1
    }

    /// The ground of cell `(gx, gy)` (the cell's centre, for the per-cell
    /// layouts; the park's paths are worked out per point by [`Plan::at`]).
    pub fn cell(&self, gx: i32, gy: i32) -> Ground {
        self.at(gx as f64, gy as f64)
    }

    /// The ground at plateau point `(x, y)` (cell centres are whole numbers).
    pub fn at(&self, x: f64, y: f64) -> Ground {
        let (gx, gy) = (x.round() as i32, y.round() as i32);
        let r = ring(gx, gy);
        let (sx, sy) = sides(gx, gy);
        match self.biome {
            Biome::NearFuture => self.city(gx, gy, r, sx, sy),
            Biome::ResearchLab => {
                // A light strip round the footprint, and across the floor
                // every six cells.
                let strip = r == 1 || (r > 1 && (gx % 6 == 0 || gy % 6 == 0) && self.seed & 1 == 0);
                if r >= 3 && sx.is_some() && sy.is_some() {
                    // Planters in the corners.
                    Ground::Bed
                } else {
                    Ground::Floor { strip }
                }
            }
            Biome::Ecofuture => {
                if r == 0 {
                    return Ground::Lawn;
                }
                // A loop path round the shrine, gently wavering, and one spur
                // meandering out from it to the plateau's edge.
                let (cx, cy) = (GRID_W as f64 / 2.0 - 0.5, GRID_H as f64 / 2.0 - 0.5);
                let (dx, dy) = (x - cx, y - cy);
                let d = (dx * dx + dy * dy).sqrt();
                let wobble = 0.3 * sin_turns(self.spur + 0.09 * x) * cos_turns(0.07 * y - self.spur);
                if (d - self.loop_r - wobble).abs() < 0.42 {
                    return Ground::Path;
                }
                if d > self.loop_r {
                    // Out through the front: along +y or +x, by the seed.
                    let (along, across, centre) = if self.sides & 1 == 0 { (y, x, cx) } else { (x, y, cy) };
                    let meander = centre + 2.2 * sin_turns(0.06 * along + self.spur);
                    if along > centre && (across - meander).abs() < 0.42 {
                        return Ground::Path;
                    }
                }
                if r >= 3 && cell_hash(self.seed, gx, gy) % 5 == 0 {
                    return Ground::Solar;
                }
                Ground::Lawn
            }
            Biome::Greenhouse => {
                if r == 0 {
                    return Ground::Tile;
                }
                if r == 1 {
                    return Ground::Gravel;
                }
                // Beds on the seed's sides, two cells deep; tiles between.
                let bed = |s: Option<Side>| s.is_some_and(|s| self.has(s));
                if (2..=3).contains(&r) && (bed(sx) || bed(sy)) {
                    Ground::Bed
                } else if r == 4 && (gx + gy) % 5 == 0 {
                    Ground::Bed
                } else {
                    Ground::Tile
                }
            }
            _ => Ground::Wild,
        }
    }

    /// A city block: the shrine on its own plot, paved, sidewalks round it,
    /// roads on the seed's sides, grass verges on the rest.
    fn city(&self, gx: i32, gy: i32, r: i32, sx: Option<Side>, sy: Option<Side>) -> Ground {
        if r == 0 {
            return Ground::Plaza;
        }
        let road_x = sx.is_some_and(|s| self.has(s));
        let road_y = sy.is_some_and(|s| self.has(s));
        match r {
            1 => {
                // The sidewalk, its curb toward the road beyond it.
                let curb = match (sx, sy) {
                    (Some(s), None) if road_x => Some(s),
                    (None, Some(s)) if road_y => Some(s),
                    _ => None,
                };
                Ground::Sidewalk { curb }
            }
            2 | 3 => {
                if sx.is_some() && sy.is_some() {
                    // A corner: a junction if both roads come here.
                    return match (road_x, road_y) {
                        (true, true) => Ground::Junction,
                        (true, false) => Ground::Road { along_y: true, lane: r == 2, crossing: false },
                        (false, true) => Ground::Road { along_y: false, lane: r == 2, crossing: false },
                        _ => Ground::Lawn,
                    };
                }
                if let Some(side) = sx.or(sy) {
                    if self.has(side) {
                        // The crossing sits at one place along each side.
                        let along = if side.along_y() { gy - ORIGIN.1 } else { gx - ORIGIN.0 };
                        let crossing = along == self.crossing as i32 || along == self.crossing as i32 + 1;
                        return Ground::Road { along_y: side.along_y(), lane: r == 2, crossing };
                    }
                }
                Ground::Lawn
            }
            _ => {
                // The far sidewalk, along the plateau's edge.
                Ground::Sidewalk { curb: None }
            }
        }
    }
}

/// A small per-cell hash, for scattering things across the plan.
pub fn cell_hash(seed: u64, gx: i32, gy: i32) -> u64 {
    crate::rng::hash3(seed ^ STREAM, gx as u64 & 0xff, gy as u64 & 0xff)
}



}

// ---- src/learn.rs ----
pub mod learn {
//! What the page's Learn mode is shown: everything about *this* shrine that
//! explains how it is made, as one JSON document, and a small array of the
//! engine's moving parts for its live views.
//!
//! Display only: nothing here feeds the sound or the picture. The JSON is
//! written by hand (no dependencies, `HC001`).

use crate::drums::{Section, Voice};
use crate::sim::{Sim, SimConfig};
use crate::track::{LoopDesign, Track};

/// Every drum section, in the order the live state's section index uses.
pub const SECTIONS: [Section; 11] = [
    Section::Out,
    Section::HatsOnly,
    Section::HalfTime,
    Section::Full,
    Section::Rolls,
    Section::Breakdown,
    Section::TwoStep,
    Section::Shuffle,
    Section::Clicks,
    Section::Pulse,
    Section::Fracture,
];

/// The live state's length, in `f64`s. Layout (see [`state`]):
/// 0 clock · 1 chord · 2 next chord · 3 morph · 4 bar in the chord ·
/// 5 drum step · 6 drum section (index into [`SECTIONS`], −1 none) ·
/// 7 drum bar · 8 hits on the latest step (bit mask: kick 1, snare 2,
/// ghost 4, hat 8, open hat 16, ride 32) · 9 drone cutoff Hz · 10 bass note
/// (MIDI) · 11 drone duck gain · 12 drone arc gain · 13–16 loop arc gains ·
/// 17–18 glitch densities · 19–33 peak meters (drone, bass, glitch 1,
/// glitch 2, echo L, echo R, reverb, drums, tape, loops 1–4, out L, out R) ·
/// 34–40 the seven slow cycles (−1..1) · 41–42 glitch steps · 43 solo
/// (index into [`SOLOS`] + 1, 0 none) · 44 solo fade · 45 drum hits so far.
pub const STATE_LEN: usize = 48;

/// The layers that can be soloed, in the order `shrine_set_solo` numbers
/// them (from 1; 0 is the full mix).
pub const SOLOS: [crate::track::Solo; 10] = {
    use crate::track::Solo::*;
    [Drone, Glitch1, Glitch2, Bass, Drums, Loop1, Loop2, Loop3, Loop4, Space]
};

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    let r = (x * 1000.0).round() / 1000.0;
    if r == r.trunc() && r.abs() < 1e15 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn arr<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    let v: Vec<String> = items.into_iter().map(f).collect();
    format!("[{}]", v.join(","))
}

fn nums(items: impl IntoIterator<Item = f64>) -> String {
    arr(items, num)
}

fn rgb(c: [u8; 4]) -> String {
    esc(&format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

/// A sustained or event layer, for the page.
fn design(d: &LoopDesign) -> String {
    format!("{{\"name\":{},\"blurb\":{},\"on\":{}}}", esc(&d.name()), esc(&d.blurb()), d.is_on())
}

/// The whole description, as JSON. Builds the music engine once (without
/// playing it), so it can be asked of a visuals-only instance too.
pub fn json(config: &SimConfig) -> String {
    let track = Track::new(config.seeds, config.settings);
    let visual = Sim::new_visual(config.clone());
    describe(config, &track, &visual)
}

fn describe(config: &SimConfig, track: &Track, visual: &Sim) -> String {
    let d = track.describe();
    let s = &config.settings;
    let h = &d.harmony;
    let sr = crate::SAMPLE_RATE as f64;
    let mut f: Vec<String> = Vec::new();
    let mut put = |k: &str, v: String| f.push(format!("{}:{}", esc(k), v));

    // The piece.
    put("name", esc(s.name.as_str()));
    put("seed", esc(&config.seed.to_string()));
    let sd = config.seeds;
    put("seeds", arr([sd.s1, sd.s2, sd.s3, sd.s4, sd.s5, sd.s6, sd.s7], |x| esc(&x.to_string())));
    put("recipe", arr(crate::recipe::encode(config), |w| w.to_string()));
    put("start_s", s.start_s.to_string());

    // Mode, tempo, harmony.
    put("mode", format!("{{\"name\":{},\"blurb\":{},\"index\":{}}}", esc(s.pace.name()), esc(s.pace.blurb()), s.pace.index()));
    put("bpm", s.bpm.to_string());
    put("beat_s", num(d.tempo.beat() as f64 / sr));
    put("drum16_s", num(d.tempo.drum16 as f64 / sr));
    put(
        "scale",
        format!(
            "{{\"name\":{},\"mood\":{},\"intervals\":{}}}",
            esc(h.scale.name()),
            esc(h.scale.mood()),
            arr(h.scale.intervals().iter(), |i| i.to_string())
        ),
    );
    put("key", format!("{{\"pc\":{},\"name\":{}}}", h.key, esc(crate::harmony::key_name(h.key))));
    put(
        "chords",
        arr(h.chords.iter(), |c| {
            format!(
                "{{\"degree\":{},\"notes\":{},\"bass\":{},\"bars\":{},\"tones\":{}}}",
                c.degree,
                nums(c.notes.iter().copied()),
                num(c.bass),
                c.bars,
                arr(c.tones.iter(), |t| t.to_string())
            )
        }),
    );
    put("chord_s", num(h.bar_samples as f64 / sr));
    put("cycle_s", num(h.cycle as f64 / sr));

    // The drone: its FM recipes and frozen spectra (96 log-spaced bands,
    // 40 Hz – 16 kHz, scaled to the loudest).
    put(
        "recipes",
        arr(d.recipes.iter(), |chord| {
            arr(chord.iter(), |(algo, [r2, r3, i2, i3], fb)| {
                format!(
                    "{{\"stack\":{},\"r2\":{},\"r3\":{},\"i2\":{},\"i3\":{},\"fb\":{}}}",
                    *algo == crate::drone::Algo::Stack,
                    num(*r2),
                    num(*r3),
                    num(*i2),
                    num(*i3),
                    num(*fb)
                )
            })
        }),
    );
    let bands = 96;
    let band = |m: &[f64], k: usize| -> f64 {
        // 40 Hz × 400^(k / bands), with the engine's own exp2 (log2 400 = 8.6439).
        let edge = |k: usize| 40.0 * crate::math::exp2(8.643_856_189_774_724 * k as f64 / bands as f64);
        let bin = |hz: f64| ((hz * d.fft_size as f64 / sr) as usize).min(m.len().saturating_sub(1));
        let (a, b) = (bin(edge(k)), bin(edge(k + 1)).max(bin(edge(k)) + 1));
        m[a..b.min(m.len())].iter().fold(0.0f64, |x, y| x.max(*y))
    };
    let spectra: Vec<Vec<f64>> = d.magnitudes.iter().map(|m| (0..bands).map(|k| band(m, k)).collect()).collect();
    let peak = spectra.iter().flatten().fold(1e-12f64, |a, b| a.max(*b));
    put("spectra", arr(spectra.iter(), |v| nums(v.iter().map(|x| (x / peak).sqrt()))));
    put("fft_size", d.fft_size.to_string());
    put("hop_s", num(d.hop as f64 / sr));
    put("drone_arc", format!("{{\"cycle_s\":{},\"hold_s\":{}}}", s.drone.cycle_s, s.drone.hold_s));
    put("drone_tone", s.drone_tone.to_string());
    put("drone_cutoff", format!("{{\"min\":{},\"octaves\":{}}}", num(crate::track::DRONE_CUTOFF_MIN), num(crate::track::DRONE_CUTOFF_OCTAVES)));
    put("drone_rhythm", s.drone_rhythm.to_string());

    // Drums.
    let v = &d.drum_voices;
    put(
        "drums",
        format!(
            "{{\"on\":{},\"kit\":{},\"blurb\":{},\"lines\":{},\"pattern\":{},\"space\":{},\"space_blurb\":{},\"overview\":{},\"slices\":{},\"source_bpm\":{},\"semitones_up\":{},\"bits\":{},\"sections\":{}}}",
            d.drums_on,
            esc(s.kit.name()),
            esc(s.kit.blurb()),
            arr(v.lines.iter(), |l| esc(l)),
            esc(d.break_pattern),
            esc(s.space.name()),
            esc(s.space.blurb()),
            nums(d.break_overview.iter().map(|x| *x as f64)),
            arr(d.slice_voices.iter(), |v| esc(&v.map_or(String::new(), |v: Voice| v.letter().to_string()))),
            num(v.source_bpm),
            num(v.semitones_up),
            v.bits,
            arr(d.sections.iter().take(64), |sec| SECTIONS.iter().position(|x| x == sec).unwrap_or(0).to_string())
        ),
    );
    put("section_names", arr(SECTIONS.iter(), |x| esc(x.name())));
    put("phrase_s", num((crate::drums::PHRASE_BARS * d.tempo.drum_bar()) as f64 / sr));

    // Glitches, echo and reverb.
    put(
        "glitch",
        arr(0..2, |i| {
            format!(
                "{{\"steps\":{},\"step_s\":{},\"summary\":{},\"on\":{}}}",
                d.layers[i].0,
                num(d.layers[i].1 as f64 / sr),
                esc(&s.glitch[i].summary()),
                s.glitch[i].enabled
            )
        }),
    );
    put("echo", format!("{{\"delay_s\":{},\"feedback\":{}}}", num(d.echo_delay as f64 / sr), num(d.echo_feedback)));
    put(
        "reverb",
        format!(
            "{{\"lines\":{},\"decay_s\":{}}}",
            arr(d.reverb_lines.iter(), |l| l.to_string()),
            num(if s.pace == crate::harmony::Pace::Micro { 1.2 } else { d.reverb_decay })
        ),
    );

    // The atmosphere loops.
    put(
        "loops",
        arr(d.loops.iter().enumerate(), |(i, l)| {
            format!(
                "{{\"design\":{},\"on\":{},\"beats\":{},\"len_s\":{},\"notes\":{},\"arc\":{{\"cycle_s\":{},\"hold_s\":{}}}}}",
                design(&s.loops[i]),
                l.on(),
                l.beats,
                num(l.len as f64 / sr),
                arr(l.notes.iter(), |n| nums(n.iter().copied())),
                s.loop_arcs[i].cycle_s,
                s.loop_arcs[i].hold_s
            )
        }),
    );
    put("transpose", s.loop_transpose.to_string());

    // The modes' own parts.
    let g = s.garage;
    put(
        "garage",
        format!(
            "{{\"swing\":{},\"progression\":{},\"lift\":{},\"bass\":{},\"stabs\":{},\"tone\":{}}}",
            g.swing,
            esc(g.progression_name()),
            esc(g.lift.name()),
            esc(g.bass.name()),
            esc(g.stabs.name()),
            esc(g.tone.name())
        ),
    );
    let m = s.micro;
    let pattern = |p: (u64, u64)| format!("[{},{}]", p.0, p.1);
    put(
        "micro",
        format!(
            "{{\"pulse\":{},\"grains\":{},\"pips\":{},\"dropouts\":{},\"chords\":{},\"shift\":{},\"drone_fragments\":{},\"loop_fragments\":{},\"shift_figure\":{}}}",
            esc(m.pulse.name()),
            esc(m.grains.name()),
            esc(m.pips.name()),
            esc(m.dropouts.name()),
            esc(m.chords.name()),
            esc(m.shift.name()),
            track.drone_gate.as_ref().map_or("null".to_string(), |g| pattern(g.pattern())),
            track.loop_gates.as_ref().map_or("null".to_string(), |gs| arr(gs.iter(), |g| pattern(g.pattern()))),
            track.drone_shift.as_ref().map_or("null".to_string(), |(_, f)| nums(f.iter().copied()))
        ),
    );
    put("bass_root", num(d.bass_root));
    put("master", arr(s.master.labels().iter().filter(|(_, on)| *on && s.master.enabled), |(n, _)| esc(n)));

    // The picture.
    let scene = &visual.scene;
    put("biome", format!("{{\"name\":{},\"blurb\":{},\"roofed\":{}}}", esc(config.biome.name()), esc(config.biome.blurb()), config.biome.roofed()));
    put("weather", format!("{{\"name\":{},\"blurb\":{}}}", esc(config.weather.name()), esc(config.weather.blurb())));
    let p = &scene.palette;
    put(
        "palette",
        format!(
            "{{\"sky\":[{},{}],\"ground\":[{},{}],\"stone\":[{},{},{}],\"foliage\":[{},{},{}],\"water\":{},\"accent\":{},\"fog\":{}}}",
            rgb(p.sky_top),
            rgb(p.sky_bottom),
            rgb(p.ground),
            rgb(p.ground_dark),
            rgb(p.stone_light),
            rgb(p.stone),
            rgb(p.stone_dark),
            rgb(p.foliage_light),
            rgb(p.foliage),
            rgb(p.foliage_dark),
            rgb(p.water),
            rgb(p.accent),
            rgb(p.fog)
        ),
    );
    let (ox, oy) = scene.shrine.origin();
    let w = crate::SHRINE_W as i32;
    put(
        "shrine",
        format!(
            "{{\"origin\":[{ox},{oy}],\"size\":{w},\"grid\":{},\"heights\":{},\"authored\":{}}}",
            crate::GRID_W,
            arr(0..w * w, |i| scene.shrine.height_at(i % w, i / w).to_string()),
            s.model.is_authored()
        ),
    );
    put("coupling", arr(crate::sim::COUPLING.iter(), |(a, b)| format!("[{},{}]", esc(a), esc(b))));
    put("camera_speed", s.camera_speed.to_string());
    put("code", format!("{{\"on\":{},\"summary\":{}}}", s.code.enabled, esc(&s.code.summary())));
    put("arch", esc(&s.arch.summary()));
    put(
        "wildlife",
        if s.wildlife { esc(&crate::wildlife::Wildlife::new(config.seed, config.biome).summary()) } else { "null".to_string() },
    );
    put("solos", arr(SOLOS.iter(), |x| esc(&format!("{x:?}"))));
    format!("{{{}}}", f.join(","))
}

/// The engine's moving parts right now, into `out` (`STATE_LEN` long). Also
/// resets the peak meters, so each call reads the peaks since the last.
pub fn state(track: &mut Track, out: &mut [f64]) {
    let snap = track.snapshot();
    out.iter_mut().for_each(|x| *x = 0.0);
    let mut set = |i: usize, v: f64| {
        if let Some(x) = out.get_mut(i) {
            *x = if v.is_finite() { v } else { 0.0 };
        }
    };
    set(0, snap.clock as f64);
    set(1, snap.harmony.chord as f64);
    set(2, snap.harmony.next as f64);
    set(3, snap.harmony.morph);
    set(4, snap.harmony.bar as f64);
    set(5, snap.drum_step as f64);
    match &snap.drum_plan {
        Some(p) => {
            set(6, SECTIONS.iter().position(|x| *x == p.section).map_or(-1.0, |i| i as f64));
            set(7, p.bar as f64);
        }
        None => set(6, -1.0),
    }
    let mask = snap.drum_hits.iter().fold(0u32, |m, v| {
        m | match v {
            Voice::Kick => 1,
            Voice::Snare => 2,
            Voice::Ghost => 4,
            Voice::Hat => 8,
            Voice::OpenHat => 16,
            Voice::Ride => 32,
        }
    });
    set(8, mask as f64);
    set(9, snap.drone_cutoff);
    set(10, snap.bass_root);
    set(11, snap.duck_gain);
    set(12, snap.drone_window);
    for k in 0..4 {
        set(13 + k, snap.loop_windows[k]);
    }
    set(17, snap.layers[0].density);
    set(18, snap.layers[1].density);
    let mt = &snap.meters;
    let meters = [
        mt.drone, mt.bass, mt.glitch1, mt.glitch2, mt.echo_l, mt.echo_r, mt.reverb, mt.drums, mt.tape, mt.loop1, mt.loop2, mt.loop3,
        mt.loop4, mt.out_l, mt.out_r,
    ];
    for (k, v) in meters.into_iter().enumerate() {
        set(19 + k, v);
    }
    for (k, (_, v)) in snap.lfos.iter().enumerate() {
        set(34 + k, *v);
    }
    set(41, snap.layers[0].step as f64);
    set(42, snap.layers[1].step as f64);
    set(43, snap.solo.and_then(|s| SOLOS.iter().position(|x| *x == s)).map_or(0.0, |i| i as f64 + 1.0));
    set(44, snap.solo_amount);
    set(45, snap.drum_hit_count as f64);
}



}

// ---- src/light.rs ----
pub mod light {
//! Light: the sun, the sky's ambient light, cast shadows, ambient occlusion,
//! rim light and the altar's glow — different for every weather.
//!
//! Each weather has a [`LightProfile`]: where the sun stands and what colour
//! it is, the colour of the shadowed sky light, how long and how dark the
//! shadows fall, and how vivid the final grade is. Clear has a warm sun and
//! crisp cool shadows; Dusk a low hot sun from the side throwing long shadows;
//! Fog almost no direct light at all, so the altar's glow carries the scene.
//!
//! Everything is quantised to a few levels with a 4x4 ordered (Bayer) dither
//! at the screen pixel, so shadow edges, occlusion and glow fall off in the
//! stepped, cross-hatched way of retro pixel art rather than in smooth ramps.
//!
//! Shadows are cast from the shrine's height field in *scene space* (the
//! model plus the architecture offset, before the camera orbit), so the sun
//! stays put in the world as the camera turns and the shadows turn with the
//! scene. When the architecture itself is rotated there is no height field to
//! cast from, and only the occlusion and the face lighting remain.

use crate::framebuffer::Rgba;
use crate::material::Albedo;
use crate::math::{cos_turns, sin_turns};
use crate::raster::{Face, Surface};
use crate::weather::WeatherKind;
use crate::{GRID_H, GRID_W};

/// Shadow-map samples per cell, along each axis.
const SUB: usize = 4;
const SW: usize = GRID_W as usize * SUB;
const SH: usize = GRID_H as usize * SUB;

/// The four neighbours, in the order of the per-cell bit masks.
const DIRS: [(f64, f64); 4] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];

/// The 4x4 ordered-dither matrix.
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// The threshold for pixel `(px, py)`, in (0, 1).
pub fn bayer(px: i32, py: i32) -> f64 {
    (BAYER[py.rem_euclid(4) as usize][px.rem_euclid(4) as usize] as f64 + 0.5) / 16.0
}

/// `x` rounded down, without a library call (`floor` is one on the
/// baseline x86-64 target, and this runs for every pixel).
#[inline]
pub fn floor(x: f64) -> f64 {
    let t = x as i64 as f64;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

/// `x` rounded to the nearest whole number, halves up.
#[inline]
fn round(x: f64) -> f64 {
    floor(x + 0.5)
}

/// `v` quantised to `steps` levels per unit, dithered by `t`.
#[inline]
pub fn dither(v: f64, steps: f64, t: f64) -> f64 {
    (floor(v * steps + t) / steps).max(0.0)
}

/// How a weather lights the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightProfile {
    /// Horizontal direction *toward* the sun, in scene axes (need not be unit).
    pub sun: (f64, f64),
    /// The sun's height above the horizon, in turns.
    pub elevation: f64,
    pub key: [u8; 3],
    pub key_k: f64,
    pub ambient: [u8; 3],
    pub ambient_k: f64,
    /// How far shadows reach (cells) and how much light they take (0-1).
    pub shadow_len: f64,
    pub shadow_k: f64,
    pub ao: f64,
    pub rim: f64,
    pub glow: f64,
    pub saturation: f64,
    pub contrast: f64,
    /// Cloud shadows drifting over the ground (0-1).
    pub clouds: f64,
    /// Far and low surfaces fading into the fog (0-1).
    pub depth_fog: f64,
}

/// The light for a weather.
pub fn profile(kind: WeatherKind) -> LightProfile {
    let base = LightProfile {
        sun: (-0.6, 0.8),
        elevation: 0.13,
        key: [255, 236, 200],
        key_k: 0.75,
        ambient: [150, 175, 230],
        ambient_k: 0.55,
        shadow_len: 2.5,
        shadow_k: 0.85,
        ao: 0.45,
        rim: 0.35,
        glow: 0.5,
        saturation: 1.15,
        contrast: 1.06,
        clouds: 0.15,
        depth_fog: 0.0,
    };
    match kind {
        WeatherKind::Clear => base,
        WeatherKind::Rain => LightProfile {
            sun: (-0.5, 0.85),
            elevation: 0.17,
            key: [200, 210, 232],
            key_k: 0.35,
            ambient: [150, 164, 198],
            ambient_k: 0.85,
            shadow_len: 1.5,
            shadow_k: 0.4,
            ao: 0.5,
            rim: 0.1,
            glow: 0.7,
            saturation: 0.95,
            contrast: 1.0,
            clouds: 0.3,
            depth_fog: 0.2,
            ..base
        },
        WeatherKind::Wind => LightProfile {
            sun: (-0.7, 0.7),
            elevation: 0.12,
            key: [242, 236, 220],
            key_k: 0.62,
            ambient: [150, 170, 212],
            ambient_k: 0.6,
            shadow_len: 2.5,
            shadow_k: 0.7,
            rim: 0.25,
            saturation: 1.05,
            contrast: 1.04,
            clouds: 0.6,
            ..base
        },
        WeatherKind::Fog => LightProfile {
            sun: (-0.5, 0.85),
            elevation: 0.2,
            key: [222, 222, 222],
            key_k: 0.15,
            ambient: [182, 186, 196],
            ambient_k: 0.95,
            shadow_len: 1.0,
            shadow_k: 0.15,
            ao: 0.55,
            rim: 0.0,
            glow: 1.0,
            saturation: 0.8,
            contrast: 0.95,
            clouds: 0.0,
            depth_fog: 0.3,
            ..base
        },
        WeatherKind::Storm => LightProfile {
            sun: (-0.4, 0.9),
            elevation: 0.2,
            key: [190, 200, 236],
            key_k: 0.25,
            ambient: [128, 138, 178],
            ambient_k: 0.82,
            shadow_len: 1.5,
            shadow_k: 0.35,
            ao: 0.5,
            rim: 0.05,
            glow: 0.9,
            saturation: 0.85,
            contrast: 1.08,
            clouds: 0.4,
            depth_fog: 0.3,
            ..base
        },
        WeatherKind::Snow => LightProfile {
            sun: (-0.6, 0.8),
            elevation: 0.1,
            key: [240, 245, 255],
            key_k: 0.45,
            ambient: [176, 196, 240],
            ambient_k: 0.74,
            shadow_len: 2.0,
            shadow_k: 0.5,
            ao: 0.4,
            rim: 0.3,
            glow: 0.6,
            saturation: 0.9,
            contrast: 1.0,
            clouds: 0.2,
            depth_fog: 0.25,
            ..base
        },
        WeatherKind::Dusk => LightProfile {
            sun: (0.85, -0.5),
            elevation: 0.045,
            key: [255, 164, 98],
            key_k: 0.88,
            ambient: [122, 96, 172],
            ambient_k: 0.55,
            shadow_len: 4.5,
            shadow_k: 0.9,
            rim: 0.5,
            glow: 1.0,
            saturation: 1.12,
            contrast: 1.08,
            clouds: 0.0,
            ..base
        },
    }
}

/// The light for a weather in a biome. The wild biomes take the weather's
/// light as it is; each future adjusts it: the city brighter and crisper,
/// the lab cool and artificial under its ceiling, the park soft and bright,
/// the greenhouse diffused through glass.
pub fn profile_in(kind: WeatherKind, biome: crate::plants::Biome) -> LightProfile {
    use crate::plants::Biome;
    let p = profile(kind);
    let mix = |a: [u8; 3], b: [u8; 3], f: f64| [0, 1, 2].map(|k| (a[k] as f64 * (1.0 - f) + b[k] as f64 * f) as u8);
    match biome {
        Biome::NearFuture => LightProfile {
            ambient: mix(p.ambient, [176, 204, 240], 0.5),
            ambient_k: (p.ambient_k + 0.14).min(1.0),
            key_k: p.key_k * 1.1,
            shadow_k: p.shadow_k * 0.85,
            saturation: p.saturation * 1.1,
            contrast: p.contrast * 1.03,
            depth_fog: p.depth_fog * 0.5,
            ..p
        },
        Biome::ResearchLab => LightProfile {
            // Ceiling panels: an even cool light whatever the sky does.
            key: mix(p.key, [214, 238, 255], 0.6),
            key_k: 0.3 + p.key_k * 0.3,
            ambient: mix(p.ambient, [150, 196, 206], 0.7),
            ambient_k: 0.9,
            shadow_len: 1.2,
            shadow_k: 0.45,
            rim: 0.45,
            glow: p.glow.max(0.85),
            saturation: 1.05,
            contrast: 1.06,
            clouds: 0.0,
            depth_fog: p.depth_fog * 0.4,
            ..p
        },
        Biome::Ecofuture => LightProfile {
            ambient_k: (p.ambient_k + 0.08).min(1.0),
            key_k: p.key_k * 1.05,
            ao: p.ao * 0.8,
            shadow_k: p.shadow_k * 0.8,
            saturation: p.saturation * 1.1,
            ..p
        },
        Biome::Greenhouse => LightProfile {
            key: mix(p.key, [255, 238, 196], 0.5),
            key_k: p.key_k * 0.9,
            ambient: mix(p.ambient, [206, 220, 196], 0.6),
            ambient_k: (p.ambient_k + 0.2).min(1.05),
            shadow_k: p.shadow_k * 0.6,
            clouds: p.clouds * 0.5,
            saturation: p.saturation * 1.1,
            depth_fog: p.depth_fog.max(0.1),
            ..p
        },
        _ => p,
    }
}

/// The light for one frame.
pub struct Rig {
    pub p: LightProfile,
    /// Unit vector toward the sun, in scene axes.
    sun: [f64; 3],
    /// The top of the highest surface in each scene cell (the ground is 0).
    tops: Vec<f64>,
    /// Lit fraction of each shadow-map sample, 1 = full sun.
    shadow: Vec<f64>,
    /// Cloud shade over each sample, 0 = clear.
    clouds: Vec<f64>,
    /// The tallest top, so a march can stop once it clears everything.
    highest: f64,
    /// The key light's colour this frame (whitened by a lightning flash) and
    /// its strength, and the ambient light's colour times its strength.
    key: [f64; 3],
    key_k: f64,
    ambient: [f64; 3],
    /// Per cell, a bit for each neighbour (+x, -x, +y, -y) standing taller
    /// than its top (occlusion), and for each sun-facing edge that drops away
    /// (rim light).
    taller: Vec<u8>,
    rim_edges: Vec<u8>,
    /// Whether shadows are cast (not when the architecture is turned).
    cast: bool,
    glow_at: Option<[f64; 3]>,
    glow: f64,
    accent: Rgba,
    fog: Rgba,
    fog_level: f64,
    t: f64,
    wind: f64,
}

/// What a frame's light is built from.
pub struct Setup {
    pub kind: WeatherKind,
    /// The biome adjusts the weather's light (`profile_in`).
    pub biome: crate::plants::Biome,
    /// Scene-space tops: `tops[gy * GRID_W + gx]`, the ground being 0.
    pub tops: Vec<f64>,
    pub cast: bool,
    /// The camera orbit in turns: the sun stays in the world, so it turns
    /// the other way in scene space.
    pub orbit: f64,
    pub glow_at: Option<[f64; 3]>,
    /// The altar's glow from the drone, 0-1.
    pub glow: f64,
    pub accent: Rgba,
    pub fog: Rgba,
    pub fog_level: f64,
    pub flash: f64,
    pub t: f64,
    pub wind: f64,
}

impl Rig {
    pub fn new(setup: Setup) -> Self {
        let p = profile_in(setup.kind, setup.biome);
        let (sx, sy) = p.sun;
        let n = (sx * sx + sy * sy).sqrt().max(1e-9);
        let (sx, sy) = (sx / n, sy / n);
        // Undo the orbit, so the sun keeps its place in the world.
        let (s, c) = (sin_turns(-setup.orbit), cos_turns(-setup.orbit));
        let (sx, sy) = (c * sx - s * sy, s * sx + c * sy);
        let (ce, se) = (cos_turns(p.elevation), sin_turns(p.elevation));
        let sun = [sx * ce, sy * ce, se];
        let mut rig = Rig {
            p,
            sun,
            tops: setup.tops,
            shadow: vec![1.0; SW * SH],
            clouds: vec![0.0; SW * SH],
            highest: 0.0,
            key: {
                let f = setup.flash.clamp(0.0, 1.0);
                [0, 1, 2].map(|k| (p.key[k] as f64 * (1.0 - f) + 250.0 * f) / 255.0)
            },
            key_k: p.key_k + setup.flash * 1.1,
            ambient: [0, 1, 2].map(|k| p.ambient[k] as f64 / 255.0 * p.ambient_k),
            taller: vec![0; (GRID_W * GRID_H) as usize],
            rim_edges: vec![0; (GRID_W * GRID_H) as usize],
            cast: setup.cast,
            glow_at: setup.glow_at,
            glow: setup.glow,
            accent: setup.accent,
            fog: setup.fog,
            fog_level: setup.fog_level,
            t: setup.t,
            wind: setup.wind,
        };
        rig.highest = rig.tops.iter().cloned().fold(0.0, f64::max);
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                let i = (gy * GRID_W as i32 + gx) as usize;
                let here = rig.tops[i];
                let (mut taller, mut rim) = (0u8, 0u8);
                for (k, (dx, dy)) in DIRS.iter().enumerate() {
                    let there = rig.top(gx as f64 + dx, gy as f64 + dy);
                    if there > here + 0.5 {
                        taller |= 1 << k;
                    }
                    if dx * rig.sun[0] + dy * rig.sun[1] > 0.3 && there < here - 0.5 {
                        rim |= 1 << k;
                    }
                }
                rig.taller[i] = taller;
                rig.rim_edges[i] = rim;
            }
        }
        if rig.p.clouds > 0.0 {
            for j in 0..SH {
                for i in 0..SW {
                    let x = (i as f64 + 0.5) / SUB as f64 - 0.5;
                    let y = (j as f64 + 0.5) / SUB as f64 - 0.5;
                    rig.clouds[j * SW + i] = rig.cloud(x, y);
                }
            }
        }
        if rig.cast {
            for j in 0..SH {
                for i in 0..SW {
                    let x = (i as f64 + 0.5) / SUB as f64 - 0.5;
                    let y = (j as f64 + 0.5) / SUB as f64 - 0.5;
                    let z = rig.top(x, y);
                    rig.shadow[j * SW + i] = if rig.occluded(x, y, z, 0.25) { 0.0 } else { 1.0 };
                }
            }
        }
        rig
    }

    /// The surface top at scene point `(x, y)`.
    fn top(&self, x: f64, y: f64) -> f64 {
        let (gx, gy) = (round(x) as i64, round(y) as i64);
        if gx < 0 || gy < 0 || gx >= GRID_W as i64 || gy >= GRID_H as i64 {
            return 0.0;
        }
        self.tops[(gy * GRID_W as i64 + gx) as usize]
    }

    /// Whether the way to the sun from `(x, y, z)` is blocked, marching in
    /// steps of `step` cells up to the shadow's reach.
    fn occluded(&self, x: f64, y: f64, z: f64, step: f64) -> bool {
        let horiz = (self.sun[0] * self.sun[0] + self.sun[1] * self.sun[1]).sqrt().max(1e-9);
        let rise = self.sun[2] / horiz;
        let (dx, dy) = (self.sun[0] / horiz, self.sun[1] / horiz);
        let steps = (self.p.shadow_len / step) as usize;
        for k in 1..=steps {
            let d = k as f64 * step;
            let ray = z + d * rise;
            if ray > self.highest {
                // Above everything there is: nothing further can block it.
                return false;
            }
            if self.top(x + dx * d, y + dy * d) > ray + 0.02 {
                return true;
            }
        }
        false
    }

    /// The lit fraction at scene `(x, y)` on its top surface, bilinear.
    fn sun_on_top(&self, x: f64, y: f64) -> f64 {
        self.sample(&self.shadow, x, y)
    }

    /// A per-sample map at scene `(x, y)`, bilinear.
    fn sample(&self, map: &[f64], x: f64, y: f64) -> f64 {
        let fx = ((x + 0.5) * SUB as f64 - 0.5).clamp(0.0, (SW - 1) as f64);
        let fy = ((y + 0.5) * SUB as f64 - 0.5).clamp(0.0, (SH - 1) as f64);
        let (i, j) = (fx as usize, fy as usize);
        let (i1, j1) = ((i + 1).min(SW - 1), (j + 1).min(SH - 1));
        let (u, v) = (fx - i as f64, fy - j as f64);
        let s = |a: usize, b: usize| map[b * SW + a];
        (s(i, j) * (1.0 - u) + s(i1, j) * u) * (1.0 - v) + (s(i, j1) * (1.0 - u) + s(i1, j1) * u) * v
    }

    /// Slow cloud shadows passing over, 0 (clear) to 1 (shadowed).
    fn cloud(&self, x: f64, y: f64) -> f64 {
        if self.p.clouds <= 0.0 {
            return 0.0;
        }
        let speed = 0.01 + 0.03 * self.wind;
        let m = (0.5 + 0.5 * sin_turns(x * 0.07 + self.t * speed)) * (0.5 + 0.5 * sin_turns(y * 0.05 - self.t * speed * 0.7 + 0.3));
        ((m - 0.3) * 3.0).clamp(0.0, 1.0)
    }

    /// How much light reaches a plant at scene `(x, y)`, relative to full sun:
    /// plants in a column's shadow sit in the same shade as the ground.
    pub fn on_plant(&self, x: f64, y: f64, px: i32, py: i32) -> f64 {
        let ndl = self.sun[2].max(0.0);
        let lit = if self.cast { self.sun_on_top(x, y) } else { 1.0 };
        let lit = dither(lit, 3.0, bayer(px, py)).min(1.0);
        let full = self.p.ambient_k + self.p.key_k * ndl;
        ((self.p.ambient_k + self.p.key_k * ndl * (1.0 - self.p.shadow_k * (1.0 - lit))) / full.max(1e-9)).clamp(0.0, 1.0)
    }

    /// Where the altar glows, in scene space, and how bright (for the halo).
    pub fn glow(&self) -> Option<([f64; 3], f64)> {
        self.glow_at.map(|g| (g, self.glow * self.p.glow))
    }

    /// Light one pixel. `at` is its scene-space point, `normal` its scene-
    /// space normal. The result is before the weather's global grade.
    pub fn shade(&self, s: &Surface, at: [f64; 3], normal: [f64; 3], alb: &Albedo) -> [f64; 3] {
        let t = bayer(s.px, s.py);
        let top = s.face == Face::Top;
        let sun = &self.sun;
        let ndl = (normal[0] * sun[0] + normal[1] * sun[1] + normal[2] * sun[2]).max(0.0);

        // Direct light: facing the sun, not in a cast or cloud shadow.
        let mut lit = 1.0;
        if self.cast && ndl > 0.0 {
            lit = if top {
                self.sun_on_top(at[0], at[1])
            } else {
                let o = [at[0] + normal[0] * 0.04, at[1] + normal[1] * 0.04, at[2]];
                if self.occluded(o[0], o[1], o[2], 0.5) {
                    0.0
                } else {
                    1.0
                }
            };
        }
        if self.p.clouds > 0.0 {
            lit *= 1.0 - self.p.clouds * self.sample(&self.clouds, at[0], at[1]);
        }
        let lit = 1.0 - self.p.shadow_k * (1.0 - dither(lit, 4.0, t).min(1.0));
        let direct = dither(ndl * lit, 6.0, t) * self.key_k;

        // Ambient occlusion: dark at a side's foot, and on a top beside a
        // taller neighbour.
        let mut occl: f64 = 0.0;
        let mut rim = 0.0;
        if top {
            if self.cast {
                let (cx, cy) = (round(at[0]), round(at[1]));
                let cell = if cx >= 0.0 && cy >= 0.0 && cx < GRID_W as f64 && cy < GRID_H as f64 {
                    (cy as usize) * GRID_W as usize + cx as usize
                } else {
                    usize::MAX
                };
                // Only where this top really is the cell's top surface.
                if cell != usize::MAX && (self.tops[cell] - at[2]).abs() < 0.01 {
                    let (taller, edges) = (self.taller[cell], self.rim_edges[cell]);
                    for (k, (dx, dy)) in DIRS.iter().enumerate() {
                        if (taller | edges) >> k & 1 == 0 {
                            continue;
                        }
                        let edge = 0.5 - ((at[0] - cx) * dx + (at[1] - cy) * dy);
                        if taller >> k & 1 == 1 {
                            occl = occl.max(1.0 - (edge / 0.35).clamp(0.0, 1.0));
                        }
                        if edges >> k & 1 == 1 && edge < 0.1 {
                            rim = self.p.rim * lit;
                        }
                    }
                }
            }
        } else {
            occl = 1.0 - at[2].clamp(0.0, 1.0);
        }
        let ao = 1.0 - self.p.ao * 0.55 * dither(occl, 4.0, t).min(1.0);

        let hemi = match s.face {
            Face::Top => 1.0,
            Face::Bottom => 0.5,
            _ => 0.72,
        };
        let key = &self.key;

        // The altar's glow: a point light in the accent colour.
        let mut g = 0.0;
        if let Some(c) = self.glow_at {
            let (dx, dy, dz) = (at[0] - c[0], at[1] - c[1], at[2] - c[2]);
            let d2 = dx * dx + dy * dy + dz * dz;
            if d2 < 3.2 * 3.2 {
                let f = 1.0 - d2.sqrt() / 3.2;
                g = dither(self.glow * self.p.glow * f * f, 5.0, t);
            }
        }

        let mut out = [0.0f64; 3];
        for k in 0..3 {
            let a = alb.colour[k] as f64;
            let amb = self.ambient[k] * hemi * ao;
            let sunlight = key[k] * (direct + rim) * ao.max(0.85);
            let mut v = a * (amb + sunlight);
            // A tint of the glow's colour on what it lights, rather than a
            // wash, so pale stone stays pale.
            v += self.accent[k] as f64 * g * (0.2 + 0.35 * a / 255.0);
            if alb.spec > 0.0 {
                v += key[k] * 255.0 * 0.6 * ndl.max(0.4) * lit;
            }
            if alb.emissive > 0.0 {
                // Runes glow with the drone, and never go fully dark.
                let e = alb.emissive * (0.35 + 0.65 * self.glow) * (0.6 + 0.4 * self.p.glow);
                v = v.max(a * e * 1.2);
            }
            out[k] = v;
        }

        // Fog swallowing the far and the low.
        if self.p.depth_fog > 0.0 {
            let far = ((46.0 - (at[0] + at[1])) / 46.0).clamp(0.0, 1.0);
            let low = 1.0 - (at[2] / 6.0).clamp(0.0, 1.0);
            let f = dither(self.p.depth_fog * self.fog_level * (0.7 * far + 0.3 * low), 5.0, t).min(0.6);
            for k in 0..3 {
                out[k] = out[k] * (1.0 - f) + self.fog[k] as f64 * f;
            }
        }
        out
    }
}

}

// ---- src/material.rs ----
pub mod material {
//! Materials: what the shrine is made of, before any light falls on it.
//!
//! The shrine is a ruin of two eras: **brutalist concrete** — board-formed
//! walls with form-tie holes and water stains, ribbed pillars, slab roofs with
//! a drip lip — poured over and around **weathered stone** — coursed ashlar
//! footings, flagstone floors, rubble. It is **ornate** (carved friezes, inset
//! rune panels, a glyph on the white altar, which glow green-blue) and **ruined** (cracks,
//! chipped edges, moss creeping up from the ground).
//!
//! Every texel is fixed to a face of a block: a face is six texels across per
//! cell and six up per level, so at the 300x300 logical size a texel is about
//! a pixel. The variation comes from hashing (seed, cell, level, texel), so a
//! block's surface never crawls and is the same on every machine (`HC004`).

use crate::framebuffer::{blend, Rgba};
use crate::palette::Palette;
use crate::plants::Biome;
use crate::raster::{Face, Surface};
use crate::rng::hash3;
use crate::shrine::kind;

/// Texels per cell across a face, and per level up a side.
const T: f64 = 6.0;

/// A surface's own colour, and how much of it glows (runes, the glyph).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Albedo {
    pub colour: Rgba,
    /// 0..1: carved runes and the altar glyph, lit by the drone's glow.
    pub emissive: f64,
    /// 0..1: a wet glint that catches the key light.
    pub spec: f64,
}

/// Everything about a block that its texture depends on.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    /// The column's visible kind (its top tile), or `kind::EMPTY` for the
    /// open ground.
    pub kind: u8,
    /// The top tile's variant (0-3).
    pub variant: u8,
    /// Model cell and the model level of the column's top.
    pub gx: i32,
    pub gy: i32,
    pub top: f64,
}

/// Weather that settles on surfaces.
#[derive(Clone, Copy, Debug)]
pub struct Exposure {
    pub cover: f64,
    pub rain: f64,
}

fn scale(c: Rgba, f: f64) -> Rgba {
    let k = |v: u8| (v as f64 * f).clamp(0.0, 255.0) as u8;
    [k(c[0]), k(c[1]), k(c[2]), c[3]]
}

/// How much moss a biome grows on stone.
fn moss_of(biome: Biome) -> f64 {
    match biome {
        Biome::Moss => 0.55,
        Biome::Verdant => 0.38,
        Biome::Fungal => 0.26,
        Biome::NearFuture => 0.04,
        Biome::ResearchLab => 0.0,
        Biome::Ecofuture => 0.18,
        Biome::Greenhouse => 0.12,
    }
}

/// The colours the materials are made from, derived from the palette so the
/// ruin always belongs to its scene.
/// The green-blue of the carved ornament (the altar's glyph and the rune
/// panels), and of the light the altar gives off.
pub const GLYPH: Rgba = [92, 206, 188, 255];

struct Stock {
    concrete: Rgba,
    stone: Rgba,
    flag: Rgba,
    /// Rubble's dark grit.
    dark: Rgba,
    /// The altar's polished white stone.
    marble: Rgba,
    earth: Rgba,
    moss: [Rgba; 2],
    glyph: Rgba,
}

impl Stock {
    fn of(p: &Palette, biome: Biome) -> Self {
        // The ruin is pale: concrete close to a warm off-white, stone a light
        // sandy brown. Each keeps a little of the palette, so it still
        // belongs to its scene.
        let concrete = blend(p.stone_light, [226, 218, 202, 255], 70, 100);
        let moss = match biome {
            Biome::Fungal => [p.foliage_dark, p.accent],
            _ => [p.foliage_dark, p.foliage],
        };
        Stock {
            concrete,
            stone: blend(p.stone, [198, 174, 142, 255], 65, 100),
            flag: blend(blend(p.ground, p.stone, 55, 100), [210, 192, 164, 255], 60, 100),
            dark: blend(scale(p.stone_dark, 0.8), [150, 130, 108, 255], 50, 100),
            marble: blend(p.stone_light, [240, 236, 226, 255], 85, 100),
            earth: p.ground_dark,
            moss,
            glyph: GLYPH,
        }
    }
}

/// The materials of one scene: built once per frame from the palette and
/// biome, then asked for each pixel.
pub struct Materials {
    stock: Stock,
    moss: f64,
    seed: u64,
    /// The built futures have their own materials (`future.rs`).
    future: Option<crate::future::Stock>,
}

impl Materials {
    pub fn new(seed: u64, palette: &Palette, biome: Biome) -> Self {
        let future = biome.futuristic().then(|| crate::future::Stock::of(seed, palette, biome));
        Materials { stock: Stock::of(palette, biome), moss: moss_of(biome), seed, future }
    }

    /// The colour the altar lights its surroundings with.
    pub fn glow(&self) -> Rgba {
        self.future.as_ref().map_or(GLYPH, |f| f.glow)
    }

    /// The colour of one pixel of a block (or of the ground, for
    /// `kind::EMPTY`).
    pub fn albedo(&self, b: &Block, s: &Surface, ex: Exposure) -> Albedo {
        match &self.future {
            Some(f) => crate::future::albedo(f, self.seed, b, s, ex),
            None => albedo(&self.stock, self.moss, self.seed, b, s, ex),
        }
    }
}

/// `n` bits of `h` from `shift`, as a number.
#[inline]
fn bits(h: u64, shift: u32, n: u32) -> u64 {
    (h >> shift) & ((1 << n) - 1)
}

/// Eight bits of `h` from `shift`, as a number in [0, 1).
#[inline]
fn u8f(h: u64, shift: u32) -> f64 {
    bits(h, shift, 8) as f64 / 256.0
}

fn albedo(stock: &Stock, moss: f64, seed: u64, b: &Block, s: &Surface, ex: Exposure) -> Albedo {
    // Three hashes a pixel, sliced into fields: one for the face's band (all
    // its carving and ruin), one for the texel, one for the stone block.
    let cell = ((b.gx as u64 & 0xffff) << 16) | (b.gy as u64 & 0xffff);
    let key = seed ^ 0x4d41_5445 ^ cell.wrapping_mul(0x9E37_79B9);
    let side = !matches!(s.face, Face::Top | Face::Bottom);

    // Face-local texel coordinates.
    let (u, v) = match s.face {
        Face::PosX | Face::NegX => (s.y - b.gy as f64 + 0.5, 0.0),
        Face::PosY | Face::NegY => (s.x - b.gx as f64 + 0.5, 0.0),
        _ => (s.x - b.gx as f64 + 0.5, s.y - b.gy as f64 + 0.5),
    };
    let tu = ((u * T) as i64).clamp(0, T as i64 - 1) as u64;
    // On a side, the level band and the texel row within it (0 at the band's
    // foot); on a top, the second texel axis.
    let band = crate::light::floor(s.z - 1e-6) as i64;
    let tz = if side { (((s.z - band as f64) * T) as i64).clamp(0, T as i64 - 1) as u64 } else { ((v * T) as i64).clamp(0, T as i64 - 1) as u64 };
    let face_id = s.face as u64;
    let band_key = (face_id << 16) | (band as u64 & 0xffff);
    // The band hashes are only needed on the architecture, not the ground.
    let (hb, hb2) = if b.kind == kind::EMPTY { (0, 0) } else { (hash3(key, 1, band_key), hash3(key, 2, band_key)) };
    let ht = hash3(key, 3, band_key << 8 | tu << 4 | tz);
    let speckle = 0.95 + 0.1 * u8f(ht, 0);

    let mut emissive = 0.0;
    let mut spec = 0.0;
    let below_top = b.top - s.z; // levels below the column's top edge

    let mut c = if b.kind == kind::EMPTY {
        // Packed earth with tufts of grass in the biome's colours.
        let patch = u8f(hash3(key, 4, (tu / 3) << 8 | tz / 3), 0);
        if u8f(ht, 8) < 0.22 * patch {
            stock.moss[bits(ht, 16, 1) as usize]
        } else {
            scale(stock.earth, speckle)
        }
    } else if side {
        // The lowest band of every column is the old stone footing; above it,
        // the variant decides between concrete and more stone, by level.
        let variant = bits(hb, 0, 2) as u8 ^ b.variant;
        let footing = s.z < 1.0 && b.kind != kind::ALTAR;
        let mut c = if b.kind == kind::ALTAR {
            // Polished white stone with a band of glowing rune dots.
            if tz == 3 && tu % 2 == 0 && below_top < 1.0 {
                emissive = 0.75;
                stock.glyph
            } else {
                scale(stock.marble, 0.97 + 0.05 * u8f(ht, 24))
            }
        } else if footing || (b.kind == kind::STONE || b.kind == kind::RUBBLE || b.kind == kind::FLOOR) && variant & 1 == 0 {
            ashlar(stock, key, band, tu, tz, speckle)
        } else if b.kind == kind::PILLAR {
            // Bush-hammered ribs, a cast band at the capital and at the base.
            let rib = if tu % 2 == 0 { 1.07 } else { 0.93 };
            let band_f = if below_top < 0.2 { 1.12 } else if s.z < 0.15 { 0.88 } else { 1.0 };
            scale(stock.concrete, rib * band_f * speckle)
        } else {
            board_formed(stock, hb2, tu, tz, speckle, &mut emissive)
        };
        // A slab roof's drip lip: a lit edge and a shadow line beneath it.
        if b.kind == kind::ROOF {
            if below_top < 0.12 {
                c = scale(c, 1.12);
            } else if below_top < 0.26 {
                c = scale(c, 0.82);
            }
        }
        // Water stains streaking down from the top edge of the concrete:
        // about a third of the texel columns, from the band hash.
        if (b.kind == kind::WALL || b.kind == kind::ROOF) && bits(hb, 2 + (tu as u32) * 2, 2) == 0 {
            c = scale(c, 1.0 - 0.2 * (1.0 - below_top / 1.8).clamp(0.0, 1.0));
        }
        // Ornament: a carved frieze on some bands.
        if bits(hb, 14, 8) % 5 == 0 && (2..=3).contains(&tz) && b.kind != kind::ALTAR {
            c = scale(c, if (tu + tz) % 3 == 0 { 0.74 } else { 1.06 });
        }
        // Ruin: a crack wandering down the band, two bits a row.
        if u8f(hb, 22) < 0.35 {
            let mut col = (bits(hb, 30, 8) % 6) as i64;
            for row in (tz..T as u64).rev() {
                col += (bits(hb, 38 + row as u32 * 2, 2) % 3) as i64 - 1;
                col = col.clamp(0, T as i64 - 1);
            }
            if col as u64 == tu {
                c = scale(c, 0.68);
            }
        }
        // Chips off the top edge, a bit per texel column.
        if below_top < 1.0 / T {
            let chip = |t: u64| bits(hb2, 32 + t as u32 * 2, 2) == 0;
            if chip(tu) {
                c = scale(c, 0.72);
            } else if tu > 0 && chip(tu - 1) {
                c = scale(c, 1.14);
            }
        }
        // Moss creeping up from the foot of the column, in patches.
        let patch = 0.15 * u8f(hb2, 48);
        let reach = (moss * (1.0 - s.z / 1.3) + patch).clamp(0.0, 1.0);
        if u8f(ht, 32) < reach * reach {
            c = stock.moss[bits(ht, 40, 1) as usize];
        }
        c
    } else {
        // Tops.
        let c = match b.kind {
            kind::FLOOR | kind::STONE => {
                // Flagstones, 2x2 or 3x2 by variant, with mossy joints.
                let (w, h) = if b.variant & 1 == 0 { (3, 3) } else { (2, 3) };
                let joint = tu % w == 0 || tz % h == 0;
                if joint {
                    if u8f(ht, 24) < moss * 0.8 {
                        stock.moss[0]
                    } else {
                        scale(stock.flag, 0.8)
                    }
                } else {
                    let slab = bits(hb, 0, 8) ^ (tu / w) * 37 ^ (tz / h) * 91;
                    scale(stock.flag, (0.92 + 0.16 * ((slab & 255) as f64 / 256.0)) * speckle)
                }
            }
            kind::RUBBLE => {
                let r = u8f(ht, 24);
                let base = blend(stock.earth, stock.dark, 50, 100);
                scale(base, if r < 0.3 { 0.75 } else if r > 0.85 { 1.18 } else { speckle })
            }
            kind::ALTAR => {
                // A carved glyph: a square ring with a cross through it.
                let ring = (tu == 1 || tu == 4) && (1..=4).contains(&tz) || (tz == 1 || tz == 4) && (1..=4).contains(&tu);
                let cross = tu == tz || tu + tz == 5;
                if ring || cross {
                    emissive = 1.0;
                    stock.glyph
                } else {
                    scale(stock.marble, 0.97 + 0.05 * u8f(ht, 24))
                }
            }
            kind::PILLAR => {
                let rim = tu == 0 || tz == 0 || tu == 5 || tz == 5;
                scale(stock.concrete, if rim { 1.08 } else { 0.96 } * speckle)
            }
            _ => {
                // Concrete slab with expansion joints.
                let joint = tu == 3 || tz == 3;
                scale(stock.concrete, if joint { 0.82 } else { speckle })
            }
        };
        // Moss in patches on stone tops.
        let patch = u8f(hb, 56);
        if b.kind != kind::ALTAR && u8f(ht, 32) < moss * 0.5 * patch * patch {
            stock.moss[1]
        } else {
            c
        }
    };

    // Weather settling on the surface: snow on tops and the top edges of the
    // sides; wet stone darker, with glints on the tops.
    let snow_here = if side { below_top < 1.0 / T || b.kind == kind::EMPTY } else { true };
    if ex.cover > 0.0 && snow_here && u8f(ht, 48) < ex.cover * 1.15 {
        c = blend([238, 242, 252, 255], [210, 222, 246, 255], (u8f(ht, 56) * 60.0) as u32, 100);
        emissive *= 0.3;
    } else if ex.rain > 0.0 {
        c = scale(c, 1.0 - 0.18 * ex.rain);
        if !side && u8f(ht, 56) < 0.06 * ex.rain {
            spec = 1.0;
        }
    }
    Albedo { colour: c, emissive, spec }
}

/// Coursed ashlar: two courses per level, blocks two to a cell, joints
/// offset course by course, each block its own tint.
fn ashlar(stock: &Stock, key: u64, band: i64, tu: u64, tz: u64, speckle: f64) -> Rgba {
    let course = tz / 3;
    // Bands below the ground are negative, so take the parity properly.
    let offset = (course as i64 + band).rem_euclid(2) as u64 * 3;
    let col = tu + offset;
    if tz % 3 == 2 || col % 3 == 0 {
        return scale(stock.stone, 0.8);
    }
    let block = hash3(key, 5, (band as u64 & 0xffff) << 16 | course << 8 | col / 3);
    let tint = 0.88 + 0.22 * u8f(block, 0);
    let warm = blend(stock.stone, stock.earth, (u8f(block, 8) * 22.0) as u32, 100);
    scale(warm, tint * speckle)
}

/// Board-formed concrete: three boards to a level, each with its own grain,
/// a seam under each, and form-tie holes; now and then an inset rune panel.
fn board_formed(stock: &Stock, hb2: u64, tu: u64, tz: u64, speckle: f64, emissive: &mut f64) -> Rgba {
    if bits(hb2, 0, 8) % 9 == 0 && (1..=4).contains(&tu) && (1..=4).contains(&tz) {
        // An inset panel with a rune: sixteen bits of carving, never empty.
        let pattern = (bits(hb2, 8, 16) as u16) | 0x0420;
        let bit = (tu - 1) + (tz - 1) * 4;
        if pattern >> bit & 1 == 1 {
            *emissive = 0.55;
            return stock.glyph;
        }
        return scale(stock.concrete, 0.8);
    }
    let board = tz / 2;
    let grain = 0.95 + 0.1 * (bits(hb2, 24 + board as u32 * 2, 2) as f64 / 3.0);
    let seam = if tz % 2 == 1 { 0.92 } else { 1.0 };
    let tie = tz == 3 && (tu == 1 || tu == 4) && bits(hb2, 30 + tu as u32, 2) != 0;
    let f = if tie { 0.62 } else { grain * seam * speckle };
    scale(stock.concrete, f)
}

}

// ---- src/math.rs ----
pub mod math {
//! Deterministic math.
//!
//! `f64::sin`, `exp`, `tanh`, `powf` etc. are delegated to the platform's libm,
//! whose results differ between operating systems, CPU architectures and WASM
//! engines, and may change between versions. Everything here is built only from
//! `+ - * /`, `floor`, `round` and `sqrt`, which IEEE-754 defines exactly, so
//! results are bit-identical everywhere (`HC004`).
//!
//! Never use `mul_add` in the engine (a fused multiply-add rounds differently).

pub const PI: f64 = core::f64::consts::PI;
pub const TAU: f64 = core::f64::consts::TAU;
const LN_2: f64 = core::f64::consts::LN_2;
const LOG2_E: f64 = core::f64::consts::LOG2_E;
const LOG2_10: f64 = core::f64::consts::LOG2_10;

/// Sine of `t` turns (1 turn = one full cycle = 2π radians).
pub fn sin_turns(t: f64) -> f64 {
    let mut x = t - (t + 0.5).floor();
    if x > 0.25 {
        x = 0.5 - x;
    } else if x < -0.25 {
        x = -0.5 - x;
    }
    let r = x * TAU;
    let r2 = r * r;
    const C: [f64; 9] = [
        -1.0 / 6.0,
        1.0 / 120.0,
        -1.0 / 5040.0,
        1.0 / 362880.0,
        -1.0 / 39916800.0,
        1.0 / 6227020800.0,
        -1.0 / 1307674368000.0,
        1.0 / 355687428096000.0,
        -1.0 / 121645100408832000.0,
    ];
    let mut p = C[8];
    let mut i = 8;
    while i > 0 {
        i -= 1;
        p = C[i] + r2 * p;
    }
    r * (1.0 + r2 * p)
}

/// Cosine of `t` turns.
pub fn cos_turns(t: f64) -> f64 {
    sin_turns(t + 0.25)
}

/// Tangent of `t` turns (only used well inside (-0.25, 0.25)).
pub fn tan_turns(t: f64) -> f64 {
    sin_turns(t) / cos_turns(t)
}

/// 2^x.
pub fn exp2(x: f64) -> f64 {
    if x < -1020.0 {
        return 0.0;
    }
    if x > 1020.0 {
        return f64::MAX;
    }
    let n = x.floor();
    let y = (x - n) * LN_2;
    let mut p = 1.0;
    let mut k = 14;
    while k >= 1 {
        p = 1.0 + p * y / k as f64;
        k -= 1;
    }
    p * f64::from_bits(((n as i64 + 1023) as u64) << 52)
}

/// e^x.
pub fn exp(x: f64) -> f64 {
    exp2(x * LOG2_E)
}

/// Hyperbolic tangent.
pub fn tanh(x: f64) -> f64 {
    if x > 19.0 {
        return 1.0;
    }
    if x < -19.0 {
        return -1.0;
    }
    let e = exp(2.0 * x);
    (e - 1.0) / (e + 1.0)
}

/// Decibels to linear gain.
pub fn db(db: f64) -> f64 {
    exp2(db * LOG2_10 / 20.0)
}

/// MIDI note number to Hz (A4 = 440).
pub fn midi_hz(note: f64) -> f64 {
    440.0 * exp2((note - 69.0) / 12.0)
}

/// Gain applied once every `samples` samples that decays by 60 dB over `seconds`.
pub fn decay_gain(samples: f64, seconds: f64, sample_rate: f64) -> f64 {
    exp2(-3.0 * LOG2_10 * samples / (seconds * sample_rate))
}

/// Per-sample multiplier that decays by 60 dB over `seconds`.
pub fn decay_coef(seconds: f64, sample_rate: f64) -> f64 {
    decay_gain(1.0, seconds, sample_rate)
}

/// Coefficient for a one-pole low-pass `y += a * (x - y)` at `hz`.
pub fn one_pole_coef(hz: f64, sample_rate: f64) -> f64 {
    1.0 - exp(-TAU * hz / sample_rate)
}

/// Equal-power pan: `pan` in [-1, 1] → (left gain, right gain).
pub fn pan_gains(pan: f64) -> (f64, f64) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.125;
    (cos_turns(p), sin_turns(p))
}

/// Replace NaN/inf with 0 and flush very small values (keeps feedback clean).
pub fn sanitize(x: f64) -> f64 {
    if x.is_finite() && x.abs() > 1e-30 {
        x
    } else {
        0.0
    }
}



}

// ---- src/name.rs ----
pub mod name {
//! The track name: a short label chosen in the setup, carried in the recipe,
//! shown at the head of the page and used as the exported WAV's title.
//!
//! Fixed-size and `Copy` so the whole configuration stays `Copy` and the recipe
//! stays a plain array of `u32` words. Only printable ASCII is kept, so the
//! name survives any encoding the browser or a tag reader may apply.

/// The longest name, in bytes (8 recipe words).
pub const CAP: usize = 32;

/// A short, user-chosen track name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; CAP],
    len: u8,
}

impl Name {
    pub const EMPTY: Name = Name { bytes: [0; CAP], len: 0 };

    /// Build from a string, keeping printable ASCII and truncating to [`CAP`].
    pub fn from_str(text: &str) -> Name {
        let mut bytes = [0u8; CAP];
        let mut len = 0usize;
        for ch in text.chars() {
            if len >= CAP {
                break;
            }
            bytes[len] = if ch.is_ascii_graphic() || ch == ' ' { ch as u8 } else { b'?' };
            len += 1;
        }
        Name { bytes, len: len as u8 }
    }

    /// The name as text. Never fails: only printable ASCII is stored.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The eight recipe words holding the name.
    pub fn words(&self) -> [u32; CAP / 4] {
        let mut out = [0u32; CAP / 4];
        for (i, w) in out.iter_mut().enumerate() {
            let b = i * 4;
            *w = u32::from_le_bytes([self.bytes[b], self.bytes[b + 1], self.bytes[b + 2], self.bytes[b + 3]]);
        }
        out
    }

    /// Rebuild from recipe words. Stops at the first NUL and drops anything
    /// that is not printable ASCII, so corrupt input cannot inject control
    /// bytes into a page or a tag.
    pub fn from_words(words: &[u32]) -> Name {
        let mut raw = [0u8; CAP];
        for (i, w) in words.iter().take(CAP / 4).enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        let mut bytes = [0u8; CAP];
        let mut len = 0usize;
        for &c in raw.iter() {
            if c == 0 {
                break;
            }
            if (0x20..=0x7e).contains(&c) {
                bytes[len] = c;
                len += 1;
            }
        }
        Name { bytes, len: len as u8 }
    }
}

impl Default for Name {
    fn default() -> Self {
        Name::EMPTY
    }
}

impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Name({:?})", self.as_str())
    }
}



}

// ---- src/overlay.rs ----
pub mod overlay {
//! The code-overlay setting: the exported artifact source scrolling, waterfall
//! style, over the shrine in the browser.
//!
//! Purely presentational — it never affects audio or the frame — but it is part
//! of the recipe so a built shrine carries its own transparency and speed.

/// The scroll overlay's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeOverlay {
    pub enabled: bool,
    /// Overlay opacity, percent (0-100).
    pub opacity: u8,
    /// Scroll speed, 0-100 (higher is faster).
    pub speed: u8,
    /// Text scale, percent (100 = normal, 200 = double).
    pub scale: u8,
    /// Tilt of the source drift, in degrees. Positive leans to the left and the
    /// text travels along that tilted axis.
    pub angle: i16,
    /// Vertical shift of the whole drift, percent of the canvas height
    /// (negative lifts it). Useful for re-centring a tilted block.
    pub offset: i16,
    /// How the drift behaves as the camera orbits: see [`PLANES`].
    pub plane: u8,
}

impl CodeOverlay {
    /// Smallest and largest text scale the overlay accepts, in percent.
    pub const MIN_SCALE: u8 = 50;
    pub const MAX_SCALE: u8 = 250;
    /// Smallest and largest tilt, in degrees.
    pub const MIN_ANGLE: i16 = -180;
    pub const MAX_ANGLE: i16 = 180;
    /// Smallest and largest vertical shift, in percent of the canvas height.
    pub const MIN_OFFSET: i16 = -100;
    pub const MAX_OFFSET: i16 = 100;
    /// Plane modes, in menu order.
    pub const PLANES: [&'static str; 3] = ["static", "flat", "sphere"];
    pub const OFF: CodeOverlay =
        CodeOverlay { enabled: false, opacity: 0, speed: 50, scale: 200, angle: 0, offset: 0, plane: 0 };
    /// The default: on, very transparent, moderate speed, double size, level,
    /// and a static code plane — the flat and sphere modes are opt-in.
    pub const DEFAULT: CodeOverlay =
        CodeOverlay { enabled: true, opacity: 14, speed: 45, scale: 200, angle: 0, offset: 0, plane: 0 };

    /// Pack for the recipe and the C ABI: bit 0 enabled, then opacity, speed,
    /// and the text scale in the top byte.
    pub fn word(self) -> u32 {
        (self.enabled as u32)
            | ((self.opacity.min(100) as u32) << 8)
            | ((self.speed.min(100) as u32) << 16)
            | ((self.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE) as u32) << 24)
            | (((self.plane as u32) & 0x7) << 1)
    }

    pub fn from_word(word: u32) -> Self {
        // Recipes written before the scale existed leave the top byte zero.
        let raw = ((word >> 24) & 0xff) as u8;
        let scale = if raw == 0 { Self::DEFAULT.scale } else { raw.clamp(Self::MIN_SCALE, Self::MAX_SCALE) };
        CodeOverlay {
            enabled: word & 1 != 0,
            opacity: ((word >> 8) & 0xff).min(100) as u8,
            speed: ((word >> 16) & 0xff).min(100) as u8,
            scale,
            angle: 0,
            offset: 0,
            plane: ((word >> 1) & 0x7).min((Self::PLANES.len() - 1) as u32) as u8,
        }
    }

    /// The layout as its own recipe word: the tilt in bits 0-8 (`0..360`,
    /// offset from `-180`) and the vertical shift in bits 9-17.
    pub fn angle_word(self) -> u32 {
        let angle = (self.angle.clamp(Self::MIN_ANGLE, Self::MAX_ANGLE) + 180) as u32;
        let offset = (self.offset.clamp(Self::MIN_OFFSET, Self::MAX_OFFSET) + 100) as u32;
        (angle & 0x1ff) | ((offset & 0x1ff) << 9)
    }

    /// Read the tilt from a [`angle_word`], tolerating corrupt values.
    pub fn angle_from_word(word: u32) -> i16 {
        (word & 0x1ff).min(360) as i16 - 180
    }

    /// Read the vertical shift from a [`angle_word`].
    pub fn offset_from_word(word: u32) -> i16 {
        ((word >> 9) & 0x1ff).min(200) as i16 - 100
    }

    pub fn summary(self) -> String {
        if self.enabled {
            let tilt = if self.angle == 0 { String::new() } else { format!(" · {angle}°", angle = self.angle) };
            let shift = if self.offset == 0 { String::new() } else { format!(" · {off}%", off = self.offset) };
            let plane = if self.plane == 0 { String::new() } else { format!(" · {}", Self::PLANES[(self.plane as usize).min(2)]) };
            format!("on · {}% · speed {} · text {}%{tilt}{shift}{plane}", self.opacity, self.speed, self.scale)
        } else {
            "off".to_string()
        }
    }
}



}

// ---- src/palette.rs ----
pub mod palette {
//! Procedural palettes.
//!
//! A [`Palette`] is a small set of named roles — sky, fog, ground, stone,
//! foliage, water, accent — derived deterministically from a seed in a muted,
//! overgrown mood. Colours use plain arithmetic only (no library trig), so a
//! seed yields the same palette on every machine (`HC004`).

use crate::framebuffer::{rgb, Rgba};
use crate::rng::Rng;

/// Stream id, so palette draws never collide with another generator's stream.
const STREAM: u64 = 0x5041_4c45; // "PALE"

/// The named colours the scene draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub sky_top: Rgba,
    pub sky_bottom: Rgba,
    pub fog: Rgba,
    pub ground: Rgba,
    pub ground_dark: Rgba,
    pub stone: Rgba,
    pub stone_light: Rgba,
    pub stone_dark: Rgba,
    pub foliage_dark: Rgba,
    pub foliage: Rgba,
    pub foliage_light: Rgba,
    pub water: Rgba,
    pub accent: Rgba,
}

/// `x` wrapped into [0, 1).
pub fn wrap01(x: f64) -> f64 {
    x - x.floor()
}

/// HSL → RGB. `h` is in turns, `s` and `l` in [0, 1].
pub fn hsl(h: f64, s: f64, l: f64) -> Rgba {
    let h = wrap01(h);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let chan = |v: f64| ((v + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    rgb(chan(r), chan(g), chan(b))
}

impl Palette {
    /// Build a palette from an already-positioned RNG stream.
    pub fn from_rng(rng: &mut Rng) -> Self {
        let base = rng.range(0.0, 1.0);
        let sat = rng.range(0.20, 0.42);

        // Sky and fog stay low and desaturated; ground and stone are earthy;
        // foliage is the one green-shifted family; the accent is a soft
        // complementary glow used for the shrine's sigil.
        let foliage_h = wrap01(base + rng.range(0.24, 0.34));
        let stone_l = rng.range(0.34, 0.46);
        let ground_l = rng.range(0.20, 0.30);

        Palette {
            sky_top: hsl(base, sat * 0.55, rng.range(0.07, 0.12)),
            sky_bottom: hsl(wrap01(base + 0.02), sat * 0.65, rng.range(0.16, 0.24)),
            fog: hsl(wrap01(base + 0.01), sat * 0.30, rng.range(0.40, 0.52)),
            ground: hsl(wrap01(base + 0.07), sat * 0.55, ground_l),
            ground_dark: hsl(wrap01(base + 0.07), sat * 0.55, ground_l * 0.55),
            stone: hsl(wrap01(base - 0.05), sat * 0.28, stone_l),
            stone_light: hsl(wrap01(base - 0.05), sat * 0.28, (stone_l + 0.16).min(0.86)),
            stone_dark: hsl(wrap01(base - 0.05), sat * 0.28, (stone_l - 0.16).max(0.08)),
            foliage_dark: hsl(foliage_h, (sat * 1.1).min(0.60), 0.16),
            foliage: hsl(foliage_h, (sat * 1.1).min(0.60), 0.28),
            foliage_light: hsl(wrap01(foliage_h + 0.02), (sat * 1.1).min(0.60), 0.42),
            water: hsl(wrap01(base + 0.50), sat * 0.50, 0.34),
            accent: hsl(wrap01(base + 0.50), (sat * 1.5).min(0.80), rng.range(0.55, 0.66)),
        }
    }

    /// Build a palette for a seed.
    pub fn from_seed(seed: u64) -> Self {
        Palette::from_rng(&mut Rng::stream(seed, STREAM))
    }

    /// The palette for a seed in a biome: the seed's own for the wild
    /// biomes, a future's colours for the built ones.
    pub fn for_biome(seed: u64, biome: crate::plants::Biome) -> Self {
        let p = Palette::from_seed(seed);
        if biome.futuristic() {
            crate::future::palette(seed, p, biome)
        } else {
            p
        }
    }

    /// Perceived luminance in [0, 255]; used for contrast guarantees.
    pub fn luminance(c: Rgba) -> f64 {
        0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64
    }
}

}

// ---- src/plants.rs ----
pub mod plants {
//! The plant library: a set of generators (grass, fern, vine, fungus) that
//! share one output shape, plus the rules that scatter them around the shrine.
//!
//! A plant is stored as a handful of **strokes** — polylines in a local frame
//! anchored to a grid cell, `x` to the right and `y` up, in fixed-point units.
//! A [`Sway`] describes how it bends in the weather. Nothing here draws; the
//! scene assembly (`FEAT-10`) turns strokes into pixels, so the same plant data
//! can drive the editor preview and the final render.
//!
//! A biome biases which generators are used; the default set is chosen at
//! setup. Everything is seeded per cell, so adding a plant family later cannot
//! shift another cell's plant.

use crate::math::sin_turns;
use crate::rng::Rng;
use crate::shrine::Shrine;
use crate::{GRID_H, GRID_W};

/// Stream id, so plants never share a sequence with another generator.
const STREAM: u64 = 0x504c_4e54; // "PLNT"

/// The most plants the scene will ever hold, for a bounded frame budget.
pub const MAX_PLANTS: usize = 260;

/// Plant families.
pub mod kind {
    pub const GRASS: u8 = 0;
    pub const FERN: u8 = 1;
    pub const VINE: u8 = 2;
    pub const FUNGUS: u8 = 3;
}

/// Which palette role a stroke draws with.
pub mod role {
    pub const DARK: u8 = 0;
    pub const MID: u8 = 1;
    pub const LIGHT: u8 = 2;
    pub const ACCENT: u8 = 3;
}

/// A biome biases the mixture of plant families. The first three are the
/// ruin's own wild places; the last four are futures built around it, each
/// with its own palette, light, ground, props and (for the roofed two) a
/// ceiling the weather lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Moss,
    Verdant,
    Fungal,
    /// A bright near-future city block: roads, sidewalks, signage.
    NearFuture,
    /// Steel, glass, blue-green neon and screens, under a panelled ceiling.
    ResearchLab,
    /// Parkland and white titanium machines.
    Ecofuture,
    /// A sunlit glass atrium full of plants, on minimal steel scaffolding.
    Greenhouse,
}

impl Biome {
    pub const ALL: [Biome; 7] = [
        Biome::Moss,
        Biome::Verdant,
        Biome::Fungal,
        Biome::NearFuture,
        Biome::ResearchLab,
        Biome::Ecofuture,
        Biome::Greenhouse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Biome::Moss => "Moss",
            Biome::Verdant => "Verdant",
            Biome::Fungal => "Fungal",
            Biome::NearFuture => "Near Future",
            Biome::ResearchLab => "Research Lab",
            Biome::Ecofuture => "Ecofuture",
            Biome::Greenhouse => "Greenhouse",
        }
    }

    /// A one-line sketch, for the setup screen and the Learn page.
    pub fn blurb(self) -> &'static str {
        match self {
            Biome::Moss => "an old ruin softened by moss, ferns and grass",
            Biome::Verdant => "a lush ruin, overgrown with leaves and flowers",
            Biome::Fungal => "a damp ruin of mushrooms and glowing caps",
            Biome::NearFuture => "a bright city block: roads, signs, a vending machine",
            Biome::ResearchLab => "steel, glass and neon screens, under a ceiling",
            Biome::Ecofuture => "parkland with white machines and solar tiles",
            Biome::Greenhouse => "a glass atrium of plants on a steel frame",
        }
    }

    /// The biome called `name`, ignoring case, spaces, `-` and `_`
    /// ("near-future", "NearFuture" and "Near Future" all match).
    pub fn from_name(name: &str) -> Option<Biome> {
        let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect::<String>();
        let want = key(name);
        Biome::ALL.into_iter().find(|b| key(b.name()) == want)
    }

    /// One of the four built futures (not the ruin's wild biomes).
    pub fn futuristic(self) -> bool {
        matches!(self, Biome::NearFuture | Biome::ResearchLab | Biome::Ecofuture | Biome::Greenhouse)
    }

    /// Under a ceiling: rain and snow land on the roof, not the ground.
    pub fn roofed(self) -> bool {
        matches!(self, Biome::ResearchLab | Biome::Greenhouse)
    }

    /// Relative weights for grass, fern, vine, fungus.
    fn weights(self) -> [f64; 4] {
        match self {
            Biome::Moss => [3.0, 1.0, 2.0, 1.5],
            Biome::Verdant => [2.0, 3.0, 1.0, 0.4],
            Biome::Fungal => [1.0, 1.0, 2.5, 4.0],
            // Verge grass and the odd weed through the paving.
            Biome::NearFuture => [4.0, 0.6, 1.0, 0.6],
            // Potted grasses and ferns; a little lab culture.
            Biome::ResearchLab => [2.0, 2.0, 0.5, 0.6],
            Biome::Ecofuture => [3.0, 2.5, 1.5, 0.3],
            Biome::Greenhouse => [1.5, 3.5, 3.0, 0.5],
        }
    }

    /// Overall plant density on open ground.
    fn ground_density(self) -> f64 {
        match self {
            Biome::Moss => 0.55,
            Biome::Verdant => 0.65,
            Biome::Fungal => 0.40,
            Biome::NearFuture => 0.14,
            Biome::ResearchLab => 0.12,
            Biome::Ecofuture => 0.62,
            Biome::Greenhouse => 0.78,
        }
    }
}

/// How a plant bends: `phase` in turns, `amplitude` in fixed-point units at the
/// tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sway {
    pub phase: f64,
    pub amplitude: f64,
}

/// A polyline in the plant's local frame (`y` up, fixed-point units).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub points: Vec<(i32, i32)>,
    pub width: u8,
    pub role: u8,
}

/// One generated plant.
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub kind: u8,
    pub gx: i32,
    pub gy: i32,
    /// Stack level the plant grows from (a shrine top, or 0 on the ground).
    pub z: i32,
    /// Sub-cell placement, fixed-point units.
    pub offset: (i32, i32),
    /// Nominal height in fixed-point units.
    pub height: i32,
    pub sway: Sway,
    pub strokes: Vec<Stroke>,
}

/// All the plants in a scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plants {
    pub items: Vec<Plant>,
}

impl Plants {
    /// Scatter plants around `shrine` for `seed` in `biome`.
    pub fn generate(seed: u64, shrine: &Shrine, biome: Biome) -> Self {
        let (ox, oy) = shrine.origin();
        let mut items = Vec::new();
        // The futures grow plants only where their layout leaves soil.
        let plan = biome.futuristic().then(|| crate::layout::Plan::new(seed, biome));

        for gy in 0..GRID_H as i32 {
            if items.len() >= MAX_PLANTS {
                break;
            }
            for gx in 0..GRID_W as i32 {
                if items.len() >= MAX_PLANTS {
                    break;
                }
                let inside = gx >= ox && gx < ox + crate::SHRINE_W as i32
                    && gy >= oy && gy < oy + crate::SHRINE_H as i32;
                if !inside && plan.is_some_and(|p| !p.cell(gx, gy).grows()) {
                    continue;
                }
                let density = if inside { biome.ground_density() * 0.18 } else { biome.ground_density() };
                let mut rng = Rng::stream(seed, STREAM ^ cell(gx, gy));
                if !rng.chance(density) {
                    continue;
                }
                let z = if inside { shrine.height_at(gx - ox, gy - oy) as i32 - 1 } else { 0 };
                if z < 0 {
                    continue;
                }
                items.push(grow(&mut rng, biome, gx, gy, z));
            }
        }

        Plants { items }
    }

    pub fn count_of(&self, kind: u8) -> usize {
        self.items.iter().filter(|p| p.kind == kind).count()
    }

    /// A stable hash of every plant, for golden tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        mix(self.items.len() as u64, &mut h);
        for p in &self.items {
            mix(p.kind as u64, &mut h);
            mix(p.gx as u64, &mut h);
            mix(p.gy as u64, &mut h);
            mix(p.z as u64, &mut h);
            mix(p.strokes.len() as u64, &mut h);
            for s in &p.strokes {
                mix(s.width as u64, &mut h);
                mix(s.role as u64, &mut h);
                for &(x, y) in &s.points {
                    mix(x as u64, &mut h);
                    mix(y as u64, &mut h);
                }
            }
        }
        h
    }
}

/// FNV-1a step, shared by the hashes.
fn mix(v: u64, h: &mut u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x0000_0100_0000_01b3);
}

fn cell(gx: i32, gy: i32) -> u64 {
    ((gx as u64) << 32) | (gy as u32 as u64)
}

/// Build one plant, choosing a family by the biome's weights.
fn grow(rng: &mut Rng, biome: Biome, gx: i32, gy: i32, z: i32) -> Plant {
    let weights = biome.weights();
    let family = rng.weighted(&weights) as u8;
    let scale = rng.range(0.8, 1.25);
    let offset = (rng.range(-28.0, 28.0) as i32, rng.range(-14.0, 14.0) as i32);
    let (strokes, height) = match family {
        kind::GRASS => grass(rng, scale),
        kind::FERN => fern(rng, scale),
        kind::VINE => vine(rng, scale),
        _ => fungus(rng, scale),
    };
    Plant {
        kind: family,
        gx,
        gy,
        z,
        offset,
        height,
        sway: Sway { phase: rng.unit(), amplitude: rng.range(6.0, 22.0) },
        strokes,
    }
}

fn grass(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let blades = rng.int(3, 6);
    let height = rng.range(26.0, 54.0) * scale;
    let mut strokes = Vec::with_capacity(blades as usize);
    for _ in 0..blades {
        let dx = rng.range(-16.0, 16.0);
        let tip = height * rng.range(0.7, 1.1);
        let role = if rng.chance(0.5) { role::MID } else { role::LIGHT };
        strokes.push(Stroke {
            points: vec![(0, 0), ((dx * 0.6) as i32, (tip * 0.6) as i32), ((dx * 1.3) as i32, tip as i32)],
            width: 1,
            role,
        });
    }
    (strokes, height as i32)
}

fn fern(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(70.0, 150.0) * scale;
    let mut strokes = vec![Stroke {
        points: vec![(0, 0), (0, (height * 0.5) as i32), (rng.range(-6.0, 6.0) as i32, height as i32)],
        width: 1,
        role: role::DARK,
    }];
    let pairs = rng.int(3, 5);
    for i in 0..pairs {
        let y = height * (0.28 + 0.15 * i as f64);
        let len = rng.range(22.0, 42.0);
        let droop = rng.range(-6.0, 10.0);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        strokes.push(Stroke {
            points: vec![(0, y as i32), ((side * len) as i32, (y + droop) as i32)],
            width: 1,
            role: if i % 2 == 0 { role::MID } else { role::LIGHT },
        });
    }
    (strokes, height as i32)
}

fn vine(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(130.0, 300.0) * scale;
    let amplitude = rng.range(8.0, 20.0);
    let waves = rng.range(0.75, 1.6);
    let phase = rng.unit();
    // Anchor the base at (0, 0) by removing the wave's value at t = 0.
    let base = sin_turns(phase);
    let xs = |t: f64| amplitude * (sin_turns(t * waves + phase) - base);
    let steps = 9;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        points.push((xs(t) as i32, (t * height) as i32));
    }
    let mut strokes = vec![Stroke { points, width: 1, role: role::DARK }];
    let leaves = rng.int(3, 6);
    for _ in 0..leaves {
        let t = rng.range(0.2, 0.95);
        let y = (t * height) as i32;
        let x = xs(t) as i32;
        let len = rng.range(12.0, 26.0);
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        strokes.push(Stroke {
            points: vec![(x, y), (x + (side * len) as i32, y + rng.range(-6.0, 6.0) as i32)],
            width: 1,
            role: if rng.chance(0.5) { role::MID } else { role::LIGHT },
        });
    }
    (strokes, height as i32)
}

fn fungus(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(30.0, 64.0) * scale;
    let cap = rng.range(10.0, 22.0);
    let mut strokes = vec![Stroke {
        points: vec![(0, 0), (0, (height * 0.7) as i32), (0, height as i32)],
        width: 1,
        role: role::DARK,
    }];
    strokes.push(Stroke {
        points: vec![(-cap as i32, height as i32), (cap as i32, height as i32)],
        width: 3,
        role: role::ACCENT,
    });
    (strokes, height as i32)
}

}

// ---- src/player.rs ----
pub mod player {
//! A live shrine instance: the simulation, its reusable buffers and the audio
//! clock.
//!
//! This is the shared core of the C ABI. [`crate::wasm`] only wraps it in
//! `extern "C"` exports for the development page; the emitted artifact supplies
//! its own baked entry point over the same `Player`, so both share one runtime.

use crate::framebuffer::Framebuffer;
use crate::scene::{Camera, Quality};
use crate::sim::Sim;
use crate::SAMPLE_RATE;

/// A running shrine: one simulation plus the buffers the ABI hands to JS.
pub struct Player {
    sim: Sim,
    fb: Framebuffer,
    buf: Vec<f32>,
    camera: Camera,
    transparent_sky: bool,
    last_sky: ([u8; 4], [u8; 4]),
    /// The Learn page's description (built once, on first ask) and live state.
    learn: Option<String>,
    learn_state: Vec<f64>,
    weather: [f64; 6],
    point: [f64; 3],
}

impl Player {
    pub fn new(sim: Sim) -> Self {
        let last_sky = sim.scene.sky_colors();
        Player {
            sim,
            fb: Framebuffer::logical(),
            buf: Vec::new(),
            camera: Camera::default(),
            transparent_sky: false,
            last_sky,
            learn: None,
            learn_state: vec![0.0; crate::learn::STATE_LEN],
            weather: [0.0; 6],
            point: [0.0; 3],
        }
    }

    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    /// Leave the sky unfilled so a page can draw its own (and put the source
    /// waterfall in it, behind the shrine).
    pub fn set_sky_transparent(&mut self, on: bool) {
        self.transparent_sky = on;
    }

    /// The lit sky gradient endpoints from the last [`render_visual`], packed
    /// `0xRRGGBB` top in the low 24 bits and bottom in the high 24. The page
    /// draws its own sky with these, so it matches the engine exactly
    /// (including the weather's lighting).
    pub fn sky_colors(&self) -> u64 {
        let pack = |c: [u8; 4]| (c[0] as u64) << 16 | (c[1] as u64) << 8 | c[2] as u64;
        pack(self.last_sky.0) | pack(self.last_sky.1) << 24
    }

    /// Nudge the camera from the pointer, in `[-1, 1]` on each axis.
    pub fn set_camera(&mut self, yaw: f64, pitch: f64) {
        self.camera = Camera { yaw: yaw.clamp(-1.0, 1.0), pitch: pitch.clamp(-1.0, 1.0) };
    }

    /// Everything the Learn page explains about this shrine, as UTF-8 JSON.
    /// Built on first ask (it sets up the music engine once, without playing it).
    pub fn learn_json(&mut self) -> &[u8] {
        let config = &self.sim.config;
        self.learn.get_or_insert_with(|| crate::learn::json(config)).as_bytes()
    }

    /// The music's moving parts now (see `learn::STATE_LEN`); all zero for a
    /// visuals-only instance.
    pub fn learn_state(&mut self) -> &[f64] {
        if let Some(track) = self.sim.track_mut() {
            crate::learn::state(track, &mut self.learn_state);
        }
        &self.learn_state
    }

    /// The weather at `sample_clock`, for the Learn page: (wind, gust, rain,
    /// snow, fog, light), each 0..1.
    pub fn weather_now(&mut self, sample_clock: u64) -> &[f64] {
        let w = self.sim.state(self.time(sample_clock)).weather;
        self.weather = [w.wind_strength, w.gust, w.rain, w.snow, w.fog, w.light];
        &self.weather
    }

    /// Where grid cell `(gx, gy)`'s top surface is on screen at
    /// `sample_clock`, in logical pixels, and its level (x = −1 off the grid).
    pub fn surface_point(&mut self, gx: i32, gy: i32, sample_clock: u64) -> &[f64] {
        let t = self.time(sample_clock);
        let mut state = self.sim.state(t);
        state.camera = self.camera;
        let (x, y, z) = self.sim.scene.surface_point(&state, gx, gy).unwrap_or((-1.0, -1.0, 0.0));
        self.point = [x, y, z];
        &self.point
    }

    /// Hear one layer alone: 1.. = `learn::SOLOS`, 0 = the full mix.
    pub fn set_solo(&mut self, n: u32) {
        let solo = (n as usize).checked_sub(1).and_then(|i| crate::learn::SOLOS.get(i).copied());
        self.sim.set_solo(solo);
    }

    /// Render `frames` stereo samples and return the interleaved buffer.
    pub fn render_audio(&mut self, frames: usize) -> &[f32] {
        self.buf.resize(frames * 2, 0.0);
        self.sim.render_audio(&mut self.buf);
        &self.buf
    }

    /// Seconds into the piece at `sample_clock` samples of playback.
    fn time(&self, sample_clock: u64) -> f64 {
        sample_clock as f64 / SAMPLE_RATE as f64 + self.sim.start_offset()
    }

    /// Render one 300x300 RGBA frame at `sample_clock` samples.
    pub fn render_visual(&mut self, sample_clock: u64) -> &[u8] {
        // At the start offset plus the clock, in step with the audio (which
        // was fast-forwarded there), as `Sim::render_visual` does.
        let t = self.time(sample_clock);
        let mut state = self.sim.state(t);
        state.camera = self.camera;
        state.draw_sky = !self.transparent_sky;
        self.last_sky = self.sim.scene.render_state(&mut self.fb, &state, Quality::High);
        self.fb.as_bytes()
    }
}

}

// ---- src/props.rs ----
pub mod props {
//! The props of the built futures: the street furniture, machines and
//! fittings that make each one a place.
//!
//! - **Near Future**: street lamps along the curbs, utility poles strung with
//!   sagging wires, traffic signals at the junctions, vending machines,
//!   bollards, small parked cars.
//! - **Research Lab**: server racks with blinking lights, terminals with
//!   glowing screens, glass partitions.
//! - **Ecofuture**: white wind turbines turning with the wind, sensor masts,
//!   hydroponic pods, round trees, a drone hovering over its pad.
//! - **Greenhouse**: tree ferns, planters, misting heads breathing vapour,
//!   baskets hanging from the roof.
//!
//! Where they stand comes from the layout (`layout.rs`) and the seed; what
//! they look like at a moment is a pure function of time and the weather, so
//! any frame can be drawn on its own. The wild biomes have none.

use crate::framebuffer::{blend, Framebuffer, Rgba};
use crate::iso::{to_px_round, Point};
use crate::layout::{cell_hash, ring, Ground, Plan, Side, ORIGIN};
use crate::math::{cos_turns, sin_turns};
use crate::plants::Biome;
use crate::raster;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    StreetLamp,
    UtilityPole,
    Signal,
    Vending,
    Bollard,
    Car,
    Rack,
    Terminal,
    Partition,
    Turbine,
    Mast,
    Pod,
    Tree,
    DronePad,
    TreeFern,
    Planter,
    Mister,
    Basket,
}

/// One prop: what, where (its centre on the plateau), which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub kind: Kind,
    pub gx: i32,
    pub gy: i32,
    pub x: f64,
    pub y: f64,
    /// Whether it lines up with the y axis (a car on a road along y, a rack
    /// in a row along y).
    pub along_y: bool,
    pub hash: u64,
}

/// Every prop of a scene, and the wires between the utility poles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    pub items: Vec<Prop>,
    /// Pairs of utility poles strung together (indices into `items`).
    pub wires: Vec<(usize, usize)>,
}

impl Props {
    pub fn generate(seed: u64, biome: Biome) -> Self {
        let mut items = Vec::new();
        if !biome.futuristic() {
            return Props::default();
        }
        let plan = Plan::new(seed, biome);
        let mut turbines = 0;
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                let r = ring(gx, gy);
                if r == 0 {
                    continue;
                }
                let h = cell_hash(seed ^ 0x5052_4f50, gx, gy); // "PROP"
                let g = plan.cell(gx, gy);
                let jitter = |k: u32| ((h >> k & 0xff) as f64 / 255.0 - 0.5) * 0.3;
                let corner = (gx < ORIGIN.0 || gx >= ORIGIN.0 + SHRINE_W as i32) && (gy < ORIGIN.1 || gy >= ORIGIN.1 + SHRINE_H as i32);
                let side_along_y = gx < ORIGIN.0 || gx >= ORIGIN.0 + SHRINE_W as i32;
                // How far along its side a ring cell is.
                let along = if side_along_y { gy } else { gx };
                let mut put = |kind: Kind, x: f64, y: f64, along_y: bool| items.push(Prop { kind, gx, gy, x, y, along_y, hash: h });
                let (cx, cy) = (gx as f64, gy as f64);
                match biome {
                    Biome::NearFuture => match g {
                        Ground::Sidewalk { curb: Some(side) } => {
                            // At the curb: lamps every four cells, bollards between.
                            let (ex, ey) = match side {
                                Side::West => (-0.36, 0.0),
                                Side::East => (0.36, 0.0),
                                Side::North => (0.0, -0.36),
                                Side::South => (0.0, 0.36),
                            };
                            if along % 4 == 1 {
                                put(Kind::StreetLamp, cx + ex, cy + ey, side.along_y());
                            } else if along % 4 == 3 && h % 3 == 0 {
                                put(Kind::Bollard, cx + ex, cy + ey, side.along_y());
                            }
                        }
                        Ground::Sidewalk { curb: None } if r == 1 => {
                            if corner {
                                put(Kind::Signal, cx, cy, false);
                            } else if h % 7 == 0 {
                                put(Kind::Vending, cx + jitter(8), cy + jitter(16), side_along_y);
                            }
                        }
                        Ground::Sidewalk { curb: None } if r == 4 && !corner && along % 5 == 2 => {
                            put(Kind::UtilityPole, cx, cy, side_along_y);
                        }
                        Ground::Road { along_y, lane: false, crossing: false } if h % 6 == 0 => {
                            put(Kind::Car, cx, cy, along_y);
                        }
                        _ => {}
                    },
                    Biome::ResearchLab => match g {
                        Ground::Floor { strip: false } if r >= 2 => match h % 7 {
                            0 | 1 => put(Kind::Rack, cx, cy, side_along_y),
                            2 => put(Kind::Terminal, cx + jitter(8), cy + jitter(16), side_along_y),
                            3 if r == 2 => put(Kind::Partition, cx, cy, side_along_y),
                            _ => {}
                        },
                        _ => {}
                    },
                    Biome::Ecofuture => match g {
                        Ground::Lawn if r >= 2 => match h % 11 {
                            0 if turbines < 3 && r == 4 => {
                                turbines += 1;
                                put(Kind::Turbine, cx, cy, side_along_y);
                            }
                            1 | 2 => put(Kind::Tree, cx + jitter(8), cy + jitter(16), false),
                            3 => put(Kind::Mast, cx, cy, false),
                            4 => put(Kind::Pod, cx + jitter(8), cy + jitter(16), side_along_y),
                            _ => {}
                        },
                        Ground::Lawn if r == 4 && corner && gx == gy => put(Kind::DronePad, cx, cy, false),
                        _ => {}
                    },
                    Biome::Greenhouse => match g {
                        Ground::Bed if h % 3 == 0 => put(Kind::TreeFern, cx + jitter(8), cy + jitter(16), false),
                        Ground::Bed if h % 3 == 1 && along % 3 == 0 => put(Kind::Mister, cx, cy, false),
                        Ground::Tile if r == 4 && h % 4 == 0 => put(Kind::Planter, cx, cy, side_along_y),
                        Ground::Gravel if h % 5 == 0 => put(Kind::Basket, cx + jitter(8), cy + jitter(16), false),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        // String each utility pole to the next one along the same side.
        let mut wires = Vec::new();
        for (i, a) in items.iter().enumerate() {
            if a.kind != Kind::UtilityPole {
                continue;
            }
            let next = items.iter().enumerate().filter(|(j, b)| {
                *j != i && b.kind == Kind::UtilityPole && b.along_y == a.along_y && {
                    let (da, db) = if a.along_y { (a.gx - b.gx, b.gy - a.gy) } else { (a.gy - b.gy, b.gx - a.gx) };
                    da == 0 && db > 0 && db <= 6
                }
            });
            if let Some((j, _)) = next.min_by_key(|(_, b)| if a.along_y { b.gy } else { b.gx }) {
                wires.push((i, j));
            }
        }
        Props { items, wires }
    }

    /// The props standing in cell `(gx, gy)`, by index.
    pub fn in_cell(&self, gx: i32, gy: i32) -> impl Iterator<Item = usize> + '_ {
        self.items.iter().enumerate().filter(move |(_, p)| p.gx == gx && p.gy == gy).map(|(i, _)| i)
    }

    /// Whether any prop stands in cell `(gx, gy)`.
    pub fn any_in(&self, gx: i32, gy: i32) -> bool {
        self.items.iter().any(|p| p.gx == gx && p.gy == gy)
    }

    /// A stable hash of every prop, for tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for p in &self.items {
            for v in [p.kind as u64, p.gx as u64, p.gy as u64, p.hash] {
                h ^= v;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// What drawing a prop needs from the frame.
pub struct Ctx<'a> {
    /// A point on the plateau (`x`, `y`) at height `z` levels, on screen.
    pub proj: &'a dyn Fn(f64, f64, f64) -> Point,
    /// The weather's light and grade, for a colour.
    pub lit: &'a dyn Fn(Rgba) -> Rgba,
    pub t: f64,
    /// Wind strength, 0-1 (turns the turbines).
    pub wind: f64,
    /// How dark it is, 0-1 (lamps glow more).
    pub night: f64,
    /// The light that reaches the prop, relative to full sun (shadows).
    pub light: f64,
    /// The roof, for things hanging from it.
    pub roof: Option<&'a crate::roof::Roof>,
}

fn shade(c: Rgba, f: f64) -> Rgba {
    let k = |v: u8| (v as f64 * f).clamp(0.0, 255.0) as u8;
    [k(c[0]), k(c[1]), k(c[2]), 255]
}

impl Ctx<'_> {
    fn px(&self, x: f64, y: f64, z: f64) -> (i32, i32) {
        let p = (self.proj)(x, y, z);
        (to_px_round(p.x), to_px_round(p.y))
    }

    /// A lit surface colour, a face of it turned `face` (1 = top).
    fn surface(&self, c: Rgba, face: f64) -> Rgba {
        shade((self.lit)(c), face * (0.55 + 0.45 * self.light))
    }

    /// A light: lit from within, never darker than itself.
    fn lamp(&self, c: Rgba) -> Rgba {
        let g = (self.lit)(c);
        [0, 1, 2, 3].map(|k| if k == 3 { 255 } else { g[k].max((c[k] as f64 * 0.9) as u8) })
    }

    /// A solid box from `(x0, y0, z0)` to `(x1, y1, z1)`; `front`, when set,
    /// colours the face toward +x (or +y when `along_y`).
    fn cuboid(&self, fb: &mut Framebuffer, a: (f64, f64, f64), b: (f64, f64, f64), c: Rgba) {
        self.cuboid_with(fb, a, b, c, None);
    }

    fn cuboid_with(&self, fb: &mut Framebuffer, (x0, y0, z0): (f64, f64, f64), (x1, y1, z1): (f64, f64, f64), c: Rgba, front: Option<(bool, Rgba)>) {
        let p = |x, y, z| (self.proj)(x, y, z);
        // Each face's corners wind counter-clockwise seen from outside, so a
        // face is toward the viewer when it winds the same way as the top.
        let faces: [([Point; 4], f64, u8); 5] = [
            ([p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)], 1.0, 0),
            ([p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)], 0.8, 1),
            ([p(x0, y1, z0), p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1)], 0.7, 2),
            ([p(x1, y1, z0), p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1)], 0.64, 3),
            ([p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)], 0.56, 4),
        ];
        let area = |q: &[Point; 4]| {
            let mut a = 0i64;
            for i in 0..4 {
                let (u, v) = (q[i], q[(i + 1) % 4]);
                a += u.x as i64 * v.y as i64 - v.x as i64 * u.y as i64;
            }
            a
        };
        let up = area(&faces[0].0).signum();
        for (q, f, id) in faces.iter() {
            let a = area(q);
            if a == 0 || a.signum() != up {
                continue;
            }
            let colour = match front {
                Some((along_y, fc)) if (*id == 1 && !along_y) || (*id == 3 && along_y) => fc,
                _ => self.surface(c, *f),
            };
            raster::fill_polygon(fb, q, colour);
        }
    }

    /// A thin upright post (a pole, a mast, a trunk).
    fn post(&self, fb: &mut Framebuffer, x: f64, y: f64, z0: f64, z1: f64, r: f64, c: Rgba) {
        self.cuboid(fb, (x - r, y - r, z0), (x + r, y + r, z1), c);
    }

    fn line(&self, fb: &mut Framebuffer, a: (f64, f64, f64), b: (f64, f64, f64), c: Rgba) {
        let (p, q) = (self.px(a.0, a.1, a.2), self.px(b.0, b.1, b.2));
        raster::line(fb, p.0, p.1, q.0, q.1, c);
    }

    /// A soft dithered glow around a light, stronger in the dark.
    fn halo(&self, fb: &mut Framebuffer, at: (f64, f64, f64), c: Rgba, r: i32, k: f64) {
        let (cx, cy) = self.px(at.0, at.1, at.2);
        let k = k * (0.25 + 0.75 * self.night);
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f64).sqrt() / r as f64;
                if d >= 1.0 {
                    continue;
                }
                let a = crate::light::dither(k * (1.0 - d) * (1.0 - d), 4.0, crate::light::bayer(x, y));
                fb.blend_over(x, y, c, (a * 110.0) as u32);
            }
        }
    }
}

const STEEL: Rgba = [150, 156, 164, 255];
const DARK: Rgba = [58, 62, 70, 255];
const WHITE: Rgba = [238, 240, 242, 255];
const WARM: Rgba = [255, 226, 160, 255];

/// Draw prop `i` of `props`.
pub fn draw(fb: &mut Framebuffer, props: &Props, i: usize, cx: &Ctx, accent: Rgba, foliage: [Rgba; 3]) {
    let p = &props.items[i];
    let (x, y) = (p.x, p.y);
    let h = p.hash;
    // Its own long and short axes.
    let (lx, ly) = if p.along_y { (0.0, 1.0) } else { (1.0, 0.0) };
    match p.kind {
        Kind::StreetLamp => {
            cx.post(fb, x, y, 0.0, 3.0, 0.05, STEEL);
            // An arm reaching out over the road, and the lamp at its end.
            let (ax, ay) = (x + 0.0, y + 0.0);
            let (dx, dy) = if p.along_y { (if x < ORIGIN.0 as f64 { -0.5 } else { 0.5 }, 0.0) } else { (0.0, if y < ORIGIN.1 as f64 { -0.5 } else { 0.5 }) };
            cx.line(fb, (ax, ay, 3.0), (ax + dx, ay + dy, 3.1), cx.surface(STEEL, 0.9));
            let head = (ax + dx, ay + dy, 3.0);
            cx.cuboid(fb, (head.0 - 0.1, head.1 - 0.1, 2.92), (head.0 + 0.1, head.1 + 0.1, 3.05), cx.lamp(WARM));
            cx.halo(fb, (head.0, head.1, 2.6), WARM, 6, 0.7);
        }
        Kind::UtilityPole => {
            cx.post(fb, x, y, 0.0, 4.4, 0.07, [128, 118, 104, 255]);
            // Crossarms with insulators.
            for z in [4.1, 3.7] {
                let (a, b) = ((x - 0.4 * ly, y - 0.4 * lx), (x + 0.4 * ly, y + 0.4 * lx));
                cx.line(fb, (a.0, a.1, z), (b.0, b.1, z), cx.surface([96, 90, 82, 255], 0.8));
            }
            // A transformer drum on some.
            if h % 2 == 0 {
                cx.cuboid(fb, (x - 0.12, y - 0.12, 3.0), (x + 0.12, y + 0.12, 3.4), [160, 164, 168, 255]);
            }
        }
        Kind::Signal => {
            cx.post(fb, x, y, 0.0, 2.6, 0.04, STEEL);
            cx.cuboid(fb, (x - 0.08, y - 0.22, 2.2), (x + 0.08, y + 0.22, 2.5), DARK);
            // Green, amber or red, changing on a slow cycle.
            let phase = ((cx.t / 9.0 + (h % 7) as f64 / 7.0) % 1.0 * 3.0) as usize;
            let c = [[90, 230, 140, 255], [255, 196, 70, 255], [250, 80, 70, 255]][phase.min(2)];
            let (lx0, ly0) = cx.px(x + 0.09, y - 0.12 + 0.12 * phase as f64, 2.35);
            fb.put(lx0, ly0, cx.lamp(c));
            fb.put(lx0 + 1, ly0, cx.lamp(c));
            cx.halo(fb, (x + 0.1, y, 2.35), c, 3, 0.5);
        }
        Kind::Vending => {
            let (w, d) = (0.28 * lx + 0.18 * ly, 0.28 * ly + 0.18 * lx);
            let body = [[232, 66, 60, 255], [60, 120, 220, 255], [240, 240, 236, 255]][(h % 3) as usize];
            cx.cuboid_with(fb, (x - w, y - d, 0.0), (x + w, y + d, 1.0), body, Some((p.along_y, cx.lamp([236, 244, 250, 255]))));
            cx.halo(fb, (x + 0.2 * ly + 0.2 * lx, y, 0.6), [220, 240, 255, 255], 4, 0.6);
        }
        Kind::Bollard => cx.post(fb, x, y, 0.0, 0.4, 0.05, [236, 196, 52, 255]),
        Kind::Car => {
            let paint = [[244, 238, 226, 255], [176, 210, 230, 255], [240, 190, 170, 255], [190, 226, 196, 255]][(h % 4) as usize];
            let (hl, hw) = (0.42, 0.24);
            let (ex, ey) = (hl * lx + hw * ly, hl * ly + hw * lx);
            cx.cuboid(fb, (x - ex, y - ey, 0.08), (x + ex, y + ey, 0.5), paint);
            let (cxx, cyy) = (0.24 * lx + 0.2 * ly, 0.24 * ly + 0.2 * lx);
            cx.cuboid_with(fb, (x - cxx, y - cyy, 0.5), (x + cxx, y + cyy, 0.9), [120, 160, 190, 255], None);
        }
        Kind::Rack => {
            let (w, d) = (0.36 * ly + 0.22 * lx, 0.36 * lx + 0.22 * ly);
            cx.cuboid(fb, (x - w, y - d, 0.0), (x + w, y + d, 1.9), DARK);
            // Status lights blinking down the front.
            for k in 0..6 {
                let on = (h >> k & 1 == 1) ^ (((cx.t * (1.0 + (k % 3) as f64)) as u64 + k) % 4 == 0);
                if on {
                    let (fx, fy) = (x + w + 0.01, y);
                    let (px, py) = cx.px(fx, fy - 0.1 + 0.05 * (k % 2) as f64, 0.3 + 0.26 * k as f64);
                    fb.put(px, py, cx.lamp(if k % 3 == 0 { accent } else { [120, 255, 160, 255] }));
                }
            }
        }
        Kind::Terminal => {
            cx.cuboid(fb, (x - 0.3, y - 0.3, 0.0), (x + 0.3, y + 0.3, 0.7), STEEL);
            // A screen on a stand, showing scrolling lines.
            let (sw, sd) = (0.26 * ly + 0.03 * lx, 0.26 * lx + 0.03 * ly);
            cx.cuboid_with(fb, (x - sw, y - sd, 0.8), (x + sw, y + sd, 1.4), DARK, Some((p.along_y, cx.lamp(blend(accent, [10, 30, 34, 255], 40, 100)))));
            cx.halo(fb, (x, y, 1.1), accent, 5, 0.55);
        }
        Kind::Partition => {
            let (w, d) = (0.5 * ly + 0.03 * lx, 0.5 * lx + 0.03 * ly);
            let corners = |z| [(x - w, y - d, z), (x + w, y - d, z), (x + w, y + d, z), (x - w, y + d, z)];
            let (lo, hi) = (corners(0.0), corners(2.2));
            let q: Vec<Point> = [lo[0], lo[2], hi[2], hi[0]].iter().map(|c| (cx.proj)(c.0, c.1, c.2)).collect();
            raster::blend_polygon(fb, &q, (cx.lit)([170, 220, 230, 255]), 70);
            cx.line(fb, hi[0], hi[2], cx.surface(STEEL, 1.0));
        }
        Kind::Turbine => {
            cx.post(fb, x, y, 0.0, 4.6, 0.06, WHITE);
            cx.cuboid(fb, (x - 0.12, y - 0.12, 4.5), (x + 0.12, y + 0.12, 4.75), WHITE);
            // Three blades turning with the wind, facing along the side.
            let spin = cx.t * (0.15 + 0.9 * cx.wind) + (h % 100) as f64 / 100.0;
            let hub = (x + 0.15 * ly, y + 0.15 * lx, 4.62);
            for k in 0..3 {
                let a = spin + k as f64 / 3.0;
                let (u, v) = (cos_turns(a) * 1.3, sin_turns(a) * 1.3);
                let tip = (hub.0 + u * lx, hub.1 + u * ly, hub.2 + v);
                cx.line(fb, hub, tip, cx.surface(WHITE, 0.95));
            }
        }
        Kind::Mast => {
            cx.post(fb, x, y, 0.0, 2.8, 0.035, WHITE);
            for z in [1.0, 1.8, 2.5] {
                cx.cuboid(fb, (x - 0.08, y - 0.08, z), (x + 0.08, y + 0.08, z + 0.08), [182, 188, 194, 255]);
            }
            let blink = ((cx.t * 0.7 + (h % 10) as f64 / 10.0) % 1.0) < 0.5;
            if blink {
                cx.cuboid(fb, (x - 0.05, y - 0.05, 2.8), (x + 0.05, y + 0.05, 2.9), cx.lamp(accent));
                cx.halo(fb, (x, y, 2.85), accent, 3, 0.6);
            }
        }
        Kind::Pod => {
            let (w, d) = (0.4 * lx + 0.26 * ly, 0.4 * ly + 0.26 * lx);
            cx.cuboid(fb, (x - w, y - d, 0.0), (x + w, y + d, 0.45), WHITE);
            // A glazed top full of green.
            cx.cuboid(fb, (x - w + 0.05, y - d + 0.05, 0.45), (x + w - 0.05, y + d - 0.05, 0.75), blend(foliage[2], [190, 230, 236, 255], 40, 100));
        }
        Kind::Tree => {
            cx.post(fb, x, y, 0.0, 1.4, 0.05, [110, 84, 62, 255]);
            // A round canopy of dithered leaves, swaying a little.
            let sway = 0.05 * sin_turns(cx.t * 0.2 + (h % 100) as f64 / 100.0) * (0.3 + cx.wind);
            let (tx, ty) = cx.px(x + sway, y, 2.1);
            let r = 6 + (h % 3) as i32;
            for py in ty - r..=ty + r {
                for px in tx - r..=tx + r {
                    let d2 = (px - tx) * (px - tx) + (py - ty) * (py - ty) * 2;
                    if d2 > r * r {
                        continue;
                    }
                    // Lighter toward the upper left, as if lit from there.
                    let lit = ((px - tx) + (py - ty)) as f64 / r as f64;
                    let k = if lit < -0.6 { 2 } else if lit < 0.4 { 1 } else { 0 };
                    let keep = crate::light::bayer(px, py) < 0.9;
                    if keep {
                        fb.put(px, py, cx.surface(foliage[k], 1.0));
                    }
                }
            }
        }
        Kind::DronePad => {
            // A flat pad with a ring, and a drone bobbing over it.
            cx.cuboid(fb, (x - 0.42, y - 0.42, 0.0), (x + 0.42, y + 0.42, 0.06), [210, 214, 218, 255]);
            let (rx, ry) = cx.px(x, y, 0.07);
            for k in 0..16 {
                let a = k as f64 / 16.0;
                fb.put(rx + (cos_turns(a) * 3.5) as i32, ry + (sin_turns(a) * 1.8) as i32, cx.lamp(accent));
            }
            let bob = 1.3 + 0.18 * sin_turns(cx.t * 0.35 + (h % 10) as f64 / 10.0);
            cx.cuboid(fb, (x - 0.12, y - 0.12, bob), (x + 0.12, y + 0.12, bob + 0.1), WHITE);
            for (ex, ey) in [(-0.22, -0.22), (0.22, -0.22), (0.22, 0.22), (-0.22, 0.22)] {
                let (px, py) = cx.px(x + ex, y + ey, bob + 0.12);
                let blur = if (cx.t * 30.0) as i64 % 2 == 0 { 1 } else { 0 };
                raster::line(fb, px - 1, py + blur, px + 1, py - blur, cx.surface([200, 204, 210, 255], 1.0));
            }
        }
        Kind::TreeFern => {
            // A slender trunk, a crown of arching fronds.
            let tall = 2.2 + (h % 5) as f64 * 0.25;
            cx.post(fb, x, y, 0.0, tall, 0.045, [92, 72, 52, 255]);
            let n = 7;
            for k in 0..n {
                let a = k as f64 / n as f64 + (h % 13) as f64 / 13.0;
                let sway = 0.04 * sin_turns(cx.t * 0.15 + a);
                let (u, v) = (cos_turns(a), sin_turns(a));
                let mid = (x + u * 0.45, y + v * 0.45, tall + 0.25);
                let tip = (x + u * (0.85 + sway), y + v * (0.85 + sway), tall - 0.35);
                let c = cx.surface(foliage[1 + (k % 2)], 1.0);
                cx.line(fb, (x, y, tall), mid, c);
                cx.line(fb, mid, tip, c);
            }
        }
        Kind::Planter => {
            let (w, d) = (0.42 * lx + 0.3 * ly, 0.42 * ly + 0.3 * lx);
            cx.cuboid(fb, (x - w, y - d, 0.0), (x + w, y + d, 0.5), [222, 226, 228, 255]);
            cx.cuboid(fb, (x - w + 0.05, y - d + 0.05, 0.5), (x + w - 0.05, y + d - 0.05, 0.56), foliage[0]);
            for k in 0..5 {
                let (ox, oy) = ((k as f64 - 2.0) * 0.16 * lx, (k as f64 - 2.0) * 0.16 * ly);
                let top = 0.9 + 0.12 * ((h >> (k * 4)) & 3) as f64;
                cx.line(fb, (x + ox, y + oy, 0.56), (x + ox * 1.4, y + oy * 1.4, top), cx.surface(foliage[1 + k % 2], 1.0));
            }
        }
        Kind::Mister => {
            cx.post(fb, x, y, 0.0, 0.9, 0.025, STEEL);
            // Breaths of vapour, rising and fading on a cycle.
            for k in 0..3 {
                let age = (cx.t * 0.25 + k as f64 / 3.0 + (h % 10) as f64 / 10.0) % 1.0;
                let (px, py) = cx.px(x + 0.1 * age, y - 0.1 * age, 1.0 + 0.9 * age);
                let r = 1 + (age * 4.0) as i32;
                for yy in py - r..=py + r {
                    for xx in px - r..=px + r {
                        if (xx - px) * (xx - px) + (yy - py) * (yy - py) <= r * r && crate::light::bayer(xx, yy) < 0.6 {
                            fb.blend_over(xx, yy, [240, 248, 246, 255], ((1.0 - age) * 90.0) as u32);
                        }
                    }
                }
            }
        }
        Kind::Basket => {
            // Hanging from the roof on a line, trailing leaves.
            let top = cx.roof.map_or(5.0, |r| r.height_at(x, y));
            let z = top - 1.6 - (h % 3) as f64 * 0.4;
            cx.line(fb, (x, y, top), (x, y, z + 0.3), cx.surface(STEEL, 0.9));
            cx.cuboid(fb, (x - 0.16, y - 0.16, z), (x + 0.16, y + 0.16, z + 0.3), [150, 112, 80, 255]);
            for k in 0..4 {
                let a = k as f64 / 4.0 + 0.125;
                let (u, v) = (cos_turns(a) * 0.22, sin_turns(a) * 0.22);
                cx.line(fb, (x + u, y + v, z + 0.3), (x + u * 1.3, y + v * 1.3, z - 0.4 - 0.1 * k as f64), cx.surface(foliage[1 + k % 2], 1.0));
            }
        }
    }
}

/// The wires between two utility poles, sagging between their tops.
pub fn wire(fb: &mut Framebuffer, a: &Prop, b: &Prop, cx: &Ctx) {
    let c = cx.surface([46, 46, 50, 255], 1.0);
    for (k, z) in [4.1f64, 3.7].iter().enumerate() {
        for s in [-0.35, 0.35] {
            let (ox, oy) = if a.along_y { (s, 0.0) } else { (0.0, s) };
            let n = 10;
            let mut prev = None;
            for i in 0..=n {
                let f = i as f64 / n as f64;
                let sag = 0.45 * 4.0 * f * (1.0 - f) * (1.0 + 0.2 * k as f64);
                let p = cx.px(a.x + ox + (b.x - a.x) * f, a.y + oy + (b.y - a.y) * f, z - sag);
                if let Some(q) = prev {
                    let (q0, q1): (i32, i32) = q;
                    raster::line(fb, q0, q1, p.0, p.1, c);
                }
                prev = Some(p);
            }
        }
    }
}



}

// ---- src/raster.rs ----
pub mod raster {
//! Raster primitives for the logical buffer.
//!
//! Everything works in the fixed-point plane from [`crate::iso`] and rounds to
//! whole logical pixels only at the final span fill. Fills are expanded by up to
//! one pixel so adjacent diamonds never leave a seam; later tiles simply
//! overwrite the overlap, and painting order is the caller's depth order.

use crate::framebuffer::{Framebuffer, Rgba};
use crate::iso::{to_world, Iso, Point, FP};

/// Floor division for a positive divisor.
#[inline]
pub fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

/// Ceiling division for a positive divisor.
#[inline]
pub fn ceil_div(a: i32, b: i32) -> i32 {
    -((-a).div_euclid(b))
}

/// Fill a diamond of half-width `hw` and half-height `hh` centred at
/// fixed-point `(cx, cy)`.
pub fn fill_diamond_at(fb: &mut Framebuffer, cx: i32, cy: i32, hw: i32, hh: i32, colour: Rgba) {
    if hw <= 0 || hh <= 0 {
        return;
    }
    let y0 = floor_div(cy - hh, FP);
    let y1 = ceil_div(cy + hh, FP);
    for py in y0..=y1 {
        // The pixel's own centre decides whether (and how far) it is inside.
        let dy = (py * FP + FP / 2 - cy).abs();
        if dy > hh {
            continue;
        }
        let halfspan = hw * (hh - dy) / hh;
        let x0 = floor_div(cx - halfspan, FP);
        let x1 = ceil_div(cx + halfspan, FP);
        fb.fill_span(py, x0, x1, colour);
    }
}

/// Fill the top face of grid cell `(gx, gy, gz)`.
pub fn fill_cell(fb: &mut Framebuffer, iso: &Iso, gx: i32, gy: i32, gz: i32, colour: Rgba) {
    let c = iso.project(gx, gy, gz);
    fill_diamond_at(fb, c.x, c.y, iso.half_w(), iso.half_h(), colour);
}

/// Fill one vertical face: a parallelogram whose top edge runs from
/// `(x0, y_at_x0)` to `(x1, y_at_x1)` and which drops by `drop` fixed-point
/// units.
pub fn fill_face(fb: &mut Framebuffer, x0: i32, x1: i32, y_at_x0: i32, y_at_x1: i32, drop: i32, colour: Rgba) {
    if x1 <= x0 || drop <= 0 {
        return;
    }
    let dx = x1 - x0;
    for px in floor_div(x0, FP)..=ceil_div(x1, FP) {
        let xc = px * FP + FP / 2;
        let ytop = y_at_x0 + ((y_at_x1 - y_at_x0) * (xc - x0)).div_euclid(dx);
        let y0 = floor_div(ytop, FP);
        let y1 = ceil_div(ytop + drop, FP);
        // A *vertical* run down this pixel column. Calling the horizontal
        // `fill_span` here swapped x and y, which drew every side face as a
        // sideways streak instead of an upright parallelogram.
        fb.fill_vertical(px, y0, y1, colour);
    }
}

/// Fill a convex quad, given its four corners in fixed-point units, clipped.
///
/// Used by the rotated architecture; the axis-aligned paths keep their own
/// primitives so their pixels cannot drift.
pub fn fill_quad(fb: &mut Framebuffer, corners: [Point; 4], colour: Rgba) {
    fill_polygon(fb, &corners, colour);
}

/// Fill a convex polygon, given its corners in order in fixed-point units.
/// For four corners this is exactly [`fill_quad`]; a face clipped by the
/// ground plane can have up to five.
pub fn fill_polygon(fb: &mut Framebuffer, corners: &[Point], colour: Rgba) {
    let n = corners.len();
    if n < 3 {
        return;
    }
    let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0);
    let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0);
    for py in floor_div(min_y, FP)..=ceil_div(max_y, FP) {
        // The pixel's own centre decides whether the span crosses it.
        let yc = py * FP + FP / 2;
        let (mut lo, mut hi, mut hits) = (i32::MAX, i32::MIN, 0);
        for i in 0..n {
            let a = corners[i];
            let b = corners[(i + 1) % n];
            if (a.y <= yc && yc <= b.y) || (b.y <= yc && yc <= a.y) {
                let dy = b.y - a.y;
                if dy != 0 {
                    let x = a.x + ((b.x - a.x) * (yc - a.y)).div_euclid(dy);
                    lo = lo.min(x);
                    hi = hi.max(x);
                    hits += 1;
                }
            }
        }
        if hits < 2 {
            continue;
        }
        fb.fill_span(py, floor_div(lo, FP), ceil_div(hi, FP), colour);
    }
}

/// Visit every pixel of a convex polygon (corners in fixed-point units, in
/// order), with the same coverage rule as [`fill_polygon`]: `f(x, y)` once
/// per covered pixel. For translucent and dithered fills (glass, frost).
pub fn for_polygon(corners: &[Point], mut f: impl FnMut(i32, i32)) {
    let n = corners.len();
    if n < 3 {
        return;
    }
    let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0);
    let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0);
    for py in floor_div(min_y, FP)..=ceil_div(max_y, FP) {
        let yc = py * FP + FP / 2;
        let (mut lo, mut hi, mut hits) = (i32::MAX, i32::MIN, 0);
        for i in 0..n {
            let a = corners[i];
            let b = corners[(i + 1) % n];
            if (a.y <= yc && yc <= b.y) || (b.y <= yc && yc <= a.y) {
                let dy = b.y - a.y;
                if dy != 0 {
                    let x = a.x + ((b.x - a.x) * (yc - a.y)).div_euclid(dy);
                    lo = lo.min(x);
                    hi = hi.max(x);
                    hits += 1;
                }
            }
        }
        if hits < 2 {
            continue;
        }
        for px in floor_div(lo, FP)..=ceil_div(hi, FP) {
            f(px, py);
        }
    }
}

/// Composite `colour` over a convex polygon at coverage `alpha` (0-255).
pub fn blend_polygon(fb: &mut Framebuffer, corners: &[Point], colour: Rgba, alpha: u32) {
    for_polygon(corners, |x, y| fb.blend_over(x, y, colour, alpha));
}

/// Which side of the ground plane (world `z = 0`) to draw.
///
/// The ground is drawn between the two halves: everything [`Half::Below`] it
/// first, so the plateau covers it wherever the plateau is, then everything
/// [`Half::Above`] it over the plateau. A face lying exactly on the plane (a
/// descending column's cap) belongs to the upper half.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Half {
    All,
    Above,
    Below,
}

/// How far from the plane still counts as on it, in world units.
const PLANE_EPS: f64 = 1e-9;

type World = (f64, f64, f64);

/// Clip one world-space face to `half`. Returns the corners to draw and how
/// many there are, or `None` when nothing of the face is on that side. A face
/// wholly on the kept side comes back untouched, so its pixels do not move.
fn clip_face(face: [World; 4], half: Half) -> Option<([World; 8], usize)> {
    let mut out = [(0.0, 0.0, 0.0); 8];
    let whole = |out: &mut [World; 8]| {
        out[..4].copy_from_slice(&face);
        Some((*out, 4))
    };
    let sign = match half {
        Half::All => return whole(&mut out),
        Half::Above => {
            if face.iter().all(|p| p.2 >= -PLANE_EPS) {
                return whole(&mut out);
            }
            1.0
        }
        Half::Below => {
            if !face.iter().any(|p| p.2 < -PLANE_EPS) {
                return None;
            }
            if face.iter().all(|p| p.2 <= PLANE_EPS) {
                return whole(&mut out);
            }
            -1.0
        }
    };
    // Sutherland-Hodgman against the single plane z = 0.
    let keep = |p: &World| sign * p.2 >= 0.0;
    let cut = |a: &World, b: &World| {
        let t = a.2 / (a.2 - b.2);
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0.0)
    };
    let mut n = 0;
    for i in 0..4 {
        let (a, b) = (&face[i], &face[(i + 1) % 4]);
        if keep(b) {
            if !keep(a) {
                out[n] = cut(a, b);
                n += 1;
            }
            out[n] = *b;
            n += 1;
        } else if keep(a) {
            out[n] = cut(a, b);
            n += 1;
        }
    }
    // A side face cut right at its foot leaves a sliver with no area; drawing
    // it would leave a one-pixel line along the ground.
    let (mut ax, mut ay, mut az) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (out[i], out[(i + 1) % n]);
        ax += (p.1 - q.1) * (p.2 + q.2);
        ay += (p.2 - q.2) * (p.0 + q.0);
        az += (p.0 - q.0) * (p.1 + q.1);
    }
    if n < 3 || ax * ax + ay * ay + az * az < PLANE_EPS {
        return None;
    }
    Some((out, n))
}

/// Draw one column of the *rotated* architecture.
///
/// The cell is a box spanning model levels `z0..=z1`: `0..=height-1` for a
/// raised column, `-depth..=0` for one that descends below the ground. Its six
/// faces are culled by their rotated normal against the view direction, so
/// exactly the camera-facing ones are drawn, and the top is filled last. Each
/// face is then clipped to `half` of the ground plane in world space. At a
/// rotation of zero the visible set and the shading match [`fill_column`]'s, so
/// turning the shrine never changes how it reads.
pub fn fill_box(
    fb: &mut Framebuffer,
    iso: &Iso,
    rot: &[[f64; 3]; 3],
    off: (f64, f64, f64),
    gx: i32,
    gy: i32,
    (z0, z1): (f64, f64),
    half: Half,
    top: Rgba,
    side_x: Rgba,
    side_y: Rgba,
) {
    if z1 < z0 {
        return;
    }
    let w = |x: f64, y: f64, z: f64| to_world(x, y, z, rot, off);
    let face = |fb: &mut Framebuffer, corners: [World; 4], colour: Rgba| {
        if let Some((pts, n)) = clip_face(corners, half) {
            let mut plane = [Point { x: 0, y: 0 }; 8];
            for i in 0..n {
                plane[i] = iso.project_world(pts[i].0, pts[i].1, pts[i].2);
            }
            fill_polygon(fb, &plane[..n], colour);
        }
    };
    let (cx, cy) = (gx as f64, gy as f64);
    let h = 0.5;

    // A face shows when its rotated normal points at the viewer, which for this
    // projection is the direction (1, 1, 1).
    let facing = |n: [f64; 3]| {
        let w = [
            rot[0][0] * n[0] + rot[0][1] * n[1] + rot[0][2] * n[2],
            rot[1][0] * n[0] + rot[1][1] * n[1] + rot[1][2] * n[2],
            rot[2][0] * n[0] + rot[2][1] * n[1] + rot[2][2] * n[2],
        ];
        w[0] + w[1] + w[2] > 0.0
    };

    if z1 > z0 {
        let sides: [([f64; 3], (f64, f64), (f64, f64), Rgba); 4] = [
            ([1.0, 0.0, 0.0], (h, -h), (h, h), side_x),
            ([-1.0, 0.0, 0.0], (-h, -h), (-h, h), side_x),
            ([0.0, 1.0, 0.0], (-h, h), (h, h), side_y),
            ([0.0, -1.0, 0.0], (-h, -h), (h, -h), side_y),
        ];
        for (normal, (ax, ay), (bx, by), colour) in sides {
            if !facing(normal) {
                continue;
            }
            face(
                fb,
                [
                    w(cx + ax, cy + ay, z1),
                    w(cx + bx, cy + by, z1),
                    w(cx + bx, cy + by, z0),
                    w(cx + ax, cy + ay, z0),
                ],
                colour,
            );
        }
    }

    // Top face, or the underside once the rotation has turned the box over.
    let z_face = if facing([0.0, 0.0, 1.0]) {
        Some(z1)
    } else if facing([0.0, 0.0, -1.0]) {
        Some(z0)
    } else {
        None
    };
    if let Some(z) = z_face {
        face(fb, [w(cx - h, cy - h, z), w(cx + h, cy - h, z), w(cx + h, cy + h, z), w(cx - h, cy + h, z)], top);
    }
}

/// Draw a full column: the two visible side faces between the ground and the
/// column's top, then its top face. `height` is the number of stacked tiles.
pub fn fill_column(fb: &mut Framebuffer, iso: &Iso, gx: i32, gy: i32, height: u32, top: Rgba, left: Rgba, right: Rgba) {
    if height == 0 {
        return;
    }
    fill_column_range(fb, iso, gx, gy, 0, height as i32 - 1, top, left, right);
}

/// Draw the part of a column between stack levels `lo` and `hi`: the two
/// visible side faces, then the top face at `hi`. The axis-aligned path uses
/// this to draw the halves of a column either side of the ground plane.
pub fn fill_column_range(
    fb: &mut Framebuffer,
    iso: &Iso,
    gx: i32,
    gy: i32,
    lo: i32,
    hi: i32,
    top: Rgba,
    left: Rgba,
    right: Rgba,
) {
    if hi < lo {
        return;
    }
    let c = iso.project(gx, gy, hi);
    let (hw, hh) = (iso.half_w(), iso.half_h());
    let drop = (hi - lo) * iso.level_h;
    if drop > 0 {
        // Right (south-east) face: from the bottom corner up to the right corner.
        fill_face(fb, c.x, c.x + hw, c.y + hh, c.y, drop, right);
        // Left (south-west) face: from the left corner down to the bottom corner.
        fill_face(fb, c.x - hw, c.x, c.y, c.y + hh, drop, left);
    }
    fill_diamond_at(fb, c.x, c.y, hw, hh, top);
}

/// Draw a 1-pixel (logical) vertical line — used for stems and rain.
pub fn vertical_line(fb: &mut Framebuffer, x: i32, y0: i32, y1: i32, colour: Rgba) {
    for y in y0.min(y1)..=y0.max(y1) {
        fb.put(x, y, colour);
    }
}

/// A 1-pixel (logical) line between two points, Bresenham. Used for stems,
/// tendrils and rain streaks.
pub fn line(fb: &mut Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, colour: Rgba) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        fb.put(x, y, colour);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

// ---------------------------------------------------------------------------
// Shaded fills: the same coverage as the flat fills above, but each pixel asks
// a shader for its colour, given where on which face of the model it lies.
// That is what lets a block carry a texture fixed to its own faces, and the
// light treat each face by its normal.
// ---------------------------------------------------------------------------

/// Which face of a block a pixel lies on, by its outward normal in the
/// model's own axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Top,
    Bottom,
    PosX,
    NegX,
    PosY,
    NegY,
}

impl Face {
    pub fn normal(self) -> [f64; 3] {
        match self {
            Face::Top => [0.0, 0.0, 1.0],
            Face::Bottom => [0.0, 0.0, -1.0],
            Face::PosX => [1.0, 0.0, 0.0],
            Face::NegX => [-1.0, 0.0, 0.0],
            Face::PosY => [0.0, 1.0, 0.0],
            Face::NegY => [0.0, -1.0, 0.0],
        }
    }
}

/// One pixel of a face: the face, the point of the model it shows (cell
/// centres are whole numbers, a column's top is at its top level), the
/// screen pixel (for ordered dithering), and how near the viewer it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub face: Face,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub px: i32,
    pub py: i32,
    /// Nearness to the viewer: `wx + wy + wz` of the world point, since this
    /// projection looks along (-1, -1, -1). Larger is nearer. Exact on the
    /// turned path (from the face's plane); on the upright path, where the
    /// painter's order is already exact, it is the model point's own sum.
    pub depth: f64,
}

/// Where a shaded fill sends each covered pixel: to be coloured and drawn,
/// or only recorded (see the visibility pre-pass in the scene).
pub type Sink<'a> = &'a mut dyn FnMut(&Surface);

/// The buffer size a fill clips to.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    pub fn of(fb: &Framebuffer) -> Self {
        Bounds { w: fb.width() as i32, h: fb.height() as i32 }
    }
}

/// [`fill_diamond_at`], shaded: the top face of model cell `(mx, my)` at
/// level `mz`. Each pixel's model point is found by undoing the diamond.
#[allow(clippy::too_many_arguments)]
pub fn fill_diamond_shaded(fb: Bounds, cx: i32, cy: i32, hw: i32, hh: i32, (mx, my, mz): (f64, f64, f64), face: Face, sink: Sink) {
    if hw <= 0 || hh <= 0 {
        return;
    }
    let y0 = floor_div(cy - hh, FP).max(0);
    let y1 = ceil_div(cy + hh, FP).min(fb.h - 1);
    for py in y0..=y1 {
        let yc = py * FP + FP / 2;
        let dy = (yc - cy).abs();
        if dy > hh {
            continue;
        }
        let halfspan = hw * (hh - dy) / hh;
        let x0 = floor_div(cx - halfspan, FP).max(0);
        let x1 = ceil_div(cx + halfspan, FP).min(fb.w - 1);
        let b = (yc - cy) as f64 / hh as f64;
        for px in x0..=x1 {
            let a = (px * FP + FP / 2 - cx) as f64 / hw as f64;
            // Screen x runs along (x - y), screen y along (x + y).
            let (dx, dyy) = (((a + b) / 2.0).clamp(-0.5, 0.5), ((b - a) / 2.0).clamp(-0.5, 0.5));
            let (x, y) = (mx + dx, my + dyy);
            sink(&Surface { face, x, y, z: mz, px, py, depth: x + y + mz });
        }
    }
}

/// [`fill_face`], shaded. `at(f)` gives the model `(x, y)` a fraction `f`
/// along the face's top edge; `z_top` is the model level of that edge and
/// `level_h` the fixed-point rise of one level.
#[allow(clippy::too_many_arguments)]
pub fn fill_face_shaded(
    fb: Bounds,
    (x0, x1): (i32, i32),
    (y_at_x0, y_at_x1): (i32, i32),
    drop: i32,
    level_h: i32,
    z_top: f64,
    face: Face,
    at: &dyn Fn(f64) -> (f64, f64),
    sink: Sink,
) {
    if x1 <= x0 || drop <= 0 {
        return;
    }
    let dx = x1 - x0;
    let levels = drop as f64 / level_h as f64;
    for px in floor_div(x0, FP)..=ceil_div(x1, FP) {
        if px < 0 || px >= fb.w {
            continue;
        }
        let xc = px * FP + FP / 2;
        let ytop = y_at_x0 + ((y_at_x1 - y_at_x0) * (xc - x0)).div_euclid(dx);
        let (mx, my) = at(((xc - x0) as f64 / dx as f64).clamp(0.0, 1.0));
        let y0 = floor_div(ytop, FP).max(0);
        let y1 = ceil_div(ytop + drop, FP).min(fb.h - 1);
        for py in y0..=y1 {
            let down = ((py * FP + FP / 2 - ytop) as f64 / level_h as f64).clamp(0.0, levels);
            let z = z_top - down;
            sink(&Surface { face, x: mx, y: my, z, px, py, depth: mx + my + z });
        }
    }
}

/// [`fill_column_range`], shaded: the part of model cell `(gx, gy)` between
/// screen levels `lo` and `hi`. `lift` is how far the architecture is raised
/// (screen level = model level + lift), so the shader is told model levels.
#[allow(clippy::too_many_arguments)]
pub fn fill_column_range_shaded(fb: Bounds, iso: &Iso, gx: i32, gy: i32, lo: i32, hi: i32, lift: i32, sink: Sink) {
    if hi < lo {
        return;
    }
    let c = iso.project(gx, gy, hi);
    let (hw, hh) = (iso.half_w(), iso.half_h());
    let drop = (hi - lo) * iso.level_h;
    let (x, y, z) = (gx as f64, gy as f64, (hi - lift) as f64);
    if drop > 0 {
        // Right face, x = +1/2: from the bottom corner up to the right corner.
        let right = |f: f64| (x + 0.5, y + 0.5 - f);
        fill_face_shaded(fb, (c.x, c.x + hw), (c.y + hh, c.y), drop, iso.level_h, z, Face::PosX, &right, sink);
        // Left face, y = +1/2: from the left corner down to the bottom corner.
        let left = |f: f64| (x - 0.5 + f, y + 0.5);
        fill_face_shaded(fb, (c.x - hw, c.x), (c.y, c.y + hh), drop, iso.level_h, z, Face::PosY, &left, sink);
    }
    fill_diamond_shaded(fb, c.x, c.y, hw, hh, (x, y, z), Face::Top, sink);
}

/// Screen -> model for one flat face, from three of its projected corners.
struct PlaneMap {
    /// The nearness at the first corner, and its change along the two edges
    /// (unclamped, so it stays exact right up to and past the face's edge).
    depth0: f64,
    ddepth: (f64, f64),
    s0: (f64, f64),
    e1: (f64, f64),
    e2: (f64, f64),
    inv_det: f64,
    m0: [f64; 3],
    d1: [f64; 3],
    d2: [f64; 3],
}

impl PlaneMap {
    fn new(s: [Point; 3], m: [[f64; 3]; 3], depth: [f64; 3]) -> Self {
        let p = |q: Point| (q.x as f64, q.y as f64);
        let (s0, s1, s2) = (p(s[0]), p(s[1]), p(s[2]));
        let e1 = (s1.0 - s0.0, s1.1 - s0.1);
        let e2 = (s2.0 - s0.0, s2.1 - s0.1);
        let det = e1.0 * e2.1 - e1.1 * e2.0;
        let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        PlaneMap {
            depth0: depth[0],
            ddepth: (depth[1] - depth[0], depth[2] - depth[0]),
            s0,
            e1,
            e2,
            // A face seen edge-on has no area to map; it shows its first corner.
            inv_det: if det.abs() > 1e-6 { 1.0 / det } else { 0.0 },
            m0: m[0],
            d1: sub(m[1], m[0]),
            d2: sub(m[2], m[0]),
        }
    }

    /// Edge coordinates of screen point `(x, y)` on the face, unclamped.
    fn edges(&self, x: f64, y: f64) -> (f64, f64) {
        let d = (x - self.s0.0, y - self.s0.1);
        ((d.0 * self.e2.1 - d.1 * self.e2.0) * self.inv_det, (self.e1.0 * d.1 - self.e1.1 * d.0) * self.inv_det)
    }

    fn depth(&self, x: f64, y: f64) -> f64 {
        let (a, b) = self.edges(x, y);
        self.depth0 + a * self.ddepth.0 + b * self.ddepth.1
    }

    fn model(&self, x: f64, y: f64) -> [f64; 3] {
        let d = (x - self.s0.0, y - self.s0.1);
        let a = ((d.0 * self.e2.1 - d.1 * self.e2.0) * self.inv_det).clamp(0.0, 1.0);
        let b = ((self.e1.0 * d.1 - self.e1.1 * d.0) * self.inv_det).clamp(0.0, 1.0);
        [0, 1, 2].map(|k| self.m0[k] + a * self.d1[k] + b * self.d2[k])
    }
}

/// [`fill_polygon`], shaded through a plane map.
fn fill_polygon_shaded(fb: Bounds, corners: &[Point], map: &PlaneMap, face: Face, sink: Sink) {
    let n = corners.len();
    if n < 3 {
        return;
    }
    let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0);
    let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0);
    for py in floor_div(min_y, FP).max(0)..=ceil_div(max_y, FP).min(fb.h - 1) {
        let yc = py * FP + FP / 2;
        let (mut lo, mut hi, mut hits) = (i32::MAX, i32::MIN, 0);
        for i in 0..n {
            let a = corners[i];
            let b = corners[(i + 1) % n];
            if (a.y <= yc && yc <= b.y) || (b.y <= yc && yc <= a.y) {
                let dy = b.y - a.y;
                if dy != 0 {
                    let x = a.x + ((b.x - a.x) * (yc - a.y)).div_euclid(dy);
                    lo = lo.min(x);
                    hi = hi.max(x);
                    hits += 1;
                }
            }
        }
        if hits < 2 {
            continue;
        }
        for px in floor_div(lo, FP).max(0)..=ceil_div(hi, FP).min(fb.w - 1) {
            let (sx, sy) = ((px * FP + FP / 2) as f64, yc as f64);
            let m = map.model(sx, sy);
            sink(&Surface { face, x: m[0], y: m[1], z: m[2], px, py, depth: map.depth(sx, sy) });
        }
    }
}

/// [`fill_box`], shaded: the same culling, clipping and drawing order, with
/// each face's pixels mapped back to the model for the shader.
#[allow(clippy::too_many_arguments)]
pub fn fill_box_shaded(
    fb: Bounds,
    iso: &Iso,
    rot: &[[f64; 3]; 3],
    off: (f64, f64, f64),
    gx: i32,
    gy: i32,
    (z0, z1): (f64, f64),
    half: Half,
    sink: Sink,
) {
    if z1 < z0 {
        return;
    }
    let (cx, cy) = (gx as f64, gy as f64);
    let h = 0.5;
    let facing = |n: [f64; 3]| {
        let w = [
            rot[0][0] * n[0] + rot[0][1] * n[1] + rot[0][2] * n[2],
            rot[1][0] * n[0] + rot[1][1] * n[1] + rot[1][2] * n[2],
            rot[2][0] * n[0] + rot[2][1] * n[1] + rot[2][2] * n[2],
        ];
        w[0] + w[1] + w[2] > 0.0
    };
    let mut draw = |model: [[f64; 3]; 4], face: Face| {
        let world = model.map(|m| to_world(m[0], m[1], m[2], rot, off));
        let corners = world.map(|p| iso.project_world(p.0, p.1, p.2));
        let near = |w: (f64, f64, f64)| w.0 + w.1 + w.2;
        let map = PlaneMap::new(
            [corners[0], corners[1], corners[3]],
            [model[0], model[1], model[3]],
            [near(world[0]), near(world[1]), near(world[3])],
        );
        if let Some((pts, n)) = clip_face(world, half) {
            let mut plane = [Point { x: 0, y: 0 }; 8];
            for i in 0..n {
                plane[i] = iso.project_world(pts[i].0, pts[i].1, pts[i].2);
            }
            fill_polygon_shaded(fb, &plane[..n], &map, face, &mut *sink);
        }
    };
    if z1 > z0 {
        let sides = [
            (Face::PosX, (h, -h), (h, h)),
            (Face::NegX, (-h, -h), (-h, h)),
            (Face::PosY, (-h, h), (h, h)),
            (Face::NegY, (-h, -h), (h, -h)),
        ];
        for (face, (ax, ay), (bx, by)) in sides {
            if !facing(face.normal()) {
                continue;
            }
            draw([[cx + ax, cy + ay, z1], [cx + bx, cy + by, z1], [cx + bx, cy + by, z0], [cx + ax, cy + ay, z0]], face);
        }
    }
    let top = if facing(Face::Top.normal()) {
        Some((z1, Face::Top))
    } else if facing(Face::Bottom.normal()) {
        Some((z0, Face::Bottom))
    } else {
        None
    };
    if let Some((z, face)) = top {
        draw([[cx - h, cy - h, z], [cx + h, cy - h, z], [cx + h, cy + h, z], [cx - h, cy + h, z]], face);
    }
}

}

// ---- src/recipe.rs ----
pub mod recipe {
//! A flat, portable encoding of a shrine's settings.
//!
//! The C ABI, the emitted artifact and the web page all speak this: a plain
//! array of `u32` words, versioned so it can never silently drift. Decoding is
//! total and defensive — every index is clamped — so a corrupt recipe can only
//! change the picture, never crash the module.
//!
//! Layout (56 words):
//! `[version][seed hi,lo][s1 hi,lo]…[s6 hi,lo][biome][weather]`
//! `[scale][chords][pace][kit][space][bpm][key][transpose]`
//! `[drone cycle][drone hold][drone tone]`
//! then four layers of `[kind][d0..d3][cycle][hold]`.

use crate::atmos::{LoopTimbre, LOOP_TIMBRES};
use crate::harmony::{Pace, Scale};
use crate::kits::{KITS, SPACES};
use crate::master::MasterFx;
use crate::overlay::CodeOverlay;
use crate::plants::Biome;
use crate::sim::SimConfig;
use crate::texture::Grid;
use crate::track::{DroneArc, KeyChoice, LoopDesign, Seeds, Settings};
use crate::weather::WeatherKind;
use crate::{DEFAULT_SEEDS, DEFAULT_SETTINGS};

pub const VERSION: u32 = 18;

/// Encode a configuration.
pub fn encode(config: &SimConfig) -> Vec<u32> {
    let mut w = Vec::with_capacity(56);
    let s = &config.settings;

    w.push(VERSION);
    w.push((config.seed >> 32) as u32);
    w.push(config.seed as u32);
    for seed in [
        config.seeds.s1,
        config.seeds.s2,
        config.seeds.s3,
        config.seeds.s4,
        config.seeds.s5,
        config.seeds.s6,
        config.seeds.s7,
    ] {
        w.push((seed >> 32) as u32);
        w.push(seed as u32);
    }
    w.push(index_of(&Biome::ALL, config.biome));
    w.push(index_of(&WeatherKind::ALL, config.weather));

    w.push(s.scale.index() as u32);
    w.push(s.chords as u32);
    w.push(s.pace.index());
    w.push(s.kit.index() as u32);
    w.push(s.space.index() as u32);
    w.push(s.bpm as u32);
    w.push(match s.key {
        KeyChoice::Seed => 0,
        KeyChoice::Note(n) => n as u32 + 1,
    });
    w.push((s.loop_transpose as i32 + 12).clamp(0, 24) as u32);
    w.push(s.drone.cycle_s as u32);
    w.push(s.drone.hold_s as u32);
    w.push(s.drone_tone as u32);

    for layer in 0..4 {
        match s.loops[layer] {
            LoopDesign::Sustained(LoopTimbre::Off) => w.extend_from_slice(&[0, 0, 0, 0, 0]),
            LoopDesign::Sustained(t) => w.extend_from_slice(&[1, t.index() as u32, 0, 0, 0]),
            LoopDesign::Events(g) => {
                let g = g.words();
                w.extend_from_slice(&[2, g[0], g[1], g[2], g[3]]);
            }
        }
        let arc = s.loop_arcs[layer];
        w.push(arc.cycle_s as u32);
        w.push(arc.hold_s as u32);
    }
    w.push(if s.drum_reverse { 1 } else { 0 });
    w.push(s.master.bits());
    w.push(s.code.word());
    w.push(s.code.angle_word());
    w.push(s.arch.rot_word());
    w.push(s.arch.off_word());
    for layer in s.glitch {
        w.push(layer.word_a());
        w.push(layer.word_b());
    }
    w.push((s.camera_speed + 180).clamp(0, 360) as u32);
    // Bit 0: the drone. Bit 1: the same rhythm on the atmosphere loops.
    // Bit 2: wildlife (an older recipe has it clear, so it decodes as off).
    w.push(s.drone_rhythm as u32 | ((s.loop_rhythm as u32) << 1) | ((s.wildlife as u32) << 2));
    // Version 16: the 2-step garage setup.
    w.push(s.garage.word());
    // Version 17: the start offset.
    w.push(s.start_s as u32);
    // Version 18: the microsound setup.
    w.push(s.micro.word());
    for word in s.model.words() {
        w.push(word);
    }
    for word in s.name.words() {
        w.push(word);
    }
    w
}

/// Decode a configuration. Short, empty or corrupt input falls back to the
/// defaults for whatever it cannot supply.
pub fn decode(words: &[u32]) -> SimConfig {
    if words.len() < 17 {
        return SimConfig::default();
    }
    let mut c = Cursor { words, at: 0 };
    let version = c.next();
    let seed = join(c.next(), c.next());
    let mut s: [u64; 7] = [0; 7];
    for (i, v) in s.iter_mut().enumerate() {
        // Version 15 added the drone-rhythm seed; older recipes carry six.
        if i == 6 && version < 15 {
            break;
        }
        *v = join(c.next(), c.next());
    }
    let biome = pick(&Biome::ALL, c.next());
    let weather = pick(&WeatherKind::ALL, c.next());

    let scale = Scale::from_index(c.next() as usize);
    let chords = (c.next() as u8).clamp(1, 4);
    let pace = match c.next() {
        1 => Pace::Jungle,
        2 => Pace::Garage,
        3 => Pace::Micro,
        _ => Pace::HalfTime,
    };
    let kit = KITS[(c.next() as usize) % KITS.len()];
    let space = SPACES[(c.next() as usize) % SPACES.len()];
    let bpm = (c.next() as u16).max(1);
    let key = match c.next() {
        0 => KeyChoice::Seed,
        n => KeyChoice::Note(((n - 1) % 12) as u8),
    };
    let loop_transpose = ((c.next() as i32 - 12).clamp(-12, 12)) as i8;
    let drone = DroneArc { cycle_s: c.next() as u16, hold_s: c.next() as u16 };
    let drone_tone = c.next() as u16;

    let mut loops = [LoopDesign::OFF; 4];
    let mut loop_arcs = [DroneArc::ALWAYS_ON; 4];
    for layer in 0..4 {
        let kind = c.next();
        let data = [c.next(), c.next(), c.next(), c.next()];
        loops[layer] = match kind {
            1 => LoopDesign::Sustained(LOOP_TIMBRES[(data[0] as usize) % LOOP_TIMBRES.len()]),
            2 => LoopDesign::Events(Grid::from_words(data)),
            _ => LoopDesign::OFF,
        };
        loop_arcs[layer] = DroneArc { cycle_s: c.next() as u16, hold_s: c.next() as u16 };
    }

    // Version 2 added the drum-reverse switch; older recipes default it to on.
    let drum_reverse = if version < 2 { true } else { c.next() != 0 };
    // Version 3 added the master effects; older recipes have none.
    let master = if version < 3 { MasterFx::OFF } else { MasterFx::from_bits(c.next()) };
    // Version 4 added the source-code overlay; version 5 added its text scale
    // (older words leave the top byte zero and read as the default scale);
    // version 6 added the track name; version 7 added the overlay's tilt;
    // version 8 packed its vertical shift into the same word; version 9 added
    // the architecture transform; version 10 added the glitch layers.
    let mut code = if version < 4 { CodeOverlay::DEFAULT } else { CodeOverlay::from_word(c.next()) };
    if version >= 7 {
        let layout = c.next();
        code.angle = CodeOverlay::angle_from_word(layout);
        if version >= 8 {
            code.offset = CodeOverlay::offset_from_word(layout);
        }
    }
    let arch = if version >= 9 {
        let (rot_x, rot_y, rot_z) = crate::arch::ArchTransform::rot_from_word(c.next());
        let (off_x, off_y, off_z) = crate::arch::ArchTransform::off_from_word(c.next());
        crate::arch::ArchTransform { rot_x, rot_y, rot_z, off_x, off_y, off_z }
    } else {
        crate::arch::ArchTransform::IDENTITY
    };
    let glitch = if version >= 10 {
        core::array::from_fn(|_| {
            let a = c.next();
            let b = c.next();
            crate::glitch_setup::GlitchSetup::from_words(a, b)
        })
    } else {
        crate::glitch_setup::GlitchSetup::DEFAULT
    };
    // Version 11 added the authored architecture: 64 words, absent (and so
    // procedural) in every earlier recipe.
    let camera_speed = if version >= 12 {
        (c.next().min(360) as i16) - 180
    } else {
        0
    };
    let (drone_rhythm, loop_rhythm, wildlife) = if version >= 15 {
        let word = c.next();
        (word & 1 != 0, word & 2 != 0, word & 4 != 0)
    } else {
        (false, false, false)
    };
    // Version 16 added 2-step garage's own settings.
    let garage = if version >= 16 { crate::garage::GarageSetup::from_word(c.next()) } else { crate::garage::GarageSetup::DEFAULT };
    // Version 17 added the start offset; older recipes start at the beginning.
    let start_s = if version >= 17 { c.next().min(crate::track::MAX_START_S as u32) as u16 } else { 0 };
    // Version 18 added microsound's own settings.
    let micro = if version >= 18 { crate::micro::MicroSetup::from_word(c.next()) } else { crate::micro::MicroSetup::DEFAULT };
    let model = if version >= 14 {
        let words: Vec<u32> = (0..crate::shrine_model::WORDS as u32).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_words(&words)
    } else if version >= 13 {
        let words: Vec<u32> = (0..(crate::shrine_model::CELLS as u32 / 4 + 1)).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_v13_words(&words)
    } else if version >= 11 {
        let words: Vec<u32> = (0..crate::shrine_model::CELLS as u32 / 4).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_legacy_words(&words)
    } else {
        crate::shrine_model::ShrineModel::EMPTY
    };
    let name = if version < 6 {
        crate::name::Name::EMPTY
    } else {
        let mut words = [0u32; crate::name::CAP / 4];
        for w in &mut words {
            *w = c.next();
        }
        crate::name::Name::from_words(&words)
    };

    SimConfig {
        seed,
        seeds: Seeds { s1: s[0], s2: s[1], s3: s[2], s4: s[3], s5: s[4], s6: s[5], s7: s[6] },
        settings: Settings {
            scale,
            chords,
            pace,
            kit,
            space,
            bpm,
            loops,
            loop_arcs,
            loop_transpose,
            key,
            drone: DroneArc { cycle_s: drone.cycle_s, hold_s: drone.hold_s.min(drone.cycle_s) },
            drone_tone,
            drum_reverse,
            master,
            code,
            name,
            arch,
            glitch,
            model,
            camera_speed,
            drone_rhythm,
            loop_rhythm,
            wildlife,
            garage,
            start_s,
            micro,
        },
        biome,
        weather,
    }
}

fn index_of<T: PartialEq + Copy>(all: &[T], value: T) -> u32 {
    all.iter().position(|v| *v == value).unwrap_or(0) as u32
}

fn pick<T: Copy>(all: &[T], i: u32) -> T {
    all[(i as usize) % all.len()]
}

fn join(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

struct Cursor<'a> {
    words: &'a [u32],
    at: usize,
}

impl Cursor<'_> {
    fn next(&mut self) -> u32 {
        let v = self.words.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        v
    }
}

/// The canonical default recipe, handy for tests and the page fallback.
pub fn default_words() -> Vec<u32> {
    encode(&SimConfig {
        seed: 1000,
        seeds: DEFAULT_SEEDS,
        settings: DEFAULT_SETTINGS,
        biome: Biome::Moss,
        weather: WeatherKind::Rain,
    })
}

impl SimConfig {
    /// A sane default shrine, used when no recipe is supplied.
    pub fn default_config() -> Self {
        SimConfig {
            seed: 1000,
            seeds: DEFAULT_SEEDS,
            settings: DEFAULT_SETTINGS,
            biome: Biome::Moss,
            weather: WeatherKind::Rain,
        }
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig::default_config()
    }
}

}

// ---- src/rng.rs ----
pub mod rng {
//! SplitMix64: a tiny, integer-only PRNG. Its output is fully specified by its
//! integer arithmetic, so a seed produces the same sequence on every platform
//! and in every WASM engine, forever (`HC004`).
//!
//! Every system takes its own stream via [`Rng::stream`], so adding randomness
//! in one generator never shifts the sequence in another.

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A well-mixed hash of three words: for stateless schedules, where "the
/// n-th cycle of slot k" has to give the same numbers whenever it is asked.
pub fn hash3(a: u64, b: u64, c: u64) -> u64 {
    mix(mix(mix(a) ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ c.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
}

#[derive(Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// An independent stream derived from `seed`, labelled by `stream`.
    pub fn stream(seed: u64, stream: u64) -> Self {
        Rng::new(mix(seed ^ mix(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15))))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.state)
    }

    /// Uniform in [0, 1), 53 bits.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }

    /// Uniform in [-1, 1).
    pub fn bipolar(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Integer in [lo, hi] inclusive.
    pub fn int(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo + 1)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// Index chosen with probability proportional to `weights`.
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        let mut x = self.unit() * total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len() - 1
    }
}



}

// ---- src/roof.rs ----
pub mod roof {
//! The roofs of the two indoor futures, drawn as a cutaway so the shrine
//! stays in view, and the weather that lands on them.
//!
//! - **Greenhouse**: a pitched glass roof on exposed, minimal steel, with
//!   glazed back walls, shafts of sun slanting down through it, rain beading
//!   and running down the panes, snow settling on the glass.
//! - **Research Lab**: a flat ceiling grid of steel and glass with light
//!   panels, and back walls of steel panels, a neon line and a band of
//!   windows with screens set into them.
//!
//! The back walls stand on the two far edges of the plateau (whichever two
//! are far as the camera turns) and are drawn behind the shrine; the roof is
//! drawn over everything, translucent. Rain splashes that would land on the
//! ground land on the roof instead: they are the very same events the
//! weather bed plays, so every one you see is still one you hear.

use crate::framebuffer::{blend, Framebuffer, Rgba};
use crate::iso::{to_px_round, Point};
use crate::light::{bayer, dither};
use crate::plants::Biome;
use crate::raster;
use crate::rng::{hash3, Rng};
use crate::weather::WeatherState;
use crate::{GRID_H, GRID_W};

/// The plateau's edges, in grid units.
const LO: f64 = -0.5;
const HI_X: f64 = GRID_W as f64 - 0.5;
const HI_Y: f64 = GRID_H as f64 - 0.5;
/// How much higher the greenhouse's ridge is than its eaves.
const PITCH: f64 = 2.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roof {
    pub biome: Biome,
    /// The eaves' height, in levels.
    pub z: f64,
}

/// What drawing the roof needs from the frame.
pub struct Ctx<'a> {
    pub proj: &'a dyn Fn(f64, f64, f64) -> Point,
    pub lit: &'a dyn Fn(Rgba) -> Rgba,
    pub seed: u64,
    pub t: f64,
    pub w: WeatherState,
    /// The future's light colour (neon, screens).
    pub accent: Rgba,
    /// The glass's own tint.
    pub glass: Rgba,
}

impl Ctx<'_> {
    fn px(&self, x: f64, y: f64, z: f64) -> (i32, i32) {
        let p = (self.proj)(x, y, z);
        (to_px_round(p.x), to_px_round(p.y))
    }

    fn line(&self, fb: &mut Framebuffer, a: (f64, f64, f64), b: (f64, f64, f64), c: Rgba) {
        let (p, q) = (self.px(a.0, a.1, a.2), self.px(b.0, b.1, b.2));
        raster::line(fb, p.0, p.1, q.0, q.1, c);
    }

    fn quad(&self, q: [(f64, f64, f64); 4]) -> [Point; 4] {
        q.map(|c| (self.proj)(c.0, c.1, c.2))
    }

    /// A light, never dimmed by the weather.
    fn lamp(&self, c: Rgba) -> Rgba {
        let g = (self.lit)(c);
        [0, 1, 2, 3].map(|k| if k == 3 { 255 } else { g[k].max((c[k] as f64 * 0.9) as u8) })
    }
}

/// One edge of the plateau: from `a` to `b` on the ground.
#[derive(Clone, Copy)]
struct Edge {
    a: (f64, f64),
    b: (f64, f64),
}

const EDGES: [Edge; 4] = [
    Edge { a: (LO, LO), b: (LO, HI_Y) },
    Edge { a: (HI_X, LO), b: (HI_X, HI_Y) },
    Edge { a: (LO, LO), b: (HI_X, LO) },
    Edge { a: (LO, HI_Y), b: (HI_X, HI_Y) },
];

impl Roof {
    /// The roof for a biome, over a shrine whose tallest column is `tallest`
    /// levels: clear of it by at least a level and a half.
    pub fn new(biome: Biome, tallest: f64) -> Option<Self> {
        biome.roofed().then(|| Roof { biome, z: (tallest + 1.5).max(5.0) })
    }

    /// The roof's height over plateau point `(x, y)`.
    pub fn height_at(&self, _x: f64, y: f64) -> f64 {
        if self.biome == Biome::Greenhouse {
            let mid = (LO + HI_Y) / 2.0;
            self.z + PITCH * (1.0 - (y - mid).abs() / (mid - LO))
        } else {
            self.z
        }
    }

    /// The two edges that are far from the viewer.
    fn far_edges(&self, cx: &Ctx) -> Vec<Edge> {
        let centre = cx.px((LO + HI_X) / 2.0, (LO + HI_Y) / 2.0, 0.0).1;
        EDGES.iter().copied().filter(|e| cx.px((e.a.0 + e.b.0) / 2.0, (e.a.1 + e.b.1) / 2.0, 0.0).1 < centre).collect()
    }

    /// The back walls, behind the shrine.
    pub fn back(&self, fb: &mut Framebuffer, cx: &Ctx) {
        let steel = (cx.lit)([214, 220, 224, 255]);
        for e in self.far_edges(cx) {
            let n = 12;
            for i in 0..n {
                let f0 = i as f64 / n as f64;
                let f1 = (i + 1) as f64 / n as f64;
                let p = |f: f64| (e.a.0 + (e.b.0 - e.a.0) * f, e.a.1 + (e.b.1 - e.a.1) * f);
                let (p0, p1) = (p(f0), p(f1));
                let (h0, h1) = (self.height_at(p0.0, p0.1), self.height_at(p1.0, p1.1));
                let panel = |z0: f64, z1a: f64, z1b: f64| cx.quad([(p0.0, p0.1, z0), (p1.0, p1.1, z0), (p1.0, p1.1, z1b), (p0.0, p0.1, z1a)]);
                if self.biome == Biome::Greenhouse {
                    // Glazing, with a low base wall.
                    raster::fill_polygon(fb, &panel(0.0, 0.5, 0.5), (cx.lit)([176, 170, 162, 255]));
                    raster::blend_polygon(fb, &panel(0.5, h0, h1), (cx.lit)(cx.glass), 60);
                    for z in [0.5, 2.0, 3.5] {
                        cx.line(fb, (p0.0, p0.1, z), (p1.0, p1.1, z), steel);
                    }
                } else {
                    // Steel panels below, a neon line, windows and screens above.
                    let panel_c = (cx.lit)(if i % 2 == 0 { [120, 130, 140, 255] } else { [110, 120, 130, 255] });
                    raster::fill_polygon(fb, &panel(0.0, 1.4, 1.4), panel_c);
                    let screen = hash3(cx.seed, 0x5343, i as u64) % 4 == 0;
                    if screen {
                        raster::fill_polygon(fb, &panel(1.6, h0 - 0.8, h1 - 0.8), cx.lamp(blend(cx.accent, [8, 24, 30, 255], 35, 100)));
                    } else {
                        raster::blend_polygon(fb, &panel(1.4, h0, h1), (cx.lit)([60, 90, 110, 255]), 120);
                    }
                    cx.line(fb, (p0.0, p0.1, 1.4), (p1.0, p1.1, 1.4), cx.lamp(cx.accent));
                    cx.line(fb, (p0.0, p0.1, h0 - 0.05), (p1.0, p1.1, h1 - 0.05), steel);
                }
                // A mullion or a column at each panel's edge.
                cx.line(fb, (p0.0, p0.1, 0.0), (p0.0, p0.1, h0), steel);
            }
            cx.line(fb, (e.b.0, e.b.1, 0.0), (e.b.0, e.b.1, self.height_at(e.b.0, e.b.1)), steel);
        }
    }

    /// The roof, over everything: glass, its frame, the light coming through
    /// it, and the weather landing on it. `splashes` are the rain's splash
    /// events this frame, `(x, y, age)`.
    pub fn draw(&self, fb: &mut Framebuffer, cx: &Ctx, splashes: &[(f64, f64, f64)], water: Rgba) {
        let w = &cx.w;
        let h = |x: f64, y: f64| self.height_at(x, y);
        let mid = (LO + HI_Y) / 2.0;
        // The roof's planes: two pitches, or one flat ceiling.
        let planes: Vec<[(f64, f64, f64); 4]> = if self.biome == Biome::Greenhouse {
            vec![
                [(LO, LO, h(LO, LO)), (HI_X, LO, h(HI_X, LO)), (HI_X, mid, h(HI_X, mid)), (LO, mid, h(LO, mid))],
                [(LO, mid, h(LO, mid)), (HI_X, mid, h(HI_X, mid)), (HI_X, HI_Y, h(HI_X, HI_Y)), (LO, HI_Y, h(LO, HI_Y))],
            ]
        } else {
            vec![[(LO, LO, self.z), (HI_X, LO, self.z), (HI_X, HI_Y, self.z), (LO, HI_Y, self.z)]]
        };

        // Sun slanting down through the glass.
        if self.biome == Biome::Greenhouse {
            let level = w.light * (1.0 - w.cloud) * (1.0 - 0.5 * w.fog);
            if level > 0.05 {
                let c = (cx.lit)([255, 240, 200, 255]);
                for i in 0..4u64 {
                    let mut r = Rng::new(hash3(cx.seed, 0x5348_4146, i));
                    let (sx, sy) = (r.range(1.0, 18.0), r.range(1.0, 18.0));
                    let wide = r.range(1.0, 2.2);
                    let breathe = 0.6 + 0.4 * crate::math::sin_turns(cx.t * r.range(0.02, 0.05) + r.unit());
                    let top = h(sx, sy);
                    let d = top * 0.45;
                    let q = cx.quad([(sx, sy, top), (sx + wide, sy, top), (sx + wide + d, sy + d, 0.0), (sx + d, sy + d, 0.0)]);
                    raster::blend_polygon(fb, &q, c, (level * breathe * 26.0) as u32);
                }
            }
        }

        // The glass itself, a faint tint.
        let (tint, alpha) = if self.biome == Biome::Greenhouse { (cx.glass, 26) } else { ([150, 190, 206, 255], 20) };
        let tint = (cx.lit)(tint);
        for q in &planes {
            raster::blend_polygon(fb, &cx.quad(*q), tint, alpha);
        }

        // Snow settling on the glass, dithered.
        if w.cover > 0.02 {
            let snow = (cx.lit)([240, 244, 252, 255]);
            for q in &planes {
                raster::for_polygon(&cx.quad(*q), |x, y| {
                    if bayer(x, y) < w.cover * 0.8 {
                        fb.blend_over(x, y, snow, 170);
                    }
                });
            }
        }

        // The frame.
        let steel = (cx.lit)(if self.biome == Biome::Greenhouse { [232, 236, 238, 255] } else { [86, 96, 108, 255] });
        let fine = (cx.lit)(if self.biome == Biome::Greenhouse { [210, 216, 220, 255] } else { [110, 122, 134, 255] });
        if self.biome == Biome::Greenhouse {
            // Rafters every two cells, heavier every six; purlins every four.
            let mut x = LO;
            while x <= HI_X + 1e-9 {
                let c = if ((x - LO) as i64) % 6 == 0 { steel } else { fine };
                cx.line(fb, (x, LO, h(x, LO)), (x, mid, h(x, mid)), c);
                cx.line(fb, (x, mid, h(x, mid)), (x, HI_Y, h(x, HI_Y)), c);
                x += 2.0;
            }
            let mut y = LO;
            while y <= HI_Y + 1e-9 {
                cx.line(fb, (LO, y, h(LO, y)), (HI_X, y, h(HI_X, y)), if (y - mid).abs() < 0.1 { steel } else { fine });
                y += 4.0;
            }
            cx.line(fb, (LO, mid, h(LO, mid)), (HI_X, mid, h(HI_X, mid)), steel);
            // Glints where the sun catches a pane.
            for i in 0..6u64 {
                let mut r = Rng::new(hash3(cx.seed, 0x474c_494e, i));
                let (gx, gy) = (r.range(0.0, 23.0), r.range(0.0, 23.0));
                let on = crate::math::sin_turns(cx.t * 0.05 + r.unit()) > 0.6;
                if on && w.light > 0.4 {
                    let (px, py) = cx.px(gx, gy, h(gx, gy));
                    for k in -2..=2 {
                        fb.blend_over(px + k, py - k / 2, [255, 255, 245, 255], 150 - 30 * (k as i32).unsigned_abs());
                    }
                }
            }
        } else {
            // A grid of beams every three cells, light panels in the bays.
            let lamp = cx.lamp([226, 246, 255, 255]);
            for by in 0..8 {
                for bx in 0..8 {
                    let (x0, y0) = (LO + bx as f64 * 3.0, LO + by as f64 * 3.0);
                    if (bx + by) % 2 == 0 {
                        let q = cx.quad([(x0 + 0.7, y0 + 1.2, self.z), (x0 + 2.3, y0 + 1.2, self.z), (x0 + 2.3, y0 + 1.8, self.z), (x0 + 0.7, y0 + 1.8, self.z)]);
                        raster::blend_polygon(fb, &q, lamp, 150);
                    }
                }
            }
            for k in 0..=8 {
                let v = LO + k as f64 * 3.0;
                cx.line(fb, (v, LO, self.z), (v, HI_Y, self.z), steel);
                cx.line(fb, (LO, v, self.z), (HI_X, v, self.z), steel);
            }
            // A neon line round the ceiling's edge.
            let neon = cx.lamp(cx.accent);
            for e in EDGES {
                cx.line(fb, (e.a.0, e.a.1, self.z), (e.b.0, e.b.1, self.z), neon);
            }
        }
        // Columns down the near edges, every six cells (the far ones stand
        // with the back walls).
        let far = self.far_edges(cx);
        for e in EDGES {
            if far.iter().any(|f| f.a == e.a && f.b == e.b) {
                continue;
            }
            for k in 0..=4 {
                let f = k as f64 / 4.0;
                let (x, y) = (e.a.0 + (e.b.0 - e.a.0) * f, e.a.1 + (e.b.1 - e.a.1) * f);
                cx.line(fb, (x, y, 0.0), (x, y, h(x, y)), steel);
            }
            cx.line(fb, (e.a.0, e.a.1, h(e.a.0, e.a.1)), (e.b.0, e.b.1, h(e.b.0, e.b.1)), steel);
        }

        // Rain on the glass: beads running down the panes, and the splashes.
        if w.rain > 0.02 {
            let bead = (cx.lit)([236, 244, 255, 255]);
            let n = (w.rain * 90.0) as u64;
            for i in 0..n {
                let mut r = Rng::new(hash3(cx.seed, 0x4245_4144, i));
                let x = r.range(LO, HI_X);
                let (y, alpha) = if self.biome == Biome::Greenhouse {
                    // Running down from the ridge to the eaves.
                    let side = if r.chance(0.5) { 1.0 } else { -1.0 };
                    let run = (cx.t * r.range(0.4, 1.2) + r.unit() * 12.0) % (mid - LO);
                    (mid + side * run, 130)
                } else {
                    // Beads gathering and letting go on the flat ceiling.
                    let life = (cx.t * r.range(0.2, 0.5) + r.unit()) % 1.0;
                    (r.range(LO, HI_Y), (dither(1.0 - life, 4.0, 0.5) * 140.0) as u32)
                };
                let (px, py) = cx.px(x, y, h(x, y));
                fb.blend_over(px, py, bead, alpha);
                fb.blend_over(px, py + 1, bead, alpha / 2);
            }
            for &(x, y, age) in splashes {
                let p = (cx.proj)(x, y, h(x, y));
                crate::weather_fx::splash(fb, p, age, true, water);
            }
        }
    }
}



}

// ---- src/scene.rs ----
pub mod scene {
//! Scene assembly: everything needed to draw one frame.
//!
//! A [`Scene`] owns the generated shrine, its plants and its weather, and turns
//! them into pixels in depth order. The *simulation state* it draws from —
//! [`SimState`] — is the same shared truth the music engine uses (see
//! [`crate::sim`]): weather lights and fogs the palette, sways the plants and
//! sets the rain, and the drone's slow arc brightens the altar. The 300x300
//! logical buffer never changes size (`HC003`); under load the [`Quality`] level
//! drops *effects*, not resolution.

use crate::arch::ArchTransform;
use crate::framebuffer::{blend, shade, Framebuffer, Rgba};
use crate::iso::{to_px_round, Iso, Point, FP};
use crate::math::sin_turns;
use crate::palette::Palette;
use crate::plants::{role, Biome, Plant, Plants};
use crate::raster;
use crate::shrine::{kind, Shrine};
use crate::weather::{EventKind, Weather, WeatherKind, WeatherState};
use crate::light::{self, Rig};
use crate::material::{self, Exposure, Materials};
use crate::weather_fx;
use crate::wildlife::Wildlife;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};


/// How much of the scene to draw. The buffer size is fixed (`HC003`); this only
/// controls effect density.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    High,
    Medium,
    Low,
}

impl Quality {
    /// Draw every `stride`-th plant on a cell.
    pub fn plant_stride(self) -> usize {
        match self {
            Quality::Low => 2,
            _ => 1,
        }
    }

    /// Fraction of the weather's other particles to draw.
    pub fn particle_fraction(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Medium => 0.6,
            Quality::Low => 0.3,
        }
    }

    /// Fraction of the rain field to draw.
    pub fn rain_fraction(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Medium => 0.5,
            Quality::Low => 0.25,
        }
    }
}

/// What the visuals read back from the shared simulation, in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualCoupling {
    /// Foliage sway gain (driven by wind).
    pub foliage_sway: f64,
    /// Rain streak density.
    pub rain_density: f64,
    /// Altar glow (driven by the drone's slow arc).
    pub glow: f64,
}

/// The slow control signals the music is built from, in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instruments {
    pub drone: f64,
    pub loops: [f64; 4],
    /// A decaying accent each beat, for pulsing detail.
    pub pulse: f64,
}

/// A bounded camera nudge from the pointer, in `[-1, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
}

/// The single source of truth for one instant, shared by audio and visuals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimState {
    pub time: f64,
    pub weather: WeatherState,
    pub coupling: VisualCoupling,
    pub instruments: Instruments,
    pub camera: Camera,
    /// Whether to fill the sky. The page clears this so its own sky (and the
    /// source waterfall behind the shrine) can show through.
    pub draw_sky: bool,
    /// How the architecture is turned and placed (`arch::ArchTransform`).
    pub arch: ArchTransform,
    /// Camera orbit, in turns: the whole scene turning about the shrine. Driven
    /// by the camera rotation speed and the clock.
    pub orbit_turns: f64,
}

impl SimState {
    /// A state derived only from weather (used by the plain preview, where the
    /// music's arcs are not in play).
    pub fn from_weather(time: f64, weather: WeatherState) -> Self {
        SimState {
            time,
            weather,
            coupling: VisualCoupling {
                foliage_sway: weather.wind_strength,
                rain_density: weather.rain,
                glow: 1.0,
            },
            instruments: Instruments { drone: 1.0, loops: [1.0; 4], pulse: 0.0 },
            camera: Camera::default(),
            draw_sky: true,
            arch: ArchTransform::IDENTITY,
            orbit_turns: 0.0,
        }
    }
}

/// The whole piece at one seed and setting.
#[derive(Clone, Debug)]
pub struct Scene {
    pub seed: u64,
    pub biome: Biome,
    pub palette: Palette,
    pub shrine: Shrine,
    pub plants: Plants,
    pub weather: Weather,
    /// The creatures that can be seen, when the Wildlife setting is on.
    pub wildlife: Option<Wildlife>,
    /// The built futures' street furniture and machines (none in the wild).
    pub props: crate::props::Props,
    /// The indoor futures' roof.
    pub roof: Option<crate::roof::Roof>,
}

impl Scene {
    pub fn new(seed: u64, biome: Biome, weather_kind: WeatherKind) -> Self {
        let shrine = Shrine::generate(seed);
        let plants = Plants::generate(seed, &shrine, biome);
        let weather = Weather::new(weather_kind, seed);
        let roof = crate::roof::Roof::new(biome, tallest(&shrine));

        Scene {
            seed,
            biome,
            palette: Palette::for_biome(seed, biome),
            shrine,
            plants,
            weather,
            wildlife: None,
            props: crate::props::Props::generate(seed, biome),
            roof,
        }
    }

    /// The palette's sky gradient endpoints, for a page that draws its own sky.
    pub fn sky_colors(&self) -> (Rgba, Rgba) {
        (self.palette.sky_top, self.palette.sky_bottom)
    }

    /// Use an authored architecture instead of the generated one.
    ///
    /// A no-op for an empty model, so the generator stays authoritative by
    /// default and nothing about today's output moves (`HC004`). Plants are
    /// regenerated against the new structure, so they still grow on it.
    pub fn author(&mut self, model: &crate::shrine_model::ShrineModel) {
        if !model.is_authored() {
            return;
        }
        self.shrine = Shrine::from_model(self.seed, model);
        self.plants = Plants::generate(self.seed, &self.shrine, self.biome);
        self.roof = crate::roof::Roof::new(self.biome, tallest(&self.shrine));
    }

    /// The weather at `t` seconds, from this scene's own weather.
    pub fn weather_state(&self, t: f64) -> WeatherState {
        self.weather.state(t)
    }

    /// Draw the scene at `t` seconds using only its weather (convenience for
    /// previews). The music's arcs are assumed full.
    pub fn render(&self, fb: &mut Framebuffer, t: f64, quality: Quality) {
        self.render_state(fb, &SimState::from_weather(t, self.weather.state(t)), quality);
    }

    /// Draw the scene from a shared [`SimState`]. This is the coupled path the
    /// artifact uses: audio and visuals evaluate the same state.
    ///
    /// Returns the two lit sky endpoints actually used, so a page that draws its
    /// own sky (to put the source waterfall behind the shrine) can match it
    /// exactly, weather lighting included.
    pub fn render_state(&self, fb: &mut Framebuffer, state: &SimState, quality: Quality) -> (Rgba, Rgba) {
        let w = state.weather;
        let mut iso = Iso::grid_24();
        iso.cam_yaw = state.camera.yaw;
        iso.cam_pitch = state.camera.pitch;

        // Sky gradient (drawn only when the scene owns the sky; the page may
        // draw it instead, so the source waterfall shows through behind the
        // shrine). The lit endpoints are returned either way.
        let (sky_top, sky_bottom) = self.sky_colors_at(&w);
        if state.draw_sky {
            let last = fb.height() as i32 - 1;
            for y in 0..=last {
                let f = y as f64 / last as f64;
                fb.fill_span(y, 0, fb.width() as i32 - 1, lerp(sky_top, sky_bottom, f));
            }
        } else {
            // The page draws its own sky, so nothing here covers that region:
            // start clear, or the previous frame's shrine lingers as a ghost.
            fb.clear([0, 0, 0, 0]);
        }

        // Everything the weather draws, and the grade it lights the scene with.
        let lit = |c: Rgba| self.lit(c, &w);
        let fx = weather_fx::Fx {
            seed: self.seed,
            t: state.time,
            w,
            weather: &self.weather,
            palette: &self.palette,
            fraction: quality.particle_fraction(),
            rain_fraction: quality.rain_fraction(),
            grade: &lit,
        };
        // The far layer sits behind the shrine: clouds, shafts of light, the
        // bolt, fog banks and the distant rain and snow.
        fx.far(fb);

        // Camera orbit, if the shrine is being turned under the viewer. The
        // world vertical is untouched, so plants still stand up and only the
        // horizontal plane turns.
        let arch = state.arch;
        let orbit = state.orbit_turns;
        let general = !arch.is_unrotated() || orbit != 0.0;
        let (rot, off) = arch.world(orbit);
        let (ground_rot, ground_off) = ArchTransform::IDENTITY.world(orbit);

        // Ground plane. When the camera orbits, the cells are turned with it, so
        // they go through the general quad path; otherwise they keep the exact
        // axis-aligned diamonds.
        let (ox, oy) = self.shrine.origin();
        // How far a cell descends below the ground (0 for most).
        let depth_at = |gx: i32, gy: i32| -> u32 {
            let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
            if inside {
                self.shrine.depth_at(gx - ox, gy - oy) as u32
            } else {
                0
            }
        };
        // Cells in depth order: each column, then the plants growing on it.
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); (GRID_W * GRID_H) as usize];
        for (i, p) in self.plants.items.iter().enumerate() {
            buckets[(p.gy * GRID_W as i32 + p.gx) as usize].push(i);
        }
        // Painter's key. A turned column can overlap its neighbours, so the
        // order has to follow the *rotated* position, not the grid's.
        let key = |gx: i32, gy: i32, height: u32| -> f64 {
            if !general {
                // Exactly the old `depth_key` order: by (gx + gy) only, with a
                // stable sort keeping the scan order for ties. Adding anything
                // else here would reorder columns within a diagonal.
                return (gx + gy) as f64;
            }
            let (x, y) = (gx as f64, gy as f64);
            let z = height.saturating_sub(1) as f64 / 2.0;
            let wx = rot[0][0] * x + rot[0][1] * y + rot[0][2] * z + off.0;
            let wy = rot[1][0] * x + rot[1][1] * y + rot[1][2] * z + off.1;
            let wz = rot[2][0] * x + rot[2][1] * y + rot[2][2] * z + off.2;
            (wx + wy) * 1024.0 + wz
        };
        let mut cells: Vec<(f64, i32, i32, u32, u32)> = Vec::new();
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
                let height = if inside { self.shrine.height_at(gx - ox, gy - oy) as u32 } else { 0 };
                let depth = depth_at(gx, gy);
                let has_plants = !buckets[(gy * GRID_W as i32 + gx) as usize].is_empty() || self.props.any_in(gx, gy);
                if height == 0 && depth == 0 && !has_plants {
                    continue;
                }
                cells.push((key(gx, gy, height), gx, gy, height, depth));
            }
        }
        cells.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        // A whole-cell world offset is just a moved projection origin, so the
        // unrotated path below stays pixel-exact however far the shrine moves.
        // Only the horizontal part goes into the origin: the vertical part is
        // added to the column's levels, so it can be split at the ground plane.
        let shrine_iso = {
            let shift = iso.offset_shift(arch.off_x as i32, arch.off_y as i32, 0);
            Iso { origin_x: iso.origin_x + shift.x, origin_y: iso.origin_y + shift.y, ..iso }
        };
        let lift = arch.off_z as i32;

        let stride = quality.plant_stride();

        // The light for this frame: the weather's sun and sky light, shadows
        // cast from the shrine's heights in scene space (model plus offset,
        // before the orbit), and the altar glowing with the drone's arc.
        let (arch_rot, arch_off) = arch.world(0.0);
        let to_scene = |x: f64, y: f64, z: f64| -> [f64; 3] {
            if arch.is_unrotated() {
                [x + arch_off.0, y + arch_off.1, z + arch_off.2]
            } else {
                let p = crate::iso::to_world(x, y, z, &arch_rot, arch_off);
                [p.0, p.1, p.2]
            }
        };
        let mut tops = vec![0.0f64; (GRID_W * GRID_H) as usize];
        let mut altar: Option<([f64; 3], f64)> = None;
        for &(_, gx, gy, height, _) in &cells {
            if height == 0 {
                continue;
            }
            let top = height as f64 - 1.0;
            let c = to_scene(gx as f64, gy as f64, top);
            let (sx, sy) = (c[0].round() as i32, c[1].round() as i32);
            if sx >= 0 && sy >= 0 && sx < GRID_W as i32 && sy < GRID_H as i32 {
                let t = &mut tops[(sy * GRID_W as i32 + sx) as usize];
                *t = t.max(c[2].max(0.0));
            }
            // An altar sunk below the plateau is hidden by it, light and all.
            if self.shrine.layers.get(gx, gy, height as i32 - 1).kind == kind::ALTAR && c[2] >= 0.0 {
                let a = altar.get_or_insert(([0.0; 3], 0.0));
                for k in 0..3 {
                    a.0[k] += c[k];
                }
                a.1 += 1.0;
            }
        }
        let materials = Materials::new(self.seed, &self.palette, self.biome);
        let rig = Rig::new(light::Setup {
            kind: self.weather.kind,
            biome: self.biome,
            tops,
            cast: arch.is_unrotated(),
            orbit,
            glow_at: altar.map(|(sum, n)| [sum[0] / n, sum[1] / n, sum[2] / n + 0.3]),
            glow: state.coupling.glow.clamp(0.0, 1.0),
            // The altar's light is the green-blue of its ornament (a
            // future's own light colour in the built biomes).
            accent: materials.glow(),
            fog: self.palette.fog,
            fog_level: w.fog,
            flash: w.flash,
            t: state.time,
            wind: w.wind_strength,
        });
        let grade = Grade::new(self, &w);
        // Under a roof nothing settles on the ground or the architecture.
        let exposure = if self.biome.roofed() { Exposure { cover: 0.0, rain: 0.0 } } else { Exposure { cover: w.cover, rain: w.rain } };
        let neon = self.biome.futuristic();
        // A face's normal in scene space: turned with the architecture.
        let normal = |face: raster::Face| -> [f64; 3] {
            let n = face.normal();
            if arch.is_unrotated() {
                return n;
            }
            [0, 1, 2].map(|i| arch_rot[i][0] * n[0] + arch_rot[i][1] * n[1] + arch_rot[i][2] * n[2])
        };

        // A cell's block: the model levels it spans and what it is made of. A
        // raised column runs from the ground up to its top tile; a descending
        // one from its depth up to its cap, which sits at ground level.
        let block = |gx: i32, gy: i32, height: u32, depth: u32| -> Option<((i32, i32), material::Block)> {
            let (range, layer) = if height > 0 {
                ((0, height as i32 - 1), height as i32 - 1)
            } else if depth > 0 {
                ((-(depth as i32), 0), 0)
            } else {
                return None;
            };
            let tile = self.shrine.layers.get(gx, gy, layer);
            Some((range, material::Block { kind: tile.kind, variant: tile.variant, gx, gy, top: range.1 as f64 }))
        };
        // The textured, lit colour of one pixel of a block or of the ground.
        let shade_block = |b: &material::Block, s: &raster::Surface| -> Rgba {
            let alb = materials.albedo(b, s, exposure);
            let c = if b.kind == kind::EMPTY {
                grade.apply(rig.shade(s, [s.x, s.y, 0.0], [0.0, 0.0, 1.0], &alb))
            } else {
                grade.apply(rig.shade(s, to_scene(s.x, s.y, s.z), normal(s.face), &alb))
            };
            if neon && alb.emissive > 0.0 {
                // A future's lights are lit from within: the weather's grade
                // can dim the scene around them, never the lights themselves.
                let e = (0.55 + 0.4 * alb.emissive).min(0.95);
                return [0, 1, 2, 3].map(|k| if k == 3 { 255 } else { c[k].max((alb.colour[k] as f64 * e) as u8) });
            }
            c
        };

        // Shading is the costly part, and in painter's order much of what is
        // drawn is later drawn over. So the geometry goes through twice: first
        // only to find which draw ends up owning each pixel, then for real,
        // shading a pixel only in the draw that keeps it. The second pass runs
        // the same draws in the same order, with the effects and plants in
        // between, so the picture is exactly what painting everything would
        // give.
        let bounds = raster::Bounds::of(fb);
        let mut owner = vec![u32::MAX; (bounds.w * bounds.h) as usize];
        // On the turned path one sort key per column cannot order every
        // overlap correctly (tall and short, tilted blocks), so there the
        // first pass keeps the *nearest* surface at each pixel, by true depth.
        // Upright, the painter's order is exact and is kept, pixel for pixel.
        let mut nearest = if general { vec![f64::NEG_INFINITY; owner.len()] } else { Vec::new() };
        let at = |s: &raster::Surface| (s.py * bounds.w + s.px) as usize;
        // Draw one half of a block, either side of the ground plane (z = 0),
        // in either pass.
        let draw = |pass: &mut Pass, id: u32, b: &material::Block, (lo, hi): (i32, i32), half: raster::Half| {
            let mut sink = |s: &raster::Surface| match pass {
                Pass::Find(o, near) => {
                    let i = at(s);
                    if near.is_empty() {
                        o[i] = id;
                    } else if s.depth >= near[i] - DEPTH_TIE {
                        // On a tie (a shared edge, coplanar tiles) the later
                        // draw wins, as the painter's order would have it.
                        near[i] = s.depth.max(near[i]);
                        o[i] = id;
                    }
                }
                Pass::Paint(fb, o) => {
                    if o[at(s)] == id {
                        fb.put(s.px, s.py, shade_block(b, s));
                    }
                }
            };
            let (gx, gy) = (b.gx, b.gy);
            if b.kind == kind::EMPTY {
                // The plateau: one flat cell, turned only by the camera orbit.
                if general {
                    raster::fill_box_shaded(bounds, &iso, &ground_rot, ground_off, gx, gy, (0.0, 0.0), raster::Half::All, &mut sink);
                } else {
                    let c = iso.project(gx, gy, 0);
                    let centre = (gx as f64, gy as f64, 0.0);
                    raster::fill_diamond_shaded(bounds, c.x, c.y, iso.half_w(), iso.half_h(), centre, raster::Face::Top, &mut sink);
                }
                return;
            }
            if general {
                let range = (lo as f64, hi as f64);
                raster::fill_box_shaded(bounds, &iso, &rot, off, gx, gy, range, half, &mut sink);
                return;
            }
            let (lo, hi) = (lo + lift, hi + lift);
            let (lo, hi) = match half {
                raster::Half::All => (lo, hi),
                // A cap exactly at ground level belongs to the upper half.
                raster::Half::Above => (lo.max(0), hi),
                raster::Half::Below if lo < 0 => (lo, hi.min(0)),
                raster::Half::Below => return,
            };
            raster::fill_column_range_shaded(bounds, &shrine_iso, gx, gy, lo, hi, lift, &mut sink);
        };

        // The plateau, whole: no cell is left out of it, so nothing can show
        // through a hole in the ground. It is earth and grass, lit like
        // everything else, so it takes the shrine's cast shadows and the
        // passing cloud shadows.
        let plateau: Vec<material::Block> = (0..GRID_H as i32)
            .flat_map(|gy| (0..GRID_W as i32).map(move |gx| material::Block { kind: kind::EMPTY, variant: 0, gx, gy, top: 0.0 }))
            .collect();
        let below: Vec<((i32, i32), material::Block)> =
            cells.iter().filter_map(|&(_, gx, gy, height, depth)| block(gx, gy, height, depth)).collect();

        // First pass: who owns each pixel. Everything below the ground plane
        // comes first, then the plateau over it (hiding it wherever the
        // plateau is, and still showing it past the plateau's edge, which has
        // no thickness), then the architecture above the ground.
        let mut id = 0u32;
        {
            let mut find = Pass::Find(&mut owner, &mut nearest);
            for (range, b) in &below {
                id += 1;
                draw(&mut find, id, b, *range, raster::Half::Below);
            }
            for g in &plateau {
                id += 1;
                draw(&mut find, id, g, (0, 0), raster::Half::All);
            }
            for (range, b) in &below {
                id += 1;
                draw(&mut find, id, b, *range, raster::Half::Above);
            }
        }

        // Second pass, for real: the same draws, in the same order.
        let mut id = 0u32;
        for (range, b) in &below {
            id += 1;
            draw(&mut Pass::Paint(fb, &owner), id, b, *range, raster::Half::Below);
        }
        for g in &plateau {
            id += 1;
            draw(&mut Pass::Paint(fb, &owner), id, g, (0, 0), raster::Half::All);
        }

        // A point of the plateau's space at height `z`, on screen: where the
        // props stand and the roof is built.
        let proj3 = |x: f64, y: f64, z: f64| -> Point {
            if general {
                iso.project_rotated(x, y, z, &ground_rot, ground_off)
            } else {
                iso.project_world(x, y, z)
            }
        };
        // The indoor futures' back walls stand behind everything else.
        let roof_cx = self.roof.map(|_| crate::roof::Ctx {
            proj: &proj3,
            lit: &lit,
            seed: self.seed,
            t: state.time,
            w,
            accent: materials.glow(),
            glass: blend(self.palette.sky_bottom, [214, 236, 224, 255], 60, 100),
        });
        if let (Some(r), Some(c)) = (&self.roof, &roof_cx) {
            r.back(fb, c);
        }
        let night = ((1.0 - w.light)
            + if self.weather.kind == WeatherKind::Dusk { 0.5 } else { 0.0 }
            + if self.biome == Biome::ResearchLab { 0.3 } else { 0.0 })
        .clamp(0.0, 1.0);
        let foliage = [self.palette.foliage_dark, self.palette.foliage, self.palette.foliage_light];

        // Where a point of the world lands on screen: on the plateau, or on the
        // architecture at model level `z` (which moves with the transform).
        let on_ground = |x: f64, y: f64| -> Point {
            if general {
                iso.project_rotated(x, y, 0.0, &ground_rot, ground_off)
            } else {
                iso.project_world(x, y, 0.0)
            }
        };
        let on_shrine = |x: f64, y: f64, z: f64| -> Point {
            if general {
                iso.project_rotated(x, y, z, &rot, off)
            } else {
                iso.project_world(x + arch.off_x as f64, y + arch.off_y as f64, z + arch.off_z as f64)
            }
        };
        // Whether a point of the architecture is above the ground plane: a
        // column sunk below it has no top for rain to land on or drip from.
        let above_ground = |x: f64, y: f64, z: f64| -> bool {
            let wz = if general { crate::iso::to_world(x, y, z, &rot, off).2 } else { z + arch.off_z as f64 };
            wz >= 0.0
        };
        // Splashes and drips, each found from the shared schedule so the weather
        // bed plays the very same ones. Those on a column are drawn with it, so
        // nearer columns hide them; those on the ground go on the plateau now.
        let water = self.lit(blend(self.palette.sky_bottom, [240, 245, 255, 255], 85, 100), &w);
        let mut on_cell: Vec<Vec<(Point, f64, f64, bool)>> = vec![Vec::new(); (GRID_W * GRID_H) as usize];
        // Under a roof the rain lands on the glass, not the ground.
        let mut on_roof: Vec<(f64, f64, f64)> = Vec::new();
        for (e, age) in weather_fx::live(&self.weather, EventKind::Splash, state.time, weather_fx::SPLASH_S) {
            let (x, y) = e.spot();
            if self.roof.is_some() {
                on_roof.push((x, y, age));
                continue;
            }
            match self.column_at(x, y).filter(|c| above_ground(x, y, c.2 as f64 - 1.0)) {
                Some((gx, gy, h)) => {
                    let at = on_shrine(x, y, h as f64 - 1.0);
                    on_cell[(gy * GRID_W as i32 + gx) as usize].push((at, age, -1.0, true));
                }
                None => weather_fx::splash(fb, on_ground(x, y), age, false, water),
            }
        }
        let columns = self.columns();
        if !columns.is_empty() && self.roof.is_none() {
            for (e, age) in weather_fx::live(&self.weather, EventKind::Drip, state.time, weather_fx::DRIP_S) {
                let (gx, gy, h) = columns[(e.hash % columns.len() as u64) as usize];
                if !above_ground(gx as f64 + 0.5, gy as f64 + 0.5, h as f64 - 1.0) {
                    continue;
                }
                // From the front corner of the column's top, down its face.
                let at = on_shrine(gx as f64 + 0.5, gy as f64 + 0.5, h as f64 - 1.0);
                let fall = (h as f64 - 1.0) * iso.level_h as f64 / FP as f64;
                on_cell[(gy * GRID_W as i32 + gx) as usize].push((at, age, fall, false));
            }
        }

        for (_, gx, gy, height, depth) in cells {
            // The upper half of each block, over the plateau. For a descending
            // column that is just its cap, flush with the ground.
            if let Some((range, b)) = block(gx, gy, height, depth) {
                id += 1;
                draw(&mut Pass::Paint(fb, &owner), id, &b, range, raster::Half::Above);
            }
            for &(at, age, fall, splash) in &on_cell[(gy * GRID_W as i32 + gx) as usize] {
                if splash {
                    weather_fx::splash(fb, at, age, true, water);
                } else {
                    weather_fx::drip(fb, at, fall, age, water);
                }
            }
            for i in self.props.in_cell(gx, gy) {
                let p = &self.props.items[i];
                let base = proj3(p.x, p.y, 0.0);
                let light = rig.on_plant(p.x, p.y, to_px_round(base.x), to_px_round(base.y));
                let pcx = crate::props::Ctx { proj: &proj3, lit: &lit, t: state.time, wind: w.wind_strength, night, light, roof: self.roof.as_ref() };
                crate::props::draw(fb, &self.props, i, &pcx, self.palette.accent, foliage);
                // A wire is strung when the nearer of its two poles is drawn.
                for &(a, b) in &self.props.wires {
                    let other = if a == i { b } else if b == i { a } else { continue };
                    let o = &self.props.items[other];
                    if (o.gx + o.gy, other) < (p.gx + p.gy, i) {
                        crate::props::wire(fb, &self.props.items[a], &self.props.items[b], &pcx);
                    }
                }
            }
            for (n, &i) in buckets[(gy * GRID_W as i32 + gx) as usize].iter().enumerate() {
                if n % stride != 0 {
                    continue;
                }
                let plant = &self.plants.items[i];
                let base = if general {
                    iso.project_rotated(plant.gx as f64, plant.gy as f64, plant.z as f64, &ground_rot, ground_off)
                } else {
                    iso.project(plant.gx, plant.gy, plant.z)
                };
                let light = rig.on_plant(plant.gx as f64, plant.gy as f64, to_px_round(base.x), to_px_round(base.y));
                self.draw_plant(fb, plant, state, base, light);
            }
        }

        // A soft, dithered halo of the altar's light over everything near it.
        if let Some((g, k)) = rig.glow() {
            let at = if general { iso.project_rotated(g[0], g[1], g[2], &ground_rot, ground_off) } else { iso.project_world(g[0], g[1], g[2]) };
            let (cx, cy) = (to_px_round(at.x), to_px_round(at.y));
            let halo = blend(self.lit(materials.glow(), &w), [255, 250, 235, 255], 30, 100);
            let r = 11;
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f64).sqrt() / r as f64;
                    if d >= 1.0 {
                        continue;
                    }
                    let a = light::dither(k * (1.0 - d) * (1.0 - d) * 0.55, 4.0, light::bayer(x, y));
                    fb.blend_over(x, y, halo, (a * 85.0) as u32);
                }
            }
        }

        // The roof, over the shrine, with the weather landing on it.
        if let (Some(r), Some(c)) = (&self.roof, &roof_cx) {
            r.draw(fb, c, &on_roof, water);
        }

        // The near layer, over everything: the close rain, snow, leaves,
        // motes, embers and sparks, then the low mist.
        fx.near(fb, &on_ground);
        if let Some(wildlife) = &self.wildlife {
            wildlife.draw(fb, self.weather.kind, state.time, &w, &lit);
        }

        (sky_top, sky_bottom)
    }

    /// Weather lighting, fog, the kind's colour grade and any lightning flash,
    /// applied to one colour. Everything drawn goes through this, the sky
    /// included, so the page's own sky matches.
    fn lit(&self, c: Rgba, w: &WeatherState) -> Rgba {
        self.lit_with(&Profiles::of(self.weather.kind, self.biome), c, w)
    }

    /// [`Scene::lit`] with the weather's grade worked out once (it is asked
    /// for every textured pixel).
    fn lit_with(&self, g: &Profiles, c: Rgba, w: &WeatherState) -> Rgba {
        let p = &g.weather;
        let brightness = (45.0 + 55.0 * self.light_level(w)) as u32;
        let fog = (w.fog * 45.0) as u32;
        // The fog is lit by the same light: in a storm it is grey, not glowing.
        let c = blend(shade(c, brightness, 100), shade(self.palette.fog, 25 + brightness * 3 / 4, 100), fog, 100);
        // The grade shifts the hue toward the tint without lifting a dark
        // palette: each channel is scaled by the tint's own balance.
        let mean = (p.tint[0] as u32 + p.tint[1] as u32 + p.tint[2] as u32).max(1);
        let tone = |k: usize| {
            let f = 100 * 3 * p.tint[k] as u32 / mean; // 100 = unchanged
            let scale = 100 * (100 - p.grade) + f * p.grade; // in 10000ths
            ((c[k] as u32 * scale) / 10_000).min(255) as u8
        };
        let c = [tone(0), tone(1), tone(2), c[3]];
        // The weather's vividness: saturation about the grey, then contrast
        // about the middle, so each weather holds together as one palette.
        let lp = &g.light;
        let mean = (c[0] as f64 + c[1] as f64 + c[2] as f64) / 3.0;
        let grade = |v: u8| {
            let v = mean + (v as f64 - mean) * lp.saturation;
            (128.0 + (v - 128.0) * lp.contrast).clamp(0.0, 255.0) as u8
        };
        let c = [grade(c[0]), grade(c[1]), grade(c[2]), c[3]];
        blend(c, [235, 240, 255, 255], (w.flash * 65.0) as u32, 100)
    }

    /// How much light reaches the scene, 0-1: the weather's own, except under
    /// a roof, where the lab's ceiling lights keep it bright and the
    /// greenhouse's glass loses a little less of it.
    fn light_level(&self, w: &WeatherState) -> f64 {
        match self.biome {
            Biome::ResearchLab => 0.6 + 0.4 * w.light,
            Biome::Greenhouse => 0.2 + 0.8 * w.light,
            _ => w.light,
        }
    }

    /// The two lit sky endpoints for a weather state. Dusk lowers a warm sun
    /// into the bottom of the sky and deepens the top.
    pub fn sky_colors_at(&self, w: &WeatherState) -> (Rgba, Rgba) {
        let (mut top, mut bottom) = (self.palette.sky_top, self.palette.sky_bottom);
        if self.weather.kind == WeatherKind::Dusk {
            top = blend(top, [70, 46, 108, 255], 45, 100);
            bottom = blend(bottom, [255, 150, 92, 255], 58, 100);
        }
        // Cloud cover darkens the sky, the top most.
        let top = shade(top, 100 - (w.cloud * 40.0) as u32, 100);
        let bottom = shade(bottom, 100 - (w.cloud * 22.0) as u32, 100);
        (self.lit(top, w), self.lit(bottom, w))
    }

    /// The column standing at grid point `(x, y)`, if any: its cell and height.
    /// Where the top surface of grid cell `(gx, gy)` is on screen in
    /// `state`, in logical pixels, and its level: the top of the shrine's
    /// column there, or the ground. The same placement `render_state` gives a
    /// rain splash, through the same camera, orbit and architecture
    /// transform, so anything put there (the page's @ orbs) stays on it.
    /// Off the grid: `None`.
    pub fn surface_point(&self, state: &SimState, gx: i32, gy: i32) -> Option<(f64, f64, f64)> {
        if gx < 0 || gy < 0 || gx >= GRID_W as i32 || gy >= GRID_H as i32 {
            return None;
        }
        let mut iso = Iso::grid_24();
        iso.cam_yaw = state.camera.yaw;
        iso.cam_pitch = state.camera.pitch;
        let arch = state.arch;
        let orbit = state.orbit_turns;
        let general = !arch.is_unrotated() || orbit != 0.0;
        let (rot, off) = arch.world(orbit);
        let (ground_rot, ground_off) = ArchTransform::IDENTITY.world(orbit);
        let (x, y) = (gx as f64, gy as f64);
        let above_ground = |z: f64| -> bool {
            let wz = if general { crate::iso::to_world(x, y, z, &rot, off).2 } else { z + arch.off_z as f64 };
            wz >= 0.0
        };
        let (at, level) = match self.column_at(x, y).filter(|c| above_ground(c.2 as f64 - 1.0)) {
            Some((_, _, h)) => {
                let z = h as f64 - 1.0;
                let p = if general {
                    iso.project_rotated(x, y, z, &rot, off)
                } else {
                    iso.project_world(x + arch.off_x as f64, y + arch.off_y as f64, z + arch.off_z as f64)
                };
                (p, h as f64)
            }
            None => {
                let p = if general { iso.project_rotated(x, y, 0.0, &ground_rot, ground_off) } else { iso.project_world(x, y, 0.0) };
                (p, 0.0)
            }
        };
        Some((at.x as f64 / FP as f64, at.y as f64 / FP as f64, level))
    }

    pub fn column_at(&self, x: f64, y: f64) -> Option<(i32, i32, u32)> {
        let (gx, gy) = (x.round() as i32, y.round() as i32);
        let (ox, oy) = self.shrine.origin();
        let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
        if !inside {
            return None;
        }
        let h = self.shrine.height_at(gx - ox, gy - oy) as u32;
        (h > 0).then_some((gx, gy, h))
    }

    /// Every column tall enough to drip from its edge, in scan order.
    pub fn columns(&self) -> Vec<(i32, i32, u32)> {
        let mut out = Vec::new();
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                if let Some(c) = self.column_at(gx as f64, gy as f64) {
                    if c.2 >= 2 {
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    fn draw_plant(&self, fb: &mut Framebuffer, plant: &Plant, state: &SimState, base: Point, light: f64) {
        let (bx, by) = (base.x + plant.offset.0, base.y + plant.offset.1);
        let w = state.weather;
        let colour = |r: u8| {
            let c = match r {
                role::DARK => self.palette.foliage_dark,
                role::MID => self.palette.foliage,
                role::LIGHT => self.palette.foliage_light,
                role::ACCENT => self.palette.accent,
                _ => self.palette.foliage,
            };
            shade(self.lit(c, &w), (light * 100.0) as u32, 100)
        };
        let amplitude = plant.sway.amplitude * (0.25 + 0.75 * state.coupling.foliage_sway);
        let sway = |y: i32| -> i32 {
            if plant.height <= 0 {
                return 0;
            }
            let f = y as f64 / plant.height as f64;
            (amplitude * f * sin_turns(state.time + plant.sway.phase + w.wind_dir)).round() as i32
        };
        for stroke in &plant.strokes {
            let mut prev: Option<(i32, i32)> = None;
            for &(x, y) in &stroke.points {
                let px = to_px_round(bx + x + sway(y));
                let py = to_px_round(by - y);
                if let Some((ox, oy)) = prev {
                    let width = stroke.width.max(1) as i32;
                    for k in 0..width {
                        let off = k - width / 2;
                        raster::line(fb, ox, oy + off, px, py + off, colour(stroke.role));
                    }
                }
                prev = Some((px, py));
            }
        }
    }
}

/// The tallest column of a shrine, in levels.
fn tallest(shrine: &Shrine) -> f64 {
    let mut h = 0u8;
    for ly in 0..SHRINE_H as i32 {
        for lx in 0..SHRINE_W as i32 {
            h = h.max(shrine.height_at(lx, ly));
        }
    }
    h as f64
}

fn lerp(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let f = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}

/// A weather's two profiles: its tint and its light's vividness.
struct Profiles {
    weather: crate::weather::Profile,
    light: light::LightProfile,
}

impl Profiles {
    fn of(kind: WeatherKind, biome: Biome) -> Self {
        Profiles { weather: kind.profile(), light: light::profile_in(kind, biome) }
    }
}

/// [`Scene::lit`] for a whole frame, as one affine map of RGB. Every step of
/// the grade — the light level, the lit fog, the tint, the saturation, the
/// contrast and a lightning flash — is linear in the colour, so the chain
/// folds into one 3x3 matrix and an offset, worked out once and applied to
/// every textured pixel in nine multiply-adds.
struct Grade {
    m: [[f64; 3]; 3],
    b: [f64; 3],
}

impl Grade {
    fn new(scene: &Scene, w: &WeatherState) -> Self {
        let p = Profiles::of(scene.weather.kind, scene.biome);
        let mut m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut b = [0.0; 3];
        // out = a * out + c, per channel.
        let diag = |m: &mut [[f64; 3]; 3], b: &mut [f64; 3], a: [f64; 3], c: [f64; 3]| {
            for k in 0..3 {
                for j in 0..3 {
                    m[k][j] *= a[k];
                }
                b[k] = b[k] * a[k] + c[k];
            }
        };
        let brightness = (45.0 + 55.0 * scene.light_level(w)).floor();
        diag(&mut m, &mut b, [brightness / 100.0; 3], [0.0; 3]);
        let fog = (w.fog * 45.0).floor() / 100.0;
        let lit_fog = (25.0 + brightness * 0.75) / 100.0;
        let f = scene.palette.fog;
        diag(&mut m, &mut b, [1.0 - fog; 3], [0, 1, 2].map(|k| f[k] as f64 * lit_fog * fog));
        let pw = &p.weather;
        let mean = (pw.tint[0] as f64 + pw.tint[1] as f64 + pw.tint[2] as f64).max(1.0);
        let g = pw.grade as f64 / 100.0;
        diag(&mut m, &mut b, [0, 1, 2].map(|k| (1.0 - g) + 3.0 * pw.tint[k] as f64 / mean * g), [0.0; 3]);
        // Saturation: out = mean + (out - mean) * s, mixing the channels.
        let sat = p.light.saturation;
        let mix = (1.0 - sat) / 3.0;
        let (m0, b0) = (m, b);
        for k in 0..3 {
            for j in 0..3 {
                m[k][j] = sat * m0[k][j] + mix * (m0[0][j] + m0[1][j] + m0[2][j]);
            }
            b[k] = sat * b0[k] + mix * (b0[0] + b0[1] + b0[2]);
        }
        let con = p.light.contrast;
        diag(&mut m, &mut b, [con; 3], [128.0 * (1.0 - con); 3]);
        let q = (w.flash * 65.0).floor() / 100.0;
        diag(&mut m, &mut b, [1.0 - q; 3], [235.0 * q, 240.0 * q, 255.0 * q]);
        Grade { m, b }
    }

    #[inline]
    fn apply(&self, c: [f64; 3]) -> Rgba {
        let k = |r: usize| (self.m[r][0] * c[0] + self.m[r][1] * c[1] + self.m[r][2] * c[2] + self.b[r]).clamp(0.0, 255.0) as u8;
        [k(0), k(1), k(2), 255]
    }
}

/// One of the two passes over the geometry (see `render_state`): finding
/// which draw owns each pixel, then painting only those.
enum Pass<'p> {
    /// Owners, and (on the turned path) the nearest depth found so far.
    Find(&'p mut [u32], &'p mut [f64]),
    Paint(&'p mut Framebuffer, &'p [u32]),
}

/// Depths closer than this count as the same surface, and the later draw
/// wins. It covers shared edges and coplanar tiles, whose planes disagree by a
/// hair because each face's corners are rounded to the fixed-point grid;
/// about a quarter of a pixel's worth of depth.
const DEPTH_TIE: f64 = 0.02;

}

// ---- src/shrine.rs ----
pub mod shrine {
//! The shrine: a 16x16 footprint of stacked 2.5D tile layers, generated
//! deterministically from a seed and centred in the 24x24 grid.
//!
//! The model is a *height field of solid columns*: every cell is filled from
//! the ground up to its top tile, so there can never be a floating tile. Walls,
//! pillars and the central dais are simply taller columns; the top tile of a
//! column carries its visible kind. This keeps the structure valid by
//! construction and easy for the future editor to manipulate.
//!
//! Generation is parameter-only (a seed), as agreed for v1: no hand-authored
//! geometry yet.

use crate::grid::{Layers, Tile};
use crate::rng::Rng;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

/// Stream id, so the shrine never shares a sequence with another generator.
const STREAM: u64 = 0x5348_5231; // "SHR1"

const SW: usize = SHRINE_W as usize;
const SH: usize = SHRINE_H as usize;
/// Tallest column the generator may produce (also the layer count).
pub const MAX_HEIGHT: u8 = 6;

/// Tile kinds stored in [`Tile::kind`]. `EMPTY` is 0, so a zeroed model is
/// empty.
pub mod kind {
    pub const EMPTY: u8 = 0;
    /// Walkable ground.
    pub const FLOOR: u8 = 1;
    /// A raised stone wall.
    pub const WALL: u8 = 2;
    /// A free-standing column, taller than the walls around it.
    pub const PILLAR: u8 = 3;
    /// A capped or weathered top.
    pub const ROOF: u8 = 4;
    /// Broken remains.
    pub const RUBBLE: u8 = 5;
    /// The focal block at the centre of the shrine.
    pub const ALTAR: u8 = 6;
    /// Unseen body of a column (the tiles below the top).
    pub const STONE: u8 = 7;
}

/// A generated shrine and the layers it lives in.
#[derive(Clone, Debug)]
pub struct Shrine {
    pub layers: Layers,
    pub seed: u64,
    origin: (i32, i32),
    height: [[u8; SW]; SH],
    /// How far a cell is dug *below* the ground plane; 0 means none.
    depths: [[u8; SW]; SH],
}

impl Shrine {
    /// Generate the shrine for `seed`.
    pub fn generate(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        let origin = (
            (GRID_W as i32 - SHRINE_W as i32) / 2,
            (GRID_H as i32 - SHRINE_H as i32) / 2,
        );

        let mut height = [[1u8; SW]; SH];
        raise_perimeter(&mut height, &mut rng);
        cut_doorways(&mut height, &mut rng);
        raise_partitions(&mut height, &mut rng);
        raise_pillars(&mut height, &mut rng);
        raise_dais(&mut height);
        weather(&mut height, &mut rng);

        let mut layers = Layers::new(MAX_HEIGHT as u32);
        for (lx, row) in height.iter().enumerate() {
            for (ly, &h) in row.iter().enumerate() {
                let h = h.clamp(1, MAX_HEIGHT);
                let gx = origin.0 + lx as i32;
                let gy = origin.1 + ly as i32;
                let top = top_kind(lx, ly, h);
                for z in 0..h {
                    let kind = if z == h - 1 { top } else { kind::STONE };
                    layers.set(gx, gy, z as i32, Tile::new(kind, variant(seed, lx, ly, z)));
                }
            }
        }

        Shrine { layers, seed, origin, height, depths: [[0u8; SW]; SH] }
    }

    /// Build the shrine from an authored model instead of the generator.
    ///
    /// The footprint is unchanged — still exactly 16x16, still centred — so
    /// `HC005` keeps its meaning; only the tiles inside it come from the model.
    pub fn from_model(seed: u64, model: &crate::shrine_model::ShrineModel) -> Self {
        let origin = (
            (GRID_W as i32 - SHRINE_W as i32) / 2,
            (GRID_H as i32 - SHRINE_H as i32) / 2,
        );
        let mut height = [[1u8; SW]; SH];
        let mut depths = [[0u8; SW]; SH];
        let mut layers = Layers::new(MAX_HEIGHT as u32);
        for lx in 0..SW {
            for ly in 0..SH {
                let signed = model.height(lx, ly);
                if signed < 0 {
                    // Blocks descending below the ground: no column above, but
                    // the top tile still caps the cell so the ground plane reads
                    // as continuous where the structure drops away.
                    let top = model.kind(lx, ly).min(kind::STONE).max(kind::FLOOR);
                    height[lx][ly] = 0;
                    depths[lx][ly] = signed.unsigned_abs().min(MAX_HEIGHT);
                    layers.set(
                        origin.0 + lx as i32,
                        origin.1 + ly as i32,
                        0,
                        Tile::new(top, model.variant(lx, ly)),
                    );
                    continue;
                }
                let h = signed.min(MAX_HEIGHT as i8) as u8;
                height[lx][ly] = h;
                if h == 0 {
                    // A hole: no column at all, so the ground shows through.
                    continue;
                }
                let top = model.kind(lx, ly).min(kind::STONE).max(kind::FLOOR);
                let top_variant = model.variant(lx, ly);
                for z in 0..h {
                    let kind = if z == h - 1 { top } else { kind::STONE };
                    // Body tiles keep the generator's per-level variation, so a
                    // model read from the generated shrine rebuilds it exactly;
                    // the model's own variant drives the visible top tile.
                    let v = if z == h - 1 { top_variant } else { variant(seed, lx, ly, z) };
                    layers.set(origin.0 + lx as i32, origin.1 + ly as i32, z as i32, Tile::new(kind, v));
                }
            }
        }
        Shrine { layers, seed, origin, height, depths }
    }

    /// Top-left cell of the footprint in grid coordinates.
    pub fn origin(&self) -> (i32, i32) {
        self.origin
    }

    /// Column height (number of tiles) at a local footprint coordinate.
    pub fn height_at(&self, lx: i32, ly: i32) -> u8 {
        if lx < 0 || ly < 0 || lx as usize >= SW || ly as usize >= SH {
            return 0;
        }
        self.height[lx as usize][ly as usize]
    }

    /// How far this cell is dug below the ground plane, 0 for none.
    pub fn depth_at(&self, lx: i32, ly: i32) -> u8 {
        if lx < 0 || ly < 0 || lx as usize >= SW || ly as usize >= SH {
            return 0;
        }
        self.depths[lx as usize][ly as usize]
    }

    /// Whether any cell descends below the ground plane. When none do, the
    /// renderer keeps its original order exactly.
    pub fn has_depths(&self) -> bool {
        self.depths.iter().any(|row| row.iter().any(|d| *d > 0))
    }

    /// A stable hash of the generated structure, for golden tests.
    pub fn hash(&self) -> u64 {
        let mut h = self.layers.hash();
        for row in self.depths.iter() {
            for &d in row.iter() {
                h ^= d as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }
}

/// The visible kind of a column's top tile, chosen from its position.
fn top_kind(lx: usize, ly: usize, h: u8) -> u8 {
    if h == 1 {
        return kind::FLOOR;
    }
    let (cx, cy) = (SW / 2, SH / 2);
    if lx == cx && ly == cy {
        return kind::ALTAR;
    }
    if lx % 4 == 2 && ly % 4 == 2 {
        return kind::PILLAR;
    }
    if lx == 0 || ly == 0 || lx == SW - 1 || ly == SH - 1 || lx == cx || ly == cy {
        return kind::WALL;
    }
    kind::ROOF
}

fn raise_perimeter(height: &mut [[u8; SW]; SH], rng: &mut Rng) {
    for lx in 0..SW {
        for ly in 0..SH {
            if lx == 0 || ly == 0 || lx == SW - 1 || ly == SH - 1 {
                height[lx][ly] = 3 + rng.int(0, 2) as u8;
            }
        }
    }
}

fn cut_doorways(height: &mut [[u8; SW]; SH], rng: &mut Rng) {
    for _ in 0..3 {
        let side = rng.below(4);
        let along = rng.below(SW - 4) + 2;
        let len = rng.int(1, 2);
        for k in 0..=len {
            let i = (along + k as usize).min(SW - 1);
            match side {
                0 => height[i][0] = 1,
                1 => height[i][SH - 1] = 1,
                2 => height[0][i] = 1,
                _ => height[SW - 1][i] = 1,
            }
        }
    }
}

fn raise_partitions(height: &mut [[u8; SW]; SH], rng: &mut Rng) {
    let (cx, cy) = (SW / 2, SH / 2);
    // A broken cross of interior walls with a gap in each arm.
    for i in 3..SW - 3 {
        if i != cx + 1 {
            height[i][cy] = 2 + rng.int(0, 1) as u8;
        }
        if i != cy + 2 {
            height[cx][i] = 2 + rng.int(0, 1) as u8;
        }
    }
}

fn raise_pillars(height: &mut [[u8; SW]; SH], rng: &mut Rng) {
    let mut lx = 2;
    while lx < SW - 2 {
        let mut ly = 2;
        while ly < SH - 2 {
            height[lx][ly] = 3 + rng.int(0, 2) as u8;
            ly += 4;
        }
        lx += 4;
    }
}

fn raise_dais(height: &mut [[u8; SW]; SH]) {
    let (cx, cy) = (SW / 2, SH / 2);
    for dx in -1i32..=1 {
        for dy in -1i32..=1 {
            let (x, y) = ((cx as i32 + dx) as usize, (cy as i32 + dy) as usize);
            height[x][y] = height[x][y].max(2);
        }
    }
    height[cx][cy] = 3;
}

/// Break down a few raised cells so the ruin reads as weathered, without ever
/// dropping back to ground level.
fn weather(height: &mut [[u8; SW]; SH], rng: &mut Rng) {
    for lx in 1..SW - 1 {
        for ly in 1..SH - 1 {
            if height[lx][ly] > 2 && rng.chance(0.12) {
                height[lx][ly] -= 1;
            }
        }
    }
}

/// A cheap, stable per-tile cosmetic variation, independent of iteration order.
fn variant(seed: u64, lx: usize, ly: usize, z: u8) -> u8 {
    let mut h = seed
        ^ (lx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (ly as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h & 3) as u8
}

}

// ---- src/shrine_model.rs ----
pub mod shrine_model {
//! The authored architecture: an optional hand-edited 16x16 model.
//!
//! A cell's height runs from **-6 to +6**. Positive stacks tiles up from the
//! ground; negative digs a pit the same distance *below* it; zero leaves the
//! ground plane alone. Because a cell can legitimately be all zeros, "is this
//! model authored?" is an explicit flag rather than an all-zero sentinel.
//!
//! When `authored` is false the seed's generator stays authoritative, so the
//! default recipe carries no architecture of its own and today's output is
//! untouched (`HC004`).
//!
//! Each cell is nine bits: a signed height biased by +8, a three-bit tile kind
//! and two bits of cosmetic variation. Three cells fit in a recipe word, so the
//! model is one flag word plus 86 cell words — which is what keeps the artifact
//! self-contained (`G001`).

use crate::shrine::{kind, Shrine, MAX_HEIGHT};
use crate::{SHRINE_H, SHRINE_W};

/// Cells in the footprint.
pub const CELLS: usize = (SHRINE_W * SHRINE_H) as usize;
/// Cells packed per recipe word.
pub const PER_WORD: usize = 3;
/// Packed words holding the cells.
pub const PACKED: usize = CELLS.div_ceil(PER_WORD);
/// Recipe words the model occupies: one flag, then the packed cells.
pub const WORDS: usize = PACKED + 1;

/// A shrine's architecture, authored by hand or still procedural.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShrineModel {
    authored: bool,
    /// Nine bits per cell, three cells per word. Kept packed rather than as a
    /// byte array because nine bits do not fit in a byte.
    packed: [u32; PACKED],
}

impl ShrineModel {
    /// Deepest pit, in levels.
    pub const MIN_HEIGHT: i8 = -6;
    /// Tallest column, in levels.
    pub const MAX: i8 = crate::shrine::MAX_HEIGHT as i8;

    /// Not authored: the generator decides.
    pub const EMPTY: ShrineModel = ShrineModel { authored: false, packed: [0; PACKED] };

    /// True once the editor has taken over.
    pub fn is_authored(&self) -> bool {
        self.authored
    }

    /// Forget every edit and hand the architecture back to the generator.
    pub fn clear(&mut self) {
        self.authored = false;
        self.packed = [0; PACKED];
    }

    /// Empty the plan: an authored model in which every square is a hole.
    pub fn blank(&mut self) {
        self.authored = true;
        self.packed = [0; PACKED];
    }

    fn index(x: usize, y: usize) -> usize {
        (y % SHRINE_H as usize) * SHRINE_W as usize + (x % SHRINE_W as usize)
    }

    /// The nine encoded bits of one cell.
    fn raw(&self, i: usize) -> u16 {
        ((self.packed[i / PER_WORD] >> (9 * (i % PER_WORD))) & 0x1ff) as u16
    }

    /// Overwrite one cell's nine bits.
    fn set_raw(&mut self, i: usize, value: u16) {
        let shift = 9 * (i % PER_WORD);
        let word = &mut self.packed[i / PER_WORD];
        *word = (*word & !(0x1ff << shift)) | ((value as u32 & 0x1ff) << shift);
    }

    /// Height of a cell: positive builds up, negative digs down, 0 is ground.
    pub fn height(&self, x: usize, y: usize) -> i8 {
        let raw = self.raw(Self::index(x, y));
        if raw == 0 {
            // Never written, so this is plain ground rather than a pit.
            return 0;
        }
        decode_height(raw as u8)
    }

    /// Tile kind of a cell's top, see [`kind`].
    pub fn kind(&self, x: usize, y: usize) -> u8 {
        ((self.raw(Self::index(x, y)) >> 4) & 0x7) as u8
    }

    /// Cosmetic variation, 0..=3.
    pub fn variant(&self, x: usize, y: usize) -> u8 {
        ((self.raw(Self::index(x, y)) >> 7) & 0x3) as u8
    }

    /// Write one cell. Heights and kinds are clamped, so the model is always
    /// valid however it is driven.
    pub fn set(&mut self, x: usize, y: usize, height: i8, kind: u8, variant: u8) {
        self.authored = true;
        let value = Self::encode(height, kind, variant);
        let i = Self::index(x, y);
        self.set_raw(i, value);
    }

    /// Nine bits: biased height, kind, variation.
    fn encode(height: i8, kind: u8, variant: u8) -> u16 {
        let h = height.clamp(Self::MIN_HEIGHT, Self::MAX);
        encode_height(h) as u16 | ((kind as u16 & 0x7) << 4) | ((variant as u16 & 0x3) << 7)
    }

    /// The height, kind and variation packed into one cell.
    fn decode(value: u16) -> (i8, u8, u8) {
        let h = decode_height((value & 0xf) as u8).clamp(Self::MIN_HEIGHT, Self::MAX);
        ((h), ((value >> 4) & 0x7) as u8, ((value >> 7) & 0x3) as u8)
    }

    /// Read the generated shrine into a model, so the editor starts from what
    /// the seed produced.
    pub fn from_shrine(shrine: &Shrine) -> Self {
        let mut model = ShrineModel::EMPTY;
        let (ox, oy) = shrine.origin();
        for y in 0..SHRINE_H as usize {
            for x in 0..SHRINE_W as usize {
                let h = shrine.height_at(x as i32, y as i32).min(MAX_HEIGHT);
                let tile = if h == 0 {
                    crate::grid::Tile::EMPTY
                } else {
                    shrine.layers.get(x as i32 + ox, y as i32 + oy, h as i32 - 1)
                };
                model.set(x, y, h as i8, tile.kind.max(kind::FLOOR), tile.variant);
            }
        }
        model
    }

    /// The model as recipe words: the authored flag, then the packed cells.
    pub fn words(&self) -> [u32; WORDS] {
        let mut out = [0u32; WORDS];
        out[0] = self.authored as u32;
        out[1..].copy_from_slice(&self.packed);
        out
    }

    /// Rebuild from recipe words. Short input leaves the model procedural.
    pub fn from_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let Some((&flag, packed)) = words.split_first() else {
            return model;
        };
        model.authored = flag & 1 != 0;
        for (i, w) in packed.iter().take(PACKED).enumerate() {
            model.packed[i] = *w;
        }
        model.sanitise();
        model
    }

    /// Rebuild from the version-13 layout: one flag word, then 8-bit cells
    /// holding height 0..6, kind and variation.
    pub fn from_v13_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let Some((&flag, packed)) = words.split_first() else {
            return model;
        };
        model.authored = flag & 1 != 0;
        for (i, w) in packed.iter().take(CELLS / 4).enumerate() {
            for (j, b) in w.to_le_bytes().iter().enumerate() {
                // The old layout: height 0..6, kind, variation, in one byte.
                model.set_raw(i * 4 + j, Self::encode((*b & 0x7) as i8, (*b >> 3) & 0x7, (*b >> 6) & 0x3));
            }
        }
        model
    }

    /// Rebuild from the version-11/12 layout: 8-bit cells with the height
    /// stored minus one, and a non-empty model meaning "authored".
    pub fn from_legacy_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let mut any = false;
        for (i, w) in words.iter().take(CELLS / 4).enumerate() {
            for (j, b) in w.to_le_bytes().iter().enumerate() {
                any |= *b != 0;
                // The old layout stored height - 1, so every old cell is one higher.
                let h = ((*b & 0x7) + 1).min(MAX_HEIGHT) as i8;
                model.set_raw(i * 4 + j, Self::encode(h, (*b >> 3) & 0x7, (*b >> 6) & 0x3));
            }
        }
        model.authored = any;
        model
    }

    /// Clamp anything out of range back into the model's own limits.
    fn sanitise(&mut self) {
        for i in 0..CELLS {
            // A zero cell is one the editor has never written: it is ground, and
            // must stay exactly as it is or every save would invent territory.
            if self.raw(i) == 0 {
                continue;
            }
            let (h, k, v) = Self::decode(self.raw(i));
            self.set_raw(i, Self::encode(h, k.min(kind::STONE), v));
        }
    }

    /// The packed words as lowercase hex, for the seed file.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(WORDS * 8);
        for w in self.words() {
            out.push_str(&format!("{w:08x}"));
        }
        out
    }

    /// Read [`to_hex`] back. Returns `None` unless the text is all hex
    /// digits and one of the two lengths a seed file has used:
    ///
    /// - today's [`WORDS`] words (eight digits each, flag first);
    /// - the earlier one byte per cell (two digits each, `CELLS * 2`), written
    ///   before cells grew to nine bits. Those bytes are the version-13 cell
    ///   layout, read the way that version read them: only an authored model
    ///   was ever written, so it is authored.
    pub fn from_hex(text: &str) -> Option<ShrineModel> {
        let t = text.trim();
        if !t.bytes().all(|b| (b as char).is_ascii_hexdigit()) {
            return None;
        }
        if t.len() == CELLS * 2 {
            let mut words = vec![1u32];
            for i in 0..CELLS / 4 {
                let byte = |j: usize| u8::from_str_radix(&t[(i * 4 + j) * 2..(i * 4 + j) * 2 + 2], 16);
                words.push(u32::from_le_bytes([byte(0).ok()?, byte(1).ok()?, byte(2).ok()?, byte(3).ok()?]));
            }
            return Some(ShrineModel::from_v13_words(&words));
        }
        if t.len() != WORDS * 8 {
            return None;
        }
        let mut words = [0u32; WORDS];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32::from_str_radix(&t[i * 8..i * 8 + 8], 16).ok()?;
        }
        Some(ShrineModel::from_words(&words))
    }

    /// A stable hash, for tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &w in self.words().iter() {
            for b in w.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        h ^ (self.authored as u64)
    }
}

/// Height bias: stored value = height + 8, so -6..=6 lands in 2..=14.
fn encode_height(height: i8) -> u8 {
    ((height.clamp(ShrineModel::MIN_HEIGHT, ShrineModel::MAX) + 8) as u8) & 0xf
}

fn decode_height(byte: u8) -> i8 {
    (byte & 0xf) as i8 - 8
}

impl Default for ShrineModel {
    fn default() -> Self {
        ShrineModel::EMPTY
    }
}

impl std::fmt::Debug for ShrineModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShrineModel({} bytes, authored: {})", CELLS, self.is_authored())
    }
}



}

// ---- src/sim.rs ----
pub mod sim {
//! The coupled simulation clock.
//!
//! One pure state — weather, the slow instrument arcs and the visual coupling
//! derived from them — is the single source of truth for both the music and the
//! picture. The audio and the visuals can run in separate WASM instances (the
//! AudioWorklet and the page) and still agree, because both are functions of
//! absolute time: no shared memory, no message passing, no drift.
//!
//! [`Sim`] owns the generated scene and the music engine built from one
//! [`SimConfig`]. The artifact uses [`Sim::render_audio`] and
//! [`Sim::render_visual`]; the native preview can use either.

use crate::audio::weather_bed::WeatherBed;
use crate::framebuffer::Framebuffer;
use crate::math::exp2;
use crate::plants::Biome;
use crate::scene::{Camera, Instruments, Quality, Scene, SimState, VisualCoupling};
use crate::track::{Seeds, Settings, Track};
use crate::weather::WeatherKind;
use crate::wildlife::Wildlife;
use crate::SAMPLE_RATE;

/// Everything a shrine is built from, shared by audio and visuals.
#[derive(Clone, Debug)]
pub struct SimConfig {
    /// The visual/palette/weather seed.
    pub seed: u64,
    /// The music's six seeds.
    pub seeds: Seeds,
    /// The music's settings.
    pub settings: Settings,
    pub biome: Biome,
    pub weather: WeatherKind,
}

/// Which instrument drives which visual channel. Data, so the eventual editor
/// can expose it rather than hard-coding the links.
pub const COUPLING: [(&str, &str); 4] = [
    ("wind", "foliage sway and rain drift"),
    ("rain", "rain streaks and surface sheen"),
    ("drone", "altar glow"),
    ("pulse", "detail flicker"),
];

/// The camera orbit at `t` seconds, in turns. Wrapped so a long performance
/// keeps full precision, which is safe because the projection is periodic in a
/// whole turn. Built from `* / rem_euclid` only, so it is bit-identical
/// everywhere (`HC004`).
pub fn camera_orbit_turns(speed_deg_per_s: i16, t: f64) -> f64 {
    if speed_deg_per_s == 0 {
        return 0.0;
    }
    (speed_deg_per_s as f64 * t / 360.0).rem_euclid(1.0)
}

/// A shrine: its generated scene and its music, on one clock.
pub struct Sim {
    pub config: SimConfig,
    pub scene: Scene,
    track: Option<Track>,
    weather_bed: Option<WeatherBed>,
    /// Stereo frames rendered so far: an exact count, so the weather bed's
    /// clock never depends on how the audio is cut into blocks.
    audio_samples: u64,
    /// Samples output since a mid-piece start, for its fade-in.
    since_start: u64,
    /// Whether a solo has been asked for: then the output is the track's
    /// monitor (the soloed layer) until it has faded back to the full mix.
    /// Never set, the output is exactly the plain render.
    soloing: bool,
    /// The mix the monitor is rendered beside (scratch).
    split: Vec<f32>,
}

/// A start part-way through fades in over this long, so it never begins
/// on the middle of a hit.
const START_FADE_S: f64 = 1.5;

impl Sim {
    pub fn new(config: SimConfig) -> Self {
        let mut scene = Scene::new(config.seed, config.biome, config.weather);
        scene.author(&config.settings.model);
        let track = Some(Track::new(config.seeds, config.settings));
        // Which cells hold a column, so a splash there is heard on stone.
        let hard = (0..crate::GRID_H as i32)
            .flat_map(|gy| (0..crate::GRID_W as i32).map(move |gx| (gx, gy)))
            .map(|(gx, gy)| scene.column_at(gx as f64, gy as f64).is_some())
            .collect();
        let wildlife = config.settings.wildlife.then(|| Wildlife::new(config.seed, config.biome));
        scene.wildlife = wildlife.clone();
        let weather_bed = Some(WeatherBed::new(config.seed, hard, wildlife).roofed(config.biome.roofed()));
        let mut sim = Sim { config, scene, track, weather_bed, audio_samples: 0, since_start: 0, soloing: false, split: Vec::new() };
        sim.fast_forward();
        sim
    }

    /// Where the piece starts, in seconds.
    pub fn start_offset(&self) -> f64 {
        self.config.settings.start_s.min(crate::track::MAX_START_S) as f64
    }

    /// Render, and throw away, everything before the start offset. Rendering
    /// does not depend on how it is cut into blocks, so what follows is
    /// exactly what an unbroken run would play there.
    fn fast_forward(&mut self) {
        let frames = (self.start_offset() * SAMPLE_RATE as f64) as usize;
        let mut scratch = vec![0f32; 4096 * 2];
        let mut left = frames;
        while left > 0 {
            let n = left.min(4096);
            self.render_raw(&mut scratch[..n * 2]);
            left -= n;
        }
    }

    /// A visuals-only instance: same scene, generated from the same recipe,
    /// without paying to synthesise the music. Used by the page's render thread.
    pub fn new_visual(config: SimConfig) -> Self {
        let mut scene = Scene::new(config.seed, config.biome, config.weather);
        scene.author(&config.settings.model);
        scene.wildlife = config.settings.wildlife.then(|| Wildlife::new(config.seed, config.biome));
        Sim { config, scene, track: None, weather_bed: None, audio_samples: 0, since_start: 0, soloing: false, split: Vec::new() }
    }

    /// The shared state at `t` seconds. Pure: the same `t` always yields the
    /// same state, so two instances stay in step.
    pub fn state(&self, t: f64) -> SimState {
        let weather = self.scene.weather_state(t);
        let sample = (t.max(0.0) * SAMPLE_RATE as f64) as u64;
        let drone = self.config.settings.drone.gain(sample);
        let loops = self.config.settings.loop_arcs.map(|a| a.gain(sample));
        let pulse = beat_pulse(self.config.settings.bpm, t);

        SimState {
            time: t,
            weather,
            coupling: VisualCoupling {
                foliage_sway: weather.wind_strength,
                rain_density: weather.rain,
                glow: drone,
            },
            instruments: Instruments { drone, loops, pulse },
            camera: Camera::default(),
            draw_sky: true,
            arch: self.config.settings.arch,
            orbit_turns: camera_orbit_turns(self.config.settings.camera_speed, t),
        }
    }

    /// Draw the visual half `t` seconds after the piece starts (so at
    /// `start offset + t` into it, in step with the audio).
    pub fn render_visual(&self, fb: &mut Framebuffer, t: f64, quality: Quality) {
        self.scene.render_state(fb, &self.state(t + self.start_offset()), quality);
    }

    /// Render the ported 0011 music alone. Bit-identical to 0011; used to prove
    /// the weather bed does not disturb it.
    pub fn render_music(&mut self, out: &mut [f32]) {
        if let Some(track) = &mut self.track {
            track.render(out);
        } else {
            out.fill(0.0);
        }
    }

    /// Render the full shrine: the music, with wind and rain mixed on top from
    /// the same weather state the visuals see.
    pub fn render_audio(&mut self, out: &mut [f32]) {
        self.render_raw(out);
        if self.config.settings.start_s > 0 {
            let fade = (START_FADE_S * SAMPLE_RATE as f64) as u64;
            for frame in out.chunks_mut(2) {
                if self.since_start >= fade {
                    break;
                }
                let x = self.since_start as f64 / fade as f64;
                for s in frame.iter_mut() {
                    *s *= (x * x) as f32;
                }
                self.since_start += 1;
            }
        }
    }

    fn render_raw(&mut self, out: &mut [f32]) {
        // The weather steps back while a layer is soloed (its share of the
        // monitor, as the solo fades in and out).
        let mut bed_gain = 1.0f32;
        match &mut self.track {
            Some(track) if self.soloing => {
                bed_gain = 1.0 - track.solo().1 as f32;
                self.split.resize(out.len(), 0.0);
                // The mix is rendered and set aside, so the music runs on
                // exactly as it would; what is heard is the monitor.
                track.render_split(&mut self.split, out);
                let (current, amount) = track.solo();
                if current.is_none() && amount == 0.0 && track.solo_target().is_none() {
                    self.soloing = false;
                }
            }
            Some(track) => track.render(out),
            None => out.fill(0.0),
        }
        if let Some(bed) = &mut self.weather_bed {
            let weather = &self.scene.weather;
            if bed_gain == 1.0 {
                bed.render_from(self.audio_samples, out, weather);
            } else {
                let mut wet = vec![0f32; out.len()];
                bed.render_from(self.audio_samples, &mut wet, weather);
                for (o, w) in out.iter_mut().zip(wet) {
                    *o += w * bed_gain;
                }
            }
        }
        self.audio_samples += out.len() as u64 / 2;
    }

    /// Hear one layer on its own (`None` = the full mix). Only what is heard
    /// changes: the music underneath carries on exactly as it would.
    pub fn set_solo(&mut self, solo: Option<crate::track::Solo>) {
        if let Some(track) = &mut self.track {
            track.set_solo(solo);
            if solo.is_some() {
                self.soloing = true;
            }
        }
    }

    /// The music engine, for the Learn page's live views (`None` for a
    /// visuals-only instance).
    pub fn track_mut(&mut self) -> Option<&mut crate::track::Track> {
        self.track.as_mut()
    }
}

/// A percussive accent each beat, decaying between them: `bpm` beats per
/// minute, value in `[0, 1]`. Deterministic, and shared with the visuals.
pub fn beat_pulse(bpm: u16, t: f64) -> f64 {
    let beats = t * bpm as f64 / 60.0;
    let frac = beats - beats.floor();
    exp2(-6.0 * frac).clamp(0.0, 1.0)
}

}

// ---- src/weather.rs ----
pub mod weather {
//! Weather: the shrine's atmosphere, fixed for the piece and chosen at setup.
//!
//! Within one shrine the *kind* never changes — there is no transition from
//! rain to clear. Only the intensity cycles: wind and rain swell and ease on
//! their own slow periods, and the wind gusts on faster ones. The state is a
//! pure function of time and seed, so the audio and the visuals sample exactly
//! the same weather (`HC004`).
//!
//! # One table for intensity
//!
//! Every field of [`WeatherState`] is a level in `[0, 1]` that *both* halves
//! read: the scene draws that many particles and the weather bed plays that
//! loud. A level of 0 means none of it — no particle and no sound — so the two
//! can never disagree about whether it is raining.
//!
//! # Events
//!
//! Lightning, splashes, drips and ember sparks are discrete events. They are
//! found by [`Weather::events`], a stateless schedule: each of a fixed set of
//! *slots* repeats on its own period, and a hash of (seed, slot, cycle) decides
//! whether that cycle fires, when, and where. Any time range can be queried
//! from scratch, so the page's visual instance and the worklet's audio instance
//! find exactly the same events without ever talking to each other.

use crate::math::{cos_turns, exp2, sin_turns};
use crate::palette::wrap01;
use crate::rng::Rng;

/// Stream id, so weather never shares a sequence with another generator.
const STREAM: u64 = 0x5745_4154; // "WEAT"

/// The kinds of atmosphere a shrine can have. New kinds are only ever
/// appended: recipes store the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Rain,
    Wind,
    Fog,
    Storm,
    Snow,
    Dusk,
}

/// How a kind looks and sounds at full intensity, before cycling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub wind: f64,
    pub rain: f64,
    pub fog: f64,
    pub light: f64,
    pub snow: f64,
    pub embers: f64,
    pub motes: f64,
    pub leaves: f64,
    pub cloud: f64,
    /// Condensation dripping off the architecture.
    pub drips: f64,
    /// Whether a butterfly now and then drifts across (a rare, quiet sight).
    pub butterflies: bool,
    /// The colour the whole scene is graded toward, and by how much (0-100).
    pub tint: [u8; 3],
    pub grade: u32,
    pub lightning: bool,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 7] = [
        WeatherKind::Clear,
        WeatherKind::Rain,
        WeatherKind::Wind,
        WeatherKind::Fog,
        WeatherKind::Storm,
        WeatherKind::Snow,
        WeatherKind::Dusk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WeatherKind::Clear => "Clear",
            WeatherKind::Rain => "Rain",
            WeatherKind::Wind => "Wind",
            WeatherKind::Fog => "Fog",
            WeatherKind::Storm => "Storm",
            WeatherKind::Snow => "Snow",
            WeatherKind::Dusk => "Dusk",
        }
    }

    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            WeatherKind::Clear => "warm light, drifting motes, a soft breeze",
            WeatherKind::Rain => "layered rain, splashes on stone, a wet sheen",
            WeatherKind::Wind => "gusts, blown leaves, fast clouds, a whistling air",
            WeatherKind::Fog => "drifting banks, low mist, slow drips",
            WeatherKind::Storm => "driving rain, lightning and the thunder after it",
            WeatherKind::Snow => "falling flakes, whitening stone, a muffled hush",
            WeatherKind::Dusk => "a low warm sun and rising embers",
        }
    }

    pub fn profile(self) -> Profile {
        let p = |wind, rain, fog, light, cloud, tint, grade| Profile {
            wind,
            rain,
            fog,
            light,
            snow: 0.0,
            embers: 0.0,
            motes: 0.0,
            leaves: 0.0,
            cloud,
            drips: 0.0,
            butterflies: false,
            tint,
            grade,
            lightning: false,
        };
        match self {
            WeatherKind::Clear => Profile { motes: 0.85, butterflies: true, ..p(0.18, 0.0, 0.08, 0.97, 0.1, [255, 222, 168], 10) },
            WeatherKind::Rain => Profile { drips: 0.5, leaves: 0.08, ..p(0.35, 0.75, 0.35, 0.62, 0.6, [150, 170, 205], 14) },
            // Dry: no drizzle, so no splash taps under the gusts.
            WeatherKind::Wind => Profile { leaves: 0.95, motes: 0.2, ..p(0.85, 0.0, 0.15, 0.82, 0.45, [200, 214, 232], 8) },
            WeatherKind::Fog => Profile { drips: 0.9, motes: 0.15, ..p(0.12, 0.05, 0.9, 0.55, 0.3, [188, 194, 202], 24) },
            WeatherKind::Storm => {
                Profile { leaves: 0.5, lightning: true, ..p(0.95, 0.95, 0.45, 0.40, 1.0, [118, 132, 172], 22) }
            }
            WeatherKind::Snow => Profile { snow: 0.85, ..p(0.3, 0.0, 0.3, 0.78, 0.5, [190, 210, 255], 22) },
            WeatherKind::Dusk => Profile { embers: 0.8, motes: 0.3, leaves: 0.05, ..p(0.22, 0.0, 0.15, 0.72, 0.25, [255, 136, 88], 30) },
        }
    }
}

/// The weather sampled at one instant. All values are in `[0, 1]`, and every
/// one is a level both the picture and the sound follow (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherState {
    /// Wind direction in turns (`0` = one way, `0.5` = the other).
    pub wind_dir: f64,
    /// The wind, gusts included: foliage sway and the wind's loudness.
    pub wind_strength: f64,
    /// The fast gust envelope alone (whistle, leaf bursts).
    pub gust: f64,
    pub rain: f64,
    pub fog: f64,
    /// Ambient light: how much the palette is lit rather than shadowed.
    pub light: f64,
    pub snow: f64,
    /// How white the tops have turned under the snow.
    pub cover: f64,
    pub embers: f64,
    pub motes: f64,
    pub leaves: f64,
    pub cloud: f64,
    pub drips: f64,
    /// Lightning: how bright the sky is flashing right now.
    pub flash: f64,
    /// The sideways wind the particles travel on, −1 (left) .. 1 (right). It
    /// turns round smoothly and slowly: each way for at least half a drift
    /// period (30 s or more), never flipping in a moment.
    pub wind_x: f64,
    /// How far that wind has carried things since the start: the exact
    /// integral of `wind_x`, in seconds at full wind. A particle drifting at
    /// `v` pixels per second of full wind is `v * drift` along, so its motion
    /// always matches the wind of the moment.
    pub drift: f64,
}

/// What a scheduled event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Strike,
    Splash,
    Drip,
    Spark,
    /// A butterfly setting off across the scene: about one every two minutes.
    Butterfly,
}

impl EventKind {
    /// (slots, shortest period, longest period, stream).
    fn schedule(self) -> (u64, f64, f64, u64) {
        match self {
            // Strikes: one slot per ~9 s window, so storms never strobe.
            EventKind::Strike => (1, 9.0, 9.0, 0x5354_524b),
            // ~90 splashes a second at the heaviest rain.
            EventKind::Splash => (96, 0.6, 1.6, 0x5350_4c53),
            EventKind::Drip => (6, 2.0, 6.0, 0x4452_4950),
            EventKind::Spark => (12, 0.4, 1.2, 0x5350_524b),
            EventKind::Butterfly => (1, 120.0, 120.0, 0x4246_4c59),
        }
    }
}

/// One scheduled event. `hash` seeds everything else about it (where, how
/// strong), through [`Event::rng`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub at: f64,
    pub hash: u64,
}

impl Event {
    /// The event's own random numbers. Draw them in a fixed order.
    pub fn rng(&self) -> Rng {
        Rng::new(self.hash)
    }

    /// A strike's strength in `[0.4, 1]` and distance in `[0, 1]`.
    pub fn strike(&self) -> (f64, f64) {
        let mut r = self.rng();
        (r.range(0.4, 1.0), r.unit())
    }

    /// When a strike's thunder arrives: sooner the closer it is.
    pub fn thunder_at(&self) -> f64 {
        self.at + 0.3 + 2.7 * self.strike().1
    }

    /// Where the event lands, in grid cells (fractional): anywhere on the
    /// 24x24 plateau.
    pub fn spot(&self) -> (f64, f64) {
        let mut r = Rng::new(self.hash ^ 0x5350_4f54);
        (r.range(0.0, crate::GRID_W as f64 - 1.0), r.range(0.0, crate::GRID_H as f64 - 1.0))
    }

    /// Stereo position in `[-1, 1]` of an event at `spot`: the screen's x.
    pub fn pan(&self) -> f64 {
        let (gx, gy) = self.spot();
        ((gx - gy) / crate::GRID_W as f64).clamp(-1.0, 1.0)
    }
}

/// A strike flickers in up to three pulses: (delay s, relative brightness).
const FLICKER: [(f64, f64); 3] = [(0.0, 1.0), (0.07, 0.6), (0.17, 0.85)];
/// How long a flash pulse takes to halve, in seconds.
const FLICKER_HALF: f64 = 0.03;

/// The sideways wind's period range, seconds: it blows one way for half a
/// period, so at least 30 s, before turning round.
const MIN_DRIFT_PERIOD_S: f64 = 60.0;
const MAX_DRIFT_PERIOD_S: f64 = 150.0;

/// A shrine's weather: one kind, with its own slow phases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub kind: WeatherKind,
    seed: u64,
    phase: f64,
    wind_period: f64,
    rain_period: f64,
    gust_periods: [f64; 3],
    gust_phases: [f64; 3],
    /// The sideways wind's period, seconds, and its phase in turns.
    drift_period: f64,
    drift_phase: f64,
}

impl Weather {
    /// The weather for a shrine, seeded so two shrines with the same kind
    /// still cycle differently.
    pub fn new(kind: WeatherKind, seed: u64) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        let phase = rng.unit();
        let wind_period = rng.range(38.0, 96.0);
        let rain_period = rng.range(24.0, 70.0);
        // Drawn after the original three, so those keep their values.
        let gust_periods = [rng.range(3.0, 5.0), rng.range(6.0, 9.0), rng.range(11.0, 17.0)];
        let gust_phases = [rng.unit(), rng.unit(), rng.unit()];
        // Its own stream, so nothing above moves.
        let mut d = Rng::stream(seed, STREAM ^ 0x0D41_F7);
        let (drift_period, drift_phase) = (d.range(MIN_DRIFT_PERIOD_S, MAX_DRIFT_PERIOD_S), d.unit());
        Weather { kind, seed, phase, wind_period, rain_period, gust_periods, gust_phases, drift_period, drift_phase }
    }

    fn wind_cycle(&self, t: f64) -> f64 {
        0.5 - 0.5 * cos_turns(t / self.wind_period + self.phase)
    }

    fn rain_cycle(&self, t: f64) -> f64 {
        0.5 - 0.5 * cos_turns(t / self.rain_period + self.phase * 0.7 + 0.13)
    }

    /// The sideways wind at `t` (−1..1) and its integral since 0 (seconds
    /// at full wind): a sine of the drift period, scaled by the kind's wind,
    /// so it reverses smoothly and only after half a period.
    fn drift(&self, t: f64) -> (f64, f64) {
        let u = self.kind.profile().wind.clamp(0.0, 1.0);
        let (p, ph) = (self.drift_period, self.drift_phase);
        let wind_x = u * sin_turns(t / p + ph);
        // ∫ sin(2π(τ/p + ph)) dτ from 0 to t.
        let drift = u * p / crate::math::TAU * (cos_turns(ph) - cos_turns(t / p + ph));
        (wind_x, drift)
    }

    /// The fast gust envelope, in `[0, 1]`: three waves, squared so calm is
    /// common and a gust is an event.
    fn gust(&self, t: f64) -> f64 {
        let w = [0.5, 0.3, 0.2];
        let mut g = 0.0;
        for k in 0..3 {
            g += w[k] * (0.5 + 0.5 * sin_turns(t / self.gust_periods[k] + self.gust_phases[k]));
        }
        g * g
    }

    /// The weather at `t` seconds. Pure and deterministic.
    pub fn state(&self, t: f64) -> WeatherState {
        let p = self.kind.profile();
        let wind_cycle = self.wind_cycle(t);
        let rain_cycle = self.rain_cycle(t);
        let gust = self.gust(t);
        let wind = (p.wind * (0.3 + 0.45 * wind_cycle + 0.25 * gust)).clamp(0.0, 1.0);
        let rain = (p.rain * (0.25 + 0.75 * rain_cycle)).clamp(0.0, 1.0);
        let snow = (p.snow * (0.3 + 0.7 * rain_cycle)).clamp(0.0, 1.0);
        let (wind_x, drift) = self.drift(t);
        WeatherState {
            wind_x,
            drift,
            wind_dir: wrap01(self.phase + t / (self.wind_period * 6.0)),
            wind_strength: wind,
            gust: (gust * p.wind).clamp(0.0, 1.0),
            rain,
            fog: (p.fog * (0.55 + 0.45 * wind_cycle)).clamp(0.0, 1.0),
            light: (p.light * (0.80 + 0.20 * wind_cycle)).clamp(0.0, 1.0),
            snow,
            cover: (p.snow * (0.45 + 0.55 * rain_cycle)).clamp(0.0, 1.0),
            embers: (p.embers * (0.4 + 0.6 * (1.0 - wind_cycle))).clamp(0.0, 1.0),
            motes: (p.motes * (0.5 + 0.5 * (1.0 - gust))).clamp(0.0, 1.0),
            leaves: (p.leaves * (0.15 + 0.85 * wind)).clamp(0.0, 1.0),
            cloud: (p.cloud * (0.6 + 0.4 * rain_cycle)).clamp(0.0, 1.0),
            drips: (p.drips * (0.4 + 0.6 * (1.0 - wind_cycle))).clamp(0.0, 1.0),
            flash: self.flash(t),
        }
    }

    /// How likely a slot's cycle is to fire, from the level at that moment.
    /// Strikes are gated on the rain rather than calling back into `state`.
    fn gate(&self, kind: EventKind, t: f64) -> f64 {
        let p = self.kind.profile();
        match kind {
            EventKind::Strike if p.lightning => 0.35 + 0.6 * self.rain_cycle(t),
            EventKind::Strike => 0.0,
            EventKind::Splash => p.rain * (0.25 + 0.75 * self.rain_cycle(t)),
            EventKind::Drip => p.drips * (0.4 + 0.6 * (1.0 - self.wind_cycle(t))),
            EventKind::Spark => 0.8 * p.embers * (0.4 + 0.6 * (1.0 - self.wind_cycle(t))),
            EventKind::Butterfly => p.butterflies as u8 as f64,
        }
    }

    /// Every event of `kind` that happens in `[t0, t1)`, in no particular
    /// order. Pure: the same range always gives the same events.
    pub fn events(&self, kind: EventKind, t0: f64, t1: f64) -> Vec<Event> {
        let mut out = Vec::new();
        if t1 <= t0 {
            return out;
        }
        let (slots, lo, hi, stream) = kind.schedule();
        let mut slot_rng = Rng::stream(self.seed, stream);
        for slot in 0..slots {
            let period = slot_rng.range(lo, hi);
            let phase = slot_rng.unit() * period;
            let first = ((t0 + phase) / period).floor() as i64 - 1;
            let last = ((t1 + phase) / period).floor() as i64;
            for n in first..=last {
                let hash = crate::rng::hash3(self.seed ^ stream, slot, n as u64);
                let mut r = Rng::new(hash);
                let at = (n as f64 + 0.9 * r.unit()) * period - phase;
                if at < t0 || at >= t1 {
                    continue;
                }
                if r.unit() >= self.gate(kind, at) {
                    continue;
                }
                out.push(Event { kind, at, hash: r.next_u64() });
            }
        }
        out
    }

    /// The lightning flash at `t`, in `[0, 1]`: every strike of the last
    /// half-second, flickering and fading, brighter the closer it struck.
    pub fn flash(&self, t: f64) -> f64 {
        if !self.kind.profile().lightning {
            return 0.0;
        }
        let mut f = 0.0;
        for e in self.events(EventKind::Strike, t - 0.6, t + 1e-9) {
            let (strength, distance) = e.strike();
            let bright = strength * (1.0 - 0.6 * distance);
            for (delay, rel) in FLICKER {
                let dt = t - e.at - delay;
                if dt >= 0.0 {
                    f += bright * rel * exp2(-dt / FLICKER_HALF);
                }
            }
        }
        f.clamp(0.0, 1.0)
    }
}

}

// ---- src/weather_fx.rs ----
pub mod weather_fx {
//! Weather as pixels: clouds, light shafts, fog, lightning, rain, snow, leaves,
//! motes, embers, splashes and drips.
//!
//! Every effect is *stateless*: a particle's position is its own hashed start
//! plus time times its speed, wrapped, so any frame can be drawn on its own and
//! the page can render at whatever rate it likes (`HC004`). How many of each are
//! drawn comes straight from the shared [`WeatherState`] levels, which the
//! weather bed also plays from — that is what keeps the sound and the picture
//! at the same intensity.
//!
//! Effects that sit in the sky are composited with [`Framebuffer::blend_over`],
//! so they still work when the page paints the sky itself and the frame is
//! transparent there.

use crate::framebuffer::{blend, Framebuffer, Rgba};
use crate::iso::{to_px_round, Point};
use crate::math::{cos_turns, exp2, sin_turns};
use crate::palette::Palette;
use crate::rng::{hash3, Rng};
use crate::weather::{Event, EventKind, Weather, WeatherState};
use crate::{LOGICAL_H, LOGICAL_W};

const W: f64 = LOGICAL_W as f64;
const H: f64 = LOGICAL_H as f64;

// Particle layers, each with its own hash stream.
const FAR_RAIN: u64 = 1;
const NEAR_RAIN: u64 = 2;
const FLAKES: u64 = 3;
const LEAVES: u64 = 4;
const MOTES: u64 = 5;
const EMBERS: u64 = 6;
const CLOUDS: u64 = 7;
const FOG: u64 = 8;
const SHAFTS: u64 = 9;

/// Most particles per layer, at full intensity and full quality.
const MAX_FAR_RAIN: usize = 240;
const MAX_NEAR_RAIN: usize = 130;
const MAX_FLAKES: usize = 420;
const MAX_LEAVES: usize = 90;
const MAX_MOTES: usize = 80;
const MAX_EMBERS: usize = 90;

/// How long a splash lasts, and a drip takes to fall, in seconds.
pub const SPLASH_S: f64 = 0.32;
pub const DRIP_S: f64 = 0.9;
/// How long a spark's flare lasts.
const SPARK_S: f64 = 0.6;
/// How long a butterfly takes to cross the scene.
pub const BUTTERFLY_S: f64 = 18.0;

/// Everything an effect needs to know about the frame.
pub struct Fx<'a> {
    pub seed: u64,
    pub t: f64,
    pub w: WeatherState,
    pub weather: &'a Weather,
    pub palette: &'a Palette,
    /// Fraction of the particles to draw (from the quality level).
    pub fraction: f64,
    /// Fraction of the rain to draw (rain is the heaviest effect).
    pub rain_fraction: f64,
    /// Weather lighting and grading, applied to one colour.
    pub grade: &'a dyn Fn(Rgba) -> Rgba,
}

/// Rain's sideways speed as a share of its fall at full wind: the streaks
/// lean by the same ratio, so they point along their travel.
const RAIN_SLANT: f64 = 0.45;

fn particle(seed: u64, layer: u64, i: usize) -> Rng {
    Rng::new(hash3(seed, layer, i as u64))
}

fn wrap(x: f64, span: f64) -> f64 {
    x.rem_euclid(span)
}

impl Fx<'_> {
    /// The sideways wind now, -1 (left) .. 1 (right).
    fn wind_x(&self) -> f64 {
        self.w.wind_x
    }

    /// How far the wind has carried things, in seconds at full wind: every
    /// particle drifts by a multiple of this, so its motion always matches
    /// the wind of the moment and turns round only when the wind does.
    fn drift(&self) -> f64 {
        self.w.drift
    }

    fn count(&self, max: usize, level: f64, fraction: f64) -> usize {
        (max as f64 * level.clamp(0.0, 1.0) * fraction) as usize
    }

    /// The far layer, behind the architecture: light shafts, clouds, the
    /// lightning bolt, fog banks, and the distant rain and snow.
    pub fn far(&self, fb: &mut Framebuffer) {
        self.shafts(fb);
        self.clouds(fb);
        self.bolt(fb);
        self.fog_banks(fb, 105.0, 200.0, 4, 120);
        self.rain(fb, FAR_RAIN, MAX_FAR_RAIN, (3, 6), (150.0, 210.0), 105);
        self.flakes(fb, true);
    }

    /// The near layer, over everything: the close rain, snow, leaves, motes,
    /// embers and sparks, then the low mist.
    pub fn near(&self, fb: &mut Framebuffer, ground: &dyn Fn(f64, f64) -> Point) {
        self.rain(fb, NEAR_RAIN, MAX_NEAR_RAIN, (9, 17), (230.0, 300.0), 235);
        self.flakes(fb, false);
        self.leaves(fb);
        self.motes(fb);
        self.embers(fb);
        self.sparks(fb, ground);
        self.butterflies(fb);
        self.fog_banks(fb, 205.0, 290.0, 3, 95);
    }

    /// Soft diagonal shafts of light, in clear weather and at dusk.
    fn shafts(&self, fb: &mut Framebuffer) {
        let level = self.w.motes * self.w.light * (1.0 - self.w.cloud);
        if level < 0.05 {
            return;
        }
        let colour = (self.grade)([255, 236, 196, 255]);
        for i in 0..3 {
            let mut r = particle(self.seed, SHAFTS, i);
            let (x0, width, drift) = (r.range(-40.0, 220.0), r.range(14.0, 34.0), r.range(0.02, 0.05));
            // Each shaft slowly brightens and fades on its own.
            let a = level * (0.5 + 0.5 * sin_turns(self.t * drift + r.unit()));
            let alpha = (a * 34.0) as u32;
            for y in 0..(H as i32 * 3 / 4) {
                // Leaning down to the right, fading toward the ground.
                let fade = 1.0 - y as f64 / (H * 0.75);
                let cx = x0 + y as f64 * 0.55;
                for x in (cx - width / 2.0) as i32..=(cx + width / 2.0) as i32 {
                    let edge = 1.0 - ((x as f64 - cx).abs() / (width / 2.0)).min(1.0);
                    fb.blend_over(x, y, colour, (alpha as f64 * fade * edge) as u32);
                }
            }
        }
    }

    /// Clouds drifting across the top of the sky with the wind; darker and
    /// heavier the stormier it is.
    fn clouds(&self, fb: &mut Framebuffer) {
        let level = self.w.cloud;
        if level < 0.05 {
            return;
        }
        let dark = blend(self.palette.fog, [26, 28, 38, 255], (level * 85.0) as u32, 100);
        let lit = blend(dark, [235, 240, 255, 255], (self.w.flash * 80.0) as u32, 100);
        let colour = (self.grade)(lit);
        let speed = 18.0;
        let n = 3 + (level * 6.0) as usize;
        for i in 0..n {
            let mut r = particle(self.seed, CLOUDS, i);
            let (w, h) = (r.range(60.0, 150.0), r.range(9.0, 22.0));
            let span = W + w * 2.0;
            let cx = wrap(r.unit() * span + self.drift() * speed * r.range(0.7, 1.3), span) - w;
            let cy = r.range(6.0, 85.0);
            let alpha = level * r.range(90.0, 170.0);
            for y in (cy - h) as i32..=(cy + h) as i32 {
                let dy = (y as f64 - cy) / h;
                for x in (cx - w) as i32..=(cx + w) as i32 {
                    let dx = (x as f64 - cx) / w;
                    // A soft-edged ellipse with a lumpy top.
                    let lump = 0.12 * sin_turns(dx * 2.5 + r.unit());
                    let d = dx * dx + (dy + lump) * (dy + lump);
                    if d < 1.0 {
                        fb.blend_over(x, y, colour, (alpha * (1.0 - d)) as u32);
                    }
                }
            }
        }
    }

    /// Horizontal fog bands between rows `y0` and `y1`, swelling and drifting.
    fn fog_banks(&self, fb: &mut Framebuffer, y0: f64, y1: f64, bands: u64, max_alpha: u32) {
        let level = self.w.fog;
        if level < 0.12 {
            return;
        }
        let colour = (self.grade)(self.palette.fog);
        // A slow creep of its own, plus the wind.
        let travel = self.drift() * 6.0 + self.t * 2.0;
        for b in 0..bands {
            let mut r = particle(self.seed, FOG, (b + (y0 as u64) * 10) as usize);
            let cy = y0 + (y1 - y0) * (b as f64 + 0.5) / bands as f64;
            let thick = r.range(10.0, 22.0);
            let (f1, f2, ph) = (r.range(1.0, 2.5), r.range(3.0, 5.0), r.unit());
            let a = level * max_alpha as f64 * (0.6 + 0.4 * sin_turns(self.t * 0.03 + ph));
            for x in 0..LOGICAL_W as i32 {
                let u = (x as f64 + travel) / W;
                let edge = cy + 6.0 * sin_turns(u * f1 + ph) + 3.0 * sin_turns(u * f2 - self.t * 0.05);
                for y in (edge - thick) as i32..=(edge + thick) as i32 {
                    let d = ((y as f64 - edge) / thick).abs();
                    fb.blend_over(x, y, colour, (a * (1.0 - d * d)) as u32);
                }
            }
        }
    }

    /// The bolt of any strike in its first quarter-second: a jagged line down
    /// from the top of the sky with a branch or two, and a soft glow.
    fn bolt(&self, fb: &mut Framebuffer) {
        if self.w.flash <= 0.0 {
            return;
        }
        for e in self.weather.events(EventKind::Strike, self.t - 0.25, self.t + 1e-9) {
            let (strength, distance) = e.strike();
            let age = self.t - e.at;
            let a = strength * (1.0 - 0.5 * distance) * exp2(-age / 0.06);
            let mut r = Rng::new(e.hash ^ 0x424f_4c54);
            let x0 = r.range(0.12, 0.88) * W;
            let bottom = r.range(90.0, 150.0) * (1.0 - 0.3 * distance);
            let core = [245, 248, 255, 255];
            let glow = [170, 190, 255, 255];
            let mut path = vec![(x0, 0.0)];
            let steps = 14;
            for k in 1..=steps {
                let (px, _) = path[k - 1];
                path.push((px + r.range(-7.0, 7.0), bottom * k as f64 / steps as f64));
            }
            let mut segments: Vec<((f64, f64), (f64, f64), f64)> = path.windows(2).map(|p| (p[0], p[1], 1.0)).collect();
            for _ in 0..r.int(1, 2) {
                let mut at = path[r.int(3, steps as u64 - 3) as usize];
                let lean = if r.chance(0.5) { -1.0 } else { 1.0 };
                for _ in 0..r.int(3, 6) {
                    let next = (at.0 + lean * r.range(2.0, 8.0), at.1 + r.range(5.0, 10.0));
                    segments.push((at, next, 0.55));
                    at = next;
                }
            }
            for (p, q, weight) in segments {
                let alpha = (a * weight * 255.0) as u32;
                stroke(fb, p, q, glow, alpha / 3, 1);
                stroke(fb, p, q, core, alpha, 0);
            }
        }
    }

    /// One rain layer: streaks tilted by the wind whose heads are bright and
    /// whose tails fade, so they read as motion rather than as lines.
    fn rain(&self, fb: &mut Framebuffer, layer: u64, max: usize, len: (u64, u64), speed: (f64, f64), alpha: u32) {
        let n = self.count(max, self.w.rain, self.rain_fraction);
        if n == 0 {
            return;
        }
        let base = blend(self.palette.sky_bottom, [232, 238, 250, 255], 78, 100);
        let colour = (self.grade)(base);
        let wind_x = self.wind_x();
        let span = H + 40.0;
        for i in 0..n {
            let mut r = particle(self.seed, layer, i);
            let (x, phase, v, l) = (r.unit() * W, r.unit(), r.range(speed.0, speed.1), r.int(len.0, len.1) as i32);
            let y = wrap(phase * span + self.t * v, span) - 20.0;
            let x = wrap(x + self.drift() * v * RAIN_SLANT, W);
            // The streak lies along the drop's travel: across by the wind,
            // down by its fall.
            let dx = wind_x * RAIN_SLANT * l as f64;
            for k in 0..=l {
                let f = k as f64 / l as f64; // 0 at the tail, 1 at the head
                let px = (x + dx * f).round() as i32;
                fb.blend_over(px, y as i32 + k, colour, (alpha as f64 * (0.15 + 0.85 * f * f)) as u32);
            }
        }
    }

    /// Snowflakes in three sizes, swaying and drifting with the wind, some of
    /// them catching the light. The far layer is the small, slow ones.
    fn flakes(&self, fb: &mut Framebuffer, far: bool) {
        let n = self.count(MAX_FLAKES, self.w.snow, self.fraction);
        if n == 0 {
            return;
        }
        let white = (self.grade)([246, 248, 255, 255]);
        let span = H + 20.0;
        for i in 0..n {
            let mut r = particle(self.seed, FLAKES, i);
            let size = if r.chance(0.55) { 0 } else if r.chance(0.7) { 1 } else { 2 };
            if far != (size == 0) {
                continue;
            }
            let (x, phase, v) = (r.unit() * W, r.unit(), r.range(10.0, 22.0) * (1.0 + size as f64 * 0.6));
            let (sway, sway_hz, sway_at) = (r.range(2.0, 7.0), r.range(0.15, 0.4), r.unit());
            let y = wrap(phase * span + self.t * v, span) - 10.0;
            let x = wrap(x + self.drift() * 30.0 + sway * sin_turns(self.t * sway_hz + sway_at), W);
            let twinkle = 0.5 + 0.5 * sin_turns(self.t * r.range(0.5, 2.0) + r.unit());
            let alpha = match size {
                0 => 150,
                1 => 190 + (65.0 * twinkle) as u32,
                _ => 240,
            };
            let (px, py) = (x as i32, y as i32);
            fb.blend_over(px, py, white, alpha);
            if size == 2 {
                for (ox, oy) in [(1, 0), (0, 1), (1, 1)] {
                    fb.blend_over(px + ox, py + oy, white, alpha * 2 / 3);
                }
            }
        }
    }

    /// Leaves and debris blown across the scene, tumbling (their colour
    /// flips as they turn) and fluttering up and down.
    fn leaves(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_LEAVES, self.w.leaves, self.fraction);
        if n == 0 {
            return;
        }
        let tones = [self.palette.foliage, self.palette.foliage_light, self.palette.accent, self.palette.ground];
        let span = W + 40.0;
        for i in 0..n {
            let mut r = particle(self.seed, LEAVES, i);
            let (x, y0, v) = (r.unit() * span, r.range(40.0, 270.0), r.range(50.0, 130.0));
            let (flutter, f_hz, at) = (r.range(3.0, 12.0), r.range(0.3, 0.9), r.unit());
            let x = wrap(x + self.drift() * v, span) - 20.0;
            let y = y0 + flutter * sin_turns(self.t * f_hz + at) + 4.0 * sin_turns(self.t * f_hz * 3.1 + at);
            let tumble = sin_turns(self.t * r.range(0.8, 2.2) + r.unit());
            // Lifted toward the light, so a leaf reads against the moss.
            let tone = blend(tones[r.below(tones.len())], [255, 236, 170, 255], 30, 100);
            let colour = (self.grade)(if tumble > 0.0 { tone } else { blend(tone, [20, 20, 20, 255], 40, 100) });
            let (px, py) = (x as i32, y as i32);
            fb.put(px, py, colour);
            fb.put(px, py + 1, colour);
            // Face-on it shows its width; edge-on, only a sliver.
            if tumble.abs() > 0.3 {
                fb.put(px + 1, py, colour);
                fb.put(px + 1, py + 1, colour);
            }
            if tumble.abs() > 0.7 {
                fb.put(px + 2, py + 1, colour);
            }
        }
    }

    /// Dust motes drifting slowly in warm light, glinting as they turn.
    fn motes(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_MOTES, self.w.motes, self.fraction);
        if n == 0 {
            return;
        }
        let colour = (self.grade)([255, 244, 214, 255]);
        for i in 0..n {
            let mut r = particle(self.seed, MOTES, i);
            let (x0, y0) = (r.unit() * W, r.range(30.0, 260.0));
            let (ax, ay, hz, at) = (r.range(6.0, 20.0), r.range(4.0, 12.0), r.range(0.03, 0.09), r.unit());
            let x = wrap(x0 + ax * sin_turns(self.t * hz + at) + self.drift() * 8.0, W);
            let y = y0 + ay * sin_turns(self.t * hz * 1.3 + at * 2.0);
            let glint = 0.5 + 0.5 * sin_turns(self.t * r.range(0.2, 0.7) + r.unit());
            fb.blend_over(x as i32, y as i32, colour, (40.0 + 200.0 * glint * glint * self.w.light) as u32);
        }
    }

    /// Embers rising from the ground, swaying, cooling from yellow through
    /// orange to a dull red as they climb, with a faint glow around the hot
    /// ones.
    fn embers(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_EMBERS, self.w.embers, self.fraction);
        if n == 0 {
            return;
        }
        let span = 240.0;
        for i in 0..n {
            let mut r = particle(self.seed, EMBERS, i);
            let (x0, phase, v) = (r.unit() * W, r.unit(), r.range(8.0, 26.0));
            let (sway, hz, at) = (r.range(3.0, 10.0), r.range(0.1, 0.35), r.unit());
            let climb = wrap(phase * span + self.t * v, span);
            let life = climb / span; // 0 just lifted, 1 burnt out
            let y = 290.0 - climb;
            let x = wrap(x0 + sway * sin_turns(self.t * hz + at) + self.drift() * 10.0, W);
            let hot = [255, 214, 120, 255];
            let cool = [170, 50, 36, 255];
            let colour = (self.grade)(blend(hot, cool, (life * 100.0) as u32, 100));
            let flicker = 0.7 + 0.3 * sin_turns(self.t * r.range(3.0, 7.0) + r.unit());
            let alpha = ((1.0 - life) * 255.0 * flicker) as u32;
            let (px, py) = (x as i32, y as i32);
            fb.blend_over(px, py, colour, alpha);
            if life < 0.6 {
                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    fb.blend_over(px + ox, py + oy, colour, alpha / 3);
                }
            }
            if life < 0.25 {
                fb.blend_over(px, py - 1, [255, 240, 200, 255], alpha / 2);
            }
        }
    }

    /// A rare butterfly drifting slowly across the scene: a steady, gently
    /// wavering path rather than a bounce, its wings opening and closing.
    fn butterflies(&self, fb: &mut Framebuffer) {
        for e in self.weather.events(EventKind::Butterfly, self.t - BUTTERFLY_S, self.t + 1e-9) {
            let age = (self.t - e.at) / BUTTERFLY_S;
            let mut r = Rng::new(e.hash ^ 0x4246_4c59);
            let dir = if r.chance(0.5) { -1.0 } else { 1.0 };
            let (y0, rise) = (r.range(110.0, 220.0), r.range(-25.0, 25.0));
            let (sway_at, flap_hz) = (r.unit(), r.range(2.5, 3.5));
            let x = if dir > 0.0 { -6.0 + age * (W + 12.0) } else { W + 6.0 - age * (W + 12.0) };
            // A slow drift up or down over the whole crossing, and a waver of
            // a pixel or so — never a bounce.
            let y = y0 + rise * age + 1.2 * sin_turns(age * 3.0 + sway_at);
            let colour = (self.grade)(blend(self.palette.accent, [255, 244, 210, 255], 35, 100));
            let (px, py) = (x as i32, y as i32);
            fb.put(px, py, colour);
            // Wings: open, a pixel either side; closed, folded up into one.
            if sin_turns(self.t * flap_hz + sway_at) > 0.0 {
                fb.put(px - 1, py - 1, colour);
                fb.put(px + 1, py - 1, colour);
            } else {
                fb.put(px, py - 1, colour);
            }
        }
    }

    /// A spark's flare: a burst of three to five hot points thrown up from
    /// where it popped, falling back as they fade. The weather bed plays a
    /// crackle at exactly `e.at`.
    fn sparks(&self, fb: &mut Framebuffer, ground: &dyn Fn(f64, f64) -> Point) {
        if self.w.embers <= 0.0 {
            return;
        }
        for e in self.weather.events(EventKind::Spark, self.t - SPARK_S, self.t + 1e-9) {
            let age = (self.t - e.at) / SPARK_S;
            let (gx, gy) = e.spot();
            let base = ground(gx, gy);
            let (bx, by) = (to_px_round(base.x) as f64, to_px_round(base.y) as f64);
            let mut r = Rng::new(e.hash ^ 0x5350_524b);
            let colour = (self.grade)([255, 200, 110, 255]);
            for _ in 0..r.int(3, 5) {
                let (vx, vy) = (r.range(-14.0, 14.0), r.range(18.0, 34.0));
                let s = age * SPARK_S;
                let x = bx + vx * s;
                let y = by - vy * s + 40.0 * s * s;
                fb.blend_over(x as i32, y as i32, colour, ((1.0 - age) * 255.0) as u32);
            }
        }
    }
}

/// The splash of a raindrop landing at plane point `at` (fixed point), `age`
/// in `[0, 1]`: a flattened ring spreading out, and a crown of droplets thrown
/// up and falling back. Larger on stone (`hard`) than in the moss.
pub fn splash(fb: &mut Framebuffer, at: Point, age: f64, hard: bool, colour: Rgba) {
    let (cx, cy) = (to_px_round(at.x) as f64, to_px_round(at.y) as f64);
    let fade = 1.0 - age;
    // The moment of impact: a bright point.
    if age < 0.2 {
        fb.blend_over(cx as i32, cy as i32, colour, 255);
    }
    let ring = (1.5 + if hard { 4.5 } else { 3.0 } * age, 0.5);
    let steps = 14;
    for k in 0..steps {
        let a = k as f64 / steps as f64;
        let x = cx + ring.0 * cos_turns(a);
        let y = cy + ring.0 * ring.1 * sin_turns(a);
        fb.blend_over(x.round() as i32, y.round() as i32, colour, (fade * 215.0) as u32);
    }
    if hard {
        for (dir, lift) in [(-1.0, 1.0), (1.0, 0.8), (-0.4, 1.3), (0.5, 1.2)] {
            let x = cx + dir * 4.0 * age;
            let y = cy - lift * 6.0 * age + 9.0 * age * age;
            fb.blend_over(x.round() as i32, y.round() as i32, colour, (fade * 230.0) as u32);
        }
    }
}

/// A drip at `age` in `[0, 1]`: a bead swelling on the edge at `at`, then
/// falling `fall` logical pixels, glinting.
pub fn drip(fb: &mut Framebuffer, at: Point, fall: f64, age: f64, colour: Rgba) {
    let (cx, cy) = (to_px_round(at.x), to_px_round(at.y) as f64);
    if age < 0.4 {
        // Swelling on the edge.
        fb.blend_over(cx, cy as i32, colour, (age / 0.4 * 220.0) as u32);
        return;
    }
    let f = (age - 0.4) / 0.6;
    let y = cy + fall * f * f;
    fb.blend_over(cx, y as i32, colour, 235);
    fb.blend_over(cx, y as i32 - 1, colour, 90);
}

/// A thick-ish line from `p` to `q` in logical pixels, composited.
fn stroke(fb: &mut Framebuffer, p: (f64, f64), q: (f64, f64), colour: Rgba, alpha: u32, radius: i32) {
    let n = ((q.0 - p.0).abs().max((q.1 - p.1).abs()) as i32).max(1);
    for k in 0..=n {
        let f = k as f64 / n as f64;
        let (x, y) = ((p.0 + (q.0 - p.0) * f) as i32, (p.1 + (q.1 - p.1) * f) as i32);
        for ox in -radius..=radius {
            fb.blend_over(x + ox, y, colour, alpha);
        }
    }
}

/// Events of `kind` whose effect is visible at `t` (begun within `span`).
pub fn live(weather: &Weather, kind: EventKind, t: f64, span: f64) -> Vec<(Event, f64)> {
    weather
        .events(kind, t - span, t + 1e-9)
        .into_iter()
        .map(|e| {
            let age = ((t - e.at) / span).clamp(0.0, 1.0);
            (e, age)
        })
        .collect()
}

}

// ---- src/wildlife.rs ----
pub mod wildlife {
//! Wildlife: a few creatures living around the shrine, heard (and sometimes
//! seen) when the **Wildlife** setting is on.
//!
//! Which creatures live here comes from the **biome**, and the seed picks two of
//! them and gives each its own voice (pitch) and temper (how often it calls).
//! Every call is an event on a stateless schedule, like the weather's, so the
//! audio instance hears a songbird at the moment the visual instance shows it
//! flit across the sky. The weather decides how much they call: freely in
//! clear air and at dusk, hardly at all in a storm or in snow.

use crate::framebuffer::{Framebuffer, Rgba};
use crate::math::sin_turns;
use crate::plants::Biome;
use crate::rng::{hash3, Rng};
use crate::weather::{WeatherKind, WeatherState};
use crate::{LOGICAL_H, LOGICAL_W};

const STREAM: u64 = 0x5749_4c44; // "WILD"

/// The creatures. Each is synthesised from scratch in
/// [`crate::creatures`]; fireflies are only seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Creature {
    /// Pulsed, resonant croaks.
    Frog,
    /// A soft two- or three-note call.
    Dove,
    /// Short phrases of quick chirps, and a small bird crossing the sky.
    Songbird,
    /// Bursts of three or four high pulses.
    Cricket,
    /// A long, fast-trembling high trill.
    Cicada,
    /// Low, breathy hoots.
    Owl,
    /// Slow green-gold glows over the plateau.
    Firefly,
}

impl Creature {
    pub fn name(self) -> &'static str {
        match self {
            Creature::Frog => "frogs",
            Creature::Dove => "doves",
            Creature::Songbird => "songbirds",
            Creature::Cricket => "crickets",
            Creature::Cicada => "cicadas",
            Creature::Owl => "owls",
            Creature::Firefly => "fireflies",
        }
    }

    /// (slots, shortest period, longest period): how the calls are scheduled.
    fn schedule(self) -> (u64, f64, f64) {
        match self {
            Creature::Frog => (4, 2.0, 5.0),
            Creature::Dove => (2, 7.0, 14.0),
            Creature::Songbird => (3, 4.0, 9.0),
            Creature::Cricket => (5, 1.2, 3.0),
            Creature::Cicada => (2, 6.0, 12.0),
            Creature::Owl => (1, 10.0, 18.0),
            Creature::Firefly => (8, 1.5, 4.0),
        }
    }

    /// Night creatures swell at dusk; birds settle.
    fn at_dusk(self) -> f64 {
        match self {
            Creature::Cricket | Creature::Cicada | Creature::Frog | Creature::Firefly | Creature::Owl => 1.6,
            Creature::Songbird => 0.5,
            Creature::Dove => 0.8,
        }
    }

    /// How long a call lasts (seconds): for drawing what goes with it.
    pub fn seen_for(self) -> f64 {
        match self {
            Creature::Songbird => 1.8,
            Creature::Firefly => 2.2,
            _ => 0.0,
        }
    }
}

/// The creatures a biome can hold, most characteristic first.
pub fn residents(biome: Biome) -> [Creature; 3] {
    match biome {
        Biome::Moss => [Creature::Frog, Creature::Dove, Creature::Cricket],
        Biome::Verdant => [Creature::Songbird, Creature::Cricket, Creature::Dove],
        Biome::Fungal => [Creature::Cicada, Creature::Owl, Creature::Firefly],
        Biome::NearFuture => [Creature::Dove, Creature::Cicada, Creature::Cricket],
        // The night shift: crickets in the ducts, a roosting owl, fireflies
        // escaped from the bio bay.
        Biome::ResearchLab => [Creature::Cricket, Creature::Firefly, Creature::Owl],
        Biome::Ecofuture => [Creature::Songbird, Creature::Frog, Creature::Cricket],
        Biome::Greenhouse => [Creature::Songbird, Creature::Frog, Creature::Firefly],
    }
}

/// One creature living here: its kind, voice and temper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resident {
    pub creature: Creature,
    /// Pitch multiplier, 0.8..1.25.
    pub pitch: f64,
    /// How readily it calls, 0.6..1.0.
    pub rate: f64,
    stream: u64,
}

/// One call: who, when, and its own random numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
    pub resident: Resident,
    pub at: f64,
    pub hash: u64,
}

impl Call {
    pub fn rng(&self) -> Rng {
        Rng::new(self.hash)
    }

    /// Stereo position in `[-1, 1]`.
    pub fn pan(&self) -> f64 {
        Rng::new(self.hash ^ 0x5041_4e).bipolar() * 0.8
    }

    /// How near it is, `0.35..1`: nearer is louder and brighter.
    pub fn nearness(&self) -> f64 {
        Rng::new(self.hash ^ 0x4e45_4152).range(0.35, 1.0)
    }
}

/// The wildlife of one shrine.
#[derive(Clone, Debug, PartialEq)]
pub struct Wildlife {
    seed: u64,
    pub residents: Vec<Resident>,
}

impl Wildlife {
    /// Two of the biome's creatures, chosen and voiced by the seed.
    pub fn new(seed: u64, biome: Biome) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        let pool = residents(biome);
        let first = rng.below(pool.len());
        let second = (first + 1 + rng.below(pool.len() - 1)) % pool.len();
        let residents = [pool[first], pool[second]]
            .into_iter()
            .enumerate()
            .map(|(k, creature)| Resident {
                creature,
                pitch: rng.range(0.8, 1.25),
                rate: rng.range(0.6, 1.0),
                stream: STREAM ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9),
            })
            .collect();
        Wildlife { seed, residents }
    }

    /// "frogs and crickets", for the setup screen and the info card.
    pub fn summary(&self) -> String {
        let names: Vec<&str> = self.residents.iter().map(|r| r.creature.name()).collect();
        names.join(" and ")
    }

    /// How much a creature calls in this weather, in `[0, 1]`.
    fn activity(creature: Creature, kind: WeatherKind) -> f64 {
        let base = match kind {
            WeatherKind::Clear => 1.0,
            WeatherKind::Dusk => creature.at_dusk(),
            WeatherKind::Rain => 0.35,
            WeatherKind::Wind => 0.45,
            WeatherKind::Fog => 0.6,
            WeatherKind::Snow => 0.08,
            WeatherKind::Storm => 0.04,
        };
        // Frogs love the rain.
        let base = if creature == Creature::Frog && kind == WeatherKind::Rain { 1.1 } else { base };
        base.clamp(0.0, 1.0)
    }

    /// Every call in `[t0, t1)`. Pure: the same range always gives the same
    /// calls, in the audio and the visuals alike.
    pub fn calls(&self, kind: WeatherKind, t0: f64, t1: f64) -> Vec<Call> {
        let mut out = Vec::new();
        if t1 <= t0 {
            return out;
        }
        for resident in &self.residents {
            let gate = Self::activity(resident.creature, kind) * resident.rate;
            let (slots, lo, hi) = resident.creature.schedule();
            let mut slot_rng = Rng::stream(self.seed, resident.stream);
            for slot in 0..slots {
                let period = slot_rng.range(lo, hi);
                let phase = slot_rng.unit() * period;
                let first = ((t0 + phase) / period).floor() as i64 - 1;
                let last = ((t1 + phase) / period).floor() as i64;
                for n in first..=last {
                    let mut r = Rng::new(hash3(self.seed ^ resident.stream, slot, n as u64));
                    let at = (n as f64 + 0.9 * r.unit()) * period - phase;
                    if at < t0 || at >= t1 || r.unit() >= gate {
                        continue;
                    }
                    out.push(Call { resident: *resident, at, hash: r.next_u64() });
                }
            }
        }
        out
    }

    /// Draw what can be seen of the calls under way at `t`: a songbird crossing
    /// the sky, fireflies glowing over the plateau. Rain thins them like it
    /// thins the calls (they are the same events).
    pub fn draw(&self, fb: &mut Framebuffer, kind: WeatherKind, t: f64, w: &WeatherState, grade: &dyn Fn(Rgba) -> Rgba) {
        let longest = self.residents.iter().map(|r| r.creature.seen_for()).fold(0.0, f64::max);
        if longest <= 0.0 {
            return;
        }
        for call in self.calls(kind, t - longest, t + 1e-9) {
            let span = call.resident.creature.seen_for();
            let age = (t - call.at) / span;
            if !(0.0..1.0).contains(&age) {
                continue;
            }
            let mut r = call.rng();
            match call.resident.creature {
                Creature::Songbird => {
                    // A small dark bird crossing the upper sky, wings beating.
                    let dir = if r.chance(0.5) { -1.0 } else { 1.0 };
                    let x0 = r.range(40.0, LOGICAL_W as f64 - 40.0);
                    let y0 = r.range(20.0, 110.0);
                    let x = x0 + dir * age * r.range(90.0, 160.0);
                    let y = y0 - 10.0 * age + 3.0 * sin_turns(age * 3.0);
                    let colour = grade([40, 36, 44, 255]);
                    let flap = sin_turns(t * 7.0 + r.unit()) > 0.0;
                    let (px, py) = (x as i32, y as i32);
                    fb.put(px, py, colour);
                    let wing = if flap { -1 } else { 0 };
                    fb.put(px - 1, py + wing, colour);
                    fb.put(px + 1, py + wing, colour);
                    fb.put(px - 2, py + wing - (flap as i32), colour);
                    fb.put(px + 2, py + wing - (flap as i32), colour);
                }
                Creature::Firefly => {
                    // A glow that swells and fades as it drifts upward.
                    let x = r.range(30.0, LOGICAL_W as f64 - 30.0) + 6.0 * sin_turns(age + r.unit());
                    let y = r.range(140.0, LOGICAL_H as f64 - 40.0) - 14.0 * age;
                    let glow = sin_turns(0.5 * age) * (1.0 - 0.5 * w.light);
                    let colour = [214, 240, 120, 255];
                    let (px, py) = (x as i32, y as i32);
                    fb.blend_over(px, py, colour, (glow * 255.0) as u32);
                    for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        fb.blend_over(px + ox, py + oy, colour, (glow * 90.0) as u32);
                    }
                }
                _ => {}
            }
        }
    }
}

}

// ============================================================================
// Music engine modules (src/audio/)
// ============================================================================
pub mod audio {
// ---- src/audio/ambience.rs ----
pub mod ambience {
//! The small sounds of the weather: drops landing, drips, ember crackle, the
//! hush of snow and its icy tinkles, leaves rustling and the low air of fog.
//!
//! Splashes, drips and sparks are *events* from the shared schedule in
//! [`crate::weather`], so each one is heard on the very sample its picture
//! appears. The textures in between (rustle, crackle, tinkles) are random, but
//! their density is the same level the scene draws its particles from.

use crate::filter::Svf;
use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::weather::WeatherState;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x414d_4249; // "AMBI"
const SR: f64 = SAMPLE_RATE as f64;
/// The most one-shot hits that can sound at once; the oldest give way.
const MAX_HITS: usize = 48;
/// The drips' echo, in seconds, and its feedback.
const ECHO_S: f64 = 0.19;
const ECHO_FB: f64 = 0.38;

/// One short sound: noise through a resonance, and/or a gliding sine.
#[derive(Clone)]
pub struct Hit {
    wait: usize,
    res: Svf,
    noise: f64,
    noise_coef: f64,
    tone: f64,
    tone_coef: f64,
    phase: f64,
    dt: f64,
    glide: f64,
    gain: (f64, f64),
    /// Whether it feeds the drip echo.
    echo: bool,
}

impl Hit {
    fn new(wait: usize, pan: f64, gain: f64) -> Self {
        let (l, r) = pan_gains(pan.clamp(-1.0, 1.0) * 0.8);
        Hit {
            wait,
            res: Svf::new(1_000.0, 1.0, SR),
            noise: 0.0,
            noise_coef: 0.0,
            tone: 0.0,
            tone_coef: 0.0,
            phase: 0.0,
            dt: 0.0,
            glide: 0.0,
            gain: (gain * l, gain * r),
            echo: false,
        }
    }

    fn with_noise(mut self, hz: f64, q: f64, seconds: f64) -> Self {
        self.res = Svf::new(hz, q, SR);
        self.noise = 1.0;
        self.noise_coef = decay_coef(seconds, SR);
        self
    }

    fn with_tone(mut self, hz: f64, to_hz: f64, seconds: f64) -> Self {
        self.tone = 1.0;
        self.tone_coef = decay_coef(seconds, SR);
        self.dt = hz / SR;
        // A straight glide that reaches `to_hz` over the tone's first time
        // constant (additive, so no logarithm is needed: `HC004`).
        self.glide = (to_hz - hz) / SR / (seconds * SR);
        self
    }

    /// A drop on stone: a short bright ping over a click.
    pub fn plink(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(1_800.0, 4_200.0);
        Hit::new(wait, pan, gain).with_noise(hz, 14.0, 0.012).with_tone(hz * 0.5, hz * 0.55, 0.025)
    }

    /// A drop in the moss: a dull, soft tap.
    pub fn tap(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        Hit::new(wait, pan, gain).with_noise(rng.range(900.0, 1_800.0), 1.2, 0.008)
    }

    /// A drip landing in a pool: a sine whose pitch leaps up as it closes.
    pub fn drip(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(700.0, 1_100.0);
        let mut h = Hit::new(wait, pan, gain).with_tone(hz, hz * 2.2, 0.045);
        h.echo = true;
        h
    }

    /// One crackle of an ember: a tiny bright click.
    pub fn crackle(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        Hit::new(wait, pan, gain).with_noise(rng.range(2_500.0, 6_000.0), 0.9, rng.range(0.002, 0.006))
    }

    /// An icy tinkle: a high, quick sine.
    pub fn tinkle(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(3_200.0, 6_400.0);
        Hit::new(wait, pan, gain).with_tone(hz, hz, 0.06)
    }

    fn next(&mut self, rng: &mut Rng) -> (f64, f64, bool) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0, self.echo);
        }
        let mut y = 0.0;
        if self.noise > 1e-4 {
            y += self.res.process(rng.bipolar() * self.noise).band * 2.0;
            self.noise *= self.noise_coef;
        }
        if self.tone > 1e-4 {
            y += sin_turns(self.phase) * self.tone;
            self.phase += self.dt;
            self.phase -= self.phase.floor();
            self.dt = (self.dt + self.glide).max(0.0);
            self.tone *= self.tone_coef;
        }
        (y * self.gain.0, y * self.gain.1, self.echo)
    }

    fn done(&self) -> bool {
        self.wait == 0 && self.noise <= 1e-4 && self.tone <= 1e-4
    }
}

/// The ambience: the event hits, the drip echo and the level-driven textures.
#[derive(Clone)]
pub struct Ambience {
    rng: Rng,
    hits: Vec<Hit>,
    echo: Vec<f64>,
    echo_at: usize,
    hush: Svf,
    fog_air: Svf,
    rustle: Svf,
    rustle_env: f64,
    rustle_coef: f64,
    /// Whether the blown leaves are heard. Off in the Wind weather, where
    /// their dense flurry of tiny bursts read as a crackle over the gusts.
    pub rustle_on: bool,
}

impl Ambience {
    pub fn new(seed: u64) -> Self {
        Ambience {
            rng: Rng::stream(seed, STREAM),
            hits: Vec::new(),
            echo: vec![0.0; (ECHO_S * SR) as usize],
            echo_at: 0,
            hush: Svf::new(380.0, 0.6, SR),
            fog_air: Svf::new(220.0, 0.6, SR),
            rustle: Svf::new(3_000.0, 0.8, SR),
            rustle_env: 0.0,
            rustle_coef: decay_coef(0.012, SR),
            rustle_on: true,
        }
    }

    /// Start a hit. `make` gets this ambience's random stream.
    pub fn trigger(&mut self, make: impl FnOnce(&mut Rng) -> Hit) {
        let hit = make(&mut self.rng);
        self.start(hit);
    }

    /// Start a hit made elsewhere (from its own event's random stream).
    pub fn start(&mut self, hit: Hit) {
        if self.hits.len() >= MAX_HITS {
            self.hits.remove(0);
        }
        self.hits.push(hit);
    }

    /// One stereo sample under weather `s`.
    pub fn next(&mut self, s: &WeatherState) -> (f64, f64) {
        // Textures whose density is the scene's particle level.
        let leaf_rate = 140.0 * s.leaves * (0.4 + 0.6 * s.gust.min(1.0) + 0.3);
        if self.rng.chance(leaf_rate / SR) {
            self.rustle_env = self.rng.range(0.3, 1.0);
        }
        if self.rng.chance(30.0 * s.embers / SR) {
            let pan = self.rng.bipolar();
            let gain = self.rng.range(0.05, 0.16);
            self.trigger(|r| Hit::crackle(0, r, pan, gain));
        }
        if self.rng.chance(4.0 * s.snow / SR) {
            let pan = self.rng.bipolar();
            let gain = self.rng.range(0.01, 0.03);
            self.trigger(|r| Hit::tinkle(0, r, pan, gain));
        }

        let x = self.rng.bipolar();
        // (Silenced, the leaves still draw their random numbers, so every
        // other sound of the bed stays exactly as it was.)
        let rustle = if self.rustle_on { self.rustle.process(x).band * self.rustle_env * 0.5 } else { 0.0 };
        self.rustle_env *= self.rustle_coef;
        let bed = 0.6 * s.snow * self.hush.process(x).low + 0.5 * s.fog * self.fog_air.process(x).low;

        let (mut l, mut r) = (bed + rustle * 0.8, bed + rustle);
        let mut wet = 0.0;
        for hit in self.hits.iter_mut() {
            let (a, b, echo) = hit.next(&mut self.rng);
            l += a;
            r += b;
            if echo {
                wet += 0.5 * (a + b);
            }
        }
        self.hits.retain(|h| !h.done());

        // The drips' echo: a short cave-like repeat.
        let back = self.echo[self.echo_at];
        self.echo[self.echo_at] = wet + back * ECHO_FB;
        self.echo_at = (self.echo_at + 1) % self.echo.len();
        (l + back * 0.6, r + back * 0.6)
    }
}



}
// ---- src/audio/atmos.rs ----
pub mod atmos {
//! Atmosphere loops: the short, evolving pad loops of 90s sample CDs and
//! game soundtracks, made the way a sampler made them.
//!
//! At start-up each chord of the progression gets its own loop, a few
//! seconds long, synthesised once by one of the timbres (formant voices, an FM
//! bell, a bell pad over a warm swell, a wave-cycle pad, or one of the noise
//! textures: Air, Hush, Gust, Shine, Rill, Veil and Mist), or one of the later
//! family: Drift, Tide, Ember, Ripple, Ebb, Bronze, Frost, Lantern and Loom. Everything that
//! moves inside the loop
//! (the vowel, the wave steps, the swells, the strikes) is timed to repeat
//! exactly once per loop. The loop point is then hidden the way a sampler
//! does it: the last moments are crossfaded into the first, so playback can
//! go round and round for ever without a seam.
//!
//! Playback reads the loop at `clock % length` and, during each chord's
//! morph bar, crossfades (equal power) into the next chord's loop at the
//! same position: the pad changes chord without ever restarting.

use crate::filter::Svf;
use crate::fm::{Algo, Fm3, Op};
use crate::harmony::Harmony;
use crate::math::{cos_turns, decay_coef, exp, exp2, midi_hz, sin_turns};
use crate::rng::Rng;
use crate::LoopDesign;

const SR: f64 = crate::SAMPLE_RATE as f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopTimbre {
    Voices,
    Bells,
    BellPad,
    WaveCycle,
    Air,
    Hush,
    Gust,
    Shine,
    Rill,
    Veil,
    Mist,
    Drift,
    Tide,
    Ember,
    Ripple,
    Ebb,
    Bronze,
    Frost,
    Lantern,
    Loom,
    Off,
}

pub const LOOP_TIMBRES: [LoopTimbre; 21] = [
    LoopTimbre::Voices,
    LoopTimbre::Bells,
    LoopTimbre::BellPad,
    LoopTimbre::WaveCycle,
    LoopTimbre::Air,
    LoopTimbre::Hush,
    LoopTimbre::Gust,
    LoopTimbre::Shine,
    LoopTimbre::Rill,
    LoopTimbre::Veil,
    LoopTimbre::Mist,
    LoopTimbre::Drift,
    LoopTimbre::Tide,
    LoopTimbre::Ember,
    LoopTimbre::Ripple,
    LoopTimbre::Ebb,
    LoopTimbre::Bronze,
    LoopTimbre::Frost,
    LoopTimbre::Lantern,
    LoopTimbre::Loom,
    LoopTimbre::Off,
];

impl LoopTimbre {
    pub fn name(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "Voices",
            LoopTimbre::Bells => "Bells",
            LoopTimbre::BellPad => "BellPad",
            LoopTimbre::WaveCycle => "Wave sequence",
            LoopTimbre::Air => "Air",
            LoopTimbre::Hush => "Hush",
            LoopTimbre::Gust => "Gust",
            LoopTimbre::Shine => "Shine",
            LoopTimbre::Rill => "Rill",
            LoopTimbre::Veil => "Veil",
            LoopTimbre::Mist => "Mist",
            LoopTimbre::Drift => "Drift",
            LoopTimbre::Tide => "Tide",
            LoopTimbre::Ember => "Ember",
            LoopTimbre::Ripple => "Ripple",
            LoopTimbre::Ebb => "Ebb",
            LoopTimbre::Bronze => "Bronze",
            LoopTimbre::Frost => "Frost",
            LoopTimbre::Lantern => "Lantern",
            LoopTimbre::Loom => "Loom",
            LoopTimbre::Off => "Off",
        }
    }

    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "synth voices whose vowel drifts a → o → u → e",
            LoopTimbre::Bells => "FM glass, struck softly in a slow pattern",
            LoopTimbre::BellPad => "a bright FM bell over a warm, swelling pad",
            LoopTimbre::WaveCycle => "the waveform changes every beat",
            LoopTimbre::Air => "noise tuned to the chord: airy, whispered",
            LoopTimbre::Hush => "noise shaped by gliding vowel formants: a wordless breath",
            LoopTimbre::Gust => "noise through a slowly opening low-pass: gusts and calm",
            LoopTimbre::Shine => "high, drifting resonances: air sparkling in the light",
            LoopTimbre::Rill => "narrow bands fluttering quickly: water over stones",
            LoopTimbre::Veil => "resonant bands sweeping up and back: a curtain of light",
            LoopTimbre::Mist => "a low, slowly breathing noise bed with almost no pitch",
            LoopTimbre::Drift => "detuned pulse-wave strings, slowly chorused and breathing",
            LoopTimbre::Tide => "a bright saw pad through a slowly sweeping notch: a phasing tide",
            LoopTimbre::Ember => "a dark pad pulsing through a soft rhythmic gate, in time",
            LoopTimbre::Ripple => "plucked strings arpeggiating the chord, trailing a soft dotted echo",
            LoopTimbre::Ebb => "reversed swells: each note breathes in backwards, then stops",
            LoopTimbre::Bronze => "inharmonic gong partials swelling and beating slowly",
            LoopTimbre::Frost => "tiny high grains scattered over the chord: crystalline frost",
            LoopTimbre::Lantern => "a breathy flute-organ with a chiff on each note and slow tremolo",
            LoopTimbre::Loom => "a soft square-wave arpeggio stepping through an opening filter",
            LoopTimbre::Off => "no loop layer",
        }
    }

    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "VOX",
            LoopTimbre::Bells => "BEL",
            LoopTimbre::BellPad => "BLP",
            LoopTimbre::WaveCycle => "WVC",
            LoopTimbre::Air => "AIR",
            LoopTimbre::Hush => "HSH",
            LoopTimbre::Gust => "GST",
            LoopTimbre::Shine => "SHN",
            LoopTimbre::Rill => "RIL",
            LoopTimbre::Veil => "VEL",
            LoopTimbre::Mist => "MST",
            LoopTimbre::Drift => "DRF",
            LoopTimbre::Tide => "TDE",
            LoopTimbre::Ember => "EMB",
            LoopTimbre::Ripple => "RPL",
            LoopTimbre::Ebb => "EBB",
            LoopTimbre::Bronze => "BRZ",
            LoopTimbre::Frost => "FRS",
            LoopTimbre::Lantern => "LNT",
            LoopTimbre::Loom => "LOM",
            LoopTimbre::Off => "OFF",
        }
    }

    pub fn from_code(code: &str) -> Option<LoopTimbre> {
        LOOP_TIMBRES.iter().copied().find(|t| t.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        LOOP_TIMBRES.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Loops are 3 to 8 seconds long, in whole half-time beats.
pub const MIN_SECONDS: u64 = 3;
pub const MAX_SECONDS: u64 = 8;
/// The loop-point crossfade: 150 ms.
pub const XFADE: usize = 7_200;
/// Each chord's loop is scaled to this RMS (per channel).
const LOOP_RMS: f64 = 0.1;
/// Coefficients of the moving filters are refreshed this often (samples).
const CONTROL: usize = 32;

/// How many beats a loop lasts: as close to `target_s` seconds as whole beats
/// allow, kept within 3-8 s. If `avoid` is given, the count is nudged so it
/// shares no factor with it, and the two loops drift against each other.
pub fn loop_beats(beat: u64, target_s: u64, avoid: Option<u64>) -> u64 {
    let sr = crate::SAMPLE_RATE as u64;
    let lo = (MIN_SECONDS * sr).div_ceil(beat).max(1);
    let hi = (MAX_SECONDS * sr / beat).max(lo);
    let n = ((target_s * sr + beat / 2) / beat).clamp(lo, hi);
    let Some(other) = avoid else { return n };
    let coprime = |m: u64| gcd(m, other) == 1;
    [n, n - 1, n + 1, n - 2, n + 2]
        .into_iter()
        .find(|m| (lo..=hi).contains(m) && coprime(*m))
        .unwrap_or(n)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// What happens in one chord's loop: chosen once, then played by both channels
/// (each with its own detune and phases), like the drone's chord recipes.
#[derive(Clone, Debug)]
pub struct Performance {
    /// MIDI notes, low to high.
    pub notes: Vec<f64>,
    /// Struck timbres: (note index, start within the loop in samples, velocity).
    pub strikes: Vec<(usize, usize, f64)>,
    /// Wave sequence: one single-cycle wave per beat, as harmonic amplitudes.
    pub waves: Vec<[f64; HARMONICS]>,
    /// Where in its cycle the slow in-loop movement starts (turns).
    pub offset: f64,
    /// Keep the FM bells' overtones whole-number (2-step garage).
    pub harmonic: bool,
    /// 2-step garage: the swung sixteenth grid (length, push), so scattered
    /// grains land on it. (0, 0) leaves them free.
    pub grid: (usize, usize),
}

pub const HARMONICS: usize = 12;
const TABLE: usize = 2048;

/// Chord tones moved into the loop register (MIDI 60-83), none closer than a
/// whole tone to another, `count` of them chosen at random.
fn pick_notes(rng: &mut Rng, chord: &[f64], count: usize) -> Vec<f64> {
    let mut pool: Vec<f64> = chord
        .iter()
        .map(|n| {
            let mut n = *n;
            while n < 60.0 {
                n += 12.0;
            }
            while n >= 84.0 {
                n -= 12.0;
            }
            n
        })
        .collect();
    pool.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pool.dedup();
    let mut out: Vec<f64> = Vec::new();
    while out.len() < count && !pool.is_empty() {
        let n = pool.remove(rng.below(pool.len()));
        if out.iter().all(|m| (m - n).abs() >= 2.0) {
            out.push(n);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn perform(timbre: LoopTimbre, rng: &mut Rng, chord: &[f64], beats: u64, beat: u64) -> Performance {
    let count = match timbre {
        LoopTimbre::BellPad | LoopTimbre::Shine | LoopTimbre::Veil | LoopTimbre::Ripple | LoopTimbre::Loom => 4,
        LoopTimbre::Mist => 1,
        LoopTimbre::Bronze => 2,
        _ => 3,
    };
    let notes = pick_notes(rng, chord, count);
    let mut strikes = Vec::new();
    match timbre {
        LoopTimbre::Bells => {
            // Each note struck once or twice, on beats or half-beats; the loop
            // always opens with a strike.
            for i in 0..notes.len() {
                for _ in 0..rng.int(1, 2) {
                    let at = rng.below(beats as usize * 2) * beat as usize / 2;
                    strikes.push((i, at, rng.range(0.55, 1.0)));
                }
            }
            if !strikes.iter().any(|s| s.1 == 0) {
                strikes[0].1 = 0;
            }
        }
        LoopTimbre::BellPad => {
            // One soft strum of the whole chord at the top of the loop, and a
            // quieter echo of one note half way through.
            for i in 0..notes.len() {
                strikes.push((i, i * 1_440, 0.9 - 0.1 * i as f64));
            }
            let half = (beats as usize / 2) * beat as usize;
            strikes.push((rng.below(notes.len()), half, 0.45));
        }
        LoopTimbre::Ember => {
            // A gate on every eighth: the downbeats always open, the rest by
            // chance, so the pad pulses in a pattern that repeats each loop.
            let step = beat as usize / 2;
            for s in 0..beats as usize * 2 {
                if s == 0 || s % 4 == 0 || rng.chance(0.62) {
                    let vel = if s % 4 == 0 { 1.0 } else { rng.range(0.45, 0.85) };
                    strikes.push((0, s * step, vel));
                }
            }
        }
        LoopTimbre::Ripple | LoopTimbre::Loom => {
            // An arpeggio on eighths, up then down the chord; Loom also leaps
            // an octave (an index past the last note means "an octave up").
            let n = notes.len().max(1);
            let span = if timbre == LoopTimbre::Loom { 2 * n } else { n };
            let mut order: Vec<usize> = (0..span).collect();
            order.extend((1..span.saturating_sub(1)).rev());
            let shift = rng.below(order.len());
            let step = beat as usize / 2;
            for s in 0..beats as usize * 2 {
                if s > 0 && rng.chance(0.18) {
                    continue;
                }
                let idx = order[(s + shift) % order.len()];
                let vel = if s % 2 == 0 { rng.range(0.8, 1.0) } else { rng.range(0.5, 0.8) };
                strikes.push((idx, s * step, vel));
            }
        }
        LoopTimbre::Ebb => {
            // Each note swells in backwards to a cut on a beat.
            for i in 0..notes.len() {
                let end = (1 + rng.below(beats as usize)) * beat as usize;
                strikes.push((i, end, rng.range(0.6, 1.0)));
            }
        }
        LoopTimbre::Lantern => {
            // Each note is re-tongued once or twice per loop, on a beat.
            for i in 0..notes.len() {
                for _ in 0..rng.int(1, 2) {
                    strikes.push((i, rng.below(beats as usize) * beat as usize, rng.range(0.6, 1.0)));
                }
            }
        }
        _ => {}
    }
    let waves = if timbre == LoopTimbre::WaveCycle {
        (0..beats)
            .map(|_| {
                let odd_only = rng.chance(0.3);
                // Roll-off: 1/√n (bright), 1/n (saw-like) or 1/n^1.5 (mellow).
                let tilt = rng.below(3);
                let mut w = [0.0; HARMONICS];
                for (h, a) in w.iter_mut().enumerate() {
                    let n = (h + 1) as f64;
                    if odd_only && (h + 1) % 2 == 0 {
                        continue;
                    }
                    let r = rng.unit();
                    let rolloff = match tilt {
                        0 => 1.0 / n.sqrt(),
                        1 => 1.0 / n,
                        _ => 1.0 / (n * n.sqrt()),
                    };
                    *a = r * r * rolloff;
                }
                w[0] = w[0].max(0.3);
                w
            })
            .collect()
    } else {
        Vec::new()
    };
    Performance { notes, strikes, waves, offset: rng.unit(), harmonic: false, grid: (0, 0) }
}

/// An event layer's per-chord settings: a few chord tones for the Knock grains
/// to strike, and a phase offset. The grid itself is the design.
fn perform_events(rng: &mut Rng, chord: &[f64]) -> Performance {
    let notes = pick_notes(rng, chord, 3);
    Performance { notes, strikes: Vec::new(), waves: Vec::new(), offset: rng.unit(), harmonic: false, grid: (0, 0) }
}

/// Band-limited sawtooth (polyBLEP), phase in turns, `dt` = increment per sample.
fn saw(phase: f64, dt: f64) -> f64 {
    let blep = if phase < dt {
        let x = phase / dt;
        x + x - x * x - 1.0
    } else if phase > 1.0 - dt {
        let x = (phase - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    };
    2.0 * phase - 1.0 - blep
}

/// Band-limited pulse of width `pw` (0..1): the difference of two saws.
fn pulse(phase: f64, pw: f64, dt: f64) -> f64 {
    let other = phase + pw;
    saw(phase, dt) - saw(other - other.floor(), dt)
}

fn advance(phase: &mut f64, dt: f64) {
    *phase += dt;
    *phase -= phase.floor();
}

/// Vowel formants (Hz) for "ah", "oh", "oo", "eh", visited in turn once per loop.
const VOWELS: [[f64; 3]; 4] = [[730.0, 1090.0, 2440.0], [570.0, 840.0, 2410.0], [300.0, 870.0, 2240.0], [530.0, 1840.0, 2480.0]];
const FORMANT_GAIN: [f64; 3] = [1.0, 0.6, 0.35];
const FORMANT_Q: [f64; 3] = [8.0, 10.0, 12.0];

/// The formants at `p` turns through the loop.
fn vowel_at(p: f64) -> [f64; 3] {
    let x = (p - p.floor()) * VOWELS.len() as f64;
    let i = x as usize % VOWELS.len();
    let j = (i + 1) % VOWELS.len();
    let f = x - x.floor();
    let s = 0.5 - 0.5 * cos_turns(0.5 * f); // eased
    [0, 1, 2].map(|k| VOWELS[i][k] + (VOWELS[j][k] - VOWELS[i][k]) * s)
}

/// One channel of one chord's loop, rendered over `total` samples: a full
/// warm-up loop (so tails and filters have settled), then the loop itself,
/// then the crossfade region.
fn render_channel(timbre: LoopTimbre, perf: &Performance, rng: &mut Rng, len: usize, beat: usize, total: usize) -> Vec<f64> {
    let mut out = vec![0.0; total];
    let turns = |i: usize| (i % len) as f64 / len as f64 + perf.offset;
    match timbre {
        LoopTimbre::Voices => {
            let mut src = vec![0.0; total];
            for note in &perf.notes {
                for d in [-7.0, 0.0, 7.0] {
                    let cents = d + rng.range(-2.5, 2.5);
                    let hz = midi_hz(note + cents / 100.0);
                    let mut ph = rng.unit();
                    let (mut vib, vib_hz) = (rng.unit(), rng.range(4.6, 5.6));
                    for s in src.iter_mut() {
                        let dt = hz * (1.0 + 0.0045 * sin_turns(vib)) / SR;
                        *s += saw(ph, dt);
                        advance(&mut ph, dt);
                        advance(&mut vib, vib_hz / SR);
                    }
                }
            }
            let mut noise = Rng::new(rng.next_u64());
            let mut formants = [Svf::default(), Svf::default(), Svf::default()];
            for (i, (o, s)) in out.iter_mut().zip(&src).enumerate() {
                if i % CONTROL == 0 {
                    let f = vowel_at(turns(i));
                    for k in 0..3 {
                        formants[k].set(f[k], FORMANT_Q[k], SR);
                    }
                }
                let x = s + 0.6 * noise.bipolar();
                let mut y = 0.0;
                for k in 0..3 {
                    y += FORMANT_GAIN[k] * formants[k].process(x).band / FORMANT_Q[k];
                }
                *o = y * (0.75 + 0.25 * sin_turns(turns(i)));
            }
        }
        LoopTimbre::Bells | LoopTimbre::BellPad => {
            let glass = timbre == LoopTimbre::Bells;
            let detune: Vec<f64> = perf.notes.iter().map(|_| rng.range(-4.0, 4.0) / 100.0).collect();
            let (r2, r3, decay) = if glass {
                // Glass: inharmonic 3.5 and 7.07, or (in garage, against
                // plain chords) the nearest whole-number ratios.
                let (a, b) = if perf.harmonic { (3.0, 4.0) } else { (3.5, 7.07) };
                (a, b, rng.range(1.6, 2.6))
            } else {
                (3.0, 1.0, rng.range(0.7, 1.1))
            };
            let ring = (4.0 * SR) as usize;
            for &(n, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n] + detune[n]);
                // The same strike in every pass, so the loop repeats exactly.
                let phases = [rng.unit(), rng.unit(), rng.unit()];
                let mut start = at;
                while start < total {
                    let mut v = Fm3::new(
                        hz,
                        if glass { Algo::Pair } else { Algo::Stack },
                        [Op::new(1.0, 0.0, phases[0]), Op::new(r2, 1.5, phases[1]), Op::new(r3, 0.7, phases[2])],
                        if glass { 0.0 } else { 0.3 },
                    );
                    let (c_amp, c_bright) = (decay_coef(decay, SR), decay_coef(decay * 0.35, SR));
                    let (mut amp, mut bright) = (vel, 1.0);
                    for o in out[start..(start + ring).min(total)].iter_mut() {
                        *o += 0.5 * amp * v.next(SR, 0.25 + 0.75 * bright);
                        amp *= c_amp;
                        bright *= c_bright;
                    }
                    start += len;
                }
            }
            // A quiet sustained bed underneath: sines for glass, a warm saw pad for BellPad.
            let mut bed = vec![0.0; total];
            for (k, note) in perf.notes.iter().enumerate() {
                let voices: &[f64] = if glass { &[0.0] } else { &[-9.0, 9.0] };
                for d in voices {
                    let dt = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    let swell_at = k as f64 / perf.notes.len() as f64;
                    for (i, b) in bed.iter_mut().enumerate() {
                        let swell = 0.5 + 0.5 * sin_turns(turns(i) + swell_at);
                        *b += swell * if glass { sin_turns(ph) } else { saw(ph, dt) };
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut lp = Svf::default();
            for (i, (o, b)) in out.iter_mut().zip(&bed).enumerate() {
                if i % CONTROL == 0 {
                    lp.set(1_000.0 + 1_400.0 * (0.5 + 0.5 * sin_turns(turns(i))), 0.8, SR);
                }
                *o += if glass { 0.12 * b } else { 0.35 * lp.process(*b).low };
            }
        }
        LoopTimbre::WaveCycle => {
            // One single-cycle table per beat, played through as a chord.
            let tables: Vec<Vec<f64>> = perf
                .waves
                .iter()
                .map(|w| {
                    let mut t: Vec<f64> = (0..TABLE)
                        .map(|i| {
                            let p = i as f64 / TABLE as f64;
                            w.iter().enumerate().map(|(h, a)| a * sin_turns((h + 1) as f64 * p)).sum::<f64>()
                        })
                        .collect();
                    let peak = t.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(1e-9);
                    t.iter_mut().for_each(|x| *x /= peak);
                    t
                })
                .collect();
            let steps = tables.len();
            let read = |t: &[f64], ph: f64| {
                let x = ph * TABLE as f64;
                let i = x as usize % TABLE;
                let f = x - x.floor();
                t[i] + (t[(i + 1) % TABLE] - t[i]) * f
            };
            let blend = beat / 4;
            for note in &perf.notes {
                let dt = midi_hz(note + rng.range(-5.0, 5.0) / 100.0) / SR;
                let mut ph = rng.unit();
                for (i, o) in out.iter_mut().enumerate() {
                    let pos = i % len;
                    let (step, into) = (pos / beat % steps, pos % beat);
                    let a = read(&tables[step], ph);
                    // Crossfade into the next wave over the last quarter of each beat.
                    let x = if into + blend >= beat {
                        let f = (into + blend - beat) as f64 / blend as f64;
                        let b = read(&tables[(step + 1) % steps], ph);
                        a + (b - a) * (0.5 - 0.5 * cos_turns(0.5 * f))
                    } else {
                        a
                    };
                    // Each step speaks with a soft accent, like a sequenced patch.
                    let accent = 0.75 + 0.25 * exp(-(into as f64) / (0.25 * beat as f64));
                    *o += x * accent;
                    advance(&mut ph, dt);
                }
            }
            let mut lp = Svf::new(5_000.0, 0.7, SR);
            out.iter_mut().for_each(|o| *o = lp.process(*o).low);
        }
        LoopTimbre::Air => {
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, Svf, f64)> = perf
                .notes
                .iter()
                .enumerate()
                .map(|(k, n)| {
                    let hz = midi_hz(*n + rng.range(-3.0, 3.0) / 100.0);
                    (Svf::new(hz, 28.0, SR), Svf::new(2.0 * hz, 22.0, SR), k as f64 / perf.notes.len() as f64)
                })
                .collect();
            let mut air = Svf::new(2_600.0, 1.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.04 * air.process(x).band;
                for (lo, hi, at) in bands.iter_mut() {
                    let swell = 0.5 + 0.5 * sin_turns(turns(i) + *at);
                    y += swell * (lo.process(x).band + 0.5 * hi.process(x).band) / 28.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Hush => {
            // Noise blown through the same three vocal formants as Voices,
            // but the formants glide through the vowel cycle on their own: a
            // wordless, breathy "shh" rather than a sung pitch.
            let mut noise = Rng::new(rng.next_u64());
            let mut formants = [Svf::default(), Svf::default(), Svf::default()];
            let mut air = Svf::new(3_200.0, 0.9, SR);
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    let f = vowel_at(turns(i));
                    for k in 0..3 {
                        formants[k].set(f[k], FORMANT_Q[k], SR);
                    }
                }
                let x = noise.bipolar();
                let mut y = 0.0;
                for k in 0..3 {
                    y += FORMANT_GAIN[k] * formants[k].process(x).band / FORMANT_Q[k];
                }
                y += 0.18 * air.process(x).band;
                *o = y * (0.7 + 0.3 * sin_turns(turns(i)));
            }
        }
        LoopTimbre::Gust => {
            // One wide low-pass gusting open and shut, plus a soft resonance on
            // the lowest chord note so the wind still belongs to the harmony.
            let mut noise = Rng::new(rng.next_u64());
            let mut lp = Svf::default();
            let mut res = Svf::new(perf.notes.first().map_or(220.0, |n| midi_hz(*n)), 3.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                if i % CONTROL == 0 {
                    // Two gusts of unequal length per loop, so calm and squall alternate.
                    let gust = 0.5 + 0.5 * sin_turns(p) * sin_turns(2.0 * p + 0.25);
                    lp.set(250.0 + 1_550.0 * gust, 0.8, SR);
                }
                let x = noise.bipolar();
                let air = 0.55 + 0.45 * (0.5 + 0.5 * sin_turns(p));
                *o = (lp.process(x).low + 0.6 * res.process(x).band) * air;
            }
        }
        LoopTimbre::Shine => {
            // Narrow high resonances on chord notes two octaves up, each lit by
            // its own slow tremolo: air catching the light.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, u64)> = perf
                .notes
                .iter()
                .map(|n| {
                    let hz = midi_hz(*n + 24.0 + rng.range(-5.0, 5.0) / 100.0);
                    (Svf::new(hz, 18.0, SR), rng.unit(), rng.int(1, 3))
                })
                .collect();
            let mut air = Svf::new(5_200.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.25 * air.process(x).band;
                for (f, phase, rate) in bands.iter_mut() {
                    let trem = 0.5 + 0.5 * sin_turns(turns(i) * *rate as f64 + *phase);
                    y += trem * f.process(x).band / 18.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Rill => {
            // Very narrow bands at the chord notes, each fluttering at its own
            // fast but whole-number rate: water running over stones.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, u64)> = perf
                .notes
                .iter()
                .map(|n| {
                    let hz = midi_hz(*n + rng.range(-6.0, 6.0) / 100.0);
                    (Svf::new(hz, 24.0, SR), rng.unit(), rng.int(6, 11))
                })
                .collect();
            let mut mid = Svf::new(1_500.0, 1.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.12 * mid.process(x).band;
                for (f, phase, rate) in bands.iter_mut() {
                    let flutter = 0.5 + 0.5 * sin_turns(turns(i) * *rate as f64 + *phase);
                    y += flutter * f.process(x).band / 24.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Veil => {
            // Four resonant bands that sweep a whole octave up and back down
            // across the loop, each at its own phase: a curtain of light that
            // opens and closes, still anchored to the chord.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, f64)> = perf
                .notes
                .iter()
                .enumerate()
                .map(|(k, n)| (Svf::default(), *n + rng.range(-4.0, 4.0) / 100.0, k as f64 / perf.notes.len().max(1) as f64))
                .collect();
            let mut air = Svf::new(4_000.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                if i % CONTROL == 0 {
                    for (f, base, at) in bands.iter_mut() {
                        let sweep = 0.5 - 0.5 * cos_turns(p + *at);
                        f.set(midi_hz(*base + 12.0 * sweep), 20.0, SR);
                    }
                }
                let x = noise.bipolar();
                let mut y = 0.22 * air.process(x).band;
                for (f, _, at) in bands.iter_mut() {
                    let sweep = 0.5 - 0.5 * cos_turns(p + *at);
                    y += (0.35 + 0.65 * sweep) * f.process(x).band / 20.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Mist => {
            // A steep low-pass breathing open and shut over one low resonance:
            // a near-pitchless bed, the quietest of the noise family.
            let mut noise = Rng::new(rng.next_u64());
            let mut lp = Svf::default();
            let mut res = Svf::new(perf.notes.first().map_or(110.0, |n| midi_hz(*n)), 1.4, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                let breath = 0.5 - 0.5 * cos_turns(p);
                if i % CONTROL == 0 {
                    lp.set(200.0 + 600.0 * breath, 0.7, SR);
                }
                let x = noise.bipolar();
                *o = (1.4 * lp.process(x).low + 0.8 * res.process(x).band) * (0.5 + 0.5 * breath);
            }
        }
        LoopTimbre::Drift => {
            // Three pulse waves per note, each narrowing and widening at its
            // own whole number of cycles per loop (the "chorus"), through a
            // soft low-pass that breathes once per loop.
            for note in &perf.notes {
                for d in [-9.0, 0.0, 9.0] {
                    let hz = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0);
                    let (mut ph, pwm_at, pwm_rate) = (rng.unit(), rng.unit(), rng.int(1, 3) as f64);
                    let (ens_at, ens_rate) = (rng.unit(), (len as f64 / SR * rng.range(2.2, 3.4)).round().max(1.0));
                    for (i, o) in out.iter_mut().enumerate() {
                        let p = turns(i);
                        let dt = hz * (1.0 + 0.0028 * sin_turns(p * ens_rate + ens_at)) / SR;
                        let pw = 0.5 + 0.38 * sin_turns(p * pwm_rate + pwm_at);
                        *o += pulse(ph, pw, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut lp = Svf::default();
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    lp.set(1_100.0 + 1_300.0 * (0.5 - 0.5 * cos_turns(turns(i))), 0.75, SR);
                }
                *o = lp.process(*o).low;
            }
        }
        LoopTimbre::Tide => {
            // Two bright saws per note through four all-pass stages whose
            // corner sweeps up and back once per loop; mixed with the dry
            // signal, the moving notches roll through the pad like a tide.
            for note in &perf.notes {
                for d in [-6.0, 6.0] {
                    let dt = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    for o in out.iter_mut() {
                        *o += saw(ph, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut stages = [(0.0f64, 0.0f64); 4];
            let (mut coef, mut fb) = (0.0, 0.0);
            let mut lp = Svf::new(4_200.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    let sweep = 0.5 - 0.5 * cos_turns(turns(i));
                    let g = crate::math::tan_turns(0.5 * 260.0 * exp2(3.6 * sweep) / SR);
                    coef = (g - 1.0) / (g + 1.0);
                }
                let dry = *o;
                let mut x = dry + 0.55 * fb;
                for (x1, y1) in stages.iter_mut() {
                    let y = coef * x + *x1 - coef * *y1;
                    *x1 = x;
                    *y1 = y;
                    x = y;
                }
                fb = x;
                *o = lp.process(0.5 * (dry + x)).low;
            }
        }
        LoopTimbre::Ember => {
            // A dark, slowly opening saw pad, opened and closed by the gate
            // pattern in the strikes. The gate is smoothed so it never clicks.
            let mut pad = vec![0.0; total];
            for note in &perf.notes {
                for d in [-8.0, 8.0] {
                    let dt = midi_hz(note - 12.0 + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    for p in pad.iter_mut() {
                        *p += saw(ph, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut gate = vec![0.0; len];
            let open = beat / 2 * 7 / 10;
            for &(_, at, vel) in &perf.strikes {
                for g in gate[at..(at + open).min(len)].iter_mut() {
                    *g = vel;
                }
            }
            let (mut env, coef) = (0.0, 1.0 - decay_coef(0.006, SR));
            let mut lp = Svf::default();
            for (i, (o, p)) in out.iter_mut().zip(&pad).enumerate() {
                if i % CONTROL == 0 {
                    lp.set(520.0 + 520.0 * (0.5 - 0.5 * cos_turns(turns(i))) + 600.0 * env, 1.1, SR);
                }
                env += (gate[i % len] - env) * coef;
                *o = lp.process(*p).low * env;
            }
        }
        LoopTimbre::Ripple => {
            // Karplus-Strong plucks: a burst of noise recirculating through a
            // gently damped delay of one period. Each pluck is the same burst
            // in every pass, so the loop repeats exactly.
            let ring = (1.6 * SR) as usize;
            for &(n, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n % perf.notes.len()] - 12.0 + rng.range(-3.0, 3.0) / 100.0);
                let period = ((SR / hz) - 0.5).round().max(2.0) as usize;
                let burst: Vec<f64> = (0..period).map(|_| rng.bipolar() * vel).collect();
                let mut start = at;
                while start < total {
                    let mut buf = burst.clone();
                    let mut k = 0;
                    for o in out[start..(start + ring).min(total)].iter_mut() {
                        let (a, b) = (buf[k], buf[(k + 1) % period]);
                        buf[k] = 0.5 * (a + b) * 0.996;
                        *o += a;
                        k = (k + 1) % period;
                    }
                    start += len;
                }
            }
            // A dotted-eighth echo, darkened on each repeat.
            let d = beat * 3 / 4;
            let mut damp = crate::filter::OnePole::new(2_400.0, SR);
            for i in d..total {
                let back = damp.process(out[i - d]);
                out[i] += 0.42 * back;
            }
        }
        LoopTimbre::Ebb => {
            // A soft FM tone per note whose envelope is a decay played
            // backwards: it swells in, brightening, and is cut on the beat.
            for &(n, end, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n] + rng.range(-4.0, 4.0) / 100.0);
                let length = ((1.5 + 1.5 * vel) * beat as f64) as usize;
                let tau = length as f64 / 4.0;
                let tail = (0.015 * SR) as usize;
                let mut stop = end;
                while stop < total + length {
                    let begin = stop.saturating_sub(length);
                    let mut v = crate::fm::Fm2::new(hz, 2.0);
                    for t in begin..(stop + tail).min(total) {
                        let env = if t < stop {
                            exp(-((stop - t) as f64) / tau)
                        } else {
                            0.5 + 0.5 * cos_turns(0.5 * (t - stop) as f64 / tail as f64)
                        };
                        out[t] += vel * env * v.next(SR, 0.2 + 1.6 * env);
                    }
                    stop += len;
                }
            }
        }
        LoopTimbre::Bronze => {
            // Inharmonic partials of a struck plate, an octave down; each is
            // paired with a twin a whole number of beats-per-loop away, so
            // the pair beats slowly and still repeats with the loop.
            const PARTIALS: [(f64, f64); 6] = [(1.0, 1.0), (1.48, 0.7), (2.09, 0.55), (2.57, 0.4), (3.14, 0.3), (4.21, 0.18)];
            let loop_hz = SR / len as f64;
            for note in &perf.notes {
                let base = midi_hz(note - 12.0);
                for (k, &(ratio, amp)) in PARTIALS.iter().enumerate() {
                    let beat_hz = loop_hz * rng.int(1, 4) as f64;
                    let swell_at = rng.unit();
                    let rate = 1.0 + (k % 2) as f64;
                    for twin in [0.0, beat_hz] {
                        // A rotating phasor: one complex multiply per sample
                        // instead of a sine (only + - *, so still exact).
                        let dt = (base * ratio + twin) / SR;
                        let start = rng.unit();
                        let (mut re, mut im) = (cos_turns(start), sin_turns(start));
                        let (c, sn) = (cos_turns(dt), sin_turns(dt));
                        let mut swell = 0.0;
                        for (i, o) in out.iter_mut().enumerate() {
                            if i % CONTROL == 0 {
                                swell = 0.25 + 0.75 * (0.5 - 0.5 * cos_turns(turns(i) * rate + swell_at));
                            }
                            *o += amp * swell * im;
                            (re, im) = (re * c - im * sn, re * sn + im * c);
                        }
                    }
                }
            }
        }
        LoopTimbre::Frost => {
            // Short windowed sine grains, two and three octaves above the
            // chord, scattered differently in each channel; the same grains
            // in every pass. A faint high air sits under them.
            let grains = (len as f64 / SR * 16.0) as usize;
            for _ in 0..grains {
                let note = perf.notes[rng.below(perf.notes.len())] + if rng.chance(0.6) { 24.0 } else { 36.0 };
                let dt = midi_hz(note.min(108.0)) / SR;
                let dur = (rng.range(0.02, 0.07) * SR) as usize;
                let (amp, phase) = (rng.range(0.3, 1.0), rng.unit());
                let mut start = rng.below(len);
                if perf.grid.0 > 0 {
                    // Garage: onto the swung sixteenth grid.
                    let (six, push) = perf.grid;
                    let k = start / six;
                    start = (k * six + if k % 2 == 1 { push } else { 0 }) % len;
                }
                while start < total {
                    let mut ph = phase;
                    for (k, o) in out[start..(start + dur).min(total)].iter_mut().enumerate() {
                        let w = 0.5 - 0.5 * cos_turns(k as f64 / dur as f64);
                        *o += amp * w * sin_turns(ph);
                        advance(&mut ph, dt);
                    }
                    start += len;
                }
            }
            let mut noise = Rng::new(rng.next_u64());
            let mut air = Svf::new(7_000.0, 0.8, SR);
            for o in out.iter_mut() {
                *o += 0.05 * air.process(noise.bipolar()).band;
            }
        }
        LoopTimbre::Lantern => {
            // A flute-organ: a sine with a little second and third harmonic,
            // breath tuned to the note, a whole-number tremolo, and a chiff
            // (a short bright puff) each time the note is re-tongued.
            let mut noise = Rng::new(rng.next_u64());
            let trem = (len as f64 / SR * 4.8).round().max(1.0);
            for (n, note) in perf.notes.iter().enumerate() {
                let hz = midi_hz(note + rng.range(-3.0, 3.0) / 100.0);
                let dt = hz / SR;
                let mut ph = rng.unit();
                let trem_at = rng.unit();
                let mut breath = Svf::new(2.0 * hz, 6.0, SR);
                let mut chiff = Svf::new(4.0 * hz, 3.0, SR);
                let tongues: Vec<usize> = perf.strikes.iter().filter(|s| s.0 == n).map(|s| s.1).collect();
                for (i, o) in out.iter_mut().enumerate() {
                    let pos = i % len;
                    // Samples since the most recent tonguing, wrapping round the loop.
                    let since = tongues.iter().map(|&at| (pos + len - at) % len).min().unwrap_or(len) as f64;
                    let attack = 1.0 - exp(-since / (0.09 * SR));
                    let puff = exp(-since / (0.03 * SR));
                    let x = noise.bipolar();
                    let tone = sin_turns(ph) + 0.28 * sin_turns(2.0 * ph) + 0.1 * sin_turns(3.0 * ph);
                    let t = 0.82 + 0.18 * sin_turns(turns(i) * trem + trem_at);
                    *o += t * (0.35 + 0.65 * attack) * (tone + 0.9 * breath.process(x).band) + 2.5 * puff * chiff.process(x).band;
                    advance(&mut ph, dt);
                }
            }
        }
        LoopTimbre::Loom => {
            // Square-wave steps, each with its own quick envelope and a
            // resonant low-pass that snaps open with the step and closes.
            let n = perf.notes.len().max(1);
            let step = beat / 2;
            let mut lp = Svf::default();
            let mut voice: Vec<(usize, f64, f64)> = vec![(0, 0.0, 0.0); len];
            for &(idx, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[idx % n] + 12.0 * (idx / n) as f64);
                for (k, v) in voice[at..(at + step).min(len)].iter_mut().enumerate() {
                    *v = (k, hz, vel);
                }
            }
            let mut ph = rng.unit();
            for (i, o) in out.iter_mut().enumerate() {
                let (k, hz, vel) = voice[i % len];
                if hz == 0.0 {
                    lp.process(0.0);
                    continue;
                }
                let since = k as f64;
                let amp = vel * (1.0 - exp(-since / (0.003 * SR))) * exp(-since / (0.22 * step as f64 + 1.0));
                if i % CONTROL == 0 {
                    lp.set(380.0 + 3_000.0 * vel * exp(-since / (0.12 * step as f64 + 1.0)), 3.5, SR);
                }
                let dt = hz / SR;
                *o = lp.process(amp * pulse(ph, 0.5, dt)).low;
                advance(&mut ph, dt);
            }
            let d = beat * 3 / 4;
            for i in d..total {
                let back = out[i - d];
                out[i] += 0.3 * back;
            }
        }
        LoopTimbre::Off => {}
    }
    out
}

/// Turns a warm-up + loop + crossfade render into a seamless loop of `len`:
/// the last `XFADE` samples' continuation is faded (equal power) into the start.
fn close_loop(s: &[f64], len: usize) -> Vec<f64> {
    (0..len)
        .map(|i| {
            if i < XFADE {
                let p = 0.25 * i as f64 / XFADE as f64;
                s[len + i] * sin_turns(p) + s[2 * len + i] * cos_turns(p)
            } else {
                s[len + i]
            }
        })
        .collect()
}

/// Each event layer is scaled to this RMS like the sustained ones, but the
/// scale is capped so a nearly-empty grid can't blow one click up to full level.
const EVENT_MAX_SCALE: f64 = 6.0;

/// One atmosphere loop layer: a loop per chord, crossfaded as the chords change.
pub struct AtmosLoop {
    design: LoopDesign,
    beats: u64,
    len: usize,
    /// Per chord: (left, right), `len` samples each.
    loops: Vec<(Vec<f32>, Vec<f32>)>,
    performances: Vec<Performance>,
    /// Gains of (chord, next chord) and which chords, refreshed every `CONTROL` samples.
    mix: (usize, usize, f64, f64),
}

impl AtmosLoop {
    /// `stream` keeps each layer's randomness separate; `target_s` is its
    /// rough length; `avoid` is the other loop's beat count, if any.
    pub fn new(
        design: LoopDesign,
        seed: u64,
        stream: u64,
        harmony: &Harmony,
        beat: u64,
        target_s: u64,
        avoid: Option<u64>,
        transpose: i8,
    ) -> Self {
        let beats = loop_beats(beat, target_s, avoid);
        let len = (beats * beat) as usize;
        let mut rng = Rng::stream(seed, stream);
        let mut loops = Vec::new();
        let mut performances = Vec::new();
        if design.is_on() {
            for chord in &harmony.chords {
                let mut perf = match design {
                    LoopDesign::Sustained(t) => perform(t, &mut rng, &chord.notes, beats, beat),
                    LoopDesign::Events(_) => perform_events(&mut rng, &chord.notes),
                };
                perf.harmonic = harmony.garage;
                perf.grid = (harmony.sixteenth as usize, harmony.swing_push as usize);
                if transpose != 0 {
                    for n in &mut perf.notes {
                        *n += transpose as f64;
                    }
                }
                let total = 2 * len + XFADE;
                let (l, r) = match design {
                    LoopDesign::Sustained(t) => (
                        close_loop(&render_channel(t, &perf, &mut rng, len, beat as usize, total), len),
                        close_loop(&render_channel(t, &perf, &mut rng, len, beat as usize, total), len),
                    ),
                    LoopDesign::Events(g) => (
                        close_loop(&crate::texture::render_channel(g, &perf.notes, &mut rng, len, total), len),
                        close_loop(&crate::texture::render_channel(g, &perf.notes, &mut rng, len, total), len),
                    ),
                };
                let power: f64 = l.iter().chain(&r).map(|x| x * x).sum::<f64>() / (2 * len) as f64;
                let mut scale = if power > 0.0 { LOOP_RMS / power.sqrt() } else { 0.0 };
                if matches!(design, LoopDesign::Events(_)) {
                    scale = scale.min(EVENT_MAX_SCALE);
                }
                loops.push((l.iter().map(|x| (x * scale) as f32).collect(), r.iter().map(|x| (x * scale) as f32).collect()));
                performances.push(perf);
            }
        }
        AtmosLoop { design, beats, len, loops, performances, mix: (0, 0, 1.0, 0.0) }
    }

    pub fn is_on(&self) -> bool {
        !self.loops.is_empty()
    }

    /// Next stereo sample at `clock`.
    pub fn next(&mut self, clock: u64, harmony: &Harmony) -> (f64, f64) {
        if self.loops.is_empty() {
            return (0.0, 0.0);
        }
        if clock % CONTROL as u64 == 0 {
            let (a, b, morph) = harmony.loop_mix(clock);
            let x = 0.25 * morph;
            self.mix = (a, b, cos_turns(x), sin_turns(x));
        }
        let i = (clock % self.len as u64) as usize;
        let (a, b, ga, gb) = self.mix;
        let (la, ra) = (&self.loops[a].0, &self.loops[a].1);
        let mut l = la[i] as f64 * ga;
        let mut r = ra[i] as f64 * ga;
        if gb > 0.0 {
            l += self.loops[b].0[i] as f64 * gb;
            r += self.loops[b].1[i] as f64 * gb;
        }
        (l, r)
    }
}

/// Read-only views for visualisation.
impl AtmosLoop {
    pub fn design(&self) -> LoopDesign {
        self.design
    }

    /// (beats, length in samples).
    pub fn length(&self) -> (u64, usize) {
        (self.beats, self.len)
    }

    pub fn performances(&self) -> &[Performance] {
        &self.performances
    }

    /// Chord `chord`'s loop (left), as `points` peak values.
    pub fn overview(&self, chord: usize, points: usize) -> Vec<f32> {
        self.loops.get(chord).map_or_else(Vec::new, |(l, _)| {
            l.chunks(self.len.div_ceil(points).max(1)).map(|c| c.iter().fold(0.0f32, |a, x| a.max(x.abs()))).collect()
        })
    }
}



}
// ---- src/audio/bass.rs ----
pub mod bass {
//! FM bass plus a pure sine sub-bass, through a low-pass.

use crate::filter::Svf;
use crate::fm::Fm2;
use crate::math::{midi_hz, sin_turns};
use crate::modulate::ModState;
use crate::rng::Rng;

/// D2: the default root (it follows each chord's root via `set_root`).
pub const ROOT: f64 = 38.0;
const RATIOS: [f64; 4] = [0.5, 1.0, 2.0, 3.0];
const FM_LEVEL: f64 = 0.22;
const SUB_LEVEL: f64 = 0.26;

pub struct Bass {
    fm: Fm2,
    sub_phase: f64,
    sub_freq: f64,
    index_lo: f64,
    index_hi: f64,
    lp: Svf,
    /// Latest modulation index and low-pass cutoff (for display).
    index: f64,
    cutoff: f64,
    /// FM detune against the sub, in semitones.
    beat: f64,
    root: f64,
}

impl Bass {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, 0xBA55);
        let sr = crate::SAMPLE_RATE as f64;
        // A hair of detune between the FM bass and the sub makes them beat slowly.
        let beat = rng.range(-1.5, 1.5) / 100.0;
        Bass {
            fm: Fm2::new(midi_hz(ROOT + beat), rng.pick(&RATIOS)),
            sub_phase: 0.0,
            sub_freq: midi_hz(ROOT - 12.0),
            index_lo: rng.range(0.1, 0.5),
            index_hi: rng.range(0.9, 2.4),
            lp: Svf::new(300.0, 0.7, sr),
            index: 0.0,
            cutoff: 300.0,
            beat,
            root: ROOT,
        }
    }

    /// Moves the bass to `midi` (called with a gliding value during chord changes).
    pub fn set_root(&mut self, midi: f64) {
        if midi != self.root {
            self.root = midi;
            self.fm.freq = midi_hz(midi + self.beat);
            self.sub_freq = midi_hz(midi - 12.0);
        }
    }

    pub fn root(&self) -> f64 {
        self.root
    }

    /// (FM ratio, modulation index, low-pass cutoff Hz).
    pub fn state(&self) -> (f64, f64, f64) {
        (self.fm.ratio, self.index, self.cutoff)
    }

    pub fn next(&mut self, clock: u64, m: &ModState) -> f64 {
        let sr = crate::SAMPLE_RATE as f64;
        if clock % 32 == 0 {
            self.cutoff = 140.0 + 360.0 * m.bass_bright;
            self.lp.set(self.cutoff, 0.9, sr);
        }
        let index = self.index_lo + (self.index_hi - self.index_lo) * m.bass_bright;
        self.index = index;
        let fm = self.fm.next(sr, index);
        let sub = sin_turns(self.sub_phase);
        self.sub_phase += self.sub_freq / sr;
        self.sub_phase -= self.sub_phase.floor();
        let filtered = self.lp.process(fm * FM_LEVEL).low;
        (filtered + sub * SUB_LEVEL) * m.bass_swell
    }
}

/// One note of the 2-step bassline, within its bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// The sixteenth it starts on (0..16).
    pub step: u8,
    /// How many sixteenths it holds for.
    pub len: u8,
    /// Semitones above the chord's root (an approach note: above the *next* root).
    pub semis: f64,
    pub vel: f64,
    /// Leading into the next chord rather than sitting on this one.
    pub approach: bool,
}

/// Where a garage bass note may start, and how likely it is: the pushes
/// around the kick rather than on it. Index = sixteenth in the bar.
const ONSETS: [f64; 16] = [0.55, 0.0, 0.3, 0.6, 0.0, 0.1, 0.55, 0.4, 0.2, 0.0, 0.5, 0.45, 0.0, 0.45, 0.55, 0.3];

/// The 2-step garage bassline: a plucked sub with a little FM bite, moving
/// in syncopated sixteenths on the chord roots, octaves and fifths, swung
/// with the drums, gliding between notes and leading into each change with
/// an approach note. Every bar's notes are a pure function of seed 4 and
/// the bar number: a two-bar figure, varied at the end of each phrase, that
/// grows a new shape every sixteen bars.
pub struct BassLine {
    seed: u64,
    six: u64,
    bar: u64,
    push: u64,
    fm: Fm2,
    lp: Svf,
    sub_phase: f64,
    freq: f64,
    target: f64,
    env: f64,
    fenv: f64,
    vel: f64,
    /// When the sounding note lets go (absolute samples).
    release_at: u64,
    plan: Option<(u64, Vec<Note>)>,
    cutoff: f64,
    midi: f64,
}

impl BassLine {
    pub fn new(seed: u64, tempo: crate::tempo::Tempo, swing: f64) -> Self {
        let mut rng = Rng::stream(seed, 0xBA56);
        let six = tempo.drum16;
        let sr = crate::SAMPLE_RATE as f64;
        BassLine {
            seed,
            six,
            bar: 16 * six,
            push: ((2.0 * swing - 1.0) * six as f64) as u64,
            fm: Fm2::new(midi_hz(ROOT), rng.pick(&[1.0, 2.0])),
            lp: Svf::new(400.0, 0.8, sr),
            sub_phase: 0.0,
            freq: midi_hz(ROOT),
            target: midi_hz(ROOT),
            env: 0.0,
            fenv: 0.0,
            vel: 0.0,
            release_at: 0,
            plan: None,
            cutoff: 400.0,
            midi: ROOT,
        }
    }

    /// The notes of bar `bar` (pure). `changing` marks a bar that leads into
    /// a new chord: its last note becomes an approach.
    pub fn plan(&self, bar: u64, changing: bool) -> Vec<Note> {
        let figure = bar % 2;
        let vary = bar % 4 == 3;
        let shape = bar / 16;
        let mut rng = Rng::stream(self.seed, 0xBA56_0000 ^ (shape << 8) ^ (figure << 4) ^ vary as u64);
        let mut steps: Vec<u8> = Vec::new();
        for (i, w) in ONSETS.iter().enumerate() {
            // Never two in a row; the variation bar is a little busier.
            let w = if vary { (w * 1.25).min(0.9) } else { *w };
            if steps.last().is_some_and(|l| *l as usize + 1 == i) {
                continue;
            }
            if rng.chance(w) {
                steps.push(i as u8);
            }
        }
        if steps.len() < 2 {
            steps = vec![0, rng.pick(&[6u8, 10, 14])];
        }
        let mut notes = Vec::with_capacity(steps.len());
        for (k, &step) in steps.iter().enumerate() {
            let next = steps.get(k + 1).copied().unwrap_or(16);
            let room = next - step;
            // Mostly held to the next note, now and then a short stab.
            let len = if rng.chance(0.3) { 1 } else { room.min(4) };
            let semis = if k == 0 {
                0.0
            } else {
                match rng.weighted(&[0.5, 0.28, 0.17, 0.05]) {
                    0 => 0.0,
                    1 => 12.0,
                    2 => 7.0,
                    _ => -5.0,
                }
            };
            let vel = if semis == 12.0 { rng.range(0.6, 0.75) } else { rng.range(0.8, 1.0) };
            notes.push(Note { step, len, semis, vel, approach: false });
        }
        if changing {
            // Walk into the next chord: a semitone under its root on the
            // last push of the bar.
            let last = notes.last().map_or(0, |n| n.step);
            if last < 14 {
                notes.push(Note { step: 14, len: 2, semis: -1.0, vel: 0.8, approach: true });
            } else if let Some(n) = notes.last_mut() {
                *n = Note { semis: -1.0, approach: true, ..*n };
            }
        }
        notes
    }

    /// Where a step's note begins within the bar (odd steps swing late).
    fn onset(&self, step: u8) -> u64 {
        step as u64 * self.six + if step % 2 == 1 { self.push } else { 0 }
    }

    /// The current note's pitch (MIDI), for display.
    pub fn pitch(&self) -> f64 {
        self.midi
    }

    /// One sample. `root` and `next_root` are this chord's and the next one's
    /// bass notes; `changing` is true in the bar before a change; `active`
    /// whether the groove is playing (the bass follows the drums' sections).
    pub fn next(&mut self, clock: u64, root: f64, next_root: f64, fifth: f64, changing: bool, active: bool, m: &ModState) -> f64 {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar, changing)));
        }
        if active {
            let notes = &self.plan.as_ref().unwrap().1;
            if let Some(n) = notes.iter().find(|n| self.onset(n.step) == in_bar) {
                let base = if n.approach { next_root } else { root };
                // The figure's fifths are this chord's own fifth.
                let semis = match n.semis {
                    s if s == 7.0 => fifth,
                    s if s == -5.0 => fifth - 12.0,
                    s => s,
                };
                // Keep the line in the sub register.
                let mut midi = base + semis;
                while midi > 50.0 {
                    midi -= 12.0;
                }
                self.target = midi_hz(midi);
                self.midi = midi;
                self.vel = n.vel;
                self.fenv = 1.0;
                let hold = (n.len as f64 - 0.25).max(0.6) * self.six as f64;
                self.release_at = clock + hold as u64;
            }
        }
        // A quick glide (about 7 ms), so even a pushed note is at pitch on
        // its beat; then the amplitude and filter envelopes.
        self.freq += (self.target - self.freq) * 0.003;
        let gate = active && clock < self.release_at;
        let goal = if gate { 1.0 } else { 0.0 };
        let rate = if gate { 0.02 } else { 0.0009 };
        self.env += (goal - self.env) * rate;
        self.fenv *= 0.99975;
        if clock % 32 == 0 {
            self.cutoff = 150.0 + 330.0 * m.bass_bright + 1300.0 * self.fenv;
            self.lp.set(self.cutoff, 0.85, sr);
        }
        self.fm.freq = self.freq;
        let fm = self.fm.next(sr, 0.3 + 1.8 * self.fenv);
        let sub = sin_turns(self.sub_phase);
        self.sub_phase += self.freq / sr;
        self.sub_phase -= self.sub_phase.floor();
        let body = self.lp.process(fm * 0.3).low;
        (sub * 0.3 + body * 0.85) * self.env * self.vel
    }
}

/// Microsound's sub pulse: a pure sine on the chord root, struck short on
/// the off-beat eighths between the sub kicks (and now and then on the
/// sixteenth before a beat), so kick and pulse rock against each other
/// without ever overlapping. No FM and no filter: just the sine, an attack
/// of a millisecond and a quick release. Every bar is a pure function of
/// seed 4 and the bar number.
pub struct SubPulse {
    seed: u64,
    six: u64,
    bar: u64,
    phase: f64,
    freq: f64,
    env: f64,
    vel: f64,
    release_at: u64,
    plan: Option<(u64, [f64; 16])>,
    midi: f64,
}

impl SubPulse {
    pub fn new(seed: u64, tempo: crate::tempo::Tempo) -> Self {
        let six = tempo.drum16;
        SubPulse { seed, six, bar: 16 * six, phase: 0.0, freq: midi_hz(ROOT), env: 0.0, vel: 0.0, release_at: 0, plan: None, midi: ROOT }
    }

    /// The velocity on each sixteenth of bar `bar` (0 = silent). Pure.
    pub fn plan(&self, bar: u64) -> [f64; 16] {
        let mut rng = Rng::stream(self.seed, 0x5B00_0000 ^ (bar / 8) << 4 ^ bar % 2);
        let mut v = [0.0; 16];
        for (i, x) in v.iter_mut().enumerate() {
            let w = match i % 4 {
                2 => 0.85,
                3 => 0.12,
                _ => 0.0,
            };
            if rng.chance(w) {
                *x = rng.range(0.6, 1.0);
            }
        }
        v
    }

    /// The current note (MIDI), for display.
    pub fn pitch(&self) -> f64 {
        self.midi
    }

    pub fn next(&mut self, clock: u64, root: f64, active: bool) -> f64 {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar)));
        }
        if active && in_bar % self.six == 0 {
            let vel = self.plan.as_ref().unwrap().1[(in_bar / self.six) as usize];
            if vel > 0.0 {
                let mut midi = root;
                while midi > 45.0 {
                    midi -= 12.0;
                }
                self.midi = midi;
                self.freq = midi_hz(midi);
                self.vel = vel;
                self.release_at = clock + self.six * 3 / 4;
            }
        }
        let gate = active && clock < self.release_at;
        self.env += if gate { (1.0 - self.env) * 0.02 } else { -self.env * 0.004 };
        let s = sin_turns(self.phase);
        self.phase += self.freq / sr;
        self.phase -= self.phase.floor();
        s * self.env * self.vel * 0.42
    }
}

}
// ---- src/audio/creatures.rs ----
pub mod creatures {
//! The voices of the wildlife: every call synthesised from sines, a little
//! breath noise and simple envelopes — no samples. Which creature calls, and
//! when, comes from the shared schedule in [`crate::wildlife`].

use crate::filter::{OnePole, Svf};
use crate::math::{cos_turns, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::wildlife::{Call, Creature};
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// The most calls that can overlap; the oldest give way.
const MAX_VOICES: usize = 12;
/// Wildlife sits well under the weather.
const GAIN: f64 = 0.08;

/// One note of a call.
#[derive(Clone, Copy)]
struct Note {
    start: usize,
    len: usize,
    /// Pitch glides from `f0` to `f1` across the note.
    f0: f64,
    f1: f64,
    amp: f64,
    /// Amplitude trembling: rate (Hz) and depth.
    trem: (f64, f64),
    /// How much of the note is breath rather than tone.
    breath: f64,
    /// A pulse train gating the note (a croak), in Hz; 0 for none.
    pulse: f64,
}

#[derive(Clone)]
struct Voice {
    wait: usize,
    t: usize,
    notes: Vec<Note>,
    phases: Vec<f64>,
    rng: Rng,
    breath: Svf,
    tone_lp: OnePole,
    gain: (f64, f64),
}

impl Voice {
    fn end(&self) -> usize {
        self.notes.iter().map(|n| n.start + n.len).max().unwrap_or(0)
    }

    fn next(&mut self) -> (f64, f64) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0);
        }
        let t = self.t;
        self.t += 1;
        let noise = self.rng.bipolar();
        let mut y = 0.0;
        for (n, ph) in self.notes.iter().zip(self.phases.iter_mut()) {
            if t < n.start || t >= n.start + n.len {
                continue;
            }
            let p = (t - n.start) as f64 / n.len as f64;
            let env = sin_turns(0.5 * p);
            let hz = n.f0 + (n.f1 - n.f0) * p;
            *ph += hz / SR;
            *ph -= ph.floor();
            let s = t as f64 / SR;
            let trem = 1.0 - n.trem.1 * (0.5 - 0.5 * cos_turns(s * n.trem.0));
            let gate = if n.pulse > 0.0 {
                let g = 0.5 + 0.5 * cos_turns(s * n.pulse);
                g * g * g
            } else {
                1.0
            };
            let tone = sin_turns(*ph) + 0.25 * sin_turns(2.0 * *ph) * n.breath;
            y += n.amp * env * trem * gate * (tone * (1.0 - n.breath) + n.breath * 3.0 * noise);
        }
        let y = self.tone_lp.process(self.breath.process(y).low);
        (y * self.gain.0, y * self.gain.1)
    }
}

/// Every call currently sounding.
#[derive(Clone, Default)]
pub struct Creatures {
    voices: Vec<Voice>,
}

impl Creatures {
    /// Start `call`, `wait` samples from now.
    pub fn trigger(&mut self, call: &Call, wait: usize) {
        let notes = notes(call);
        if notes.is_empty() {
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        let near = call.nearness();
        let (l, r) = pan_gains(call.pan());
        let top = notes.iter().map(|n| n.f0.max(n.f1)).fold(0.0, f64::max);
        self.voices.push(Voice {
            wait,
            t: 0,
            phases: vec![0.0; notes.len()],
            notes,
            rng: Rng::new(call.hash ^ 0x4252_4541),
            // Softens the breath and the edges; farther calls are duller.
            breath: Svf::new((top * 2.5).min(16_000.0), 0.7, SR),
            tone_lp: OnePole::new(2_500.0 + 9_000.0 * near, SR),
            gain: (GAIN * near * l, GAIN * near * r),
        });
    }

    pub fn next(&mut self) -> (f64, f64) {
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            let (a, b) = v.next();
            l += a;
            r += b;
        }
        self.voices.retain(|v| v.wait > 0 || v.t < v.end());
        (l, r)
    }
}

/// The notes of one call, from the creature and the call's own randomness.
fn notes(call: &Call) -> Vec<Note> {
    let mut r = call.rng();
    let pitch = call.resident.pitch;
    let s = |seconds: f64| (seconds * SR) as usize;
    let note = |start, len, f0, f1, amp| Note { start, len, f0, f1, amp, trem: (0.0, 0.0), breath: 0.0, pulse: 0.0 };
    let mut out = Vec::new();
    let mut at = 0;
    match call.resident.creature {
        Creature::Songbird => {
            // A phrase of quick chirps, each sweeping up or down.
            for _ in 0..r.int(3, 6) {
                let len = s(r.range(0.04, 0.12));
                let f0 = r.range(2_600.0, 4_800.0) * pitch;
                out.push(note(at, len, f0, f0 * r.range(0.6, 1.6), r.range(0.7, 1.0)));
                at += len + s(r.range(0.02, 0.08));
            }
        }
        Creature::Dove => {
            // "hoo-HOO-hoo": soft, a little breathy, the middle note higher.
            let base = 420.0 * pitch;
            for (k, rel) in [1.0, 1.12, 0.95].into_iter().enumerate().take(r.int(2, 3) as usize) {
                let len = s(if k == 1 { 0.42 } else { 0.3 });
                out.push(Note { trem: (6.0, 0.15), breath: 0.15, ..note(at, len, base * rel, base * rel * 0.97, 0.9) });
                at += len + s(0.12);
            }
        }
        Creature::Frog => {
            // One to three croaks: a buzzing pulse train on a low tone.
            for _ in 0..r.int(1, 3) {
                let len = s(r.range(0.25, 0.5));
                let f0 = r.range(440.0, 620.0) * pitch;
                out.push(Note { breath: 0.35, pulse: r.range(22.0, 34.0), ..note(at, len, f0, f0 * 0.92, 1.0) });
                at += len + s(r.range(0.12, 0.25));
            }
        }
        Creature::Cricket => {
            // Three or four quick, high pulses.
            let f = r.range(4_300.0, 5_000.0) * pitch;
            for _ in 0..r.int(3, 4) {
                out.push(note(at, s(0.018), f, f, 0.8));
                at += s(0.04);
            }
        }
        Creature::Cicada => {
            // A long trill, trembling fast, swelling and dying away.
            let f = r.range(5_800.0, 7_000.0) * pitch;
            out.push(Note { trem: (r.range(40.0, 56.0), 0.9), ..note(0, s(r.range(1.0, 2.2)), f, f * 0.98, 0.6) });
        }
        Creature::Owl => {
            // Two or three low hoots, the last one dipping.
            let f = r.range(300.0, 380.0) * pitch;
            let n = r.int(2, 3);
            for k in 0..n {
                let dip = if k + 1 == n { 0.86 } else { 0.95 };
                out.push(Note { breath: 0.25, ..note(at, s(0.45), f, f * dip, 1.0) });
                at += s(0.7);
            }
        }
        Creature::Firefly => {}
    }
    out
}

}
// ---- src/audio/delay.rs ----
pub mod delay {
//! Stereo ping-pong echo with a darkening feedback loop.

use crate::filter::OnePole;
use crate::math::{sanitize, sin_turns, tanh};

pub struct PingPong {
    left: Vec<f64>,
    right: Vec<f64>,
    pos: usize,
    delay: usize,
    feedback: f64,
    damp_l: OnePole,
    damp_r: OnePole,
}

impl PingPong {
    pub fn new(delay: usize, feedback: f64) -> Self {
        let len = delay + 1;
        PingPong {
            left: vec![0.0; len],
            right: vec![0.0; len],
            pos: 0,
            delay,
            feedback,
            damp_l: OnePole::new(3200.0, crate::SAMPLE_RATE as f64),
            damp_r: OnePole::new(3200.0, crate::SAMPLE_RATE as f64),
        }
    }

    /// (delay in samples, feedback).
    pub fn settings(&self) -> (usize, f64) {
        (self.delay, self.feedback)
    }

    /// Input enters on the left and bounces right, left, right...
    /// Empty the echo (its repeats stop).
    pub fn clear(&mut self) {
        self.left.iter_mut().for_each(|x| *x = 0.0);
        self.right.iter_mut().for_each(|x| *x = 0.0);
    }

    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let len = self.left.len();
        let read = (self.pos + len - self.delay) % len;
        let out_l = self.damp_l.process(self.left[read]);
        let out_r = self.damp_r.process(self.right[read]);
        self.left[self.pos] = sanitize(0.5 * (in_l + in_r) + self.feedback * out_r);
        self.right[self.pos] = sanitize(self.feedback * out_l);
        self.pos = (self.pos + 1) % len;
        (out_l, out_r)
    }
}

/// Stereo width of the tape echo's ping-pong (1 = hard left/right, 0 = mono).
const TAPE_WIDTH: f64 = 0.5;

/// A ping-pong tape echo for the drums' snare-type hits: the delay time
/// wobbles (wow and flutter), and every repeat is saturated, darkened and
/// thinned in the feedback loop, like a tape loop.
pub struct TapeEcho {
    left: Vec<f64>,
    right: Vec<f64>,
    pos: usize,
    delay: f64,
    feedback: f64,
    lp: [OnePole; 2],
    hp: [OnePole; 2],
}

/// Wow (~0.5 Hz) and flutter (~6 Hz) periods, in samples, and their depths.
const WOW_PERIOD: u64 = 96_000;
const FLUTTER_PERIOD: u64 = 8_000;
const WOW_DEPTH: f64 = 0.003;
const FLUTTER_DEPTH: f64 = 0.0005;

impl TapeEcho {
    pub fn new(delay: usize, feedback: f64) -> Self {
        let len = (delay as f64 * 1.02) as usize + 8;
        let sr = crate::SAMPLE_RATE as f64;
        TapeEcho {
            left: vec![0.0; len],
            right: vec![0.0; len],
            pos: 0,
            delay: delay as f64,
            feedback,
            lp: [OnePole::new(2500.0, sr), OnePole::new(2500.0, sr)],
            hp: [OnePole::new(150.0, sr), OnePole::new(150.0, sr)],
        }
    }

    /// (delay in samples, feedback)
    pub fn settings(&self) -> (f64, f64) {
        (self.delay, self.feedback)
    }

    fn read(buf: &[f64], pos: usize, d: f64) -> f64 {
        // `d` is always shorter than the buffer, so one wrap is enough.
        let mut p = pos as f64 - d;
        if p < 0.0 {
            p += buf.len() as f64;
        }
        let i = p.floor();
        let f = p - i;
        let i = i as usize;
        let a = buf[i % buf.len()];
        let b = buf[(i + 1) % buf.len()];
        a + (b - a) * f
    }

    /// Mono input enters on the left, then bounces right, left, right…
    /// `clock` drives the wow and flutter, so output never depends on block size.
    pub fn process(&mut self, clock: u64, input: f64) -> (f64, f64) {
        let wow = sin_turns((clock % WOW_PERIOD) as f64 / WOW_PERIOD as f64);
        let flutter = sin_turns((clock % FLUTTER_PERIOD) as f64 / FLUTTER_PERIOD as f64);
        let d = self.delay * (1.0 + WOW_DEPTH * wow + FLUTTER_DEPTH * flutter);
        let out_l = Self::read(&self.left, self.pos, d);
        let out_r = Self::read(&self.right, self.pos, d);
        // Tape colour in the loop: soft saturation, darker, thinner.
        let shape = |x: f64, lp: &mut OnePole, hp: &mut OnePole| {
            let x = lp.process(tanh(x * 1.3) / 1.3);
            x - hp.process(x)
        };
        let fb_l = shape(out_l * self.feedback, &mut self.lp[0], &mut self.hp[0]);
        let fb_r = shape(out_r * self.feedback, &mut self.lp[1], &mut self.hp[1]);
        self.left[self.pos] = sanitize(input + fb_r);
        self.right[self.pos] = sanitize(fb_l);
        self.pos = (self.pos + 1) % self.left.len();
        // Narrow the bounces to about halfway out, so they don't jump hard left/right.
        let (a, b) = ((1.0 + TAPE_WIDTH) * 0.5, (1.0 - TAPE_WIDTH) * 0.5);
        (out_l * a + out_r * b, out_r * a + out_l * b)
    }
}

}
// ---- src/audio/drone.rs ----
pub mod drone {
//! The frozen chords: each 3-operator FM chord of the progression is rendered
//! once, analysed into an average magnitude spectrum, then resynthesised
//! forever by random-phase spectral freezing (same magnitudes, fresh random
//! phases every hop, windowed overlap-add). Chord changes are spectral
//! morphs: during a chord's last bar each new frame's power spectrum is
//! blended towards the next chord's.

use crate::fft::Fft;
use crate::harmony::Harmony;
pub use crate::fm::Algo;
use crate::fm::{Fm3, Op};
use crate::math::{cos_turns, midi_hz, sin_turns};
use crate::rng::Rng;

/// Modulator ratios to choose from. Near-integers keep the chord harmonic
/// with a slight shimmer.
const RATIOS: [f64; 7] = [0.5, 1.0, 1.0, 2.0, 2.0, 3.0, 4.003];
/// Length of the FM chord rendered for analysis.
const CHORD_SECONDS: f64 = 8.0;
/// FFT / window size. 2^15 at 48 kHz ≈ 0.68 s window: a deep, smooth smear.
pub const N: usize = 1 << 15;
/// Hop: quarter window. Hann² over 4 overlapping hops sums to a constant 1.5.
/// Must leave room for all of a frame's work units (checked in `new`).
pub const HOP: usize = N / 4;
/// Output level (RMS per channel).
const DRONE_RMS: f64 = 0.16;
/// Bins below this are left empty (sub rumble is the bass's job).
const LOW_CUT_HZ: f64 = 110.0;
/// The thin (microsound) drone leaves more room below.
const THIN_LOW_CUT_HZ: f64 = 220.0;

/// Building one frame is split into small work units, one every
/// `UNIT_SPACING` samples through the hop, so no single moment carries a
/// whole 32k-point IFFT (that would overrun a browser audio callback).
/// Scheduled on the sample position, so output never depends on block size.
const FILL_UNITS: usize = 8;
const UNIT_SPACING: usize = 256;

pub struct Drone {
    fft: Fft,
    window: Vec<f64>,
    /// Frozen (left, right) magnitude spectra, one pair per chord.
    mags: Vec<(Vec<f64>, Vec<f64>)>,
    harmony: Harmony,
    /// The frame under construction blends chord `.0` into `.1` by `.2`.
    mix: (usize, usize, f64),
    /// Samples output so far.
    pos: u64,
    rng: Rng,
    /// Work buffers for the frame under construction.
    re: Vec<f64>,
    im: Vec<f64>,
    /// Five windowed frames per channel: four sounding, one being built.
    slots_l: Vec<Vec<f64>>,
    slots_r: Vec<Vec<f64>>,
    /// Slot indices of the sounding frames, newest first.
    sounding: [usize; 4],
    building: usize,
    /// Position within the current hop.
    offset: usize,
    /// Frames that have started sounding so far.
    frames: u64,
    recipes: Vec<Vec<Recipe>>,
}

/// One chord note's FM settings: (wiring, [op2 ratio, op3 ratio, op2 index, op3 index], op3 feedback).
pub type Recipe = (Algo, [f64; 4], f64);

fn render_chord(rng: &mut Rng, voicing: &[f64], recipes: &[Recipe]) -> Vec<f64> {
    let sr = crate::SAMPLE_RATE as f64;
    let len = (CHORD_SECONDS * sr) as usize;
    let mut out = vec![0.0; len];
    for (note, (algo, [r2, r3, i2, i3], fb)) in voicing.iter().zip(recipes) {
        let detune = rng.range(-7.0, 7.0) / 100.0; // ± 7 cents per channel
        let mut voice = Fm3::new(
            midi_hz(note + detune),
            *algo,
            [
                Op::new(1.0, 0.0, rng.unit()),
                Op::new(*r2, *i2, rng.unit()),
                Op::new(*r3, *i3, rng.unit()),
            ],
            *fb,
        );
        // Higher notes quieter.
        let gain = 1.0 / (1.0 + (note - 45.0).max(0.0) / 18.0);
        // Slowly swaying modulation depth enriches the averaged spectrum.
        let sway_hz = rng.range(0.08, 0.3);
        let sway_phase = rng.unit();
        for (i, s) in out.iter_mut().enumerate() {
            let sway = 0.7 + 0.3 * sin_turns(sway_phase + sway_hz * i as f64 / sr);
            *s += gain * voice.next(sr, sway);
        }
    }
    out
}

fn recipes_for(rng: &mut Rng, notes: &[f64]) -> Vec<Recipe> {
    // One FM recipe per chord note, shared by both channels so they're the
    // same chord, but each channel gets its own detune and phases.
    notes
        .iter()
        .map(|_| {
            let algo = if rng.chance(0.5) { Algo::Stack } else { Algo::Pair };
            let r2 = rng.pick(&RATIOS);
            let r3 = rng.pick(&RATIOS);
            (algo, [r2, r3, rng.range(0.3, 2.2), rng.range(0.1, 1.2)], rng.range(0.0, 0.6))
        })
        .collect()
}

impl Drone {
    /// `first` must be the stream chord 1's voicing was drawn from (it goes
    /// on to choose chord 1's FM recipes, exactly as in 0009); `rest` serves
    /// the other chords.
    pub fn new(seed: u64, harmony: Harmony, first: Rng, rest: Rng) -> Self {
        Drone::build(seed, harmony, first, rest, false)
    }

    /// Microsound's drone: the same chords frozen from pure sines (every FM
    /// index at zero), with the low end cut higher, so it is a thin, still
    /// cluster of partials rather than a thick cloud.
    pub fn thin(seed: u64, harmony: Harmony, first: Rng, rest: Rng) -> Self {
        Drone::build(seed, harmony, first, rest, true)
    }

    fn build(seed: u64, harmony: Harmony, mut first: Rng, mut rest: Rng, thin: bool) -> Self {
        let fft = Fft::new(N);
        let window: Vec<f64> = (0..N).map(|i| 0.5 - 0.5 * cos_turns(i as f64 / N as f64)).collect();
        let low_cut = if thin { THIN_LOW_CUT_HZ } else { LOW_CUT_HZ };
        let low_bin = (low_cut * N as f64 / crate::SAMPLE_RATE as f64) as usize;
        let mut mags = Vec::new();
        let mut all_recipes = Vec::new();
        for (i, chord) in harmony.chords.iter().enumerate() {
            let rng = if i == 0 { &mut first } else { &mut rest };
            let mut recipes = recipes_for(rng, &chord.notes);
            if thin {
                for (_, [_, _, i2, i3], fb) in recipes.iter_mut() {
                    *i2 = 0.0;
                    *i3 = 0.0;
                    *fb = 0.0;
                }
            }
            let left = render_chord(rng, &chord.notes, &recipes);
            let right = render_chord(rng, &chord.notes, &recipes);
            let mut mag_l = average_magnitudes(&fft, &window, &left);
            let mut mag_r = average_magnitudes(&fft, &window, &right);
            // Scale so the resynthesis comes out at DRONE_RMS. With an unscaled
            // inverse FFT, each frame's mean power is 2·Σ|m|² (Parseval), and four
            // overlapping Hann-windowed frames of uncorrelated noise sum to 1.5×.
            for m in [&mut mag_l, &mut mag_r] {
                for x in m[..low_bin].iter_mut() {
                    *x = 0.0;
                }
                let power: f64 = m.iter().map(|x| x * x).sum();
                let scale = DRONE_RMS / (3.0 * power).sqrt();
                for x in m.iter_mut() {
                    *x *= scale;
                }
            }
            mags.push((mag_l, mag_r));
            all_recipes.push(recipes);
        }

        assert!((FILL_UNITS + 1 + 15 + 1) * UNIT_SPACING <= HOP);
        let mut drone = Drone {
            fft,
            window,
            mags,
            harmony,
            mix: (0, 0, 0.0),
            pos: 0,
            rng: Rng::stream(seed, 0xD1),
            re: vec![0.0; N],
            im: vec![0.0; N],
            slots_l: vec![vec![0.0; N]; 5],
            slots_r: vec![vec![0.0; N]; 5],
            // Frames go in slots 0, 1, 2 (sounding) and 4 (entering at the
            // first sample); slot 3 is the one recycled first.
            sounding: [2, 1, 0, 3],
            building: 4,
            offset: 0,
            frames: 0,
            recipes: all_recipes,
        };
        // Start in the steady state: three frames already sounding, a fourth
        // built and waiting to enter at the first sample.
        for slot in [0, 1, 2, 4] {
            drone.building = slot;
            for unit in 0..drone.units() {
                drone.work(unit);
            }
        }
        drone
    }

    fn units(&self) -> usize {
        FILL_UNITS + 1 + self.fft.stages() as usize + 1
    }

    /// One slice of building the next random-phase frame for both channels.
    /// Both go through one complex IFFT: Z = X_L + i·X_R, whose real part is
    /// the left signal and imaginary part the right (both spectra Hermitian).
    fn work(&mut self, unit: usize) {
        let (re, im) = (&mut self.re, &mut self.im);
        let stages = self.fft.stages() as usize;
        if unit < FILL_UNITS {
            if unit == 0 {
                // Which chord(s) this frame belongs to, judged at its centre.
                let centre = self.pos + HOP as u64 + N as u64 / 2;
                let p = self.harmony.at(centre);
                self.mix = (p.chord, p.next, p.morph);
            }
            let (a, b, x) = self.mix;
            let per = N / 2 / FILL_UNITS;
            for k in (unit * per).max(1)..(unit + 1) * per {
                let (pl, pr) = (self.rng.unit(), self.rng.unit());
                let (ml, mr) = if x == 0.0 {
                    (self.mags[a].0[k], self.mags[a].1[k])
                } else {
                    // Blend power, not amplitude, so loudness holds steady.
                    let blend = |p: f64, q: f64| ((1.0 - x) * p * p + x * q * q).sqrt();
                    (blend(self.mags[a].0[k], self.mags[b].0[k]), blend(self.mags[a].1[k], self.mags[b].1[k]))
                };
                let (lr, li) = (ml * cos_turns(pl), ml * sin_turns(pl));
                let (rr, ri) = (mr * cos_turns(pr), mr * sin_turns(pr));
                re[k] = lr - ri;
                im[k] = li + rr;
                re[N - k] = lr + ri;
                im[N - k] = rr - li;
            }
        } else if unit == FILL_UNITS {
            re[0] = 0.0;
            im[0] = 0.0;
            re[N / 2] = 0.0;
            im[N / 2] = 0.0;
            self.fft.bit_reverse(re, im);
        } else if unit <= FILL_UNITS + stages {
            self.fft.stage(re, im, (unit - FILL_UNITS - 1) as u32, true);
        } else if unit == FILL_UNITS + stages + 1 {
            let (l, r) = (&mut self.slots_l[self.building], &mut self.slots_r[self.building]);
            for i in 0..N {
                l[i] = re[i] * self.window[i];
                r[i] = im[i] * self.window[i];
            }
        }
    }

    /// Next stereo sample.
    pub fn next(&mut self) -> (f64, f64) {
        if self.offset == 0 {
            // The finished frame starts sounding; the oldest slot is recycled.
            let freed = self.sounding[3];
            self.sounding = [self.building, self.sounding[0], self.sounding[1], self.sounding[2]];
            self.building = freed;
            self.frames += 1;
        }
        if self.offset % UNIT_SPACING == 0 {
            self.work(self.offset / UNIT_SPACING);
        }
        // Overlap-add, oldest frame first.
        let (mut l, mut r) = (0.0, 0.0);
        for age in (0..4).rev() {
            let i = age * HOP + self.offset;
            l += self.slots_l[self.sounding[age]][i];
            r += self.slots_r[self.sounding[age]][i];
        }
        self.offset = (self.offset + 1) % HOP;
        self.pos += 1;
        (l, r)
    }
}

/// Read-only views for visualisation.
impl Drone {
    /// Chord `i`'s frozen magnitude spectra (left, right), bins 0..=N/2.
    pub fn magnitudes(&self, i: usize) -> (&[f64], &[f64]) {
        (&self.mags[i].0, &self.mags[i].1)
    }

    pub fn harmony(&self) -> &Harmony {
        &self.harmony
    }

    /// Chord `i`'s FM recipes, one per note.
    pub fn recipes(&self, i: usize) -> &[Recipe] {
        &self.recipes[i]
    }

    /// Frames that have started sounding so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Position within the current hop, 0..HOP.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The newest sounding frame (left), as `points` peak values.
    pub fn newest_frame_overview(&self, points: usize) -> Vec<f32> {
        let frame = &self.slots_l[self.sounding[0]];
        frame
            .chunks(N.div_ceil(points))
            .map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32)
            .collect()
    }
}

/// RMS-averaged magnitude spectrum (bins 0..=N/2) over Hann-windowed frames.
fn average_magnitudes(fft: &Fft, window: &[f64], signal: &[f64]) -> Vec<f64> {
    let mut power = vec![0.0; N / 2 + 1];
    let mut re = vec![0.0; N];
    let mut im = vec![0.0; N];
    let mut frames = 0;
    let mut start = 0;
    while start + N <= signal.len() {
        for i in 0..N {
            re[i] = signal[start + i] * window[i];
            im[i] = 0.0;
        }
        fft.transform(&mut re, &mut im, false);
        for (k, p) in power.iter_mut().enumerate() {
            *p += re[k] * re[k] + im[k] * im[k];
        }
        frames += 1;
        start += HOP;
    }
    power.iter().map(|p| (p / frames as f64).sqrt()).collect()
}

}
// ---- src/audio/drone_rhythm.rs ----
pub mod drone_rhythm {
//! A rhythmic amplitude LFO for the drone.
//!
//! Off by default, and when it is off nothing here is consulted, so the sound
//! and every golden hash are exactly as they were. When it is on, a seed of its
//! own decides the rhythm: a subdivision of the bar and an accent per pulse.
//! Manual shaping can layer on top of this later, but the seed is the starting
//! point.
//!
//! The level ducks at each pulse and recovers before the next one, which reads
//! as rhythm without pushing the mix into clipping.

use crate::math::cos_turns;
use crate::rng::Rng;

/// Stream id, so this generator never shares a sequence with another.
const STREAM: u64 = 0x4452_4859; // "DRHY"

/// How far the deepest pulse ducks, 0..1.
const DEPTH: f64 = 0.72;

pub struct DroneRhythm {
    /// Pulse length in samples.
    step: u64,
    /// Accent per pulse, 0..1.
    accents: Vec<f64>,
}

impl DroneRhythm {
    /// Build the rhythm the seed asks for. `bpm` sets the bar, so the pulses
    /// stay locked to the drums.
    pub fn new(seed: u64, bpm: u16) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        // Pulses per bar: whole, half, quarter or eighth notes.
        let steps = [4usize, 8, 8, 16][rng.below(4) as usize];
        let bar = (crate::SAMPLE_RATE as u64) * 4 * 60 / bpm.max(1) as u64;
        let step = (bar / steps as u64).max(1);
        let accents = (0..steps)
            .map(|i| {
                // Strong on the beat, weaker off it, then jittered by the seed.
                let weight = if i % 4 == 0 {
                    1.0
                } else if i % 2 == 0 {
                    0.6
                } else {
                    0.25
                };
                (weight * rng.range(0.6, 1.0)).clamp(0.0, 1.0)
            })
            .collect();
        DroneRhythm { step, accents }
    }

    /// Amplitude window at `clock`, around 1.0 with a duck at each pulse.
    ///
    /// Stateless, so it can be evaluated in any order and never drifts.
    pub fn level(&self, clock: u64) -> f64 {
        let steps = self.accents.len() as u64;
        if steps == 0 || self.step == 0 {
            return 1.0;
        }
        let accent = self.accents[((clock / self.step) % steps) as usize];
        let phase = (clock % self.step) as f64 / self.step as f64;
        // One-sided bump: full at the pulse, gone by the next one.
        let envelope = 0.5 + 0.5 * cos_turns(phase * 0.5);
        (1.0 - DEPTH * accent * envelope * envelope).clamp(0.0, 1.0)
    }
}



}
// ---- src/audio/drums.rs ----
pub mod drums {
//! Jungle drums, made the way jungle producers worked, but entirely synthesised.
//!
//! 1. A two-bar breakbeat is *played* once at start-up by synthesised drum
//!    voices (from one of the kit families in `kits.rs`) at a funk-record
//!    tempo (125-150 BPM), with human micro-timing and cymbal choke.
//! 2. It is bussed for punch (compression, soft clip), then "sampled": run
//!    through a sampler colour chain (saturation, sample-and-hold, bit depth,
//!    a lowpass, a small room), all set by the kit.
//! 3. It is chopped into 32 sixteenth-note slices and re-sequenced live at
//!    the chosen tempo (168 BPM by default). Playing a slower break at jungle
//!    tempo is the classic move: the break speeds up and its pitch rises with it.
//!
//! Every bar's slice plan is a pure function of seed 5, the bar number and
//! the slow LFOs, so playback never depends on block size or history.
//! The kit "Off" switches the drums off.
//!
//! **2-step garage** plays the same machine differently. The loop is played
//! at the track's own tempo (no pitch-up), shuffled by the swing setting,
//! from rules rather than a record: a kick on the one and a second that
//! *skips* past beat three to land just after it, claps on two and four,
//! rimshots pushing the off-beats, shuffled sixteenth hats with an open hat
//! on each off-beat eighth. Its edits lean toward the glitch: hat stutters,
//! reversed claps sucking into the snare, kicks dropping out, pitched-down
//! tails; and its sections breathe between hats, a kick-less breakdown, the
//! full 2-step, and 2-step with the glitches let loose.
//!
//! **Microsound** records its loop at the track's tempo too, dead straight
//! and with no human timing (exactness is the sound): a sub-sine kick on each
//! beat, clicks of a few samples on the beats and off-beats, ticks and
//! grains scattered over the sixteenths. It skips the sampler colour, so
//! every click stays as sharp as it was made. Its edits are digital
//! failures: a step drops out, skips back to repeat the one before, or
//! ratchets four times in a sixteenth.

use crate::filter::{OnePole, Svf};
use crate::harmony::Harmony;
use crate::kits::{DrumSpace, KitParams};
use crate::math::{sanitize, tanh};
use crate::modulate::Mods;
use crate::rng::Rng;
use crate::tempo::Tempo;

pub const STEPS: usize = 16;
pub const PHRASE_BARS: u64 = 4;
pub const SLICES: usize = 32;
/// Short fades where the slice sequence is discontinuous (in samples).
const DECLICK: u64 = 24;
/// Intro: the first phrases stay ambient.
const SILENT_PHRASES: u64 = 2;
/// Snare-type hits are this much quieter when the drums are spread (Wide, Wide + tape echo).
const SPREAD_SNARE_LEVEL: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Kick,
    Snare,
    Ghost,
    Hat,
    OpenHat,
    Ride,
}

impl Voice {
    pub fn letter(self) -> char {
        match self {
            Voice::Kick => 'K',
            Voice::Snare => 'S',
            Voice::Ghost => 'g',
            Voice::Hat => 'h',
            Voice::OpenHat => 'O',
            Voice::Ride => 'R',
        }
    }

    pub fn is_cymbal(self) -> bool {
        matches!(self, Voice::Hat | Voice::OpenHat | Voice::Ride)
    }
}

/// How a step plays its slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Play,
    Reverse,
    /// Retriggered twice within the step (a 1/32 stutter roll).
    Roll,
    /// Played slower, so lower and shorter.
    PitchDown,
    /// Half speed across two steps: the "half-time" drag.
    Half,
    /// Silence for the step: a microsound dropout.
    Gap,
    /// Retriggered four times within the step (a 1/64 ratchet).
    Ratchet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Out,
    /// Only the high end of the break: hats and ride.
    HatsOnly,
    HalfTime,
    Full,
    /// Full, with many more edits and rolls.
    Rolls,
    /// 2-step garage: the kick filtered away, claps and hats floating.
    Breakdown,
    /// 2-step garage: the full groove.
    TwoStep,
    /// 2-step garage, with stutters and reversals let loose.
    Shuffle,
    /// Microsound: the clicks and grains without the kick.
    Clicks,
    /// Microsound: the full pulse.
    Pulse,
    /// Microsound: the pulse, failing more often.
    Fracture,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::Out => "out",
            Section::HatsOnly => "hats only",
            Section::HalfTime => "half-time",
            Section::Full => "full break",
            Section::Rolls => "full + rolls",
            Section::Breakdown => "breakdown",
            Section::TwoStep => "2-step",
            Section::Shuffle => "2-step + glitch",
            Section::Clicks => "clicks only",
            Section::Pulse => "micro pulse",
            Section::Fracture => "pulse + dropouts",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub slice: u8,
    pub op: Op,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub bar: u64,
    pub section: Section,
    pub fill: bool,
    pub steps: [Step; STEPS],
    /// Step `i` continues seamlessly from step `i - 1` (no fade needed).
    join: [bool; STEPS],
}

/// The break's original patterns, at 32 sixteenths over two bars:
/// (name, kicks, snares, ghosts). Hats and ride are added on top. The names
/// describe the shape only (`HC002`): they never reach the sound.
const PATTERNS: [(&str, &[usize], &[usize], &[usize]); 3] = [
    ("busy break: doubled kicks, many ghosts", &[0, 2, 10, 11, 16, 18, 26], &[4, 12, 20, 28], &[7, 9, 15, 23, 25, 31]),
    ("open break: sparse, late kicks", &[0, 6, 10, 16, 23, 26], &[4, 12, 20, 28], &[14, 30]),
    ("rolling break: pushed kicks", &[0, 3, 8, 10, 16, 19, 24], &[4, 12, 20, 28], &[13, 29, 31]),
];

const SR: f64 = crate::SAMPLE_RATE as f64;

fn add_scaled(buf: &mut [f64], at: usize, sound: &[f64], gain: f64) {
    if gain == 1.0 {
        return add(buf, at, sound);
    }
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s * gain;
        }
    }
}

fn add(buf: &mut [f64], at: usize, sound: &[f64]) {
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s;
        }
    }
}

pub struct Drums {
    enabled: bool,
    /// Whether steps may play their slice backwards.
    reverse: bool,
    seed: u64,
    /// The kit family and every voice's numbers.
    params: KitParams,
    /// A sixteenth at the break's source tempo, and the playback rate to the track's tempo.
    src16: usize,
    rate: f64,
    /// A sixteenth and a bar at the track's tempo (`Tempo::drum16`), in samples.
    six: u64,
    bar: u64,
    /// The sampled break (stereo), and the snare-type hits alone (mono) for the tape echo.
    brk: Vec<f64>,
    brk_r: Vec<f64>,
    perc: Vec<f64>,
    space: DrumSpace,
    /// (source position, voice) of every hit in the break.
    hits: Vec<(usize, Voice)>,
    pattern: &'static str,
    plans: [Option<Plan>; 2],
    highpass: [Svf; 2],
    hp_mix: f64,
    /// 2-step garage: the swing (the first sixteenth's share of each pair,
    /// 0.5 = straight), or `None` for jungle.
    swing: Option<f64>,
    /// The garage breakdown's low cut, and how far it is in.
    low_cut: [Svf; 2],
    lc_mix: f64,
    /// Microsound: its pulse, grains and dropouts, or `None` for the other modes.
    micro: Option<crate::micro::MicroSetup>,
    // Display-only.
    last_hits: Vec<Voice>,
    hit_count: u64,
    step: usize,
}

impl Drums {
    /// Enable or disable reversed slices in the break edits.
    pub fn set_reverse(&mut self, on: bool) {
        self.reverse = on;
    }

    pub fn new(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, None, None)
    }

    /// 2-step garage drums, shuffled by `swing` (0.5 straight .. about 0.66).
    pub fn garage(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo, swing: f64) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, Some(swing.clamp(0.5, 0.72)), None)
    }

    /// Microsound drums: straight, exact, small.
    pub fn micro(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo, setup: crate::micro::MicroSetup) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, None, Some(setup))
    }

    fn styled(
        seed: u64,
        kit: crate::kits::Kit,
        space: DrumSpace,
        harmony: &Harmony,
        tempo: Tempo,
        swing: Option<f64>,
        micro: Option<crate::micro::MicroSetup>,
    ) -> Self {
        let mut params = KitParams::new(seed, kit, harmony);
        if swing.is_some() {
            // A rounder, shorter kick that leaves room for a moving bassline.
            params.kick_decay = params.kick_decay.min(0.3);
            params.kick_drop *= 0.8;
        }
        let six = tempo.drum16;
        // Garage and microsound play their loop at the track's own tempo, unpitched.
        let src16 = if swing.is_some() || micro.is_some() { six as usize } else { (60.0 / params.source_bpm / 4.0 * SR).round() as usize };
        let mut d = Drums {
            enabled: kit != crate::kits::Kit::Off,
            reverse: true,
            seed,
            rate: src16 as f64 / six as f64,
            src16,
            six,
            bar: STEPS as u64 * six,
            params,
            brk: Vec::new(),
            brk_r: Vec::new(),
            perc: Vec::new(),
            space,
            hits: Vec::new(),
            pattern: "off",
            plans: [None, None],
            highpass: [Svf::new(5000.0, 0.7, SR), Svf::new(5000.0, 0.7, SR)],
            hp_mix: 0.0,
            swing,
            low_cut: [Svf::new(260.0, 0.7, SR), Svf::new(260.0, 0.7, SR)],
            lc_mix: 0.0,
            micro,
            last_hits: Vec::new(),
            hit_count: 0,
            step: 0,
        };
        if d.enabled {
            d.record_break();
        }
        d
    }

    fn break_len(&self) -> usize {
        SLICES * self.src16
    }

    /// The jungle break's hits: one of the classic patterns, played with
    /// human timing at the source tempo. (Its draws on `rng` are exactly the
    /// break's original ones, so the break never changes.)
    fn jungle_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let (name, kicks, snares, ghosts) = PATTERNS[rng.below(PATTERNS.len())];
        self.pattern = name;
        let p = &self.params;
        let src16 = self.src16;
        let swing = (src16 as f64 * 0.05) as usize;
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        let at = |step: usize, rng: &mut Rng| {
            // Human timing: ±1.2 ms push/pull, plus a little swing on off-beats.
            let jitter = (rng.bipolar() * 58.0) as i64;
            let base = step * src16 + if step % 2 == 1 { swing } else { 0 };
            (base as i64 + jitter).max(0) as usize
        };
        for s in 0..SLICES {
            if kicks.contains(&s) || (s % 8 == 6 && rng.chance(0.08)) {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.85, 1.0)));
            }
            if snares.contains(&s) {
                hits.push((at(s, rng), Voice::Snare, rng.range(0.9, 1.0)));
            } else if ghosts.contains(&s) || (s % 2 == 1 && rng.chance(0.12)) {
                hits.push((at(s, rng), Voice::Ghost, rng.range(0.22, 0.38)));
            }
            // The 8ths on the ride or closed hats (accents alternate), some off-16th hats,
            // and an open hat to close bar 2.
            if s % 2 == 0 {
                let accent = if s % 4 == 0 { 0.75 } else { 0.45 } * rng.range(0.85, 1.0);
                let v = if p.ride_on_8ths { Voice::Ride } else { Voice::Hat };
                hits.push((at(s, rng), v, accent));
            } else if rng.chance(p.hat_density) {
                hits.push((at(s, rng), Voice::Hat, rng.range(0.25, 0.45)));
            }
            if s == 30 {
                hits.push((at(s, rng), Voice::OpenHat, 0.5));
            }
        }
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// The 2-step garage loop's hits, at the track's own tempo, from the
    /// genre's rules with seed 5 choosing within them.
    fn garage_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let six = self.src16;
        let p = &self.params;
        let swing = self.swing.unwrap_or(0.5);
        // Each odd sixteenth is pushed late by the swing.
        let push = ((2.0 * swing - 1.0) * six as f64) as usize;
        // A hair of human timing, only ever late: every hit begins inside its
        // own slice, so moving or fading a slice never splits a hit.
        let at = |step: usize, rng: &mut Rng| {
            let jitter = (rng.unit() * 24.0) as usize;
            step * six + if step % 2 == 1 { push } else { 0 } + jitter
        };
        // The skip: bar A's second kick lands just past beat three (the "and"
        // or the "a"), bar B's anywhere from just before to well after it.
        let skip_a = rng.pick(&[10usize, 11]);
        let skip_b = rng.pick(&[9usize, 10, 11, 14]);
        let pickup = rng.chance(0.45);
        let ghost_kick = if rng.chance(0.4) { Some(rng.pick(&[3usize, 7])) } else { None };
        let rim_a = rng.pick(&[6usize, 7, 14]);
        let rim_b = rng.pick(&[6usize, 14, 15]);
        // Open hats on the off-beat eighths: always on two of them.
        let open: [bool; 4] = [true, rng.chance(0.7), true, rng.chance(0.7)];
        let name = match (skip_a, pickup) {
            (10, false) => "2-step: skip on the and of 3",
            (10, true) => "2-step: skip on the and of 3, pickup",
            (_, false) => "2-step: skip on the a of 3",
            (_, true) => "2-step: skip on the a of 3, pickup",
        };
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        for s in 0..SLICES {
            let (b, second) = (s % STEPS, s >= STEPS);
            let kick = b == 0 || (!second && b == skip_a) || (second && b == skip_b);
            if kick {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.9, 1.0)));
            } else if second && pickup && b == 15 {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.55, 0.7)));
            } else if !second && ghost_kick == Some(b) {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.4, 0.5)));
            }
            if b == 4 || b == 12 {
                hits.push((at(s, rng), Voice::Snare, rng.range(0.88, 1.0)));
            } else if (!second && b == rim_a) || (second && b == rim_b) || (b % 2 == 1 && rng.chance(0.08)) {
                hits.push((at(s, rng), Voice::Ghost, rng.range(0.25, 0.4)));
            }
            // Shuffled sixteenth hats, an open hat on the off-beat eighths.
            if b % 4 == 2 && open[b / 4] {
                hits.push((at(s, rng), Voice::OpenHat, rng.range(0.42, 0.55)));
            } else if b % 2 == 0 {
                let accent = if b % 4 == 0 { 0.5 } else { 0.4 } * rng.range(0.85, 1.0);
                hits.push((at(s, rng), Voice::Hat, accent));
            } else if rng.chance(0.55 + 0.4 * p.hat_density) {
                hits.push((at(s, rng), Voice::Hat, rng.range(0.2, 0.34)));
            }
        }
        self.pattern = name;
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// The microsound loop's hits: exactly on the sixteenths, from the
    /// pulse and grain settings with seed 5 choosing within them.
    fn micro_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let six = self.src16;
        let m = self.micro.unwrap_or(crate::micro::MicroSetup::DEFAULT);
        let g = m.grains.amount();
        let kick = m.pulse != crate::micro::Pulse::Off;
        let click = m.pulse == crate::micro::Pulse::SubClick;
        // Bar B may leave one beat's kick out.
        let skip = if rng.chance(0.5) { Some(rng.pick(&[4usize, 8, 12])) } else { None };
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        for s in 0..SLICES {
            let (b, second) = (s % STEPS, s >= STEPS);
            let at = s * six;
            if kick && b % 4 == 0 && !(second && skip == Some(b)) {
                hits.push((at, Voice::Kick, rng.range(0.85, 1.0)));
            }
            if click {
                if b % 4 == 0 {
                    hits.push((at, Voice::Snare, rng.range(0.35, 0.5)));
                } else if b % 4 == 2 && rng.chance(0.5) {
                    hits.push((at, Voice::Snare, rng.range(0.25, 0.4)));
                }
            }
            // Ticks on the sixteenths, softer off the eighths.
            if rng.chance(0.25 + 0.45 * g) {
                let accent = if b % 2 == 0 { 1.0 } else { 0.75 };
                hits.push((at, Voice::Hat, rng.range(0.12, 0.35) * accent));
            }
            if rng.chance(0.08 + 0.22 * g) {
                hits.push((at, Voice::Ghost, rng.range(0.2, 0.5)));
            }
            if rng.chance(0.03 + 0.04 * g) {
                hits.push((at, Voice::OpenHat, rng.range(0.25, 0.35)));
            }
        }
        self.pattern = match (kick, skip.is_some()) {
            (false, _) => "micro: clicks and grains, no pulse",
            (true, false) => "micro: four-on-the-floor sub",
            (true, true) => "micro: four-on-the-floor sub, a beat left out",
        };
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// Plays and samples the break (once, at start-up).
    fn record_break(&mut self) {
        let garage = self.swing.is_some();
        let micro = self.micro.is_some();
        let mut rng = Rng::stream(self.seed, if micro { 0x31C0 } else if garage { 0x2577 } else { 0xB4EA });
        let hits = if micro {
            self.micro_hits(&mut rng)
        } else if garage {
            self.garage_hits(&mut rng)
        } else {
            self.jungle_hits(&mut rng)
        };
        let click_on_kick = self.micro.is_some_and(|m| m.pulse == crate::micro::Pulse::SubClick);
        let p = self.params.clone();
        let len = self.break_len() + SR as usize;
        let (mut buf, mut buf_r, mut perc) = (vec![0.0; len], vec![0.0; len], vec![0.0; len]);
        // Panning has its own random stream, so it never shifts the break itself.
        let mut pan_rng = Rng::stream(self.seed, 0x9A4);
        let space = self.space;
        for (i, (pos, voice, vel)) in hits.iter().enumerate() {
            let sound = match voice {
                Voice::Kick if micro => {
                    let mut k = crate::kits::sub_kick(&p, *vel);
                    if click_on_kick {
                        for (j, x) in crate::kits::micro_click(&p, 0.5 * vel, false, &mut rng).iter().enumerate() {
                            k[j] += x;
                        }
                    }
                    k
                }
                Voice::Snare if micro => crate::kits::micro_click(&p, *vel, false, &mut rng),
                Voice::Hat if micro => crate::kits::micro_click(&p, *vel, true, &mut rng),
                Voice::Ghost | Voice::OpenHat | Voice::Ride if micro => crate::kits::micro_grain(&p, *vel, &mut rng),
                Voice::Kick => crate::kits::kick(&p, *vel, &mut rng),
                Voice::Snare if garage => {
                    // A clap over the kit's snare, set a little back.
                    let mut c = crate::kits::clap(&p, *vel, &mut rng);
                    let s = crate::kits::snare(&p, *vel * 0.7, false, &mut rng);
                    for (k, x) in s.iter().enumerate() {
                        if let Some(y) = c.get_mut(k) {
                            *y += 0.45 * x;
                        }
                    }
                    c
                }
                Voice::Snare => crate::kits::snare(&p, *vel, false, &mut rng),
                Voice::Ghost => crate::kits::snare(&p, *vel, true, &mut rng),
                v => {
                    // Choke: a cymbal stops when the next cymbal is struck.
                    let next = hits[i + 1..].iter().find(|h| h.1.is_cymbal()).map(|h| h.0);
                    let max_len = next.map_or(usize::MAX, |n| n.saturating_sub(*pos).max(96) + 96);
                    crate::kits::cymbal(&p, *v, *vel, max_len, &mut rng)
                }
            };
            // Where this hit sits: kick centred; snare-type hits 45–65% to a random side,
            // cymbals 50–75% (never near centre, so the clicks always use the space).
            // (2-step garage keeps its claps in the middle with the kick: they
            // are the backbeat. Its rims and ghosts still spread out.)
            // (Microsound places every small sound exactly, anywhere across
            // the field; the kick stays in the middle.)
            let (lo, hi) = match voice {
                Voice::Kick => (0.0, 0.0),
                _ if micro => (0.2, 0.95),
                Voice::Snare if garage => (0.0, 0.0),
                Voice::Snare | Voice::Ghost => (0.45, 0.65),
                _ => (0.5, 0.75),
            };
            let pan = if space == DrumSpace::Centred || hi == 0.0 {
                0.0
            } else {
                let side = if pan_rng.chance(0.5) { -1.0 } else { 1.0 };
                side * pan_rng.range(lo, hi)
            };
            // Panning only turns the far side down, so a panned hit never drives the bus
            // harder than a centred one; snares also sit a little lower when spread.
            let (gl, gr) = if pan == 0.0 {
                (1.0, 1.0)
            } else {
                let (a, b) = crate::math::pan_gains(pan);
                let level = if matches!(voice, Voice::Snare | Voice::Ghost) { SPREAD_SNARE_LEVEL } else { 1.0 };
                (level * a / a.max(b), level * b / a.max(b))
            };
            add_scaled(&mut buf, *pos, &sound, gl);
            add_scaled(&mut buf_r, *pos, &sound, gr);
            if space == DrumSpace::Tape && matches!(voice, Voice::Snare | Voice::Ghost) {
                add(&mut perc, *pos, &sound);
            }
        }
        self.hits = hits.iter().map(|(p, v, _)| (*p, *v)).collect();

        if micro {
            // No bus, no sampler, no room: each click as sharp as it was
            // made. A fixed level, so a loop of only grains stays small.
            let peak = buf.iter().chain(buf_r.iter()).fold(1.0f64, |a, x| a.max(x.abs()));
            for x in buf.iter_mut().chain(buf_r.iter_mut()) {
                *x = sanitize(*x * 0.9 / peak);
            }
            self.brk = buf;
            self.brk_r = buf_r;
            self.perc = perc;
            return;
        }

        // Punch: compress and soft-clip the whole break, as if bussed through hardware.
        crate::kits::punch_bus(&mut buf, &mut buf_r, p.punch);

        // The sampler: saturation, sample-and-hold, bit depth, a lowpass.
        let joint = |l: &[f64], r: &[f64]| l.iter().zip(r).fold(1e-9f64, |a, (x, y)| a.max(x.abs()).max(y.abs()));
        let levels = (1u64 << (p.bits - 1)) as f64;
        let sample = |b: &mut [f64], peak: f64| {
            let mut lp = OnePole::new(p.lp_hz, SR);
            let mut held = 0.0;
            for (i, x) in b.iter_mut().enumerate() {
                let v = tanh(1.4 * *x / peak) / tanh(1.4);
                if i % p.hold == 0 {
                    held = (v * levels).round() / levels;
                }
                *x = lp.process(held);
            }
        };
        let peak = joint(&buf, &buf_r);
        sample(&mut buf, peak);
        sample(&mut buf_r, peak);

        // A small room; wide spaces cross-feed it a little between the channels.
        let taps = [(529usize, 0.16), (811, 0.11), (1103, 0.08)];
        let cross = if space == DrumSpace::Centred { 0.0 } else { 0.15 };
        let (dry_l, dry_r) = (buf.clone(), buf_r.clone());
        let mut room_lp = [OnePole::new(4000.0, SR), OnePole::new(4000.0, SR)];
        for i in 0..len {
            let (mut wl, mut wr) = (0.0, 0.0);
            for (d, g) in taps {
                if i >= d {
                    if cross == 0.0 {
                        wl += dry_l[i - d] * g;
                        wr += dry_r[i - d] * g;
                    } else {
                        wl += (dry_l[i - d] * (1.0 - cross) + dry_r[i - d] * cross) * g;
                        wr += (dry_r[i - d] * (1.0 - cross) + dry_l[i - d] * cross) * g;
                    }
                }
            }
            buf[i] += room_lp[0].process(wl);
            buf_r[i] += room_lp[1].process(wr);
        }
        let peak = joint(&buf, &buf_r);
        for x in buf.iter_mut().chain(buf_r.iter_mut()) {
            *x = sanitize(*x * 0.9 / peak);
        }
        if space == DrumSpace::Tape {
            let peak = perc.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
            sample(&mut perc, peak);
            let peak = perc.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
            for x in perc.iter_mut() {
                *x = sanitize(*x * 0.9 / peak);
            }
        }
        self.brk = buf;
        self.brk_r = buf_r;
        self.perc = perc;
    }

    /// The arrangement section for phrase `p` (pure).
    fn section(&self, p: u64, mods: &Mods) -> Section {
        if p < SILENT_PHRASES {
            return Section::Out;
        }
        if p == SILENT_PHRASES {
            return Section::HatsOnly;
        }
        if self.swing.is_some() {
            return self.garage_section(p, mods);
        }
        if self.micro.is_some() {
            return self.micro_section(p, mods);
        }
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
        let mut rng = Rng::stream(self.seed, 0x5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.22 => Section::Out,
            x if x < 0.34 => Section::HatsOnly,
            x if x < 0.48 => Section::HalfTime,
            x if x < 0.8 => Section::Full,
            _ => Section::Rolls,
        }
    }

    /// A garage phrase's section: hats, the breakdown, the 2-step, or the
    /// 2-step with its glitches, by the same energy LFO as jungle's.
    fn garage_section(&self, p: u64, mods: &Mods) -> Section {
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
        let mut rng = Rng::stream(self.seed, 0x2577_5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.2 => Section::Out,
            x if x < 0.32 => Section::HatsOnly,
            x if x < 0.46 => Section::Breakdown,
            x if x < 0.8 => Section::TwoStep,
            _ => Section::Shuffle,
        }
    }

    /// A microsound phrase's section: clicks alone, the pulse, or the pulse
    /// failing, by the same energy LFO.
    fn micro_section(&self, p: u64, mods: &Mods) -> Section {
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
        let mut rng = Rng::stream(self.seed, 0x31C0_5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.2 => Section::Out,
            x if x < 0.36 => Section::Clicks,
            x if x < 0.82 => Section::Pulse,
            _ => Section::Fracture,
        }
    }

    /// The slice plan for a microsound bar (pure): the loop in order, with
    /// digital failures on top — a step dropping out, skipping back to repeat
    /// the one before, or ratcheting.
    fn micro_plan(&self, bar: u64, mods: &Mods) -> Plan {
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = Step { slice: (half + i) as u8, op: Op::Play };
        }
        let mut rng = Rng::stream(self.seed, 0x31C0_BA40_0000 ^ bar);
        let base = self.micro.map_or(0.0, |m| m.dropouts.chance());
        let p = if section == Section::Out || base == 0.0 {
            0.0
        } else {
            base + if section == Section::Fracture { 0.12 } else { 0.0 } + if fill { 0.1 } else { 0.0 }
        };
        for i in 1..STEPS {
            if rng.chance(p) {
                let here = steps[i].slice;
                steps[i] = match rng.weighted(&[0.3, 0.25, 0.25, 0.2]) {
                    0 => Step { slice: here, op: Op::Gap },
                    1 => Step { slice: steps[i - 1].slice, op: Op::Play },
                    2 => Step { slice: here, op: Op::Ratchet },
                    _ => Step { slice: here, op: Op::Roll },
                };
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = a.op == Op::Play && b.op == Op::Play && b.slice == a.slice + 1;
        }
        Plan { bar, section, fill, steps, join }
    }

    /// The slice plan for a garage bar (pure): the loop in order, with
    /// glitch edits on top.
    fn garage_plan(&self, bar: u64, mods: &Mods) -> Plan {
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = Step { slice: (half + i) as u8, op: Op::Play };
        }
        let mut rng = Rng::stream(self.seed, 0x2577_BA40_0000 ^ bar);
        let p = match section {
            Section::Out => 0.0,
            Section::HatsOnly => 0.06,
            Section::Breakdown => 0.1,
            Section::Shuffle => 0.26,
            _ => 0.08,
        } + if fill && section != Section::Out { 0.15 } else { 0.0 };
        let strong = self.strong_slices();
        // Slices holding only hats and rims: what a stutter or a dropped kick
        // borrows — only from the same place in the swing (an on-beat slice
        // for an on-beat step, a pushed one for a pushed step), so a borrowed
        // hit lands exactly where that step's own would.
        let weak_at = |i: usize| -> Vec<u8> { (0..SLICES as u8).filter(|s| !strong.contains(s) && *s as usize % 2 == i % 2).collect() };
        for (i, s) in steps.iter_mut().enumerate().skip(1) {
            let weak = weak_at(i);
            if rng.chance(p) {
                *s = match rng.weighted(&[0.35, 0.2, 0.2, 0.25]) {
                    // A hat stutter.
                    0 if !weak.is_empty() => Step { slice: rng.pick(&weak), op: Op::Roll },
                    // A clap reversed into the beat after it.
                    1 if self.reverse => Step { slice: if i % 8 == 3 { s.slice + 1 } else { s.slice }, op: Op::Reverse },
                    // The kick drops out for a beat.
                    2 if strong.contains(&s.slice) && !weak.is_empty() => Step { slice: rng.pick(&weak), op: Op::Play },
                    3 => Step { slice: s.slice, op: Op::PitchDown },
                    _ => *s,
                };
            }
            // Fill: a reversed clap and a stutter into the next phrase.
            if fill && section != Section::Out {
                if i == STEPS - 2 && self.reverse && rng.chance(0.6) {
                    *s = Step { slice: (half + 12) as u8, op: Op::Reverse };
                } else if i == STEPS - 1 && rng.chance(0.6) {
                    *s = Step { slice: s.slice, op: Op::Roll };
                }
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = a.op == Op::Play && b.op == Op::Play && b.slice == a.slice + 1;
        }
        Plan { bar, section, fill, steps, join }
    }

    /// Slices containing a kick or a snare: what jungle edits love to move around.
    fn strong_slices(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self
            .hits
            .iter()
            .filter(|(_, voice)| matches!(voice, Voice::Kick | Voice::Snare))
            .map(|(p, _)| ((p + self.src16 / 2) / self.src16).min(SLICES - 1) as u8)
            .collect();
        v.dedup();
        v
    }

    /// The slice plan for bar `bar` (pure).
    pub fn plan(&self, bar: u64, mods: &Mods) -> Plan {
        if self.swing.is_some() {
            return self.garage_plan(bar, mods);
        }
        if self.micro.is_some() {
            return self.micro_plan(bar, mods);
        }
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = if section == Section::HalfTime {
                Step { slice: (half + i / 2) as u8, op: Op::Half }
            } else {
                Step { slice: (half + i) as u8, op: Op::Play }
            };
        }
        let mut rng = Rng::stream(self.seed, 0xBA40_0000 ^ bar);
        let p = match section {
            Section::Out | Section::HalfTime => 0.0,
            Section::HatsOnly => 0.08,
            Section::Full => 0.14,
            Section::Rolls => 0.32,
            // Garage and microsound sections never reach the jungle plan.
            Section::Breakdown | Section::TwoStep | Section::Shuffle | Section::Clicks | Section::Pulse | Section::Fracture => 0.0,
        } + if fill && section != Section::Out { 0.2 } else { 0.0 };
        let strong = self.strong_slices();
        for (i, s) in steps.iter_mut().enumerate().skip(1) {
            if rng.chance(p) {
                *s = match rng.weighted(&[0.4, 0.2, 0.2, 0.2]) {
                    0 if !strong.is_empty() => Step { slice: rng.pick(&strong), op: Op::Play },
                    1 => Step { slice: s.slice, op: if self.reverse { Op::Reverse } else { Op::Play } },
                    2 => Step { slice: s.slice, op: Op::Roll },
                    _ => Step { slice: s.slice, op: Op::PitchDown },
                };
            }
            // Fill: a snare roll into the next phrase.
            if fill && i >= STEPS - 3 && section != Section::HalfTime && rng.chance(0.6) {
                let snare = strong.iter().copied().find(|x| *x as usize % 8 == 4).unwrap_or(4);
                *s = Step { slice: snare, op: Op::Roll };
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = match (a.op, b.op) {
                (Op::Play, Op::Play) => b.slice == a.slice + 1,
                (Op::Half, Op::Half) => b.slice == a.slice || b.slice == a.slice + 1,
                _ => false,
            };
        }
        Plan { bar, section, fill, steps, join }
    }

    fn sample_at(&self, pos: f64) -> (f64, f64, f64) {
        let i = pos.floor();
        let f = pos - i;
        let i = i as usize;
        let read = |b: &[f64]| match (b.get(i), b.get(i + 1)) {
            (Some(a), Some(c)) => a + (c - a) * f,
            (Some(a), None) => *a,
            _ => 0.0,
        };
        (read(&self.brk), read(&self.brk_r), if self.perc.is_empty() { 0.0 } else { read(&self.perc) })
    }

    fn current(&mut self, bar: u64, mods: &Mods) -> Plan {
        let fresh = |p: &Option<Plan>| p.as_ref().is_some_and(|p| p.bar == bar);
        if !fresh(&self.plans[0]) {
            self.plans[0] = if fresh(&self.plans[1]) { self.plans[1].take() } else { Some(self.plan(bar, mods)) };
            self.plans[1] = Some(self.plan(bar + 1, mods));
        }
        self.plans[0].clone().unwrap()
    }

    /// Where a stutter retriggers within a step, and where in the slice the
    /// repeat reads from. Halfway, from the slice's start — except a swung
    /// garage off-beat, whose hit sits late in its slice: there the repeat
    /// starts at the hit and lands halfway through the rest, so both strikes
    /// stay on the swung grid.
    fn roll_split(&self, slice: u8) -> (u64, u64) {
        let six = self.six;
        match self.swing {
            Some(swing) if slice % 2 == 1 => {
                let push = ((2.0 * swing - 1.0) * six as f64) as u64;
                (push + (six - push) / 2, push)
            }
            _ => (six / 2, 0),
        }
    }

    /// The source range a step reads, for counting hits.
    fn range(&self, step: Step, i: usize) -> (f64, f64) {
        let base = step.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
        match step.op {
            Op::Play | Op::Reverse => (base, base + len),
            Op::Roll => (base, base + len / 2.0),
            Op::Ratchet => (base, base + len / 4.0),
            Op::Gap => (base, base),
            Op::PitchDown => (base, base + len * 0.75),
            Op::Half => {
                let h = if i % 2 == 0 { 0.0 } else { len / 2.0 };
                (base + h, base + h + len / 2.0)
            }
        }
    }

    /// (left, right, snare-type hits for the tape echo)
    pub fn next(&mut self, clock: u64, mods: &Mods) -> (f64, f64, f64) {
        if !self.enabled {
            return (0.0, 0.0, 0.0);
        }
        let six = self.six;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        let i = (in_bar / six) as usize;
        let t = in_bar % six;
        if in_bar == 0 || self.plans[0].is_none() {
            self.current(bar, mods);
        }
        let (st, section, joined, next_joins) = {
            let plan = self.plans[0].as_ref().unwrap();
            let st = plan.steps[i];
            let next_joins = if i + 1 < STEPS {
                plan.join[i + 1]
            } else {
                self.plans[1].as_ref().is_some_and(|n| {
                    n.section == plan.section
                        && matches!((st.op, n.steps[0].op), (Op::Play, Op::Play))
                        && n.steps[0].slice as usize == (st.slice as usize + 1) % SLICES
                })
            };
            (st, plan.section, plan.join[i], next_joins)
        };

        // Retriggers: count which drum hits this step plays (display only).
        let (roll_at, roll_from) = self.roll_split(st.slice);
        let quarter = (six / 4).max(1);
        let retrigger = t == 0 || (st.op == Op::Roll && t == roll_at) || (st.op == Op::Ratchet && t % quarter == 0);
        if retrigger && section != Section::Out {
            self.step = i;
            let (lo, hi) = self.range(st, i);
            self.last_hits = self
                .hits
                .iter()
                .filter(|(p, v)| {
                    (*p as f64) >= lo
                        && (*p as f64) < hi
                        && (section != Section::HatsOnly || v.is_cymbal())
                        && (section != Section::Clicks || *v != Voice::Kick)
                })
                .map(|(_, v)| *v)
                .collect();
            self.hit_count += self.last_hits.len() as u64;
        }

        let base = st.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
        let rate = self.rate;
        let tt = t as f64;
        let (pos, local, local_len) = match st.op {
            Op::Play => (base + tt * rate, t, six),
            Op::Reverse => (base + len - 1.0 - tt * rate, t, six),
            Op::Roll => {
                if t < roll_at {
                    (base + tt * rate, t, roll_at)
                } else {
                    let u = t - roll_at;
                    (base + (roll_from + u) as f64 * rate, u, six - roll_at)
                }
            }
            Op::PitchDown => (base + tt * rate * 0.75, t, six),
            Op::Ratchet => {
                let u = t % quarter;
                (base + u as f64 * rate, u, quarter)
            }
            Op::Gap => (base, t, six),
            Op::Half => (base + (tt + (i % 2) as f64 * six as f64) * rate * 0.5, t, six),
        };
        let (mut x, mut xr, mut xp) = if section == Section::Out || st.op == Op::Gap { (0.0, 0.0, 0.0) } else { self.sample_at(pos) };

        // Fade only where the sequence actually jumps.
        let joined_in = joined && !matches!(st.op, Op::Roll | Op::Ratchet);
        let fade_in = if joined_in && local == t { 1.0 } else { (local as f64 / DECLICK as f64).min(1.0) };
        let to_end = local_len - local;
        let fade_out = if next_joins && !matches!(st.op, Op::Roll | Op::Ratchet) { 1.0 } else { (to_end as f64 / DECLICK as f64).min(1.0) };
        x *= fade_in * fade_out;
        xr *= fade_in * fade_out;
        xp *= fade_in * fade_out;

        // Hats-only sections: glide a high-pass in, so only the cymbals remain
        // (and the snare-only tape send fades out with it).
        let target = if matches!(section, Section::HatsOnly | Section::Clicks) { 1.0 } else { 0.0 };
        self.hp_mix += (target - self.hp_mix) * 0.0005;
        let low = self.highpass[0].process(x).low;
        let high = x - low;
        let low_r = self.highpass[1].process(xr).low;
        let high_r = xr - low_r;
        let (mut l, mut r) = (x + (high - x) * self.hp_mix, xr + (high_r - xr) * self.hp_mix);
        if self.swing.is_some() {
            // The garage breakdown: the kick and the bass end of the loop
            // filtered away, so the claps and hats float.
            let target = if section == Section::Breakdown { 1.0 } else { 0.0 };
            self.lc_mix += (target - self.lc_mix) * 0.0004;
            let (ll, lr) = (self.low_cut[0].process(l).low, self.low_cut[1].process(r).low);
            l -= ll * self.lc_mix;
            r -= lr * self.lc_mix;
        }
        (l, r, xp * (1.0 - self.hp_mix))
    }
}

/// Read-only views for visualisation.
impl Drums {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn space(&self) -> DrumSpace {
        self.space
    }

    pub fn pattern_name(&self) -> &'static str {
        self.pattern
    }

    /// Every hit of the recorded loop: (source position in samples, voice).
    pub fn hits(&self) -> &[(usize, Voice)] {
        &self.hits
    }

    /// A sixteenth and a bar at the track's tempo, in samples.
    pub fn sixteenth(&self) -> u64 {
        self.six
    }

    pub fn bar(&self) -> u64 {
        self.bar
    }

    /// The current bar's plan, if playing.
    pub fn plan_now(&self) -> Option<&Plan> {
        self.plans[0].as_ref()
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn last_hits(&self) -> &[Voice] {
        &self.last_hits
    }

    pub fn hit_count(&self) -> u64 {
        self.hit_count
    }

    /// The sampled break as `points` peak values, and where each slice's hits are.
    pub fn overview(&self, points: usize) -> Vec<f32> {
        if self.brk.is_empty() {
            return Vec::new();
        }
        let n = self.break_len();
        let chunk = n.div_ceil(points);
        let both: Vec<f64> = self.brk[..n].iter().zip(&self.brk_r[..n]).map(|(l, r)| l.abs().max(r.abs())).collect();
        both.chunks(chunk).map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32).collect()
    }

    /// The main voice in each of the 32 slices (for labelling the chop grid).
    pub fn slice_voices(&self) -> Vec<Option<Voice>> {
        (0..SLICES)
            .map(|s| {
                let src = self.src16;
                let (lo, hi) = (s * src, (s + 1) * src);
                let mut best: Option<Voice> = None;
                for (p, v) in &self.hits {
                    if *p >= lo.saturating_sub(src / 4) && *p < hi.saturating_sub(src / 4) {
                        let rank = |v: Voice| match v {
                            Voice::Kick => 5,
                            Voice::Snare => 4,
                            Voice::Ghost => 3,
                            Voice::OpenHat => 2,
                            Voice::Ride => 1,
                            Voice::Hat => 0,
                        };
                        if best.is_none_or(|b| rank(*v) > rank(b)) {
                            best = Some(*v);
                        }
                    }
                }
                best
            })
            .collect()
    }
}

/// What the TUI shows about the kit.
#[derive(Clone, Debug)]
pub struct VoiceInfo {
    pub kit: &'static str,
    /// One line each for kick, snare and cymbals.
    pub lines: [String; 3],
    pub kick_from_hz: f64,
    pub kick_to_hz: f64,
    pub kick_drop_ms: f64,
    pub snare_hz: [f64; 2],
    pub cymbal_hz: Vec<f64>,
    pub source_bpm: f64,
    pub play_bpm: f64,
    pub semitones_up: f64,
    pub bits: u32,
    pub hold_khz: f64,
}

impl Drums {
    pub fn voice_info(&self) -> VoiceInfo {
        let p = &self.params;
        VoiceInfo {
            kit: if self.enabled { p.kit.name() } else { "off" },
            lines: p.describe(),
            kick_from_hz: p.kick_from,
            kick_to_hz: p.kick_to,
            kick_drop_ms: p.kick_drop * 1000.0,
            snare_hz: p.snare_tones,
            cymbal_hz: p.cymbal_freqs(),
            source_bpm: 60.0 / (self.src16 as f64 / SR * 4.0),
            play_bpm: 60.0 / (self.six as f64 / SR * 4.0),
            semitones_up: 12.0 * log2(self.rate),
            bits: p.bits,
            hold_khz: SR / p.hold as f64 / 1000.0,
        }
    }
}

/// log2 by bisection on the deterministic `exp2` (display only).
fn log2(r: f64) -> f64 {
    let (mut lo, mut hi) = (-8.0f64, 8.0f64);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if crate::math::exp2(mid) < r {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

}
// ---- src/audio/fft.rs ----
pub mod fft {
//! In-place iterative radix-2 complex FFT, twiddles from the deterministic `math`.

use crate::math::{cos_turns, sin_turns};

pub struct Fft {
    n: usize,
    cos: Vec<f64>,
    sin: Vec<f64>,
    rev: Vec<u32>,
}

impl Fft {
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let cos = (0..n / 2).map(|k| cos_turns(k as f64 / n as f64)).collect();
        let sin = (0..n / 2).map(|k| sin_turns(k as f64 / n as f64)).collect();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        Fft { n, cos, sin, rev }
    }

    /// Forward: X[k] = Σ x[n]·e^(-2πikn/N). Inverse is unscaled (no 1/N).
    pub fn transform(&self, re: &mut [f64], im: &mut [f64], inverse: bool) {
        self.bit_reverse(re, im);
        for s in 0..self.stages() {
            self.stage(re, im, s, inverse);
        }
    }

    /// Number of butterfly stages (log2 N).
    pub fn stages(&self) -> u32 {
        self.n.trailing_zeros()
    }

    /// The transform's first step. `transform` = `bit_reverse` then every
    /// `stage` in order; exposed separately so the work can be spread out.
    pub fn bit_reverse(&self, re: &mut [f64], im: &mut [f64]) {
        assert!(re.len() == self.n && im.len() == self.n);
        for i in 0..self.n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
    }

    /// Butterfly stage `s` (0-based; combines blocks of length 2^(s+1)).
    pub fn stage(&self, re: &mut [f64], im: &mut [f64], s: u32, inverse: bool) {
        let n = self.n;
        let sign = if inverse { 1.0 } else { -1.0 };
        let len = 2 << s;
        let half = len / 2;
        let step = n / len;
        let mut start = 0;
        while start < n {
            for j in 0..half {
                let wr = self.cos[j * step];
                let wi = sign * self.sin[j * step];
                let a = start + j;
                let b = a + half;
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
            start += len;
        }
    }
}



}
// ---- src/audio/filter.rs ----
pub mod filter {
//! Filters.

use crate::math::{one_pole_coef, sanitize, tan_turns};

/// Topology-preserving-transform state-variable filter (Zavalishin / Simper).
#[derive(Clone, Default)]
pub struct Svf {
    k: f64,
    a1: f64,
    a2: f64,
    a3: f64,
    ic1: f64,
    ic2: f64,
}

pub struct SvfOut {
    pub low: f64,
    pub band: f64,
}

impl Svf {
    pub fn new(cutoff: f64, q: f64, sample_rate: f64) -> Self {
        let mut f = Svf::default();
        f.set(cutoff, q, sample_rate);
        f
    }

    pub fn set(&mut self, cutoff: f64, q: f64, sample_rate: f64) {
        let fc = cutoff.clamp(10.0, sample_rate * 0.45);
        let g = tan_turns(0.5 * fc / sample_rate);
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    pub fn process(&mut self, x: f64) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = sanitize(2.0 * v1 - self.ic1);
        self.ic2 = sanitize(2.0 * v2 - self.ic2);
        SvfOut { low: v2, band: v1 }
    }
}

/// One-pole low-pass.
#[derive(Clone, Default)]
pub struct OnePole {
    a: f64,
    y: f64,
}

impl OnePole {
    pub fn new(cutoff: f64, sample_rate: f64) -> Self {
        OnePole { a: one_pole_coef(cutoff, sample_rate), y: 0.0 }
    }

    /// Move the cutoff, keeping the filter's state (no click).
    pub fn set_cutoff(&mut self, cutoff: f64, sample_rate: f64) {
        self.a = one_pole_coef(cutoff, sample_rate);
    }

    pub fn process(&mut self, x: f64) -> f64 {
        self.y = sanitize(self.y + self.a * (x - self.y));
        self.y
    }
}

/// DC blocker.
#[derive(Clone, Default)]
pub struct DcBlock {
    x1: f64,
    y1: f64,
}

impl DcBlock {
    pub fn process(&mut self, x: f64) -> f64 {
        let y = sanitize(x - self.x1 + 0.9995 * self.y1);
        self.x1 = x;
        self.y1 = y;
        y
    }
}

}
// ---- src/audio/fm.rs ----
pub mod fm {
//! Phase-modulation ("FM") operators.

use crate::math::{sin_turns, TAU};

/// How three operators are wired. Operator 1 is always the carrier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Algo {
    /// 3 → 2 → 1
    Stack,
    /// 2 → 1 ← 3
    Pair,
}

#[derive(Clone, Copy)]
pub struct Op {
    pub ratio: f64,
    /// Modulation depth in radians of the operator it feeds.
    pub index: f64,
    phase: f64,
}

impl Op {
    pub fn new(ratio: f64, index: f64, phase: f64) -> Self {
        Op { ratio, index, phase }
    }

    fn advance(&mut self, freq: f64, sample_rate: f64) {
        self.phase += freq * self.ratio / sample_rate;
        self.phase -= self.phase.floor();
    }
}

/// Three-operator FM voice with self-feedback on operator 3.
pub struct Fm3 {
    pub freq: f64,
    pub ops: [Op; 3],
    pub algo: Algo,
    /// Operator-3 self-feedback in radians.
    pub feedback: f64,
    last3: f64,
}

impl Fm3 {
    pub fn new(freq: f64, algo: Algo, ops: [Op; 3], feedback: f64) -> Self {
        Fm3 { freq, ops, algo, feedback, last3: 0.0 }
    }

    /// One sample in [-1, 1]. `index_scale` scales both modulator depths.
    pub fn next(&mut self, sample_rate: f64, index_scale: f64) -> f64 {
        let [o1, o2, o3] = &self.ops;
        let s3 = sin_turns(o3.phase + self.feedback * self.last3 / TAU);
        self.last3 = s3;
        let m3 = s3 * o3.index * index_scale / TAU;
        let out = match self.algo {
            Algo::Stack => {
                let m2 = sin_turns(o2.phase + m3) * o2.index * index_scale / TAU;
                sin_turns(o1.phase + m2)
            }
            Algo::Pair => {
                let m2 = sin_turns(o2.phase) * o2.index * index_scale / TAU;
                sin_turns(o1.phase + m2 + m3)
            }
        };
        for op in self.ops.iter_mut() {
            op.advance(self.freq, sample_rate);
        }
        out
    }
}

/// Two-operator FM voice (modulator → carrier).
pub struct Fm2 {
    pub freq: f64,
    pub ratio: f64,
    carrier: f64,
    modulator: f64,
}

impl Fm2 {
    pub fn new(freq: f64, ratio: f64) -> Self {
        Fm2 { freq, ratio, carrier: 0.0, modulator: 0.0 }
    }

    pub fn next(&mut self, sample_rate: f64, index: f64) -> f64 {
        let m = sin_turns(self.modulator) * index / TAU;
        let out = sin_turns(self.carrier + m);
        self.carrier += self.freq / sample_rate;
        self.carrier -= self.carrier.floor();
        self.modulator += self.freq * self.ratio / sample_rate;
        self.modulator -= self.modulator.floor();
        out
    }
}

}
// ---- src/audio/garage.rs ----
pub mod garage {
//! The 2-step garage mode's own settings: how hard it swings, whether the
//! bass moves in rhythm or holds as a drone, how often the chord stabs come
//! and how bright they are. They only matter when the mode is 2-step garage;
//! the other modes ignore them, so their sound never moves.

/// Swing limits, in percent: the first sixteenth's share of each pair.
pub const MIN_SWING: u8 = 50;
pub const MAX_SWING: u8 = 66;

/// How the bass plays in 2-step garage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BassMode {
    /// The sustained FM bass the other modes use.
    Drone,
    /// A syncopated bassline on the chord roots, octaves and fifths.
    Rhythmic,
}

pub const BASS_MODES: [BassMode; 2] = [BassMode::Drone, BassMode::Rhythmic];

impl BassMode {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            BassMode::Drone => "the sustained bass the other modes use",
            BassMode::Rhythmic => "a swung, syncopated bassline on roots, octaves and fifths",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BassMode::Drone => "drone",
            BassMode::Rhythmic => "rhythmic",
        }
    }
}

/// How often the chord stabs play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stabs {
    Off,
    /// A stab or two a bar, on the pushes.
    Sparse,
    /// Answering phrases, most bars.
    Busy,
}

pub const STABS: [Stabs; 3] = [Stabs::Off, Stabs::Sparse, Stabs::Busy];

impl Stabs {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Stabs::Off => "no chord stabs",
            Stabs::Sparse => "a stab or two a bar, on the pushes",
            Stabs::Busy => "answering phrases, most bars",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stabs::Off => "off",
            Stabs::Sparse => "sparse",
            Stabs::Busy => "busy",
        }
    }
}

/// The colour of the stabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StabTone {
    /// A soft, round electric-piano tone.
    Dark,
    /// A bright, glassy bell-organ tone.
    Bright,
}

pub const STAB_TONES: [StabTone; 2] = [StabTone::Dark, StabTone::Bright];

impl StabTone {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            StabTone::Dark => "a soft, round electric piano",
            StabTone::Bright => "a glassy bell-organ",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            StabTone::Dark => "dark",
            StabTone::Bright => "bright",
        }
    }
}

/// The key lift: the whole progression steps up a whole tone for a while,
/// then comes home — the classic game-menu lift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyLift {
    Off,
    /// Four passes at home, four lifted.
    Every4,
    /// Eight passes at home, eight lifted.
    Every8,
}

pub const KEY_LIFTS: [KeyLift; 3] = [KeyLift::Off, KeyLift::Every4, KeyLift::Every8];

impl KeyLift {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            KeyLift::Off => "the progression stays in its key",
            KeyLift::Every4 => "four loops at home, four a whole tone up",
            KeyLift::Every8 => "eight loops at home, eight a whole tone up",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            KeyLift::Off => "off",
            KeyLift::Every4 => "every 4 loops",
            KeyLift::Every8 => "every 8 loops",
        }
    }

    /// Passes through the progression before each step (0 = never).
    pub fn passes(self) -> usize {
        match self {
            KeyLift::Off => 0,
            KeyLift::Every4 => 4,
            KeyLift::Every8 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GarageSetup {
    /// Swing, percent (50 = straight).
    pub swing: u8,
    pub bass: BassMode,
    pub stabs: Stabs,
    pub tone: StabTone,
    /// 0 = the seed picks; otherwise a 1-based index into `harmony::PROGRESSIONS`.
    pub progression: u8,
    pub lift: KeyLift,
}

impl GarageSetup {
    pub const DEFAULT: GarageSetup =
        GarageSetup { swing: 58, bass: BassMode::Rhythmic, stabs: Stabs::Sparse, tone: StabTone::Bright, progression: 0, lift: KeyLift::Off };

    /// The progression's name, or "seed picks".
    pub fn progression_name(self) -> &'static str {
        match self.progression as usize {
            0 => "seed picks",
            i => crate::harmony::PROGRESSIONS.get(i - 1).map_or("seed picks", |p| p.name),
        }
    }

    /// The swing as a fraction (0.5 .. 0.66).
    pub fn swing_fraction(self) -> f64 {
        self.swing.clamp(MIN_SWING, MAX_SWING) as f64 / 100.0
    }

    /// One recipe word: swing in the low byte, then the bass, the stabs and
    /// their tone.
    pub fn word(self) -> u32 {
        fn idx<T: PartialEq>(all: &[T], v: T) -> u32 {
            all.iter().position(|x| *x == v).unwrap_or(0) as u32
        }
        self.swing as u32
            | idx(&BASS_MODES, self.bass) << 8
            | idx(&STABS, self.stabs) << 10
            | idx(&STAB_TONES, self.tone) << 12
            | (self.progression as u32 & 0x1f) << 13
            | idx(&KEY_LIFTS, self.lift) << 18
    }

    pub fn from_word(w: u32) -> Self {
        GarageSetup {
            swing: ((w & 0xff) as u8).clamp(MIN_SWING, MAX_SWING),
            bass: BASS_MODES[(w >> 8 & 3) as usize % BASS_MODES.len()],
            stabs: STABS[(w >> 10 & 3) as usize % STABS.len()],
            tone: STAB_TONES[(w >> 12 & 1) as usize],
            progression: {
                let p = (w >> 13 & 0x1f) as usize;
                if p > crate::harmony::PROGRESSIONS.len() { 0 } else { p as u8 }
            },
            lift: KEY_LIFTS[(w >> 18 & 3) as usize % KEY_LIFTS.len()],
        }
    }
}



}
// ---- src/audio/glitch.rs ----
pub mod glitch {
//! Glitch artifact sounds: short one-shot voices triggered by the pattern layers.

use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;

/// Recent drone output, so "stutter" glitches can chop up the frozen chord.
pub struct History {
    l: Vec<f64>,
    r: Vec<f64>,
    pos: usize,
}

pub const HISTORY_LEN: usize = 1 << 16;

impl History {
    pub fn new() -> Self {
        History { l: vec![0.0; HISTORY_LEN], r: vec![0.0; HISTORY_LEN], pos: 0 }
    }

    pub fn push(&mut self, l: f64, r: f64) {
        self.l[self.pos] = l;
        self.r[self.pos] = r;
        self.pos = (self.pos + 1) % HISTORY_LEN;
    }

    fn get(&self, index: usize) -> (f64, f64) {
        let i = index % HISTORY_LEN;
        (self.l[i], self.r[i])
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Bit-crushed, sample-held FM blip.
    Blip,
    /// Differentiated noise tick.
    Tick,
    /// A repeated grain chopped from the drone.
    Stutter,
    /// Inharmonic bell ping.
    Ping,
    /// Buzzing square-wave dropout.
    Click,
    /// Bit-crushed noise burst.
    Crush,
}

pub const KINDS: [Kind; 6] =
    [Kind::Blip, Kind::Tick, Kind::Stutter, Kind::Ping, Kind::Click, Kind::Crush];

impl Kind {
    /// Display name for the setup menu.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Blip => "Blip",
            Kind::Tick => "Tick",
            Kind::Stutter => "Stutter",
            Kind::Ping => "Ping",
            Kind::Click => "Click",
            Kind::Crush => "Crush",
        }
    }
}

/// What a pattern step plays.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub kind: Kind,
    pub freq: f64,
    pub gain: f64,
    pub pan: f64,
    /// Length in samples.
    pub len: u32,
    /// Kind-specific: crush levels, stutter grain size, etc.
    pub param: f64,
    /// Retriggers within one step (1-4).
    pub ratchet: u32,
    /// The step only sounds while the layer's density is above this.
    pub threshold: f64,
    pub noise_seed: u64,
}

#[derive(Clone)]
pub struct Voice {
    ev: Event,
    age: u32,
    env: f64,
    decay: f64,
    phase: f64,
    phase2: f64,
    held: f64,
    prev: f64,
    grain_start: usize,
    gains: (f64, f64),
    noise: Rng,
}

const FADE: u32 = 24;

impl Voice {
    pub fn new(ev: Event, history: &History) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        let seconds = ev.len as f64 / sr;
        let decay = match ev.kind {
            Kind::Blip | Kind::Ping | Kind::Tick => decay_coef(seconds, sr),
            _ => 1.0,
        };
        let grain = (ev.param as usize).max(64);
        Voice {
            ev,
            age: 0,
            env: 1.0,
            decay,
            phase: 0.0,
            phase2: 0.0,
            held: 0.0,
            prev: 0.0,
            grain_start: history.pos + HISTORY_LEN - grain - (ev.noise_seed % 9000) as usize,
            gains: pan_gains(ev.pan),
            noise: Rng::new(ev.noise_seed),
        }
    }

    pub fn active(&self) -> bool {
        self.age < self.ev.len
    }

    pub fn event(&self) -> &Event {
        &self.ev
    }

    /// How far through its sound this voice is, 0..1.
    pub fn progress(&self) -> f64 {
        self.age as f64 / self.ev.len as f64
    }

    pub fn next(&mut self, history: &History) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let ev = &self.ev;
        // Short linear fades on gated kinds so the stream clicks only on purpose.
        let gate = {
            let to_end = ev.len - self.age;
            (self.age.min(to_end).min(FADE) as f64) / FADE as f64
        };
        let (l, r) = match ev.kind {
            Kind::Blip => {
                let hold = ev.param as u32;
                if self.age % hold.max(1) == 0 {
                    let x = sin_turns(self.phase + 0.35 * sin_turns(self.phase2));
                    let levels = 3.0 + (ev.param % 7.0);
                    self.held = (x * levels).round() / levels;
                }
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                self.phase2 += ev.freq * 3.0 / sr;
                self.phase2 -= self.phase2.floor();
                let s = self.held * self.env * gate;
                (s, s)
            }
            Kind::Tick => {
                let n = self.noise.bipolar();
                let s = (n - self.prev) * 0.5 * self.env;
                self.prev = n;
                (s, s)
            }
            Kind::Stutter => {
                let grain = (ev.param as u32).max(64);
                let pos = self.age % grain;
                let edge = pos.min(grain - pos).min(FADE) as f64 / FADE as f64;
                let reverse = ev.noise_seed % 2 == 0;
                let offset = if reverse { grain - 1 - pos } else { pos };
                let (gl, gr) = history.get(self.grain_start + offset as usize);
                (gl * 2.5 * edge * gate, gr * 2.5 * edge * gate)
            }
            Kind::Ping => {
                let s = 0.7 * sin_turns(self.phase) + 0.3 * sin_turns(self.phase2);
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                let ratio = if ev.param > 0.0 { ev.param } else { 2.756 };
                self.phase2 += ev.freq * ratio / sr;
                self.phase2 -= self.phase2.floor();
                let attack = (self.age.min(96) as f64) / 96.0;
                let s = s * self.env * attack;
                (s, s)
            }
            Kind::Click => {
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                let s = if self.phase < 0.5 { 0.6 } else { -0.6 } * gate;
                (s, s)
            }
            Kind::Crush => {
                let hold = (ev.param as u32).max(1);
                if self.age % hold == 0 {
                    self.held = (self.noise.bipolar() * 3.0).round() / 3.0;
                }
                let s = self.held * 0.5 * gate;
                (s, s)
            }
        };
        self.env *= self.decay;
        self.age += 1;
        (l * ev.gain * self.gains.0, r * ev.gain * self.gains.1)
    }
}

}
// ---- src/audio/glitch_setup.rs ----
pub mod glitch_setup {
//! Per-layer glitch controls: which kinds play, and how often.
//!
//! The two glitch layers used to be hard-coded by `layer1`/`layer2` in
//! `track.rs`. They are now derived from [`GlitchSetup`], whose defaults
//! reproduce those constants *exactly* — same values, same order, so `Layer`
//! draws the same random numbers and the coupled audio is unchanged. Only
//! moving a setting changes the sound (`HC004`).
//!
//! Two words carry each layer: [`word_a`] holds what plays (kinds, fill, rate,
//! steps, ratchet) and [`word_b`] holds levels and the mutation cadence.

use crate::harmony::Harmony;
use crate::pattern::LayerConfig;
use crate::tempo::Tempo;

/// Step-length multipliers, indexed by [`GlitchSetup::rate`]. A larger
/// multiplier means longer steps, so the layer fires less often.
pub const RATES: [u32; 4] = [1, 2, 4, 8];

pub const MIN_FILL: u8 = 0;
pub const MAX_FILL: u8 = 100;
pub const MIN_STEPS: u8 = 1;
pub const MAX_STEPS: u8 = 24;
pub const MIN_REPEATS: u8 = 1;
pub const MAX_REPEATS: u8 = 24;
pub const MIN_RATCHET: u8 = 1;
pub const MAX_RATCHET: u8 = 8;
pub const MIN_GAIN: u8 = 0;
pub const MAX_GAIN: u8 = 200;

/// The parts of a layer the setup scales rather than replaces: the per-kind
/// weights, the output range and the pitched span.
struct Fixture {
    /// Relative weight of each `crate::glitch::KINDS` entry when the kind is switched on.
    weights: [f64; 6],
    /// The layer's original gain range; [`GlitchSetup::gain`] scales it.
    gain: (f64, f64),
    pan_width: f64,
    /// Lowest and highest MIDI note, relative to the key.
    notes: (f64, f64),
}

const FIXTURES: [Fixture; 2] = [
    Fixture { weights: [1.0, 3.0, 2.0, 0.3, 2.0, 1.5], gain: (0.25, 0.6), pan_width: 0.9, notes: (84.0, 97.0) },
    Fixture { weights: [3.0, 0.5, 2.0, 2.5, 0.5, 0.5], gain: (0.18, 0.45), pan_width: 0.6, notes: (60.0, 81.0) },
];

/// One glitch layer's creative settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlitchSetup {
    /// When off, the layer keeps evolving but is muted.
    pub enabled: bool,
    /// One flag per entry of `crate::glitch::KINDS`, in that order.
    pub kinds: [bool; 6],
    /// Chance a step fires, percent: the layer's base "how often".
    pub fill: u8,
    /// Index into [`RATES`].
    pub rate: u8,
    /// Output level, percent of the layer's original gain.
    pub gain: u8,
    /// Pattern length, in steps.
    pub steps: u8,
    /// How many loops the pattern holds before it mutates.
    pub repeats_min: u8,
    pub repeats_max: u8,
    /// Retriggers within one step.
    pub ratchet: u8,
}

impl GlitchSetup {
    /// Layer 1 as it has always sounded.
    pub const DEFAULT_0: GlitchSetup = GlitchSetup {
        enabled: true,
        kinds: [true; 6],
        fill: 55,
        rate: 0,
        gain: 100,
        steps: 7,
        repeats_min: 4,
        repeats_max: 9,
        ratchet: 4,
    };

    /// Layer 2 as it has always sounded.
    pub const DEFAULT_1: GlitchSetup = GlitchSetup {
        enabled: true,
        kinds: [true; 6],
        fill: 45,
        rate: 1,
        gain: 100,
        steps: 11,
        repeats_min: 3,
        repeats_max: 7,
        ratchet: 3,
    };

    pub const DEFAULT: [GlitchSetup; 2] = [Self::DEFAULT_0, Self::DEFAULT_1];

    /// How many kinds are switched on.
    pub fn kinds_on(&self) -> usize {
        self.kinds.iter().filter(|k| **k).count()
    }

    /// The audio layer this setup describes. Every clamp here matches the one
    /// the setup menu applies, so what you hear is what you set.
    pub fn layer_config(self, which: usize, harmony: &Harmony, tempo: Tempo) -> LayerConfig {
        let fixture = &FIXTURES[which.min(FIXTURES.len() - 1)];
        let weights = core::array::from_fn(|i| if self.kinds[i] { fixture.weights[i] } else { 0.0 });
        let level = self.gain.min(MAX_GAIN) as f64 / 100.0;
        let (lo, hi) = (self.repeats_min.clamp(MIN_REPEATS, MAX_REPEATS), self.repeats_max.clamp(MIN_REPEATS, MAX_REPEATS));
        LayerConfig {
            steps: self.steps.clamp(MIN_STEPS, MAX_STEPS) as usize,
            step_len: tempo.glitch16() * RATES[(self.rate as usize).min(RATES.len() - 1)] as u64,
            weights,
            notes: if harmony.garage {
                // Over moving (and borrowed) chords, only the key's root and
                // fifth: the notes the progressions keep coming back to.
                let iv = harmony.scale.intervals();
                let safe: Vec<u8> = [0u8, 7].into_iter().filter(|i| iv.contains(i)).collect();
                let notes: Vec<f64> = harmony
                    .scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
                    .into_iter()
                    .filter(|m| safe.contains(&((*m as i64 - harmony.key as i64).rem_euclid(12) as u8)))
                    .collect();
                if notes.is_empty() {
                    harmony.scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
                } else {
                    notes
                }
            } else {
                harmony.scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
            },
            fill: self.fill.min(MAX_FILL) as f64 / 100.0,
            gain: (fixture.gain.0 * level, fixture.gain.1 * level),
            pan_width: fixture.pan_width,
            repeats: (lo.min(hi) as u64, lo.max(hi) as u64),
            max_ratchet: self.ratchet.clamp(MIN_RATCHET, MAX_RATCHET) as u64,
            ping_ratio: if harmony.garage { 3.0 } else { 0.0 },
            swing_push: harmony.swing_push,
            sixteenth: harmony.sixteenth,
            len_scale: 1.0,
        }
    }

    /// What plays: enabled, kinds, fill, rate, steps, ratchet.
    pub fn word_a(self) -> u32 {
        let kinds = self.kinds.iter().enumerate().fold(0u32, |acc, (i, on)| acc | ((*on as u32) << i));
        (self.enabled as u32)
            | ((kinds & 0x3f) << 1)
            | ((self.fill.min(MAX_FILL) as u32) << 7)
            | (((self.rate as u32) & 0x7) << 14)
            | (((self.steps.clamp(MIN_STEPS, MAX_STEPS) - MIN_STEPS) as u32 & 0x1f) << 17)
            | (((self.ratchet.clamp(MIN_RATCHET, MAX_RATCHET) - MIN_RATCHET) as u32 & 0x7) << 22)
    }

    /// How loud, and how often the pattern mutates.
    pub fn word_b(self) -> u32 {
        (self.gain.min(MAX_GAIN) as u32)
            | (((self.repeats_min.clamp(MIN_REPEATS, MAX_REPEATS) - MIN_REPEATS) as u32 & 0x1f) << 8)
            | (((self.repeats_max.clamp(MIN_REPEATS, MAX_REPEATS) - MIN_REPEATS) as u32 & 0x1f) << 13)
    }

    /// Rebuild from a word pair, clamping anything corrupt.
    pub fn from_words(a: u32, b: u32) -> Self {
        let kinds = core::array::from_fn(|i| (a >> (1 + i)) & 1 != 0);
        GlitchSetup {
            enabled: a & 1 != 0,
            kinds,
            fill: ((a >> 7) & 0x7f).min(MAX_FILL as u32) as u8,
            rate: ((a >> 14) & 0x7).min((RATES.len() - 1) as u32) as u8,
            gain: (b & 0xff).min(MAX_GAIN as u32) as u8,
            steps: MIN_STEPS + ((a >> 17) & 0x1f).min((MAX_STEPS - MIN_STEPS) as u32) as u8,
            repeats_min: MIN_REPEATS + ((b >> 8) & 0x1f).min((MAX_REPEATS - MIN_REPEATS) as u32) as u8,
            repeats_max: MIN_REPEATS + ((b >> 13) & 0x1f).min((MAX_REPEATS - MIN_REPEATS) as u32) as u8,
            ratchet: MIN_RATCHET + ((a >> 22) & 0x7).min((MAX_RATCHET - MIN_RATCHET) as u32) as u8,
        }
    }

    /// One line for the recipe card and the menu.
    pub fn summary(self) -> String {
        if !self.enabled {
            return "off".to_string();
        }
        format!("{} kinds · {}% · ×{} · {} steps", self.kinds_on(), self.fill, RATES[(self.rate as usize).min(3)], self.steps)
    }
}



}
// ---- src/audio/grains.rs ----
pub mod grains {
//! Microsound's grain cloud: the sustained bed (the atmosphere loops and the
//! drone) is listened to, and scattered back as grains — 20–80 ms slices of
//! the last second and a half, each under a smooth window, now and then an
//! octave up or down, every one at its own place in the field. Dense, it
//! becomes a shimmering cloud; sparse, a few motes of the bed.
//!
//! The next grain's time, length, source and pan are drawn from a stream
//! keyed by the grain's own number and placed on the exact sample, so the
//! cloud never depends on how the audio is cut into blocks.

use crate::math::{pan_gains, sin_turns};
use crate::rng::Rng;

/// The listening buffer: 1.5 s.
const HISTORY: usize = 72_000;
const VOICES: usize = 24;

#[derive(Clone, Copy, Default)]
struct Grain {
    /// Read position in the history (fractional), and its step per sample.
    read: f64,
    step: f64,
    pos: u64,
    len: u64,
    gl: f64,
    gr: f64,
    on: bool,
}

pub struct Cloud {
    seed: u64,
    hist_l: Vec<f64>,
    hist_r: Vec<f64>,
    write: usize,
    /// Samples written so far (the clock the grains are placed on).
    n: u64,
    /// When the next grain starts, and its number.
    next_at: u64,
    count: u64,
    /// Mean gap between grains, in samples.
    gap: f64,
    voices: [Grain; VOICES],
}

impl Cloud {
    /// `amount` 0 (sparse) .. 1 (dense).
    pub fn new(seed: u64, amount: f64) -> Self {
        let gap = 0.12 - 0.1 * amount.clamp(0.0, 1.0);
        Cloud {
            seed,
            hist_l: vec![0.0; HISTORY],
            hist_r: vec![0.0; HISTORY],
            write: 0,
            n: 0,
            next_at: 4_800,
            count: 0,
            gap: gap * crate::SAMPLE_RATE as f64,
            voices: [Grain::default(); VOICES],
        }
    }

    fn read(buf: &[f64], at: f64) -> f64 {
        let i = at.floor();
        let f = at - i;
        let i = (i as usize) % HISTORY;
        let j = (i + 1) % HISTORY;
        buf[i] + (buf[j] - buf[i]) * f
    }

    /// Listen to one stereo sample of the bed, and return the cloud's.
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        self.hist_l[self.write] = l;
        self.hist_r[self.write] = r;
        if self.n == self.next_at {
            let mut rng = Rng::stream(self.seed, 0x6A41_0000 ^ self.count);
            self.count += 1;
            let len = (rng.range(0.02, 0.08) * sr) as u64;
            let step = match rng.weighted(&[0.7, 0.15, 0.15]) {
                0 => 1.0,
                1 => 2.0,
                _ => 0.5,
            };
            // Far enough back that even a doubled-speed grain never reaches
            // the write head.
            let back = rng.range(0.1, 1.3) * sr + len as f64 * step;
            let read = (self.write as f64 + HISTORY as f64 - back).rem_euclid(HISTORY as f64);
            let (gl, gr) = pan_gains(rng.range(-0.95, 0.95));
            let free = self.voices.iter().position(|g| !g.on);
            if let Some(k) = free {
                self.voices[k] = Grain { read, step, pos: 0, len, gl, gr, on: true };
            }
            self.next_at = self.n + 1 + (self.gap * rng.range(0.3, 1.7)) as u64;
        }
        let (mut ol, mut or) = (0.0, 0.0);
        for g in self.voices.iter_mut() {
            if !g.on {
                continue;
            }
            if g.pos >= g.len {
                g.on = false;
                continue;
            }
            let w = sin_turns(0.5 * g.pos as f64 / g.len as f64);
            let w = w * w;
            // The grain is heard from both channels of the bed, summed, then placed.
            let s = 0.5 * (Self::read(&self.hist_l, g.read) + Self::read(&self.hist_r, g.read)) * w;
            g.read = (g.read + g.step).rem_euclid(HISTORY as f64);
            g.pos += 1;
            ol += s * g.gl;
            or += s * g.gr;
        }
        self.write = (self.write + 1) % HISTORY;
        self.n += 1;
        (ol, or)
    }
}

/// Microsound's fragment gate: a sustained sound (the drone, an atmosphere
/// loop) heard only in short pieces, in a steady rhythm locked to the bar:
/// on for a fixed number of sixteenths, off for a fixed number, the pattern
/// always fitting a bar or two exactly so every fragment starts where the
/// groove expects it. The seed picks the pattern from `patterns` and, for a
/// pattern shorter than its turn, which beat it starts on. Opening takes
/// 3 ms and closing 25 ms, so a fragment is cut cleanly rather than clicked.
/// The gate is a pure function of the sample clock, so it never depends on
/// how the audio is cut into blocks.
pub struct Gate {
    six: u64,
    /// Sixteenths on, then off.
    on: u64,
    off: u64,
    /// Where in its cycle the pattern starts, in sixteenths.
    shift: u64,
    env: f64,
}

const GATE_ATTACK: f64 = 1.0 / 144.0;
const GATE_RELEASE: f64 = 1.0 / 1200.0;

impl Gate {
    /// `patterns`: (on, off) in sixteenths; each should add up to 16 or 32.
    pub fn new(seed: u64, stream: u64, sixteenth: u64, patterns: &[(u64, u64)]) -> Self {
        let mut rng = Rng::stream(seed, stream);
        let (on, off) = rng.pick(patterns);
        // Start on a beat of the cycle (layers that share a pattern then
        // interlock instead of doubling each other).
        let beats = ((on + off) / 4).max(1);
        let shift = 4 * rng.below(beats as usize) as u64;
        Gate { six: sixteenth.max(1), on: on.max(1), off, shift, env: 0.0 }
    }

    /// (on, off) in sixteenths, for display.
    pub fn pattern(&self) -> (u64, u64) {
        (self.on, self.off)
    }

    /// Which fragment `clock` falls in (counting from the first).
    pub fn fragment(&self, clock: u64) -> u64 {
        let period = (self.on + self.off) * self.six;
        (clock + period - (self.shift * self.six) % period) / period
    }

    /// The gain at `clock` (call once per sample, in order).
    pub fn next(&mut self, clock: u64) -> f64 {
        let period = (self.on + self.off) * self.six;
        let phase = (clock + period - (self.shift * self.six) % period) % period;
        if phase < self.on * self.six {
            self.env = (self.env + GATE_ATTACK).min(1.0);
        } else {
            self.env = (self.env - GATE_RELEASE).max(0.0);
        }
        // A smooth curve over the linear ramp.
        self.env * self.env * (3.0 - 2.0 * self.env)
    }
}

/// A plain delay-line pitch shifter: two read heads sweep through a 50 ms
/// window at the shifted speed, each faded in and out so one is always
/// smooth while the other jumps back. On a still, frozen drone it is clean;
/// at no shift it passes the sound through untouched.
pub struct Shifter {
    buf: [Vec<f64>; 2],
    write: usize,
    phase: f64,
    ratio: f64,
}

const SHIFT_BUF: usize = 4096;
const SHIFT_WINDOW: f64 = 2400.0;

impl Shifter {
    pub fn new() -> Self {
        Shifter { buf: [vec![0.0; SHIFT_BUF], vec![0.0; SHIFT_BUF]], write: 0, phase: 0.0, ratio: 1.0 }
    }

    /// Transpose by `semis` from now on.
    pub fn set(&mut self, semis: f64) {
        self.ratio = crate::math::exp2(semis / 12.0);
    }

    fn tap(&self, ch: usize, delay: f64) -> f64 {
        let at = self.write as f64 + SHIFT_BUF as f64 - delay;
        let i = at.floor();
        let f = at - i;
        let i = i as usize % SHIFT_BUF;
        let b = &self.buf[ch];
        b[i] + (b[(i + 1) % SHIFT_BUF] - b[i]) * f
    }

    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.buf[0][self.write] = l;
        self.buf[1][self.write] = r;
        let out = if self.ratio == 1.0 {
            (l, r)
        } else {
            let p1 = self.phase;
            let p2 = (p1 + 0.5) % 1.0;
            let (d1, d2) = (1.0 + p1 * SHIFT_WINDOW, 1.0 + p2 * SHIFT_WINDOW);
            // sin² windows half a cycle apart always sum to one.
            let g1 = sin_turns(0.5 * p1);
            let g2 = sin_turns(0.5 * p2);
            let (g1, g2) = (g1 * g1, g2 * g2);
            (self.tap(0, d1) * g1 + self.tap(0, d2) * g2, self.tap(1, d1) * g1 + self.tap(1, d2) * g2)
        };
        // A higher pitch reads faster than it writes, so the delay shrinks.
        self.phase = (self.phase + (1.0 - self.ratio) / SHIFT_WINDOW).rem_euclid(1.0);
        self.write = (self.write + 1) % SHIFT_BUF;
        out
    }
}

impl Default for Shifter {
    fn default() -> Self {
        Shifter::new()
    }
}

}
// ---- src/audio/harmony.rs ----
pub mod harmony {
//! Harmony: the scale, the key, and a weighted progression of 1–4 complex,
//! drone-friendly chords, all chosen from the settings and seed 1.

use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Lydian,
    Ionian,
    Dorian,
    Phrygian,
    Aeolian,
    Mixolydian,
    HarmonicMinor,
    MelodicMinor,
    WholeTone,
    Hirajoshi,
    PentatonicMinor,
    DoubleHarmonic,
    /// The major pentatonic: bright Japanese pop's "no fourth, no seventh" scale.
    Yonanuki,
    /// The Okinawan scale: major, without the second and sixth.
    Ryukyu,
}

pub const SCALES: [Scale; 14] = [
    Scale::Lydian,
    Scale::Ionian,
    Scale::Dorian,
    Scale::Phrygian,
    Scale::Aeolian,
    Scale::Mixolydian,
    Scale::HarmonicMinor,
    Scale::MelodicMinor,
    Scale::WholeTone,
    Scale::Hirajoshi,
    Scale::PentatonicMinor,
    Scale::DoubleHarmonic,
    Scale::Yonanuki,
    Scale::Ryukyu,
];

impl Scale {
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Ionian => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::WholeTone => &[0, 2, 4, 6, 8, 10],
            Scale::Hirajoshi => &[0, 2, 3, 7, 8],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::DoubleHarmonic => &[0, 1, 4, 5, 7, 8, 11],
            Scale::Yonanuki => &[0, 2, 4, 7, 9],
            Scale::Ryukyu => &[0, 4, 5, 7, 11],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scale::Lydian => "Lydian",
            Scale::Ionian => "Ionian (major)",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Aeolian => "Aeolian (natural minor)",
            Scale::Mixolydian => "Mixolydian",
            Scale::HarmonicMinor => "Harmonic minor",
            Scale::MelodicMinor => "Melodic minor",
            Scale::WholeTone => "Whole tone",
            Scale::Hirajoshi => "Hirajoshi",
            Scale::PentatonicMinor => "Pentatonic minor",
            Scale::DoubleHarmonic => "Double harmonic",
            Scale::Yonanuki => "Major pentatonic (yonanuki)",
            Scale::Ryukyu => "Ryukyu",
        }
    }

    /// A one-line character sketch, for the setup screen.
    pub fn mood(self) -> &'static str {
        match self {
            Scale::Lydian => "bright, floating, unresolved (raised 4th)",
            Scale::Ionian => "open and consonant",
            Scale::Dorian => "minor but warm, a hopeful 6th",
            Scale::Phrygian => "dark, with a tense flat 2nd",
            Scale::Aeolian => "plain melancholy minor",
            Scale::Mixolydian => "major with a relaxed flat 7th",
            Scale::HarmonicMinor => "minor with a dramatic leading tone",
            Scale::MelodicMinor => "minor below, major above: misty",
            Scale::WholeTone => "dreamlike, no gravity at all",
            Scale::Hirajoshi => "Japanese pentatonic, sparse and stark",
            Scale::PentatonicMinor => "five notes, nothing can clash",
            Scale::DoubleHarmonic => "exotic, two augmented seconds",
            Scale::Yonanuki => "bright and sunny, five notes, no tension",
            Scale::Ryukyu => "island-bright, major with a gentle lift",
        }
    }

    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            Scale::Lydian => "LYD",
            Scale::Ionian => "ION",
            Scale::Dorian => "DOR",
            Scale::Phrygian => "PHR",
            Scale::Aeolian => "AEO",
            Scale::Mixolydian => "MIX",
            Scale::HarmonicMinor => "HMI",
            Scale::MelodicMinor => "MMI",
            Scale::WholeTone => "WHO",
            Scale::Hirajoshi => "HIR",
            Scale::PentatonicMinor => "PMI",
            Scale::DoubleHarmonic => "DBH",
            Scale::Yonanuki => "YON",
            Scale::Ryukyu => "RYU",
        }
    }

    pub fn from_code(code: &str) -> Option<Scale> {
        SCALES.iter().copied().find(|s| s.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        SCALES.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// A major-third scale (else minor), for choosing garage progressions.
    pub fn is_major(self) -> bool {
        self.intervals().contains(&4) && !self.intervals().contains(&3)
    }

    pub fn from_index(i: usize) -> Scale {
        SCALES[i % SCALES.len()]
    }
}

/// How long a bar is, for chord changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Four half-time beats per bar (≈2.86 s at the default 168 BPM): slow, drone-like changes.
    HalfTime,
    /// Bars at the drum tempo (≈1.43 s at 168 BPM), the drums' own bar.
    Jungle,
    /// 2-step garage: chords change every two drum bars (as half-time), and
    /// the drums, bass and stabs play the garage way.
    Garage,
    /// Microsound: clicks, grains and sine pips on a faint pulse; chords
    /// change slowly (as half-time), everything plays small and exact.
    Micro,
}

/// The modes, in setup order.
pub const PACES: [Pace; 4] = [Pace::HalfTime, Pace::Jungle, Pace::Garage, Pace::Micro];

impl Pace {
    pub fn name(self) -> &'static str {
        match self {
            Pace::HalfTime => "half-time",
            Pace::Jungle => "jungle",
            Pace::Garage => "2-step garage",
            Pace::Micro => "microsound",
        }
    }

    /// A one-line sketch, for the setup screen and the Learn page.
    pub fn blurb(self) -> &'static str {
        match self {
            Pace::HalfTime => "slow chords and a half-speed break: drifting, spacious",
            Pace::Jungle => "the chopped break at full speed under the slow bed",
            Pace::Garage => "swung 2-step drums, a bassline and chord stabs: bright, melodic",
            Pace::Micro => "clicks, grains and sine pips on a faint, exact pulse",
        }
    }

    /// The mode called `name` (case and spacing ignored; "garage" is enough).
    pub fn from_name(name: &str) -> Option<Pace> {
        let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect::<String>();
        let want = key(name);
        PACES.into_iter().find(|p| key(p.name()) == want || (want == "garage" && *p == Pace::Garage) || (want == "micro" && *p == Pace::Micro))
    }

    /// Recipe value.
    pub fn index(self) -> u32 {
        match self {
            Pace::HalfTime => 0,
            Pace::Jungle => 1,
            Pace::Garage => 2,
            Pace::Micro => 3,
        }
    }

    /// A bar's length, given one half-time beat (`Tempo::beat`) in samples.
    pub fn bar_samples(self, beat: u64) -> u64 {
        match self {
            Pace::HalfTime => 4 * beat,
            Pace::Jungle => 2 * beat,
            Pace::Garage => 4 * beat,
            Pace::Micro => 4 * beat,
        }
    }
}

/// Bars per chord, by how many chords there are. Chords 1 and 2 carry most of
/// the 32-bar cycle.
pub fn bar_weights(chords: usize) -> &'static [u32] {
    match chords {
        0 | 1 => &[32],
        2 => &[20, 12],
        3 => &[16, 12, 4],
        _ => &[14, 10, 4, 4],
    }
}

/// Voicing shapes, in scale steps above the chord's degree (7-note scales).
/// The first four are the four 0009 voicings of D lydian, so the default
/// settings reproduce that chord exactly; the last two are a stacked-fourths
/// (quartal) cloud and a wide open-thirds spread.
const SHAPES: [&[i32]; 6] = [
    &[0, 4, 8, 9, 13, 17, 19],
    &[0, 4, 6, 8, 10, 12, 16],
    &[-3, 1, 4, 8, 9, 12, 13, 17],
    &[0, 2, 6, 8, 10, 13, 15, 20],
    &[0, 3, 6, 9, 12, 15, 18],
    &[0, 4, 9, 11, 13, 16, 18, 22],
];

/// Seed 1 picks the key; the mapping is rotated so the canonical seed is D.
pub fn key_for(seed: u64) -> u8 {
    let pick = |s: u64| (Rng::stream(s, 0x4B).next_u64() % 12) as i64;
    let d = 2i64;
    (d + pick(seed) - pick(crate::DEFAULT_SEEDS.s1)).rem_euclid(12) as u8
}

#[derive(Clone, Debug)]
pub struct Chord {
    /// Scale degree (0 = tonic).
    pub degree: usize,
    /// MIDI notes, low to high.
    pub notes: Vec<f64>,
    /// The bass note (MIDI) for this chord.
    pub bass: f64,
    pub bars: u32,
    /// Its pitch classes (0-11), root first: the chord the stabs voice.
    pub tones: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Harmony {
    /// How long each change's crossfade lasts (samples): a whole bar for
    /// the slow modes, a single beat in 2-step garage.
    morph_len: u64,
    /// 2-step garage: which chord plays in each two-bar step of the cycle
    /// (the progression, then — with the key lift — its lifted copy). Empty
    /// for the slow modes, which weight their chords' lengths instead.
    sequence: Vec<usize>,
    /// Chords in one pass of the progression (before any lift).
    pub loop_len: usize,
    /// 2-step garage: how late a swung sixteenth is pushed (samples) and how
    /// long a sixteenth is, so the loops and glitches can swing with the
    /// drums. Zero for the slow modes.
    pub swing_push: u64,
    pub sixteenth: u64,
    /// 2-step garage: plain chords, so the timbres keep to whole-number
    /// (harmonic) overtones and the glitch pitches to the safest notes.
    pub garage: bool,
    pub scale: Scale,
    /// Pitch class of the tonic, 0 = C.
    pub key: u8,
    pub chords: Vec<Chord>,
    pub bar_samples: u64,
    /// Samples in one full trip through every chord.
    pub cycle: u64,
}

/// Where the progression is at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct Position {
    pub chord: usize,
    pub next: usize,
    /// Bar within the current chord (0-based).
    pub bar: u32,
    /// 0 → fully on `chord`, 1 → fully on `next` (during the last bar only).
    pub morph: f64,
}

impl Harmony {
    /// `first` is the drone's seed-1 stream: chord 1's voicing is the first
    /// draw from it, exactly as in 0009, so the default reproduces that chord.
    /// Everything else comes from `rest`, leaving `first` for chord 1's FM recipes.
    pub fn new(
        scale: Scale,
        chords: usize,
        pace: Pace,
        beat: u64,
        key: u8,
        first: &mut Rng,
        rest: &mut Rng,
    ) -> Harmony {
        let iv = scale.intervals();
        let n = iv.len() as i32;
        let base = 48 + key as i32; // tonic in octave 3 (D3 = 50)
        let note = |step: i32| -> f64 {
            let oct = step.div_euclid(n);
            (base + 12 * oct + iv[step.rem_euclid(n) as usize] as i32) as f64
        };
        let weights = bar_weights(chords);
        // Degrees for chords after the first: strong ones are more likely.
        let pool: Vec<(usize, f64)> = if n == 7 {
            vec![(3, 3.0), (4, 3.0), (5, 3.0), (1, 2.0), (2, 1.0), (6, 1.0)]
        } else {
            (1..n as usize).map(|d| (d, 1.0)).collect()
        };
        let mut out: Vec<Chord> = Vec::new();
        for (i, bars) in weights.iter().enumerate() {
            let shape = if i == 0 { SHAPES[first.below(4)] } else { SHAPES[rest.below(SHAPES.len())] };
            let degree = if i == 0 {
                0
            } else {
                let prev = out[i - 1].degree;
                let choices: Vec<(usize, f64)> = pool.iter().copied().filter(|(d, _)| *d != prev).collect();
                let w: Vec<f64> = choices.iter().map(|c| c.1).collect();
                choices[rest.weighted(&w)].0
            };
            // Shapes are written for 7-note scales; stretch steps for others.
            let mut notes: Vec<f64> = shape
                .iter()
                .map(|s| {
                    let s = if n == 7 { *s } else { (*s as f64 * n as f64 / 7.0).round() as i32 };
                    note(degree as i32 + s)
                })
                .collect();
            // Later chords sit a little lower if their degree pushes them up.
            let shift = if notes[0] > 55.0 { -12.0 } else { 0.0 };
            for x in notes.iter_mut() {
                *x += shift;
                while *x > 90.0 {
                    *x -= 12.0;
                }
            }
            notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
            notes.dedup();
            let mut bass = (36 + key as i32 + iv[degree] as i32) as f64;
            if bass > 45.0 {
                bass -= 12.0;
            }
            let tones = [0usize, 2, 4, 6].iter().map(|s| ((key as usize + iv[(degree + s) % n as usize] as usize) % 12) as u8).collect();
            out.push(Chord { degree, notes, bass, bars: *bars, tones });
        }
        let bar_samples = pace.bar_samples(beat);
        let cycle = weights.iter().map(|b| *b as u64).sum::<u64>() * bar_samples;
        let loop_len = out.len();
        Harmony { morph_len: bar_samples, sequence: Vec::new(), loop_len, swing_push: 0, sixteenth: 0, garage: false, scale, key, chords: out, bar_samples, cycle }
    }

    /// 2-step garage's drone: the tonic as an open fifth (root, fifth,
    /// octave, fifth above) — no third, no colour notes — so it sits under
    /// every chord of the progression without rubbing against it.
    pub fn pedal(scale: Scale, beat: u64, key: u8) -> Harmony {
        let iv = scale.intervals();
        let fifth = if iv.len() == 7 { iv[4] as f64 } else { 7.0 };
        let root = (48 + key as i32) as f64;
        let notes = vec![root, root + fifth, root + 12.0, root + 12.0 + fifth];
        let mut bass = (36 + key as i32) as f64;
        if bass > 45.0 {
            bass -= 12.0;
        }
        let bar_samples = Pace::Garage.bar_samples(beat);
        Harmony {
            morph_len: bar_samples,
            garage: true,
            scale,
            key,
            chords: vec![Chord { degree: 0, notes, bass, bars: 32, tones: vec![key % 12, (key + fifth as u8) % 12] }],
            sequence: Vec::new(),
            loop_len: 1,
            swing_push: 0,
            sixteenth: 0,
            bar_samples,
            cycle: 32 * bar_samples,
        }
    }

    /// 2-step garage's harmony, each chord two drum bars long and changing
    /// on the downbeat (the loops crossfade over just the last beat).
    ///
    /// With four chords it plays one of the named [`PROGRESSIONS`] — chosen
    /// with `progression` (1-based), or by the seed from those of the scale's
    /// family when it is 0 — with chords borrowed from outside the key where
    /// the progression asks (secondary dominants, borrowed iv, ♭VII). With
    /// fewer chords it keeps to the scale's own. `lift` > 0 steps the whole
    /// progression up a whole tone every `lift` passes, then home again.
    pub fn garage(scale: Scale, chords: usize, beat: u64, key: u8, rng: &mut Rng, progression: usize, lift: usize) -> Harmony {
        let iv = scale.intervals();
        let n = iv.len();
        let major = scale.is_major();
        let k = chords.clamp(1, 4);
        let shape: Vec<(u8, Quality)> = if k == 4 {
            let pick = if progression >= 1 && progression <= PROGRESSIONS.len() {
                &PROGRESSIONS[progression - 1]
            } else {
                let family: Vec<&Progression> = PROGRESSIONS.iter().filter(|p| p.major == major).collect();
                family[rng.below(family.len())]
            };
            pick.chords.to_vec()
        } else {
            // Short loops: the scale's own chords.
            let pool: &[&[usize]] = match (major, k) {
                (_, 1) => &[&[0]],
                (true, 2) => &[&[0, 3], &[0, 4]],
                (true, _) => &[&[0, 3, 4], &[5, 3, 4]],
                (false, 2) => &[&[0, 3], &[0, 6]],
                (false, _) => &[&[0, 3, 6], &[0, 6, 3]],
            };
            let degrees = pool[rng.below(pool.len())];
            degrees.iter().map(|d| diatonic(iv, d % n)).collect()
        };
        let mut out: Vec<Chord> = shape.iter().map(|&(root, q)| garage_chord(iv, key, root, q)).collect();
        let loop_len = out.len();
        let mut sequence: Vec<usize> = (0..loop_len).collect();
        if lift > 0 {
            // The lifted copy, a whole tone up, after `lift` passes at home.
            out.extend(shape.iter().map(|&(root, q)| garage_chord(iv, key, (root + 2) % 12, q)).map(|mut c| {
                // Keep the lifted bass and shells in the same registers.
                if c.bass > 45.0 {
                    c.bass -= 12.0;
                }
                c
            }));
            sequence = (0..lift).flat_map(|_| 0..loop_len).chain((0..lift).flat_map(|_| loop_len..2 * loop_len)).collect();
        }
        let bar_samples = Pace::Garage.bar_samples(beat);
        let cycle = sequence.len() as u64 * bar_samples;
        Harmony { morph_len: bar_samples / 8, sequence, loop_len, swing_push: 0, sixteenth: 0, garage: true, scale, key, chords: out, bar_samples, cycle }
    }

    /// How the atmosphere loops blend at `t`: (from, to, 0..1). The slow
    /// modes crossfade during each chord's last bar, exactly as [`at`]. In
    /// 2-step garage the loops change *on* the downbeat with the bass and
    /// stabs: a quick crossfade over the first sixteenth of the new chord.
    ///
    /// [`at`]: Harmony::at
    pub fn loop_mix(&self, t: u64) -> (usize, usize, f64) {
        if self.sequence.is_empty() {
            let p = self.at(t);
            return (p.chord, p.next, p.morph);
        }
        let step = (t / self.bar_samples) as usize;
        let cur = self.sequence[step % self.sequence.len()];
        let fade = (self.bar_samples / 32).max(1);
        let pos = t % self.bar_samples;
        if step > 0 && pos < fade {
            let prev = self.sequence[(step - 1) % self.sequence.len()];
            if prev != cur {
                let x = pos as f64 / fade as f64;
                return (prev, cur, x * x * (3.0 - 2.0 * x));
            }
        }
        (cur, cur, 0.0)
    }

    /// Whether chord `i` is the first of a pass through the progression.
    pub fn starts_pass(&self, i: usize) -> bool {
        i % self.loop_len.max(1) == 0
    }

    /// Whether chord `i` has a tone a semitone from the tonic or its fifth
    /// (the garage drone's pedal): the drone steps back under it.
    pub fn rubs_pedal(&self, i: usize) -> bool {
        let iv = self.scale.intervals();
        let fifth = if iv.contains(&7) { 7 } else { iv.get(iv.len() / 2).copied().unwrap_or(7) };
        let pedal = [self.key % 12, (self.key + fifth) % 12];
        self.chords[i].tones.iter().any(|t| pedal.iter().any(|p| (12 + *t as i32 - *p as i32) % 12 == 1 || (12 + *p as i32 - *t as i32) % 12 == 1))
    }

    /// The progression at sample `t`. The last bar of each chord morphs into the next.
    /// The same chords, each held for exactly `step` samples in turn (rather
    /// than weighted over the slow cycle), crossfading over the last `morph`
    /// samples of each: microsound's regular chord changes.
    pub fn stepped(mut self, step: u64, morph: u64) -> Harmony {
        let step = step.max(1);
        self.sequence = (0..self.chords.len()).collect();
        self.bar_samples = step;
        self.morph_len = morph.clamp(1, step);
        self.cycle = step * self.chords.len() as u64;
        self
    }

    pub fn at(&self, t: u64) -> Position {
        if !self.sequence.is_empty() {
            let step = (t / self.bar_samples) as usize % self.sequence.len();
            let (chord, next) = (self.sequence[step], self.sequence[(step + 1) % self.sequence.len()]);
            let pos = t % self.bar_samples;
            let morph_start = self.bar_samples - self.morph_len;
            let morph = if chord != next && pos >= morph_start {
                let x = (pos - morph_start) as f64 / self.morph_len as f64;
                x * x * (3.0 - 2.0 * x)
            } else {
                0.0
            };
            return Position { chord, next, bar: step as u32, morph };
        }
        let n = self.chords.len();
        if n == 1 {
            return Position { chord: 0, next: 0, bar: ((t / self.bar_samples) % 32) as u32, morph: 0.0 };
        }
        let mut pos = t % self.cycle;
        for (i, c) in self.chords.iter().enumerate() {
            let len = c.bars as u64 * self.bar_samples;
            if pos < len {
                let bar = (pos / self.bar_samples) as u32;
                let morph_start = len - self.morph_len;
                let morph = if pos >= morph_start {
                    let x = (pos - morph_start) as f64 / self.morph_len as f64;
                    x * x * (3.0 - 2.0 * x) // smoothstep
                } else {
                    0.0
                };
                return Position { chord: i, next: (i + 1) % n, bar, morph };
            }
            pos -= len;
        }
        Position::default()
    }

    /// Every scale tone (MIDI) from `lo` to `hi` inclusive.
    pub fn scale_notes(&self, lo: f64, hi: f64) -> Vec<f64> {
        let iv = self.scale.intervals();
        (0..128)
            .filter(|m| iv.contains(&(((m - self.key as i32).rem_euclid(12)) as u8)))
            .map(|m| m as f64)
            .filter(|m| *m >= lo && *m <= hi)
            .collect()
    }
}

/// A chord's quality, as its intervals above the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Maj7,
    Dom7,
    Min7,
    HalfDim,
}

impl Quality {
    pub fn intervals(self) -> [u8; 4] {
        match self {
            Quality::Maj7 => [0, 4, 7, 11],
            Quality::Dom7 => [0, 4, 7, 10],
            Quality::Min7 => [0, 3, 7, 10],
            Quality::HalfDim => [0, 3, 6, 10],
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Quality::Maj7 => "maj7",
            Quality::Dom7 => "7",
            Quality::Min7 => "m7",
            Quality::HalfDim => "m7b5",
        }
    }
}

/// A named garage progression: four chords as (semitones above the key, quality).
pub struct Progression {
    pub name: &'static str,
    /// How it reads in roman numerals.
    pub numerals: &'static str,
    /// Major-key (else minor-key) family, for the seed's pick.
    pub major: bool,
    pub chords: [(u8, Quality); 4],
}

use Quality::{Dom7 as D7, HalfDim as HD, Maj7 as M7, Min7 as M};

/// The 2-step garage progressions: the bright, upbeat turns of Japanese pop,
/// city pop and game menus, many with chords borrowed from outside the key.
pub const PROGRESSIONS: [Progression; 15] = [
    Progression { name: "Royal road", numerals: "IVmaj7 V7 iii7 vi7", major: true, chords: [(5, M7), (7, D7), (4, M), (9, M)] },
    Progression { name: "Royal road, bright", numerals: "IVmaj7 V7 III7 vi7", major: true, chords: [(5, M7), (7, D7), (4, D7), (9, M)] },
    Progression { name: "Soft turn", numerals: "IVmaj7 III7 vi7 I7", major: true, chords: [(5, M7), (4, D7), (9, M), (0, D7)] },
    Progression { name: "City lights", numerals: "ii7 V7 Imaj7 VI7", major: true, chords: [(2, M), (7, D7), (0, M7), (9, D7)] },
    Progression { name: "Sunrise", numerals: "IVmaj7 V7 vi7 bVIImaj7", major: true, chords: [(5, M7), (7, D7), (9, M), (10, M7)] },
    Progression { name: "Menu loop", numerals: "Imaj7 vi7 ii7 V7", major: true, chords: [(0, M7), (9, M), (2, M), (7, D7)] },
    Progression { name: "Wistful", numerals: "Imaj7 III7 IVmaj7 iv7", major: true, chords: [(0, M7), (4, D7), (5, M7), (5, M)] },
    Progression { name: "Two-five-one", numerals: "ii7 V7 Imaj7 vi7", major: true, chords: [(2, M), (7, D7), (0, M7), (9, M)] },
    Progression { name: "Pop loop", numerals: "vi7 IVmaj7 Imaj7 V7", major: true, chords: [(9, M), (5, M7), (0, M7), (7, D7)] },
    Progression { name: "Daydream", numerals: "Imaj7 II7 IVmaj7 iv7", major: true, chords: [(0, M7), (2, D7), (5, M7), (5, M)] },
    Progression { name: "Night drive", numerals: "i7 iv7 bVII7 bIIImaj7", major: false, chords: [(0, M), (5, M), (10, D7), (3, M7)] },
    Progression { name: "Neon", numerals: "i7 IV7 bVIImaj7 bIIImaj7", major: false, chords: [(0, M), (5, D7), (10, M7), (3, M7)] },
    Progression { name: "Climb", numerals: "bVImaj7 bVII7 i7 V7", major: false, chords: [(8, M7), (10, D7), (0, M), (7, D7)] },
    Progression { name: "Rain city", numerals: "i7 bVImaj7 bIIImaj7 bVII7", major: false, chords: [(0, M), (8, M7), (3, M7), (10, D7)] },
    Progression { name: "Descent", numerals: "ii7b5 V7 i7 bVImaj7", major: false, chords: [(2, HD), (7, D7), (0, M), (8, M7)] },
];

/// The scale's own seventh chord on degree `d`, as (semitones above the key, quality).
fn diatonic(iv: &[u8], d: usize) -> (u8, Quality) {
    let n = iv.len();
    let at = |k: usize| (iv[(d + k) % n] as i32 + 12 * ((d + k) / n) as i32 - iv[d] as i32) as u8;
    let (third, fifth, seventh) = (at(2), at(4), at(6));
    let q = match (third, fifth, seventh) {
        (4, _, 11) => Quality::Maj7,
        (4, _, _) => Quality::Dom7,
        (3, 6, _) => Quality::HalfDim,
        _ => Quality::Min7,
    };
    (iv[d], q)
}

/// A garage chord: its tones, the loops' open shell (root, fifth, tenth,
/// rooted C4–B4 so it opens out above the stabs), and its bass note.
fn garage_chord(iv: &[u8], key: u8, root: u8, q: Quality) -> Chord {
    let qi = q.intervals();
    let r = (key + root) % 12;
    let tones: Vec<u8> = qi.iter().map(|i| (r + i) % 12).collect();
    let mut low = 60.0 + r as f64;
    while low > 71.0 {
        low -= 12.0;
    }
    let notes = vec![low, low + qi[2] as f64, low + qi[1] as f64 + 12.0];
    let mut bass = (36 + key as i32 + root as i32) as f64;
    while bass > 45.0 {
        bass -= 12.0;
    }
    let degree = iv.iter().position(|x| *x == root).unwrap_or(0);
    Chord { degree, notes, bass, bars: 1, tones }
}

pub fn key_name(pc: u8) -> &'static str {
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][pc as usize % 12]
}

}
// ---- src/audio/kits.rs ----
pub mod kits {
//! Drum kit families. The kit is chosen at setup; seed 5 then sets every
//! voice's numbers (and the break) within that family's ranges. Every kit keeps the roles of a
//! breakbeat (kick, snare, ghost, hats, open hat, ride) so it always feels
//! like drums, but how each role is synthesised differs widely.

use crate::filter::{OnePole, Svf};
use crate::harmony::Harmony;
use crate::math::{decay_coef, exp, midi_hz, sin_turns, tanh};
use crate::rng::Rng;

const SR: f64 = crate::SAMPLE_RATE as f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kit {
    /// Synthesised acoustic funk kit, like a sampled break.
    Acoustic,
    /// Everything made by frequency modulation: rubbery kicks, clangy snares, bell hats.
    FmMetal,
    /// Resonating modes tuned to the key and scale: drums that belong to the harmony.
    Modal,
    /// Bit-crushed, sample-starved, 1-bit cymbals.
    Crush,
    /// Filtered noise only: resonant sweeps and pings.
    NoiseSculpt,
    /// Deep short sine kicks and tiny clicks: sparse and airy.
    SubClicks,
    /// Glitch blips: tiny sine thumps, crushed FM blips, noise ticks.
    MicroBlips,
    /// Bell pings tuned to the scale, soft round kicks.
    GlassPings,
    /// Vinyl-dust crackle and single-sample clicks.
    DataDust,
    /// Square beeps and dropout buzzes, like a modem.
    PulseCode,
    /// No drums at all: pure ambient.
    Off,
}

/// Where the drums sit in the stereo field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrumSpace {
    /// Mono, dead centre (how every recipe without a space letter sounds).
    Centred,
    /// Each hit panned a little: kick centred, snares 45–65% out, hats 50–75%.
    Wide,
    /// Wide, plus a ping-pong tape echo on the snare-type hits.
    Tape,
}

pub const SPACES: [DrumSpace; 3] = [DrumSpace::Centred, DrumSpace::Wide, DrumSpace::Tape];

impl DrumSpace {
    /// One letter in recipes.
    pub fn code(self) -> &'static str {
        match self {
            DrumSpace::Centred => "C",
            DrumSpace::Wide => "W",
            DrumSpace::Tape => "T",
        }
    }

    pub fn from_code(code: &str) -> Option<DrumSpace> {
        SPACES.iter().copied().find(|s| s.code().eq_ignore_ascii_case(code))
    }

    pub fn name(self) -> &'static str {
        match self {
            DrumSpace::Centred => "Centred",
            DrumSpace::Wide => "Wide",
            DrumSpace::Tape => "Wide + tape echo",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            DrumSpace::Centred => "mono, every hit dead centre",
            DrumSpace::Wide => "kick centred, snares 45–65% out, hats 50–75%",
            DrumSpace::Tape => "wide, plus a wobbly ping-pong tape echo on the snares",
        }
    }

    pub fn index(self) -> usize {
        SPACES.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// In setup order: the light, minimal kits first.
pub const KITS: [Kit; 11] = [
    Kit::SubClicks,
    Kit::MicroBlips,
    Kit::GlassPings,
    Kit::DataDust,
    Kit::PulseCode,
    Kit::Acoustic,
    Kit::FmMetal,
    Kit::Modal,
    Kit::Crush,
    Kit::NoiseSculpt,
    Kit::Off,
];

/// The original order, frozen: old recipes without a kit code map seed 5 through it.
const LEGACY_KITS: [Kit; 6] = [Kit::Acoustic, Kit::FmMetal, Kit::Modal, Kit::Crush, Kit::NoiseSculpt, Kit::SubClicks];

impl Kit {
    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            Kit::Acoustic => "ACO",
            Kit::FmMetal => "FMM",
            Kit::Modal => "MOD",
            Kit::Crush => "CRU",
            Kit::NoiseSculpt => "NOI",
            Kit::SubClicks => "SUB",
            Kit::MicroBlips => "MIC",
            Kit::GlassPings => "GLS",
            Kit::DataDust => "DST",
            Kit::PulseCode => "PCM",
            Kit::Off => "OFF",
        }
    }

    /// The light, minimal kits (gentle processing, short sounds).
    pub fn is_light(self) -> bool {
        matches!(self, Kit::SubClicks | Kit::MicroBlips | Kit::GlassPings | Kit::DataDust | Kit::PulseCode)
    }

    pub fn from_code(code: &str) -> Option<Kit> {
        KITS.iter().copied().find(|k| k.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        KITS.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// A one-line character sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Kit::Acoustic => "a synthesised funk kit, like a sampled break",
            Kit::FmMetal => "rubbery FM kicks, clangy snares, bell hats",
            Kit::Modal => "resonant drums tuned to the key and scale",
            Kit::Crush => "crushed to a few bits, 1-bit pulse hats",
            Kit::NoiseSculpt => "resonant noise sweeps and pings",
            Kit::SubClicks => "deep short sine kicks and tiny clicks",
            Kit::MicroBlips => "tiny thumps, crushed FM blips, noise ticks",
            Kit::GlassPings => "bell pings tuned to the scale, soft kicks",
            Kit::DataDust => "vinyl-dust crackle and single-sample clicks",
            Kit::PulseCode => "square beeps and dropout buzzes",
            Kit::Off => "no drums: pure ambient",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kit::Acoustic => "Acoustic break",
            Kit::FmMetal => "FM metal",
            Kit::Modal => "Modal (tuned)",
            Kit::Crush => "Digital crush",
            Kit::NoiseSculpt => "Noise sculpture",
            Kit::SubClicks => "Sub & clicks",
            Kit::MicroBlips => "Micro blips",
            Kit::GlassPings => "Glass pings",
            Kit::DataDust => "Data dust",
            Kit::PulseCode => "Pulse code",
            Kit::Off => "Off",
        }
    }
}

/// Every number a kit's voices are built from.
#[derive(Clone, Debug)]
pub struct KitParams {
    pub kit: Kit,
    /// The break's original tempo (it is sped up to 168 BPM).
    pub source_bpm: f64,
    /// Sampler colour.
    pub bits: u32,
    pub hold: usize,
    pub lp_hz: f64,
    // Kick: pitch from → to, drop time constant (s), decay (s), drive.
    pub kick_from: f64,
    pub kick_to: f64,
    pub kick_drop: f64,
    pub kick_decay: f64,
    pub kick_drive: f64,
    pub kick_fm: (f64, f64),
    // Snare: two tones, wire/noise centre, decay, drive.
    pub snare_tones: [f64; 2],
    pub snare_wire: f64,
    pub snare_decay: f64,
    pub snare_drive: f64,
    pub snare_fm: (f64, f64),
    // Cymbals: oscillator/partial frequencies, filter centre, decays, noise share.
    pub metal: [f64; 6],
    pub cymbal_centre: f64,
    pub hat_decay: f64,
    pub ride_decay: f64,
    pub noise_amt: f64,
    /// Modal kit: scale notes (Hz) the bars are tuned to.
    pub tuned: Vec<f64>,
    /// 8ths on the ride (else closed hats), and how many off-beat hats.
    pub ride_on_8ths: bool,
    pub hat_density: f64,
    /// How much of the punch chain (transient boost, drive, bus compression)
    /// to use: light kits keep only a touch of it.
    pub punch: f64,
}

/// The classic 808-style metallic oscillator bank (Hz).
const METAL: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

/// How seed 5 picked the kit before kits were chosen at setup. Only used to
/// read older recipes (without a kit code) so they still sound the same.
pub fn legacy_kit_for(seed: u64) -> Kit {
    let n = LEGACY_KITS.len() as i64;
    let pick = |s: u64| (Rng::stream(s, 0x4B17).next_u64() % n as u64) as i64;
    LEGACY_KITS[(pick(seed) - pick(crate::DEFAULT_SEEDS.s5)).rem_euclid(n) as usize]
}

impl KitParams {
    pub fn new(seed: u64, kit: Kit, harmony: &Harmony) -> KitParams {
        let mut r = Rng::stream(seed, 0x4B175);
        let metal = {
            let scale = r.range(0.9, 1.15);
            METAL.map(|f| f * scale)
        };
        let root = midi_hz(36.0 + harmony.key as f64); // C2..B2
        let tuned: Vec<f64> = harmony.scale_notes(84.0, 100.0).iter().map(|m| midi_hz(*m)).collect();
        let mut p = KitParams {
            kit,
            source_bpm: r.range(132.0, 142.0),
            bits: r.int(11, 13) as u32,
            hold: 2,
            lp_hz: r.range(9000.0, 12_000.0),
            kick_from: r.range(150.0, 190.0),
            kick_to: r.range(45.0, 55.0),
            kick_drop: r.range(0.022, 0.035),
            kick_decay: r.range(0.25, 0.4),
            kick_drive: r.range(2.0, 2.6),
            kick_fm: (1.0, 0.0),
            snare_tones: [r.range(170.0, 200.0), r.range(300.0, 360.0)],
            snare_wire: r.range(1800.0, 2600.0),
            snare_decay: r.range(0.15, 0.22),
            snare_drive: r.range(1.4, 1.8),
            snare_fm: (1.0, 0.0),
            metal,
            cymbal_centre: r.range(6500.0, 9000.0),
            hat_decay: r.range(0.035, 0.06),
            ride_decay: r.range(0.35, 0.55),
            noise_amt: r.range(0.28, 0.36),
            tuned,
            ride_on_8ths: r.chance(0.7),
            hat_density: r.range(0.25, 0.45),
            punch: 1.0,
        };
        // Shared by the light kits: a clean sampler and gentle drive.
        let light = |p: &mut KitParams, r: &mut Rng| {
            p.source_bpm = r.range(125.0, 140.0);
            p.bits = 14;
            p.hold = 1;
            p.lp_hz = r.range(14_000.0, 16_000.0);
            p.kick_drive = r.range(1.0, 1.3);
            p.snare_drive = r.range(1.0, 1.2);
            p.ride_on_8ths = r.chance(0.2);
            p.hat_density = r.range(0.2, 0.45);
            p.punch = r.range(0.2, 0.3);
        };
        match kit {
            Kit::Acoustic => {}
            Kit::FmMetal => {
                p.source_bpm = r.range(128.0, 150.0);
                p.bits = r.int(12, 14) as u32;
                p.hold = r.int(1, 2) as usize;
                p.kick_from = r.range(160.0, 220.0);
                p.kick_to = r.range(42.0, 55.0);
                p.kick_fm = (if r.chance(0.5) { 1.0 } else { 0.5 }, r.range(6.0, 10.0));
                p.snare_tones = [r.range(180.0, 260.0), 0.0];
                p.snare_fm = (r.range(1.41, 1.73), r.range(8.0, 14.0));
                p.snare_decay = r.range(0.12, 0.2);
                p.cymbal_centre = r.range(3000.0, 5000.0); // FM bell carrier
                p.metal[0] = r.range(3.7, 4.3); // bell ratio
                p.metal[1] = r.range(4.0, 8.0); // bell index
                p.ride_decay = r.range(0.4, 0.7);
            }
            Kit::Modal => {
                p.source_bpm = r.range(126.0, 140.0);
                p.bits = r.int(13, 14) as u32;
                p.hold = 1;
                p.kick_to = root;
                p.kick_from = root * 1.3;
                p.kick_decay = r.range(0.35, 0.55);
                p.kick_drive = r.range(1.2, 1.6);
                let mid = harmony.scale_notes(55.0, 66.0);
                p.snare_tones = [midi_hz(r.pick(&mid)), 0.0];
                p.snare_decay = r.range(0.18, 0.3);
                p.hat_decay = r.range(0.08, 0.16);
                p.ride_decay = r.range(0.5, 0.9);
                p.ride_on_8ths = r.chance(0.5);
            }
            Kit::Crush => {
                p.source_bpm = r.range(130.0, 150.0);
                p.bits = r.int(8, 10) as u32;
                p.hold = r.int(2, 4) as usize;
                p.lp_hz = r.range(7000.0, 10_000.0);
                p.kick_from = r.range(110.0, 150.0);
                p.kick_drive = r.range(2.5, 3.5);
                p.cymbal_centre = r.range(3000.0, 7000.0); // pulse pitch
                p.hat_decay = r.range(0.015, 0.04);
                p.ride_decay = r.range(0.2, 0.4);
            }
            Kit::NoiseSculpt => {
                p.bits = r.int(12, 14) as u32;
                p.hold = 1;
                p.kick_from = r.range(300.0, 500.0);
                p.kick_to = r.range(45.0, 60.0);
                p.kick_drop = r.range(0.03, 0.06);
                p.snare_wire = r.range(2500.0, 3500.0);
                p.cymbal_centre = r.range(6000.0, 11_000.0);
                // Resonant noise washes easily: keep the ride short and often off the 8ths.
                p.ride_decay = r.range(0.15, 0.3);
                p.ride_on_8ths = r.chance(0.4);
            }
            Kit::SubClicks => {
                light(&mut p, &mut r);
                p.kick_from = r.range(70.0, 90.0);
                p.kick_to = r.range(38.0, 45.0);
                p.kick_drop = r.range(0.04, 0.07);
                // Short enough that the drone's duck lets go quickly.
                p.kick_decay = r.range(0.28, 0.45);
                p.kick_drive = r.range(1.1, 1.4);
                p.snare_decay = r.range(0.05, 0.08);
                p.ride_on_8ths = false;
                p.hat_density = r.range(0.1, 0.25);
            }
            Kit::MicroBlips => {
                light(&mut p, &mut r);
                p.kick_from = r.range(110.0, 140.0);
                p.kick_to = r.range(55.0, 70.0);
                p.kick_drop = r.range(0.008, 0.015);
                p.kick_decay = r.range(0.05, 0.08);
                p.snare_tones = [r.range(600.0, 1400.0), 0.0]; // blip pitch
                p.snare_fm = (r.int(3, 7) as f64, r.int(2, 6) as f64); // (levels, hold)
                p.snare_decay = r.range(0.03, 0.05);
                p.hat_decay = r.range(0.002, 0.006);
                p.ride_decay = r.range(0.03, 0.06);
                p.cymbal_centre = r.range(5000.0, 9000.0);
            }
            Kit::GlassPings => {
                light(&mut p, &mut r);
                p.kick_from = r.range(70.0, 95.0);
                p.kick_to = r.range(55.0, 65.0);
                p.kick_drop = r.range(0.015, 0.025);
                p.kick_decay = r.range(0.08, 0.15);
                let bells = harmony.scale_notes(72.0, 84.0);
                p.snare_tones = [midi_hz(r.pick(&bells)), 0.0];
                p.snare_decay = r.range(0.06, 0.1);
                p.hat_decay = r.range(0.02, 0.04);
                p.ride_decay = r.range(0.08, 0.15);
            }
            Kit::DataDust => {
                light(&mut p, &mut r);
                p.kick_from = r.range(90.0, 120.0);
                p.kick_to = r.range(50.0, 60.0);
                p.kick_drop = r.range(0.01, 0.02);
                p.kick_decay = r.range(0.04, 0.07);
                p.snare_wire = r.range(1500.0, 3500.0);
                p.snare_decay = r.range(0.06, 0.12);
                p.snare_fm = (r.range(0.015, 0.04), 0.0); // crackle density
                p.hat_decay = r.range(0.004, 0.01);
                p.ride_decay = r.range(0.05, 0.1);
                p.cymbal_centre = r.range(3000.0, 7000.0);
            }
            Kit::Off => {}
            Kit::PulseCode => {
                light(&mut p, &mut r);
                p.kick_from = r.range(60.0, 90.0);
                p.kick_to = p.kick_from * 0.9;
                p.kick_drop = 0.02;
                p.kick_decay = r.range(0.04, 0.07);
                let t = r.range(400.0, 900.0);
                p.snare_tones = [t, t * r.pick(&[1.25, 1.5, 2.0])];
                p.snare_decay = r.range(0.015, 0.03);
                p.hat_decay = r.range(0.0005, 0.0015);
                p.ride_decay = r.range(0.015, 0.03);
                p.cymbal_centre = r.range(5000.0, 9000.0);
            }
        }
        p
    }

    /// One line per voice, for the TUI.
    pub fn describe(&self) -> [String; 3] {
        match self.kit {
            Kit::Acoustic => [
                format!("sine dive {:.0}→{:.0} Hz + click", self.kick_from, self.kick_to),
                format!("body {:.0}/{:.0} Hz + wires ~{:.1} kHz", self.snare_tones[0], self.snare_tones[1], self.snare_wire / 1000.0),
                "6 ring-modded squares + coloured noise".to_string(),
            ],
            Kit::FmMetal => [
                format!("FM ratio {} index {:.0}→0", self.kick_fm.0, self.kick_fm.1),
                format!("FM {:.0} Hz ×{:.2} index {:.0}", self.snare_tones[0], self.snare_fm.0, self.snare_fm.1),
                format!("FM bells ×{:.2}, index {:.0}", self.metal[0], self.metal[1]),
            ],
            Kit::Modal => [
                format!("membrane modes on {:.0} Hz (the key)", self.kick_to),
                format!("modes on {:.0} Hz, noise-struck", self.snare_tones[0]),
                "free-bar modes on scale notes".to_string(),
            ],
            Kit::Crush => [
                format!("crushed dive {:.0}→{:.0} Hz, 5 bits", self.kick_from, self.kick_to),
                "3-bit noise + square body".to_string(),
                format!("1-bit pulses ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
            Kit::NoiseSculpt => [
                format!("noise, resonant LP {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                format!("noise, band sweep from {:.1} kHz", self.snare_wire / 1000.0),
                format!("resonant noise pings ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
            Kit::SubClicks => [
                format!("short sine {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                "a click + a breath of noise".to_string(),
                "micro clicks".to_string(),
            ],
            Kit::MicroBlips => [
                format!("{:.0} ms thump {:.0}→{:.0} Hz", self.kick_decay * 1000.0, self.kick_from, self.kick_to),
                format!("crushed FM blip {:.0} Hz, {} levels", self.snare_tones[0], self.snare_fm.0),
                "noise ticks · high blips".to_string(),
            ],
            Kit::GlassPings => [
                format!("soft bump {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                format!("bell ping {:.0} Hz (1 : 2.756)", self.snare_tones[0]),
                "pings on scale notes".to_string(),
            ],
            Kit::DataDust => [
                format!("tock {:.0}→{:.0} Hz + impulse", self.kick_from, self.kick_to),
                format!("crackle burst ~{:.1} kHz", self.snare_wire / 1000.0),
                "single-sample dust".to_string(),
            ],
            Kit::Off => ["no drums".to_string(), String::new(), String::new()],
            Kit::PulseCode => [
                format!("square beep {:.0} Hz", self.kick_from),
                format!("buzz {:.0}/{:.0} Hz", self.snare_tones[0], self.snare_tones[1]),
                format!("clicks · beeps ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
        }
    }

    /// Frequencies plotted in the TUI's cymbal panel.
    pub fn cymbal_freqs(&self) -> Vec<f64> {
        match self.kit {
            Kit::Acoustic => self.metal.to_vec(),
            Kit::FmMetal => (1..=5).map(|k| self.cymbal_centre * (1.0 + k as f64 * self.metal[0] / 4.0) / 6.0).collect(),
            Kit::Modal | Kit::GlassPings => self.tuned.iter().take(6).map(|f| f / 8.0).collect(),
            _ => vec![self.cymbal_centre / 10.0],
        }
    }
}

// ---------------------------------------------------------------- shaping

/// Punch: boost the first `ms` milliseconds, then drive (with optional
/// asymmetry, which adds even harmonics for body).
fn shape(v: &mut [f64], boost: f64, ms: f64, drive: f64, asym: f64) {
    let n = (ms / 1000.0 * SR) as usize;
    let norm = tanh(drive * (1.0 + asym)) .max(1e-9);
    for (i, x) in v.iter_mut().enumerate() {
        if i < n {
            *x *= 1.0 + boost * (1.0 - i as f64 / n as f64);
        }
        *x = (tanh(drive * (*x + asym)) - tanh(drive * asym)) / norm;
    }
}

/// Feed-forward compressor over the whole break (3 ms attack, 80 ms release,
/// 4:1 above −12 dB of the peak), then an asymmetric soft clip, blended with
/// the dry break by `amount` (light kits keep only a touch). Stereo-linked:
/// one envelope and one gain for both channels, so the image never shifts
/// (with identical channels this is exactly the mono computation).
pub fn punch_bus(left: &mut [f64], right: &mut [f64], amount: f64) {
    let joint = |l: &[f64], r: &[f64]| l.iter().zip(r).fold(1e-9f64, |a, (x, y)| a.max(x.abs()).max(y.abs()));
    let dry_peak = joint(left, right);
    let dry_l: Vec<f64> = left.iter().map(|x| x / dry_peak).collect();
    let dry_r: Vec<f64> = right.iter().map(|x| x / dry_peak).collect();
    let thr = dry_peak * 0.25; // −12 dB
    let att = 1.0 - exp(-1.0 / (0.003 * SR));
    let rel = 1.0 - exp(-1.0 / (0.08 * SR));
    let mut env = 0.0f64;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let a = l.abs().max(r.abs());
        env += (if a > env { att } else { rel }) * (a - env);
        if env > thr {
            // 4:1: gain = (thr · (env/thr)^(1/4)) / env, with x^(1/4) = √√x.
            let over = env / thr;
            let g = thr * over.sqrt().sqrt() / env;
            *l *= g;
            *r *= g;
        }
    }
    let peak = joint(left, right);
    for (x, d) in left.iter_mut().zip(dry_l).chain(right.iter_mut().zip(dry_r)) {
        let v = *x / peak * 1.4;
        let wet = tanh(v + 0.06) - tanh(0.06);
        *x = d + (wet - d) * amount;
    }
}

fn crush(v: f64, levels: f64) -> f64 {
    (v * levels).round() / levels
}

// ---------------------------------------------------------------- voices

/// A falling pitch: `to + (from − to)·e^(−t/drop)`, sampled per sample.
struct Dive {
    sweep: f64,
    k: f64,
    from: f64,
    to: f64,
}

impl Dive {
    fn new(from: f64, to: f64, drop: f64) -> Self {
        Dive { sweep: 1.0, k: exp(-1.0 / (drop * SR)), from, to }
    }

    fn next(&mut self) -> f64 {
        let f = self.to + (self.from - self.to) * self.sweep;
        self.sweep *= self.k;
        f
    }
}

pub fn kick(p: &KitParams, vel: f64, rng: &mut Rng) -> Vec<f64> {
    let len = ((p.kick_decay * 1.4).min(1.5) * SR) as usize;
    let amp_k = decay_coef(p.kick_decay, SR);
    let mut dive = Dive::new(p.kick_from, p.kick_to, p.kick_drop);
    let (mut amp, mut ph, mut mph) = (1.0, 0.0, 0.0);
    let mut lp = Svf::new(p.kick_from * 2.0, 6.0, SR);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let f = dive.next();
            let t = i as f64 / SR;
            let attack = (i as f64 / 48.0).min(1.0);
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.kick_fm.1 * exp(-t / 0.03);
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += f * p.kick_fm.0 / SR;
                    mph -= mph.floor();
                    sin_turns(ph + m)
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, (ratio, dec)) in [(1.0, 1.0), (1.59, 0.6), (2.14, 0.4), (2.3, 0.3)].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.13) * exp(-t / (p.kick_decay * dec * 0.3)) / (k + 1) as f64;
                    }
                    s
                }
                Kit::NoiseSculpt => {
                    if i % 32 == 0 {
                        lp.set(f, 8.0, SR);
                    }
                    lp.process(rng.bipolar()).low * 3.0
                }
                Kit::Crush => {
                    if i % 6 == 0 {
                        let sq = if sin_turns(ph) > 0.0 { 0.6 } else { -0.6 };
                        held = crush(0.6 * sin_turns(ph) + 0.4 * sq, 16.0);
                    }
                    held
                }
                Kit::PulseCode => {
                    if i % 3 == 0 {
                        held = crush(if sin_turns(ph) > 0.0 { 0.7 } else { -0.7 }, 8.0);
                    }
                    held
                }
                _ => sin_turns(ph),
            };
            ph += f / SR;
            ph -= ph.floor();
            let out = s * amp * attack;
            amp *= amp_k;
            let click = match p.kit {
                Kit::SubClicks if i < 48 => (1.0 - i as f64 / 48.0) * 0.5,
                Kit::MicroBlips if i < 24 => rng.bipolar() * (1.0 - i as f64 / 24.0) * 0.25,
                Kit::DataDust if i < 3 => 0.8,
                Kit::NoiseSculpt | Kit::GlassPings | Kit::PulseCode | Kit::SubClicks | Kit::MicroBlips | Kit::DataDust => 0.0,
                _ if i < 96 => rng.bipolar() * (1.0 - i as f64 / 96.0) * 0.4,
                _ => 0.0,
            };
            (out + click) * vel
        })
        .collect();
    shape(&mut v, 0.6 * p.punch, 7.0, p.kick_drive, 0.0);
    v
}

/// A hand clap: four quick bursts of band-passed noise (several hands, a
/// hair apart), then a short diffuse tail. Its colour follows the kit's
/// snare wires, so it still belongs to the kit. Used by 2-step garage, layered
/// over the kit's own snare.
pub fn clap(p: &KitParams, vel: f64, rng: &mut Rng) -> Vec<f64> {
    let len = (0.3 * SR) as usize;
    let centre = (p.snare_wire * 0.6).clamp(900.0, 1800.0);
    let mut bp = Svf::new(centre, 1.4, SR);
    let mut air = Svf::new(centre * 2.4, 0.9, SR);
    // Burst onsets (seconds) and their levels, nudged a little per hit.
    let gaps = [0.0, 0.0095, 0.0185, 0.029];
    let jit: [f64; 4] = core::array::from_fn(|_| rng.range(-0.0012, 0.0012));
    let burst_k = decay_coef(0.0032, SR);
    let tail_k = decay_coef(0.11, SR);
    let mut bursts = [0.0f64; 4];
    let mut tail = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SR;
            let n = rng.bipolar();
            for k in 0..4 {
                let on = (gaps[k] + jit[k]).max(0.0);
                if t >= on && t - 1.0 / SR < on {
                    bursts[k] = [0.75, 0.65, 0.85, 1.0][k];
                }
                bursts[k] *= burst_k;
            }
            if t >= gaps[3] && t - 1.0 / SR < gaps[3] {
                tail = 0.55;
            }
            tail *= tail_k;
            let env = bursts.iter().sum::<f64>() + tail;
            (bp.process(n).band * 1.6 + air.process(n).band * 0.5) * env * vel
        })
        .collect();
    shape(&mut v, 0.4 * p.punch, 3.0, p.snare_drive, 0.0);
    v
}

/// Microsound's kick: a pure sub sine (42–58 Hz from the kit's own kick)
/// with the barest pitch dip and a short decay, so the beat is felt more than
/// heard. No click of its own; the pulse adds one when asked.
pub fn sub_kick(p: &KitParams, vel: f64) -> Vec<f64> {
    let to = p.kick_to.clamp(42.0, 58.0);
    let len = (0.12 * SR) as usize;
    let amp_k = decay_coef(0.045, SR);
    let mut dive = Dive::new(to * 1.6, to, 0.008);
    let (mut amp, mut ph) = (1.0, 0.0);
    (0..len)
        .map(|i| {
            let f = dive.next();
            let attack = (i as f64 / 96.0).min(1.0);
            let s = sin_turns(ph) * amp * attack;
            ph += f / SR;
            ph -= ph.floor();
            amp *= amp_k;
            // The last 4 ms fade, so the tail never ends on a step.
            let tail = ((len - i) as f64 / 192.0).min(1.0);
            s * tail * vel
        })
        .collect()
}

/// Microsound's click: one to three samples of impulse, rung through a
/// very short high resonance. `bright` lifts it toward the top of hearing.
pub fn micro_click(p: &KitParams, vel: f64, bright: bool, rng: &mut Rng) -> Vec<f64> {
    let centre = if bright { rng.range(7_000.0, 12_000.0) } else { rng.range(2_500.0, 6_000.0) };
    let mut bp = Svf::new(centre, rng.range(2.0, 5.0), SR);
    let width = 1 + rng.below(3);
    let len = 96;
    let crushed = matches!(p.kit, Kit::Crush | Kit::DataDust | Kit::PulseCode);
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let x = if i < width { if i % 2 == 0 { 1.0 } else { -0.6 } } else { 0.0 };
            let ring = bp.process(x).band;
            (x * 0.5 + ring * 1.5) * vel
        })
        .collect();
    if crushed {
        for x in v.iter_mut() {
            *x = crush(*x, 6.0);
        }
    }
    v
}

/// Microsound's grain: 2–8 ms under a smooth window, either a sliver of
/// band-passed noise or a high sine, panned by the caller.
pub fn micro_grain(p: &KitParams, vel: f64, rng: &mut Rng) -> Vec<f64> {
    let len = (rng.range(0.002, 0.008) * SR) as usize;
    let tone = rng.chance(0.45);
    let f = if tone {
        // The kit's metal partials, lifted into the air.
        rng.pick(&p.metal) * rng.pick(&[8.0, 12.0, 16.0])
    } else {
        rng.range(1_500.0, 9_000.0)
    };
    let mut bp = Svf::new(f.min(16_000.0), 3.0, SR);
    let mut ph = rng.unit();
    (0..len)
        .map(|i| {
            let w = sin_turns(0.5 * i as f64 / len as f64);
            let s = if tone {
                let s = sin_turns(ph);
                ph += f / SR;
                ph -= ph.floor();
                s
            } else {
                bp.process(rng.bipolar()).band * 2.5
            };
            s * w * w * vel
        })
        .collect()
}

pub fn snare(p: &KitParams, vel: f64, ghost: bool, rng: &mut Rng) -> Vec<f64> {
    let decay = if ghost { p.snare_decay * 0.4 } else { p.snare_decay };
    let len = ((decay * 1.6 + 0.05) * SR) as usize;
    let mut bp = Svf::new(p.snare_wire, 0.75, SR);
    let mut sweep = Svf::new(p.snare_wire, 3.0, SR);
    let noise_k = decay_coef(decay, SR);
    let body_k = decay_coef(decay * 0.5, SR);
    let (mut ne, mut be) = (1.0, 1.0);
    let (mut ph, mut ph2, mut mph) = (0.0, 0.25, 0.0);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SR;
            let n = rng.bipolar();
            let attack = (i as f64 / 24.0).min(1.0);
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.snare_fm.1 * exp(-t / 0.05);
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += p.snare_tones[0] * p.snare_fm.0 / SR;
                    mph -= mph.floor();
                    let tone = sin_turns(ph + m) * be;
                    ph += p.snare_tones[0] / SR;
                    0.8 * tone + 0.35 * bp.process(n).band * ne
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, ratio) in [1.0, 1.47, 1.97, 2.43].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.21) * exp(-t / (decay * 0.35 / (1.0 + k as f64 * 0.4)));
                    }
                    ph += p.snare_tones[0] / SR;
                    0.5 * s + 0.4 * bp.process(n).band * exp(-t / 0.02)
                }
                Kit::Crush => {
                    if i % 4 == 0 {
                        let sq = if ph < 0.5 { 0.5 } else { -0.5 };
                        held = crush(n * ne + sq * be, 3.0);
                    }
                    ph += 190.0 / SR;
                    held
                }
                Kit::NoiseSculpt => {
                    if i % 32 == 0 {
                        sweep.set(p.snare_wire * (0.3 + 0.7 * exp(-t / 0.04)), 3.0, SR);
                    }
                    sweep.process(n).band * 2.2 * ne
                }
                Kit::SubClicks => {
                    let click = if i < 72 { 1.0 - i as f64 / 72.0 } else { 0.0 };
                    click * 0.9 + bp.process(n).band * 0.8 * ne
                }
                Kit::MicroBlips => {
                    // A glitch blip: FM tone, sample-held and crushed to a few levels.
                    let (levels, hold) = (p.snare_fm.0.max(2.0), p.snare_fm.1.max(1.0) as usize);
                    if i % hold == 0 {
                        held = crush(sin_turns(ph + 0.35 * sin_turns(ph2)), levels);
                    }
                    ph += p.snare_tones[0] / SR;
                    ph2 += p.snare_tones[0] * 3.0 / SR;
                    let tick = if i < 96 { n * (1.0 - i as f64 / 96.0) * 0.35 } else { 0.0 };
                    held * ne + tick
                }
                Kit::GlassPings => {
                    let s = 0.7 * sin_turns(ph) + 0.3 * sin_turns(ph2);
                    ph += p.snare_tones[0] / SR;
                    ph2 += p.snare_tones[0] * 2.756 / SR;
                    s * ne + bp.process(n).band * 0.15 * exp(-t / 0.01)
                }
                Kit::DataDust => {
                    // Crackle: sparse random impulses, rung through a band-pass.
                    let imp = if rng.chance(p.snare_fm.0.max(0.005)) { rng.bipolar() * 3.0 } else { 0.0 };
                    bp.process(imp).band * 1.4 * ne + imp * 0.25 * ne
                }
                Kit::PulseCode => {
                    // A dropout buzz: two square tones swapping every few milliseconds.
                    let f = if (i / 240) % 2 == 0 { p.snare_tones[0] } else { p.snare_tones[1] };
                    ph += f / SR;
                    (if ph.fract() < 0.5 { 0.55 } else { -0.55 }) * ne
                }
                Kit::Acoustic | Kit::Off => {
                    let mut body = 0.0;
                    for (k, f) in p.snare_tones.iter().enumerate() {
                        let phase = if k == 0 { &mut ph } else { &mut ph2 };
                        body += sin_turns(*phase) * be;
                        *phase += f * (1.0 + 0.15 * be) / SR;
                    }
                    let wires = (bp.process(n).band * 1.6 + n * 0.3) * ne;
                    let crack = if i < 144 { n * (1.0 - i as f64 / 144.0) } else { 0.0 };
                    0.55 * body + 0.9 * wires + 0.8 * crack
                }
            };
            ph -= ph.floor();
            ph2 -= ph2.floor();
            ne *= noise_k;
            be *= body_k;
            s * attack * vel
        })
        .collect();
    shape(&mut v, 0.5 * p.punch, 6.0, p.snare_drive, 0.12 * p.punch);
    v
}

/// Hats, open hat and ride. `max_len` chokes the cymbal at the next cymbal hit.
pub fn cymbal(p: &KitParams, voice: crate::drums::Voice, vel: f64, max_len: usize, rng: &mut Rng) -> Vec<f64> {
    use crate::drums::Voice;
    let decay = match voice {
        Voice::Hat => p.hat_decay,
        Voice::OpenHat => p.hat_decay * 5.0,
        _ => p.ride_decay,
    };
    let len = ((decay * 1.3 + 0.01) * SR).min(max_len as f64).max(96.0) as usize;
    let env_k = decay_coef(decay, SR);
    let bright = 0.85 + 0.3 * vel; // harder hits are brighter
    let ride = voice == Voice::Ride;
    // Per-hit variation: detuned oscillators, random noise colours, a random tuned note.
    let detune: [f64; 6] = core::array::from_fn(|_| 1.0 + rng.range(-0.03, 0.03));
    let centre = p.cymbal_centre * bright * if ride { 0.7 } else { 1.0 };
    let mut bp = Svf::new(centre, 1.1, SR);
    let mut n1 = Svf::new(centre * rng.range(0.7, 1.0), 0.9, SR);
    let mut n2 = Svf::new(centre * rng.range(1.0, 1.35), 0.9, SR);
    let mut hp = OnePole::new(if ride { 3000.0 } else { 5000.0 }, SR);
    let mut ping = Svf::new(centre, if ride { 10.0 } else { 14.0 }, SR);
    let note = if p.tuned.is_empty() { 1000.0 } else { rng.pick(&p.tuned) * if ride { 0.5 } else { 1.0 } };
    let mut phases: [f64; 6] = core::array::from_fn(|_| rng.unit());
    let (mut env, mut ph, mut mph) = (1.0, 0.0, 0.0);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SR;
            let n = rng.bipolar();
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.metal[1] * exp(-t / (decay * 0.5));
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += centre * p.metal[0] / SR;
                    mph -= mph.floor();
                    let s = sin_turns(ph + m);
                    ph += centre / SR;
                    s * 0.8
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, ratio) in [1.0, 2.756, 5.404, 8.933].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.17) * exp(-t / (decay / (1.0 + k as f64))) / (k + 1) as f64;
                    }
                    ph += note / SR;
                    s * 0.9
                }
                Kit::Crush => {
                    let hold = if ride { 7 } else { 3 };
                    if i % hold == 0 {
                        held = if ride {
                            if n > 0.0 { 0.5 } else { -0.5 }
                        } else if sin_turns(ph) > 0.0 {
                            0.6
                        } else {
                            -0.6
                        };
                    }
                    ph += centre / SR;
                    held
                }
                Kit::NoiseSculpt => ping.process(n).band * 2.4,
                Kit::MicroBlips => {
                    if ride {
                        // A short high blip on a scale note.
                        let s = crush(sin_turns(ph), 5.0);
                        ph += note / SR;
                        s * 0.6
                    } else {
                        // A tick: differentiated noise, only the crackly top.
                        let tick = n - held;
                        held = n;
                        tick * 0.5
                    }
                }
                Kit::GlassPings => {
                    let s = 0.7 * sin_turns(ph) + 0.3 * sin_turns(ph * 2.756);
                    ph += note / SR;
                    s * if ride { 0.45 } else { 0.6 }
                }
                Kit::DataDust => {
                    let density = if ride { 0.05 } else { 0.15 };
                    if rng.chance(density) { rng.bipolar() } else { 0.0 }
                }
                Kit::PulseCode => {
                    if ride {
                        let s = sin_turns(ph) * 0.5;
                        ph += centre / SR;
                        s
                    } else if i < 2 {
                        if i == 0 { 0.8 } else { -0.8 }
                    } else {
                        0.0
                    }
                }
                Kit::SubClicks => {
                    let click = if i < 30 { n * (1.0 - i as f64 / 30.0) } else { 0.0 };
                    let tick = if ride { sin_turns(ph) * 0.3 } else { 0.0 };
                    ph += 5200.0 / SR;
                    click + tick
                }
                Kit::Acoustic | Kit::Off => {
                    // Squares, plus ring-modulated pairs for denser, less regular partials.
                    let mut sq = [0.0; 6];
                    for (k, (p_, f)) in phases.iter_mut().zip(p.metal).enumerate() {
                        sq[k] = if *p_ < 0.5 { 1.0 } else { -1.0 };
                        *p_ += f * detune[k] * if ride { 1.37 } else { 1.0 } / SR;
                        *p_ -= p_.floor();
                    }
                    let squares: f64 = sq.iter().sum::<f64>() / 6.0;
                    let rings = (sq[0] * sq[3] + sq[1] * sq[4] + sq[2] * sq[5]) / 3.0;
                    let metal = bp.process(0.6 * squares + 0.4 * rings).band;
                    let wash = n1.process(n).band + n2.process(n).band;
                    let mut s = metal * (1.0 - p.noise_amt) * 1.6 + wash * p.noise_amt * 0.9;
                    if ride {
                        s += 0.1 * sin_turns(ph) * exp(-t / 0.25);
                        ph += 2930.0 * detune[0] / SR;
                    }
                    s
                }
            };
            ph -= ph.floor();
            let high = s - hp.process(s);
            let out = if matches!(p.kit, Kit::Acoustic | Kit::Crush | Kit::NoiseSculpt) { high * 1.8 } else { s };
            let o = out * env * vel;
            env *= env_k;
            let attack = (i as f64 / 8.0).min(1.0);
            // Choke / end: a 2 ms fade.
            let fade = ((len - i) as f64 / 96.0).min(1.0);
            o * attack * fade
        })
        .collect();
    shape(&mut v, 0.3 * p.punch, 3.0, 1.0 + 0.2 * p.punch, 0.0);
    v
}

}
// ---- src/audio/master.rs ----
pub mod master {
//! End-of-chain master effects: a slight digital-glitch character over the whole
//! mix. Each effect is a toggle, and any combination may run.
//!
//! These sit *after* the mix and the monitor, so soloing still hears a layer
//! clean. With every effect off, the chain is an exact pass-through — which is
//! what keeps the golden hashes unchanged.
//!
//! All of it is per-sample, deterministic and bounded: same seed, same glitches.

use crate::filter::{DcBlock, OnePole, Svf};
use crate::math::{exp2, sanitize, sin_turns, tanh};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// Delay-line length, seconds.
const DELAY_SECONDS: f64 = 0.02;

/// Which master effects are enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MasterFx {
    /// The master switch; the individual effects are remembered regardless.
    pub enabled: bool,
    /// Wow and flutter, saturation, hiss.
    pub tape: bool,
    /// Sample-rate/bit reduction with occasional skips.
    pub cd: bool,
    /// Slow pitch drift with occasional drops.
    pub pitch: bool,
    /// Band-limited, crackling radio.
    pub radio: bool,
    /// Coarse bit/sample crush.
    pub crush: bool,
    /// Rare, short signal dropouts.
    pub dropout: bool,
}

impl MasterFx {
    pub const OFF: MasterFx = MasterFx {
        enabled: false,
        tape: false,
        cd: false,
        pitch: false,
        radio: false,
        crush: false,
        dropout: false,
    };

    /// Are any individual effects selected?
    pub fn any(self) -> bool {
        self.tape || self.cd || self.pitch || self.radio || self.crush || self.dropout
    }

    /// Does the chain actually run?
    pub fn active(self) -> bool {
        self.enabled && self.any()
    }

    /// A stable name, for the menu and the recipe card.
    pub fn labels(self) -> [(&'static str, bool); 6] {
        [
            ("Tape", self.tape),
            ("CD artifacts", self.cd),
            ("Pitch drift", self.pitch),
            ("Radio", self.radio),
            ("Crush", self.crush),
            ("Dropouts", self.dropout),
        ]
    }

    pub fn bits(self) -> u32 {
        (self.enabled as u32)
            | (self.tape as u32) << 1
            | (self.cd as u32) << 2
            | (self.pitch as u32) << 3
            | (self.radio as u32) << 4
            | (self.crush as u32) << 5
            | (self.dropout as u32) << 6
    }

    pub fn from_bits(bits: u32) -> Self {
        MasterFx {
            enabled: bits & 1 != 0,
            tape: bits & (1 << 1) != 0,
            cd: bits & (1 << 2) != 0,
            pitch: bits & (1 << 3) != 0,
            radio: bits & (1 << 4) != 0,
            crush: bits & (1 << 5) != 0,
            dropout: bits & (1 << 6) != 0,
        }
    }
}

/// The running chain: one instance of each effect's state.
pub struct MasterChain {
    fx: MasterFx,
    // Tape.
    tape_l: Vec<f64>,
    tape_r: Vec<f64>,
    tape_write: usize,
    wow: f64,
    flutter: f64,
    tape_lp: [OnePole; 2],
    hiss: Rng,
    // Pitch drift.
    pitch_l: Vec<f64>,
    pitch_r: Vec<f64>,
    pitch_write: usize,
    pitch_offset: f64,
    pitch_vel: f64,
    pitch_rng: Rng,
    // CD artifacts.
    cd_hold: [f64; 2],
    cd_count: u32,
    cd_rng: Rng,
    // Radio.
    radio_bp: [Svf; 2],
    radio_lp: [OnePole; 2],
    radio_am: f64,
    radio_rng: Rng,
    // Crush.
    crush_hold: [f64; 2],
    crush_count: u32,
    // Dropouts.
    drop_count: u32,
    drop_gain: f64,
    drop_rng: Rng,
    dc: [DcBlock; 2],
}

impl MasterChain {
    pub fn new(fx: MasterFx, seed: u64) -> Self {
        let n = (DELAY_SECONDS * SR) as usize;
        MasterChain {
            fx,
            tape_l: vec![0.0; n],
            tape_r: vec![0.0; n],
            tape_write: 0,
            wow: 0.0,
            flutter: 0.0,
            tape_lp: [OnePole::new(9000.0, SR), OnePole::new(9000.0, SR)],
            hiss: Rng::stream(seed, 0x5441_5045), // "TAPE"
            pitch_l: vec![0.0; n],
            pitch_r: vec![0.0; n],
            pitch_write: 0,
            pitch_offset: 0.0,
            pitch_vel: 0.0,
            pitch_rng: Rng::stream(seed, 0x5049_5443), // "PITC"
            cd_hold: [0.0; 2],
            cd_count: 0,
            cd_rng: Rng::stream(seed, 0x4344_0000), // "CD"
            radio_bp: [Svf::new(1200.0, 0.7, SR), Svf::new(1200.0, 0.7, SR)],
            radio_lp: [OnePole::new(3200.0, SR), OnePole::new(3200.0, SR)],
            radio_am: 0.0,
            radio_rng: Rng::stream(seed, 0x5241_4449), // "RADI"
            crush_hold: [0.0; 2],
            crush_count: 0,
            drop_count: 0,
            drop_gain: 1.0,
            drop_rng: Rng::stream(seed, 0x4452_4f50), // "DROP"
            dc: [DcBlock::default(), DcBlock::default()],
        }
    }

    pub fn set_fx(&mut self, fx: MasterFx) {
        self.fx = fx;
    }

    /// One stereo sample through the chain. A pass-through when nothing is on.
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        if !self.fx.active() {
            return (l, r);
        }
        let (mut l, mut r) = (l, r);
        if self.fx.tape {
            (l, r) = self.tape(l, r);
        }
        if self.fx.pitch {
            (l, r) = self.pitch(l, r);
        }
        if self.fx.cd {
            (l, r) = self.cd(l, r);
        }
        if self.fx.radio {
            (l, r) = self.radio(l, r);
        }
        if self.fx.crush {
            (l, r) = self.crush(l, r);
        }
        if self.fx.dropout {
            (l, r) = self.dropout(l, r);
        }
        (sanitize(self.dc[0].process(l)), sanitize(self.dc[1].process(r)))
    }

    fn tape(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.wow = (self.wow + 0.6 / SR).rem_euclid(1.0);
        self.flutter = (self.flutter + 6.5 / SR).rem_euclid(1.0);
        let modulate = 0.00035 * sin_turns(self.wow) + 0.00004 * sin_turns(self.flutter);
        let (wl, wr) = read_delay(&self.tape_l, &self.tape_r, self.tape_write, (0.006 + modulate) * SR);
        self.tape_l[self.tape_write] = l;
        self.tape_r[self.tape_write] = r;
        self.tape_write = (self.tape_write + 1) % self.tape_l.len();

        let drive = 1.4;
        let sat_l = self.tape_lp[0].process(tanh(wl * drive) / drive);
        let sat_r = self.tape_lp[1].process(tanh(wr * drive) / drive);
        let hiss = self.hiss.bipolar() * 0.0012;
        (l * 0.7 + (sat_l + hiss) * 0.3, r * 0.7 + (sat_r + hiss) * 0.3)
    }

    fn pitch(&mut self, l: f64, r: f64) -> (f64, f64) {
        // A drifting read pointer: its rate is the pitch shift.
        self.pitch_vel = (self.pitch_vel + self.pitch_rng.bipolar() * 0.0006) * 0.999;
        self.pitch_offset += self.pitch_vel;
        if self.pitch_offset.abs() > 160.0 {
            // A jump, heard as a momentary drop/spin-up.
            self.pitch_offset = 0.0;
            self.pitch_vel = 0.0;
        }
        let (wl, wr) = read_delay(&self.pitch_l, &self.pitch_r, self.pitch_write, 60.0 + self.pitch_offset);
        self.pitch_l[self.pitch_write] = l;
        self.pitch_r[self.pitch_write] = r;
        self.pitch_write = (self.pitch_write + 1) % self.pitch_l.len();

        // Mostly wet: a pitch shift has to replace the dry signal.
        (l * 0.35 + wl * 0.65, r * 0.35 + wr * 0.65)
    }

    fn cd(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.cd_count == 0 {
            self.cd_hold = [l, r];
            let base = 1.0 + self.cd_rng.range(0.0, 6.0);
            self.cd_count = base as u32;
            if self.cd_rng.chance(0.02) {
                self.cd_count += self.cd_rng.range(20.0, 160.0) as u32;
            }
        } else {
            self.cd_count -= 1;
        }
        let q = exp2(12.0);
        ((self.cd_hold[0] * q).round() / q, (self.cd_hold[1] * q).round() / q)
    }

    fn radio(&mut self, l: f64, r: f64) -> (f64, f64) {
        let (bl, br) = (self.radio_bp[0].process(l).band, self.radio_bp[1].process(r).band);
        let (ll, lr) = (self.radio_lp[0].process(bl), self.radio_lp[1].process(br));
        self.radio_am = (self.radio_am + 31.0 / SR).rem_euclid(1.0);
        let am = 1.0 - 0.18 * (0.5 + 0.5 * sin_turns(self.radio_am));
        let crackle = if self.radio_rng.chance(0.0012) {
            self.radio_rng.bipolar() * 0.35
        } else {
            0.0
        };
        let wet_l = tanh(ll * am * 1.6) + crackle;
        let wet_r = tanh(lr * am * 1.6) + crackle * 0.7;
        (l * 0.45 + wet_l * 0.55, r * 0.45 + wet_r * 0.55)
    }

    fn crush(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.crush_count == 0 {
            self.crush_hold = [l, r];
            self.crush_count = 4;
        } else {
            self.crush_count -= 1;
        }
        let q = exp2(9.0);
        ((self.crush_hold[0] * q).round() / q, (self.crush_hold[1] * q).round() / q)
    }

    fn dropout(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.drop_count == 0 {
            if self.drop_rng.chance(0.00006) {
                self.drop_count = self.drop_rng.range(60.0, 500.0) as u32;
                self.drop_gain = self.drop_rng.range(0.0, 0.25);
            }
        } else {
            self.drop_count -= 1;
        }
        let gain = if self.drop_count > 0 { self.drop_gain } else { 1.0 };
        (l * gain, r * gain)
    }
}

fn read_delay(left: &[f64], right: &[f64], write: usize, delay: f64) -> (f64, f64) {
    let n = left.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let read = (write as f64 - delay).rem_euclid(n as f64);
    let i0 = read.floor() as usize % n;
    let i1 = (i0 + 1) % n;
    let f = read - read.floor();
    (left[i0] + (left[i1] - left[i0]) * f, right[i0] + (right[i1] - right[i0]) * f)
}



}
// ---- src/audio/micro.rs ----
pub mod micro {
//! The microsound mode's own settings. Microsound works at the smallest time
//! scale of music: clicks a few samples long, grains of a few milliseconds,
//! short sine pips, and silence, placed exactly. This mode keeps a faint
//! pulse under them (the "clicks and cuts" way): a sub-sine kick on the beat,
//! a sub pulse on the off-beats, ticks and grains on the sixteenths, digital
//! dropouts as edits, the bed scattered into a grain cloud, and the drone in
//! short fragments that keep time with the bar, its chords changing (and its
//! fragments transposing) on a regular count. The settings only
//! matter in microsound; the other modes ignore them, so their sound never
//! moves.

/// What carries the beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pulse {
    /// No beat: only the ticks, grains and pips.
    Off,
    /// A sub-sine kick on each beat and a sub pulse on the off-beats.
    Sub,
    /// The same, with a click on every kick and a few on the off-beats.
    SubClick,
}

pub const PULSES: [Pulse; 3] = [Pulse::Off, Pulse::Sub, Pulse::SubClick];

impl Pulse {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Pulse::Off => "no beat: only the ticks, grains and pips",
            Pulse::Sub => "a sub-sine kick on each beat, a sub pulse between",
            Pulse::SubClick => "the same, with a click on each kick",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pulse::Off => "off",
            Pulse::Sub => "sub",
            Pulse::SubClick => "sub + click",
        }
    }
}

/// How thick the grains are, in the drums and in the cloud the bed becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grains {
    Sparse,
    Medium,
    Dense,
}

pub const GRAINS: [Grains; 3] = [Grains::Sparse, Grains::Medium, Grains::Dense];

impl Grains {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Grains::Sparse => "a few ticks and grains; the bed mostly itself",
            Grains::Medium => "ticks on most sixteenths; half the bed a grain cloud",
            Grains::Dense => "ticks everywhere; the bed almost all grains",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Grains::Sparse => "sparse",
            Grains::Medium => "medium",
            Grains::Dense => "dense",
        }
    }

    /// 0 (sparse) .. 1 (dense).
    pub fn amount(self) -> f64 {
        match self {
            Grains::Sparse => 0.0,
            Grains::Medium => 0.5,
            Grains::Dense => 1.0,
        }
    }
}

/// How often the high sine pips play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pips {
    Off,
    Sparse,
    Busy,
}

pub const PIPS: [Pips; 3] = [Pips::Off, Pips::Sparse, Pips::Busy];

impl Pips {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Pips::Off => "no sine pips",
            Pips::Sparse => "a few high sine pips a bar",
            Pips::Busy => "a scatter of pips, some in pairs",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pips::Off => "off",
            Pips::Sparse => "sparse",
            Pips::Busy => "busy",
        }
    }
}

/// How often the beat fails: gaps, skips and tiny repeats, like a skipping
/// disc or a dropping buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropouts {
    Off,
    Some,
    Many,
}

pub const DROPOUTS: [Dropouts; 3] = [Dropouts::Off, Dropouts::Some, Dropouts::Many];

impl Dropouts {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Dropouts::Off => "the pulse never fails",
            Dropouts::Some => "now and then a step drops, skips or ratchets",
            Dropouts::Many => "the pulse keeps failing, like a skipping disc",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dropouts::Off => "off",
            Dropouts::Some => "some",
            Dropouts::Many => "many",
        }
    }

    /// The chance a step is edited.
    pub fn chance(self) -> f64 {
        match self {
            Dropouts::Off => 0.0,
            Dropouts::Some => 0.07,
            Dropouts::Many => 0.18,
        }
    }
}

/// How often the chords change (all of them: the drone, the pips, the sub
/// pulse and the loops follow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordRate {
    /// The slow, weighted cycle the ambient modes use.
    Slow,
    Every4,
    Every2,
    EveryBar,
}

pub const CHORD_RATES: [ChordRate; 4] = [ChordRate::Slow, ChordRate::Every4, ChordRate::Every2, ChordRate::EveryBar];

impl ChordRate {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            ChordRate::Slow => "the slow, weighted cycle of the ambient modes",
            ChordRate::Every4 => "a new chord every four bars",
            ChordRate::Every2 => "a new chord every two bars",
            ChordRate::EveryBar => "a new chord every bar",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ChordRate::Slow => "slow",
            ChordRate::Every4 => "every 4 bars",
            ChordRate::Every2 => "every 2 bars",
            ChordRate::EveryBar => "every bar",
        }
    }

    /// Drum bars per chord (0 = the slow cycle).
    pub fn bars(self) -> u64 {
        match self {
            ChordRate::Slow => 0,
            ChordRate::Every4 => 4,
            ChordRate::Every2 => 2,
            ChordRate::EveryBar => 1,
        }
    }
}

/// How the drone's fragments are transposed: a repeating figure of four,
/// one transposition per fragment, so the movement keeps the groove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroneShift {
    Off,
    /// Up or down an octave.
    Octaves,
    /// Fifths and fourths (and the octave).
    Fifths,
    /// Steps of the scale.
    Scale,
}

pub const DRONE_SHIFTS: [DroneShift; 4] = [DroneShift::Off, DroneShift::Octaves, DroneShift::Fifths, DroneShift::Scale];

impl DroneShift {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            DroneShift::Off => "every drone fragment at home pitch",
            DroneShift::Octaves => "fragments jump an octave up or down",
            DroneShift::Fifths => "fragments move by fifths, fourths and octaves",
            DroneShift::Scale => "fragments step through the scale",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DroneShift::Off => "off",
            DroneShift::Octaves => "octaves",
            DroneShift::Fifths => "fifths",
            DroneShift::Scale => "scale steps",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MicroSetup {
    pub pulse: Pulse,
    pub grains: Grains,
    pub pips: Pips,
    pub dropouts: Dropouts,
    pub chords: ChordRate,
    pub shift: DroneShift,
}

impl MicroSetup {
    pub const DEFAULT: MicroSetup =
        MicroSetup {
            pulse: Pulse::SubClick,
            grains: Grains::Medium,
            pips: Pips::Sparse,
            dropouts: Dropouts::Some,
            chords: ChordRate::Every2,
            shift: DroneShift::Fifths,
        };

    /// One recipe word: two bits each.
    pub fn word(self) -> u32 {
        fn idx<T: PartialEq>(all: &[T], v: T) -> u32 {
            all.iter().position(|x| *x == v).unwrap_or(0) as u32
        }
        idx(&PULSES, self.pulse)
            | idx(&GRAINS, self.grains) << 2
            | idx(&PIPS, self.pips) << 4
            | idx(&DROPOUTS, self.dropouts) << 6
            | idx(&CHORD_RATES, self.chords) << 8
            | idx(&DRONE_SHIFTS, self.shift) << 10
    }

    pub fn from_word(w: u32) -> Self {
        fn at<T: Copy>(all: &[T], i: u32) -> T {
            all[(i & 3) as usize % all.len()]
        }
        MicroSetup {
            pulse: at(&PULSES, w),
            grains: at(&GRAINS, w >> 2),
            pips: at(&PIPS, w >> 4),
            dropouts: at(&DROPOUTS, w >> 6),
            chords: at(&CHORD_RATES, w >> 8),
            shift: at(&DRONE_SHIFTS, w >> 10),
        }
    }
}



}
// ---- src/audio/modulate.rs ----
pub mod modulate {
//! Very slow cyclic modulation that shapes the endless form.
//!
//! Every LFO's phase is computed directly from the integer sample clock
//! (`clock % period`), so there is no accumulated drift however long the
//! piece plays, and the value at any moment is independent of block size.
//! The periods are mutually prime numbers of seconds, so the combined
//! pattern only repeats after billions of years.

use crate::math::sin_turns;
use crate::rng::Rng;

/// The LFO periods, in seconds (all prime).
pub const PERIODS: [u64; 7] = [41, 61, 97, 151, 233, 307, 113];

pub struct Lfo {
    period: u64,
    offset: u64,
}

impl Lfo {
    fn new(seconds: u64, rng: &mut Rng) -> Self {
        let period = seconds * crate::SAMPLE_RATE as u64;
        Lfo { period, offset: rng.next_u64() % period }
    }

    /// Value in [-1, 1].
    pub fn at(&self, clock: u64) -> f64 {
        let p = (clock.wrapping_add(self.offset)) % self.period;
        sin_turns(p as f64 / self.period as f64)
    }
}

/// The modulation sources, sampled at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModState {
    /// Drone filter openness, 0..1.
    pub drone_open: f64,
    /// Drone filter resonance, 0..1.
    pub drone_reso: f64,
    /// Glitch layer densities, 0..1.
    pub density1: f64,
    pub density2: f64,
    /// Bass brightness, 0..1.
    pub bass_bright: f64,
    /// Bass swell, 0..1.
    pub bass_swell: f64,
    /// Drum energy, 0..1: picks each phrase's arrangement section.
    pub drums: f64,
    /// Atmosphere loop level, 0.7..1: a gentle drift on top of each loop.
    pub loop_level: f64,
    /// Loop 2's level drift, on different cycles so the two never swell together.
    pub loop2_level: f64,
}

pub struct Mods {
    l41: Lfo,
    l61: Lfo,
    l97: Lfo,
    l151: Lfo,
    l233: Lfo,
    l307: Lfo,
    l113: Lfo,
}

impl Mods {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, 0x40D);
        Mods {
            l41: Lfo::new(41, &mut rng),
            l61: Lfo::new(61, &mut rng),
            l97: Lfo::new(97, &mut rng),
            l151: Lfo::new(151, &mut rng),
            l233: Lfo::new(233, &mut rng),
            l307: Lfo::new(307, &mut rng),
            l113: Lfo::new(113, &mut rng),
        }
    }

    /// Every LFO's (period in seconds, value) at `clock`, in `PERIODS` order.
    pub fn lfos(&self, clock: u64) -> [(u64, f64); 7] {
        let all = [&self.l41, &self.l61, &self.l97, &self.l151, &self.l233, &self.l307, &self.l113];
        let mut out = [(0, 0.0); 7];
        for ((o, lfo), secs) in out.iter_mut().zip(all).zip(PERIODS) {
            *o = (secs, lfo.at(clock));
        }
        out
    }

    pub fn at(&self, clock: u64) -> ModState {
        let (l41, l61, l97) = (self.l41.at(clock), self.l61.at(clock), self.l97.at(clock));
        let (l151, l233, l307) = (self.l151.at(clock), self.l233.at(clock), self.l307.at(clock));
        let l113 = self.l113.at(clock);
        let unit = |x: f64| x.clamp(0.0, 1.0);
        ModState {
            drone_open: unit(0.5 + 0.3 * l61 + 0.2 * l233),
            drone_reso: unit(0.35 + 0.25 * l97 * l61 + 0.2 * l151),
            density1: unit(0.6 + 0.3 * l97 + 0.25 * l307),
            density2: unit(0.55 + 0.3 * l151 + 0.25 * l41 * l233),
            bass_bright: unit(0.4 + 0.35 * l113 + 0.2 * l41),
            bass_swell: unit(0.7 + 0.3 * l307 * l233),
            drums: unit(0.5 + 0.34 * l233 + 0.25 * l97 * l41),
            loop_level: 0.85 + 0.15 * l113 * l61,
            loop2_level: 0.85 + 0.15 * l97 * l233,
        }
    }
}

}
// ---- src/audio/pattern.rs ----
pub mod pattern {
//! Cyclic repeats: each glitch layer loops a short step pattern a number of
//! times, then mutates one step and loops again. Two layers with co-prime
//! cycle lengths drift in and out of phase with each other.

use crate::glitch::{Event, History, Kind, Voice, KINDS};
use crate::math::midi_hz;
use crate::rng::Rng;

pub struct LayerConfig {
    pub steps: usize,
    /// Step length in samples (divisible by 1-4 for ratchets).
    pub step_len: u64,
    /// Relative weight of each `KINDS` entry.
    pub weights: [f64; 6],
    /// MIDI notes for pitched kinds.
    pub notes: Vec<f64>,
    /// Chance a step holds an event.
    pub fill: f64,
    pub gain: (f64, f64),
    pub pan_width: f64,
    /// Loops before mutating (min, max).
    pub repeats: (u64, u64),
    pub max_ratchet: u64,
    /// The ping's upper partial, as a ratio of its note; 0 = the usual
    /// inharmonic 2.756 (2-step garage uses a whole-number 3).
    pub ping_ratio: f64,
    /// 2-step garage: a ratchet that splits a step into sixteenths swings
    /// its off-beat strikes late by `swing_push` (0 = straight).
    pub swing_push: u64,
    pub sixteenth: u64,
    /// How long the ringing kinds (pings, blips, stutters) last, as a share
    /// of their usual length: 1 normally, much less in microsound.
    pub len_scale: f64,
}

const MAX_VOICES: usize = 8;

/// What the last mutation did (for display; never read by the audio path).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mutation {
    /// A step was given a new random event of this kind.
    Replace(Kind),
    /// A step was emptied.
    Clear,
    /// The whole pattern rotated one step left.
    Rotate,
    /// A step kept its sound but got a new pitch/pan.
    Nudge,
}

pub struct Layer {
    cfg: LayerConfig,
    rng: Rng,
    pattern: Vec<Option<Event>>,
    loops: u64,
    loops_until_mutation: u64,
    voices: Vec<Voice>,
    /// (clock, step, what) of the latest mutation.
    last_mutation: Option<(u64, usize, Mutation)>,
    triggers: u64,
}

impl Layer {
    pub fn new(cfg: LayerConfig, seed: u64, stream: u64) -> Self {
        let mut rng = Rng::stream(seed, stream);
        let mut layer = Layer {
            pattern: Vec::new(),
            loops: 0,
            loops_until_mutation: rng.int(cfg.repeats.0, cfg.repeats.1),
            voices: Vec::with_capacity(MAX_VOICES),
            last_mutation: None,
            triggers: 0,
            cfg,
            rng,
        };
        for _ in 0..layer.cfg.steps {
            let step = layer.maybe_event();
            layer.pattern.push(step);
        }
        layer
    }

    fn maybe_event(&mut self) -> Option<Event> {
        if self.rng.chance(self.cfg.fill) {
            Some(self.random_event())
        } else {
            None
        }
    }

    fn random_event(&mut self) -> Event {
        let rng = &mut self.rng;
        let cfg = &self.cfg;
        let sr = crate::SAMPLE_RATE as f64;
        let kind = KINDS[rng.weighted(&cfg.weights)];
        let note = rng.pick(&cfg.notes);
        let ms = |rng: &mut Rng, lo: f64, hi: f64| (rng.range(lo, hi) * sr / 1000.0) as u32;
        let (len, param, gain_trim) = match kind {
            Kind::Blip => (ms(rng, 25.0, 140.0), rng.int(1, 12) as f64, 1.0),
            Kind::Tick => (ms(rng, 2.0, 9.0), 0.0, 1.4),
            Kind::Stutter => {
                let grain = ms(rng, 12.0, 90.0);
                (grain * rng.int(2, 6) as u32, grain as f64, 1.0)
            }
            Kind::Ping => (ms(rng, 300.0, 1600.0), cfg.ping_ratio, 0.8),
            Kind::Click => (ms(rng, 3.0, 30.0), 0.0, 0.5),
            Kind::Crush => (ms(rng, 15.0, 80.0), rng.int(2, 40) as f64, 0.6),
        };
        // (Scaled after the draws, so the pattern is the same one, only shorter.)
        let len = match kind {
            Kind::Ping | Kind::Blip | Kind::Stutter if cfg.len_scale < 1.0 => (len as f64 * cfg.len_scale) as u32,
            _ => len,
        };
        let freq = match kind {
            Kind::Click => rng.range(300.0, 3000.0),
            _ => midi_hz(note),
        };
        Event {
            kind,
            freq,
            gain: rng.range(cfg.gain.0, cfg.gain.1) * gain_trim,
            pan: rng.bipolar() * cfg.pan_width,
            len: len.max(48),
            param,
            ratchet: if rng.chance(0.25) { rng.int(2, cfg.max_ratchet) as u32 } else { 1 },
            threshold: rng.unit(),
            noise_seed: rng.next_u64(),
        }
    }

    fn mutate(&mut self, clock: u64) {
        let i = self.rng.below(self.cfg.steps);
        let what = match self.rng.weighted(&[0.55, 0.2, 0.15, 0.1]) {
            0 => {
                let ev = self.random_event();
                self.pattern[i] = Some(ev);
                Mutation::Replace(ev.kind)
            }
            1 => {
                self.pattern[i] = None;
                Mutation::Clear
            }
            2 => {
                self.pattern.rotate_left(1);
                Mutation::Rotate
            }
            _ => {
                // Nudge an existing step's pitch / pan, keeping its character.
                let note = self.rng.pick(&self.cfg.notes);
                let pan = self.rng.bipolar() * self.cfg.pan_width;
                if let Some(ev) = self.pattern[i].as_mut() {
                    if ev.kind != Kind::Click {
                        ev.freq = midi_hz(note);
                    }
                    ev.pan = pan;
                }
                Mutation::Nudge
            }
        };
        self.last_mutation = Some((clock, i, what));
    }

    fn trigger(&mut self, ev: Event, history: &History) {
        self.voices.retain(|v| v.active());
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice::new(ev, history));
        self.triggers += 1;
    }

    pub fn config(&self) -> &LayerConfig {
        &self.cfg
    }

    pub fn pattern(&self) -> &[Option<Event>] {
        &self.pattern
    }

    /// (completed loops, loops before the next mutation).
    pub fn loops(&self) -> (u64, u64) {
        (self.loops, self.loops_until_mutation)
    }

    pub fn voices(&self) -> impl Iterator<Item = &Voice> {
        self.voices.iter().filter(|v| v.active())
    }

    pub fn last_mutation(&self) -> Option<(u64, usize, Mutation)> {
        self.last_mutation
    }

    /// Total events triggered so far.
    pub fn triggers(&self) -> u64 {
        self.triggers
    }

    pub fn next(&mut self, clock: u64, density: f64, history: &History) -> (f64, f64) {
        let step_len = self.cfg.step_len;
        let phase = clock % step_len;
        let step = ((clock / step_len) % self.cfg.steps as u64) as usize;
        if phase == 0 && step == 0 && clock > 0 {
            self.loops += 1;
            if self.loops >= self.loops_until_mutation {
                self.mutate(clock);
                self.loops = 0;
                self.loops_until_mutation = self.rng.int(self.cfg.repeats.0, self.cfg.repeats.1);
            }
        }
        if let Some(ev) = self.pattern[step] {
            let sub = step_len / ev.ratchet as u64;
            let on = if self.cfg.swing_push > 0 && sub == self.cfg.sixteenth {
                // Swung sixteenths: the odd strikes come late.
                let k = phase / sub;
                phase == k * sub + if k % 2 == 1 { self.cfg.swing_push } else { 0 }
            } else {
                phase % sub == 0
            };
            if on && density > ev.threshold {
                self.trigger(ev, history);
            }
        }
        let mut out = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if v.active() {
                let (l, r) = v.next(history);
                out.0 += l;
                out.1 += r;
            }
        }
        out
    }
}

}
// ---- src/audio/pips.rs ----
pub mod pips {
//! Microsound's sine pips: the melodic dust of the mode. Each pip is one
//! tone of the current chord lifted three or four octaves (about 1.5–8 kHz),
//! a pure sine 5–40 ms long under a smooth window, set at an exact place in
//! the stereo field. They land on the sixteenths, now and then on a
//! thirty-second between them, and sometimes come as a pair a thirty-second
//! apart. A short, quiet ping-pong echo gives each a small room.
//!
//! Every bar's pips are a pure function of seed 6 and the bar number.

use crate::delay::PingPong;
use crate::harmony::Harmony;
use crate::math::{midi_hz, sin_turns};
use crate::micro::Pips as Density;
use crate::rng::Rng;
use crate::tempo::Tempo;

const VOICES: usize = 8;
const LEVEL: f64 = 0.16;
const ECHO_MIX: f64 = 0.3;
const ECHO_FEEDBACK: f64 = 0.2;

/// One pip in a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pip {
    /// Where it starts, in thirty-seconds (0..32).
    pub at: u8,
    /// Which chord tone (index into the chord's tones, wrapped).
    pub tone: u8,
    /// Octaves above the chord tone's own register.
    pub lift: u8,
    /// Length in milliseconds.
    pub ms: f64,
    pub vel: f64,
    pub pan: f64,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    freq: f64,
    phase: f64,
    pos: u64,
    len: u64,
    vel: f64,
    pan: f64,
    on: bool,
}

pub struct Pips {
    seed: u64,
    density: Density,
    /// A thirty-second and a bar, in samples.
    t32: u64,
    bar: u64,
    voices: [Voice; VOICES],
    next_voice: usize,
    plan: Option<(u64, Vec<Pip>)>,
    echo: PingPong,
}

impl Pips {
    pub fn new(seed: u64, density: Density, tempo: Tempo) -> Self {
        let six = tempo.drum16;
        Pips {
            seed,
            density,
            t32: six / 2,
            bar: 16 * six,
            voices: [Voice::default(); VOICES],
            next_voice: 0,
            plan: None,
            // A dotted eighth: three sixteenths, so the repeats fall between.
            echo: PingPong::new((3 * six) as usize, ECHO_FEEDBACK),
        }
    }

    /// The pips of bar `bar` (pure).
    pub fn plan(&self, bar: u64) -> Vec<Pip> {
        let (lo, hi) = match self.density {
            Density::Off => return Vec::new(),
            Density::Sparse => (0, 3),
            Density::Busy => (3, 8),
        };
        // A two-bar figure, reshaped every eight bars.
        let (figure, shape) = (bar % 2, bar / 8);
        let mut rng = Rng::stream(self.seed, 0x919_0000 ^ (shape << 4) ^ figure);
        let n = lo + rng.below(hi - lo + 1);
        let mut out: Vec<Pip> = Vec::new();
        for _ in 0..n {
            // Mostly on the sixteenths.
            let at = (rng.below(16) * 2 + if rng.chance(0.2) { 1 } else { 0 }) as u8;
            if out.iter().any(|p| p.at == at) {
                continue;
            }
            let pip = Pip {
                at,
                tone: rng.below(4) as u8,
                lift: if rng.chance(0.6) { 3 } else { 4 },
                ms: rng.range(5.0, 40.0),
                vel: rng.range(0.5, 1.0),
                pan: rng.range(-0.9, 0.9),
            };
            out.push(pip);
            // Now and then a twin, a thirty-second later, on the other side.
            if at < 31 && rng.chance(0.25) {
                out.push(Pip { at: at + 1, vel: pip.vel * 0.6, pan: -pip.pan, ..pip });
            }
        }
        out.sort_by_key(|p| p.at);
        out
    }

    /// One stereo sample.
    pub fn next(&mut self, clock: u64, harmony: &Harmony, active: bool) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar)));
        }
        if active && in_bar % self.t32 == 0 {
            let slot = (in_bar / self.t32) as u8;
            let due: Vec<Pip> = self.plan.as_ref().unwrap().1.iter().copied().filter(|p| p.at == slot).collect();
            if !due.is_empty() {
                let chord = &harmony.chords[harmony.at(clock).chord];
                for p in due {
                    let tones = &chord.notes;
                    let base = tones[p.tone as usize % tones.len()];
                    // The tone's pitch class, lifted into the air.
                    let mut midi = base + 12.0 * p.lift as f64;
                    while midi > 112.0 {
                        midi -= 12.0;
                    }
                    while midi < 82.0 {
                        midi += 12.0;
                    }
                    let v = &mut self.voices[self.next_voice];
                    self.next_voice = (self.next_voice + 1) % VOICES;
                    *v = Voice {
                        freq: midi_hz(midi),
                        phase: 0.0,
                        pos: 0,
                        len: ((p.ms / 1000.0) * sr) as u64,
                        vel: p.vel,
                        pan: p.pan,
                        on: true,
                    };
                }
            }
        }
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if !v.on {
                continue;
            }
            if v.pos >= v.len {
                v.on = false;
                continue;
            }
            let w = sin_turns(0.5 * v.pos as f64 / v.len as f64);
            let s = sin_turns(v.phase) * w * w * v.vel;
            v.phase += v.freq / sr;
            v.phase -= v.phase.floor();
            v.pos += 1;
            let (gl, gr) = crate::math::pan_gains(v.pan);
            l += s * gl;
            r += s * gr;
        }
        let (el, er) = self.echo.process(l, r);
        ((l + el * ECHO_MIX) * LEVEL, (r + er * ECHO_MIX) * LEVEL)
    }
}

}
// ---- src/audio/rain.rs ----
pub mod rain {
//! The rain's wash: the broadband hiss of a field of drops too many to hear
//! one by one, with a soft low body under it. Its level *is* the rain level the
//! scene draws its streaks from, with no floor: no rain, no hiss.
//!
//! The drops you can hear individually — taps in the moss, plinks on stone —
//! are the splash events, played by [`crate::ambience`] from the same schedule
//! the scene draws its splashes from.

use crate::filter::Svf;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5241_494e; // "RAIN"
const SR: f64 = SAMPLE_RATE as f64;

/// The wash: decorrelated hiss and body in each channel.
#[derive(Clone)]
pub struct Rain {
    rng: Rng,
    hiss: [Svf; 2],
    body: [Svf; 2],
}

impl Rain {
    pub fn new(seed: u64) -> Self {
        Rain {
            rng: Rng::stream(seed, STREAM),
            hiss: [Svf::new(4_800.0, 0.6, SR), Svf::new(5_400.0, 0.6, SR)],
            body: [Svf::new(700.0, 0.7, SR), Svf::new(760.0, 0.7, SR)],
        }
    }

    /// One stereo sample for a rain level in `[0, 1]`.
    pub fn next(&mut self, rain: f64) -> (f64, f64) {
        let rain = rain.clamp(0.0, 1.0);
        let mut out = [0.0; 2];
        for (c, o) in out.iter_mut().enumerate() {
            let x = self.rng.bipolar();
            // Heavier rain is also darker: more body under the hiss.
            *o = rain * (0.55 * self.hiss[c].process(x).band + (0.25 + 0.35 * rain) * self.body[c].process(x).low);
        }
        (out[0], out[1])
    }
}

}
// ---- src/audio/reverb.rs ----
pub mod reverb {
//! Long reverb: input diffusers into an 8-line feedback delay network with a
//! Hadamard mixing matrix and damping in the loop.

use crate::filter::OnePole;
use crate::math::{decay_gain, sanitize};

/// Time for the tail to fall by 60 dB.
pub const DECAY_SECONDS: f64 = 18.0;
const DAMP_HZ: f64 = 5200.0;
/// Keeps the low end out of the long tail so it doesn't pile up into mud.
const INPUT_HIGHPASS_HZ: f64 = 180.0;
pub const LINES: [usize; 8] = [3049, 3779, 4513, 5153, 5869, 6577, 7351, 8101];
const DIFFUSERS: [[usize; 4]; 2] = [[142, 379, 107, 277], [151, 353, 113, 263]];

struct Allpass {
    buf: Vec<f64>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass { buf: vec![0.0; len], pos: 0 }
    }

    fn process(&mut self, x: f64) -> f64 {
        const G: f64 = 0.62;
        let delayed = self.buf[self.pos];
        let y = delayed - G * x;
        self.buf[self.pos] = sanitize(x + G * y);
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

struct Line {
    buf: Vec<f64>,
    pos: usize,
    gain: f64,
    damp: OnePole,
}

pub struct Reverb {
    lines: Vec<Line>,
    diffusers: [Vec<Allpass>; 2],
    highpass: [OnePole; 2],
    /// Peak level per delay line since last read (for display).
    peaks: [f64; 8],
}

impl Reverb {
    pub fn new() -> Self {
        Reverb::with_decay(DECAY_SECONDS)
    }

    /// The same network with a different decay (seconds to fall 60 dB):
    /// microsound's small, close room.
    pub fn with_decay(decay_s: f64) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        Reverb {
            lines: LINES
                .iter()
                .map(|&len| Line {
                    buf: vec![0.0; len],
                    pos: 0,
                    gain: decay_gain(len as f64, decay_s, sr),
                    damp: OnePole::new(DAMP_HZ, sr),
                })
                .collect(),
            diffusers: DIFFUSERS.map(|lens| lens.iter().map(|&l| Allpass::new(l)).collect()),
            highpass: [OnePole::new(INPUT_HIGHPASS_HZ, sr), OnePole::new(INPUT_HIGHPASS_HZ, sr)],
            peaks: [0.0; 8],
        }
    }

    /// Peak level of each delay line since the last call.
    pub fn take_peaks(&mut self) -> [f64; 8] {
        core::mem::replace(&mut self.peaks, [0.0; 8])
    }

    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let mut l = in_l - self.highpass[0].process(in_l);
        let mut r = in_r - self.highpass[1].process(in_r);
        for ap in self.diffusers[0].iter_mut() {
            l = ap.process(l);
        }
        for ap in self.diffusers[1].iter_mut() {
            r = ap.process(r);
        }

        let mut v = [0.0; 8];
        for (x, line) in v.iter_mut().zip(self.lines.iter_mut()) {
            *x = line.damp.process(line.buf[line.pos]);
        }
        for (p, x) in self.peaks.iter_mut().zip(v) {
            *p = p.max(x.abs());
        }
        let out_l = 0.5 * (v[0] - v[2] + v[4] - v[6]);
        let out_r = 0.5 * (v[1] - v[3] + v[5] - v[7]);

        // Fast Walsh-Hadamard transform, normalised to stay energy-preserving.
        let mut h = 1;
        while h < 8 {
            let mut i = 0;
            while i < 8 {
                for j in i..i + h {
                    let (a, b) = (v[j], v[j + h]);
                    v[j] = a + b;
                    v[j + h] = a - b;
                }
                i += h * 2;
            }
            h *= 2;
        }
        let norm = 1.0 / 8f64.sqrt();
        for (i, line) in self.lines.iter_mut().enumerate() {
            let input = if i % 2 == 0 { l } else { r };
            line.buf[line.pos] = sanitize(input + v[i] * norm * line.gain);
            line.pos = (line.pos + 1) % line.buf.len();
        }
        (out_l, out_r)
    }
}

}
// ---- src/audio/stabs.rs ----
pub mod stabs {
//! 2-step garage's chord stabs: the melodic, bright-eyed part of the mode.
//!
//! Each stab is the current chord as an extended voicing around middle C —
//! the third, fifth, seventh and ninth stacked from the scale, the root left
//! to the bass — struck short on the pushes that make the groove skip (the
//! "a" of one, the "and" of two, around beat three, the "a" of four), swung
//! with the drums, in a two-bar call and answer. Stabs late in the bar
//! before a change anticipate the next chord. In the glitchier sections a
//! stab can stutter, retriggering a swung thirty-second later.
//!
//! Two tones: *dark*, a soft electric-piano FM with a barking attack that
//! mellows; *bright*, a glassy bell-organ of a sine, its octave and a quickly
//! fading inharmonic bell partial. Both sit in a dotted-eighth ping-pong echo
//! and the shared reverb, under the glitch-ambient bed.
//!
//! Every bar's stabs are a pure function of seed 6 and the bar number.

use crate::delay::PingPong;
use crate::drums::Section;
use crate::garage::{StabTone, Stabs as Density};
use crate::harmony::Harmony;
use crate::math::{midi_hz, sin_turns};
use crate::rng::Rng;
use crate::tempo::Tempo;

/// Voices sounding at once (two overlapping four-note chords).
const VOICES: usize = 8;
/// Where stabs like to land, and how much (index = sixteenth in the bar).
const PUSHES: [f64; 16] = [0.05, 0.0, 0.15, 0.6, 0.0, 0.1, 0.55, 0.2, 0.0, 0.15, 0.45, 0.4, 0.0, 0.15, 0.5, 0.2];
/// Echo: a dotted quarter, six sixteenths — an even number, so every repeat
/// keeps its stab's place in the swing (a dotted eighth, three, flips each
/// repeat to the other side of it and lands it off the groove).
const ECHO_SIXTEENTHS: u64 = 6;
const ECHO_FEEDBACK: f64 = 0.38;
const ECHO_MIX: f64 = 0.32;
const LEVEL: f64 = 0.34;

/// One stab in a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub step: u8,
    /// How long it is held, in sixteenths.
    pub len: f64,
    /// Retriggers a swung thirty-second later.
    pub stutter: bool,
    pub vel: f64,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    freq: f64,
    phase: f64,
    mod_phase: f64,
    bell_phase: f64,
    env: f64,
    fenv: f64,
    vel: f64,
    pan: f64,
    release_at: u64,
    on: bool,
}

pub struct Stabs {
    seed: u64,
    density: Density,
    tone: StabTone,
    six: u64,
    bar: u64,
    push: u64,
    voices: [Voice; VOICES],
    next_voice: usize,
    plan: Option<(u64, Vec<Hit>)>,
    /// The stutter waiting to fire: (when, chord voicing).
    pending: Option<(u64, Vec<f64>, f64, u64)>,
    echo: PingPong,
    /// The progression's voicings, worked out on first use.
    voiced: Vec<Vec<f64>>,
    /// The chord last heard, and when it last changed: the echo fades out,
    /// empties and fades back in, so the old chord's repeats never ring over
    /// the new one.
    chord: usize,
    changed_at: Option<u64>,
}

/// The echo's clean-out at a chord change: fade over this many samples,
/// empty, fade back in over as many (about 20 ms in all).
const CUT: u64 = 480;

/// The seventh chord on each chord of the progression (root, third, fifth
/// and seventh from the scale), in close position around middle C and
/// voice-led: each chord takes whichever inversion lies nearest the one
/// before it, so the top line moves by step. (Scales with fewer than seven
/// notes can repeat a pitch class; then the chord is the distinct ones.)
pub fn voicings(harmony: &Harmony) -> Vec<Vec<f64>> {
    // Each chord's own tones (borrowed chords included), root first.
    let pcs = |c: &crate::harmony::Chord| -> Vec<i64> {
        let mut v: Vec<i64> = Vec::new();
        for t in &c.tones {
            if !v.contains(&(*t as i64)) {
                v.push(*t as i64);
            }
        }
        v
    };
    // Every close-position stack of the chord with its lowest note in 50..62.
    let candidates = |p: &[i64]| -> Vec<Vec<f64>> {
        let mut out = Vec::new();
        for low in 55i64..=64 {
            if let Some(r) = p.iter().position(|c| *c == low.rem_euclid(12)) {
                let mut stack = vec![low];
                for k in 1..p.len() {
                    let want = p[(r + k) % p.len()];
                    let mut m = stack[k - 1] + 1;
                    while m.rem_euclid(12) != want {
                        m += 1;
                    }
                    stack.push(m);
                }
                out.push(stack.iter().map(|m| *m as f64).collect());
            }
        }
        out
    };
    let distance = |a: &[f64], b: &[f64]| -> f64 {
        let k = a.len().min(b.len());
        (0..k).map(|i| (a[i] - b[i]).abs()).sum::<f64>() + (a.len() as f64 - b.len() as f64).abs() * 3.0
    };
    let mut out: Vec<Vec<f64>> = Vec::new();
    for c in &harmony.chords {
        let cands = candidates(&pcs(c));
        let best = match out.last() {
            // The first chord: centred on middle C and a little above.
            None => cands.iter().min_by(|a, b| {
                let centre = |v: &Vec<f64>| (v.iter().sum::<f64>() / v.len() as f64 - 63.0).abs();
                centre(a).partial_cmp(&centre(b)).unwrap()
            }),
            Some(prev) => cands.iter().min_by(|a, b| distance(a, prev).partial_cmp(&distance(b, prev)).unwrap()),
        };
        out.push(best.cloned().unwrap_or_default());
    }
    out
}

impl Stabs {
    pub fn new(seed: u64, density: Density, tone: StabTone, tempo: Tempo, swing: f64) -> Self {
        let six = tempo.drum16;
        Stabs {
            seed,
            density,
            tone,
            six,
            bar: 16 * six,
            push: ((2.0 * swing - 1.0) * six as f64) as u64,
            voices: [Voice::default(); VOICES],
            next_voice: 0,
            plan: None,
            pending: None,
            voiced: Vec::new(),
            chord: 0,
            changed_at: None,
            echo: PingPong::new((ECHO_SIXTEENTHS * six) as usize, ECHO_FEEDBACK),
        }
    }

    /// The stabs of bar `bar` in `section` (pure).
    pub fn plan(&self, bar: u64, section: Section) -> Vec<Hit> {
        if self.density == Density::Off {
            return Vec::new();
        }
        let busy = self.density == Density::Busy;
        // A two-bar call and answer, reshaped every eight bars.
        let (figure, shape) = (bar % 2, bar / 8);
        let mut rng = Rng::stream(self.seed, 0x57AB_0000 ^ (shape << 4) ^ figure);
        // The answer bar is sparser than the call.
        let want = match (busy, figure) {
            (false, 0) => 2,
            (false, _) => 1,
            (true, 0) => 3,
            (true, _) => 2,
        };
        let mut hits: Vec<Hit> = Vec::new();
        let mut weights = PUSHES;
        for _ in 0..want {
            let i = rng.weighted(&weights);
            weights[i] = 0.0;
            // Keep a neighbour from crowding it.
            if i > 0 {
                weights[i - 1] *= 0.2;
            }
            if i < 15 {
                weights[i + 1] *= 0.2;
            }
            let len = if rng.chance(0.75) { rng.range(0.6, 1.1) } else { rng.range(1.8, 3.0) };
            hits.push(Hit { step: i as u8, len, stutter: false, vel: rng.range(0.75, 1.0) });
        }
        hits.sort_by(|a, b| a.step.cmp(&b.step));
        match section {
            // In the breakdown the stabs bloom into held chords.
            Section::Breakdown => {
                for h in hits.iter_mut() {
                    h.len = h.len.max(2.5);
                }
            }
            Section::Shuffle => {
                let mut g = Rng::stream(self.seed, 0x57AB_5700 ^ bar);
                for h in hits.iter_mut() {
                    h.stutter = g.chance(0.35);
                }
            }
            _ => {}
        }
        hits
    }

    fn onset(&self, step: u8) -> u64 {
        step as u64 * self.six + if step % 2 == 1 { self.push } else { 0 }
    }

    fn strike(&mut self, clock: u64, chord: &[f64], vel: f64, len: u64) {
        let mut r = Rng::new(crate::rng::hash3(self.seed, 0x57AB, clock));
        for (k, m) in chord.iter().enumerate() {
            let v = &mut self.voices[self.next_voice];
            self.next_voice = (self.next_voice + 1) % VOICES;
            *v = Voice {
                freq: midi_hz(*m),
                phase: 0.0,
                mod_phase: 0.0,
                bell_phase: 0.0,
                env: 0.0,
                fenv: 1.0,
                vel: vel * r.range(0.85, 1.0),
                // Spread across the field, low notes nearer the middle.
                pan: (k as f64 - 1.5) / 1.5 * 0.45 + r.range(-0.1, 0.1),
                // A hair of strum: each note a few samples later.
                release_at: clock + len + k as u64 * 40,
                on: true,
            };
        }
    }

    /// One stereo sample. `active` follows the drums' sections; `section` is
    /// the current one.
    pub fn next(&mut self, clock: u64, harmony: &Harmony, changing: bool, active: bool, section: Section) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar, section)));
        }
        if self.voiced.is_empty() {
            self.voiced = voicings(harmony);
        }
        if clock % 32 == 0 {
            let now = harmony.at(clock).chord;
            if now != self.chord {
                self.chord = now;
                self.changed_at = Some(clock);
            }
        }
        if active {
            let hit = self.plan.as_ref().unwrap().1.iter().copied().find(|h| self.onset(h.step) == in_bar);
            if let Some(h) = hit {
                let p = harmony.at(clock);
                // Late in the bar before a change: anticipate the next chord.
                let chord = if changing && h.step >= 12 { p.next } else { p.chord };
                let notes = self.voiced[chord].clone();
                let len = (h.len * self.six as f64) as u64;
                self.strike(clock, &notes, h.vel, len);
                if h.stutter {
                    let gap = (self.six - if h.step % 2 == 1 { self.push } else { 0 }) / 2;
                    self.pending = Some((clock + gap.max(1), notes, h.vel * 0.7, len / 2));
                }
            }
            if self.pending.as_ref().is_some_and(|p| p.0 == clock) {
                let (_, notes, vel, len) = self.pending.take().unwrap();
                self.strike(clock, &notes, vel, len);
            }
        }
        let (mut l, mut r) = (0.0, 0.0);
        let bright = self.tone == StabTone::Bright;
        for v in self.voices.iter_mut() {
            if !v.on {
                continue;
            }
            let gate = clock < v.release_at;
            v.env += if gate { (1.0 - v.env) * 0.06 } else { -v.env * 0.0016 };
            if !gate && v.env < 1e-4 {
                v.on = false;
                continue;
            }
            // The tone itself, decaying while held.
            let s = if bright {
                v.fenv *= 0.99955;
                let bell = sin_turns(v.bell_phase) * 0.35 * v.fenv * v.fenv;
                // A whole-number overtone (an octave and a fifth): bell-like,
                // but in tune with the chord.
                v.bell_phase += v.freq * 3.0 / sr;
                v.bell_phase -= v.bell_phase.floor();
                sin_turns(v.phase) * 0.7 + sin_turns(2.0 * v.phase) * 0.25 + bell
            } else {
                v.fenv *= 0.99985;
                let index = 0.25 + 1.4 * v.fenv * v.fenv;
                let m = sin_turns(v.mod_phase) * index / crate::math::TAU;
                v.mod_phase += v.freq / sr;
                v.mod_phase -= v.mod_phase.floor();
                sin_turns(v.phase + m)
            };
            v.phase += v.freq / sr;
            v.phase -= v.phase.floor();
            // A slow natural decay under the gate.
            let held = 0.55 + 0.45 * v.fenv;
            let x = s * v.env * held * v.vel;
            let (gl, gr) = crate::math::pan_gains(v.pan);
            l += x * gl;
            r += x * gr;
        }
        let (el, er) = self.echo.process(l * ECHO_MIX, r * ECHO_MIX);
        let gate = match self.changed_at {
            Some(c) if clock - c < CUT => 1.0 - (clock - c) as f64 / CUT as f64,
            Some(c) if clock - c == CUT => {
                self.echo.clear();
                0.0
            }
            Some(c) if clock - c < 2 * CUT => (clock - c - CUT) as f64 / CUT as f64,
            _ => 1.0,
        };
        ((l + el * gate) * LEVEL, (r + er * gate) * LEVEL)
    }
}



}
// ---- src/audio/telemetry.rs ----
pub mod telemetry {
//! Read-only views into the running engine, for visualisation.
//!
//! Nothing here feeds back into the sound: `tick` only ever *writes* the
//! display state, so the golden-hash test is unaffected by any of it.

use crate::drone::{Recipe, HOP, N};
use crate::drums::{Plan, Voice};
use crate::glitch::Kind;
use crate::harmony::{Harmony, Position};
use crate::modulate::ModState;
use crate::pattern::{Layer, Mutation};
use crate::{Seeds, Track};

/// Peak absolute level of each component since the last snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    pub drone: f64,
    pub bass: f64,
    pub glitch1: f64,
    pub glitch2: f64,
    pub echo_l: f64,
    pub echo_r: f64,
    pub reverb: f64,
    pub drums: f64,
    /// The drums' tape echo return.
    pub tape: f64,
    /// Atmosphere loops 1 and 2.
    pub loop1: f64,
    pub loop2: f64,
    pub loop3: f64,
    pub loop4: f64,
    pub out_l: f64,
    pub out_r: f64,
}

/// Everything about the track that never changes while it plays.
#[derive(Clone, Debug)]
pub struct Description {
    pub seeds: Seeds,
    pub settings: crate::Settings,
    /// The tempo (drums at `bpm`, ambient layers at half).
    pub tempo: crate::tempo::Tempo,
    pub sample_rate: u32,
    /// Scale, key and the chords (notes, bass, bars).
    pub harmony: Harmony,
    /// Each chord's FM recipes, one per note.
    pub recipes: Vec<Vec<Recipe>>,
    /// Each chord's frozen left-channel magnitude spectrum, bins 0..=fft_size/2.
    pub magnitudes: Vec<Vec<f64>>,
    /// Drums: on/off, the break pattern's name, its waveform (peaks), the main voice per slice.
    pub drums_on: bool,
    pub break_pattern: &'static str,
    pub break_overview: Vec<f32>,
    pub slice_voices: Vec<Option<Voice>>,
    /// The drum kit family and how each voice is made.
    pub drum_voices: crate::drums::VoiceInfo,
    /// The drum section of each of the first 128 four-bar phrases (all "out"
    /// when the kit is Off): the arrangement, known in advance.
    pub sections: Vec<crate::drums::Section>,
    pub fft_size: usize,
    pub hop: usize,
    /// Per glitch layer: (steps, step length in samples).
    pub layers: [(usize, u64); 2],
    pub echo_delay: usize,
    pub echo_feedback: f64,
    pub reverb_lines: [usize; 8],
    pub reverb_decay: f64,
    pub bass_root: f64,
    /// The atmosphere loop layers (a layer that is Off has no chords).
    pub loops: [LoopInfo; 4],
    /// The repeating entrance/exit arc of the frozen-chord drone.
    pub drone_arc: crate::DroneArc,
}

/// One atmosphere loop layer, as built at start-up.
#[derive(Clone, Debug)]
pub struct LoopInfo {
    pub design: crate::LoopDesign,
    pub beats: u64,
    /// Loop length in samples.
    pub len: usize,
    /// The loop-point crossfade, in samples.
    pub xfade: usize,
    /// Per chord: the notes it plays, and its waveform (left, 256 peaks).
    pub notes: Vec<Vec<f64>>,
    pub overviews: Vec<Vec<f32>>,
}

impl LoopInfo {
    pub fn on(&self) -> bool {
        !self.notes.is_empty()
    }
}

fn loop_info(l: &crate::atmos::AtmosLoop) -> LoopInfo {
    let (beats, len) = l.length();
    let perfs = l.performances();
    LoopInfo {
        design: l.design(),
        beats,
        len,
        xfade: crate::atmos::XFADE,
        notes: perfs.iter().map(|p| p.notes.clone()).collect(),
        overviews: (0..perfs.len()).map(|i| l.overview(i, 256)).collect(),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StepInfo {
    pub kind: Kind,
    pub freq: f64,
    pub ratchet: u32,
    /// The step is silent while the layer's density is below this.
    pub threshold: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceInfo {
    pub kind: Kind,
    pub progress: f64,
    pub pan: f64,
}

#[derive(Clone, Debug)]
pub struct LayerSnapshot {
    pub steps: Vec<Option<StepInfo>>,
    /// Current step and how far through it (0..1).
    pub step: usize,
    pub step_phase: f64,
    pub loops: u64,
    pub loops_until_mutation: u64,
    pub density: f64,
    pub voices: Vec<VoiceInfo>,
    /// (clock, step, what) of the latest mutation.
    pub last_mutation: Option<(u64, usize, Mutation)>,
    pub triggers: u64,
}

/// The engine's moving parts at one moment.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub clock: u64,
    pub mods: ModState,
    /// (period in seconds, value -1..1) per slow LFO.
    pub lfos: [(u64, f64); 7],
    pub drone_cutoff: f64,
    pub drone_q: f64,
    pub drone_frames: u64,
    pub drone_offset: usize,
    /// Peak envelope of the newest drone frame, when a new one started sounding.
    pub new_frame: Option<Vec<f32>>,
    pub layers: [LayerSnapshot; 2],
    /// (FM ratio, modulation index, low-pass cutoff Hz).
    pub bass: (f64, f64, f64),
    pub reverb_lines: [f64; 8],
    pub meters: Meters,
    /// The layer soloed on the monitor (if any) and how far faded in.
    pub solo: Option<crate::Solo>,
    pub solo_amount: f64,
    /// The drone's gain from the drum-aware mix (1 = untouched).
    pub duck_gain: f64,
    /// The drone's gain from its repeating entrance/exit arc (1 = always on).
    pub drone_window: f64,
    /// Each atmosphere layer's gain from its own arc (1 = always on).
    pub loop_windows: [f64; 4],
    /// The bass's current (gliding) root, MIDI.
    pub bass_root: f64,
    /// Where the chord progression is.
    pub harmony: Position,
    /// The bar being played by the drums, the step within it, the hits on the
    /// latest step, and a running hit count.
    pub drum_plan: Option<Plan>,
    pub drum_step: usize,
    pub drum_hits: Vec<Voice>,
    pub drum_hit_count: u64,
}

fn layer_snapshot(layer: &Layer, clock: u64, density: f64) -> LayerSnapshot {
    let cfg = layer.config();
    let (loops, loops_until_mutation) = layer.loops();
    // The step that sounded last (clock has already advanced past it).
    let at = clock.saturating_sub(1);
    LayerSnapshot {
        steps: layer
            .pattern()
            .iter()
            .map(|s| {
                s.map(|e| StepInfo { kind: e.kind, freq: e.freq, ratchet: e.ratchet, threshold: e.threshold })
            })
            .collect(),
        step: ((at / cfg.step_len) % cfg.steps as u64) as usize,
        step_phase: (at % cfg.step_len) as f64 / cfg.step_len as f64,
        loops,
        loops_until_mutation,
        density,
        voices: layer
            .voices()
            .map(|v| VoiceInfo { kind: v.event().kind, progress: v.progress(), pan: v.event().pan })
            .collect(),
        last_mutation: layer.last_mutation(),
        triggers: layer.triggers(),
    }
}

impl Track {
    pub fn describe(&self) -> Description {
        let (delay, feedback) = self.echo.settings();
        // The drone's own chords (in 2-step garage it holds one pedal chord
        // under the progression).
        let chords = self.drone.harmony().chords.len();
        Description {
            seeds: self.seeds,
            settings: self.settings,
            tempo: self.tempo,
            sample_rate: crate::SAMPLE_RATE,
            harmony: self.harmony.clone(),
            recipes: (0..chords).map(|i| self.drone.recipes(i).to_vec()).collect(),
            magnitudes: (0..chords).map(|i| self.drone.magnitudes(i).0.to_vec()).collect(),
            drums_on: self.drums.enabled(),
            break_pattern: self.drums.pattern_name(),
            break_overview: self.drums.overview(512),
            slice_voices: self.drums.slice_voices(),
            drum_voices: self.drums.voice_info(),
            sections: (0..128u64)
                .map(|p| {
                    if self.drums.enabled() {
                        self.drums.plan(p * crate::drums::PHRASE_BARS, &self.mods).section
                    } else {
                        crate::drums::Section::Out
                    }
                })
                .collect(),
            fft_size: N,
            hop: HOP,
            layers: [
                (self.glitch1.config().steps, self.glitch1.config().step_len),
                (self.glitch2.config().steps, self.glitch2.config().step_len),
            ],
            echo_delay: delay,
            echo_feedback: feedback,
            reverb_lines: crate::reverb::LINES,
            reverb_decay: crate::reverb::DECAY_SECONDS,
            bass_root: crate::bass::ROOT,
            loops: std::array::from_fn(|i| loop_info(&self.loops[i])),
            drone_arc: self.settings.drone,
        }
    }

    /// The current state; also resets the peak meters.
    pub fn snapshot(&mut self) -> Snapshot {
        let clock = self.clock;
        let mods = self.mods.at(clock.saturating_sub(1));
        let frames = self.drone.frames();
        let new_frame = (frames != self.overview_frame).then(|| {
            self.overview_frame = frames;
            self.drone.newest_frame_overview(256)
        });
        Snapshot {
            clock,
            mods,
            lfos: self.mods.lfos(clock),
            drone_cutoff: self.drone_cutoff,
            drone_q: self.drone_q,
            drone_frames: frames,
            drone_offset: self.drone.offset(),
            new_frame,
            layers: [
                layer_snapshot(&self.glitch1, clock, mods.density1),
                layer_snapshot(&self.glitch2, clock, mods.density2),
            ],
            bass: self.bass.state(),
            reverb_lines: self.reverb.take_peaks(),
            meters: core::mem::take(&mut self.meters),
            bass_root: self.bass.root(),
            duck_gain: self.duck_gain,
            drone_window: self.drone_window,
            loop_windows: self.loop_windows,
            solo: self.solo().0,
            solo_amount: self.solo().1,
            harmony: self.harmony.at(clock.saturating_sub(1)),
            drum_plan: self.drums.plan_now().cloned(),
            drum_step: self.drums.step(),
            drum_hits: self.drums.last_hits().to_vec(),
            drum_hit_count: self.drums.hit_count(),
        }
    }

    /// Samples rendered so far.
    pub fn clock(&self) -> u64 {
        self.clock
    }
}

}
// ---- src/audio/tempo.rs ----
pub mod tempo {
//! The tempo: one BPM setting for the drums, with every ambient layer
//! (glitch grid, chord bars, echoes) locked at exactly half of it.
//!
//! All timing is whole samples, so it can never drift. A drum sixteenth is
//! rounded to a multiple of 12 samples, which keeps 2-, 3- and 4-way glitch
//! ratchets and the drums' half-step rolls exact; the cost is a tempo at
//! most about 0.1 % away from the number asked for. At 168 BPM a drum
//! sixteenth is 4,284 samples: exactly 0010's timing.


pub const MIN_BPM: u16 = 70;
pub const MAX_BPM: u16 = 180;
pub const DEFAULT_BPM: u16 = 168;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tempo {
    /// The drum tempo asked for.
    pub bpm: u16,
    /// A drum sixteenth, in samples (a multiple of 12).
    pub drum16: u64,
}

impl Tempo {
    pub fn new(bpm: u16) -> Self {
        let bpm = bpm.clamp(MIN_BPM, MAX_BPM) as u64;
        // round(60·SR / (bpm·4) / 12) · 12, in integers.
        let per_minute = 60 * crate::SAMPLE_RATE as u64;
        let n = (per_minute + 24 * bpm) / (48 * bpm);
        Tempo { bpm: bpm as u16, drum16: 12 * n }
    }

    /// A glitch-grid sixteenth: half-time, so twice a drum sixteenth.
    pub fn glitch16(self) -> u64 {
        2 * self.drum16
    }

    /// One half-time beat (four glitch sixteenths).
    pub fn beat(self) -> u64 {
        4 * self.glitch16()
    }

    /// One drum bar (16 drum sixteenths, = two half-time beats).
    pub fn drum_bar(self) -> u64 {
        16 * self.drum16
    }

    /// The drum tempo actually played, after rounding to whole samples.
    pub fn exact_bpm(self) -> f64 {
        60.0 * crate::SAMPLE_RATE as f64 / (4.0 * self.drum16 as f64)
    }

    /// The half-time tempo the ambient layers move at.
    pub fn half_bpm(self) -> f64 {
        self.exact_bpm() / 2.0
    }
}



}
// ---- src/audio/texture.rs ----
pub mod texture {
//! Physical texture loops: an editable event grid.
//!
//! The sustained timbres in `atmos` are pads; this module makes the *foley*
//! kind of atmosphere — fire crackling, water running, stones knocking, wood
//! creaking, rocks and logs struck, cave drips and natural chimes — as short
//! synthesised grains placed on an 8 x 16 grid (eight event rows, sixteen
//! steps). Nothing is sampled and nothing is a pure tone: each hit is a short
//! burst of noise exciting a few heavily damped, inharmonic resonances, all
//! rolled off through a low-pass. That is what keeps the hits dull and woody
//! rather than ringing. The grains are wrapped circularly into the loop the
//! same way the sustained timbres are, so the seam is hidden.
//!
//! The grid is the design. The Fire / Water / Stones / Wood presets are just
//! starting grids; every bit is editable in setup and stored in the recipe.

use crate::filter::Svf;
use crate::math::{cos_turns, exp, midi_hz, sin_turns};
use crate::rng::Rng;

const SR: f64 = crate::SAMPLE_RATE as f64;

/// Event rows.
pub const ROWS: usize = 8;
/// Steps per grid row.
pub const STEPS: usize = 16;
/// The rows, top to bottom, for the editor and the recipe.
pub const EVENT_NAMES: [&str; ROWS] = ["Crackle", "Knock", "Creak", "Hiss", "Rock", "Log", "Drip", "Chime"];

/// An 8 x 16 on/off grid: one bitmask per event row (row 0 in the low bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub rows: [u16; ROWS],
}

impl Grid {
    pub const EMPTY: Grid = Grid { rows: [0; ROWS] };

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|r| *r == 0)
    }

    pub fn get(&self, row: usize, step: usize) -> bool {
        self.rows[row] & (1 << step) != 0
    }

    pub fn set(&mut self, row: usize, step: usize, on: bool) {
        if on {
            self.rows[row] |= 1 << step;
        } else {
            self.rows[row] &= !(1 << step);
        }
    }

    pub fn toggle(&mut self, row: usize, step: usize) {
        self.rows[row] ^= 1 << step;
    }

    /// The whole grid as 128 bits (row 0 in the low 16 bits).
    pub fn bits(&self) -> u128 {
        self.rows.iter().enumerate().fold(0u128, |b, (i, r)| b | ((*r as u128) << (16 * i)))
    }

    pub fn from_bits(bits: u128) -> Grid {
        let mut rows = [0u16; ROWS];
        for (i, r) in rows.iter_mut().enumerate() {
            *r = (bits >> (16 * i)) as u16;
        }
        Grid { rows }
    }

    /// Four 32-bit words, for the WASM ABI.
    pub fn words(&self) -> [u32; 4] {
        let b = self.bits();
        [b as u32, (b >> 32) as u32, (b >> 64) as u32, (b >> 96) as u32]
    }

    pub fn from_words(w: [u32; 4]) -> Grid {
        let b = (w[0] as u128) | ((w[1] as u128) << 32) | ((w[2] as u128) << 64) | ((w[3] as u128) << 96);
        Grid::from_bits(b)
    }

    /// 32 lowercase hex digits, used as the recipe token.
    pub fn to_hex(&self) -> String {
        format!("{:032x}", self.bits())
    }

    pub fn from_hex(s: &str) -> Option<Grid> {
        // The old 4-row form (16 digits) is the low half of the new one.
        let padded;
        let s = match s.len() {
            32 => s,
            16 => {
                padded = format!("{}{s}", "0".repeat(16));
                &padded
            }
            _ => return None,
        };
        u128::from_str_radix(s, 16).ok().map(Grid::from_bits)
    }

    pub fn fire() -> Grid {
        Grid { rows: [0b1110_1110_1110_1110, 0b0000_0010_0000_0010, 0, 0b1000_0000_1000_0000, 0, 0, 0, 0] }
    }

    pub fn water() -> Grid {
        Grid { rows: [0, 0b0001_0001_0001_0001, 0, 0b1111_1111_1111_1111, 0, 0, 0, 0] }
    }

    pub fn stones() -> Grid {
        Grid { rows: [0b0001_0000_0100_0000, 0b1010_1010_1010_1010, 0, 0, 0b1000_0010_0000_1000, 0, 0, 0] }
    }

    pub fn wood() -> Grid {
        Grid { rows: [0, 0b0000_0010_0000_0010, 0b1010_1010_1010_1010, 0, 0, 0b1010_1010_1010_1010, 0, 0] }
    }
}

/// Render one channel of an event-grid loop over `total` samples, for a loop of
/// `len`. Grains are laid down for three passes (warm-up, loop, crossfade), so
/// tails from the end of one cycle are already present at the start of the next
/// and `close_loop` has a real continuation to fade in.
pub fn render_channel(grid: Grid, notes: &[f64], rng: &mut Rng, len: usize, total: usize) -> Vec<f64> {
    let mut out = vec![0.0; total];
    let mut noise = Rng::new(rng.next_u64());
    let step_len = (len / STEPS).max(1);
    for pass in 0..3usize {
        let base = pass * len;
        for row in 0..ROWS {
            let mut bits = grid.rows[row];
            while bits != 0 {
                let step = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let start = base + step * step_len;
                if start < total {
                    grain(row, &mut out, start, notes, &mut noise, rng);
                }
            }
        }
    }
    out
}

fn add(out: &mut [f64], i: usize, x: f64) {
    if let Some(o) = out.get_mut(i) {
        *o += x;
    }
}

/// One synthesised event, written into `out` at `start`.
fn grain(kind: usize, out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    match kind {
        0 => crackle(out, start, noise, rng),
        1 => knock(out, start, notes, noise, rng),
        2 => creak(out, start, noise, rng),
        3 => hiss(out, start, noise, rng),
        4 => rock(out, start, noise, rng),
        5 => log(out, start, notes, noise, rng),
        6 => drip(out, start, notes, noise, rng),
        _ => chime(out, start, notes, noise, rng),
    }
}

/// Noise-excited damped modal synthesis: a short burst of noise drives a bank
/// of band-passes, each damped fast. Low Q and short taus keep it woody and
/// dull rather than ringing, and the inharmonic centres keep it unpitched. This
/// is the organic core every struck hit is built on.
fn modes(out: &mut [f64], start: usize, list: &[(f64, f64, f64, f64)], excite_ms: f64, lp_hz: f64, noise: &mut Rng) {
    let tau = list.iter().map(|m| m.2).fold(0.0, f64::max);
    let dur = ((tau * 6.0 * SR) as usize).max(1);
    let excite = ((excite_ms * 0.001 * SR) as usize).min(dur);
    let mut fs: Vec<Svf> = list.iter().map(|m| Svf::new(m.0, m.1, SR)).collect();
    let cs: Vec<f64> = list.iter().map(|m| exp(-1.0 / (m.2 * SR))).collect();
    let mut envs: Vec<f64> = list.iter().map(|m| m.3).collect();
    let mut lp = Svf::new(lp_hz, 0.7, SR);
    for i in 0..dur {
        let x = if i < excite { noise.bipolar() } else { 0.0 };
        let mut y = 0.0;
        for k in 0..list.len() {
            y += envs[k] * fs[k].process(x).band;
            envs[k] *= cs[k];
        }
        add(out, start + i, lp.process(y).low);
    }
}

/// A short burst of low-passed noise: the dull direct impact of one object on
/// another, with no resonance of its own.
fn thud(out: &mut [f64], start: usize, dur_ms: f64, lp_hz: f64, amp: f64, noise: &mut Rng, rng: &mut Rng) {
    let dur = ((dur_ms * 0.001 * SR) as usize).max(1);
    let tau = (dur as f64 / SR) * rng.range(0.25, 0.5);
    let mut f = Svf::new(lp_hz, 0.8, SR);
    let c = exp(-1.0 / (tau * SR));
    let mut env = amp;
    for i in 0..dur {
        add(out, start + i, f.process(noise.bipolar()).low * env);
        env *= c;
    }
}

/// A short burst of band-passed noise: the bright tick at the moment of
/// contact. Low Q, so it is a tick rather than a ping.
fn click(out: &mut [f64], start: usize, dur_ms: f64, hz: f64, q: f64, amp: f64, noise: &mut Rng, rng: &mut Rng) {
    let dur = ((dur_ms * 0.001 * SR) as usize).max(1);
    let tau = (dur as f64 / SR) * rng.range(0.2, 0.4);
    let mut f = Svf::new(hz, q, SR);
    let c = exp(-1.0 / (tau * SR));
    let mut env = amp;
    for i in 0..dur {
        add(out, start + i, f.process(noise.bipolar()).band * env);
        env *= c;
    }
}

/// A crackle: a small cluster of tiny ticks, gone in tens of milliseconds, so
/// a sparse grid still hisses like fire rather than clicking once.
fn crackle(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let pops = rng.int(2, 4);
    let mut at = start;
    for _ in 0..pops {
        click(out, at, rng.range(2.0, 7.0), rng.range(1_600.0, 5_200.0), rng.range(1.0, 2.2), rng.range(0.5, 1.0), noise, rng);
        at += (rng.range(0.003, 0.018) * SR) as usize;
    }
}

/// A knock: knuckle on wood. A dry click plus two or three short, inharmonic,
/// heavily damped modes — no sustained ring.
fn knock(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 62.0 } else { notes[rng.below(notes.len())] };
    let hz = midi_hz(base + rng.range(-3.0, 3.0)) * rng.range(0.7, 1.0);
    click(out, start, rng.range(1.0, 2.5), hz * 6.0, 0.9, 0.5, noise, rng);
    let m = [
        (hz, rng.range(3.0, 6.0), rng.range(0.015, 0.04), 1.0),
        (hz * rng.range(2.2, 2.8), rng.range(2.5, 5.0), rng.range(0.01, 0.03), rng.range(0.4, 0.7)),
        (hz * rng.range(3.7, 4.8), rng.range(2.0, 4.0), rng.range(0.008, 0.02), rng.range(0.2, 0.4)),
    ];
    modes(out, start, &m, rng.range(0.4, 1.0), 3_200.0, noise);
}

/// A creak: band-passed noise whose centre bends upward with a rough,
/// stick-slip envelope. Left almost unpitched and rolled off, so it reads as
/// wood under load rather than a resonant sweep.
fn creak(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(160.0, 560.0);
    let f1 = f0 * rng.range(1.2, 2.0);
    let q = rng.range(2.0, 4.5);
    let tau = rng.range(0.1, 0.32);
    let dur = (tau * 6.0 * SR) as usize;
    let rate = rng.int(4, 9) as f64;
    let phase = rng.unit();
    let mut f = Svf::default();
    let mut lp = Svf::new(rng.range(1_800.0, 3_500.0), 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        let rough = 0.5 + 0.5 * sin_turns(rate * u + phase);
        let env = (0.5 - 0.5 * cos_turns(0.5 * u)) * rough * rough;
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low * env * 0.8);
    }
}

/// Hiss / flow: noise through a slowly moving, low-Q band-pass, rolled off.
/// The running-water bed.
fn hiss(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(400.0, 2_400.0);
    let f1 = f0 * rng.range(0.6, 1.6);
    let q = rng.range(0.7, 1.8);
    let tau = rng.range(0.25, 0.7);
    let dur = (tau * 4.0 * SR) as usize;
    let mut f = Svf::new(f0, q, SR);
    let mut lp = Svf::new(3_000.0, 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        let env = 0.5 - 0.5 * cos_turns(u);
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low * env * 0.6);
    }
}

/// A rock struck: mostly dull noise, with only one short low mode for body.
/// Unpitched and over quickly, like a stone landing on stone.
fn rock(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    thud(out, start, rng.range(8.0, 22.0), rng.range(600.0, 1_400.0), 1.0, noise, rng);
    click(out, start, rng.range(1.0, 2.5), rng.range(800.0, 2_000.0), rng.range(0.8, 1.4), rng.range(0.4, 0.8), noise, rng);
    let m = [(rng.range(80.0, 160.0), rng.range(2.0, 4.0), rng.range(0.02, 0.05), rng.range(0.4, 0.8))];
    modes(out, start, &m, rng.range(0.4, 1.0), 1_800.0, noise);
}

/// A hollow log hit: dull and low, with a hollow low mode and one inharmonic
/// partner. Woodier and lower than a knock.
fn log(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 55.0 } else { notes[rng.below(notes.len())] - 5.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0)) * rng.range(0.5, 0.8);
    click(out, start, rng.range(1.0, 2.5), hz * 7.0, 1.0, 0.5, noise, rng);
    let m = [
        (hz, rng.range(3.5, 6.0), rng.range(0.03, 0.07), 1.0),
        (hz * rng.range(1.9, 2.5), rng.range(2.5, 4.5), rng.range(0.015, 0.04), rng.range(0.3, 0.6)),
    ];
    modes(out, start, &m, rng.range(0.4, 1.0), 2_200.0, noise);
}

/// A cave/water drip: a very short plink that bends upward, with a tick at the
/// impact. Kept brief and rolled off so it does not ring.
fn drip(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 72.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz0 = midi_hz(base) * rng.range(0.9, 1.1);
    let hz1 = hz0 * rng.range(1.4, 2.0);
    let q = rng.range(3.0, 6.0);
    let tau = rng.range(0.02, 0.045);
    let dur = (tau * 7.0 * SR) as usize;
    click(out, start, 1.0, hz1 * 2.0, 1.0, 0.2, noise, rng);
    let mut f = Svf::default();
    let mut lp = Svf::new(4_500.0, 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 16 == 0 {
            f.set(hz0 + (hz1 - hz0) * u, q, SR);
        }
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low);
    }
}

/// A natural chime: four inharmonic, damped modes — brighter and a little
/// longer than the other hits, but still short and rolled off rather than a
/// ringing bell.
fn chime(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 79.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0));
    click(out, start, rng.range(0.8, 1.8), hz * 4.0, 1.0, 0.3, noise, rng);
    let m = [
        (hz, rng.range(4.0, 7.0), rng.range(0.09, 0.2), 1.0),
        (hz * rng.range(2.6, 3.2), rng.range(3.5, 6.0), rng.range(0.07, 0.16), rng.range(0.5, 0.8)),
        (hz * rng.range(5.0, 6.0), rng.range(3.0, 5.0), rng.range(0.05, 0.11), rng.range(0.25, 0.5)),
        (hz * rng.range(8.0, 9.8), rng.range(2.5, 4.5), rng.range(0.03, 0.08), rng.range(0.1, 0.3)),
    ];
    modes(out, start, &m, rng.range(0.4, 0.9), 4_500.0, noise);
}



}
// ---- src/audio/thunder.rs ----
pub mod thunder {
//! Thunder: one roll per lightning strike, arriving after the flash.
//!
//! The strike's distance (from the shared schedule in [`crate::weather`])
//! sets everything: how long after the flash the thunder arrives, whether it
//! opens with a crack, how dark the rumble is and how long it rolls on.

use crate::filter::Svf;
use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// The most rolls that can overlap.
const MAX_ROLLS: usize = 4;

#[derive(Clone)]
struct Roll {
    /// Samples until it starts.
    wait: usize,
    age: usize,
    rng: Rng,
    rumble: [Svf; 2],
    crack: Svf,
    crack_env: f64,
    crack_coef: f64,
    gain: f64,
    attack: f64,
    decay: f64,
    /// Slow swells in the roll, as echoes come back off far hills.
    rolls: [(f64, f64); 3],
    pan: (f64, f64),
}

impl Roll {
    fn next(&mut self) -> (f64, f64) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0);
        }
        let s = self.age as f64 / SR;
        self.age += 1;
        let env = (s / self.attack).min(1.0) * crate::math::exp(-s / self.decay);
        let mut swell = 0.0;
        for (hz, at) in self.rolls {
            swell += 0.5 + 0.5 * sin_turns(s * hz + at);
        }
        let swell = 0.4 + 0.6 * swell / 3.0;
        let x = self.rng.bipolar();
        let dark = self.rumble[0].process(x).low;
        let rumble = self.rumble[1].process(dark).low * 3.0;
        let crack = self.crack.process(x).band * self.crack_env;
        self.crack_env *= self.crack_coef;
        let y = self.gain * (rumble * env * swell + crack);
        (y * self.pan.0, y * self.pan.1)
    }

    fn done(&self) -> bool {
        self.wait == 0 && self.age as f64 / SR > 1.0 + 6.0 * self.decay
    }
}

/// All the thunder currently rolling.
#[derive(Clone, Default)]
pub struct Thunder {
    rolls: Vec<Roll>,
}

impl Thunder {
    /// A strike's thunder, starting `wait` samples from now. `strength` in
    /// `[0.4, 1]`, `distance` in `[0, 1]`, `pan` in `[-1, 1]`, `hash` its seed.
    pub fn add(&mut self, wait: usize, strength: f64, distance: f64, pan: f64, hash: u64) {
        if self.rolls.len() >= MAX_ROLLS {
            self.rolls.remove(0);
        }
        let mut rng = Rng::new(hash ^ 0x5448_554e);
        let cutoff = 90.0 + 160.0 * (1.0 - distance);
        let close = (1.0 - 2.0 * distance).max(0.0);
        let (l, r) = pan_gains(pan * 0.7);
        let rolls = [(rng.range(0.4, 0.9), rng.unit()), (rng.range(1.1, 2.0), rng.unit()), (rng.range(2.3, 3.4), rng.unit())];
        self.rolls.push(Roll {
            wait,
            age: 0,
            rumble: [Svf::new(cutoff, 0.7, SR), Svf::new(cutoff * 1.4, 0.6, SR)],
            crack: Svf::new(2_400.0, 0.7, SR),
            crack_env: 1.6 * close,
            crack_coef: decay_coef(0.05, SR),
            gain: strength * (1.0 - 0.55 * distance),
            attack: 0.04 + 0.25 * distance,
            decay: 1.2 + 2.6 * distance,
            rolls,
            pan: (l, r),
            rng,
        });
    }

    pub fn next(&mut self) -> (f64, f64) {
        let (mut l, mut r) = (0.0, 0.0);
        for roll in self.rolls.iter_mut() {
            let (a, b) = roll.next();
            l += a;
            r += b;
        }
        self.rolls.retain(|roll| !roll.done());
        (l, r)
    }
}

}
// ---- src/audio/track.rs ----
pub mod track {
//! 0011 — an endless Glitch Ambient track with jungle breaks, built as a
//! software artifact.
//!
//! ```text
//! seed1 → chord progression (scale, key) → 3-op FM chords → random-phase FFT freeze + morphs → filter → drone arc ─┐
//! seed2 → glitch artifacts 1 → cyclic repeats 1 ─┐                                                    │
//! seed3 → glitch artifacts 2 → cyclic repeats 2 ─┴→ echo/delay ───────────────────────────────────────┼→ long reverb
//! seed4 → FM bass + sub-bass (follows chord roots) → low-pass ─────────────────────────────────────────┤
//! seed5 → synthesised break → sampler colour → chopper / arranger → drum bus ─────────────────────────┘
//! ```
//!
//! Determinism is the core design constraint: given the same seeds, the
//! output is bit-identical on every platform (native and WASM), at any
//! render block size, and with any future compiler. See `math.rs`.




use crate::atmos::{AtmosLoop, LoopTimbre};
use crate::texture::Grid;
use crate::bass::Bass;
use crate::delay::{PingPong, TapeEcho};
use crate::drone::Drone;
use crate::drums::{Drums, Section};
use crate::harmony::{Harmony, Pace, Scale};
use crate::master::{MasterChain, MasterFx};
use crate::overlay::CodeOverlay;
use crate::rng::Rng;
use crate::filter::{DcBlock, OnePole, Svf};
use crate::glitch::History;
use crate::modulate::Mods;
use crate::pattern::Layer;
use crate::reverb::Reverb;
use crate::telemetry::Meters;
use crate::tempo::Tempo;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seeds {
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    /// Shapes the drum kit's sounds and break (the kit itself is a setting).
    pub s5: u64,
    /// Shapes the atmosphere loops (their timbres are settings).
    pub s6: u64,
    /// Decides the drone's rhythmic amplitude LFO (off by default).
    pub s7: u64,
}

/// The canonical version of the track.
pub const DEFAULT_SEEDS: Seeds = Seeds { s1: 1000, s2: 9, s3: 1009, s4: 2026, s5: 168, s6: 11, s7: 7 };

/// How one atmosphere layer is made: one of the sustained timbres, or an
/// editable event grid of physical-texture grains (fire, water, stones, wood).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopDesign {
    Sustained(LoopTimbre),
    Events(Grid),
}

impl LoopDesign {
    /// No atmosphere on this layer.
    pub const OFF: LoopDesign = LoopDesign::Sustained(LoopTimbre::Off);

    pub fn is_on(&self) -> bool {
        match self {
            LoopDesign::Sustained(t) => *t != LoopTimbre::Off,
            LoopDesign::Events(g) => !g.is_empty(),
        }
    }

    /// The sustained timbre, if this is one.
    pub fn timbre(&self) -> Option<LoopTimbre> {
        match self {
            LoopDesign::Sustained(t) => Some(*t),
            LoopDesign::Events(_) => None,
        }
    }

    /// The recipe token: a timbre code, or `E` + 16 hex digits for a grid.
    pub fn token(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.code().to_string(),
            LoopDesign::Events(g) => format!("E{}", g.to_hex()),
        }
    }

    pub fn from_token(s: &str) -> Option<LoopDesign> {
        if let Some(hex) = s.strip_prefix('E').or_else(|| s.strip_prefix('e')) {
            return Grid::from_hex(hex).map(LoopDesign::Events);
        }
        LoopTimbre::from_code(s).map(LoopDesign::Sustained)
    }

    pub fn name(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.name().to_string(),
            LoopDesign::Events(g) if g.is_empty() => "Event grid (empty)".to_string(),
            LoopDesign::Events(_) => "Event grid".to_string(),
        }
    }

    /// A very short label for a box or header.
    pub fn short(&self) -> &'static str {
        match self {
            LoopDesign::Sustained(t) => t.code(),
            LoopDesign::Events(_) => "EVT",
        }
    }

    pub fn blurb(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.blurb().to_string(),
            LoopDesign::Events(_) => "fire, water, stones, wood: grains on an editable grid".to_string(),
        }
    }
}

/// Where the tonic comes from: the seed (the default, unchanged), or an
/// explicit pitch class the player chooses at setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyChoice {
    Seed,
    /// Pitch class 0..11 (0 = C).
    Note(u8),
}

/// A repeating entrance/exit arc for the frozen-chord drone: every `cycle_s`
/// seconds the drone swells up for `hold_s` seconds, holds, then fades back to
/// silence until the cycle comes round again. `cycle_s == 0` means always on.
/// Timing is in whole seconds (like the slow form cycles), so changing the BPM
/// moves the rhythm but leaves this long breathing of the piece alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DroneArc {
    pub cycle_s: u16,
    pub hold_s: u16,
}

impl DroneArc {
    /// No arc: the drone plays continuously, exactly as it always has.
    pub const ALWAYS_ON: DroneArc = DroneArc { cycle_s: 0, hold_s: 0 };

    /// Whether the arc gates the drone at all.
    pub fn is_on(&self) -> bool {
        self.cycle_s > 0
    }

    /// (cycle, hold) in samples; the hold is clamped to the cycle.
    pub fn samples(&self) -> (u64, u64) {
        let sr = crate::SAMPLE_RATE as u64;
        let cycle = self.cycle_s as u64 * sr;
        (cycle, (self.hold_s as u64 * sr).min(cycle))
    }

    /// Gain 0..1 at `clock`: a raised-cosine swell at each end of the hold,
    /// flat in between, and silence for the rest of the cycle. Always exactly
    /// 1.0 when the arc is off, so the default sound is untouched.
    pub fn gain(&self, clock: u64) -> f64 {
        let (cycle, hold) = self.samples();
        if cycle == 0 {
            return 1.0;
        }
        let p = clock % cycle;
        if p >= hold {
            return 0.0;
        }
        // A quarter of the hold, no shorter than 2 s and no longer than 15 s
        // (and never more than half the hold, so the plateau survives).
        let sr = crate::SAMPLE_RATE as u64;
        let fade = (hold / 4).clamp(2 * sr, 15 * sr).min(hold / 2).max(1);
        if p < fade {
            0.5 - 0.5 * crate::math::cos_turns(0.5 * p as f64 / fade as f64)
        } else if p + fade > hold {
            0.5 - 0.5 * crate::math::cos_turns(0.5 * (hold - p) as f64 / fade as f64)
        } else {
            1.0
        }
    }
}

/// Musical choices made up front (the seeds fill in the details).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub scale: Scale,
    /// 1 to 4.
    pub chords: u8,
    pub pace: Pace,
    /// The drum kit family (seed 5 shapes it).
    pub kit: crate::kits::Kit,
    /// Where the drums sit in the stereo field.
    pub space: crate::kits::DrumSpace,    /// The drum tempo; every ambient layer moves at exactly half of it.
    pub bpm: u16,
    /// The four atmosphere loop layers (seed 6 shapes the sustained ones).
    pub loops: [LoopDesign; 4],
    /// A fade-in/out arc per atmosphere layer (cycle 0 = always on).
    pub loop_arcs: [DroneArc; 4],
    /// Semitones the atmosphere loops are transposed by (0 = the chord tones
    /// exactly, so they match the chosen root note).
    pub loop_transpose: i8,
    /// Where the tonic comes from (seed 1 picks it by default).
    pub key: KeyChoice,
    /// The repeating entrance/exit arc of the frozen-chord drone.
    pub drone: DroneArc,
    /// An end-of-chain low-pass on the drone, in Hz (0 = off, bypass).
    pub drone_tone: u16,
    /// Whether drum break edits may play slices backwards.
    pub drum_reverse: bool,
    /// End-of-chain master effects.
    pub master: MasterFx,
    /// The browser's source-code scroll overlay.
    pub code: CodeOverlay,
    /// The track's name: shown at the head of the page and used as the WAV
    /// title. Empty falls back to the product name.
    pub name: crate::name::Name,
    /// How the architecture is turned and placed. Identity by default, so the
    /// shrine is exactly the centred 16x16 model (`HC005`).
    pub arch: crate::arch::ArchTransform,
    /// The two glitch layers: which kinds play, and how often. The defaults
    /// reproduce the originally hard-coded layers exactly.
    pub glitch: [crate::glitch_setup::GlitchSetup; 2],
    /// An authored architecture. Empty means the seed's generator decides.
    pub model: crate::shrine_model::ShrineModel,
    /// Constant camera rotation, in degrees per second; the whole viewport
    /// orbits the shrine. Negative goes the other way, 0 holds still.
    pub camera_speed: i16,
    /// Whether the drone's rhythmic amplitude LFO runs. Off by default, so the
    /// drone sound is exactly as it always was.
    pub drone_rhythm: bool,
    /// Whether that same rhythm also shapes the atmosphere loops.
    pub loop_rhythm: bool,
    /// Whether the shrine's wildlife (chosen by the seed and biome) is heard
    /// and seen. Off by default.
    pub wildlife: bool,
    /// The 2-step garage mode's swing, bass and stabs (ignored by the other
    /// modes).
    pub garage: crate::garage::GarageSetup,
    /// Where the piece starts, in seconds into the music (0 = the beginning).
    /// Everything is deterministic, so starting later is the very same piece
    /// joined part-way: the audio is fast-forwarded exactly, the visuals and
    /// weather simply begin at that time.
    pub start_s: u16,
    /// The microsound mode's pulse, grains, pips and dropouts (ignored by
    /// the other modes).
    pub micro: crate::micro::MicroSetup,
}

/// Microsound's drone-shift figure: four transpositions (semitones), the
/// first always home, the rest drawn from the chosen set by seed 1.
fn shift_figure(seed: u64, shift: crate::micro::DroneShift, scale: Scale) -> [f64; 4] {
    use crate::micro::DroneShift;
    let set: Vec<f64> = match shift {
        DroneShift::Off => vec![0.0],
        DroneShift::Octaves => vec![12.0, -12.0, 0.0],
        DroneShift::Fifths => vec![7.0, -5.0, 12.0, 5.0],
        DroneShift::Scale => {
            let iv = scale.intervals();
            iv.iter().skip(1).flat_map(|i| [*i as f64, *i as f64 - 12.0]).collect()
        }
    };
    let mut rng = Rng::stream(seed, 0x5F17);
    [0.0, rng.pick(&set), rng.pick(&set), rng.pick(&set)]
}

impl Settings {
    /// What suits each mode, when nothing was chosen by hand: (tempo,
    /// scale, chord count). Garage lives around 130 BPM in a plain major
    /// scale on a four-chord loop; microsound around 120 in a gapped
    /// pentatonic, so its sine pips can never clash, on two slow chords.
    pub fn mode_defaults(pace: Pace) -> (u16, Scale, u8) {
        match pace {
            Pace::HalfTime | Pace::Jungle => (DEFAULT_SETTINGS.bpm, DEFAULT_SETTINGS.scale, DEFAULT_SETTINGS.chords),
            Pace::Garage => (132, Scale::Ionian, 4),
            Pace::Micro => (120, Scale::PentatonicMinor, 2),
        }
    }

    /// Switch to `pace`. Tempo, scale and chord count that were still at the
    /// old mode's defaults move to the new mode's; anything chosen by hand
    /// is kept.
    pub fn set_mode(&mut self, pace: Pace) {
        let (bpm, scale, chords) = Settings::mode_defaults(self.pace);
        let (nbpm, nscale, nchords) = Settings::mode_defaults(pace);
        if self.bpm == bpm {
            self.bpm = nbpm;
        }
        if self.scale == scale {
            self.scale = nscale;
        }
        if self.chords == chords {
            self.chords = nchords;
        }
        self.pace = pace;
    }
}

/// The latest start offset, in seconds (the audio must be fast-forwarded to
/// it, which takes a moment per minute).
pub const MAX_START_S: u16 = 600;

pub const DEFAULT_SETTINGS: Settings =
    Settings {
        scale: Scale::Lydian,
        chords: 3,
        pace: Pace::HalfTime,
        kit: crate::kits::Kit::SubClicks,
        space: crate::kits::DrumSpace::Wide,
        bpm: crate::tempo::DEFAULT_BPM,
        loops: [
            LoopDesign::Sustained(LoopTimbre::Voices),
            LoopDesign::Sustained(LoopTimbre::Bells),
            LoopDesign::OFF,
            LoopDesign::OFF,
        ],
        loop_arcs: [DroneArc::ALWAYS_ON; 4],
        loop_transpose: 0,
        key: KeyChoice::Seed,
        drone: DroneArc::ALWAYS_ON,
        drone_tone: 0,
        drum_reverse: true,
        master: MasterFx::OFF,
        code: CodeOverlay::DEFAULT,
        name: crate::name::Name::EMPTY,
        arch: crate::arch::ArchTransform::IDENTITY,
        glitch: crate::glitch_setup::GlitchSetup::DEFAULT,
        model: crate::shrine_model::ShrineModel::EMPTY,
        camera_speed: -2,
        drone_rhythm: false,
        loop_rhythm: false,
        wildlife: false,
        garage: crate::garage::GarageSetup::DEFAULT,
        start_s: 0,
        micro: crate::micro::MicroSetup::DEFAULT,
    };

/// 0010's sound: the default settings with no atmosphere loops.
pub const NO_LOOPS: [LoopDesign; 4] = [LoopDesign::OFF; 4];

/// The ping-pong echo's delay, in glitch sixteenths (3/16).
/// How far the garage bassline dips under each kick (sidechain).
const GARAGE_BASS_DUCK: f64 = 0.5;
/// Garage's balance: the drone about 6 dB back, the loops about 3 dB up.
const GARAGE_DRONE: f64 = 0.5;
/// The drone's further step back under a chord that rubs against it.
const GARAGE_PEDAL_RUB: f64 = 0.25;
const GARAGE_LOOPS: f64 = 1.4;
/// How far the stabs dip under each kick, and how much of them goes to the reverb.
const GARAGE_STAB_DUCK: f64 = 0.3;
const STAB_SEND: f64 = 0.35;
/// How far the garage reverb send dips before each change.
const GARAGE_SEND_DIP: f64 = 0.7;
/// Microsound's balance: the thin drone well back, the glitches forward,
/// less of everything in the reverb, the whole about 3 dB quieter (which
/// keeps the output stage clear of saturation), the pips' reverb send.
const MICRO_DRONE: f64 = 0.4;
const MICRO_GLITCH: f64 = 1.25;
const MICRO_SEND: f64 = 0.5;
const MICRO_WET: f64 = 0.7;
const MICRO_LEVEL: f64 = 0.7;
const PIP_SEND: f64 = 0.25;
/// How much of the loops stays as it is, sparse .. dense grains, and the cloud's level.
const MICRO_BED_DRY: [f64; 2] = [0.8, 0.35];
const CLOUD_LEVEL: f64 = 0.9;
/// Microsound keeps everything short: the reverb is a small room, the glitch
/// echo barely repeats, the ringing glitches last a quarter as long, and the
/// drone and loops sound only in fragments, in a steady rhythm: (on, off)
/// in sixteenths, each pattern exactly one or two bars long.
const MICRO_DECAY_S: f64 = 1.2;
const MICRO_ECHO_FEEDBACK: f64 = 0.25;
const MICRO_GLITCH_LEN: f64 = 0.25;
/// The drone: two beats on and two off, one on three off, three on one off,
/// a bar on and a bar off, two beats in two bars, one beat on and off.
const DRONE_FRAGMENTS: [(u64, u64); 6] = [(8, 8), (4, 12), (12, 4), (16, 16), (8, 24), (4, 4)];
/// The loops: shorter, eighth notes to a dotted quarter.
const LOOP_FRAGMENTS: [(u64, u64); 6] = [(2, 2), (2, 6), (4, 4), (4, 12), (6, 10), (2, 14)];
const ECHO_SIXTEENTHS: u64 = 3;
/// The drums' tape echo delay, in drum sixteenths.
const TAPE_SIXTEENTHS: u64 = 3;
const GARAGE_TAPE_SIXTEENTHS: u64 = 6;

// Mix. "send" levels feed the shared long reverb.
const DRONE_DRY: f64 = 1.0;
const DRONE_SEND: f64 = 0.35;
const BASS_DRY: f64 = 1.0;
const BASS_SEND: f64 = 0.0;
const GLITCH1_DRY: f64 = 0.8;
const GLITCH1_ECHO: f64 = 0.35;
const GLITCH1_SEND: f64 = 0.3;
const GLITCH2_DRY: f64 = 0.7;
const GLITCH2_ECHO: f64 = 0.6;
const GLITCH2_SEND: f64 = 0.4;
const ECHO_DRY: f64 = 0.6;
const ECHO_SEND: f64 = 0.5;
const ECHO_FEEDBACK: f64 = 0.55;
/// Per atmosphere layer: dry level, reverb send, target length in seconds, and
/// stereo side level (1.0 = full width).
const LOOP_DRY: [f64; 4] = [0.8, 0.7, 0.6, 0.55];
const LOOP_SEND: [f64; 4] = [0.45, 0.5, 0.45, 0.4];
const LOOP_SECONDS: [u64; 4] = [6, 4, 5, 7];
const LOOP_WIDTH: [f64; 4] = [1.0, 0.6, 0.8, 0.7];
const DRUMS_DRY: f64 = 0.85;
const DRUMS_SEND: f64 = 0.1;
/// The drums' tape echo (Wide + tape echo only): send of the snare-type hits,
/// its return in the mix, and a little of it into the reverb.
const TAPE_SEND: f64 = 0.55;
const TAPE_WET: f64 = 0.6;
const TAPE_REVERB: f64 = 0.15;
const TAPE_FEEDBACK: f64 = 0.5;

// Making room for the drums: while they play, the drone dips and darkens.
/// Follower times (seconds) for "drums are playing" and for individual hits.
const PRESENCE_ATTACK: f64 = 0.05;
const PRESENCE_RELEASE: f64 = 1.5;
const PUNCH_ATTACK: f64 = 0.002;
const PUNCH_RELEASE: f64 = 0.06;
/// Drum-bus levels that count as "fully playing" / "a full hit".
const PRESENCE_REF: f64 = 0.3;
const PUNCH_REF: f64 = 0.6;
/// Depths: drone level (≈ −5 dB), per-hit pump (≈ −1 dB, quick), high shelf (−6 dB), glitch/echo dip.
const DUCK_DEPTH: f64 = 0.45;
const PUMP_DEPTH: f64 = 0.12;
const SHELF_DEPTH: f64 = 0.5;
const SHELF_HZ: f64 = 2500.0;
const GLITCH_DUCK: f64 = 0.15;
const REVERB_WET: f64 = 0.55;
const MASTER_GAIN: f64 = 0.9;
const FADE_IN_SECONDS: u64 = 10;

/// Drone filter sweep range.
pub const DRONE_CUTOFF_MIN: f64 = 400.0;
pub const DRONE_CUTOFF_OCTAVES: f64 = 4.2;

// The two glitch layers are built from `glitch_setup::GlitchSetup`, whose
// defaults reproduce the values that used to be hard-coded here.

/// A layer to hear on its own. Soloing changes only the *monitor* output
/// (`render_split`); the mix, and so every recording, is never touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solo {
    Drone,
    Glitch1,
    Glitch2,
    Bass,
    Drums,
    /// Atmosphere loop 1.
    Loop1,
    /// Atmosphere loop 2.
    Loop2,
    /// Atmosphere loop 3.
    Loop3,
    /// Atmosphere loop 4.
    Loop4,
    /// The echo and reverb returns only: the room without the instruments.
    Space,
}

/// Monitor-only state: its own echo and reverb (so a soloed layer keeps its
/// space), its own master stage, and a ramp for click-free switching.
pub(crate) struct Monitor {
    /// The echo's delay, to rebuild it fresh on each switch.
    echo_delay: usize,
    target: Option<Solo>,
    current: Option<Solo>,
    amount: f64,
    echo: PingPong,
    reverb: Reverb,
    dc: [DcBlock; 2],
    out: (f64, f64),
}

/// About 20 ms to fade between the mix and a solo.
const SOLO_RAMP: f64 = 1.0 / (0.02 * crate::SAMPLE_RATE as f64);

impl Monitor {
    fn new(echo_delay: usize) -> Self {
        Monitor {
            echo_delay,
            target: None,
            current: None,
            amount: 0.0,
            echo: PingPong::new(echo_delay, ECHO_FEEDBACK),
            reverb: Reverb::new(),
            dc: Default::default(),
            out: (0.0, 0.0),
        }
    }
}

/// Attack/release envelope follower.
pub(crate) struct Follower {
    env: f64,
    attack: f64,
    release: f64,
}

impl Follower {
    fn new(attack_s: f64, release_s: f64) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        let coef = |t: f64| 1.0 - crate::math::exp(-1.0 / (t * sr));
        Follower { env: 0.0, attack: coef(attack_s), release: coef(release_s) }
    }

    fn process(&mut self, x: f64) -> f64 {
        let a = if x > self.env { self.attack } else { self.release };
        self.env = crate::math::sanitize(self.env + a * (x - self.env));
        self.env
    }
}

pub struct Track {
    pub(crate) seeds: Seeds,
    pub(crate) tempo: Tempo,
    pub(crate) tape: TapeEcho,
    pub(crate) monitor: Monitor,
    pub(crate) presence: Follower,
    pub(crate) punch: Follower,
    pub(crate) shelf: [OnePole; 2],
    /// Drone gain from the drum-aware mix (display only).
    pub(crate) duck_gain: f64,
    pub(crate) settings: Settings,
    pub(crate) master: MasterChain,
    pub(crate) harmony: Harmony,
    pub(crate) drums: Drums,
    pub(crate) clock: u64,
    pub(crate) mods: Mods,
    pub(crate) drone: Drone,
    /// The drone's rhythmic amplitude LFO, if it is switched on.
    drone_rhythm: crate::drone_rhythm::DroneRhythm,
    pub(crate) drone_filter: [Svf; 2],
    /// End-of-chain drone character filter (a chosen low-pass).
    pub(crate) drone_tone_filter: [Svf; 2],
    /// Drone gain from the repeating entrance/exit arc (1 = always on).
    pub(crate) drone_window: f64,
    pub(crate) history: History,
    pub(crate) glitch1: Layer,
    pub(crate) glitch2: Layer,
    pub(crate) loops: [AtmosLoop; 4],
    /// Per-layer gain from each atmosphere layer's own arc.
    pub(crate) loop_windows: [f64; 4],
    pub(crate) echo: PingPong,
    pub(crate) bass: Bass,
    /// 2-step garage's rhythmic bassline (in place of the drone bass).
    pub(crate) bassline: Option<crate::bass::BassLine>,
    /// 2-step garage's chord stabs.
    pub(crate) stabs: Option<crate::stabs::Stabs>,
    /// Microsound's sine pips, sub pulse and grain cloud.
    pub(crate) pips: Option<crate::pips::Pips>,
    pub(crate) sub_pulse: Option<crate::bass::SubPulse>,
    pub(crate) cloud: Option<crate::grains::Cloud>,
    /// Microsound's fragment gates on the drone and each atmosphere loop.
    pub(crate) drone_gate: Option<crate::grains::Gate>,
    pub(crate) loop_gates: Option<[crate::grains::Gate; 4]>,
    /// Microsound: the drone's fragments transposed by a repeating figure of
    /// four (semitones), and the fragment last heard.
    pub(crate) drone_shift: Option<(crate::grains::Shifter, [f64; 4])>,
    pub(crate) drone_fragment: u64,
    /// 2-step garage: how far the drone (and the pitched glitches) step back
    /// under a chord that rubs against the drone's open fifth.
    pub(crate) pedal_gain: f64,
    pub(crate) pedal_target: f64,
    pub(crate) reverb: Reverb,
    pub(crate) dc: [DcBlock; 2],
    // Display-only state: written by `tick`, never read by it.
    pub(crate) meters: Meters,
    pub(crate) drone_cutoff: f64,
    pub(crate) drone_q: f64,
    pub(crate) overview_frame: u64,
}

impl Track {
    pub fn new(seeds: Seeds, settings: Settings) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        // Chord 1's voicing is the first draw from the drone's stream (as in 0009).
        let mut first = Rng::stream(seeds.s1, 0xD0);
        let mut rest = Rng::stream(seeds.s1, 0xD7);
        let key = match settings.key {
            KeyChoice::Seed => crate::harmony::key_for(seeds.s1),
            KeyChoice::Note(n) => n % 12,
        };
        let tempo = Tempo::new(settings.bpm);
        let settings = Settings { bpm: tempo.bpm, ..settings };
        let chords = settings.chords.clamp(1, 4) as usize;
        let garage = settings.pace == Pace::Garage;
        let harmony = Harmony::new(settings.scale, if garage { 1 } else { chords }, settings.pace, tempo.beat(), key, &mut first, &mut rest);
        // 2-step garage: the drone holds the tonic (the harmony just built,
        // one chord), while everything else follows the genre progression.
        let (mut harmony, mut drone_harmony) = if garage {
            let mut g = Rng::stream(seeds.s1, 0x6A4A_6E);
            let gs = settings.garage;
            (
                Harmony::garage(settings.scale, chords, tempo.beat(), key, &mut g, gs.progression as usize, gs.lift.passes()),
                Harmony::pedal(settings.scale, tempo.beat(), key),
            )
        } else {
            (harmony.clone(), harmony)
        };
        // Microsound: the chords can change on a regular count of bars,
        // crossing over in a beat.
        let micro = settings.pace == Pace::Micro;
        let rate = settings.micro.chords.bars();
        if micro && rate > 0 {
            let (step, morph) = (rate * tempo.drum_bar(), 4 * tempo.drum16);
            harmony = harmony.stepped(step, morph);
            drone_harmony = drone_harmony.stepped(step, morph);
        }
        if garage {
            // The loops and glitches swing with the drums.
            harmony.swing_push = ((2.0 * settings.garage.swing_fraction() - 1.0) * tempo.drum16 as f64) as u64;
            harmony.sixteenth = tempo.drum16;
        }
        let echo_delay = (ECHO_SIXTEENTHS * tempo.glitch16()) as usize;
        // Loop lengths: whole beats, 3-8 s, pairwise co-prime so all four drift.
        let mut avoid = 1u64;
        let mut avoids = [0u64; 4];
        for (i, a) in avoids.iter_mut().enumerate() {
            *a = avoid;
            let beats = crate::atmos::loop_beats(tempo.beat(), LOOP_SECONDS[i], (i > 0).then_some(avoid));
            avoid = avoid.wrapping_mul(beats);
        }
        let mut drums = match settings.pace {
            Pace::Garage => Drums::garage(seeds.s5, settings.kit, settings.space, &harmony, tempo, settings.garage.swing_fraction()),
            Pace::Micro => Drums::micro(seeds.s5, settings.kit, settings.space, &harmony, tempo, settings.micro),
            Pace::HalfTime | Pace::Jungle => Drums::new(seeds.s5, settings.kit, settings.space, &harmony, tempo),
        };
        drums.set_reverse(settings.drum_reverse);
        let glitch_config = |i: usize| {
            let mut c = settings.glitch[i].layer_config(i, &harmony, tempo);
            if micro {
                c.len_scale = MICRO_GLITCH_LEN;
            }
            c
        };
        let gate = |stream: u64, patterns: &[(u64, u64)]| crate::grains::Gate::new(seeds.s6, stream, tempo.drum16, patterns);
        Track {
            seeds,
            tempo,
            // Garage: an even number of sixteenths, so each echo keeps its
            // hit's place in the swing (three would flip it to the other side).
            tape: TapeEcho::new((if garage { GARAGE_TAPE_SIXTEENTHS } else { TAPE_SIXTEENTHS } * tempo.drum16) as usize, TAPE_FEEDBACK),
            monitor: Monitor::new(echo_delay),
            presence: Follower::new(PRESENCE_ATTACK, PRESENCE_RELEASE),
            punch: Follower::new(PUNCH_ATTACK, PUNCH_RELEASE),
            shelf: [OnePole::new(SHELF_HZ, sr), OnePole::new(SHELF_HZ, sr)],
            duck_gain: 1.0,
            settings,
            master: MasterChain::new(settings.master, seeds.s1),
            harmony: harmony.clone(),
            drums,
            clock: 0,
            mods: Mods::new(seeds.s1),
            glitch1: Layer::new(glitch_config(0), seeds.s2, 0x61),
            glitch2: Layer::new(glitch_config(1), seeds.s3, 0x62),
            loops: std::array::from_fn(|i| {
                AtmosLoop::new(
                    settings.loops[i],
                    seeds.s6,
                    0xA1 + i as u64,
                    &harmony,
                    tempo.beat(),
                    LOOP_SECONDS[i],
                    (i > 0).then_some(avoids[i]),
                    settings.loop_transpose,
                )
            }),
            loop_windows: [1.0; 4],
            drone: if micro { Drone::thin(seeds.s1, drone_harmony, first, rest) } else { Drone::new(seeds.s1, drone_harmony, first, rest) },
            drone_rhythm: crate::drone_rhythm::DroneRhythm::new(seeds.s7, settings.bpm),
            drone_filter: [Svf::new(1000.0, 0.7, sr), Svf::new(1000.0, 0.7, sr)],
            drone_tone_filter: [Svf::new(8_000.0, 0.7, sr), Svf::new(8_000.0, 0.7, sr)],
            drone_window: 1.0,
            history: History::new(),
            echo: PingPong::new(echo_delay, if micro { MICRO_ECHO_FEEDBACK } else { ECHO_FEEDBACK }),
            bass: Bass::new(seeds.s4),
            bassline: (settings.pace == Pace::Garage && settings.garage.bass == crate::garage::BassMode::Rhythmic)
                .then(|| crate::bass::BassLine::new(seeds.s4, tempo, settings.garage.swing_fraction())),
            pedal_gain: 1.0,
            pedal_target: 1.0,
            stabs: (settings.pace == Pace::Garage && settings.garage.stabs != crate::garage::Stabs::Off).then(|| {
                crate::stabs::Stabs::new(seeds.s6, settings.garage.stabs, settings.garage.tone, tempo, settings.garage.swing_fraction())
            }),
            pips: (micro && settings.micro.pips != crate::micro::Pips::Off)
                .then(|| crate::pips::Pips::new(seeds.s6, settings.micro.pips, tempo)),
            sub_pulse: (micro && settings.micro.pulse != crate::micro::Pulse::Off).then(|| crate::bass::SubPulse::new(seeds.s4, tempo)),
            cloud: micro.then(|| crate::grains::Cloud::new(seeds.s6, settings.micro.grains.amount())),
            drone_gate: micro.then(|| gate(0x6A7E_D0, &DRONE_FRAGMENTS)),
            loop_gates: micro.then(|| std::array::from_fn(|k| gate(0x6A7E_A1 + k as u64, &LOOP_FRAGMENTS))),
            drone_shift: (micro && settings.micro.shift != crate::micro::DroneShift::Off)
                .then(|| (crate::grains::Shifter::new(), shift_figure(seeds.s1, settings.micro.shift, settings.scale))),
            drone_fragment: u64::MAX,
            reverb: if micro { Reverb::with_decay(MICRO_DECAY_S) } else { Reverb::new() },
            dc: Default::default(),
            meters: Meters::default(),
            drone_cutoff: 0.0,
            drone_q: 0.0,
            overview_frame: u64::MAX,
        }
    }

    /// Fill `out` with interleaved stereo samples. Output depends only on the
    /// seeds and the running sample position, never on how it is chunked.
    pub fn render(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            let (l, r) = self.tick();
            frame[0] = l as f32;
            frame[1] = r as f32;
        }
    }

    /// The tempo this version plays at.
    pub fn tempo(&self) -> Tempo {
        self.tempo
    }

    /// Choose a layer to hear on its own (or `None` for the full mix). Only
    /// the monitor output of `render_split` changes.
    pub fn set_solo(&mut self, solo: Option<Solo>) {
        self.monitor.target = solo;
    }

    /// The layer asked for (the monitor fades to it).
    pub fn solo_target(&self) -> Option<Solo> {
        self.monitor.target
    }

    /// The layer currently soloed on the monitor, and how far faded in (0..1).
    pub fn solo(&self) -> (Option<Solo>, f64) {
        (self.monitor.current, self.monitor.amount)
    }

    /// Like `render`, but also fills `monitor` with what to *listen* to: the
    /// mix, or the soloed layer. `mix` is bit-identical to `render`'s output.
    pub fn render_split(&mut self, mix: &mut [f32], monitor: &mut [f32]) {
        for (m, o) in mix.chunks_exact_mut(2).zip(monitor.chunks_exact_mut(2)) {
            let (l, r) = self.tick();
            m[0] = l as f32;
            m[1] = r as f32;
            o[0] = self.monitor.out.0 as f32;
            o[1] = self.monitor.out.1 as f32;
        }
    }

    fn tick(&mut self) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let clock = self.clock;
        let m = self.mods.at(clock);

        // 1: frozen drone through the slowly cycling filter and the repeating
        // entrance/exit arc.
        if clock % 32 == 0 {
            let cutoff = DRONE_CUTOFF_MIN * crate::math::exp2(m.drone_open * DRONE_CUTOFF_OCTAVES);
            let q = 0.6 + 2.4 * m.drone_reso;
            for f in self.drone_filter.iter_mut() {
                f.set(cutoff, q, sr);
            }
            self.drone_cutoff = cutoff;
            self.drone_q = q;
            // The arc, times the rhythmic LFO if it is switched on. Off, the
            // multiplier is exactly 1.0, so the sound is unchanged.
            let rhythm = if self.settings.drone_rhythm || self.settings.loop_rhythm {
                self.drone_rhythm.level(clock)
            } else {
                1.0
            };
            self.drone_window = self.settings.drone.gain(clock)
                * if self.settings.drone_rhythm { rhythm } else { 1.0 };
            for k in 0..4 {
                // The loops can breathe with the same rhythm, on their own arc.
                self.loop_windows[k] = self.settings.loop_arcs[k].gain(clock)
                    * if self.settings.loop_rhythm { rhythm } else { 1.0 };
            }
            if self.settings.drone_tone > 0 {
                for f in self.drone_tone_filter.iter_mut() {
                    f.set(self.settings.drone_tone as f64, 0.7, sr);
                }
            }
        }
        let window = self.drone_window;
        let (dl, dr) = self.drone.next();
        let dl = self.drone_filter[0].process(dl).low * window;
        let dr = self.drone_filter[1].process(dr).low * window;
        // The chosen drone character filter, at the very end of the drone chain.
        let (dl, dr) = if self.settings.drone_tone > 0 {
            (self.drone_tone_filter[0].process(dl).low, self.drone_tone_filter[1].process(dr).low)
        } else {
            (dl, dr)
        };
        self.history.push(dl, dr);

        // 5 (computed early): jungle drums, and how present they are right now.
        let (d, d_r, d_perc) = self.drums.next(clock, &self.mods);
        let d_abs = d.abs().max(d_r.abs());
        let presence = (self.presence.process(d_abs) / PRESENCE_REF).min(1.0);
        let punch = (self.punch.process(d_abs) / PUNCH_REF).min(1.0);
        // The tape echo only runs in "Wide + tape echo".
        let (tl, tr) = if self.settings.space == crate::kits::DrumSpace::Tape {
            self.tape.process(clock, d_perc * TAPE_SEND)
        } else {
            (0.0, 0.0)
        };
        // Make room: the drone dips, pumps with each hit, and loses some top end.
        // (With no drums, presence and punch stay exactly 0 and nothing changes.)
        let duck = (1.0 - DUCK_DEPTH * presence) * (1.0 - PUMP_DEPTH * punch);
        let shelf = SHELF_DEPTH * presence;
        let dl = (dl - (dl - self.shelf[0].process(dl)) * shelf) * duck;
        let dr = (dr - (dr - self.shelf[1].process(dr)) * shelf) * duck;
        self.duck_gain = duck;
        // 2-step garage leans on the atmosphere loops: the drone steps back,
        // and further still under a chord that rubs against its open fifth
        // (a borrowed chord, or the key lift).
        let mut drone_bed = (0.0, 0.0);
        let (dl, dr) = if self.settings.pace == Pace::Garage {
            if clock % 32 == 0 {
                let p = self.harmony.at(clock);
                self.pedal_target = if self.harmony.rubs_pedal(p.chord) { GARAGE_PEDAL_RUB } else { 1.0 };
            }
            self.pedal_gain += (self.pedal_target - self.pedal_gain) * 0.0004;
            let g = GARAGE_DRONE * self.pedal_gain;
            (dl * g, dr * g)
        } else if let Some(gate) = self.drone_gate.as_mut() {
            // Microsound: the drone heard only in fragments. The whole of it
            // still feeds the grain cloud below.
            drone_bed = (dl * MICRO_DRONE, dr * MICRO_DRONE);
            // Each fragment takes the next transposition of the figure, set
            // as it opens.
            let (sl, sr_) = match self.drone_shift.as_mut() {
                Some((shifter, figure)) => {
                    let f = gate.fragment(clock);
                    if f != self.drone_fragment {
                        self.drone_fragment = f;
                        shifter.set(figure[(f % 4) as usize]);
                    }
                    shifter.process(drone_bed.0, drone_bed.1)
                }
                None => drone_bed,
            };
            let g = gate.next(clock);
            (sl * g, sr_ * g)
        } else {
            (dl, dr)
        };
        let others = 1.0 - GLITCH_DUCK * presence;

        // 2 & 3: glitch layers. A muted layer keeps ticking, so its pattern
        // evolution stays deterministic, but its density is zero and it is
        // silent.
        let d1 = if self.settings.glitch[0].enabled { m.density1 } else { 0.0 };
        let d2 = if self.settings.glitch[1].enabled { m.density2 } else { 0.0 };
        let g1 = self.glitch1.next(clock, d1, &self.history);
        let g2 = self.glitch2.next(clock, d2, &self.history);
        // Garage: the glitches (which chop the drone) step back with it.
        let (g1, g2) = if self.settings.pace == Pace::Garage {
            let k = 0.4 + 0.6 * self.pedal_gain;
            ((g1.0 * k, g1.1 * k), (g2.0 * k, g2.1 * k))
        } else if self.settings.pace == Pace::Micro {
            let k = MICRO_GLITCH;
            ((g1.0 * k, g1.1 * k), (g2.0 * k, g2.1 * k))
        } else {
            (g1, g2)
        };
        let (el, er) = self.echo.process(
            g1.0 * GLITCH1_ECHO + g2.0 * GLITCH2_ECHO,
            g1.1 * GLITCH1_ECHO + g2.1 * GLITCH2_ECHO,
        );

        // The four atmosphere layers: they make room for the drums like the
        // drone, drift gently in level, and each breathes on its own arc.
        let mut a = [(0.0f64, 0.0f64); 4];
        for k in 0..4 {
            if self.loops[k].is_on() {
                let (l, r) = self.loops[k].next(clock, &self.harmony);
                let level = if k % 2 == 0 { m.loop_level } else { m.loop2_level };
                let g = duck * level * self.loop_windows[k];
                let g = if self.settings.pace == Pace::Garage { g * GARAGE_LOOPS } else { g };
                a[k] = if LOOP_WIDTH[k] == 1.0 {
                    // Full width: bit-identical to the old un-narrowed mix.
                    (l * g, r * g)
                } else {
                    let (mid, side) = (0.5 * (l + r), 0.5 * (l - r) * LOOP_WIDTH[k]);
                    ((mid + side) * g, (mid - side) * g)
                };
            }
        }

        // Microsound: the bed is listened to and scattered back as grains;
        // the loops themselves step back to make room for their cloud.
        let cloud = match self.cloud.as_mut() {
            Some(c) => {
                let bed = (0..4).fold((drone_bed.0 * 0.5, drone_bed.1 * 0.5), |(x, y), k| (x + a[k].0 * LOOP_DRY[k], y + a[k].1 * LOOP_DRY[k]));
                let (cl, cr) = c.process(bed.0, bed.1);
                let g = self.settings.micro.grains.amount();
                let keep = MICRO_BED_DRY[0] + (MICRO_BED_DRY[1] - MICRO_BED_DRY[0]) * g;
                // Each loop itself is heard only in fragments of its own.
                let gates = self.loop_gates.as_mut().expect("microsound has loop gates");
                for (k, x) in a.iter_mut().enumerate() {
                    let gk = keep * gates[k].next(clock);
                    *x = (x.0 * gk, x.1 * gk);
                }
                Some((cl * CLOUD_LEVEL, cr * CLOUD_LEVEL))
            }
            None => None,
        };
        // Microsound: the pips and the sub pulse play when the pulse does
        // (the pips also through the clicks-only sections).
        let micro = (self.settings.pace == Pace::Micro).then(|| {
            let section = self.drums.plan_now().map(|plan| plan.section);
            match section {
                Some(s) => (matches!(s, Section::Pulse | Section::Fracture), matches!(s, Section::Clicks | Section::Pulse | Section::Fracture)),
                None => (!self.drums.enabled(), !self.drums.enabled()),
            }
        });
        let pip = match (self.pips.as_mut(), micro) {
            (Some(p), Some((_, on))) => Some(p.next(clock, &self.harmony, on)),
            _ => None,
        };

        // 4: bass, gliding to each chord's root during the morph bar. The bass
        // leaves with the drone: one arrangement, one envelope.
        // 2-step garage: the bassline and the stabs play with the groove
        // (they carry the breakdown and rest when the drums do), lead into
        // each chord change, and duck under each kick.
        let garage = (self.settings.pace == Pace::Garage).then(|| {
            let p = self.harmony.at(clock);
            // The second of the chord's two bars, before a change.
            let changing = p.next != p.chord && (clock / self.tempo.drum_bar()) % 2 == 1;
            let section = self.drums.plan_now().map(|plan| plan.section);
            let active = match section {
                Some(s) => matches!(s, Section::Breakdown | Section::TwoStep | Section::Shuffle),
                None => !self.drums.enabled(),
            };
            (p.chord, p.next, changing, active, section.unwrap_or(Section::TwoStep))
        });
        let stab = match (self.stabs.as_mut(), garage) {
            (Some(st), Some((_, _, changing, active, section))) => {
                let (sl, sr_) = st.next(clock, &self.harmony, changing, active, section);
                let k = 1.0 - GARAGE_STAB_DUCK * punch;
                Some((sl * k, sr_ * k))
            }
            _ => None,
        };
        let b = if let (Some(line), Some((chord, next, changing, active, _))) = (self.bassline.as_mut(), garage) {
            let (root, next_root) = (self.harmony.chords[chord].bass, self.harmony.chords[next].bass);
            // The bass walks up only into the top of the loop, not every change;
            // its fifth is the chord's own (a flat fifth on a m7b5).
            let fifth = (self.harmony.chords[chord].tones[2] as f64 - self.harmony.chords[chord].tones[0] as f64).rem_euclid(12.0);
            line.next(clock, root, next_root, fifth, changing && self.harmony.starts_pass(next), active, &m) * (1.0 - GARAGE_BASS_DUCK * punch)
        } else if let Some((active, _)) = micro {
            // Microsound has no drone bass: only the sub pulse, if any.
            let root = self.harmony.chords[self.harmony.at(clock).chord].bass;
            match self.sub_pulse.as_mut() {
                Some(sp) => sp.next(clock, root, active) * window,
                None => 0.0,
            }
        } else {
            if clock % 32 == 0 {
                let p = self.harmony.at(clock);
                let (a, z) = (self.harmony.chords[p.chord].bass, self.harmony.chords[p.next].bass);
                self.bass.set_root(a + (z - a) * p.morph);
            }
            self.bass.next(clock, &m) * window
        };

        // Shared long reverb.
        // 2-step garage: in the beat before a change the stabs' and loops'
        // reverb send dips, so the old chord doesn't build a tail over the new.
        let dip = match garage {
            Some(_) => 1.0 - GARAGE_SEND_DIP * self.harmony.at(clock).morph,
            None => 1.0,
        };
        let loop_send = |k: usize| -> f64 { (0..4).map(|j| [a[j].0, a[j].1][k] * LOOP_SEND[j]).sum() };
        let send = |x: f64, k: usize| {
            if micro.is_some() {
                let p = pip.map_or(0.0, |p| [p.0, p.1][k]);
                let c = cloud.map_or(0.0, |c| [c.0, c.1][k]);
                return (x + p * PIP_SEND + c * LOOP_SEND[0]) * MICRO_SEND;
            }
            if garage.is_none() {
                return x;
            }
            let x = x - loop_send(k) * (1.0 - dip);
            match stab {
                Some(s) => x + [s.0, s.1][k] * STAB_SEND * dip,
                None => x,
            }
        };
        let (rl, rr) = self.reverb.process(
            send(dl * DRONE_SEND
                + a[0].0 * LOOP_SEND[0]
                + a[1].0 * LOOP_SEND[1]
                + a[2].0 * LOOP_SEND[2]
                + a[3].0 * LOOP_SEND[3]
                + b * BASS_SEND
                + g1.0 * GLITCH1_SEND
                + g2.0 * GLITCH2_SEND
                + el * ECHO_SEND
                + d * DRUMS_SEND
                + tl * TAPE_REVERB, 0),
            send(dr * DRONE_SEND
                + a[0].1 * LOOP_SEND[0]
                + a[1].1 * LOOP_SEND[1]
                + a[2].1 * LOOP_SEND[2]
                + a[3].1 * LOOP_SEND[3]
                + b * BASS_SEND
                + g1.1 * GLITCH1_SEND
                + g2.1 * GLITCH2_SEND
                + er * ECHO_SEND
                + d_r * DRUMS_SEND
                + tr * TAPE_REVERB, 1),
        );

        let mut l = dl * DRONE_DRY
            + a[0].0 * LOOP_DRY[0]
            + a[1].0 * LOOP_DRY[1]
            + a[2].0 * LOOP_DRY[2]
            + a[3].0 * LOOP_DRY[3]
            + b * BASS_DRY
            + (g1.0 * GLITCH1_DRY + g2.0 * GLITCH2_DRY + el * ECHO_DRY) * others
            + d * DRUMS_DRY
            + rl * REVERB_WET
            + tl * TAPE_WET;
        let mut r = dr * DRONE_DRY
            + a[0].1 * LOOP_DRY[0]
            + a[1].1 * LOOP_DRY[1]
            + a[2].1 * LOOP_DRY[2]
            + a[3].1 * LOOP_DRY[3]
            + b * BASS_DRY
            + (g1.1 * GLITCH1_DRY + g2.1 * GLITCH2_DRY + er * ECHO_DRY) * others
            + d_r * DRUMS_DRY
            + rr * REVERB_WET
            + tr * TAPE_WET;

        if let Some((sl, sr_)) = stab {
            l += sl;
            r += sr_;
        }
        if micro.is_some() {
            // Less room, the pips and the cloud on top.
            l -= rl * REVERB_WET * (1.0 - MICRO_WET);
            r -= rr * REVERB_WET * (1.0 - MICRO_WET);
            for (x, y) in [pip, cloud].into_iter().flatten() {
                l += x;
                r += y;
            }
            l *= MICRO_LEVEL;
            r *= MICRO_LEVEL;
        }

        let fade_len = FADE_IN_SECONDS * crate::SAMPLE_RATE as u64;
        if clock < fade_len {
            let x = clock as f64 / fade_len as f64;
            l *= x * x;
            r *= x * x;
        }

        self.clock += 1;
        let l = crate::math::tanh(self.dc[0].process(l) * MASTER_GAIN);
        let r = crate::math::tanh(self.dc[1].process(r) * MASTER_GAIN);

        let mt = &mut self.meters;
        mt.drone = mt.drone.max(dl.abs()).max(dr.abs());
        mt.bass = mt.bass.max(b.abs());
        mt.glitch1 = mt.glitch1.max(g1.0.abs()).max(g1.1.abs());
        mt.glitch2 = mt.glitch2.max(g2.0.abs()).max(g2.1.abs());
        mt.echo_l = mt.echo_l.max(el.abs());
        mt.echo_r = mt.echo_r.max(er.abs());
        mt.reverb = mt.reverb.max(rl.abs()).max(rr.abs());
        mt.drums = mt.drums.max(d_abs);
        mt.loop1 = mt.loop1.max(a[0].0.abs()).max(a[0].1.abs());
        mt.loop2 = mt.loop2.max(a[1].0.abs()).max(a[1].1.abs());
        mt.loop3 = mt.loop3.max(a[2].0.abs()).max(a[2].1.abs());
        mt.loop4 = mt.loop4.max(a[3].0.abs()).max(a[3].1.abs());
        mt.tape = mt.tape.max(tl.abs()).max(tr.abs());
        mt.out_l = mt.out_l.max(l.abs());
        mt.out_r = mt.out_r.max(r.abs());

        // The monitor: computed from the same signals, after the mix, never feeding back.
        let mon = &mut self.monitor;
        if mon.current.is_none() && mon.target.is_none() {
            mon.out = (l, r);
        } else {
            if mon.current != mon.target {
                // Fade back to the mix, then switch layer (with fresh echo/reverb).
                mon.amount -= SOLO_RAMP;
                if mon.amount <= 0.0 {
                    mon.amount = 0.0;
                    mon.current = mon.target;
                    mon.echo = PingPong::new(mon.echo_delay, ECHO_FEEDBACK);
                    mon.reverb = Reverb::new();
                    mon.dc = Default::default();
                }
            } else {
                mon.amount = (mon.amount + SOLO_RAMP).min(1.0);
            }
            let fade = if clock < fade_len { let x = clock as f64 / fade_len as f64; x * x } else { 1.0 };
            let (sl, sr) = match mon.current {
                None => (0.0, 0.0),
                Some(Solo::Space) => (
                    el * ECHO_DRY * others + rl * REVERB_WET + tl * TAPE_WET,
                    er * ECHO_DRY * others + rr * REVERB_WET + tr * TAPE_WET,
                ),
                Some(solo) => {
                    // The layer's dry signal, its echo (glitches only) and its own reverb.
                    let (dry, echo_in, send) = match solo {
                        Solo::Drone => ((dl * DRONE_DRY, dr * DRONE_DRY), (0.0, 0.0), (dl * DRONE_SEND, dr * DRONE_SEND)),
                        Solo::Bass => ((b * BASS_DRY, b * BASS_DRY), (0.0, 0.0), (b * BASS_SEND, b * BASS_SEND)),
                        Solo::Loop1 => ((a[0].0 * LOOP_DRY[0], a[0].1 * LOOP_DRY[0]), (0.0, 0.0), (a[0].0 * LOOP_SEND[0], a[0].1 * LOOP_SEND[0])),
                        Solo::Loop2 => ((a[1].0 * LOOP_DRY[1], a[1].1 * LOOP_DRY[1]), (0.0, 0.0), (a[1].0 * LOOP_SEND[1], a[1].1 * LOOP_SEND[1])),
                        Solo::Loop3 => ((a[2].0 * LOOP_DRY[2], a[2].1 * LOOP_DRY[2]), (0.0, 0.0), (a[2].0 * LOOP_SEND[2], a[2].1 * LOOP_SEND[2])),
                        Solo::Loop4 => ((a[3].0 * LOOP_DRY[3], a[3].1 * LOOP_DRY[3]), (0.0, 0.0), (a[3].0 * LOOP_SEND[3], a[3].1 * LOOP_SEND[3])),
                        Solo::Drums => (
                            (d * DRUMS_DRY + tl * TAPE_WET, d_r * DRUMS_DRY + tr * TAPE_WET),
                            (0.0, 0.0),
                            (d * DRUMS_SEND + tl * TAPE_REVERB, d_r * DRUMS_SEND + tr * TAPE_REVERB),
                        ),
                        Solo::Glitch1 => (
                            (g1.0 * GLITCH1_DRY * others, g1.1 * GLITCH1_DRY * others),
                            (g1.0 * GLITCH1_ECHO, g1.1 * GLITCH1_ECHO),
                            (g1.0 * GLITCH1_SEND, g1.1 * GLITCH1_SEND),
                        ),
                        _ => (
                            (g2.0 * GLITCH2_DRY * others, g2.1 * GLITCH2_DRY * others),
                            (g2.0 * GLITCH2_ECHO, g2.1 * GLITCH2_ECHO),
                            (g2.0 * GLITCH2_SEND, g2.1 * GLITCH2_SEND),
                        ),
                    };
                    let (mel, mer) = mon.echo.process(echo_in.0, echo_in.1);
                    let (wl, wr) = mon.reverb.process(send.0 + mel * ECHO_SEND, send.1 + mer * ECHO_SEND);
                    (
                        dry.0 + mel * ECHO_DRY * others + wl * REVERB_WET,
                        dry.1 + mer * ECHO_DRY * others + wr * REVERB_WET,
                    )
                }
            };
            let sl = crate::math::tanh(mon.dc[0].process(sl * fade) * MASTER_GAIN);
            let sr = crate::math::tanh(mon.dc[1].process(sr * fade) * MASTER_GAIN);
            let a = mon.amount;
            mon.out = (l + (sl - l) * a, r + (sr - r) * a);
        }
        self.master.process(l, r)
    }
}

}
// ---- src/audio/weather_bed.rs ----
pub mod weather_bed {
//! The weather bed: the weather as sound.
//!
//! It plays from the *same* [`Weather`] the visuals use. The continuous layers
//! (wind, the rain's wash, snow hush, fog air, rustle, crackle) follow the
//! shared [`crate::weather::WeatherState`] levels the scene draws its particles
//! from, and every event the scene draws — a splash, a drip, an ember's spark,
//! a lightning strike — is found from the same schedule and heard on the same
//! sample clock (thunder after its flash, by the strike's distance). It is
//! mixed on top of the ported 0011 music, which is left untouched.

use crate::audio::ambience::{Ambience, Hit};
use crate::audio::creatures::Creatures;
use crate::audio::rain::Rain;
use crate::audio::thunder::Thunder;
use crate::audio::wind::Wind;
use crate::filter::OnePole;
use crate::rng::Rng;
use crate::weather::{EventKind, Weather, WeatherState};
use crate::weather_fx::DRIP_S;
use crate::wildlife::Wildlife;
use crate::{GRID_H, GRID_W, SAMPLE_RATE};

const SR: f64 = SAMPLE_RATE as f64;
/// The weather state is sampled this often (samples); it moves over seconds.
const CONTROL: usize = 32;

/// The weather bed sits this far under the music (about -8 dB), so it reads as
/// background rather than a foreground instrument.
const BED_GAIN: f64 = 0.4;

/// Whether each raindrop the scene draws is also heard as its own tap or
/// plink. Off: the rain is heard as its wash.
const RAINDROP_HITS: bool = false;

/// Wind, rain, thunder and the small sounds, mixed into an existing
/// interleaved stereo buffer.
#[derive(Clone)]
pub struct WeatherBed {
    wind: Wind,
    rain: Rain,
    thunder: Thunder,
    ambience: Ambience,
    /// The creatures, when the Wildlife setting is on.
    wildlife: Option<Wildlife>,
    creatures: Creatures,
    /// Which grid cells hold a column (a splash there lands on stone).
    hard: Vec<bool>,
    /// Snow muffles everything in the bed.
    muffle: [OnePole; 2],
    /// Under a roof: the weather is heard through glass, every splash lands
    /// on a pane, and the wind is a muffled pressure outside.
    roofed: bool,
    state: Option<WeatherState>,
    tick: usize,
    /// Sounds found by the schedule, waiting for their exact sample. They
    /// start then (not at the top of the block that found them), so nothing
    /// about the bed depends on how the audio is cut into blocks.
    queue: Vec<(u64, Due)>,
}

/// A scheduled sound, waiting to start.
#[derive(Clone)]
enum Due {
    Hit(Hit),
    Call(crate::wildlife::Call),
    Thunder { strength: f64, distance: f64, pan: f64, hash: u64 },
}

impl WeatherBed {
    /// `hard` is one flag per grid cell, row-major: does a column stand there?
    /// `wildlife` is `Some` when the Wildlife setting is on.
    pub fn new(seed: u64, hard: Vec<bool>, wildlife: Option<Wildlife>) -> Self {
        WeatherBed {
            wind: Wind::new(seed),
            rain: Rain::new(seed ^ 0x9E37_79B9_7F4A_7C15),
            thunder: Thunder::default(),
            ambience: Ambience::new(seed),
            wildlife,
            creatures: Creatures::default(),
            hard,
            muffle: [OnePole::new(9_000.0, SR), OnePole::new(9_000.0, SR)],
            roofed: false,
            state: None,
            tick: 0,
            queue: Vec::new(),
        }
    }

    /// Heard from under a roof (the indoor futures).
    pub fn roofed(mut self, on: bool) -> Self {
        self.roofed = on;
        self
    }

    fn is_hard(&self, x: f64, y: f64) -> bool {
        if self.roofed {
            // Every drop lands on the glass.
            return true;
        }
        let (gx, gy) = (x.round() as i64, y.round() as i64);
        if gx < 0 || gy < 0 || gx >= GRID_W as i64 || gy >= GRID_H as i64 {
            return false;
        }
        self.hard.get((gy * GRID_W as i64 + gx) as usize).copied().unwrap_or(false)
    }

    /// Queue every event that sounds in samples `[s0, s1)`, at its own
    /// sample. Each sound's randomness comes from its event, never from a
    /// stream shared with the bed's continuous noise.
    fn schedule(&mut self, weather: &Weather, s0: u64, s1: u64) {
        // Look a couple of samples wider than the block, then keep exactly
        // the sounds whose own sample falls inside it: each sound belongs to
        // one block by its sample number, never by a rounded time.
        let pad = 2.0 / SR;
        let (t0, t1) = (s0 as f64 / SR - pad, s1 as f64 / SR + pad);
        let at_sample = |at: f64| (at.max(0.0) * SR) as u64;
        let mut due: Vec<(u64, Due)> = Vec::new();
        // Sounds owned by an event in this block but landing after it.
        let mut spill: Vec<(u64, Due)> = Vec::new();
        // Single raindrops (taps, plinks, drips) are seen but no longer heard:
        // as separate pitched hits they bubbled over the music. The rain is
        // heard as its wash alone.
        if RAINDROP_HITS {
            for e in weather.events(EventKind::Splash, t0, t1) {
                let (x, y) = e.spot();
                let (pan, hard) = (e.pan(), self.is_hard(x, y));
                let gain = e.rng().range(0.08, 0.22);
                let mut r = Rng::new(e.hash ^ 0x4849_5453);
                let hit = if hard { Hit::plink(0, &mut r, pan, gain) } else { Hit::tap(0, &mut r, pan, gain * 1.4) };
                due.push((at_sample(e.at), Due::Hit(hit)));
            }
            // A drip is heard when it lands, at the end of its fall.
            for e in weather.events(EventKind::Drip, t0 - DRIP_S, t1 - DRIP_S) {
                let mut r = Rng::new(e.hash ^ 0x4452_4950);
                due.push((at_sample(e.at + DRIP_S), Due::Hit(Hit::drip(0, &mut r, e.pan(), 0.16))));
            }
        }
        for e in weather.events(EventKind::Spark, t0, t1) {
            let pan = e.pan();
            let mut r = e.rng();
            // A spark's crackles belong to the spark, wherever they land.
            if at_sample(e.at) < s0 || at_sample(e.at) >= s1 {
                continue;
            }
            for k in 0..r.int(2, 4) {
                let at = e.at + k as f64 * r.range(0.006, 0.02);
                let mut rr = Rng::new(e.hash ^ 0x5350_4b00 ^ k);
                spill.push((at_sample(at), Due::Hit(Hit::crackle(0, &mut rr, pan, 0.22))));
            }
        }
        if let Some(wildlife) = &self.wildlife {
            for call in wildlife.calls(weather.kind, t0, t1) {
                due.push((at_sample(call.at), Due::Call(call)));
            }
        }
        for e in weather.events(EventKind::Strike, t0 - 3.1, t1) {
            let at = e.thunder_at();
            if at >= t0 && at < t1 {
                let (strength, distance) = e.strike();
                due.push((at_sample(at), Due::Thunder { strength, distance, pan: e.pan(), hash: e.hash }));
            }
        }
        due.retain(|d| d.0 >= s0 && d.0 < s1);
        due.extend(spill);
        // Stable: sounds due on the same sample keep the schedule's order.
        due.sort_by_key(|d| d.0);
        self.queue.extend(due);
        self.queue.sort_by_key(|d| d.0);
    }

    /// Start everything due at sample `s`.
    fn start_due(&mut self, s: u64) {
        while self.queue.first().is_some_and(|d| d.0 <= s) {
            let (_, due) = self.queue.remove(0);
            match due {
                Due::Hit(hit) => self.ambience.start(hit),
                Due::Call(call) => self.creatures.trigger(&call, 0),
                Due::Thunder { strength, distance, pan, hash } => self.thunder.add(0, strength, distance, pan, hash),
            }
        }
    }

    /// Add the bed into `out`, starting at `t0` seconds. `out` must already hold
    /// the music mix; this only adds to it.
    pub fn render(&mut self, out: &mut [f32], t0: f64, weather: &Weather) {
        self.render_from((t0 * SR).round() as u64, out, weather);
    }

    /// [`WeatherBed::render`] from sample `s0`: the exact clock, so the bed
    /// is the same however the audio is cut into blocks.
    pub fn render_from(&mut self, s0: u64, out: &mut [f32], weather: &Weather) {
        let frames = out.len() / 2;
        self.ambience.rustle_on = weather.kind != crate::weather::WeatherKind::Wind;
        self.schedule(weather, s0, s0 + frames as u64);
        for i in 0..frames {
            let s = s0 + i as u64;
            self.start_due(s);
            if self.tick % CONTROL == 0 || self.state.is_none() {
                let st = weather.state(s as f64 / SR);
                for m in self.muffle.iter_mut() {
                    let open = 9_000.0 - 6_800.0 * st.snow;
                    m.set_cutoff(if self.roofed { open * 0.4 } else { open }, SR);
                }
                self.state = Some(st);
            }
            self.tick = self.tick.wrapping_add(1);
            let s = self.state.expect("sampled above");
            let (mut wl, mut wr) = self.wind.next(s.wind_strength, s.gust, s.wind_dir);
            if self.roofed {
                wl *= 0.45;
                wr *= 0.45;
            }
            let (rl, rr) = self.rain.next(s.rain);
            let (tl, tr) = self.thunder.next();
            let (al, ar) = self.ambience.next(&s);
            let (cl, cr) = self.creatures.next();
            let l = self.muffle[0].process(wl + rl + al + cl) + tl;
            let r = self.muffle[1].process(wr + rr + ar + cr) + tr;
            out[2 * i] += (l * BED_GAIN).clamp(-1.0, 1.0) as f32;
            out[2 * i + 1] += (r * BED_GAIN).clamp(-1.0, 1.0) as f32;
        }
    }
}

}
// ---- src/audio/wind.rs ----
pub mod wind {
//! Continuous wind: noise through a slow filter, with a whistle that rises
//! with each gust.
//!
//! This is a first-class instrument, not an effect: the same
//! [`crate::weather::WeatherState`] that sways the foliage drives it, gusts
//! included, so the trees move in time with what you hear.

use crate::filter::{OnePole, Svf};
use crate::math::pan_gains;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5749_4e44; // "WIND"
const SR: f64 = SAMPLE_RATE as f64;
/// How often the whistle's pitch is retuned, in samples.
const CONTROL: usize = 32;

/// A gusting wind voice.
#[derive(Clone)]
pub struct Wind {
    rng: Rng,
    lp: OnePole,
    bp: Svf,
    whistle: Svf,
    tick: usize,
}

impl Wind {
    pub fn new(seed: u64) -> Self {
        Wind {
            rng: Rng::stream(seed, STREAM),
            lp: OnePole::new(140.0, SR),
            bp: Svf::new(480.0, 0.8, SR),
            whistle: Svf::new(600.0, 9.0, SR),
            tick: 0,
        }
    }

    /// One stereo sample. `strength` is the wind (gusts included), `gust` the
    /// gust alone and `dir` the direction in turns; all in `[0, 1]`.
    pub fn next(&mut self, strength: f64, gust: f64, dir: f64) -> (f64, f64) {
        let (strength, gust) = (strength.clamp(0.0, 1.0), gust.clamp(0.0, 1.0));
        if self.tick % CONTROL == 0 {
            // The whistle climbs as the gust builds, as wind does round stone.
            self.whistle.set(520.0 + 1_100.0 * gust, 9.0, SR);
        }
        self.tick = self.tick.wrapping_add(1);
        let white = self.rng.bipolar();
        let low = self.lp.process(white);
        let body = self.bp.process(low).band;
        let whistle = self.whistle.process(white).band;
        let amp = strength * (0.9 * body + 0.35 * (0.4 + 0.6 * gust) * whistle);
        let pan = ((dir.clamp(0.0, 1.0) * 2.0 - 1.0) * 0.6).clamp(-1.0, 1.0);
        let (l, r) = pan_gains(pan);
        (amp * l, amp * r)
    }
}

}
pub use self::track::*;
}

// ============================================================================
// Baked recipe — this artifact is this shrine
// ============================================================================
/// `recipe::encode` of the settings the setup menu produced.
const RECIPE: [u32; 168] = [18, 0, 65088, 0, 40762, 0, 29313, 0, 97911, 0, 9058, 0, 72736, 0, 71667, 0, 91505, 1, 6, 3, 3, 0, 8, 1, 156, 4, 12, 0, 0, 6000, 1, 1, 0, 0, 0, 40, 25, 1, 15, 0, 0, 0, 300, 226, 1, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 120, 77, 0, 72, 3358395393, 51380, 188928180, 133152, 9592815, 57929, 14049389, 42053, 178, 1, 5434, 0, 2646, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 892416048, 1444949280, 1633905775, 7173486, 0, 0, 0, 0];

/// Build date as `YYYYMMDD`, stamped when this file was emitted.
const BUILD: u32 = 20261021;

/// The source-code overlay word: bit 0 enabled, byte 1 opacity, byte 2 speed, byte 3 text scale.
const CODE_OVERLAY: u32 = 3358395393;

/// The overlay's tilt: positive leans the drift to the left.
const CODE_ANGLE: i32 = 0;

/// The overlay's vertical shift, percent of the canvas height.
const CODE_OFFSET: i32 = 0;

/// Camera rotation speed, degrees per second.
const CAMERA_SPEED: i32 = -2;

/// The track name, shown at the head of the page and used as the WAV title.
const NAME: &str = "0015 - Volcanum";

/// Everything needed to recreate this shrine.
const INFO: &str = "ShrineGen  v20261021\n\nseed       65088\nbiome      Verdant\nweather    Dusk\nwildlife   off\n\nscale      Phrygian\nroot note  D#\nchords     3\nmode       half-time\ntempo      156 BPM\nstart      at the beginning\nkit        Digital crush\nspace      Wide\ndrum rev   off\nmaster fx  off\ncode over  on · 12% · speed 45 · text 200%\narch       centred, upright\ncamera     -2°/s\nglitch 1   5 kinds · 63% · ×2 · 10 steps\nglitch 2   4 kinds · 64% · ×2 · 12 steps\ntranspose  0 semitones\n\ndrone arc  always on\ndrone tone 6000 Hz low-pass\nlayer 1    Bells\n  arc 1    40s cycle, 25s up\nlayer 2    Ebb\n  arc 2    300s cycle, 226s up\nlayer 3    Gust\n  arc 3    always on\nlayer 4    Off\n  arc 4    120s cycle, 77s up\n\nseeds      40762 29313 97911 9058 72736 71667 91505\n           keys&chords · glitch1 · glitch2 · bass · drums · atmosphere · drone rhythm\nrhythm     drone on · loops off\n\nMusic License: CC0 / Public Domain\nCode License: MIT";

// ============================================================================
// C ABI — plain extern "C", loadable with the standard WebAssembly API
// ============================================================================

/// Build date as `YYYYMMDD` (e.g. 20261011).
#[no_mangle]
pub extern "C" fn shrine_build_date() -> u32 {
    BUILD
}

/// The source-code overlay word: bit 0 enabled, byte 1 opacity, byte 2 speed.
#[no_mangle]
pub extern "C" fn shrine_code_overlay() -> u32 {
    CODE_OVERLAY
}

/// The overlay's tilt in degrees (positive leans the drift to the left).
#[no_mangle]
pub extern "C" fn shrine_code_angle() -> i32 {
    CODE_ANGLE
}

/// The overlay's vertical shift, percent of the canvas height.
#[no_mangle]
pub extern "C" fn shrine_code_offset() -> i32 {
    CODE_OFFSET
}

/// The camera rotation speed in degrees per second, so the page can orbit its
/// own sky in step with the engine.
#[no_mangle]
pub extern "C" fn shrine_camera_speed() -> i32 {
    CAMERA_SPEED
}

/// Pointer to the UTF-8 info card (`shrine_info_len` bytes).
#[no_mangle]
pub extern "C" fn shrine_info_ptr() -> *const u8 {
    INFO.as_ptr()
}

/// Length of the info card in bytes.
#[no_mangle]
pub extern "C" fn shrine_info_len() -> u32 {
    INFO.len() as u32
}

/// Pointer to the UTF-8 track name (`shrine_name_len` bytes).
#[no_mangle]
pub extern "C" fn shrine_name_ptr() -> *const u8 {
    NAME.as_ptr()
}

/// Length of the track name in bytes (0 when none was chosen).
#[no_mangle]
pub extern "C" fn shrine_name_len() -> u32 {
    NAME.len() as u32
}

/// Nudge the camera from the page's pointer, in `[-1, 1]` per axis.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_set_camera(player: *mut crate::player::Player, yaw: f64, pitch: f64) {
    let p = unsafe { &mut *player };
    p.set_camera(yaw, pitch);
}

#[no_mangle]
pub unsafe extern "C" fn shrine_set_sky_transparent(player: *mut crate::player::Player, on: u32) {
    let p = unsafe { &mut *player };
    p.set_sky_transparent(on != 0);
}

#[no_mangle]
pub unsafe extern "C" fn shrine_sky_colors(player: *mut crate::player::Player) -> u64 {
    let p = unsafe { &*player };
    p.sky_colors()
}

/// Build a player. `mode`: 1 = audio, 2 = visuals. The caller's words are
/// ignored: this artifact always uses its baked `RECIPE`.
///
/// # Safety
/// Returns an owned pointer to be released with `shrine_free`.
#[no_mangle]
pub unsafe extern "C" fn shrine_new(_words: *const u32, _words_len: u32, mode: u32) -> *mut crate::player::Player {
    let config = crate::recipe::decode(&RECIPE);
    let sim = if mode == 2 { crate::sim::Sim::new_visual(config) } else { crate::sim::Sim::new(config) };
    Box::into_raw(Box::new(crate::player::Player::new(sim)))
}

/// Render `frames` stereo samples; the pointer is valid until the next call.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_render_audio(player: *mut crate::player::Player, frames: u32) -> *const f32 {
    let p = unsafe { &mut *player };
    p.render_audio(frames as usize).as_ptr()
}

/// Render one 300x300 RGBA frame at `sample_clock`; valid until the next call.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_render_visual(player: *mut crate::player::Player, sample_clock: u64) -> *const u8 {
    let p = unsafe { &mut *player };
    p.render_visual(sample_clock).as_ptr()
}

/// The Learn page's description of this shrine: UTF-8 JSON, `shrine_learn_len` bytes.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_ptr(player: *mut crate::player::Player) -> *const u8 {
    let p = unsafe { &mut *player };
    p.learn_json().as_ptr()
}

/// Length of the description in bytes.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_len(player: *mut crate::player::Player) -> u32 {
    let p = unsafe { &mut *player };
    p.learn_json().len() as u32
}

/// The music's moving parts now: `shrine_learn_state_len` f64s (layout in
/// `learn::STATE_LEN`). Display only.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_state(player: *mut crate::player::Player) -> *const f64 {
    let p = unsafe { &mut *player };
    p.learn_state().as_ptr()
}

#[no_mangle]
pub extern "C" fn shrine_learn_state_len() -> u32 {
    crate::learn::STATE_LEN as u32
}

/// The weather at `sample_clock`: 6 f64s (wind, gust, rain, snow, fog, light).
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_weather(player: *mut crate::player::Player, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.weather_now(sample_clock).as_ptr()
}

/// Where grid cell `(gx, gy)`'s top surface is on screen at `sample_clock`:
/// 3 f64s (x, y in logical pixels, its level); x = −1 off the grid.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_surface_point(player: *mut crate::player::Player, gx: i32, gy: i32, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.surface_point(gx, gy, sample_clock).as_ptr()
}

/// Hear one layer alone (1 drone, 2–3 glitch layers, 4 bass, 5 drums, 6–9
/// atmosphere loops, 10 the echo and reverb), or 0 for the full mix.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_set_solo(player: *mut crate::player::Player, layer: u32) {
    let p = unsafe { &mut *player };
    p.set_solo(layer);
}

/// Release a player.
///
/// # Safety
/// `player` must come from `shrine_new` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn shrine_free(player: *mut crate::player::Player) {
    if !player.is_null() {
        unsafe { drop(Box::from_raw(player)) };
    }
}
