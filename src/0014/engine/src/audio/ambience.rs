//! The small sounds of the weather: drops landing, drips, ember crackle, the
//! hush of snow and its icy tinkles, leaves rustling and the low air of fog.
//!
//! Splashes, drips and sparks are *events* from the shared schedule in
//! [`crate::weather`], so each one is heard on the very sample its picture
//! appears. The textures in between (rustle, crackle, tinkles) are random, but
//! their density is the same level the scene draws its particles from.

use crate::filter::Svf;
use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::weather::WeatherState;
use crate::SAMPLE_RATE;

const STREAM: u64 = 0x414d_4249; // "AMBI"
const SR: f64 = SAMPLE_RATE as f64;
/// The most one-shot hits that can sound at once; the oldest give way.
const MAX_HITS: usize = 48;
/// The drips' echo, in seconds, and its feedback.
const ECHO_S: f64 = 0.19;
const ECHO_FB: f64 = 0.38;

/// One short sound: noise through a resonance, and/or a gliding sine.
#[derive(Clone)]
pub struct Hit {
    wait: usize,
    res: Svf,
    noise: f64,
    noise_coef: f64,
    tone: f64,
    tone_coef: f64,
    phase: f64,
    dt: f64,
    glide: f64,
    gain: (f64, f64),
    /// Whether it feeds the drip echo.
    echo: bool,
}

impl Hit {
    fn new(wait: usize, pan: f64, gain: f64) -> Self {
        let (l, r) = pan_gains(pan.clamp(-1.0, 1.0) * 0.8);
        Hit {
            wait,
            res: Svf::new(1_000.0, 1.0, SR),
            noise: 0.0,
            noise_coef: 0.0,
            tone: 0.0,
            tone_coef: 0.0,
            phase: 0.0,
            dt: 0.0,
            glide: 0.0,
            gain: (gain * l, gain * r),
            echo: false,
        }
    }

    fn with_noise(mut self, hz: f64, q: f64, seconds: f64) -> Self {
        self.res = Svf::new(hz, q, SR);
        self.noise = 1.0;
        self.noise_coef = decay_coef(seconds, SR);
        self
    }

    fn with_tone(mut self, hz: f64, to_hz: f64, seconds: f64) -> Self {
        self.tone = 1.0;
        self.tone_coef = decay_coef(seconds, SR);
        self.dt = hz / SR;
        // A straight glide that reaches `to_hz` over the tone's first time
        // constant (additive, so no logarithm is needed: `HC004`).
        self.glide = (to_hz - hz) / SR / (seconds * SR);
        self
    }

    /// A drop on stone: a short bright ping over a click.
    pub fn plink(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(1_800.0, 4_200.0);
        Hit::new(wait, pan, gain).with_noise(hz, 14.0, 0.012).with_tone(hz * 0.5, hz * 0.55, 0.025)
    }

    /// A drop in the moss: a dull, soft tap.
    pub fn tap(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        Hit::new(wait, pan, gain).with_noise(rng.range(900.0, 1_800.0), 1.2, 0.008)
    }

    /// A drip landing in a pool: a sine whose pitch leaps up as it closes.
    pub fn drip(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(700.0, 1_100.0);
        let mut h = Hit::new(wait, pan, gain).with_tone(hz, hz * 2.2, 0.045);
        h.echo = true;
        h
    }

    /// One crackle of an ember: a tiny bright click.
    pub fn crackle(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        Hit::new(wait, pan, gain).with_noise(rng.range(2_500.0, 6_000.0), 0.9, rng.range(0.002, 0.006))
    }

    /// An icy tinkle: a high, quick sine.
    pub fn tinkle(wait: usize, rng: &mut Rng, pan: f64, gain: f64) -> Self {
        let hz = rng.range(3_200.0, 6_400.0);
        Hit::new(wait, pan, gain).with_tone(hz, hz, 0.06)
    }

