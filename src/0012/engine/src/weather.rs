//! Weather: the shrine's atmosphere, fixed for the piece and chosen at setup.
//!
//! Within one shrine the *kind* never changes — there is no transition from
//! rain to clear. Only the intensity cycles: wind and rain swell and ease on
//! their own slow periods. The state is a pure function of time and seed, so
//! the audio and the visuals sample exactly the same weather (`HC004`).

use crate::math::cos_turns;
use crate::palette::wrap01;
use crate::rng::Rng;

/// Stream id, so weather never shares a sequence with another generator.
const STREAM: u64 = 0x5745_4154; // "WEAT"

/// The kinds of atmosphere a shrine can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Rain,
    Wind,
    Fog,
    Storm,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 5] = [
        WeatherKind::Clear,
        WeatherKind::Rain,
        WeatherKind::Wind,
        WeatherKind::Fog,
        WeatherKind::Storm,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WeatherKind::Clear => "Clear",
            WeatherKind::Rain => "Rain",
            WeatherKind::Wind => "Wind",
            WeatherKind::Fog => "Fog",
            WeatherKind::Storm => "Storm",
        }
    }

    /// Baseline `(wind, rain, fog, light)` for the kind, before cycling.
    fn baseline(self) -> (f64, f64, f64, f64) {
        match self {
            WeatherKind::Clear => (0.18, 0.00, 0.10, 0.95),
            WeatherKind::Rain => (0.35, 0.75, 0.35, 0.62),
            WeatherKind::Wind => (0.85, 0.10, 0.18, 0.80),
            WeatherKind::Fog => (0.15, 0.15, 0.85, 0.55),
            WeatherKind::Storm => (0.95, 0.95, 0.45, 0.42),
        }
    }
}

/// The weather sampled at one instant. All values are in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherState {
    /// Wind direction in turns (`0` = one way, `0.5` = the other).
    pub wind_dir: f64,
    pub wind_strength: f64,
    pub rain: f64,
    pub fog: f64,
    /// Ambient light: how much the palette is lit rather than shadowed.
    pub light: f64,
}

/// A shrine's weather: one kind, with its own slow phases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub kind: WeatherKind,
    phase: f64,
    wind_period: f64,
    rain_period: f64,
}

impl Weather {
    /// The weather for a shrine, seeded so two shrines with the same kind
    /// still cycle differently.
    pub fn new(kind: WeatherKind, seed: u64) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        Weather {
            kind,
            phase: rng.unit(),
            wind_period: rng.range(38.0, 96.0),
            rain_period: rng.range(24.0, 70.0),
        }
    }

    /// The weather at `t` seconds. Pure and deterministic.
    pub fn state(&self, t: f64) -> WeatherState {
        let wind_cycle = 0.5 - 0.5 * cos_turns(t / self.wind_period + self.phase);
        let rain_cycle = 0.5 - 0.5 * cos_turns(t / self.rain_period + self.phase * 0.7 + 0.13);
        let (bw, br, bf, bl) = self.kind.baseline();
        WeatherState {
            wind_dir: wrap01(self.phase + t / (self.wind_period * 6.0)),
            wind_strength: (bw * (0.35 + 0.65 * wind_cycle)).clamp(0.0, 1.0),
            rain: (br * (0.25 + 0.75 * rain_cycle)).clamp(0.0, 1.0),
            fog: (bf * (0.55 + 0.45 * wind_cycle)).clamp(0.0, 1.0),
            light: (bl * (0.80 + 0.20 * wind_cycle)).clamp(0.0, 1.0),
        }
    }
}
