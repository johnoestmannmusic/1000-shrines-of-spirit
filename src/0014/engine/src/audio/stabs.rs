//! 2-step garage's chord stabs: the melodic, bright-eyed part of the mode.
//!
//! Each stab is the current chord as an extended voicing around middle C —
//! the third, fifth, seventh and ninth stacked from the scale, the root left
//! to the bass — struck short on the pushes that make the groove skip (the
//! "a" of one, the "and" of two, around beat three, the "a" of four), swung
//! with the drums, in a two-bar call and answer. Stabs late in the bar
//! before a change anticipate the next chord. In the glitchier sections a
//! stab can stutter, retriggering a swung thirty-second later.
//!
//! Two tones: *dark*, a soft electric-piano FM with a barking attack that
//! mellows; *bright*, a glassy bell-organ of a sine, its octave and a quickly
//! fading inharmonic bell partial. Both sit in a dotted-eighth ping-pong echo
//! and the shared reverb, under the glitch-ambient bed.
//!
//! Every bar's stabs are a pure function of seed 6 and the bar number.

use crate::delay::PingPong;
use crate::drums::Section;
use crate::garage::{StabTone, Stabs as Density};
use crate::harmony::Harmony;
use crate::math::{midi_hz, sin_turns};
use crate::rng::Rng;
use crate::tempo::Tempo;

/// Voices sounding at once (two overlapping four-note chords).
const VOICES: usize = 8;
/// Where stabs like to land, and how much (index = sixteenth in the bar).
const PUSHES: [f64; 16] = [0.05, 0.0, 0.15, 0.6, 0.0, 0.1, 0.55, 0.2, 0.0, 0.15, 0.45, 0.4, 0.0, 0.15, 0.5, 0.2];
/// Echo: a dotted eighth, three sixteenths.
const ECHO_SIXTEENTHS: u64 = 3;
const ECHO_FEEDBACK: f64 = 0.38;
const ECHO_MIX: f64 = 0.32;
const LEVEL: f64 = 0.34;

/// One stab in a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub step: u8,
    /// How long it is held, in sixteenths.
    pub len: f64,
    /// Retriggers a swung thirty-second later.
    pub stutter: bool,
    pub vel: f64,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    freq: f64,
    phase: f64,
    mod_phase: f64,
    bell_phase: f64,
    env: f64,
    fenv: f64,
    vel: f64,
    pan: f64,
    release_at: u64,
    on: bool,
}

pub struct Stabs {
    seed: u64,
    density: Density,
    tone: StabTone,
    six: u64,
    bar: u64,
    push: u64,
    voices: [Voice; VOICES],
    next_voice: usize,
    plan: Option<(u64, Vec<Hit>)>,
    /// The stutter waiting to fire: (when, chord voicing).
    pending: Option<(u64, Vec<f64>, f64, u64)>,
    echo: PingPong,
    /// The progression's voicings, worked out on first use.
    voiced: Vec<Vec<f64>>,
}

/// The seventh chord on each chord of the progression (root, third, fifth
/// and seventh from the scale), in close position around middle C and
/// voice-led: each chord takes whichever inversion lies nearest the one
/// before it, so the top line moves by step. (Scales with fewer than seven
/// notes can repeat a pitch class; then the chord is the distinct ones.)
pub fn voicings(harmony: &Harmony) -> Vec<Vec<f64>> {
    let iv = harmony.scale.intervals();
    let n = iv.len();
    let pcs = |degree: usize| -> Vec<i64> {
        let mut v: Vec<i64> = [0usize, 2, 4, 6].iter().map(|s| (harmony.key as i64 + iv[(degree + s) % n] as i64).rem_euclid(12)).collect();
        v.dedup();
        let first = v[0];
        if v.len() > 1 && *v.last().unwrap() == first {
            v.pop();
        }
        v
    };
    // Every close-position stack of the chord with its lowest note in 50..62.
    let candidates = |p: &[i64]| -> Vec<Vec<f64>> {
        let mut out = Vec::new();
        for low in 50i64..=62 {
            if let Some(r) = p.iter().position(|c| *c == low.rem_euclid(12)) {
                let mut stack = vec![low];
                for k in 1..p.len() {
                    let want = p[(r + k) % p.len()];
                    let mut m = stack[k - 1] + 1;
                    while m.rem_euclid(12) != want {
                        m += 1;
                    }
                    stack.push(m);
                }
                out.push(stack.iter().map(|m| *m as f64).collect());
            }
        }
        out
    };
    let distance = |a: &[f64], b: &[f64]| -> f64 {
        let k = a.len().min(b.len());
        (0..k).map(|i| (a[i] - b[i]).abs()).sum::<f64>() + (a.len() as f64 - b.len() as f64).abs() * 3.0
    };
    let mut out: Vec<Vec<f64>> = Vec::new();
    for c in &harmony.chords {
        let cands = candidates(&pcs(c.degree));
        let best = match out.last() {
            // The first chord: centred on middle C and a little above.
            None => cands.iter().min_by(|a, b| {
                let centre = |v: &Vec<f64>| (v.iter().sum::<f64>() / v.len() as f64 - 63.0).abs();
                centre(a).partial_cmp(&centre(b)).unwrap()
            }),
            Some(prev) => cands.iter().min_by(|a, b| distance(a, prev).partial_cmp(&distance(b, prev)).unwrap()),
        };
        out.push(best.cloned().unwrap_or_default());
    }
    out
}