    fn next(&mut self, rng: &mut Rng) -> (f64, f64, bool) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0, self.echo);
        }
        let mut y = 0.0;
        if self.noise > 1e-4 {
            y += self.res.process(rng.bipolar() * self.noise).band * 2.0;
            self.noise *= self.noise_coef;
        }
        if self.tone > 1e-4 {
            y += sin_turns(self.phase) * self.tone;
            self.phase += self.dt;
            self.phase -= self.phase.floor();
            self.dt = (self.dt + self.glide).max(0.0);
            self.tone *= self.tone_coef;
        }
        (y * self.gain.0, y * self.gain.1, self.echo)
    }

    fn done(&self) -> bool {
        self.wait == 0 && self.noise <= 1e-4 && self.tone <= 1e-4
    }
}

/// The ambience: the event hits, the drip echo and the level-driven textures.
#[derive(Clone)]
pub struct Ambience {
    rng: Rng,
    hits: Vec<Hit>,
    echo: Vec<f64>,
    echo_at: usize,
    hush: Svf,
    fog_air: Svf,
    rustle: Svf,
    rustle_env: f64,
    rustle_coef: f64,
}

impl Ambience {
    pub fn new(seed: u64) -> Self {
        Ambience {
            rng: Rng::stream(seed, STREAM),
            hits: Vec::new(),
            echo: vec![0.0; (ECHO_S * SR) as usize],
            echo_at: 0,
            hush: Svf::new(380.0, 0.6, SR),
            fog_air: Svf::new(220.0, 0.6, SR),
            rustle: Svf::new(3_000.0, 0.8, SR),
            rustle_env: 0.0,
            rustle_coef: decay_coef(0.012, SR),
        }
    }

    /// Start a hit. `make` gets this ambience's random stream.
    pub fn trigger(&mut self, make: impl FnOnce(&mut Rng) -> Hit) {
        let hit = make(&mut self.rng);
        self.start(hit);
    }

    /// Start a hit made elsewhere (from its own event's random stream).
    pub fn start(&mut self, hit: Hit) {
        if self.hits.len() >= MAX_HITS {
            self.hits.remove(0);
        }
        self.hits.push(hit);
    }

    /// One stereo sample under weather `s`.
    pub fn next(&mut self, s: &WeatherState) -> (f64, f64) {
        // Textures whose density is the scene's particle level.
        let leaf_rate = 140.0 * s.leaves * (0.4 + 0.6 * s.gust.min(1.0) + 0.3);
        if self.rng.chance(leaf_rate / SR) {
            self.rustle_env = self.rng.range(0.3, 1.0);
        }
        if self.rng.chance(30.0 * s.embers / SR) {
            let pan = self.rng.bipolar();
            let gain = self.rng.range(0.05, 0.16);
            self.trigger(|r| Hit::crackle(0, r, pan, gain));
        }
        if self.rng.chance(4.0 * s.snow / SR) {
            let pan = self.rng.bipolar();
            let gain = self.rng.range(0.01, 0.03);
            self.trigger(|r| Hit::tinkle(0, r, pan, gain));
        }

        let x = self.rng.bipolar();
        let rustle = self.rustle.process(x).band * self.rustle_env * 0.5;
        self.rustle_env *= self.rustle_coef;
        let bed = 0.6 * s.snow * self.hush.process(x).low + 0.5 * s.fog * self.fog_air.process(x).low;

        let (mut l, mut r) = (bed + rustle * 0.8, bed + rustle);
        let mut wet = 0.0;
        for hit in self.hits.iter_mut() {
            let (a, b, echo) = hit.next(&mut self.rng);
            l += a;
            r += b;
            if echo {
                wet += 0.5 * (a + b);
            }
        }
        self.hits.retain(|h| !h.done());

        // The drips' echo: a short cave-like repeat.
        let back = self.echo[self.echo_at];
        self.echo[self.echo_at] = wet + back * ECHO_FB;
        self.echo_at = (self.echo_at + 1) % self.echo.len();
        (l + back * 0.6, r + back * 0.6)
    }
}
