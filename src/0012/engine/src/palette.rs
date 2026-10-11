//! Procedural palettes.
//!
//! A [`Palette`] is a small set of named roles — sky, fog, ground, stone,
//! foliage, water, accent — derived deterministically from a seed in a muted,
//! overgrown mood. Colours use plain arithmetic only (no library trig), so a
//! seed yields the same palette on every machine (`HC004`).

use crate::framebuffer::{rgb, Rgba};
use crate::rng::Rng;

/// Stream id, so palette draws never collide with another generator's stream.
const STREAM: u64 = 0x5041_4c45; // "PALE"

/// The named colours the scene draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub sky_top: Rgba,
    pub sky_bottom: Rgba,
    pub fog: Rgba,
    pub ground: Rgba,
    pub ground_dark: Rgba,
    pub stone: Rgba,
    pub stone_light: Rgba,
    pub stone_dark: Rgba,
    pub foliage_dark: Rgba,
    pub foliage: Rgba,
    pub foliage_light: Rgba,
    pub water: Rgba,
    pub accent: Rgba,
}

/// `x` wrapped into [0, 1).
pub fn wrap01(x: f64) -> f64 {
    x - x.floor()
}

/// HSL → RGB. `h` is in turns, `s` and `l` in [0, 1].
pub fn hsl(h: f64, s: f64, l: f64) -> Rgba {
    let h = wrap01(h);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let chan = |v: f64| ((v + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    rgb(chan(r), chan(g), chan(b))
}

impl Palette {
    /// Build a palette from an already-positioned RNG stream.
    pub fn from_rng(rng: &mut Rng) -> Self {
        let base = rng.range(0.0, 1.0);
        let sat = rng.range(0.20, 0.42);

        // Sky and fog stay low and desaturated; ground and stone are earthy;
        // foliage is the one green-shifted family; the accent is a soft
        // complementary glow used for the shrine's sigil.
        let foliage_h = wrap01(base + rng.range(0.24, 0.34));
        let stone_l = rng.range(0.34, 0.46);
        let ground_l = rng.range(0.20, 0.30);

        Palette {
            sky_top: hsl(base, sat * 0.55, rng.range(0.07, 0.12)),
            sky_bottom: hsl(wrap01(base + 0.02), sat * 0.65, rng.range(0.16, 0.24)),
            fog: hsl(wrap01(base + 0.01), sat * 0.30, rng.range(0.40, 0.52)),
            ground: hsl(wrap01(base + 0.07), sat * 0.55, ground_l),
            ground_dark: hsl(wrap01(base + 0.07), sat * 0.55, ground_l * 0.55),
            stone: hsl(wrap01(base - 0.05), sat * 0.28, stone_l),
            stone_light: hsl(wrap01(base - 0.05), sat * 0.28, (stone_l + 0.16).min(0.86)),
            stone_dark: hsl(wrap01(base - 0.05), sat * 0.28, (stone_l - 0.16).max(0.08)),
            foliage_dark: hsl(foliage_h, (sat * 1.1).min(0.60), 0.16),
            foliage: hsl(foliage_h, (sat * 1.1).min(0.60), 0.28),
            foliage_light: hsl(wrap01(foliage_h + 0.02), (sat * 1.1).min(0.60), 0.42),
            water: hsl(wrap01(base + 0.50), sat * 0.50, 0.34),
            accent: hsl(wrap01(base + 0.50), (sat * 1.5).min(0.80), rng.range(0.55, 0.66)),
        }
    }

    /// Build a palette for a seed.
    pub fn from_seed(seed: u64) -> Self {
        Palette::from_rng(&mut Rng::stream(seed, STREAM))
    }

    /// Perceived luminance in [0, 255]; used for contrast guarantees.
    pub fn luminance(c: Rgba) -> f64 {
        0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64
    }
}
