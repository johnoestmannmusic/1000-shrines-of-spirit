//! Continuous wind: noise through a slow filter, gusting with the weather.
//!
//! This is a first-class instrument, not an effect: the same
//! [`crate::weather::Weather`] state that sways the foliage also drives this,
//! so the trees move in time with what you hear.

use crate::filter::{OnePole, Svf};
use crate::math::{pan_gains, tanh};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5749_4e44; // "WIND"
const SR: f64 = SAMPLE_RATE as f64;

/// A gusting wind voice.
#[derive(Clone)]
pub struct Wind {
    rng: Rng,
    lp: OnePole,
    bp: Svf,
    drift: f64,
}

impl Wind {
    pub fn new(seed: u64) -> Self {
        Wind {
            rng: Rng::stream(seed, STREAM),
            lp: OnePole::new(140.0, SR),
            bp: Svf::new(480.0, 0.8, SR),
            drift: 0.0,
        }
    }

    /// One stereo sample for a wind strength and direction, both in `[0, 1]`.
    pub fn next(&mut self, strength: f64, dir: f64) -> (f64, f64) {
        let white = self.rng.bipolar();
        let low = self.lp.process(white);
        let band = self.bp.process(low).band;
        // A slow random walk adds gust texture on top of the weather's cycle.
        self.drift = (self.drift * 0.9995 + white * 0.0015).clamp(-1.0, 1.0);
        let gust = (0.35 + 0.65 * strength.clamp(0.0, 1.0)) * (0.75 + 0.25 * tanh(self.drift * 3.0));
        let amp = band * gust * 0.6;
        let pan = ((dir.clamp(0.0, 1.0) * 2.0 - 1.0) * 0.6).clamp(-1.0, 1.0);
        let (l, r) = pan_gains(pan);
        (amp * l, amp * r)
    }
}