impl Stabs {
    pub fn new(seed: u64, density: Density, tone: StabTone, tempo: Tempo, swing: f64) -> Self {
        let six = tempo.drum16;
        Stabs {
            seed,
            density,
            tone,
            six,
            bar: 16 * six,
            push: ((2.0 * swing - 1.0) * six as f64) as u64,
            voices: [Voice::default(); VOICES],
            next_voice: 0,
            plan: None,
            pending: None,
            voiced: Vec::new(),
            echo: PingPong::new((ECHO_SIXTEENTHS * six) as usize, ECHO_FEEDBACK),
        }
    }

    /// The stabs of bar `bar` in `section` (pure).
    pub fn plan(&self, bar: u64, section: Section) -> Vec<Hit> {
        if self.density == Density::Off {
            return Vec::new();
        }
        let busy = self.density == Density::Busy;
        // A two-bar call and answer, reshaped every eight bars.
        let (figure, shape) = (bar % 2, bar / 8);
        let mut rng = Rng::stream(self.seed, 0x57AB_0000 ^ (shape << 4) ^ figure);
        // The answer bar is sparser than the call.
        let want = match (busy, figure) {
            (false, 0) => 2,
            (false, _) => 1,
            (true, 0) => 3,
            (true, _) => 2,
        };
        let mut hits: Vec<Hit> = Vec::new();
        let mut weights = PUSHES;
        for _ in 0..want {
            let i = rng.weighted(&weights);
            weights[i] = 0.0;
            // Keep a neighbour from crowding it.
            if i > 0 {
                weights[i - 1] *= 0.2;
            }
            if i < 15 {
                weights[i + 1] *= 0.2;
            }
            let len = if rng.chance(0.75) { rng.range(0.6, 1.1) } else { rng.range(1.8, 3.0) };
            hits.push(Hit { step: i as u8, len, stutter: false, vel: rng.range(0.75, 1.0) });
        }
        hits.sort_by(|a, b| a.step.cmp(&b.step));
        match section {
            // In the breakdown the stabs bloom into held chords.
            Section::Breakdown => {
                for h in hits.iter_mut() {
                    h.len = h.len.max(2.5);
                }
            }
            Section::Shuffle => {
                let mut g = Rng::stream(self.seed, 0x57AB_5700 ^ bar);
                for h in hits.iter_mut() {
                    h.stutter = g.chance(0.35);
                }
            }
            _ => {}
        }
        hits
    }

    fn onset(&self, step: u8) -> u64 {
        step as u64 * self.six + if step % 2 == 1 { self.push } else { 0 }
    }

    fn strike(&mut self, clock: u64, chord: &[f64], vel: f64, len: u64) {
        let mut r = Rng::new(crate::rng::hash3(self.seed, 0x57AB, clock));
        for (k, m) in chord.iter().enumerate() {
            let v = &mut self.voices[self.next_voice];
            self.next_voice = (self.next_voice + 1) % VOICES;
            *v = Voice {
                freq: midi_hz(*m),
                phase: 0.0,
                mod_phase: 0.0,
                bell_phase: 0.0,
                env: 0.0,
                fenv: 1.0,
                vel: vel * r.range(0.85, 1.0),
                // Spread across the field, low notes nearer the middle.
                pan: (k as f64 - 1.5) / 1.5 * 0.45 + r.range(-0.1, 0.1),
                // A hair of strum: each note a few samples later.
                release_at: clock + len + k as u64 * 40,
                on: true,
            };
        }
    }

