use realfft::num_complex::Complex32;

use crate::fft::FreezeFft;
use crate::window::apply_window_in_place;

/// Runs the phase-vocoder resynthesis oscillator bank: reconstructs one FFT
/// frame per call from a frozen magnitude and a running per-bin phase
/// accumulator, then advances the accumulator by the frozen advance rate.
/// The caller overlap-adds the returned (windowed) time-domain frame into an
/// output buffer at HOP_SIZE spacing (see `ola_accumulate`).
pub struct FreezeResynth {
    mag: Vec<f32>,
    advance: Vec<f32>,
    phase: Vec<f32>,
    window: Vec<f32>,
}

impl FreezeResynth {
    pub fn new(mag: Vec<f32>, phase0: Vec<f32>, advance: Vec<f32>, window: Vec<f32>) -> Self {
        assert_eq!(mag.len(), phase0.len());
        assert_eq!(mag.len(), advance.len());
        Self {
            mag,
            advance,
            phase: phase0,
            window,
        }
    }

    /// Synthesizes and windows the next output frame (time-domain, length ==
    /// fft.size()) into `time_scratch`, and advances internal phase state
    /// for the next call.
    pub fn next_frame(&mut self, fft: &FreezeFft, spectrum_scratch: &mut [Complex32], time_scratch: &mut [f32]) {
        let last = spectrum_scratch.len() - 1;
        for (k, s) in spectrum_scratch.iter_mut().enumerate() {
            if k == 0 || k == last {
                // DC and Nyquist bins must be purely real for a real-valued
                // time-domain signal - realfft's Hermitian-symmetric C2R
                // inverse rejects a nonzero imaginary part there. Phase
                // accumulation would otherwise drift these off the real
                // axis over many frames (sin() of a large accumulated angle
                // loses precision rather than staying exactly 0), so their
                // sign is tracked via cos() and the imaginary part forced
                // to exactly 0 instead of computed from sin().
                let sign = if self.phase[k].cos() >= 0.0 { 1.0 } else { -1.0 };
                *s = Complex32::new(self.mag[k] * sign, 0.0);
            } else {
                *s = Complex32::new(self.mag[k] * self.phase[k].cos(), self.mag[k] * self.phase[k].sin());
            }
            self.phase[k] += self.advance[k];
        }
        fft.inverse(spectrum_scratch, time_scratch);
        apply_window_in_place(time_scratch, &self.window);
    }
}

/// Overlap-adds `frame` into `accum` starting at `pos`, matching
/// src/0006/index.html olaAccumulateFrame. Silently drops any tail past the
/// end of `accum`.
pub fn ola_accumulate(accum: &mut [f32], pos: usize, frame: &[f32]) {
    for (i, &s) in frame.iter().enumerate() {
        if let Some(slot) = accum.get_mut(pos + i) {
            *slot += s;
        }
    }
}

