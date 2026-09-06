use crate::analysis::analyze_frame;
use crate::fft::{FreezeFft, HOP_SIZE};
use crate::phase_advance::compute_advance;
use crate::window::sine_window;

pub struct FrozenSpectrum {
    pub mag: Vec<f32>,
    pub phase0: Vec<f32>,
    pub advance: Vec<f32>,
}

/// Analyzes the seed frame at `freeze_point_pct` (0-100) of `signal`'s
/// usable length and derives a frozen magnitude + per-bin phase-advance
/// description, matching src/0006/index.html renderFreeze (lines 3378-3394):
/// magnitude is frozen outright (the seed frame's `mag`), phase keeps
/// advancing per-bin at the measured true rate between the seed frame and
/// the next hop's frame.
pub fn analyze_freeze_point(signal: &[f32], freeze_point_pct: f32, fft: &FreezeFft) -> FrozenSpectrum {
    let n = fft.size();
    let window = sine_window(n);
    let len = signal.len();
    let max_pos = len.saturating_sub(n);
    let pct = freeze_point_pct.clamp(0.0, 100.0) / 100.0;
    let pos0 = ((pct * max_pos as f32).round() as usize).min(max_pos);
    let pos1 = (pos0 + HOP_SIZE).min(max_pos);

    let f0 = analyze_frame(signal, pos0, fft, &window);
    let f1 = analyze_frame(signal, pos1, fft, &window);

    let advance = compute_advance(&f0.phase, &f1.phase, HOP_SIZE, n);

    FrozenSpectrum {
        mag: f0.mag,
        phase0: f0.phase,
        advance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_signal(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32;
                0.5 * (t * 0.02).sin() + 0.3 * (t * 0.007).sin() + 0.05 * (t * 1.3).sin()
            })
            .collect()
    }

    #[test]
    fn magnitude_frozen_across_frames() {
        // The FrozenSpectrum's `mag` IS the magnitude every resynthesized
        // frame will reuse forever - this test locks down that it is
        // derived from exactly the seed frame (pos0), not some blend with
        // pos1 or a running average.
        let fft = FreezeFft::new();
        let signal = make_test_signal(fft.size() * 8);

        let frozen = analyze_freeze_point(&signal, 25.0, &fft);
        let window = sine_window(fft.size());
        let expected_f0 = analyze_frame(&signal, {
            let max_pos = signal.len() - fft.size();
            ((0.25 * max_pos as f32).round() as usize).min(max_pos)
        }, &fft, &window);

        for (a, b) in frozen.mag.iter().zip(expected_f0.mag.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn freeze_point_clamped_to_valid_range() {
        let fft = FreezeFft::new();
        let signal = make_test_signal(fft.size() * 4);

        // Should not panic and should behave like 0/100 respectively.
        let low = analyze_freeze_point(&signal, -50.0, &fft);
        let clamped_low = analyze_freeze_point(&signal, 0.0, &fft);
        for (a, b) in low.mag.iter().zip(clamped_low.mag.iter()) {
            assert!((a - b).abs() < 1e-4);
        }

        let high = analyze_freeze_point(&signal, 500.0, &fft);
        let clamped_high = analyze_freeze_point(&signal, 100.0, &fft);
        for (a, b) in high.mag.iter().zip(clamped_high.mag.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
    }
}
