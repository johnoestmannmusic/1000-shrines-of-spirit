//! Microsound's sine pips: the melodic dust of the mode. Each pip is one
//! tone of the current chord lifted three or four octaves (about 1.5–8 kHz),
//! a pure sine 5–40 ms long under a smooth window, set at an exact place in
//! the stereo field. They land on the sixteenths, now and then on a
//! thirty-second between them, and sometimes come as a pair a thirty-second
//! apart. A short, quiet ping-pong echo gives each a small room.
//!
//! Every bar's pips are a pure function of seed 6 and the bar number.

use crate::delay::PingPong;
use crate::harmony::Harmony;
use crate::math::{midi_hz, sin_turns};
use crate::micro::Pips as Density;
use crate::rng::Rng;
use crate::tempo::Tempo;

const VOICES: usize = 8;
const LEVEL: f64 = 0.16;
const ECHO_MIX: f64 = 0.3;
const ECHO_FEEDBACK: f64 = 0.3;

/// One pip in a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pip {
    /// Where it starts, in thirty-seconds (0..32).
    pub at: u8,
    /// Which chord tone (index into the chord's tones, wrapped).
    pub tone: u8,
    /// Octaves above the chord tone's own register.
    pub lift: u8,
    /// Length in milliseconds.
    pub ms: f64,
    pub vel: f64,
    pub pan: f64,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    freq: f64,
    phase: f64,
    pos: u64,
    len: u64,
    vel: f64,
    pan: f64,
    on: bool,
}

pub struct Pips {
    seed: u64,
    density: Density,
    /// A thirty-second and a bar, in samples.
    t32: u64,
    bar: u64,
    voices: [Voice; VOICES],
    next_voice: usize,
    plan: Option<(u64, Vec<Pip>)>,
    echo: PingPong,
}

impl Pips {
    pub fn new(seed: u64, density: Density, tempo: Tempo) -> Self {
        let six = tempo.drum16;
        Pips {
            seed,
            density,
            t32: six / 2,
            bar: 16 * six,
            voices: [Voice::default(); VOICES],
            next_voice: 0,
            plan: None,
            // A dotted eighth: three sixteenths, so the repeats fall between.
            echo: PingPong::new((3 * six) as usize, ECHO_FEEDBACK),
        }
    }

    /// The pips of bar `bar` (pure).
    pub fn plan(&self, bar: u64) -> Vec<Pip> {
        let (lo, hi) = match self.density {
            Density::Off => return Vec::new(),
            Density::Sparse => (0, 3),
            Density::Busy => (3, 8),
        };
        // A two-bar figure, reshaped every eight bars.
        let (figure, shape) = (bar % 2, bar / 8);
        let mut rng = Rng::stream(self.seed, 0x919_0000 ^ (shape << 4) ^ figure);
        let n = lo + rng.below(hi - lo + 1);
        let mut out: Vec<Pip> = Vec::new();
        for _ in 0..n {
            // Mostly on the sixteenths.
            let at = (rng.below(16) * 2 + if rng.chance(0.2) { 1 } else { 0 }) as u8;
            if out.iter().any(|p| p.at == at) {
                continue;
            }
            let pip = Pip {
                at,
                tone: rng.below(4) as u8,
                lift: if rng.chance(0.6) { 3 } else { 4 },
                ms: rng.range(5.0, 40.0),
                vel: rng.range(0.5, 1.0),
                pan: rng.range(-0.9, 0.9),
            };
            out.push(pip);
            // Now and then a twin, a thirty-second later, on the other side.
            if at < 31 && rng.chance(0.25) {
                out.push(Pip { at: at + 1, vel: pip.vel * 0.6, pan: -pip.pan, ..pip });
            }
        }
        out.sort_by_key(|p| p.at);
        out
    }

    /// One stereo sample.
    pub fn next(&mut self, clock: u64, harmony: &Harmony, active: bool) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar)));
        }
        if active && in_bar % self.t32 == 0 {
            let slot = (in_bar / self.t32) as u8;
            let due: Vec<Pip> = self.plan.as_ref().unwrap().1.iter().copied().filter(|p| p.at == slot).collect();
            if !due.is_empty() {
                let chord = &harmony.chords[harmony.at(clock).chord];
                for p in due {
                    let tones = &chord.notes;
                    let base = tones[p.tone as usize % tones.len()];
                    // The tone's pitch class, lifted into the air.
                    let mut midi = base + 12.0 * p.lift as f64;
                    while midi > 112.0 {
                        midi -= 12.0;
                    }
                    while midi < 82.0 {
                        midi += 12.0;
                    }
                    let v = &mut self.voices[self.next_voice];
                    self.next_voice = (self.next_voice + 1) % VOICES;
                    *v = Voice {
                        freq: midi_hz(midi),
                        phase: 0.0,
                        pos: 0,
                        len: ((p.ms / 1000.0) * sr) as u64,
                        vel: p.vel,
                        pan: p.pan,
                        on: true,
                    };
                }
            }
        }
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if !v.on {
                continue;
            }
            if v.pos >= v.len {
                v.on = false;
                continue;
            }
            let w = sin_turns(0.5 * v.pos as f64 / v.len as f64);
            let s = sin_turns(v.phase) * w * w * v.vel;
            v.phase += v.freq / sr;
            v.phase -= v.phase.floor();
            v.pos += 1;
            let (gl, gr) = crate::math::pan_gains(v.pan);
            l += s * gl;
            r += s * gr;
        }
        let (el, er) = self.echo.process(l, r);
        ((l + el * ECHO_MIX) * LEVEL, (r + er * ECHO_MIX) * LEVEL)
    }
}
