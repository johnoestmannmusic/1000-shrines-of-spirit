//! The authored architecture: an optional hand-edited 16x16 model.
//!
//! A cell's height runs from **-6 to +6**. Positive stacks tiles up from the
//! ground; negative digs a pit the same distance *below* it; zero leaves the
//! ground plane alone. Because a cell can legitimately be all zeros, "is this
//! model authored?" is an explicit flag rather than an all-zero sentinel.
//!
//! When `authored` is false the seed's generator stays authoritative, so the
//! default recipe carries no architecture of its own and today's output is
//! untouched (`HC004`).
//!
//! Each cell is nine bits: a signed height biased by +8, a three-bit tile kind
//! and two bits of cosmetic variation. Three cells fit in a recipe word, so the
//! model is one flag word plus 86 cell words — which is what keeps the artifact
//! self-contained (`G001`).

use crate::shrine::{kind, Shrine, MAX_HEIGHT};
use crate::{SHRINE_H, SHRINE_W};

/// Cells in the footprint.
pub const CELLS: usize = (SHRINE_W * SHRINE_H) as usize;
/// Cells packed per recipe word.
pub const PER_WORD: usize = 3;
/// Packed words holding the cells.
pub const PACKED: usize = CELLS.div_ceil(PER_WORD);
/// Recipe words the model occupies: one flag, then the packed cells.
pub const WORDS: usize = PACKED + 1;

/// A shrine's architecture, authored by hand or still procedural.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShrineModel {
    authored: bool,
    /// Nine bits per cell, three cells per word. Kept packed rather than as a
    /// byte array because nine bits do not fit in a byte.
    packed: [u32; PACKED],
}

impl ShrineModel {
    /// Deepest pit, in levels.
    pub const MIN_HEIGHT: i8 = -6;
    /// Tallest column, in levels.
    pub const MAX: i8 = crate::shrine::MAX_HEIGHT as i8;

    /// Not authored: the generator decides.
    pub const EMPTY: ShrineModel = ShrineModel { authored: false, packed: [0; PACKED] };

    /// True once the editor has taken over.
    pub fn is_authored(&self) -> bool {
        self.authored
    }

    /// Forget every edit and hand the architecture back to the generator.
    pub fn clear(&mut self) {
        self.authored = false;
        self.packed = [0; PACKED];
    }

    /// Empty the plan: an authored model in which every square is a hole.
    pub fn blank(&mut self) {
        self.authored = true;
        self.packed = [0; PACKED];
    }

    fn index(x: usize, y: usize) -> usize {
        (y % SHRINE_H as usize) * SHRINE_W as usize + (x % SHRINE_W as usize)
    }

    /// The nine encoded bits of one cell.
    fn raw(&self, i: usize) -> u16 {
        ((self.packed[i / PER_WORD] >> (9 * (i % PER_WORD))) & 0x1ff) as u16
    }

    /// Overwrite one cell's nine bits.
    fn set_raw(&mut self, i: usize, value: u16) {
        let shift = 9 * (i % PER_WORD);
        let word = &mut self.packed[i / PER_WORD];
        *word = (*word & !(0x1ff << shift)) | ((value as u32 & 0x1ff) << shift);
    }

    /// Height of a cell: positive builds up, negative digs down, 0 is ground.
    pub fn height(&self, x: usize, y: usize) -> i8 {
        let raw = self.raw(Self::index(x, y));
        if raw == 0 {
            // Never written, so this is plain ground rather than a pit.
            return 0;
        }
        decode_height(raw as u8)
    }

    /// Tile kind of a cell's top, see [`kind`].
    pub fn kind(&self, x: usize, y: usize) -> u8 {
        ((self.raw(Self::index(x, y)) >> 4) & 0x7) as u8
    }

    /// Cosmetic variation, 0..=3.
    pub fn variant(&self, x: usize, y: usize) -> u8 {
        ((self.raw(Self::index(x, y)) >> 7) & 0x3) as u8
    }

    /// Write one cell. Heights and kinds are clamped, so the model is always
    /// valid however it is driven.
    pub fn set(&mut self, x: usize, y: usize, height: i8, kind: u8, variant: u8) {
        self.authored = true;
        let value = Self::encode(height, kind, variant);
        let i = Self::index(x, y);
        self.set_raw(i, value);
    }

