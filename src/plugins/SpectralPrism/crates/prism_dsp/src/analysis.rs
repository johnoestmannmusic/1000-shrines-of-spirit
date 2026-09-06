use crate::fft::FreezeFft;
use crate::window::apply_window_in_place;

pub struct FrameAnalysis {
    pub mag: Vec<f32>,
    pub phase: Vec<f32>,
}

/// Extracts windowed magnitude/phase for the FFT-sized frame of `signal`
/// starting at `pos`. Zero-pads if the frame would run past the end of
/// `signal`, matching the source's tolerance for seed positions near the
/// end (src/0006/index.html fusionFramePhaseAndMag, lines 3353-3364).
pub fn analyze_frame(signal: &[f32], pos: usize, fft: &FreezeFft, window: &[f32]) -> FrameAnalysis {
    let n = fft.size();
    let mut frame = vec![0.0f32; n];
    for i in 0..n {
        if let Some(&s) = signal.get(pos + i) {
            frame[i] = s;
        }
    }
    apply_window_in_place(&mut frame, window);

    let mut spectrum = fft.make_spectrum_buffer();
    fft.forward(&mut frame, &mut spectrum);

    let mag: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();
    let phase: Vec<f32> = spectrum.iter().map(|c| c.arg()).collect();
    FrameAnalysis { mag, phase }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::sine_window;

    #[test]
    fn analyze_pure_tone_peaks_at_expected_bin() {
        let fft = FreezeFft::new();
        let window = sine_window(fft.size());
        let n = fft.size();
        let bin = 100;
        let freq_hz_per_bin_cycle = bin as f32; // exactly bin-aligned tone over N samples
        let signal: Vec<f32> = (0..n * 2)
            .map(|i| (2.0 * std::f32::consts::PI * freq_hz_per_bin_cycle * i as f32 / n as f32).sin())
            .collect();

        let result = analyze_frame(&signal, 0, &fft, &window);
        let peak_bin = result
            .mag
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(peak_bin, bin);
    }
}
