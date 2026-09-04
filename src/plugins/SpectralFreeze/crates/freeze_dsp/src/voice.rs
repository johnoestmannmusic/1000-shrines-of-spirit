use crate::envelope::ArEnvelope;
use crate::render::LoopBufferData;
use crate::resample::{playback_rate, PlaybackReader};

pub const MAX_VOICES: usize = 16;
pub const ATTACK_MS: f32 = 10.0;
pub const RELEASE_MS: f32 = 150.0;

pub struct Voice {
    pub id: i32,
    pub note: u8,
    pub channel: u8,
    pub gain: f32,
    pub rate: f64,
    pub reader: PlaybackReader,
    pub env: ArEnvelope,
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
}

impl VoiceManager {
    pub fn new(sample_rate: f32, root_note: u8) -> Self {
        Self {
            voices: (0..MAX_VOICES).map(|_| None).collect(),
            sample_rate,
            root_note,
            clock: 0,
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }

    pub fn note_on(&mut self, note: u8, channel: u8, velocity: f32, id: i32) {
        let rate = playback_rate(note, self.root_note);
        let mut env = ArEnvelope::new(self.sample_rate, ATTACK_MS, RELEASE_MS);
        env.note_on();
        let voice = Voice {
            id,
            note,
            channel,
            gain: velocity,
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
    pub fn process_block(&mut self, buffer: &LoopBufferData, out_left: &mut [f32], out_right: &mut [f32]) {
        debug_assert_eq!(out_left.len(), out_right.len());
        for sample in out_left.iter_mut() {
            *sample = 0.0;
        }
        for sample in out_right.iter_mut() {
            *sample = 0.0;
        }

        let Some(left_channel) = buffer.channels.first() else {
            return;
        };
        let right_channel = buffer.channels.get(1).unwrap_or(left_channel);

        for slot in self.voices.iter_mut() {
            let Some(voice) = slot else { continue };
            for i in 0..out_left.len() {
                let (l, r) = voice.reader.read_stereo_and_advance(left_channel, right_channel, voice.rate);
                let level = voice.env.advance();
                out_left[i] += l * level * voice.gain;
                out_right[i] += r * level * voice.gain;
            }
            if voice.env.is_finished() {
                *slot = None;
            }
        }
        self.clock += out_left.len() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::DEFAULT_ROOT_NOTE;

    fn make_buffer() -> LoopBufferData {
        LoopBufferData {
            channels: vec![vec![0.5f32; 4096], vec![0.5f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        }
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

        let buffer = LoopBufferData {
            channels: vec![vec![0.7f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        };
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
    fn distinct_channels_produce_distinct_output() {
        // VoiceManager itself must faithfully reproduce whatever difference
        // the (already width-shaped) buffer contains - it does no width
        // processing of its own.
        let mut vm = VoiceManager::new(48000.0, DEFAULT_ROOT_NOTE);
        vm.note_on(60, 0, 1.0, 1);

        let buffer = LoopBufferData {
            channels: vec![vec![1.0f32; 4096], vec![0.5f32; 4096]],
            sample_rate: 48000.0,
            root_note: DEFAULT_ROOT_NOTE,
        };
        let mut out_left = vec![0.0f32; 4096];
        let mut out_right = vec![0.0f32; 4096];
        for _ in 0..5 {
            vm.process_block(&buffer, &mut out_left, &mut out_right);
        }

        let tail_start = out_left.len() - 100;
        for i in tail_start..out_left.len() {
            assert!((out_left[i] - 1.0).abs() < 1e-3, "got {}", out_left[i]);
            assert!((out_right[i] - 0.5).abs() < 1e-3, "got {}", out_right[i]);
        }
    }
}
