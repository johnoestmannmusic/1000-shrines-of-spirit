//! Stacked 2.5D tile layers — the data model the shrine generator and the
//! future editor both write to.
//!
//! A [`Layers`] value owns a fixed 24x24 grid, repeated for each height level.
//! A level is a flat diamond of tiles at one `z`; the shrine is a stack of
//! them. The module is deliberately shrine-agnostic: it stores tiles and
//! nothing else, so other scenes can reuse it (`SC002`).

use crate::{GRID_H, GRID_W};

/// Cells per level (24x24).
pub const CELLS: usize = (GRID_W * GRID_H) as usize;

/// Upper bound on stack height, so the model is a flat array rather than a
/// growable tree. The shrine is expected to use far fewer levels.
pub const MAX_LEVELS: u32 = 32;

/// One cell of a layer. `kind` 0 means empty; `variant` is cosmetic and
/// deterministic (seeded), never behaviour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    pub kind: u8,
    pub variant: u8,
}

impl Tile {
    pub const EMPTY: Tile = Tile { kind: 0, variant: 0 };

    pub const fn new(kind: u8, variant: u8) -> Self {
        Tile { kind, variant }
    }

    pub const fn is_empty(self) -> bool {
        self.kind == 0
    }
}

/// A stack of tile layers, row-major within each level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers {
    tiles: Vec<Tile>,
    levels: u32,
}

impl Layers {
    /// New empty layers, clamped to [`MAX_LEVELS`].
    pub fn new(levels: u32) -> Self {
        let levels = levels.min(MAX_LEVELS);
        Layers {
            tiles: vec![Tile::EMPTY; levels as usize * CELLS],
            levels,
        }
    }

    pub fn levels(&self) -> u32 {
        self.levels
    }

    pub fn in_bounds(&self, gx: i32, gy: i32, gz: i32) -> bool {
        gx >= 0
            && gy >= 0
            && gz >= 0
            && (gx as u32) < GRID_W
            && (gy as u32) < GRID_H
            && (gz as u32) < self.levels
    }

    fn index(gx: u32, gy: u32, gz: u32) -> usize {
        (gz * CELLS as u32 + gy * GRID_W + gx) as usize
    }

    /// The tile at a cell, or [`Tile::EMPTY`] when out of bounds.
    pub fn get(&self, gx: i32, gy: i32, gz: i32) -> Tile {
        if !self.in_bounds(gx, gy, gz) {
            return Tile::EMPTY;
        }
        self.tiles[Self::index(gx as u32, gy as u32, gz as u32)]
    }

    /// Write a tile. Returns false when the cell is out of bounds.
    pub fn set(&mut self, gx: i32, gy: i32, gz: i32, tile: Tile) -> bool {
        if !self.in_bounds(gx, gy, gz) {
            return false;
        }
        let i = Self::index(gx as u32, gy as u32, gz as u32);
        self.tiles[i] = tile;
        true
    }

    /// Fill an entire level with one tile.
    pub fn fill_level(&mut self, gz: i32, tile: Tile) {
        if gz < 0 || gz as u32 >= self.levels {
            return;
        }
        let start = gz as usize * CELLS;
        for t in &mut self.tiles[start..start + CELLS] {
            *t = tile;
        }
    }

    /// The tiles of one level as a flat, row-major slice.
    pub fn level(&self, gz: u32) -> &[Tile] {
        let start = gz as usize * CELLS;
        &self.tiles[start..start + CELLS]
    }

    pub fn count_non_empty(&self) -> usize {
        self.tiles.iter().filter(|t| !t.is_empty()).count()
    }

    /// A stable FNV-1a hash of every tile, for golden tests and previews.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for t in &self.tiles {
            h ^= t.kind as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
            h ^= t.variant as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// Iterate `(gx, gy, tile)` for one level, row-major.
    pub fn iter_level(&self, gz: u32) -> impl Iterator<Item = (u32, u32, Tile)> + '_ {
        self.level(gz).iter().enumerate().map(|(i, &tile)| {
            let i = i as u32;
            (i % GRID_W, i / GRID_W, tile)
        })
    }
}
