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
/// This is deliberately the *only* mechanism protecting against polyphony
/// overload - there is no per-voice or per-block gain compensation based on
/// how many voices are currently active (see `process_block`'s docs on why
/// that was tried, three times, and removed: any gain change reacting to
/// unrelated notes' lifecycle is audible as pumping/ducking no matter how
/// it's tuned, which is not how real synths/instruments behave - playing
/// more notes just makes more sound, the same way a piano or an analog
/// synth does). Voices are simply summed at a fixed, uncompensated gain,
/// and this limiter's smooth, bounded knee is what keeps that safe: it can
/// never produce a hard clip regardless of how many voices stack, it only
/// gets progressively denser/more saturated-sounding, which is a normal,
/// expected texture for an unusually large chord rather than a bug to
/// prevent. Measured with 5 simultaneous full-gain voices on a realistic
/// frozen-loop amplitude (~0.64 peak): an uncompensated attack transient
/// reached roughly 2.5x full scale - this limiter's job is to turn that
/// into smooth saturation instead of harsh clipping, not to prevent the
/// loudness increase from happening at all.
fn soft_limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= SOFT_LIMIT_THRESHOLD {
        return x;
    }
    let headroom = 1.0 - SOFT_LIMIT_THRESHOLD;
    let compressed = SOFT_LIMIT_THRESHOLD + headroom * ((magnitude - SOFT_LIMIT_THRESHOLD) / headroom).tanh();
    x.signum() * compressed
}

/// Minimal xorshift32 PRNG, used only for the per-voice pan randomizer
/// (`VoiceManager::pan_width`) - deliberately not a real `rand`-crate
/// dependency, since nothing here needs cryptographic or even rigorous
/// statistical quality, just a perceptually-varied spread across notes.
/// Deterministic given the same call sequence, which keeps it testable
/// (same note sequence always produces the same pan pattern).
struct SimpleRng(u32);

impl SimpleRng {
    /// Must be non-zero - xorshift32 stays at zero forever if seeded there.
    const SEED: u32 = 0x9E3779B9;

