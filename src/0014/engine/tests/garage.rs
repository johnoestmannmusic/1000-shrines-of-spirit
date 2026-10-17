//! 2-step garage: the drums keep the genre's shape, swing as asked, and
//! never touch the other modes.

use shrine0014::drums::{Drums, Section, Voice};
use shrine0014::garage::GarageSetup;
use shrine0014::harmony::{Harmony, Pace, Scale};
use shrine0014::kits::{DrumSpace, Kit};
use shrine0014::modulate::Mods;
use shrine0014::rng::Rng;
use shrine0014::tempo::Tempo;
use shrine0014::{Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

fn harmony(tempo: Tempo) -> Harmony {
    let (mut a, mut b) = (Rng::stream(1000, 0xD0), Rng::stream(1000, 0xD7));
    Harmony::new(Scale::Lydian, 2, Pace::Garage, tempo.beat(), 2, &mut a, &mut b)
}

/// Each hit's step (0..32) and how far after its step's start it falls, as a
/// fraction of a sixteenth.
fn steps(d: &Drums, six: u64) -> Vec<(usize, f64, Voice)> {
    d.hits()
        .iter()
        .map(|&(pos, v)| {
            let step = (pos as u64 + six / 2) / six;
            (step as usize, (pos as f64 - (step * six) as f64) / six as f64, v)
        })
        .collect()
}

#[test]
fn the_kick_skips_beat_three_and_the_claps_land_on_two_and_four() {
    let tempo = Tempo::new(132);
    let h = harmony(tempo);
    for seed in 0..24u64 {
        for kit in [Kit::Acoustic, Kit::SubClicks, Kit::GlassPings] {
            let d = Drums::garage(seed, kit, DrumSpace::Centred, &h, tempo, 0.58);
            let hits = steps(&d, tempo.drum16);
            let at = |step: usize, v: Voice| hits.iter().any(|h| h.0 == step && h.2 == v);
            for bar in [0, 16] {
                assert!(at(bar, Voice::Kick), "seed {seed} {kit:?}: no kick on the one of bar {}", bar / 16 + 1);
                assert!(!at(bar + 8, Voice::Kick), "seed {seed} {kit:?}: a kick on beat three — that is not 2-step");
                assert!(at(bar + 4, Voice::Snare) && at(bar + 12, Voice::Snare), "seed {seed} {kit:?}: claps off two and four");
            }
            // The skip: a second kick in each bar, after beat three's place.
            assert!((9..=14).any(|s| at(s, Voice::Kick)), "seed {seed}: bar 1 never skips");
            assert!((9..=14).any(|s| at(16 + s, Voice::Kick)), "seed {seed}: bar 2 never skips");
            assert!(d.pattern_name().starts_with("2-step"));
        }
    }
}

#[test]
fn the_off_sixteenths_swing_by_the_setting() {
    let tempo = Tempo::new(132);
    let h = harmony(tempo);
    for swing in [0.5, 0.58, 0.66] {
        let d = Drums::garage(7, Kit::Acoustic, DrumSpace::Centred, &h, tempo, swing);
        let push = 2.0 * swing - 1.0;
        let jitter = 25.0 / tempo.drum16 as f64;
        for (step, late, _) in steps(&d, tempo.drum16) {
            let want = if step % 2 == 1 { push } else { 0.0 };
            assert!((late - want).abs() <= jitter, "swing {swing}: step {step} is {late:.3} late, wanted {want:.3}");
        }
    }
}

#[test]
fn garage_sections_are_its_own_and_the_loop_plays_in_order() {
    let tempo = Tempo::new(132);
    let h = harmony(tempo);
    let d = Drums::garage(3, Kit::SubClicks, DrumSpace::Wide, &h, tempo, 0.6);
    let mods = Mods::new(DEFAULT_SEEDS.s1);
    let mut seen = std::collections::BTreeSet::new();
    for bar in 0..400u64 {
        let plan = d.plan(bar, &mods);
        assert!(!matches!(plan.section, Section::HalfTime | Section::Full | Section::Rolls), "a jungle section in garage");
        seen.insert(plan.section.name());
        if matches!(plan.section, Section::TwoStep) && !plan.fill {
            let in_order = plan.steps.iter().enumerate().filter(|(i, s)| s.slice as u64 == (bar % 2) * 16 + *i as u64).count();
            assert!(in_order >= 12, "bar {bar}: the groove is edited beyond recognition ({in_order}/16)");
        }
    }
    assert!(seen.contains("2-step"), "{seen:?}");
}

fn render(settings: Settings, seconds: usize) -> Vec<f32> {
    let mut t = Track::new(DEFAULT_SEEDS, settings);
    let mut out = vec![0f32; SAMPLE_RATE as usize * 2 * seconds];
    for chunk in out.chunks_mut(4096 * 2) {
        t.render(chunk);
    }
    out
}

#[test]
fn a_garage_track_is_clean_and_the_swing_is_heard() {
    let garage = Settings { pace: Pace::Garage, bpm: 132, ..DEFAULT_SETTINGS };
    let a = render(garage, 30);
    assert!(a.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "a garage render is not clean");
    assert!(a.iter().any(|x| x.abs() > 0.01), "a garage render is silent");
    let straight = Settings { garage: GarageSetup { swing: 50, ..GarageSetup::DEFAULT }, ..garage };
    assert_ne!(a, render(straight, 30), "the swing changed nothing");
    // The garage settings mean nothing outside the mode.
    let jungle = Settings { pace: Pace::Jungle, ..DEFAULT_SETTINGS };
    assert_eq!(render(jungle, 12), render(Settings { garage: GarageSetup { swing: 66, ..GarageSetup::DEFAULT }, ..jungle }, 12));
}
