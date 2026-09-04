/// Sine window: w[n] = sin(pi*n/N). Applied at BOTH analysis and synthesis;
/// double application equals a periodic Hann window, which reconstructs at
/// unity overlap-add gain at 50% hop with no extra normalization (see
/// `cola_unity_gain` below, and src/0006/index.html lines 3175-3183).
pub fn sine_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|n| (std::f32::consts::PI * n as f32 / size as f32).sin())
        .collect()
}

pub fn apply_window_in_place(frame: &mut [f32], window: &[f32]) {
    assert_eq!(frame.len(), window.len());
    for (s, w) in frame.iter_mut().zip(window.iter()) {
        *s *= w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::HOP_SIZE;

    /// Classic constant-overlap-add check: windowing a signal at analysis
    /// time, then windowing again and overlap-adding at synthesis time with
    /// no other processing (identity freeze), should reconstruct the
    /// original signal at unity gain - directly verifying the source
    /// comment's claim that a double sine window needs no extra
    /// normalization at 50% hop.
    #[test]
    fn cola_unity_gain() {
        let n = 2048;
        let window = sine_window(n);

        // Use a slowly-varying signal so windowed+summed frames actually
        // reconstruct it (a single frame near the very start/end of a short
        // signal would be zero-padded and break the identity check).
        let total_len = n * 4;
        let signal: Vec<f32> = (0..total_len)
            .map(|i| (i as f32 * 0.001).sin() * 0.5)
            .collect();

        let mut accum = vec![0.0f32; total_len];
        let mut pos = 0usize;
        while pos + n <= total_len {
            let mut frame = signal[pos..pos + n].to_vec();
            apply_window_in_place(&mut frame, &window);
            apply_window_in_place(&mut frame, &window);
            for i in 0..n {
                accum[pos + i] += frame[i];
            }
            pos += HOP_SIZE;
        }

        // Only check the region covered by full overlap (away from the
        // start/end edges, where boundary effects are expected).
        let margin = n;
        for i in margin..(total_len - margin) {
            assert!(
                (accum[i] - signal[i]).abs() < 1e-3,
                "COLA mismatch at {}: {} vs {}",
                i,
                accum[i],
                signal[i]
            );
        }
    }
}