    fn new() -> Self {
        Self(Self::SEED)
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// A value in `[-1.0, 1.0]`.
    fn next_bipolar(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
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
    /// Fixed at `note_on` time (see `VoiceManager::pan_center`/`pan_width`) -
    /// like gain and rate, panning doesn't change for a voice already
    /// playing, only for the next one triggered. `-1.0` = full left, `0.0`
    /// = center, `1.0` = full right.
    pub pan: f32,
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
    /// How much MIDI velocity affects a newly triggered voice's gain, from
    /// 0.0 (every note plays at a fixed full gain, ignoring velocity - for
    /// players/controllers where velocity scaling isn't wanted) to 1.0 (gain
    /// equals velocity exactly, the original behavior). Like `AdsrSettings`,
    /// only affects voices triggered after it's set - not already-playing
    /// ones.
    velocity_sensitivity: f32,
    /// Center point for the per-voice pan randomizer, in `[-1.0, 1.0]`
    /// (`-1.0` = full left, `0.0` = center, `1.0` = full right). Like
    /// `AdsrSettings`, only affects voices triggered after it's set.
    pan_center: f32,
    /// How far each newly triggered voice's pan can randomly land from
    /// `pan_center`, in `[0.0, 1.0]` - `0.0` disables randomization
    /// entirely (every voice pans to exactly `pan_center`), `1.0` allows
    /// the full range on either side (clamped to stay within `[-1.0, 1.0]`
    /// overall).
    pan_width: f32,
    /// Advanced once per `note_on` - see `SimpleRng`.
    pan_rng: SimpleRng,
    /// Current pitch bend amount in semitones (already `wheel_position *
    /// range` - see `PrismPluginParams::pitch_bend_range_semitones` on the
    /// plugin side; this struct doesn't need to know about raw MIDI
    /// encoding or the range param, just the final musical value). Unlike
    /// `AdsrSettings`/`velocity_sensitivity`/pan, this is a real-time
    /// continuous control applied to *every* currently-active voice's
    /// playback rate every block, not just newly triggered ones - that's
    /// what real MIDI pitch bend does (it bends whatever is currently
    /// sounding on the channel).
    pitch_bend_semitones: f32,
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
            velocity_sensitivity: DEFAULT_VELOCITY_SENSITIVITY,
            pan_center: 0.0,
            pan_width: 0.0,
            pan_rng: SimpleRng::new(),
            pitch_bend_semitones: 0.0,
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

    /// Applied to voices triggered from now on - see `pan_center`/`pan_width`.
    pub fn set_pan_settings(&mut self, center: f32, width: f32) {
        self.pan_center = center.clamp(-1.0, 1.0);
        self.pan_width = width.clamp(0.0, 1.0);
    }

    /// Applied to every currently-active voice on the next block - see
    /// `pitch_bend_semitones`.
    pub fn set_pitch_bend_semitones(&mut self, semitones: f32) {
        self.pitch_bend_semitones = semitones;
    }

    pub fn note_on(&mut self, note: u8, channel: u8, velocity: f32, id: i32) {
        let free_slot = self.find_free_slot();

        let rate = playback_rate(note, self.root_note);
        let mut env =
            AdsrEnvelope::new(self.sample_rate, self.adsr.attack_ms, self.adsr.decay_ms, self.adsr.sustain_level, self.adsr.release_ms);
        env.note_on();
        // Linearly blends between a fixed full gain (sensitivity 0.0, e.g.
        // a controller/player where velocity scaling isn't wanted) and the
        // original gain-equals-velocity behavior (sensitivity 1.0).
        let gain = 1.0 - self.velocity_sensitivity * (1.0 - velocity);
        // Randomized once per voice within [pan_center - pan_width,
        // pan_center + pan_width], clamped to stay in range - width 0.0
        // always lands exactly on pan_center (no randomization at all).
        let pan = (self.pan_center + self.pan_width * self.pan_rng.next_bipolar()).clamp(-1.0, 1.0);
        let voice = Voice {
            id,
            note,
            channel,
            gain,
            rate,
            reader: PlaybackReader::new(0.0),
            env,
            triggered_at: self.clock,
            pan,
        };

        let slot = free_slot.unwrap_or_else(|| self.steal_slot());
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

        // Voices are summed at a fixed, uncompensated gain - no attempt is
        // made to keep the mixed level constant as the number of active
        // voices changes (see `soft_limit`'s docs for why: three earlier
        // attempts at reactive polyphony gain compensation each fixed one
        // audible problem but introduced another, because any gain change
        // that responds to unrelated notes' lifecycle is itself a form of
        // pumping/ducking). More notes held just means more sound, the same
        // way a real piano or analog synth behaves; `soft_limit` below is
        // the sole, purely reactive-to-the-actual-signal safety net that
        // keeps an unusually large chord from hard-clipping.
        // Computed once per block, not per-sample: pitch bend is a MIDI
        // wheel position, not audio-rate. Applied to *every* active voice's
        // read rate (real MIDI pitch bend affects whatever's currently
        // sounding on the channel, not just newly triggered notes - unlike
        // gain/pan/ADSR, which are only decided once at `note_on`).
        let bend_multiplier = 2.0_f64.powf(self.pitch_bend_semitones as f64 / 12.0);

        for slot in self.voices.iter_mut() {
            let Some(voice) = slot else { continue };
            let bent_rate = voice.rate * bend_multiplier;
            // Linear (not constant-power) pan: deliberately chosen so it's
            // exact *identity* at the default `pan == 0.0` (left_gain ==
            // right_gain == 1.0, i.e. the original unpanned behavior) rather
            // than collapsing to mono at center the way a constant-power law
            // would - important here because Stereo Width already bakes a
            // real L/R difference into the source at render time, and this
            // must not quietly undo that whenever the pan randomizer is left
            // at its default (`pan_width == 0.0`, so every voice pans to
            // exactly 0.0). Panning instead reduces the *opposite* channel's
            // contribution as pan moves away from center.
            let pan = voice.pan.clamp(-1.0, 1.0);
            let left_gain = 1.0 - pan.max(0.0);
            let right_gain = 1.0 + pan.min(0.0);
            for i in 0..out_left.len() {
                let pos_before_advance = voice.reader.read_pos;
                let (mut l, mut r) = voice.reader.read_stereo_and_advance(left_channel, right_channel, bent_rate);

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
                out_left[i] += l * left_gain * level * voice.gain;
                out_right[i] += r * right_gain * level * voice.gain;
            }
            if voice.env.is_finished() {
                *slot = None;
            }
        }

        for i in 0..out_left.len() {
            out_left[i] = soft_limit(out_left[i]);
            out_right[i] = soft_limit(out_right[i]);
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
        // velocity (usually well below 1.0) used to provide "for free". With
        // no polyphony gain compensation at all (see `soft_limit`'s docs), a
        // realistic 5-note chord hit together genuinely sums well past unity
        // (measured ~2.5x, using this same buffer amplitude and voice count).
        // This proves the safety limiter actually catches it, one sample at
        // a time so the exact peak during the attack is captured.
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
            // 100ms, comfortably past the attack transient.
            vm.process_block(&buffer, &mut out_l, &mut out_r);
            peak = peak.max(out_l[0].abs()).max(out_r[0].abs());
        }

        assert!(peak <= 1.0 + 1e-4, "chord attack at zero velocity sensitivity must never exceed unity, got peak={peak}");
    }

    #[test]
    fn voices_sum_linearly_with_no_polyphony_compensation() {
        // `FREEZE-BUG-010`: three successive attempts at reactive polyphony
        // gain compensation (1/n, then 1/sqrt(n) with a ratchet, then a
        // ratchet plus a slow recovery) each fixed one audible problem but
        // introduced another, because any gain that reacts to unrelated
        // notes' lifecycle is itself audible as pumping/ducking. The fix was
        // to remove compensation entirely: voices are summed at a fixed
        // gain regardless of how many are active, the same way a real
        // piano or analog synth behaves (more notes = more sound). With N
        // fully-correlated (identical buffer, same constant-value signal at
        // every sample regardless of pitch) voices, the sum must be exactly
        // N times a single voice's own level - not sqrt(n), not 1/n,
        // nothing in between. Buffer amplitude kept low enough that even 4
        // summed voices stay under `SOFT_LIMIT_THRESHOLD`, so the safety
        // limiter doesn't confound this test's own concern (linear
        // summation, not peak safety - see
        // `chord_attack_at_zero_velocity_sensitivity_never_exceeds_unity`
        // for that).
        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.2f32; 8192], vec![0.2f32; 8192]],
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
        let expected_quad_level = single_level * 4.0;

        assert!(
            (quad_level - expected_quad_level).abs() < 1e-2,
            "expected 4 fully-correlated, uncompensated voices to sum to 4x a single voice: single={}, quad={}, expected_quad={}",
            single_level,
            quad_level,
            expected_quad_level
        );
    }