    /// Nine bits: biased height, kind, variation.
    fn encode(height: i8, kind: u8, variant: u8) -> u16 {
        let h = height.clamp(Self::MIN_HEIGHT, Self::MAX);
        encode_height(h) as u16 | ((kind as u16 & 0x7) << 4) | ((variant as u16 & 0x3) << 7)
    }

    /// The height, kind and variation packed into one cell.
    fn decode(value: u16) -> (i8, u8, u8) {
        let h = decode_height((value & 0xf) as u8).clamp(Self::MIN_HEIGHT, Self::MAX);
        ((h), ((value >> 4) & 0x7) as u8, ((value >> 7) & 0x3) as u8)
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
                model.set(x, y, h as i8, tile.kind.max(kind::FLOOR), tile.variant);
            }
        }
        model
    }

    /// The model as recipe words: the authored flag, then the packed cells.
    pub fn words(&self) -> [u32; WORDS] {
        let mut out = [0u32; WORDS];
        out[0] = self.authored as u32;
        out[1..].copy_from_slice(&self.packed);
        out
    }

    /// Rebuild from recipe words. Short input leaves the model procedural.
    pub fn from_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let Some((&flag, packed)) = words.split_first() else {
            return model;
        };
        model.authored = flag & 1 != 0;
        for (i, w) in packed.iter().take(PACKED).enumerate() {
            model.packed[i] = *w;
        }
        model.sanitise();
        model
    }

    /// Rebuild from the version-13 layout: one flag word, then 8-bit cells
    /// holding height 0..6, kind and variation.
    pub fn from_v13_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let Some((&flag, packed)) = words.split_first() else {
            return model;
        };
        model.authored = flag & 1 != 0;
        for (i, w) in packed.iter().take(CELLS / 4).enumerate() {
            for (j, b) in w.to_le_bytes().iter().enumerate() {
                // The old layout: height 0..6, kind, variation, in one byte.
                model.set_raw(i * 4 + j, Self::encode((*b & 0x7) as i8, (*b >> 3) & 0x7, (*b >> 6) & 0x3));
            }
        }
        model
    }

    /// Rebuild from the version-11/12 layout: 8-bit cells with the height
    /// stored minus one, and a non-empty model meaning "authored".
    pub fn from_legacy_words(words: &[u32]) -> Self {
        let mut model = ShrineModel::EMPTY;
        let mut any = false;
        for (i, w) in words.iter().take(CELLS / 4).enumerate() {
            for (j, b) in w.to_le_bytes().iter().enumerate() {
                any |= *b != 0;
                // The old layout stored height - 1, so every old cell is one higher.
                let h = ((*b & 0x7) + 1).min(MAX_HEIGHT) as i8;
                model.set_raw(i * 4 + j, Self::encode(h, (*b >> 3) & 0x7, (*b >> 6) & 0x3));
            }
        }
        model.authored = any;
        model
    }

    /// Clamp anything out of range back into the model's own limits.
    fn sanitise(&mut self) {
        for i in 0..CELLS {
            // A zero cell is one the editor has never written: it is ground, and
            // must stay exactly as it is or every save would invent territory.
            if self.raw(i) == 0 {
                continue;
            }
            let (h, k, v) = Self::decode(self.raw(i));
            self.set_raw(i, Self::encode(h, k.min(kind::STONE), v));
        }
    }

    /// The packed words as lowercase hex, for the seed file.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(WORDS * 8);
        for w in self.words() {
            out.push_str(&format!("{w:08x}"));
        }
        out
    }

    /// Read [`to_hex`] back. Returns `None` unless the text is exactly the
    /// expected length of hex digits.
    pub fn from_hex(text: &str) -> Option<ShrineModel> {
        let t = text.trim();
        if t.len() != WORDS * 8 || !t.bytes().all(|b| (b as char).is_ascii_hexdigit()) {
            return None;
        }
        let mut words = [0u32; WORDS];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32::from_str_radix(&t[i * 8..i * 8 + 8], 16).ok()?;
        }
        Some(ShrineModel::from_words(&words))
    }

    /// A stable hash, for tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &w in self.words().iter() {
            for b in w.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        h ^ (self.authored as u64)
    }
}

