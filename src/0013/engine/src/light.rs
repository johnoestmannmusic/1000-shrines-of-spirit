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
        let p = profile(setup.kind);
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
