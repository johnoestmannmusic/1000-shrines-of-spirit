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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_indoor_futures_are_roofed_and_the_roof_clears_the_shrine() {
        for biome in Biome::ALL {
            let roof = Roof::new(biome, 6.0);
            assert_eq!(roof.is_some(), biome.roofed());
            if let Some(r) = roof {
                assert!(r.height_at(0.0, 0.0) >= 7.5);
            }
        }
        let g = Roof::new(Biome::Greenhouse, 3.0).unwrap();
        assert!(g.height_at(5.0, 11.5) > g.height_at(5.0, -0.5), "the greenhouse ridge is highest");
    }
}
