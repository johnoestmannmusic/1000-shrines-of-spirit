//! Per-layer glitch controls: which kinds play, and how often.
//!
//! The two glitch layers used to be hard-coded by `layer1`/`layer2` in
//! `track.rs`. They are now derived from [`GlitchSetup`], whose defaults
//! reproduce those constants *exactly* — same values, same order, so `Layer`
//! draws the same random numbers and the coupled audio is unchanged. Only
//! moving a setting changes the sound (`HC004`).
//!
//! Two words carry each layer: [`word_a`] holds what plays (kinds, fill, rate,
//! steps, ratchet) and [`word_b`] holds levels and the mutation cadence.

use crate::harmony::Harmony;
use crate::pattern::LayerConfig;
use crate::tempo::Tempo;

/// Step-length multipliers, indexed by [`GlitchSetup::rate`]. A larger
/// multiplier means longer steps, so the layer fires less often.
pub const RATES: [u32; 4] = [1, 2, 4, 8];

pub const MIN_FILL: u8 = 0;
pub const MAX_FILL: u8 = 100;
pub const MIN_STEPS: u8 = 1;
pub const MAX_STEPS: u8 = 24;
pub const MIN_REPEATS: u8 = 1;
pub const MAX_REPEATS: u8 = 24;
pub const MIN_RATCHET: u8 = 1;
pub const MAX_RATCHET: u8 = 8;
pub const MIN_GAIN: u8 = 0;
pub const MAX_GAIN: u8 = 200;

/// The parts of a layer the setup scales rather than replaces: the per-kind
/// weights, the output range and the pitched span.
struct Fixture {
    /// Relative weight of each `crate::glitch::KINDS` entry when the kind is switched on.
    weights: [f64; 6],
    /// The layer's original gain range; [`GlitchSetup::gain`] scales it.
    gain: (f64, f64),
    pan_width: f64,
    /// Lowest and highest MIDI note, relative to the key.
    notes: (f64, f64),
}

const FIXTURES: [Fixture; 2] = [
    Fixture { weights: [1.0, 3.0, 2.0, 0.3, 2.0, 1.5], gain: (0.25, 0.6), pan_width: 0.9, notes: (84.0, 97.0) },
    Fixture { weights: [3.0, 0.5, 2.0, 2.5, 0.5, 0.5], gain: (0.18, 0.45), pan_width: 0.6, notes: (60.0, 81.0) },
];

/// One glitch layer's creative settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlitchSetup {
    /// When off, the layer keeps evolving but is muted.
    pub enabled: bool,
    /// One flag per entry of `crate::glitch::KINDS`, in that order.
    pub kinds: [bool; 6],
    /// Chance a step fires, percent: the layer's base "how often".
    pub fill: u8,
    /// Index into [`RATES`].
    pub rate: u8,
    /// Output level, percent of the layer's original gain.
    pub gain: u8,
    /// Pattern length, in steps.
    pub steps: u8,
    /// How many loops the pattern holds before it mutates.
    pub repeats_min: u8,
    pub repeats_max: u8,
    /// Retriggers within one step.
    pub ratchet: u8,
}

impl GlitchSetup {
    /// Layer 1 as it has always sounded.
    pub const DEFAULT_0: GlitchSetup = GlitchSetup {
        enabled: true,
        kinds: [true; 6],
        fill: 55,
        rate: 0,
        gain: 100,
        steps: 7,
        repeats_min: 4,
        repeats_max: 9,
        ratchet: 4,
    };

    /// Layer 2 as it has always sounded.
    pub const DEFAULT_1: GlitchSetup = GlitchSetup {
        enabled: true,
        kinds: [true; 6],
        fill: 45,
        rate: 1,
        gain: 100,
        steps: 11,
        repeats_min: 3,
        repeats_max: 7,
        ratchet: 3,
    };

    pub const DEFAULT: [GlitchSetup; 2] = [Self::DEFAULT_0, Self::DEFAULT_1];

    /// How many kinds are switched on.
    pub fn kinds_on(&self) -> usize {
        self.kinds.iter().filter(|k| **k).count()
    }