    /// One stereo sample. `active` follows the drums' sections; `section` is
    /// the current one.
    pub fn next(&mut self, clock: u64, harmony: &Harmony, changing: bool, active: bool, section: Section) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar, section)));
        }
        if self.voiced.is_empty() {
            self.voiced = voicings(harmony);
        }
        if active {
            let hit = self.plan.as_ref().unwrap().1.iter().copied().find(|h| self.onset(h.step) == in_bar);
            if let Some(h) = hit {
                let p = harmony.at(clock);
                // Late in the bar before a change: anticipate the next chord.
                let chord = if changing && h.step >= 12 { p.next } else { p.chord };
                let notes = self.voiced[chord].clone();
                let len = (h.len * self.six as f64) as u64;
                self.strike(clock, &notes, h.vel, len);
                if h.stutter {
                    let gap = (self.six - if h.step % 2 == 1 { self.push } else { 0 }) / 2;
                    self.pending = Some((clock + gap.max(1), notes, h.vel * 0.7, len / 2));
                }
            }
            if self.pending.as_ref().is_some_and(|p| p.0 == clock) {
                let (_, notes, vel, len) = self.pending.take().unwrap();
                self.strike(clock, &notes, vel, len);
            }
        }
        let (mut l, mut r) = (0.0, 0.0);
        let bright = self.tone == StabTone::Bright;
        for v in self.voices.iter_mut() {
            if !v.on {
                continue;
            }
            let gate = clock < v.release_at;
            v.env += if gate { (1.0 - v.env) * 0.06 } else { -v.env * 0.0016 };
            if !gate && v.env < 1e-4 {
                v.on = false;
                continue;
            }
            // The tone itself, decaying while held.
            let s = if bright {
                v.fenv *= 0.99955;
                let bell = sin_turns(v.bell_phase) * 0.35 * v.fenv * v.fenv;
                v.bell_phase += v.freq * 3.51 / sr;
                v.bell_phase -= v.bell_phase.floor();
                sin_turns(v.phase) * 0.7 + sin_turns(2.0 * v.phase) * 0.25 + bell
            } else {
                v.fenv *= 0.99985;
                let index = 0.25 + 1.4 * v.fenv * v.fenv;
                let m = sin_turns(v.mod_phase) * index / crate::math::TAU;
                v.mod_phase += v.freq / sr;
                v.mod_phase -= v.mod_phase.floor();
                sin_turns(v.phase + m)
            };
            v.phase += v.freq / sr;
            v.phase -= v.phase.floor();
            // A slow natural decay under the gate.
            let held = 0.55 + 0.45 * v.fenv;
            let x = s * v.env * held * v.vel;
            let (gl, gr) = crate::math::pan_gains(v.pan);
            l += x * gl;
            r += x * gr;
        }
        let (el, er) = self.echo.process(l * ECHO_MIX, r * ECHO_MIX);
        ((l + el) * LEVEL, (r + er) * LEVEL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voicings_are_close_sevenths_that_move_by_small_steps() {
        for scale in [crate::harmony::Scale::Ionian, crate::harmony::Scale::Dorian, crate::harmony::Scale::Aeolian] {
            for seed in 0..12 {
                let t = Tempo::new(132);
                let h = Harmony::garage(scale, 4, t.beat(), 2, &mut Rng::new(seed));
                let v = voicings(&h);
                for (i, chord) in v.iter().enumerate() {
                    assert_eq!(chord.len(), 4, "{scale:?}: a seventh chord has four notes");
                    assert!(chord.windows(2).all(|w| w[1] > w[0] && w[1] - w[0] <= 5.0), "{scale:?}: not close {chord:?}");
                    assert!(chord.iter().all(|m| (50.0..=76.0).contains(m)), "{chord:?}");
                    if i > 0 {
                        let moved: f64 = chord.iter().zip(&v[i - 1]).map(|(a, b)| (a - b).abs()).sum();
                        assert!(moved <= 10.0, "{scale:?} seed {seed}: chord {i} leaps ({moved} semitones)");
                    }
                }
            }
        }
    }

    #[test]
    fn the_answer_is_sparser_than_the_call_and_off_is_silent() {
        let t = Tempo::new(132);
        let s = Stabs::new(3, Density::Sparse, StabTone::Bright, t, 0.58);
        for bar in (0..64).step_by(2) {
            assert!(s.plan(bar, Section::TwoStep).len() > s.plan(bar + 1, Section::TwoStep).len());
        }
        let off = Stabs::new(3, Density::Off, StabTone::Bright, t, 0.58);
        assert!(off.plan(0, Section::TwoStep).is_empty());
    }
}
