use crate::envelope::AdsrEnvelope;
use crate::render::LoopBufferData;
use crate::resample::{playback_rate, sample_stereo_at, PlaybackReader};
use std::sync::Arc;

pub const MAX_VOICES: usize = 16;
pub const ATTACK_MS: f32 = 10.0;
pub const DECAY_MS: f32 = 100.0;
pub const SUSTAIN_LEVEL: f32 = 1.0;
pub const RELEASE_MS: f32 = 150.0;
/// How long to crossfade into a freshly rendered loop buffer (e.g. after a
/// Freeze Point / Formant Shift / Stereo Width change) instead of hard-
/// cutting to it. Two frozen spectra generally differ in content at any
/// given playhead position, so an instant swap is a real waveform
/// discontinuity - audible as a click, and as stuttering when a host
/// automates a param quickly enough to trigger several swaps in a row.
pub const BUFFER_CROSSFADE_MS: f32 = 15.0;
/// How quickly the polyphony gain compensation (see `process_block`) chases
/// its target as the active voice count changes, instead of jumping to it
/// instantly. Without this, the *instant* a voice actually finishes and
/// frees its slot (not when it's triggered - releasing voices are still
/// "active" and already compensated for), every other still-sounding voice
/// gets an abrupt, audible volume jump as the divisor changes underneath
/// them. Confirmed by measurement: RMS held steady while 2 notes overlapped,
/// then jumped ~1.6x the instant the first note's voice was freed.
pub const GAIN_COMPENSATION_SMOOTHING_MS: f32 = 30.0;
/// Default velocity sensitivity: 1.0 reproduces the plugin's original
/// behavior (gain equals velocity exactly).
pub const DEFAULT_VELOCITY_SENSITIVITY: f32 = 1.0;
/// Below this level, `soft_limit` is exact identity - only signals that
/// would otherwise exceed it are affected.
pub const SOFT_LIMIT_THRESHOLD: f32 = 0.9;

/// Soft-knee limiter applied as the final safety stage on the mixed output
/// (see `process_block`): identity below `SOFT_LIMIT_THRESHOLD`, smoothly
/// compressing louder signal toward an asymptotic ceiling of 1.0 via a tanh
/// knee that's C1-continuous at the threshold (its derivative there is
/// exactly 1, matching the identity region, so there's no audible kink).
///
/// This exists because the polyphony gain-compensation *target* (see
/// `GAIN_COMPENSATION_SMOOTHING_MS`) is deliberately smoothed rather than
/// applied instantly - so several voices attacked together (a real chord)
/// can genuinely sum well past unity for the first ~30-50ms while the
/// compensation is still chasing its new, lower target down. Measured with
/// 5 simultaneous full-gain voices on a realistic frozen-loop amplitude
/// (~0.64 peak): the uncompensated attack transient reached roughly 2.5x
/// full scale before this limiter existed. That was previously masked by
/// real MIDI velocity naturally sitting below 1.0 most of the time - it
/// became clearly audible once Velocity Sensitivity could force every voice
/// to gain 1.0 regardless of how hard a key was struck.
fn soft_limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= SOFT_LIMIT_THRESHOLD {
        return x;
    }
    let headroom = 1.0 - SOFT_LIMIT_THRESHOLD;
    let compressed = SOFT_LIMIT_THRESHOLD + headroom * ((magnitude - SOFT_LIMIT_THRESHOLD) / headroom).tanh();
    x.signum() * compressed
}

/// Attack/Decay/Sustain/Release timing applied to newly triggered voices
/// (like most synths, changing these doesn't reshape a note already
/// mid-envelope - only the *next* `note_on` picks up a change). Defaults to
/// `SUSTAIN_LEVEL = 1.0`, which makes Decay a no-op regardless of
/// `decay_ms` and reproduces the plugin's original fixed Attack/Release-only
/// behavior exactly.
#[derive(Clone, Copy, PartialEq)]
pub struct AdsrSettings {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain_level: f32,
    pub release_ms: f32,
}

impl Default for AdsrSettings {
    fn default() -> Self {
        Self { attack_ms: ATTACK_MS, decay_ms: DECAY_MS, sustain_level: SUSTAIN_LEVEL, release_ms: RELEASE_MS }
    }
}

