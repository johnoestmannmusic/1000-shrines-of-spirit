//! Microsound's grain cloud: the sustained bed (the atmosphere loops and the
//! drone) is listened to, and scattered back as grains — 20–80 ms slices of
//! the last second and a half, each under a smooth window, now and then an
//! octave up or down, every one at its own place in the field. Dense, it
//! becomes a shimmering cloud; sparse, a few motes of the bed.
//!
//! The next grain's time, length, source and pan are drawn from a stream
//! keyed by the grain's own number and placed on the exact sample, so the
//! cloud never depends on how the audio is cut into blocks.

use crate::math::{pan_gains, sin_turns};
use crate::rng::Rng;

/// The listening buffer: 1.5 s.
const HISTORY: usize = 72_000;
const VOICES: usize = 24;

#[derive(Clone, Copy, Default)]
struct Grain {
    /// Read position in the history (fractional), and its step per sample.
    read: f64,
    step: f64,
    pos: u64,
    len: u64,
    gl: f64,
    gr: f64,
    on: bool,
}

pub struct Cloud {
    seed: u64,
    hist_l: Vec<f64>,
    hist_r: Vec<f64>,
    write: usize,
    /// Samples written so far (the clock the grains are placed on).
    n: u64,
    /// When the next grain starts, and its number.
    next_at: u64,
    count: u64,
    /// Mean gap between grains, in samples.
    gap: f64,
    voices: [Grain; VOICES],
}

impl Cloud {
    /// `amount` 0 (sparse) .. 1 (dense).
    pub fn new(seed: u64, amount: f64) -> Self {
        let gap = 0.12 - 0.1 * amount.clamp(0.0, 1.0);
        Cloud {
            seed,
            hist_l: vec![0.0; HISTORY],
            hist_r: vec![0.0; HISTORY],
            write: 0,
            n: 0,
            next_at: 4_800,
            count: 0,
            gap: gap * crate::SAMPLE_RATE as f64,
            voices: [Grain::default(); VOICES],
        }
    }

    fn read(buf: &[f64], at: f64) -> f64 {
        let i = at.floor();
        let f = at - i;
        let i = (i as usize) % HISTORY;
        let j = (i + 1) % HISTORY;
        buf[i] + (buf[j] - buf[i]) * f
    }

    /// Listen to one stereo sample of the bed, and return the cloud's.
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        self.hist_l[self.write] = l;
        self.hist_r[self.write] = r;
        if self.n == self.next_at {
            let mut rng = Rng::stream(self.seed, 0x6A41_0000 ^ self.count);
            self.count += 1;
            let len = (rng.range(0.02, 0.08) * sr) as u64;
            let step = match rng.weighted(&[0.7, 0.15, 0.15]) {
                0 => 1.0,
                1 => 2.0,
                _ => 0.5,
            };
            // Far enough back that even a doubled-speed grain never reaches
            // the write head.
            let back = rng.range(0.1, 1.3) * sr + len as f64 * step;
            let read = (self.write as f64 + HISTORY as f64 - back).rem_euclid(HISTORY as f64);
            let (gl, gr) = pan_gains(rng.range(-0.95, 0.95));
            let free = self.voices.iter().position(|g| !g.on);
            if let Some(k) = free {
                self.voices[k] = Grain { read, step, pos: 0, len, gl, gr, on: true };
            }
            self.next_at = self.n + 1 + (self.gap * rng.range(0.3, 1.7)) as u64;
        }
        let (mut ol, mut or) = (0.0, 0.0);
        for g in self.voices.iter_mut() {
            if !g.on {
                continue;
            }
            if g.pos >= g.len {
                g.on = false;
                continue;
            }
            let w = sin_turns(0.5 * g.pos as f64 / g.len as f64);
            let w = w * w;
            // The grain is heard from both channels of the bed, summed, then placed.
            let s = 0.5 * (Self::read(&self.hist_l, g.read) + Self::read(&self.hist_r, g.read)) * w;
            g.read = (g.read + g.step).rem_euclid(HISTORY as f64);
            g.pos += 1;
            ol += s * g.gl;
            or += s * g.gr;
        }
        self.write = (self.write + 1) % HISTORY;
        self.n += 1;
        (ol, or)
    }
}

