//! The setup's Randomize screen: roll any chosen groups of settings at once,
//! leaving everything else exactly as it was.
//!
//! Each group is randomised within ranges that suit making music, not the
//! full extremes the recipe can hold, so every roll is something you might
//! keep: tempos stay near the mode's own, glitch layers stay audible, the
//! architecture usually stays upright.

use shrine0016::arch::ArchTransform;
use shrine0016::atmos::{LoopTimbre, LOOP_TIMBRES};
use shrine0016::garage::{BASS_MODES, KEY_LIFTS, STABS, STAB_TONES};
use shrine0016::glitch_setup::GlitchSetup;
use shrine0016::harmony::{Pace, PACES, PROGRESSIONS, SCALES};
use shrine0016::kits::{Kit, KITS, SPACES};
use shrine0016::micro::{CHORD_RATES, DROPOUTS, DRONE_SHIFTS, GRAINS, PIPS, PULSES};
use shrine0016::plants::Biome;
use shrine0016::rng::Rng;
use shrine0016::sim::SimConfig;
use shrine0016::track::{DroneArc, KeyChoice, LoopDesign, Seeds, Settings};
use shrine0016::weather::WeatherKind;

/// One group of settings the screen can roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Seed,
    MusicSeeds,
    Biome,
    Weather,
    Mode,
    Harmony,
    Tempo,
    Drums,
    ModeDetails,
    Atmosphere,
    Drone,
    Glitches,
    MasterFx,
    Wildlife,
    Look,
    Architecture,
}

pub const GROUPS: [Group; 16] = [
    Group::Seed,
    Group::MusicSeeds,
    Group::Biome,
    Group::Weather,
    Group::Mode,
    Group::Harmony,
    Group::Tempo,
    Group::Drums,
    Group::ModeDetails,
    Group::Atmosphere,
    Group::Drone,
    Group::Glitches,
    Group::MasterFx,
    Group::Wildlife,
    Group::Look,
    Group::Architecture,
];

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Seed => "Shrine seed",
            Group::MusicSeeds => "Music seeds",
            Group::Biome => "Biome",
            Group::Weather => "Weather",
            Group::Mode => "Mode",
            Group::Harmony => "Scale, root & chords",
            Group::Tempo => "Tempo",
            Group::Drums => "Drum kit & space",
            Group::ModeDetails => "Mode details",
            Group::Atmosphere => "Atmosphere layers",
            Group::Drone => "Drone",
            Group::Glitches => "Glitch layers",
            Group::MasterFx => "Master FX",
            Group::Wildlife => "Wildlife",
            Group::Look => "Code overlay & camera",
            Group::Architecture => "Architecture",
        }
    }

    /// What it changes, in a line.
    pub fn blurb(self) -> &'static str {
        match self {
            Group::Seed => "the shrine, its plants, palette and weather cycles",
            Group::MusicSeeds => "all seven music seeds: chords, glitches, bass, drums, loops",
            Group::Biome => "what grows around the shrine, or which future surrounds it",
            Group::Weather => "the weather kind",
            Group::Mode => "half-time, jungle, 2-step garage or microsound (with its defaults)",
            Group::Harmony => "the scale, the root note and how many chords",
            Group::Tempo => "the tempo, near the mode's own",
            Group::Drums => "the kit, its stereo space and break reversals",
            Group::ModeDetails => "the garage or microsound settings of the current mode",
            Group::Atmosphere => "the four loop layers, their arcs and transpose",
            Group::Drone => "the drone's arc, tone filter and rhythm",
            Group::Glitches => "both glitch layers: sounds, density, rate, level, length",
            Group::MasterFx => "which end-of-chain effects are on",
            Group::Wildlife => "creatures on or off",
            Group::Look => "the code overlay and the camera's turn",
            Group::Architecture => "turn and move the shrine (usually upright)",
        }
    }
}

/// Seconds of a drone or loop arc to choose from (0 = always on).
const ARCS: [u16; 6] = [0, 0, 60, 120, 180, 300];