pub struct Voice {
    pub id: i32,
    pub note: u8,
    pub channel: u8,
    pub gain: f32,
    pub rate: f64,
    pub reader: PlaybackReader,
    pub env: AdsrEnvelope,
    pub triggered_at: u64,
}

/// Fixed-size (no audio-thread allocation) polyphonic voice pool reading
/// from a shared frozen loop buffer at a per-note playback rate. Kept free
/// of any nih-plug/threading dependency so voice-stealing and mixing logic
/// stay unit-testable in isolation.
///
/// Stereo Width is NOT handled here - it's baked into the two channels of
/// `LoopBufferData` at render time (see `render::render_frozen_loop` and
/// `stereo::decorrelation_spread`), because a post-hoc mid-side transform
/// on the mixed output can only reveal difference that already exists
/// between channels, not create it when the source has none. This manager
/// just reads both of the buffer's (already width-shaped) channels in
/// lockstep and mixes voices together.
pub struct VoiceManager {
    voices: Vec<Option<Voice>>,
    sample_rate: f32,
    root_note: u8,
    clock: u64,
    current_buffer: Option<Arc<LoopBufferData>>,
    outgoing_buffer: Option<Arc<LoopBufferData>>,
    crossfade_elapsed: usize,
    adsr: AdsrSettings,
    smoothed_gain_compensation: f32,
    /// How much MIDI velocity affects a newly triggered voice's gain, from
    /// 0.0 (every note plays at a fixed full gain, ignoring velocity - for
    /// players/controllers where velocity scaling isn't wanted) to 1.0 (gain
    /// equals velocity exactly, the original behavior). Like `AdsrSettings`,
    /// only affects voices triggered after it's set - not already-playing
    /// ones.
    velocity_sensitivity: f32,
}

