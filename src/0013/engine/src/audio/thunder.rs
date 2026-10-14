//! Thunder: one roll per lightning strike, arriving after the flash.
//!
//! The strike's distance (from the shared schedule in [`crate::weather`])
//! sets everything: how long after the flash the thunder arrives, whether it
//! opens with a crack, how dark the rumble is and how long it rolls on.

use crate::filter::Svf;
use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// The most rolls that can overlap.
const MAX_ROLLS: usize = 4;

#[derive(Clone)]
struct Roll {
    /// Samples until it starts.
    wait: usize,
    age: usize,
    rng: Rng,
    rumble: [Svf; 2],
    crack: Svf,
    crack_env: f64,
    crack_coef: f64,
    gain: f64,
    attack: f64,
    decay: f64,
    /// Slow swells in the roll, as echoes come back off far hills.
    rolls: [(f64, f64); 3],
    pan: (f64, f64),
}

impl Roll {
    fn next(&mut self) -> (f64, f64) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0);
        }
        let s = self.age as f64 / SR;
        self.age += 1;
        let env = (s / self.attack).min(1.0) * crate::math::exp(-s / self.decay);
        let mut swell = 0.0;
        for (hz, at) in self.rolls {
            swell += 0.5 + 0.5 * sin_turns(s * hz + at);
        }
        let swell = 0.4 + 0.6 * swell / 3.0;
        let x = self.rng.bipolar();
        let dark = self.rumble[0].process(x).low;
        let rumble = self.rumble[1].process(dark).low * 3.0;
        let crack = self.crack.process(x).band * self.crack_env;
        self.crack_env *= self.crack_coef;
        let y = self.gain * (rumble * env * swell + crack);
        (y * self.pan.0, y * self.pan.1)
    }

    fn done(&self) -> bool {
        self.wait == 0 && self.age as f64 / SR > 1.0 + 6.0 * self.decay
    }
}

/// All the thunder currently rolling.
#[derive(Clone, Default)]
pub struct Thunder {
    rolls: Vec<Roll>,
}

impl Thunder {
    /// A strike's thunder, starting `wait` samples from now. `strength` in
    /// `[0.4, 1]`, `distance` in `[0, 1]`, `pan` in `[-1, 1]`, `hash` its seed.
    pub fn add(&mut self, wait: usize, strength: f64, distance: f64, pan: f64, hash: u64) {
        if self.rolls.len() >= MAX_ROLLS {
            self.rolls.remove(0);
        }
        let mut rng = Rng::new(hash ^ 0x5448_554e);
        let cutoff = 90.0 + 160.0 * (1.0 - distance);
        let close = (1.0 - 2.0 * distance).max(0.0);
        let (l, r) = pan_gains(pan * 0.7);
        let rolls = [(rng.range(0.4, 0.9), rng.unit()), (rng.range(1.1, 2.0), rng.unit()), (rng.range(2.3, 3.4), rng.unit())];
        self.rolls.push(Roll {
            wait,
            age: 0,
            rumble: [Svf::new(cutoff, 0.7, SR), Svf::new(cutoff * 1.4, 0.6, SR)],
            crack: Svf::new(2_400.0, 0.7, SR),
            crack_env: 1.6 * close,
            crack_coef: decay_coef(0.05, SR),
            gain: strength * (1.0 - 0.55 * distance),
            attack: 0.04 + 0.25 * distance,
            decay: 1.2 + 2.6 * distance,
            rolls,
            pan: (l, r),
            rng,
        });
    }

    pub fn next(&mut self) -> (f64, f64) {
        let (mut l, mut r) = (0.0, 0.0);
        for roll in self.rolls.iter_mut() {
            let (a, b) = roll.next();
            l += a;
            r += b;
        }
        self.rolls.retain(|roll| !roll.done());
        (l, r)
    }
}