    /// The audio layer this setup describes. Every clamp here matches the one
    /// the setup menu applies, so what you hear is what you set.
    pub fn layer_config(self, which: usize, harmony: &Harmony, tempo: Tempo) -> LayerConfig {
        let fixture = &FIXTURES[which.min(FIXTURES.len() - 1)];
        let weights = core::array::from_fn(|i| if self.kinds[i] { fixture.weights[i] } else { 0.0 });
        let level = self.gain.min(MAX_GAIN) as f64 / 100.0;
        let (lo, hi) = (self.repeats_min.clamp(MIN_REPEATS, MAX_REPEATS), self.repeats_max.clamp(MIN_REPEATS, MAX_REPEATS));
        LayerConfig {
            steps: self.steps.clamp(MIN_STEPS, MAX_STEPS) as usize,
            step_len: tempo.glitch16() * RATES[(self.rate as usize).min(RATES.len() - 1)] as u64,
            weights,
            notes: if harmony.garage {
                // Over moving chords, only the notes that sit well on all of
                // them: the key's root, second and fifth.
                let iv = harmony.scale.intervals();
                let safe: Vec<u8> = [0usize, 1, 4].iter().filter_map(|d| iv.get(*d).copied()).collect();
                let notes: Vec<f64> = harmony
                    .scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
                    .into_iter()
                    .filter(|m| safe.contains(&((*m as i64 - harmony.key as i64).rem_euclid(12) as u8)))
                    .collect();
                if notes.is_empty() {
                    harmony.scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
                } else {
                    notes
                }
            } else {
                harmony.scale_notes(fixture.notes.0 + harmony.key as f64, fixture.notes.1)
            },
            fill: self.fill.min(MAX_FILL) as f64 / 100.0,
            gain: (fixture.gain.0 * level, fixture.gain.1 * level),
            pan_width: fixture.pan_width,
            repeats: (lo.min(hi) as u64, lo.max(hi) as u64),
            max_ratchet: self.ratchet.clamp(MIN_RATCHET, MAX_RATCHET) as u64,
            ping_ratio: if harmony.garage { 3.0 } else { 0.0 },
        }
    }

    /// What plays: enabled, kinds, fill, rate, steps, ratchet.
    pub fn word_a(self) -> u32 {
        let kinds = self.kinds.iter().enumerate().fold(0u32, |acc, (i, on)| acc | ((*on as u32) << i));
        (self.enabled as u32)
            | ((kinds & 0x3f) << 1)
            | ((self.fill.min(MAX_FILL) as u32) << 7)
            | (((self.rate as u32) & 0x7) << 14)
            | (((self.steps.clamp(MIN_STEPS, MAX_STEPS) - MIN_STEPS) as u32 & 0x1f) << 17)
            | (((self.ratchet.clamp(MIN_RATCHET, MAX_RATCHET) - MIN_RATCHET) as u32 & 0x7) << 22)
    }

    /// How loud, and how often the pattern mutates.
    pub fn word_b(self) -> u32 {
        (self.gain.min(MAX_GAIN) as u32)
            | (((self.repeats_min.clamp(MIN_REPEATS, MAX_REPEATS) - MIN_REPEATS) as u32 & 0x1f) << 8)
            | (((self.repeats_max.clamp(MIN_REPEATS, MAX_REPEATS) - MIN_REPEATS) as u32 & 0x1f) << 13)
    }

    /// Rebuild from a word pair, clamping anything corrupt.
    pub fn from_words(a: u32, b: u32) -> Self {
        let kinds = core::array::from_fn(|i| (a >> (1 + i)) & 1 != 0);
        GlitchSetup {
            enabled: a & 1 != 0,
            kinds,
            fill: ((a >> 7) & 0x7f).min(MAX_FILL as u32) as u8,
            rate: ((a >> 14) & 0x7).min((RATES.len() - 1) as u32) as u8,
            gain: (b & 0xff).min(MAX_GAIN as u32) as u8,
            steps: MIN_STEPS + ((a >> 17) & 0x1f).min((MAX_STEPS - MIN_STEPS) as u32) as u8,
            repeats_min: MIN_REPEATS + ((b >> 8) & 0x1f).min((MAX_REPEATS - MIN_REPEATS) as u32) as u8,
            repeats_max: MIN_REPEATS + ((b >> 13) & 0x1f).min((MAX_REPEATS - MIN_REPEATS) as u32) as u8,
            ratchet: MIN_RATCHET + ((a >> 22) & 0x7).min((MAX_RATCHET - MIN_RATCHET) as u32) as u8,
        }
    }