fn arc(r: &mut Rng) -> DroneArc {
    let cycle = r.pick(&ARCS);
    if cycle == 0 {
        DroneArc::ALWAYS_ON
    } else {
        DroneArc { cycle_s: cycle, hold_s: (cycle as f64 * r.range(0.4, 0.8)) as u16 }
    }
}

fn int(r: &mut Rng, lo: i64, hi: i64) -> i64 {
    lo + r.below((hi - lo + 1) as usize) as i64
}

/// The light kits microsound uses.
fn kits_for(pace: Pace) -> Vec<Kit> {
    KITS.into_iter().filter(|k| pace != Pace::Micro || k.is_light()).collect()
}

/// Roll every group in `groups` of `c`, leaving the rest untouched.
pub fn apply(c: &mut SimConfig, groups: &[Group], r: &mut Rng) {
    let has = |g: Group| groups.contains(&g);
    if has(Group::Seed) {
        c.seed = r.below(100_000) as u64;
    }
    if has(Group::MusicSeeds) {
        let mut s = || r.below(100_000) as u64;
        c.seeds = Seeds { s1: s(), s2: s(), s3: s(), s4: s(), s5: s().max(1), s6: s(), s7: s() };
    }
    if has(Group::Biome) {
        c.biome = r.pick(&Biome::ALL);
    }
    if has(Group::Weather) {
        c.weather = r.pick(&WeatherKind::ALL);
    }
    let s: &mut Settings = &mut c.settings;
    // The mode first: it moves tempo, scale and chords that were at its
    // defaults, which the groups below may then roll in turn.
    if has(Group::Mode) {
        s.set_mode(r.pick(&PACES));
        if s.pace == Pace::Micro && !s.kit.is_light() && s.kit != Kit::Off {
            s.kit = Kit::MicroBlips;
        }
    }
    if has(Group::Harmony) {
        s.scale = r.pick(&SCALES);
        s.key = if r.chance(0.3) { KeyChoice::Seed } else { KeyChoice::Note(r.below(12) as u8) };
        s.chords = int(r, 1, 4) as u8;
    }
    if has(Group::Tempo) {
        let (home, _, _) = Settings::mode_defaults(s.pace);
        let spread = (home as f64 * 0.12) as i64;
        s.bpm = int(r, home as i64 - spread, home as i64 + spread).clamp(70, 180) as u16;
    }
    if has(Group::Drums) {
        s.kit = r.pick(&kits_for(s.pace));
        s.space = r.pick(&SPACES);
        s.drum_reverse = r.chance(0.6);
    }
    if has(Group::ModeDetails) {
        match s.pace {
            Pace::Garage => {
                let g = &mut s.garage;
                g.swing = int(r, 54, 64) as u8;
                g.bass = r.pick(&BASS_MODES);
                g.stabs = r.pick(&STABS);
                g.tone = r.pick(&STAB_TONES);
                g.progression = r.below(PROGRESSIONS.len() + 1) as u8;
                g.lift = r.pick(&KEY_LIFTS);
                // A named progression needs all four chords.
                if g.progression > 0 {
                    s.chords = 4;
                }
            }
            Pace::Micro => {
                let m = &mut s.micro;
                m.pulse = r.pick(&PULSES);
                m.grains = r.pick(&GRAINS);
                m.pips = r.pick(&PIPS);
                m.dropouts = r.pick(&DROPOUTS);
                m.chords = r.pick(&CHORD_RATES);
                m.shift = r.pick(&DRONE_SHIFTS);
            }
            Pace::HalfTime | Pace::Jungle => {}
        }
    }
    if has(Group::Atmosphere) {
        let timbres: Vec<LoopTimbre> = LOOP_TIMBRES.into_iter().filter(|t| *t != LoopTimbre::Off).collect();
        // Two to four layers on.
        let on = int(r, 2, 4) as usize;
        for i in 0..4 {
            s.loops[i] = if i < on { LoopDesign::Sustained(r.pick(&timbres)) } else { LoopDesign::OFF };
            s.loop_arcs[i] = arc(r);
        }
        s.loop_transpose = r.pick(&[0i8, 0, 0, 12, -12, 7, -5]);
    }
    if has(Group::Drone) {
        s.drone = arc(r);
        s.drone_tone = r.pick(&[0u16, 0, 800, 1500, 3000, 6000]);
        s.drone_rhythm = r.chance(0.3);
        s.loop_rhythm = s.drone_rhythm && r.chance(0.5);
    }
    if has(Group::Glitches) {
        for g in s.glitch.iter_mut() {
            let mut kinds = [0; 6].map(|_| r.chance(0.6));
            if !kinds.iter().any(|k| *k) {
                kinds[r.below(6)] = true;
            }
            let (a, b) = (int(r, 2, 8) as u8, int(r, 2, 8) as u8);
            *g = GlitchSetup {
                enabled: r.chance(0.85),
                kinds,
                fill: int(r, 25, 75) as u8,
                rate: r.below(3) as u8,
                gain: int(r, 60, 130) as u8,
                steps: int(r, 5, 16) as u8,
                repeats_min: a.min(b),
                repeats_max: a.max(b),
                ratchet: int(r, 1, 4) as u8,
            };
        }
    }
    if has(Group::MasterFx) {
        let m = &mut s.master;
        m.enabled = r.chance(0.5);
        m.tape = r.chance(0.4);
        m.cd = r.chance(0.25);
        m.pitch = r.chance(0.25);
        m.radio = r.chance(0.15);
        m.crush = r.chance(0.15);
        m.dropout = r.chance(0.2);
    }
    if has(Group::Wildlife) {
        s.wildlife = r.chance(0.5);
    }
    if has(Group::Look) {
        s.code.enabled = r.chance(0.5);
        s.code.opacity = int(r, 15, 45) as u8;
        s.code.plane = r.below(3) as u8;
        s.camera_speed = r.pick(&[0i16, -2, -2, -4, 2, 4, -6]);
    }
    if has(Group::Architecture) {
        s.arch = if r.chance(0.6) {
            ArchTransform::IDENTITY
        } else {
            ArchTransform { rot_z: r.pick(&[0i16, 15, -15, 30, 45, 90]), off_z: r.pick(&[0i16, 0, 0, 1, -1]), ..ArchTransform::IDENTITY }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_chosen_groups_change() {
        let base = SimConfig::default();
        let words = |c: &SimConfig| shrine0016::recipe::encode(c);
        for g in GROUPS {
            // Over a few rolls, the group moves something...
            let moved = (0..20u64).any(|k| {
                let mut c = base.clone();
                // Mode details need a mode that has some.
                if g == Group::ModeDetails {
                    c.settings.set_mode(if k % 2 == 0 { Pace::Garage } else { Pace::Micro });
                }
                let before = words(&c);
                apply(&mut c, &[g], &mut Rng::new(k));
                words(&c) != before
            });
            assert!(moved, "{g:?} never changes anything");
        }
        // ...and rolling nothing changes nothing.
        let mut c = base.clone();
        apply(&mut c, &[], &mut Rng::new(3));
        assert_eq!(words(&c), words(&base));
        // The seed alone touches nothing else.
        let mut c = base.clone();
        apply(&mut c, &[Group::Seed], &mut Rng::new(9));
        assert_eq!(c.settings, base.settings);
        assert_eq!((c.biome, c.weather, c.seeds), (base.biome, base.weather, base.seeds));
    }

    #[test]
    fn rolling_everything_gives_a_valid_recipe() {
        for k in 0..64u64 {
            let mut c = SimConfig::default();
            apply(&mut c, &GROUPS, &mut Rng::new(k));
            let back = shrine0016::recipe::decode(&shrine0016::recipe::encode(&c));
            assert_eq!(back.settings, c.settings, "roll {k} does not survive the recipe");
            assert!(c.settings.pace != Pace::Micro || c.settings.kit.is_light() || c.settings.kit == Kit::Off);
        }
    }
}
