//! Glitch artifact sounds: short one-shot voices triggered by the pattern layers.

use crate::math::{decay_coef, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// Recent drone output, so "stutter" glitches can chop up the frozen chord.
pub struct History {
    l: Vec<f64>,
    r: Vec<f64>,
    pos: usize,
}

pub const HISTORY_LEN: usize = 1 << 16;

impl History {
    pub fn new() -> Self {
        History { l: vec![0.0; HISTORY_LEN], r: vec![0.0; HISTORY_LEN], pos: 0 }
    }

    pub fn push(&mut self, l: f64, r: f64) {
        self.l[self.pos] = l;
        self.r[self.pos] = r;
        self.pos = (self.pos + 1) % HISTORY_LEN;
    }

    fn get(&self, index: usize) -> (f64, f64) {
        let i = index % HISTORY_LEN;
        (self.l[i], self.r[i])
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Bit-crushed, sample-held FM blip.
    Blip,
    /// Differentiated noise tick.
    Tick,
    /// A repeated grain chopped from the drone.
    Stutter,
    /// Inharmonic bell ping.
    Ping,
    /// Buzzing square-wave dropout.
    Click,
    /// Bit-crushed noise burst.
    Crush,
}

pub const KINDS: [Kind; 6] =
    [Kind::Blip, Kind::Tick, Kind::Stutter, Kind::Ping, Kind::Click, Kind::Crush];

/// What a pattern step plays.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub kind: Kind,
    pub freq: f64,
    pub gain: f64,
    pub pan: f64,
    /// Length in samples.
    pub len: u32,
    /// Kind-specific: crush levels, stutter grain size, etc.
    pub param: f64,
    /// Retriggers within one step (1-4).
    pub ratchet: u32,
    /// The step only sounds while the layer's density is above this.
    pub threshold: f64,
    pub noise_seed: u64,
}

#[derive(Clone)]
pub struct Voice {
    ev: Event,
    age: u32,
    env: f64,
    decay: f64,
    phase: f64,
    phase2: f64,
    held: f64,
    prev: f64,
    grain_start: usize,
    gains: (f64, f64),
    noise: Rng,
}

const FADE: u32 = 24;

impl Voice {
    pub fn new(ev: Event, history: &History) -> Self {
        let sr = SAMPLE_RATE as f64;
        let seconds = ev.len as f64 / sr;
        let decay = match ev.kind {
            Kind::Blip | Kind::Ping | Kind::Tick => decay_coef(seconds, sr),
            _ => 1.0,
        };
        let grain = (ev.param as usize).max(64);
        Voice {
            ev,
            age: 0,
            env: 1.0,
            decay,
            phase: 0.0,
            phase2: 0.0,
            held: 0.0,
            prev: 0.0,
            grain_start: history.pos + HISTORY_LEN - grain - (ev.noise_seed % 9000) as usize,
            gains: pan_gains(ev.pan),
            noise: Rng::new(ev.noise_seed),
        }
    }

    pub fn active(&self) -> bool {
        self.age < self.ev.len
    }

    pub fn event(&self) -> &Event {
        &self.ev
    }

    /// How far through its sound this voice is, 0..1.
    pub fn progress(&self) -> f64 {
        self.age as f64 / self.ev.len as f64
    }

    pub fn next(&mut self, history: &History) -> (f64, f64) {
        let sr = SAMPLE_RATE as f64;
        let ev = &self.ev;
        // Short linear fades on gated kinds so the stream clicks only on purpose.
        let gate = {
            let to_end = ev.len - self.age;
            (self.age.min(to_end).min(FADE) as f64) / FADE as f64
        };
        let (l, r) = match ev.kind {
            Kind::Blip => {
                let hold = ev.param as u32;
                if self.age % hold.max(1) == 0 {
                    let x = sin_turns(self.phase + 0.35 * sin_turns(self.phase2));
                    let levels = 3.0 + (ev.param % 7.0);
                    self.held = (x * levels).round() / levels;
                }
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                self.phase2 += ev.freq * 3.0 / sr;
                self.phase2 -= self.phase2.floor();
                let s = self.held * self.env * gate;
                (s, s)
            }
            Kind::Tick => {
                let n = self.noise.bipolar();
                let s = (n - self.prev) * 0.5 * self.env;
                self.prev = n;
                (s, s)
            }
            Kind::Stutter => {
                let grain = (ev.param as u32).max(64);
                let pos = self.age % grain;
                let edge = pos.min(grain - pos).min(FADE) as f64 / FADE as f64;
                let reverse = ev.noise_seed % 2 == 0;
                let offset = if reverse { grain - 1 - pos } else { pos };
                let (gl, gr) = history.get(self.grain_start + offset as usize);
                (gl * 2.5 * edge * gate, gr * 2.5 * edge * gate)
            }
            Kind::Ping => {
                let s = 0.7 * sin_turns(self.phase) + 0.3 * sin_turns(self.phase2);
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                self.phase2 += ev.freq * 2.756 / sr;
                self.phase2 -= self.phase2.floor();
                let attack = (self.age.min(96) as f64) / 96.0;
                let s = s * self.env * attack;
                (s, s)
            }
            Kind::Click => {
                self.phase += ev.freq / sr;
                self.phase -= self.phase.floor();
                let s = if self.phase < 0.5 { 0.6 } else { -0.6 } * gate;
                (s, s)
            }
            Kind::Crush => {
                let hold = (ev.param as u32).max(1);
                if self.age % hold == 0 {
                    self.held = (self.noise.bipolar() * 3.0).round() / 3.0;
                }
                let s = self.held * 0.5 * gate;
                (s, s)
            }
        };
        self.env *= self.decay;
        self.age += 1;
        (l * ev.gain * self.gains.0, r * ev.gain * self.gains.1)
    }
}
