//! Microsound: the pulse sits exactly on the grid, every setting is heard,
//! the output never depends on how it is rendered, and the other modes
//! never hear any of it.

use shrine0015::drums::{Drums, Voice};
use shrine0015::harmony::{Harmony, Pace, Scale};
use shrine0015::kits::{DrumSpace, Kit};
use shrine0015::micro::{Dropouts, Grains, MicroSetup, Pips, Pulse};
use shrine0015::rng::Rng;
use shrine0015::tempo::Tempo;
use shrine0015::{Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

fn harmony(tempo: Tempo) -> Harmony {
    let (mut a, mut b) = (Rng::stream(1000, 0xD0), Rng::stream(1000, 0xD7));
    Harmony::new(Scale::PentatonicMinor, 2, Pace::Micro, tempo.beat(), 2, &mut a, &mut b)
}

fn micro() -> Settings {
    let mut s = DEFAULT_SETTINGS;
    s.set_mode(Pace::Micro);
    s
}

fn render(settings: Settings, seconds: usize, block: usize) -> Vec<f32> {
    let mut t = Track::new(DEFAULT_SEEDS, settings);
    let mut out = vec![0f32; seconds * SAMPLE_RATE as usize * 2];
    for chunk in out.chunks_mut(block * 2) {
        t.render(chunk);
    }
    out
}

#[test]
fn the_pulse_is_four_on_the_floor_and_everything_is_on_the_grid() {
    let tempo = Tempo::new(120);
    let h = harmony(tempo);
    let six = tempo.drum16 as usize;
    for seed in 0..24u64 {
        for kit in [Kit::SubClicks, Kit::MicroBlips, Kit::DataDust] {
            let d = Drums::micro(seed, kit, DrumSpace::Wide, &h, tempo, MicroSetup::DEFAULT);
            assert!(d.hits().iter().all(|(pos, _)| pos % six == 0), "seed {seed} {kit:?}: a hit off the grid");
            let kicks: Vec<usize> = d.hits().iter().filter(|h| h.1 == Voice::Kick).map(|h| h.0 / six).collect();
            // Bar A's four beats always; bar B may leave one out, never the one.
            for step in [0, 4, 8, 12, 16] {
                assert!(kicks.contains(&step), "seed {seed} {kit:?}: no kick on step {step}");
            }
            assert!(kicks.iter().all(|s| s % 4 == 0), "seed {seed} {kit:?}: a kick off the beat");
            assert!(kicks.len() >= 7, "seed {seed} {kit:?}: more than one beat left out");
        }
        let off = MicroSetup { pulse: Pulse::Off, ..MicroSetup::DEFAULT };
        let d = Drums::micro(seed, Kit::SubClicks, DrumSpace::Wide, &h, tempo, off);
        assert!(d.hits().iter().all(|h| h.1 != Voice::Kick), "seed {seed}: a kick with the pulse off");
    }
}

#[test]
fn microsound_renders_the_same_however_it_is_cut() {
    let s = micro();
    let a = render(s, 30, 4096);
    assert!(a.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    assert!(a.iter().any(|x| x.abs() > 0.01), "microsound is silent");
    assert_eq!(a, render(s, 30, 333), "the output depends on the block size");
}

#[test]
fn every_microsound_setting_is_heard() {
    let base = render(micro(), 40, 4096);
    let changes = [
        ("pulse", MicroSetup { pulse: Pulse::Off, ..MicroSetup::DEFAULT }),
        ("grains", MicroSetup { grains: Grains::Dense, ..MicroSetup::DEFAULT }),
        ("pips", MicroSetup { pips: Pips::Busy, ..MicroSetup::DEFAULT }),
        ("dropouts", MicroSetup { dropouts: Dropouts::Many, ..MicroSetup::DEFAULT }),
    ];
    for (name, m) in changes {
        let s = Settings { micro: m, ..micro() };
        assert_ne!(render(s, 40, 4096), base, "{name} changed nothing");
    }
}

#[test]
fn the_other_modes_never_hear_the_microsound_settings() {
    let other = MicroSetup { pulse: Pulse::Off, grains: Grains::Dense, pips: Pips::Busy, dropouts: Dropouts::Many };
    for pace in [Pace::HalfTime, Pace::Jungle, Pace::Garage] {
        let mut s = DEFAULT_SETTINGS;
        s.set_mode(pace);
        assert_eq!(render(s, 6, 4096), render(Settings { micro: other, ..s }, 6, 4096), "{pace:?} heard the microsound settings");
    }
    assert_ne!(render(micro(), 6, 4096), render(DEFAULT_SETTINGS, 6, 4096), "microsound sounds like half-time");
}

#[test]
fn switching_modes_moves_only_untouched_defaults() {
    let mut s = DEFAULT_SETTINGS;
    s.set_mode(Pace::Micro);
    assert_eq!((s.bpm, s.scale, s.chords), Settings::mode_defaults(Pace::Micro));
    s.set_mode(Pace::Garage);
    assert_eq!((s.bpm, s.scale, s.chords), Settings::mode_defaults(Pace::Garage));
    s.set_mode(Pace::HalfTime);
    assert_eq!((s.bpm, s.scale, s.chords), (DEFAULT_SETTINGS.bpm, DEFAULT_SETTINGS.scale, DEFAULT_SETTINGS.chords));
    // A hand-picked tempo survives a change of mode.
    s.bpm = 140;
    s.set_mode(Pace::Micro);
    assert_eq!(s.bpm, 140);
}
