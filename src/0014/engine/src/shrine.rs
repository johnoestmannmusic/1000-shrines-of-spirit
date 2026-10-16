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
