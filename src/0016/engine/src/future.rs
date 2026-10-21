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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_future_has_its_own_palette_and_the_wild_keeps_theirs() {
        let p = Palette::from_seed(9);
        for biome in Biome::ALL {
            let q = palette(9, p, biome);
            assert_eq!(q == p, !biome.futuristic(), "{biome:?}");
        }
    }
}
