//! Cyclic repeats: each glitch layer loops a short step pattern a number of
//! times, then mutates one step and loops again. Two layers with co-prime
//! cycle lengths drift in and out of phase with each other.

use crate::glitch::{Event, History, Kind, Voice, KINDS};
use crate::math::midi_hz;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

pub struct LayerConfig {
    pub steps: usize,
    /// Step length in samples (divisible by 1-4 for ratchets).
    pub step_len: u64,
    /// Relative weight of each `KINDS` entry.
    pub weights: [f64; 6],
    /// MIDI notes for pitched kinds.
    pub notes: Vec<f64>,
    /// Chance a step holds an event.
    pub fill: f64,
    pub gain: (f64, f64),
    pub pan_width: f64,
    /// Loops before mutating (min, max).
    pub repeats: (u64, u64),
    pub max_ratchet: u64,
}

const MAX_VOICES: usize = 8;

/// What the last mutation did (for display; never read by the audio path).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mutation {
    /// A step was given a new random event of this kind.
    Replace(Kind),
    /// A step was emptied.
    Clear,
    /// The whole pattern rotated one step left.
    Rotate,
    /// A step kept its sound but got a new pitch/pan.
    Nudge,
}

pub struct Layer {
    cfg: LayerConfig,
    rng: Rng,
    pattern: Vec<Option<Event>>,
    loops: u64,
    loops_until_mutation: u64,
    voices: Vec<Voice>,
    /// (clock, step, what) of the latest mutation.
    last_mutation: Option<(u64, usize, Mutation)>,
    triggers: u64,
}

impl Layer {
    pub fn new(cfg: LayerConfig, seed: u64, stream: u64) -> Self {
        let mut rng = Rng::stream(seed, stream);
        let mut layer = Layer {
            pattern: Vec::new(),
            loops: 0,
            loops_until_mutation: rng.int(cfg.repeats.0, cfg.repeats.1),
            voices: Vec::with_capacity(MAX_VOICES),
            last_mutation: None,
            triggers: 0,
            cfg,
            rng,
        };
        for _ in 0..layer.cfg.steps {
            let step = layer.maybe_event();
            layer.pattern.push(step);
        }
        layer
    }

    fn maybe_event(&mut self) -> Option<Event> {
        if self.rng.chance(self.cfg.fill) {
            Some(self.random_event())
        } else {
            None
        }
    }

    fn random_event(&mut self) -> Event {
        let rng = &mut self.rng;
        let cfg = &self.cfg;
        let sr = SAMPLE_RATE as f64;
        let kind = KINDS[rng.weighted(&cfg.weights)];
        let note = rng.pick(&cfg.notes);
        let ms = |rng: &mut Rng, lo: f64, hi: f64| (rng.range(lo, hi) * sr / 1000.0) as u32;
        let (len, param, gain_trim) = match kind {
            Kind::Blip => (ms(rng, 25.0, 140.0), rng.int(1, 12) as f64, 1.0),
            Kind::Tick => (ms(rng, 2.0, 9.0), 0.0, 1.4),
            Kind::Stutter => {
                let grain = ms(rng, 12.0, 90.0);
                (grain * rng.int(2, 6) as u32, grain as f64, 1.0)
            }
            Kind::Ping => (ms(rng, 300.0, 1600.0), 0.0, 0.8),
            Kind::Click => (ms(rng, 3.0, 30.0), 0.0, 0.5),
            Kind::Crush => (ms(rng, 15.0, 80.0), rng.int(2, 40) as f64, 0.6),
        };
        let freq = match kind {
            Kind::Click => rng.range(300.0, 3000.0),
            _ => midi_hz(note),
        };
        Event {
            kind,
            freq,
            gain: rng.range(cfg.gain.0, cfg.gain.1) * gain_trim,
            pan: rng.bipolar() * cfg.pan_width,
            len: len.max(48),
            param,
            ratchet: if rng.chance(0.25) { rng.int(2, cfg.max_ratchet) as u32 } else { 1 },
            threshold: rng.unit(),
            noise_seed: rng.next_u64(),
        }
    }

    fn mutate(&mut self, clock: u64) {
        let i = self.rng.below(self.cfg.steps);
        let what = match self.rng.weighted(&[0.55, 0.2, 0.15, 0.1]) {
            0 => {
                let ev = self.random_event();
                self.pattern[i] = Some(ev);
                Mutation::Replace(ev.kind)
            }
            1 => {
                self.pattern[i] = None;
                Mutation::Clear
            }
            2 => {
                self.pattern.rotate_left(1);
                Mutation::Rotate
            }
            _ => {
                // Nudge an existing step's pitch / pan, keeping its character.
                let note = self.rng.pick(&self.cfg.notes);
                let pan = self.rng.bipolar() * self.cfg.pan_width;
                if let Some(ev) = self.pattern[i].as_mut() {
                    if ev.kind != Kind::Click {
                        ev.freq = midi_hz(note);
                    }
                    ev.pan = pan;
                }
                Mutation::Nudge
            }
        };
        self.last_mutation = Some((clock, i, what));
    }

    fn trigger(&mut self, ev: Event, history: &History) {
        self.voices.retain(|v| v.active());
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice::new(ev, history));
        self.triggers += 1;
    }

    pub fn config(&self) -> &LayerConfig {
        &self.cfg
    }

    pub fn pattern(&self) -> &[Option<Event>] {
        &self.pattern
    }

    /// (completed loops, loops before the next mutation).
    pub fn loops(&self) -> (u64, u64) {
        (self.loops, self.loops_until_mutation)
    }

    pub fn voices(&self) -> impl Iterator<Item = &Voice> {
        self.voices.iter().filter(|v| v.active())
    }

    pub fn last_mutation(&self) -> Option<(u64, usize, Mutation)> {
        self.last_mutation
    }

    /// Total events triggered so far.
    pub fn triggers(&self) -> u64 {
        self.triggers
    }

    pub fn next(&mut self, clock: u64, density: f64, history: &History) -> (f64, f64) {
        let step_len = self.cfg.step_len;
        let phase = clock % step_len;
        let step = ((clock / step_len) % self.cfg.steps as u64) as usize;
        if phase == 0 && step == 0 && clock > 0 {
            self.loops += 1;
            if self.loops >= self.loops_until_mutation {
                self.mutate(clock);
                self.loops = 0;
                self.loops_until_mutation = self.rng.int(self.cfg.repeats.0, self.cfg.repeats.1);
            }
        }
        if let Some(ev) = self.pattern[step] {
            let sub = step_len / ev.ratchet as u64;
            if phase % sub == 0 && density > ev.threshold {
                self.trigger(ev, history);
            }
        }
        let mut out = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if v.active() {
                let (l, r) = v.next(history);
                out.0 += l;
                out.1 += r;
            }
        }
        out
    }
}
