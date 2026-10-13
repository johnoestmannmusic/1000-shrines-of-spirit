//! Weather must be a bounded, pure function of time, and each kind must read
//! distinctly.

use shrine0012::weather::{Weather, WeatherKind};

#[test]
fn state_is_pure_and_reproducible() {
    for kind in WeatherKind::ALL {
        let a = Weather::new(kind, 1000);
        let b = Weather::new(kind, 1000);
        for i in 0..500 {
            let t = i as f64 * 0.37;
            assert_eq!(a.state(t), b.state(t), "{kind:?} at t={t}");
        }
    }
}

#[test]
fn every_value_is_bounded() {
    for kind in WeatherKind::ALL {
        let w = Weather::new(kind, 7);
        for i in 0..2000 {
            let s = w.state(i as f64 * 1.13);
            for (name, v) in [
                ("wind", s.wind_strength),
                ("rain", s.rain),
                ("fog", s.fog),
                ("light", s.light),
            ] {
                assert!((0.0..=1.0).contains(&v), "{kind:?} {name}={v}");
            }
            assert!((0.0..1.0).contains(&s.wind_dir));
        }
    }
}

#[test]
fn rain_swells_and_eases_over_time() {
    let w = Weather::new(WeatherKind::Rain, 3);
    let samples: Vec<f64> = (0..4000).map(|i| w.state(i as f64 * 0.25).rain).collect();
    let lo = samples.iter().cloned().fold(f64::MAX, f64::min);
    let hi = samples.iter().cloned().fold(f64::MIN, f64::max);
    assert!(hi - lo > 0.15, "rain barely moves: {lo:.2}..{hi:.2}");
}

#[test]
fn calm_kinds_are_calmer_than_wild_ones() {
    let at = |kind, t| Weather::new(kind, 1).state(t);
    let clear = at(WeatherKind::Clear, 5.0);
    let storm = at(WeatherKind::Storm, 5.0);
    assert!(storm.rain > clear.rain);
    assert!(storm.wind_strength > clear.wind_strength);
    assert!(clear.light > storm.light);

    let fog = at(WeatherKind::Fog, 5.0);
    assert!(fog.fog > clear.fog);
}

#[test]
fn the_kind_never_changes() {
    let w = Weather::new(WeatherKind::Wind, 99);
    for i in 0..100 {
        let _ = w.state(i as f64 * 3.0);
        assert_eq!(w.kind, WeatherKind::Wind);
    }
}

#[test]
fn the_same_kind_differs_between_shrines() {
    let a = Weather::new(WeatherKind::Rain, 1);
    let b = Weather::new(WeatherKind::Rain, 2);
    let differ = (0..200).any(|i| {
        let t = i as f64;
        a.state(t).rain != b.state(t).rain
    });
    assert!(differ, "two shrines with the same weather cycle identically");
}
