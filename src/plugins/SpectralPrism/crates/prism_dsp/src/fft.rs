use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const FFT_SIZE: usize = 2048;
pub const HOP_SIZE: usize = 1024;
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;

/// Wraps a pair of realfft R2C/C2R plans for a fixed FFT size, built once and
/// reused across every analysis/synthesis call. realfft handles real-signal
/// packing internally, exposing only the N/2+1 unique bins - which matches
/// this algorithm's mag/phase arrays directly and lets the same plans double
/// as the cepstral envelope transform in `formant.rs` (see its doc comment).
pub struct FreezeFft {
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
}

impl FreezeFft {
    pub fn new() -> Self {
        Self::with_size(FFT_SIZE)
    }

    pub fn with_size(size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(size);
        let c2r = planner.plan_fft_inverse(size);
        Self { r2c, c2r }
    }

    pub fn size(&self) -> usize {
        self.r2c.len()
    }

    pub fn num_bins(&self) -> usize {
        self.size() / 2 + 1
    }

    /// Forward real -> complex transform. `time_domain` may be used as
    /// scratch space by realfft; pass a copy if the original samples are
    /// still needed afterward.
    pub fn forward(&self, time_domain: &mut [f32], spectrum: &mut [Complex32]) {
        self.r2c
            .process(time_domain, spectrum)
            .expect("realfft forward transform failed (mismatched buffer sizes)");
    }

    /// Inverse complex -> real transform, normalized so that forward+inverse
    /// round-trips to the original signal (realfft/rustfft's raw inverse is
    /// unnormalized by convention - see fft::realfft_roundtrip below).
    pub fn inverse(&self, spectrum: &mut [Complex32], time_domain: &mut [f32]) {
        self.c2r
            .process(spectrum, time_domain)
            .expect("realfft inverse transform failed (mismatched buffer sizes)");
        let n = self.size() as f32;
        for sample in time_domain.iter_mut() {
            *sample /= n;
        }
    }

    pub fn make_time_buffer(&self) -> Vec<f32> {
        self.r2c.make_input_vec()
    }

    pub fn make_spectrum_buffer(&self) -> Vec<Complex32> {
        self.r2c.make_output_vec()
    }
}

impl Default for FreezeFft {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realfft_roundtrip() {
        let fft = FreezeFft::new();
        let mut rng_state: u32 = 12345;
        let mut next_f32 = move || {
            // xorshift, deterministic, no extra dependency needed
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 17;
            rng_state ^= rng_state << 5;
            (rng_state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };

        let original: Vec<f32> = (0..fft.size()).map(|_| next_f32()).collect();
        let mut time_domain = original.clone();
        let mut spectrum = fft.make_spectrum_buffer();
        fft.forward(&mut time_domain, &mut spectrum);

        let mut roundtripped = fft.make_time_buffer();
        fft.inverse(&mut spectrum, &mut roundtripped);

        for (a, b) in original.iter().zip(roundtripped.iter()) {
            assert!((a - b).abs() < 1e-4, "roundtrip mismatch: {} vs {}", a, b);
        }
    }

    #[test]
    fn num_bins_matches_fft_size() {
        let fft = FreezeFft::new();
        assert_eq!(fft.num_bins(), FFT_SIZE / 2 + 1);
        assert_eq!(fft.num_bins(), NUM_BINS);
    }
}
