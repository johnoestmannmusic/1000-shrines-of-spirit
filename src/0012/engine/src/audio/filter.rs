//! Filters.

use crate::math::{one_pole_coef, sanitize, tan_turns};

/// Topology-preserving-transform state-variable filter (Zavalishin / Simper).
#[derive(Clone, Default)]
pub struct Svf {
    k: f64,
    a1: f64,
    a2: f64,
    a3: f64,
    ic1: f64,
    ic2: f64,
}

pub struct SvfOut {
    pub low: f64,
    pub band: f64,
}

impl Svf {
    pub fn new(cutoff: f64, q: f64, sample_rate: f64) -> Self {
        let mut f = Svf::default();
        f.set(cutoff, q, sample_rate);
        f
    }

    pub fn set(&mut self, cutoff: f64, q: f64, sample_rate: f64) {
        let fc = cutoff.clamp(10.0, sample_rate * 0.45);
        let g = tan_turns(0.5 * fc / sample_rate);
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    pub fn process(&mut self, x: f64) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = sanitize(2.0 * v1 - self.ic1);
        self.ic2 = sanitize(2.0 * v2 - self.ic2);
        SvfOut { low: v2, band: v1 }
    }
}

/// One-pole low-pass.
#[derive(Clone, Default)]
pub struct OnePole {
    a: f64,
    y: f64,
}

impl OnePole {
    pub fn new(cutoff: f64, sample_rate: f64) -> Self {
        OnePole { a: one_pole_coef(cutoff, sample_rate), y: 0.0 }
    }

    pub fn process(&mut self, x: f64) -> f64 {
        self.y = sanitize(self.y + self.a * (x - self.y));
        self.y
    }
}

/// DC blocker.
#[derive(Clone, Default)]
pub struct DcBlock {
    x1: f64,
    y1: f64,
}

impl DcBlock {
    pub fn process(&mut self, x: f64) -> f64 {
        let y = sanitize(x - self.x1 + 0.9995 * self.y1);
        self.x1 = x;
        self.y1 = y;
        y
    }
}
