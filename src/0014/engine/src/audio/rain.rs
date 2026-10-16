//! The rain's wash: the broadband hiss of a field of drops too many to hear
//! one by one, with a soft low body under it. Its level *is* the rain level the
//! scene draws its streaks from, with no floor: no rain, no hiss.
//!
//! The drops you can hear individually — taps in the moss, plinks on stone —
//! are the splash events, played by [`crate::ambience`] from the same schedule
//! the scene draws its splashes from.

use crate::filter::Svf;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x5241_494e; // "RAIN"
const SR: f64 = SAMPLE_RATE as f64;

/// The wash: decorrelated hiss and body in each channel.
#[derive(Clone)]
pub struct Rain {
    rng: Rng,
    hiss: [Svf; 2],
    body: [Svf; 2],
}

impl Rain {
    pub fn new(seed: u64) -> Self {
        Rain {
            rng: Rng::stream(seed, STREAM),
            hiss: [Svf::new(4_800.0, 0.6, SR), Svf::new(5_400.0, 0.6, SR)],
            body: [Svf::new(700.0, 0.7, SR), Svf::new(760.0, 0.7, SR)],
        }
    }

    /// One stereo sample for a rain level in `[0, 1]`.
    pub fn next(&mut self, rain: f64) -> (f64, f64) {
        let rain = rain.clamp(0.0, 1.0);
        let mut out = [0.0; 2];
        for (c, o) in out.iter_mut().enumerate() {
            let x = self.rng.bipolar();
            // Heavier rain is also darker: more body under the hiss.
            *o = rain * (0.55 * self.hiss[c].process(x).band + (0.25 + 0.35 * rain) * self.body[c].process(x).low);
        }
        (out[0], out[1])
    }
}
