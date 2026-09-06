#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Attack/Decay/Sustain/Release envelope. Decay and Release both use the
/// same exponential-approach shape (reaching within ~1e-4 of their target
/// over the given time), just with different targets: Decay approaches
/// `sustain_level` from 1.0, Release approaches 0.0 from whatever level the
/// envelope was actually at when note-off happened (not assumed to be 1.0
/// or `sustain_level` - a note released mid-attack or mid-decay releases
/// from its current level, not from a stage-boundary value).
pub struct AdsrEnvelope {
    level: f32,
    attack_incr: f32,
    decay_coeff: f32,
    sustain_level: f32,
    release_coeff: f32,
    stage: Stage,
}

impl AdsrEnvelope {
    pub fn new(sample_rate: f32, attack_ms: f32, decay_ms: f32, sustain_level: f32, release_ms: f32) -> Self {
        let attack_samples = (sample_rate * attack_ms / 1000.0).max(1.0);
        let decay_samples = (sample_rate * decay_ms / 1000.0).max(1.0);
        let release_samples = (sample_rate * release_ms / 1000.0).max(1.0);
        Self {
            level: 0.0,
            attack_incr: 1.0 / attack_samples,
            // Exponential approach reaching ~ -80dB (1e-4) of the remaining
            // distance to the target over decay_samples/release_samples.
            decay_coeff: (-9.2103_f32 / decay_samples).exp(),
            sustain_level: sustain_level.clamp(0.0, 1.0),
            release_coeff: (-9.2103_f32 / release_samples).exp(),
            stage: Stage::Idle,
        }
    }

    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Advances the envelope by one sample and returns the current level.
    pub fn advance(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_incr;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = self.sustain_level + (self.level - self.sustain_level) * self.decay_coeff;
                if (self.level - self.sustain_level).abs() < 1e-4 {
                    self.level = self.sustain_level;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.level *= self.release_coeff;
                if self.level < 1e-4 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {}
        }
        self.level
    }

    pub fn is_finished(&self) -> bool {
        self.stage == Stage::Idle && self.level == 0.0
    }

    pub fn is_releasing(&self) -> bool {
        self.stage == Stage::Release
    }

    pub fn level(&self) -> f32 {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adsr_envelope_monotonic_through_attack_and_release() {
        let sample_rate = 48000.0;
        let mut env = AdsrEnvelope::new(sample_rate, 10.0, 50.0, 1.0, 150.0);
        assert!(env.is_finished());

        env.note_on();
        let mut prev = 0.0f32;
        let mut reached_one = false;
        for _ in 0..(sample_rate as usize) {
            let level = env.advance();
            assert!(level >= prev - 1e-6, "attack must be monotonically non-decreasing");
            prev = level;
            if level >= 1.0 {
                reached_one = true;
                break;
            }
        }
        assert!(reached_one, "attack should reach 1.0 within one second");

        env.note_off();
        assert!(env.is_releasing());
        let mut prev = 1.0f32;
        let mut finished = false;
        for _ in 0..(sample_rate as usize) {
            let level = env.advance();
            assert!(level <= prev + 1e-6, "release must be monotonically non-increasing");
            prev = level;
            if env.is_finished() {
                finished = true;
                break;
            }
        }
        assert!(finished, "release should reach 0 and finish within one second");
    }

    #[test]
    fn decay_settles_at_sustain_level_not_zero() {
        let sample_rate = 48000.0;
        let sustain_level = 0.6;
        let mut env = AdsrEnvelope::new(sample_rate, 5.0, 50.0, sustain_level, 150.0);
        env.note_on();

        // Run well past attack + decay (Decay reaches within 1e-4 of target
        // in ~9.21 decay-time-constants - 50ms decay is nowhere near 1s).
        let mut level = 0.0;
        for _ in 0..(sample_rate as usize) {
            level = env.advance();
        }
        assert!((level - sustain_level).abs() < 1e-3, "expected to settle at sustain_level={}, got {}", sustain_level, level);

        // And it should stay there indefinitely (Sustain has no timeout).
        for _ in 0..1000 {
            level = env.advance();
        }
        assert!((level - sustain_level).abs() < 1e-3);
    }

    #[test]
    fn note_off_during_decay_releases_from_current_level_not_from_sustain_or_peak() {
        let sample_rate = 48000.0;
        // A long decay so note-off definitely lands mid-decay, not after it
        // has already settled at the sustain level.
        let mut env = AdsrEnvelope::new(sample_rate, 1.0, 2000.0, 0.1, 50.0);
        env.note_on();
        for _ in 0..1000 {
            env.advance();
        }
        let level_at_release = env.level();
        assert!(level_at_release > 0.15, "expected note-off to land well above the sustain level of 0.1 while still decaying, got {}", level_at_release);

        env.note_off();
        let first_release_level = env.advance();
        assert!(
            first_release_level <= level_at_release + 1e-6,
            "release must start from wherever the envelope actually was, not jump to the peak or sustain level first"
        );
    }

    #[test]
    fn sustain_level_at_max_behaves_like_the_old_attack_release_envelope() {
        // sustain_level = 1.0 should make Decay a no-op (it's already at its
        // target the instant Attack finishes) - preserves the exact old
        // ArEnvelope behavior as the default, regardless of decay_ms.
        let sample_rate = 48000.0;
        let mut env = AdsrEnvelope::new(sample_rate, 10.0, 9999.0, 1.0, 150.0);
        env.note_on();
        let mut reached_one_immediately_settled = false;
        for _ in 0..(sample_rate as usize) {
            let level = env.advance();
            if level >= 1.0 {
                // The very next sample should already be in Sustain, not
                // still decaying away from 1.0 despite the huge decay_ms.
                let next = env.advance();
                assert!((next - 1.0).abs() < 1e-6, "expected to hold at 1.0 immediately, got {}", next);
                reached_one_immediately_settled = true;
                break;
            }
        }
        assert!(reached_one_immediately_settled);
    }

    #[test]
    fn note_off_before_note_on_is_a_no_op() {
        let mut env = AdsrEnvelope::new(48000.0, 10.0, 50.0, 1.0, 150.0);
        env.note_off();
        assert!(env.is_finished());
        assert!(!env.is_releasing());
    }
}
