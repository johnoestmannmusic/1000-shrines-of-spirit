//! Deterministic math.
//!
//! `f64::sin`, `exp`, `tanh`, `powf` etc. are delegated to the platform's libm,
//! whose results differ between operating systems, CPU architectures and
//! WASM engines, and may change between versions. Everything here is built
//! only from `+ - * /`, `floor`, `round` and `sqrt`, which IEEE-754 defines to
//! be exact (correctly rounded), so results are bit-identical everywhere.
//! Never use `mul_add` here (a fused multiply-add rounds differently).

pub const PI: f64 = core::f64::consts::PI;
pub const TAU: f64 = core::f64::consts::TAU;
const LN_2: f64 = core::f64::consts::LN_2;
const LOG2_E: f64 = core::f64::consts::LOG2_E;
const LOG2_10: f64 = core::f64::consts::LOG2_10;

/// Sine of `t` turns (1 turn = one full cycle = 2π radians).
pub fn sin_turns(t: f64) -> f64 {
    // Reduce to [-0.5, 0.5), then fold to [-0.25, 0.25] using sin(π - x) = sin(x).
    let mut x = t - (t + 0.5).floor();
    if x > 0.25 {
        x = 0.5 - x;
    } else if x < -0.25 {
        x = -0.5 - x;
    }
    let r = x * TAU; // [-π/2, π/2]
    let r2 = r * r;
    // Taylor series to r^19: truncation error < 3e-16 at ±π/2.
    const C: [f64; 9] = [
        -1.0 / 6.0,
        1.0 / 120.0,
        -1.0 / 5040.0,
        1.0 / 362880.0,
        -1.0 / 39916800.0,
        1.0 / 6227020800.0,
        -1.0 / 1307674368000.0,
        1.0 / 355687428096000.0,
        -1.0 / 121645100408832000.0,
    ];
    let mut p = C[8];
    let mut i = 8;
    while i > 0 {
        i -= 1;
        p = C[i] + r2 * p;
    }
    r * (1.0 + r2 * p)
}

/// Cosine of `t` turns.
pub fn cos_turns(t: f64) -> f64 {
    sin_turns(t + 0.25)
}

/// Tangent of `t` turns (only used well inside (-0.25, 0.25)).
pub fn tan_turns(t: f64) -> f64 {
    sin_turns(t) / cos_turns(t)
}

/// 2^x.
pub fn exp2(x: f64) -> f64 {
    if x < -1020.0 {
        return 0.0;
    }
    if x > 1020.0 {
        return f64::MAX;
    }
    let n = x.floor();
    let y = (x - n) * LN_2; // [0, ln 2)
    // e^y by Taylor series in Horner form, to y^14.
    let mut p = 1.0;
    let mut k = 14;
    while k >= 1 {
        p = 1.0 + p * y / k as f64;
        k -= 1;
    }
    p * f64::from_bits(((n as i64 + 1023) as u64) << 52)
}

/// e^x.
pub fn exp(x: f64) -> f64 {
    exp2(x * LOG2_E)
}

/// Hyperbolic tangent.
pub fn tanh(x: f64) -> f64 {
    if x > 19.0 {
        return 1.0;
    }
    if x < -19.0 {
        return -1.0;
    }
    let e = exp(2.0 * x);
    (e - 1.0) / (e + 1.0)
}

/// Decibels to linear gain.
pub fn db(db: f64) -> f64 {
    exp2(db * LOG2_10 / 20.0)
}

/// MIDI note number to Hz (A4 = 440).
pub fn midi_hz(note: f64) -> f64 {
    440.0 * exp2((note - 69.0) / 12.0)
}

/// Gain applied once every `samples` samples that decays by 60 dB over `seconds`.
pub fn decay_gain(samples: f64, seconds: f64, sample_rate: f64) -> f64 {
    exp2(-3.0 * LOG2_10 * samples / (seconds * sample_rate))
}

/// Per-sample multiplier that decays by 60 dB over `seconds`.
pub fn decay_coef(seconds: f64, sample_rate: f64) -> f64 {
    decay_gain(1.0, seconds, sample_rate)
}

/// Coefficient for a one-pole low-pass `y += a * (x - y)` at `hz`.
pub fn one_pole_coef(hz: f64, sample_rate: f64) -> f64 {
    1.0 - exp(-TAU * hz / sample_rate)
}

/// Equal-power pan: `pan` in [-1, 1] → (left gain, right gain).
pub fn pan_gains(pan: f64) -> (f64, f64) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.125; // [0, 0.25] turns
    (cos_turns(p), sin_turns(p))
}

/// Replace NaN/inf with 0 and flush very small values (keeps feedback paths clean).
pub fn sanitize(x: f64) -> f64 {
    if x.is_finite() && x.abs() > 1e-30 {
        x
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_match_std() {
        for i in -4000..4000 {
            let t = i as f64 * 0.00137;
            assert!((sin_turns(t) - (t * TAU).sin()).abs() < 1e-14, "sin {t}");
            assert!((cos_turns(t) - (t * TAU).cos()).abs() < 1e-14, "cos {t}");
        }
    }

    #[test]
    fn exp_tanh_match_std() {
        for i in -2000..2000 {
            let x = i as f64 * 0.013;
            let rel = (exp(x) - x.exp()).abs() / x.exp();
            assert!(rel < 1e-13, "exp {x}: {rel}");
            assert!((tanh(x) - x.tanh()).abs() < 1e-12, "tanh {x}");
        }
        assert_eq!(exp2(3.0), 8.0);
        assert!((midi_hz(69.0) - 440.0).abs() < 1e-12);
    }
}
