//! The authored architecture: an optional hand-edited 16x16 model.
//!
//! A cell has a height of **0 to 6**. Height 0 means there is no column there
//! at all, so the ground shows through — the editor's "remove the base for this
//! square". Because a cell can legitimately be all zeros, "is this model
//! authored?" is an explicit flag rather than an all-zero sentinel.
//!
//! When `authored` is false the seed's generator stays authoritative, so the
//! default recipe carries no architecture of its own and today's output is
//! untouched (`HC004`).
//!
//! Each cell is one byte: bits 0-2 the height (0..6), bits 3-5 the tile kind,
//! bits 6-7 the cosmetic variation. That is 256 bytes plus the flag, or 65
//! recipe words, which is what keeps the artifact self-contained (`G001`).

use crate::shrine::{kind, Shrine, MAX_HEIGHT};
use crate::{SHRINE_H, SHRINE_W};

/// Cells in the footprint.
pub const CELLS: usize = (SHRINE_W * SHRINE_H) as usize;
/// Recipe words the model occupies: one flag, then four cells per word.
pub const WORDS: usize = CELLS / 4 + 1;

/// A shrine's architecture, authored by hand or still procedural.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShrineModel {
    authored: bool,
    cells: [u8; CELLS],
}

impl ShrineModel {
    /// Not authored: the generator decides.
    pub const EMPTY: ShrineModel = ShrineModel { authored: false, cells: [0; CELLS] };

    /// True once the editor has taken over.
    pub fn is_authored(&self) -> bool {
        self.authored
    }

    /// Forget every edit and hand the architecture back to the generator.
    pub fn clear(&mut self) {
        self.authored = false;
        self.cells = [0; CELLS];
    }

    /// Empty the plan: an authored model in which every square is a hole.
    pub fn blank(&mut self) {
        self.authored = true;
        self.cells = [0; CELLS];
    }

    fn index(x: usize, y: usize) -> usize {
        (y % SHRINE_H as usize) * SHRINE_W as usize + (x % SHRINE_W as usize)
    }

    /// Height of a cell, 0 (no column) to 6.
    pub fn height(&self, x: usize, y: usize) -> u8 {
        self.cells[Self::index(x, y)] & 0x7
    }

    /// Tile kind of a cell's top, see [`kind`].
    pub fn kind(&self, x: usize, y: usize) -> u8 {
        (self.cells[Self::index(x, y)] >> 3) & 0x7
    }

    /// Cosmetic variation, 0..=3.
    pub fn variant(&self, x: usize, y: usize) -> u8 {
        (self.cells[Self::index(x, y)] >> 6) & 0x3
    }

    /// Write one cell. Heights and kinds are clamped, so the model is always
    /// valid however it is driven.
    pub fn set(&mut self, x: usize, y: usize, height: u8, kind: u8, variant: u8) {
        self.authored = true;
        let h = height.min(MAX_HEIGHT);
        let k = kind & 0x7;
        let v = variant & 0x3;
        let i = Self::index(x, y);
        self.cells[i] = (h & 0x7) | (k << 3) | (v << 6);
    }

    /// Read the generated shrine into a model, so the editor starts from what
    /// the seed produced.
    pub fn from_shrine(shrine: &Shrine) -> Self {
        let mut model = ShrineModel::EMPTY;
        let (ox, oy) = shrine.origin();
        for y in 0..SHRINE_H as usize {
            for x in 0..SHRINE_W as usize {
                let h = shrine.height_at(x as i32, y as i32).min(MAX_HEIGHT);
                let tile = if h == 0 {
                    crate::grid::Tile::EMPTY
                } else {
                    shrine.layers.get(x as i32 + ox, y as i32 + oy, h as i32 - 1)
                };
                model.set(x, y, h, tile.kind.max(kind::FLOOR), tile.variant);
            }
        }
        model
    }

    /// The model as recipe words: the authored flag, then four cells per word.
    pub fn words(&self) -> [u32; WORDS] {
        let mut out = [0u32; WORDS];
        out[0] = self.authored as u32;
        for (i, w) in out.iter_mut().skip(1).enumerate() {
            let b = i * 4;
            *w = u32::from_le_bytes([self.cells[b], self.cells[b + 1], self.cells[b + 2], self.cells[b + 3]]);
        }
        out
    }

