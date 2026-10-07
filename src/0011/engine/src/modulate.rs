//! Very slow cyclic modulation that shapes the endless form.
//!
//! Every LFO's phase is computed directly from the integer sample clock
//! (`clock % period`), so there is no accumulated drift however long the
//! piece plays, and the value at any moment is independent of block size.
//! The periods are mutually prime numbers of seconds, so the combined
//! pattern only repeats after billions of years.

use crate::math::sin_turns;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// The LFO periods, in seconds (all prime).
pub const PERIODS: [u64; 7] = [41, 61, 97, 151, 233, 307, 113];

pub struct Lfo {
    period: u64,
    offset: u64,
}

impl Lfo {
    fn new(seconds: u64, rng: &mut Rng) -> Self {
        let period = seconds * SAMPLE_RATE as u64;
        Lfo { period, offset: rng.next_u64() % period }
    }

    /// Value in [-1, 1].
    pub fn at(&self, clock: u64) -> f64 {
        let p = (clock.wrapping_add(self.offset)) % self.period;
        sin_turns(p as f64 / self.period as f64)
    }
}

/// The modulation sources, sampled at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModState {
    /// Drone filter openness, 0..1.
    pub drone_open: f64,
    /// Drone filter resonance, 0..1.
    pub drone_reso: f64,
    /// Glitch layer densities, 0..1.
    pub density1: f64,
    pub density2: f64,
    /// Bass brightness, 0..1.
    pub bass_bright: f64,
    /// Bass swell, 0..1.
    pub bass_swell: f64,
    /// Drum energy, 0..1: picks each phrase's arrangement section.
    pub drums: f64,
    /// Atmosphere loop level, 0.7..1: a gentle drift on top of each loop.
    pub loop_level: f64,
}

pub struct Mods {
    l41: Lfo,
    l61: Lfo,
    l97: Lfo,
    l151: Lfo,
    l233: Lfo,
    l307: Lfo,
    l113: Lfo,
}

impl Mods {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, 0x40D);
        Mods {
            l41: Lfo::new(41, &mut rng),
            l61: Lfo::new(61, &mut rng),
            l97: Lfo::new(97, &mut rng),
            l151: Lfo::new(151, &mut rng),
            l233: Lfo::new(233, &mut rng),
            l307: Lfo::new(307, &mut rng),
            l113: Lfo::new(113, &mut rng),
        }
    }

    /// Every LFO's (period in seconds, value) at `clock`, in `PERIODS` order.
    pub fn lfos(&self, clock: u64) -> [(u64, f64); 7] {
        let all = [&self.l41, &self.l61, &self.l97, &self.l151, &self.l233, &self.l307, &self.l113];
        let mut out = [(0, 0.0); 7];
        for ((o, lfo), secs) in out.iter_mut().zip(all).zip(PERIODS) {
            *o = (secs, lfo.at(clock));
        }
        out
    }

    pub fn at(&self, clock: u64) -> ModState {
        let (l41, l61, l97) = (self.l41.at(clock), self.l61.at(clock), self.l97.at(clock));
        let (l151, l233, l307) = (self.l151.at(clock), self.l233.at(clock), self.l307.at(clock));
        let l113 = self.l113.at(clock);
        let unit = |x: f64| x.clamp(0.0, 1.0);
        ModState {
            drone_open: unit(0.5 + 0.3 * l61 + 0.2 * l233),
            drone_reso: unit(0.35 + 0.25 * l97 * l61 + 0.2 * l151),
            density1: unit(0.6 + 0.3 * l97 + 0.25 * l307),
            density2: unit(0.55 + 0.3 * l151 + 0.25 * l41 * l233),
            bass_bright: unit(0.4 + 0.35 * l113 + 0.2 * l41),
            bass_swell: unit(0.7 + 0.3 * l307 * l233),
            drums: unit(0.5 + 0.34 * l233 + 0.25 * l97 * l41),
            loop_level: 0.85 + 0.15 * l113 * l61,
        }
    }
}
