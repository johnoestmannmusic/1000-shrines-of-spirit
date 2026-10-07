//! FM bass plus a pure sine sub-bass, through a low-pass.

use crate::filter::Svf;
use crate::fm::Fm2;
use crate::math::{midi_hz, sin_turns};
use crate::modulate::ModState;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

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
        let sr = SAMPLE_RATE as f64;
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
        let sr = SAMPLE_RATE as f64;
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
