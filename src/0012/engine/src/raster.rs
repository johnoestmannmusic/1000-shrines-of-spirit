//! Raster primitives for the logical buffer.
//!
//! Everything works in the fixed-point plane from [`crate::iso`] and rounds to
//! whole logical pixels only at the final span fill. Fills are expanded by up to
//! one pixel so adjacent diamonds never leave a seam; later tiles simply
//! overwrite the overlap, and painting order is the caller's depth order.

use crate::framebuffer::{Framebuffer, Rgba};
use crate::iso::{Iso, FP};

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
        fb.fill_span(px, y0, y1, colour);
    }
}

/// Draw a full column: the two visible side faces between the ground and the
/// column's top, then its top face. `height` is the number of stacked tiles.
pub fn fill_column(fb: &mut Framebuffer, iso: &Iso, gx: i32, gy: i32, height: u32, top: Rgba, left: Rgba, right: Rgba) {
    if height == 0 {
        return;
    }
    let c = iso.project(gx, gy, height as i32 - 1);
    let (hw, hh) = (iso.half_w(), iso.half_h());
    let drop = (height as i32 - 1) * iso.level_h;
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
