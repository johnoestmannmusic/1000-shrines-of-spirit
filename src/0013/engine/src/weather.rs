//! Weather: the shrine's atmosphere, fixed for the piece and chosen at setup.
//!
//! Within one shrine the *kind* never changes — there is no transition from
//! rain to clear. Only the intensity cycles: wind and rain swell and ease on
//! their own slow periods, and the wind gusts on faster ones. The state is a
//! pure function of time and seed, so the audio and the visuals sample exactly
//! the same weather (`HC004`).
//!
//! # One table for intensity
//!
//! Every field of [`WeatherState`] is a level in `[0, 1]` that *both* halves
//! read: the scene draws that many particles and the weather bed plays that
//! loud. A level of 0 means none of it — no particle and no sound — so the two
//! can never disagree about whether it is raining.
//!
//! # Events
//!
//! Lightning, splashes, drips and ember sparks are discrete events. They are
//! found by [`Weather::events`], a stateless schedule: each of a fixed set of
//! *slots* repeats on its own period, and a hash of (seed, slot, cycle) decides
//! whether that cycle fires, when, and where. Any time range can be queried
//! from scratch, so the page's visual instance and the worklet's audio instance
//! find exactly the same events without ever talking to each other.

use crate::math::{cos_turns, exp2, sin_turns};
use crate::palette::wrap01;
use crate::rng::Rng;

/// Stream id, so weather never shares a sequence with another generator.
const STREAM: u64 = 0x5745_4154; // "WEAT"

/// The kinds of atmosphere a shrine can have. New kinds are only ever
/// appended: recipes store the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Rain,
    Wind,
    Fog,
    Storm,
    Snow,
    Dusk,
}

/// How a kind looks and sounds at full intensity, before cycling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub wind: f64,
    pub rain: f64,
    pub fog: f64,
    pub light: f64,
    pub snow: f64,
    pub embers: f64,
    pub motes: f64,
    pub leaves: f64,
    pub cloud: f64,
    /// Condensation dripping off the architecture.
    pub drips: f64,
    /// Whether a butterfly now and then drifts across (a rare, quiet sight).
    pub butterflies: bool,
    /// The colour the whole scene is graded toward, and by how much (0-100).
    pub tint: [u8; 3],
    pub grade: u32,
    pub lightning: bool,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 7] = [
        WeatherKind::Clear,
        WeatherKind::Rain,
        WeatherKind::Wind,
        WeatherKind::Fog,
        WeatherKind::Storm,
        WeatherKind::Snow,
        WeatherKind::Dusk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WeatherKind::Clear => "Clear",
            WeatherKind::Rain => "Rain",
            WeatherKind::Wind => "Wind",
            WeatherKind::Fog => "Fog",
            WeatherKind::Storm => "Storm",
            WeatherKind::Snow => "Snow",
            WeatherKind::Dusk => "Dusk",
        }
    }

    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            WeatherKind::Clear => "warm light, drifting motes, a soft breeze",
            WeatherKind::Rain => "layered rain, splashes on stone, a wet sheen",
            WeatherKind::Wind => "gusts, blown leaves, fast clouds, a whistling air",
            WeatherKind::Fog => "drifting banks, low mist, slow drips",
            WeatherKind::Storm => "driving rain, lightning and the thunder after it",
            WeatherKind::Snow => "falling flakes, whitening stone, a muffled hush",
            WeatherKind::Dusk => "a low warm sun and rising embers",
        }
    }

    pub fn profile(self) -> Profile {
        let p = |wind, rain, fog, light, cloud, tint, grade| Profile {
            wind,
            rain,
            fog,
            light,
            snow: 0.0,
            embers: 0.0,
            motes: 0.0,
            leaves: 0.0,
            cloud,
            drips: 0.0,
            butterflies: false,
            tint,
            grade,
            lightning: false,
        };
        match self {
            WeatherKind::Clear => Profile { motes: 0.85, butterflies: true, ..p(0.18, 0.0, 0.08, 0.97, 0.1, [255, 222, 168], 10) },
            WeatherKind::Rain => Profile { drips: 0.5, leaves: 0.08, ..p(0.35, 0.75, 0.35, 0.62, 0.6, [150, 170, 205], 14) },
            WeatherKind::Wind => Profile { leaves: 0.95, motes: 0.2, ..p(0.85, 0.05, 0.15, 0.82, 0.45, [200, 214, 232], 8) },
            WeatherKind::Fog => Profile { drips: 0.9, motes: 0.15, ..p(0.12, 0.05, 0.9, 0.55, 0.3, [188, 194, 202], 24) },
            WeatherKind::Storm => {
                Profile { leaves: 0.5, lightning: true, ..p(0.95, 0.95, 0.45, 0.40, 1.0, [118, 132, 172], 22) }
            }
            WeatherKind::Snow => Profile { snow: 0.85, ..p(0.3, 0.0, 0.3, 0.78, 0.5, [190, 210, 255], 22) },
            WeatherKind::Dusk => Profile { embers: 0.8, motes: 0.3, leaves: 0.05, ..p(0.22, 0.0, 0.15, 0.72, 0.25, [255, 136, 88], 30) },
        }
    }
}

