//! Continuous wind: noise through a slow filter, with a whistle that rises
//! with each gust.
//!
//! This is a first-class instrument, not an effect: the same
//! [`crate::weather::WeatherState`] that sways the foliage drives it, gusts
//! included, so the trees move in time with what you hear.

use crate::filter::{OnePole, Svf};
use crate::math::pan_gains;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5749_4e44; // "WIND"
const SR: f64 = SAMPLE_RATE as f64;
/// How often the whistle's pitch is retuned, in samples.
const CONTROL: usize = 32;

/// A gusting wind voice.
#[derive(Clone)]
pub struct Wind {
    rng: Rng,
    lp: OnePole,
    bp: Svf,
    whistle: Svf,
    tick: usize,
}

impl Wind {
    pub fn new(seed: u64) -> Self {
        Wind {
            rng: Rng::stream(seed, STREAM),
            lp: OnePole::new(140.0, SR),
            bp: Svf::new(480.0, 0.8, SR),
            whistle: Svf::new(600.0, 9.0, SR),
            tick: 0,
        }
    }

    /// One stereo sample. `strength` is the wind (gusts included), `gust` the
    /// gust alone and `dir` the direction in turns; all in `[0, 1]`.
    pub fn next(&mut self, strength: f64, gust: f64, dir: f64) -> (f64, f64) {
        let (strength, gust) = (strength.clamp(0.0, 1.0), gust.clamp(0.0, 1.0));
        if self.tick % CONTROL == 0 {
            // The whistle climbs as the gust builds, as wind does round stone.
            self.whistle.set(520.0 + 1_100.0 * gust, 9.0, SR);
        }
        self.tick = self.tick.wrapping_add(1);
        let white = self.rng.bipolar();
        let low = self.lp.process(white);
        let body = self.bp.process(low).band;
        let whistle = self.whistle.process(white).band;
        let amp = strength * (0.9 * body + 0.35 * (0.4 + 0.6 * gust) * whistle);
        let pan = ((dir.clamp(0.0, 1.0) * 2.0 - 1.0) * 0.6).clamp(-1.0, 1.0);
        let (l, r) = pan_gains(pan);
        (amp * l, amp * r)
    }
}