    /// One line for the recipe card and the menu.
    pub fn summary(self) -> String {
        if !self.enabled {
            return "off".to_string();
        }
        format!("{} kinds · {}% · ×{} · {} steps", self.kinds_on(), self.fill, RATES[(self.rate as usize).min(3)], self.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed harmony, so these tests only exercise the glitch setup.
    fn harmony() -> Harmony {
        Harmony::new(
            crate::harmony::Scale::Lydian,
            3,
            crate::harmony::Pace::HalfTime,
            10_000,
            0,
            &mut crate::rng::Rng::stream(1, 1),
            &mut crate::rng::Rng::stream(1, 2),
        )
    }

    #[test]
    fn the_defaults_reproduce_the_original_layers() {
        // These are the constants that used to live in `track.rs`. If any of
        // them drift, the coupled audio and every existing recording changes.
        let h = harmony();
        let t = Tempo::new(145);
        let l1 = GlitchSetup::DEFAULT_0.layer_config(0, &h, t);
        assert_eq!(l1.steps, 7);
        assert_eq!(l1.step_len, t.glitch16());
        assert_eq!(l1.weights, [1.0, 3.0, 2.0, 0.3, 2.0, 1.5]);
        assert_eq!(l1.fill, 0.55);
        assert_eq!(l1.gain, (0.25, 0.6));
        assert_eq!(l1.pan_width, 0.9);
        assert_eq!(l1.repeats, (4, 9));
        assert_eq!(l1.max_ratchet, 4);
        assert_eq!(l1.notes, h.scale_notes(84.0 + h.key as f64, 97.0));

        let l2 = GlitchSetup::DEFAULT_1.layer_config(1, &h, t);
        assert_eq!(l2.steps, 11);
        assert_eq!(l2.step_len, t.glitch16() * 2);
        assert_eq!(l2.weights, [3.0, 0.5, 2.0, 2.5, 0.5, 0.5]);
        assert_eq!(l2.fill, 0.45);
        assert_eq!(l2.gain, (0.18, 0.45));
        assert_eq!(l2.repeats, (3, 7));
        assert_eq!(l2.max_ratchet, 3);
    }

    #[test]
    fn turning_a_kind_off_only_zeroes_its_weight() {
        let h = harmony();
        let t = Tempo::new(145);
        let mut setup = GlitchSetup::DEFAULT_0;
        setup.kinds[2] = false;
        let cfg = setup.layer_config(0, &h, t);
        assert_eq!(cfg.weights[2], 0.0);
        assert_eq!(cfg.weights[0], 1.0, "other weights must not move");
        assert_eq!(cfg.notes, GlitchSetup::DEFAULT_0.layer_config(0, &h, t).notes);
    }

    #[test]
    fn the_words_round_trip_and_clamp() {
        for fx in [GlitchSetup::DEFAULT_0, GlitchSetup::DEFAULT_1] {
            assert_eq!(GlitchSetup::from_words(fx.word_a(), fx.word_b()), fx);
        }
        let wild = GlitchSetup {
            enabled: true,
            kinds: [true; 6],
            fill: 255,
            rate: 99,
            gain: 255,
            steps: 255,
            repeats_min: 255,
            repeats_max: 0,
            ratchet: 255,
        };
        let back = GlitchSetup::from_words(wild.word_a(), wild.word_b());
        assert_eq!(back.fill, MAX_FILL);
        assert_eq!(back.rate as usize, RATES.len() - 1);
        assert_eq!(back.gain, MAX_GAIN);
        assert_eq!(back.steps, MAX_STEPS);
        assert_eq!(back.repeats_min, MAX_REPEATS);
        assert_eq!(back.repeats_max, MIN_REPEATS);
        assert_eq!(back.ratchet, MAX_RATCHET);
        // Corruption must never panic.
        assert!(GlitchSetup::from_words(u32::MAX, u32::MAX).enabled);
    }

    #[test]
    fn a_repeats_range_is_never_inverted() {
        let h = harmony();
        let setup = GlitchSetup { repeats_min: 9, repeats_max: 2, ..GlitchSetup::DEFAULT_0 };
        let cfg = setup.layer_config(0, &h, Tempo::new(145));
        assert_eq!(cfg.repeats, (2, 9), "min/max must be ordered");
    }
}