/// The weather sampled at one instant. All values are in `[0, 1]`, and every
/// one is a level both the picture and the sound follow (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherState {
    /// Wind direction in turns (`0` = one way, `0.5` = the other).
    pub wind_dir: f64,
    /// The wind, gusts included: foliage sway and the wind's loudness.
    pub wind_strength: f64,
    /// The fast gust envelope alone (whistle, leaf bursts).
    pub gust: f64,
    pub rain: f64,
    pub fog: f64,
    /// Ambient light: how much the palette is lit rather than shadowed.
    pub light: f64,
    pub snow: f64,
    /// How white the tops have turned under the snow.
    pub cover: f64,
    pub embers: f64,
    pub motes: f64,
    pub leaves: f64,
    pub cloud: f64,
    pub drips: f64,
    /// Lightning: how bright the sky is flashing right now.
    pub flash: f64,
}

/// What a scheduled event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Strike,
    Splash,
    Drip,
    Spark,
    /// A butterfly setting off across the scene: about one every two minutes.
    Butterfly,
}

impl EventKind {
    /// (slots, shortest period, longest period, stream).
    fn schedule(self) -> (u64, f64, f64, u64) {
        match self {
            // Strikes: one slot per ~9 s window, so storms never strobe.
            EventKind::Strike => (1, 9.0, 9.0, 0x5354_524b),
            // ~90 splashes a second at the heaviest rain.
            EventKind::Splash => (96, 0.6, 1.6, 0x5350_4c53),
            EventKind::Drip => (6, 2.0, 6.0, 0x4452_4950),
            EventKind::Spark => (12, 0.4, 1.2, 0x5350_524b),
            EventKind::Butterfly => (1, 120.0, 120.0, 0x4246_4c59),
        }
    }
}

/// One scheduled event. `hash` seeds everything else about it (where, how
/// strong), through [`Event::rng`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub at: f64,
    pub hash: u64,
}

impl Event {
    /// The event's own random numbers. Draw them in a fixed order.
    pub fn rng(&self) -> Rng {
        Rng::new(self.hash)
    }

    /// A strike's strength in `[0.4, 1]` and distance in `[0, 1]`.
    pub fn strike(&self) -> (f64, f64) {
        let mut r = self.rng();
        (r.range(0.4, 1.0), r.unit())
    }

    /// When a strike's thunder arrives: sooner the closer it is.
    pub fn thunder_at(&self) -> f64 {
        self.at + 0.3 + 2.7 * self.strike().1
    }

    /// Where the event lands, in grid cells (fractional): anywhere on the
    /// 24x24 plateau.
    pub fn spot(&self) -> (f64, f64) {
        let mut r = Rng::new(self.hash ^ 0x5350_4f54);
        (r.range(0.0, crate::GRID_W as f64 - 1.0), r.range(0.0, crate::GRID_H as f64 - 1.0))
    }

    /// Stereo position in `[-1, 1]` of an event at `spot`: the screen's x.
    pub fn pan(&self) -> f64 {
        let (gx, gy) = self.spot();
        ((gx - gy) / crate::GRID_W as f64).clamp(-1.0, 1.0)
    }
}

/// A strike flickers in up to three pulses: (delay s, relative brightness).
const FLICKER: [(f64, f64); 3] = [(0.0, 1.0), (0.07, 0.6), (0.17, 0.85)];
/// How long a flash pulse takes to halve, in seconds.
const FLICKER_HALF: f64 = 0.03;

/// A shrine's weather: one kind, with its own slow phases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub kind: WeatherKind,
    seed: u64,
    phase: f64,
    wind_period: f64,
    rain_period: f64,
    gust_periods: [f64; 3],
    gust_phases: [f64; 3],
}

