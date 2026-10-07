//! The tempo: one BPM setting for the drums, with every ambient layer
//! (glitch grid, chord bars, echoes) locked at exactly half of it.
//!
//! All timing is whole samples, so it can never drift. A drum sixteenth is
//! rounded to a multiple of 12 samples, which keeps 2-, 3- and 4-way glitch
//! ratchets and the drums' half-step rolls exact; the cost is a tempo at
//! most about 0.1 % away from the number asked for. At 168 BPM a drum
//! sixteenth is 4,284 samples: exactly 0010's timing.

use crate::SAMPLE_RATE;

pub const MIN_BPM: u16 = 70;
pub const MAX_BPM: u16 = 180;
pub const DEFAULT_BPM: u16 = 168;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tempo {
    /// The drum tempo asked for.
    pub bpm: u16,
    /// A drum sixteenth, in samples (a multiple of 12).
    pub drum16: u64,
}

impl Tempo {
    pub fn new(bpm: u16) -> Self {
        let bpm = bpm.clamp(MIN_BPM, MAX_BPM) as u64;
        // round(60·SR / (bpm·4) / 12) · 12, in integers.
        let per_minute = 60 * SAMPLE_RATE as u64;
        let n = (per_minute + 24 * bpm) / (48 * bpm);
        Tempo { bpm: bpm as u16, drum16: 12 * n }
    }

    /// A glitch-grid sixteenth: half-time, so twice a drum sixteenth.
    pub fn glitch16(self) -> u64 {
        2 * self.drum16
    }

    /// One half-time beat (four glitch sixteenths).
    pub fn beat(self) -> u64 {
        4 * self.glitch16()
    }

    /// One drum bar (16 drum sixteenths, = two half-time beats).
    pub fn drum_bar(self) -> u64 {
        16 * self.drum16
    }

    /// The drum tempo actually played, after rounding to whole samples.
    pub fn exact_bpm(self) -> f64 {
        60.0 * SAMPLE_RATE as f64 / (4.0 * self.drum16 as f64)
    }

    /// The half-time tempo the ambient layers move at.
    pub fn half_bpm(self) -> f64 {
        self.exact_bpm() / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_0010_timing() {
        let t = Tempo::new(DEFAULT_BPM);
        assert_eq!(t.drum16, 4284);
        assert_eq!(t.glitch16(), 8568);
        assert_eq!(t.beat(), 4 * 8568);
    }

    #[test]
    fn every_tempo_is_close_and_divisible() {
        for bpm in MIN_BPM..=MAX_BPM {
            let t = Tempo::new(bpm);
            assert_eq!(t.drum16 % 12, 0);
            assert!((t.exact_bpm() / bpm as f64 - 1.0).abs() < 0.002, "{bpm}: {}", t.exact_bpm());
        }
    }
}
