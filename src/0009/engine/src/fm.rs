//! Phase-modulation ("FM") operators.

use crate::math::{sin_turns, TAU};

/// How three operators are wired. Operator 1 is always the carrier.
#[derive(Clone, Copy)]
pub enum Algo {
    /// 3 → 2 → 1
    Stack,
    /// 2 → 1 ← 3
    Pair,
}

#[derive(Clone, Copy)]
pub struct Op {
    pub ratio: f64,
    /// Modulation depth in radians of the operator it feeds.
    pub index: f64,
    phase: f64,
}

impl Op {
    pub fn new(ratio: f64, index: f64, phase: f64) -> Self {
        Op { ratio, index, phase }
    }

    fn advance(&mut self, freq: f64, sample_rate: f64) {
        self.phase += freq * self.ratio / sample_rate;
        self.phase -= self.phase.floor();
    }
}

/// Three-operator FM voice with self-feedback on operator 3.
pub struct Fm3 {
    pub freq: f64,
    pub ops: [Op; 3],
    pub algo: Algo,
    /// Operator-3 self-feedback in radians.
    pub feedback: f64,
    last3: f64,
}

impl Fm3 {
    pub fn new(freq: f64, algo: Algo, ops: [Op; 3], feedback: f64) -> Self {
        Fm3 { freq, ops, algo, feedback, last3: 0.0 }
    }

    /// One sample in [-1, 1]. `index_scale` scales both modulator depths.
    pub fn next(&mut self, sample_rate: f64, index_scale: f64) -> f64 {
        let [o1, o2, o3] = &self.ops;
        let s3 = sin_turns(o3.phase + self.feedback * self.last3 / TAU);
        self.last3 = s3;
        let m3 = s3 * o3.index * index_scale / TAU;
        let out = match self.algo {
            Algo::Stack => {
                let m2 = sin_turns(o2.phase + m3) * o2.index * index_scale / TAU;
                sin_turns(o1.phase + m2)
            }
            Algo::Pair => {
                let m2 = sin_turns(o2.phase) * o2.index * index_scale / TAU;
                sin_turns(o1.phase + m2 + m3)
            }
        };
        for op in self.ops.iter_mut() {
            op.advance(self.freq, sample_rate);
        }
        out
    }
}

/// Two-operator FM voice (modulator → carrier).
pub struct Fm2 {
    pub freq: f64,
    pub ratio: f64,
    carrier: f64,
    modulator: f64,
}

impl Fm2 {
    pub fn new(freq: f64, ratio: f64) -> Self {
        Fm2 { freq, ratio, carrier: 0.0, modulator: 0.0 }
    }

    pub fn next(&mut self, sample_rate: f64, index: f64) -> f64 {
        let m = sin_turns(self.modulator) * index / TAU;
        let out = sin_turns(self.carrier + m);
        self.carrier += self.freq / sample_rate;
        self.carrier -= self.carrier.floor();
        self.modulator += self.freq * self.ratio / sample_rate;
        self.modulator -= self.modulator.floor();
        out
    }
}
