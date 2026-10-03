//! Stereo ping-pong echo with a darkening feedback loop.

use crate::filter::OnePole;
use crate::math::{sanitize, sin_turns, tanh};
use crate::SAMPLE_RATE;

pub struct PingPong {
    left: Vec<f64>,
    right: Vec<f64>,
    pos: usize,
    delay: usize,
    feedback: f64,
    damp_l: OnePole,
    damp_r: OnePole,
}

impl PingPong {
    pub fn new(delay: usize, feedback: f64) -> Self {
        let len = delay + 1;
        PingPong {
            left: vec![0.0; len],
            right: vec![0.0; len],
            pos: 0,
            delay,
            feedback,
            damp_l: OnePole::new(3200.0, SAMPLE_RATE as f64),
            damp_r: OnePole::new(3200.0, SAMPLE_RATE as f64),
        }
    }

    /// (delay in samples, feedback).
    pub fn settings(&self) -> (usize, f64) {
        (self.delay, self.feedback)
    }

    /// Input enters on the left and bounces right, left, right...
    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let len = self.left.len();
        let read = (self.pos + len - self.delay) % len;
        let out_l = self.damp_l.process(self.left[read]);
        let out_r = self.damp_r.process(self.right[read]);
        self.left[self.pos] = sanitize(0.5 * (in_l + in_r) + self.feedback * out_r);
        self.right[self.pos] = sanitize(self.feedback * out_l);
        self.pos = (self.pos + 1) % len;
        (out_l, out_r)
    }
}

/// A ping-pong tape echo for the drums' snare-type hits: the delay time
/// wobbles (wow and flutter), and every repeat is saturated, darkened and
/// thinned in the feedback loop, like a tape loop.
pub struct TapeEcho {
    left: Vec<f64>,
    right: Vec<f64>,
    pos: usize,
    delay: f64,
    feedback: f64,
    lp: [OnePole; 2],
    hp: [OnePole; 2],
}

/// Wow (~0.5 Hz) and flutter (~6 Hz) periods, in samples, and their depths.
const WOW_PERIOD: u64 = 96_000;
const FLUTTER_PERIOD: u64 = 8_000;
const WOW_DEPTH: f64 = 0.003;
const FLUTTER_DEPTH: f64 = 0.0005;

impl TapeEcho {
    pub fn new(delay: usize, feedback: f64) -> Self {
        let len = (delay as f64 * 1.02) as usize + 8;
        let sr = SAMPLE_RATE as f64;
        TapeEcho {
            left: vec![0.0; len],
            right: vec![0.0; len],
            pos: 0,
            delay: delay as f64,
            feedback,
            lp: [OnePole::new(2500.0, sr), OnePole::new(2500.0, sr)],
            hp: [OnePole::new(150.0, sr), OnePole::new(150.0, sr)],
        }
    }

    /// (delay in samples, feedback)
    pub fn settings(&self) -> (f64, f64) {
        (self.delay, self.feedback)
    }

    fn read(buf: &[f64], pos: usize, d: f64) -> f64 {
        // `d` is always shorter than the buffer, so one wrap is enough.
        let mut p = pos as f64 - d;
        if p < 0.0 {
            p += buf.len() as f64;
        }
        let i = p.floor();
        let f = p - i;
        let i = i as usize;
        let a = buf[i % buf.len()];
        let b = buf[(i + 1) % buf.len()];
        a + (b - a) * f
    }

    /// Mono input enters on the left, then bounces right, left, right…
    /// `clock` drives the wow and flutter, so output never depends on block size.
    pub fn process(&mut self, clock: u64, input: f64) -> (f64, f64) {
        let wow = sin_turns((clock % WOW_PERIOD) as f64 / WOW_PERIOD as f64);
        let flutter = sin_turns((clock % FLUTTER_PERIOD) as f64 / FLUTTER_PERIOD as f64);
        let d = self.delay * (1.0 + WOW_DEPTH * wow + FLUTTER_DEPTH * flutter);
        let out_l = Self::read(&self.left, self.pos, d);
        let out_r = Self::read(&self.right, self.pos, d);
        // Tape colour in the loop: soft saturation, darker, thinner.
        let shape = |x: f64, lp: &mut OnePole, hp: &mut OnePole| {
            let x = lp.process(tanh(x * 1.3) / 1.3);
            x - hp.process(x)
        };
        let fb_l = shape(out_l * self.feedback, &mut self.lp[0], &mut self.hp[0]);
        let fb_r = shape(out_r * self.feedback, &mut self.lp[1], &mut self.hp[1]);
        self.left[self.pos] = sanitize(input + fb_r);
        self.right[self.pos] = sanitize(fb_l);
        self.pos = (self.pos + 1) % self.left.len();
        (out_l, out_r)
    }
}