impl VoiceManager {
    pub fn new(sample_rate: f32, root_note: u8) -> Self {
        Self {
            voices: (0..MAX_VOICES).map(|_| None).collect(),
            sample_rate,
            root_note,
            clock: 0,
            current_buffer: None,
            outgoing_buffer: None,
            crossfade_elapsed: 0,
            adsr: AdsrSettings::default(),
            smoothed_gain_compensation: 1.0,
            velocity_sensitivity: DEFAULT_VELOCITY_SENSITIVITY,
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }

    /// Applied to voices triggered from now on - see `AdsrSettings`.
    pub fn set_adsr(&mut self, adsr: AdsrSettings) {
        self.adsr = adsr;
    }

    /// Applied to voices triggered from now on - see the `velocity_sensitivity` field.
    pub fn set_velocity_sensitivity(&mut self, sensitivity: f32) {
        self.velocity_sensitivity = sensitivity;
    }

    pub fn note_on(&mut self, note: u8, channel: u8, velocity: f32, id: i32) {
        let rate = playback_rate(note, self.root_note);
        let mut env =
            AdsrEnvelope::new(self.sample_rate, self.adsr.attack_ms, self.adsr.decay_ms, self.adsr.sustain_level, self.adsr.release_ms);
        env.note_on();
        // Linearly blends between a fixed full gain (sensitivity 0.0, e.g.
        // a controller/player where velocity scaling isn't wanted) and the
        // original gain-equals-velocity behavior (sensitivity 1.0).
        let gain = 1.0 - self.velocity_sensitivity * (1.0 - velocity);
        let voice = Voice {
            id,
            note,
            channel,
            gain,
            rate,
            reader: PlaybackReader::new(0.0),
            env,
            triggered_at: self.clock,
        };

        let slot = self.find_free_slot().unwrap_or_else(|| self.steal_slot());
        self.voices[slot] = Some(voice);
    }

    pub fn note_off(&mut self, note: u8, channel: u8) {
        for v in self.voices.iter_mut().flatten() {
            if v.note == note && v.channel == channel {
                v.env.note_off();
            }
        }
    }

    pub fn choke_all(&mut self) {
        for slot in self.voices.iter_mut() {
            *slot = None;
        }
    }

    fn find_free_slot(&self) -> Option<usize> {
        self.voices.iter().position(|v| v.is_none())
    }

    /// Voice-stealing policy: prefer a voice already releasing (picking the
    /// one with the lowest current level - closest to silent), else steal
    /// the oldest currently-active voice (FIFO), which is audibly less
    /// disruptive than cutting off whichever voice happens to be loudest.
    fn steal_slot(&self) -> usize {
        let releasing_candidate = self
            .voices
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
            .filter(|(_, v)| v.env.is_releasing())
            .min_by(|(_, a), (_, b)| a.env.level().partial_cmp(&b.env.level()).unwrap());

        if let Some((idx, _)) = releasing_candidate {
            return idx;
        }

        self.voices
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
            .min_by_key(|(_, v)| v.triggered_at)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Renders one block of stereo audio (summed across voices) into
    /// `out_left`/`out_right`, reading both of `buffer`'s channels in
    /// lockstep per voice. If `buffer` has only one channel, that channel
    /// is duplicated to both outputs.
    ///
    /// `buffer` is compared by `Arc` identity (not content) against the
    /// previously passed buffer: a fresh `Arc` (e.g. published by a
    /// background render triggered by a param change) starts a short
    /// crossfade from the outgoing buffer rather than an instant swap - see
    /// `BUFFER_CROSSFADE_MS`.
    pub fn process_block(&mut self, buffer: &Arc<LoopBufferData>, out_left: &mut [f32], out_right: &mut [f32]) {
        debug_assert_eq!(out_left.len(), out_right.len());
        for sample in out_left.iter_mut() {
            *sample = 0.0;
        }
        for sample in out_right.iter_mut() {
            *sample = 0.0;
        }

        let is_new_buffer = match &self.current_buffer {
            Some(current) => !Arc::ptr_eq(current, buffer),
            None => true,
        };
        if is_new_buffer {
            if let Some(previous) = self.current_buffer.replace(buffer.clone()) {
                self.outgoing_buffer = Some(previous);
                self.crossfade_elapsed = 0;
            }
        }

        let current = self.current_buffer.as_ref().expect("just set above if it was None");
        let Some(left_channel) = current.channels.first() else {
            return;
        };
        let right_channel = current.channels.get(1).unwrap_or(left_channel);

        let crossfade_total_samples = (((BUFFER_CROSSFADE_MS / 1000.0) * self.sample_rate) as usize).max(1);
        let crossfade = if self.crossfade_elapsed < crossfade_total_samples {
            self.outgoing_buffer.as_ref().and_then(|outgoing| {
                let l = outgoing.channels.first()?;
                let r = outgoing.channels.get(1).unwrap_or(l);
                Some((l, r))
            })
        } else {
            None
        };

        // Voices are summed with no per-voice headroom, so a held chord
        // clips without this. A single frozen note can already sit close to
        // full scale, and frozen spectra from the same source can be highly
        // correlated across notes (e.g. a small formant/pitch difference),
        // so 1/sqrt(n) (tuned for uncorrelated signals) isn't conservative
        // enough - it still let 4 voices clip in practice. 1/n guarantees
        // the sum can never exceed a single voice's own peak even in the
        // fully-correlated worst case, at the cost of chords getting quieter
        // faster than perceived loudness would suggest.
        //
        // The target is read from the count *before* this block's voices
        // are processed (a voice that finishes partway through this same
        // block was still contributing real signal for most of it, so this
        // block must still be compensated as if it were active - using the
        // post-removal count here would apply next block's lower divisor to
        // audio this block that still includes that voice's tail).
        //
        // Applied as a smoothed post-sum multiply, not per-voice inside the
        // loop below: mathematically identical for a constant multiplier
        // (gain * sum(x_i) == sum(gain * x_i)), but this is the only way to
        // *smooth* it - the active count (and therefore the target) can
        // change between blocks whenever a voice starts or finishes, and
        // applying that new divisor instantly causes a real, audible volume
        // jump on every other already-sounding voice, not just the one that
        // changed. During the brief chase toward a lower target the
        // smoothed value can sit slightly above the strict worst-case-safe
        // bound - an accepted, standard tradeoff for avoiding a hard step,
        // same as any other audio-rate smoothing.
        let target_gain_compensation = 1.0 / self.active_voice_count().max(1) as f32;

        for slot in self.voices.iter_mut() {
            let Some(voice) = slot else { continue };
            for i in 0..out_left.len() {
                let pos_before_advance = voice.reader.read_pos;
                let (mut l, mut r) = voice.reader.read_stereo_and_advance(left_channel, right_channel, voice.rate);

                if let Some((outgoing_left, outgoing_right)) = crossfade {
                    let elapsed = self.crossfade_elapsed + i;
                    if elapsed < crossfade_total_samples {
                        let t = elapsed as f32 / crossfade_total_samples as f32;
                        let (old_l, old_r) = sample_stereo_at(outgoing_left, outgoing_right, pos_before_advance);
                        l = old_l * (1.0 - t) + l * t;
                        r = old_r * (1.0 - t) + r * t;
                    }
                }

                let level = voice.env.advance();
                out_left[i] += l * level * voice.gain;
                out_right[i] += r * level * voice.gain;
            }
            if voice.env.is_finished() {
                *slot = None;
            }
        }

        let smoothing_coeff = (-1.0 / ((GAIN_COMPENSATION_SMOOTHING_MS / 1000.0) * self.sample_rate)).exp();
        for i in 0..out_left.len() {
            self.smoothed_gain_compensation =
                target_gain_compensation + (self.smoothed_gain_compensation - target_gain_compensation) * smoothing_coeff;
            out_left[i] = soft_limit(out_left[i] * self.smoothed_gain_compensation);
            out_right[i] = soft_limit(out_right[i] * self.smoothed_gain_compensation);
        }

        if self.outgoing_buffer.is_some() {
            self.crossfade_elapsed += out_left.len();
            if self.crossfade_elapsed >= crossfade_total_samples {
                self.outgoing_buffer = None;
            }
        }

        self.clock += out_left.len() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::DEFAULT_ROOT_NOTE;

    fn make_buffer() -> Arc<LoopBufferData> {
        Arc::new(LoopBufferData {
            channels: vec![vec![0.5f32; 4096], vec![0.5f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        })
    }

    #[test]
    fn steals_oldest_active_when_full() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        for i in 0..MAX_VOICES {
            vm.note_on(60, 0, 1.0, i as i32);
        }
        assert_eq!(vm.active_voice_count(), MAX_VOICES);

        // Trigger one more note-on; should steal slot 0 (the oldest,
        // triggered_at == 0), not any of the newer voices.
        vm.note_on(72, 0, 1.0, 999);
        let ids: Vec<i32> = vm.voices.iter().filter_map(|v| v.as_ref().map(|v| v.id)).collect();
        assert!(!ids.contains(&0), "oldest voice (id 0) should have been stolen");
        assert!(ids.contains(&999));
        assert_eq!(vm.active_voice_count(), MAX_VOICES);
    }

    #[test]
    fn prefers_releasing_voices_when_stealing() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        for i in 0..MAX_VOICES {
            vm.note_on(60, 0, 1.0, i as i32);
        }
        // Release voice at index 5's note explicitly (all notes are the
        // same pitch/channel here, so note_off would hit all of them - use
        // distinct notes instead for a clean single-target release).
        vm.choke_all();

        for i in 0..MAX_VOICES {
            vm.note_on(60 + i as u8, 0, 1.0, i as i32);
        }
        vm.note_off(60 + 5, 0); // put id=5's voice into release

        vm.note_on(90, 0, 1.0, 999);
        let ids: Vec<i32> = vm.voices.iter().filter_map(|v| v.as_ref().map(|v| v.id)).collect();
        assert!(!ids.contains(&5), "releasing voice should have been stolen over active ones");
        assert!(ids.contains(&999));
    }

    #[test]
    fn process_block_removes_finished_voices() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 1.0, 1);
        vm.note_off(60, 0);

        let buffer = make_buffer();
        let mut out_left = vec![0.0f32; 1024];
        let mut out_right = vec![0.0f32; 1024];
        // Release is 150ms; at 48kHz that's ~7200 samples, so a handful of
        // 1024-sample blocks should fully release and free the voice.
        for _ in 0..20 {
            vm.process_block(&buffer, &mut out_left, &mut out_right);
        }
        assert_eq!(vm.active_voice_count(), 0);
    }

    #[test]
    fn process_block_produces_finite_audio() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 0.8, 1);
        vm.note_on(67, 0, 0.6, 2);

