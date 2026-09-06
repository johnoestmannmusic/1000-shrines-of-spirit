use realfft::num_complex::Complex32;

use crate::fft::FreezeFft;

pub const LIFTER_CUTOFF: usize = 50;

/// Cepstral spectral-envelope extraction: log-magnitude -> inverse transform
/// -> real cepstrum -> keep only low-quefrency components -> forward
/// transform -> exp. Mirrors src/0006/index.html computeSpectralEnvelope
/// (lines 3256-3266), but reuses the same realfft R2C/C2R plans as the main
/// analysis FFT: log-magnitude is a real-valued "spectrum" (zero imaginary
/// part), so realfft's Hermitian-symmetric C2R inverse reconstructs the real
/// cepstrum directly, with no separate complex-FFT dependency or manual
/// spectrum-mirroring code needed.
pub fn compute_spectral_envelope(mag: &[f32], fft: &FreezeFft) -> Vec<f32> {
    let n = fft.size();
    let num_bins = fft.num_bins();
    assert_eq!(mag.len(), num_bins);

    let mut log_mag_spectrum: Vec<Complex32> = mag
        .iter()
        .map(|&m| Complex32::new(m.max(1e-8).ln(), 0.0))
        .collect();
    let mut cepstrum = vec![0.0f32; n];
    fft.inverse(&mut log_mag_spectrum, &mut cepstrum);

    // Lifter: keep only low-quefrency components at both ends of the
    // symmetric real cepstrum, zero the rest.
    for c in cepstrum.iter_mut().take(n - LIFTER_CUTOFF).skip(LIFTER_CUTOFF) {
        *c = 0.0;
    }

    let mut envelope_spectrum = fft.make_spectrum_buffer();
    fft.forward(&mut cepstrum, &mut envelope_spectrum);

    envelope_spectrum.iter().map(|c| c.re.exp()).collect()
}

/// Shifts an envelope along frequency by `ratio = 2^(semitones/12)` via
/// linear interpolation: shifted[k] = env[k/ratio]. ratio == 1.0 is a no-op.
pub fn shift_envelope(env: &[f32], ratio: f32) -> Vec<f32> {
    if ratio == 1.0 {
        return env.to_vec();
    }
    let n = env.len();
    (0..n)
        .map(|k| {
            let src = k as f32 / ratio;
            if src <= 0.0 {
                env[0]
            } else if src >= (n - 1) as f32 {
                env[n - 1]
            } else {
                let lo = src.floor() as usize;
                let hi = lo + 1;
                let frac = src - lo as f32;
                env[lo] * (1.0 - frac) + env[hi] * frac
            }
        })
        .collect()
}

/// Reimposes a (possibly shifted) spectral envelope onto a magnitude
/// spectrum while preserving fine structure: magOut = (mag/env) * shiftedEnv.
pub fn reimpose_envelope(mag: &[f32], env: &[f32], shifted_env: &[f32]) -> Vec<f32> {
    assert_eq!(mag.len(), env.len());
    assert_eq!(mag.len(), shifted_env.len());
    mag.iter()
        .zip(env.iter())
        .zip(shifted_env.iter())
        .map(|((&m, &e), &se)| (m / e.max(1e-8)) * se)
        .collect()
}

pub fn semitones_to_ratio(semitones: f32) -> f32 {
    2f32.powf(semitones / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::FreezeFft;

    fn synthetic_formant_spectrum(fft: &FreezeFft, peak_bin: usize) -> Vec<f32> {
        let num_bins = fft.num_bins();
        (0..num_bins)
            .map(|k| {
                let d = (k as f32 - peak_bin as f32) / 20.0;
                (-d * d).exp() + 0.01
            })
            .collect()
    }

    #[test]
    fn envelope_peak_near_expected_bin() {
        let fft = FreezeFft::new();
        let peak_bin = 200;
        let mag = synthetic_formant_spectrum(&fft, peak_bin);
        let env = compute_spectral_envelope(&mag, &fft);

        let found_peak = env
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            (found_peak as i64 - peak_bin as i64).abs() <= 10,
            "envelope peak at {} not near expected {}",
            found_peak,
            peak_bin
        );
    }

    #[test]
    fn shift_envelope_moves_peak_by_ratio() {
        let fft = FreezeFft::new();
        let peak_bin = 150;
        let mag = synthetic_formant_spectrum(&fft, peak_bin);
        let env = compute_spectral_envelope(&mag, &fft);

        let ratio = 1.5;
        let shifted = shift_envelope(&env, ratio);
        let shifted_peak = shifted
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        let expected_peak = (peak_bin as f32 * ratio).round() as i64;
        assert!(
            (shifted_peak as i64 - expected_peak).abs() <= 10,
            "shifted peak at {} not near expected {}",
            shifted_peak,
            expected_peak
        );
    }

    #[test]
    fn shift_envelope_ratio_one_is_identity() {
        let fft = FreezeFft::new();
        let mag = synthetic_formant_spectrum(&fft, 100);
        let env = compute_spectral_envelope(&mag, &fft);
        let shifted = shift_envelope(&env, 1.0);
        assert_eq!(env, shifted);
    }

    #[test]
    fn reimpose_identity_at_ratio_one() {
        let fft = FreezeFft::new();
        let mag = synthetic_formant_spectrum(&fft, 120);
        let env = compute_spectral_envelope(&mag, &fft);
        let shifted = shift_envelope(&env, 1.0);
        let out = reimpose_envelope(&mag, &env, &shifted);
        for (a, b) in out.iter().zip(mag.iter()) {
            assert!((a - b).abs() < 1e-2, "{} vs {}", a, b);
        }
    }

    #[test]
    fn semitones_to_ratio_matches_equal_temperament() {
        assert!((semitones_to_ratio(0.0) - 1.0).abs() < 1e-6);
        assert!((semitones_to_ratio(12.0) - 2.0).abs() < 1e-4);
        assert!((semitones_to_ratio(-12.0) - 0.5).abs() < 1e-4);
    }
}
