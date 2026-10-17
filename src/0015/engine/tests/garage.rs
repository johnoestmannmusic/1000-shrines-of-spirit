//! 2-step garage: the drums keep the genre's shape, swing as asked, and
//! never touch the other modes.

use shrine0015::drums::{Drums, Section, Voice};
use shrine0015::garage::GarageSetup;
use shrine0015::harmony::{Harmony, Pace, Scale};
use shrine0015::kits::{DrumSpace, Kit};
use shrine0015::modulate::Mods;
use shrine0015::rng::Rng;
use shrine0015::tempo::Tempo;
use shrine0015::{Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

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

#[test]
fn the_bassline_and_stabs_are_heard_and_can_be_turned_off() {
    use shrine0015::garage::{BassMode, Stabs, StabTone};
    let garage = Settings { pace: Pace::Garage, bpm: 132, ..DEFAULT_SETTINGS };
    let full = render(garage, 40);
    let set = |g: GarageSetup| render(Settings { garage: g, ..garage }, 40);
    let drone_bass = set(GarageSetup { bass: BassMode::Drone, ..GarageSetup::DEFAULT });
    let no_stabs = set(GarageSetup { stabs: Stabs::Off, ..GarageSetup::DEFAULT });
    let busy = set(GarageSetup { stabs: Stabs::Busy, ..GarageSetup::DEFAULT });
    let dark = set(GarageSetup { tone: StabTone::Dark, ..GarageSetup::DEFAULT });
    for (name, other) in [("drone bass", &drone_bass), ("no stabs", &no_stabs), ("busy stabs", &busy), ("dark stabs", &dark)] {
        assert_ne!(&full, other, "{name} sounds the same as the default garage");
        assert!(other.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{name} is not clean");
    }
}

#[test]
fn the_bassline_skips_with_the_groove_and_walks_into_changes() {
    use shrine0015::bass::BassLine;
    let line = BassLine::new(4, Tempo::new(132), 0.58);
    for bar in 0..64 {
        let notes = line.plan(bar, false);
        assert!(notes.len() >= 2, "bar {bar}: {notes:?}");
        assert!(notes.windows(2).all(|w| w[1].step > w[0].step + 1), "bar {bar}: notes crowd together");
        assert!(notes.iter().all(|n| !n.approach));
        let changing = line.plan(bar, true);
        assert!(changing.last().is_some_and(|n| n.approach && n.semis == -1.0), "bar {bar}: no walk into the change");
    }
    // Two bars make a figure: it comes round again until the phrase varies.
    assert_eq!(line.plan(0, false), line.plan(2, false));
}

#[test]
fn garage_chords_change_every_two_bars_on_the_downbeat() {
    let tempo = Tempo::new(132);
    for scale in [Scale::Ionian, Scale::Dorian, Scale::Aeolian, Scale::Lydian, shrine0015::harmony::Scale::PentatonicMinor] {
        for chords in 1..=4 {
            for seed in 0..10u64 {
                let h = Harmony::garage(scale, chords, tempo.beat(), 2, &mut Rng::new(seed), 0, 0);
                assert_eq!(h.chords.len(), chords);
                let two_bars = 2 * tempo.drum_bar();
                for (i, c) in h.chords.iter().enumerate() {
                    let start = i as u64 * two_bars;
                    assert_eq!(h.at(start).chord, i, "{scale:?}: chord {i} is not there on its downbeat");
                    assert_eq!(h.at(start + two_bars - 1).chord, i, "{scale:?}: chord {i} leaves early");
                    assert!(c.bass >= 24.0 && c.bass <= 45.0);
                }
                // The crossfade is only the last beat.
                if chords > 1 {
                    assert_eq!(h.at(two_bars - tempo.drum_bar() / 2).morph, 0.0);
                }
                // Nothing repeats back to back.
                assert!(h.chords.windows(2).all(|w| w[0].tones != w[1].tones), "{scale:?}: a chord repeats");
            }
        }
    }
}

#[test]
fn garage_layers_keep_out_of_each_others_way() {
    let tempo = Tempo::new(132);
    for scale in [Scale::Ionian, Scale::Dorian, Scale::Aeolian] {
        for key in 0..12u8 {
            // The drone is an open fifth on the tonic: nothing to rub against a chord.
            let drone = Harmony::pedal(scale, tempo.beat(), key);
            for n in &drone.chords[0].notes {
                let iv = (*n as i64 - key as i64).rem_euclid(12);
                assert!(iv == 0 || iv == 7, "{scale:?} key {key}: the drone holds {n}, not a root or fifth");
            }
            let g = Harmony::garage(scale, 4, tempo.beat(), key, &mut Rng::new(key as u64), 0, 0);
            assert!(g.garage);
            // The loops' shells sit above the stabs' middle register and below F6.
            for c in &g.chords {
                assert!(c.notes[0] >= 60.0 && c.notes.iter().all(|m| *m <= 87.0), "{scale:?}: loops at {:?}", c.notes);
            }
            // The glitch layers pitch only to the root, second and fifth.
            let cfg = shrine0015::glitch_setup::GlitchSetup::DEFAULT_0.layer_config(0, &g, tempo);
            assert_eq!(cfg.ping_ratio, 3.0);
            let iv = scale.intervals();
            for m in &cfg.notes {
                let d = (*m as i64 - key as i64).rem_euclid(12) as u8;
                assert!([iv[0], iv[1], iv[4]].contains(&d), "{scale:?}: glitch note {m}");
            }
        }
    }
    // The slow modes keep their inharmonic pings and every scale note.
    let (mut a, mut b) = (Rng::stream(1, 0xD0), Rng::stream(1, 0xD7));
    let h = Harmony::new(Scale::Lydian, 3, Pace::HalfTime, tempo.beat(), 2, &mut a, &mut b);
    assert_eq!(shrine0015::glitch_setup::GlitchSetup::DEFAULT_0.layer_config(0, &h, tempo).ping_ratio, 0.0);
}

#[test]
fn the_claps_sit_in_the_middle() {
    // The jungle rule throws snare hits 45-65% to a random side; garage's
    // claps are the backbeat and stay centred whatever the seed.
    let tempo = Tempo::new(132);
    let h = harmony(tempo);
    let mods = Mods::new(1);
    let six = tempo.drum16 as usize;
    for seed in 0..6u64 {
        let mut d = Drums::garage(seed, Kit::SubClicks, DrumSpace::Wide, &h, tempo, 0.58);
        let claps: Vec<usize> = d.hits().iter().filter(|h| h.1 == Voice::Snare).map(|h| h.0).collect();
        let (mut el, mut er) = (vec![0.0f64; claps.len()], vec![0.0f64; claps.len()]);
        for clock in 0..(24 * 16 * six) as u64 {
            let (l, r, _) = d.next(clock, &mods);
            let at = clock as usize % (32 * six);
            for (k, c) in claps.iter().enumerate() {
                if at >= *c && at < c + six {
                    el[k] += l * l;
                    er[k] += r * r;
                }
            }
        }
        for k in 0..claps.len() {
            let ratio = (el[k] / er[k]).sqrt();
            assert!((0.95..1.05).contains(&ratio), "seed {seed}: clap {k} leans {ratio:.2} left/right");
        }
    }
}

#[test]
fn garage_edits_keep_every_hit_on_its_swung_grid() {
    let tempo = Tempo::new(132);
    let h = harmony(tempo);
    let mods = Mods::new(1);
    for seed in 0..8u64 {
        let d = Drums::garage(seed, Kit::SubClicks, DrumSpace::Wide, &h, tempo, 0.6);
        // Every hit starts inside its own slice.
        let six = tempo.drum16 as usize;
        for (pos, v) in d.hits() {
            let slice = (pos + six / 2) / six;
            assert!(*pos >= slice * six, "seed {seed}: a {v:?} starts before its slice");
        }
        // A moved slice keeps the step's place in the swing — except the
        // reversed clap swelling into the beat after it.
        for bar in 0..300u64 {
            let p = d.plan(bar, &mods);
            for (i, s) in p.steps.iter().enumerate() {
                let reversed_clap = s.op == shrine0015::drums::Op::Reverse && i % 8 == 3;
                assert!(s.slice as usize % 2 == i % 2 || reversed_clap, "seed {seed} bar {bar} step {i}: {s:?} is off its swing");
            }
        }
    }
}

#[test]
fn every_named_progression_plays_its_chords_in_every_key() {
    use shrine0015::harmony::PROGRESSIONS;
    let tempo = Tempo::new(132);
    for (i, p) in PROGRESSIONS.iter().enumerate() {
        for key in 0..12u8 {
            let scale = if p.major { Scale::Ionian } else { Scale::Aeolian };
            let h = Harmony::garage(scale, 4, tempo.beat(), key, &mut Rng::new(1), i + 1, 0);
            assert_eq!(h.chords.len(), 4);
            for (c, (root, q)) in h.chords.iter().zip(p.chords) {
                let want: Vec<u8> = q.intervals().iter().map(|x| (key + root + x) % 12).collect();
                assert_eq!(c.tones, want, "{} in key {key}", p.name);
                assert_eq!((c.bass as i64).rem_euclid(12) as u8, (key + root) % 12, "{}: the bass is not the root", p.name);
                assert!(c.notes[0] >= 60.0 && c.notes.iter().all(|m| *m <= 87.0));
            }
            // The stabs voice every chord as four close notes, moving smoothly.
            let v = shrine0015::stabs::voicings(&h);
            for (k, chord) in v.iter().enumerate() {
                assert_eq!(chord.len(), 4, "{}", p.name);
                if k > 0 {
                    let moved: f64 = chord.iter().zip(&v[k - 1]).map(|(a, b)| (a - b).abs()).sum();
                    assert!(moved <= 12.0, "{} key {key}: chord {k} leaps {moved}", p.name);
                }
            }
        }
    }
}

#[test]
fn the_seed_picks_a_progression_of_the_scales_own_family() {
    use shrine0015::harmony::PROGRESSIONS;
    let tempo = Tempo::new(132);
    for (scale, major) in [(Scale::Ionian, true), (Scale::Yonanuki, true), (Scale::Ryukyu, true), (Scale::Dorian, false), (Scale::Aeolian, false)] {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..60u64 {
            let h = Harmony::garage(scale, 4, tempo.beat(), 0, &mut Rng::new(seed), 0, 0);
            let tones: Vec<Vec<u8>> = h.chords.iter().map(|c| c.tones.clone()).collect();
            let p = PROGRESSIONS.iter().position(|p| p.chords.iter().zip(&tones).all(|((r, q), t)| q.intervals().iter().map(|x| (r + x) % 12).collect::<Vec<_>>() == *t)).expect("not a named progression");
            assert_eq!(PROGRESSIONS[p].major, major, "{scale:?} picked {}", PROGRESSIONS[p].name);
            seen.insert(p);
        }
        assert!(seen.len() >= 3, "{scale:?}: the seed only ever picks {seen:?}");
    }
}

#[test]
fn the_key_lift_steps_up_a_whole_tone_and_comes_home() {
    let tempo = Tempo::new(132);
    let h = Harmony::garage(Scale::Ionian, 4, tempo.beat(), 2, &mut Rng::new(3), 1, 4);
    assert_eq!(h.chords.len(), 8);
    let two_bars = 2 * tempo.drum_bar();
    // Four passes at home, then four a whole tone up, then home again.
    for pass in 0..10u64 {
        let c = h.at(pass * 4 * two_bars).chord;
        let lifted = (pass / 4) % 2 == 1;
        assert_eq!(c, if lifted { 4 } else { 0 }, "pass {pass}");
    }
    for i in 0..4 {
        let (home, up) = (&h.chords[i].tones, &h.chords[i + 4].tones);
        assert!(home.iter().zip(up).all(|(a, b)| (a + 2) % 12 == *b));
        assert!(h.starts_pass(0) && h.starts_pass(4) && !h.starts_pass(1));
    }
}

#[test]
fn the_drone_steps_back_under_chords_that_rub_against_it() {
    use shrine0015::harmony::PROGRESSIONS;
    let tempo = Tempo::new(132);
    // C major: the bright royal road's III7 (E7) has G#, a semitone from the drone's G.
    let bright = PROGRESSIONS.iter().position(|p| p.name == "Royal road, bright").unwrap();
    let h = Harmony::garage(Scale::Ionian, 4, tempo.beat(), 0, &mut Rng::new(1), bright + 1, 0);
    // Fmaj7 (F A C E) and Am7 sit on the C-G drone; G7's B rubs against its
    // C, and E7's G# against its G (and B against C).
    assert_eq!((0..4).map(|i| h.rubs_pedal(i)).collect::<Vec<_>>(), vec![false, true, true, false]);
}

#[test]
fn the_loops_change_chord_on_the_downbeat_with_the_bass() {
    let tempo = Tempo::new(132);
    let h = Harmony::garage(Scale::Ionian, 4, tempo.beat(), 2, &mut Rng::new(5), 1, 4);
    let two_bars = 2 * tempo.drum_bar();
    for step in 1..40u64 {
        let down = step * two_bars;
        let (before, now) = (h.at(down - 1).chord, h.at(down).chord);
        // Up to the downbeat the loops are wholly on the chord the bass is on...
        assert_eq!(h.loop_mix(down - 1), (before, before, 0.0), "step {step}: the loops moved early");
        // ...and from it they move with the bass, done within a sixteenth.
        let (from, to, _) = h.loop_mix(down);
        assert_eq!((from, to), (before, now));
        assert_eq!(h.loop_mix(down + tempo.drum16), (now, now, 0.0));
    }
    // The slow modes keep their bar-long crossfade exactly.
    let (mut a, mut b) = (Rng::stream(1, 0xD0), Rng::stream(1, 0xD7));
    let slow = Harmony::new(Scale::Lydian, 3, Pace::HalfTime, tempo.beat(), 2, &mut a, &mut b);
    for t in (0..slow.cycle).step_by(4099) {
        let p = slow.at(t);
        assert_eq!(slow.loop_mix(t), (p.chord, p.next, p.morph));
    }
}