/// Height bias: stored value = height + 8, so -6..=6 lands in 2..=14.
fn encode_height(height: i8) -> u8 {
    ((height.clamp(ShrineModel::MIN_HEIGHT, ShrineModel::MAX) + 8) as u8) & 0xf
}

fn decode_height(byte: u8) -> i8 {
    (byte & 0xf) as i8 - 8
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
        assert_eq!(ShrineModel::EMPTY.height(4, 4), 0);
    }

    #[test]
    fn a_cell_can_be_a_pit() {
        let mut model = ShrineModel::EMPTY;
        model.set(4, 4, -3, kind::FLOOR, 0);
        assert!(model.is_authored());
        assert_eq!(model.height(4, 4), -3, "a negative height digs down");
        // And it survives the round trip, sign and all.
        assert_eq!(ShrineModel::from_words(&model.words()), model);
    }

    #[test]
    fn the_whole_range_round_trips() {
        let mut model = ShrineModel::EMPTY;
        for h in ShrineModel::MIN_HEIGHT..=ShrineModel::MAX {
            let x = (h - ShrineModel::MIN_HEIGHT) as usize;
            model.set(x, 0, h, kind::WALL, (x % 4) as u8);
            assert_eq!(model.height(x, 0), h, "height {h}");
        }
        assert_eq!(ShrineModel::from_words(&model.words()), model);
        // Clamping keeps out-of-range input inside the model.
        model.set(0, 0, -99, kind::WALL, 0);
        assert_eq!(model.height(0, 0), ShrineModel::MIN_HEIGHT);
        model.set(0, 0, 99, kind::WALL, 0);
        assert_eq!(model.height(0, 0), ShrineModel::MAX);
    }

    #[test]
    fn wild_words_clamp_and_short_input_stays_procedural() {
        let wild = vec![u32::MAX; WORDS];
        let fixed = ShrineModel::from_words(&wild);
        for y in 0..SHRINE_H as usize {
            for x in 0..SHRINE_W as usize {
                assert!((ShrineModel::MIN_HEIGHT..=ShrineModel::MAX).contains(&fixed.height(x, y)));
                assert!(fixed.kind(x, y) <= kind::STONE);
                assert!(fixed.variant(x, y) <= 3);
            }
        }
        assert!(!ShrineModel::from_words(&[]).is_authored());
    }

    #[test]
    fn a_model_round_trips_through_hex() {
        let mut model = ShrineModel::EMPTY;
        model.set(0, 0, 1, kind::FLOOR, 0);
        model.set(7, 9, 6, kind::ALTAR, 3);
        model.set(2, 2, -4, kind::FLOOR, 0);
        let text = model.to_hex();
        assert_eq!(text.len(), WORDS * 8);
        assert_eq!(ShrineModel::from_hex(&text), Some(model));
        assert_eq!(ShrineModel::from_hex("nope"), None);
        assert_eq!(ShrineModel::from_hex(&"z".repeat(WORDS * 8)), None);
    }

    #[test]
    fn earlier_layouts_still_load() {
        // Version 13: flag word, then 8-bit cells holding height 0..6.
        let mut v13 = vec![1u32, 0, 0, 0];
        // Cell 0: height 3, kind WALL.
        v13[1] = 3 | ((kind::WALL as u32) << 3);
        let model = ShrineModel::from_v13_words(&v13);
        assert!(model.is_authored());
        assert_eq!(model.height(0, 0), 3);
        assert_eq!(model.kind(0, 0), kind::WALL);

        // Versions 11 and 12 stored height - 1 and had no flag.
        let mut old = vec![0u32; CELLS / 4];
        old[0] = 4 | ((kind::PILLAR as u32) << 3);
        let model = ShrineModel::from_legacy_words(&old);
        assert!(model.is_authored());
        assert_eq!(model.height(0, 0), 5, "the old layout held height - 1");
        assert_eq!(model.kind(0, 0), kind::PILLAR);
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
        let rebuilt = Shrine::from_model(888, &model);
        assert_eq!(rebuilt.hash(), shrine.hash(), "a model read from the generator must rebuild it");
    }
}