        let buffer = make_buffer();
        let mut out_left = vec![0.0f32; 512];
        let mut out_right = vec![0.0f32; 512];
        vm.process_block(&buffer, &mut out_left, &mut out_right);
        for s in out_left.iter().chain(out_right.iter()) {
            assert!(s.is_finite());
        }
    }

    #[test]
    fn mono_source_duplicates_single_channel_to_both_outputs() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 1.0, 1);

        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.7f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        });
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        for _ in 0..5 {
            vm.process_block(&buffer, &mut out_left, &mut out_right);
        }

        for (l, r) in out_left.iter().zip(out_right.iter()) {
            assert!((l - r).abs() < 1e-4, "single-channel buffer should read identically on both outputs");
        }
    }

    #[test]
    fn zero_velocity_sensitivity_ignores_velocity() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.set_velocity_sensitivity(0.0);
        vm.note_on(DEFAULT_ROOT_NOTE, 0, 0.2, 1);

        let buffer = make_buffer();
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        vm.process_block(&buffer, &mut out_left, &mut out_right);

        // Well past the 10ms default attack, so the envelope has settled at
        // its sustain level (1.0 by default) - the only thing left to prove
        // is that a low velocity (0.2) didn't scale the output down at all.
        let settled = out_left[out_left.len() - 1];
        assert!((settled - 0.5).abs() < 1e-3, "sensitivity 0.0 should ignore velocity entirely, got {settled}");
    }

    #[test]
    fn full_velocity_sensitivity_scales_output_with_velocity() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.set_velocity_sensitivity(1.0);
        vm.note_on(DEFAULT_ROOT_NOTE, 0, 0.2, 1);

        let buffer = make_buffer();
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        vm.process_block(&buffer, &mut out_left, &mut out_right);

        let settled = out_left[out_left.len() - 1];
        assert!((settled - 0.1).abs() < 1e-3, "sensitivity 1.0 should scale output by velocity (0.5 * 0.2 = 0.1), got {settled}");
    }

    #[test]
    fn chord_attack_at_zero_velocity_sensitivity_never_exceeds_unity() {
        // Regression test for real distortion reported by ear: with Velocity
        // Sensitivity at 0% every voice plays at gain 1.0 regardless of how
        // hard a key is struck, removing the natural headroom real MIDI
        // velocity (usually well below 1.0) used to provide "for free". A
        // realistic 5-note chord hit together then genuinely sums well past
        // unity for the first ~30-50ms while `GAIN_COMPENSATION_SMOOTHING_MS`
        // is still chasing its new, lower target down (measured ~2.5x before
        // `soft_limit` existed, using this same buffer amplitude and voice
        // count). This proves the safety limiter actually catches it, one
        // sample at a time so the exact peak during the attack is captured.
        let sample_rate = 48000.0;
        let buffer = Arc::new(LoopBufferData {
            // Amplitude representative of a real frozen loop (measured
            // ~0.22-0.64 peak on the project's bundled test asset), not the
            // artificial 1.0 used by other tests that aren't about peak level.
            channels: vec![vec![0.64f32; 4096], vec![0.64f32; 4096]],
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        });

        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.set_velocity_sensitivity(0.0);
        for (i, note) in [60u8, 64, 67, 70, 74].into_iter().enumerate() {
            // Velocity deliberately varied (as a real hand on a keybed
            // would) to prove sensitivity 0.0 - not the input velocities -
            // is what's making every voice play at full gain.
            vm.note_on(note, 0, 0.3 + 0.1 * i as f32, i as i32);
        }

        let mut out_l = [0.0f32];
        let mut out_r = [0.0f32];
        let mut peak = 0.0f32;
        for _ in 0..(sample_rate as usize / 10) {
            // 100ms, comfortably past the attack/compensation transient.
            vm.process_block(&buffer, &mut out_l, &mut out_r);
            peak = peak.max(out_l[0].abs()).max(out_r[0].abs());
        }

        assert!(peak <= 1.0 + 1e-4, "chord attack at zero velocity sensitivity must never exceed unity, got peak={peak}");
    }

    #[test]
    fn gain_compensation_scales_down_with_more_active_voices() {
        // A held chord must not clip even in the fully-correlated worst
        // case: N simultaneous identical voices should sum to the same
        // level as a single voice, not N times.
        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![1.0f32; 8192], vec![1.0f32; 8192]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        });

        let mut single = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        single.note_on(DEFAULT_ROOT_NOTE, 0, 1.0, 1);
        let mut single_out_l = vec![0.0f32; 8192];
        let mut single_out_r = vec![0.0f32; 8192];
        for _ in 0..5 {
            single.process_block(&buffer, &mut single_out_l, &mut single_out_r);
        }

        let mut quad = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        for i in 0..4 {
            quad.note_on(DEFAULT_ROOT_NOTE, 0, 1.0, i);
        }
        let mut quad_out_l = vec![0.0f32; 8192];
        let mut quad_out_r = vec![0.0f32; 8192];
        for _ in 0..5 {
            quad.process_block(&buffer, &mut quad_out_l, &mut quad_out_r);
        }

        let tail_start = single_out_l.len() - 100;
        let single_level: f32 = single_out_l[tail_start..].iter().sum::<f32>() / 100.0;
        let quad_level: f32 = quad_out_l[tail_start..].iter().sum::<f32>() / 100.0;

        assert!(
            (quad_level - single_level).abs() < 1e-2,
            "expected 4 voices (1/4 compensation) to sum to the same level as a single voice: single={}, quad={}",
            single_level,
            quad_level
        );
    }

    #[test]
    fn gain_compensation_ramps_smoothly_when_a_voice_finishes() {
        // Regression test for a real bug found by ear: a second, still-
        // sounding voice got an abrupt, audible volume jump the instant an
        // earlier released voice actually finished and freed its slot (the
        // compensation divisor changing from 2 to 1 with no smoothing).
        // Measured live: RMS held steady while overlapping, then jumped
        // ~1.6x the instant the old voice's slot freed.
        //
        // Processes one sample per "block" specifically so the exact sample
        // where `active_voice_count()` changes can be pinpointed. Note the
        // target used for a given sample is computed from the count
        // *before* that sample's voices are processed (see the comment in
        // `process_block`), so the sample where the count is first observed
        // to drop to 1 was itself still rendered with the *old* (2-voice)
        // target - the new target only takes effect starting the sample
        // after that. At either of these two samples, voice A's own
        // envelope has *already* been forced to precisely 0.0 by
        // `AdsrEnvelope::advance` (the same sample that flips it to
        // `Stage::Idle`), so its contribution to the raw sum is identically
        // 0 at both. That isolates the comparison to *only* whatever the
        // compensation multiplier does at this boundary, with no
        // contamination from the envelope's own (legitimate, and otherwise
        // easily confusable with this bug) ongoing decay.
        // Buffer amplitude kept below `SOFT_LIMIT_THRESHOLD` (0.9) so the
        // safety limiter added for `FREEZE-PLAN-007` doesn't confound this
        // test's own concern (compensation-jump smoothness, not peak level).
        let sample_rate = 48000.0;
        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.5f32; 8192], vec![0.5f32; 8192]],
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        });

        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.set_adsr(AdsrSettings { attack_ms: 0.1, decay_ms: 0.1, sustain_level: 1.0, release_ms: 5.0 });
        vm.note_on(60, 0, 1.0, 1);
        vm.note_on(72, 0, 1.0, 2);

        let mut out_l = [0.0f32];
        let mut out_r = [0.0f32];

        // Run past attack so both voices have settled at full level with
        // compensation applied for 2 active voices.
        for _ in 0..100 {
            vm.process_block(&buffer, &mut out_l, &mut out_r);
        }
        assert_eq!(vm.active_voice_count(), 2);

        vm.note_off(60, 0);

        let mut level_at_transition: Option<f32> = None;
        let mut boundary_jump = None;
        for _ in 0..(sample_rate as usize) {
            let was_two = vm.active_voice_count() == 2;
            vm.process_block(&buffer, &mut out_l, &mut out_r);
            let level = out_l[0];
            if let Some(prev) = level_at_transition {
                boundary_jump = Some((level - prev).abs());
                break;
            }
            if was_two && vm.active_voice_count() == 1 {
                level_at_transition = Some(level);
            }
        }

        let boundary_jump = boundary_jump.expect("voice A should have finished and freed its slot within the test window");
        // The old (unsmoothed) behavior stepped by close to the full
        // (1.0 - 0.5) = 0.5 compensation change in this single sample. A
        // smoothed transition should move only a tiny fraction of that in
        // one sample, given a 30ms smoothing time constant.
        assert!(
            boundary_jump < 0.01,
            "expected a smooth transition right at the voice-freed boundary, not a jump: jump={}",
            boundary_jump
        );

        for _ in 0..(sample_rate as usize / 5) {
            vm.process_block(&buffer, &mut out_l, &mut out_r);
        }
        assert!(
            (out_l[0] - 0.5).abs() < 1e-2,
            "expected to settle back at full level with 1 active voice (no compensation needed), got {}",
            out_l[0]
        );
    }

    #[test]
    fn distinct_channels_produce_distinct_output() {
        // VoiceManager itself must faithfully reproduce whatever difference
        // the (already width-shaped) buffer contains - it does no width
        // processing of its own. Amplitudes kept below `SOFT_LIMIT_THRESHOLD`
        // (0.9) so the safety limiter added for `FREEZE-PLAN-007` doesn't
        // confound this test's own concern (channel fidelity, not peak level).
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 1.0, 1);

        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.7f32; 4096], vec![0.35f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        });
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        for _ in 0..5 {
            vm.process_block(&buffer, &mut out_left, &mut out_right);
        }

        let tail_start = out_left.len() - 100;
        for i in tail_start..out_left.len() {
            assert!((out_left[i] - 0.7).abs() < 1e-3, "got {}", out_left[i]);
            assert!((out_right[i] - 0.35).abs() < 1e-3, "got {}", out_right[i]);
        }
    }

    #[test]
    fn buffer_swap_crossfades_instead_of_clicking() {
        // Two maximally different (opposite-polarity, constant) buffers
        // stand in for "two very different frozen spectra" - swapping
        // between them with no crossfade would jump by 1.0 in a single
        // sample. With the crossfade, the largest sample-to-sample delta
        // anywhere in the transition should be far smaller than that.
        // Amplitude kept below `SOFT_LIMIT_THRESHOLD` (0.9) so the safety
        // limiter added for `FREEZE-PLAN-007` doesn't confound this test's
        // own concern (crossfade smoothness, not peak level).
        let sample_rate = 48000.0;
        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.note_on(DEFAULT_ROOT_NOTE, 0, 1.0, 1);

        let buffer_a = Arc::new(LoopBufferData {
            channels: vec![vec![0.5f32; 4096], vec![0.5f32; 4096]],
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        });
        let buffer_b = Arc::new(LoopBufferData {
            channels: vec![vec![-0.5f32; 4096], vec![-0.5f32; 4096]],
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        });

        // Run past the attack envelope on buffer_a so the swap isn't masked
        // by the note also fading in at the same time.
        let mut scratch_l = vec![0.0f32; 512];
        let mut scratch_r = vec![0.0f32; 512];
        for _ in 0..10 {
            vm.process_block(&buffer_a, &mut scratch_l, &mut scratch_r);
        }

        // Swap to the opposite-polarity buffer and capture the transition.
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        vm.process_block(&buffer_b, &mut out_left, &mut out_right);

        let max_delta = out_left.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(
            max_delta < 0.25,
            "expected the crossfade to smooth the transition (max single-sample delta far below the 1.0 hard-swap jump), got {}",
            max_delta
        );

        // And the crossfade must actually finish: after it's well past
        // BUFFER_CROSSFADE_MS, output should have fully settled on buffer_b.
        for _ in 0..10 {
            vm.process_block(&buffer_b, &mut out_left, &mut out_right);
        }
        let tail_start = out_left.len() - 100;
        for i in tail_start..out_left.len() {
            assert!((out_left[i] - -0.5).abs() < 1e-3, "expected to have settled on buffer_b, got {}", out_left[i]);
        }
    }
}
