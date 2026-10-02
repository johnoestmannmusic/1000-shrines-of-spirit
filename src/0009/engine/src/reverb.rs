//! Long reverb: input diffusers into an 8-line feedback delay network with a
//! Hadamard mixing matrix and damping in the loop.

use crate::filter::OnePole;
use crate::math::{decay_gain, sanitize};
use crate::SAMPLE_RATE;

/// Time for the tail to fall by 60 dB.
const DECAY_SECONDS: f64 = 18.0;
const DAMP_HZ: f64 = 5200.0;
/// Keeps the low end out of the long tail so it doesn't pile up into mud.
const INPUT_HIGHPASS_HZ: f64 = 180.0;
const LINES: [usize; 8] = [3049, 3779, 4513, 5153, 5869, 6577, 7351, 8101];
const DIFFUSERS: [[usize; 4]; 2] = [[142, 379, 107, 277], [151, 353, 113, 263]];

struct Allpass {
    buf: Vec<f64>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass { buf: vec![0.0; len], pos: 0 }
    }

    fn process(&mut self, x: f64) -> f64 {
        const G: f64 = 0.62;
        let delayed = self.buf[self.pos];
        let y = delayed - G * x;
        self.buf[self.pos] = sanitize(x + G * y);
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

struct Line {
    buf: Vec<f64>,
    pos: usize,
    gain: f64,
    damp: OnePole,
}

pub struct Reverb {
    lines: Vec<Line>,
    diffusers: [Vec<Allpass>; 2],
    highpass: [OnePole; 2],
}

impl Reverb {
    pub fn new() -> Self {
        let sr = SAMPLE_RATE as f64;
        Reverb {
            lines: LINES
                .iter()
                .map(|&len| Line {
                    buf: vec![0.0; len],
                    pos: 0,
                    gain: decay_gain(len as f64, DECAY_SECONDS, sr),
                    damp: OnePole::new(DAMP_HZ, sr),
                })
                .collect(),
            diffusers: DIFFUSERS.map(|lens| lens.iter().map(|&l| Allpass::new(l)).collect()),
            highpass: [OnePole::new(INPUT_HIGHPASS_HZ, sr), OnePole::new(INPUT_HIGHPASS_HZ, sr)],
        }
    }

    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let mut l = in_l - self.highpass[0].process(in_l);
        let mut r = in_r - self.highpass[1].process(in_r);
        for ap in self.diffusers[0].iter_mut() {
            l = ap.process(l);
        }
        for ap in self.diffusers[1].iter_mut() {
            r = ap.process(r);
        }

        let mut v = [0.0; 8];
        for (x, line) in v.iter_mut().zip(self.lines.iter_mut()) {
            *x = line.damp.process(line.buf[line.pos]);
        }
        let out_l = 0.5 * (v[0] - v[2] + v[4] - v[6]);
        let out_r = 0.5 * (v[1] - v[3] + v[5] - v[7]);

        // Fast Walsh-Hadamard transform, normalised to stay energy-preserving.
        let mut h = 1;
        while h < 8 {
            let mut i = 0;
            while i < 8 {
                for j in i..i + h {
                    let (a, b) = (v[j], v[j + h]);
                    v[j] = a + b;
                    v[j + h] = a - b;
                }
                i += h * 2;
            }
            h *= 2;
        }
        let norm = 1.0 / 8f64.sqrt();
        for (i, line) in self.lines.iter_mut().enumerate() {
            let input = if i % 2 == 0 { l } else { r };
            line.buf[line.pos] = sanitize(input + v[i] * norm * line.gain);
            line.pos = (line.pos + 1) % line.buf.len();
        }
        (out_l, out_r)
    }
}
