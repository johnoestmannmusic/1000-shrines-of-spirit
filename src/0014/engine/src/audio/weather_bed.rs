//! The weather bed: the weather as sound.
//!
//! It plays from the *same* [`Weather`] the visuals use. The continuous layers
//! (wind, the rain's wash, snow hush, fog air, rustle, crackle) follow the
//! shared [`crate::weather::WeatherState`] levels the scene draws its particles
//! from, and every event the scene draws — a splash, a drip, an ember's spark,
//! a lightning strike — is found from the same schedule and heard on the same
//! sample clock (thunder after its flash, by the strike's distance). It is
//! mixed on top of the ported 0011 music, which is left untouched.

use crate::audio::ambience::{Ambience, Hit};
use crate::audio::creatures::Creatures;
use crate::audio::rain::Rain;
use crate::audio::thunder::Thunder;
use crate::audio::wind::Wind;
use crate::filter::OnePole;
use crate::weather::{EventKind, Weather, WeatherState};
use crate::weather_fx::DRIP_S;
use crate::wildlife::Wildlife;
use crate::{GRID_H, GRID_W, SAMPLE_RATE};

const SR: f64 = SAMPLE_RATE as f64;
/// The weather state is sampled this often (samples); it moves over seconds.
const CONTROL: usize = 32;

/// The weather bed sits this far under the music (about -8 dB), so it reads as
/// background rather than a foreground instrument.
const BED_GAIN: f64 = 0.4;

/// Wind, rain, thunder and the small sounds, mixed into an existing
/// interleaved stereo buffer.
#[derive(Clone)]
pub struct WeatherBed {
    wind: Wind,
    rain: Rain,
    thunder: Thunder,
    ambience: Ambience,
    /// The creatures, when the Wildlife setting is on.
    wildlife: Option<Wildlife>,
    creatures: Creatures,
    /// Which grid cells hold a column (a splash there lands on stone).
    hard: Vec<bool>,
    /// Snow muffles everything in the bed.
    muffle: [OnePole; 2],
    /// Under a roof: the weather is heard through glass, every splash lands
    /// on a pane, and the wind is a muffled pressure outside.
    roofed: bool,
    state: Option<WeatherState>,
    tick: usize,
}

impl WeatherBed {
    /// `hard` is one flag per grid cell, row-major: does a column stand there?
    /// `wildlife` is `Some` when the Wildlife setting is on.
    pub fn new(seed: u64, hard: Vec<bool>, wildlife: Option<Wildlife>) -> Self {
        WeatherBed {
            wind: Wind::new(seed),
            rain: Rain::new(seed ^ 0x9E37_79B9_7F4A_7C15),
            thunder: Thunder::default(),
            ambience: Ambience::new(seed),
            wildlife,
            creatures: Creatures::default(),
            hard,
            muffle: [OnePole::new(9_000.0, SR), OnePole::new(9_000.0, SR)],
            roofed: false,
            state: None,
            tick: 0,
        }
    }

    /// Heard from under a roof (the indoor futures).
    pub fn roofed(mut self, on: bool) -> Self {
        self.roofed = on;
        self
    }

    fn is_hard(&self, x: f64, y: f64) -> bool {
        if self.roofed {
            // Every drop lands on the glass.
            return true;
        }
        let (gx, gy) = (x.round() as i64, y.round() as i64);
        if gx < 0 || gy < 0 || gx >= GRID_W as i64 || gy >= GRID_H as i64 {
            return false;
        }
        self.hard.get((gy * GRID_W as i64 + gx) as usize).copied().unwrap_or(false)
    }

    /// Queue every event that sounds in `[t0, t1)`, at its sample offset.
    fn schedule(&mut self, weather: &Weather, t0: f64, t1: f64) {
        let offset = |at: f64| ((at - t0) * SR).max(0.0) as usize;
        for e in weather.events(EventKind::Splash, t0, t1) {
            let (x, y) = e.spot();
            let (wait, pan, hard) = (offset(e.at), e.pan(), self.is_hard(x, y));
            let gain = e.rng().range(0.08, 0.22);
            self.ambience.trigger(|r| if hard { Hit::plink(wait, r, pan, gain) } else { Hit::tap(wait, r, pan, gain * 1.4) });
        }
        // A drip is heard when it lands, at the end of its fall.
        for e in weather.events(EventKind::Drip, t0 - DRIP_S, t1 - DRIP_S) {
            let (wait, pan) = (offset(e.at + DRIP_S), e.pan());
            self.ambience.trigger(|r| Hit::drip(wait, r, pan, 0.16));
        }
        for e in weather.events(EventKind::Spark, t0, t1) {
            let (wait, pan) = (offset(e.at), e.pan());
            let mut r = e.rng();
            for k in 0..r.int(2, 4) {
                let w = wait + (k as f64 * r.range(0.006, 0.02) * SR) as usize;
                self.ambience.trigger(|rr| Hit::crackle(w, rr, pan, 0.22));
            }
        }
        if let Some(wildlife) = &self.wildlife {
            for call in wildlife.calls(weather.kind, t0, t1) {
                self.creatures.trigger(&call, offset(call.at));
            }
        }
        for e in weather.events(EventKind::Strike, t0 - 3.1, t1) {
            let at = e.thunder_at();
            if at >= t0 && at < t1 {
                let (strength, distance) = e.strike();
                self.thunder.add(offset(at), strength, distance, e.pan(), e.hash);
            }
        }
    }

    /// Add the bed into `out`, starting at `t0` seconds. `out` must already hold
    /// the music mix; this only adds to it.
    pub fn render(&mut self, out: &mut [f32], t0: f64, weather: &Weather) {
        let frames = out.len() / 2;
        self.schedule(weather, t0, t0 + frames as f64 / SR);
        for i in 0..frames {
            if self.tick % CONTROL == 0 || self.state.is_none() {
                let s = weather.state(t0 + i as f64 / SR);
                for m in self.muffle.iter_mut() {
                    let open = 9_000.0 - 6_800.0 * s.snow;
                    m.set_cutoff(if self.roofed { open * 0.4 } else { open }, SR);
                }
                self.state = Some(s);
            }
            self.tick = self.tick.wrapping_add(1);
            let s = self.state.expect("sampled above");
            let (mut wl, mut wr) = self.wind.next(s.wind_strength, s.gust, s.wind_dir);
            if self.roofed {
                wl *= 0.45;
                wr *= 0.45;
            }
            let (rl, rr) = self.rain.next(s.rain);
            let (tl, tr) = self.thunder.next();
            let (al, ar) = self.ambience.next(&s);
            let (cl, cr) = self.creatures.next();
            let l = self.muffle[0].process(wl + rl + al + cl) + tl;
            let r = self.muffle[1].process(wr + rr + ar + cr) + tr;
            out[2 * i] += (l * BED_GAIN).clamp(-1.0, 1.0) as f32;
            out[2 * i + 1] += (r * BED_GAIN).clamp(-1.0, 1.0) as f32;
        }
    }
}
