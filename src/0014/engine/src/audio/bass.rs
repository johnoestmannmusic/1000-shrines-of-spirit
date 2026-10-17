//! FM bass plus a pure sine sub-bass, through a low-pass.

use crate::filter::Svf;
use crate::fm::Fm2;
use crate::math::{midi_hz, sin_turns};
use crate::modulate::ModState;
use crate::rng::Rng;

/// D2: the default root (it follows each chord's root via `set_root`).
pub const ROOT: f64 = 38.0;
const RATIOS: [f64; 4] = [0.5, 1.0, 2.0, 3.0];
const FM_LEVEL: f64 = 0.22;
const SUB_LEVEL: f64 = 0.26;

pub struct Bass {
    fm: Fm2,
    sub_phase: f64,
    sub_freq: f64,
    index_lo: f64,
    index_hi: f64,
    lp: Svf,
    /// Latest modulation index and low-pass cutoff (for display).
    index: f64,
    cutoff: f64,
    /// FM detune against the sub, in semitones.
    beat: f64,
    root: f64,
}

impl Bass {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, 0xBA55);
        let sr = crate::SAMPLE_RATE as f64;
        // A hair of detune between the FM bass and the sub makes them beat slowly.
        let beat = rng.range(-1.5, 1.5) / 100.0;
        Bass {
            fm: Fm2::new(midi_hz(ROOT + beat), rng.pick(&RATIOS)),
            sub_phase: 0.0,
            sub_freq: midi_hz(ROOT - 12.0),
            index_lo: rng.range(0.1, 0.5),
            index_hi: rng.range(0.9, 2.4),
            lp: Svf::new(300.0, 0.7, sr),
            index: 0.0,
            cutoff: 300.0,
            beat,
            root: ROOT,
        }
    }

    /// Moves the bass to `midi` (called with a gliding value during chord changes).
    pub fn set_root(&mut self, midi: f64) {
        if midi != self.root {
            self.root = midi;
            self.fm.freq = midi_hz(midi + self.beat);
            self.sub_freq = midi_hz(midi - 12.0);
        }
    }

    pub fn root(&self) -> f64 {
        self.root
    }

    /// (FM ratio, modulation index, low-pass cutoff Hz).
    pub fn state(&self) -> (f64, f64, f64) {
        (self.fm.ratio, self.index, self.cutoff)
    }

    pub fn next(&mut self, clock: u64, m: &ModState) -> f64 {
        let sr = crate::SAMPLE_RATE as f64;
        if clock % 32 == 0 {
            self.cutoff = 140.0 + 360.0 * m.bass_bright;
            self.lp.set(self.cutoff, 0.9, sr);
        }
        let index = self.index_lo + (self.index_hi - self.index_lo) * m.bass_bright;
        self.index = index;
        let fm = self.fm.next(sr, index);
        let sub = sin_turns(self.sub_phase);
        self.sub_phase += self.sub_freq / sr;
        self.sub_phase -= self.sub_phase.floor();
        let filtered = self.lp.process(fm * FM_LEVEL).low;
        (filtered + sub * SUB_LEVEL) * m.bass_swell
    }
}

/// One note of the 2-step bassline, within its bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// The sixteenth it starts on (0..16).
    pub step: u8,
    /// How many sixteenths it holds for.
    pub len: u8,
    /// Semitones above the chord's root (an approach note: above the *next* root).
    pub semis: f64,
    pub vel: f64,
    /// Leading into the next chord rather than sitting on this one.
    pub approach: bool,
}

/// Where a garage bass note may start, and how likely it is: the pushes
/// around the kick rather than on it. Index = sixteenth in the bar.
const ONSETS: [f64; 16] = [0.55, 0.0, 0.3, 0.6, 0.0, 0.1, 0.55, 0.4, 0.2, 0.0, 0.5, 0.45, 0.0, 0.45, 0.55, 0.3];

/// The 2-step garage bassline: a plucked sub with a little FM bite, moving
/// in syncopated sixteenths on the chord roots, octaves and fifths, swung
/// with the drums, gliding between notes and leading into each change with
/// an approach note. Every bar's notes are a pure function of seed 4 and
/// the bar number: a two-bar figure, varied at the end of each phrase, that
/// grows a new shape every sixteen bars.
pub struct BassLine {
    seed: u64,
    six: u64,
    bar: u64,
    push: u64,
    fm: Fm2,
    lp: Svf,
    sub_phase: f64,
    freq: f64,
    target: f64,
    env: f64,
    fenv: f64,
    vel: f64,
    /// When the sounding note lets go (absolute samples).
    release_at: u64,
    plan: Option<(u64, Vec<Note>)>,
    cutoff: f64,
    midi: f64,
}

impl BassLine {
    pub fn new(seed: u64, tempo: crate::tempo::Tempo, swing: f64) -> Self {
        let mut rng = Rng::stream(seed, 0xBA56);
        let six = tempo.drum16;
        let sr = crate::SAMPLE_RATE as f64;
        BassLine {
            seed,
            six,
            bar: 16 * six,
            push: ((2.0 * swing - 1.0) * six as f64) as u64,
            fm: Fm2::new(midi_hz(ROOT), rng.pick(&[1.0, 2.0])),
            lp: Svf::new(400.0, 0.8, sr),
            sub_phase: 0.0,
            freq: midi_hz(ROOT),
            target: midi_hz(ROOT),
            env: 0.0,
            fenv: 0.0,
            vel: 0.0,
            release_at: 0,
            plan: None,
            cutoff: 400.0,
            midi: ROOT,
        }
    }