/// Microsound's fragment gate: a sustained sound (the drone, an atmosphere
/// loop) heard only in short pieces, in a steady rhythm locked to the bar:
/// on for a fixed number of sixteenths, off for a fixed number, the pattern
/// always fitting a bar or two exactly so every fragment starts where the
/// groove expects it. The seed picks the pattern from `patterns` and, for a
/// pattern shorter than its turn, which beat it starts on. Opening takes
/// 3 ms and closing 25 ms, so a fragment is cut cleanly rather than clicked.
/// The gate is a pure function of the sample clock, so it never depends on
/// how the audio is cut into blocks.
pub struct Gate {
    six: u64,
    /// Sixteenths on, then off.
    on: u64,
    off: u64,
    /// Where in its cycle the pattern starts, in sixteenths.
    shift: u64,
    env: f64,
}

const GATE_ATTACK: f64 = 1.0 / 144.0;
const GATE_RELEASE: f64 = 1.0 / 1200.0;

impl Gate {
    /// `patterns`: (on, off) in sixteenths; each should add up to 16 or 32.
    pub fn new(seed: u64, stream: u64, sixteenth: u64, patterns: &[(u64, u64)]) -> Self {
        let mut rng = Rng::stream(seed, stream);
        let (on, off) = rng.pick(patterns);
        // Start on a beat of the cycle (layers that share a pattern then
        // interlock instead of doubling each other).
        let beats = ((on + off) / 4).max(1);
        let shift = 4 * rng.below(beats as usize) as u64;
        Gate { six: sixteenth.max(1), on: on.max(1), off, shift, env: 0.0 }
    }

    /// (on, off) in sixteenths, for display.
    pub fn pattern(&self) -> (u64, u64) {
        (self.on, self.off)
    }

    /// Which fragment `clock` falls in (counting from the first).
    pub fn fragment(&self, clock: u64) -> u64 {
        let period = (self.on + self.off) * self.six;
        (clock + period - (self.shift * self.six) % period) / period
    }

    /// The gain at `clock` (call once per sample, in order).
    pub fn next(&mut self, clock: u64) -> f64 {
        let period = (self.on + self.off) * self.six;
        let phase = (clock + period - (self.shift * self.six) % period) % period;
        if phase < self.on * self.six {
            self.env = (self.env + GATE_ATTACK).min(1.0);
        } else {
            self.env = (self.env - GATE_RELEASE).max(0.0);
        }
        // A smooth curve over the linear ramp.
        self.env * self.env * (3.0 - 2.0 * self.env)
    }
}

/// A plain delay-line pitch shifter: two read heads sweep through a 50 ms
/// window at the shifted speed, each faded in and out so one is always
/// smooth while the other jumps back. On a still, frozen drone it is clean;
/// at no shift it passes the sound through untouched.
pub struct Shifter {
    buf: [Vec<f64>; 2],
    write: usize,
    phase: f64,
    ratio: f64,
}

const SHIFT_BUF: usize = 4096;
const SHIFT_WINDOW: f64 = 2400.0;

impl Shifter {
    pub fn new() -> Self {
        Shifter { buf: [vec![0.0; SHIFT_BUF], vec![0.0; SHIFT_BUF]], write: 0, phase: 0.0, ratio: 1.0 }
    }

    /// Transpose by `semis` from now on.
    pub fn set(&mut self, semis: f64) {
        self.ratio = crate::math::exp2(semis / 12.0);
    }

    fn tap(&self, ch: usize, delay: f64) -> f64 {
        let at = self.write as f64 + SHIFT_BUF as f64 - delay;
        let i = at.floor();
        let f = at - i;
        let i = i as usize % SHIFT_BUF;
        let b = &self.buf[ch];
        b[i] + (b[(i + 1) % SHIFT_BUF] - b[i]) * f
    }

    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.buf[0][self.write] = l;
        self.buf[1][self.write] = r;
        let out = if self.ratio == 1.0 {
            (l, r)
        } else {
            let p1 = self.phase;
            let p2 = (p1 + 0.5) % 1.0;
            let (d1, d2) = (1.0 + p1 * SHIFT_WINDOW, 1.0 + p2 * SHIFT_WINDOW);
            // sin² windows half a cycle apart always sum to one.
            let g1 = sin_turns(0.5 * p1);
            let g2 = sin_turns(0.5 * p2);
            let (g1, g2) = (g1 * g1, g2 * g2);
            (self.tap(0, d1) * g1 + self.tap(0, d2) * g2, self.tap(1, d1) * g1 + self.tap(1, d2) * g2)
        };
        // A higher pitch reads faster than it writes, so the delay shrinks.
        self.phase = (self.phase + (1.0 - self.ratio) / SHIFT_WINDOW).rem_euclid(1.0);
        self.write = (self.write + 1) % SHIFT_BUF;
        out
    }
}

impl Default for Shifter {
    fn default() -> Self {
        Shifter::new()
    }
}
