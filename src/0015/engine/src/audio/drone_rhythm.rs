//! A rhythmic amplitude LFO for the drone.
//!
//! Off by default, and when it is off nothing here is consulted, so the sound
//! and every golden hash are exactly as they were. When it is on, a seed of its
//! own decides the rhythm: a subdivision of the bar and an accent per pulse.
//! Manual shaping can layer on top of this later, but the seed is the starting
//! point.
//!
//! The level ducks at each pulse and recovers before the next one, which reads
//! as rhythm without pushing the mix into clipping.

use crate::math::cos_turns;
use crate::rng::Rng;

/// Stream id, so this generator never shares a sequence with another.
const STREAM: u64 = 0x4452_4859; // "DRHY"

/// How far the deepest pulse ducks, 0..1.
const DEPTH: f64 = 0.72;

pub struct DroneRhythm {
    /// Pulse length in samples.
    step: u64,
    /// Accent per pulse, 0..1.
    accents: Vec<f64>,
}

impl DroneRhythm {
    /// Build the rhythm the seed asks for. `bpm` sets the bar, so the pulses
    /// stay locked to the drums.
    pub fn new(seed: u64, bpm: u16) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        // Pulses per bar: whole, half, quarter or eighth notes.
        let steps = [4usize, 8, 8, 16][rng.below(4) as usize];
        let bar = (crate::SAMPLE_RATE as u64) * 4 * 60 / bpm.max(1) as u64;
        let step = (bar / steps as u64).max(1);
        let accents = (0..steps)
            .map(|i| {
                // Strong on the beat, weaker off it, then jittered by the seed.
                let weight = if i % 4 == 0 {
                    1.0
                } else if i % 2 == 0 {
                    0.6
                } else {
                    0.25
                };
                (weight * rng.range(0.6, 1.0)).clamp(0.0, 1.0)
            })
            .collect();
        DroneRhythm { step, accents }
    }

    /// Amplitude window at `clock`, around 1.0 with a duck at each pulse.
    ///
    /// Stateless, so it can be evaluated in any order and never drifts.
    pub fn level(&self, clock: u64) -> f64 {
        let steps = self.accents.len() as u64;
        if steps == 0 || self.step == 0 {
            return 1.0;
        }
        let accent = self.accents[((clock / self.step) % steps) as usize];
        let phase = (clock % self.step) as f64 / self.step as f64;
        // One-sided bump: full at the pulse, gone by the next one.
        let envelope = 0.5 + 0.5 * cos_turns(phase * 0.5);
        (1.0 - DEPTH * accent * envelope * envelope).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pulse_ducks_and_recovers() {
        let rhythm = DroneRhythm::new(7, 120);
        let step = rhythm.step;
        // Right at a pulse the window is at its lowest for that accent, and by
        // the end of the step it is back to full.
        let at_pulse = rhythm.level(0);
        let before_next = rhythm.level(step - 1);
        assert!(at_pulse < 0.5, "the first pulse should duck hard, got {at_pulse}");
        assert!(before_next > 0.95, "it should recover before the next pulse, got {before_next}");
        assert!(rhythm.level(0) <= 1.0 && rhythm.level(0) >= 0.0);
    }

    #[test]
    fn it_is_the_same_every_bar_and_the_seed_decides_it() {
        let a = DroneRhythm::new(7, 120);
        let b = DroneRhythm::new(7, 120);
        let other = DroneRhythm::new(8, 120);
        let bar = a.step * a.accents.len() as u64;
        assert_eq!(a.level(0), a.level(bar), "the rhythm repeats each bar");
        assert_eq!(a.level(1234), b.level(1234), "the same seed is the same rhythm");
        assert_ne!(a.level(0), other.level(0), "a different seed should differ");
    }

    #[test]
    fn it_stays_locked_to_the_tempo() {
        let slow = DroneRhythm::new(3, 60);
        let fast = DroneRhythm::new(3, 120);
        assert!(slow.step > fast.step, "a slower tempo gives longer pulses");
    }
}
