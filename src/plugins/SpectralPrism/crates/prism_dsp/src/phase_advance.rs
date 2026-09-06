use std::f32::consts::PI;

/// Wraps `x` into (-pi, pi]. Implemented via floored modulo rather than a
/// naive `%` + branch: the naive form is numerically fragile exactly at the
/// +-pi boundary (float rounding can land just on the wrong side of the
/// branch, e.g. -3*pi should wrap to +pi but a `%`-based check can miss it),
/// which matters here because phase values legitimately land near this
/// boundary for real audio.
pub fn principal_value(x: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let a = PI - x;
    let floored_mod = a - two_pi * (a / two_pi).floor();
    PI - floored_mod
}

/// Per-bin measured true phase advance between two analysis frames `hop`
/// samples apart, unwrapped against the naive theoretical bin-center rate.
/// See src/0006/index.html lines 3365-3377: the naive rate `2*pi*k*hop/n`
/// degenerates into a periodic (and audibly buzzy) rate at exactly 50% hop,
/// so the true advance is measured from two real analyzed frames instead.
pub fn compute_advance(phase0: &[f32], phase1: &[f32], hop: usize, fft_size: usize) -> Vec<f32> {
    assert_eq!(phase0.len(), phase1.len());
    let n = fft_size as f32;
    (0..phase0.len())
        .map(|k| {
            let nominal = 2.0 * PI * k as f32 * hop as f32 / n;
            nominal + principal_value(phase1[k] - phase0[k] - nominal)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_value_table() {
        let cases: [(f32, f32); 5] = [
            (0.0, 0.0),
            (PI, PI),
            (-PI, PI),
            (3.0 * PI, PI),
            (-3.0 * PI, PI),
        ];
        for (input, expected) in cases {
            let v = principal_value(input);
            assert!(
                (v - expected).abs() < 1e-4,
                "principal_value({}) = {}, expected {}",
                input,
                v,
                expected
            );
            assert!(v > -PI - 1e-4 && v <= PI + 1e-4);
        }
    }

    #[test]
    fn advance_matches_known_frequency_bin_aligned() {
        // A tone sitting exactly on bin k has a true phase advance per hop
        // equal to the naive theoretical rate (the degenerate case the
        // source comment describes - it's only a problem in general because
        // real audio essentially never sits exactly on a bin center).
        let n = 2048usize;
        let hop = 1024usize;
        let k = 40usize;
        let nominal = 2.0 * PI * k as f32 * hop as f32 / n as f32;

        // Two phase snapshots consistent with a bin-aligned sinusoid: phase
        // advances by exactly `nominal` between the frames.
        let phase0 = vec![0.3f32; n / 2 + 1];
        let mut phase1 = phase0.clone();
        phase1[k] = principal_value(phase0[k] + nominal);

        let advance = compute_advance(&phase0, &phase1, hop, n);
        assert!(
            (advance[k] - nominal).abs() < 1e-4,
            "expected advance {} to match nominal rate {}",
            advance[k],
            nominal
        );
    }

    #[test]
    fn advance_matches_detuned_frequency_not_naive_rate() {
        // A tone detuned between bins (k + 0.3 bins) has a true phase
        // advance different from the naive bin-center rate 2*pi*k*hop/n.
        // This is the regression test for the ~21.5Hz buzz bug the
        // algorithm's measured-advance approach avoids at 50% hop.
        let n = 2048usize;
        let hop = 1024usize;
        let k = 40usize;
        let detune_bins = 0.3f32;
        let true_freq_bins = k as f32 + detune_bins;
        let true_advance = 2.0 * PI * true_freq_bins * hop as f32 / n as f32;
        let naive_rate = 2.0 * PI * k as f32 * hop as f32 / n as f32;

        let phase0 = vec![0.0f32; n / 2 + 1];
        let mut phase1 = phase0.clone();
        phase1[k] = principal_value(phase0[k] + true_advance);

        let advance = compute_advance(&phase0, &phase1, hop, n);

        // Matches the true advance (wrapped into an equivalent principal
        // range relative to the nominal rate)...
        let nominal = naive_rate;
        let expected = nominal + principal_value(true_advance - nominal);
        assert!(
            (advance[k] - expected).abs() < 1e-4,
            "advance {} did not match expected true rate {}",
            advance[k],
            expected
        );

        // ...and does NOT match the naive rate, since true_advance was
        // constructed to differ from it by more than a full period wrap.
        assert!(
            (advance[k] - naive_rate).abs() > 1e-3,
            "advance should not equal the naive bin-center rate for a detuned tone"
        );
    }
}
