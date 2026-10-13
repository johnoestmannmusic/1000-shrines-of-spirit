//! Frame-time budget: pick an effects quality from how fast frames render.
//!
//! The logical buffer and canvas never change size (`HC003`). Under load the
//! scene instead draws *fewer effects* — every other plant, less rain — so the
//! output contract stays fixed while the cost falls.

use crate::scene::Quality;

/// Observes frame durations and steps `Quality` up or down with hysteresis.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    quality: Quality,
    slow: u32,
    fast: u32,
}

impl Budget {
    pub fn new() -> Self {
        Budget { quality: Quality::High, slow: 0, fast: 0 }
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    /// Feed one frame's duration in milliseconds; returns the quality to use
    /// for the next frame. Slow frames accumulate toward a downgrade, fast ones
    /// toward an upgrade, so brief spikes never thrash the setting.
    pub fn observe(&mut self, frame_ms: f64) -> Quality {
        if frame_ms > 20.0 {
            self.slow += 1;
            self.fast = 0;
        } else if frame_ms < 12.0 {
            self.fast += 1;
            self.slow = 0;
        } else {
            self.slow = self.slow.saturating_sub(1);
            self.fast = self.fast.saturating_sub(1);
        }

        if self.slow >= 8 {
            self.quality = match self.quality {
                Quality::High => Quality::Medium,
                Quality::Medium => Quality::Low,
                Quality::Low => Quality::Low,
            };
            self.slow = 0;
        } else if self.fast >= 240 {
            self.quality = match self.quality {
                Quality::Low => Quality::Medium,
                Quality::Medium => Quality::High,
                Quality::High => Quality::High,
            };
            self.fast = 0;
        }
        self.quality
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}
