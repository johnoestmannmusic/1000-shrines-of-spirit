use crate::fft::{FreezeFft, FFT_SIZE, HOP_SIZE};
use crate::formant::{compute_spectral_envelope, reimpose_envelope, semitones_to_ratio, shift_envelope};
use crate::freeze::analyze_freeze_point;
use crate::resynth::{ola_accumulate_circular, FreezeResynth};
use crate::stereo::{decorrelation_spread, width_multiplier};
use crate::window::sine_window;
use std::f32::consts::TAU;

pub const DEFAULT_ROOT_NOTE: u8 = 60;
/// Absolute safety bounds `loop_length_seconds` is clamped to - just sane
/// limits on the user-facing "Loop Length" param: long enough that a single
/// loop doesn't feel obviously short/repetitive by default, short enough to
/// cap memory/export size at the other end. The seam itself is made
/// click-free at any length by construction (see `render_frozen_loop`'s doc
/// comment on phase-locking + circular OLA), not by picking a "safe"
/// length or blending the seam after the fact.
pub const MIN_LOOP_SECONDS: f32 = 0.5;
pub const MAX_LOOP_SECONDS: f32 = 8.0;
/// Preserves the old (pre-`FREEZE-PLAN-024`) auto-computed minimum as the
/// new param's default, so a freshly-created instance (or a preset saved
/// before this param existed) starts at a length already known to sound
/// reasonable rather than defaulting to the new, much shorter floor.
pub const DEFAULT_LOOP_SECONDS: f32 = 4.0;

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
/// (0-100). `loop_length_seconds` is user-chosen (clamped to
/// `[MIN_LOOP_SECONDS, MAX_LOOP_SECONDS]` as a sanity bound only) and
/// constructed as an integer number of hops (`num_hops`).
///
/// Making the wraparound itself click-free (not just an integer hop count)
/// takes two things working together, since the frozen resynthesis keeps
/// advancing each bin's phase hop after hop indefinitely:
/// 1. **Phase-locking** (`quantize_advance_for_loop`): each bin's per-hop
///    phase advance is snapped to the nearest value that completes a whole
///    number of cycles over exactly `num_hops` hops. That makes the
///    oscillator bank's phase state after `num_hops` hops identical to its
///    state at hop 0 - the synthesized signal is now genuinely periodic
///    with period `out_len`, not just "long enough that repetition is hard
///    to notice."
/// 2. **Circular OLA** (`ola_accumulate_circular`): the last frame's tail,
///    which would normally spill past the end of the buffer and get
///    dropped, is wrapped back around to the start instead - exactly what
///    would happen if the (now-periodic) frame sequence simply carried on
///    forever. Without this, the first ~`HOP_SIZE` samples would be
///    under-windowed (no "previous frame" to overlap with), which is its
///    own source of a seam even with perfectly phase-locked content.
///
/// Together these make the loop mathematically periodic rather than merely
/// smoothed at the join - no time-domain crossfade/blend is applied (an
/// earlier attempt at that masked the click's magnitude but introduced its
/// own audible comb-filtering/phasing from summing two out-of-phase
/// snapshots of the same oscillators, which is why this replaced it rather
/// than being layered on top of it).
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
    loop_length_seconds: f32,
    root_note: u8,
) -> LoopBufferData {
    let fft = FreezeFft::new();
    let window = sine_window(FFT_SIZE);

    let source_left = channels.first().expect("render_frozen_loop requires at least one channel");
    let source_right = channels.get(1).unwrap_or(source_left);

    let loop_seconds = loop_length_seconds.clamp(MIN_LOOP_SECONDS, MAX_LOOP_SECONDS);
    let num_hops = ((loop_seconds * sample_rate) / HOP_SIZE as f32).round().max(1.0) as usize;
    let out_len = num_hops * HOP_SIZE;

    let formant_ratio = semitones_to_ratio(formant_shift_semitones);
    let width = width_multiplier(stereo_width_pct);

    let frozen_left = analyze_freeze_point(source_left, freeze_point_pct, &fft);
    let frozen_right_natural = analyze_freeze_point(source_right, freeze_point_pct, &fft);

    // Quantized once and shared by both channels (matching how the
    // un-quantized `advance` was already shared before this) - phase0
    // differs per channel (the right channel's decorrelation offset is
    // just a constant added before accumulation begins) but that doesn't
    // affect periodicity: any starting phase returns to itself after
    // `num_hops` steps of a step size that's already a whole number of
    // cycles by construction.
    let quantized_advance = quantize_advance_for_loop(&frozen_left.advance, num_hops);

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

    let render_channel = |mag: Vec<f32>, phase0: Vec<f32>, advance: Vec<f32>| -> Vec<f32> {
        let mag = if formant_shift_semitones != 0.0 {
            let env = compute_spectral_envelope(&mag, &fft);
            let shifted_env = shift_envelope(&env, formant_ratio);
            reimpose_envelope(&mag, &env, &shifted_env)
        } else {
            mag
        };

        let mut resynth = FreezeResynth::new(mag, phase0, advance, window.clone());
        let mut accum = vec![0.0f32; out_len];
        let mut spectrum_scratch = fft.make_spectrum_buffer();
        let mut time_scratch = fft.make_time_buffer();

        let mut pos = 0usize;
        while pos < out_len {
            resynth.next_frame(&fft, &mut spectrum_scratch, &mut time_scratch);
            ola_accumulate_circular(&mut accum, pos, &time_scratch);
            pos += HOP_SIZE;
        }

        accum
    };

    let left_out = render_channel(frozen_left.mag, frozen_left.phase0, quantized_advance.clone());
    let right_out = render_channel(right_mag, right_phase0, quantized_advance);

    LoopBufferData {
        channels: vec![left_out, right_out],
        sample_rate,
        root_note,
    }
}

