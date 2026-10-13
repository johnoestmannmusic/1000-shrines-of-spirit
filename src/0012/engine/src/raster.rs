//! Raster primitives for the logical buffer.
//!
//! Everything works in the fixed-point plane from [`crate::iso`] and rounds to
//! whole logical pixels only at the final span fill. Fills are expanded by up to
//! one pixel so adjacent diamonds never leave a seam; later tiles simply
//! overwrite the overlap, and painting order is the caller's depth order.

use crate::framebuffer::{Framebuffer, Rgba};
use crate::iso::{Iso, Point, FP};

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
    let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0);
    let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0);
    for py in floor_div(min_y, FP)..=ceil_div(max_y, FP) {
        // The pixel's own centre decides whether the span crosses it.
        let yc = py * FP + FP / 2;
        let (mut lo, mut hi, mut hits) = (i32::MAX, i32::MIN, 0);
        for i in 0..4 {
            let a = corners[i];
            let b = corners[(i + 1) % 4];
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

/// Draw one column of the *rotated* architecture.
///
/// The cell is a box from the ground up to its top level. Its six faces are
/// culled by their rotated normal against the view direction, so exactly the
/// camera-facing ones are drawn, and the top is filled last. At a rotation of
/// zero the visible set and the shading match [`fill_column`]'s, so turning the
/// shrine never changes how it reads.
pub fn fill_box(
    fb: &mut Framebuffer,
    iso: &Iso,
    rot: &[[f64; 3]; 3],
    off: (f64, f64, f64),
    gx: i32,
    gy: i32,
    height: u32,
    top: Rgba,
    side_x: Rgba,
    side_y: Rgba,
) {
    if height == 0 {
        return;
    }
    let zt = (height - 1) as f64;
    let p = |x: f64, y: f64, z: f64| iso.project_rotated(x, y, z, rot, off);
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

    if zt > 0.0 {
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
            fill_quad(
                fb,
                [
                    p(cx + ax, cy + ay, zt),
                    p(cx + bx, cy + by, zt),
                    p(cx + bx, cy + by, 0.0),
                    p(cx + ax, cy + ay, 0.0),
                ],
                colour,
            );
        }
    }

    // Top face, or the underside once the rotation has turned the box over.
    let z_face = if facing([0.0, 0.0, 1.0]) {
        Some(zt)
    } else if facing([0.0, 0.0, -1.0]) {
        Some(0.0)
    } else {
        None
    };
    if let Some(z) = z_face {
        fill_quad(
            fb,
            [
                p(cx - h, cy - h, z),
                p(cx + h, cy - h, z),
                p(cx + h, cy + h, z),
                p(cx - h, cy + h, z),
            ],
            top,
        );
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
