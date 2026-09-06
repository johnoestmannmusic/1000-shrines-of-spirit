/// Classic-sampler playback rate for `note` relative to `root_note`, in
/// equal temperament: rate = 2^((note - root_note) / 12).
pub fn playback_rate(note: u8, root_note: u8) -> f64 {
    2f64.powf((note as f64 - root_note as f64) / 12.0)
}

/// Naive linear-interpolation whole-buffer resample from `from_rate` to
/// `to_rate` samples/sec (no anti-aliasing filter, so downsampling by a
/// large factor can alias - acceptable for bringing an imported sample to
/// the plugin's operating rate, not a mastering-grade resampler). Without
/// this, a loaded sample whose native rate differs from the host's would
/// play back pitch/speed-shifted, since `VoiceManager` reads the frozen
/// loop assuming it's already at the plugin's operating rate.
pub fn resample_linear(source: &[f32], from_rate: f32, to_rate: f32) -> Vec<f32> {
    if source.is_empty() || from_rate <= 0.0 || to_rate <= 0.0 {
        return source.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((source.len() as f64 / ratio).round() as usize).max(1);
    let last = source.len() - 1;
    (0..out_len)
        .map(|i| {
            let pos = (i as f64 * ratio).min(last as f64);
            let i0 = pos.floor() as usize;
            let i1 = (i0 + 1).min(last);
            let frac = (pos - pos.floor()) as f32;
            source[i0] * (1.0 - frac) + source[i1] * frac
        })
        .collect()
}

/// Reads `left`/`right` at an arbitrary fractional `pos` with linear
/// interpolation, wrapping around the buffer length. Doesn't touch any
/// `PlaybackReader` state - used both by `read_stereo_and_advance` below and
/// by `VoiceManager`'s buffer-swap crossfade, which needs to sample an
/// *outgoing* buffer at the same position a `PlaybackReader` is using for
/// its current (incoming) one.
pub fn sample_stereo_at(left: &[f32], right: &[f32], pos: f64) -> (f32, f32) {
    let len = left.len();
    debug_assert_eq!(len, right.len());
    if len == 0 {
        return (0.0, 0.0);
    }
    let pos = pos.rem_euclid(len as f64);
    let i0 = pos.floor() as usize % len;
    let i1 = (i0 + 1) % len;
    let frac = (pos - pos.floor()) as f32;
    let l = left[i0] * (1.0 - frac) + left[i1] * frac;
    let r = right[i0] * (1.0 - frac) + right[i1] * frac;
    (l, r)
}

/// Reads a loop buffer at a fractional position with linear interpolation,
/// wrapping around the loop length so playback never reads out of bounds
/// even if the underlying buffer is swapped out for a differently-sized one
/// between calls.
pub struct PlaybackReader {
    pub read_pos: f64,
}

impl PlaybackReader {
    pub fn new(start_pos: f64) -> Self {
        Self { read_pos: start_pos }
    }

    pub fn read_and_advance(&mut self, buffer: &[f32], rate: f64) -> f32 {
        let len = buffer.len();
        if len == 0 {
            return 0.0;
        }
        let pos = self.read_pos.rem_euclid(len as f64);
        let i0 = pos.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = (pos - pos.floor()) as f32;
        let sample = buffer[i0] * (1.0 - frac) + buffer[i1] * frac;
        self.read_pos = (pos + rate).rem_euclid(len as f64);
        sample
    }

    /// Reads `left`/`right` at the same fractional position/rate in lockstep
    /// (a single shared `read_pos`, not two independent readers) so the two
    /// channels never drift apart sample-to-sample.
    pub fn read_stereo_and_advance(&mut self, left: &[f32], right: &[f32], rate: f64) -> (f32, f32) {
        let len = left.len();
        debug_assert_eq!(len, right.len());
        if len == 0 {
            return (0.0, 0.0);
        }
        let pos = self.read_pos.rem_euclid(len as f64);
        let sample = sample_stereo_at(left, right, pos);
        self.read_pos = (pos + rate).rem_euclid(len as f64);
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_linear_preserves_duration_in_seconds() {
        let from_rate = 44100.0;
        let to_rate = 48000.0;
        let source = vec![0.0f32; 44100]; // 1 second at from_rate
        let resampled = resample_linear(&source, from_rate, to_rate);
        let duration_seconds = resampled.len() as f32 / to_rate;
        assert!(
            (duration_seconds - 1.0).abs() < 0.001,
            "expected ~1.0s at the new rate, got {}s ({} samples)",
            duration_seconds,
            resampled.len()
        );
    }

    #[test]
    fn resample_linear_preserves_cycle_count() {
        // A known tone resampled to a different rate should still contain
        // roughly the same number of cycles - i.e. playing it back at the
        // new rate reproduces the same pitch, not a shifted one.
        let from_rate = 44100.0;
        let to_rate = 48000.0;
        let cycles = 100.0f32;
        let len = from_rate as usize;
        let source: Vec<f32> =
            (0..len).map(|i| (2.0 * std::f32::consts::PI * cycles * i as f32 / from_rate).sin()).collect();
        let resampled = resample_linear(&source, from_rate, to_rate);

        let count_crossings = |sig: &[f32]| -> usize {
            sig.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count()
        };
        let ratio = count_crossings(&resampled) as f32 / count_crossings(&source) as f32;
        assert!((ratio - 1.0).abs() < 0.02, "expected ~the same number of cycles after resampling, got ratio {}", ratio);
    }

    #[test]
    fn pitch_ratio_formula() {
        let root = 60u8;
        let cases: [(i32, f64); 5] = [(-12, 0.5), (-7, 0.6674), (0, 1.0), (7, 1.4983), (12, 2.0)];
        for (offset, expected) in cases {
            let note = (root as i32 + offset) as u8;
            let rate = playback_rate(note, root);
            assert!(
                (rate - expected).abs() < 1e-3,
                "playback_rate({}, {}) = {}, expected ~{}",
                note,
                root,
                rate,
                expected
            );
        }
    }

    #[test]
    fn resampled_frequency_matches_ratio() {
        // Build a loop buffer containing a tone at a known frequency (in
        // cycles per buffer length), read it back through the resampler at
        // a known rate, and confirm the *effective* number of cycles
        // reproduced over one full read of the original buffer length
        // scales by that rate (i.e. reading `rate` samples advances the
        // phase `rate` times faster).
        let len = 4096usize;
        let cycles = 8.0f32;
        let buffer: Vec<f32> = (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * cycles * i as f32 / len as f32).sin())
            .collect();

        let rate = 1.5f64;
        let mut reader = PlaybackReader::new(0.0);
        let read_len = (len as f64 / rate) as usize;
        let mut output = Vec::with_capacity(read_len);
        for _ in 0..read_len {
            output.push(reader.read_and_advance(&buffer, rate));
        }

        // Count zero crossings in input vs output: output should have
        // roughly the same number of crossings (same number of cycles
        // squeezed into a shorter buffer at a proportionally faster rate).
        let count_crossings = |sig: &[f32]| -> usize {
            sig.windows(2)
                .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                .count()
        };
        let input_crossings = count_crossings(&buffer);
        let output_crossings = count_crossings(&output);
        let ratio = output_crossings as f32 / input_crossings as f32;
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "expected output to contain ~the same number of cycles as input read at {}x rate over a proportionally shorter span, got ratio {}",
            rate,
            ratio
        );
    }

    #[test]
    fn read_and_advance_wraps_around_loop() {
        let buffer = vec![1.0, 2.0, 3.0, 4.0];
        let mut reader = PlaybackReader::new(3.5);
        // Starting past the last sample index should wrap, not panic or
        // read out of bounds.
        let sample = reader.read_and_advance(&buffer, 1.0);
        assert!(sample.is_finite());
    }

    #[test]
    fn read_stereo_and_advance_stays_locked_step() {
        let left = vec![1.0, 2.0, 3.0, 4.0];
        let right = vec![10.0, 20.0, 30.0, 40.0];
        let mut reader = PlaybackReader::new(0.0);
        for _ in 0..8 {
            let (l, r) = reader.read_stereo_and_advance(&left, &right, 1.0);
            // right is always exactly 10x left at every sample index in
            // this fixture, so the two channels must read it back in
            // lockstep (same fractional position) every time.
            assert!((r - l * 10.0).abs() < 1e-4, "l={} r={} should satisfy r == 10*l", l, r);
        }
    }
}