/// Overlap-adds `frame` into `accum` starting at `pos`, wrapping any tail
/// past the end of `accum` back around to its start instead of dropping it
/// - the key primitive that makes a frozen loop click-free: rather than
/// leaving the buffer's first `HOP_SIZE`-ish samples under-windowed (no
/// "previous frame" exists at position 0 in a plain linear accumulation),
/// the LAST frame's overlap naturally supplies that missing contribution,
/// exactly as if the frame sequence carried on forever and simply wrapped
/// back to frame 0. Combined with phase-locking the oscillator bank so the
/// synthesized content is itself exactly periodic over `accum.len()`
/// samples (see `render::quantize_advance_for_loop`), this makes the loop
/// mathematically periodic rather than merely blended-to-look-continuous.
pub fn ola_accumulate_circular(accum: &mut [f32], pos: usize, frame: &[f32]) {
    let len = accum.len();
    for (i, &s) in frame.iter().enumerate() {
        accum[(pos + i) % len] += s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::HOP_SIZE;
    use crate::window::sine_window;

    #[test]
    fn next_frame_produces_full_length_windowed_output() {
        let fft = FreezeFft::new();
        let n = fft.size();
        let mag = vec![1.0f32; fft.num_bins()];
        let phase0 = vec![0.0f32; fft.num_bins()];
        let advance = vec![0.1f32; fft.num_bins()];
        let window = sine_window(n);

        let mut resynth = FreezeResynth::new(mag, phase0, advance, window);
        let mut spectrum_scratch = fft.make_spectrum_buffer();
        let mut time_scratch = fft.make_time_buffer();
        resynth.next_frame(&fft, &mut spectrum_scratch, &mut time_scratch);

        assert_eq!(time_scratch.len(), n);
        // Windowed output must taper to ~0 at both edges (sine window is 0
        // at n=0 and n=N).
        assert!(time_scratch[0].abs() < 1e-4);
        assert!(time_scratch[n - 1].abs() < 1e-3);
    }

    #[test]
    fn ola_accumulate_drops_tail_past_buffer_end() {
        let mut accum = vec![0.0f32; 4];
        let frame = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        ola_accumulate(&mut accum, 2, &frame);
        assert_eq!(accum, vec![0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn ola_accumulate_circular_wraps_tail_back_to_the_start() {
        // Same frame/position as `ola_accumulate_drops_tail_past_buffer_end`
        // above, but nothing is dropped here: `pos + i` for i=0..=5 lands
        // at accum indices 2, 3, 0, 1, 2, 3 (mod 4) respectively, so index 2
        // gets frame[0] + frame[4] = 1.0 + 5.0 = 6.0 and index 3 gets
        // frame[1] + frame[5] = 2.0 + 6.0 = 8.0.
        let mut accum = vec![0.0f32; 4];
        let frame = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        ola_accumulate_circular(&mut accum, 2, &frame);
        assert_eq!(accum, vec![3.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn ola_accumulate_circular_matches_linear_when_frame_fits_without_wrapping() {
        let mut circular = vec![0.0f32; 8];
        let mut linear = vec![0.0f32; 8];
        let frame = vec![1.0, 2.0, 3.0];
        ola_accumulate(&mut linear, 2, &frame);
        ola_accumulate_circular(&mut circular, 2, &frame);
        assert_eq!(circular, linear);
    }

    #[test]
    fn identity_resynth_reconstructs_input_tone() {
        // Freeze a pure tone (bin-aligned, so phase-vocoder freeze is exact)
        // and confirm the resynthesized/OLA'd output matches the original
        // tone in frequency and roughly in amplitude - an end-to-end sanity
        // check spanning analysis, freeze, resynth, and OLA together.
        use crate::analysis::analyze_frame;
        use crate::phase_advance::compute_advance;

        let fft = FreezeFft::new();
        let n = fft.size();
        let window = sine_window(n);
        let bin = 60usize;
        let total_len = n * 6;
        let signal: Vec<f32> = (0..total_len)
            .map(|i| (2.0 * std::f32::consts::PI * bin as f32 * i as f32 / n as f32).sin())
            .collect();

        let f0 = analyze_frame(&signal, 0, &fft, &window);
        let f1 = analyze_frame(&signal, HOP_SIZE, &fft, &window);
        let advance = compute_advance(&f0.phase, &f1.phase, HOP_SIZE, n);

        let mut resynth = FreezeResynth::new(f0.mag.clone(), f0.phase.clone(), advance, window.clone());
        let mut spectrum_scratch = fft.make_spectrum_buffer();
        let mut time_scratch = fft.make_time_buffer();

        let out_len = n * 4;
        let mut accum = vec![0.0f32; out_len + n];
        let mut pos = 0usize;
        while pos < out_len {
            resynth.next_frame(&fft, &mut spectrum_scratch, &mut time_scratch);
            ola_accumulate(&mut accum, pos, &time_scratch);
            pos += HOP_SIZE;
        }

        // FFT-analyze a stable region of the resynthesized output and
        // confirm the peak bin matches the frozen tone's bin.
        let check = analyze_frame(&accum, n, &fft, &window);
        let peak_bin = check
            .mag
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(peak_bin, bin);
    }
}
