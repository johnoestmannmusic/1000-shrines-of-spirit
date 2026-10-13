//! Continuous rain: broadband hiss with sparse droplets, denser as the weather
//! intensifies. Driven by the same weather state as the rain streaks on screen.

use crate::filter::Svf;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5241_494e; // "RAIN"
const SR: f64 = SAMPLE_RATE as f64;

/// A rain voice: two decorrelated hiss bands plus decaying droplets.
#[derive(Clone)]
pub struct Rain {
    rng: Rng,
    hiss_l: Svf,
    hiss_r: Svf,
    drop: f64,
}

impl Rain {
    pub fn new(seed: u64) -> Self {
        Rain {
            rng: Rng::stream(seed, STREAM),
            hiss_l: Svf::new(4800.0, 0.5, SR),
            hiss_r: Svf::new(5200.0, 0.5, SR),
            drop: 0.0,
        }
    }

    /// One stereo sample for a rain density in `[0, 1]`.
    pub fn next(&mut self, rain: f64) -> (f64, f64) {
        let rain = rain.clamp(0.0, 1.0);
        let hiss = (self.hiss_l.process(self.rng.bipolar()).band + self.hiss_r.process(self.rng.bipolar()).band) * 0.5;
        // Sparse droplets, more likely the heavier the rain.
        if rain > 0.02 && self.rng.chance(0.0015 * rain) {
            self.drop = self.rng.range(0.3, 1.0);
        }
        self.drop *= 0.982;
        let out = hiss * (0.10 + 0.45 * rain) + self.drop * 0.30;
        (out, out * 0.85)
    }
}
