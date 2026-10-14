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
        let moss = if biome == Biome::Fungal { [p.foliage_dark, p.accent] } else { [p.foliage_dark, p.foliage] };
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
}

impl Materials {
    pub fn new(seed: u64, palette: &Palette, biome: Biome) -> Self {
        Materials { stock: Stock::of(palette, biome), moss: moss_of(biome), seed }
    }

    /// The colour of one pixel of a block (or of the ground, for
    /// `kind::EMPTY`).
    pub fn albedo(&self, b: &Block, s: &Surface, ex: Exposure) -> Albedo {
        albedo(&self.stock, self.moss, self.seed, b, s, ex)
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
