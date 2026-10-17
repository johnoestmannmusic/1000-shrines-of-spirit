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
