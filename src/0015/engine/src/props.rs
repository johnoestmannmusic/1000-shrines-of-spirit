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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_futures_have_props_and_each_has_some() {
        for biome in Biome::ALL {
            for seed in 0..6 {
                let p = Props::generate(seed, biome);
                assert_eq!(p.items.is_empty(), !biome.futuristic(), "{biome:?} seed {seed}");
                assert_eq!(p, Props::generate(seed, biome));
            }
        }
    }

    #[test]
    fn the_city_strings_its_poles() {
        let any = (0..10).any(|s| !Props::generate(s, Biome::NearFuture).wires.is_empty());
        assert!(any, "no wires between the utility poles");
    }
}