    /// The notes of bar `bar` (pure). `changing` marks a bar that leads into
    /// a new chord: its last note becomes an approach.
    pub fn plan(&self, bar: u64, changing: bool) -> Vec<Note> {
        let figure = bar % 2;
        let vary = bar % 4 == 3;
        let shape = bar / 16;
        let mut rng = Rng::stream(self.seed, 0xBA56_0000 ^ (shape << 8) ^ (figure << 4) ^ vary as u64);
        let mut steps: Vec<u8> = Vec::new();
        for (i, w) in ONSETS.iter().enumerate() {
            // Never two in a row; the variation bar is a little busier.
            let w = if vary { (w * 1.25).min(0.9) } else { *w };
            if steps.last().is_some_and(|l| *l as usize + 1 == i) {
                continue;
            }
            if rng.chance(w) {
                steps.push(i as u8);
            }
        }
        if steps.len() < 2 {
            steps = vec![0, rng.pick(&[6u8, 10, 14])];
        }
        let mut notes = Vec::with_capacity(steps.len());
        for (k, &step) in steps.iter().enumerate() {
            let next = steps.get(k + 1).copied().unwrap_or(16);
            let room = next - step;
            // Mostly held to the next note, now and then a short stab.
            let len = if rng.chance(0.3) { 1 } else { room.min(4) };
            let semis = if k == 0 {
                0.0
            } else {
                match rng.weighted(&[0.5, 0.28, 0.17, 0.05]) {
                    0 => 0.0,
                    1 => 12.0,
                    2 => 7.0,
                    _ => -5.0,
                }
            };
            let vel = if semis == 12.0 { rng.range(0.6, 0.75) } else { rng.range(0.8, 1.0) };
            notes.push(Note { step, len, semis, vel, approach: false });
        }
        if changing {
            // Walk into the next chord: a semitone under its root on the
            // last push of the bar.
            let last = notes.last().map_or(0, |n| n.step);
            if last < 14 {
                notes.push(Note { step: 14, len: 2, semis: -1.0, vel: 0.8, approach: true });
            } else if let Some(n) = notes.last_mut() {
                *n = Note { semis: -1.0, approach: true, ..*n };
            }
        }
        notes
    }

    /// Where a step's note begins within the bar (odd steps swing late).
    fn onset(&self, step: u8) -> u64 {
        step as u64 * self.six + if step % 2 == 1 { self.push } else { 0 }
    }

    /// The current note's pitch (MIDI), for display.
    pub fn pitch(&self) -> f64 {
        self.midi
    }

    /// One sample. `root` and `next_root` are this chord's and the next one's
    /// bass notes; `changing` is true in the bar before a change; `active`
    /// whether the groove is playing (the bass follows the drums' sections).
    pub fn next(&mut self, clock: u64, root: f64, next_root: f64, fifth: f64, changing: bool, active: bool, m: &ModState) -> f64 {
        let sr = crate::SAMPLE_RATE as f64;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        if self.plan.as_ref().is_none_or(|(b, _)| *b != bar) {
            self.plan = Some((bar, self.plan(bar, changing)));
        }
        if active {
            let notes = &self.plan.as_ref().unwrap().1;
            if let Some(n) = notes.iter().find(|n| self.onset(n.step) == in_bar) {
                let base = if n.approach { next_root } else { root };
                // The figure's fifths are this chord's own fifth.
                let semis = match n.semis {
                    s if s == 7.0 => fifth,
                    s if s == -5.0 => fifth - 12.0,
                    s => s,
                };
                // Keep the line in the sub register.
                let mut midi = base + semis;
                while midi > 50.0 {
                    midi -= 12.0;
                }
                self.target = midi_hz(midi);
                self.midi = midi;
                self.vel = n.vel;
                self.fenv = 1.0;
                let hold = (n.len as f64 - 0.25).max(0.6) * self.six as f64;
                self.release_at = clock + hold as u64;
            }
        }
        // A quick glide (about 7 ms), so even a pushed note is at pitch on
        // its beat; then the amplitude and filter envelopes.
        self.freq += (self.target - self.freq) * 0.003;
        let gate = active && clock < self.release_at;
        let goal = if gate { 1.0 } else { 0.0 };
        let rate = if gate { 0.02 } else { 0.0009 };
        self.env += (goal - self.env) * rate;
        self.fenv *= 0.99975;
        if clock % 32 == 0 {
            self.cutoff = 150.0 + 330.0 * m.bass_bright + 1300.0 * self.fenv;
            self.lp.set(self.cutoff, 0.85, sr);
        }
        self.fm.freq = self.freq;
        let fm = self.fm.next(sr, 0.3 + 1.8 * self.fenv);
        let sub = sin_turns(self.sub_phase);
        self.sub_phase += self.freq / sr;
        self.sub_phase -= self.sub_phase.floor();
        let body = self.lp.process(fm * 0.3).low;
        (sub * 0.3 + body * 0.85) * self.env * self.vel
    }
}