    #[test]
    fn surviving_voice_matches_its_solo_level_the_instant_a_sibling_finishes() {
        // With gain compensation removed entirely (`FREEZE-BUG-010`), a
        // surviving voice's level must match exactly what the same note
        // would sound like played completely alone, the very instant a
        // sibling voice finishes - no jump, no gradual "recovery" needed,
        // because there was never any voice-count-dependent gain applied
        // in the first place. This is the property all three earlier
        // compensation designs were each trying (and each failing in a
        // different way) to approximate reactively - removing the reactive
        // part makes it exactly true instead of approximately true.
        // Buffer amplitude kept below `SOFT_LIMIT_THRESHOLD` (0.9) so the
        // safety limiter doesn't confound this test's own concern.
        let sample_rate = 48000.0;
        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.5f32; 8192], vec![0.5f32; 8192]],
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        });
        let adsr = AdsrSettings { attack_ms: 0.1, decay_ms: 0.1, sustain_level: 1.0, release_ms: 5.0 };

        // Ground truth: what note B sounds like played completely alone.
        let mut solo = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        solo.set_adsr(adsr);
        solo.note_on(72, 0, 1.0, 2);
        let mut solo_out_l = [0.0f32];
        let mut solo_out_r = [0.0f32];
        for _ in 0..100 {
            solo.process_block(&buffer, &mut solo_out_l, &mut solo_out_r);
        }
        let solo_level = solo_out_l[0];

        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.set_adsr(adsr);
        vm.note_on(60, 0, 1.0, 1);
        vm.note_on(72, 0, 1.0, 2);

        let mut out_l = [0.0f32];
        let mut out_r = [0.0f32];
        for _ in 0..100 {
            vm.process_block(&buffer, &mut out_l, &mut out_r);
        }
        assert_eq!(vm.active_voice_count(), 2);

        vm.note_off(60, 0);
        let mut reached_one = false;
        for _ in 0..(sample_rate as usize) {
            vm.process_block(&buffer, &mut out_l, &mut out_r);
            if vm.active_voice_count() == 1 {
                reached_one = true;
                break;
            }
        }
        assert!(reached_one, "voice A should have finished and freed its slot within the test window");

        assert!(
            (out_l[0] - solo_level).abs() < 1e-4,
            "surviving voice should match its solo level immediately once its sibling finishes, got {} (expected {})",
            out_l[0],
            solo_level
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

    #[test]
    fn pitch_bend_scales_playback_rate() {
        let sample_rate = 48000.0;
        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.note_on(DEFAULT_ROOT_NOTE, 0, 1.0, 1);
        let base_rate = vm.voices[0].as_ref().unwrap().rate;

        vm.set_pitch_bend_semitones(12.0); // one octave up -> rate should double
        let buffer = make_buffer();
        let mut out_l = [0.0f32];
        let mut out_r = [0.0f32];
        vm.process_block(&buffer, &mut out_l, &mut out_r);

        let advanced = vm.voices[0].as_ref().unwrap().reader.read_pos;
        let expected = base_rate * 2.0;
        assert!(
            (advanced - expected).abs() < 1e-6,
            "expected read position to advance by the pitch-bent rate ({expected}), got {advanced}"
        );
    }

    #[test]
    fn zero_pitch_bend_is_identity() {
        let sample_rate = 48000.0;
        let mut vm = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        vm.note_on(DEFAULT_ROOT_NOTE, 0, 1.0, 1);
        let base_rate = vm.voices[0].as_ref().unwrap().rate;

        vm.set_pitch_bend_semitones(0.0);
        let buffer = make_buffer();
        let mut out_l = [0.0f32];
        let mut out_r = [0.0f32];
        vm.process_block(&buffer, &mut out_l, &mut out_r);

        let advanced = vm.voices[0].as_ref().unwrap().reader.read_pos;
        assert!(
            (advanced - base_rate).abs() < 1e-9,
            "zero bend should leave the playback rate completely unchanged, expected {base_rate}, got {advanced}"
        );
    }

    #[test]
    fn default_pan_settings_preserve_existing_stereo_image() {
        // pan_center=0.0, pan_width=0.0 (the defaults) must be a no-op -
        // regression test for the risk that adding per-voice panning could
        // quietly undo Stereo Width's already-baked L/R difference.
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 1.0, 1);

        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.6f32; 4096], vec![0.3f32; 4096]],
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
            assert!((out_left[i] - 0.6).abs() < 1e-3, "got {}", out_left[i]);
            assert!((out_right[i] - 0.3).abs() < 1e-3, "got {}", out_right[i]);
        }
    }

    #[test]
    fn hard_pan_silences_the_opposite_channel() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.set_pan_settings(1.0, 0.0); // hard right, no randomization
        vm.note_on(60, 0, 1.0, 1);

        let buffer = Arc::new(LoopBufferData {
            channels: vec![vec![0.6f32; 4096], vec![0.3f32; 4096]],
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
            assert!(out_left[i].abs() < 1e-4, "hard-right pan should silence the left channel entirely, got {}", out_left[i]);
            assert!(
                (out_right[i] - 0.3).abs() < 1e-3,
                "hard-right pan should leave the right channel's own content unchanged, got {}",
                out_right[i]
            );
        }
    }

    #[test]
    fn pan_randomizer_stays_within_configured_width_and_varies() {
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.set_pan_settings(0.0, 0.5);

        let mut pans = Vec::new();
        for i in 0..40 {
            vm.note_on(60, 0, 1.0, i);
            let pan = vm.voices.iter().find_map(|v| v.as_ref()).map(|v| v.pan).unwrap();
            pans.push(pan);
            vm.choke_all();
        }

        assert!(pans.iter().all(|&p| (-0.5..=0.5).contains(&p)), "all pans should stay within the configured width: {pans:?}");
        let distinct = pans.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6);
        assert!(distinct, "expected pan values to vary across notes, got all identical: {pans:?}");
    }
}
