//! Stereo ping-pong echo with a darkening feedback loop.

use crate::filter::OnePole;
use crate::math::sanitize;
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