impl Weather {
    /// The weather for a shrine, seeded so two shrines with the same kind
    /// still cycle differently.
    pub fn new(kind: WeatherKind, seed: u64) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        let phase = rng.unit();
        let wind_period = rng.range(38.0, 96.0);
        let rain_period = rng.range(24.0, 70.0);
        // Drawn after the original three, so those keep their values.
        let gust_periods = [rng.range(3.0, 5.0), rng.range(6.0, 9.0), rng.range(11.0, 17.0)];
        let gust_phases = [rng.unit(), rng.unit(), rng.unit()];
        Weather { kind, seed, phase, wind_period, rain_period, gust_periods, gust_phases }
    }

    fn wind_cycle(&self, t: f64) -> f64 {
        0.5 - 0.5 * cos_turns(t / self.wind_period + self.phase)
    }

    fn rain_cycle(&self, t: f64) -> f64 {
        0.5 - 0.5 * cos_turns(t / self.rain_period + self.phase * 0.7 + 0.13)
    }

    /// The fast gust envelope, in `[0, 1]`: three waves, squared so calm is
    /// common and a gust is an event.
    fn gust(&self, t: f64) -> f64 {
        let w = [0.5, 0.3, 0.2];
        let mut g = 0.0;
        for k in 0..3 {
            g += w[k] * (0.5 + 0.5 * sin_turns(t / self.gust_periods[k] + self.gust_phases[k]));
        }
        g * g
    }

    /// The weather at `t` seconds. Pure and deterministic.
    pub fn state(&self, t: f64) -> WeatherState {
        let p = self.kind.profile();
        let wind_cycle = self.wind_cycle(t);
        let rain_cycle = self.rain_cycle(t);
        let gust = self.gust(t);
        let wind = (p.wind * (0.3 + 0.45 * wind_cycle + 0.25 * gust)).clamp(0.0, 1.0);
        let rain = (p.rain * (0.25 + 0.75 * rain_cycle)).clamp(0.0, 1.0);
        let snow = (p.snow * (0.3 + 0.7 * rain_cycle)).clamp(0.0, 1.0);
        WeatherState {
            wind_dir: wrap01(self.phase + t / (self.wind_period * 6.0)),
            wind_strength: wind,
            gust: (gust * p.wind).clamp(0.0, 1.0),
            rain,
            fog: (p.fog * (0.55 + 0.45 * wind_cycle)).clamp(0.0, 1.0),
            light: (p.light * (0.80 + 0.20 * wind_cycle)).clamp(0.0, 1.0),
            snow,
            cover: (p.snow * (0.45 + 0.55 * rain_cycle)).clamp(0.0, 1.0),
            embers: (p.embers * (0.4 + 0.6 * (1.0 - wind_cycle))).clamp(0.0, 1.0),
            motes: (p.motes * (0.5 + 0.5 * (1.0 - gust))).clamp(0.0, 1.0),
            leaves: (p.leaves * (0.15 + 0.85 * wind)).clamp(0.0, 1.0),
            cloud: (p.cloud * (0.6 + 0.4 * rain_cycle)).clamp(0.0, 1.0),
            drips: (p.drips * (0.4 + 0.6 * (1.0 - wind_cycle))).clamp(0.0, 1.0),
            flash: self.flash(t),
        }
    }

    /// How likely a slot's cycle is to fire, from the level at that moment.
    /// Strikes are gated on the rain rather than calling back into `state`.
    fn gate(&self, kind: EventKind, t: f64) -> f64 {
        let p = self.kind.profile();
        match kind {
            EventKind::Strike if p.lightning => 0.35 + 0.6 * self.rain_cycle(t),
            EventKind::Strike => 0.0,
            EventKind::Splash => p.rain * (0.25 + 0.75 * self.rain_cycle(t)),
            EventKind::Drip => p.drips * (0.4 + 0.6 * (1.0 - self.wind_cycle(t))),
            EventKind::Spark => 0.8 * p.embers * (0.4 + 0.6 * (1.0 - self.wind_cycle(t))),
            EventKind::Butterfly => p.butterflies as u8 as f64,
        }
    }

    /// Every event of `kind` that happens in `[t0, t1)`, in no particular
    /// order. Pure: the same range always gives the same events.
    pub fn events(&self, kind: EventKind, t0: f64, t1: f64) -> Vec<Event> {
        let mut out = Vec::new();
        if t1 <= t0 {
            return out;
        }
        let (slots, lo, hi, stream) = kind.schedule();
        let mut slot_rng = Rng::stream(self.seed, stream);
        for slot in 0..slots {
            let period = slot_rng.range(lo, hi);
            let phase = slot_rng.unit() * period;
            let first = ((t0 + phase) / period).floor() as i64 - 1;
            let last = ((t1 + phase) / period).floor() as i64;
            for n in first..=last {
                let hash = crate::rng::hash3(self.seed ^ stream, slot, n as u64);
                let mut r = Rng::new(hash);
                let at = (n as f64 + 0.9 * r.unit()) * period - phase;
                if at < t0 || at >= t1 {
                    continue;
                }
                if r.unit() >= self.gate(kind, at) {
                    continue;
                }
                out.push(Event { kind, at, hash: r.next_u64() });
            }
        }
        out
    }

    /// The lightning flash at `t`, in `[0, 1]`: every strike of the last
    /// half-second, flickering and fading, brighter the closer it struck.
    pub fn flash(&self, t: f64) -> f64 {
        if !self.kind.profile().lightning {
            return 0.0;
        }
        let mut f = 0.0;
        for e in self.events(EventKind::Strike, t - 0.6, t + 1e-9) {
            let (strength, distance) = e.strike();
            let bright = strength * (1.0 - 0.6 * distance);
            for (delay, rel) in FLICKER {
                let dt = t - e.at - delay;
                if dt >= 0.0 {
                    f += bright * rel * exp2(-dt / FLICKER_HALF);
                }
            }
        }
        f.clamp(0.0, 1.0)
    }
}
