#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Sustain,
    Release,
}

/// Simple two-stage Attack/Release envelope. The frozen loop content has no
/// transient to shape (it's a static, already-smooth spectral texture), so
/// AR is enough to avoid clicks on note-on/note-off without the complexity
/// of a full ADSR.
pub struct ArEnvelope {
    level: f32,
    attack_incr: f32,
    release_coeff: f32,
    stage: Stage,
}

impl ArEnvelope {
    pub fn new(sample_rate: f32, attack_ms: f32, release_ms: f32) -> Self {
        let attack_samples = (sample_rate * attack_ms / 1000.0).max(1.0);
        let release_samples = (sample_rate * release_ms / 1000.0).max(1.0);
        Self {
            level: 0.0,
            attack_incr: 1.0 / attack_samples,
            // Exponential decay reaching ~ -80dB (1e-4) over release_samples.
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
    fn ar_envelope_monotonic_bounds() {
        let sample_rate = 48000.0;
        let mut env = ArEnvelope::new(sample_rate, 10.0, 150.0);
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
    fn note_off_before_note_on_is_a_no_op() {
        let mut env = ArEnvelope::new(48000.0, 10.0, 150.0);
        env.note_off();
        assert!(env.is_finished());
        assert!(!env.is_releasing());
    }
}
