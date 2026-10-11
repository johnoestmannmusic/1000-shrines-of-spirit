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
