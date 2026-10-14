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