/// Snaps each bin's per-hop phase advance to the nearest value that
/// completes a whole number of cycles over exactly `num_hops` hops - i.e.
/// the nearest multiple of `TAU / num_hops`. A bin whose advance is
/// `quantized`, accumulated `num_hops` times, sums to `round(cycles) * TAU`
/// (an exact integer number of full turns), so the phase after a full loop
/// is identical to the phase at the start - the oscillator bank becomes
/// exactly periodic with period `num_hops` hops, whatever `phase0` each
/// channel starts from (a constant offset doesn't change whether the *step*
/// is periodic). The quantization step in frequency terms is
/// `1 / loop_duration_seconds` Hz - vanishingly small at the 4-8s end of
/// the range, and still only a couple of Hz even at the shortest allowed
/// loop, which is an inherent, inaudible-in-practice tradeoff of making any
/// synthesized content loop exactly (this is how every exactly-looping
/// wavetable/additive synth works).
fn quantize_advance_for_loop(advance: &[f32], num_hops: usize) -> Vec<f32> {
    let n = num_hops as f32;
    advance
        .iter()
        .map(|&a| {
            let cycles = (a * n / TAU).round();
            cycles * TAU / n
        })
        .collect()
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
    fn quantize_advance_for_loop_produces_an_exact_whole_number_of_cycles() {
        let num_hops = 20usize;
        let advance = vec![0.13, 1.0, 3.0, -2.5, 6.0];
        let quantized = quantize_advance_for_loop(&advance, num_hops);
        for &q in &quantized {
            let cycles = q * num_hops as f32 / TAU;
            assert!(
                (cycles - cycles.round()).abs() < 1e-4,
                "quantized advance {} over {} hops should be a whole number of cycles, got {} cycles",
                q,
                num_hops,
                cycles
            );
        }
    }

    #[test]
    fn quantize_advance_for_loop_stays_close_to_the_original() {
        // The quantization step shrinks as num_hops grows (TAU/num_hops),
        // so for a reasonably long loop every bin's advance should barely
        // move at all.
        let num_hops = 200usize;
        let advance = vec![0.7, 1.4, 2.1];
        let quantized = quantize_advance_for_loop(&advance, num_hops);
        for (&original, &q) in advance.iter().zip(quantized.iter()) {
            assert!((original - q).abs() < TAU / num_hops as f32, "quantized advance {} strayed too far from original {}", q, original);
        }
    }

    #[test]
    fn phase_locked_resynthesis_is_exactly_periodic_over_num_hops() {
        // Direct proof of the periodicity claim `quantize_advance_for_loop`
        // is built on: run the same oscillator bank for TWO full loops back
        // to back (plain linear OLA, no wraparound involved) and confirm
        // the second loop's audio is (near enough) identical to the
        // first's - i.e. the synthesized signal genuinely repeats with
        // period `num_hops` hops once its advance has been quantized.
        use crate::resynth::ola_accumulate;

        let fft = FreezeFft::new();
        let window = sine_window(FFT_SIZE);
        let num_bins = fft.num_bins();
        let num_hops = 30usize;
        let out_len = num_hops * HOP_SIZE;

        let mag = vec![1.0f32; num_bins];
        let phase0 = vec![0.4f32; num_bins];
        // Deliberately non-bin-aligned advances, like real measured advance
        // values would be.
        let raw_advance: Vec<f32> = (0..num_bins).map(|k| 0.037 * k as f32 + 0.6).collect();
        let advance = quantize_advance_for_loop(&raw_advance, num_hops);

        let mut resynth = FreezeResynth::new(mag, phase0, advance, window);
        let mut accum = vec![0.0f32; 2 * out_len + FFT_SIZE];
        let mut spectrum_scratch = fft.make_spectrum_buffer();
        let mut time_scratch = fft.make_time_buffer();

        let mut pos = 0usize;
        while pos < 2 * out_len {
            resynth.next_frame(&fft, &mut spectrum_scratch, &mut time_scratch);
            ola_accumulate(&mut accum, pos, &time_scratch);
            pos += HOP_SIZE;
        }

        // Compare a stable interior window from loop 1 against the same
        // offset in loop 2 - skip a little past each loop's own start/end
        // so we're not comparing OLA's naturally under-windowed edges
        // (which this plain linear accumulation, unlike the circular one
        // `render_frozen_loop` actually uses, doesn't fix).
        let check_start = HOP_SIZE * 2;
        let check_len = out_len - HOP_SIZE * 4;
        let mut max_diff = 0.0f32;
        for i in 0..check_len {
            let a = accum[check_start + i];
            let b = accum[check_start + out_len + i];
            max_diff = max_diff.max((a - b).abs());
        }
        assert!(max_diff < 1e-3, "expected loop 2 to match loop 1 near-exactly once advance is phase-locked, max diff = {}", max_diff);
    }

    #[test]
    fn loop_length_is_integer_number_of_hops() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 6.0) as usize);
        let result = render_frozen_loop(&[signal], sample_rate, 30.0, 0.0, 0.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
        let len = result.channels[0].len();
        assert_eq!(len % HOP_SIZE, 0, "loop length {} is not a multiple of HOP_SIZE", len);
    }

    #[test]
    fn loop_length_seconds_clamped_to_min_max_range() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 5.0) as usize);

        let short_result = render_frozen_loop(&[signal.clone()], sample_rate, 30.0, 0.0, 0.0, 0.0, DEFAULT_ROOT_NOTE);
        let short_seconds = short_result.channels[0].len() as f32 / sample_rate;
        assert!(short_seconds >= MIN_LOOP_SECONDS - 0.1, "requesting 0s should clamp up to MIN_LOOP_SECONDS, got {}", short_seconds);

        let long_result = render_frozen_loop(&[signal], sample_rate, 30.0, 0.0, 0.0, 30.0, DEFAULT_ROOT_NOTE);
        let long_seconds = long_result.channels[0].len() as f32 / sample_rate;
        assert!(long_seconds <= MAX_LOOP_SECONDS + 0.1, "requesting 30s should clamp down to MAX_LOOP_SECONDS, got {}", long_seconds);
    }

    #[test]
    fn loop_length_seconds_is_honored_within_range() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 5.0) as usize);
        let result = render_frozen_loop(&[signal], sample_rate, 30.0, 0.0, 0.0, 1.0, DEFAULT_ROOT_NOTE);
        let seconds = result.channels[0].len() as f32 / sample_rate;
        assert!((seconds - 1.0).abs() < 0.05, "expected ~1.0s loop, got {}", seconds);
    }

    #[test]
    fn deterministic_for_same_params() {
        let sample_rate = 48000.0;
        let signal = make_test_signal((sample_rate * 5.0) as usize);
        let a = render_frozen_loop(&[signal.clone()], sample_rate, 42.0, 2.0, 30.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
        let b = render_frozen_loop(&[signal], sample_rate, 42.0, 2.0, 30.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
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
        let mono_result = render_frozen_loop(&[mono_signal], sample_rate, 30.0, 0.0, 40.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
        assert_eq!(mono_result.channels.len(), 2);
        assert_eq!(mono_result.channels[0].len(), mono_result.channels[1].len());

        let left = make_test_signal((sample_rate * 5.0) as usize);
        let right: Vec<f32> = left.iter().map(|s| s * 0.5).collect();
        let stereo_result = render_frozen_loop(&[left, right], sample_rate, 30.0, 0.0, 40.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
        assert_eq!(stereo_result.channels.len(), 2);
        assert_eq!(stereo_result.channels[0].len(), stereo_result.channels[1].len());
    }

    #[test]
    fn stereo_width_zero_centers_output_regardless_of_source_correlation() {
        let sample_rate = 48000.0;
        // A genuinely stereo source with a real difference between channels.
        let left = make_test_signal((sample_rate * 5.0) as usize);
        let right: Vec<f32> = left.iter().enumerate().map(|(i, s)| s * 0.3 + (i as f32 * 0.05).sin() * 0.2).collect();

        let result = render_frozen_loop(&[left, right], sample_rate, 30.0, 0.0, 0.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
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
        let result = render_frozen_loop(&[mono_signal], sample_rate, 30.0, 0.0, 100.0, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);

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
            let result = render_frozen_loop(&[mono_signal.clone()], sample_rate, 30.0, 0.0, width, DEFAULT_LOOP_SECONDS, DEFAULT_ROOT_NOTE);
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
