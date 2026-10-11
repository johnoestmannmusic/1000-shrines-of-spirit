//! The weather bed: wind and rain as sound.
//!
//! It samples the *same* [`Weather`] the visuals use, one sample at a time, so
//! the foliage sway, the rain streaks and this bed are all one system. It is
//! mixed on top of the ported 0011 music, which is left untouched.

use crate::audio::rain::Rain;
use crate::audio::wind::Wind;
use crate::weather::Weather;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;

/// The weather bed sits this far under the music (about -8 dB), so it reads as
/// background rather than a foreground instrument.
const BED_GAIN: f64 = 0.4;

/// Wind and rain, mixed into an existing interleaved stereo buffer.
#[derive(Clone)]
pub struct WeatherBed {
    wind: Wind,
    rain: Rain,
}

impl WeatherBed {
    pub fn new(seed: u64) -> Self {
        WeatherBed {
            wind: Wind::new(seed),
            rain: Rain::new(seed ^ 0x9E37_79B9_7F4A_7C15),
        }
    }

    /// Add the bed into `out`, starting at `t0` seconds. `out` must already hold
    /// the music mix; this only adds to it.
    pub fn render(&mut self, out: &mut [f32], t0: f64, weather: &Weather) {
        let frames = out.len() / 2;
        for i in 0..frames {
            let t = t0 + i as f64 / SR;
            let s = weather.state(t);
            let (wl, wr) = self.wind.next(s.wind_strength, s.wind_dir);
            let (rl, rr) = self.rain.next(s.rain);
            out[2 * i] += ((wl + rl) * BED_GAIN).clamp(-1.0, 1.0) as f32;
            out[2 * i + 1] += ((wr + rr) * BED_GAIN).clamp(-1.0, 1.0) as f32;
        }
    }
}
