use std::f32::consts::PI;

pub const DEFAULT_STEREO_WIDTH_PCT: f32 = 0.0;

/// Maps the Stereo Width parameter (0-100%) to a 0.0-1.0 blend/spread
/// amount used by `render::render_frozen_loop`.
pub fn width_multiplier(stereo_width_pct: f32) -> f32 {
    stereo_width_pct.clamp(0.0, 100.0) / 100.0
}

/// Deterministic per-bin phase spread (covering (-pi, pi]) used to
/// synthetically decorrelate the right channel from the left for the
/// Stereo Width control. A mid-side transform on already-rendered audio can
/// only ever *reveal* difference that already exists between channels - it
/// is mathematically incapable of adding width when the source's L/R are
/// identical or highly correlated (this project's own test sample turned
/// out to have bit-identical L/R channels, which is what first exposed the
/// gap). Adding a frequency-domain phase offset to the right channel's
/// resynthesis instead works regardless of the source's natural
/// correlation, since it changes the right channel's actual content rather
/// than rescaling a difference that might not exist. Golden-ratio stepping
/// spreads the offsets across bins without periodic/comb-like artifacts
/// that a simple linear or small-integer-multiple stepping would produce.
pub fn decorrelation_spread(bin: usize) -> f32 {
    const GOLDEN_RATIO_CONJUGATE: f32 = 0.618_034;
    let frac = (bin as f32 * GOLDEN_RATIO_CONJUGATE).fract();
    (frac - 0.5) * 2.0 * PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_multiplier_clamps_and_scales_linearly() {
        assert_eq!(width_multiplier(0.0), 0.0);
        assert_eq!(width_multiplier(100.0), 1.0);
        assert!((width_multiplier(50.0) - 0.5).abs() < 1e-6);
        assert_eq!(width_multiplier(-20.0), 0.0);
        assert_eq!(width_multiplier(150.0), 1.0);
    }

    #[test]
    fn decorrelation_spread_is_deterministic_and_in_range() {
        for k in 0..2048 {
            let a = decorrelation_spread(k);
            let b = decorrelation_spread(k);
            assert_eq!(a, b, "must be a pure deterministic function of bin index");
            assert!(a > -PI - 1e-4 && a <= PI + 1e-4, "bin {} spread {} out of range", k, a);
        }
    }

    #[test]
    fn decorrelation_spread_varies_across_bins() {
        let values: Vec<f32> = (0..2048).map(decorrelation_spread).collect();
        let min = values.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max - min > PI,
            "expected a wide spread of offsets across bins, got range [{}, {}]",
            min,
            max
        );
    }
}
