//! In-place iterative radix-2 complex FFT, twiddles from the deterministic `math`.

use crate::math::{cos_turns, sin_turns};

pub struct Fft {
    n: usize,
    cos: Vec<f64>,
    sin: Vec<f64>,
    rev: Vec<u32>,
}

impl Fft {
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let cos = (0..n / 2).map(|k| cos_turns(k as f64 / n as f64)).collect();
        let sin = (0..n / 2).map(|k| sin_turns(k as f64 / n as f64)).collect();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        Fft { n, cos, sin, rev }
    }

    /// Forward: X[k] = Σ x[n]·e^(-2πikn/N). Inverse is unscaled (no 1/N).
    pub fn transform(&self, re: &mut [f64], im: &mut [f64], inverse: bool) {
        self.bit_reverse(re, im);
        for s in 0..self.stages() {
            self.stage(re, im, s, inverse);
        }
    }

    /// Number of butterfly stages (log2 N).
    pub fn stages(&self) -> u32 {
        self.n.trailing_zeros()
    }

    /// The transform's first step. `transform` = `bit_reverse` then every
    /// `stage` in order; exposed separately so the work can be spread out.
    pub fn bit_reverse(&self, re: &mut [f64], im: &mut [f64]) {
        assert!(re.len() == self.n && im.len() == self.n);
        for i in 0..self.n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
    }

    /// Butterfly stage `s` (0-based; combines blocks of length 2^(s+1)).
    pub fn stage(&self, re: &mut [f64], im: &mut [f64], s: u32, inverse: bool) {
        let n = self.n;
        let sign = if inverse { 1.0 } else { -1.0 };
        let len = 2 << s;
        let half = len / 2;
        let step = n / len;
        let mut start = 0;
        while start < n {
            for j in 0..half {
                let wr = self.cos[j * step];
                let wi = sign * self.sin[j * step];
                let a = start + j;
                let b = a + half;
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
            start += len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    #[test]
    fn round_trip() {
        let n = 1024;
        let fft = Fft::new(n);
        let mut rng = Rng::new(7);
        let orig: Vec<f64> = (0..n).map(|_| rng.bipolar()).collect();
        let mut re = orig.clone();
        let mut im = vec![0.0; n];
        fft.transform(&mut re, &mut im, false);
        fft.transform(&mut re, &mut im, true);
        for i in 0..n {
            assert!((re[i] / n as f64 - orig[i]).abs() < 1e-12);
            assert!((im[i] / n as f64).abs() < 1e-12);
        }
    }

    #[test]
    fn single_bin() {
        let n = 64;
        let fft = Fft::new(n);
        let mut re: Vec<f64> = (0..n).map(|i| cos_turns(5.0 * i as f64 / n as f64)).collect();
        let mut im = vec![0.0; n];
        fft.transform(&mut re, &mut im, false);
        assert!((re[5] - n as f64 / 2.0).abs() < 1e-9);
        assert!((re[n - 5] - n as f64 / 2.0).abs() < 1e-9);
        assert!(re[6].abs() < 1e-9);
    }
}
