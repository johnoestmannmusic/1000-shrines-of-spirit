use crate::fft::{FreezeFft, FFT_SIZE, HOP_SIZE};
use crate::formant::{compute_spectral_envelope, reimpose_envelope, semitones_to_ratio, shift_envelope};
use crate::freeze::analyze_freeze_point;
use crate::resynth::{ola_accumulate, FreezeResynth};
use crate::stereo::{decorrelation_spread, width_multiplier};
use crate::window::sine_window;

pub const DEFAULT_ROOT_NOTE: u8 = 60;
pub const MIN_LOOP_SECONDS: f32 = 4.0;
pub const MAX_LOOP_SECONDS: f32 = 8.0;

pub struct LoopBufferData {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: f32,
    pub root_note: u8,
}

/// Renders a frozen, indefinitely-loopable STEREO buffer (always exactly 2
/// output channels, regardless of source channel count) from `channels`
/// (one Vec<f32> per source channel, all the same length) at `sample_rate`,
/// seeded from `freeze_point_pct` (0-100), shaped by
/// `formant_shift_semitones` (-12..12), and spread by `stereo_width_pct`
/// (0-100). Loop length is clamped 4-8 seconds based on the source sample's
/// own length (matching src/0006/index.html renderFreeze), constructed as
/// an integer number of hops so wrapping the loop introduces no seam
/// discontinuity beyond what OLA itself already produces internally.
///
/// Stereo width is baked in here rather than applied as a post-process on
/// the rendered audio, because a mid-side transform on already-rendered
/// audio can only ever *reveal* difference that already exists between
/// channels - it cannot create width when the source's L/R are identical or
/// highly correlated (this project's own test sample turned out to have
/// bit-identical L/R channels, which is what exposed this). At
/// `stereo_width_pct == 0` the right channel is built from the LEFT
/// channel's own frozen magnitude/phase/advance (not its own natural
/// analysis), guaranteeing a perfectly centered mono output regardless of
/// the source. As width increases toward 100%, the right channel's
/// magnitude blends toward its own naturally-analyzed content (revealing
/// any real per-channel difference the source has) AND a deterministic
/// per-bin phase offset (`decorrelation_spread`) is added on top, which
/// creates an audible stereo image even when the source has none at all.
pub fn render_frozen_loop(
    channels: &[Vec<f32>],
    sample_rate: f32,
    freeze_point_pct: f32,
    formant_shift_semitones: f32,
    stereo_width_pct: f32,
    root_note: u8,
) -> LoopBufferData {
    let fft = FreezeFft::new();
    let window = sine_window(FFT_SIZE);

    let source_left = channels.first().expect("render_frozen_loop requires at least one channel");
    let source_right = channels.get(1).unwrap_or(source_left);

    let loop_seconds = (source_left.len() as f32 / sample_rate).clamp(MIN_LOOP_SECONDS, MAX_LOOP_SECONDS);
    let num_hops = ((loop_seconds * sample_rate) / HOP_SIZE as f32).round().max(1.0) as usize;
    let out_len = num_hops * HOP_SIZE;

    let formant_ratio = semitones_to_ratio(formant_shift_semitones);
    let width = width_multiplier(stereo_width_pct);

    let frozen_left = analyze_freeze_point(source_left, freeze_point_pct, &fft);
    let frozen_right_natural = analyze_freeze_point(source_right, freeze_point_pct, &fft);

    let right_mag: Vec<f32> = frozen_left
        .mag
        .iter()
        .zip(frozen_right_natural.mag.iter())
        .map(|(&l, &r)| l + (r - l) * width)
        .collect();
    let right_phase0: Vec<f32> = frozen_left
        .phase0
        .iter()
        .enumerate()
        .map(|(k, &p)| p + width * decorrelation_spread(k))
        .collect();
    let right_advance = frozen_left.advance.clone();

    let render_channel = |mag: Vec<f32>, phase0: Vec<f32>, advance: Vec<f32>| -> Vec<f32> {
        let mag = if formant_shift_semitones != 0.0 {
            let env = compute_spectral_envelope(&mag, &fft);
            let shifted_env = shift_envelope(&env, formant_ratio);
            reimpose_envelope(&mag, &env, &shifted_env)
        } else {
            mag
        };

        let mut resynth = FreezeResynth::new(mag, phase0, advance, window.clone());
        let mut accum = vec![0.0f32; out_len + FFT_SIZE];
        let mut spectrum_scratch = fft.make_spectrum_buffer();
        let mut time_scratch = fft.make_time_buffer();

        let mut pos = 0usize;
        while pos < out_len {
            resynth.next_frame(&fft, &mut spectrum_scratch, &mut time_scratch);
            ola_accumulate(&mut accum, pos, &time_scratch);
            pos += HOP_SIZE;
        }

        accum.truncate(out_len);
        accum
    };

    let left_out = render_channel(frozen_left.mag, frozen_left.phase0, frozen_left.advance);
    let right_out = render_channel(right_mag, right_phase0, right_advance);

    LoopBufferData {
        channels: vec![left_out, right_out],
        sample_rate,
        root_note,
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
    fn loop_length_is_integer_number_of_hops() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 6.0) as usize);
        let result = render_frozen_loop(&[signal], sample_rate, 30.0, 0.0, 0.0, DEFAULT_ROOT_NOTE);
        let len = result.channels[0].len();
        assert_eq!(len % HOP_SIZE, 0, "loop length {} is not a multiple of HOP_SIZE", len);
    }

    #[test]
    fn loop_seconds_clamped_to_4_to_8_range() {
        let sample_rate = 48000.0;

        let short_signal = make_test_signal((sample_rate * 1.0) as usize);
        let short_result = render_frozen_loop(&[short_signal], sample_rate, 30.0, 0.0, 0.0, DEFAULT_ROOT_NOTE);
        let short_seconds = short_result.channels[0].len() as f32 / sample_rate;
        assert!(short_seconds >= MIN_LOOP_SECONDS - 0.1);

        let long_signal = make_test_signal((sample_rate * 20.0) as usize);
        let long_result = render_frozen_loop(&[long_signal], sample_rate, 30.0, 0.0, 0.0, DEFAULT_ROOT_NOTE);
        let long_seconds = long_result.channels[0].len() as f32 / sample_rate;
        assert!(long_seconds <= MAX_LOOP_SECONDS + 0.1);
    }

    #[test]
    fn deterministic_for_same_params() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 5.0) as usize);
        let a = render_frozen_loop(&[signal.clone()], sample_rate, 42.0, 2.0, 30.0, DEFAULT_ROOT_NOTE);
        let b = render_frozen_loop(&[signal], sample_rate, 42.0, 2.0, 30.0, DEFAULT_ROOT_NOTE);
        assert_eq!(a.channels[0].len(), b.channels[0].len());
        for (x, y) in a.channels[0].iter().zip(b.channels[0].iter()) {
            assert_eq!(x, y);
        }
        for (x, y) in a.channels[1].iter().zip(b.channels[1].iter()) {
            assert_eq!(x, y);
        }
    }

    #[test]
    fn always_outputs_exactly_two_channels() {
        let sample_rate = 48000.0;
        let mono_signal = make_test_signal((sample_rate * 5.0) as usize);
        let mono_result = render_frozen_loop(&[mono_signal], sample_rate, 30.0, 0.0, 40.0, DEFAULT_ROOT_NOTE);
        assert_eq!(mono_result.channels.len(), 2);
        assert_eq!(mono_result.channels[0].len(), mono_result.channels[1].len());

        let left = make_test_signal((sample_rate * 5.0) as usize);
        let right: Vec<f32> = left.iter().map(|s| s * 0.5).collect();
        let stereo_result = render_frozen_loop(&[left, right], sample_rate, 30.0, 0.0, 40.0, DEFAULT_ROOT_NOTE);
        assert_eq!(stereo_result.channels.len(), 2);
        assert_eq!(stereo_result.channels[0].len(), stereo_result.channels[1].len());
    }

    #[test]
    fn stereo_width_zero_centers_output_regardless_of_source_correlation() {
        let sample_rate = 48000.0;
        // A genuinely stereo source with a real difference between channels.
        let left = make_test_signal((sample_rate * 5.0) as usize);
        let right: Vec<f32> = left.iter().enumerate().map(|(i, s)| s * 0.3 + (i as f32 * 0.05).sin() * 0.2).collect();

        let result = render_frozen_loop(&[left, right], sample_rate, 30.0, 0.0, 0.0, DEFAULT_ROOT_NOTE);
        for (l, r) in result.channels[0].iter().zip(result.channels[1].iter()) {
            assert!((l - r).abs() < 1e-4, "width=0 must produce identical (centered) L/R even for a stereo source");
        }
    }

    #[test]
    fn stereo_width_hundred_differs_even_for_mono_source() {
        // The core bug fix: a mono source has zero natural L/R difference,
        // so a post-hoc mid-side transform could never create width. Baking
        // in a deterministic phase decorrelation must produce an audible
        // (non-trivial) difference between channels even here.
        let sample_rate = 48000.0;
        let mono_signal = make_test_signal((sample_rate * 5.0) as usize);
        let result = render_frozen_loop(&[mono_signal], sample_rate, 30.0, 0.0, 100.0, DEFAULT_ROOT_NOTE);

        let diff_energy: f32 = result.channels[0]
            .iter()
            .zip(result.channels[1].iter())
            .map(|(l, r)| (l - r).powi(2))
            .sum();
        let left_energy: f32 = result.channels[0].iter().map(|s| s.powi(2)).sum();
        assert!(
            diff_energy > left_energy * 0.01,
            "expected a meaningfully non-zero L/R difference from decorrelation alone, got diff_energy={} vs left_energy={}",
            diff_energy,
            left_energy
        );
    }

    #[test]
    fn stereo_width_is_monotonic_in_difference_for_mono_source() {
        let sample_rate = 48000.0;
        let mono_signal = make_test_signal((sample_rate * 5.0) as usize);

        let diff_energy_at = |width: f32| -> f32 {
            let result = render_frozen_loop(&[mono_signal.clone()], sample_rate, 30.0, 0.0, width, DEFAULT_ROOT_NOTE);
            result.channels[0]
                .iter()
                .zip(result.channels[1].iter())
                .map(|(l, r)| (l - r).powi(2))
                .sum()
        };

        let d0 = diff_energy_at(0.0);
        let d50 = diff_energy_at(50.0);
        let d100 = diff_energy_at(100.0);
        assert!(d0 < 1e-6, "width=0 should have ~zero difference, got {}", d0);
        assert!(d50 > d0, "width=50 ({}) should exceed width=0 ({})", d50, d0);
        assert!(d100 > d50, "width=100 ({}) should exceed width=50 ({})", d100, d50);
    }
}