    /// Rebuild from recipe words. Short input leaves the rest unauthored.
    pub fn from_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let Some((&flag, rest)) = words.split_first() else {
            return model;
        };
        model.authored = flag & 1 != 0;
        for (i, w) in rest.iter().take(CELLS / 4).enumerate() {
            model.cells[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        // Keep every cell inside the model's own valid range.
        for c in model.cells.iter_mut() {
            let h = (*c & 0x7).min(MAX_HEIGHT);
            let k = ((*c >> 3) & 0x7).min(kind::STONE);
            let v = (*c >> 6) & 0x3;
            *c = h | (k << 3) | (v << 6);
        }
        model
    }

    /// Rebuild from the pre-flag layout (recipe versions 11 and 12), where the
    /// words were all cells and a non-empty model meant "authored".
    pub fn from_legacy_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        for (i, w) in words.iter().take(CELLS / 4).enumerate() {
            model.cells[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        model.authored = model.cells.iter().any(|c| *c != 0);
        for c in model.cells.iter_mut() {
            // The old layout held height - 1, so every old cell is one higher.
            let h = ((*c & 0x7) + 1).min(MAX_HEIGHT);
            let k = ((*c >> 3) & 0x7).min(kind::STONE);
            let v = (*c >> 6) & 0x3;
            *c = h | (k << 3) | (v << 6);
        }
        model
    }

    /// The raw cells as lowercase hex, for the seed file. Two hex digits per
    /// cell, so an authored architecture survives export and import.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(CELLS * 2);
        for &c in &self.cells {
            out.push(char::from_digit((c >> 4) as u32, 16).unwrap_or('0'));
            out.push(char::from_digit((c & 0xf) as u32, 16).unwrap_or('0'));
        }
        out
    }

    /// Read [`to_hex`] back. Returns `None` if the text is the wrong length or
    /// carries characters that are not hex.
    pub fn from_hex(text: &str) -> Option<ShrineModel> {
        let t = text.trim();
        if t.len() != CELLS * 2 {
            return None;
        }
        let bytes = t.as_bytes();
        let mut cells = [0u8; CELLS];
        for i in 0..CELLS {
            let hi = (bytes[i * 2] as char).to_digit(16)?;
            let lo = (bytes[i * 2 + 1] as char).to_digit(16)?;
            cells[i] = (hi * 16 + lo) as u8;
        }
        let mut model = ShrineModel { authored: true, cells };
        model = ShrineModel::from_words(&{
            let mut w = [0u32; WORDS];
            w.copy_from_slice(&model.words());
            w
        });
        Some(model)
    }

    /// A stable hash, for tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.cells {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^ (self.authored as u64)
    }
}

impl Default for ShrineModel {
    fn default() -> Self {
        ShrineModel::EMPTY
    }
}

impl std::fmt::Debug for ShrineModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShrineModel({} bytes, authored: {})", CELLS, self.is_authored())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_model_is_not_authored() {
        assert!(!ShrineModel::EMPTY.is_authored());
        assert_eq!(ShrineModel::default(), ShrineModel::EMPTY);
    }

    #[test]
    fn a_cell_can_be_a_hole() {
        let mut model = ShrineModel::EMPTY;
        model.set(4, 4, 0, kind::FLOOR, 0);
        assert!(model.is_authored());
        assert_eq!(model.height(4, 4), 0, "height 0 removes the base");
        // A blanked plan is authored but empty, and distinguishable from
        // "never edited".
        let mut blank = ShrineModel::EMPTY;
        blank.blank();
        assert!(blank.is_authored());
        assert_eq!(blank.height(4, 4), 0);
    }

    #[test]
    fn a_written_model_round_trips_and_survives_corruption() {
        let mut model = ShrineModel::EMPTY;
        model.set(3, 5, 4, kind::PILLAR, 2);
        model.set(15, 0, 6, kind::ALTAR, 3);
        model.set(1, 1, 0, kind::FLOOR, 1);
        assert!(model.is_authored());
        assert_eq!(model.height(3, 5), 4);
        assert_eq!(model.kind(3, 5), kind::PILLAR);
        assert_eq!(model.variant(3, 5), 2);
        assert_eq!(ShrineModel::from_words(&model.words()), model);

        // Wild bytes must clamp into range, never panic.
        let wild = vec![u32::MAX; WORDS];
        let fixed = ShrineModel::from_words(&wild);
        for y in 0..SHRINE_H as usize {
            for x in 0..SHRINE_W as usize {
                assert!(fixed.height(x, y) <= MAX_HEIGHT);
                assert!(fixed.kind(x, y) <= kind::STONE);
                assert!(fixed.variant(x, y) <= 3);
            }
        }
        // Short input leaves the model procedural.
        assert!(!ShrineModel::from_words(&[]).is_authored());
    }

    #[test]
    fn a_model_round_trips_through_hex() {
        let mut model = ShrineModel::EMPTY;
        model.set(0, 0, 1, kind::FLOOR, 0);
        model.set(7, 9, 6, kind::ALTAR, 3);
        model.set(2, 2, 0, kind::FLOOR, 0);
        let text = model.to_hex();
        assert_eq!(text.len(), CELLS * 2);
        assert_eq!(ShrineModel::from_hex(&text), Some(model));
        // Junk is rejected, not guessed at.
        assert_eq!(ShrineModel::from_hex("nope"), None);
        assert_eq!(ShrineModel::from_hex(&"z".repeat(CELLS * 2)), None);
    }

    #[test]
    fn clearing_hands_the_architecture_back_to_the_generator() {
        let mut model = ShrineModel::EMPTY;
        model.set(1, 1, 3, kind::WALL, 0);
        assert!(model.is_authored());
        model.clear();
        assert!(!model.is_authored());
    }

    #[test]
    fn reading_the_generator_produces_an_authored_model_that_matches_it() {
        let shrine = Shrine::generate(888);
        let model = ShrineModel::from_shrine(&shrine);
        assert!(model.is_authored(), "the generator always leaves real tiles");
        // Rebuilding from that model must give the same structure.
        let rebuilt = Shrine::from_model(888, &model);
        assert_eq!(rebuilt.hash(), shrine.hash(), "a model read from the generator must rebuild it");
    }
}
